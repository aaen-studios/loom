//! The engine: owns sessions, model calls, streaming, tools, and titles.
//!
//! Streams and tool loops are engine-owned tasks, not request handlers — a
//! chat keeps producing while the user switches chats, minimises the window,
//! or reloads the webview. Events go out through a callback supplied by the
//! shell, so this crate stays free of Tauri.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::attachments::Attachment;
use crate::config::{AppConfig, ChatDefaults, ModelRef, PermissionMode};
use crate::db::{now_ms, Database, Message, Role, Session, SessionUpdate};
use crate::providers::stream::{self, Cancellation};
use crate::providers::{
    ChatRequest, ContentPart, Delta, ToolDef, Usage, WireMessage, WireToolCall,
};
use crate::provider::{ModelSpec, ProviderConfig};
use crate::tools::{self, ToolCall, ToolContext};
use crate::{catalog, provider as provider_mod, secrets, Error, Result};

/// Hard cap on tool round-trips per user turn.
pub const MAX_TOOL_ROUNDS: usize = 8;
/// How long an "ask" permission prompt waits before denying.
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(600);

pub type SharedConfig = Arc<Mutex<AppConfig>>;
pub type EmitFn = Arc<dyn Fn(EngineEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone, Serialize)]
// `rename_all` renames the *variants* (Started -> "started"); the fields of an
// enum variant need `rename_all_fields`, otherwise the wire carries
// `session_id` while the frontend reads `sessionId` and every event is dropped.
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "type")]
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
    },
    ToolCallStarted {
        session_id: String,
        message_id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    ToolCallFinished {
        session_id: String,
        message_id: String,
        call_id: String,
        ok: bool,
        output: String,
    },
    ToolPermissionRequest {
        session_id: String,
        message_id: String,
        call_id: String,
        name: String,
        arguments: String,
        read_only: bool,
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
            EngineEvent::ToolPermissionRequest { .. } => "tool-permission",
            EngineEvent::Done { .. } => "done",
            EngineEvent::Error { .. } => "error",
            EngineEvent::Title { .. } => "title",
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
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredExtra {
    #[serde(default)]
    tool_calls: Vec<StoredToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usage: Option<Usage>,
    /// Why a turn failed, kept so the reason survives a reload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub fn parse_stored_tools(extra: Option<&str>) -> Vec<StoredToolCall> {
    extra
        .and_then(|raw| serde_json::from_str::<StoredExtra>(raw).ok())
        .map(|stored| stored.tool_calls)
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
) -> Option<String> {
    if tool_calls.is_empty() && usage.is_none() && error.is_none() {
        return None;
    }
    serde_json::to_string(&StoredExtra {
        tool_calls: tool_calls.to_vec(),
        usage,
        error: error.map(str::to_string),
    })
    .ok()
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
    /// Connected MCP servers and their cached tool lists.
    mcp: tokio::sync::Mutex<McpState>,
}

#[derive(Default)]
struct McpState {
    clients: HashMap<String, crate::mcp::McpClient>,
    tools: Vec<(String, crate::mcp::McpTool)>,
    /// Set when servers are added/removed so the next turn reconnects.
    dirty: bool,
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
                mcp: tokio::sync::Mutex::new(McpState::default()),
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
            workdir: None,
            permission_mode: None,
            created_at: now_ms(),
            updated_at: now_ms(),
        };
        self.db().create_session(&session)?;
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
        let fetched = crate::providers::detect::fetch_models(
            &self.inner.client,
            &provider,
            api_key.as_deref(),
        )
        .await?;

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
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(Error::Other(
                "send() needs a Tokio runtime: call it from an async Tauri command".into(),
            ));
        }
        let text = text.trim();
        if text.is_empty() && attachments.is_empty() {
            return Err(Error::Other("message is empty".into()));
        }

        let session = self
            .db()
            .get_session(session_id)?
            .ok_or_else(|| Error::UnknownSession(session_id.to_string()))?;

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
            None => self.effective_model(&session)?,
        };

        let config = self.config();
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
        self.db().add_message(&Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            role: Role::User,
            content: text.to_string(),
            reasoning: None,
            extra: crate::attachments::serialize_extra(&attachments),
            created_at: now,
        })?;

        let assistant_id = uuid::Uuid::new_v4().to_string();
        self.db().add_message(&Message {
            id: assistant_id.clone(),
            session_id: session_id.to_string(),
            role: Role::Assistant,
            content: String::new(),
            reasoning: None,
            extra: None,
            created_at: now + 1,
        })?;

        let persona = session
            .persona_id
            .as_ref()
            .and_then(|id| config.personas.iter().find(|p| &p.id == id).cloned());

        let variant = session
            .variant
            .clone()
            .or_else(|| persona.as_ref().and_then(|p| p.variant.clone()))
            .or_else(|| config.chat.variant.clone());

        let permission_mode = session
            .permission_mode
            .as_deref()
            .and_then(parse_permission_mode)
            .unwrap_or(config.chat.permission_mode);

        let tool_context = ToolContext {
            workdir: session.workdir.clone().map(std::path::PathBuf::from),
        };

        let system = session
            .system_prompt
            .clone()
            .or_else(|| persona.as_ref().map(|p| p.system_prompt.clone()));

        // Project instructions, the same convention agents use elsewhere.
        let system = match tool_context.workdir.as_ref() {
            Some(workdir) => {
                let agents_md = workdir.join("AGENTS.md");
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

        let chat = config.chat.clone();
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
                        variant,
                        system,
                        chat,
                        api_key,
                        cancel,
                        permission_mode,
                        tool_context,
                    )
                    .await;
            });

            if let Err(join_error) = turn.await {
                let reason = if join_error.is_panic() {
                    "the turn crashed while running a tool or provider call — nothing was lost, try again"
                } else {
                    "the turn was interrupted"
                };
                supervisor.report_task_failure(&supervised_session, &supervised_message, reason);
            }
        });

        Ok(assistant_id)
    }

    /// Terminal event for a task that died without one: records the reason and
    /// clears the busy flag so the UI cannot hang.
    fn report_task_failure(&self, session_id: &str, message_id: &str, reason: &str) {
        eprintln!("[loom] turn failed without a terminal event: {reason}");
        let extra = serialize_extra(&[], None, Some(reason));
        let _ = self
            .db()
            .update_message_extra(message_id, extra.as_deref());
        self.inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .remove(session_id);
        self.emit(EngineEvent::Error {
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            error: reason.to_string(),
        });
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_completion(
        &self,
        session_id: String,
        message_id: String,
        provider: ProviderConfig,
        model: ModelRef,
        variant: Option<String>,
        system: Option<String>,
        chat: ChatDefaults,
        api_key: Option<String>,
        cancel: Cancellation,
        permission_mode: PermissionMode,
        tool_context: ToolContext,
    ) {
        let tool_defs: Vec<ToolDef> = tools::specs()
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
            .chain(self.mcp_tool_defs().await)
            .collect();

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut usage = Usage::default();
        let mut stored_calls: Vec<StoredToolCall> = Vec::new();
        let mut error: Option<String> = None;

        for _round in 0..MAX_TOOL_ROUNDS {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let history = match self
                .db()
                .recent_messages(&session_id, chat.history_limit.max(2))
            {
                Ok(history) => history,
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
                max_output_tokens: Some(chat.max_output_tokens),
                stream: true,
                tools: tool_defs.clone(),
                session_id: Some(&session_id),
            };

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
                            let flush = text_buffer
                                .lock()
                                .expect("text buffer")
                                .push(&text);
                            if let Some(batch) = flush {
                                self.emit(EngineEvent::Delta {
                                    session_id: session.clone(),
                                    message_id: message.clone(),
                                    text: batch,
                                });
                            }
                        }
                        Delta::Reasoning { text } => {
                            round_reasoning.lock().expect("reasoning mutex").push_str(&text);
                            let flush = reasoning_buffer
                                .lock()
                                .expect("reasoning buffer")
                                .push(&text);
                            if let Some(batch) = flush {
                                self.emit(EngineEvent::Reasoning {
                                    session_id: session.clone(),
                                    message_id: message.clone(),
                                    text: batch,
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
            if let Some(batch) = reasoning_buffer
                .lock()
                .expect("reasoning buffer")
                .flush()
            {
                self.emit(EngineEvent::Reasoning {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    text: batch,
                });
            }

            let text = round_text.lock().expect("text mutex").clone();
            content.push_str(&text);
            reasoning.push_str(&round_reasoning.lock().expect("reasoning mutex"));
            let calls = std::mem::take(&mut *round_calls.lock().expect("calls mutex"));

            match result {
                Ok(round_usage) => {
                    if round_usage.input_tokens.is_some() {
                        usage.input_tokens = round_usage.input_tokens;
                    }
                    if round_usage.output_tokens.is_some() {
                        usage.output_tokens = round_usage.output_tokens;
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

            // Execute the calls the model asked for, then loop so it can use
            // the results.
            for call in calls {
                let call = ToolCall {
                    id: call.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    name: call.name.unwrap_or_default(),
                    arguments: call.arguments,
                };

                if call.name.is_empty() {
                    continue;
                }

                self.emit(EngineEvent::ToolCallStarted {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });

                let allowed = self
                    .request_permission(
                        &session_id,
                        &message_id,
                        &call,
                        permission_mode,
                    )
                    .await;

                let outcome = if allowed {
                    match crate::mcp::parse_tool_name(&call.name) {
                        Some((server, tool)) => {
                            match self.call_mcp_tool(&server, &tool, &call.arguments).await {
                                Ok(output) => crate::tools::ToolOutcome {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    ok: true,
                                    output,
                                },
                                Err(error) => crate::tools::ToolOutcome {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    ok: false,
                                    output: error.to_string(),
                                },
                            }
                        }
                        None => match self.run_web_tool(&call).await {
                            Some(outcome) => outcome,
                            None => {
                                match self
                                    .run_agent_tool(
                                        &session_id,
                                        &call,
                                        &provider,
                                        &model,
                                        api_key.as_deref(),
                                        &tool_context,
                                        permission_mode,
                                    )
                                    .await
                                {
                                    Some(outcome) => outcome,
                                    None => tools::execute(&call, &tool_context),
                                }
                            }
                        },
                    }
                } else {
                    crate::tools::ToolOutcome {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        ok: false,
                        output: "denied by the user".to_string(),
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
                });

                self.emit(EngineEvent::ToolCallFinished {
                    session_id: session_id.clone(),
                    message_id: message_id.clone(),
                    call_id: outcome.id.clone(),
                    ok: outcome.ok,
                    output: outcome.output.clone(),
                });
            }

            // Persist what we have so far: a tool round that follows needs the
            // reasoning echoed back, and a crash should not lose the text.
            let _ = self
                .db()
                .update_message(&message_id, &content, Some(&reasoning));
            let _ = self.db().update_message_extra(
                &message_id,
                serialize_extra(&stored_calls, None, None).as_deref(),
            );

            if cancel.load(Ordering::Relaxed) {
                break;
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
            serialize_extra(&stored_calls, Some(usage), None).as_deref(),
        );

        self.inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned")
            .remove(&session_id);

        match error {
            Some(failure) => {
                // Record why, so the reason is visible after a reload instead of
                // leaving an empty reply behind.
                let extra = serialize_extra(&stored_calls, None, Some(&failure));
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

        if !cancel.load(Ordering::Relaxed) {
            self.maybe_generate_title(&session_id, &provider, &model, api_key.as_deref())
                .await;
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

    pub fn cancel(&self, session_id: &str) {
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
        let cancels = self
            .inner
            .cancels
            .lock()
            .expect("cancels mutex poisoned");
        for cancel in cancels.values() {
            cancel.store(true, Ordering::Relaxed);
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
                crate::web::search(&self.inner.client, &query, max).await
            }
            crate::web::FETCH_TOOL => {
                let url = arguments
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                crate::web::fetch(&self.inner.client, &url).await
            }
            _ => return None,
        };

        Some(match result {
            Ok(output) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: true,
                output,
            },
            Err(error) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: error.to_string(),
            },
        })
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
    ) -> Option<crate::tools::ToolOutcome> {
        let arguments: serde_json::Value = if call.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null)
        };

        let result = match call.name.as_str() {
            "run_command" => {
                let command = arguments
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                crate::tools::run_command(tool_context, &command).await
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
                self.spawn_subagent(provider, model, api_key, system.as_deref(), &task, Some(session_id))
                    .await
            }
            _ => return None,
        };

        Some(match result {
            Ok(output) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: true,
                output,
            },
            Err(error) => crate::tools::ToolOutcome {
                id: call.id.clone(),
                name: call.name.clone(),
                ok: false,
                output: error.to_string(),
            },
        })
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

        let url = format!(
            "{}/images/generations",
            provider.normalized_base_url()
        );
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

        let request = ChatRequest {
            provider,
            model: &model.model_id,
            system: Some(system.unwrap_or(
                "You are a focused subagent. Complete the task and reply with the result only.",
            )),
            messages: vec![WireMessage::text("user", task)],
            variant: None,
            max_output_tokens: Some(4_096),
            stream: false,
            tools: Vec::new(),
            session_id,
        };

        let (content, _, _) =
            stream::run_once(&self.inner.client, &request, api_key).await?;
        Ok(content)
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
    /// unavailable). Metadata comes from the bundled catalog when known.
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
                .or_insert_with(|| catalog::lookup(model_id).unwrap_or_else(catalog::fallback));
            config.clone()
        };
        crate::config::save(&snapshot)
    }

    /// Edits the metadata the picker shows for one model.
    pub fn set_model_spec(
        &self,
        provider_id: &str,
        model_id: &str,
        context: Option<u32>,
        output: Option<u32>,
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
        state
            .tools
            .iter()
            .filter(|(id, _)| id == server)
            .count()
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

/// Builds the provider wire from stored history, including tool calls and
/// their results.
fn build_wire(history: &[Message]) -> Vec<WireMessage> {
    let mut wire = Vec::new();

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
                    // Reasoning models (DeepSeek and friends) reject requests
                    // that omit the thinking they produced earlier.
                    reasoning: message.reasoning.clone().filter(|text| !text.trim().is_empty()),
                });

                for call in &stored_tools {
                    let body = if call.status == "ok" {
                        call.output.clone()
                    } else {
                        format!("ERROR: {}", call.output)
                    };
                    wire.push(WireMessage::tool_result(call.id.clone(), body));
                }
            }
        }
    }

    wire
}

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
                Ok(bytes) => parts.push(ContentPart::Image {
                    mime: attachment.mime.clone(),
                    base64: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        bytes,
                    ),
                    name: attachment.name.clone(),
                }),
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

fn permission_mode_str(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Ask => "ask",
        PermissionMode::AutoReadOnly => "auto-read-only",
        PermissionMode::AutoAll => "auto-all",
    }
}

fn parse_permission_mode(value: &str) -> Option<PermissionMode> {
    match value {
        "ask" => Some(PermissionMode::Ask),
        "auto-read-only" => Some(PermissionMode::AutoReadOnly),
        "auto-all" => Some(PermissionMode::AutoAll),
        _ => None,
    }
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
            .create_session(None, None, None, None, None)
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
                created_at: now_ms(),
            })
            .unwrap();
        engine
            .db()
            .update_message(&message_id, &"", None)
            .unwrap();

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
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn choosing_a_model_is_persisted_on_the_session() {
        use crate::config::AppConfig;
        use std::sync::Arc;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let db = Database::open(&dir.path().join("loom.db")).unwrap();
        let config: SharedConfig = Arc::new(Mutex::new(AppConfig::default()));
        let engine = Engine::new(db, config, Arc::new(|_| {}));

        let session = engine
            .create_session(None, Some("provider-a".into()), Some("model-a".into()), None, None)
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
        assert_eq!(
            engine.effective_model(&stored).unwrap().model_id,
            "model-b"
        );

        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn favourites_toggle_and_persist_in_config() {
        use crate::config::{AppConfig, ModelRef};
        use crate::provider::{ModelSpec, ProviderConfig};
        use std::sync::Arc;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let db = Database::open(&dir.path().join("loom.db")).unwrap();
        let mut config = AppConfig::default();
        let mut provider = ProviderConfig {
            name: "Test".into(),
            base_url: "https://example.com/v1".into(),
            ..Default::default()
        };
        provider.models.insert("model-a".into(), ModelSpec::default());
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

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            .create_session(None, None, None, None, None)
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
        assert_eq!(clean_title(candidate.lines().last().unwrap_or_default()), "Capital of Norway");
    }

    #[test]
    fn titles_are_cleaned() {
        assert_eq!(clean_title("  \"Rust ownership help.\"\n"), "Rust ownership help.");
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
        merge_tool_delta(&mut calls, 0, Some("a".into()), Some("read_file".into()), None);
        merge_tool_delta(&mut calls, 0, None, None, Some("{\"path\":".into()));
        merge_tool_delta(&mut calls, 0, None, None, Some("\"x\"}".into()));
        merge_tool_delta(&mut calls, 1, Some("b".into()), Some("datetime".into()), Some("{}".into()));

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id.as_deref(), Some("a"));
        assert_eq!(calls[0].arguments, "{\"path\":\"x\"}");
        assert_eq!(calls[1].name.as_deref(), Some("datetime"));
    }

    #[test]
    fn wire_includes_tool_results_after_assistant_calls() {
        let stored = serialize_extra(&[StoredToolCall {
            id: "call-1".into(),
            name: "read_file".into(),
            arguments: "{\"path\":\"a.txt\"}".into(),
            status: "ok".into(),
            output: "contents".into(),
        }], None, None)
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

    #[test]
    fn denied_tool_results_are_marked_as_errors() {
        let stored = serialize_extra(&[StoredToolCall {
            id: "c1".into(),
            name: "write_file".into(),
            arguments: "{}".into(),
            status: "denied".into(),
            output: "denied by the user".into(),
        }], None, None)
        .unwrap();
        let history = vec![message(Role::Assistant, "trying", Some(&stored))];
        let wire = build_wire(&history);
        assert!(wire[1].joined_text().starts_with("ERROR:"));
    }

    #[test]
    fn permission_modes_encode_and_parse() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::AutoReadOnly,
            PermissionMode::AutoAll,
        ] {
            assert_eq!(parse_permission_mode(permission_mode_str(mode)), Some(mode));
        }
        assert_eq!(parse_permission_mode("nonsense"), None);
    }
}






