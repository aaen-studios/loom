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
use crate::process::Running;
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
/// turn resumes on its own.
const PAUSE_IDLE_RESUME: Duration = Duration::from_secs(30);

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
    },
    Error {
        session_id: String,
        message_id: String,
        error: String,
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
            EngineEvent::Error { .. } => "error",
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredExtra {
    #[serde(default)]
    tool_calls: Vec<StoredToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reasoning_blocks: Vec<StoredReasoningBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
    /// Why a turn failed, kept so the reason survives a reload.
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

/// The failure recorded on a message, if the turn did not complete.
pub fn parse_error(extra: Option<&str>) -> Option<String> {
    extra
        .and_then(|raw| serde_json::from_str::<StoredExtra>(raw).ok())
        .and_then(|stored| stored.error)
}

fn serialize_extra(
    tool_calls: &[StoredToolCall],
    usage: Option<Usage>,
    error: Option<&str>,
    model: Option<&ModelRef>,
) -> Option<String> {
    serialize_extra_reasoning(tool_calls, &[], usage, error, model)
}

fn serialize_extra_reasoning(
    tool_calls: &[StoredToolCall],
    reasoning_blocks: &[StoredReasoningBlock],
    usage: Option<Usage>,
    error: Option<&str>,
    model: Option<&ModelRef>,
) -> Option<String> {
    if tool_calls.is_empty()
        && reasoning_blocks.is_empty()
        && usage.is_none()
        && error.is_none()
        && model.is_none()
    {
        return None;
    }
    serde_json::to_string(&StoredExtra {
        tool_calls: tool_calls.to_vec(),
        reasoning_blocks: reasoning_blocks.to_vec(),
        usage,
        error: error.map(str::to_string),
        model: model.cloned(),
    })
    .ok()
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
    input * spec.input_price.unwrap_or(0.0) as f64 + output * spec.output_price.unwrap_or(0.0) as f64
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
    computer_state: Mutex<HashMap<String, crate::computer::ComputerState>>,
    /// Chats whose turn is paused because the user took over, with when the
    /// pause started. Presence is the signal; the waiter polls this map, so a
    /// resume can never be missed.
    paused: Mutex<HashMap<String, std::time::Instant>>,
    /// Input hooks, installed only while a computer turn runs.
    takeover: Mutex<Option<Arc<crate::computer::TakeoverWatch>>>,
    /// Detached runs: how many are running, and which tasks wait for a slot.
    task_queue: Mutex<TaskQueue>,
    /// Live shell commands, keyed by command id. Only handles live here; the
    /// database is the durable record, and a missing handle just means there
    /// is nothing left to wait on.
    commands: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Running>>>>,
    /// Per-session watermark (epoch ms) for the memory extraction pass.
    memory_scan: Mutex<HashMap<String, i64>>,
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
                computer_state: Mutex::new(HashMap::new()),
                paused: Mutex::new(HashMap::new()),
                takeover: Mutex::new(None),
                task_queue: Mutex::new(TaskQueue::default()),
                commands: Mutex::new(HashMap::new()),
                memory_scan: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Database> {
        self.inner.db.lock().expect("db mutex poisoned")
    }

    pub fn config(&self) -> AppConfig {
        self.inner
            .config
            .lock()
            .expect("config mutex poisoned")
            .clone()
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

    /// Arms or disarms computer use for one chat (the composer's Computer chip).
    pub fn set_session_computer_access(&self, id: &str, enabled: bool) -> Result<()> {
        self.db().update_session(
            id,
            SessionUpdate {
                computer_access: Some(enabled),
                ..Default::default()
            },
        )
    }

    /// Sets or clears this chat's goal (the `/goal` command).
    pub fn set_session_goal(&self, id: &str, goal: Option<&str>) -> Result<()> {
        self.db().set_session_goal(id, goal)
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

    /// Marks a model as favourite so the picker can float it to the top.
    pub fn set_model_favorite(
        &self,
        provider_id: &str,
        model_id: &str,
        favorite: bool,
    ) -> Result<()> {
        let snapshot = {
            let mut config = self.inner.config.lock().expect("config mutex poisoned");
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

        let mut config = self.inner.config.lock().expect("config mutex poisoned");
        if let Some(entry) = config.providers.get_mut(provider_id) {
            entry.models = crate::providers::detect::merge_models(&entry.models, fetched);
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

        for extra in self.db().assistant_extras()? {
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
            let mut config = self.inner.config.lock().expect("config mutex poisoned");
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
            config
                .chat
                .computer_variant
                .clone()
                .filter(|variant| variant == "off" || variant == "none" || {
                    let spec = self.model_spec(&model.provider_id, &model.model_id);
                    spec.reasoning
                        .as_ref()
                        .is_some_and(|reasoning| reasoning.variants.iter().any(|v| v == variant))
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
        let tool_context = ToolContext {
            workdir: session
                .workdir
                .clone()
                .map(std::path::PathBuf::from)
                .or_else(|| crate::paths::scratch_dir().ok()),
            computer: computer_access,
        };

        // The chat's cast, when it has one. Multi-persona chats resolve each
        // speaker independently and tell the model who is present.
        let cast = self.session_cast(session_id).unwrap_or_default();
        let vars = persona_vars(&config, persona.as_ref(), &model, session.workdir.as_deref());

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
        let system = with_computer_mode(system, computer_access);
        let system = with_agent_mode(system, agent_mode);

        let chat = config.chat.clone();
        let plan = TurnPlan {
            system,
            variant,
            permission_mode,
            agent_mode,
            temperature: persona.as_ref().and_then(|p| p.capabilities.temperature),
            top_p: persona.as_ref().and_then(|p| p.capabilities.top_p),
            max_output_tokens: persona
                .as_ref()
                .and_then(|p| p.capabilities.max_output_tokens),
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
                    supervisor.report_task_failure(&supervised_session, &supervised_message, reason);
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
    fn report_task_failure(&self, session_id: &str, message_id: &str, reason: &str) {
        eprintln!("[loom] turn failed without a terminal event: {reason}");
        let extra = serialize_extra(&[], None, Some(reason), None);
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
        self.emit(EngineEvent::Error {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            error: reason.to_string(),
        });

        // A detached run whose turn died must still release its queue slot.
        if let Ok(Some(task)) = self.db().task_for_session(session_id) {
            if task.status == "running" {
                self.complete_task(&task.id, "failed", Some(reason), None);
            }
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
            max_steps,
            max_cost_usd,
            task_id,
        } = plan;
        let mut tool_defs: Vec<ToolDef> =
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
                .collect();

        // Persona-owned tools: memory only when the persona opted in, handoff
        // only when there is somebody to hand the turn to.
        if memory_enabled {
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
        let mut error: Option<String> = None;
        // Stream order across thinking spells and tool calls, so the
        // transcript can interleave them even when no text separates them.
        let mut seq = 0usize;

        // Budget the request against the model's own context window instead of
        // a message count: tool output ranges from a few tokens to hundreds of
        // thousands, so only a token budget keeps the wire inside the window.
        let spec = context::model_spec(&provider, &model.model_id);
        let max_output = context::output_limit(persona_max_output.unwrap_or(chat.max_output_tokens), &spec);
        let fixed = system
            .as_deref()
            .map(context::tokens_for)
            .unwrap_or(0)
            .saturating_add(
                serde_json::to_string(&tool_defs)
                    .map(|schema| context::tokens_for(&schema))
                    .unwrap_or(0),
            );
        let budget = context::input_budget(
            context::context_window(&provider, &model.model_id),
            max_output,
            fixed,
        );

        let max_rounds = (max_steps.unwrap_or(if computer_access {
            chat.max_tool_rounds.max(80)
        } else {
            chat.max_tool_rounds
        }) as usize)
            .clamp(1, 200);
        let mut round = 0usize;
        let mut wrapping_up = false;
        let mut last_call: Option<(String, String)> = None;
        let mut last_result: Option<(bool, String)> = None;
        let mut spent_usd = 0.0f64;

        // Computer turns own global state (hooks, the single-turn lock, a
        // pending pause, held keys). The guard hands it all back on every
        // exit path, and the supervisor's release covers `panic = "abort"`,
        // where Drop never runs.
        let _computer_guard = computer_access.then(|| ComputerTurnGuard::new(self, &session_id));

        // Input hooks exist only for computer turns: watching every keystroke
        // an app makes is not something Loom should do by default. The bridge
        // task turns a real input event into a pause.
        if computer_access {
            if let Ok(watch) = crate::computer::TakeoverWatch::start() {
                let watch = Arc::new(watch);
                *self.inner.takeover.lock().expect("takeover mutex poisoned") =
                    Some(Arc::clone(&watch));
                let engine = self.clone();
                let session = session_id.clone();
                let stop = watch.stop_flag();
                tokio::spawn(async move {
                    while !stop.load(Ordering::Relaxed) {
                        if watch.tripped() {
                            engine.pause_computer(&session);
                        }
                        tokio::time::sleep(Duration::from_millis(150)).await;
                    }
                });
            }
        }

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
                });
                let _ = self.db().update_message_extra(
                    &message_id,
                    serialize_extra(&stored_calls, None, None, None).as_deref(),
                );
            }
            round += 1;

            let history = match self.db().messages(&session_id) {
                Ok(history) => context::fit(&history, budget),
                Err(failure) => {
                    error = Some(failure.to_string());
                    break;
                }
            };

            let request = ChatRequest {
                provider: &provider,
                model: &model.model_id,
                system: system.as_deref(),
                messages: build_wire(&history),
                variant: variant.as_deref(),
                max_output_tokens: max_output,
                temperature,
                top_p,
                stream: true,
                // The wrap-up round offers no tools, so the model has nothing
                // to call and must write its summary.
                tools: if wrapping_up {
                    Vec::new()
                } else {
                    tool_defs.clone()
                },
                session_id: Some(&session_id),
            };

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

            let result = {
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
            let round_thinking = std::mem::take(&mut *round_reasoning.lock().expect("reasoning mutex"));
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
                    // Detached runs carry a spend cap; stop cleanly once the
                    // estimated cost of the whole run reaches it.
                    if let Some(cap) = max_cost_usd {
                        spent_usd += round_cost(&round_usage, &spec);
                        if spent_usd >= cap {
                            error = Some(format!(
                                "stopped: this run reached its ${cap:.2} spend cap \
                                 (~${spent_usd:.2} so far). Partial results are kept."
                            ));
                        }
                    }
                }
                Err(failure) => {
                    error = Some(failure.to_string());
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
            if computer_access && self.computer_paused(&session_id) {
                match self.wait_if_paused(&session_id, &cancel).await {
                    PauseExit::Resumed => {
                        // The resume invalidates what the model last saw — the
                        // user may have navigated somewhere. Force a fresh look.
                        if let Some(state) = self
                            .inner
                            .computer_state
                            .lock()
                            .expect("computer state mutex poisoned")
                            .get_mut(&session_id)
                        {
                            state.last_shot = None;
                            state.last_ui = None;
                        }
                        seq += 1;
                        stored_calls.push(StoredToolCall {
                            id: format!("loom-takeover-{}", uuid::Uuid::new_v4()),
                            name: "user_takeover".to_string(),
                            arguments: "{}".to_string(),
                            status: "ok".to_string(),
                            output: TAKEOVER_RESUME_NOTE.to_string(),
                            after: content.chars().count(),
                            seq,
                            images: Vec::new(),
                        });
                        let _ = self.db().update_message_extra(
                            &message_id,
                            serialize_extra(&stored_calls, None, None, None).as_deref(),
                        );
                        self.emit(EngineEvent::ComputerResumed {
                            session_id: session_id.clone(),
                        });
                    }
                    PauseExit::TimedOut => {
                        error = Some(TAKEOVER_TIMEOUT.to_string());
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

                // Read-only agent modes refuse mutating tools outright: no
                // permission card, just an error the model reads and works
                // around.
                let mode_blocked =
                    agent_mode.blocks_writes() && tools::is_blocked_in_plan(&call.name);

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

                // `ask_user` never goes through the permission gate: the card
                // is the prompt, and the user's answer is the outcome.
                let allowed = if call.name == tools::ASK_USER {
                    true
                } else if mode_blocked || harness_blocked.is_some() || scope_blocked {
                    false
                } else if computer_allowed {
                    true
                } else {
                    self.request_permission(&session_id, &message_id, &call, permission_mode)
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
                                        .run_computer_tool(&session_id, &call, &tool_context, &cancel)
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
                                            None => tools::execute(&call, &tool_context),
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

                stored_calls.push(StoredToolCall {
                    id: outcome.id.clone(),
                    name: outcome.name.clone(),
                    arguments: call.arguments.clone(),
                    status: if allowed {
                        if outcome.ok {
                            "ok".to_string()
                        } else {
                            "error".to_string()
                        }
                    } else {
                        "denied".to_string()
                    },
                    output: outcome.output.clone(),
                    after: call_offset,
                    seq: call_seq,
                    images: outcome.images.clone(),
                });

                self.emit(EngineEvent::ToolCallFinished {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: outcome.id.clone(),
                    ok: outcome.ok,
                    output: outcome.output.clone(),
                    images: outcome.images.clone(),
                });

                // The same call with byte-identical arguments returning the
                // same result twice in a row is a loop, not progress. A fresh
                // screenshot is exempt: waiting for a page and looking again
                // is not a loop.
                let signature = (call.name.clone(), call.arguments.clone());
                let result = (outcome.ok, outcome.output.clone());
                if call.name != "screenshot"
                    && last_call.as_ref() == Some(&signature)
                    && last_result.as_ref() == Some(&result)
                {
                    error = Some(format!(
                        "no progress: `{}` was called twice in a row with identical arguments \
                         and returned the same result. Stopped so a different approach can be \
                         tried.",
                        call.name
                    ));
                }
                last_call = Some(signature);
                last_result = Some(result);
            }

            if error.is_some() {
                break;
            }

            // Persist what we have so far: a tool round that follows needs the
            // reasoning echoed back, and a crash should not lose the text.
            let _ = self
                .db()
                .update_message(&message_id, &content, Some(&reasoning));
            let _ = self.db().update_message_extra(
                &message_id,
                serialize_extra_reasoning(&stored_calls, &reasoning_blocks, None, None, None)
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
            serialize_extra_reasoning(&stored_calls, &reasoning_blocks, Some(usage), None, Some(&model))
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
            } else if let Some(failure) = &error {
                ("failed", failure.clone())
            } else {
                ("done", "finished".to_string())
            };
            self.complete_task(task_id, status, Some(&detail), Some(&content));
        }

        match error {
            Some(failure) => {
                // Record why, so the reason is visible after a reload instead of
                // leaving an empty reply behind.
                let extra = serialize_extra_reasoning(
                    &stored_calls,
                    &reasoning_blocks,
                    None,
                    Some(&failure),
                    Some(&model),
                );
                let _ = self
                    .db()
                    .update_message_extra(&message_id, extra.as_deref());

                self.emit(EngineEvent::Error {
                    session_id: session_id.clone(),
                    message_id,
                    error: failure,
                })
            }
            None => {
                self.emit(EngineEvent::Done {
                    session_id: session_id.clone(),
                    message_id,
                    content,
                    reasoning: final_reasoning,
                    usage,
                });
            }
        }

        if task_id.is_none() && !cancel.load(Ordering::Relaxed) {
            self.maybe_generate_title(&session_id, &provider, &model, api_key.as_deref())
                .await;
            self.maybe_extract_memories(&session_id, &provider, &model, api_key.as_deref());
        }
    }

    /// Emits a permission prompt (Ask mode) and waits for the answer.
    async fn request_permission(
        &self,
        session_id: &str,
        message_id: &str,
        call: &ToolCall,
        mode: PermissionMode,
    ) -> bool {
        if !tools::requires_confirmation(mode, &call.name) {
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
        });

        // A detached run that needs approval says so in the Runs popup, and
        // waits: the timeout is long, but the run is not stuck by accident.
        if let Ok(Some(task)) = self.db().task_for_session(session_id) {
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
    /// repeated real input while paused changes nothing. The idle clock starts
    /// here, so walking away auto-resumes after [`PAUSE_IDLE_RESUME`].
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
        let mut paused = self.inner.paused.lock().expect("paused mutex poisoned");
        if paused.is_empty() {
            return false;
        }
        paused.clear();
        true
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
    /// lock, a pending pause, held keys and buttons, and screenshot retention.
    /// Safe to call twice (turn end and the supervisor's failure path).
    fn release_computer_turn(&self, session_id: &str) {
        crate::computer::release_held(&self.inner.computer_state, session_id);
        self.inner
            .paused
            .lock()
            .expect("paused mutex poisoned")
            .remove(session_id);
        if let Some(watch) = self
            .inner
            .takeover
            .lock()
            .expect("takeover mutex poisoned")
            .take()
        {
            watch.stop();
        }
        {
            let mut holder = self.inner.computer.lock().expect("computer mutex poisoned");
            if holder.as_deref() == Some(session_id) {
                *holder = None;
            }
        }
        self.prune_computer_screenshots(session_id);
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
            let mut config = self.inner.config.lock().expect("config mutex poisoned");
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
        {
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
                Some(_) => {}
                None => *holder = Some(session_id.to_string()),
            }
        }

        let options = crate::computer::ComputerOptions {
            screenshot_edge: self.config().chat.computer_screenshot_edge,
            cancel: Some(cancel.clone()),
        };
        Some(crate::computer::run(session_id, call, &self.inner.computer_state, &options).await)
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
            "run_command" => self.run_shell_command(session_id, tool_context, &arguments).await,
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
                self.stop_command(&id).map(|command| {
                    format!(
                        "stopped \"{}\" (id {}, pid {})",
                        command.label, command.id, command.pid
                    )
                })
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
                    tool_context
                        .workdir
                        .as_ref()
                        .and_then(|path| path.to_str()),
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
                    tool_context
                        .workdir
                        .as_ref()
                        .and_then(|path| path.to_str()),
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
        let image_model = self
            .config()
            .chat
            .image_model
            .clone()
            .unwrap_or_else(|| "gpt-image-1".to_string());

        let url = format!("{}/images/generations", provider.normalized_base_url());
        let mut request = self
            .inner
            .client
            .post(&url)
            .json(&crate::images::build_body(&image_model, prompt, size));
        if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
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
            max_output_tokens: context::output_limit(0, &spec),
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
        let provider = config
            .providers
            .get(&model.provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(model.provider_id.clone()))?;
        let embedding_model = config
            .chat
            .embedding_model
            .clone()
            .unwrap_or_else(|| "text-embedding-3-small".to_string());
        drop(config);

        let api_key = secrets::get_api_key(&model.provider_id)?;
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
        let provider = config
            .providers
            .get(&model.provider_id)
            .cloned()
            .ok_or_else(|| Error::UnknownProvider(model.provider_id.clone()))?;
        let embedding_model = config
            .chat
            .embedding_model
            .clone()
            .unwrap_or_else(|| "text-embedding-3-small".to_string());
        drop(config);

        let chunks = self.db().chunks(session_id)?;
        if chunks.is_empty() {
            return Ok(
                "The workspace index is empty. Ask the user to run \"Index workspace\" from the workspace menu first."
                    .to_string(),
            );
        }

        let api_key = secrets::get_api_key(&model.provider_id)?;
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
            let mut config = self.inner.config.lock().expect("config mutex poisoned");
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
            let mut config = self.inner.config.lock().expect("config mutex poisoned");
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
            let mut config = self.inner.config.lock().expect("config mutex poisoned");
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

    async fn maybe_generate_title(
        &self,
        session_id: &str,
        provider: &ProviderConfig,
        model: &ModelRef,
        api_key: Option<&str>,
    ) {
        let session = match self.db().get_session(session_id) {
            Ok(Some(session)) => session,
            _ => return,
        };
        if !session.title.trim().is_empty() {
            return;
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

        let (Some(user), Some(assistant)) = (first_user, first_assistant) else {
            return;
        };

        let config = self.config();
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
        drop(config);

        let title_key = secrets::get_api_key(&title_provider_id).unwrap_or_else(|_| {
            if title_provider_id == model.provider_id {
                api_key.map(str::to_string)
            } else {
                None
            }
        });

        let prompt = format!(
            "User: {}\n\nAssistant: {}",
            truncate(&user, 600),
            truncate(&assistant, 600)
        );

        let request = ChatRequest {
            provider: &title_provider,
            model: &title_model,
            system: Some("Write a title for this conversation. Reply with the title only: at most 6 words, no quotes, no trailing punctuation."),
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
                    return;
                }
            };

        // Fall back to the tail of the model's thinking when it never got to
        // writing a title.
        let candidate = if content.trim().is_empty() {
            reasoning.unwrap_or_default()
        } else {
            content
        };
        let title = clean_title(candidate.lines().last().unwrap_or_default());
        if title.is_empty() {
            eprintln!("[loom] title generation produced nothing usable");
            return;
        }

        let _ = self.db().update_session(
            session_id,
            SessionUpdate {
                title: Some(&title),
                ..Default::default()
            },
        );
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
            // line, or the file could not be written at all. The handle still
            // holds what arrived, so fall back to that.
            other => {
                if let Some(handle) = self.command_handle(id) {
                    if let Ok(handle) = handle.try_lock() {
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
        let running_now = self
            .inner
            .commands
            .lock()
            .expect("commands mutex poisoned")
            .len();
        if running_now >= MAX_BACKGROUND_COMMANDS {
            return Err(Error::other(format!(
                "{MAX_BACKGROUND_COMMANDS} commands are already running — wait for one to \
                 finish, or stop one with stop_command"
            )));
        }

        let log_path = crate::process::command_log_path(&uuid::Uuid::new_v4().to_string())?;
        let running = Running::spawn(command, cwd, &log_path)?;
        self.track_command(
            session_id,
            cwd,
            command,
            label,
            background,
            log_path,
            running,
        )
    }

    /// Records an already-spawned process and watches it to completion.
    ///
    /// Deliberately not subject to [`MAX_BACKGROUND_COMMANDS`]: the process
    /// exists either way, and refusing to track it would leave it invisible
    /// and unstoppable.
    fn track_command(
        &self,
        session_id: Option<&str>,
        cwd: &std::path::Path,
        command: &str,
        label: Option<&str>,
        background: bool,
        log_path: std::path::PathBuf,
        running: Running,
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

        let handle = Arc::new(tokio::sync::Mutex::new(running));
        self.inner
            .commands
            .lock()
            .expect("commands mutex poisoned")
            .insert(record.id.clone(), Arc::clone(&handle));

        // One watcher per command: it ends when the process does, whatever
        // ended it (exit, stop_command, or a crash).
        let engine = self.clone();
        let watched = record.id.clone();
        tokio::spawn(async move {
            let status = handle.lock().await.wait().await;
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

        match running
            .wait_timeout(crate::process::COMMAND_TIMEOUT)
            .await
        {
            Some(status) => {
                let (stdout, stderr) = running.finish().await;
                drop(running);
                // A command that finished inside the cap is reported in the
                // transcript like any other tool; there is nothing to track,
                // so the log goes with it.
                let _ = std::fs::remove_file(&log_path);
                Ok(format_command_report(
                    status.code(),
                    &stdout,
                    &stderr,
                ))
            }
            None => {
                // Still going after two minutes. Adopt it: the row is what
                // makes it visible in the Runs panel and stoppable, and the
                // log keeps filling either way.
                let record = self.track_command(
                    Some(session_id),
                    &root,
                    &command,
                    label.as_deref(),
                    false,
                    log_path.clone(),
                    running,
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

    fn command_handle(&self, id: &str) -> Option<Arc<tokio::sync::Mutex<Running>>> {
        self.inner
            .commands
            .lock()
            .ok()?
            .get(id)
            .map(Arc::clone)
    }

    /// Records a command's exit and drops its handle. A command the user
    /// stopped keeps the word "stopped", whatever its exit code says.
    fn finish_command(&self, id: &str, exit_code: Option<i32>) {
        let stopped = self
            .db()
            .command(id)
            .ok()
            .flatten()
            .map(|record| record.status == "stopped")
            .unwrap_or(false);
        let status = match (stopped, exit_code) {
            (true, _) => "stopped",
            (false, Some(0)) => "done",
            (false, _) => "failed",
        };
        if let Err(error) = self.db().set_command_status(id, status, exit_code) {
            eprintln!("[loom] could not update command {id}: {error}");
        }
        if let Ok(mut commands) = self.inner.commands.lock() {
            commands.remove(id);
        }
        // Only announce it if the row is still there (it may have been deleted
        // while the process was winding down).
        if let Ok(Some(command)) = self.db().command(id) {
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
        crate::process::kill_tree(record.pid);
        self.db().set_command_status(id, "stopped", None)?;
        let updated = self.db().command(id)?.unwrap_or(record);
        self.emit(EngineEvent::CommandChanged {
            command: updated.clone(),
        });
        Ok(updated)
    }

    /// Forgets a command, stopping it first: deleting the row of a live
    /// process would leave a process nobody can find again.
    pub fn delete_command(&self, id: &str) -> Result<()> {
        if let Some(record) = self.db().command(id)? {
            if record.status == "running" {
                crate::process::kill_tree(record.pid);
            }
            let _ = std::fs::remove_file(&record.log_path);
        }
        if let Ok(mut commands) = self.inner.commands.lock() {
            commands.remove(id);
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
        let (provider_id, model_id) = match (
            request.provider_id.clone(),
            request.model_id.clone(),
        ) {
            (Some(provider), Some(model)) => (provider, model),
            _ => match (config.chat.provider_id.clone(), config.chat.model_id.clone()) {
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

        if let Err(error) = self.send_limited(&task.session_id, &task.prompt, model, Vec::new(), limits)
        {
            self.complete_task(
                &task_id,
                "failed",
                Some(&error.to_string()),
                None,
            );
        }
    }

    fn update_task(&self, task_id: &str, status: &str, detail: Option<&str>, result: Option<&str>) {
        if let Err(error) = self.db().set_task_status(task_id, status, detail, result) {
            eprintln!("[loom] could not update task {task_id}: {error}");
            return;
        }
        if let Ok(Some(task)) = self.db().task(task_id) {
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
            if let Ok(Some(task)) = self.db().task(task_id) {
                if let Some(origin) = task.origin_session.clone() {
                    let text = match (status, result) {
                        ("done", Some(result)) if !result.trim().is_empty() => {
                            format!("Background run \"{}\" finished:\n\n{}", task.title, result)
                        }
                        ("done", _) => format!("Background run \"{}\" finished.", task.title),
                        (_, Some(detail)) if !detail.trim().is_empty() => {
                            format!("Background run \"{}\" failed: {detail}", task.title)
                        }
                        _ => format!("Background run \"{}\" failed.", task.title),
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
                .recall_facts(&scopes, &text, crate::memory::RECALL_LIMIT, provider, api_key)
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
        let config = self.config();
        let embedding_model = config
            .chat
            .embedding_model
            .clone()
            .unwrap_or_else(|| "text-embedding-3-small".to_string());
        drop(config);

        crate::embeddings::embed(
            &self.inner.client,
            provider,
            api_key,
            &embedding_model,
            inputs,
        )
        .await
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
            scope,
            content,
            pinned,
            source,
            session_id,
            message_id,
            bytes,
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
                Some(lite_provider) => (lite.provider_id.clone(), lite_provider.clone(), lite.model_id),
                None => (model.provider_id.clone(), provider.clone(), model.model_id.clone()),
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

        let memory = crate::memory::new_memory(
            scope,
            content,
            pinned,
            "user",
            None,
            None,
            embedding,
        );
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
    /// confirmation card (see `tools::always_asks`).
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
        if let Ok(Some(fresh)) = self.db().job(&job.id) {
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
fn reasoning_echo(message: &Message, index: usize, history: &[Message]) -> Option<String> {
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
    let last_user = history.iter().rposition(|message| message.role == Role::User);
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
fn format_command_report(
    code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> String {
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
    }
}

fn parse_agent_mode(value: &str) -> Option<AgentMode> {
    match value {
        "plan" => Some(AgentMode::Plan),
        "review" => Some(AgentMode::Review),
        "build" => Some(AgentMode::Build),
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

/// Appends the computer-use note to whatever system prompt is in play, so the
/// model knows it can see and drive the machine — and how to do it without
/// flailing. Placed before the agent-mode note, so Plan gets the last word.
fn with_computer_mode(system: Option<String>, computer: bool) -> Option<String> {
    if !computer {
        return system;
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
    if let Some(items) = arguments
        .get("todos")
        .and_then(serde_json::Value::as_array)
    {
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
fn append_note(system: Option<String>, note: &str) -> String {    match system {
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

        let event = receiver.try_recv().expect("a terminal event");
        assert!(matches!(event, EngineEvent::Error { .. }), "{event:?}");

        let stored = engine.messages(&session.id).unwrap();
        let assistant = stored.iter().find(|m| m.id == message_id).unwrap();
        assert_eq!(
            parse_error(assistant.extra.as_deref()).as_deref(),
            Some("the turn crashed")
        );
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
                    Some(event @ EngineEvent::Error { .. }) => break event,
                    Some(event @ EngineEvent::Done { .. }) => break event,
                    Some(_) => continue,
                    None => panic!("the engine stopped without a terminal event"),
                }
            }
        })
        .await
        .expect("a terminal event arrived");
        assert!(
            matches!(event, EngineEvent::Error { .. }),
            "expected an error event, got {event:?}"
        );

        let messages = engine.messages(&session.id).unwrap();
        let assistant = messages.last().expect("assistant message");
        let recorded = parse_error(assistant.extra.as_deref())
            .expect("the failure reason is stored on the message");
        assert!(!recorded.trim().is_empty());

        // And it survives a reload from disk, which is what the UI does.
        let reopened = Database::open(&dir.path().join("loom.db")).unwrap();
        let stored = reopened.messages(&session.id).unwrap();
        assert!(parse_error(stored.last().unwrap().extra.as_deref()).is_some());

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
        };
        let json = serde_json::to_string(&done).unwrap();
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"inputTokens\""), "{json}");

        let permission = EngineEvent::ToolPermissionRequest {
            session_id: "s1".into(),
            message_id: "m1".into(),
            call_id: "c1".into(),
            name: "read_file".into(),
            arguments: "{}".into(),
            read_only: true,
        };
        let json = serde_json::to_string(&permission).unwrap();
        assert!(json.contains("\"callId\":\"c1\""), "{json}");
        assert!(json.contains("\"readOnly\":true"), "{json}");

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
            Some(Usage {
                input_tokens: Some(1_000_000),
                output_tokens: Some(1_000_000),
            }),
            None,
            Some(&ModelRef::new("priced", "priced-model")),
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
            Some(Usage {
                input_tokens: Some(500),
                output_tokens: Some(100),
            }),
            None,
            None,
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
                Some(Usage {
                    input_tokens: Some(input),
                    output_tokens: Some(output),
                }),
                None,
                Some(&ModelRef::new(provider, "m")),
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
            }],
            None,
            None,
            None,
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
            }],
            None,
            None,
            None,
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
            }],
            None,
            None,
            None,
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
        for mode in [AgentMode::Plan, AgentMode::Build] {
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
}
