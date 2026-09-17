//! Tauri command layer. Thin: every command delegates to `loom-core` and
//! returns camelCase JSON the frontend already has types for.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use loom_core::config::AppConfig;
use loom_core::db::{Job, Memory, Message, Session, Task};
use loom_core::engine::{Engine, SharedConfig};
use loom_core::persona::Persona;
use loom_core::provider::{
    Modality, ModelSpec, ProviderConfig, ProviderPreset, ReasoningSpec, PRESETS,
};
use loom_core::{config, paths, secrets, Error};

pub struct AppState {
    pub engine: Engine,
    pub config: SharedConfig,
    /// Voice mode's worker thread and cancellation flag.
    pub voice: crate::voice::VoiceService,
    /// Speech to text: its own thread, so recognition is never queued behind
    /// synthesis.
    pub dictation: crate::voice::DictationService,
    /// Whether an install is running, so two cannot start at once.
    pub installing: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    pub fn new(engine: Engine, config: SharedConfig) -> Self {
        Self {
            engine,
            config,
            voice: crate::voice::VoiceService::new(),
            dictation: crate::voice::DictationService::new(),
            installing: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn snapshot(&self) -> AppConfig {
        self.config.lock().expect("config mutex poisoned").clone()
    }

    /// Saves the whole config back to disk.
    ///
    /// `pub(crate)` rather than private: the voice module owns its own settings
    /// surface and needs the same write-and-reload path, including the
    /// mutex discipline, rather than a second implementation of it.
    pub(crate) fn mutate(&self, change: impl FnOnce(&mut AppConfig)) -> Result<AppConfig, String> {
        let snapshot = {
            let mut guard = self.config.lock().expect("config mutex poisoned");
            change(&mut guard);
            guard.clone()
        };
        config::save(&snapshot).map_err(|e| e.to_string())?;
        Ok(snapshot)
    }

    /// Mutates harness state through `loom_core::harness`, so UI-driven edits
    /// share the model-driven validation, the pre-write backup, and the MCP
    /// cache invalidation. Returns the whole config, as the commands do.
    fn harness_mutate(&self, name: &str, args: serde_json::Value) -> Result<AppConfig, String> {
        let section = loom_core::harness::section_of(name);
        let snapshot = {
            let mut guard = self.config.lock().expect("config mutex poisoned");
            if let Err(error) = loom_core::harness::backup(&guard) {
                eprintln!("[loom] harness backup failed: {error}");
            }
            loom_core::harness::apply_ui(&mut guard, name, &args).map_err(to_string)?;
            guard.clone()
        };
        config::save(&snapshot).map_err(|e| e.to_string())?;
        if section == "mcp" {
            self.engine.invalidate_mcp();
        }
        Ok(snapshot)
    }
}

fn to_string(error: Error) -> String {
    error.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
    loom_home: String,
}

#[tauri::command]
pub fn app_info() -> Result<AppInfo, String> {
    let loom_home = paths::loom_home().map_err(to_string)?;
    Ok(AppInfo {
        name: "Loom",
        version: env!("CARGO_PKG_VERSION"),
        loom_home: loom_home.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.snapshot()
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: AppConfig) -> Result<AppConfig, String> {
    // The UI only owns appearance fields; providers, personas, chat defaults,
    // MCP servers, prompts, workspaces, and the search backend are mutated
    // through their own commands and preserved here. MCP servers and prompts
    // are on the list because the model can write them from Atelier while the
    // UI is mid-debounce: without this, a theme tweak would revert the model's
    // work.
    state.mutate(|current| {
        let providers = std::mem::take(&mut current.providers);
        let personas = std::mem::take(&mut current.personas);
        let persona_groups = std::mem::take(&mut current.persona_groups);
        let user_profile = current.user_profile.clone();
        let mcp_servers = std::mem::take(&mut current.mcp_servers);
        let prompts = std::mem::take(&mut current.prompts);
        let chat = current.chat.clone();
        let workspaces = std::mem::take(&mut current.workspaces);
        let search_provider = current.search_provider;
        *current = config.clone();
        current.providers = providers;
        current.personas = personas;
        current.persona_groups = persona_groups;
        current.user_profile = user_profile;
        current.mcp_servers = mcp_servers;
        current.prompts = prompts;
        current.chat = chat;
        current.workspaces = workspaces;
        current.search_provider = search_provider;
    })
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_provider_presets() -> &'static [ProviderPreset] {
    PRESETS
}

#[tauri::command]
pub fn upsert_provider(
    state: State<'_, AppState>,
    id: String,
    provider: ProviderConfig,
) -> Result<AppConfig, String> {
    if id.trim().is_empty() {
        return Err("provider id must not be empty".into());
    }
    state.mutate(move |config| {
        config.providers.insert(id, provider);
    })
}

#[tauri::command]
pub fn delete_provider(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    let _ = secrets::delete_api_key(&id);
    state.mutate(move |config| {
        config.providers.remove(&id);
        if config.chat.provider_id.as_deref() == Some(id.as_str()) {
            config.chat.provider_id = None;
            config.chat.model_id = None;
        }
        if config.chat.lite.as_ref().map(|l| l.provider_id.as_str()) == Some(id.as_str()) {
            config.chat.lite = None;
        }
        // Aux refs that named the deleted provider lose the qualification but
        // keep the model id, so `resolve_aux_model` can fall back to whichever
        // provider still serves it rather than silently embedding nowhere.
        for reference in [
            config.chat.image_model.as_mut(),
            config.chat.embedding_model.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            if reference.provider_id == id {
                reference.provider_id.clear();
            }
        }
        // Recent models pointing into the deleted provider would show up in the
        // picker as dead rows.
        config
            .chat
            .recent_models
            .retain(|model| model.provider_id != id);
    })
}

/// Copies a provider instance so one vendor can be configured more than once —
/// two OpenCode Go plans, each with its own key. The copy inherits the
/// endpoint, headers, model catalogue and model selection, but no API key:
/// keys live in the credential vault under the provider id, so the new
/// instance starts keyless and cannot bill the original's account by accident.
#[tauri::command]
pub fn duplicate_provider(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state
        .engine
        .duplicate_provider(&id)
        .map_err(to_string)?;
    Ok(state.snapshot())
}

/// Turns a batch of models on or off for one provider. Batched because
/// "select all shown" across a search result is a single user action.
#[tauri::command]
pub fn set_models_selected(
    state: State<'_, AppState>,
    provider_id: String,
    model_ids: Vec<String>,
    selected: bool,
) -> Result<AppConfig, String> {
    state
        .engine
        .set_models_selected(&provider_id, &model_ids, selected)
        .map_err(to_string)?;
    Ok(state.snapshot())
}

/// Sets a provider's `autoSelectModels` flag: whether models discovered by a
/// refresh arrive selected. Off makes a large gateway opt-in.
#[tauri::command]
pub fn set_provider_auto_select(
    state: State<'_, AppState>,
    id: String,
    auto_select: bool,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        if let Some(provider) = config.providers.get_mut(&id) {
            provider.auto_select_models = auto_select;
            if auto_select {
                // Switching back on clears the denylist, so the whole catalogue
                // is visible again rather than only future discoveries.
                provider.disabled_models.clear();
            }
        }
    })
}

/// Applies the configured hotkey immediately (register, re-register, or none).
#[tauri::command]
pub fn set_hotkey(app: AppHandle, enabled: bool, keys: String) -> Result<(), String> {
    crate::set_hotkey_now(&app, enabled, &keys)
}

/// Whether Loom is registered to launch when the user signs in. The registry
/// entry, not `config.json`, is the source of truth, so the toggle always
/// reflects what is actually configured.
#[tauri::command]
pub fn autostart_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Enables or disables launch-at-login and returns the state that took effect.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())?;
    } else {
        manager.disable().map_err(|e| e.to_string())?;
    }
    manager.is_enabled().map_err(|e| e.to_string())
}

/// Replaces the whole interface section (thinking display, send key,
/// notifications, hotkey, density) in one call.
#[tauri::command]
pub fn set_interface_settings(
    state: State<'_, AppState>,
    interface: loom_core::config::InterfaceConfig,
) -> Result<AppConfig, String> {
    state.mutate(move |config| config.interface = interface)
}

#[tauri::command]
pub fn set_chat_settings(
    state: State<'_, AppState>,
    permission_mode: Option<loom_core::config::PermissionMode>,
    agent_mode: Option<loom_core::config::AgentMode>,
    max_output_tokens: Option<u32>,
    auto_title: Option<bool>,
    max_tool_rounds: Option<u32>,
    computer_variant: Option<serde_json::Value>,
    computer_model: Option<serde_json::Value>,
    computer_screenshot_edge: Option<u32>,
) -> Result<AppConfig, String> {
    // Atelier is deliberately per chat: it hands the model write access to
    // Loom itself, so it is never something a chat inherits by default.
    if permission_mode == Some(loom_core::config::PermissionMode::Atelier) {
        return Err(
            "Atelier is per chat: switch it from the permission chip in the composer, not the \
             global default."
                .into(),
        );
    }
    state.mutate(move |config| {
        if let Some(auto) = auto_title {
            config.chat.auto_title = auto;
        }
        if let Some(mode) = permission_mode {
            config.chat.permission_mode = mode;
        }
        if let Some(mode) = agent_mode {
            config.chat.agent_mode = mode;
        }
        if let Some(max) = max_output_tokens {
            // Zero means "the model's own limit".
            config.chat.max_output_tokens = max.min(200_000);
        }
        if let Some(rounds) = max_tool_rounds {
            config.chat.max_tool_rounds = rounds.clamp(1, 200);
        }
        if let Some(variant) = computer_variant {
            config.chat.computer_variant = match variant {
                serde_json::Value::Null => None,
                other => other
                    .as_str()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
            };
        }
        if let Some(model) = computer_model {
            config.chat.computer_model = match model {
                serde_json::Value::Null => None,
                other => serde_json::from_value::<loom_core::config::ModelRef>(other).ok(),
            };
        }
        if let Some(edge) = computer_screenshot_edge {
            config.chat.computer_screenshot_edge = edge.min(4096);
        }
    })
}

#[tauri::command]
pub fn set_provider_key(id: String, key: String) -> Result<(), String> {
    secrets::set_api_key(&id, key.trim()).map_err(to_string)
}

#[tauri::command]
pub fn clear_provider_key(id: String) -> Result<(), String> {
    secrets::delete_api_key(&id).map_err(to_string)
}

#[tauri::command]
pub fn provider_key_status(id: String) -> bool {
    secrets::has_api_key(&id)
}

#[tauri::command]
pub async fn refresh_provider_models(
    state: State<'_, AppState>,
    id: String,
) -> Result<AppConfig, String> {
    state.engine.refresh_models(&id).await.map_err(to_string)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub fn set_provider_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        if let Some(provider) = config.providers.get_mut(&id) {
            provider.enabled = enabled;
        }
    })
}

/// Flat list for the model picker: every enabled provider's models with their
/// metadata and whether a key is present.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub provider_id: String,
    pub provider_name: String,
    pub kind: loom_core::provider::ProviderKind,
    /// Ready to use: the provider is on *and* the model is selected. Existing
    /// consumers only ever ask this, so folding selection in here is what makes
    /// an unselected model disappear from every picker at once.
    pub enabled: bool,
    /// The provider's own switch, independent of selection — settings needs to
    /// tell "provider off" apart from "model unselected".
    pub provider_enabled: bool,
    pub selected: bool,
    pub key_ready: bool,
    pub key_required: bool,
    pub model_id: String,
    pub spec: ModelSpec,
}

#[tauri::command]
pub fn list_models(state: State<'_, AppState>) -> Vec<ModelEntry> {
    let config = state.snapshot();
    let mut entries = Vec::new();
    for (provider_id, provider) in &config.providers {
        let key_ready = !provider.key_required || secrets::has_api_key(provider_id);
        for (model_id, spec) in &provider.models {
            // `enabled` is the single field every consumer already filters on —
            // the picker, the image-model list, the badges. Folding selection
            // into it is what makes unselecting a model take effect everywhere
            // at once, without teaching four call sites about a second flag.
            let selected = provider.model_selected(model_id);
            entries.push(ModelEntry {
                provider_id: provider_id.clone(),
                provider_name: provider.name.clone(),
                kind: provider.kind,
                enabled: provider.enabled && selected,
                provider_enabled: provider.enabled,
                selected,
                key_ready,
                key_required: provider.key_required,
                model_id: model_id.clone(),
                spec: spec.clone(),
            });
        }
    }
    entries
}

#[tauri::command]
pub fn set_default_model(
    state: State<'_, AppState>,
    provider_id: Option<String>,
    model_id: Option<String>,
    variant: Option<String>,
    lite_provider_id: Option<String>,
    lite_model_id: Option<String>,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.chat.provider_id = provider_id;
        config.chat.model_id = model_id;
        config.chat.variant = variant;
        config.chat.lite = lite_provider_id
            .zip(lite_model_id)
            .map(|(provider_id, model_id)| loom_core::config::ModelRef::new(provider_id, model_id));
    })
}

// ---------------------------------------------------------------------------
// Personas
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn upsert_prompt(
    state: State<'_, AppState>,
    prompt: loom_core::config::Prompt,
) -> Result<AppConfig, String> {
    let args = serde_json::to_value(&prompt).map_err(|e| e.to_string())?;
    state.harness_mutate(loom_core::harness::UPSERT_PROMPT, args)
}

#[tauri::command]
pub fn delete_prompt(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state.harness_mutate(
        loom_core::harness::DELETE_PROMPT,
        serde_json::json!({ "id": id }),
    )
}

#[tauri::command]
pub fn upsert_persona(state: State<'_, AppState>, persona: Persona) -> Result<AppConfig, String> {
    let args = serde_json::to_value(&persona).map_err(|e| e.to_string())?;
    state.harness_mutate(loom_core::harness::UPSERT_PERSONA, args)
}

#[tauri::command]
pub fn delete_persona(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state.harness_mutate(
        loom_core::harness::DELETE_PERSONA,
        serde_json::json!({ "id": id }),
    )
}

#[tauri::command]
pub fn set_user_profile(
    state: State<'_, AppState>,
    profile: loom_core::config::UserProfile,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.user_profile = profile;
    })
}

#[tauri::command]
pub fn upsert_persona_group(
    state: State<'_, AppState>,
    group: loom_core::config::PersonaGroup,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        let mut group = group;
        if group.id.trim().is_empty() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            group.id = format!("group-{stamp}");
        }
        // Drop members whose persona no longer exists.
        group
            .members
            .retain(|id| config.personas.iter().any(|persona| &persona.id == id));
        match config
            .persona_groups
            .iter_mut()
            .find(|existing| existing.id == group.id)
        {
            Some(existing) => *existing = group,
            None => config.persona_groups.push(group),
        }
    })
}

#[tauri::command]
pub fn delete_persona_group(
    state: State<'_, AppState>,
    id: String,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.persona_groups.retain(|group| group.id != id);
    })
}

// ---------------------------------------------------------------------------
// Persona memory and casts
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn persona_memory(
    state: State<'_, AppState>,
    persona_id: String,
) -> Result<Vec<loom_core::db::MemoryEntry>, String> {
    state
        .engine
        .persona_memory(&persona_id)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_persona_memory(
    state: State<'_, AppState>,
    persona_id: String,
    key: String,
    value: String,
    source: Option<String>,
) -> Result<Vec<loom_core::db::MemoryEntry>, String> {
    state
        .engine
        .set_persona_memory(
            &persona_id,
            key.trim(),
            value.trim(),
            source.as_deref().unwrap_or("user"),
        )
        .map_err(to_string)?;
    state.engine.persona_memory(&persona_id).map_err(to_string)
}

#[tauri::command]
pub fn delete_persona_memory(
    state: State<'_, AppState>,
    persona_id: String,
    id: String,
) -> Result<Vec<loom_core::db::MemoryEntry>, String> {
    state.engine.delete_persona_memory(&id).map_err(to_string)?;
    state.engine.persona_memory(&persona_id).map_err(to_string)
}

#[tauri::command]
pub fn clear_persona_memory(
    state: State<'_, AppState>,
    persona_id: String,
) -> Result<Vec<loom_core::db::MemoryEntry>, String> {
    state
        .engine
        .clear_persona_memory(&persona_id)
        .map_err(to_string)?;
    state.engine.persona_memory(&persona_id).map_err(to_string)
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_session(
    state: State<'_, AppState>,
    title: Option<String>,
    persona_id: Option<String>,
    workdir: Option<String>,
) -> Result<Session, String> {
    state
        .engine
        .create_session(title, None, None, persona_id, None, workdir)
        .map_err(to_string)
}

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    state.engine.list_sessions().map_err(to_string)
}

#[tauri::command]
pub fn delete_session(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.delete_session(&id).map_err(to_string)
}

/// Drops chats that were created but never used, so the list does not fill
/// with empty "New chat" rows. The open chat is kept.
#[tauri::command]
pub fn prune_empty_sessions(
    state: State<'_, AppState>,
    keep: Option<String>,
) -> Result<usize, String> {
    state
        .engine
        .prune_empty_sessions(keep.as_deref())
        .map_err(to_string)
}

#[tauri::command]
pub fn rename_session(state: State<'_, AppState>, id: String, title: String) -> Result<(), String> {
    state.engine.rename_session(&id, &title).map_err(to_string)
}

#[tauri::command]
pub fn reorder_sessions(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<(), String> {
    state.engine.reorder_sessions(&ids).map_err(to_string)
}

/// Every file in a workspace folder, for the composer's `@` picker.
///
/// Paths only, and no state kept: the popup asks when it opens, and skipping
/// build and dependency folders is what keeps the answer small enough to hold.
#[tauri::command]
pub fn list_workspace_files(workdir: Option<String>, limit: Option<usize>) -> Vec<String> {
    let limit = limit.unwrap_or(4_000).clamp(1, 20_000);
    match workdir {
        Some(path) => {
            loom_core::fsutil::walk_files(std::path::Path::new(&path), limit)
        }
        None => Vec::new(),
    }
}

#[tauri::command]
pub fn session_messages(state: State<'_, AppState>, id: String) -> Result<Vec<Message>, String> {
    state.engine.messages(&id).map_err(to_string)
}

#[tauri::command]
pub fn delete_message(state: State<'_, AppState>, message_id: String) -> Result<(), String> {
    state.engine.delete_message(&message_id).map_err(to_string)
}

/// Writes a chat to `path` as markdown.
#[tauri::command]
pub fn export_session(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let markdown = state
        .engine
        .export_session(&session_id)
        .map_err(to_string)?;
    std::fs::write(&path, markdown).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_model_favorite(
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
    favorite: bool,
) -> Result<AppConfig, String> {
    state
        .engine
        .set_model_favorite(&provider_id, &model_id, favorite)
        .map_err(to_string)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub fn set_session_model(
    state: State<'_, AppState>,
    id: String,
    provider_id: String,
    model_id: String,
    variant: Option<String>,
) -> Result<(), String> {
    state
        .engine
        .set_session_model(&id, &provider_id, &model_id, variant)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_variant(
    state: State<'_, AppState>,
    id: String,
    variant: Option<String>,
) -> Result<(), String> {
    state
        .engine
        .set_session_variant(&id, variant)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_workdir(
    state: State<'_, AppState>,
    id: String,
    workdir: Option<String>,
) -> Result<(), String> {
    state
        .engine
        .set_session_workdir(&id, workdir)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_permission_mode(
    state: State<'_, AppState>,
    id: String,
    mode: Option<loom_core::config::PermissionMode>,
) -> Result<(), String> {
    state
        .engine
        .set_session_permission_mode(&id, mode)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_agent_mode(
    state: State<'_, AppState>,
    id: String,
    mode: Option<loom_core::config::AgentMode>,
) -> Result<(), String> {
    state
        .engine
        .set_session_agent_mode(&id, mode)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_computer_access(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .engine
        .set_session_computer_access(&id, enabled)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_goal(
    state: State<'_, AppState>,
    id: String,
    goal: Option<String>,
) -> Result<(), String> {
    state
        .engine
        .set_session_goal(&id, goal.as_deref())
        .map_err(to_string)
}

#[tauri::command]
pub fn session_goal(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    state.engine.session_goal(&id).map_err(to_string)
}

/// The chat's condensed view of its older turns, for the transcript's
/// expander. Fetched on demand rather than copied onto every reply, because the
/// summary is identical for every turn between two folds.
#[tauri::command]
pub fn session_summary(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<loom_core::db::SessionSummary>, String> {
    state.engine.session_summary(&id).map_err(to_string)
}

#[tauri::command]
pub fn session_todos(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<loom_core::db::Todo>, String> {
    state.engine.session_todos(&id).map_err(to_string)
}

#[tauri::command]
pub fn set_todos(
    state: State<'_, AppState>,
    id: String,
    todos: Vec<loom_core::db::Todo>,
) -> Result<Vec<loom_core::db::Todo>, String> {
    state
        .engine
        .replace_todos(&id, todos)
        .map_err(to_string)?;
    state.engine.session_todos(&id).map_err(to_string)
}

/// The pill's Stop, the chip's Stop, and the panic hotkey.
///
/// Ends the chat that is driving the machine — and only that one. It used to
/// call `cancel_all`, so pressing Stop on a pill that says "Loom is controlling
/// your computer" also cancelled every other chat's turn and every detached
/// background run.
#[tauri::command]
pub fn stop_computer(app: AppHandle, state: State<'_, AppState>) {
    let stopped = state.engine.stop_computer();
    crate::hide_computer_pill(&app);
    let _ = app.emit("loom://computer-stopped", stopped);
}

/// The pill's Resume (and the chip's): the paused turn carries on, after
/// being told everything it saw is stale.
#[tauri::command]
pub fn resume_computer(state: State<'_, AppState>) -> bool {
    state.engine.resume_computer()
}

/// Debug-only: trips the takeover latch exactly as a real key press does.
///
/// The resume path cannot be tested otherwise — proving that a Resume sticks
/// needs a takeover that no human had to perform. Compiled out of release
/// builds entirely, and the probe script is the only caller.
#[tauri::command]
pub fn debug_trip_computer_takeover() -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        loom_core::computer::trip_takeover_for_debug();
        Ok(())
    }
    #[cfg(not(debug_assertions))]
    {
        Err("takeover simulation is available only in debug builds".to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerStatus {
    /// `hidden`, `active`, or `paused`.
    pub state: String,
    pub session_id: Option<String>,
    pub idle_seconds: u64,
    pub paused_seconds: u64,
    /// Seconds of silence that will auto-resume a paused turn. The engine's own
    /// constant, so the countdown the pill shows is the one that is in force.
    pub resume_in_seconds: u64,
    /// False when the input hooks could not be installed, so the user is not
    /// left believing Loom will stop the moment they touch the mouse.
    pub takeover_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub takeover_error: Option<String>,
}

/// Polled by the pill so its paused state can show the auto-resume countdown.
#[tauri::command]
pub fn computer_status(state: State<'_, AppState>) -> ComputerStatus {
    let holder = state.engine.computer_holder();
    let (paused, idle_ms, paused_ms) = state.engine.computer_status();
    let (takeover_active, takeover_error) = state.engine.takeover_health();
    let label = if paused {
        "paused"
    } else if holder.is_some() {
        "active"
    } else {
        "hidden"
    };
    ComputerStatus {
        state: label.to_string(),
        session_id: holder,
        idle_seconds: idle_ms / 1000,
        paused_seconds: paused_ms / 1000,
        resume_in_seconds: loom_core::engine::PAUSE_IDLE_RESUME_SECONDS,
        takeover_active,
        takeover_error,
    }
}

#[tauri::command]
pub fn respond_tool_permission(
    state: State<'_, AppState>,
    call_id: String,
    allow: bool,
    remember: Option<String>,
) -> Result<(), String> {
    state.engine.respond_permission(&call_id, allow);

    // "remember" persists a broader mode so the question stops coming back.
    if let Some(scope) = remember {
        if allow {
            state.mutate(move |config| {
                config.chat.permission_mode = match scope.as_str() {
                    "read-only" => loom_core::config::PermissionMode::AutoReadOnly,
                    _ => loom_core::config::PermissionMode::AutoAll,
                };
            })?;
        }
    }

    Ok(())
}

#[tauri::command]
pub fn respond_question(
    state: State<'_, AppState>,
    call_id: String,
    answer: loom_core::tools::Answer,
) -> Result<(), String> {
    state.engine.respond_question(&call_id, answer);
    Ok(())
}

#[tauri::command]
pub fn list_tools() -> Vec<loom_core::tools::ToolSpec> {
    // Harness tools are included so Settings → Tools documents them and the
    // permission card can look up their scope. They are only ever *offered*
    // to the model in Atelier; the engine's gate enforces that.
    let mut tools = loom_core::tools::specs();
    tools.extend(loom_core::harness::specs());
    // Memory and handoff tools are offered contextually; this is their
    // documentation entry.
    tools.extend(loom_core::tools::persona_tool_specs());
    // Computer tools document themselves in Settings → Tools; the engine only
    // offers them to a chat whose Computer chip is on.
    tools.extend(loom_core::computer::specs());
    tools
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub workdir: Option<String>,
    pub branch: Option<String>,
    pub is_repo: bool,
}

#[tauri::command]
pub fn workspace_info(workdir: Option<String>) -> WorkspaceInfo {
    match workdir {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            let is_repo = loom_core::workspace::repo_root(&path).is_some();
            WorkspaceInfo {
                workdir: Some(path.to_string_lossy().into_owned()),
                branch: loom_core::workspace::git_branch(&path),
                is_repo,
            }
        }
        None => WorkspaceInfo {
            workdir: None,
            branch: None,
            is_repo: false,
        },
    }
}

#[tauri::command]
pub fn set_session_persona(
    state: State<'_, AppState>,
    id: String,
    persona_id: Option<String>,
    system_prompt: Option<String>,
) -> Result<(), String> {
    state
        .engine
        .set_session_persona(&id, persona_id, system_prompt)
        .map_err(to_string)
}

#[tauri::command]
pub fn set_session_cast(
    state: State<'_, AppState>,
    id: String,
    persona_ids: Vec<String>,
) -> Result<(), String> {
    state
        .engine
        .set_session_cast(&id, persona_ids)
        .map_err(to_string)
}

#[tauri::command]
pub fn session_cast(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<Persona>, String> {
    state.engine.session_cast(&id).map_err(to_string)
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

/// Async on purpose: Tauri runs async commands on its Tokio runtime, which
/// `Engine::send` needs in order to spawn the streaming task. A synchronous
/// command runs on the event-loop thread, where `tokio::spawn` panics and takes
/// the process down.
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    provider_id: Option<String>,
    model_id: Option<String>,
    attachments: Option<Vec<loom_core::attachments::Attachment>>,
    persona_id: Option<String>,
) -> Result<String, String> {
    let model = provider_id
        .zip(model_id)
        .map(|(provider_id, model_id)| loom_core::config::ModelRef::new(provider_id, model_id));
    state
        .engine
        .send_as(
            &session_id,
            &text,
            model,
            attachments.unwrap_or_default(),
            persona_id,
        )
        .map_err(to_string)
}

// ---------------------------------------------------------------------------
// Workspaces
// ---------------------------------------------------------------------------

/// The name a workspace gets before the user renames it.
fn folder_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Adds a folder to the saved list; adding one that is already there just
/// refreshes it. Returns the updated config so the UI can apply it.
#[tauri::command]
pub fn add_workspace(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> Result<AppConfig, String> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err("workspace path must not be empty".into());
    }
    state.mutate(move |config| {
        let preferred = name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        match config
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.path == path)
        {
            // Already saved: keep the name the user may have given it.
            Some(_) => {}
            None => config.workspaces.push(loom_core::config::Workspace {
                name: preferred.unwrap_or_else(|| folder_name(&path)),
                path,
                added_at: loom_core::db::now_ms(),
            }),
        }
    })
}

#[tauri::command]
pub fn rename_workspace(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> Result<AppConfig, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("workspace name must not be empty".into());
    }
    state.mutate(move |config| {
        if let Some(workspace) = config
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.path == path)
        {
            workspace.name = name;
        }
    })
}

/// Forgets a workspace. Chats that used it keep their folder and still group
/// together; they just fall back to the folder's name.
#[tauri::command]
pub fn remove_workspace(state: State<'_, AppState>, path: String) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.workspaces.retain(|workspace| workspace.path != path);
    })
}

// ---------------------------------------------------------------------------
// Web search
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_search_provider(
    state: State<'_, AppState>,
    provider: loom_core::config::SearchProvider,
) -> Result<AppConfig, String> {
    state.mutate(move |config| config.search_provider = provider)
}

/// Stores the Jina API key; an empty string clears it.
#[tauri::command]
pub fn set_search_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        secrets::delete_named_secret(secrets::JINA_KEY).map_err(to_string)
    } else {
        secrets::set_named_secret(secrets::JINA_KEY, key).map_err(to_string)
    }
}

#[tauri::command]
pub fn search_key_status() -> bool {
    secrets::has_named_secret(secrets::JINA_KEY)
}

// ---------------------------------------------------------------------------
// Workspace index
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn index_workspace(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<usize, String> {
    state
        .engine
        .index_workspace(&session_id)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub fn workspace_index_status(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<usize, String> {
    state.engine.index_status(&session_id).map_err(to_string)
}

#[tauri::command]
pub fn clear_workspace_index(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state.engine.clear_index(&session_id).map_err(to_string)
}

#[tauri::command]
pub fn set_embedding_model(
    state: State<'_, AppState>,
    model: Option<loom_core::config::AuxModelRef>,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        // An empty model id clears the setting; a bare `{providerId: "",
        // modelId}` is what an unqualified ref deserializes to.
        config.chat.embedding_model = model.filter(|value| !value.model_id.trim().is_empty());
    })
}

#[tauri::command]
pub fn add_model(
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
) -> Result<AppConfig, String> {
    state
        .engine
        .add_model(&provider_id, &model_id)
        .map_err(to_string)?;
    Ok(state.snapshot())
}

#[tauri::command]
pub fn set_model_spec(
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
    context: Option<u32>,
    output: Option<u32>,
    input_modalities: Option<Vec<Modality>>,
    reasoning: Option<ReasoningSpec>,
) -> Result<AppConfig, String> {
    state
        .engine
        .set_model_spec(
            &provider_id,
            &model_id,
            context,
            output,
            input_modalities.unwrap_or_default(),
            reasoning,
        )
        .map_err(to_string)?;
    Ok(state.snapshot())
}

/// Forgets detected metadata and re-reads the bundled catalog, keeping the
/// favourite flag. The UI offers this when a kept old guess looks wrong.
#[tauri::command]
pub fn reset_model_spec(
    state: State<'_, AppState>,
    provider_id: String,
    model_id: String,
) -> Result<AppConfig, String> {
    state
        .engine
        .reset_model_spec(&provider_id, &model_id)
        .map_err(to_string)?;
    Ok(state.snapshot())
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

/// Providers whose vendor exposes a usage/quota endpoint Loom can read.
#[tauri::command]
pub fn usage_capable_providers(
    state: State<'_, AppState>,
) -> Vec<loom_core::engine::UsageCapableProvider> {
    state.engine.usage_capable_providers()
}

/// Live limits/balance straight from the vendor (OpenCode Go windows,
/// OpenRouter credits, DeepSeek balance, Z.ai quota).
#[tauri::command]
pub async fn provider_usage(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<loom_core::usage::ProviderUsage, String> {
    state
        .engine
        .provider_usage(&provider_id)
        .await
        .map_err(to_string)
}

/// Token/cost totals per provider. The scan walks every stored reply, so it
/// runs on the blocking pool rather than the IPC thread.
#[tauri::command]
pub async fn usage_summary(
    state: State<'_, AppState>,
) -> Result<loom_core::engine::UsageSummary, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.usage_summary().map_err(to_string))
        .await
        .map_err(|error| error.to_string())?
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub data_dir: String,
    pub database: u64,
    pub attachments: u64,
    pub generated: u64,
    pub backgrounds: u64,
    pub cache: u64,
    pub total: u64,
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(reader) = std::fs::read_dir(path) else {
        return 0;
    };
    reader
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

#[tauri::command]
pub fn storage_usage() -> Result<StorageUsage, String> {
    let home = loom_core::paths::loom_home().map_err(to_string)?;
    let database = std::fs::metadata(home.join("loom.db"))
        .map(|meta| meta.len())
        .unwrap_or(0);
    let attachments = dir_size(&home.join("attachments"));
    let generated = dir_size(&home.join("generated"));
    let backgrounds = dir_size(&home.join("backgrounds"));
    let cache = dir_size(&home.join("cache"));

    Ok(StorageUsage {
        data_dir: home.to_string_lossy().into_owned(),
        database,
        attachments,
        generated,
        backgrounds,
        cache,
        total: database + attachments + generated + backgrounds + cache,
    })
}

/// Removes downloaded update payloads; nothing else is touched.
#[tauri::command]
pub fn clear_cache() -> Result<u64, String> {
    let directory = loom_core::paths::cache_dir().map_err(to_string)?;
    let freed = dir_size(&directory);
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(freed)
}

/// Removes generated images (they stay viewable in the chat as references).
#[tauri::command]
pub fn clear_generated() -> Result<u64, String> {
    let directory = loom_core::paths::loom_home()
        .map_err(to_string)?
        .join("generated");
    let freed = dir_size(&directory);
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|e| e.to_string())?;
    }
    Ok(freed)
}

// ---------------------------------------------------------------------------
// Skills, updates
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_skills() -> Result<Vec<loom_core::skills::Skill>, String> {
    loom_core::skills::list().map_err(to_string)
}

/// Writes `~/.loom/skills/<id>.md` with the same validation the model-driven
/// `write_skill` tool uses; an overwrite backs the old file up first.
#[tauri::command]
pub fn save_skill(
    id: String,
    name: String,
    description: String,
    body: String,
) -> Result<(), String> {
    loom_core::skills::write(&id, &name, &description, &body)
        .map(|_| ())
        .map_err(to_string)
}

#[tauri::command]
pub fn delete_skill(id: String) -> Result<(), String> {
    loom_core::skills::delete(&id).map_err(to_string)
}

#[tauri::command]
pub fn read_skill(id: String) -> Result<loom_core::skills::Skill, String> {
    loom_core::skills::read(&id).map_err(to_string)
}

#[tauri::command]
pub fn set_image_model(
    state: State<'_, AppState>,
    model: Option<loom_core::config::AuxModelRef>,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.chat.image_model = model.filter(|value| !value.model_id.trim().is_empty());
    })
}

pub(crate) const UPDATE_MANIFEST_URL: &str =
    "https://github.com/aaen-studios/loom/releases/latest/download/update.json";

#[tauri::command]
pub async fn check_for_updates(
    state: State<'_, AppState>,
) -> Result<loom_core::updater::UpdateCheck, String> {
    loom_core::updater::check(
        &state.engine.http_client(),
        env!("CARGO_PKG_VERSION"),
        UPDATE_MANIFEST_URL,
    )
    .await
    .map_err(to_string)
}

/// Downloads + verifies the update and stages it next to the install folder.
#[tauri::command]
pub async fn download_update(
    state: State<'_, AppState>,
    manifest: loom_core::updater::UpdateManifest,
) -> Result<String, String> {
    let archive = loom_core::updater::download(&state.engine.http_client(), &manifest)
        .await
        .map_err(to_string)?;

    let install_dir = install_dir()?;
    let staging = loom_core::updater::stage_zip(&archive, &install_dir).map_err(to_string)?;
    Ok(staging.to_string_lossy().into_owned())
}

/// Writes the swap script, launches it, and exits so files can be replaced.
#[tauri::command]
pub fn apply_update(app: AppHandle, staging: String) -> Result<(), String> {
    let install_dir = install_dir()?;
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "loom.exe".to_string());

    let script = loom_core::updater::apply_after_exit(
        std::path::Path::new(&staging),
        &install_dir,
        &exe_name,
    )
    .map_err(to_string)?;

    // Hidden: the swap runs while the app is exiting, and a console window
    // appearing at that moment reads like a crash.
    loom_core::process::hidden_std("cmd")
        .arg("/C")
        .arg(script)
        .spawn()
        .map_err(|e| e.to_string())?;

    app.exit(0);
    Ok(())
}

fn install_dir() -> Result<std::path::PathBuf, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(|path| path.to_path_buf())
        .ok_or_else(|| "could not resolve the install folder".to_string())
}

#[tauri::command]
pub fn upsert_mcp_server(
    state: State<'_, AppState>,
    id: String,
    server: loom_core::mcp::McpServerConfig,
) -> Result<AppConfig, String> {
    state.harness_mutate(
        loom_core::harness::UPSERT_MCP_SERVER,
        serde_json::json!({
            "id": id,
            "name": server.name,
            "command": server.command,
            "args": server.args,
            "env": server.env,
            "enabled": server.enabled,
        }),
    )
}

#[tauri::command]
pub fn delete_mcp_server(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state.harness_mutate(
        loom_core::harness::DELETE_MCP_SERVER,
        serde_json::json!({ "id": id }),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolView {
    pub server: String,
    pub name: String,
    pub description: String,
    /// Prefixed name the model sees.
    pub model_name: String,
}

/// Connects enabled MCP servers and reports the tools they expose.
#[tauri::command]
pub async fn mcp_tools(state: State<'_, AppState>) -> Result<Vec<McpToolView>, String> {
    // Force a reconnect so the UI always reflects the current config.
    state.engine.invalidate_mcp();
    let tools = state.engine.mcp_tool_defs().await;
    Ok(tools
        .into_iter()
        .map(|tool| {
            let (server, name) = loom_core::mcp::parse_tool_name(&tool.name)
                .unwrap_or_else(|| ("unknown".to_string(), tool.name.clone()));
            McpToolView {
                server,
                name,
                description: tool.description,
                model_name: tool.name,
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Attachments & backgrounds
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn attach_files(
    session_id: String,
    paths: Vec<String>,
) -> Result<Vec<loom_core::attachments::Attachment>, String> {
    paths
        .iter()
        .map(|path| {
            loom_core::attachments::store(&session_id, std::path::Path::new(path))
                .map_err(to_string)
        })
        .collect()
}

#[tauri::command]
pub fn attach_bytes(
    session_id: String,
    name: String,
    data: String,
) -> Result<loom_core::attachments::Attachment, String> {
    loom_core::attachments::store_base64(&session_id, &name, &data).map_err(to_string)
}

/// Physical centre of the quick-ask overlay: the monitor the user is looking
/// at, and therefore the one worth capturing.
fn overlay_center(app: &AppHandle) -> Option<(i32, i32)> {
    let window = app.get_webview_window("ask")?;
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some((
        position.x + (size.width / 2) as i32,
        position.y + (size.height / 2) as i32,
    ))
}

/// The frame taken when the overlay opened, held until the first message
/// claims it (or the next summon overwrites it).
struct Stash {
    shot: Option<loom_core::screen::Shot>,
    busy: bool,
    generation: u64,
}

static STASH: Mutex<Stash> = Mutex::new(Stash {
    shot: None,
    busy: false,
    generation: 0,
});

fn capture_on_send(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|state| state.snapshot().interface.capture_on_send)
        .unwrap_or(false)
}

/// Grabs the screen as the overlay opens, before anything is typed. Runs on
/// its own thread: showing the window should not wait for a capture.
pub(crate) fn stash_screen(app: &AppHandle) {
    if !capture_on_send(app) {
        return;
    }
    let Some((x, y)) = overlay_center(app) else {
        return;
    };

    let generation = {
        let Ok(mut stash) = STASH.lock() else {
            return;
        };
        stash.generation += 1;
        stash.busy = true;
        stash.generation
    };

    std::thread::spawn(move || {
        let shot = loom_core::screen::capture_at(x, y).ok();
        if let Ok(mut stash) = STASH.lock() {
            // A newer summon wins; a slower, older capture must not clobber it.
            if stash.generation == generation {
                stash.shot = shot;
                stash.busy = false;
            }
        }
    });
}

/// Stores the frame stashed when the overlay opened, for the first message of
/// that summon. `None` when there is nothing waiting — the caller falls back
/// to a capture at send time.
#[tauri::command]
pub async fn claim_screen(
    session_id: String,
) -> Result<Option<loom_core::attachments::Attachment>, String> {
    let mut shot = None;
    // The user may type faster than the capture; wait briefly for it. The
    // capture itself may take half a second on a fresh duplication session.
    for _ in 0..60 {
        {
            let Ok(mut stash) = STASH.lock() else {
                return Err("screen stash is unavailable".to_string());
            };
            if !stash.busy {
                shot = stash.shot.take();
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let Some(shot) = shot else {
        return Ok(None);
    };
    let mut attachment = loom_core::attachments::store_bytes(&session_id, &shot.name, &shot.bytes)
        .map_err(to_string)?;
    attachment.hidden = true;
    Ok(Some(attachment))
}

/// True when the overlay is hidden from screen capture, so the shot can be
/// taken with it still on screen.
#[cfg(windows)]
fn overlay_is_excluded(window: &tauri::WebviewWindow) -> bool {
    window
        .hwnd()
        .ok()
        .map(|hwnd| loom_core::screen::exclude_from_capture(hwnd.0 as isize))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn overlay_is_excluded(_window: &tauri::WebviewWindow) -> bool {
    false
}

async fn capture_monitor(point: Option<(i32, i32)>) -> Result<loom_core::screen::Shot, Error> {
    tauri::async_runtime::spawn_blocking(move || match point {
        Some((x, y)) => loom_core::screen::capture_at(x, y),
        None => Err(Error::Other("overlay window has no position".into())),
    })
    .await
    .map_err(|error| Error::Other(error.to_string()))?
}

/// Screenshots the monitor under the overlay and stores it as a hidden
/// attachment: the model gets the image, the transcript shows only a tag.
///
/// Windows keeps the overlay out of captures (`WDA_EXCLUDEFROMCAPTURE`), so
/// the common path never hides the window — the shot simply shows the desktop
/// underneath. Builds without that support, and other platforms, fall back to
/// hiding for the moment it takes to grab a frame.
#[tauri::command]
pub async fn capture_screen(
    app: AppHandle,
    session_id: String,
) -> Result<loom_core::attachments::Attachment, String> {
    let point = overlay_center(&app);
    let window = app.get_webview_window("ask");
    let excluded = window.as_ref().is_some_and(overlay_is_excluded);

    let mut hidden = false;
    if window.is_some() && !excluded {
        if let Some(window) = window.as_ref() {
            let _ = window.hide();
        }
        hidden = true;
        // The compositor needs a beat to drop the window before the capture.
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    }

    let mut captured = capture_monitor(point).await;

    // Duplication can be unavailable (some drivers and VMs). If the overlay is
    // still visible, hide it and try once more before giving up.
    if captured.is_err() && window.is_some() && !hidden {
        if let Some(window) = window.as_ref() {
            let _ = window.hide();
        }
        hidden = true;
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        captured = capture_monitor(point).await;
    }

    if hidden {
        if let Some(window) = window.as_ref() {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    let shot = captured.map_err(|error| error.to_string())?;

    let mut attachment = loom_core::attachments::store_bytes(&session_id, &shot.name, &shot.bytes)
        .map_err(to_string)?;
    attachment.hidden = true;
    Ok(attachment)
}

/// Copies a picked background image/video into `~/.loom/backgrounds` and points
/// the config at it.
#[tauri::command]
pub fn set_background_file(
    state: State<'_, AppState>,
    kind: loom_core::config::BackgroundKind,
    source: String,
) -> Result<AppConfig, String> {
    let source = std::path::PathBuf::from(source);
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| "background has no file name".to_string())?;

    let directory = loom_core::paths::backgrounds_dir().map_err(to_string)?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let target = directory.join(format!("{}-{name}", uuid::Uuid::new_v4()));
    std::fs::copy(&source, &target).map_err(|e| e.to_string())?;

    state.mutate(move |config| {
        config.background.kind = kind;
        config.background.path = Some(target.to_string_lossy().into_owned());
    })
}

#[tauri::command]
pub fn cancel_stream(state: State<'_, AppState>, session_id: String) {
    state.engine.cancel(&session_id);
}

#[tauri::command]
pub fn busy_sessions(state: State<'_, AppState>) -> Vec<String> {
    state.engine.busy_sessions()
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn hide_overlay(app: AppHandle) {
    if let Some(window) = app.get_webview_window("ask") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn show_main(app: AppHandle, session_id: Option<String>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    // The overlay can open a chat straight from its mini transcript.
    if let Some(session_id) = session_id {
        let _ = app.emit("loom://open-session", session_id);
    }
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    let _ = app.emit("loom://open-settings", ());
    show_main(app, None);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Used by the overlay to know where typed text should go.
#[tauri::command]
pub fn overlay_target(state: State<'_, AppState>) -> Option<Session> {
    state
        .engine
        .list_sessions()
        .ok()
        .and_then(|sessions| sessions.into_iter().next())
}

// ---------------------------------------------------------------------------
// Detached runs (background subagents, job firings)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>, job_id: Option<String>) -> Result<Vec<Task>, String> {
    state.engine.tasks(job_id.as_deref()).map_err(to_string)
}

// ---------------------------------------------------------------------------
// Shell commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_commands(
    state: State<'_, AppState>,
    session_id: Option<String>,
) -> Result<Vec<loom_core::db::CommandRun>, String> {
    state
        .engine
        .commands(session_id.as_deref())
        .map_err(to_string)
}

// All three of these block: `stop_command` and `delete_command` wait on
// `taskkill`/`kill`, and `command_output` reads up to 256 KiB of log. A
// synchronous Tauri command runs on the event-loop thread, so the window would
// stop painting and accepting input for the duration — which is exactly what a
// Stop press must not do. `spawn_blocking`, as the usage commands already do.

#[tauri::command]
pub async fn command_output(
    state: State<'_, AppState>,
    id: String,
    lines: Option<usize>,
) -> Result<String, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.command_output(&id, lines.unwrap_or(200)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(to_string)
}

#[tauri::command]
pub async fn stop_command(
    state: State<'_, AppState>,
    id: String,
) -> Result<loom_core::db::CommandRun, String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.stop_command(&id))
        .await
        .map_err(|error| error.to_string())?
        .map_err(to_string)
}

#[tauri::command]
pub async fn delete_command(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.delete_command(&id))
        .await
        .map_err(|error| error.to_string())?
        .map_err(to_string)
}

#[tauri::command]
pub fn cancel_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.cancel_task(&id).map_err(to_string)
}

#[tauri::command]
pub fn retry_task(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state.engine.retry_task(&id).map_err(to_string)
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.delete_task(&id).map_err(to_string)
}

/// Starts a run from the UI; the model reaches the same path through
/// `spawn_agent` with `background: true`.
#[tauri::command]
pub fn start_task(
    state: State<'_, AppState>,
    prompt: String,
    title: Option<String>,
    session_id: Option<String>,
    model: Option<loom_core::config::ModelRef>,
) -> Result<String, String> {
    let session = session_id
        .as_deref()
        .and_then(|id| state.engine.session(id).ok().flatten());
    let workdir = session.as_ref().and_then(|session| session.workdir.clone());
    state
        .engine
        .spawn_task(loom_core::engine::TaskRequest {
            title: title.unwrap_or_else(|| {
                prompt
                    .lines()
                    .next()
                    .unwrap_or("Task")
                    .chars()
                    .take(80)
                    .collect()
            }),
            prompt,
            origin_session: session_id,
            job_id: None,
            provider_id: model.as_ref().map(|model| model.provider_id.clone()),
            model_id: model.as_ref().map(|model| model.model_id.clone()),
            persona_id: session.as_ref().and_then(|session| session.persona_id.clone()),
            workdir,
            permission_mode: Some("auto-read-only".to_string()),
            notify: true,
            max_steps: None,
            max_cost_usd: None,
        })
        .map_err(to_string)
}

// ---------------------------------------------------------------------------
// Jobs (cron schedules)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_jobs(state: State<'_, AppState>) -> Result<Vec<Job>, String> {
    state.engine.jobs().map_err(to_string)
}

#[tauri::command]
pub fn upsert_job(state: State<'_, AppState>, job: Job) -> Result<Job, String> {
    state.engine.upsert_job(job).map_err(to_string)
}

#[tauri::command]
pub fn delete_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.delete_job(&id).map_err(to_string)
}

#[tauri::command]
pub fn run_job_now(state: State<'_, AppState>, id: String) -> Result<String, String> {
    state.engine.run_job_now(&id).map_err(to_string)
}

/// The next firings for a cron expression, for the job editor's preview.
#[tauri::command]
pub fn preview_schedule(cron: String, count: Option<usize>) -> Result<Vec<i64>, String> {
    loom_core::jobs::upcoming(&cron, count.unwrap_or(5))
}

// ---------------------------------------------------------------------------
// Long-term memory
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_memories(state: State<'_, AppState>, scope: Option<String>) -> Result<Vec<Memory>, String> {
    state.engine.memories(scope.as_deref()).map_err(to_string)
}

#[tauri::command]
pub async fn upsert_memory(
    state: State<'_, AppState>,
    id: Option<String>,
    scope: String,
    content: String,
    pinned: bool,
) -> Result<Memory, String> {
    state
        .engine
        .upsert_memory(id.as_deref(), &scope, &content, pinned)
        .await
        .map_err(to_string)
}

#[tauri::command]
pub fn delete_memory(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.delete_memory(&id).map_err(to_string)
}

#[tauri::command]
pub fn clear_memories(state: State<'_, AppState>, scope: String) -> Result<usize, String> {
    state.engine.clear_memories(&scope).map_err(to_string)
}
