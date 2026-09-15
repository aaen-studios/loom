//! Provider and model configuration.
//!
//! Deliberately ZCode-shaped: a map of providers keyed by id, each with a
//! protocol `kind`, a base URL, optional extra headers, and a model map whose
//! values carry the metadata the UI needs (context window, modalities,
//! reasoning variants). API keys never live here — see `secrets`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Native OpenAI chat-completions dialect (also used by most vendors).
    #[default]
    OpenaiCompatible,
    /// Anthropic messages dialect.
    Anthropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelsSource {
    #[default]
    Manual,
    Fetched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    #[default]
    Text,
    Image,
    Audio,
    Video,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ReasoningSpec {
    pub enabled: bool,
    /// Provider-specific effort variants, e.g. `["low", "medium", "high"]`.
    pub variants: Vec<String>,
    pub default_variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelSpec {
    /// Display name when it differs from the id.
    pub name: Option<String>,
    pub context: Option<u32>,
    pub output: Option<u32>,
    pub input_modalities: Vec<Modality>,
    pub reasoning: Option<ReasoningSpec>,
    /// Marks the user's favourite models for the picker.
    pub favorite: bool,
}

impl ModelSpec {
    pub fn with_modalities(modalities: &[Modality]) -> Self {
        Self {
            input_modalities: modalities.to_vec(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderConfig {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    /// Extra request headers (e.g. OpenRouter's `HTTP-Referer`).
    pub headers: BTreeMap<String, String>,
    pub enabled: bool,
    pub models: BTreeMap<String, ModelSpec>,
    pub models_source: ModelsSource,
    pub last_fetched_at: Option<i64>,
    /// Empty means no key required (Ollama, LM Studio).
    pub key_required: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: ProviderKind::OpenaiCompatible,
            base_url: String::new(),
            headers: BTreeMap::new(),
            enabled: true,
            models: BTreeMap::new(),
            models_source: ModelsSource::Manual,
            last_fetched_at: None,
            key_required: true,
        }
    }
}

impl ProviderConfig {
    /// Normalises a base URL: trims whitespace and trailing slashes, and adds
    /// the conventional `/v1` suffix for OpenAI-compatible hosts when the user
    /// typed only the origin.
    pub fn normalized_base_url(&self) -> String {
        let trimmed = self.base_url.trim().trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            return trimmed;
        }
        if self.kind == ProviderKind::Anthropic {
            return trimmed;
        }
        if trimmed.ends_with("/v1") || trimmed.contains("/v1/") {
            return trimmed;
        }
        format!("{trimmed}/v1")
    }
}

/// A built-in provider template offered in the "Add provider" flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ProviderKind,
    pub base_url: &'static str,
    pub key_required: bool,
    pub note: &'static str,
}

pub const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openai",
        name: "OpenAI",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.openai.com/v1",
        key_required: true,
        note: "Official OpenAI API",
    },
    ProviderPreset {
        id: "anthropic",
        name: "Anthropic",
        kind: ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com",
        key_required: true,
        note: "Claude models, native Messages API",
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://openrouter.ai/api/v1",
        key_required: true,
        note: "One key for most models",
    },
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.deepseek.com/v1",
        key_required: true,
        note: "DeepSeek chat and reasoner",
    },
    ProviderPreset {
        id: "zai",
        name: "Z.ai",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.z.ai/api/paas/v4",
        key_required: true,
        note: "GLM family",
    },
    ProviderPreset {
        id: "groq",
        name: "Groq",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.groq.com/openai/v1",
        key_required: true,
        note: "Very fast inference",
    },
    ProviderPreset {
        id: "xai",
        name: "xAI",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.x.ai/v1",
        key_required: true,
        note: "Grok models",
    },
    ProviderPreset {
        id: "google",
        name: "Google AI Studio",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        key_required: true,
        note: "Gemini via the OpenAI-compatible endpoint",
    },
    ProviderPreset {
        id: "opencode-go",
        name: "OpenCode Go",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://opencode.ai/zen/go/v1",
        key_required: true,
        note: "OpenCode Go subscription (same key as Zen)",
    },
    ProviderPreset {
        id: "opencode-zen",
        name: "OpenCode Zen",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://opencode.ai/zen/v1",
        key_required: true,
        note: "Pay-as-you-go gateway from the OpenCode team",
    },
    ProviderPreset {
        id: "ollama",
        name: "Ollama",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "http://localhost:11434/v1",
        key_required: false,
        note: "Local models, no key",
    },
    ProviderPreset {
        id: "lmstudio",
        name: "LM Studio",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "http://localhost:1234/v1",
        key_required: false,
        note: "Local models, no key",
    },
];

pub fn preset(id: &str) -> Option<&'static ProviderPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_trailing_slashes_and_adds_v1() {
        let provider = ProviderConfig {
            base_url: "https://api.example.com/".into(),
            ..Default::default()
        };
        assert_eq!(provider.normalized_base_url(), "https://api.example.com/v1");

        let provider = ProviderConfig {
            base_url: "https://api.example.com/v1/".into(),
            ..Default::default()
        };
        assert_eq!(provider.normalized_base_url(), "https://api.example.com/v1");

        let provider = ProviderConfig {
            base_url: "https://openrouter.ai/api/v1".into(),
            ..Default::default()
        };
        assert_eq!(
            provider.normalized_base_url(),
            "https://openrouter.ai/api/v1"
        );
    }

    #[test]
    fn anthropic_base_url_is_left_alone() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            ..Default::default()
        };
        assert_eq!(provider.normalized_base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn presets_have_unique_ids_and_valid_kinds() {
        let mut ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len());
        assert!(preset("ollama").is_some_and(|p| !p.key_required));
        assert!(preset("nope").is_none());
    }

    #[test]
    fn opencode_presets_point_at_the_right_gateways() {
        let go = preset("opencode-go").unwrap();
        assert_eq!(go.base_url, "https://opencode.ai/zen/go/v1");
        assert!(go.key_required);

        let zen = preset("opencode-zen").unwrap();
        assert_eq!(zen.base_url, "https://opencode.ai/zen/v1");

        let provider = ProviderConfig {
            base_url: go.base_url.into(),
            ..Default::default()
        };
        // Already versioned, so normalization must not double the /v1.
        assert_eq!(
            provider.normalized_base_url(),
            "https://opencode.ai/zen/go/v1"
        );
    }
}
