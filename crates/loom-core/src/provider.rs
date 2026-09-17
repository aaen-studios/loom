//! Provider and model configuration.
//!
//! Deliberately ZCode-shaped: a map of providers keyed by id, each with a
//! protocol `kind`, a base URL, optional extra headers, and a model map whose
//! values carry the metadata the UI needs (context window, modalities,
//! reasoning variants). API keys never live here — see `secrets`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

impl Modality {
    /// Parses the modality names gateways and datasets use. `file` is how
    /// OpenRouter spells a document/PDF input.
    pub fn from_wire(name: &str) -> Option<Self> {
        match name {
            "text" => Some(Modality::Text),
            "image" => Some(Modality::Image),
            "audio" => Some(Modality::Audio),
            "video" => Some(Modality::Video),
            "file" | "pdf" => Some(Modality::Pdf),
            _ => None,
        }
    }
}

/// Where a model's metadata came from. Refreshes may overwrite anything below
/// `User`; a spec tagged `User` is never touched by detection again.
///
/// Serialises as `"user"`, `"api"`, `"unknown"`, or `"catalog@N"` so the wire
/// value stays legible in `config.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MetadataSource {
    /// The user typed or confirmed it; detection must not overwrite it.
    User,
    /// Read from the gateway's `/models` response.
    Api,
    /// Bundled catalogue, tagged with the `CATALOG_VERSION` that produced it.
    Catalog(u32),
    /// Nothing is known about where these values came from.
    #[default]
    Unknown,
}

impl MetadataSource {
    /// Merge rank: the pair wins against any lower pair. Versions order
    /// catalogue entries so a newer table can correct an older guess.
    pub fn authority(self) -> (u8, u32) {
        match self {
            MetadataSource::Unknown => (0, 0),
            MetadataSource::Catalog(version) => (1, version),
            MetadataSource::Api => (2, 0),
            MetadataSource::User => (3, 0),
        }
    }

    pub fn is_catalog(self) -> bool {
        matches!(self, MetadataSource::Catalog(_))
    }
}

impl std::fmt::Display for MetadataSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataSource::User => write!(formatter, "user"),
            MetadataSource::Api => write!(formatter, "api"),
            MetadataSource::Unknown => write!(formatter, "unknown"),
            MetadataSource::Catalog(version) => write!(formatter, "catalog@{version}"),
        }
    }
}

impl Serialize for MetadataSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            MetadataSource::User => serializer.serialize_str("user"),
            MetadataSource::Api => serializer.serialize_str("api"),
            MetadataSource::Unknown => serializer.serialize_str("unknown"),
            MetadataSource::Catalog(version) => {
                serializer.serialize_str(&format!("catalog@{version}"))
            }
        }
    }
}

impl<'de> Deserialize<'de> for MetadataSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "user" => MetadataSource::User,
            "api" => MetadataSource::Api,
            other => match other
                .strip_prefix("catalog@")
                .and_then(|rest| rest.parse().ok())
            {
                Some(version) => MetadataSource::Catalog(version),
                None => MetadataSource::Unknown,
            },
        })
    }
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
    /// USD per 1M input tokens, when known. The user can correct it.
    pub input_price: Option<f32>,
    /// USD per 1M output tokens, when known.
    pub output_price: Option<f32>,
    /// Who supplied this metadata; refreshes never overwrite `user`.
    pub source: MetadataSource,
}

impl ModelSpec {
    pub fn with_modalities(modalities: &[Modality]) -> Self {
        Self {
            input_modalities: modalities.to_vec(),
            ..Default::default()
        }
    }

    /// Whether any field carries real information. An explicit catalogue row
    /// for a family whose values are unknown is all-empty by design.
    ///
    /// Deliberately says nothing about whether the model is *selected*: the
    /// selection lives on the provider, not on the spec.
    pub fn has_metadata(&self) -> bool {
        self.name.is_some()
            || self.context.is_some()
            || self.output.is_some()
            || !self.input_modalities.is_empty()
            || self.reasoning.is_some()
            || self.input_price.is_some()
            || self.output_price.is_some()
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
    /// Header the gateway wants filled with a stable per-conversation id
    /// (OpenCode Go uses `x-opencode-session` for routing and prompt caching).
    pub session_header: Option<String>,
    /// Which preset this instance was created from; `None` for hand-made
    /// providers. Kept so a *duplicated* provider — whose id is suffixed and
    /// therefore no longer matches a preset id — still resolves its preset.
    pub preset_id: Option<String>,
    /// Models the user has switched off. Stored as a denylist so a model added
    /// by any other route (a catalogue row, a manual add, an older build) is
    /// selected by default, and an empty set means "everything selected" —
    /// which is what an existing config file deserializes to.
    ///
    /// This lives here rather than as a flag on `ModelSpec` because `ModelSpec`
    /// is *metadata*: a `GET /models` refresh replaces it wholesale, and the
    /// user's choice has to outlive that.
    pub disabled_models: BTreeSet<String>,
    /// Whether models discovered by a refresh start selected. `true` keeps the
    /// historical behaviour; `false` makes a large gateway opt-in.
    pub auto_select_models: bool,
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
            session_header: None,
            preset_id: None,
            disabled_models: BTreeSet::new(),
            // Must agree with the `Default` impl, because the container-level
            // `#[serde(default)]` sends every absent field here. A plain `false`
            // would silently switch every existing config to opt-in and empty
            // the pickers.
            auto_select_models: true,
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

    // ------------------------------------------------------------------
    // Model selection
    // ------------------------------------------------------------------

    /// Whether `model_id` should appear in pickers. Absence from the denylist
    /// is what makes a brand-new model selected without any write.
    pub fn model_selected(&self, model_id: &str) -> bool {
        !self.disabled_models.contains(model_id)
    }

    /// The selected members of this provider's catalogue, in id order.
    pub fn selected_models(&self) -> impl Iterator<Item = &String> {
        self.models
            .keys()
            .filter(|id| self.model_selected(id))
    }

    /// Turns one model's selection on or off. Returns `true` when the denylist
    /// actually changed, so callers can skip a pointless config write.
    pub fn set_model_selected(&mut self, model_id: &str, selected: bool) -> bool {
        if selected {
            self.disabled_models.remove(model_id)
        } else {
            // Only record ids that exist; a stale id in the denylist would
            // otherwise linger forever after the model left the catalogue.
            if !self.models.contains_key(model_id) {
                return false;
            }
            self.disabled_models.insert(model_id.to_string())
        }
    }

    /// Forgets denylist entries for models that are no longer in the
    /// catalogue. Called after a refresh so the set cannot grow without bound.
    pub fn prune_disabled_models(&mut self) -> bool {
        let before = self.disabled_models.len();
        let known: BTreeSet<&String> = self.models.keys().collect();
        self.disabled_models.retain(|id| known.contains(id));
        self.disabled_models.len() != before
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
    /// Set when the gateway requires a session identifier header.
    pub session_header: Option<&'static str>,
}

pub const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openai",
        name: "OpenAI",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.openai.com/v1",
        key_required: true,
        note: "Official OpenAI API",
        session_header: None,
    },
    ProviderPreset {
        id: "anthropic",
        name: "Anthropic",
        kind: ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com",
        key_required: true,
        note: "Claude models, native Messages API",
        session_header: None,
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://openrouter.ai/api/v1",
        key_required: true,
        note: "One key for most models",
        session_header: None,
    },
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.deepseek.com/v1",
        key_required: true,
        note: "DeepSeek chat and reasoner",
        session_header: None,
    },
    ProviderPreset {
        id: "zai",
        name: "Z.ai",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.z.ai/api/paas/v4",
        key_required: true,
        note: "GLM family",
        session_header: None,
    },
    ProviderPreset {
        id: "groq",
        name: "Groq",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.groq.com/openai/v1",
        key_required: true,
        note: "Very fast inference",
        session_header: None,
    },
    ProviderPreset {
        id: "xai",
        name: "xAI",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://api.x.ai/v1",
        key_required: true,
        note: "Grok models",
        session_header: None,
    },
    ProviderPreset {
        id: "google",
        name: "Google AI Studio",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        key_required: true,
        note: "Gemini via the OpenAI-compatible endpoint",
        session_header: None,
    },
    ProviderPreset {
        id: "opencode-go",
        name: "OpenCode Go",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://opencode.ai/zen/go/v1",
        key_required: true,
        note: "OpenCode Go subscription (same key as Zen)",
        session_header: Some("x-opencode-session"),
    },
    ProviderPreset {
        id: "opencode-zen",
        name: "OpenCode Zen",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "https://opencode.ai/zen/v1",
        key_required: true,
        note: "Pay-as-you-go gateway from the OpenCode team",
        session_header: None,
    },
    ProviderPreset {
        id: "ollama",
        name: "Ollama",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "http://localhost:11434/v1",
        key_required: false,
        note: "Local models, no key",
        session_header: None,
    },
    ProviderPreset {
        id: "lmstudio",
        name: "LM Studio",
        kind: ProviderKind::OpenaiCompatible,
        base_url: "http://localhost:1234/v1",
        key_required: false,
        note: "Local models, no key",
        session_header: None,
    },
];

pub fn preset(id: &str) -> Option<&'static ProviderPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// The preset a *configured* provider came from.
///
/// `preset(id)` alone is not enough once a provider can be duplicated: a copy
/// is stored under a suffixed id (`opencode-go-x9z2`), which matches no preset,
/// so the gateway-specific session header would never be applied to it. This
/// falls back to the recorded preset id, then to a unique base-URL match —
/// which also repairs providers whose id the user renamed by hand.
pub fn preset_for(id: &str, provider: &ProviderConfig) -> Option<&'static ProviderPreset> {
    if let Some(found) = preset(id) {
        return Some(found);
    }
    if let Some(preset_id) = provider.preset_id.as_deref() {
        if let Some(found) = preset(preset_id) {
            return Some(found);
        }
    }
    preset_by_base_url(&provider.base_url)
}

/// The preset id to *record* as a provider's provenance, if any.
///
/// Deliberately narrower than [`preset_for`]. Only an exact id match counts,
/// plus an id already on record (which is how a duplicated instance keeps its
/// preset). A base-URL match is evidence about *behaviour* — it is what gives a
/// renamed provider its gateway session header back — but it is not proof of
/// provenance: a hand-made provider pointed at a local Ollama URL is not an
/// Ollama-preset provider, and recording that guess would freeze it, outliving
/// any later edit to the URL.
pub fn preset_provenance(id: &str, provider: &ProviderConfig) -> Option<&'static str> {
    if let Some(found) = preset(id) {
        return Some(found.id);
    }
    // An id already on record is trusted, so a duplicated or renamed provider
    // never loses the preset it was built from. `preset` is re-checked so a
    // preset withdrawn in a later release cannot linger in a config file.
    let stored = provider.preset_id.as_deref()?;
    preset(stored).map(|found| found.id)
}

/// The preset whose base URL this provider uses, when exactly one does.
///
/// Ambiguity returns `None` on purpose: two presets sharing a host would
/// otherwise make the choice depend on `PRESETS` ordering.
fn preset_by_base_url(base_url: &str) -> Option<&'static ProviderPreset> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let normalized = ProviderConfig {
        base_url: trimmed.to_string(),
        ..Default::default()
    }
    .normalized_base_url();

    let matches: Vec<&'static ProviderPreset> = PRESETS
        .iter()
        .filter(|preset| {
            let candidate = ProviderConfig {
                kind: preset.kind,
                base_url: preset.base_url.to_string(),
                ..Default::default()
            }
            .normalized_base_url();
            candidate == normalized
        })
        .collect();

    match matches.as_slice() {
        [only] => Some(only),
        _ => None,
    }
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

    #[test]
    fn selection_is_a_denylist_so_new_models_arrive_selected() {
        let mut provider = ProviderConfig::default();
        provider.models.insert("a".into(), ModelSpec::default());
        provider.models.insert("b".into(), ModelSpec::default());

        assert!(provider.model_selected("a"));
        assert!(provider.model_selected("b"));
        // An id not in the catalogue reads as selected: only an explicit
        // unselect hides anything, which is what keeps an existing config
        // behaving exactly as it did before this setting existed.
        assert!(provider.model_selected("not-added-yet"));

        assert!(provider.set_model_selected("a", false));
        assert!(!provider.model_selected("a"));
        assert_eq!(
            provider.selected_models().map(String::as_str).collect::<Vec<_>>(),
            vec!["b"]
        );

        // Setting the state it is already in is not a change, so the caller
        // can skip a pointless config write.
        assert!(!provider.set_model_selected("a", false));
        assert!(provider.set_model_selected("a", true));
        assert!(!provider.set_model_selected("a", true));
    }

    #[test]
    fn an_unknown_id_is_never_recorded_in_the_denylist() {
        // Otherwise a stale id would linger forever — and re-hide the model if
        // a later refresh re-discovered it.
        let mut provider = ProviderConfig::default();
        assert!(!provider.set_model_selected("ghost", false));
        assert!(provider.disabled_models.is_empty());
    }

    #[test]
    fn pruning_forgets_models_that_left_the_catalogue() {
        let mut provider = ProviderConfig::default();
        provider.models.insert("kept".into(), ModelSpec::default());
        provider.disabled_models.insert("kept".into());
        provider.disabled_models.insert("gone".into());

        assert!(provider.prune_disabled_models());
        assert_eq!(
            provider.disabled_models.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["kept"]
        );
        // Nothing left to prune the second time.
        assert!(!provider.prune_disabled_models());
    }

    #[test]
    fn preset_for_resolves_a_duplicated_instance() {
        let go = preset("opencode-go").unwrap();
        let provider = ProviderConfig {
            base_url: go.base_url.into(),
            preset_id: Some("opencode-go".into()),
            ..Default::default()
        };

        // A duplicate's suffixed id matches no preset by name...
        assert!(preset("opencode-go-2").is_none());
        // ...so the recorded preset id is what resolves it, session header and
        // all. Without this, a second plan never gets the header its routing
        // and prompt caching depend on.
        let resolved = preset_for("opencode-go-2", &provider).unwrap();
        assert_eq!(resolved.id, "opencode-go");
        assert_eq!(resolved.session_header, Some("x-opencode-session"));
    }

    #[test]
    fn preset_for_falls_back_to_a_unique_base_url() {
        // A provider whose id was renamed by hand still gets its gateway setup.
        let go = preset("opencode-go").unwrap();
        let renamed = ProviderConfig {
            base_url: go.base_url.into(),
            ..Default::default()
        };
        assert_eq!(preset_for("my-go", &renamed).unwrap().id, "opencode-go");

        // An endpoint no preset knows stays unresolved.
        let local = ProviderConfig {
            base_url: "http://127.0.0.1:9999/v1".into(),
            ..Default::default()
        };
        assert!(preset_for("custom", &local).is_none());
    }

    #[test]
    fn a_base_url_match_never_becomes_recorded_provenance() {
        // The trap this guards: `apply_preset_defaults` runs on every startup,
        // so a *guessed* provenance would be written to disk immediately and
        // then outlive any later edit to the URL — pinning a hand-made provider
        // to a preset it was never created from.
        let provider = ProviderConfig {
            base_url: preset("ollama").unwrap().base_url.into(),
            ..Default::default()
        };
        // Behaviourally it *is* Ollama...
        assert_eq!(preset_for("my-local", &provider).unwrap().id, "ollama");
        // ...but nothing is on record, so there is nothing to persist.
        assert!(preset_provenance("my-local", &provider).is_none());

        // An exact id match is real provenance.
        assert_eq!(preset_provenance("ollama", &provider), Some("ollama"));

        // So is an id already recorded, which is what a duplicate carries.
        let duplicated = ProviderConfig {
            preset_id: Some("opencode-go".into()),
            ..Default::default()
        };
        assert_eq!(
            preset_provenance("opencode-go-2", &duplicated),
            Some("opencode-go")
        );
    }
}
