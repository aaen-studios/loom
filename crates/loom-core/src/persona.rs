//! Personas: named system prompts with optional prompt sections, few-shot
//! examples, capability defaults (model, sampling, modes, tool allowlists) and
//! persistent memory. Managed through the settings UI and stored in the config
//! file.

use serde::{Deserialize, Serialize};

use crate::config::{AgentMode, ModelRef, PermissionMode};

/// How many example exchanges one persona may carry.
pub const MAX_EXAMPLES: usize = 12;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Persona {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub model_ref: Option<ModelRef>,
    pub variant: Option<String>,
    /// One-line summary shown in pickers and tooltips.
    pub description: String,
    /// Free-form labels for search and grouping.
    pub tags: Vec<String>,
    /// Avatar emoji, e.g. "🛠".
    pub emoji: Option<String>,
    /// Avatar colour token, e.g. "violet".
    pub color: Option<String>,
    /// Kokoro voice id for this persona, e.g. `af_heart`.
    ///
    /// `None` uses the global default. An id that does not look like a voice
    /// also falls back, so a typo mutes nothing — see
    /// [`crate::voice::config::VoiceConfig::voice_for`].
    pub voice: Option<String>,
    /// Pinned to the top of the picker.
    pub favorite: bool,
    /// Opening assistant message for chats started with this persona.
    pub greeting: String,
    /// Optional prompt sections, appended after `system_prompt` in order.
    pub style: String,
    pub rules: String,
    pub output_format: String,
    /// Few-shot example turns, appended after the sections.
    pub examples: Vec<PersonaExample>,
    /// Runtime defaults applied when the session has not overridden them.
    pub capabilities: PersonaCapabilities,
    /// Persistent memory across chats.
    pub memory: PersonaMemory,
    /// Bumped on every write; chats snapshot the prompt, so the UI compares
    /// snapshot and live prompt to flag a stale chat.
    pub revision: u32,
    /// Last write (ms since epoch).
    pub updated_at: i64,
}

/// One `user`/`assistant` example turn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonaExample {
    pub user: String,
    pub assistant: String,
}

/// Runtime defaults a persona can apply. Session values always win; these win
/// over the global chat defaults. Capability edits are settings-only — the
/// model never sees these fields through the harness tools.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonaCapabilities {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    /// Reply cap for this persona; the session's chat default still applies
    /// when this is `None`.
    pub max_output_tokens: Option<u32>,
    /// Default permission mode. Atelier is deliberately not accepted here:
    /// it stays a per-chat choice.
    pub permission_mode: Option<PermissionMode>,
    pub agent_mode: Option<AgentMode>,
    /// Tool-name allowlist; empty means every tool the mode offers.
    pub tools: Vec<String>,
    /// MCP server-id allowlist; empty means every connected server.
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonaMemory {
    /// Offer `remember` / `forget` and inject stored entries.
    pub enabled: bool,
    /// Token budget for injected entries; zero uses the default.
    pub token_budget: u32,
}

impl Default for PersonaMemory {
    fn default() -> Self {
        Self {
            enabled: false,
            token_budget: 0,
        }
    }
}

/// Default token budget for injected persona memory.
pub const MEMORY_TOKEN_BUDGET: u32 = 1_200;

/// Values available to `{{variable}}` interpolation.
#[derive(Debug, Clone, Default)]
pub struct PersonaVars {
    pub user_name: String,
    pub user_pronouns: String,
    pub user_about: String,
    pub workdir: String,
    pub model: String,
    pub provider: String,
    pub persona_name: String,
    pub date: String,
}

impl Persona {
    pub fn new(name: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            system_prompt: system_prompt.into(),
            model_ref: None,
            variant: None,
            description: String::new(),
            tags: Vec::new(),
            emoji: None,
            color: None,
            voice: None,
            favorite: false,
            greeting: String::new(),
            style: String::new(),
            rules: String::new(),
            output_format: String::new(),
            examples: Vec::new(),
            capabilities: PersonaCapabilities::default(),
            memory: PersonaMemory::default(),
            revision: 1,
            updated_at: crate::db::now_ms(),
        }
    }

    /// The full system prompt this persona contributes: the main prompt, then
    /// the optional sections, then the example exchanges, with `{{variables}}`
    /// filled in.
    pub fn assemble(&self, vars: &PersonaVars) -> String {
        let mut parts: Vec<String> = Vec::new();
        push_section(&mut parts, self.system_prompt.trim(), None);
        push_section(&mut parts, self.style.trim(), Some("Style"));
        push_section(&mut parts, self.rules.trim(), Some("Rules"));
        push_section(&mut parts, self.output_format.trim(), Some("Output format"));

        let examples: Vec<&PersonaExample> = self
            .examples
            .iter()
            .filter(|example| !example.user.trim().is_empty())
            .collect();
        if !examples.is_empty() {
            let mut text = String::from("Example exchanges:");
            for example in examples {
                text.push_str(&format!(
                    "\nUser: {}\nAssistant: {}",
                    example.user.trim(),
                    example.assistant.trim()
                ));
            }
            parts.push(text);
        }

        interpolate(&parts.join("\n\n"), vars)
    }

    /// Whether this persona offers/uses the `remember` and `forget` tools.
    pub fn memory_enabled(&self) -> bool {
        self.memory.enabled
    }

    /// Prompt-side description of the tool allowlist, if any, so the model
    /// knows why some tools are missing instead of retrying them.
    pub fn tool_notice(&self) -> Option<String> {
        if self.capabilities.tools.is_empty() && self.capabilities.mcp_servers.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if !self.capabilities.tools.is_empty() {
            parts.push(format!(
                "tools: {}",
                self.capabilities.tools.join(", ")
            ));
        }
        if !self.capabilities.mcp_servers.is_empty() {
            parts.push(format!(
                "MCP servers: {}",
                self.capabilities.mcp_servers.join(", ")
            ));
        }
        Some(format!(
            "Tool scope for this persona — only these are available: {}. Do not attempt other tools.",
            parts.join("; ")
        ))
    }
}

fn push_section(parts: &mut Vec<String>, body: &str, label: Option<&str>) {
    if body.is_empty() {
        return;
    }
    match label {
        Some(label) => parts.push(format!("{label}:\n{body}")),
        None => parts.push(body.to_string()),
    }
}

/// Replaces `{{name}}` placeholders. Unknown placeholders are left alone so a
/// typo is visible instead of silently vanishing.
pub fn interpolate(text: &str, vars: &PersonaVars) -> String {
    let pairs = [
        ("user", vars.user_name.as_str()),
        ("userName", vars.user_name.as_str()),
        ("userPronouns", vars.user_pronouns.as_str()),
        ("userAbout", vars.user_about.as_str()),
        ("workdir", vars.workdir.as_str()),
        ("model", vars.model.as_str()),
        ("provider", vars.provider.as_str()),
        ("persona", vars.persona_name.as_str()),
        ("personaName", vars.persona_name.as_str()),
        ("date", vars.date.as_str()),
    ];
    let mut out = text.to_string();
    for (key, value) in pairs {
        if value.is_empty() {
            continue;
        }
        out = out.replace(&format!("{{{{{key}}}}}"), value);
    }
    out
}
