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
use crate::provider::ProviderConfig;
use crate::{paths, Error, Result};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub theme: Theme,
    pub background: BackgroundConfig,
    pub sidebar_collapsed: bool,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub personas: Vec<Persona>,
    pub mcp_servers: BTreeMap<String, crate::mcp::McpServerConfig>,
    pub chat: ChatDefaults,
    pub interface: InterfaceConfig,
    /// Reusable prompt snippets offered in the composer's slash menu.
    pub prompts: Vec<Prompt>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: Theme::Dark,
            background: BackgroundConfig::default(),
            sidebar_collapsed: false,
            providers: BTreeMap::new(),
            personas: Vec::new(),
            mcp_servers: BTreeMap::new(),
            chat: ChatDefaults::default(),
            interface: InterfaceConfig::default(),
            prompts: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    #[default]
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
    /// Built-in preset id (see the UI's preset list).
    pub preset: String,
    /// Absolute path for `Image`/`Video` kinds.
    pub path: Option<String>,
    /// 0..=100 black overlay strength.
    pub dim: u8,
    /// 0..=64 px blur applied to the background layer.
    pub blur: u8,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            kind: BackgroundKind::Builtin,
            preset: "rei".to_string(),
            path: None,
            dim: 30,
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
    /// How many past messages to send as context.
    pub history_limit: u32,
    pub max_output_tokens: u32,
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
            history_limit: 40,
            max_output_tokens: 8_192,
        }
    }
}

/// Fills in settings that a newer release requires but an existing config file
/// cannot have — today, the session header that gateways such as OpenCode Go
/// need. Returns `true` when the config changed and should be saved.
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

    changed
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

/// Which keystroke sends a message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendKey {
    #[default]
    Enter,
    CtrlEnter,
}

/// Interface behaviour that is not tied to a single chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InterfaceConfig {
    pub show_thinking: ThinkingDisplay,
    pub send_key: SendKey,
    pub notify_on_completion: bool,
    /// Register the quick-ask overlay hotkey at all.
    pub hotkey_enabled: bool,
    /// Hotkey string understood by the global-shortcut plugin.
    pub hotkey: String,
    /// Keep the transcript pinned to the newest text even while reading older
    /// messages.
    pub always_follow: bool,
    /// Keep the chats popup open until it is explicitly closed.
    pub sidebar_pinned: bool,
    /// Denser transcript and smaller text.
    pub compact: bool,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            show_thinking: ThinkingDisplay::Collapsed,
            send_key: SendKey::Enter,
            notify_on_completion: true,
            hotkey_enabled: true,
            hotkey: "Ctrl+Shift+Space".to_string(),
            always_follow: false,
            sidebar_pinned: false,
            compact: false,
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
        assert_eq!(config.theme, Theme::Dark);
        assert_eq!(config.background.preset, "rei");
        assert_eq!(config.chat.permission_mode, PermissionMode::Ask);
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
        config.personas.push(Persona::new("Coder", "You write Rust."));

        let mut provider = ProviderConfig {
            name: "Local".into(),
            kind: ProviderKind::OpenaiCompatible,
            base_url: "http://localhost:11434/v1".into(),
            key_required: false,
            ..Default::default()
        };
        provider
            .models
            .insert("qwen3:8b".into(), ModelSpec::with_modalities(&[Modality::Text]));
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
}




