//! App configuration (`~/.loom/config.json`).
//!
//! Forward compatible by design: every field has a default, unknown fields are
//! preserved on rewrite, and `schemaVersion` gates future migrations. Provider
//! and persona configuration lives here; API keys do not (see `secrets`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
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

/// A reference to one of the auxiliary models — image generation, embeddings.
///
/// These started life as bare model-id strings. That is unambiguous only while
/// a model id maps to one provider, which stops being true the moment the same
/// catalogue is configured twice (two OpenCode Go plans both serving
/// `qwen3:8b`). A ref may therefore name its provider.
///
/// An empty `provider_id` means "infer it at call time", which is exactly what
/// a legacy bare string deserializes to. `Serialize` writes a bare string back
/// whenever the provider is unset, so an untouched config round-trips
/// byte-identically and there is no migration to run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuxModelRef {
    /// Empty means "infer the provider".
    pub provider_id: String,
    pub model_id: String,
}

impl AuxModelRef {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }

    /// The legacy shape: a bare model id with no provider.
    pub fn bare(model_id: impl Into<String>) -> Self {
        Self {
            provider_id: String::new(),
            model_id: model_id.into(),
        }
    }

    pub fn is_qualified(&self) -> bool {
        !self.provider_id.trim().is_empty()
    }

    /// Whether this ref means the given model. An unqualified ref matches the
    /// id in *any* provider, which is what makes legacy configs keep working.
    pub fn matches(&self, provider_id: &str, model_id: &str) -> bool {
        self.model_id == model_id && (!self.is_qualified() || self.provider_id == provider_id)
    }
}

impl Serialize for AuxModelRef {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeMap;

        // A local model (Ollama, LM Studio) is often one provider deep, and a
        // legacy config is unqualified throughout: write the old shape back so
        // those files show no diff after a save.
        if !self.is_qualified() {
            return serializer.serialize_str(&self.model_id);
        }
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("providerId", &self.provider_id)?;
        map.serialize_entry("modelId", &self.model_id)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for AuxModelRef {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AuxVisitor;

        impl<'de> serde::de::Visitor<'de> for AuxVisitor {
            type Value = AuxModelRef;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a model id string, or {\"providerId\", \"modelId\"}")
            }

            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<AuxModelRef, E> {
                Ok(AuxModelRef::bare(value))
            }

            fn visit_string<E: serde::de::Error>(
                self,
                value: String,
            ) -> std::result::Result<AuxModelRef, E> {
                Ok(AuxModelRef::bare(value))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<AuxModelRef, A::Error> {
                let mut provider_id: Option<String> = None;
                let mut model_id: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "providerId" | "provider_id" => {
                            provider_id = map.next_value::<Option<String>>()?.or(provider_id);
                        }
                        "modelId" | "model_id" => {
                            model_id = Some(map.next_value::<String>()?);
                        }
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                let model_id = model_id
                    .ok_or_else(|| serde::de::Error::missing_field("modelId"))?;
                Ok(AuxModelRef {
                    provider_id: provider_id.unwrap_or_default(),
                    model_id,
                })
            }
        }

        deserializer.deserialize_any(AuxVisitor)
    }
}

/// Why an [`AuxModelRef`] resolved to the provider it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuxResolution {
    /// The ref named a provider that is configured and serves that model.
    Explicit(ModelRef),
    /// The ref was unqualified and exactly one configured provider serves the
    /// id, so the choice is forced.
    Unique(ModelRef),
    /// The ref was unqualified and several providers serve the id. The winner
    /// is the one chosen by precedence; the rest are listed so the UI can warn
    /// rather than silently switch providers when a duplicate plan is added.
    Ambiguous {
        chosen: ModelRef,
        others: Vec<ModelRef>,
    },
}

impl AuxResolution {
    pub fn model(&self) -> &ModelRef {
        match self {
            AuxResolution::Explicit(model) | AuxResolution::Unique(model) => model,
            AuxResolution::Ambiguous { chosen, .. } => chosen,
        }
    }

    /// True when more than one provider could serve the ref, so the settings
    /// page should say which one is actually in use.
    pub fn is_ambiguous(&self) -> bool {
        matches!(self, AuxResolution::Ambiguous { .. })
    }

    pub fn others(&self) -> &[ModelRef] {
        match self {
            AuxResolution::Ambiguous { others, .. } => others,
            _ => &[],
        }
    }
}

/// Resolves an auxiliary model ref against the configured providers.
///
/// Precedence, in order:
///
/// 1. the provider the ref names, when it exists and serves the model;
/// 2. the only provider that serves the id, when there is exactly one;
/// 3. the provider of the app-wide default model, when it serves the id;
/// 4. otherwise the first provider (in id order) that serves the id.
///
/// Rules 3 and 4 are what make an unqualified legacy ref deterministic instead
/// of dependent on map ordering. Returns `None` when no configured provider
/// serves the id at all.
pub fn resolve_aux_model(config: &AppConfig, reference: &AuxModelRef) -> Option<AuxResolution> {
    let model_id = reference.model_id.trim();
    if model_id.is_empty() {
        return None;
    }

    let servers: Vec<&str> = config
        .providers
        .iter()
        .filter(|(_, provider)| provider.models.contains_key(model_id))
        .map(|(id, _)| id.as_str())
        .collect();

    if reference.is_qualified() {
        let provider_id = reference.provider_id.trim();
        if config
            .providers
            .get(provider_id)
            .is_some_and(|provider| provider.models.contains_key(model_id))
        {
            return Some(AuxResolution::Explicit(ModelRef::new(provider_id, model_id)));
        }
        // A named provider that no longer serves the model falls through: the
        // model clearly still exists, so preferring it beats failing outright.
    }

    if servers.is_empty() {
        return None;
    }

    if let [only] = servers.as_slice() {
        return Some(AuxResolution::Unique(ModelRef::new(*only, model_id)));
    }

    let default_provider = config.chat.provider_id.as_deref();
    let chosen = default_provider
        .filter(|candidate| servers.contains(candidate))
        .unwrap_or(servers[0]);

    let others = servers
        .iter()
        .filter(|candidate| **candidate != chosen)
        .map(|candidate| ModelRef::new(*candidate, model_id))
        .collect();

    Some(AuxResolution::Ambiguous {
        chosen: ModelRef::new(chosen, model_id),
        others,
    })
}

/// Aux model refs that cannot be resolved as things stand, for the settings
/// page to report. Each entry is the field label, the ref, and how many
/// providers serve the id.
pub fn unresolved_aux_models(config: &AppConfig) -> Vec<(&'static str, AuxModelRef)> {
    let mut unresolved = Vec::new();
    for (label, reference) in [
        ("imageModel", &config.chat.image_model),
        ("embeddingModel", &config.chat.embedding_model),
    ] {
        if let Some(reference) = reference {
            if !reference.model_id.trim().is_empty()
                && resolve_aux_model(config, reference).is_none()
            {
                unresolved.push((label, reference.clone()));
            }
        }
    }
    unresolved
}

/// `{id}-2`, `{id}-3`, … — the first id not already taken.
fn unique_provider_id(config: &AppConfig, base: &str) -> String {
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|candidate| !config.providers.contains_key(candidate))
        .expect("an unused numeric suffix always exists")
}

/// `"{name} 2"`, … — the first free display name, so two cards built from one
/// preset can be told apart in pickers without opening settings.
fn unique_provider_name(config: &AppConfig, base: &str) -> String {
    let base = base.trim();
    (2..)
        .map(|suffix| format!("{base} {suffix}"))
        .find(|candidate| {
            !config
                .providers
                .values()
                .any(|provider| provider.name == *candidate)
        })
        .expect("an unused numeric suffix always exists")
}

/// Copies a configured provider under a fresh id and display name, returning
/// the new id.
///
/// This is what makes one vendor usable more than once: two OpenCode Go plans,
/// say, each with its own subscription. The copy inherits the endpoint, kind,
/// headers, gateway session header, `preset_id`, the model catalogue **and**
/// the model selection.
///
/// It deliberately inherits **no** API key: keys live in the credential vault
/// under the provider id, so a new id starts keyless and cannot quietly bill
/// the original's account.
pub fn duplicate_provider(config: &mut AppConfig, provider_id: &str) -> Result<String> {
    let source = config
        .providers
        .get(provider_id)
        .cloned()
        .ok_or_else(|| Error::UnknownProvider(provider_id.to_string()))?;

    let new_id = unique_provider_id(config, provider_id);
    let mut copy = source;
    copy.name = unique_provider_name(config, &copy.name);
    // A copied timestamp would claim this instance had already been polled,
    // hiding the fact that its models came from the original.
    copy.last_fetched_at = None;
    config.providers.insert(new_id.clone(), copy);
    Ok(new_id)
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
    /// Model used by the `generate_image` tool. `providerId` is optional: an
    /// unqualified ref runs against whichever provider serves the id (see
    /// [`resolve_aux_model`]), which is what a legacy bare string means.
    pub image_model: Option<AuxModelRef>,
    /// Embedding model used by the workspace index and memory search.
    pub embedding_model: Option<AuxModelRef>,
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
        // `preset_for`, not `preset`: a duplicated provider is stored under a
        // suffixed id that matches no preset, and it still needs the gateway's
        // session header. Resolving by base URL also repairs renamed ids.
        let Some(preset) = crate::provider::preset_for(id, provider) else {
            continue;
        };
        // Provenance is recorded only from an exact id match (`preset_provenance`),
        // never from the base-URL fallback: this runs on every start, so a
        // guess would be written to disk immediately and then outlive any later
        // edit to the URL.
        if provider.preset_id.is_none() {
            if let Some(confirmed) = crate::provider::preset_provenance(id, provider) {
                provider.preset_id = Some(confirmed.to_string());
                changed = true;
            }
        }
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
    /// Hand-placed order of the workspace groups in the chats popup, by folder
    /// path. A group listed here keeps that place; one that is not listed —
    /// including a folder just added — sorts above them by recency. In grouped
    /// mode this and each chat's `position` column are the whole order: the
    /// Sort menu governs the flat list only.
    pub sidebar_workspace_order: Vec<String>,
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
            sidebar_workspace_order: Vec::new(),
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

    // ---------------------------------------------------------- selection

    #[test]
    fn a_provider_saved_before_selection_still_selects_every_model() {
        // The serde trap: `ProviderConfig` carries a container-level
        // `#[serde(default)]`, so every absent field takes its value from the
        // hand-written `Default` impl. Had `auto_select_models` defaulted to
        // `false`, opening an existing config would have silently switched it
        // to opt-in mode and emptied the pickers.
        let provider: ProviderConfig = serde_json::from_str(
            r#"{
                 "name": "OpenCode Go",
                 "baseUrl": "https://opencode.ai/zen/go/v1",
                 "models": { "glm-4.7": {}, "qwen3-coder": {} }
               }"#,
        )
        .unwrap();

        assert!(provider.auto_select_models);
        assert!(provider.disabled_models.is_empty());
        assert!(provider.preset_id.is_none());
        assert!(provider.model_selected("glm-4.7"));
        assert!(provider.model_selected("qwen3-coder"));
    }

    #[test]
    fn duplicating_a_provider_copies_its_catalogue_and_selection() {
        let mut config = AppConfig::default();
        let mut provider = ProviderConfig {
            name: "OpenCode Go".into(),
            base_url: "https://opencode.ai/zen/go/v1".into(),
            preset_id: Some("opencode-go".into()),
            session_header: Some("x-opencode-session".into()),
            ..Default::default()
        };
        for model in ["glm-4.7", "qwen3-coder"] {
            provider.models.insert(model.into(), ModelSpec::default());
        }
        provider.set_model_selected("qwen3-coder", false);
        provider.last_fetched_at = Some(1_700_000_000_000);
        config.providers.insert("opencode-go".into(), provider);

        let new_id = duplicate_provider(&mut config, "opencode-go").unwrap();
        assert_eq!(new_id, "opencode-go-2");

        let copy = &config.providers[&new_id];
        assert_eq!(copy.name, "OpenCode Go 2");
        assert_eq!(copy.base_url, "https://opencode.ai/zen/go/v1");
        // Carried so the second plan keeps its gateway behaviour even though a
        // suffixed id matches no preset by name.
        assert_eq!(copy.preset_id.as_deref(), Some("opencode-go"));
        assert_eq!(copy.session_header.as_deref(), Some("x-opencode-session"));
        assert_eq!(copy.models.len(), 2);
        // The selection comes along, so the second plan starts where the first
        // is rather than offering 300 models again.
        assert!(copy.model_selected("glm-4.7"));
        assert!(!copy.model_selected("qwen3-coder"));
        // A copied timestamp would claim this instance had already been polled.
        assert!(copy.last_fetched_at.is_none());
        // The original is untouched.
        assert_eq!(config.providers["opencode-go"].name, "OpenCode Go");
    }

    #[test]
    fn duplicating_twice_keeps_ids_and_names_apart() {
        let mut config = AppConfig::default();
        config.providers.insert(
            "p".into(),
            ProviderConfig {
                name: "Plan".into(),
                ..Default::default()
            },
        );

        assert_eq!(duplicate_provider(&mut config, "p").unwrap(), "p-2");
        assert_eq!(duplicate_provider(&mut config, "p").unwrap(), "p-3");
        assert_eq!(config.providers["p-2"].name, "Plan 2");
        assert_eq!(config.providers["p-3"].name, "Plan 3");
        assert!(duplicate_provider(&mut config, "nope").is_err());
    }

    // ------------------------------------------------------- aux model refs

    #[test]
    fn a_bare_aux_ref_round_trips_unchanged() {
        // No migration step: a config that never pinned a provider writes back
        // the exact shape it had, so merely saving does not rewrite the file.
        let bare = AuxModelRef::bare("qwen3:8b");
        assert_eq!(serde_json::to_string(&bare).unwrap(), r#""qwen3:8b""#);

        let qualified = AuxModelRef::new("opencode-go-2", "qwen3:8b");
        let text = serde_json::to_string(&qualified).unwrap();
        assert!(text.contains("opencode-go-2"));
        assert_eq!(
            serde_json::from_str::<AuxModelRef>(&text).unwrap(),
            qualified
        );
    }

    #[test]
    fn aux_model_refs_accept_both_shapes() {
        let bare: AuxModelRef =
            serde_json::from_str(r#""text-embedding-3-small""#).unwrap();
        assert_eq!(bare, AuxModelRef::bare("text-embedding-3-small"));
        assert!(!bare.is_qualified());

        let qualified: AuxModelRef =
            serde_json::from_str(r#"{"providerId":"go-2","modelId":"qwen3:8b"}"#).unwrap();
        assert_eq!(qualified, AuxModelRef::new("go-2", "qwen3:8b"));
        assert!(qualified.is_qualified());

        // A null provider is how an unqualified object ref arrives.
        let null_provider: AuxModelRef =
            serde_json::from_str(r#"{"providerId":null,"modelId":"x"}"#).unwrap();
        assert!(!null_provider.is_qualified());
        assert_eq!(null_provider.model_id, "x");

        // A missing model id is an error rather than an empty ref.
        assert!(serde_json::from_str::<AuxModelRef>(r#"{"providerId":"p"}"#).is_err());
    }

    /// One provider per id, each serving the ids listed, with an optional app
    /// default provider.
    fn config_with(entries: &[(&str, &[&str])], default_provider: Option<&str>) -> AppConfig {
        let mut config = AppConfig::default();
        for (id, models) in entries {
            let mut provider = ProviderConfig {
                name: (*id).to_string(),
                ..Default::default()
            };
            for model in *models {
                provider.models.insert((*model).to_string(), ModelSpec::default());
            }
            config.providers.insert((*id).to_string(), provider);
        }
        config.chat.provider_id = default_provider.map(str::to_string);
        config
    }

    #[test]
    fn an_unqualified_ref_with_one_server_resolves_to_it() {
        let config = config_with(&[("a", &["embed"]), ("b", &["other"])], None);
        let resolved = resolve_aux_model(&config, &AuxModelRef::bare("embed")).unwrap();
        assert_eq!(resolved.model(), &ModelRef::new("a", "embed"));
        assert!(!resolved.is_ambiguous());
    }

    #[test]
    fn a_qualified_ref_is_not_second_guessed_by_the_default() {
        // Two plans serve the same id and the app default points at the first.
        // Naming the second must win — otherwise embeddings would move to a
        // different, separately billed account without saying so.
        let config = config_with(&[("go", &["embed"]), ("go-2", &["embed"])], Some("go"));
        let resolved = resolve_aux_model(&config, &AuxModelRef::new("go-2", "embed")).unwrap();
        assert_eq!(resolved.model(), &ModelRef::new("go-2", "embed"));
        assert!(!resolved.is_ambiguous());
    }

    #[test]
    fn an_ambiguous_ref_prefers_the_default_and_reports_the_rest() {
        let config = config_with(&[("go", &["embed"]), ("go-2", &["embed"])], Some("go-2"));
        let resolved = resolve_aux_model(&config, &AuxModelRef::bare("embed")).unwrap();
        assert_eq!(resolved.model(), &ModelRef::new("go-2", "embed"));
        assert!(resolved.is_ambiguous());
        // The losers are named so the UI can say which provider is not in use.
        assert_eq!(resolved.others().to_vec(), vec![ModelRef::new("go", "embed")]);
    }

    #[test]
    fn an_ambiguous_ref_without_a_default_is_deterministic() {
        // Falls back to provider-id order, never to map iteration order.
        let config = config_with(&[("b", &["embed"]), ("a", &["embed"])], None);
        let resolved = resolve_aux_model(&config, &AuxModelRef::bare("embed")).unwrap();
        assert_eq!(resolved.model(), &ModelRef::new("a", "embed"));
    }

    #[test]
    fn a_named_provider_that_dropped_the_model_falls_back() {
        // The model still exists elsewhere, so using it beats failing outright.
        let config = config_with(&[("go", &["other"]), ("go-2", &["embed"])], None);
        let resolved = resolve_aux_model(&config, &AuxModelRef::new("go", "embed")).unwrap();
        assert_eq!(resolved.model(), &ModelRef::new("go-2", "embed"));
    }

    #[test]
    fn a_ref_no_provider_serves_resolves_to_nothing() {
        let config = config_with(&[("a", &["other"])], Some("a"));
        assert!(resolve_aux_model(&config, &AuxModelRef::bare("embed")).is_none());
        // An empty id is "unset", not "the empty model".
        assert!(resolve_aux_model(&config, &AuxModelRef::bare("   ")).is_none());
        assert!(unresolved_aux_models(&config).is_empty());
    }

    #[test]
    fn an_unresolvable_setting_is_reported_for_the_ui() {
        let mut config = config_with(&[("a", &["other"])], Some("a"));
        config.chat.embedding_model = Some(AuxModelRef::bare("gone"));

        let unresolved = unresolved_aux_models(&config);
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].0, "embeddingModel");
        assert_eq!(unresolved[0].1.model_id, "gone");
    }
}
