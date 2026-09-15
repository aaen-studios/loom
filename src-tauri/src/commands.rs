//! Tauri command layer. Thin: every command delegates to `loom-core` and
//! returns camelCase JSON the frontend already has types for.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use loom_core::config::AppConfig;
use loom_core::db::{Message, Session};
use loom_core::engine::{Engine, SharedConfig};
use loom_core::persona::Persona;
use loom_core::provider::{ModelSpec, ProviderConfig, ProviderPreset, PRESETS};
use loom_core::{config, paths, secrets, Error};

pub struct AppState {
    pub engine: Engine,
    pub config: SharedConfig,
}

impl AppState {
    pub fn new(engine: Engine, config: SharedConfig) -> Self {
        Self { engine, config }
    }

    pub fn snapshot(&self) -> AppConfig {
        self.config.lock().expect("config mutex poisoned").clone()
    }

    fn mutate(&self, change: impl FnOnce(&mut AppConfig)) -> Result<AppConfig, String> {
        let snapshot = {
            let mut guard = self.config.lock().expect("config mutex poisoned");
            change(&mut guard);
            guard.clone()
        };
        config::save(&snapshot).map_err(|e| e.to_string())?;
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
    // The UI only owns appearance fields; providers, personas, and chat
    // defaults are mutated through their own commands and preserved here.
    state.mutate(|current| {
        let providers = std::mem::take(&mut current.providers);
        let personas = std::mem::take(&mut current.personas);
        let chat = current.chat.clone();
        *current = config.clone();
        current.providers = providers;
        current.personas = personas;
        current.chat = chat;
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
    })
}

/// Applies the configured hotkey immediately (register, re-register, or none).
#[tauri::command]
pub fn set_hotkey(app: AppHandle, enabled: bool, keys: String) -> Result<(), String> {
    crate::set_hotkey_now(&app, enabled, &keys)
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
    history_limit: Option<u32>,
    max_output_tokens: Option<u32>,
    auto_title: Option<bool>,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        if let Some(auto) = auto_title {
            config.chat.auto_title = auto;
        }
        if let Some(mode) = permission_mode {
            config.chat.permission_mode = mode;
        }
        if let Some(limit) = history_limit {
            config.chat.history_limit = limit.clamp(2, 500);
        }
        if let Some(max) = max_output_tokens {
            config.chat.max_output_tokens = max.clamp(256, 200_000);
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
    state
        .engine
        .refresh_models(&id)
        .await
        .map_err(to_string)?;
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
    pub enabled: bool,
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
            entries.push(ModelEntry {
                provider_id: provider_id.clone(),
                provider_name: provider.name.clone(),
                kind: provider.kind,
                enabled: provider.enabled,
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
    if prompt.title.trim().is_empty() {
        return Err("a prompt needs a title".into());
    }
    state.mutate(move |config| {
        let mut prompt = prompt;
        if prompt.id.trim().is_empty() {
            prompt.id = uuid::Uuid::new_v4().to_string();
        }
        match config.prompts.iter_mut().find(|entry| entry.id == prompt.id) {
            Some(existing) => *existing = prompt,
            None => config.prompts.push(prompt),
        }
    })
}

#[tauri::command]
pub fn delete_prompt(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state.mutate(move |config| config.prompts.retain(|entry| entry.id != id))
}

#[tauri::command]
pub fn upsert_persona(state: State<'_, AppState>, persona: Persona) -> Result<AppConfig, String> {
    state.mutate(move |config| match config.personas.iter_mut().find(|p| p.id == persona.id) {
        Some(existing) => *existing = persona,
        None => config.personas.push(persona),
    })
}

#[tauri::command]
pub fn delete_persona(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state.mutate(move |config| config.personas.retain(|p| p.id != id))
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_session(
    state: State<'_, AppState>,
    title: Option<String>,
    persona_id: Option<String>,
) -> Result<Session, String> {
    state
        .engine
        .create_session(title, None, None, persona_id, None)
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

#[tauri::command]
pub fn rename_session(state: State<'_, AppState>, id: String, title: String) -> Result<(), String> {
    state.engine.rename_session(&id, &title).map_err(to_string)
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
    let markdown = state.engine.export_session(&session_id).map_err(to_string)?;
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
pub fn list_tools() -> Vec<loom_core::tools::ToolSpec> {
    loom_core::tools::specs()
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
) -> Result<String, String> {
    let model = provider_id
        .zip(model_id)
        .map(|(provider_id, model_id)| loom_core::config::ModelRef::new(provider_id, model_id));
    state
        .engine
        .send(&session_id, &text, model, attachments.unwrap_or_default())
        .map_err(to_string)
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
pub fn clear_workspace_index(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    state.engine.clear_index(&session_id).map_err(to_string)
}

#[tauri::command]
pub fn set_embedding_model(
    state: State<'_, AppState>,
    model: Option<String>,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.chat.embedding_model = model.filter(|value| !value.trim().is_empty());
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
) -> Result<AppConfig, String> {
    state
        .engine
        .set_model_spec(&provider_id, &model_id, context, output)
        .map_err(to_string)?;
    Ok(state.snapshot())
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

#[tauri::command]
pub fn set_image_model(
    state: State<'_, AppState>,
    model: Option<String>,
) -> Result<AppConfig, String> {
    state.mutate(move |config| {
        config.chat.image_model = model.filter(|value| !value.trim().is_empty());
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

    std::process::Command::new("cmd")
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
    if id.trim().is_empty() {
        return Err("MCP server id must not be empty".into());
    }
    state.engine.invalidate_mcp();
    state.mutate(move |config| {
        config.mcp_servers.insert(id, server);
    })
}

#[tauri::command]
pub fn delete_mcp_server(state: State<'_, AppState>, id: String) -> Result<AppConfig, String> {
    state.engine.invalidate_mcp();
    state.mutate(move |config| {
        config.mcp_servers.remove(&id);
    })
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
pub fn show_main(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    let _ = app.emit("loom://open-settings", ());
    show_main(app);
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
