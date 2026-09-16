//! App configuration (`~/.loom/config.json`).
//!
//! Forward compatible by design: every field has a default, unknown fields are
//! preserved on rewrite, and `schemaVersion` gates future migrations. Provider
//! and persona configuration lives here; API keys do not (see `secrets`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fsutil::atomic_write;
use crate::persona::Persona;
use crate::provider::{MetadataSource, ModelSpec, ProviderConfig};
use crate::voice::config::VoiceConfig;
use crate::{paths, Error, Result};

pub const SCHEMA_VERSION: u32 = 1;

/// The provenance pass in [`migrate_metadata`]. Bump when its rules change.
pub const METADATA_VERSION: u32 = 1;

/// Configs saved before provenance existed report version 0, whatever the
/// container default says, so the one-time pass runs exactly once.
fn legacy_metadata_version() -> u32 {
    0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub schema_version: u32,
    /// Bumped when [`migrate_metadata`] needs to run once on an older config.
    #[serde(default = "legacy_metadata_version")]
    pub metadata_version: u32,
    pub theme: Theme,
    pub background: BackgroundConfig,
    pub sidebar_collapsed: bool,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub personas: Vec<Persona>,
    /// Personas grouped for organisation and for multi-persona casts.
    pub persona_groups: Vec<PersonaGroup>,
    /// Who the user is: feeds `{{user}}` and friends in persona prompts.
    pub user_profile: UserProfile,
    pub mcp_servers: BTreeMap<String, crate::mcp::McpServerConfig>,
    pub chat: ChatDefaults,
    pub interface: InterfaceConfig,
    /// Voice mode: whether Loom speaks, in which voice, and where the assets
    /// are. Present but silent until asked — see [`VoiceConfig::default`].
    pub voice: VoiceConfig,
    /// Reusable prompt snippets offered in the composer's slash menu.
    pub prompts: Vec<Prompt>,
    /// Folders the user added; chats point at one by path.
    pub workspaces: Vec<Workspace>,
    /// Which service backs the web tools.
    pub search_provider: SearchProvider,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            metadata_version: METADATA_VERSION,
            theme: Theme::Light,
            background: BackgroundConfig::default(),
            sidebar_collapsed: false,
            providers: BTreeMap::new(),
            personas: Vec::new(),
            persona_groups: Vec::new(),
            user_profile: UserProfile::default(),
            mcp_servers: BTreeMap::new(),
            chat: ChatDefaults::default(),
            interface: InterfaceConfig::default(),
            voice: VoiceConfig::default(),
            prompts: Vec::new(),
            workspaces: Vec::new(),
            search_provider: SearchProvider::default(),
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// The default. The built-in backgrounds are light, and a light surface
    /// flatters the app's dark ink and its glass panels.
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundKind {
    #[default]
    Builtin,
    Image,
    Video,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackgroundConfig {
    pub kind: BackgroundKind,
    /// Built-in preset id (see the UI's preset list). An id the UI no longer
    /// knows falls back to the first preset, so retiring one needs no
    /// migration here.
    pub preset: String,
    /// Absolute path for `Image`/`Video` kinds.
    pub path: Option<String>,
    /// 0..=100 black overlay strength, for taming artwork the user supplies.
    /// The built-in presets are authored to be legible already, so the default
    /// is 0. Dark mode applies a floor of its own regardless (see the UI's
    /// `DARK_DIM_FLOOR`), because near-white ink needs a bright background
    /// held down.
    pub dim: u8,
    /// 0..=64 px blur applied to the background layer.
    pub blur: u8,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            kind: BackgroundKind::Builtin,
            // Porcelain: a cool near-white, painted in CSS. Chosen as the
            // default because it is the quietest of the set, and because a
            // light surface flatters the app's dark ink and glass equally.
            preset: "porcelain".to_string(),
            path: None,
            dim: 0,
            blur: 0,
        }
    }
}

/// A `provider/model` pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub provider_id: String,
    pub model_id: String,
}

impl ModelRef {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode {
    /// Confirm every tool call.
    #[default]
    Ask,
    /// Read-only tools run silently; anything that writes or executes asks.
    AutoReadOnly,
    /// Everything runs without prompting.
    AutoAll,
    /// Everything Auto all runs, plus the harness tools that let the model
    /// edit Loom itself (personas, MCP servers, skills, prompts, providers,
    /// and settings). The five deletes still ask. Deliberately per chat only:
    /// it is never accepted as the global default.
    Atelier,
}

/// Whether the model plans, reviews, builds, or just chats. Read-only modes
/// refuse anything that can change the workspace; `Chat` is not read-only but
/// *narrow* — it is offered a fixed handful of tools and refuses the rest. The
/// permission mode keeps governing everything else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Research and propose; the mutating tools are refused.
    Plan,
    /// Inspect and report findings; the mutating tools are refused.
    Review,
    #[default]
    Build,
    /// Answer quickly from the model and the web; only the web pair, the
    /// clock and `ask_user` are offered. See `tools::is_allowed_in_chat`.
    Chat,
}

impl AgentMode {
    /// Read-only modes refuse anything that can change the workspace.
    ///
    /// `Chat` is deliberately *not* in here. It is a narrower mode, not a
    /// read-only one: reusing this flag would let the read-only halves of
    /// computer use and the workspace tools leak into a chat.
    pub fn blocks_writes(self) -> bool {
        matches!(self, AgentMode::Plan | AgentMode::Review)
    }

    /// Pure chat: a small fixed tool list, and nothing else.
    pub fn is_chat(self) -> bool {
        matches!(self, AgentMode::Chat)
    }

    /// Display name for prompts and refusals.
    pub fn label(self) -> &'static str {
        match self {
            AgentMode::Plan => "Plan",
            AgentMode::Review => "Review",
            AgentMode::Build => "Build",
            AgentMode::Chat => "Chat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatDefaults {
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub variant: Option<String>,
    /// Cheap model used for chat titles and small background jobs.
    pub lite: Option<ModelRef>,
    /// Model used by the `generate_image` tool (chat provider is used).
    pub image_model: Option<String>,
    /// Embedding model used by the workspace index.
    pub embedding_model: Option<String>,
    /// Generate a chat title with the lite model after the first reply.
    pub auto_title: bool,
    /// Models used recently, newest first (the picker lists them on top).
    pub recent_models: Vec<ModelRef>,
    /// Global default for the tool permission mode (per-chat override exists).
    pub permission_mode: PermissionMode,
    /// Global default for the agent mode (per-chat override exists).
    pub agent_mode: AgentMode,
    /// Cap on the reply length. Zero asks for the model's own limit; the
    /// engine still reserves room for it when fitting the history into the
    /// model's context window.
    pub max_output_tokens: u32,
    /// How much of the model's window a condensed block of older turns may
    /// occupy, as a percentage. `0` switches condensing off and restores the
    /// old behaviour of dropping older turns outright. Clamped 0–50 when read:
    /// above that the block starts crowding out the live turn it exists to
    /// protect.
    pub condense_share: u32,
    /// Tool round-trips a single user turn may take. Clamped 1–200; the
    /// engine tells the model to summarise when the budget runs out.
    pub max_tool_rounds: u32,
    /// Reasoning variant computer turns run with when the chat has no explicit
    /// variant: `"off"` for none, `"low"` for the cheapest effort, `None` to
    /// inherit the usual chain. Computer use is latency-bound, so the default
    /// deliberately skips deep thinking.
    pub computer_variant: Option<String>,
    /// Model used only while the Computer chip is armed, so slow main models
    /// can hand the wheel to a fast vision model. `None` keeps the chat's own.
    pub computer_model: Option<ModelRef>,
    /// Longest edge of computer screenshots. `0` is native resolution; any
    /// other value downscales (1568 is the most providers honour, 1280 is
    /// cheaper). Quality-first default: native.
    pub computer_screenshot_edge: u32,
}

impl Default for ChatDefaults {
    fn default() -> Self {
        Self {
            provider_id: None,
            model_id: None,
            variant: None,
            lite: None,
            image_model: None,
            embedding_model: None,
            auto_title: true,
            recent_models: Vec::new(),
            permission_mode: PermissionMode::Ask,
            agent_mode: AgentMode::Build,
            max_output_tokens: 0,
            condense_share: 20,
            max_tool_rounds: 40,
            computer_variant: Some("low".to_string()),
            computer_model: None,
            computer_screenshot_edge: 0,
        }
    }
}

/// Fills in settings that a newer release requires but an existing config file
/// cannot have — the session header that gateways such as OpenCode Go need,
/// and an output cap that meant something else in older builds. Returns `true`
/// when the config changed and should be saved.
///
/// Only fields that are still at their default are touched, so user choices are
/// never overwritten.
pub fn apply_preset_defaults(config: &mut AppConfig) -> bool {
    let mut changed = false;

    for (id, provider) in config.providers.iter_mut() {
        let Some(preset) = crate::provider::preset(id) else {
            continue;
        };
        if provider.session_header.is_none() {
            if let Some(header) = preset.session_header {
                provider.session_header = Some(header.to_string());
                changed = true;
            }
        }
    }

    // `maxOutputTokens` shipped as 8192, which cut long coding replies short
    // and shared the budget with a reasoning model's thinking. Zero now means
    // "the model's own limit".
    const LEGACY_MAX_OUTPUT: u32 = 8_192;
    if config.chat.max_output_tokens == LEGACY_MAX_OUTPUT {
        config.chat.max_output_tokens = 0;
        changed = true;
    }

    changed
}

/// One-time provenance pass over models saved before `source` existed.
///
/// The only metadata an older build let the user edit was `context` and
/// `output`, so agreement with the bundled catalogue there means the rest of
/// the spec was machine-derived and a refresh may correct it (`catalog@0`,
/// below the current version so it re-syncs). A difference means a hand edit
/// and is kept as `user`. Ids the catalogue does not know, or knows nothing
/// about, stay `unknown` so a refresh can fill them without ever pretending.
///
/// Returns `true` when the config changed and should be saved.
pub fn migrate_metadata(config: &mut AppConfig) -> bool {
    if config.metadata_version >= METADATA_VERSION {
        return false;
    }

    for provider in config.providers.values_mut() {
        for (model_id, spec) in provider.models.iter_mut() {
            if spec.source != MetadataSource::Unknown {
                continue;
            }
            let detected = crate::catalog::lookup(model_id);
            let favorite = spec.favorite;
            let name = spec.name.take();

            *spec = match detected {
                // The catalogue knows the family and nothing else: stale
                // defaults from an old build are worse than honesty.
                Some(detected) if !detected.has_metadata() => ModelSpec {
                    favorite,
                    name,
                    ..detected
                },
                // No values to protect: first refresh fills it.
                _ if spec.context.is_none() && spec.output.is_none() => ModelSpec {
                    favorite,
                    name,
                    ..spec.clone()
                },
                Some(detected)
                    if spec.context == detected.context && spec.output == detected.output =>
                {
                    ModelSpec {
                        favorite,
                        name,
                        source: MetadataSource::Catalog(0),
                        ..spec.clone()
                    }
                }
                Some(_) => ModelSpec {
                    favorite,
                    name,
                    source: MetadataSource::User,
                    ..spec.clone()
                },
                None => ModelSpec {
                    favorite,
                    name,
                    ..spec.clone()
                },
            };
        }
    }

    config.metadata_version = METADATA_VERSION;
    true
}

/// Who the user is, for `{{user}}` and friends in persona prompts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserProfile {
    pub name: String,
    /// e.g. "she/her".
    pub pronouns: String,
    /// A short paragraph the user wants personas to know.
    pub about: String,
}

/// A named set of personas. `cast` groups double as the starting line-up for
/// multi-persona chats; ordinary groups are just folders.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonaGroup {
    pub id: String,
    pub name: String,
    /// Persona ids, in display order.
    pub members: Vec<String>,
    /// When true the group can start a multi-persona chat.
    pub cast: bool,
}

/// A reusable prompt snippet, managed in the settings and offered in the
/// composer alongside skills.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prompt {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// A folder the user added as a workspace. Chats remember their workspace by
/// path; this list is what keeps the folder around after a restart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Workspace {
    pub path: String,
    /// Display name; defaults to the folder's name, but can be renamed.
    pub name: String,
    /// When the folder was added (ms since epoch).
    pub added_at: i64,
}

/// Which service backs the `web_search` and `fetch_url` tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchProvider {
    /// Jina when an API key is stored, DuckDuckGo otherwise.
    #[default]
    Auto,
    /// Jina AI search and reader (needs a key for search).
    Jina,
    /// Keyless DuckDuckGo HTML results and a local page reader.
    Duckduckgo,
}

/// How much of a reasoning model''s thinking to show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingDisplay {
    /// A one-line header you expand when you want it.
    #[default]
    Collapsed,
    /// Not rendered unless opened from the message actions.
    Hidden,
    /// Open from the start.
    Expanded,
}

/// How much of the tool calls' detail to show in the transcript.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallDisplay {
    /// Compact rows you expand when you want the arguments and output.
    #[default]
    Collapsed,
    /// Arguments and output open from the start.
    Expanded,
    /// Nothing rendered at all.
    Hidden,
}

/// Which keystroke sends a message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendKey {
    #[default]
    Enter,
    CtrlEnter,
}

/// Whether the chat list is split into workspace groups or shown flat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarGrouping {
    /// One group per workspace folder.
    #[default]
    Workspace,
    /// A single flat list.
    None,
}

/// Order of chats in the sidebar list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarSort {
    /// Newest activity first.
    #[default]
    Recent,
    /// Oldest activity first.
    Oldest,
    /// By title, A to Z.
    Title,
}

/// Interface behaviour that is not tied to a single chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InterfaceConfig {
    pub show_thinking: ThinkingDisplay,
    /// Whether search/file/tool activity is shown in the transcript.
    pub show_tool_calls: ToolCallDisplay,
    pub send_key: SendKey,
    pub notify_on_completion: bool,
    /// Register the quick-ask overlay hotkey at all.
    pub hotkey_enabled: bool,
    /// Hotkey string understood by the global-shortcut plugin.
    pub hotkey: String,
    /// Keep the transcript pinned to the newest text even while reading older
    /// messages.
    pub always_follow: bool,
    /// Keep the chats list docked beside the chat instead of floating over it.
    pub sidebar_pinned: bool,
    /// Width in pixels of the docked sidebar.
    pub sidebar_width: u32,
    /// Whether the sidebar splits chats by workspace or lists them flat.
    pub sidebar_grouping: SidebarGrouping,
    /// Order of chats in the sidebar list.
    pub sidebar_sort: SidebarSort,
    /// Denser transcript and smaller text.
    pub compact: bool,
    /// Let the model render ```loom-ui blocks as live, themed widgets.
    pub generated_ui: bool,
    /// Show a faint line under a reply that was answered from a condensed view
    /// of the chat's older turns, expandable to the text the model was given.
    pub show_condensing: bool,
    /// Attach a screenshot of the current monitor to every quick-ask send.
    pub capture_on_send: bool,
    /// Let the model build long-term memory in the background: a lite-model
    /// pass after each reply proposes durable facts, which are saved to the
    /// Memory page. Facts always save instantly when the model calls
    /// `remember_fact`; this only governs the automatic pass.
    pub auto_memory: bool,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            show_thinking: ThinkingDisplay::Collapsed,
            show_tool_calls: ToolCallDisplay::Collapsed,
            send_key: SendKey::Enter,
            notify_on_completion: true,
            hotkey_enabled: true,
            hotkey: "Ctrl+Shift+Space".to_string(),
            always_follow: false,
            sidebar_pinned: false,
            sidebar_width: 264,
            sidebar_grouping: SidebarGrouping::Workspace,
            sidebar_sort: SidebarSort::Recent,
            compact: false,
            generated_ui: true,
            show_condensing: true,
            capture_on_send: true,
            auto_memory: true,
        }
    }
}

/// Loads configuration from the standard location, falling back to defaults
/// when the file does not exist yet.
pub fn load() -> Result<AppConfig> {
    load_from(&paths::config_path()?)
}

/// Saves configuration to the standard location.
pub fn save(config: &AppConfig) -> Result<()> {
    save_to(&paths::config_path()?, config)
}

/// Path-explicit variant used by tests and portable setups.
pub fn load_from(path: &Path) -> Result<AppConfig> {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| Error::json(path, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Path-explicit variant used by tests and portable setups.
pub fn save_to(path: &Path, config: &AppConfig) -> Result<()> {
    let mut json = serde_json::to_string_pretty(config).map_err(|e| Error::json(path, e))?;
    json.push('\n');
    atomic_write(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Modality, ModelSpec, ProviderKind};

    fn temp_config_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("config.json")
    }

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(&temp_config_path(&dir)).unwrap();
        assert_eq!(config, AppConfig::default());
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.background.preset, "porcelain");
        assert_eq!(config.chat.permission_mode, PermissionMode::Ask);
        assert_eq!(config.chat.agent_mode, AgentMode::Build);
    }

    #[test]
    fn round_trip_preserves_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_config_path(&dir);

        let mut config = AppConfig::default();
        config.theme = Theme::Light;
        config.sidebar_collapsed = true;
        config.background.kind = BackgroundKind::Video;
        config.background.path = Some("C:/art/loop.webm".to_string());
        config.background.dim = 55;
        config.background.blur = 18;
        config.chat.lite = Some(ModelRef::new("openai", "gpt-4o-mini"));
        config
            .personas
            .push(Persona::new("Coder", "You write Rust."));
        config.search_provider = SearchProvider::Jina;
        config.workspaces.push(Workspace {
            path: "C:/work/loom".into(),
            name: "Loom".into(),
            added_at: 42,
        });

        let mut provider = ProviderConfig {
            name: "Local".into(),
            kind: ProviderKind::OpenaiCompatible,
            base_url: "http://localhost:11434/v1".into(),
            key_required: false,
            ..Default::default()
        };
        provider.models.insert(
            "qwen3:8b".into(),
            ModelSpec::with_modalities(&[Modality::Text]),
        );
        config.providers.insert("ollama".into(), provider);

        save_to(&path, &config).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn partial_json_fills_defaults_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_config_path(&dir);

        std::fs::write(
            &path,
            r#"{ "theme": "light", "futureFeature": { "enabled": true } }"#,
        )
        .unwrap();

        let config = load_from(&path).unwrap();
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.background, BackgroundConfig::default());
        assert_eq!(config.schema_version, SCHEMA_VERSION);
        assert!(config.providers.is_empty());
        assert!(config.extra.contains_key("futureFeature"));

        save_to(&path, &config).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("futureFeature"));
    }

    #[test]
    fn serializes_camel_case_keys() {
        let config = AppConfig::default();
        let raw = serde_json::to_string(&config).unwrap();
        assert!(raw.contains("schemaVersion"));
        assert!(raw.contains("sidebarCollapsed"));
        assert!(raw.contains("permissionMode"));
        assert!(raw.contains("agentMode"));
        assert!(raw.contains("searchProvider"));
        assert!(raw.contains("workspaces"));
    }

    #[test]
    fn search_provider_round_trips_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&SearchProvider::Duckduckgo).unwrap(),
            "\"duckduckgo\""
        );
        assert_eq!(
            serde_json::to_string(&SearchProvider::Auto).unwrap(),
            "\"auto\""
        );
        let config: AppConfig = serde_json::from_str(r#"{ "searchProvider": "jina" }"#).unwrap();
        assert_eq!(config.search_provider, SearchProvider::Jina);
    }

    #[test]
    fn agent_mode_round_trips_as_lowercase() {
        assert_eq!(serde_json::to_string(&AgentMode::Plan).unwrap(), "\"plan\"");
        let config: AppConfig =
            serde_json::from_str(r#"{ "chat": { "agentMode": "plan" } }"#).unwrap();
        assert_eq!(config.chat.agent_mode, AgentMode::Plan);

        // Chat is a global default as well as a per-chat override, so pin both
        // its wire value and the key. An older build that does not know "chat"
        // fails to parse the string and falls back to the built-in default,
        // the same downgrade story as `permissionMode: "atelier"`.
        assert_eq!(serde_json::to_string(&AgentMode::Chat).unwrap(), "\"chat\"");
        let config: AppConfig =
            serde_json::from_str(r#"{ "chat": { "agentMode": "chat" } }"#).unwrap();
        assert_eq!(config.chat.agent_mode, AgentMode::Chat);
    }

    #[test]
    fn chat_is_narrow_rather_than_read_only() {
        // The distinction the whole mode rests on. `blocks_writes` gates the
        // read-only halves of computer use and the workspace tools; Chat must
        // not be swept into either.
        assert!(!AgentMode::Chat.blocks_writes());
        assert!(AgentMode::Chat.is_chat());
        assert_eq!(AgentMode::Chat.label(), "Chat");

        assert!(!AgentMode::Build.is_chat());
        assert!(!AgentMode::Plan.is_chat());
        assert!(!AgentMode::Review.is_chat());
        assert!(AgentMode::Plan.blocks_writes());
        assert!(AgentMode::Review.blocks_writes());
    }

    #[test]
    fn permission_mode_wire_values_are_stable() {
        assert_eq!(
            serde_json::to_string(&PermissionMode::AutoReadOnly).unwrap(),
            "\"auto-read-only\""
        );
        // An older build that does not know Atelier must fail to parse the
        // string rather than misread it; the session then falls back to the
        // global default. That is the whole downgrade story.
        assert_eq!(
            serde_json::to_string(&PermissionMode::Atelier).unwrap(),
            "\"atelier\""
        );
        let config: AppConfig =
            serde_json::from_str(r#"{ "chat": { "permissionMode": "atelier" } }"#).unwrap();
        assert_eq!(config.chat.permission_mode, PermissionMode::Atelier);
    }

    #[test]
    fn invalid_json_reports_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_config_path(&dir);
        std::fs::write(&path, "{ not json").unwrap();

        let err = load_from(&path).unwrap_err();
        assert!(matches!(err, Error::Json { .. }));
    }

    #[test]
    fn provider_kind_round_trips_as_kebab_case() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            ..Default::default()
        };
        let raw = serde_json::to_string(&provider).unwrap();
        assert!(raw.contains("\"anthropic\""));
    }

    #[test]
    fn preset_defaults_upgrade_existing_providers() {
        let mut config = AppConfig::default();
        // A provider saved by an older build, before session headers existed.
        let legacy = ProviderConfig {
            name: "OpenCode Go".into(),
            base_url: "https://opencode.ai/zen/go/v1".into(),
            session_header: None,
            ..Default::default()
        };
        config.providers.insert("opencode-go".into(), legacy);

        assert!(apply_preset_defaults(&mut config));
        assert_eq!(
            config.providers["opencode-go"].session_header.as_deref(),
            Some("x-opencode-session")
        );

        // Running again is a no-op.
        assert!(!apply_preset_defaults(&mut config));
    }

    #[test]
    fn legacy_output_default_is_upgraded_to_the_model_limit() {
        let mut config = AppConfig::default();
        config.chat.max_output_tokens = 8_192;

        assert!(apply_preset_defaults(&mut config));
        assert_eq!(config.chat.max_output_tokens, 0);

        // A deliberate cap is respected, and running again is a no-op.
        config.chat.max_output_tokens = 4_096;
        assert!(!apply_preset_defaults(&mut config));
        assert_eq!(config.chat.max_output_tokens, 4_096);
    }

    #[test]
    fn preset_defaults_leave_custom_providers_alone() {
        let mut config = AppConfig::default();
        config.providers.insert(
            "my-local".into(),
            ProviderConfig {
                name: "Local".into(),
                base_url: "http://localhost:11434/v1".into(),
                ..Default::default()
            },
        );

        assert!(!apply_preset_defaults(&mut config));
        assert!(config.providers["my-local"].session_header.is_none());
    }

    #[test]
    fn metadata_migration_separates_detected_from_edited() {
        use crate::provider::MetadataSource;

        let mut config = AppConfig::default();
        config.metadata_version = 0;
        let mut provider = ProviderConfig {
            name: "P".into(),
            ..Default::default()
        };
        // Matches the catalogue exactly: machine-derived, re-syncable.
        provider.models.insert(
            "gpt-4o".into(),
            ModelSpec {
                context: Some(128_000),
                output: Some(16_000),
                ..Default::default()
            },
        );
        // A hand-typed window: protected.
        provider.models.insert(
            "gpt-5".into(),
            ModelSpec {
                context: Some(99),
                output: None,
                ..Default::default()
            },
        );
        // Unknown to the catalogue, but has values: keep them, still unknown.
        provider.models.insert(
            "my-finetune".into(),
            ModelSpec {
                context: Some(4_096),
                ..Default::default()
            },
        );
        // An explicitly unknown family: stale fallback defaults are dropped.
        provider.models.insert(
            "hy3".into(),
            ModelSpec {
                context: Some(128_000),
                output: Some(8_192),
                input_modalities: vec![Modality::Text],
                ..Default::default()
            },
        );

        config.providers.insert("P".into(), provider);

        assert!(migrate_metadata(&mut config));
        let models = &config.providers["P"].models;
        assert_eq!(models["gpt-4o"].source, MetadataSource::Catalog(0));
        assert_eq!(models["gpt-5"].source, MetadataSource::User);
        assert_eq!(models["gpt-5"].context, Some(99), "hand edit survives");
        assert_eq!(models["my-finetune"].source, MetadataSource::Unknown);
        assert_eq!(models["my-finetune"].context, Some(4_096));
        assert_eq!(models["hy3"].source, MetadataSource::Unknown);
        assert!(models["hy3"].context.is_none());
        assert!(models["hy3"].input_modalities.is_empty());

        // Running again is a no-op.
        assert!(!migrate_metadata(&mut config));
    }

    #[test]
    fn configs_without_a_metadata_version_upgrade_once() {
        use crate::provider::MetadataSource;

        let dir = tempfile::tempdir().unwrap();
        let path = temp_config_path(&dir);
        std::fs::write(
            &path,
            r#"{ "providers": { "p": { "models": { "gpt-4o": { "context": 128000, "output": 16000 } } } } }"#,
        )
        .unwrap();

        let mut config = load_from(&path).unwrap();
        assert_eq!(config.metadata_version, 0);
        assert!(migrate_metadata(&mut config));
        assert_eq!(config.metadata_version, METADATA_VERSION);
        assert_eq!(
            config.providers["p"].models["gpt-4o"].source,
            MetadataSource::Catalog(0)
        );

        save_to(&path, &config).unwrap();
        let mut reloaded = load_from(&path).unwrap();
        assert!(!migrate_metadata(&mut reloaded));
        assert_eq!(reloaded, config);
    }
}
