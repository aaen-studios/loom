//! App configuration (`~/.loom/config.json`).
//!
//! The schema is intentionally forward compatible: every field has a default,
//! unknown fields are preserved on rewrite, and `schemaVersion` gates future
//! migrations. Provider/model configuration joins this file in M1.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fsutil::atomic_write;
use crate::{paths, Error, Result};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub schema_version: u32,
    pub theme: Theme,
    pub background: BackgroundConfig,
    pub sidebar_collapsed: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            theme: Theme::Light,
            background: BackgroundConfig::default(),
            sidebar_collapsed: false,
            extra: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
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
            preset: "aurora".to_string(),
            path: None,
            dim: 26,
            blur: 0,
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
    let mut json =
        serde_json::to_string_pretty(config).map_err(|e| Error::json(path, e))?;
    json.push('\n');
    atomic_write(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_path(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("config.json")
    }

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(&temp_config_path(&dir)).unwrap();
        assert_eq!(config, AppConfig::default());
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.background.preset, "aurora");
    }

    #[test]
    fn round_trip_preserves_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_config_path(&dir);

        let mut config = AppConfig::default();
        config.theme = Theme::Dark;
        config.sidebar_collapsed = true;
        config.background.kind = BackgroundKind::Video;
        config.background.path = Some("C:/art/loop.webm".to_string());
        config.background.dim = 55;
        config.background.blur = 18;

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
            r#"{ "theme": "dark", "futureFeature": { "enabled": true } }"#,
        )
        .unwrap();

        let config = load_from(&path).unwrap();
        assert_eq!(config.theme, Theme::Dark);
        assert_eq!(config.background, BackgroundConfig::default());
        assert_eq!(config.schema_version, SCHEMA_VERSION);
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
    }

    #[test]
    fn invalid_json_reports_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_config_path(&dir);
        std::fs::write(&path, "{ not json").unwrap();

        let err = load_from(&path).unwrap_err();
        assert!(matches!(err, Error::Json { .. }));
    }
}
