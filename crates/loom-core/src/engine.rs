//! The engine: owns sessions, model calls, streaming, tools, and titles.
//!
//! Streams and tool loops are engine-owned tasks, not request handlers — a
//! chat keeps producing while the user switches chats, minimises the window,
//! or reloads the webview. Events go out through a callback supplied by the
//! shell, so this crate stays free of Tauri.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::attachments::Attachment;
use crate::config::{AgentMode, AppConfig, ChatDefaults, ModelRef, PermissionMode};
use crate::db::{
    now_ms, CommandRun, Database, MemoryEntry, Message, Role, Session, SessionUpdate, Todo,
};
use crate::persona::{Persona, PersonaVars};
use crate::process::{Running, TailHandle, Wait};
use crate::provider::{MetadataSource, Modality, ModelSpec, ProviderConfig, ReasoningSpec};
use crate::providers::stream::{self, Cancellation};
use crate::providers::{
    ChatRequest, ContentPart, Delta, ToolDef, Usage, WireMessage, WireToolCall,
};
use crate::tools::{self, ToolCall, ToolContext};
use crate::{catalog, context, harness, provider as provider_mod, secrets, Error, Result};

/// How long an "ask" permission prompt waits before denying.
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(600);

/// How long an MCP server gets to start, and to answer `tools/list`, during a
/// `test_mcp_server` call.
const MCP_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How long an `ask_user` question waits for an answer before the turn gives
/// up and carries on. Longer than a permission prompt: this one is a real
/// question the user may need to go and read something to answer.
const QUESTION_TIMEOUT: Duration = Duration::from_secs(1_800);

/// How long the user must leave the machine alone before a paused computer
/// turn resumes on its own. Public so the pill shows the same number the
/// engine uses instead of hardcoding its own copy.
pub const PAUSE_IDLE_RESUME_SECONDS: u64 = 30;

const PAUSE_IDLE_RESUME: Duration = Duration::from_secs(PAUSE_IDLE_RESUME_SECONDS);

/// A paused computer turn gives up after this long, so a chat can never hang
/// busy forever while the user is away.
const PAUSE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Why a paused computer turn started running again.
enum PauseExit {
    /// Explicit resume or 30 s of input silence.
    Resumed,
    /// The user stopped the turn.
    Cancelled,
    /// Nobody came back for 15 minutes.
    TimedOut,
}

/// What the model is told when a takeover pause ends: everything it saw is
/// suspect, and looking again is the first move.
const TAKEOVER_RESUME_NOTE: &str = "The user took over the computer and has now resumed you. \
    Anything you saw before is stale: take a fresh screenshot and verify the current state \
    before acting.";

/// Why a paused computer turn ended.
const TAKEOVER_TIMEOUT: &str = "The computer stayed paused for 15 minutes, so Loom stopped.";

/// Why a computer turn stopped by the user ended. Shown in the transcript, so
/// a stopped turn is explained instead of just going quiet.
const COMPUTER_STOPPED_NOTE: &str =
    "The user stopped computer use for this turn (the control pill, the Computer chip, or \
     Ctrl+Alt+Esc). Nothing further was clicked or typed.";

/// Why a computer turn ended when the Computer chip was switched off while it
/// was running: revoking consent has to bite at once, not next turn.
const COMPUTER_REVOKED_NOTE: &str =
    "The Computer chip was switched off, so this turn stopped and control was handed back.";

/// Largest reply budget a persona may set. The global default is capped at
/// 200k by the settings writer; a persona is not, so it is capped here.
const MAX_PERSONA_OUTPUT_TOKENS: u32 = 200_000;

/// Waits before re-sending a round the transport refused. Short: the user is
/// watching a chat, and a provider that is merely busy recovers in seconds.
const RETRY_BACKOFF: [std::time::Duration; 3] = [
    std::time::Duration::from_millis(500),
    std::time::Duration::from_millis(1_500),
    std::time::Duration::from_millis(3_000),
];

/// How many times one round may re-send after a transport refusal.
const MAX_TRANSPORT_RETRIES: usize = 3;

/// How many times a turn tells the model that an identical repeat changed
/// nothing before it takes the tools away for a wrap-up round.
const MAX_NUDGES: usize = 2;

/// Whether a provider failure is worth re-sending: a busy or briefly broken
/// gateway, not a bad request, a bad key, or a refusal.
fn is_transient_error(message: &str) -> bool {
    let haystack = message.to_ascii_lowercase();
    const PATTERNS: &[&str] = &[
        "http 429",
        "http 500",
        "http 502",
        "http 503",
        "http 504",
        "http 529",
        "rate limit",
        "rate_limit",
        "overloaded",
        "server error",
        "service unavailable",
        "bad gateway",
        "gateway timeout",
        "timed out",
        "timeout",
        "connection reset",
        "connection refused",
        "connection closed",
        "error sending request",
        "broken pipe",
        "temporarily",
    ];
    PATTERNS.iter().any(|pattern| haystack.contains(pattern))
}

/// Releases a computer turn's global state (input hooks, the single-turn lock,
/// a pending pause, held keys, screenshot retention) on every exit path,
/// including a panic that skips the rest of [`Engine::run_completion`].
struct ComputerTurnGuard {
    engine: Engine,
    session_id: String,
}

impl ComputerTurnGuard {
    fn new(engine: &Engine, session_id: &str) -> Self {
        Self {
            engine: engine.clone(),
            session_id: session_id.to_string(),
        }
    }
}

impl Drop for ComputerTurnGuard {
    fn drop(&mut self) {
        self.engine.release_computer_turn(&self.session_id);
    }
}

/// Why a browser turn ended when the Browser chip was switched off while it
/// ran.
const BROWSER_REVOKED_NOTE: &str =
    "The Browser chip was switched off, so this turn stopped and the browser was handed back.";

/// Releases a browser turn's global state (the single-turn lock, and the
/// host's per-chat holds) on every exit path, including a panic that skips the
/// rest of [`Engine::run_completion`].
struct BrowserTurnGuard {
    engine: Engine,
    session_id: String,
}

impl BrowserTurnGuard {
    fn new(engine: &Engine, session_id: &str) -> Self {
        Self {
            engine: engine.clone(),
            session_id: session_id.to_string(),
        }
    }
}

impl Drop for BrowserTurnGuard {
    fn drop(&mut self) {
        self.engine.release_browser_turn(&self.session_id);
    }
}

/// The config lock, recovering from a poisoned mutex.
///
/// A panic anywhere while the lock is held poisons it, and `Mutex::lock` then
/// returns `Err` for the rest of the process: every later config read panics in
/// turn, so one bad turn becomes an app that cannot load its own settings or
/// save them again. The value is plain data, so the state written before the
/// panic is still the best one available, and the two locks in `src-tauri`
/// (`blocker.rs` and `browser.rs`) already recover this way.
///
/// A free function, not a method: `commands.rs` reaches it too, and its own
/// `AppState::config` wraps a different `Mutex`. It sits here, above
/// `SharedConfig`, so it is at module scope — an earlier version of this was
/// anchored inside `impl Engine`, which made it an associated function and left
/// every bare `lock_config(..)` call in the file unable to see it.
pub fn lock_config(
    config: &std::sync::Mutex<AppConfig>,
) -> std::sync::MutexGuard<'_, AppConfig> {
    config.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub type SharedConfig = Arc<Mutex<AppConfig>>;
pub type EmitFn = Arc<dyn Fn(EngineEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone, Serialize)]
// `rename_all` renames the *variants* (Started -> "started"); the fields of an
// enum variant need `rename_all_fields`, otherwise the wire carries
// `session_id` while the frontend reads `sessionId` and every event is dropped.
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum EngineEvent {
    Started {
        session_id: String,
        message_id: String,
    },
    Delta {
        session_id: String,
        message_id: String,
        text: String,
    },
    Reasoning {
        session_id: String,
        message_id: String,
        text: String,
        /// Code points of reply text emitted before this thinking spell; the
        /// transcript places the block here rather than at the top.
        after: usize,
        /// Stream order within the turn, for ties at the same offset.
        seq: usize,
    },
    ToolCallStarted {
        session_id: String,
        message_id: String,
        call_id: String,
        name: String,
        arguments: String,
        /// Stream order within the turn, for ties at the same offset.
        seq: usize,
    },
    ToolCallFinished {
        session_id: String,
        message_id: String,
        call_id: String,
        ok: bool,
        output: String,
        #[serde(default)]
        images: Vec<tools::ToolImage>,
    },
    /// The user touched the machine while Loom was driving it; the turn is held
    /// until they go idle (auto-resume), press Resume, or Stop.
    ComputerPaused {
        session_id: String,
    },
    ComputerResumed {
        session_id: String,
    },
    ToolPermissionRequest {
        session_id: String,
        message_id: String,
        call_id: String,
        name: String,
        arguments: String,
        read_only: bool,
        /// Why this call is asking, when the mode would otherwise have let it
        /// through. A delete that only cards because of what it would destroy
        /// says so here; the card shows it, because "delete_path" on its own
        /// tells the user nothing about what they are being asked to approve.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The model called `ask_user`: the turn is paused until the user answers.
    QuestionRequest {
        session_id: String,
        message_id: String,
        call_id: String,
        question: tools::AskQuestion,
    },
    Done {
        session_id: String,
        message_id: String,
        content: String,
        reasoning: Option<String>,
        usage: Usage,
        /// Set when this reply was answered from a condensed view of the
        /// chat's older turns. Purely informational: the turn succeeded.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        condensed: Option<context::Condensed>,
    },
    /// The turn ended early — a limit, a provider refusal, a loop. Not an
    /// error: the transcript shows a neutral note, and any partial reply
    /// already streamed stays where it is.
    Notice {
        session_id: String,
        /// The turn that stopped, when it is a turn in a chat.
        message_id: Option<String>,
        text: String,
        detail: Option<String>,
    },
    Title {
        session_id: String,
        title: String,
    },
    /// A harness tool changed Loom's config; the UI reloads to stay in step.
    HarnessChanged {
        session_id: String,
        section: String,
        summary: String,
    },
    /// A detached run (background subagent or job firing) changed status.
    TaskChanged {
        task: crate::db::Task,
    },
    /// A shell command Loom started changed status (started, finished,
    /// stopped). The Runs panel's Shell tab follows these live.
    CommandChanged {
        command: CommandRun,
    },
    /// A scheduled job was created, edited, deleted, or fired.
    JobChanged {
        job: crate::db::Job,
    },
    /// Long-term memory gained facts (the extraction pass, or a tool).
    MemoryChanged {
        scope: String,
        added: usize,
        session_id: String,
        message_id: String,
    },
    /// This chat's task list was rewritten; the panel above the composer
    /// follows live.
    TodosChanged {
        session_id: String,
        todos: Vec<crate::db::Todo>,
    },
}

impl EngineEvent {
    /// Short name used for diagnostics (`[loom] -> delta`).
    pub fn label(&self) -> &'static str {
        match self {
            EngineEvent::Started { .. } => "started",
            EngineEvent::Delta { .. } => "delta",
            EngineEvent::Reasoning { .. } => "reasoning",
            EngineEvent::ToolCallStarted { .. } => "tool-call-started",
            EngineEvent::ToolCallFinished { .. } => "tool-call-finished",
            EngineEvent::ComputerPaused { .. } => "computer-paused",
            EngineEvent::ComputerResumed { .. } => "computer-resumed",
            EngineEvent::ToolPermissionRequest { .. } => "tool-permission",
            EngineEvent::QuestionRequest { .. } => "question-request",
            EngineEvent::Done { .. } => "done",
            EngineEvent::Notice { .. } => "notice",
            EngineEvent::Title { .. } => "title",
            EngineEvent::HarnessChanged { .. } => "harness-changed",
            EngineEvent::TaskChanged { .. } => "task-changed",
            EngineEvent::CommandChanged { .. } => "command-changed",
            EngineEvent::JobChanged { .. } => "job-changed",
            EngineEvent::MemoryChanged { .. } => "memory-changed",
            EngineEvent::TodosChanged { .. } => "todos-changed",
        }
    }
}

/// Tool call record stored in a message's `extra` column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    /// `ok` | `error` | `denied`
    pub status: String,
    pub output: String,
    /// Code points of reply text emitted before this call started. The
    /// transcript uses it to interleave text and calls in the order they
    /// actually streamed; zero for records written by older builds.
    #[serde(default)]
    pub after: usize,
    /// Stream order within a turn, breaking ties when a call and a thinking
    /// spell share an offset (no text between them).
    #[serde(default)]
    pub seq: usize,
    /// Screenshots and other images the call produced, kept on disk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<tools::ToolImage>,
    /// Set when this call repeated the previous one exactly, arguments and
    /// result both. The call still ran; the transcript just explains why the
    /// model was told nothing had changed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeated: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One spell of thinking, with the reply-text offset it preceded. The
/// transcript keeps each spell where it happened instead of pinning every
/// block to the top of the reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredReasoningBlock {
    pub text: String,
    #[serde(default)]
    pub after: usize,
    #[serde(default)]
    pub seq: usize,
}

/// Why a turn ended before the model was finished.
///
/// Deliberately not called an error. A stop usually means the turn hit a
/// limit, the provider refused one request, or the model looped, and the
/// partial reply is still the user's to read: showing that as a red failure
/// misrepresents what happened and hides work worth keeping. `detail` carries
/// the raw provider text for a Details toggle; `text` is the one line the
/// transcript shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Notice {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            detail: None,
        }
    }

    /// A stop caused by a provider failure: a plain-language line plus the
    /// provider's own words, kept for the details toggle.
    pub fn provider(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            text: summary.into(),
            detail: Some(detail.into()),
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredExtra {
    #[serde(default)]
    tool_calls: Vec<StoredToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reasoning_blocks: Vec<StoredReasoningBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
    /// Set when this reply was answered from a condensed view of the chat's
    /// older turns, so the faint line under it survives a reload. The summary
    /// *text* is deliberately not copied here: it is identical for every turn
    /// between two folds, and at a few thousand characters a copy on each reply
    /// would roughly double the transcript's size. The expander fetches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    condensed: Option<context::Condensed>,
    /// Why a turn stopped early, kept so the reason survives a reload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notice: Option<Notice>,
    /// Why a turn failed, written by builds before stops had their own field.
    /// Still parsed so an older reply's card renders (and upgrades) on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Which model produced this reply (for cost attribution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<ModelRef>,
}

pub fn parse_stored_tools(extra: Option<&str>) -> Vec<StoredToolCall> {
    extra
        .and_then(|raw| serde_json::from_str::<StoredExtra>(raw).ok())
        .map(|stored| stored.tool_calls)
        .unwrap_or_default()
}

/// Thinking spells in stream order, for the transcript's interleaving.
pub fn parse_reasoning_blocks(extra: Option<&str>) -> Vec<StoredReasoningBlock> {
    extra
        .and_then(|raw| serde_json::from_str::<StoredExtra>(raw).ok())
        .map(|stored| stored.reasoning_blocks)
        .unwrap_or_default()
}

/// Token usage recorded on an assistant message, when the provider reported it.
pub fn parse_usage(extra: Option<&str>) -> Option<Usage> {
    extra
        .and_then(|raw| serde_json::from_str::<StoredExtra>(raw).ok())
        .and_then(|stored| stored.usage)
}

/// Why a turn stopped early, if it did. Reads the current field, and upgrades
/// a reply written by a build that only had `error`.
pub fn parse_notice(extra: Option<&str>) -> Option<Notice> {
    let stored: StoredExtra = serde_json::from_str(extra?).ok()?;
    stored.notice.or_else(|| stored.error.map(Notice::new))
}

/// How a turn ended, as recorded on its message. Bundled rather than passed as
/// five positional arguments so a new field cannot be silently dropped at one
/// call site and honoured at another.
#[derive(Default)]
struct TurnOutcome<'a> {
    usage: Option<Usage>,
    notice: Option<&'a Notice>,
    model: Option<&'a ModelRef>,
    /// Set when this reply was answered from a condensed view of the chat's
    /// older turns. Recorded on the message so the line under it survives a
    /// reload, rather than being something only the live event knew.
    condensed: Option<context::Condensed>,
}

fn serialize_extra(tool_calls: &[StoredToolCall], outcome: TurnOutcome<'_>) -> Option<String> {
    serialize_extra_reasoning(tool_calls, &[], outcome)
}

fn serialize_extra_reasoning(
    tool_calls: &[StoredToolCall],
    reasoning_blocks: &[StoredReasoningBlock],
    outcome: TurnOutcome<'_>,
) -> Option<String> {
    let TurnOutcome {
        usage,
        notice,
        model,
        condensed,
    } = outcome;
    if tool_calls.is_empty()
        && reasoning_blocks.is_empty()
        && usage.is_none()
        && notice.is_none()
        && model.is_none()
        && condensed.is_none()
    {
        return None;
    }
    serde_json::to_string(&StoredExtra {
        tool_calls: tool_calls.to_vec(),
        reasoning_blocks: reasoning_blocks.to_vec(),
        usage,
        notice: notice.cloned(),
        condensed,
        // Never written again; the field exists so older replies still parse.
        error: None,
        model: model.cloned(),
    })
    .ok()
}

/// Drops a message's tool images, folding each into the text breadcrumb the
/// wire already uses for an older screenshot, and leaves everything else
/// alone.
///
/// The context budget needs this because an inlined screenshot is the one
/// piece of a request nothing else can shrink: `elide_outputs` only rewrites
/// text, so without this a native-resolution frame on a small-window model is
/// over budget on its own and no amount of eliding can rescue the request.
pub(crate) fn clear_tool_images(message: &Message) -> Message {
    let Some(extra) = message.extra.as_deref() else {
        return message.clone();
    };
    let Ok(mut stored) = serde_json::from_str::<StoredExtra>(extra) else {
        return message.clone();
    };
    let mut changed = false;
    for call in &mut stored.tool_calls {
        if call.images.is_empty() {
            continue;
        }
        call.output.push('\n');
        call.output.push_str(
            &call
                .images
                .iter()
                .map(|image| {
                    format!(
                        "[screenshot omitted to fit the model's context window: {}]",
                        image.name
                    )
                })
                .collect::<Vec<_>>()
                .join(" "),
        );
        call.images.clear();
        changed = true;
    }
    if !changed {
        return message.clone();
    }
    Message {
        extra: serde_json::to_string(&stored).ok(),
        ..message.clone()
    }
}

/// Rewrites every stored tool output through `map`, leaving usage, error, and
/// model metadata (and the rest of the message) untouched. Used by the
/// context budget to compress old tool output without losing the turn's shape.
pub(crate) fn map_tool_outputs(message: &Message, mut map: impl FnMut(&str) -> String) -> Message {
    let Some(extra) = message.extra.as_deref() else {
        return message.clone();
    };
    let Ok(mut stored) = serde_json::from_str::<StoredExtra>(extra) else {
        return message.clone();
    };
    let mut changed = false;
    for call in &mut stored.tool_calls {
        let mapped = map(&call.output);
        if mapped != call.output {
            call.output = mapped;
            changed = true;
        }
    }
    if !changed {
        return message.clone();
    }
    Message {
        extra: serde_json::to_string(&stored).ok(),
        ..message.clone()
    }
}

/// The model recorded on a message, when there is one.
pub fn parse_model(extra: Option<&str>) -> Option<ModelRef> {
    extra
        .and_then(|raw| serde_json::from_str::<StoredExtra>(raw).ok())
        .and_then(|stored| stored.model)
}

/// Estimated USD cost of one round's reported usage, when the model has
/// list prices. Zero when unknown, so a spend cap never blocks a run just
/// because the provider publishes no pricing.
fn round_cost(usage: &Usage, spec: &ModelSpec) -> f64 {
    let input = usage.input_tokens.unwrap_or(0) as f64 / 1_000_000.0;
    let output = usage.output_tokens.unwrap_or(0) as f64 / 1_000_000.0;
    input * spec.input_price.unwrap_or(0.0) as f64
        + output * spec.output_price.unwrap_or(0.0) as f64
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub replies: u64,
    pub priced_replies: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: f64,
    /// The same totals split by the provider that produced each reply, most
    /// estimated spend first.
    pub providers: Vec<ProviderTotals>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTotals {
    pub provider_id: String,
    pub replies: u64,
    pub priced_replies: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: f64,
}

/// A provider whose vendor exposes a usage/quota endpoint for the stored key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCapableProvider {
    pub provider_id: String,
    pub name: String,
    /// Vendor label, e.g. `OpenCode Go`.
    pub source: String,
}

pub struct Engine {
    inner: Arc<Inner>,
}

impl Clone for Engine {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct Inner {
    db: Mutex<Database>,
    config: SharedConfig,
    emit: EmitFn,
    client: reqwest::Client,
    cancels: Mutex<HashMap<String, Cancellation>>,
    /// Pending permission prompts, keyed by tool call id.
    pending: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
    /// Pending `ask_user` questions, keyed by tool call id.
    questions: Mutex<HashMap<String, tokio::sync::oneshot::Sender<tools::Answer>>>,
    /// A `handoff` tool call waiting to start the next speaker's turn, keyed by
    /// session id. Consumed by the supervisor once the current turn ends.
    pending_handoffs: Mutex<HashMap<String, PendingHandoff>>,
    /// Connected MCP servers and their cached tool lists.
    mcp: tokio::sync::Mutex<McpState>,
    /// The one chat currently driving the computer (`computer_access` chats
    /// take turns; two mice in one app is how clicks land in the wrong place).
    computer: Mutex<Option<String>>,
    /// Per-chat computer-use state: last screenshot, held keys, UI tree.
    /// In an `Arc` so a tool call can carry it onto a blocking thread.
    computer_state: Arc<Mutex<HashMap<String, crate::computer::ComputerState>>>,
    /// Chats whose turn is paused because the user took over, with when the
    /// pause started. Presence is the signal; the waiter polls this map, so a
    /// resume can never be missed.
    paused: Mutex<HashMap<String, std::time::Instant>>,
    /// Input hooks, installed only while a computer turn runs.
    takeover: Mutex<Option<Arc<crate::computer::TakeoverWatch>>>,
    /// Whether those hooks installed, and why not when they did not. `None`
    /// means no computer turn has needed them yet, so there is nothing to warn
    /// about; `Some(Err(..))` means takeover detection is blind and the pill
    /// says so rather than pretending to watch.
    takeover_health: Mutex<Option<std::result::Result<(), String>>>,
    /// Why a chat's turn was stopped, read by the loop once it breaks. Without
    /// it a stopped turn ends silently and reads like a crash.
    stop_notes: Mutex<HashMap<String, String>>,
    /// Detached runs: how many are running, and which tasks wait for a slot.
    task_queue: Mutex<TaskQueue>,
    /// Live shell commands, plus the slots being started right now.
    commands: Mutex<CommandTracker>,
    /// Per-session watermark (epoch ms) for the memory extraction pass.
    memory_scan: Mutex<HashMap<String, i64>>,
    /// Per-session fold point the summary pass has already been run for. The
    /// stored summary is the durable record; this stops a pass that keeps
    /// failing (no key, a provider refusal) from being retried on every turn
    /// of a long chat.
    condense_scan: Mutex<HashMap<String, String>>,
    /// The title the auto-title pass last wrote, per session. This is what lets
    /// the confirming pass tell its own guess from a name the user chose: a
    /// stored title that no longer matches what is recorded here was renamed by
    /// hand.
    titles: Mutex<HashMap<String, String>>,
    /// Sessions whose title has already been confirmed against a finished
    /// reply. Once is the whole point — re-asking on every turn would spend a
    /// model call per message to re-answer a question already settled.
    title_done: Mutex<std::collections::HashSet<String>>,
    /// The built-in browser, or a stub that refuses everything. Held as a trait
    /// object so this crate never learns what a webview is, and so tests can
    /// run without a window.
    browser: Mutex<Arc<dyn crate::browser::BrowserHost>>,
    /// Which chat is driving the browser. Same rule as `computer` above, and
    /// the same reason: two turns fighting over one tab is how clicks land in
    /// the wrong place.
    browser_holder: crate::browser::Holder,
    /// How far this model's tokeniser runs from the character estimate, keyed
    /// by `provider/model`. Learned from the counts providers report, so the
    /// fit converges instead of trusting four characters per token.
    calibration: Mutex<HashMap<String, context::Calibration>>,
}

/// The live shell commands, keyed by command id, holding only their captured
/// output. The child handle itself lives in that command's watcher task, so
/// reading a running command's output never waits on the waiter. The database
/// stays the durable record; no entry here means the command is no longer ours.
///
/// `starting` is what makes [`MAX_BACKGROUND_COMMANDS`] a real cap: a start
/// claims a slot before it spawns, so two concurrent `background: true` calls
/// cannot both read "7 running" and both proceed.
#[derive(Default)]
struct CommandTracker {
    handles: HashMap<String, TailHandle>,
    starting: usize,
}

/// Detached runs are capped; extras queue FIFO.
#[derive(Default)]
struct TaskQueue {
    running: usize,
    waiting: std::collections::VecDeque<String>,
}

/// How many detached runs may run at once.
pub const TASK_CONCURRENCY: usize = 3;

/// How many background commands may run at once. Past this, `run_command`
/// refuses instead of quietly piling up processes nobody is watching.
pub const MAX_BACKGROUND_COMMANDS: usize = 8;

/// Tool steps a detached run may take unless the caller says otherwise.
pub const TASK_MAX_STEPS: u32 = 40;

#[derive(Default)]
struct McpState {
    clients: HashMap<String, crate::mcp::McpClient>,
    tools: Vec<(String, crate::mcp::McpTool)>,
    /// Set when servers are added/removed so the next turn reconnects.
    dirty: bool,
}

/// A persona waiting for the floor after a `handoff` call.
#[derive(Debug, Clone)]
struct PendingHandoff {
    persona_id: String,
    chain: u32,
}

/// How many consecutive handoffs one user turn may trigger.
const MAX_HANDOFF_CHAIN: u32 = 6;

/// Which of the two title passes is running. See
/// [`Engine::maybe_generate_title`] for what each one is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitlePass {
    /// The instant the user sends: name the chat from their own message, so
    /// the sidebar has something to show before the reply exists.
    Early,
    /// Once the first reply has landed: check the name against what the
    /// exchange turned out to be, and replace it only if it does not fit.
    Confirm,
}

/// Pure chat's ceiling on tool rounds: enough for a search, a page fetch, and
/// an answer. Interactive chat turns only; a detached run keeps its own budget.
const CHAT_MAX_ROUNDS: usize = 3;

/// Everything about a turn that the persona and session decided, resolved in
/// one place so the send path stays readable.
struct TurnPlan {
    system: Option<String>,
    variant: Option<String>,
    permission_mode: PermissionMode,
    agent_mode: AgentMode,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
    tool_allow: Option<Vec<String>>,
    mcp_allow: Option<Vec<String>>,
    persona_id: Option<String>,
    memory_enabled: bool,
    /// `(id, name)` for every persona in a multi-persona chat.
    cast: Vec<(String, String)>,
    handoff_chain: u32,
    computer_access: bool,
    /// The built-in browser is armed for this chat. Carried on the plan rather
    /// than read again in `run_completion`, so the offer, the gate and the
    /// prompt note all agree on one value.
    browser_access: bool,
    /// Budget overrides for detached runs; `None` for interactive turns.
    max_steps: Option<u32>,
    max_cost_usd: Option<f64>,
    /// Set when this turn belongs to a detached task.
    task_id: Option<String>,
}

/// Budgets for a turn. Interactive turns use the default (no overrides); a
/// detached run carries its task's caps.
#[derive(Debug, Clone, Default)]
pub struct TurnLimits {
    pub max_steps: Option<u32>,
    pub max_cost_usd: Option<f64>,
    pub task_id: Option<String>,
}

/// What a detached run needs: who asked for it, what to run, and how.
#[derive(Debug, Clone, Default)]
pub struct TaskRequest {
    pub prompt: String,
    pub title: String,
    /// The chat that spawned it, when a chat did.
    pub origin_session: Option<String>,
    pub job_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub persona_id: Option<String>,
    pub workdir: Option<String>,
    pub permission_mode: Option<String>,
    /// Notify when the run finishes successfully. Failures always notify.
    pub notify: bool,
    pub max_steps: Option<u32>,
    pub max_cost_usd: Option<f64>,
}

impl Engine {
    pub fn new(db: Database, config: SharedConfig, emit: EmitFn) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("loom/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build http client");

        Self {
            inner: Arc::new(Inner {
                db: Mutex::new(db),
                config,
                emit,
                client,
                cancels: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
                questions: Mutex::new(HashMap::new()),
                pending_handoffs: Mutex::new(HashMap::new()),
                mcp: tokio::sync::Mutex::new(McpState::default()),
                computer: Mutex::new(None),
                computer_state: Arc::new(Mutex::new(HashMap::new())),
                // A stub until the shell installs a real host. `NoBrowser`
                // rather than an `Option`, so every call site has something to
                // call and the "no browser here" path is one implementation
                // instead of a `None` check in five places.
                browser: Mutex::new(Arc::new(crate::browser::NoBrowser::default())),
                browser_holder: crate::browser::Holder::default(),
                paused: Mutex::new(HashMap::new()),
                takeover: Mutex::new(None),
                takeover_health: Mutex::new(None),
                stop_notes: Mutex::new(HashMap::new()),
                task_queue: Mutex::new(TaskQueue::default()),
                commands: Mutex::new(CommandTracker::default()),
                memory_scan: Mutex::new(HashMap::new()),
                condense_scan: Mutex::new(HashMap::new()),
                titles: Mutex::new(HashMap::new()),
                title_done: Mutex::new(std::collections::HashSet::new()),
                calibration: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// The one database lock.
    ///
    /// **Never call this twice in one expression, or hold the guard across
    /// another `self.db()` call.** `Mutex` is not reentrant: the second lock on
    /// the same thread blocks forever, holding the first, and every other
    /// database user in the process queues behind it. That is a total freeze,
    /// not a slow path. In particular a guard bound by an `if let Ok(Some(..))
    /// = self.db().…` scrutinee lives to the end of the block, so read the row
    /// into a local first and let the guard drop. See `finish_command` and
    /// `complete_task` for the shape that caused it.
    ///
    /// In a debug build a reentrant call panics here and names both call sites
    /// instead of hanging, which is what `db_reentry` is for.
    #[track_caller]
    fn db(&self) -> crate::db_reentry::Guard<'_, Database> {
        // `#[track_caller]` on this function is what makes `Location::caller()`
        // here mean the caller of `db()`. The tripwire cannot work that out for
        // itself: reading the location inside `db_reentry` would name the one
        // line there that every `db()` in the process shares.
        crate::db_reentry::lock(&self.inner.db, std::panic::Location::caller())
    }

    pub fn config(&self) -> AppConfig {
        lock_config(&self.inner.config).clone()
    }

    /// Shared HTTP client (updater, tooling).
    pub fn http_client(&self) -> reqwest::Client {
        self.inner.client.clone()
    }

    fn emit(&self, event: EngineEvent) {
        (self.inner.emit)(event);
    }

    // ------------------------------------------------------------------
    // Sessions
    // ------------------------------------------------------------------

    pub fn create_session(
        &self,
        title: Option<String>,
        provider_id: Option<String>,
        model_id: Option<String>,
        persona_id: Option<String>,
        system_prompt: Option<String>,
        workdir: Option<String>,
    ) -> Result<Session> {
        let chat = self.config().chat;
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.unwrap_or_default(),
            provider_id: provider_id.or(chat.provider_id),
            model_id: model_id.or(chat.model_id),
            variant: None,
            persona_id,
            system_prompt,
            workdir,
            permission_mode: None,
            agent_mode: None,
            computer_access: false,
            browser_access: false,
            position: None,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        self.db().create_session(&session)?;

        // A persona's greeting opens the chat, attributed so multi-persona
        // transcripts show who is speaking.
        if let Some(persona_id) = session.persona_id.clone() {
            let greeting = self
                .config()
                .personas
                .iter()
                .find(|persona| persona.id == persona_id)
                .map(|persona| persona.greeting.trim().to_string())
                .filter(|greeting| !greeting.is_empty());
            if let Some(greeting) = greeting {
                self.db().add_message(&Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session.id.clone(),
                    role: Role::Assistant,
                    content: greeting,
                    reasoning: None,
                    extra: None,
                    persona_id: Some(persona_id),
                    created_at: now_ms(),
                })?;
            }
        }

        Ok(session)
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        self.db().list_sessions()
    }

    /// Writes the hand-placed order for one group of chats in the sidebar.
    pub fn reorder_sessions(&self, ids: &[String]) -> Result<()> {
        self.db().set_session_positions(ids)
    }

    pub fn session(&self, id: &str) -> Result<Option<Session>> {
        self.db().get_session(id)
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        self.cancel(id);
        self.db().delete_session(id)
    }

    /// Deletes chats that were never used (no messages, no title). `keep`
    /// protects the chat the user is currently looking at.
    pub fn prune_empty_sessions(&self, keep: Option<&str>) -> Result<usize> {
        self.db().prune_empty_sessions(keep)
    }

    pub fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                title: Some(title),
                ..Default::default()
            },
        )
    }

    pub fn set_session_model(
        &self,
        id: &str,
        provider_id: &str,
        model_id: &str,
        variant: Option<String>,
    ) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                model: Some((provider_id, model_id)),
                variant: Some(variant.as_deref()),
                ..Default::default()
            },
        )
    }

    pub fn set_session_variant(&self, id: &str, variant: Option<String>) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                variant: Some(variant.as_deref()),
                ..Default::default()
            },
        )
    }

    pub fn set_session_persona(
        &self,
        id: &str,
        persona_id: Option<String>,
        system_prompt: Option<String>,
    ) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                persona: Some(persona_id.as_deref()),
                system_prompt: Some(system_prompt.as_deref()),
                ..Default::default()
            },
        )
    }

    /// Replaces the multi-persona cast for a chat. An empty list restores the
    /// ordinary single-persona behaviour.
    pub fn set_session_cast(&self, id: &str, persona_ids: Vec<String>) -> Result<()> {
        self.db().set_session_cast(id, &persona_ids)
    }

    /// Resolves a chat's cast to live personas, dropping any that were deleted.
    pub fn session_cast(&self, id: &str) -> Result<Vec<Persona>> {
        let ids = self.db().session_cast(id)?;
        let config = self.config();
        Ok(ids
            .iter()
            .filter_map(|persona_id| {
                config
                    .personas
                    .iter()
                    .find(|persona| &persona.id == persona_id)
                    .cloned()
            })
            .collect())
    }

    // ------------------------------------------------------- persona memory

    pub fn persona_memory(&self, persona_id: &str) -> Result<Vec<MemoryEntry>> {
        self.db().persona_memory(persona_id)
    }

    pub fn set_persona_memory(
        &self,
        persona_id: &str,
        key: &str,
        value: &str,
        source: &str,
    ) -> Result<MemoryEntry> {
        self.db()
            .upsert_persona_memory(persona_id, key, value, source)
    }

    pub fn delete_persona_memory(&self, id: &str) -> Result<()> {
        self.db().delete_persona_memory(id)
    }

    pub fn clear_persona_memory(&self, persona_id: &str) -> Result<usize> {
        self.db().clear_persona_memory(persona_id)
    }

    pub fn set_session_workdir(&self, id: &str, workdir: Option<String>) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                workdir: Some(workdir.as_deref()),
                ..Default::default()
            },
        )
    }

    pub fn set_session_permission_mode(
        &self,
        id: &str,
        mode: Option<PermissionMode>,
    ) -> Result<()> {
        let encoded = mode.map(|mode| permission_mode_str(mode).to_string());
        self.db().update_session(
            id,
            SessionUpdate {
                permission_mode: Some(encoded.as_deref()),
                ..Default::default()
            },
        )
    }

    pub fn set_session_agent_mode(&self, id: &str, mode: Option<AgentMode>) -> Result<()> {
        let encoded = mode.map(|mode| agent_mode_str(mode).to_string());
        self.db().update_session(
            id,
            SessionUpdate {
                agent_mode: Some(encoded.as_deref()),
                ..Default::default()
            },
        )
    }

    /// Arms or disarms the built-in browser for one chat (the Browser chip).
    ///
    /// Disarming is a revocation, not a note for later: if this chat is driving
    /// tabs when the switch goes off, its turn ends now. The same rule as the
    /// Computer chip, and for the same reason — a switch that only took effect
    /// on the next turn would leave the model clicking after consent was
    /// withdrawn.
    pub fn set_session_browser_access(&self, id: &str, enabled: bool) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                browser_access: Some(enabled),
                ..Default::default()
            },
        )?;
        if !enabled && self.browser_holder().as_deref() == Some(id) {
            self.cancel_with_note(id, BROWSER_REVOKED_NOTE);
        }
        Ok(())
    }

    /// The chat currently driving the browser, if any. Read by the composer
    /// chip and by Stop, both of which need to know who holds it.
    pub fn browser_holder(&self) -> Option<String> {
        self.inner.browser_holder.current()
    }

    /// Installs the shell's browser host. Called once at startup.
    ///
    /// Takes a trait object rather than making the engine generic: this is what
    /// keeps the crate free of any webview type, and it is what lets a test
    /// hand the engine a fake and exercise dispatch with no window at all.
    pub fn set_browser_host(&self, host: Arc<dyn crate::browser::BrowserHost>) {
        let mut slot = self.inner.browser.lock().expect("browser mutex poisoned");
        *slot = host;
    }

    /// Releases everything a browser turn owned: the single-turn lock, and
    /// whatever the host was holding for this chat. Safe to call twice — the
    /// turn guard and an explicit release both reach it.
    fn release_browser_turn(&self, session_id: &str) {
        // If this chat never held the browser, it never called the host, so
        // there is nothing to give back.
        if !self.inner.browser_holder.release(session_id) {
            return;
        }
        // The host may hold per-chat state: which tab this chat was working in,
        // and whatever the panel was told about it. A turn that ended must not
        // leave it behind, or the next turn starts by trusting a hold from last
        // time instead of looking. The page itself is left open — it is a page
        // the user may still want, and the model's claim on it is all that goes.
        let host = self
            .inner
            .browser
            .lock()
            .expect("browser mutex poisoned")
            .clone();
        host.release(session_id);
    }

    /// Arms or disarms computer use for one chat (the composer's Computer chip).
    ///
    /// Disarming is a revocation, not a note for later: if this chat is driving
    /// the machine when the switch goes off, its turn ends now. Leaving the
    /// model clicking and typing after the user has taken the consent back
    /// would make the switch a lie.
    pub fn set_session_computer_access(&self, id: &str, enabled: bool) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                computer_access: Some(enabled),
                ..Default::default()
            },
        )?;
        if !enabled && self.computer_holder().as_deref() == Some(id) {
            self.cancel_with_note(id, COMPUTER_REVOKED_NOTE);
        }
        Ok(())
    }

    /// Sets or clears this chat's goal (the `/goal` command).
    pub fn set_session_goal(&self, id: &str, goal: Option<&str>) -> Result<()> {
        self.db().set_session_goal(id, goal)
    }

    /// The chat's condensed view of its older turns, for the transcript's
    /// expander. `None` until the background pass has written one.
    pub fn session_summary(&self, id: &str) -> Result<Option<crate::db::SessionSummary>> {
        self.db().session_summary(id)
    }

    pub fn session_goal(&self, id: &str) -> Result<Option<String>> {
        self.db().session_goal(id)
    }

    /// The chat's live task list, as the panel and the model both read it.
    pub fn session_todos(&self, id: &str) -> Result<Vec<Todo>> {
        self.db().todos(id)
    }

    /// Replaces the task list and tells every view; the model gets the
    /// rendered list back so it can check its own work.
    pub fn replace_todos(&self, session_id: &str, todos: Vec<Todo>) -> Result<String> {
        self.db().replace_todos(session_id, &todos)?;
        self.emit(EngineEvent::TodosChanged {
            session_id: session_id.to_string(),
            todos: todos.clone(),
        });
        Ok(render_todos(&todos))
    }

    pub fn read_todos(&self, session_id: &str) -> Result<String> {
        let todos = self.session_todos(session_id)?;
        Ok(render_todos(&todos))
    }

    pub fn messages(&self, session_id: &str) -> Result<Vec<Message>> {
        self.db().messages(session_id)
    }

    /// Renders a chat as markdown, including the last reported token usage.
    pub fn export_session(&self, session_id: &str) -> Result<String> {
        let session = self
            .db()
            .get_session(session_id)?
            .ok_or_else(|| Error::UnknownSession(session_id.to_string()))?;
        let messages = self.db().messages(session_id)?;
        let usage = messages
            .iter()
            .rev()
            .find_map(|message| parse_usage(message.extra.as_deref()));
        Ok(crate::export::session_markdown(&session, &messages, usage))
    }

    /// Removes a single message (used by retry, which drops the failed turn).
    pub fn delete_message(&self, message_id: &str) -> Result<()> {
        self.db().delete_message(message_id)
    }

    /// The files in a chat's workspace, for the composer's `@` picker.
    ///
    /// Paths only, and nothing cached: the popup asks when it opens, and a walk
    /// that only reads names is cheap enough that an index would be more moving
    /// parts than the problem needs. A chat with no folder gets an empty list
    /// rather than an error, because "no workspace yet" is an ordinary state.
    pub fn workspace_files(&self, workdir: Option<&str>, limit: usize) -> Vec<String> {
        match workdir {
            Some(path) if !path.trim().is_empty() => {
                crate::fsutil::walk_files(std::path::Path::new(path), limit)
            }
            _ => Vec::new(),
        }
    }

    /// `list_chats`: the user's other conversations, newest first.
    ///
    /// `db().list_sessions()` filters to `kind = 'chat'`, so the hidden sessions
    /// that detached runs write to are unreachable from here — a background
    /// subagent's transcript is not a chat the user has, and the model has no
    /// business reading one.
    fn list_chats_for_tool(&self, arguments: &serde_json::Value) -> Result<String> {
        let limit = arguments
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(30)
            .clamp(1, 100) as usize;

        let sessions = self.db().list_sessions()?;
        if sessions.is_empty() {
            return Ok("There are no chats yet.".to_string());
        }

        let mut out = format!("{} chat(s), newest first:\n", sessions.len());
        for session in sessions.iter().take(limit) {
            let title = match session.title.trim() {
                "" => "(untitled)",
                title => title,
            };
            out.push_str(&format!(
                "- \"{}\" id {} — {} — last active {}",
                title,
                session.id,
                session.workdir.as_deref().unwrap_or("no workspace"),
                stamp_utc(session.updated_at)
            ));
            out.push('\n');
        }
        if sessions.len() > limit {
            out.push_str(&format!(
                "({} more not shown; raise limit to see them)\n",
                sessions.len() - limit
            ));
        }
        Ok(out)
    }

    /// `read_chat`: another chat's transcript, found by id, prefix or title.
    fn read_chat_for_tool(&self, arguments: &serde_json::Value) -> Result<String> {
        /// Enough for most chats whole, and small enough that reading the wrong
        /// one is not a catastrophe.
        const DEFAULT_BUDGET: usize = 40_000;

        let reference = arguments
            .get("chat")
            .or_else(|| arguments.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if reference.is_empty() {
            return Err(Error::other("read_chat needs a chat id or title"));
        }

        let sessions = self.db().list_sessions()?;
        let folded = |value: &str| -> String {
            value
                .chars()
                .filter(|ch| *ch != '-')
                .collect::<String>()
                .to_lowercase()
        };
        let needle = folded(reference);

        // Most specific first, so an exact id beats a title that happens to
        // start with the same characters.
        let found = sessions
            .iter()
            .find(|session| session.id == reference)
            .or_else(|| {
                sessions
                    .iter()
                    .find(|session| folded(&session.id) == needle)
            })
            .or_else(|| {
                sessions
                    .iter()
                    .find(|session| folded(&session.id).starts_with(&needle))
            })
            .or_else(|| {
                sessions.iter().find(|session| {
                    !session.title.trim().is_empty()
                        && session.title.trim().to_lowercase() == reference.to_lowercase()
                })
            });

        let Some(session) = found else {
            return Ok(format!(
                "No chat matches \"{reference}\". Call list_chats to see what there is."
            ));
        };

        let max_chars = arguments
            .get("max_chars")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(DEFAULT_BUDGET as u64)
            .clamp(2_000, 200_000) as usize;

        let messages = self.db().messages(&session.id)?;
        let usage = messages
            .iter()
            .rev()
            .find_map(|message| parse_usage(message.extra.as_deref()));
        // The same renderer the Export button uses, so there is one definition
        // of what a chat looks like as markdown and no second one to keep true.
        let markdown = crate::export::session_markdown(session, &messages, usage);

        if markdown.chars().count() <= max_chars {
            return Ok(markdown);
        }
        // Head and tail rather than a prefix: the opening turns say what the
        // chat was for and the closing ones say where it got to, and those are
        // the two ends a caller needs. A silent truncation would let the model
        // believe it had read the whole thing.
        let chars: Vec<char> = markdown.chars().collect();
        let head: usize = max_chars / 2;
        let tail = max_chars - head;
        let omitted = chars.len() - max_chars;
        let head_text: String = chars[..head].iter().collect();
        let tail_text: String = chars[chars.len() - tail..].iter().collect();
        Ok(format!(
            "{head_text}\n\n[{omitted} characters omitted from the middle of this chat — \
             call read_chat again with a larger max_chars to see more]\n\n{tail_text}"
        ))
    }

    /// Marks a model as favourite so the picker can float it to the top.
    pub fn set_model_favorite(
        &self,
        provider_id: &str,
        model_id: &str,
        favorite: bool,
    ) -> Result<()> {
        let snapshot = {
            let mut config = lock_config(&self.inner.config);
            let model = config
                .providers
                .get_mut(provider_id)
                .and_then(|provider| provider.models.get_mut(model_id))
                .ok_or_else(|| Error::Other(format!("unknown model {provider_id}/{model_id}")))?;
            model.favorite = favorite;
            config.clone()
        };
        crate::config::save(&snapshot)
    }

    /// Turns a batch of models on or off in one config write.
    ///
    /// Batched because "select all shown" over a search result is one action
    /// from the user's point of view, and writing the config once per model
    /// would make a 300-model gateway crawl.
    pub fn set_models_selected(
        &self,
        provider_id: &str,
        model_ids: &[String],
        selected: bool,
    ) -> Result<()> {
        let snapshot = {
            let mut config = lock_config(&self.inner.config);
            let provider = config
                .providers
                .get_mut(provider_id)
                .ok_or_else(|| Error::UnknownProvider(provider_id.to_string()))?;
            let mut changed = false;
            for model_id in model_ids {
                changed |= provider.set_model_selected(model_id, selected);
            }
            if !changed {
                return Ok(());
            }
            config.clone()
        };
        crate::config::save(&snapshot)
    }

    /// Copies a provider instance: same kind, endpoint, headers, gateway
    /// session header, model catalogue and model selection, under a new id and
    /// display name.
    ///
    /// This is what makes two plans from one vendor possible — two OpenCode Go
    /// subscriptions, say. The copy starts with **no** API key on purpose:
    /// keys are stored in the credential vault under the provider id, so a new
    /// id cannot collide with (or silently inherit) the original's.
    ///
    /// `preset_id` is carried over so [`crate::provider::preset_for`] still
    /// resolves the gateway behaviour a suffixed id no longer matches by name.
    pub fn duplicate_provider(&self, provider_id: &str) -> Result<String> {
        let (snapshot, new_id) = {
            let mut config = lock_config(&self.inner.config);
            let new_id = crate::config::duplicate_provider(&mut config, provider_id)?;
            (config.clone(), new_id)
        };
        crate::config::save(&snapshot)?;
        Ok(new_id)
    }

    // ------------------------------------------------------------------
    // Models
    // ------------------------------------------------------------------

    /// Fetches the provider's model list and merges it into the config.
    pub async fn refresh_models(&self, provider_id: &str) -> Result<usize> {
        let provider = self
            .config()
            .providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(provider_id.to_string()))?;

        let api_key = secrets::get_api_key(provider_id)?;
        let mut fetched = crate::providers::detect::fetch_models(
            &self.inner.client,
            &provider,
            api_key.as_deref(),
        )
        .await?;

        // The gateway knows how it routes a model; an external table fills in
        // what a bare-id gateway leaves blank. Both are `api` priority, so the
        // gateway keeps every field it supplied and the table only adds.
        if let Some(index) = crate::external::model_index(&self.inner.client).await {
            for (id, spec) in &mut fetched {
                if let Some(external) = index.get(id) {
                    crate::providers::detect::merge_spec(spec, external.clone());
                }
            }
        }

        let mut config = lock_config(&self.inner.config);
        if let Some(entry) = config.providers.get_mut(provider_id) {
            let known: std::collections::BTreeSet<String> = entry.models.keys().cloned().collect();
            entry.models = crate::providers::detect::merge_models(&entry.models, fetched);

            // A gateway with hundreds of models is opt-in: nothing the user has
            // not seen before arrives selected, so a refresh cannot flood the
            // picker. Providers left on the default (which is every provider
            // that predates this setting) get everything selected.
            if !entry.auto_select_models {
                let discovered: Vec<String> = entry
                    .models
                    .keys()
                    .filter(|id| !known.contains(*id))
                    .cloned()
                    .collect();
                for id in discovered {
                    entry.disabled_models.insert(id);
                }
            }

            // Only ever *adds* to the denylist above, so a refresh cannot
            // discard a selection — but a model that has left the catalogue
            // should not linger in it either.
            entry.prune_disabled_models();

            entry.models_source = provider_mod::ModelsSource::Fetched;
            entry.last_fetched_at = Some(now_ms());
            let count = entry.models.len();
            drop(config);
            crate::config::save(&self.config())?;
            return Ok(count);
        }

        Err(Error::UnknownProvider(provider_id.to_string()))
    }

    /// Token totals across every chat, plus an estimated spend using the prices
    /// known for each model (catalog list prices, or whatever the user set).
    /// Also split per provider so subscriptions and pay-as-you-go cards can
    /// show what this machine actually spent.
    pub fn usage_summary(&self) -> Result<UsageSummary> {
        let config = self.config();
        let mut summary = UsageSummary::default();
        let mut by_provider: BTreeMap<String, ProviderTotals> = BTreeMap::new();

        // Bound to a local so the guard is released before the loop. The loop
        // walks every assistant row in the database and touches the database
        // not at all, so holding the one connection's lock across it would
        // stall every other database user in the process for the length of the
        // scan — which is the "lots of things at once" freeze, in one call.
        let extras = self.db().assistant_extras()?;
        for extra in extras {
            let Some(usage) = parse_usage(Some(&extra)) else {
                continue;
            };
            summary.replies += 1;
            summary.input_tokens += usage.input_tokens.unwrap_or(0) as u64;
            summary.output_tokens += usage.output_tokens.unwrap_or(0) as u64;

            let Some(model) = parse_model(Some(&extra)) else {
                continue;
            };
            let entry = by_provider.entry(model.provider_id.clone()).or_default();
            entry.provider_id = model.provider_id.clone();
            entry.replies += 1;
            entry.input_tokens += usage.input_tokens.unwrap_or(0) as u64;
            entry.output_tokens += usage.output_tokens.unwrap_or(0) as u64;

            let Some(spec) = config
                .providers
                .get(&model.provider_id)
                .and_then(|provider| provider.models.get(&model.model_id))
            else {
                continue;
            };
            if let (Some(input_price), Some(output_price)) = (spec.input_price, spec.output_price) {
                let input = usage.input_tokens.unwrap_or(0) as f64;
                let output = usage.output_tokens.unwrap_or(0) as f64;
                let cost = input / 1_000_000.0 * input_price as f64
                    + output / 1_000_000.0 * output_price as f64;
                summary.estimated_cost_usd += cost;
                summary.priced_replies += 1;
                entry.estimated_cost_usd += cost;
                entry.priced_replies += 1;
            }
        }

        summary.providers = by_provider.into_values().collect();
        summary.providers.sort_by(|left, right| {
            right
                .estimated_cost_usd
                .partial_cmp(&left.estimated_cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    (right.input_tokens + right.output_tokens)
                        .cmp(&(left.input_tokens + left.output_tokens))
                })
                .then_with(|| left.provider_id.cmp(&right.provider_id))
        });
        Ok(summary)
    }

    /// Providers whose vendor exposes a usage/quota endpoint Loom can read
    /// with the stored key. Only enabled providers are listed.
    pub fn usage_capable_providers(&self) -> Vec<UsageCapableProvider> {
        let mut providers: Vec<UsageCapableProvider> = self
            .config()
            .providers
            .iter()
            .filter(|(_, provider)| provider.enabled)
            .filter_map(|(id, provider)| {
                crate::usage::source_for(provider).map(|source| UsageCapableProvider {
                    provider_id: id.clone(),
                    name: provider.name.clone(),
                    source: source.label.to_string(),
                })
            })
            .collect();
        providers.sort_by(|left, right| left.name.cmp(&right.name));
        providers
    }

    /// Fetches the vendor's usage/quota view for one provider (OpenCode Go's
    /// subscription windows, OpenRouter credits, DeepSeek balance, Z.ai
    /// coding-plan quota).
    pub async fn provider_usage(&self, provider_id: &str) -> Result<crate::usage::ProviderUsage> {
        let provider = self
            .config()
            .providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(provider_id.to_string()))?;
        let api_key = secrets::get_api_key(provider_id)?;
        crate::usage::fetch(
            &self.inner.client,
            provider_id,
            &provider,
            api_key.as_deref(),
        )
        .await
    }

    /// Records a model as recently used (newest first, capped) so the picker
    /// can offer it without a search.
    fn remember_model(&self, model: &ModelRef) {
        let snapshot = {
            let mut config = lock_config(&self.inner.config);
            config.chat.recent_models.retain(|entry| entry != model);
            config.chat.recent_models.insert(0, model.clone());
            config.chat.recent_models.truncate(5);
            config.clone()
        };
        if let Err(error) = crate::config::save(&snapshot) {
            eprintln!("[loom] could not save recent models: {error}");
        }
    }

    pub fn effective_model(&self, session: &Session) -> Result<ModelRef> {
        session
            .provider_id
            .clone()
            .zip(session.model_id.clone())
            .map(|(provider_id, model_id)| ModelRef::new(provider_id, model_id))
            .or_else(|| {
                let chat = &self.config().chat;
                chat.provider_id
                    .clone()
                    .zip(chat.model_id.clone())
                    .map(|(p, m)| ModelRef::new(p, m))
            })
            .ok_or_else(|| Error::Other("no model selected yet".into()))
    }

    pub fn model_spec(&self, provider_id: &str, model_id: &str) -> ModelSpec {
        self.config()
            .providers
            .get(provider_id)
            .and_then(|provider| provider.models.get(model_id).cloned())
            .unwrap_or_else(catalog::fallback)
    }

    // ------------------------------------------------------------------
    // Chat
    // ------------------------------------------------------------------

    /// Starts a turn and returns immediately; the reply streams in a spawned
    /// task.
    ///
    /// # Panics / errors
    ///
    /// Must be called from inside a Tokio runtime (Tauri async commands and
    /// `#[tokio::test]` are; a synchronous Tauri command is **not**).
    /// Without one this returns an error rather than panicking, because a panic
    /// here aborts the whole app.
    pub fn send(
        &self,
        session_id: &str,
        text: &str,
        model: Option<ModelRef>,
        attachments: Vec<Attachment>,
    ) -> Result<String> {
        self.start_turn(session_id, Some(text), model, attachments, None, 0)
    }

    /// Starts a detached run's turn with its budget overrides applied.
    pub(crate) fn send_limited(
        &self,
        session_id: &str,
        text: &str,
        model: Option<ModelRef>,
        attachments: Vec<Attachment>,
        limits: TurnLimits,
    ) -> Result<String> {
        self.start_turn_limited(session_id, Some(text), model, attachments, None, 0, limits)
    }

    /// Like [`send`], but the reply is attributed to a specific cast persona.
    pub fn send_as(
        &self,
        session_id: &str,
        text: &str,
        model: Option<ModelRef>,
        attachments: Vec<Attachment>,
        speaker: Option<String>,
    ) -> Result<String> {
        self.start_turn(session_id, Some(text), model, attachments, speaker, 0)
    }

    fn start_turn(
        &self,
        session_id: &str,
        text: Option<&str>,
        model: Option<ModelRef>,
        attachments: Vec<Attachment>,
        speaker: Option<String>,
        handoff_chain: u32,
    ) -> Result<String> {
        self.start_turn_limited(
            session_id,
            text,
            model,
            attachments,
            speaker,
            handoff_chain,
            TurnLimits::default(),
        )
    }

    /// Starts a turn. `text` is `None` when a `handoff` opened the floor, in
    /// which case no user message is recorded — the tool call is the cue.
    #[allow(clippy::too_many_arguments)]
    fn start_turn_limited(
        &self,
        session_id: &str,
        text: Option<&str>,
        model: Option<ModelRef>,
        attachments: Vec<Attachment>,
        speaker: Option<String>,
        handoff_chain: u32,
        limits: TurnLimits,
    ) -> Result<String> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(Error::Other(
                "send() needs a Tokio runtime: call it from an async Tauri command".into(),
            ));
        }
        let text = text.map(str::trim);
        if text.is_some_and(str::is_empty) && attachments.is_empty() && speaker.is_none() {
            return Err(Error::Other("message is empty".into()));
        }

        let session = self
            .db()
            .get_session(session_id)?
            .ok_or_else(|| Error::UnknownSession(session_id.to_string()))?;

        let config = self.config();

        // The persona that owns this turn: an explicit cast speaker wins, then
        // the chat's selected persona.
        let persona: Option<Persona> = speaker
            .as_ref()
            .and_then(|id| config.personas.iter().find(|p| &p.id == id).cloned())
            .or_else(|| {
                session
                    .persona_id
                    .as_ref()
                    .and_then(|id| config.personas.iter().find(|p| &p.id == id).cloned())
            });

        // Session model first, then the persona's own model, then the global
        // default. An armed computer chat may hand the wheel to its dedicated
        // fast vision model instead.
        let model = match model {
            Some(model) => {
                self.db().update_session(
                    session_id,
                    SessionUpdate {
                        model: Some((&model.provider_id, &model.model_id)),
                        ..Default::default()
                    },
                )?;
                model
            }
            None => {
                // Fall back to the chat's own model when the dedicated
                // computer model points at a provider that is gone or off.
                let computer_model = if session.computer_access {
                    config.chat.computer_model.clone().filter(|candidate| {
                        config
                            .providers
                            .get(&candidate.provider_id)
                            .is_some_and(|provider| provider.enabled)
                    })
                } else {
                    None
                };
                computer_model
                    .or_else(|| {
                        session
                            .provider_id
                            .clone()
                            .zip(session.model_id.clone())
                            .map(|(provider_id, model_id)| ModelRef::new(provider_id, model_id))
                    })
                    .or_else(|| persona.as_ref().and_then(|p| p.model_ref.clone()))
                    .or_else(|| {
                        config
                            .chat
                            .provider_id
                            .clone()
                            .zip(config.chat.model_id.clone())
                            .map(|(provider_id, model_id)| ModelRef::new(provider_id, model_id))
                    })
                    .ok_or_else(|| Error::Other("no model selected yet".into()))?
            }
        };

        let provider = config
            .providers
            .get(&model.provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(model.provider_id.clone()))?;

        if !provider.enabled {
            return Err(Error::Other(format!(
                "provider \"{}\" is disabled",
                provider.name
            )));
        }

        // Remember the model for the picker's "recent" list.
        self.remember_model(&model);

        let now = now_ms();
        if let Some(text) = text {
            if !text.is_empty() || !attachments.is_empty() {
                self.db().add_message(&Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session_id.to_string(),
                    role: Role::User,
                    content: text.to_string(),
                    reasoning: None,
                    extra: crate::attachments::serialize_extra(&attachments),
                    persona_id: None,
                    created_at: now,
                })?;
            }
        }

        let assistant_id = uuid::Uuid::new_v4().to_string();
        self.db().add_message(&Message {
            id: assistant_id.clone(),
            session_id: session_id.to_string(),
            role: Role::Assistant,
            content: String::new(),
            reasoning: None,
            extra: None,
            persona_id: persona.as_ref().map(|p| p.id.clone()),
            created_at: now + 1,
        })?;

        // An armed computer chat runs with cheap thinking: the variant is a
        // latency knob, and every click does not deserve a paragraph of
        // deliberation. Only applied when the model actually offers it (or
        // "off", which both adapters already treat as no reasoning), and never
        // over an explicit per-chat choice.
        let computer_variant = if session.computer_access {
            config.chat.computer_variant.clone().filter(|variant| {
                variant == "off" || variant == "none" || {
                    let spec = self.model_spec(&model.provider_id, &model.model_id);
                    spec.reasoning
                        .as_ref()
                        .is_some_and(|reasoning| reasoning.variants.iter().any(|v| v == variant))
                }
            })
        } else {
            None
        };

        let variant = session
            .variant
            .clone()
            .or(computer_variant)
            .or_else(|| persona.as_ref().and_then(|p| p.variant.clone()))
            .or_else(|| config.chat.variant.clone());

        // Atelier is never granted by a persona: it stays the user's per-chat
        // choice, so a persona can narrow tools but never widen them.
        let persona_permission = persona
            .as_ref()
            .and_then(|p| p.capabilities.permission_mode)
            .filter(|mode| *mode != PermissionMode::Atelier);
        let permission_mode = session
            .permission_mode
            .as_deref()
            .and_then(parse_permission_mode)
            .or(persona_permission)
            .unwrap_or(config.chat.permission_mode);

        let agent_mode = session
            .agent_mode
            .as_deref()
            .and_then(parse_agent_mode)
            .or_else(|| persona.as_ref().and_then(|p| p.capabilities.agent_mode))
            .unwrap_or(config.chat.agent_mode);

        // With no workspace chosen, tools resolve against the scratch folder so
        // they do not hard-fail; `has_workspace` keeps the model from treating
        // that folder as the user's project.
        let has_workspace = session.workdir.is_some();
        let computer_access = session.computer_access;
        let browser_access = session.browser_access;
        let tool_context = ToolContext {
            workdir: session
                .workdir
                .clone()
                .map(std::path::PathBuf::from)
                .or_else(|| crate::paths::scratch_dir().ok()),
            computer: computer_access,
            browser: browser_access,
        };

        // The chat's cast, when it has one. Multi-persona chats resolve each
        // speaker independently and tell the model who is present.
        let cast = self.session_cast(session_id).unwrap_or_default();
        let vars = persona_vars(
            &config,
            persona.as_ref(),
            &model,
            session.workdir.as_deref(),
        );

        // The session snapshot keeps a chat stable across persona edits; a cast
        // member without a snapshot uses its live, assembled prompt.
        let is_snapshot_persona = persona
            .as_ref()
            .is_some_and(|p| session.persona_id.as_deref() == Some(p.id.as_str()));
        let mut system = if is_snapshot_persona {
            session
                .system_prompt
                .clone()
                .or_else(|| persona.as_ref().map(|p| p.assemble(&vars)))
        } else {
            persona.as_ref().map(|p| p.assemble(&vars))
        };

        if let Some(persona) = &persona {
            if let Some(notice) = persona.tool_notice() {
                system = Some(append_note(system, &notice));
            }
            if persona.memory_enabled() {
                if let Some(memory) = self.memory_note(persona) {
                    system = Some(append_note(system, &memory));
                }
            }
        }

        if cast.len() > 1 {
            if let Some(speaker) = &persona {
                let others: Vec<&str> = cast
                    .iter()
                    .filter(|member| member.id != speaker.id)
                    .map(|member| member.name.as_str())
                    .collect();
                system = Some(append_note(
                    system,
                    &format!(
                        "This is a group chat. You are {}. Others present: {}. Only you speak this \
                         turn; do not write their lines. To pass the floor, call the `handoff` tool.",
                        speaker.name,
                        if others.is_empty() {
                            "no one else".to_string()
                        } else {
                            others.join(", ")
                        }
                    ),
                ));
            }
        }

        // Project instructions, the same convention agents use elsewhere. Only a
        // real workspace counts — the scratch folder is not a project.
        let system = match session.workdir.as_deref() {
            Some(workdir) => {
                let agents_md = std::path::Path::new(workdir).join("AGENTS.md");
                match std::fs::read_to_string(&agents_md) {
                    Ok(instructions) if !instructions.trim().is_empty() => Some(format!(
                        "{}\n\nProject instructions (AGENTS.md):\n{}",
                        system.clone().unwrap_or_default(),
                        instructions.trim()
                    )),
                    _ => system,
                }
            }
            None => system,
        };

        // The user's goal for this chat and the model's own task list, so a
        // long task resumes in the right place instead of starting over.
        let goal = self.db().session_goal(session_id).unwrap_or_default();
        let todos = self.db().todos(session_id).unwrap_or_default();
        let system = with_session_context(system, goal.as_deref(), &todos);

        // The house rules for model-generated UI, so the model knows the fence
        // exists, what may go inside it and how it should look.
        let system = if config.interface.generated_ui {
            Some(match system {
                Some(existing) if !existing.trim().is_empty() => {
                    format!("{existing}\n\n{}", crate::ui_guide::GENERATED_UI_GUIDE)
                }
                _ => crate::ui_guide::GENERATED_UI_GUIDE.to_string(),
            })
        } else {
            system
        };

        // No workspace chosen: the scratch folder keeps tools working, but the
        // model should steer the user to a real folder before doing real work.
        // Before the mode notes, so they keep the final word.
        let system = match tool_context.workdir.as_ref() {
            Some(workdir) if !has_workspace => with_scratch_notice(system, workdir),
            _ => system,
        };

        // Tell the model which mode it is in before it starts calling tools:
        // Atelier may edit the harness, and in Plan it should ask more
        // questions and propose, never write. The agent-mode note goes last so
        // Plan has the final word.
        let system = with_harness_mode(system, permission_mode);
        let system = with_computer_mode(
            system,
            computer_access && !agent_mode.is_chat(),
            agent_mode.blocks_writes(),
        );
        let system = with_browser_mode(
            system,
            browser_access && !agent_mode.is_chat(),
            agent_mode.blocks_writes(),
            config.browser.prefer_over_fetch,
        );
        let system = with_agent_mode(system, agent_mode);

        let chat = config.chat.clone();
        let plan = TurnPlan {
            system,
            variant,
            permission_mode,
            agent_mode,
            temperature: persona.as_ref().and_then(|p| p.capabilities.temperature),
            top_p: persona.as_ref().and_then(|p| p.capabilities.top_p),
            max_output_tokens: Self::persona_output_cap(
                persona
                    .as_ref()
                    .and_then(|p| p.capabilities.max_output_tokens),
            ),
            tool_allow: persona
                .as_ref()
                .map(|p| p.capabilities.tools.clone())
                .filter(|tools| !tools.is_empty()),
            mcp_allow: persona
                .as_ref()
                .map(|p| p.capabilities.mcp_servers.clone())
                .filter(|servers| !servers.is_empty()),
            persona_id: persona.as_ref().map(|p| p.id.clone()),
            memory_enabled: persona.as_ref().is_some_and(Persona::memory_enabled),
            cast: cast
                .iter()
                .map(|member| (member.id.clone(), member.name.clone()))
                .collect(),
            handoff_chain,
            computer_access,
            browser_access,
            max_steps: limits.max_steps,
            max_cost_usd: limits.max_cost_usd,
            task_id: limits.task_id,
        };
        let api_key = secrets::get_api_key(&model.provider_id)?;
        let cancel = stream::cancellation();
        self.inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .insert(session_id.to_string(), Arc::clone(&cancel));

        self.emit(EngineEvent::Started {
            session_id: session_id.to_string(),
            message_id: assistant_id.clone(),
        });

        // Name the chat now, from the user's own message, rather than waiting
        // for the reply. The sidebar row is what you scan to find a chat, and
        // sitting on "New chat" for the length of a slow turn is the difference
        // between a list you can navigate and one you have to open chats in.
        //
        // Spawned rather than awaited: this is a model call, and the turn it is
        // describing must not wait behind it. Detached runs are skipped — their
        // sessions are hidden from the sidebar, so a title would be work with
        // nothing to show for it.
        if plan.task_id.is_none() {
            let engine = self.clone();
            let target = session_id.to_string();
            let title_provider = provider.clone();
            let title_model = model.clone();
            let key = api_key.clone();
            tokio::spawn(async move {
                engine
                    .maybe_generate_title(
                        &target,
                        &title_provider,
                        &title_model,
                        key.as_deref(),
                        TitlePass::Early,
                    )
                    .await;
            });
        }

        let engine = self.clone();
        let supervisor = self.clone();
        let session_id_owned = session_id.to_string();
        let assistant_id_for_task = assistant_id.clone();
        let supervised_session = session_id.to_string();
        let supervised_message = assistant_id.clone();

        // The turn runs in its own task so the UI thread is never blocked; the
        // outer task watches it so a panic inside a tool or provider call still
        // produces a terminal event instead of stranding the chat on "stop".
        tokio::spawn(async move {
            let turn = tokio::spawn(async move {
                engine
                    .run_completion(
                        session_id_owned,
                        assistant_id_for_task,
                        provider,
                        model,
                        chat,
                        api_key,
                        cancel,
                        tool_context,
                        plan,
                    )
                    .await;
            });

            match turn.await {
                Err(join_error) => {
                    let reason = if join_error.is_panic() {
                        "the turn crashed while running a tool or provider call — nothing was lost, try again"
                    } else {
                        "the turn was interrupted"
                    };
                    supervisor.report_task_failure(
                        &supervised_session,
                        &supervised_message,
                        reason,
                    );
                }
                Ok(()) => {
                    // A `handoff` opens the next speaker's turn. The chain is
                    // bounded so two personas cannot ping-pong forever.
                    if let Some(next) = supervisor.take_pending_handoff(&supervised_session) {
                        if next.chain <= MAX_HANDOFF_CHAIN {
                            let _ = supervisor.start_turn(
                                &supervised_session,
                                None,
                                None,
                                Vec::new(),
                                Some(next.persona_id),
                                next.chain,
                            );
                        }
                    }
                }
            }
        });

        Ok(assistant_id)
    }

    fn take_pending_handoff(&self, session_id: &str) -> Option<PendingHandoff> {
        self.inner
            .pending_handoffs
            .lock()
            .expect("handoffs mutex poisoned")
            .remove(session_id)
    }

    /// A persona's reply cap comes from a settings field, or from the harness.
    /// Neither can exceed a quarter of the window once the budget is spent:
    /// declaring more than the window reserves is the one thing that makes a
    /// request impossible to fit.
    fn persona_output_cap(value: Option<u32>) -> Option<u32> {
        // Bounded here rather than at each call site so a new one cannot
        // forget: the budget clamps again against the real window, and this
        // only stops nonsense before it reaches it.
        value.map(|cap| cap.clamp(1, MAX_PERSONA_OUTPUT_TOKENS))
    }

    /// Renders the persona's stored memory into a system-prompt note, newest
    /// entries first until the token budget runs out.
    fn memory_note(&self, persona: &Persona) -> Option<String> {
        let entries = self.db().persona_memory(&persona.id).ok()?;
        if entries.is_empty() {
            return None;
        }
        let budget = if persona.memory.token_budget > 0 {
            persona.memory.token_budget
        } else {
            crate::persona::MEMORY_TOKEN_BUDGET
        };
        let mut lines: Vec<String> = Vec::new();
        let mut used = 0u32;
        for entry in entries.iter().rev() {
            let line = format!("- {}: {}", entry.key, entry.value);
            let cost = context::tokens_for(&line);
            if used + cost > budget && !lines.is_empty() {
                break;
            }
            used += cost;
            lines.push(line);
        }
        lines.reverse();
        let mut text = String::from(
            "Persona memory (persists across chats; use `remember` and `forget` to update it):",
        );
        for line in lines {
            text.push('\n');
            text.push_str(&line);
        }
        Some(text)
    }

    /// Terminal event for a task that died without one: records the reason and
    /// clears the busy flag so the UI cannot hang.
    ///
    /// A crashed turn is bad news but it is not the user's mistake and it is
    /// not a "failure" of their reply: the partial text is kept and the card
    /// reads as a stop.
    fn report_task_failure(&self, session_id: &str, message_id: &str, reason: &str) {
        eprintln!("[loom] turn ended without a terminal event: {reason}");
        let notice = Notice::new(
            "This turn ended early — the reply above is everything that arrived. \
             Sending again usually works.",
        )
        .with_detail(reason);
        let extra = serialize_extra(
            &[],
            TurnOutcome {
                notice: Some(&notice),
                ..Default::default()
            },
        );
        let _ = self.db().update_message_extra(message_id, extra.as_deref());
        self.inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .remove(session_id);

        // A panicking computer turn may never have reached its guard's cleanup
        // (`panic = "abort"` in release); do it here so the hooks come down and
        // the machine is not left locked.
        let watch_live = self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .is_some();
        if watch_live
            || self.computer_holder().as_deref() == Some(session_id)
            || self.computer_paused(session_id)
        {
            self.release_computer_turn(session_id);
        }
        self.emit(EngineEvent::Notice {
            session_id: session_id.to_string(),
            message_id: Some(message_id.to_string()),
            text: notice.text.clone(),
            detail: notice.detail.clone(),
        });

        // A detached run whose turn died must still release its queue slot.
        // Read the row, drop the guard, *then* call: `complete_task` takes the
        // database lock again, so doing it inline would self-deadlock.
        let stalled = self
            .db()
            .task_for_session(session_id)
            .ok()
            .flatten()
            .filter(|task| task.status == "running")
            .map(|task| task.id);
        if let Some(task_id) = stalled {
            self.complete_task(&task_id, "stopped", Some(reason), None);
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_completion(
        &self,
        session_id: String,
        message_id: String,
        provider: ProviderConfig,
        model: ModelRef,
        chat: ChatDefaults,
        api_key: Option<String>,
        cancel: Cancellation,
        tool_context: ToolContext,
        plan: TurnPlan,
    ) {
        let TurnPlan {
            system,
            variant,
            permission_mode,
            agent_mode,
            temperature,
            top_p,
            max_output_tokens: persona_max_output,
            tool_allow,
            mcp_allow,
            persona_id,
            memory_enabled,
            cast,
            handoff_chain,
            computer_access,
            browser_access,
            max_steps,
            max_cost_usd,
            task_id,
        } = plan;
        let mut tool_defs: Vec<ToolDef> = if agent_mode.is_chat() {
            // A fixed, tiny list: the web pair, the clock and `ask_user`. Nothing
            // else is offered, so nothing has to be argued about — and because
            // the MCP chain lives in the `else` arm, a chat skips tool discovery
            // entirely, which is a round-trip before the first token.
            tools::specs_for(permission_mode)
                .into_iter()
                .filter(|spec| tools::is_allowed_in_chat(spec.name))
                .map(|spec| ToolDef {
                    name: spec.name.to_string(),
                    description: spec.description.to_string(),
                    parameters: spec.parameters,
                })
                .chain(crate::web::tool_specs().into_iter().filter_map(
                    |(name, description, parameters)| {
                        tools::is_allowed_in_chat(&name).then_some(ToolDef {
                            name,
                            description,
                            parameters,
                        })
                    },
                ))
                .collect()
        } else {
            tools::specs_for(permission_mode)
                .into_iter()
                .map(|spec| ToolDef {
                    name: spec.name.to_string(),
                    description: spec.description.to_string(),
                    parameters: spec.parameters,
                })
                .chain(crate::web::tool_specs().into_iter().map(
                    |(name, description, parameters)| ToolDef {
                        name,
                        description,
                        parameters,
                    },
                ))
                .chain(
                    if computer_access {
                        crate::computer::specs()
                            // A read-only mode refuses these anyway; offering
                            // them only to refuse them costs a round per tool.
                            .into_iter()
                            .filter(|spec| !agent_mode.blocks_writes() || spec.read_only)
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                    .into_iter()
                    .map(|spec| ToolDef {
                        name: spec.name.to_string(),
                        description: spec.description.to_string(),
                        parameters: spec.parameters,
                    }),
                )
                .chain(
                    if browser_access {
                        let tiers = self.config().browser.tiers;
                        crate::browser::specs()
                            // A read-only mode refuses these anyway; offering
                            // them only to refuse them costs a round per tool.
                            .into_iter()
                            .filter(|spec| !agent_mode.blocks_writes() || spec.read_only)
                            // Tiers are a context-cost switch, not a gate: a
                            // chat that does not need the escape hatch should
                            // not pay for its schemas on every request.
                            .filter(|spec| crate::browser::tier_enabled(&tiers, spec.name))
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    }
                    .into_iter()
                    .map(|spec| ToolDef {
                        name: spec.name.to_string(),
                        description: spec.description.to_string(),
                        parameters: spec.parameters,
                    }),
                )
                .chain(self.mcp_tool_defs().await)
                .collect()
        };

        // Persona-owned tools: memory only when the persona opted in, handoff
        // only when there is somebody to hand the turn to. A chat never offers
        // the memory writers — it is answering, not curating — though recalled
        // facts still ride along in the prompt.
        if memory_enabled && !agent_mode.is_chat() {
            tool_defs.extend(tools::memory_specs().into_iter().map(|spec| ToolDef {
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                parameters: spec.parameters,
            }));
        }
        if cast.len() > 1 {
            let names: Vec<String> = cast.iter().map(|(_, name)| name.clone()).collect();
            let spec = tools::handoff_spec(&names);
            tool_defs.push(ToolDef {
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                parameters: spec.parameters,
            });
        }
        // Capability allowlists narrow what the mode offers; they never widen
        // it. `ask_user` is always kept so the model can still check in.
        if let Some(allow) = &tool_allow {
            tool_defs.retain(|def| allow.contains(&def.name) || def.name == tools::ASK_USER);
        }
        if let Some(allow) = &mcp_allow {
            tool_defs.retain(|def| match crate::mcp::parse_tool_name(&def.name) {
                Some((server, _)) => allow.contains(&server.to_string()),
                None => true,
            });
        }

        // Long-term memory: the pinned core plus anything related to this
        // turn, appended after the persona notes so it reads as background
        // knowledge rather than instructions.
        let system = match self
            .long_term_memory_block(
                &session_id,
                tool_context.workdir.as_deref(),
                &provider,
                api_key.as_deref(),
            )
            .await
        {
            Some(block) => Some(match system {
                Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{block}"),
                _ => block,
            }),
            None => system,
        };

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut reasoning_blocks: Vec<StoredReasoningBlock> = Vec::new();
        let mut usage = Usage::default();
        let mut stored_calls: Vec<StoredToolCall> = Vec::new();
        // Why the turn stopped early, if it did. Every path that used to set a
        // fatal error sets one of these instead: a partial reply is still worth
        // reading, and "This reply failed" is never the truth of what happened.
        let mut notice: Option<Notice> = None;
        // Stream order across thinking spells and tool calls, so the
        // transcript can interleave them even when no text separates them.
        let mut seq = 0usize;

        // Budget the request against the model's own context window instead of
        // a message count: tool output ranges from a few tokens to hundreds of
        // thousands, so only a token budget keeps the wire inside the window.
        //
        // The reply budget below is the *same number* the adapters put on the
        // wire. Reserving a different one is how a request Loom believed had
        // 375k tokens to spare was rejected: `max_tokens` was omitted, the
        // gateway reserved 384k of its own, and the difference came out of the
        // safety margin.
        let spec = context::model_spec(&provider, &model.model_id);
        let window = context::context_window(&provider, &model.model_id);
        let configured = persona_max_output.unwrap_or(chat.max_output_tokens);
        // Thinking tokens count inside `max_tokens` on the Anthropic dialect,
        // and the API requires max_tokens to exceed the thinking budget, so ask
        // for room for both. The adapter then trims the budget to whatever fits
        // rather than inflating the declared number behind the budget's back.
        let thinking = (provider.kind == crate::provider::ProviderKind::Anthropic)
            .then(|| {
                variant
                    .as_deref()
                    .and_then(crate::providers::anthropic::thinking_budget)
            })
            .flatten();
        let max_output = match thinking {
            Some(budget) => context::output_limit(configured.max(budget + 1_024), &spec, window),
            None => context::output_limit(configured, &spec, window),
        };
        let fixed = system
            .as_deref()
            .map(context::tokens_for)
            .unwrap_or(0)
            .saturating_add(
                serde_json::to_string(&tool_defs)
                    // Schemas are JSON: punctuation-dense, so the prose ratio
                    // under-counts them.
                    .map(|schema| context::tokens_for_json(&schema))
                    .unwrap_or(0),
            );
        let root_budget = context::input_budget(window, max_output, fixed);

        // What the fit needs in order to fold older turns rather than drop
        // them: the stored summary (if the background pass has written one),
        // the chat's goal, and its task list, so a fresh digest can name both.
        //
        // Read once per turn. The fold point only moves when a rewrite lands,
        // and a summary whose message has since been edited away is ignored by
        // the fit itself rather than trusted.
        let stored_summary = self.db().session_summary(&session_id).ok().flatten();
        let goal = self.db().session_goal(&session_id).ok().flatten();
        let todos = self.db().todos(&session_id).unwrap_or_default();
        let condense_share = chat
            .condense_share
            .min(crate::condense::MAX_CONDENSED_SHARE);
        // The widest fold this turn used, reported once it ends.
        let mut condensed: Option<context::Condensed> = None;

        // A browser turn gets its own step budget, read once. A page task is
        // many small round trips — click, wait, assert — and the default 40
        // runs out mid-task on anything real. The same treatment a computer
        // turn gets, and for the same reason.
        let browser_steps = if browser_access {
            self.config().browser.max_steps
        } else {
            0
        };
        let max_rounds = (max_steps.unwrap_or(if computer_access || browser_access {
            chat.max_tool_rounds.max(browser_steps.max(80))
        } else {
            chat.max_tool_rounds
        }) as usize)
            .clamp(1, 200);
        // Pure chat is capped hard: a search or two, then an answer. Applied as
        // a ceiling, never a floor — a user who lowered `maxToolRounds` still
        // gets their smaller number, and a detached run keeps its task budget.
        let max_rounds = if agent_mode.is_chat() && max_steps.is_none() {
            max_rounds.min(CHAT_MAX_ROUNDS)
        } else {
            max_rounds
        };
        let mut round = 0usize;
        let mut wrapping_up = false;
        let mut last_call: Option<(String, String)> = None;
        let mut last_result: Option<(bool, String)> = None;
        let mut spent_usd = 0.0f64;
        // How many times this turn has told the model that an identical repeat
        // changed nothing.
        let mut nudges = 0usize;
        // Set when a pause inside a batch ends the turn (stopped, or nobody
        // came back): the inner loop breaks, and this carries it out.
        let mut halt = false;

        // How far this model's tokeniser runs from the character estimate. The
        // provider reports the real count on every round, so the fit can be
        // corrected instead of hoping four characters per token holds.
        let calibration_key = format!("{}/{}", model.provider_id, model.model_id);
        let mut calibration = self
            .inner
            .calibration
            .lock()
            .expect("calibration mutex poisoned")
            .get(&calibration_key)
            .copied()
            .unwrap_or_default();

        // Computer turns own global state (hooks, the single-turn lock, a
        // pending pause, held keys). The guard hands it all back on every
        // exit path, and the supervisor's release covers `panic = "abort"`,
        // where Drop never runs.
        let _computer_guard = computer_access.then(|| ComputerTurnGuard::new(self, &session_id));
        // The same arrangement for the browser. Keeping the hold only in the
        // host would be enough for the happy path and wrong for every early
        // return, which is exactly the class of leak the computer guard exists
        // to prevent.
        let _browser_guard = browser_access.then(|| BrowserTurnGuard::new(self, &session_id));

        // Input hooks are installed lazily, by `run_computer_tool`, for the
        // one chat that actually takes the computer. A chat that merely has
        // the chip armed must not watch the user's input, or a second armed
        // chat would pause itself on the first chat's clicks.

        loop {
            let round_started = std::time::Instant::now();
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            // Out of tool steps: tell the model once, then give it a single
            // tool-free round to summarise instead of stopping mid-task in
            // silence. The synthetic result rides on the assistant message so
            // every provider sees a tool result after its call.
            if round >= max_rounds {
                if wrapping_up {
                    break;
                }
                wrapping_up = true;
                seq += 1;
                stored_calls.push(StoredToolCall {
                    id: format!("loom-budget-{}", uuid::Uuid::new_v4()),
                    name: "round_budget".to_string(),
                    arguments: "{}".to_string(),
                    status: "ok".to_string(),
                    output: format!(
                        "You have used all {max_rounds} tool steps for this turn, and no more \
                         tools will run. Summarise what you completed, what is left, and what \
                         the user should do next."
                    ),
                    after: content.chars().count(),
                    seq,
                    images: Vec::new(),
                    repeated: false,
                });
                let _ = self.db().update_message_extra(
                    &message_id,
                    serialize_extra(&stored_calls, TurnOutcome::default()).as_deref(),
                );
            }
            round += 1;

            let history = match self.db().messages(&session_id) {
                Ok(history) => history,
                Err(failure) => {
                    notice = Some(Notice::provider(
                        "This turn stopped: Loom could not read the conversation back from its \
                         database. Nothing was lost — sending again should work.",
                        failure.to_string(),
                    ));
                    break;
                }
            };
            // Every round starts from the untightened budget and re-tightens if
            // it meets another refusal; otherwise one cramped round would leave
            // the rest of the turn needlessly amnesiac.
            let mut history_budget = root_budget;
            // Cleared for the one retry after a length rejection: a screenshot
            // is the only part of a request that eliding cannot shrink.
            let mut allow_images = true;
            // One length retry per round, so a provider that rejects the
            // smaller request too does not loop.
            let mut length_retried = false;

            // Everything this round thinks or calls comes after the text the
            // previous rounds produced; thinking shares the reply-text offset
            // convention so the transcript can interleave it.
            let round_after = content.chars().count();
            // Reserved now so the streaming events and the stored block agree
            // even before the round has finished thinking.
            let reasoning_seq = seq + 1;

            let round_text = Arc::new(Mutex::new(String::new()));
            let round_reasoning = Arc::new(Mutex::new(String::new()));
            let round_calls: Arc<Mutex<Vec<PartialCall>>> = Arc::new(Mutex::new(Vec::new()));
            let text_buffer = Arc::new(Mutex::new(DeltaCoalescer::new(96)));
            let reasoning_buffer = Arc::new(Mutex::new(DeltaCoalescer::new(256)));

            // One round may take more than one attempt at the wire, but only
            // before the model has produced anything. A request refused for
            // length gets one smaller, image-free resend; a transport refusal
            // gets a short backoff. Re-sending after text has streamed would
            // repeat or restart a reply the user is already reading, so that
            // never happens — a partial reply and a note beat a duplicate.
            let mut transport_retries = 0usize;
            let mut pushback: Option<String> = None;

            let (result, sent_estimate) = loop {
                let fitted = context::fit_report(
                    &history,
                    history_budget,
                    allow_images,
                    context::Folding {
                        window,
                        summary: stored_summary.as_ref(),
                        goal: goal.as_deref(),
                        todos: &todos,
                        share: Some(condense_share),
                    },
                );
                let sent_estimate = fitted.estimate;
                // The fold can move during a turn — a length retry narrows the
                // budget, and that reaches further back — so the turn reports
                // the widest coverage it ever used rather than the last one.
                if let Some(found) = fitted.condensed {
                    if condensed.is_none_or(|widest| found.covered > widest.covered) {
                        condensed = Some(found);
                    }
                }

                let request = ChatRequest {
                    provider: &provider,
                    model: &model.model_id,
                    system: system.as_deref(),
                    messages: build_wire(&fitted.messages),
                    variant: variant.as_deref(),
                    max_output_tokens: Some(max_output),
                    temperature,
                    top_p,
                    stream: true,
                    // The wrap-up round offers no tools, so the model has
                    // nothing to call and must write its summary.
                    tools: if wrapping_up {
                        Vec::new()
                    } else {
                        tool_defs.clone()
                    },
                    session_id: Some(&session_id),
                };

                let attempt = {
                    let round_text = Arc::clone(&round_text);
                    let round_reasoning = Arc::clone(&round_reasoning);
                    let round_calls = Arc::clone(&round_calls);
                    let text_buffer = Arc::clone(&text_buffer);
                    let reasoning_buffer = Arc::clone(&reasoning_buffer);
                    let session = session_id.clone();
                    let message = message_id.clone();
                    stream::run_stream(
                        &self.inner.client,
                        &request,
                        api_key.as_deref(),
                        &cancel,
                        move |delta| match delta {
                            Delta::Text { text } => {
                                round_text.lock().expect("text mutex").push_str(&text);
                                let flush = text_buffer.lock().expect("text buffer").push(&text);
                                if let Some(batch) = flush {
                                    self.emit(EngineEvent::Delta {
                                        session_id: session.clone(),
                                        message_id: message.clone(),
                                        text: batch,
                                    });
                                }
                            }
                            Delta::Reasoning { text } => {
                                round_reasoning
                                    .lock()
                                    .expect("reasoning mutex")
                                    .push_str(&text);
                                let flush = reasoning_buffer
                                    .lock()
                                    .expect("reasoning buffer")
                                    .push(&text);
                                if let Some(batch) = flush {
                                    self.emit(EngineEvent::Reasoning {
                                        session_id: session.clone(),
                                        message_id: message.clone(),
                                        text: batch,
                                        after: round_after,
                                        seq: reasoning_seq,
                                    });
                                }
                            }
                            Delta::ToolCall {
                                index,
                                id,
                                name,
                                argument_fragment,
                            } => {
                                merge_tool_delta(
                                    &mut round_calls.lock().expect("calls mutex"),
                                    index,
                                    id,
                                    name,
                                    argument_fragment,
                                );
                            }
                            Delta::Usage { .. } => {}
                        },
                    )
                    .await
                };

                match attempt {
                    Ok(round_usage) => break (Ok(round_usage), sent_estimate),
                    Err(failure) => {
                        let reason = failure.to_string();
                        // Anything already streamed makes a resend a duplicate,
                        // so retries stop the moment the model has spoken.
                        let started = !round_text.lock().expect("text mutex").is_empty()
                            || !round_reasoning.lock().expect("reasoning mutex").is_empty()
                            || !round_calls.lock().expect("calls mutex").is_empty();

                        if !started && !cancel.load(Ordering::Relaxed) {
                            if context::is_context_length_error(&reason) && !length_retried {
                                length_retried = true;
                                history_budget = context::retry_budget(
                                    window,
                                    max_output,
                                    fixed,
                                    history_budget,
                                );
                                // The inlined screenshot is the one part of the
                                // request nothing else can shrink.
                                allow_images = false;
                                pushback = Some(reason);
                                continue;
                            }
                            if is_transient_error(&reason)
                                && transport_retries < MAX_TRANSPORT_RETRIES
                            {
                                let delay = RETRY_BACKOFF
                                    .get(transport_retries)
                                    .copied()
                                    .unwrap_or(RETRY_BACKOFF[RETRY_BACKOFF.len() - 1]);
                                transport_retries += 1;
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                        }
                        break (Err(failure), sent_estimate);
                    }
                }
            };

            // Flush whatever the coalescers are still holding.
            if let Some(batch) = text_buffer.lock().expect("text buffer").flush() {
                self.emit(EngineEvent::Delta {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    text: batch,
                });
            }
            if let Some(batch) = reasoning_buffer.lock().expect("reasoning buffer").flush() {
                self.emit(EngineEvent::Reasoning {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    text: batch,
                    after: round_after,
                    seq: reasoning_seq,
                });
            }

            let text = round_text.lock().expect("text mutex").clone();
            content.push_str(&text);
            let round_thinking =
                std::mem::take(&mut *round_reasoning.lock().expect("reasoning mutex"));
            if !round_thinking.trim().is_empty() {
                seq = reasoning_seq;
                reasoning_blocks.push(StoredReasoningBlock {
                    text: round_thinking.clone(),
                    after: round_after,
                    seq,
                });
            }
            reasoning.push_str(&round_thinking);
            let calls = std::mem::take(&mut *round_calls.lock().expect("calls mutex"));

            match result {
                Ok(round_usage) => {
                    if round_usage.input_tokens.is_some() {
                        usage.input_tokens = round_usage.input_tokens;
                    }
                    if round_usage.output_tokens.is_some() {
                        usage.output_tokens = round_usage.output_tokens;
                    }
                    // Learn from what the provider actually charged. Its count
                    // covers the system prompt and tool schemas too, so add
                    // those to the estimate before comparing — otherwise every
                    // round would look like an over-count and the correction
                    // would never engage.
                    if let Some(reported) = round_usage.input_tokens {
                        calibration.observe(reported, sent_estimate.saturating_add(fixed));
                        self.inner
                            .calibration
                            .lock()
                            .expect("calibration mutex poisoned")
                            .insert(calibration_key.clone(), calibration);
                    }
                    // Detached runs carry a spend cap; stop cleanly once the
                    // estimated cost of the whole run reaches it.
                    if let Some(cap) = max_cost_usd {
                        spent_usd += round_cost(&round_usage, &spec);
                        if spent_usd >= cap {
                            notice = Some(Notice::new(format!(
                                "This run reached its ${cap:.2} spend cap after about \
                                 ${spent_usd:.2}. Everything it produced so far is above."
                            )));
                        }
                    }
                }
                Err(failure) => {
                    let reason = failure.to_string();
                    notice = Some(match &pushback {
                        // The retry already ran and was refused as well, so say
                        // what Loom did about it rather than showing the raw
                        // rejection twice.
                        Some(first) if context::is_context_length_error(&reason) => {
                            Notice::provider(
                                "This reply stopped: even after trimming the conversation, the \
                                 model's context window was too small for this request. The \
                                 reply above is everything that arrived.",
                                format!("{first}\n\nAfter trimming:\n{reason}"),
                            )
                        }
                        _ => Notice::provider(
                            "This reply stopped early — the provider refused the request. Any \
                             text above is everything that arrived before it did.",
                            reason,
                        ),
                    });
                    break;
                }
            }

            if calls.is_empty() {
                break;
            }
            if wrapping_up {
                // Defensive: the wrap-up round offers no tools, so a provider
                // should never return calls here. If one does, ignore them.
                break;
            }

            // The user touching the machine holds the whole turn before the
            // next batch of actions runs: acting on a screen someone else is
            // using is how clicks land on the wrong window.
            if computer_access {
                match self
                    .gate_on_pause(
                        &session_id,
                        &message_id,
                        &cancel,
                        &content,
                        &mut stored_calls,
                        &mut seq,
                    )
                    .await
                {
                    PauseExit::Resumed => {}
                    PauseExit::TimedOut => {
                        notice = Some(Notice::new(TAKEOVER_TIMEOUT));
                        break;
                    }
                    PauseExit::Cancelled => break,
                }
            }

            // Execute the calls the model asked for, then loop so it can use
            // the results. Every call in the round starts after the round's
            // text, so they share one offset into the reply.
            let call_offset = content.chars().count();
            for call in calls {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let call = ToolCall {
                    id: call.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    name: call.name.unwrap_or_default(),
                    arguments: call.arguments,
                };

                if call.name.is_empty() {
                    continue;
                }

                // A computer call is gated on the pause as well as the gap
                // between rounds. The model batches its actions — focus, click,
                // type, press — and a hand on the mouse must stop the rest of
                // the batch, not just the next round.
                if computer_access && crate::computer::is_computer_tool(&call.name) {
                    match self
                        .gate_on_pause(
                            &session_id,
                            &message_id,
                            &cancel,
                            &content,
                            &mut stored_calls,
                            &mut seq,
                        )
                        .await
                    {
                        PauseExit::Resumed => {}
                        PauseExit::TimedOut => {
                            notice = Some(Notice::new(TAKEOVER_TIMEOUT));
                            halt = true;
                            break;
                        }
                        PauseExit::Cancelled => {
                            halt = true;
                            break;
                        }
                    }
                }

                seq += 1;
                let call_seq = seq;

                self.emit(EngineEvent::ToolCallStarted {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                    seq: call_seq,
                });

                // A restricted agent mode refuses its own tools outright: no
                // permission card, just an error the model reads and works
                // around. Read-only modes block the mutating tools; pure chat
                // blocks everything outside its small allowlist, with one
                // exception — `handoff` is conversational plumbing rather than
                // a capability, and without it every turn in a group chat would
                // end on a refused call.
                let mode_blocked = if agent_mode.is_chat() {
                    !tools::is_allowed_in_chat(&call.name) && call.name != tools::HANDOFF
                } else {
                    agent_mode.blocks_writes() && tools::is_blocked_in_plan(&call.name)
                };

                // Harness tools exist only in Atelier. Outside it they are
                // refused outright rather than carded — a permission card for
                // a call that would be denied anyway is noise — and the
                // refusal names the mode so the next turn can ask for it.
                let harness_blocked = harness_denial(permission_mode, &call);

                // Persona tool allowlists narrow what the mode offers. A call
                // outside the scope is refused, like a plan-mode violation, so
                // the model stops retrying without a permission card.
                let scope_blocked =
                    tool_scope_blocked(&call.name, tool_allow.as_ref(), mcp_allow.as_ref());

                // The Computer chip is the standing consent for that chat's
                // computer tools: once the user armed it, a card per click
                // (or keystroke) would just be noise between them and the task.
                let computer_allowed =
                    tool_context.computer && crate::computer::is_computer_tool(&call.name);

                // The Browser chip is the standing consent in exactly the same
                // way, and deliberately so: a permission card per click in a
                // browser would be unworkable for the same reason it is on the
                // desktop. Switching the chip off is the revocation.
                let browser_allowed =
                    tool_context.browser && crate::browser::is_browser_tool(&call.name);

                // `ask_user` never goes through the permission gate: the card
                // is the prompt, and the user's answer is the outcome.
                let allowed = if call.name == tools::ASK_USER {
                    true
                } else if mode_blocked || harness_blocked.is_some() || scope_blocked {
                    false
                } else if computer_allowed || browser_allowed {
                    true
                } else {
                    self.request_permission(
                        &session_id,
                        &message_id,
                        &call,
                        permission_mode,
                        &tool_context,
                    )
                    .await
                };

                let outcome = if call.name == tools::ASK_USER {
                    self.ask_user(&session_id, &message_id, &call, &cancel)
                        .await
                } else if allowed {
                    match crate::mcp::parse_tool_name(&call.name) {
                        Some((server, tool)) => {
                            match self.call_mcp_tool(&server, &tool, &call.arguments).await {
                                Ok(output) => crate::tools::ToolOutcome {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    ok: true,
                                    output,
                                    images: Vec::new(),
                                },
                                Err(error) => crate::tools::ToolOutcome {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    ok: false,
                                    output: error.to_string(),
                                    images: Vec::new(),
                                },
                            }
                        }
                        None => match self.run_web_tool(&call).await {
                            Some(outcome) => outcome,
                            None => match self.run_harness_tool(&session_id, &call).await {
                                Some(outcome) => outcome,
                                None => {
                                    match self
                                        .run_computer_tool(
                                            &session_id,
                                            &call,
                                            &tool_context,
                                            &cancel,
                                        )
                                        .await
                                    {
                                        Some(outcome) => outcome,
                                        None => match self
                                            .run_browser_tool(&session_id, &call, &tool_context)
                                            .await
                                        {
                                            Some(outcome) => outcome,
                                            None => match self
                                                .run_agent_tool(
                                                    &session_id,
                                                    &call,
                                                    &provider,
                                                    &model,
                                                    api_key.as_deref(),
                                                    &tool_context,
                                                    permission_mode,
                                                    persona_id.as_deref(),
                                                    &cast,
                                                    handoff_chain,
                                                )
                                                .await
                                            {
                                                Some(outcome) => outcome,
                                                None => {
                                                    self.run_local_tool(&call, &tool_context).await
                                                }
                                            },
                                        },
                                    }
                                }
                            },
                        },
                    }
                } else {
                    crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: if let Some(denial) = &harness_blocked {
                            denial.output.clone()
                        } else if mode_blocked {
                            tools::mode_refusal(agent_mode.label(), &call.name)
                        } else if scope_blocked {
                            format!(
                                "`{}` is outside this persona's tool scope. Do not call it again; \
                                 continue with the tools you have, or tell the user which tool the \
                                 persona would need.",
                                call.name
                            )
                        } else {
                            "denied by the user".to_string()
                        },
                        images: Vec::new(),
                    }
                };

                // The same call with byte-identical arguments returning the
                // same result twice in a row is a loop, not progress. The call
                // still runs — running the same command again after an edit is
                // most of what any of this work is, and only a *changed* result
                // proves anything — but the model is told plainly that nothing
                // moved, because it evidently expects something to. A fresh
                // screenshot or a wait is exempt: looking again after a pause
                // is not a loop.
                let signature = (call.name.clone(), call.arguments.clone());
                let result = (outcome.ok, outcome.output.clone());
                let repeated = !matches!(call.name.as_str(), "screenshot" | "wait")
                    && last_call.as_ref() == Some(&signature)
                    && last_result.as_ref() == Some(&result);
                let status = if allowed {
                    if outcome.ok {
                        "ok".to_string()
                    } else {
                        "error".to_string()
                    }
                } else {
                    "denied".to_string()
                };
                let mut stored_output = outcome.output.clone();
                if repeated && nudges < MAX_NUDGES {
                    nudges += 1;
                    stored_output.push_str(&format!(
                        "\n\n[You called `{}` with exactly these arguments and got exactly this \
                         result twice in a row, so nothing has changed since. Repeating it will \
                         not help: verify the current state, change the arguments, or tell the \
                         user what is blocking you.]",
                        call.name
                    ));
                }
                last_call = Some(signature);
                last_result = Some(result);

                stored_calls.push(StoredToolCall {
                    id: outcome.id.clone(),
                    name: outcome.name.clone(),
                    arguments: call.arguments.clone(),
                    status,
                    output: stored_output,
                    after: call_offset,
                    seq: call_seq,
                    images: outcome.images.clone(),
                    repeated,
                });

                self.emit(EngineEvent::ToolCallFinished {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: outcome.id.clone(),
                    ok: outcome.ok,
                    output: outcome.output.clone(),
                    images: outcome.images.clone(),
                });
            }

            // A pause inside the batch that ended the turn: stop now rather
            // than asking the model for another round.
            if halt {
                break;
            }

            // Two nudges is the model's cue to try something else. If it
            // repeats itself anyway, take the tools away for one round so it
            // explains what is stuck instead of looping until the step budget
            // runs out — the user gets a reply either way.
            if nudges >= MAX_NUDGES && !wrapping_up {
                wrapping_up = true;
            }

            if cancel.load(Ordering::Relaxed) {
                break;
            }

            // Persist what we have so far: a tool round that follows needs the
            // reasoning echoed back, and a crash should not lose the text.
            let _ = self
                .db()
                .update_message(&message_id, &content, Some(&reasoning));
            let _ = self.db().update_message_extra(
                &message_id,
                serialize_extra_reasoning(&stored_calls, &reasoning_blocks, TurnOutcome::default())
                    .as_deref(),
            );

            if cancel.load(Ordering::Relaxed) {
                break;
            }

            // Computer turns are latency-bound; a per-round time on stderr
            // makes "why is it slow" answerable without a profiler.
            if computer_access {
                eprintln!(
                    "[loom] computer round {round}: {}ms",
                    round_started.elapsed().as_millis()
                );
            }
        }

        let final_reasoning = if reasoning.is_empty() {
            None
        } else {
            Some(reasoning.clone())
        };

        let _ = self
            .db()
            .update_message(&message_id, &content, final_reasoning.as_deref());
        let _ = self.db().update_message_extra(
            &message_id,
            serialize_extra_reasoning(
                &stored_calls,
                &reasoning_blocks,
                TurnOutcome {
                    usage: Some(usage),
                    notice: notice.as_ref(),
                    model: Some(&model),
                    condensed,
                },
            )
            .as_deref(),
        );

        self.inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .remove(&session_id);

        // Computer turns own a few pieces of global state; `_computer_guard`
        // (created before the tool loop) released them on the way out.

        // Detached runs finish here: record the outcome and free the queue
        // slot. Done before the events below move `content`.
        if let Some(task_id) = &task_id {
            let cancelled = cancel.load(Ordering::Relaxed);
            let (status, detail) = if cancelled {
                ("cancelled", "stopped".to_string())
            } else if let Some(stopped) = &notice {
                // "stopped", not "failed": the run ended before its own end,
                // and the notification says so in the same words.
                ("stopped", stopped.text.clone())
            } else {
                ("done", "finished".to_string())
            };
            self.complete_task(task_id, status, Some(&detail), Some(&content));
        }

        // A turn the user stopped (the pill's Stop, the chip, Ctrl+Alt+Esc)
        // says so, instead of just falling silent: a reply that stops with no
        // explanation reads like a crash.
        if notice.is_none() {
            if let Some(note) = self
                .inner
                .stop_notes
                .lock()
                .expect("stop notes mutex poisoned")
                .remove(&session_id)
            {
                notice = Some(Notice::new(note));
            }
        }

        // A turn answered from a condensed view is not a stopped turn: it ends
        // with `Done` like any other, and the summary is reported there. It
        // used to arrive as a `Notice`, which painted a red strip with a Retry
        // button over a reply that had in fact succeeded, and suppressed the
        // `Done` event entirely.
        match notice {
            Some(stopped) => {
                // Record why, so the reason is visible after a reload instead of
                // leaving an empty reply behind.
                let extra = serialize_extra_reasoning(
                    &stored_calls,
                    &reasoning_blocks,
                    TurnOutcome {
                        notice: Some(&stopped),
                        model: Some(&model),
                        ..Default::default()
                    },
                );
                let _ = self
                    .db()
                    .update_message_extra(&message_id, extra.as_deref());

                self.emit(EngineEvent::Notice {
                    session_id: session_id.clone(),
                    message_id: Some(message_id),
                    text: stopped.text,
                    detail: stopped.detail,
                })
            }
            None => {
                self.emit(EngineEvent::Done {
                    session_id: session_id.clone(),
                    message_id,
                    content,
                    reasoning: final_reasoning,
                    usage,
                    condensed,
                });
            }
        }

        if task_id.is_none() && !cancel.load(Ordering::Relaxed) {
            // The checking pass: the early title was written from one side of
            // the conversation, so once there is a reply to read, ask whether
            // the name still fits and replace it only if it does not.
            self.maybe_generate_title(
                &session_id,
                &provider,
                &model,
                api_key.as_deref(),
                TitlePass::Confirm,
            )
            .await;
            self.maybe_extract_memories(&session_id, &provider, &model, api_key.as_deref());
            self.maybe_condense(&session_id, &provider, &model, api_key.as_deref());
        }
    }

    /// Emits a permission prompt (Ask mode) and waits for the answer.
    async fn request_permission(
        &self,
        session_id: &str,
        message_id: &str,
        call: &ToolCall,
        mode: PermissionMode,
        context: &ToolContext,
    ) -> bool {
        let gated = tools::requires_confirmation(mode, &call.name);
        // The mode judges a tool by its *name*, which is all `Ask` and
        // `AutoReadOnly` need — they gate whole classes of call. `Auto all` and
        // Atelier run everything, so a delete has to be judged by what this
        // particular call would destroy instead. That needs the arguments and
        // the filesystem, so it cannot live in `requires_confirmation`.
        //
        // Atelier is exempt, and this is the one asymmetry between it and
        // `Auto all`: the risk card exists to protect a user who asked for
        // tools to run silently, whereas Atelier is a deliberate per-chat
        // handover that includes the filesystem.
        let reason = if !gated && call.name == tools::DELETE_PATH && mode != PermissionMode::Atelier
        {
            tools::delete_risk(&call.arguments, context)
        } else {
            None
        };
        if !gated && reason.is_none() {
            return true;
        }

        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.inner
            .pending
            .lock()
            .expect("pending mutex poisoned")
            .insert(call.id.clone(), sender);

        self.emit(EngineEvent::ToolPermissionRequest {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            read_only: tools::is_read_only(&call.name),
            reason,
        });

        // A detached run that needs approval says so in the Runs popup, and
        // waits: the timeout is long, but the run is not stuck by accident.
        //
        // The row is bound to a local first, and that is not a style choice.
        // `self.db()` in an `if let` scrutinee is a temporary that lives to the
        // end of the whole `if let` — body included — and `update_task` calls
        // `self.db()` again. Both are on this thread, so the second lock would
        // wait forever on the first: the app would freeze at precisely the
        // moment a background run asked for approval, which is the least
        // explicable time for it to stop responding.
        //
        // **This was live.** The tripwire below would have caught it if any
        // test exercised a detached run awaiting approval; none does, which is
        // why the fix is here and not in a failing test.
        let waiting = self.db().task_for_session(session_id).ok().flatten();
        if let Some(task) = waiting {
            if task.status == "running" {
                self.update_task(
                    &task.id,
                    "running",
                    Some(&format!("waiting for your approval of `{}`", call.name)),
                    None,
                );
            }
        }

        let allowed = matches!(
            tokio::time::timeout(PERMISSION_TIMEOUT, receiver).await,
            Ok(Ok(true))
        );

        self.inner
            .pending
            .lock()
            .expect("pending mutex poisoned")
            .remove(&call.id);

        allowed
    }

    /// Answer a pending permission prompt.
    pub fn respond_permission(&self, call_id: &str, allow: bool) -> bool {
        match self
            .inner
            .pending
            .lock()
            .expect("pending mutex poisoned")
            .remove(call_id)
        {
            Some(sender) => sender.send(allow).is_ok(),
            None => false,
        }
    }

    /// Emits an `ask_user` card and waits for the answer. Answered, skipped,
    /// timed out and cancelled questions all leave the turn running; the last
    /// three just come back as a dismissal so the model carries on.
    async fn ask_user(
        &self,
        session_id: &str,
        message_id: &str,
        call: &ToolCall,
        cancel: &Cancellation,
    ) -> crate::tools::ToolOutcome {
        let question = match tools::AskQuestion::parse(&call.arguments) {
            Ok(question) => question,
            Err(error) => {
                return crate::tools::ToolOutcome {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    ok: false,
                    output: error.to_string(),
                    images: Vec::new(),
                }
            }
        };

        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.inner
            .questions
            .lock()
            .expect("questions mutex poisoned")
            .insert(call.id.clone(), sender);

        self.emit(EngineEvent::QuestionRequest {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            call_id: call.id.clone(),
            question: question.clone(),
        });

        let answer = wait_for_answer(receiver, cancel).await;

        self.inner
            .questions
            .lock()
            .expect("questions mutex poisoned")
            .remove(&call.id);

        crate::tools::ToolOutcome {
            id: call.id.clone(),
            name: call.name.clone(),
            ok: true,
            output: question.to_tool_output(&answer),
            images: Vec::new(),
        }
    }

    /// Answer a pending `ask_user` question.
    pub fn respond_question(&self, call_id: &str, answer: tools::Answer) -> bool {
        match self
            .inner
            .questions
            .lock()
            .expect("questions mutex poisoned")
            .remove(call_id)
        {
            Some(sender) => sender.send(answer).is_ok(),
            None => false,
        }
    }

    pub fn cancel(&self, session_id: &str) {
        self.inner
            .pending_handoffs
            .lock()
            .expect("handoffs mutex poisoned")
            .remove(session_id);
        if let Some(cancel) = self
            .inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .get(session_id)
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel_all(&self) {
        let cancels = self.inner.cancels.lock().expect("cancels mutex poisoned");
        for cancel in cancels.values() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Holds a computer turn because the user touched the machine. Idempotent:
    /// the bridge consumes one trip per real input event, so repeated events
    /// while already paused change nothing. The idle clock starts here, so
    /// walking away auto-resumes after [`PAUSE_IDLE_RESUME`].
    pub fn pause_computer(&self, session_id: &str) {
        let inserted = self
            .inner
            .paused
            .lock()
            .expect("paused mutex poisoned")
            .insert(session_id.to_string(), std::time::Instant::now())
            .is_none();
        if !inserted {
            return;
        }

        // A pause mid-drag must not leave the button (or a modifier) held.
        crate::computer::release_held(&self.inner.computer_state, session_id);
        if let Some(watch) = self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .as_ref()
        {
            watch.mark_input_now();
        }
        self.emit(EngineEvent::ComputerPaused {
            session_id: session_id.to_string(),
        });
    }

    /// Wakes a paused computer turn (the pill's Resume, or the chip). Returns
    /// false when nothing was paused.
    pub fn resume_computer(&self) -> bool {
        {
            let mut paused = self.inner.paused.lock().expect("paused mutex poisoned");
            if paused.is_empty() {
                return false;
            }
            paused.clear();
        }
        // Pressing Resume is itself a click, and the input hooks see it either
        // way: on a build where the pill's window registration failed, or for
        // the millisecond between the click landing and the pause beginning.
        // Dropping any trip recorded before now is what makes Resume stick
        // instead of being undone by the very click that asked for it.
        if let Some(watch) = self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .as_ref()
        {
            watch.clear_trip();
        }
        true
    }

    /// Whether the input hooks are live, and the reason when they are not.
    /// `None` means no computer turn has needed them yet.
    pub fn takeover_health(&self) -> (bool, Option<String>) {
        match self
            .inner
            .takeover_health
            .lock()
            .expect("takeover health mutex poisoned")
            .as_ref()
        {
            Some(Err(error)) => (false, Some(error.clone())),
            _ => (true, None),
        }
    }

    /// Stops the computer turn, and nothing else.
    ///
    /// Only one chat can be holding the computer, so there is no requester to
    /// disambiguate: the pill, the chip and the panic hotkey all mean "stop
    /// the turn that is driving this machine". The old behaviour —
    /// `cancel_all` — ended every other chat's turn and every detached run too,
    /// which made the panic key far more destructive than its label.
    ///
    /// Returns the chat that was stopped, if any.
    pub fn stop_computer(&self) -> Option<String> {
        let target = self.computer_holder()?;
        self.cancel_with_note(&target, COMPUTER_STOPPED_NOTE);
        Some(target)
    }

    /// Stops a chat's turn, recording why so the stop is explained rather than
    /// silent.
    pub fn cancel_with_note(&self, session_id: &str, note: &str) {
        self.inner
            .stop_notes
            .lock()
            .expect("stop notes mutex poisoned")
            .insert(session_id.to_string(), note.to_string());
        self.cancel(session_id);
    }

    /// Whether a chat's computer turn is paused (waiting on the user).
    pub fn computer_paused(&self, session_id: &str) -> bool {
        self.inner
            .paused
            .lock()
            .expect("paused mutex poisoned")
            .contains_key(session_id)
    }

    /// The chat currently holding the computer, for the pill.
    pub fn computer_holder(&self) -> Option<String> {
        self.inner
            .computer
            .lock()
            .expect("computer mutex poisoned")
            .clone()
    }

    /// `(paused, idle_ms, paused_ms)` for the pill's status poll.
    pub fn computer_status(&self) -> (bool, u64, u64) {
        let paused = self
            .inner
            .paused
            .lock()
            .expect("paused mutex poisoned")
            .values()
            .next()
            .map(|started| started.elapsed().as_millis() as u64);
        let idle = self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .as_ref()
            .map(|watch| watch.idle_ms())
            .unwrap_or(0);
        (paused.is_some(), idle, paused.unwrap_or(0))
    }

    /// Releases everything a computer turn owned: input hooks, the single-turn
    /// lock, a pending pause, held keys and buttons, this chat's cached shot
    /// and UI tree, and screenshot retention.
    /// Safe to call twice (turn end and the supervisor's failure path).
    fn release_computer_turn(&self, session_id: &str) {
        crate::computer::release_held(&self.inner.computer_state, session_id);
        self.inner
            .paused
            .lock()
            .expect("paused mutex poisoned")
            .remove(session_id);
        // The stop note is consumed by the turn that reads it, but a turn that
        // ended some other way must not leave one behind for the next turn to
        // inherit.
        self.inner
            .stop_notes
            .lock()
            .expect("stop notes mutex poisoned")
            .remove(session_id);
        if let Some(watch) = self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .take()
        {
            watch.clear_trip();
            watch.stop();
        }
        {
            let mut holder = self.inner.computer.lock().expect("computer mutex poisoned");
            if holder.as_deref() == Some(session_id) {
                *holder = None;
            }
        }
        // A finished turn must not leave a stale screenshot or UI tree behind:
        // the next turn in this chat starts by looking, not by trusting pixels
        // from last time, and the map would otherwise grow without bound.
        self.inner
            .computer_state
            .lock()
            .expect("computer state mutex poisoned")
            .remove(session_id);
        self.prune_computer_screenshots(session_id);
    }

    /// Installs the input hooks for the chat that has just taken the computer,
    /// and starts the bridge that turns a real input event into a pause.
    ///
    /// Only the holder ever has a watch. A chat that is merely armed used to
    /// install one too, so a second armed chat would pause itself when the
    /// user clicked anywhere — including on the chat that was actually
    /// driving.
    async fn arm_takeover_watch(&self, session_id: &str) {
        if self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .is_some()
        {
            return;
        }
        // Installing the hooks is a pair of `SetWindowsHookExW` calls plus a
        // bounded wait to find out whether they took, so it goes to a blocking
        // thread like every other piece of synchronous Win32 here. The wait is
        // only ever paid once per process (the hook thread outlives each turn),
        // but a blocking sleep on an async worker is still a blocking sleep.
        let started = tokio::task::spawn_blocking(crate::computer::TakeoverWatch::start).await;
        let outcome = match started {
            Ok(outcome) => outcome,
            // The blocking task panicked: treat it exactly like a failure to
            // install, because that is what it is from here.
            Err(error) => Err(crate::Error::Other(format!(
                "the input hook task panicked: {error}"
            ))),
        };
        match outcome {
            Ok(watch) => {
                let watch = Arc::new(watch);
                *self.inner.takeover.lock().expect("takeover mutex poisoned") =
                    Some(Arc::clone(&watch));
                *self
                    .inner
                    .takeover_health
                    .lock()
                    .expect("takeover health mutex poisoned") = Some(Ok(()));
                let engine = self.clone();
                let session = session_id.to_string();
                let stop = watch.stop_flag();
                tokio::spawn(async move {
                    while !stop.load(Ordering::Relaxed) {
                        // Consumed, not sampled: one real input event is one
                        // pause. Sampling a latch that nothing cleared is what
                        // made every Resume fail.
                        if watch.take_trip()
                            && engine.computer_holder().as_deref() == Some(session.as_str())
                        {
                            engine.pause_computer(&session);
                        }
                        tokio::time::sleep(Duration::from_millis(150)).await;
                    }
                });
            }
            Err(error) => {
                // Takeover protection is off. Say so, loudly and durably: the
                // pill shows this, because a user who believes Loom will stop
                // when they touch the mouse will not be watching it.
                eprintln!("[loom] takeover detection is unavailable: {error}");
                *self
                    .inner
                    .takeover_health
                    .lock()
                    .expect("takeover health mutex poisoned") = Some(Err(error.to_string()));
            }
        }
    }

    /// Waits out a takeover pause, and when the turn comes back tells the model
    /// that everything it saw is stale.
    ///
    /// Called between rounds *and* before every computer call inside a batch: a
    /// reply that asks for ten clicks has to stop for the user just as
    /// completely as the gap after a single one. Returns immediately when
    /// nothing paused, and writes the note once per pause episode.
    async fn gate_on_pause(
        &self,
        session_id: &str,
        message_id: &str,
        cancel: &Cancellation,
        content: &str,
        stored_calls: &mut Vec<StoredToolCall>,
        seq: &mut usize,
    ) -> PauseExit {
        if !self.computer_paused(session_id) {
            return PauseExit::Resumed;
        }
        match self.wait_if_paused(session_id, cancel).await {
            PauseExit::Resumed => {
                self.note_takeover_resume(session_id, message_id, content, stored_calls, seq);
                PauseExit::Resumed
            }
            ended => ended,
        }
    }

    /// Records that the user handed the machine back. The model's next look
    /// must be fresh, and the transcript says why its bearings changed.
    fn note_takeover_resume(
        &self,
        session_id: &str,
        message_id: &str,
        content: &str,
        stored_calls: &mut Vec<StoredToolCall>,
        seq: &mut usize,
    ) {
        // The resume invalidates what the model last saw — the user may have
        // navigated somewhere. Force a fresh look.
        if let Some(state) = self
            .inner
            .computer_state
            .lock()
            .expect("computer state mutex poisoned")
            .get_mut(session_id)
        {
            state.last_shot = None;
            state.last_ui = None;
        }
        *seq += 1;
        stored_calls.push(StoredToolCall {
            id: format!("loom-takeover-{}", uuid::Uuid::new_v4()),
            name: "user_takeover".to_string(),
            arguments: "{}".to_string(),
            status: "ok".to_string(),
            output: TAKEOVER_RESUME_NOTE.to_string(),
            after: content.chars().count(),
            seq: *seq,
            images: Vec::new(),
            repeated: false,
        });
        let _ = self.db().update_message_extra(
            message_id,
            serialize_extra(stored_calls, TurnOutcome::default()).as_deref(),
        );
        self.emit(EngineEvent::ComputerResumed {
            session_id: session_id.to_string(),
        });
    }

    /// Waits out a pause: an explicit resume, [`PAUSE_IDLE_RESUME`] of real
    /// input silence, the [`PAUSE_TIMEOUT`] cap, or the turn being stopped.
    async fn wait_if_paused(&self, session_id: &str, cancel: &Cancellation) -> PauseExit {
        let Some(started) = self
            .inner
            .paused
            .lock()
            .expect("paused mutex poisoned")
            .get(session_id)
            .copied()
        else {
            return PauseExit::Resumed;
        };

        loop {
            if cancel.load(Ordering::Relaxed) {
                return PauseExit::Cancelled;
            }
            // Explicit resume (or any other path out) clears the map, so the
            // map itself is the source of truth — no missed wakeups.
            if !self.computer_paused(session_id) {
                return PauseExit::Resumed;
            }
            if started.elapsed() >= PAUSE_TIMEOUT {
                self.inner
                    .paused
                    .lock()
                    .expect("paused mutex poisoned")
                    .remove(session_id);
                return PauseExit::TimedOut;
            }
            let idle = self
                .inner
                .takeover
                .lock()
                .expect("takeover mutex poisoned")
                .as_ref()
                .map(|watch| watch.idle_ms())
                .unwrap_or(0);
            if idle >= PAUSE_IDLE_RESUME.as_millis() as u64 {
                self.inner
                    .paused
                    .lock()
                    .expect("paused mutex poisoned")
                    .remove(session_id);
                return PauseExit::Resumed;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    pub fn busy_sessions(&self) -> Vec<String> {
        self.inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .keys()
            .cloned()
            .collect()
    }

    // ------------------------------------------------------------------
    // MCP
    // ------------------------------------------------------------------

    /// Connects enabled MCP servers (when needed) and returns their tools as
    /// provider tool definitions.
    pub async fn mcp_tool_defs(&self) -> Vec<ToolDef> {
        let servers = self.config().mcp_servers;
        let mut state = self.inner.mcp.lock().await;

        if state.dirty {
            state.clients.clear();
            state.tools.clear();
            state.dirty = false;
        }

        // Drop servers that were removed or disabled.
        let enabled: Vec<(String, crate::mcp::McpServerConfig)> = servers
            .into_iter()
            .filter(|(_, config)| config.enabled && !config.command.trim().is_empty())
            .collect();
        state
            .clients
            .retain(|id, _| enabled.iter().any(|(candidate, _)| candidate == id));
        state
            .tools
            .retain(|(id, _)| enabled.iter().any(|(candidate, _)| candidate == id));

        for (id, config) in enabled {
            if state.clients.contains_key(&id) {
                continue;
            }
            match crate::mcp::McpClient::connect(&id, &config).await {
                Ok(mut client) => match client.list_tools().await {
                    Ok(tools) => {
                        for tool in tools {
                            state.tools.push((id.clone(), tool));
                        }
                        state.clients.insert(id, client);
                    }
                    Err(error) => eprintln!("[loom] MCP \"{id}\" tools/list failed: {error}"),
                },
                Err(error) => eprintln!("[loom] MCP \"{id}\" failed: {error}"),
            }
        }

        state
            .tools
            .iter()
            .map(|(server, tool)| ToolDef {
                name: crate::mcp::tool_name(server, &tool.name),
                description: format!("[MCP: {server}] {}", tool.description),
                parameters: tool.input_schema.clone(),
            })
            .collect()
    }

    /// Calls a tool on a connected MCP server.
    pub async fn call_mcp_tool(&self, server: &str, tool: &str, arguments: &str) -> Result<String> {
        let parsed: serde_json::Value = if arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(arguments)
                .map_err(|e| Error::Other(format!("invalid tool arguments: {e}")))?
        };

        let mut state = self.inner.mcp.lock().await;
        let client = state
            .clients
            .get_mut(server)
            .ok_or_else(|| Error::Other(format!("MCP server \"{server}\" is not connected")))?;
        client.call_tool(tool, parsed).await
    }

    /// Handles the web tools (search + fetch). Returns `None` for other tools.
    async fn run_web_tool(&self, call: &ToolCall) -> Option<crate::tools::ToolOutcome> {
        let arguments: serde_json::Value = if call.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null)
        };

        let result = match call.name.as_str() {
            crate::web::SEARCH_TOOL => {
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let max = arguments
                    .get("max_results")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(5) as usize;
                crate::web::search(
                    &self.inner.client,
                    &query,
                    max,
                    self.config().search_provider,
                )
                .await
            }
            crate::web::FETCH_TOOL => {
                let url = arguments
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                crate::web::fetch(&self.inner.client, &url, self.config().search_provider).await
            }
            _ => return None,
        };

        Some(match result {
            Ok(output) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: true,
                output,
                images: Vec::new(),
            },
            Err(error) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: error.to_string(),
                images: Vec::new(),
            },
        })
    }

    /// Handles the harness tools (offered in Atelier only). Returns `None` for
    /// other tools. Every write goes through [`Engine::harness_mutate`], so the
    /// backup, the save, the MCP cache invalidation, and the `HarnessChanged`
    /// event cannot drift apart.
    async fn run_harness_tool(
        &self,
        session_id: &str,
        call: &ToolCall,
    ) -> Option<crate::tools::ToolOutcome> {
        if !harness::is_harness_tool(&call.name) {
            return None;
        }

        let arguments: serde_json::Value = if call.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_str(&call.arguments) {
                Ok(value) => value,
                Err(error) => {
                    return Some(crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: format!("invalid harness tool arguments: {error}"),
                        images: Vec::new(),
                    })
                }
            }
        };

        let result = if call.name == harness::TEST_MCP_SERVER {
            self.test_mcp_server(&arguments).await
        } else if harness::is_harness_read(&call.name) {
            self.list_harness(&arguments)
        } else {
            self.harness_mutate(session_id, &call.name, &arguments)
        };

        Some(match result {
            Ok(output) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: true,
                output,
                images: Vec::new(),
            },
            Err(error) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: error.to_string(),
                images: Vec::new(),
            },
        })
    }

    /// `list_harness`: a bounded JSON view, filtered to the requested sections.
    fn list_harness(&self, arguments: &serde_json::Value) -> Result<String> {
        let sections: Vec<String> = match arguments.get("sections") {
            None | Some(serde_json::Value::Null) => Vec::new(),
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| Error::other("sections must be an array of strings"))
                })
                .collect::<Result<Vec<_>>>()?,
            Some(_) => return Err(Error::other("sections must be an array of strings")),
        };
        let view = harness::view_sections(&self.config(), &sections)?;
        serde_json::to_string_pretty(&view)
            .map_err(|error| Error::other(format!("could not render the harness: {error}")))
    }

    /// Lock, back up, mutate, save, announce. The only writer of harness state.
    fn harness_mutate(
        &self,
        session_id: &str,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<String> {
        let (summary, snapshot, section) = {
            let mut config = lock_config(&self.inner.config);
            // Best effort: a failed backup is logged, never fatal.
            if let Err(error) = harness::backup(&config) {
                eprintln!("[loom] harness backup failed: {error}");
            }
            let summary = harness::apply(&mut config, name, args)?;
            (summary, config.clone(), harness::section_of(name))
        };
        crate::config::save(&snapshot)?;
        if section == "mcp" {
            // The next turn reconnects, so the edit is visible immediately.
            self.invalidate_mcp();
        }
        self.emit(EngineEvent::HarnessChanged {
            session_id: session_id.to_string(),
            section,
            summary: summary.clone(),
        });
        Ok(summary)
    }

    /// `test_mcp_server`: one short-lived connect + `tools/list`, then drop the
    /// client (which kills the child; see `kill_on_drop` in `mcp.rs`).
    async fn test_mcp_server(&self, arguments: &serde_json::Value) -> Result<String> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::other("test_mcp_server needs an id"))?;
        let server = self
            .config()
            .mcp_servers
            .get(id)
            .cloned()
            .ok_or_else(|| Error::other(format!("unknown MCP server \"{id}\"")))?;

        let connect = crate::mcp::McpClient::connect(id, &server);
        let mut client = tokio::time::timeout(MCP_TEST_TIMEOUT, connect)
            .await
            .map_err(|_| Error::other(format!("MCP server \"{id}\" did not start within 10s")))??;
        let tools = tokio::time::timeout(MCP_TEST_TIMEOUT, client.list_tools())
            .await
            .map_err(|_| {
                Error::other(format!(
                    "MCP server \"{id}\" did not list its tools within 10s"
                ))
            })??;
        drop(client);

        let count = tools.len();
        let mut names: Vec<String> = tools.into_iter().map(|tool| tool.name).collect();
        let truncated = names.len() > 12;
        names.truncate(12);
        if names.is_empty() {
            return Ok(format!("{id}: connected, but it exposed no tools"));
        }
        Ok(format!(
            "{id}: {count} tool{} — {}{}",
            if count == 1 { "" } else { "s" },
            names.join(", "),
            if truncated { ", …" } else { "" }
        ))
    }

    /// Files beyond the newest [`crate::computer::KEEP_SHOTS`] screenshots are
    /// deleted, and any stored tool calls that pointed at them are blanked so
    /// the transcript never holds broken images.
    fn prune_computer_screenshots(&self, session_id: &str) {
        let removed = match crate::computer::prune_screenshots(session_id) {
            Ok(removed) => removed,
            Err(_) => return,
        };
        if removed.is_empty() {
            return;
        }
        let Ok(messages) = self.db().messages(session_id) else {
            return;
        };
        for message in messages {
            let Some(extra) = message.extra.as_deref() else {
                continue;
            };
            let Ok(mut stored) = serde_json::from_str::<StoredExtra>(extra) else {
                continue;
            };
            let mut changed = false;
            for call in &mut stored.tool_calls {
                let before = call.images.len();
                call.images.retain(|image| !removed.contains(&image.path));
                if call.images.len() != before {
                    changed = true;
                }
            }
            if changed {
                let _ = self.db().update_message_extra(
                    &message.id,
                    serde_json::to_string(&stored).ok().as_deref(),
                );
            }
        }
    }

    /// Handles computer-use tools: screenshots, mouse, keyboard, windows,
    /// processes, clipboard, and UI Automation. Returns `None` for tools that
    /// belong to other layers.
    async fn run_computer_tool(
        &self,
        session_id: &str,
        call: &ToolCall,
        context: &ToolContext,
        cancel: &Cancellation,
    ) -> Option<crate::tools::ToolOutcome> {
        if !crate::computer::is_computer_tool(&call.name) {
            return None;
        }

        // The chip is the permission; without it the model should not even see
        // these tools, and a stale plan that names one gets a clear refusal.
        if !context.computer {
            return Some(crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: crate::computer::DISABLED_NOTE.to_string(),
                images: Vec::new(),
            });
        }

        // One chat drives at a time: two turns fighting over the cursor is how
        // clicks land in the wrong window.
        let became_holder = {
            let mut holder = self.inner.computer.lock().expect("computer mutex poisoned");
            match holder.as_deref() {
                Some(owner) if owner != session_id => {
                    return Some(crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: "Another chat is controlling the computer right now. Wait for \
                                it to finish."
                            .to_string(),
                        images: Vec::new(),
                    });
                }
                Some(_) => false,
                None => {
                    *holder = Some(session_id.to_string());
                    true
                }
            }
        };

        // The hooks go up for the chat that is actually driving, at the moment
        // it takes the wheel — not at turn start for every armed chat.
        if became_holder {
            self.arm_takeover_watch(session_id).await;
        }

        let options = crate::computer::ComputerOptions {
            screenshot_edge: self.config().chat.computer_screenshot_edge,
            cancel: Some(cancel.clone()),
        };
        // Every one of these tools is a synchronous Win32 call — a capture, a
        // `SendInput`, a UI Automation walk, a `launch_app` that may wait thirty
        // seconds for a window — so the computer layer hands them to a blocking
        // thread rather than tying up an async worker.
        Some(crate::computer::run(session_id, call, &self.inner.computer_state, &options).await)
    }

    /// Handles browser tools: pages, tabs, cookies, the network. Returns `None`
    /// for tools that belong to other layers.
    ///
    /// Deliberately the same shape as [`Engine::run_computer_tool`]: the chip is
    /// the permission, one chat drives at a time, and the work happens on a
    /// blocking thread because everything underneath it is a synchronous
    /// webview call.
    async fn run_browser_tool(
        &self,
        session_id: &str,
        call: &ToolCall,
        context: &ToolContext,
    ) -> Option<crate::tools::ToolOutcome> {
        if !crate::browser::is_browser_tool(&call.name) {
            return None;
        }

        // The chip is the permission. Without it the model should not even see
        // these tools, and a stale plan that names one gets a clear refusal
        // rather than a mysterious failure.
        if !context.browser {
            return Some(crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: crate::browser::DISABLED_NOTE.to_string(),
                images: Vec::new(),
            });
        }

        // One chat drives at a time. A second chat is told to wait rather than
        // being handed the tabs: two turns typing into one form is how a field
        // ends up holding half of each. It is not a dead end — `fetch_url` still
        // reads a page for whoever is waiting.
        if self.inner.browser_holder.claim(session_id).is_err() {
            return Some(crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: crate::browser::BUSY_NOTE.to_string(),
                images: Vec::new(),
            });
        }

        let options = {
            let config = self.config();
            crate::browser::BrowserOptions {
                armed: true,
                // The browser's own edge, falling back to the computer one so a
                // user who has already tuned the size gets the same answer in
                // both places rather than two settings meaning one thing.
                screenshot_edge: if config.browser.screenshot_edge > 0 {
                    config.browser.screenshot_edge
                } else {
                    config.chat.computer_screenshot_edge
                },
            }
        };

        let host = self
            .inner
            .browser
            .lock()
            .expect("browser mutex poisoned")
            .clone();
        let call_id = call.id.clone();
        let call_name = call.name.clone();
        let call = call.clone();
        let session = session_id.to_string();

        // Every one of these is a synchronous webview call — a navigation, a
        // script evaluation, a COM cookie read, a capture — so it goes to a
        // blocking thread rather than occupying an async worker. The runtime is
        // a small fixed pool, and a page load can take seconds.
        Some(
            tokio::task::spawn_blocking(move || {
                crate::browser::run(&session, &call, host.as_ref(), &options)
            })
            .await
            .unwrap_or_else(|_| crate::tools::ToolOutcome {
                id: call_id,
                name: call_name,
                ok: false,
                output: "that browser call panicked".to_string(),
                images: Vec::new(),
            }),
        )
    }

    /// Runs a plain workspace tool.
    ///
    /// These are all synchronous, and some are slow — a `read_file` of a large
    /// log, a `grep` across a tree, a `git diff` in a big repository — so they
    /// run on a blocking thread rather than occupying one of the runtime's
    /// async workers. A runtime is a small, fixed pool; blocking a worker on
    /// file IO stalls every other task that needs one, including the streaming
    /// reply in another chat.
    async fn run_local_tool(
        &self,
        call: &ToolCall,
        context: &crate::tools::ToolContext,
    ) -> crate::tools::ToolOutcome {
        let call = call.clone();
        let context = context.clone();
        let id = call.id.clone();
        let name = call.name.clone();
        match tokio::task::spawn_blocking(move || crate::tools::execute(&call, &context)).await {
            Ok(outcome) => outcome,
            Err(_) => crate::tools::ToolOutcome {
                id,
                name,
                ok: false,
                output: "that tool call panicked".to_string(),
                images: Vec::new(),
            },
        }
    }

    /// Handles engine-side agent tools: shell commands, image generation, and
    /// subagents. Returns `None` for tools handled elsewhere.
    #[allow(clippy::too_many_arguments)]
    async fn run_agent_tool(
        &self,
        session_id: &str,
        call: &ToolCall,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
        tool_context: &ToolContext,
        _permission_mode: PermissionMode,
        persona_id: Option<&str>,
        cast: &[(String, String)],
        handoff_chain: u32,
    ) -> Option<crate::tools::ToolOutcome> {
        let arguments: serde_json::Value = if call.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null)
        };

        let result = match call.name.as_str() {
            crate::tools::LIST_CHATS => self.list_chats_for_tool(&arguments),
            crate::tools::READ_CHAT => self.read_chat_for_tool(&arguments),
            "run_command" => {
                self.run_shell_command(session_id, tool_context, &arguments)
                    .await
            }
            crate::tools::LIST_COMMANDS => self.list_commands_for_tool().await,
            crate::tools::COMMAND_OUTPUT => {
                let id = arguments
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let tail = arguments
                    .get("tail")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(60) as usize;
                self.command_output(&id, tail)
            }
            crate::tools::STOP_COMMAND => {
                let id = arguments
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // `stop_command` waits on `taskkill`/`kill` with a blocking
                // call, so it goes to the blocking pool rather than stalling a
                // Tokio worker the rest of the turn needs. No `?` here: this
                // function returns `Option`, so a join error is turned into an
                // engine error explicitly.
                let engine = self.clone();
                match tokio::task::spawn_blocking(move || engine.stop_command(&id)).await {
                    Ok(result) => result.map(|command| {
                        format!(
                            "stopped \"{}\" (id {}, pid {})",
                            command.label, command.id, command.pid
                        )
                    }),
                    Err(error) => Err(Error::other(format!(
                        "stopping that command did not finish: {error}"
                    ))),
                }
            }
            "generate_image" => {
                let prompt = arguments
                    .get("prompt")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let size = arguments
                    .get("size")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.generate_image(provider, model, api_key, &prompt, size.as_deref())
                    .await
            }
            "search_workspace" => {
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(6) as usize;
                self.search_workspace(session_id, &query, limit).await
            }
            "spawn_agent" => {
                let task = arguments
                    .get("task")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let system = arguments
                    .get("system")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let persona_ref = arguments
                    .get("persona")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let background = arguments
                    .get("background")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if background {
                    let title: String = task
                        .lines()
                        .next()
                        .unwrap_or("Background task")
                        .chars()
                        .take(80)
                        .collect();
                    self.spawn_task(TaskRequest {
                        prompt: task.clone(),
                        title,
                        origin_session: Some(session_id.to_string()),
                        job_id: None,
                        provider_id: Some(model.provider_id.clone()),
                        model_id: Some(model.model_id.clone()),
                        persona_id: persona_ref
                            .clone()
                            .or_else(|| self.session_persona_id(session_id)),
                        workdir: tool_context
                            .workdir
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned()),
                        permission_mode: Some("auto-read-only".to_string()),
                        notify: true,
                        max_steps: None,
                        max_cost_usd: None,
                    })
                    .map(|task_id| {
                        format!(
                            "Started the task in the background (id {task_id}). It appears in \
                             the Runs popup; its result will be posted here when it finishes. \
                             Do not wait for it."
                        )
                    })
                } else {
                    match persona_ref {
                        Some(reference) => {
                            let (system, sub_model, sub_provider, sub_key) =
                                self.subagent_persona(&reference, model, provider, api_key);
                            self.spawn_subagent(
                                &sub_provider,
                                &sub_model,
                                sub_key.as_deref(),
                                system.as_deref(),
                                &task,
                                Some(session_id),
                            )
                            .await
                        }
                        None => {
                            self.spawn_subagent(
                                provider,
                                model,
                                api_key,
                                system.as_deref(),
                                &task,
                                Some(session_id),
                            )
                            .await
                        }
                    }
                }
            }
            crate::tools::RECALL => {
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let limit = arguments
                    .get("limit")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(crate::memory::RECALL_LIMIT as u64)
                    .clamp(1, 10) as usize;
                let scopes = crate::memory::scopes_for(
                    tool_context.workdir.as_ref().and_then(|path| path.to_str()),
                );
                match self
                    .recall_facts(&scopes, &query, limit, provider, api_key)
                    .await
                {
                    Ok(memories) if memories.is_empty() => {
                        Ok(format!("Nothing in memory matches \"{query}\"."))
                    }
                    Ok(memories) => {
                        let mut out = format!("{} memories for \"{query}\":\n", memories.len());
                        for memory in memories {
                            out.push_str(&format!(
                                "- [{}] {} (id {})\n",
                                memory.scope, memory.content, memory.id
                            ));
                        }
                        Ok(out)
                    }
                    Err(error) => Err(error),
                }
            }
            crate::tools::REMEMBER_FACT => {
                let fact = arguments
                    .get("fact")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let pinned = arguments
                    .get("pinned")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let scope = crate::memory::scope_for_fact(
                    arguments
                        .get("scope")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("global"),
                    tool_context.workdir.as_ref().and_then(|path| path.to_str()),
                );
                self.store_fact(
                    &scope,
                    &fact,
                    pinned,
                    "model",
                    Some(session_id),
                    None,
                    provider,
                    api_key,
                )
                .await
                .map(|(memory, updated)| {
                    if updated {
                        format!("Updated an existing memory: {}", memory.content)
                    } else {
                        format!("Saved to memory ({}): {}", memory.scope, memory.content)
                    }
                })
            }
            crate::tools::FORGET_FACT => {
                let id = arguments
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let query = arguments
                    .get("query")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                self.forget_fact(id.as_deref(), query.as_deref())
            }
            crate::tools::LIST_JOBS => self.list_jobs_for_tool(),
            crate::tools::SCHEDULE_JOB => {
                self.schedule_job_from_tool(&arguments, session_id, model, tool_context)
            }
            crate::tools::DELETE_JOB => self.delete_job_from_tool(&arguments),
            crate::tools::REMEMBER => {
                let key = trimmed_json(&arguments, "key");
                let value = trimmed_json(&arguments, "value");
                let Some(persona_id) = persona_id else {
                    return Some(crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: "this chat has no persona, so there is nowhere to store memory"
                            .to_string(),
                        images: Vec::new(),
                    });
                };
                if key.is_empty() || value.is_empty() {
                    Err(Error::other("remember needs a key and a value"))
                } else {
                    self.set_persona_memory(persona_id, &key, &value, "model")
                        .map(|entry| format!("Remembered \"{}\".", entry.key))
                }
            }
            crate::tools::FORGET => {
                let key = trimmed_json(&arguments, "key");
                let Some(persona_id) = persona_id else {
                    return Some(crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: "this chat has no persona, so there is no memory to forget"
                            .to_string(),
                        images: Vec::new(),
                    });
                };
                if key.is_empty() {
                    Err(Error::other("forget needs a key"))
                } else {
                    match self.persona_memory(persona_id) {
                        Ok(entries) => match entries.iter().find(|entry| entry.key == key) {
                            Some(entry) => match self.delete_persona_memory(&entry.id) {
                                Ok(()) => Ok(format!("Forgot \"{key}\".")),
                                Err(error) => Err(error),
                            },
                            None => Err(Error::other(format!("no memory entry named \"{key}\""))),
                        },
                        Err(error) => Err(error),
                    }
                }
            }
            crate::tools::HANDOFF => {
                let target = trimmed_json(&arguments, "persona");
                let note = trimmed_json(&arguments, "note");
                if target.is_empty() {
                    return Some(crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: "handoff needs a persona".to_string(),
                        images: Vec::new(),
                    });
                }
                let member = cast
                    .iter()
                    .find(|(id, name)| id == &target || name.eq_ignore_ascii_case(&target));
                match member {
                    Some((id, name)) if Some(id.as_str()) != persona_id => {
                        self.inner
                            .pending_handoffs
                            .lock()
                            .expect("handoffs mutex poisoned")
                            .insert(
                                session_id.to_string(),
                                PendingHandoff {
                                    persona_id: id.clone(),
                                    chain: handoff_chain + 1,
                                },
                            );
                        Ok(format!(
                            "Handed the turn to {name}.{}",
                            if note.is_empty() {
                                String::new()
                            } else {
                                format!(" Note: {note}")
                            }
                        ))
                    }
                    Some((_, name)) => Err(Error::other(format!(
                        "you cannot hand the turn to yourself ({name})"
                    ))),
                    None => Err(Error::other(format!(
                        "unknown cast member \"{target}\"; cast: {}",
                        cast.iter()
                            .map(|(_, name)| name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))),
                }
            }
            crate::tools::TODO_WRITE => {
                let todos = parse_todos(&arguments);
                self.replace_todos(session_id, todos)
            }
            crate::tools::TODO_READ => self.read_todos(session_id),
            _ => return None,
        };

        Some(match result {
            Ok(output) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: true,
                output,
                images: Vec::new(),
            },
            Err(error) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: error.to_string(),
                images: Vec::new(),
            },
        })
    }

    /// `list_commands` renders the tracked shell commands for the model.
    async fn list_commands_for_tool(&self) -> Result<String> {
        let commands = self.commands(None)?;
        if commands.is_empty() {
            return Ok("No shell commands have been run yet.".to_string());
        }
        let mut out = format!("{} command(s), newest first:\n", commands.len());
        for command in commands.iter().take(30) {
            let mut line = format!(
                "- {} [{}] {} ({}{})",
                command.label,
                command.id,
                command.status,
                command.command,
                command
                    .exit_code
                    .map(|code| format!(", exit {code}"))
                    .unwrap_or_default()
            );
            if command.status == "orphaned" {
                line.push_str(" — Loom restarted; it may still be running");
            }
            if command.background {
                line.push_str(" [background]");
            }
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }

    async fn generate_image(
        &self,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
        prompt: &str,
        size: Option<&str>,
    ) -> Result<String> {
        if prompt.trim().is_empty() {
            return Err(Error::other("generate_image needs a prompt"));
        }
        let reference = self.config().chat.image_model.clone();
        // `chat.imageModel` may name its provider, in which case that provider
        // and its key replace the chat's — see `aux_target`.
        let (image_model, provider, api_key) =
            self.aux_target(reference.as_ref(), "gpt-image-1", provider, api_key);

        let url = format!("{}/images/generations", provider.normalized_base_url());
        let mut request = self
            .inner
            .client
            .post(&url)
            .json(&crate::images::build_body(&image_model, prompt, size));
        if let Some(key) = api_key.as_deref().filter(|key| !key.trim().is_empty()) {
            request = request.header("authorization", format!("Bearer {key}"));
        }
        for (name, value) in &provider.headers {
            request = request.header(name, value);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Http(format!("image request failed: {e}")))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| Error::Provider(format!("unexpected image response: {e}")))?;

        let path = crate::images::parse_response(&value, status)?;
        let _ = model;
        Ok(path.to_string_lossy().into_owned())
    }

    async fn spawn_subagent(
        &self,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
        system: Option<&str>,
        task: &str,
        session_id: Option<&str>,
    ) -> Result<String> {
        if task.trim().is_empty() {
            return Err(Error::other("spawn_agent needs a task"));
        }

        let spec = context::model_spec(provider, &model.model_id);
        let request = ChatRequest {
            provider,
            model: &model.model_id,
            system: Some(system.unwrap_or(
                "You are a focused subagent. Complete the task and reply with the result only.",
            )),
            messages: vec![WireMessage::text("user", task)],
            variant: None,
            max_output_tokens: Some(context::output_limit(
                0,
                &spec,
                context::context_window(provider, &model.model_id),
            )),
            temperature: None,
            top_p: None,
            stream: false,
            tools: Vec::new(),
            session_id,
        };

        let (content, _, _) = stream::run_once(&self.inner.client, &request, api_key).await?;
        Ok(content)
    }

    /// Resolves a `spawn_agent` persona by id or name. A persona with its own
    /// model runs on that model; otherwise the caller's model is inherited.
    fn subagent_persona(
        &self,
        reference: &str,
        fallback_model: &ModelRef,
        fallback_provider: &ProviderConfig,
        fallback_key: Option<&str>,
    ) -> (Option<String>, ModelRef, ProviderConfig, Option<String>) {
        let config = self.config();
        let Some(persona) = config
            .personas
            .iter()
            .find(|p| p.id == reference || p.name.eq_ignore_ascii_case(reference))
        else {
            return (
                None,
                fallback_model.clone(),
                fallback_provider.clone(),
                fallback_key.map(str::to_string),
            );
        };
        let vars = persona_vars(&config, Some(persona), fallback_model, None);
        let system = persona.assemble(&vars);
        match persona.model_ref.clone() {
            Some(model_ref) => match config.providers.get(&model_ref.provider_id).cloned() {
                Some(provider) => {
                    let key = secrets::get_api_key(&model_ref.provider_id).ok().flatten();
                    (Some(system), model_ref, provider, key)
                }
                None => (
                    Some(system),
                    fallback_model.clone(),
                    fallback_provider.clone(),
                    fallback_key.map(str::to_string),
                ),
            },
            None => (
                Some(system),
                fallback_model.clone(),
                fallback_provider.clone(),
                fallback_key.map(str::to_string),
            ),
        }
    }

    // ------------------------------------------------------------------
    // Workspace index (RAG)
    // ------------------------------------------------------------------

    /// Chunks and embeds the chat's workspace folder. Returns the chunk count.
    pub async fn index_workspace(&self, session_id: &str) -> Result<usize> {
        let session = self
            .db()
            .get_session(session_id)?
            .ok_or_else(|| Error::UnknownSession(session_id.to_string()))?;
        let workdir = session
            .workdir
            .clone()
            .ok_or_else(|| Error::Other("this chat has no workspace folder set".into()))?;
        let model = self.effective_model(&session)?;

        let config = self.config();
        let fallback_provider = config
            .providers
            .get(&model.provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(model.provider_id.clone()))?;
        drop(config);

        let fallback_key = secrets::get_api_key(&model.provider_id)?;
        let (embedding_model, provider, api_key) =
            self.embedding_target(&fallback_provider, fallback_key.as_deref());
        let files = crate::index::collect_files(std::path::Path::new(&workdir));
        if files.is_empty() {
            return Err(Error::Other(
                "no indexable text files found in the workspace".into(),
            ));
        }

        let (target, overlap) = crate::index::chunk_settings();
        let mut items: Vec<(String, String)> = Vec::new();
        for (path, content) in &files {
            for chunk in crate::index::chunk_text(content, target, overlap) {
                items.push((path.clone(), chunk));
            }
        }

        let inputs: Vec<String> = items
            .iter()
            .map(|(path, content)| format!("File: {path}\n{content}"))
            .collect();

        let vectors = crate::embeddings::embed(
            &self.inner.client,
            &provider,
            api_key.as_deref(),
            &embedding_model,
            &inputs,
        )
        .await?;

        let chunks: Vec<crate::db::Chunk> = items
            .into_iter()
            .zip(vectors)
            .map(|((path, content), vector)| crate::db::Chunk {
                id: uuid::Uuid::new_v4().to_string(),
                path,
                content,
                embedding: crate::embeddings::encode(&vector),
            })
            .collect();

        let count = chunks.len();
        self.db().replace_chunks(session_id, &chunks)?;
        Ok(count)
    }

    pub fn index_status(&self, session_id: &str) -> Result<usize> {
        self.db().chunk_count(session_id)
    }

    pub fn clear_index(&self, session_id: &str) -> Result<()> {
        self.db().clear_chunks(session_id)
    }

    /// Semantic search over the stored chunks.
    async fn search_workspace(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<String> {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::Other("search_workspace needs a query".into()));
        }

        let session = self
            .db()
            .get_session(session_id)?
            .ok_or_else(|| Error::UnknownSession(session_id.to_string()))?;
        let model = self.effective_model(&session)?;

        let config = self.config();
        let fallback_provider = config
            .providers
            .get(&model.provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(model.provider_id.clone()))?;
        drop(config);

        let chunks = self.db().chunks(session_id)?;
        if chunks.is_empty() {
            return Ok(
                "The workspace index is empty. Ask the user to run \"Index workspace\" from the workspace menu first."
                    .to_string(),
            );
        }

        let fallback_key = secrets::get_api_key(&model.provider_id)?;
        let (embedding_model, provider, api_key) =
            self.embedding_target(&fallback_provider, fallback_key.as_deref());
        let vectors = crate::embeddings::embed(
            &self.inner.client,
            &provider,
            api_key.as_deref(),
            &embedding_model,
            &[query.to_string()],
        )
        .await?;
        let query_vector = vectors
            .into_iter()
            .next()
            .ok_or_else(|| Error::Provider("no query embedding returned".into()))?;

        let ranked = crate::index::rank(&chunks, &query_vector, limit.clamp(1, 12));
        if ranked.is_empty() {
            return Ok(format!("No indexed content matched \"{query}\"."));
        }

        let mut out = format!("{} relevant chunks for \"{query}\":\n", ranked.len());
        for (chunk, score) in ranked {
            let snippet: String = chunk.content.chars().take(1_200).collect();
            out.push_str(&format!(
                "\n--- {} (score {:.2}) ---\n{}\n",
                chunk.path, score, snippet
            ));
        }
        Ok(out)
    }

    /// Adds a model id by hand (for providers whose `/models` endpoint is
    /// unavailable). Metadata comes from the bundled catalog when known, but
    /// the spec is stamped `user`: a hand-picked id's metadata is the user's
    /// to keep. "Reset to detected" in settings hands it back to the catalog.
    pub fn add_model(&self, provider_id: &str, model_id: &str) -> Result<()> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err(Error::Other("model id must not be empty".into()));
        }

        let snapshot = {
            let mut config = lock_config(&self.inner.config);
            let provider = config
                .providers
                .get_mut(provider_id)
                .ok_or_else(|| Error::UnknownProvider(provider_id.to_string()))?;
            provider
                .models
                .entry(model_id.to_string())
                .or_insert_with(|| {
                    let mut spec = catalog::lookup(model_id).unwrap_or_else(catalog::fallback);
                    spec.source = MetadataSource::User;
                    spec
                });
            // Adding an id by hand is a deliberate act, so it is selected even
            // on a provider that is otherwise opt-in.
            provider.disabled_models.remove(model_id);
            config.clone()
        };
        crate::config::save(&snapshot)
    }

    /// Replaces the metadata the picker shows for one model. Every argument
    /// replaces the stored value (so `None` or empty clears it), and the spec
    /// is stamped `user` so a refresh never takes the edit back.
    pub fn set_model_spec(
        &self,
        provider_id: &str,
        model_id: &str,
        context: Option<u32>,
        output: Option<u32>,
        input_modalities: Vec<Modality>,
        reasoning: Option<ReasoningSpec>,
    ) -> Result<()> {
        let snapshot = {
            let mut config = lock_config(&self.inner.config);
            let model = config
                .providers
                .get_mut(provider_id)
                .and_then(|provider| provider.models.get_mut(model_id))
                .ok_or_else(|| Error::Other(format!("unknown model {provider_id}/{model_id}")))?;
            model.context = context;
            model.output = output;
            model.input_modalities = input_modalities;
            model.reasoning = reasoning;
            model.source = MetadataSource::User;
            config.clone()
        };
        crate::config::save(&snapshot)
    }

    /// Forgets every detected value for a model and re-reads the bundled
    /// catalog. The favourite flag survives; the source becomes `catalog@N`
    /// (or `unknown` when the catalog knows nothing), so refreshes can correct
    /// it again. This is the escape hatch for guesses kept by the migration.
    pub fn reset_model_spec(&self, provider_id: &str, model_id: &str) -> Result<()> {
        let snapshot = {
            let mut config = lock_config(&self.inner.config);
            let model = config
                .providers
                .get_mut(provider_id)
                .and_then(|provider| provider.models.get_mut(model_id))
                .ok_or_else(|| Error::Other(format!("unknown model {provider_id}/{model_id}")))?;
            let favorite = model.favorite;
            let mut detected = catalog::lookup(model_id).unwrap_or_else(catalog::fallback);
            detected.favorite = favorite;
            *model = detected;
            config.clone()
        };
        crate::config::save(&snapshot)
    }

    /// Marks the MCP connection cache stale (called when config changes).
    pub fn invalidate_mcp(&self) {
        if let Ok(mut state) = self.inner.mcp.try_lock() {
            state.dirty = true;
        }
    }

    /// Tool names currently offered by connected MCP servers, for the UI.
    pub async fn mcp_tool_count(&self, server: &str) -> usize {
        let state = self.inner.mcp.lock().await;
        state.tools.iter().filter(|(id, _)| id == server).count()
    }

    // ------------------------------------------------------------------
    // Titles
    // ------------------------------------------------------------------

    /// Names a chat, in two passes.
    ///
    /// [`TitlePass::Early`] runs the instant the user sends, from their own
    /// message alone. The row in the sidebar gets a real name immediately
    /// instead of sitting on "New chat" for however long the turn takes —
    /// which, on a slow model, is the difference between a list you can scan
    /// and one you have to open chats to navigate.
    ///
    /// [`TitlePass::Confirm`] runs once, after the first reply has landed. A
    /// title written from one side of a conversation is a guess, and by now
    /// there is more to go on — so it shows the model the existing title and
    /// the exchange, and asks whether the name still fits. It is told to reply
    /// `KEEP` when it does, so the common case costs no write and no visible
    /// change. This is the "check whether it is fine, and replace it if not"
    /// pass.
    ///
    /// A title the user typed is never touched: guesses are recorded in
    /// `inner.titles`, and anything that does not match what was recorded was
    /// renamed by hand.
    async fn maybe_generate_title(
        &self,
        session_id: &str,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
        pass: TitlePass,
    ) {
        let config = self.config();
        // Settings → Chat. This toggle was written by the settings UI, carried
        // through the harness schema, and then read by nothing at all: turning
        // auto-titles off changed nothing. Reading it here is the fix, and it
        // matters more now that a title is two model calls rather than one.
        if !config.chat.auto_title {
            return;
        }

        // One confirm per chat. Without this the question would be re-asked on
        // every turn for the life of the conversation, at one model call each.
        if pass == TitlePass::Confirm
            && self
                .inner
                .title_done
                .lock()
                .expect("title mutex poisoned")
                .contains(session_id)
        {
            return;
        }

        let session = match self.db().get_session(session_id) {
            Ok(Some(session)) => session,
            _ => return,
        };
        let existing = session.title.trim().to_string();

        if pass == TitlePass::Early {
            // Anything already there — the user's, or a name an earlier turn
            // produced — is left alone.
            if !existing.is_empty() {
                return;
            }
            // A confirm that has already run holds the better title, because it
            // read the reply. This pass is detached, so it can land *after* the
            // turn it was spawned from has finished; without this check a fast
            // reply could have its confirmed title overwritten by the guess
            // made before that reply existed.
            if self
                .inner
                .title_done
                .lock()
                .expect("title mutex poisoned")
                .contains(session_id)
            {
                return;
            }
        } else {
            // The confirm pass runs whatever the title is, including empty: an
            // early pass that failed (no key, a provider refusal) leaves it
            // blank, and blank is exactly the case worth filling in now that
            // there is a reply to read.
            let recorded = self
                .inner
                .titles
                .lock()
                .expect("title mutex poisoned")
                .get(session_id)
                .cloned();
            if !existing.is_empty() && !title_is_ours(&existing, recorded.as_deref()) {
                // Renamed by hand. Leave it alone, and do not ask again.
                self.inner
                    .title_done
                    .lock()
                    .expect("title mutex poisoned")
                    .insert(session_id.to_string());
                return;
            }
        }

        let messages = match self.db().messages(session_id) {
            Ok(messages) => messages,
            Err(_) => return,
        };
        let first_user = messages
            .iter()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.clone());
        let first_assistant = messages
            .iter()
            .find(|m| m.role == Role::Assistant && !m.content.is_empty())
            .map(|m| m.content.clone());

        let Some(user) = first_user.filter(|text| !text.trim().is_empty()) else {
            return;
        };
        // The early pass deliberately has only the user's side — that is what
        // makes it available before the reply exists.
        let assistant = match pass {
            TitlePass::Confirm => first_assistant.unwrap_or_default(),
            TitlePass::Early => String::new(),
        };

        let (title_provider_id, title_provider, title_model) = match config.chat.lite.clone() {
            Some(lite) => match config.providers.get(&lite.provider_id) {
                Some(provider) => (lite.provider_id.clone(), provider.clone(), lite.model_id),
                None => (
                    model.provider_id.clone(),
                    provider.clone(),
                    model.model_id.clone(),
                ),
            },
            None => (
                model.provider_id.clone(),
                provider.clone(),
                model.model_id.clone(),
            ),
        };

        let title_key = secrets::get_api_key(&title_provider_id).unwrap_or_else(|_| {
            if title_provider_id == model.provider_id {
                api_key.map(str::to_string)
            } else {
                None
            }
        });

        let (system, prompt) = match pass {
            TitlePass::Early => (
                "Write a title for this conversation from the user's opening message. Reply \
                 with the title only: at most 6 words, no quotes, no trailing punctuation.",
                format!("User: {}", truncate(&user, 600)),
            ),
            TitlePass::Confirm => (
                "You are shown the current title of a conversation and the exchange it \
                 describes. If the current title already fits, reply with exactly KEEP. \
                 Otherwise reply with a better title: at most 6 words, no quotes, no \
                 trailing punctuation. Judge strictly — a title that is vague, generic, or \
                 that merely echoes the user's greeting does not fit.",
                format!(
                    "Current title: {}\n\nUser: {}\n\nAssistant: {}",
                    if existing.is_empty() {
                        "(none)"
                    } else {
                        existing.as_str()
                    },
                    truncate(&user, 600),
                    truncate(&assistant, 600),
                ),
            ),
        };

        let request = ChatRequest {
            provider: &title_provider,
            model: &title_model,
            system: Some(system),
            messages: vec![WireMessage::text("user", prompt)],
            variant: None,
            // Generous on purpose: reasoning models spend the first tokens
            // thinking, and a tight cap leaves `content` empty so no title is
            // ever produced.
            max_output_tokens: Some(512),
            temperature: None,
            top_p: None,
            stream: false,
            tools: Vec::new(),
            session_id: Some(session_id),
        };

        let (content, reasoning) =
            match stream::run_once(&self.inner.client, &request, title_key.as_deref()).await {
                Ok((content, reasoning, _)) => (content, reasoning),
                Err(error) => {
                    eprintln!("[loom] title generation failed: {error}");
                    // A confirm that could not run is not an answer, so the flag
                    // stays clear and a later turn may still ask. An early pass
                    // has nothing to remember either way.
                    return;
                }
            };

        // Fall back to the tail of the model's thinking when it never got to
        // writing a reply.
        let candidate = if content.trim().is_empty() {
            reasoning.unwrap_or_default()
        } else {
            content
        };

        let title = match pass {
            TitlePass::Early => {
                let title = clean_title(candidate.lines().last().unwrap_or_default());
                if title.is_empty() {
                    eprintln!("[loom] title generation produced nothing usable");
                    return;
                }
                title
            }
            TitlePass::Confirm => match parse_confirm_reply(&candidate) {
                // Replacing a title with itself would emit a redundant update
                // and make the sidebar re-render for nothing.
                Some(next) if next != existing => next,
                // `KEEP`, an empty reply, or the same words back: the title
                // stands, and the question has now been answered.
                _ => {
                    self.inner
                        .title_done
                        .lock()
                        .expect("title mutex poisoned")
                        .insert(session_id.to_string());
                    return;
                }
            },
        };

        let _ = self.db().update_session(
            session_id,
            SessionUpdate {
                title: Some(&title),
                ..Default::default()
            },
        );
        // Record what *we* wrote before announcing it, so a confirm pass that
        // somehow races this one still recognises the title as its own.
        self.inner
            .titles
            .lock()
            .expect("title mutex poisoned")
            .insert(session_id.to_string(), title.clone());
        if pass == TitlePass::Confirm {
            self.inner
                .title_done
                .lock()
                .expect("title mutex poisoned")
                .insert(session_id.to_string());
        }
        self.emit(EngineEvent::Title {
            session_id: session_id.to_string(),
            title,
        });
    }

    // ------------------------------------------------------------------
    // Shell commands (background runs, and timed-out foreground ones)
    // ------------------------------------------------------------------

    pub fn commands(&self, session_id: Option<&str>) -> Result<Vec<CommandRun>> {
        self.db().list_commands(session_id)
    }

    pub fn command(&self, id: &str) -> Result<Option<CommandRun>> {
        self.db().command(id)
    }

    /// What a command has produced so far. Reads the log file, which is
    /// flushed after every chunk, so this is the whole story for a finished
    /// command and the story so far for a running one.
    pub fn command_output(&self, id: &str, lines: usize) -> Result<String> {
        let record = self
            .db()
            .command(id)?
            .ok_or_else(|| Error::other(format!("no command with id {id}")))?;

        let path = std::path::PathBuf::from(&record.log_path);
        match crate::process::log_tail(&path, lines) {
            Ok(text) if text != "(no output yet)" => Ok(text),
            // Nothing on disk yet: the process may not have written its first
            // line, or its log could not be opened. The handle holds what
            // arrived, so fall back to that before reporting nothing.
            other => {
                if let Some(handle) = self.command_handle(id) {
                    let (stdout, stderr) = handle.tails();
                    let mut text = String::new();
                    if !stdout.trim().is_empty() {
                        text.push_str("stdout:\n");
                        text.push_str(&stdout);
                    }
                    if !stderr.trim().is_empty() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str("stderr:\n");
                        text.push_str(&stderr);
                    }
                    if !text.trim().is_empty() {
                        return Ok(tools::truncate_output(&text));
                    }
                }
                other
            }
        }
    }

    /// Starts a command that outlives the turn: hidden, logged, tracked, and
    /// stoppable by id.
    pub fn start_command(
        &self,
        session_id: Option<&str>,
        cwd: &std::path::Path,
        command: &str,
        label: Option<&str>,
        background: bool,
    ) -> Result<CommandRun> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(Error::other("running a command needs a Tokio runtime"));
        }
        // Claim the slot while the count is held, so the check and the claim
        // cannot be torn apart by another start. Released below if the spawn
        // fails, and consumed by `track_command` when the row is written.
        {
            let mut tracker = self.inner.commands.lock().expect("commands mutex poisoned");
            if tracker.handles.len() + tracker.starting >= MAX_BACKGROUND_COMMANDS {
                return Err(Error::other(format!(
                    "{MAX_BACKGROUND_COMMANDS} commands are already running — wait for one to \
                     finish, or stop one with stop_command"
                )));
            }
            tracker.starting += 1;
        }

        let log_path = crate::process::command_log_path(&uuid::Uuid::new_v4().to_string())?;
        let running = match Running::spawn(command, cwd, &log_path) {
            Ok(running) => running,
            Err(error) => {
                // Give the claimed slot back, or a failed spawn would quietly
                // shrink the cap.
                if let Ok(mut tracker) = self.inner.commands.lock() {
                    tracker.starting = tracker.starting.saturating_sub(1);
                }
                return Err(error);
            }
        };
        self.track_command(
            session_id, cwd, command, label, background, log_path, running, true,
        )
    }

    /// Records an already-spawned process and watches it to completion.
    ///
    /// `claimed` says whether the caller reserved a slot for it: `true` from
    /// `start_command`, which counted it against [`MAX_BACKGROUND_COMMANDS`],
    /// and `false` for an adopted command — a foreground one that outlived its
    /// cap. Adopting is deliberately exempt from the cap, because the process
    /// exists either way and refusing to track it would leave it invisible and
    /// unstoppable.
    #[allow(clippy::too_many_arguments)]
    fn track_command(
        &self,
        session_id: Option<&str>,
        cwd: &std::path::Path,
        command: &str,
        label: Option<&str>,
        background: bool,
        log_path: std::path::PathBuf,
        running: Running,
        claimed: bool,
    ) -> Result<CommandRun> {
        let record = CommandRun {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.map(str::to_string),
            label: label
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| command_label(command)),
            command: command.to_string(),
            cwd: cwd.to_string_lossy().into_owned(),
            pid: running.pid(),
            status: "running".to_string(),
            exit_code: None,
            log_path: log_path.to_string_lossy().into_owned(),
            background,
            created_at: now_ms(),
            finished_at: None,
        };
        self.db().insert_command(&record)?;

        {
            let mut tracker = self.inner.commands.lock().expect("commands mutex poisoned");
            tracker
                .handles
                .insert(record.id.clone(), running.tail_handle());
            if claimed {
                tracker.starting = tracker.starting.saturating_sub(1);
            }
        }

        // One watcher per command: it owns the child, and ends when the
        // process does, whatever ended it (exit, stop_command, or a crash).
        let engine = self.clone();
        let watched = record.id.clone();
        tokio::spawn(async move {
            let mut running = running;
            let status = running.wait().await;
            // Drain the pipes last, so the log holds the final lines before
            // anything reads it back.
            running.finish().await;
            engine.finish_command(&watched, status.and_then(|status| status.code()));
        });

        self.emit(EngineEvent::CommandChanged {
            command: record.clone(),
        });
        Ok(record)
    }

    /// `run_command`: wait up to [`crate::process::COMMAND_TIMEOUT`], then
    /// hand a command that is still going to the background tracker. It is
    /// never killed — the user asked for a long build to be allowed to finish.
    async fn run_shell_command(
        &self,
        session_id: &str,
        context: &ToolContext,
        arguments: &serde_json::Value,
    ) -> Result<String> {
        let command = arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if command.is_empty() {
            return Err(Error::other("run_command needs a command"));
        }
        let root = context
            .workdir
            .clone()
            .ok_or_else(|| Error::other("this chat has no workspace folder set"))?;
        let label = arguments
            .get("label")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let background = arguments
            .get("background")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if background {
            let record =
                self.start_command(Some(session_id), &root, &command, label.as_deref(), true)?;
            return Ok(format!(
                "started \"{}\" in the background (id {}, pid {}). It is still running and will \
                 keep running after this turn. Its output is being written to {} — read what \
                 it has produced with `command_output`, and end it with `stop_command`. Do not \
                 wait for it.",
                record.label, record.id, record.pid, record.log_path
            ));
        }

        let log_path = crate::process::command_log_path(&uuid::Uuid::new_v4().to_string())?;
        let mut running = Running::spawn(&command, &root, &log_path)?;

        match running.wait_timeout(crate::process::COMMAND_TIMEOUT).await {
            Wait::Exited(status) => {
                let (stdout, stderr) = running.finish().await;
                drop(running);
                // A command that finished inside the cap is reported in the
                // transcript like any other tool; there is nothing to track,
                // so the log goes with it.
                let _ = std::fs::remove_file(&log_path);
                Ok(format_command_report(status.code(), &stdout, &stderr))
            }
            Wait::Running | Wait::Unknown => {
                // Still going after two minutes (or the wait itself failed, so
                // we no longer know: adopting it is the safe reading, since
                // the process is not ours to declare dead).
                let record = self.track_command(
                    Some(session_id),
                    &root,
                    &command,
                    label.as_deref(),
                    false,
                    log_path.clone(),
                    running,
                    false,
                )?;
                let tail = crate::process::log_tail(&log_path, 40).unwrap_or_default();
                let mut report = format!(
                    "still running after {}s (id {}, pid {}). It was not stopped — it keeps \
                     running, and its output goes to the log below. Read more with \
                     `command_output`, or end it with `stop_command`.\n",
                    crate::process::COMMAND_TIMEOUT.as_secs(),
                    record.id,
                    record.pid,
                );
                if !tail.trim().is_empty() {
                    report.push_str("output so far:\n");
                    report.push_str(&tools::truncate_output(&tail));
                }
                Ok(report)
            }
        }
    }

    fn command_handle(&self, id: &str) -> Option<TailHandle> {
        self.inner.commands.lock().ok()?.handles.get(id).cloned()
    }

    /// Records a command's exit and drops its handle.
    ///
    /// Only writes when the row still says `running`: if anything else already
    /// ended it — the user pressed Stop, or the row was deleted — that account
    /// of what happened stands, and "stopped" outranks the exit code of a
    /// process that was killed on purpose.
    fn finish_command(&self, id: &str, exit_code: Option<i32>) {
        // The row is read and the guard let go *before* `set_command_status`,
        // which takes the same lock. Binding the read instead of writing it
        // inline as an `if let` scrutinee is the whole point: a `MutexGuard`
        // held there lives to the end of the block, so a second `self.db()`
        // inside it re-locks the same non-reentrant mutex and hangs the
        // thread — with the lock still held, so every other database user
        // blocks behind it and Loom freezes for good.
        let current = self.db().command(id).ok().flatten();
        if let Some(record) = current {
            if record.status == "running" {
                let status = if exit_code == Some(0) {
                    "done"
                } else {
                    "failed"
                };
                if let Err(error) = self.db().set_command_status(id, status, exit_code) {
                    eprintln!("[loom] could not update command {id}: {error}");
                }
            }
        }
        if let Ok(mut tracker) = self.inner.commands.lock() {
            tracker.handles.remove(id);
        }
        // Only announce it if the row is still there (it may have been deleted
        // while the process was winding down).
        let current = self.db().command(id).ok().flatten();
        if let Some(command) = current {
            self.emit(EngineEvent::CommandChanged { command });
        }
    }

    /// Ends a running command and everything it spawned.
    pub fn stop_command(&self, id: &str) -> Result<CommandRun> {
        let record = self
            .db()
            .command(id)?
            .ok_or_else(|| Error::other(format!("no command with id {id}")))?;
        if record.status != "running" {
            return Err(Error::other(format!(
                "command {id} is not running (it is {})",
                record.status
            )));
        }
        // `false` means the signal could not be delivered at all — usually
        // because the process had already exited, occasionally because it
        // refused. The row still moves to `stopped`: the user asked for it to
        // end and Loom is no longer waiting on it, so leaving it `running`
        // would be a lie the Stop button could never correct.
        let delivered = crate::process::kill_tree(record.pid);
        self.db().set_command_status(id, "stopped", None)?;
        let updated = self.db().command(id)?.unwrap_or(record);
        if !delivered {
            // Not an error to the user: the commonest cause is that the
            // process had already exited. Worth a log line, because the other
            // cause is a tree that resisted and is still running.
            eprintln!("[loom] command {id} was marked stopped but the signal was not delivered");
        }
        self.emit(EngineEvent::CommandChanged {
            command: updated.clone(),
        });
        Ok(updated)
    }

    /// Forgets a command, stopping it first: deleting the row of a live
    /// process would leave a process nobody can find again.
    pub fn delete_command(&self, id: &str) -> Result<()> {
        // Nothing blocking happens with the database lock held: `kill_tree`
        // waits on a process, and this runs from an IPC handler the user is
        // looking at, so holding it would stall every other reader.
        let live = self
            .db()
            .command(id)?
            .filter(|record| record.status == "running")
            .map(|record| record.pid);
        if let Some(pid) = live {
            crate::process::kill_tree(pid);
        }
        // Re-read for the log path: the row may have gone while we killed it.
        let log_path = self.db().command(id)?.and_then(|record| {
            if record.log_path.trim().is_empty() {
                None
            } else {
                Some(record.log_path)
            }
        });
        if let Some(log_path) = log_path {
            let _ = std::fs::remove_file(&log_path);
        }
        if let Ok(mut tracker) = self.inner.commands.lock() {
            tracker.handles.remove(id);
        }
        self.db().delete_command(id)
    }

    /// After a restart, commands recorded as running are no longer ours. Loom
    /// does not kill background commands when it quits, so the process may
    /// well still be alive — the row says so rather than pretending it ended.
    /// Called once at launch.
    pub fn mark_interrupted_commands(&self) -> usize {
        let unfinished = match self.db().running_commands() {
            Ok(commands) => commands,
            Err(_) => return 0,
        };
        let count = unfinished.len();
        for command in unfinished {
            if let Err(error) = self.db().set_command_status(&command.id, "orphaned", None) {
                eprintln!("[loom] could not reconcile command {}: {error}", command.id);
                continue;
            }
            if crate::process::is_alive(command.pid) {
                eprintln!(
                    "[loom] command {} ({}) was left running by a previous session",
                    command.id, command.command
                );
            }
        }
        count
    }

    // ------------------------------------------------------------------
    // Detached runs (background subagents, job firings)
    // ------------------------------------------------------------------

    pub fn tasks(&self, job_id: Option<&str>) -> Result<Vec<crate::db::Task>> {
        self.db().list_tasks(job_id)
    }

    pub fn task(&self, id: &str) -> Result<Option<crate::db::Task>> {
        self.db().task(id)
    }

    pub fn delete_task(&self, id: &str) -> Result<()> {
        self.db().delete_task(id)
    }

    fn session_persona_id(&self, session_id: &str) -> Option<String> {
        self.db()
            .get_session(session_id)
            .ok()
            .flatten()
            .and_then(|session| session.persona_id)
    }

    /// Creates the task row and its hidden session, then queues the run.
    pub fn spawn_task(&self, request: TaskRequest) -> Result<String> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(Error::Other("spawn_task needs a Tokio runtime".into()));
        }
        let prompt = request.prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(Error::Other("a task needs a prompt".into()));
        }

        let config = self.config();
        let (provider_id, model_id) = match (request.provider_id.clone(), request.model_id.clone())
        {
            (Some(provider), Some(model)) => (provider, model),
            _ => match (
                config.chat.provider_id.clone(),
                config.chat.model_id.clone(),
            ) {
                (Some(provider), Some(model)) => (provider, model),
                _ => return Err(Error::Other("no model selected for this task".into())),
            },
        };
        drop(config);

        let now = now_ms();
        let session_id = uuid::Uuid::new_v4().to_string();
        self.db().create_task_session(&Session {
            id: session_id.clone(),
            title: request.title.clone(),
            provider_id: Some(provider_id.clone()),
            model_id: Some(model_id.clone()),
            variant: None,
            persona_id: request.persona_id.clone(),
            system_prompt: None,
            workdir: request.workdir.clone(),
            permission_mode: request.permission_mode.clone(),
            agent_mode: Some("build".to_string()),
            computer_access: false,
            browser_access: false,
            position: None,
            created_at: now,
            updated_at: now,
        })?;

        let task = crate::db::Task {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            origin_session: request.origin_session.clone(),
            job_id: request.job_id.clone(),
            title: request.title.clone(),
            prompt,
            provider_id: Some(provider_id),
            model_id: Some(model_id),
            status: "queued".to_string(),
            detail: None,
            result: None,
            notify: request.notify,
            created_at: now,
            started_at: None,
            finished_at: None,
        };
        self.db().create_task(&task)?;
        self.emit(EngineEvent::TaskChanged { task: task.clone() });

        {
            let mut queue = self.inner.task_queue.lock().expect("task queue poisoned");
            queue.waiting.push_back(task.id.clone());
        }
        self.pump_tasks();

        Ok(task.id)
    }

    /// Re-runs a finished task as a new run, keeping the old row as history.
    pub fn retry_task(&self, id: &str) -> Result<String> {
        let task = self
            .db()
            .task(id)?
            .ok_or_else(|| Error::Other("no such task".into()))?;
        if matches!(task.status.as_str(), "queued" | "running") {
            return Err(Error::Other("this task is already running".into()));
        }
        self.spawn_task(TaskRequest {
            prompt: task.prompt,
            title: task.title,
            origin_session: task.origin_session,
            job_id: task.job_id,
            provider_id: task.provider_id,
            model_id: task.model_id,
            persona_id: None,
            workdir: self
                .db()
                .get_session(&task.session_id)
                .ok()
                .flatten()
                .and_then(|session| session.workdir),
            permission_mode: None,
            notify: task.notify,
            max_steps: None,
            max_cost_usd: None,
        })
    }

    pub fn cancel_task(&self, id: &str) -> Result<()> {
        let task = self
            .db()
            .task(id)?
            .ok_or_else(|| Error::Other("no such task".into()))?;
        match task.status.as_str() {
            "running" => {
                self.cancel(&task.session_id);
                self.update_task(id, "cancelled", Some("stopped by the user"), None);
                Ok(())
            }
            "queued" => {
                {
                    let mut queue = self.inner.task_queue.lock().expect("task queue poisoned");
                    queue.waiting.retain(|waiting| waiting != id);
                }
                self.update_task(id, "cancelled", Some("cancelled before it started"), None);
                Ok(())
            }
            _ => Err(Error::Other("this task has already finished".into())),
        }
    }

    /// Starts queued runs while there is room. Cheap: just bookkeeping and
    /// `tokio::spawn`.
    fn pump_tasks(&self) {
        loop {
            let task_id = {
                let mut queue = self.inner.task_queue.lock().expect("task queue poisoned");
                if queue.running >= TASK_CONCURRENCY {
                    return;
                }
                match queue.waiting.pop_front() {
                    Some(id) => {
                        queue.running += 1;
                        id
                    }
                    None => return,
                }
            };

            let engine = self.clone();
            tokio::spawn(async move {
                engine.run_task(task_id).await;
            });
        }
    }

    fn task_slot_done(&self) {
        {
            let mut queue = self.inner.task_queue.lock().expect("task queue poisoned");
            queue.running = queue.running.saturating_sub(1);
        }
        self.pump_tasks();
    }

    async fn run_task(&self, task_id: String) {
        let task = match self.db().task(&task_id) {
            Ok(Some(task)) => task,
            _ => {
                self.task_slot_done();
                return;
            }
        };
        if task.status != "queued" {
            self.task_slot_done();
            return;
        }

        self.update_task(&task_id, "running", Some("running"), None);

        let model = match (task.provider_id.clone(), task.model_id.clone()) {
            (Some(provider_id), Some(model_id)) => Some(ModelRef::new(provider_id, model_id)),
            _ => None,
        };
        let limits = TurnLimits {
            max_steps: Some(TASK_MAX_STEPS),
            max_cost_usd: None,
            task_id: Some(task_id.clone()),
        };

        if let Err(error) =
            self.send_limited(&task.session_id, &task.prompt, model, Vec::new(), limits)
        {
            self.complete_task(&task_id, "failed", Some(&error.to_string()), None);
        }
    }

    fn update_task(&self, task_id: &str, status: &str, detail: Option<&str>, result: Option<&str>) {
        if let Err(error) = self.db().set_task_status(task_id, status, detail, result) {
            eprintln!("[loom] could not update task {task_id}: {error}");
            return;
        }
        // Read into a local, then emit. `emit` crosses into the shell to reach
        // the webview, so the database guard has no business being held across
        // it: `TaskChanged` fires on every run status change, and this is on
        // the path that a permission prompt is waiting behind.
        let task = self.db().task(task_id).ok().flatten();
        if let Some(task) = task {
            self.emit(EngineEvent::TaskChanged { task });
        }
    }

    /// Terminal update for a run: records the outcome, frees the slot, and
    /// starts the next queued run.
    fn complete_task(
        &self,
        task_id: &str,
        status: &str,
        detail: Option<&str>,
        result: Option<&str>,
    ) {
        self.update_task(task_id, status, detail, result);

        // Post the outcome into the chat that asked for the run, so the result
        // lands in that conversation's context, not only the Runs panel.
        if matches!(status, "done" | "failed") {
            // Bind the row and drop the guard before `add_message`, which locks
            // the database again: inline, this froze the app on every
            // background run that finished with a chat behind it.
            let origin = self.db().task(task_id).ok().flatten().and_then(|task| {
                task.origin_session
                    .clone()
                    .map(|origin| (origin, task.title))
            });
            if let Some((origin, title)) = origin {
                let text = match (status, result) {
                    ("done", Some(result)) if !result.trim().is_empty() => {
                        format!("Background run \"{title}\" finished:\n\n{result}")
                    }
                    ("done", _) => format!("Background run \"{title}\" finished."),
                    (_, Some(detail)) if !detail.trim().is_empty() => {
                        format!("Background run \"{title}\" failed: {detail}")
                    }
                    _ => format!("Background run \"{title}\" failed."),
                };
                let _ = self.db().add_message(&Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: origin,
                    role: Role::Assistant,
                    content: text,
                    reasoning: None,
                    extra: None,
                    persona_id: None,
                    created_at: now_ms(),
                });
            }
        }

        self.task_slot_done();
    }

    // ------------------------------------------------------------------
    // Long-term memory
    // ------------------------------------------------------------------

    /// The memory block for a turn: pinned facts plus anything the current
    /// message matches. `None` when there is nothing to inject.
    async fn long_term_memory_block(
        &self,
        session_id: &str,
        workdir: Option<&std::path::Path>,
        provider: &ProviderConfig,
        api_key: Option<&str>,
    ) -> Option<String> {
        let scopes = crate::memory::scopes_for(workdir.and_then(|path| path.to_str()));
        let memories = self.db().memories(None).ok()?;
        let mut pinned: Vec<crate::db::Memory> = memories
            .iter()
            .filter(|memory| memory.pinned && scopes.contains(&memory.scope))
            .cloned()
            .collect();
        pinned.sort_by_key(|memory| std::cmp::Reverse(memory.updated_at));

        let query = self
            .db()
            .messages(session_id)
            .ok()?
            .into_iter()
            .rev()
            .find(|message| message.role == Role::User)
            .map(|message| message.content);

        let recalled = match query {
            Some(text) if !text.trim().is_empty() => self
                .recall_facts(
                    &scopes,
                    &text,
                    crate::memory::RECALL_LIMIT,
                    provider,
                    api_key,
                )
                .await
                .unwrap_or_default(),
            _ => Vec::new(),
        };

        crate::memory::prompt_block(&pinned, &recalled)
    }

    /// Ranked memories for a query. Empty (rather than an error) when the
    /// provider cannot embed: memory then degrades to pinned facts and text
    /// matching, which the caller handles.
    async fn recall_facts(
        &self,
        scopes: &[String],
        query: &str,
        limit: usize,
        provider: &ProviderConfig,
        api_key: Option<&str>,
    ) -> Result<Vec<crate::db::Memory>> {
        let memories = self.db().memories_in_scopes(scopes)?;
        if memories.is_empty() {
            return Ok(Vec::new());
        }

        let vectors = match self
            .embed_facts(provider, api_key, &[query.to_string()])
            .await
        {
            Ok(vectors) => vectors,
            Err(_) => return Ok(Vec::new()),
        };
        let Some(query_vector) = vectors.into_iter().next() else {
            return Ok(Vec::new());
        };

        Ok(crate::memory::rank(&memories, &query_vector, limit)
            .into_iter()
            .map(|(memory, _)| memory.clone())
            .collect())
    }

    async fn embed_facts(
        &self,
        provider: &ProviderConfig,
        api_key: Option<&str>,
        inputs: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        // Callers hand in the *chat's* provider and key, which is only right
        // when the embedding ref is unqualified. A qualified ref — the same
        // embedding id configured on two plans — settles it here instead.
        let (embedding_model, provider, api_key) = self.embedding_target(provider, api_key);

        crate::embeddings::embed(
            &self.inner.client,
            &provider,
            api_key.as_deref(),
            &embedding_model,
            inputs,
        )
        .await
    }

    /// Where an embedding call should go: model id, provider, and that
    /// provider's own key.
    fn embedding_target(
        &self,
        fallback_provider: &ProviderConfig,
        fallback_key: Option<&str>,
    ) -> (String, ProviderConfig, Option<String>) {
        let reference = self.config().chat.embedding_model.clone();
        self.aux_target(
            reference.as_ref(),
            "text-embedding-3-small",
            fallback_provider,
            fallback_key,
        )
    }

    /// Resolves an auxiliary model ref to the model id to send, the provider to
    /// send it to, and that provider's key.
    ///
    /// The ref decides *both* the id and the provider, because two instances of
    /// one vendor mean the same id can live on two plans with two keys — sending
    /// it to the chat's provider would bill the wrong account. When the ref does
    /// not resolve (unset, or an id no configured provider serves) the chat's
    /// own provider and key are used, which is exactly what these calls did
    /// before a ref could name a provider: an untouched config is unchanged.
    fn aux_target(
        &self,
        reference: Option<&crate::config::AuxModelRef>,
        default_model_id: &str,
        fallback_provider: &ProviderConfig,
        fallback_key: Option<&str>,
    ) -> (String, ProviderConfig, Option<String>) {
        let config = self.config();
        let resolved = reference
            .filter(|reference| !reference.model_id.trim().is_empty())
            .and_then(|reference| crate::config::resolve_aux_model(&config, reference));

        if let Some(resolution) = resolved {
            let model = resolution.model().clone();
            if let Some(provider) = config.providers.get(&model.provider_id).cloned() {
                let key = secrets::get_api_key(&model.provider_id).ok().flatten();
                return (model.model_id, provider, key);
            }
        }

        let model_id = reference
            .map(|reference| reference.model_id.trim().to_string())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| default_model_id.to_string());
        (
            model_id,
            fallback_provider.clone(),
            fallback_key.map(str::to_string),
        )
    }

    /// Stores one fact, replacing a near-duplicate in the same scope. Returns
    /// the stored memory and whether an existing row was updated.
    #[allow(clippy::too_many_arguments)]
    async fn store_fact(
        &self,
        scope: &str,
        content: &str,
        pinned: bool,
        source: &str,
        session_id: Option<&str>,
        message_id: Option<&str>,
        provider: &ProviderConfig,
        api_key: Option<&str>,
    ) -> Result<(crate::db::Memory, bool)> {
        let content = content.trim();
        if content.is_empty() {
            return Err(Error::Other("a memory needs some text".into()));
        }

        let embedding = self
            .embed_facts(provider, api_key, &[content.to_string()])
            .await
            .ok()
            .and_then(|mut vectors| vectors.pop());
        let bytes = embedding
            .as_ref()
            .map(|vector| crate::embeddings::encode(vector));

        let existing = self.db().memories(Some(scope))?;
        if let Some(index) = embedding
            .as_ref()
            .and_then(|vector| crate::memory::near_duplicate(&existing, vector))
        {
            let old = &existing[index];
            self.db()
                .update_memory(&old.id, content, old.pinned || pinned, bytes.as_deref())?;
            let fresh = self
                .db()
                .memories(Some(scope))?
                .into_iter()
                .find(|memory| memory.id == old.id)
                .unwrap_or_else(|| old.clone());
            return Ok((fresh, true));
        }

        let memory = crate::memory::new_memory(
            scope, content, pinned, source, session_id, message_id, bytes,
        );
        self.db().insert_memory(&memory)?;
        Ok((memory, false))
    }

    /// Forgets a memory by id, or by unambiguous text match.
    fn forget_fact(&self, id: Option<&str>, query: Option<&str>) -> Result<String> {
        if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
            self.db().delete_memory(id.trim())?;
            return Ok("Deleted that memory.".to_string());
        }

        let query = query
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| Error::Other("forget_fact needs an id or a query".into()))?;
        let needle = query.to_lowercase();
        let matches: Vec<crate::db::Memory> = self
            .db()
            .memories(None)?
            .into_iter()
            .filter(|memory| memory.content.to_lowercase().contains(&needle))
            .collect();

        match matches.len() {
            0 => Ok(format!("No memory matches \"{query}\".")),
            1 => {
                self.db().delete_memory(&matches[0].id)?;
                Ok(format!("Forgot: {}", matches[0].content))
            }
            _ => {
                let list = matches
                    .iter()
                    .take(8)
                    .map(|memory| format!("- [{}] {}", memory.id, memory.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(format!(
                    "Several memories match; delete one by id with forget_fact:\n{list}"
                ))
            }
        }
    }

    /// Kicks off the background extraction pass for an interactive turn.
    /// Cheap no-op when the setting is off or nothing new happened.
    fn maybe_extract_memories(
        &self,
        session_id: &str,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
    ) {
        if !self.config().interface.auto_memory {
            return;
        }

        let watermark = self
            .inner
            .memory_scan
            .lock()
            .expect("memory scan mutex poisoned")
            .get(session_id)
            .copied()
            .unwrap_or(0);

        let session = match self.db().get_session(session_id) {
            Ok(Some(session)) => session,
            _ => return,
        };
        let messages = match self.db().messages(session_id) {
            Ok(messages) => messages,
            Err(_) => return,
        };
        let fresh: Vec<&Message> = messages
            .iter()
            .filter(|message| message.created_at > watermark && !message.content.trim().is_empty())
            .collect();
        if fresh.is_empty() {
            return;
        }

        let newest = fresh
            .iter()
            .map(|message| message.created_at)
            .max()
            .unwrap_or(watermark);
        let excerpt = fresh
            .iter()
            .rev()
            .take(12)
            .rev()
            .map(|message| {
                let role = if message.role == Role::User {
                    "User"
                } else {
                    "Assistant"
                };
                format!("{role}: {}", truncate(&message.content, 900))
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let last_message = fresh
            .iter()
            .rev()
            .find(|message| message.role == Role::Assistant)
            .map(|message| message.id.clone())
            .unwrap_or_default();

        // The lite model does extraction, like titles: cheap and frequent.
        let config = self.config();
        let (fact_provider_id, fact_provider, fact_model) = match config.chat.lite.clone() {
            Some(lite) => match config.providers.get(&lite.provider_id) {
                Some(lite_provider) => (
                    lite.provider_id.clone(),
                    lite_provider.clone(),
                    lite.model_id,
                ),
                None => (
                    model.provider_id.clone(),
                    provider.clone(),
                    model.model_id.clone(),
                ),
            },
            None => (
                model.provider_id.clone(),
                provider.clone(),
                model.model_id.clone(),
            ),
        };
        drop(config);
        let fact_key = secrets::get_api_key(&fact_provider_id).unwrap_or_else(|_| {
            if fact_provider_id == model.provider_id {
                api_key.map(str::to_string)
            } else {
                None
            }
        });

        let engine = self.clone();
        let session_id = session_id.to_string();
        let workdir = session.workdir;
        tokio::spawn(async move {
            engine
                .run_extraction(
                    session_id,
                    workdir,
                    excerpt,
                    newest,
                    last_message,
                    fact_provider,
                    fact_model,
                    fact_key,
                )
                .await;
        });
    }

    /// The extraction pass: ask the lite model for durable facts, store them,
    /// and advance the session watermark. Never fails the turn.
    #[allow(clippy::too_many_arguments)]
    async fn run_extraction(
        &self,
        session_id: String,
        workdir: Option<String>,
        excerpt: String,
        watermark: i64,
        message_id: String,
        provider: ProviderConfig,
        model_id: String,
        api_key: Option<String>,
    ) {
        let existing = self.db().memories(None).unwrap_or_default();
        let prompt = crate::memory::extraction_prompt(&excerpt, &existing);
        let request = ChatRequest {
            provider: &provider,
            model: &model_id,
            system: Some(
                "You extract durable facts for a personal assistant's long-term memory. \
                 Reply with the JSON array only.",
            ),
            messages: vec![WireMessage::text("user", prompt)],
            variant: None,
            max_output_tokens: Some(1_024),
            temperature: None,
            top_p: None,
            stream: false,
            tools: Vec::new(),
            session_id: Some(&session_id),
        };

        let (content, reasoning) =
            match stream::run_once(&self.inner.client, &request, api_key.as_deref()).await {
                Ok((content, reasoning, _)) => (content, reasoning),
                Err(_) => {
                    self.advance_memory_watermark(&session_id, watermark);
                    return;
                }
            };
        let raw = if content.trim().is_empty() {
            reasoning.unwrap_or_default()
        } else {
            content
        };

        let facts = crate::memory::parse_extraction(&raw);
        let mut added = 0usize;
        let mut last_scope = crate::memory::GLOBAL.to_string();
        for fact in facts {
            let scope = crate::memory::scope_for_fact(&fact.scope, workdir.as_deref());
            match self
                .store_fact(
                    &scope,
                    &fact.fact,
                    false,
                    "auto",
                    Some(&session_id),
                    Some(&message_id),
                    &provider,
                    api_key.as_deref(),
                )
                .await
            {
                Ok((memory, _)) => {
                    added += 1;
                    last_scope = memory.scope;
                }
                Err(error) => eprintln!("[loom] memory write skipped: {error}"),
            }
        }

        self.advance_memory_watermark(&session_id, watermark);
        if added > 0 {
            self.emit(EngineEvent::MemoryChanged {
                scope: last_scope,
                added,
                session_id,
                message_id,
            });
        }
    }

    /// Kicks off the background summary pass for an interactive turn.
    ///
    /// Cheap no-op until the request comes close to its budget. The point of
    /// running it that early is that a written summary is already on file when
    /// the fold first becomes *necessary*, so the common case is a real
    /// summary rather than the in-process digest.
    ///
    /// Watermarked on the fold point, so it fires once per fold rather than
    /// once per turn, and never while condensing is switched off.
    fn maybe_condense(
        &self,
        session_id: &str,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
    ) {
        let config = self.config();
        let share = config
            .chat
            .condense_share
            .min(crate::condense::MAX_CONDENSED_SHARE);
        let configured = config.chat.max_output_tokens;
        drop(config);
        if share == 0 {
            return;
        }

        let session = match self.db().get_session(session_id) {
            Ok(Some(session)) => session,
            _ => return,
        };
        let history = match self.db().messages(session_id) {
            Ok(history) => history,
            Err(_) => return,
        };
        // The live turn is everything from the last user message onwards; the
        // summary covers what has aged out before it. Nothing has aged out on a
        // first turn, so there is nothing to fold.
        let Some(last_user) = history
            .iter()
            .rposition(|message| message.role == Role::User)
        else {
            return;
        };
        if last_user == 0 {
            return;
        }
        let covered = &history[..last_user];
        let covers_through_id = history[last_user - 1].id.clone();
        let covers_through_at = history[last_user - 1].created_at;

        // Once per fold point. The stored summary is the durable record; this
        // map is what stops a pass that keeps failing (no key, a provider
        // refusal) from being retried on every turn of a long chat.
        {
            let mut scan = self
                .inner
                .condense_scan
                .lock()
                .expect("condense scan mutex poisoned");
            if scan.get(session_id) == Some(&covers_through_id) {
                return;
            }
            scan.insert(session_id.to_string(), covers_through_id.clone());
        }

        let spec = context::model_spec(provider, &model.model_id);
        let window = context::context_window(provider, &model.model_id);
        let max_output = context::output_limit(configured, &spec, window);
        // The fixed payload (system prompt, tool schemas) is not rebuilt here.
        // Omitting it only under-states the budget, which makes the pass fire
        // slightly later — never wrong, just later.
        let root_budget = context::input_budget(window, max_output, 0);
        // What the wire would cost right now with nothing folded. A budget
        // nothing can exceed, so the fit reports the untrimmed cost rather than
        // silently eliding the history before it is measured.
        let used = context::fit_report(&history, u32::MAX, true, context::Folding::none()).estimate;
        if !crate::condense::should_condense(used, root_budget) {
            return;
        }

        // The pass extends the summary it already has, so the compression ratio
        // rises as the chat grows instead of the block growing linearly.
        let previous = self
            .db()
            .session_summary(session_id)
            .ok()
            .flatten()
            .map(|stored| stored.text);
        let excerpt = crate::condense::transcript_excerpt(covered, 24_000);
        let budget = crate::condense::condensed_budget(share, window, root_budget);
        let covered_count = covered.len() as i64;

        // The lite model does this, like titles and memory: cheap and frequent.
        let config = self.config();
        let (pass_provider_id, pass_provider, pass_model) = match config.chat.lite.clone() {
            Some(lite) => match config.providers.get(&lite.provider_id) {
                Some(lite_provider) => (
                    lite.provider_id.clone(),
                    lite_provider.clone(),
                    lite.model_id,
                ),
                None => (
                    model.provider_id.clone(),
                    provider.clone(),
                    model.model_id.clone(),
                ),
            },
            None => (
                model.provider_id.clone(),
                provider.clone(),
                model.model_id.clone(),
            ),
        };
        drop(config);
        let pass_key = secrets::get_api_key(&pass_provider_id).unwrap_or_else(|_| {
            if pass_provider_id == model.provider_id {
                api_key.map(str::to_string)
            } else {
                None
            }
        });

        let engine = self.clone();
        let session_id = session_id.to_string();
        let _ = session.workdir;
        tokio::spawn(async move {
            engine
                .run_condense(
                    session_id,
                    covers_through_id,
                    covers_through_at,
                    covered_count,
                    previous,
                    excerpt,
                    budget,
                    pass_provider,
                    pass_model,
                    pass_key,
                )
                .await;
        });
    }

    /// The summary pass: ask the lite model for a condensed view of the turns
    /// that have aged out, store it, and let the next turn's fit fold on it.
    /// Never fails the turn — on any error the digest stands and the fold point
    /// is simply not advanced, so the pass is attempted again later.
    #[allow(clippy::too_many_arguments)]
    async fn run_condense(
        &self,
        session_id: String,
        covers_through_id: String,
        covers_through_at: i64,
        covered_count: i64,
        previous: Option<String>,
        excerpt: String,
        budget: u32,
        provider: ProviderConfig,
        model_id: String,
        api_key: Option<String>,
    ) {
        let prompt = crate::condense::summary_prompt(previous.as_deref(), &excerpt);
        let request = ChatRequest {
            provider: &provider,
            model: &model_id,
            system: Some(
                "You maintain a running summary of a coding conversation. Reply with the \
                 summary itself and nothing else.",
            ),
            messages: vec![WireMessage::text("user", prompt)],
            variant: None,
            // Room for a thorough summary; the character cap in the prompt is
            // what actually bounds it.
            max_output_tokens: Some(4_096),
            temperature: None,
            top_p: None,
            stream: false,
            tools: Vec::new(),
            session_id: Some(&session_id),
        };

        let (content, reasoning) =
            match stream::run_once(&self.inner.client, &request, api_key.as_deref()).await {
                Ok((content, reasoning, _)) => (content, reasoning),
                Err(_) => return,
            };
        let raw = if content.trim().is_empty() {
            reasoning.unwrap_or_default()
        } else {
            content
        };
        let Some((text, tokens)) = crate::condense::parse_summary(&raw, budget) else {
            return;
        };

        let summary = crate::db::SessionSummary {
            session_id,
            covers_through_id,
            covers_through_at,
            covered_count,
            text,
            tokens,
            model: Some(model_id),
            updated_at: crate::db::now_ms(),
        };
        if let Err(error) = self.db().set_session_summary(&summary) {
            eprintln!("[loom] summary write skipped: {error}");
        }
    }

    fn advance_memory_watermark(&self, session_id: &str, watermark: i64) {
        let mut scan = self
            .inner
            .memory_scan
            .lock()
            .expect("memory scan mutex poisoned");
        let entry = scan.entry(session_id.to_string()).or_insert(0);
        if watermark > *entry {
            *entry = watermark;
        }
    }

    // ------------------------------------------------------------------
    // Jobs (cron)
    // ------------------------------------------------------------------

    pub fn jobs(&self) -> Result<Vec<crate::db::Job>> {
        self.db().jobs()
    }

    /// Memory rows for the Memory page. `None` returns every scope.
    pub fn memories(&self, scope: Option<&str>) -> Result<Vec<crate::db::Memory>> {
        self.db().memories(scope)
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        self.db().delete_memory(id)
    }

    pub fn clear_memories(&self, scope: &str) -> Result<usize> {
        self.db().clear_memories(scope)
    }

    /// A user edit from the Memory page: saves the text and refreshes the
    /// embedding so retrieval follows the new wording.
    pub async fn upsert_memory(
        &self,
        id: Option<&str>,
        scope: &str,
        content: &str,
        pinned: bool,
    ) -> Result<crate::db::Memory> {
        let content = content.trim();
        if content.is_empty() {
            return Err(Error::Other("a memory needs some text".into()));
        }

        let config = self.config();
        let provider_id = config.chat.provider_id.clone();
        let provider = provider_id
            .as_ref()
            .and_then(|id| config.providers.get(id))
            .cloned();
        drop(config);
        let embedding = match (provider_id.as_deref(), provider) {
            (Some(provider_id), Some(provider)) => {
                let api_key = secrets::get_api_key(provider_id).ok().flatten();
                self.embed_facts(&provider, api_key.as_deref(), &[content.to_string()])
                    .await
                    .ok()
                    .and_then(|mut vectors| vectors.pop())
                    .map(|vector| crate::embeddings::encode(&vector))
            }
            _ => None,
        };

        if let Some(id) = id {
            let existing = self
                .db()
                .memories(None)?
                .into_iter()
                .find(|memory| memory.id == id)
                .ok_or_else(|| Error::Other("no such memory".into()))?;
            self.db()
                .update_memory(id, content, pinned, embedding.as_deref())?;
            return Ok(self
                .db()
                .memories(Some(&existing.scope))?
                .into_iter()
                .find(|memory| memory.id == id)
                .unwrap_or(existing));
        }

        let memory =
            crate::memory::new_memory(scope, content, pinned, "user", None, None, embedding);
        self.db().insert_memory(&memory)?;
        Ok(memory)
    }

    /// After a crash or a forced quit, anything still queued or running is
    /// frankly not. Called once at launch.
    pub fn mark_interrupted_tasks(&self) -> usize {
        let unfinished = match self.db().unfinished_tasks() {
            Ok(tasks) => tasks,
            Err(_) => return 0,
        };
        let count = unfinished.len();
        for task in unfinished {
            self.update_task(
                &task.id,
                "interrupted",
                Some("Loom quit while this was running"),
                None,
            );
        }
        count
    }

    /// How `list_jobs` renders the schedule for the model.
    fn list_jobs_for_tool(&self) -> Result<String> {
        let jobs = self.jobs()?;
        if jobs.is_empty() {
            return Ok("No scheduled jobs yet.".to_string());
        }
        let mut out = format!("{} scheduled job(s):\n", jobs.len());
        for job in jobs {
            let next = job
                .next_run_at
                .map(crate::jobs::format_local)
                .unwrap_or_else(|| "not scheduled".to_string());
            out.push_str(&format!(
                "- {} [{}] cron \"{}\" ({}), next {}{}\n",
                job.name,
                job.id,
                job.cron,
                if job.enabled { "enabled" } else { "paused" },
                next,
                job.last_status
                    .as_deref()
                    .map(|status| format!(", last run {status}"))
                    .unwrap_or_default()
            ));
        }
        Ok(out)
    }

    fn delete_job_from_tool(&self, arguments: &serde_json::Value) -> Result<String> {
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let job = self
            .db()
            .job(&id)?
            .ok_or_else(|| Error::Other(format!("no job with id \"{id}\"")))?;
        self.delete_job(&id)?;
        Ok(format!("Deleted the job \"{}\".", job.name))
    }

    /// `schedule_job`: create or update a job from the model, always behind a
    /// confirmation card, because it changes what Loom does while nobody is
    /// watching (see `tools::always_asks`).
    fn schedule_job_from_tool(
        &self,
        arguments: &serde_json::Value,
        session_id: &str,
        model: &ModelRef,
        tool_context: &ToolContext,
    ) -> Result<String> {
        let name = arguments
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let cron = arguments
            .get("cron")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let prompt = arguments
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let id = arguments
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let workspace = arguments
            .get("workspace")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                tool_context
                    .workdir
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            });
        let permission_mode = arguments
            .get("permission_mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("auto-read-only")
            .to_string();
        let notify_on_success = arguments
            .get("notify_on_success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let expression = crate::jobs::Cron::parse(&cron).map_err(Error::other)?;
        let now = now_ms();
        let existing = match &id {
            Some(id) => self.db().job(id)?,
            None => None,
        };
        let job = crate::db::Job {
            id: existing
                .as_ref()
                .map(|job| job.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name,
            cron,
            enabled: existing.as_ref().map(|job| job.enabled).unwrap_or(true),
            prompt,
            provider_id: Some(model.provider_id.clone()),
            model_id: Some(model.model_id.clone()),
            persona_id: existing
                .as_ref()
                .and_then(|job| job.persona_id.clone())
                .or_else(|| self.session_persona_id(session_id)),
            workdir: workspace,
            permission_mode: Some(permission_mode),
            notify_on_success,
            catch_up_minutes: existing
                .as_ref()
                .map(|job| job.catch_up_minutes)
                .unwrap_or(720),
            last_run_at: existing.as_ref().and_then(|job| job.last_run_at),
            last_status: existing.as_ref().and_then(|job| job.last_status.clone()),
            next_run_at: expression.next_after(now),
            created_at: existing.as_ref().map(|job| job.created_at).unwrap_or(now),
            updated_at: now,
        };
        let stored = self.upsert_job(job)?;
        let mut out = format!(
            "Scheduled \"{}\" — cron `{}`.\nNext runs:\n",
            stored.name, stored.cron
        );
        for run in crate::jobs::upcoming(&stored.cron, 3).unwrap_or_default() {
            out.push_str(&format!("- {}\n", crate::jobs::format_local(run)));
        }
        Ok(out)
    }

    /// Creates or updates a job, computing its next firing from now.
    pub fn upsert_job(&self, mut job: crate::db::Job) -> Result<crate::db::Job> {
        if job.name.trim().is_empty() {
            return Err(Error::Other("a job needs a name".into()));
        }
        if job.prompt.trim().is_empty() {
            return Err(Error::Other("a job needs a prompt".into()));
        }
        let cron = crate::jobs::Cron::parse(&job.cron).map_err(Error::other)?;
        job.cron = job.cron.trim().to_string();
        job.next_run_at = cron.next_after(now_ms());
        job.updated_at = now_ms();
        self.db().upsert_job(&job)?;
        self.emit(EngineEvent::JobChanged { job: job.clone() });
        Ok(job)
    }

    pub fn delete_job(&self, id: &str) -> Result<()> {
        let job = self.db().job(id)?;
        self.db().delete_job(id)?;
        if let Some(job) = job {
            self.emit(EngineEvent::JobChanged { job });
        }
        Ok(())
    }

    /// Runs a job now, without touching its schedule.
    pub fn run_job_now(&self, id: &str) -> Result<String> {
        let job = self
            .db()
            .job(id)?
            .ok_or_else(|| Error::Other("no such job".into()))?;
        self.fire_job(&job)
    }

    fn fire_job(&self, job: &crate::db::Job) -> Result<String> {
        let task_id = self.spawn_task(TaskRequest {
            prompt: job.prompt.clone(),
            title: job.name.clone(),
            origin_session: None,
            job_id: Some(job.id.clone()),
            provider_id: job.provider_id.clone(),
            model_id: job.model_id.clone(),
            persona_id: job.persona_id.clone(),
            workdir: job.workdir.clone(),
            permission_mode: job.permission_mode.clone(),
            notify: job.notify_on_success,
            max_steps: None,
            max_cost_usd: None,
        })?;
        let now = now_ms();
        let next = crate::jobs::Cron::parse(&job.cron)
            .ok()
            .and_then(|cron| cron.next_after(now));
        self.db()
            .set_job_schedule(&job.id, next, Some(now), Some("fired"))?;
        // Read into a local, then emit, for the same reason `update_task` does:
        // a guard bound by an `if let` scrutinee lives to the end of the block,
        // so it would be held across `emit` and the webview crossing inside it.
        let fresh = self.db().job(&job.id).ok().flatten();
        if let Some(fresh) = fresh {
            self.emit(EngineEvent::JobChanged { job: fresh });
        }
        Ok(task_id)
    }

    /// One scheduler tick: fires due jobs (once, inside the catch-up window),
    /// skips jobs whose previous run is still going, and records misses.
    /// Called on launch and every ~30 s while the app runs.
    pub async fn tick_jobs(&self) {
        let jobs = match self.db().jobs() {
            Ok(jobs) => jobs,
            Err(error) => {
                eprintln!("[loom] scheduler could not read jobs: {error}");
                return;
            }
        };
        let now = now_ms();

        for job in jobs {
            if !job.enabled {
                continue;
            }
            let cron = match crate::jobs::Cron::parse(&job.cron) {
                Ok(cron) => cron,
                Err(error) => {
                    eprintln!("[loom] job \"{}\" has a bad schedule: {error}", job.name);
                    continue;
                }
            };

            let Some(next) = job.next_run_at else {
                // First sighting: start the clock, don't fire retroactively.
                let next = cron.next_after(now);
                let _ = self.db().set_job_schedule(&job.id, next, None, None);
                continue;
            };
            if next > now {
                continue;
            }

            let busy = self
                .db()
                .list_tasks(Some(&job.id))
                .map(|tasks| {
                    tasks
                        .iter()
                        .any(|task| task.status == "queued" || task.status == "running")
                })
                .unwrap_or(false);

            let missed_by = now.saturating_sub(next);
            let window = job.catch_up_minutes.max(0).saturating_mul(60_000);

            if busy {
                // The previous firing is still going; keep one slot per job.
                let next = cron.next_after(now);
                let _ = self
                    .db()
                    .set_job_schedule(&job.id, next, None, Some("skipped"));
            } else if missed_by <= window {
                // Due now, or missed while the app was closed but still
                // inside the catch-up window: fire once.
                if let Err(error) = self.fire_job(&job) {
                    eprintln!("[loom] job \"{}\" could not start: {error}", job.name);
                }
            } else {
                let next = cron.next_after(now);
                let _ = self
                    .db()
                    .set_job_schedule(&job.id, next, None, Some("missed"));
            }
        }
    }
}

/// Coalesces streaming deltas so the UI is not flooded with one event per
/// token. Flushes when the buffer is big enough, and always at the end of a
/// round.
#[derive(Default)]
struct DeltaCoalescer {
    pending: String,
    threshold: usize,
}

impl DeltaCoalescer {
    fn new(threshold: usize) -> Self {
        Self {
            pending: String::new(),
            threshold,
        }
    }

    /// Returns text to emit when the buffer crosses the threshold.
    fn push(&mut self, text: &str) -> Option<String> {
        self.pending.push_str(text);
        if self.pending.len() >= self.threshold {
            return Some(std::mem::take(&mut self.pending));
        }
        None
    }

    /// Returns any remaining text.
    fn flush(&mut self) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.pending))
    }
}

/// Assembles streaming tool-call fragments into complete calls.
#[derive(Default)]
struct PartialCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

fn merge_tool_delta(
    calls: &mut Vec<PartialCall>,
    index: usize,
    id: Option<String>,
    name: Option<String>,
    argument_fragment: Option<String>,
) {
    while calls.len() <= index {
        calls.push(PartialCall::default());
    }
    let call = &mut calls[index];
    if let Some(id) = id {
        call.id = Some(id);
    }
    if let Some(name) = name {
        call.name = Some(name);
    }
    if let Some(fragment) = argument_fragment {
        call.arguments.push_str(&fragment);
    }
}

/// The thinking an assistant message hands back to the provider.
///
/// DeepSeek-family gateways require the reasoner's own thinking on the message
/// whose tool calls are being continued — but only the most recent spell, and
/// only for the turn still in progress. Echoing the whole aggregate made the
/// model re-read and re-derive its earlier thinking every round, so a hard
/// task ballooned into tens of near-identical spells (one message reached 31
/// blocks and 390k characters of reasoning). Completed turns' thinking is not
/// required back, and handing it over just invites the model to repeat itself.
///
/// The context budget calls this too: it must charge for exactly the thinking
/// that goes on the wire, no more. Counting the whole `reasoning` column
/// instead over-estimated long reasoning turns badly enough to drop history
/// that would have fitted.
pub(crate) fn reasoning_echo(
    message: &Message,
    index: usize,
    history: &[Message],
) -> Option<String> {
    if history[index + 1..]
        .iter()
        .any(|later| later.role == Role::User)
    {
        return None;
    }
    let blocks = parse_reasoning_blocks(message.extra.as_deref());
    if let Some(last) = blocks.last().filter(|block| !block.text.trim().is_empty()) {
        return Some(last.text.clone());
    }
    message
        .reasoning
        .clone()
        .filter(|text| !text.trim().is_empty())
}

/// Builds the provider wire from stored history, including tool calls and
/// their results.
fn build_wire(history: &[Message]) -> Vec<WireMessage> {
    let mut wire = Vec::new();

    // Screenshots are the most recent pair of eyes only: the model needs the
    // latest one, and older ones only exist as a text breadcrumb. Everything
    // from the current user turn may carry images; every older call gets a
    // placeholder. Only the newest image-bearing call is inlined.
    let last_user = history
        .iter()
        .rposition(|message| message.role == Role::User);
    let last_image_call = history
        .iter()
        .enumerate()
        .rev()
        .filter(|(index, _)| Some(*index) > last_user)
        .flat_map(|(_, message)| parse_stored_tools(message.extra.as_deref()))
        .find(|call| !call.images.is_empty())
        .map(|call| call.id);

    for (index, message) in history.iter().enumerate() {
        let stored_tools = parse_stored_tools(message.extra.as_deref());

        if message.role == Role::Assistant
            && message.content.trim().is_empty()
            && stored_tools.is_empty()
        {
            continue;
        }

        let is_last_user = message.role == Role::User
            && history[index + 1..]
                .iter()
                .all(|later| later.role != Role::User);

        match message.role {
            Role::User => wire.push(WireMessage {
                role: "user".to_string(),
                parts: message_parts(message, is_last_user),
                tool_calls: Vec::new(),
                tool_call_id: None,
                reasoning: None,
            }),
            Role::Assistant => {
                wire.push(WireMessage {
                    role: "assistant".to_string(),
                    parts: message_parts(message, false),
                    tool_calls: stored_tools
                        .iter()
                        .map(|call| WireToolCall {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                        })
                        .collect(),
                    tool_call_id: None,
                    reasoning: reasoning_echo(message, index, history),
                });

                for call in &stored_tools {
                    let mut body = if call.status == "ok" {
                        call.output.clone()
                    } else {
                        format!("ERROR: {}", call.output)
                    };
                    // Computer chatter from earlier turns is context, not
                    // instruction: one line each keeps prefill small on long
                    // sessions. The current turn stays exact.
                    let current_turn = last_user.is_none_or(|user| index > user);
                    if !current_turn && crate::computer::is_computer_tool(&call.name) {
                        body = first_line(&body);
                    }
                    // Browser chatter from earlier turns is the same shape of
                    // thing, and the same rule: one line each. A snapshot is
                    // already compact, but a console dump or a network listing
                    // is not, and a twenty-step page task buries the live turn
                    // in the transcripts of its own earlier steps.
                    if !current_turn && crate::browser::is_browser_tool(&call.name) {
                        body = first_line(&body);
                    }
                    let inline = last_image_call.as_deref() == Some(call.id.as_str());
                    if !inline && !call.images.is_empty() {
                        body.push('\n');
                        body.push_str(
                            &call
                                .images
                                .iter()
                                .map(|image| format!("[screenshot: {}]", image.name))
                                .collect::<Vec<_>>()
                                .join(" "),
                        );
                    }
                    if inline {
                        wire.push(WireMessage::tool_result_with_images(
                            call.id.clone(),
                            body,
                            inline_images(call),
                        ));
                    } else {
                        wire.push(WireMessage::tool_result(call.id.clone(), body));
                    }
                }
            }
        }
    }

    wire
}

/// The first line of a tool output, for compacting older turns.
/// `2026-09-15 14:03` in UTC, for the chat list a tool reads.
///
/// Deliberately plain and zone-free: a model reasoning about "which chat was
/// this" needs an ordering it can compare, not a locale's idea of yesterday.
fn stamp_utc(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let (year, month, day) = crate::fsutil::civil_from_days(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        time / 3_600,
        (time % 3_600) / 60
    )
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let line = if line.chars().count() > 200 {
        let mut cut: String = line.chars().take(200).collect();
        cut.push('…');
        cut
    } else {
        line.to_string()
    };
    if text.lines().count() > 1 {
        format!("{line} …")
    } else {
        line
    }
}

/// Reads a stored call's images from disk as wire parts, skipping any file
/// that has since been pruned.
fn inline_images(call: &StoredToolCall) -> Vec<ContentPart> {
    use base64::Engine;

    let mut parts = Vec::new();
    for image in &call.images {
        match std::fs::read(&image.path) {
            Ok(bytes) => parts.push(ContentPart::Image {
                mime: image.mime.clone(),
                base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                name: image.name.clone(),
            }),
            Err(_) => parts.push(ContentPart::Text {
                text: format!("[screenshot {} unavailable]", image.name),
            }),
        }
    }
    parts
}

/// Sent alongside an automatic screenshot so the model knows the image is the
/// user's actual screen, not something they chose to attach.
const SCREENSHOT_NOTE: &str = "The image below is an actual screenshot of the user's screen, \
    captured automatically when they opened the quick-ask overlay. Treat it as exactly what \
    the user is looking at.";

/// Builds the wire parts for one stored message, inlining text/PDF content and
/// (for the newest user turn only) base64 images.
fn message_parts(message: &Message, include_images: bool) -> Vec<ContentPart> {
    use crate::attachments::{self, AttachmentKind};

    let mut parts = Vec::new();

    for attachment in attachments::parse_extra(message.extra.as_deref()) {
        if !attachments::is_managed(&attachment) {
            continue;
        }
        match attachment.kind {
            AttachmentKind::Image if include_images => match attachments::read_bytes(&attachment) {
                Ok(bytes) => {
                    if attachment.hidden {
                        parts.push(ContentPart::Text {
                            text: SCREENSHOT_NOTE.to_string(),
                        });
                    }
                    parts.push(ContentPart::Image {
                        mime: attachment.mime.clone(),
                        base64: base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            bytes,
                        ),
                        name: attachment.name.clone(),
                    });
                }
                Err(error) => parts.push(ContentPart::Text {
                    text: format!("[image {} unavailable: {error}]", attachment.name),
                }),
            },
            AttachmentKind::Image => parts.push(ContentPart::Text {
                text: format!("[earlier image: {}]", attachment.name),
            }),
            _ => match attachments::extract_text(&attachment) {
                Ok(text) => parts.push(ContentPart::Text {
                    text: format!("File: {}\n\n{text}", attachment.name),
                }),
                Err(error) => parts.push(ContentPart::Text {
                    text: format!("File: {} (could not read: {error})", attachment.name),
                }),
            },
        }
    }

    if !message.content.trim().is_empty() {
        parts.push(ContentPart::Text {
            text: message.content.clone(),
        });
    }

    if parts.is_empty() {
        parts.push(ContentPart::Text {
            text: String::new(),
        });
    }

    parts
}

/// Waits for the user's answer, the question timeout, or the turn being
/// stopped; the last two read as a dismissal.
async fn wait_for_answer(
    mut receiver: tokio::sync::oneshot::Receiver<tools::Answer>,
    cancel: &Cancellation,
) -> tools::Answer {
    let dismissed = || tools::Answer {
        cancelled: true,
        ..Default::default()
    };

    tokio::select! {
        received = &mut receiver => received.unwrap_or_else(|_| dismissed()),
        _ = tokio::time::sleep(QUESTION_TIMEOUT) => dismissed(),
        _ = wait_for_cancel(cancel) => dismissed(),
    }
}

/// Resolves once the turn's cancellation flag is set.
async fn wait_for_cancel(cancel: &Cancellation) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// The report a finished foreground command returns to the model.
fn format_command_report(code: Option<i32>, stdout: &str, stderr: &str) -> String {
    let mut report = String::new();
    report.push_str(&format!("exit code: {}\n", code.unwrap_or(-1)));
    if !stdout.trim().is_empty() {
        report.push_str("stdout:\n");
        report.push_str(&tools::truncate_output(stdout));
        report.push('\n');
    }
    if !stderr.trim().is_empty() {
        report.push_str("stderr:\n");
        report.push_str(&tools::truncate_output(stderr));
    }
    report.trim_end().to_string()
}

/// A command's first non-blank line, shortened: the label a tracked command
/// shows when the model did not give one.
fn command_label(command: &str) -> String {
    let line = command
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("command");
    line.chars().take(80).collect()
}

fn permission_mode_str(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Ask => "ask",
        PermissionMode::AutoReadOnly => "auto-read-only",
        PermissionMode::AutoAll => "auto-all",
        PermissionMode::Atelier => "atelier",
    }
}

fn parse_permission_mode(value: &str) -> Option<PermissionMode> {
    match value {
        "ask" => Some(PermissionMode::Ask),
        "auto-read-only" => Some(PermissionMode::AutoReadOnly),
        "auto-all" => Some(PermissionMode::AutoAll),
        "atelier" => Some(PermissionMode::Atelier),
        _ => None,
    }
}

fn agent_mode_str(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Plan => "plan",
        AgentMode::Review => "review",
        AgentMode::Build => "build",
        AgentMode::Chat => "chat",
    }
}

fn parse_agent_mode(value: &str) -> Option<AgentMode> {
    match value {
        "plan" => Some(AgentMode::Plan),
        "review" => Some(AgentMode::Review),
        "build" => Some(AgentMode::Build),
        "chat" => Some(AgentMode::Chat),
        _ => None,
    }
}

/// The denied outcome for a harness call made outside Atelier, if any. Kept as
/// a free function so the gate can be tested without a provider round-trip.
fn harness_denial(
    permission_mode: PermissionMode,
    call: &ToolCall,
) -> Option<crate::tools::ToolOutcome> {
    if !harness::is_harness_tool(&call.name) || permission_mode == PermissionMode::Atelier {
        return None;
    }
    Some(crate::tools::ToolOutcome {
        id: call.id.clone(),
        name: call.name.clone(),
        ok: false,
        output: harness::refusal(&call.name),
        images: Vec::new(),
    })
}

/// Appends a note about Atelier to whatever system prompt is in play, so the
/// model knows it may edit the harness before its first tool call. Placed
/// before the agent-mode note, so Plan still gets the last word.
fn with_harness_mode(system: Option<String>, mode: PermissionMode) -> Option<String> {
    if mode != PermissionMode::Atelier {
        return system;
    }
    let note = "You are in Atelier mode: besides every tool Auto all runs, you can edit Loom's \
        own harness with the harness tools — personas, MCP servers, skills, prompts, \
        providers/models, and settings. Use list_harness to read the current state before you \
        change it, make one change at a time, and say plainly what you changed. Deletions ask \
        the user for confirmation; everything else applies immediately and affects every chat \
        and every future launch.";
    Some(match system {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{note}"),
        _ => note.to_string(),
    })
}

/// Appends the browser note to whatever system prompt is in play, so the model
/// knows it has a real browser and how to use it without flailing. Placed
/// before the agent-mode note, so Plan gets the last word.
///
/// The three rules that carry this are: snapshot before you click, assert
/// instead of screenshotting, and read the user-activity footer. The first two
/// are what keep a long turn cheap; the third is what makes shared control
/// safe, because it is how the model learns that the user is in the page.
fn with_browser_mode(
    system: Option<String>,
    browser: bool,
    look_only: bool,
    prefer_over_fetch: bool,
) -> Option<String> {
    if !browser {
        return system;
    }

    let fetch_note = if prefer_over_fetch {
        "Reading a page that needs no interaction is often cheaper with `fetch_url` — no tab, \
         no page load — so use that for a quick read and the browser when the page needs \
         JavaScript, a login, or a click."
    } else {
        "`fetch_url` reads a page without a tab and is usually the cheaper choice when a page \
         needs no interaction."
    };

    if look_only {
        let note = format!(
            "The built-in browser is enabled for this chat, but the agent mode is read-only: \
             you can look and report, not act. `browser_open`, `browser_snapshot`, \
             `browser_read`, `browser_find`, `browser_wait`, `browser_assert`, \
             `browser_screenshot`, `browser_console`, `browser_cookies` and `browser_storage` \
             are available; every tool that clicks, types, navigates an existing tab or clears \
             data is refused. Do not call them. Open the pages you need, read them, then \
             report what you found and what you would do about it. {fetch_note}"
        );
        return Some(append_note(system, &note));
    }

    let note = format!(
        "The built-in browser is enabled for this chat: a real Chromium you share with the \
         user, with the pages they are already signed into. Rules, in order of how much they \
         matter. **Prefer `browser_snapshot` to `browser_screenshot`**: a snapshot is an \
         indexed list of what is clickable, costs a fraction of the tokens, and is what the \
         `[n]` indexes in `browser_click` and `browser_type` refer to. Take a screenshot only \
         when the appearance is the point — a layout, a chart, a canvas — and never to find \
         out whether something worked; `browser_assert` does that for a fraction of the cost. \
         **Verify with `browser_assert`, not with another look.** After clicking, ask whether \
         the URL changed, whether the confirmation text is there, whether the console is \
         clean — that is one cheap call, and it is what keeps a twenty-step turn flat. **Use \
         `browser_wait` rather than re-snapshotting in a loop**: wait for a selector, a piece \
         of text, a URL, or network idle, and it returns the moment the condition holds. \
         **Read the user-activity footer** on any result that carries one. The user is in the \
         same browser, and their typing, clicks and scrolling are reported to you rather than \
         pausing anything — so if it says they have typed into a field, re-read before you \
         submit, and if it says they are somewhere else on the page, that is where they are, \
         not a fault to correct. Never submit a form whose fields the user is mid-way through; \
         the page refuses it and tells you why. Say briefly what you are doing between steps, \
         keep it to one short sentence, and do not narrate a plan you are about to execute. \
         {fetch_note}"
    );
    Some(append_note(system, &note))
}

/// Appends the computer-use note to whatever system prompt is in play, so the
/// model knows it can see and drive the machine — and how to do it without
/// flailing. Placed before the agent-mode note, so Plan gets the last word.
fn with_computer_mode(system: Option<String>, computer: bool, look_only: bool) -> Option<String> {
    if !computer {
        return system;
    }
    // A read-only agent mode refuses every mutating tool, computer ones
    // included. The prompt used to advertise mouse and keyboard anyway, so the
    // model spent a round finding out it could not move.
    if look_only {
        let note = "Computer use is enabled for this chat, but the agent mode is read-only: you \
            can look and report, not act. `screenshot` (with `region` and `scale` for small \
            text), `list_windows`, `list_processes`, and `wait` are available; every mouse, \
            keyboard, UI Automation, clipboard, window and process tool is refused. Do not \
            call them. Take the shots you need, then report what you found and what you would \
            do about it.";
        return Some(append_note(system, note));
    }
    let note = "Computer use is enabled for this chat. You can see the screen with `screenshot` \
        and act with `mouse`, `keyboard`, `ui`, `window`, `launch_app`, `clipboard`, \
        `list_windows`, `list_processes`, `kill_process` and `wait`. Rules: take a screenshot \
        first and use its image pixels for every mouse coordinate; verify with a fresh \
        screenshot after each action; prefer `ui` (accessibility) over pixel clicks and \
        keyboard shortcuts over menu hunting; `type` handles arbitrary Unicode; use `wait` \
        when something is loading instead of screenshotting in a loop; never repeat an action \
        that had no effect — change approach instead; say briefly what you are doing between \
        steps. Be fast: act as soon as you know the next step instead of deliberating between \
        trivial actions; batch independent calls in one reply (for example `keyboard type` \
        then `keyboard press enter`, or `window focus` then `mouse click`); do not take \
        another screenshot unless the next step depends on what changed; keep commentary to \
        one short sentence and never restate the plan; for small text, screenshot a `region` \
        with `scale` rather than guessing. The user can take over at any time: if you are \
        paused, wait — you will be told when they hand the machine back, and then look again \
        before you act.";
    Some(match system {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{note}"),
        _ => note.to_string(),
    })
}

/// Appends a note about the current mode to whatever system prompt is in
/// play, so the model knows what it may do before its first tool call. Plan
/// mode explicitly invites clarifying questions; Build signals that a plan the
/// user has already approved may now be executed.
fn with_agent_mode(system: Option<String>, mode: AgentMode) -> Option<String> {
    let note = match mode {
        AgentMode::Plan => {
            "You are in Plan mode. Do not modify the workspace or run commands: inspect and \
             search as much as you need, ask the user any clarifying questions that would \
             change the plan — more questions than usual are welcome here — then present a \
             concrete plan (steps, files, risks) and wait for the user to switch to Build. \
             write_file, edit_file and run_command are unavailable in this mode."
        }
        AgentMode::Review => {
            "You are in Review mode. Do not modify the workspace or run commands: read, search, \
             and examine as much as you need, then report findings. Lead with what is wrong, \
             each finding ranked by severity with file and line, why it matters, and a concrete \
             proposed fix. Skip praise and filler; if nothing is wrong, say so plainly. \
             write_file, edit_file and run_command are unavailable in this mode, so propose \
             fixes rather than applying them — the user can switch to Build to have them done."
        }
        AgentMode::Build => "You are in Build mode: you may change the workspace and run commands.",
        AgentMode::Chat => {
            "You are in Chat mode: answer quickly and directly, in prose, from what you \
             already know. You have only four tools — `web_search`, `fetch_url`, `datetime` \
             and `ask_user` — and every other tool is refused, so do not reach for one. \
             Search the web when the answer depends on something recent or specific, and \
             fetch a page when a snippet is not enough; do not search to confirm what you \
             already know. Do not call `ask_user` unless the request genuinely cannot be \
             answered without it — a short, direct answer beats a question. If the request \
             needs work on files or commands, say so plainly and suggest switching to \
             Build mode."
        }
    };
    Some(match system {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{note}"),
        _ => note.to_string(),
    })
}

/// Appends the standing instruction for chats with no workspace: a scratch
/// folder stands in so tools do not hard-fail, but it is not the user's project
/// and the model should not do real work there unless explicitly told to.
fn with_scratch_notice(system: Option<String>, scratch: &std::path::Path) -> Option<String> {
    let note = format!(
        "Workspace note: this chat has no workspace folder selected. A disposable scratch \
         folder at {} stands in so tools do not hard-fail, but it is not the user's project. \
         Do not use it for real work — only read, write, or run commands there when the user \
         explicitly tells you to. When a task needs files, ask the user to choose a workspace \
         folder first.",
        scratch.display()
    );
    Some(match system {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{note}"),
        _ => note,
    })
}

/// The user's goal and the model's task list, appended for every turn so a
/// long job resumes with its bearings. Kept out of the prompt entirely when
/// both are empty.
fn with_session_context(
    system: Option<String>,
    goal: Option<&str>,
    todos: &[Todo],
) -> Option<String> {
    if goal.is_none() && todos.is_empty() {
        return system;
    }

    let mut note = String::new();
    if let Some(goal) = goal.filter(|goal| !goal.trim().is_empty()) {
        note.push_str(&format!("The user's goal for this chat: {goal}\n"));
    }
    if !todos.is_empty() {
        note.push_str(
            "Current task list (maintained with todo_write; keep exactly one item in_progress, \
             and mark items completed as you finish them):\n",
        );
        for todo in todos {
            note.push_str(&render_todo_line(todo));
            note.push('\n');
        }
    }
    Some(append_note(system, note.trim_end()))
}

/// One task as a line: `[x]` done, `[~]` in progress, `[ ]` pending.
fn render_todo_line(todo: &Todo) -> String {
    let mark = match todo.status.as_str() {
        "completed" => "x",
        "in_progress" => "~",
        _ => " ",
    };
    format!("- [{mark}] {}", todo.content)
}

fn render_todos(todos: &[Todo]) -> String {
    if todos.is_empty() {
        return "The task list is empty.".to_string();
    }
    let done = todos
        .iter()
        .filter(|todo| todo.status == "completed")
        .count();
    let mut out = format!("Task list ({} of {} done):\n", done, todos.len());
    for todo in todos {
        out.push_str(&render_todo_line(todo));
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Reads a `todo_write` payload into rows: unknown statuses become `pending`,
/// blank items are dropped, and the list is capped so a runaway model cannot
/// fill the panel.
fn parse_todos(arguments: &serde_json::Value) -> Vec<Todo> {
    let mut todos: Vec<Todo> = Vec::new();
    if let Some(items) = arguments.get("todos").and_then(serde_json::Value::as_array) {
        for item in items.iter().take(50) {
            let content = item
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim();
            if content.is_empty() {
                continue;
            }
            let status = match item.get("status").and_then(serde_json::Value::as_str) {
                Some("in_progress") => "in_progress",
                Some("completed") => "completed",
                _ => "pending",
            };
            todos.push(Todo {
                id: uuid::Uuid::new_v4().to_string(),
                content: content.chars().take(300).collect(),
                status: status.to_string(),
                position: todos.len() as i64,
            });
        }
    }
    todos
}

/// A trimmed string argument from a tool-call payload.
fn trimmed_json(args: &serde_json::Value, key: &str) -> String {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Appends a note to a system prompt, keeping existing content first.
fn append_note(system: Option<String>, note: &str) -> String {
    match system {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{note}"),
        _ => note.to_string(),
    }
}

/// Whether a tool call falls outside a persona's allowlists.
fn tool_scope_blocked(
    name: &str,
    tool_allow: Option<&Vec<String>>,
    mcp_allow: Option<&Vec<String>>,
) -> bool {
    if name == tools::ASK_USER {
        return false;
    }
    match crate::mcp::parse_tool_name(name) {
        Some((server, _)) => {
            mcp_allow.is_some_and(|allow| !allow.contains(&server.to_string()))
                || tool_allow.is_some_and(|allow| !allow.contains(&name.to_string()))
        }
        None => tool_allow.is_some_and(|allow| !allow.contains(&name.to_string())),
    }
}

/// Values available to `{{variable}}` interpolation in persona prompts.
fn persona_vars(
    config: &AppConfig,
    persona: Option<&Persona>,
    model: &ModelRef,
    workdir: Option<&str>,
) -> PersonaVars {
    PersonaVars {
        user_name: config.user_profile.name.clone(),
        user_pronouns: config.user_profile.pronouns.clone(),
        user_about: config.user_profile.about.clone(),
        workdir: workdir.unwrap_or_default().to_string(),
        model: model.model_id.clone(),
        provider: model.provider_id.clone(),
        persona_name: persona.map(|p| p.name.clone()).unwrap_or_default(),
        date: today_utc(),
    }
}

/// The word the confirm pass is told to reply with when the title it was shown
/// already fits.
///
/// A sentinel rather than an inferred answer, because "did it change its mind"
/// is not something the text of a title can tell you: a model asked to improve
/// a good title will usually return that title, or a near-identical paraphrase
/// of it, and neither is distinguishable from a genuine correction.
const KEEP_TITLE: &str = "KEEP";

/// Whether a title is one the auto-title pass wrote.
///
/// Only its own guesses may be replaced. A stored title that does not match
/// what was recorded here was typed by the user, and a name they chose being
/// silently overwritten a moment later is worse than any automatic title.
fn title_is_ours(existing: &str, recorded: Option<&str>) -> bool {
    recorded.is_some_and(|title| title == existing)
}

/// The title to adopt from the confirm pass's reply, or `None` to keep the one
/// already there.
///
/// The sentinel is read from the raw last line rather than from the cleaned
/// title, because `clean_title` would hand back `KEEP` as a title quite
/// happily — it is a well-formed six-letter word, and once it has been through
/// there nothing distinguishes it from a real answer.
fn parse_confirm_reply(reply: &str) -> Option<String> {
    let last = reply.lines().last().unwrap_or_default();
    let bare = last
        .trim()
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '.' | '!' | '*' | '`'));
    if bare.eq_ignore_ascii_case(KEEP_TITLE) {
        return None;
    }
    let title = clean_title(last);
    if title.is_empty() || title.eq_ignore_ascii_case(KEEP_TITLE) {
        return None;
    }
    Some(title)
}

/// `YYYY-MM-DD` in UTC, without pulling in a date library.
fn today_utc() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let (year, month, day) = crate::fsutil::civil_from_days((seconds / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn clean_title(raw: &str) -> String {
    let mut title = raw.trim().trim_matches('"').trim().replace('\n', " ");
    while title.contains("  ") {
        title = title.replace("  ", " ");
    }
    if title.len() > 64 {
        title.truncate(64);
        title = title.trim_end().to_string();
    }
    title
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut cut = max;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}...", &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::WireToolCall;

    fn message(role: Role, content: &str, extra: Option<&str>) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: "s".into(),
            role,
            content: content.into(),
            reasoning: None,
            extra: extra.map(str::to_string),
            persona_id: None,
            created_at: 0,
        }
    }

    /// A task that dies without a terminal event must still clear the busy flag
    /// and report why, or the chat hangs on "stop" forever.
    #[test]
    fn a_dead_task_reports_a_terminal_error() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit: EmitFn = Arc::new(move |event| {
            let _ = sender.send(event);
        });
        let engine = Engine::new(db, Arc::new(Mutex::new(AppConfig::default())), emit);

        let session = engine
            .create_session(None, None, None, None, None, None)
            .expect("session");
        let message_id = uuid::Uuid::new_v4().to_string();
        engine
            .db()
            .add_message(&Message {
                id: message_id.clone(),
                session_id: session.id.clone(),
                role: Role::Assistant,
                content: String::new(),
                reasoning: None,
                extra: None,
                persona_id: None,
                created_at: now_ms(),
            })
            .unwrap();
        engine.db().update_message(&message_id, &"", None).unwrap();

        engine.report_task_failure(&session.id, &message_id, "the turn crashed");

        // A dead turn reports a stop, never a failure: the same event carries
        // the reason, and the UI reads it as a note.
        let event = receiver.try_recv().expect("a terminal event");
        assert!(matches!(event, EngineEvent::Notice { .. }), "{event:?}");

        let stored = engine.messages(&session.id).unwrap();
        let assistant = stored.iter().find(|m| m.id == message_id).unwrap();
        let notice = parse_notice(assistant.extra.as_deref()).expect("the reason is kept");
        // `text` is the one line the transcript shows; the reason the engine was
        // handed is kept for the Details toggle, which is where it belongs. A
        // crashed turn reads as a stop, not as the user's mistake.
        assert_eq!(notice.detail.as_deref(), Some("the turn crashed"));
        assert!(notice.text.contains("ended early"), "{}", notice.text);
        assert!(engine.busy_sessions().is_empty());
    }

    /// A synchronous caller (a Tauri command) must get an error, never a panic:
    /// a panic in an IPC handler aborts the process.
    #[test]
    fn send_outside_a_runtime_is_an_error_not_a_panic() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );

        let outcome = engine.send("missing-session", "hello", None, Vec::new());
        let error = outcome.expect_err("no runtime means no dispatch");
        assert!(error.to_string().contains("Tokio runtime"), "{error}");
    }

    /// Serialises tests that point `LOOM_HOME` at a temporary directory.
    /// Shared with every other module through `paths::env_lock`.

    #[test]
    fn choosing_a_model_is_persisted_on_the_session() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let db = Database::open(&dir.path().join("loom.db")).unwrap();
        let config: SharedConfig = Arc::new(Mutex::new(AppConfig::default()));
        let engine = Engine::new(db, config, Arc::new(|_| {}));

        let session = engine
            .create_session(
                None,
                Some("provider-a".into()),
                Some("model-a".into()),
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(session.model_id.as_deref(), Some("model-a"));

        // What the picker does when a model is chosen.
        engine
            .set_session_model(&session.id, "provider-b", "model-b", Some("high".into()))
            .unwrap();

        let stored = engine.session(&session.id).unwrap().unwrap();
        assert_eq!(stored.provider_id.as_deref(), Some("provider-b"));
        assert_eq!(stored.model_id.as_deref(), Some("model-b"));
        assert_eq!(stored.variant.as_deref(), Some("high"));

        // The chat-level default survives a restart of the process.
        let reloaded = crate::db::Database::open(&dir.path().join("loom.db")).unwrap();
        let stored = reloaded.get_session(&session.id).unwrap().unwrap();
        assert_eq!(stored.model_id.as_deref(), Some("model-b"));

        // And the effective model resolves for sending.
        assert_eq!(engine.effective_model(&stored).unwrap().model_id, "model-b");

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn favourites_toggle_and_persist_in_config() {
        use crate::config::{AppConfig, ModelRef};
        use crate::provider::{ModelSpec, ProviderConfig};
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let db = Database::open(&dir.path().join("loom.db")).unwrap();
        let mut config = AppConfig::default();
        let mut provider = ProviderConfig {
            name: "Test".into(),
            base_url: "https://example.com/v1".into(),
            ..Default::default()
        };
        provider
            .models
            .insert("model-a".into(), ModelSpec::default());
        config.providers.insert("test".into(), provider);
        let shared: SharedConfig = Arc::new(Mutex::new(config));
        let engine = Engine::new(db, shared, Arc::new(|_| {}));

        engine.set_model_favorite("test", "model-a", true).unwrap();
        let toggled = engine.config().providers["test"].models["model-a"].favorite;
        assert!(toggled);

        // The saved config file is what the next launch reads.
        let saved = crate::config::load().unwrap();
        assert!(saved.providers["test"].models["model-a"].favorite);

        let _ = ModelRef::new("test", "model-a");
        std::env::remove_var("LOOM_HOME");
    }
    /// A turn that cannot reach the provider must leave a visible reason on the
    /// message — the bug was an empty reply with the error only in a transient
    /// UI event.
    #[tokio::test]
    async fn a_failed_turn_records_the_reason_on_the_message() {
        use crate::config::{AppConfig, ChatDefaults};
        use crate::provider::ProviderConfig;
        use std::sync::Arc;
        use std::time::Duration;

        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let db = Database::open(&dir.path().join("loom.db")).unwrap();

        let mut config = AppConfig::default();
        config.providers.insert(
            "unreachable".into(),
            ProviderConfig {
                name: "Unreachable".into(),
                // Port 9 (discard) refuses connections immediately.
                base_url: "http://127.0.0.1:9/v1".into(),
                key_required: false,
                ..Default::default()
            },
        );
        config.chat = ChatDefaults {
            provider_id: Some("unreachable".into()),
            model_id: Some("model".into()),
            ..Default::default()
        };

        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit: EmitFn = Arc::new(move |event| {
            let _ = sender.send(event);
        });
        let engine = Engine::new(db, Arc::new(Mutex::new(config)), emit);

        let session = engine
            .create_session(None, None, None, None, None, None)
            .expect("session");

        engine
            .send(&session.id, "hello", None, Vec::new())
            .expect("send starts");

        // Drain deltas until the terminal event arrives.
        let event = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match receiver.recv().await {
                    Some(event @ EngineEvent::Notice { .. }) => break event,
                    Some(event @ EngineEvent::Done { .. }) => break event,
                    Some(_) => continue,
                    None => panic!("the engine stopped without a terminal event"),
                }
            }
        })
        .await
        .expect("a terminal event arrived");
        assert!(
            matches!(event, EngineEvent::Notice { .. }),
            "expected a stop event, got {event:?}"
        );

        let messages = engine.messages(&session.id).unwrap();
        let assistant = messages.last().expect("assistant message");
        let recorded = parse_notice(assistant.extra.as_deref())
            .expect("the stop reason is stored on the message");
        assert!(!recorded.text.trim().is_empty());

        // And it survives a reload from disk, which is what the UI does.
        let reopened = Database::open(&dir.path().join("loom.db")).unwrap();
        let stored = reopened.messages(&session.id).unwrap();
        assert!(parse_notice(stored.last().unwrap().extra.as_deref()).is_some());

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn events_serialize_with_the_ids_the_frontend_reads() {
        let started = EngineEvent::Started {
            session_id: "s1".into(),
            message_id: "m1".into(),
        };
        let json = serde_json::to_string(&started).unwrap();
        assert!(json.contains("\"type\":\"started\""), "{json}");
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"messageId\":\"m1\""), "{json}");

        let delta = EngineEvent::Delta {
            session_id: "s1".into(),
            message_id: "m1".into(),
            text: "hi".into(),
        };
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"messageId\":\"m1\""), "{json}");

        let done = EngineEvent::Done {
            session_id: "s1".into(),
            message_id: "m1".into(),
            content: "x".into(),
            reasoning: None,
            usage: Usage::default(),
            condensed: None,
        };
        let json = serde_json::to_string(&done).unwrap();
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"inputTokens\""), "{json}");
        // A reply that was not condensed carries no field at all, so the
        // frontend's optional parse stays the only reader.
        assert!(!json.contains("condensed"), "{json}");

        // A condensed one carries the count and the source, camelCase, which is
        // what the faint line under the reply reads.
        let folded = EngineEvent::Done {
            session_id: "s1".into(),
            message_id: "m1".into(),
            content: "x".into(),
            reasoning: None,
            usage: Usage::default(),
            condensed: Some(context::Condensed {
                covered: 24,
                source: context::Source::Summary,
                tokens: 3_000,
            }),
        };
        let json = serde_json::to_string(&folded).unwrap();
        assert!(json.contains("\"condensed\""), "{json}");
        assert!(json.contains("\"covered\":24"), "{json}");
        assert!(json.contains("\"source\":\"summary\""), "{json}");
        assert!(json.contains("\"tokens\":3000"), "{json}");

        let permission = EngineEvent::ToolPermissionRequest {
            session_id: "s1".into(),
            message_id: "m1".into(),
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
            read_only: true,
            reason: None,
        };
        let json = serde_json::to_string(&permission).unwrap();
        assert!(json.contains("\"callId\":\"c1\""), "{json}");
        assert!(json.contains("\"readOnly\":true"), "{json}");
        // Absent, not null: the card falls back to the plain prompt when the
        // mode is what gated the call.
        assert!(!json.contains("\"reason\""), "{json}");

        let risky = EngineEvent::ToolPermissionRequest {
            session_id: "s1".into(),
            message_id: "m1".into(),
            call_id: "c1".into(),
            name: "delete_path".into(),
            arguments: r#"{"path":"src"}"#.into(),
            read_only: false,
            reason: Some("3 of the 40 entries under src are uncommitted".into()),
        };
        let json = serde_json::to_string(&risky).unwrap();
        assert!(
            json.contains("\"reason\":\"3 of the 40 entries under src are uncommitted\""),
            "{json}"
        );

        let question = EngineEvent::QuestionRequest {
            session_id: "s1".into(),
            message_id: "m1".into(),
            call_id: "c1".into(),
            question: tools::AskQuestion::parse(r#"{"question":"Which database?"}"#).unwrap(),
        };
        let json = serde_json::to_string(&question).unwrap();
        assert!(json.contains("\"type\":\"questionRequest\""), "{json}");
        assert!(json.contains("\"callId\":\"c1\""), "{json}");
        assert!(json.contains("\"allowFreeText\":true"), "{json}");
    }

    #[tokio::test]
    async fn ask_user_resolves_with_the_answer() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );

        let call = ToolCall {
            id: "q1".into(),
            name: tools::ASK_USER.into(),
            arguments: serde_json::json!({
                "question": "Which database?",
                "options": ["Postgres", "SQLite"]
            })
            .to_string(),
        };
        let cancel = stream::cancellation();
        let responder = engine.clone();
        let waiting =
            tokio::spawn(async move { engine.ask_user("s1", "m1", &call, &cancel).await });

        // Poll until the card is registered, then answer: the emit and the
        // registration happen inside the spawned task.
        let mut answered = false;
        for _ in 0..100 {
            if responder.respond_question(
                "q1",
                tools::Answer {
                    selected: vec!["Postgres".into()],
                    ..Default::default()
                },
            ) {
                answered = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(answered, "the question was never registered");

        let outcome = waiting.await.unwrap();
        assert!(outcome.ok, "{}", outcome.output);
        assert!(outcome.output.contains("Postgres"), "{}", outcome.output);
    }

    #[tokio::test]
    async fn ask_user_gives_up_when_the_turn_is_stopped() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );

        let call = ToolCall {
            id: "q1".into(),
            name: tools::ASK_USER.into(),
            arguments: serde_json::json!({ "question": "Still there?" }).to_string(),
        };
        let cancel = stream::cancellation();
        cancel.store(true, Ordering::Relaxed);

        let outcome = engine.ask_user("s1", "m1", &call, &cancel).await;
        assert!(outcome.ok, "{}", outcome.output);
        assert!(outcome.output.contains("dismissed"), "{}", outcome.output);
    }

    #[test]
    fn coalescer_holds_small_chunks_and_flushes_on_threshold() {
        let mut coalescer = DeltaCoalescer::new(10);
        assert_eq!(coalescer.push("abc"), None);
        assert_eq!(coalescer.push("def"), None);
        assert_eq!(coalescer.push("ghij"), Some("abcdefghij".to_string()));
        assert_eq!(coalescer.flush(), None);
        assert_eq!(coalescer.push("xy"), None);
        assert_eq!(coalescer.flush(), Some("xy".to_string()));
    }

    #[test]
    fn coalescer_flush_is_empty_when_nothing_pending() {
        let mut coalescer = DeltaCoalescer::new(10);
        assert_eq!(coalescer.flush(), None);
    }

    #[test]
    fn usage_summary_estimates_cost_from_known_prices() {
        use crate::config::AppConfig;
        use crate::provider::{ModelSpec, ProviderConfig};
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let mut config = AppConfig::default();
        let mut provider = ProviderConfig {
            name: "Priced".into(),
            base_url: "https://example.com/v1".into(),
            ..Default::default()
        };
        provider.models.insert(
            "priced-model".into(),
            ModelSpec {
                input_price: Some(1.0),
                output_price: Some(2.0),
                ..Default::default()
            },
        );
        config.providers.insert("priced".into(), provider);

        let engine = Engine::new(db, Arc::new(Mutex::new(config)), Arc::new(|_| {}));
        let session = engine
            .create_session(None, None, None, None, None, None)
            .unwrap();

        // One reply with 1M input and 1M output tokens: 1 * 1.0 + 1 * 2.0.
        let extra = serialize_extra(
            &[],
            TurnOutcome {
                usage: Some(Usage {
                    input_tokens: Some(1_000_000),
                    output_tokens: Some(1_000_000),
                }),
                model: Some(&ModelRef::new("priced", "priced-model")),
                ..Default::default()
            },
        );
        engine
            .db()
            .add_message(&Message {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session.id.clone(),
                role: Role::Assistant,
                content: "hi".into(),
                reasoning: None,
                extra,
                persona_id: None,
                created_at: now_ms(),
            })
            .unwrap();

        let summary = engine.usage_summary().unwrap();
        assert_eq!(summary.replies, 1);
        assert_eq!(summary.priced_replies, 1);
        assert_eq!(summary.input_tokens, 1_000_000);
        assert!(
            (summary.estimated_cost_usd - 3.0).abs() < 0.0001,
            "{summary:?}"
        );
    }

    /// A command line that prints, waits, then prints — enough to prove the
    /// log is readable while the process is still going.
    fn slow_command() -> &'static str {
        if cfg!(windows) {
            "echo first && ping -n 4 127.0.0.1 >nul && echo last"
        } else {
            "echo first; sleep 3; echo last"
        }
    }

    /// The watcher must not hang when a tracked command ends on its own.
    ///
    /// Before the fix this test never returned: `finish_command` re-locked the
    /// database mutex it was already holding, on a multi-threaded runtime where
    /// the watcher genuinely runs. A current-thread runtime hid it, because
    /// `stop_command` had already written `stopped` before the watcher was ever
    /// polled.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_background_command_that_ends_on_its_own_is_marked_done() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", home.path());

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );
        let workdir = tempfile::tempdir().unwrap();

        let record = engine
            .start_command(Some("session-1"), workdir.path(), "echo hi", None, true)
            .unwrap();

        // A hang here is the deadlock, so bound the wait rather than the test.
        let status = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let row = engine.command(&record.id).unwrap().unwrap();
                if row.status != "running" {
                    break row.status;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("the watcher must finish; a timeout here is the database deadlock");

        assert_eq!(status, "done");

        // And the engine still answers: the lock is free, which is the real
        // symptom — a frozen app rather than a wrong status.
        assert!(engine.commands(None).is_ok());

        std::env::remove_var("LOOM_HOME");
    }

    /// Stop races the watcher. Whichever wins, the row settles and the engine
    /// keeps answering.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stopping_a_command_races_the_watcher_without_deadlocking() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", home.path());

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );
        let workdir = tempfile::tempdir().unwrap();

        let record = engine
            .start_command(
                Some("session-1"),
                workdir.path(),
                slow_command(),
                None,
                true,
            )
            .unwrap();

        // Let it write something first, so the watcher has real work to drain.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let _ = engine.stop_command(&record.id).unwrap();

        let status = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let row = engine.command(&record.id).unwrap().unwrap();
                if row.status != "running" {
                    break row.status;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("stopping must not deadlock the watcher");

        assert!(
            status == "stopped" || status == "done",
            "unexpected status {status}"
        );
        assert!(engine.commands(None).is_ok());

        std::env::remove_var("LOOM_HOME");
    }

    /// A finished detached run posts into the chat that asked for it. Before
    /// the fix, any background run with an origin chat froze Loom here.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_finished_run_posts_into_its_origin_chat() {
        use crate::config::AppConfig;
        use crate::db::Task;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );

        let origin = engine
            .create_session(None, None, None, None, None, None)
            .unwrap();
        let run_session = engine
            .create_session(None, None, None, None, None, None)
            .unwrap();

        let task = Task {
            id: "task-1".to_string(),
            session_id: run_session.id.clone(),
            origin_session: Some(origin.id.clone()),
            job_id: None,
            title: "a run".to_string(),
            prompt: "do something".to_string(),
            provider_id: None,
            model_id: None,
            status: "running".to_string(),
            detail: None,
            result: None,
            notify: false,
            created_at: now_ms(),
            started_at: Some(now_ms()),
            finished_at: None,
        };
        engine.db().create_task(&task).unwrap();

        // Bounded: the deadlock is a hang, not an error.
        tokio::time::timeout(Duration::from_secs(15), async {
            engine.complete_task("task-1", "done", Some("finished"), Some("the answer"));
        })
        .await
        .expect("completing a run with an origin chat must not deadlock");

        let posted = engine.messages(&origin.id).unwrap();
        assert_eq!(posted.len(), 1, "the origin chat gains the result");
        assert!(
            posted[0].content.contains("the answer"),
            "{}",
            posted[0].content
        );

        // The lock is free again.
        assert!(engine.commands(None).is_ok());
    }

    /// `report_task_failure` against a session that really has a running run
    /// row — the path the existing test never reached, because its session had
    /// no task.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_crashed_run_releases_its_slot() {
        use crate::config::AppConfig;
        use crate::db::Task;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );

        let session = engine
            .create_session(None, None, None, None, None, None)
            .unwrap();
        // `add_message` returns `()`, so the id is kept in its own binding.
        let message_id = uuid::Uuid::new_v4().to_string();
        engine
            .db()
            .add_message(&Message {
                id: message_id.clone(),
                session_id: session.id.clone(),
                role: Role::Assistant,
                content: String::new(),
                reasoning: None,
                extra: None,
                persona_id: None,
                created_at: now_ms(),
            })
            .unwrap();

        let task = Task {
            id: "task-2".to_string(),
            session_id: session.id.clone(),
            origin_session: None,
            job_id: None,
            title: "a run".to_string(),
            prompt: "do something".to_string(),
            provider_id: None,
            model_id: None,
            status: "running".to_string(),
            detail: None,
            result: None,
            notify: false,
            created_at: now_ms(),
            started_at: Some(now_ms()),
            finished_at: None,
        };
        engine.db().create_task(&task).unwrap();

        tokio::time::timeout(Duration::from_secs(15), async {
            engine.report_task_failure(&session.id, &message_id, "the turn crashed");
        })
        .await
        .expect("reporting a dead run must not deadlock");

        let row = engine.task("task-2").unwrap().unwrap();
        assert_eq!(row.status, "stopped", "the slot is released");
    }

    #[tokio::test]
    async fn a_background_command_is_tracked_read_and_stopped() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        // The log lives under LOOM_HOME, so this test takes the crate lock.
        let _guard = crate::paths::env_lock();
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", home.path());

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );
        let workdir = tempfile::tempdir().unwrap();

        let record = engine
            .start_command(
                Some("session-1"),
                workdir.path(),
                slow_command(),
                Some("  a slow one  "),
                true,
            )
            .unwrap();

        assert_eq!(record.status, "running");
        assert_eq!(record.label, "a slow one", "the label is trimmed");
        assert!(
            record.pid != 0,
            "a started command knows its pid so it can be stopped"
        );
        assert!(record.background);
        assert_eq!(record.session_id.as_deref(), Some("session-1"));
        assert!(record.log_path.contains("cmd-"));
        assert!(engine.command(&record.id).unwrap().is_some());

        // Output is readable while it is still running.
        tokio::time::sleep(Duration::from_millis(600)).await;
        let output = engine.command_output(&record.id, 50).unwrap();
        assert!(output.contains("first"), "{output}");

        let stopped = engine.stop_command(&record.id).unwrap();
        assert_eq!(stopped.status, "stopped");
        assert!(stopped.finished_at.is_some());
        // Stopping twice is a clear error rather than a second kill.
        assert!(engine.stop_command(&record.id).is_err());

        // The watcher must not overwrite "stopped" with an exit-code verdict.
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(
            engine.command(&record.id).unwrap().unwrap().status,
            "stopped",
            "the user's stop is the last word"
        );

        engine.delete_command(&record.id).unwrap();
        assert!(engine.command(&record.id).unwrap().is_none());

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn unknown_command_ids_are_errors_not_silent_successes() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", home.path());

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );
        assert!(engine.command_output("nope", 10).is_err());
        assert!(engine.stop_command("nope").is_err());
        // Deleting nothing is harmless, and idempotent.
        assert!(engine.delete_command("nope").is_ok());

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn a_restart_orphans_commands_but_keeps_their_logs() {
        use crate::config::AppConfig;
        use crate::db::CommandRun;
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", home.path());

        let db = Database::open_in_memory().unwrap();
        let log = home.path().join("logs/cmd-kept.log");
        let running = CommandRun {
            id: "c1".to_string(),
            session_id: None,
            label: "left running".to_string(),
            command: "npm run dev".to_string(),
            cwd: "C:/work".to_string(),
            // A pid that cannot exist, so nothing is actually probed or killed.
            pid: 0,
            status: "running".to_string(),
            exit_code: None,
            log_path: log.to_string_lossy().into_owned(),
            background: true,
            created_at: now_ms(),
            finished_at: None,
        };
        db.insert_command(&running).unwrap();
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "still going\n").unwrap();

        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );
        assert_eq!(engine.mark_interrupted_commands(), 1);

        let reconciled = engine.command("c1").unwrap().unwrap();
        assert_eq!(reconciled.status, "orphaned");
        assert!(reconciled.finished_at.is_some());
        // The process is not killed and the log is not thrown away: the user
        // asked for background commands to survive Loom.
        assert_eq!(
            std::fs::read_to_string(&reconciled.log_path).unwrap(),
            "still going\n"
        );
        assert_eq!(engine.command_output("c1", 10).unwrap(), "still going");

        // Reconciling twice is harmless.
        assert_eq!(engine.mark_interrupted_commands(), 0);

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn usage_summary_skips_replies_without_a_known_price() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let engine = Engine::new(
            db,
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        );
        let session = engine
            .create_session(None, None, None, None, None, None)
            .unwrap();
        let extra = serialize_extra(
            &[],
            TurnOutcome {
                usage: Some(Usage {
                    input_tokens: Some(500),
                    output_tokens: Some(100),
                }),
                ..Default::default()
            },
        );
        engine
            .db()
            .add_message(&Message {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: session.id.clone(),
                role: Role::Assistant,
                content: "hi".into(),
                reasoning: None,
                extra,
                persona_id: None,
                created_at: now_ms(),
            })
            .unwrap();

        let summary = engine.usage_summary().unwrap();
        assert_eq!(summary.replies, 1);
        assert_eq!(summary.priced_replies, 0);
        assert_eq!(summary.estimated_cost_usd, 0.0);
    }

    #[test]
    fn usage_summary_splits_totals_by_provider() {
        use crate::config::AppConfig;
        use crate::provider::{ModelSpec, ProviderConfig};
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let mut config = AppConfig::default();
        for (id, input_price, output_price) in [("cheap", 1.0, 1.0), ("dear", 10.0, 10.0)] {
            let mut provider = ProviderConfig {
                name: id.to_string(),
                base_url: "https://example.com/v1".into(),
                ..Default::default()
            };
            provider.models.insert(
                "m".into(),
                ModelSpec {
                    input_price: Some(input_price),
                    output_price: Some(output_price),
                    ..Default::default()
                },
            );
            config.providers.insert(id.into(), provider);
        }

        let engine = Engine::new(db, Arc::new(Mutex::new(config)), Arc::new(|_| {}));
        let session = engine
            .create_session(None, None, None, None, None, None)
            .unwrap();

        for (provider, input, output) in [("cheap", 1_000_000, 0), ("dear", 0, 1_000_000)] {
            let extra = serialize_extra(
                &[],
                TurnOutcome {
                    usage: Some(Usage {
                        input_tokens: Some(input),
                        output_tokens: Some(output),
                    }),
                    model: Some(&ModelRef::new(provider, "m")),
                    ..Default::default()
                },
            );
            engine
                .db()
                .add_message(&Message {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: session.id.clone(),
                    role: Role::Assistant,
                    content: "hi".into(),
                    reasoning: None,
                    extra,
                    persona_id: None,
                    created_at: now_ms(),
                })
                .unwrap();
        }

        let summary = engine.usage_summary().unwrap();
        assert_eq!(summary.providers.len(), 2);
        // Most estimated spend first: dear ($10) before cheap ($1).
        assert_eq!(summary.providers[0].provider_id, "dear");
        assert_eq!(summary.providers[0].estimated_cost_usd, 10.0);
        assert_eq!(summary.providers[0].input_tokens, 0);
        assert_eq!(summary.providers[0].output_tokens, 1_000_000);
        assert_eq!(summary.providers[1].provider_id, "cheap");
        assert_eq!(summary.providers[1].estimated_cost_usd, 1.0);
        assert_eq!(summary.input_tokens, 1_000_000);
        assert_eq!(summary.output_tokens, 1_000_000);
    }

    #[test]
    fn usage_capable_providers_lists_only_known_vendors() {
        use crate::config::AppConfig;
        use crate::provider::ProviderConfig;
        use std::sync::Arc;

        let db = Database::open_in_memory().unwrap();
        let mut config = AppConfig::default();
        config.providers.insert(
            "go".into(),
            ProviderConfig {
                name: "OpenCode Go".into(),
                base_url: "https://opencode.ai/zen/go/v1".into(),
                ..Default::default()
            },
        );
        config.providers.insert(
            "plain".into(),
            ProviderConfig {
                name: "OpenAI".into(),
                base_url: "https://api.openai.com/v1".into(),
                ..Default::default()
            },
        );
        config.providers.insert(
            "off".into(),
            ProviderConfig {
                name: "Disabled Go".into(),
                base_url: "https://opencode.ai/zen/go/v1".into(),
                enabled: false,
                ..Default::default()
            },
        );

        let engine = Engine::new(db, Arc::new(Mutex::new(config)), Arc::new(|_| {}));
        let capable = engine.usage_capable_providers();
        assert_eq!(capable.len(), 1);
        assert_eq!(capable[0].provider_id, "go");
        assert_eq!(capable[0].source, "OpenCode Go");
    }

    #[test]
    fn title_falls_back_to_the_thinking_tail() {
        // Reasoning models sometimes return empty content; the last line of the
        // thinking is still a usable title.
        let content = "";
        let reasoning = Some("The user asks about Norway.\nCapital of Norway");
        let candidate = if content.trim().is_empty() {
            reasoning.unwrap_or_default()
        } else {
            content
        };
        assert_eq!(
            clean_title(candidate.lines().last().unwrap_or_default()),
            "Capital of Norway"
        );
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(
            clean_title("  \"Rust ownership help.\"\n"),
            "Rust ownership help."
        );
        assert_eq!(clean_title("a  b"), "a b");
        assert!(clean_title(&"x".repeat(200)).len() <= 64);
    }

    // ----------------------------------------------------- the title passes

    #[test]
    fn the_confirm_pass_keeps_a_title_it_was_told_is_fine() {
        // The sentinel has to survive the shapes a model actually returns it
        // in. Each of these would otherwise become a chat called "KEEP".
        for reply in [
            "KEEP",
            "keep",
            " KEEP ",
            "\"KEEP\".",
            "*keep*",
            "`KEEP`",
            "The user asks about Norway.\nKEEP",
        ] {
            assert_eq!(parse_confirm_reply(reply), None, "{reply:?}");
        }
    }

    #[test]
    fn the_confirm_pass_takes_a_better_title() {
        assert_eq!(
            parse_confirm_reply("Rust ownership rules"),
            Some("Rust ownership rules".into())
        );
        assert_eq!(
            parse_confirm_reply("Thinking about it...\n\"Norway's capital.\""),
            Some("Norway's capital.".into())
        );
    }

    #[test]
    fn an_unusable_confirm_reply_keeps_the_title_rather_than_blanking_it() {
        // Nothing to go on. Keeping the existing name is the safe direction:
        // the worst case is a dull title, not a missing one.
        assert_eq!(parse_confirm_reply(""), None);
        assert_eq!(parse_confirm_reply("   \n  "), None);
    }

    #[test]
    fn only_the_auto_passes_own_guesses_may_be_replaced() {
        // What the pass wrote, still unchanged: fair game.
        assert!(title_is_ours("Rust help", Some("Rust help")));
        // Renamed by hand after the guess landed: must never be overwritten.
        assert!(!title_is_ours("My parser work", Some("Rust help")));
        // Nothing recorded — the app restarted, or the chat predates the map.
        // Treated as the user's, because a wrong guess overwritten is
        // recoverable and a chosen name destroyed is not.
        assert!(!title_is_ours("Rust help", None));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let cut = truncate("hello wörld", 3);
        assert!(cut.ends_with("..."));
        assert!(cut.len() <= "hello wörld".len() + 3);
    }

    #[test]
    fn tool_fragments_merge_by_index() {
        let mut calls: Vec<PartialCall> = Vec::new();
        merge_tool_delta(
            &mut calls,
            0,
            Some("a".into()),
            Some("read_file".into()),
            None,
        );
        merge_tool_delta(&mut calls, 0, None, None, Some("{\"path\":".into()));
        merge_tool_delta(&mut calls, 0, None, None, Some("\"x\"}".into()));
        merge_tool_delta(
            &mut calls,
            1,
            Some("b".into()),
            Some("datetime".into()),
            Some("{}".into()),
        );

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id.as_deref(), Some("a"));
        assert_eq!(calls[0].arguments, "{\"path\":\"x\"}");
        assert_eq!(calls[1].name.as_deref(), Some("datetime"));
    }

    #[test]
    fn wire_includes_tool_results_after_assistant_calls() {
        let stored = serialize_extra(
            &[StoredToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"a.txt\"}".into(),
                status: "ok".into(),
                output: "contents".into(),
                after: 0,
                seq: 0,
                images: Vec::new(),
                repeated: false,
            }],
            TurnOutcome::default(),
        )
        .unwrap();

        let history = vec![
            message(Role::User, "read a.txt", None),
            message(Role::Assistant, "Reading it.", Some(&stored)),
        ];

        let wire = build_wire(&history);
        assert_eq!(wire.len(), 3);
        assert_eq!(wire[1].role, "assistant");
        assert_eq!(wire[1].tool_calls.len(), 1);
        assert_eq!(
            wire[1].tool_calls[0],
            WireToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"a.txt\"}".into()
            }
        );
        assert_eq!(wire[2].role, "tool");
        assert_eq!(wire[2].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(wire[2].joined_text(), "contents");
    }

    #[test]
    fn empty_assistant_placeholders_are_skipped() {
        let history = vec![
            message(Role::User, "hi", None),
            message(Role::Assistant, "", None),
        ];
        let wire = build_wire(&history);
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].role, "user");
    }

    /// The quick-ask overlay's screenshots are only useful if the model knows
    /// they show the user's real screen; the note must precede the image.
    #[test]
    fn hidden_screenshots_arrive_with_their_context() {
        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let mut attachment =
            crate::attachments::store_bytes("s", "Screen 1.png", &[0x89, 0x50, 0x4E, 0x47])
                .unwrap();
        attachment.hidden = true;
        let extra = crate::attachments::serialize_extra(&[attachment]);

        let history = vec![message(Role::User, "what is this?", extra.as_deref())];
        let wire = build_wire(&history);

        assert!(
            matches!(&wire[0].parts[0], ContentPart::Text { text } if text.contains("actual screenshot")),
            "{:?}",
            wire[0].parts
        );
        assert!(matches!(&wire[0].parts[1], ContentPart::Image { .. }));

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn denied_tool_results_are_marked_as_errors() {
        let stored = serialize_extra(
            &[StoredToolCall {
                id: "c1".into(),
                name: "write_file".into(),
                arguments: "{}".into(),
                status: "denied".into(),
                output: "denied by the user".into(),
                after: 0,
                seq: 0,
                images: Vec::new(),
                repeated: false,
            }],
            TurnOutcome::default(),
        )
        .unwrap();
        let history = vec![message(Role::Assistant, "trying", Some(&stored))];
        let wire = build_wire(&history);
        assert!(wire[1].joined_text().starts_with("ERROR:"));
    }

    #[test]
    fn stored_tool_calls_keep_their_text_offset() {
        let stored = serialize_extra(
            &[StoredToolCall {
                id: "c1".into(),
                name: "web_search".into(),
                arguments: "{}".into(),
                status: "ok".into(),
                output: "results".into(),
                after: 12,
                seq: 3,
                images: Vec::new(),
                repeated: false,
            }],
            TurnOutcome::default(),
        )
        .unwrap();
        assert_eq!(parse_stored_tools(Some(&stored))[0].after, 12);

        // Records written before the field existed still parse, at offset 0.
        let legacy = r#"{"toolCalls":[{"id":"c2","name":"datetime","arguments":"{}","status":"ok","output":""}]}"#;
        assert_eq!(parse_stored_tools(Some(legacy))[0].after, 0);
    }

    #[test]
    fn permission_modes_encode_and_parse() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::AutoReadOnly,
            PermissionMode::AutoAll,
            PermissionMode::Atelier,
        ] {
            assert_eq!(parse_permission_mode(permission_mode_str(mode)), Some(mode));
        }
        assert_eq!(parse_permission_mode("nonsense"), None);
    }

    /// The mode gate: a harness call in Auto all is refused with a hint that
    /// names Atelier, and nothing in the config is touched on the way.
    #[test]
    fn a_harness_call_outside_atelier_is_refused() {
        let call = ToolCall {
            id: "c1".into(),
            name: "upsert_persona".into(),
            arguments: r#"{"name":"X","systemPrompt":"x"}"#.into(),
        };

        let denial = harness_denial(PermissionMode::AutoAll, &call).expect("refused in Auto all");
        assert!(!denial.ok);
        assert!(denial.output.contains("Atelier"), "{}", denial.output);
        assert_eq!(denial.id, "c1");

        // Atelier admits it; a built-in tool is never harness-gated.
        assert!(harness_denial(PermissionMode::Atelier, &call).is_none());
        assert!(harness_denial(
            PermissionMode::AutoAll,
            &ToolCall {
                id: "c2".into(),
                name: "read_file".into(),
                arguments: "{}".into(),
            }
        )
        .is_none());
        // `list_harness` is still a harness tool, so outside Atelier it is
        // refused too: the tools are never listed there.
        assert!(harness_denial(
            PermissionMode::AutoAll,
            &ToolCall {
                id: "c3".into(),
                name: "list_harness".into(),
                arguments: "{}".into(),
            }
        )
        .is_some());
    }

    /// The mutation funnel: a persona written through it lands in config.json,
    /// survives a fresh load, emits `HarnessChanged`, and leaves a backup.
    #[test]
    fn a_harness_call_in_atelier_lands_in_config_json() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let db = Database::open(&dir.path().join("loom.db")).unwrap();
        let config: SharedConfig = Arc::new(Mutex::new(AppConfig::default()));
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let emit: EmitFn = Arc::new(move |event| {
            let _ = sender.send(event);
        });
        let engine = Engine::new(db, Arc::clone(&config), emit);

        let args = serde_json::json!({ "name": "Reviewer", "systemPrompt": "Be terse." });
        let summary = engine
            .harness_mutate("s1", "upsert_persona", &args)
            .expect("mutation succeeds");
        assert!(summary.contains("Reviewer"), "{summary}");

        // The in-memory config, the file on disk, and a fresh load agree.
        assert_eq!(engine.config().personas.len(), 1);
        let saved = crate::config::load().unwrap();
        assert_eq!(saved.personas.len(), 1);
        assert_eq!(saved.personas[0].name, "Reviewer");

        let event = receiver.try_recv().expect("an event");
        match event {
            EngineEvent::HarnessChanged {
                section, summary, ..
            } => {
                assert_eq!(section, "personas");
                assert!(summary.contains("Reviewer"), "{summary}");
            }
            other => panic!("expected harness-changed, got {other:?}"),
        }

        // A snapshot was taken before the write.
        let backups: Vec<_> = std::fs::read_dir(crate::paths::backups_dir().unwrap())
            .unwrap()
            .flatten()
            .collect();
        assert!(!backups.is_empty(), "a backup should exist");

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn the_atelier_note_reaches_the_system_prompt_before_the_agent_note() {
        let system = with_harness_mode(Some("Be terse.".into()), PermissionMode::Atelier).unwrap();
        assert!(system.starts_with("Be terse."), "{system}");
        assert!(system.contains("Atelier"), "{system}");
        let system = with_agent_mode(Some(system), AgentMode::Plan).unwrap();
        assert!(
            system.find("Atelier").unwrap() < system.find("Plan mode").unwrap(),
            "Plan must have the last word: {system}"
        );

        // Other modes get nothing added.
        assert_eq!(
            with_harness_mode(Some("Be terse.".into()), PermissionMode::AutoAll).unwrap(),
            "Be terse."
        );
        assert!(with_harness_mode(None, PermissionMode::AutoAll).is_none());
    }

    #[test]
    fn harness_changed_serializes_with_camel_case_fields() {
        let event = EngineEvent::HarnessChanged {
            session_id: "s1".into(),
            section: "personas".into(),
            summary: "Created persona \"Reviewer\"".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"harnessChanged\""), "{json}");
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"section\":\"personas\""), "{json}");
        assert!(json.contains("\"summary\""), "{json}");
        assert_eq!(event.label(), "harness-changed");
    }

    #[test]
    fn agent_modes_encode_and_parse() {
        for mode in [
            AgentMode::Plan,
            AgentMode::Review,
            AgentMode::Build,
            AgentMode::Chat,
        ] {
            assert_eq!(parse_agent_mode(agent_mode_str(mode)), Some(mode));
        }
        assert_eq!(parse_agent_mode("nonsense"), None);
    }

    #[test]
    fn the_agent_mode_reaches_the_system_prompt() {
        let system = with_agent_mode(Some("Be terse.".into()), AgentMode::Plan).unwrap();
        assert!(system.starts_with("Be terse."), "{system}");
        assert!(system.contains("Plan mode"), "{system}");
        assert!(system.contains("ask the user"), "{system}");

        let bare = with_agent_mode(None, AgentMode::Build).unwrap();
        assert!(bare.contains("Build mode"), "{bare}");
        assert!(!bare.contains("Plan mode"), "{bare}");

        let chat = with_agent_mode(Some("Be terse.".into()), AgentMode::Chat).unwrap();
        assert!(chat.starts_with("Be terse."), "{chat}");
        assert!(chat.contains("Chat mode"), "{chat}");
        // It has to name what it *can* use, or the model discovers the edge of
        // the mode one refused call at a time.
        for tool in ["web_search", "fetch_url", "datetime", "ask_user"] {
            assert!(chat.contains(tool), "note should name {tool}: {chat}");
        }
        assert!(chat.contains("Build mode"), "{chat}");
    }

    #[test]
    fn the_scratch_notice_discourages_the_fallback_folder() {
        let scratch = std::path::Path::new(r"C:\Users\me\.loom\scratch");
        let system = with_scratch_notice(Some("Be terse.".into()), scratch).unwrap();
        assert!(system.starts_with("Be terse."), "{system}");
        assert!(system.contains(r"C:\Users\me\.loom\scratch"), "{system}");
        assert!(system.contains("no workspace folder"), "{system}");
        assert!(system.contains("explicitly"), "{system}");

        let bare = with_scratch_notice(None, scratch).unwrap();
        assert!(bare.contains("scratch folder"), "{bare}");
    }

    // ------------------------------------------------- reading other chats

    /// A bare engine over an in-memory database.
    fn test_engine() -> Engine {
        use crate::config::AppConfig;
        use std::sync::Arc;

        Engine::new(
            Database::open_in_memory().unwrap(),
            Arc::new(Mutex::new(AppConfig::default())),
            Arc::new(|_| {}),
        )
    }

    /// An engine with two chats, each with its own message, for the
    /// `list_chats` / `read_chat` tests. Returns the engine and both ids.
    fn engine_with_chats() -> (Engine, String, String) {
        let engine = test_engine();

        let first = engine
            .create_session(
                Some("Renaming the parser".into()),
                None,
                None,
                None,
                None,
                Some("C:/work/loom".into()),
            )
            .unwrap();
        let second = engine
            .create_session(Some("Updater notes".into()), None, None, None, None, None)
            .unwrap();

        // The helper builds a message with a placeholder session, and the
        // foreign key is on, so each message has to be pointed at a real chat
        // before it is written.
        //
        // `first` is the chat `read_chat` resolves, so it carries the
        // distinctive line a test can look for. `second` exists so the listing
        // has more than one row and the "not shown" line has something to
        // count.
        let mut question = message(Role::User, "the lexer is fine, it is the parser", None);
        question.session_id = first.id.clone();
        engine.db().add_message(&question).expect("a message");

        let mut answer = message(Role::Assistant, "found it in parse_expr", None);
        answer.session_id = first.id.clone();
        engine.db().add_message(&answer).expect("a reply");

        let mut other = message(Role::User, "the updater is fine", None);
        other.session_id = second.id.clone();
        engine.db().add_message(&other).expect("a message");

        (engine, first.id, second.id)
    }

    #[test]
    fn list_chats_names_every_conversation_with_its_id() {
        let (engine, first, second) = engine_with_chats();
        let listed = engine
            .list_chats_for_tool(&serde_json::json!({}))
            .expect("a list");

        assert!(listed.contains("Renaming the parser"), "{listed}");
        assert!(listed.contains("Updater notes"), "{listed}");
        // The id is the whole point of the listing: it is what `read_chat`
        // takes, and what a `#mention` carries.
        assert!(listed.contains(&first), "{listed}");
        assert!(listed.contains(&second), "{listed}");
        assert!(listed.contains("C:/work/loom"), "{listed}");
    }

    #[test]
    fn list_chats_says_when_there_is_nothing_to_list() {
        let listed = test_engine()
            .list_chats_for_tool(&serde_json::json!({}))
            .expect("a list");
        assert!(listed.contains("no chats"), "{listed}");
    }

    #[test]
    fn list_chats_honours_its_limit_and_says_what_it_left_out() {
        let (engine, _first, _second) = engine_with_chats();
        let listed = engine
            .list_chats_for_tool(&serde_json::json!({ "limit": 1 }))
            .expect("a list");
        assert!(listed.contains("1 more not shown"), "{listed}");
    }

    #[test]
    fn read_chat_finds_a_conversation_by_id_prefix_or_title() {
        let (engine, first, _second) = engine_with_chats();
        let prefix: String = first.chars().take(8).collect();

        for reference in [first.as_str(), prefix.as_str(), "Renaming the parser"] {
            let read = engine
                .read_chat_for_tool(&serde_json::json!({ "chat": reference }))
                .expect("a transcript");
            assert!(
                read.contains("found it in parse_expr"),
                "{reference} did not resolve: {read}"
            );
            assert!(read.starts_with("# Renaming the parser"), "{read}");
        }
    }

    #[test]
    fn read_chat_resolves_an_id_that_a_mention_wrote() {
        let (engine, first, _second) = engine_with_chats();
        // What `resolveChatMentions` puts in a message: eight characters with
        // the dashes taken out.
        let short: String = first.chars().filter(|ch| *ch != '-').take(8).collect();
        let read = engine
            .read_chat_for_tool(&serde_json::json!({ "chat": short }))
            .expect("a transcript");
        assert!(read.contains("parse_expr"), "{read}");
    }

    #[test]
    fn read_chat_says_so_when_nothing_matches() {
        let (engine, _first, _second) = engine_with_chats();
        let read = engine
            .read_chat_for_tool(&serde_json::json!({ "chat": "no such chat" }))
            .expect("a reply, not an error: the model should be able to recover");
        assert!(read.contains("No chat matches"), "{read}");
        // It has to point somewhere, or the model apologises instead of trying.
        assert!(read.contains("list_chats"), "{read}");
    }

    #[test]
    fn read_chat_needs_something_to_look_up() {
        let (engine, _first, _second) = engine_with_chats();
        assert!(engine.read_chat_for_tool(&serde_json::json!({})).is_err());
        assert!(engine
            .read_chat_for_tool(&serde_json::json!({ "chat": "   " }))
            .is_err());
    }

    #[test]
    fn read_chat_omits_the_middle_of_a_long_conversation() {
        let engine = test_engine();
        let session = engine
            .create_session(Some("Long".into()), None, None, None, None, None)
            .unwrap();
        for index in 0..40 {
            let mut entry = message(
                Role::User,
                &format!("turn {index} {}", "x".repeat(400)),
                None,
            );
            entry.session_id = session.id.clone();
            engine.db().add_message(&entry).expect("a message");
        }

        let read = engine
            .read_chat_for_tool(&serde_json::json!({
                "chat": session.id,
                "max_chars": 4_000,
            }))
            .expect("a transcript");

        assert!(read.chars().count() < 5_000, "{}", read.chars().count());
        // Both ends survive: the first turns say what the chat was, the last
        // say where it got to.
        assert!(read.starts_with("# Long"), "{read}");
        assert!(read.contains("turn 39"), "{read}");
        // And the omission is stated, never silent.
        assert!(read.contains("characters omitted"), "{read}");
    }

    #[test]
    fn workspace_files_lists_a_folder_and_skips_the_junk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/dep.js"), "").unwrap();

        let found = test_engine().workspace_files(dir.path().to_str(), 100);
        assert!(found.contains(&"main.rs".to_string()), "{found:?}");
        assert!(found.contains(&"src/lib.rs".to_string()), "{found:?}");
        assert!(
            !found.iter().any(|path| path.contains("node_modules")),
            "{found:?}"
        );
    }

    #[test]
    fn workspace_files_is_empty_rather_than_an_error_without_a_folder() {
        let engine = test_engine();
        // "No workspace yet" is an ordinary state, not a failure.
        assert!(engine.workspace_files(None, 100).is_empty());
        assert!(engine.workspace_files(Some("  "), 100).is_empty());
    }
}
