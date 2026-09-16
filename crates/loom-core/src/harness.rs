//! Harness tools: the model editing Loom's own configuration.
//!
//! One owner for harness state. The engine (model-driven edits) and the Tauri
//! command layer (UI-driven edits) both come through here, so validation and
//! the backup rule exist once. The tools are only *offered* in Atelier mode;
//! a call in any other mode is refused by the engine before it gets here.
//!
//! Every mutation is preceded by a snapshot of the config in
//! `~/.loom/backups/`, so a change the user regrets can be recovered by hand
//! even though there is no restore UI yet.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::config::{
    AppConfig, ModelRef, Prompt, SearchProvider, SendKey, Theme, ThinkingDisplay, ToolCallDisplay,
};
use crate::mcp::McpServerConfig;
use crate::persona::Persona;
use crate::provider::{Modality, ProviderConfig, ProviderKind, ReasoningSpec};
use crate::tools::{ToolScope, ToolSpec};
use crate::{fsutil, paths, skills, Error, Result};

pub const LIST_HARNESS: &str = "list_harness";
pub const UPSERT_PERSONA: &str = "upsert_persona";
pub const DELETE_PERSONA: &str = "delete_persona";
pub const UPSERT_PROMPT: &str = "upsert_prompt";
pub const DELETE_PROMPT: &str = "delete_prompt";
pub const WRITE_SKILL: &str = "write_skill";
pub const DELETE_SKILL: &str = "delete_skill";
pub const UPSERT_MCP_SERVER: &str = "upsert_mcp_server";
pub const DELETE_MCP_SERVER: &str = "delete_mcp_server";
pub const TEST_MCP_SERVER: &str = "test_mcp_server";
pub const UPSERT_PROVIDER: &str = "upsert_provider";
pub const UPDATE_MODEL: &str = "update_model";
pub const DELETE_PROVIDER: &str = "delete_provider";
pub const UPDATE_SETTINGS: &str = "update_settings";

/// The sections `list_harness` can return and `view` builds.
pub const SECTIONS: [&str; 6] = [
    "personas",
    "mcp",
    "skills",
    "prompts",
    "providers",
    "settings",
];

/// The five calls that ask for confirmation even in Atelier; everything else
/// runs silently, like Auto all.
const DESTRUCTIVE: [&str; 5] = [
    DELETE_PERSONA,
    DELETE_SKILL,
    DELETE_PROMPT,
    DELETE_PROVIDER,
    DELETE_MCP_SERVER,
];

const MAX_NAME_CHARS: usize = 80;
const MAX_PERSONA_PROMPT: usize = 32 * 1024;
const MAX_PROMPT_BODY: usize = 32 * 1024;

/// How much of a persona prompt or prompt body `view` returns.
const VIEW_CHARS: usize = 2_000;

/// Config snapshots kept in `~/.loom/backups`.
const KEEP_BACKUPS: usize = 10;

/// Writable keys of `update_settings`, deny-by-default.
const INTERFACE_KEYS: [&str; 10] = [
    "showThinking",
    "showToolCalls",
    "sendKey",
    "notifyOnCompletion",
    "alwaysFollow",
    "compact",
    "generatedUi",
    "captureOnSend",
    "sidebarPinned",
    "sidebarWidth",
];
const CHAT_KEYS: [&str; 8] = [
    "maxOutputTokens",
    "autoTitle",
    "embeddingModel",
    "imageModel",
    "lite",
    "computerVariant",
    "computerModel",
    "computerScreenshotEdge",
];

/// Tool specs, in the house style of `tools::specs()`.
pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: LIST_HARNESS,
            description: "Read Loom's harness: personas, MCP servers, skills, prompts, providers and models, and settings. Call it before editing so you start from the current state. Long persona and prompt bodies are truncated; MCP environment values and API keys never appear.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "sections": {
                        "type": "array",
                        "description": "Sections to return; omit for all of them",
                        "items": {
                            "type": "string",
                            "enum": ["personas", "mcp", "skills", "prompts", "providers", "settings"]
                        }
                    }
                },
                "additionalProperties": false
            }),
            read_only: true,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: UPSERT_PERSONA,
            description: "Create or update a persona (a named system prompt offered in the composer). Pass an existing id to update it, or omit id to create one. Chats that already selected the persona keep the prompt they snapshotted. You may write the voice: description, tags, emoji, color, favorite, greeting, style, rules, outputFormat and examples. Sampling, permission, memory and tool-scope settings are the user's to choose in Settings and are not accepted here.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Existing persona id; omit to create" },
                    "name": { "type": "string", "description": "1-80 characters" },
                    "systemPrompt": { "type": "string", "description": "The system prompt (max 32 KiB)" },
                    "description": { "type": "string", "description": "One-line summary for the picker" },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Labels for search"
                    },
                    "emoji": { "type": "string", "description": "Single avatar emoji" },
                    "color": { "type": "string", "description": "Avatar colour token, e.g. violet" },
                    "favorite": { "type": "boolean", "description": "Pin to the top of the picker" },
                    "greeting": { "type": "string", "description": "Opening assistant message for new chats" },
                    "style": { "type": "string", "description": "Optional Style section appended to the prompt" },
                    "rules": { "type": "string", "description": "Optional Rules section appended to the prompt" },
                    "outputFormat": { "type": "string", "description": "Optional Output format section appended to the prompt" },
                    "examples": {
                        "type": "array",
                        "description": "Few-shot example turns",
                        "items": {
                            "type": "object",
                            "properties": {
                                "user": { "type": "string" },
                                "assistant": { "type": "string" }
                            },
                            "required": ["user", "assistant"],
                            "additionalProperties": false
                        }
                    },
                    "modelRef": {
                        "type": ["object", "null"],
                        "properties": {
                            "providerId": { "type": "string" },
                            "modelId": { "type": "string" }
                        },
                        "required": ["providerId", "modelId"],
                        "additionalProperties": false
                    },
                    "variant": { "type": ["string", "null"], "description": "Reasoning variant, e.g. high" }
                },
                "required": ["name", "systemPrompt"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: DELETE_PERSONA,
            description: "Delete a persona by id. Chats that already snapshotted its prompt keep working. This always asks the user for confirmation.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: UPSERT_PROMPT,
            description: "Create or update a reusable prompt snippet, offered in the composer's / menu next to skills. Pass an existing id to update, or omit id to create one.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Existing prompt id; omit to create" },
                    "title": { "type": "string", "description": "1-80 characters" },
                    "body": { "type": "string", "description": "The prompt text (max 32 KiB)" }
                },
                "required": ["title", "body"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: DELETE_PROMPT,
            description: "Delete a prompt snippet by id. This always asks the user for confirmation.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: WRITE_SKILL,
            description: "Write a skill to ~/.loom/skills/<id>.md: a short name, a one-line description, and the prompt body. Skills appear in the composer's / menu. Overwriting an existing skill backs the old file up first; lowercase letters, digits and dashes only in the id.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "File name, e.g. review" },
                    "name": { "type": "string", "description": "Display name; defaults to the id" },
                    "description": { "type": "string", "description": "One line shown in the / menu" },
                    "body": { "type": "string", "description": "The prompt text (max 64 KiB)" }
                },
                "required": ["id", "body"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: DELETE_SKILL,
            description: "Delete a skill's markdown file. A missing file comes back as an error you can read. This always asks the user for confirmation.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: UPSERT_MCP_SERVER,
            description: "Create or update a stdio MCP server. `command` is the executable and `args` its arguments. Omitting env keeps the existing environment; env values are never shown by list_harness, so leave env alone unless you are replacing it. Server ids become part of mcp__<id>__<tool>, so they may not contain `__`.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Lowercase letters, digits, _ and -; no __" },
                    "name": { "type": "string", "description": "Display name; defaults to the id" },
                    "command": { "type": "string", "description": "Executable to spawn" },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "env": {
                        "type": "object",
                        "description": "Extra environment variables; omit to keep the current ones",
                        "additionalProperties": { "type": "string" }
                    },
                    "enabled": { "type": "boolean" }
                },
                "required": ["id", "command"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: DELETE_MCP_SERVER,
            description: "Delete an MCP server. Its tools disappear on the next turn. This always asks the user for confirmation.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: TEST_MCP_SERVER,
            description: "Start the named MCP server once, list its tools, then shut it down. Use it after writing a server to prove the command works; a spawn failure comes back as the error text.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: UPSERT_PROVIDER,
            description: "Create or update a model provider: base URL, protocol kind, extra headers, whether it needs a key. It never accepts an API key — ask the user to paste it in Settings → Providers. openai-compatible is right for most endpoints; anthropic is the native Messages API.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Provider id used in model references" },
                    "name": { "type": "string" },
                    "baseUrl": { "type": "string", "description": "http:// or https://" },
                    "kind": { "type": "string", "enum": ["openai-compatible", "anthropic"] },
                    "enabled": { "type": "boolean" },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    },
                    "keyRequired": { "type": "boolean" }
                },
                "required": ["id", "name", "baseUrl"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: UPDATE_MODEL,
            description: "Add a model id to a provider by hand, edit its metadata, mark it a favourite, remove it, or reset it to what the bundled catalog detects. Editing context, output, inputModalities, or reasoning stamps the spec as the user's, so later refreshes never overwrite it. Pass null for a field to clear it; omitted fields are left alone.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "providerId": { "type": "string" },
                    "modelId": { "type": "string" },
                    "context": { "type": ["integer", "null"], "description": "Context window in tokens; null clears it" },
                    "output": { "type": ["integer", "null"], "description": "Max output in tokens; null clears it" },
                    "inputModalities": {
                        "type": ["array", "null"],
                        "items": { "type": "string", "enum": ["text", "image", "audio", "video", "pdf"] },
                        "description": "Input types the model accepts; null or [] means unknown"
                    },
                    "reasoning": {
                        "type": ["object", "null"],
                        "properties": {
                            "enabled": { "type": "boolean" },
                            "variants": { "type": "array", "items": { "type": "string" } },
                            "defaultVariant": { "type": ["string", "null"] }
                        },
                        "additionalProperties": false,
                        "description": "Thinking support; null clears it"
                    },
                    "favorite": { "type": "boolean" },
                    "reset": { "type": "boolean", "description": "Forget detected values and re-read the bundled catalog (keeps the favourite flag)" },
                    "remove": { "type": "boolean", "description": "Remove the model instead of editing it" }
                },
                "required": ["providerId", "modelId"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: DELETE_PROVIDER,
            description: "Delete a provider. Refused when it is the last enabled one, because Loom would have no model to reach. This always asks the user for confirmation.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: UPDATE_SETTINGS,
            description: "Change app-wide settings. Deny by default: an unknown key is refused with the allowed list. Allowed: theme, searchProvider, interface {showThinking, showToolCalls, sendKey, notifyOnCompletion, alwaysFollow, compact, generatedUi, captureOnSend, sidebarPinned, sidebarWidth}, chat {maxOutputTokens, autoTitle, embeddingModel, imageModel, lite, computerVariant, computerModel, computerScreenshotEdge}. Deliberately excluded: the global hotkey and hotkeyEnabled (re-registering needs the app shell; the user changes it in Settings → General), background (a picked file must be copied into ~/.loom/backgrounds first, which Settings does), and API keys (they live in Windows Credential Manager).",
            parameters: json!({
                "type": "object",
                "properties": {
                    "theme": { "type": "string", "enum": ["light", "dark"] },
                    "searchProvider": { "type": "string", "enum": ["auto", "jina", "duckduckgo"] },
                    "interface": {
                        "type": "object",
                        "properties": {
                            "showThinking": { "type": "string", "enum": ["collapsed", "hidden", "expanded"] },
                            "showToolCalls": { "type": "string", "enum": ["collapsed", "expanded", "hidden"] },
                            "sendKey": { "type": "string", "enum": ["enter", "ctrl-enter"] },
                            "notifyOnCompletion": { "type": "boolean" },
                            "alwaysFollow": { "type": "boolean" },
                            "compact": { "type": "boolean" },
                            "generatedUi": { "type": "boolean" },
                            "captureOnSend": { "type": "boolean" },
                            "sidebarPinned": { "type": "boolean" },
                            "sidebarWidth": { "type": "integer", "minimum": 0 }
                        },
                        "additionalProperties": false
                    },
                    "chat": {
                        "type": "object",
                        "properties": {
                            "maxOutputTokens": { "type": "integer", "description": "0 keeps the model's own limit; otherwise clamped to 256-200000" },
                            "autoTitle": { "type": "boolean" },
                            "embeddingModel": { "type": ["string", "null"] },
                            "imageModel": { "type": ["string", "null"] },
                            "lite": {
                                "type": ["object", "null"],
                                "properties": {
                                    "providerId": { "type": "string" },
                                    "modelId": { "type": "string" }
                                },
                                "required": ["providerId", "modelId"],
                                "additionalProperties": false
                            },
                            "computerVariant": { "type": ["string", "null"], "enum": ["off", "low", "medium", "high", null], "description": "Thinking effort on computer turns; null inherits the usual chain" },
                            "computerModel": {
                                "type": ["object", "null"],
                                "properties": {
                                    "providerId": { "type": "string" },
                                    "modelId": { "type": "string" }
                                },
                                "required": ["providerId", "modelId"],
                                "additionalProperties": false
                            },
                            "computerScreenshotEdge": { "type": "integer", "description": "Longest screenshot edge; 0 is native resolution" }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
    ]
}

pub fn is_harness_tool(name: &str) -> bool {
    matches!(
        name,
        LIST_HARNESS
            | UPSERT_PERSONA
            | DELETE_PERSONA
            | UPSERT_PROMPT
            | DELETE_PROMPT
            | WRITE_SKILL
            | DELETE_SKILL
            | UPSERT_MCP_SERVER
            | DELETE_MCP_SERVER
            | TEST_MCP_SERVER
            | UPSERT_PROVIDER
            | UPDATE_MODEL
            | DELETE_PROVIDER
            | UPDATE_SETTINGS
    )
}

/// The one harness tool that only reads.
pub fn is_harness_read(name: &str) -> bool {
    name == LIST_HARNESS
}

/// The five deletes that still ask for confirmation in Atelier.
pub fn is_destructive(name: &str) -> bool {
    DESTRUCTIVE.contains(&name)
}

/// Which transcript section a tool changed, for the `HarnessChanged` event.
pub fn section_of(name: &str) -> String {
    match name {
        UPSERT_PERSONA | DELETE_PERSONA => "personas",
        UPSERT_PROMPT | DELETE_PROMPT => "prompts",
        WRITE_SKILL | DELETE_SKILL => "skills",
        UPSERT_MCP_SERVER | DELETE_MCP_SERVER | TEST_MCP_SERVER => "mcp",
        UPSERT_PROVIDER | UPDATE_MODEL | DELETE_PROVIDER => "providers",
        UPDATE_SETTINGS => "settings",
        _ => "harness",
    }
    .to_string()
}

/// What the model is told when it calls a harness tool outside Atelier. The
/// point is that the next attempt can find the mode.
pub fn refusal(name: &str) -> String {
    format!(
        "`{name}` edits Loom's own harness, which is only available in Atelier mode. \
         Tell the user to switch this chat's permission chip to Atelier, then call it again."
    )
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

/// Bounded JSON view of the whole harness.
pub fn view(config: &AppConfig) -> Result<Value> {
    view_sections(config, &[])
}

/// Same, restricted to `sections` (empty means all). Unknown names are an
/// error the model can read.
pub fn view_sections(config: &AppConfig, sections: &[String]) -> Result<Value> {
    for section in sections {
        if !SECTIONS.contains(&section.as_str()) {
            return Err(Error::other(format!(
                "unknown harness section \"{section}\"; known sections: {}",
                SECTIONS.join(", ")
            )));
        }
    }
    let wanted = |name: &str| sections.is_empty() || sections.iter().any(|section| section == name);

    let mut out = Map::new();
    if wanted("personas") {
        out.insert(
            "personas".into(),
            Value::Array(config.personas.iter().map(persona_view).collect()),
        );
    }
    if wanted("mcp") {
        out.insert(
            "mcp".into(),
            Value::Array(
                config
                    .mcp_servers
                    .iter()
                    .map(|(id, server)| mcp_view(id, server))
                    .collect(),
            ),
        );
    }
    if wanted("skills") {
        let skills = skills::list()?;
        out.insert(
            "skills".into(),
            Value::Array(skills.iter().map(skill_view).collect()),
        );
    }
    if wanted("prompts") {
        out.insert(
            "prompts".into(),
            Value::Array(config.prompts.iter().map(prompt_view).collect()),
        );
    }
    if wanted("providers") {
        out.insert(
            "providers".into(),
            Value::Array(
                config
                    .providers
                    .iter()
                    .map(|(id, provider)| provider_view(id, provider))
                    .collect(),
            ),
        );
    }
    if wanted("settings") {
        out.insert("settings".into(), settings_view(config));
    }
    Ok(Value::Object(out))
}

fn body_view(body: &str) -> Value {
    let chars = body.chars().count();
    if chars <= VIEW_CHARS {
        return json!({ "text": body });
    }
    json!({
        "text": body.chars().take(VIEW_CHARS).collect::<String>(),
        "truncated": true,
        "fullLength": chars,
    })
}

fn persona_view(persona: &Persona) -> Value {
    json!({
        "id": persona.id,
        "name": persona.name,
        "prompt": body_view(&persona.system_prompt),
        "description": persona.description,
        "tags": persona.tags,
        "emoji": persona.emoji,
        "color": persona.color,
        "favorite": persona.favorite,
        "greeting": persona.greeting,
        "style": body_view(&persona.style),
        "rules": body_view(&persona.rules),
        "outputFormat": body_view(&persona.output_format),
        "examples": persona.examples,
        "modelRef": persona.model_ref,
        "variant": persona.variant,
        "revision": persona.revision,
        "updatedAt": persona.updated_at,
        "memoryEnabled": persona.memory.enabled,
        // Capability fields are user-owned; shown so the model can respect
        // them, never writable through this tool.
        "capabilities": persona.capabilities,
    })
}

fn prompt_view(prompt: &Prompt) -> Value {
    json!({
        "id": prompt.id,
        "title": prompt.title,
        "body": body_view(&prompt.body),
    })
}

fn skill_view(skill: &skills::Skill) -> Value {
    json!({
        "id": skill.id,
        "name": skill.name,
        "description": skill.description,
    })
}

fn mcp_view(id: &str, server: &McpServerConfig) -> Value {
    // Environment values may be tokens; only the names are shown.
    let mut env_keys: Vec<&String> = server.env.keys().collect();
    env_keys.sort();
    json!({
        "id": id,
        "name": server.name,
        "command": server.command,
        "args": server.args,
        "envKeys": env_keys,
        "enabled": server.enabled,
    })
}

fn provider_view(id: &str, provider: &ProviderConfig) -> Value {
    let models: Vec<Value> = provider
        .models
        .iter()
        .map(|(model_id, spec)| {
            json!({
                "id": model_id,
                "name": spec.name,
                "context": spec.context,
                "output": spec.output,
                "inputModalities": spec.input_modalities,
                "reasoning": spec.reasoning,
                "favorite": spec.favorite,
                "source": spec.source,
            })
        })
        .collect();
    json!({
        "id": id,
        "name": provider.name,
        "kind": provider.kind,
        "baseUrl": provider.base_url,
        "enabled": provider.enabled,
        "keyRequired": provider.key_required,
        "headers": provider.headers,
        "models": models,
    })
}

fn settings_view(config: &AppConfig) -> Value {
    json!({
        "theme": config.theme,
        "searchProvider": config.search_provider,
        "interface": {
            "showThinking": config.interface.show_thinking,
            "showToolCalls": config.interface.show_tool_calls,
            "sendKey": config.interface.send_key,
            "notifyOnCompletion": config.interface.notify_on_completion,
            "alwaysFollow": config.interface.always_follow,
            "compact": config.interface.compact,
            "generatedUi": config.interface.generated_ui,
            "captureOnSend": config.interface.capture_on_send,
            "sidebarPinned": config.interface.sidebar_pinned,
            "sidebarWidth": config.interface.sidebar_width,
        },
        "chat": {
            "maxOutputTokens": config.chat.max_output_tokens,
            "maxToolRounds": config.chat.max_tool_rounds,
            "autoTitle": config.chat.auto_title,
            "embeddingModel": config.chat.embedding_model,
            "imageModel": config.chat.image_model,
            "lite": config.chat.lite,
            "computerVariant": config.chat.computer_variant,
            "computerModel": config.chat.computer_model,
            "computerScreenshotEdge": config.chat.computer_screenshot_edge,
        },
    })
}

// ---------------------------------------------------------------------------
// Backups
// ---------------------------------------------------------------------------

/// Snapshots the current config to `~/.loom/backups/config-<utc>.json` and
/// keeps only the newest ten. Returns the file written.
pub fn backup(config: &AppConfig) -> Result<Option<PathBuf>> {
    let directory = paths::backups_dir()?;
    std::fs::create_dir_all(&directory).map_err(|e| Error::io(&directory, e))?;
    let path = directory.join(format!("config-{}.json", fsutil::utc_stamp()));

    let mut json = serde_json::to_string_pretty(config).map_err(|e| Error::json(&path, e))?;
    json.push('\n');
    fsutil::atomic_write(&path, json.as_bytes())?;

    prune_backups(&directory);
    Ok(Some(path))
}

fn prune_backups(directory: &Path) {
    let Ok(reader) = std::fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<PathBuf> = reader
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("config-") && name.ends_with(".json"))
        })
        .collect();

    if files.len() <= KEEP_BACKUPS {
        return;
    }
    // Fixed-width UTC names sort chronologically.
    files.sort();
    for path in &files[..files.len() - KEEP_BACKUPS] {
        let _ = std::fs::remove_file(path);
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// The dispatcher: name -> mutation. Returns a one-line human summary for the
/// transcript row and the `HarnessChanged` event. Foreign names are an error.
///
/// This is the *model* path: capability and memory fields are ignored here, so
/// Atelier can write a persona's voice but never its powers.
pub fn apply(config: &mut AppConfig, name: &str, args: &Value) -> Result<String> {
    apply_with(config, name, args, false)
}

/// The UI path: like [`apply`], but persona capability and memory fields are
/// honoured. Only the settings commands call this.
pub fn apply_ui(config: &mut AppConfig, name: &str, args: &Value) -> Result<String> {
    apply_with(config, name, args, true)
}

fn apply_with(config: &mut AppConfig, name: &str, args: &Value, ui: bool) -> Result<String> {
    match name {
        UPSERT_PERSONA => upsert_persona(config, args, ui),
        DELETE_PERSONA => delete_persona(config, args),
        UPSERT_PROMPT => upsert_prompt(config, args),
        DELETE_PROMPT => delete_prompt(config, args),
        WRITE_SKILL => write_skill(config, args),
        DELETE_SKILL => delete_skill(config, args),
        UPSERT_MCP_SERVER => upsert_mcp_server(config, args),
        DELETE_MCP_SERVER => delete_mcp_server(config, args),
        UPSERT_PROVIDER => upsert_provider(config, args),
        UPDATE_MODEL => update_model(config, args),
        DELETE_PROVIDER => delete_provider(config, args),
        UPDATE_SETTINGS => update_settings(config, args),
        other => Err(Error::other(format!("unknown harness tool: {other}"))),
    }
}

// ---------------------------------------------------------------------------
// Personas and prompts
// ---------------------------------------------------------------------------

pub fn upsert_persona(config: &mut AppConfig, args: &Value, ui: bool) -> Result<String> {
    let name = required_arg(args, UPSERT_PERSONA, "name")?;
    check_name("persona", &name)?;
    let prompt = string_arg(args, "systemPrompt").unwrap_or_default();
    if prompt.len() > MAX_PERSONA_PROMPT {
        return Err(Error::other(format!(
            "systemPrompt is {} bytes, larger than the {MAX_PERSONA_PROMPT} byte limit",
            prompt.len()
        )));
    }
    let model_ref = match args.get("modelRef") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            serde_json::from_value::<ModelRef>(value.clone())
                .map_err(|_| Error::other("modelRef needs providerId and modelId"))?,
        ),
    };
    let variant = trimmed_arg(args, "variant");
    let id = trimmed_arg(args, "id").unwrap_or_default();

    if id.is_empty() {
        let mut persona = Persona::new(name.clone(), prompt);
        persona.model_ref = model_ref;
        persona.variant = variant;
        apply_persona_voice_fields(&mut persona, args);
        if ui {
            apply_persona_power_fields(&mut persona, args);
        }
        config.personas.push(persona);
        return Ok(format!("Created persona \"{name}\""));
    }

    let persona = config
        .personas
        .iter_mut()
        .find(|persona| persona.id == id)
        .ok_or_else(|| {
            Error::other(format!(
                "no persona with id \"{id}\"; omit id to create one"
            ))
        })?;
    persona.name = name.clone();
    persona.system_prompt = prompt;
    persona.model_ref = model_ref;
    persona.variant = variant;
    apply_persona_voice_fields(persona, args);
    if ui {
        apply_persona_power_fields(persona, args);
    }
    persona.revision = persona.revision.saturating_add(1);
    persona.updated_at = crate::db::now_ms();
    Ok(format!("Updated persona \"{name}\""))
}

/// A persona's voice: prompt sections, examples and presentation. Both the
/// model (in Atelier) and the settings UI may write these.
fn apply_persona_voice_fields(persona: &mut Persona, args: &Value) {
    if args.get("description").is_some() {
        persona.description = string_arg(args, "description")
            .unwrap_or_default()
            .trim()
            .to_string();
    }
    if args.get("tags").is_some() {
        persona.tags = string_list_arg(args, "tags");
    }
    if args.get("emoji").is_some() {
        persona.emoji = trimmed_arg(args, "emoji").filter(|value| !value.is_empty());
    }
    if args.get("color").is_some() {
        persona.color = trimmed_arg(args, "color").filter(|value| !value.is_empty());
    }
    if let Some(favorite) = args.get("favorite").and_then(Value::as_bool) {
        persona.favorite = favorite;
    }
    if args.get("greeting").is_some() {
        persona.greeting = string_arg(args, "greeting").unwrap_or_default();
    }
    if args.get("style").is_some() {
        persona.style = string_arg(args, "style").unwrap_or_default();
    }
    if args.get("rules").is_some() {
        persona.rules = string_arg(args, "rules").unwrap_or_default();
    }
    if args.get("outputFormat").is_some() {
        persona.output_format = string_arg(args, "outputFormat").unwrap_or_default();
    }
    if args.get("examples").is_some() {
        persona.examples = parse_examples(args);
    }
}

/// A persona's powers: sampling, modes, tool scope and memory. Only the
/// settings UI calls this, so Atelier cannot grant itself permissions, tools,
/// or model settings through a persona.
fn apply_persona_power_fields(persona: &mut Persona, args: &Value) {
    if args.get("capabilities").is_some() {
        let mut capabilities: crate::persona::PersonaCapabilities =
            serde_json::from_value(args["capabilities"].clone()).unwrap_or_default();
        // Atelier stays a per-chat choice, even from the UI.
        if capabilities.permission_mode == Some(crate::config::PermissionMode::Atelier) {
            capabilities.permission_mode = None;
        }
        persona.capabilities = capabilities;
    }
    if args.get("memory").is_some() {
        persona.memory = serde_json::from_value(args["memory"].clone()).unwrap_or_default();
    }
    if persona.updated_at == 0 {
        persona.revision = 1;
        persona.updated_at = crate::db::now_ms();
    }
}

fn string_list_arg(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        Some(Value::String(text)) => text
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_examples(args: &Value) -> Vec<crate::persona::PersonaExample> {
    args.get("examples")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .take(crate::persona::MAX_EXAMPLES)
                .collect()
        })
        .unwrap_or_default()
}

pub fn delete_persona(config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, DELETE_PERSONA, "id")?;
    let name = config
        .personas
        .iter()
        .find(|persona| persona.id == id)
        .map(|persona| persona.name.clone())
        .ok_or_else(|| Error::other(format!("no persona with id \"{id}\"")))?;
    config.personas.retain(|persona| persona.id != id);
    // A deleted persona must not linger in a group's roster or a cast.
    for group in &mut config.persona_groups {
        group.members.retain(|member| member != &id);
    }
    Ok(format!("Deleted persona \"{name}\""))
}

pub fn upsert_prompt(config: &mut AppConfig, args: &Value) -> Result<String> {
    let title = required_arg(args, UPSERT_PROMPT, "title")?;
    check_name("prompt", &title)?;
    let body = string_arg(args, "body").unwrap_or_default();
    if body.len() > MAX_PROMPT_BODY {
        return Err(Error::other(format!(
            "body is {} bytes, larger than the {MAX_PROMPT_BODY} byte limit",
            body.len()
        )));
    }
    let id = trimmed_arg(args, "id").unwrap_or_default();

    if id.is_empty() {
        config.prompts.push(Prompt {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.clone(),
            body,
        });
        return Ok(format!("Created prompt \"{title}\""));
    }

    let prompt = config
        .prompts
        .iter_mut()
        .find(|prompt| prompt.id == id)
        .ok_or_else(|| {
            Error::other(format!("no prompt with id \"{id}\"; omit id to create one"))
        })?;
    prompt.title = title.clone();
    prompt.body = body;
    Ok(format!("Updated prompt \"{title}\""))
}

pub fn delete_prompt(config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, DELETE_PROMPT, "id")?;
    let title = config
        .prompts
        .iter()
        .find(|prompt| prompt.id == id)
        .map(|prompt| prompt.title.clone())
        .ok_or_else(|| Error::other(format!("no prompt with id \"{id}\"")))?;
    config.prompts.retain(|prompt| prompt.id != id);
    Ok(format!("Deleted prompt \"{title}\""))
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

pub fn write_skill(_config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, WRITE_SKILL, "id")?;
    let name = trimmed_arg(args, "name").unwrap_or_else(|| id.clone());
    check_name("skill", &name)?;
    let description = string_arg(args, "description").unwrap_or_default();
    let body = string_arg(args, "body").unwrap_or_default();
    skills::write(&id, &name, &description, &body)?;
    Ok(format!("Wrote skill /{id}"))
}

pub fn delete_skill(_config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, DELETE_SKILL, "id")?;
    skills::delete(&id)?;
    Ok(format!("Deleted skill /{id}"))
}

// ---------------------------------------------------------------------------
// MCP servers
// ---------------------------------------------------------------------------

pub fn upsert_mcp_server(config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, UPSERT_MCP_SERVER, "id")?;
    validate_mcp_id(&id)?;
    let command = required_arg(args, UPSERT_MCP_SERVER, "command")?;
    let name = trimmed_arg(args, "name").unwrap_or_else(|| id.clone());

    let mut server_args: Vec<String> = Vec::new();
    if let Some(value) = args.get("args") {
        let items = value
            .as_array()
            .ok_or_else(|| Error::other("\"args\" must be an array of strings"))?;
        for item in items {
            server_args.push(
                item.as_str()
                    .ok_or_else(|| Error::other("\"args\" must be an array of strings"))?
                    .to_string(),
            );
        }
    }

    // Absent env keeps the existing environment; `list_harness` never shows
    // values, so replacing them costs the user something.
    let env = match args.get("env") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => {
            let mut env = HashMap::new();
            for (key, value) in map {
                if key.trim().is_empty() {
                    return Err(Error::other("environment variable names must not be empty"));
                }
                env.insert(
                    key.clone(),
                    value
                        .as_str()
                        .ok_or_else(|| {
                            Error::other(format!(
                                "environment value for \"{key}\" must be a string"
                            ))
                        })?
                        .to_string(),
                );
            }
            Some(env)
        }
        Some(_) => return Err(Error::other("\"env\" must be an object of string values")),
    };
    let enabled = bool_arg(args, "enabled")?;

    let existed = config.mcp_servers.contains_key(&id);
    let entry = config.mcp_servers.entry(id.clone()).or_default();
    entry.name = name;
    entry.command = command;
    entry.args = server_args;
    if let Some(env) = env {
        entry.env = env;
    }
    if let Some(enabled) = enabled {
        entry.enabled = enabled;
    }
    Ok(format!(
        "{} MCP server \"{id}\"",
        if existed { "Updated" } else { "Added" }
    ))
}

pub fn delete_mcp_server(config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, DELETE_MCP_SERVER, "id")?;
    if config.mcp_servers.remove(&id).is_none() {
        return Err(Error::other(format!("no MCP server with id \"{id}\"")));
    }
    Ok(format!("Deleted MCP server \"{id}\""))
}

/// The id becomes part of `mcp__<id>__<tool>`, so `__`, whitespace, uppercase
/// and a trailing dash would all corrupt `mcp::parse_tool_name`.
fn validate_mcp_id(id: &str) -> Result<()> {
    let mut chars = id.chars();
    let valid = match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {
            id.len() <= 48
                && !id.contains("__")
                && !id.ends_with('-')
                && chars
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        }
        _ => false,
    };
    if valid {
        return Ok(());
    }
    Err(Error::other(format!(
        "invalid MCP server id \"{id}\": use lowercase letters, digits, `_` and `-`, start \
         with a letter or digit, never contain `__`, and do not end with `-`"
    )))
}

// ---------------------------------------------------------------------------
// Providers and models
// ---------------------------------------------------------------------------

pub fn upsert_provider(config: &mut AppConfig, args: &Value) -> Result<String> {
    if args.get("key").is_some() || args.get("apiKey").is_some() || args.get("api_key").is_some() {
        return Err(Error::other(
            "upsert_provider never accepts an API key: keys live in Windows Credential Manager. \
             Ask the user to paste it in Settings → Providers.",
        ));
    }
    let id = required_arg(args, UPSERT_PROVIDER, "id")?;
    let name = required_arg(args, UPSERT_PROVIDER, "name")?;
    let base_url = required_arg(args, UPSERT_PROVIDER, "baseUrl")?;
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err(Error::other(format!(
            "baseUrl \"{base_url}\" must start with http:// or https://"
        )));
    }
    let kind = match args.get("kind") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => match value.as_str() {
            "openai-compatible" => Some(ProviderKind::OpenaiCompatible),
            "anthropic" => Some(ProviderKind::Anthropic),
            other => {
                return Err(Error::other(format!(
                    "unknown provider kind \"{other}\"; use \"openai-compatible\" or \"anthropic\""
                )))
            }
        },
        Some(_) => return Err(Error::other("\"kind\" must be a string")),
    };
    let enabled = bool_arg(args, "enabled")?;
    let key_required = bool_arg(args, "keyRequired")?;
    let headers = match args.get("headers") {
        None | Some(Value::Null) => None,
        Some(Value::Object(map)) => {
            let mut headers = BTreeMap::new();
            for (key, value) in map {
                if key.trim().is_empty() {
                    return Err(Error::other("header names must not be empty"));
                }
                headers.insert(
                    key.clone(),
                    value
                        .as_str()
                        .ok_or_else(|| Error::other(format!("header \"{key}\" must be a string")))?
                        .to_string(),
                );
            }
            Some(headers)
        }
        Some(_) => {
            return Err(Error::other(
                "\"headers\" must be an object of string values",
            ))
        }
    };

    let existed = config.providers.contains_key(&id);
    let provider = config.providers.entry(id.clone()).or_default();
    provider.name = name;
    provider.base_url = base_url;
    if let Some(kind) = kind {
        provider.kind = kind;
    }
    if let Some(enabled) = enabled {
        provider.enabled = enabled;
    }
    if let Some(key_required) = key_required {
        provider.key_required = key_required;
    }
    if let Some(headers) = headers {
        provider.headers = headers;
    }
    Ok(format!(
        "{} provider \"{id}\"",
        if existed { "Updated" } else { "Added" }
    ))
}

pub fn update_model(config: &mut AppConfig, args: &Value) -> Result<String> {
    let provider_id = required_arg(args, UPDATE_MODEL, "providerId")?;
    let model_id = required_arg(args, UPDATE_MODEL, "modelId")?;
    let provider = config
        .providers
        .get_mut(&provider_id)
        .ok_or_else(|| Error::other(format!("unknown provider \"{provider_id}\"")))?;

    if bool_arg(args, "remove")? == Some(true) {
        provider.models.remove(&model_id).ok_or_else(|| {
            Error::other(format!(
                "unknown model \"{model_id}\" on provider \"{provider_id}\""
            ))
        })?;
        return Ok(format!(
            "Removed model \"{model_id}\" from \"{provider_id}\""
        ));
    }

    if bool_arg(args, "reset")? == Some(true) {
        let favorite = provider
            .models
            .get(&model_id)
            .map(|spec| spec.favorite)
            .unwrap_or(false);
        let mut detected =
            crate::catalog::lookup(&model_id).unwrap_or_else(crate::catalog::fallback);
        detected.favorite = favorite;
        let source = detected.source;
        provider.models.insert(model_id.clone(), detected);
        return Ok(format!(
            "Reset model \"{model_id}\" on \"{provider_id}\" to detected metadata ({source})"
        ));
    }

    let context = clearable_u32(args, "context")?;
    let output = clearable_u32(args, "output")?;
    let favorite = bool_arg(args, "favorite")?;
    let modalities = args.get("inputModalities");
    let reasoning = args.get("reasoning");

    let existed = provider.models.contains_key(&model_id);
    let spec = provider.models.entry(model_id.clone()).or_insert_with(|| {
        crate::catalog::lookup(&model_id).unwrap_or_else(crate::catalog::fallback)
    });

    let mut edited = false;
    if let Some(context) = context {
        spec.context = context;
        edited = true;
    }
    if let Some(output) = output {
        spec.output = output;
        edited = true;
    }
    if let Some(favorite) = favorite {
        spec.favorite = favorite;
    }
    if let Some(value) = modalities {
        spec.input_modalities = if value.is_null() {
            Vec::new()
        } else {
            serde_json::from_value::<Vec<Modality>>(value.clone())
                .map_err(|e| Error::other(format!("\"inputModalities\" is invalid: {e}")))?
        };
        edited = true;
    }
    if let Some(value) = reasoning {
        spec.reasoning = if value.is_null() {
            None
        } else {
            Some(
                serde_json::from_value::<ReasoningSpec>(value.clone())
                    .map_err(|e| Error::other(format!("\"reasoning\" is invalid: {e}")))?,
            )
        };
        edited = true;
    }
    if edited {
        spec.source = crate::provider::MetadataSource::User;
    }

    Ok(if existed {
        format!("Updated model \"{model_id}\" on \"{provider_id}\"")
    } else {
        format!("Added model \"{model_id}\" to \"{provider_id}\"")
    })
}

pub fn delete_provider(config: &mut AppConfig, args: &Value) -> Result<String> {
    let id = required_arg(args, DELETE_PROVIDER, "id")?;
    let Some(provider) = config.providers.get(&id) else {
        return Err(Error::other(format!("unknown provider \"{id}\"")));
    };
    let provider_enabled = provider.enabled;
    let another_enabled = config
        .providers
        .iter()
        .any(|(other, candidate)| other != &id && candidate.enabled);
    if provider_enabled && !another_enabled {
        return Err(Error::other(format!(
            "refusing to delete \"{id}\": it is the last enabled provider, so Loom would have \
             no model to reach. Add or enable another provider first."
        )));
    }

    let removed = config.providers.remove(&id).expect("checked above");
    if config.chat.provider_id.as_deref() == Some(id.as_str()) {
        config.chat.provider_id = None;
        config.chat.model_id = None;
    }
    if config
        .chat
        .lite
        .as_ref()
        .map(|lite| lite.provider_id.as_str())
        == Some(id.as_str())
    {
        config.chat.lite = None;
    }
    Ok(format!("Deleted provider \"{}\"", removed.name))
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

pub fn update_settings(config: &mut AppConfig, args: &Value) -> Result<String> {
    let map = args
        .as_object()
        .ok_or_else(|| Error::other("update_settings arguments must be an object"))?;
    for key in map.keys() {
        if !["theme", "searchProvider", "interface", "chat"].contains(&key.as_str()) {
            return Err(settings_key_error(key));
        }
    }

    let mut changed: Vec<String> = Vec::new();

    if let Some(value) = map.get("theme") {
        config.theme = match value.as_str() {
            Some("light") => Theme::Light,
            Some("dark") => Theme::Dark,
            _ => return Err(Error::other("\"theme\" must be \"light\" or \"dark\"")),
        };
        changed.push("theme".to_string());
    }

    if let Some(value) = map.get("searchProvider") {
        config.search_provider = match value.as_str() {
            Some("auto") => SearchProvider::Auto,
            Some("jina") => SearchProvider::Jina,
            Some("duckduckgo") => SearchProvider::Duckduckgo,
            _ => {
                return Err(Error::other(
                    "\"searchProvider\" must be \"auto\", \"jina\" or \"duckduckgo\"",
                ))
            }
        };
        changed.push("searchProvider".to_string());
    }

    if let Some(value) = map.get("interface") {
        let interface = value
            .as_object()
            .ok_or_else(|| Error::other("\"interface\" must be an object"))?;
        for (key, value) in interface {
            match key.as_str() {
                "showThinking" => {
                    config.interface.show_thinking =
                        match value.as_str() {
                            Some("collapsed") => ThinkingDisplay::Collapsed,
                            Some("hidden") => ThinkingDisplay::Hidden,
                            Some("expanded") => ThinkingDisplay::Expanded,
                            _ => return Err(Error::other(
                                "\"interface.showThinking\" must be collapsed, hidden or expanded",
                            )),
                        };
                }
                "showToolCalls" => {
                    config.interface.show_tool_calls =
                        match value.as_str() {
                            Some("collapsed") => ToolCallDisplay::Collapsed,
                            Some("expanded") => ToolCallDisplay::Expanded,
                            Some("hidden") => ToolCallDisplay::Hidden,
                            _ => return Err(Error::other(
                                "\"interface.showToolCalls\" must be collapsed, expanded or hidden",
                            )),
                        };
                }
                "sendKey" => {
                    config.interface.send_key = match value.as_str() {
                        Some("enter") => SendKey::Enter,
                        Some("ctrl-enter") => SendKey::CtrlEnter,
                        _ => {
                            return Err(Error::other(
                                "\"interface.sendKey\" must be \"enter\" or \"ctrl-enter\"",
                            ))
                        }
                    };
                }
                "notifyOnCompletion" => {
                    config.interface.notify_on_completion =
                        as_bool(value, "interface.notifyOnCompletion")?;
                }
                "alwaysFollow" => {
                    config.interface.always_follow = as_bool(value, "interface.alwaysFollow")?;
                }
                "compact" => {
                    config.interface.compact = as_bool(value, "interface.compact")?;
                }
                "generatedUi" => {
                    config.interface.generated_ui = as_bool(value, "interface.generatedUi")?;
                }
                "captureOnSend" => {
                    config.interface.capture_on_send = as_bool(value, "interface.captureOnSend")?;
                }
                "sidebarPinned" => {
                    config.interface.sidebar_pinned = as_bool(value, "interface.sidebarPinned")?;
                }
                "sidebarWidth" => {
                    config.interface.sidebar_width = value
                        .as_u64()
                        .and_then(|width| u32::try_from(width).ok())
                        .ok_or_else(|| {
                            Error::other(
                                "\"interface.sidebarWidth\" must be a non-negative integer",
                            )
                        })?;
                }
                other => return Err(settings_key_error(&format!("interface.{other}"))),
            }
            changed.push(format!("interface.{key}"));
        }
    }

    if let Some(value) = map.get("chat") {
        let chat = value
            .as_object()
            .ok_or_else(|| Error::other("\"chat\" must be an object"))?;
        for (key, value) in chat {
            match key.as_str() {
                "maxOutputTokens" => {
                    let tokens = value
                        .as_u64()
                        .and_then(|tokens| u32::try_from(tokens).ok())
                        .ok_or_else(|| {
                            Error::other("\"chat.maxOutputTokens\" must be a non-negative integer")
                        })?;
                    // Zero still means "the model's own limit"; anything else
                    // gets the documented floor and ceiling.
                    config.chat.max_output_tokens = if tokens == 0 {
                        0
                    } else {
                        tokens.clamp(256, 200_000)
                    };
                }
                "autoTitle" => {
                    config.chat.auto_title = as_bool(value, "chat.autoTitle")?;
                }
                "embeddingModel" => {
                    config.chat.embedding_model =
                        optional_model_name(value, "chat.embeddingModel")?;
                }
                "imageModel" => {
                    config.chat.image_model = optional_model_name(value, "chat.imageModel")?;
                }
                "lite" => {
                    config.chat.lite = match value {
                        Value::Null => None,
                        other => Some(serde_json::from_value::<ModelRef>(other.clone()).map_err(
                            |_| Error::other("chat.lite needs providerId and modelId, or null"),
                        )?),
                    };
                }
                "computerVariant" => {
                    config.chat.computer_variant = match value {
                        Value::Null => None,
                        other => Some(
                            other
                                .as_str()
                                .ok_or_else(|| {
                                    Error::other(
                                        "chat.computerVariant must be a string or null",
                                    )
                                })?
                                .to_string(),
                        ),
                    };
                }
                "computerModel" => {
                    config.chat.computer_model = match value {
                        Value::Null => None,
                        other => Some(serde_json::from_value::<ModelRef>(other.clone()).map_err(
                            |_| {
                                Error::other(
                                    "chat.computerModel needs providerId and modelId, or null",
                                )
                            },
                        )?),
                    };
                }
                "computerScreenshotEdge" => {
                    let edge = value
                        .as_u64()
                        .ok_or_else(|| {
                            Error::other(
                                "chat.computerScreenshotEdge must be a non-negative integer",
                            )
                        })?
                        .min(4096) as u32;
                    config.chat.computer_screenshot_edge = edge;
                }
                other => return Err(settings_key_error(&format!("chat.{other}"))),
            }
            changed.push(format!("chat.{key}"));
        }
    }

    if changed.is_empty() {
        return Ok("No settings changed".to_string());
    }
    Ok(format!("Updated settings: {}", changed.join(", ")))
}

/// The reason a settings key is refused, tailored where the plan names one.
fn settings_key_error(path: &str) -> Error {
    if path.to_lowercase().contains("hotkey") {
        return Error::other(format!(
            "\"{path}\" cannot be changed by a tool: re-registering the global hotkey needs the \
             app shell. Ask the user to change it in Settings → General."
        ));
    }
    if path.contains("background") {
        return Error::other(format!(
            "\"{path}\" cannot be changed by a tool: a picked file is copied into \
             ~/.loom/backgrounds first, which Settings does. Ask the user to pick it there."
        ));
    }
    if path.to_lowercase().contains("key") || path.to_lowercase().contains("secret") {
        return Error::other(format!(
            "\"{path}\" cannot be written by a tool: API keys and secrets live in Windows \
             Credential Manager. Ask the user to paste it in Settings."
        ));
    }
    Error::other(format!(
        "unknown settings key \"{path}\". Allowed: theme, searchProvider, interface \
         ({}), chat ({}).",
        INTERFACE_KEYS.join(", "),
        CHAT_KEYS.join(", ")
    ))
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn trimmed_arg(args: &Value, key: &str) -> Option<String> {
    string_arg(args, key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_arg(args: &Value, tool: &str, key: &str) -> Result<String> {
    trimmed_arg(args, key)
        .ok_or_else(|| Error::other(format!("{tool} needs a non-empty \"{key}\"")))
}

fn bool_arg(args: &Value, key: &str) -> Result<Option<bool>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(Error::other(format!("\"{key}\" must be true or false"))),
    }
}

/// Absent means "leave alone", null means "clear", a number means "set".
fn clearable_u32(args: &Value, key: &str) -> Result<Option<Option<u32>>> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .map(|value| Some(Some(value)))
            .ok_or_else(|| {
                Error::other(format!("\"{key}\" must be a non-negative integer or null"))
            }),
        Some(_) => Err(Error::other(format!(
            "\"{key}\" must be a non-negative integer or null"
        ))),
    }
}

fn as_bool(value: &Value, path: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| Error::other(format!("\"{path}\" must be true or false")))
}

fn optional_model_name(value: &Value, path: &str) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.trim().to_string()).filter(|text| !text.is_empty())),
        _ => Err(Error::other(format!("\"{path}\" must be a string or null"))),
    }
}

fn check_name(kind: &str, name: &str) -> Result<()> {
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(Error::other(format!(
            "{kind} name is longer than {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home<T>(run: impl FnOnce() -> T) -> T {
        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());
        let outcome = run();
        std::env::remove_var("LOOM_HOME");
        outcome
    }

    #[test]
    fn persona_and_prompt_round_trip() {
        let mut config = AppConfig::default();

        let summary = apply(
            &mut config,
            UPSERT_PERSONA,
            &json!({ "name": "Reviewer", "systemPrompt": "Be terse." }),
        )
        .unwrap();
        assert!(summary.starts_with("Created persona"), "{summary}");
        assert_eq!(config.personas.len(), 1);
        let id = config.personas[0].id.clone();

        let summary = apply(
            &mut config,
            UPSERT_PERSONA,
            &json!({ "id": id, "name": "Reviewer 2", "systemPrompt": "Still terse." }),
        )
        .unwrap();
        assert!(summary.starts_with("Updated persona"), "{summary}");
        assert_eq!(config.personas.len(), 1);
        assert_eq!(config.personas[0].name, "Reviewer 2");

        // An unknown id on update is an error, not a silent create.
        assert!(apply(
            &mut config,
            UPSERT_PERSONA,
            &json!({ "id": "nope", "name": "X", "systemPrompt": "x" })
        )
        .is_err());

        apply(&mut config, DELETE_PERSONA, &json!({ "id": id })).unwrap();
        assert!(config.personas.is_empty());
        assert!(apply(&mut config, DELETE_PERSONA, &json!({ "id": id })).is_err());

        apply(
            &mut config,
            UPSERT_PROMPT,
            &json!({ "title": "Explain", "body": "Explain this code." }),
        )
        .unwrap();
        assert_eq!(config.prompts.len(), 1);
        let prompt_id = config.prompts[0].id.clone();
        apply(&mut config, DELETE_PROMPT, &json!({ "id": prompt_id })).unwrap();
        assert!(config.prompts.is_empty());
    }

    /// The new persona fields: Atelier writes the voice (sections, examples,
    /// presentation) through `upsert_persona`.
    #[test]
    fn atelier_writes_a_personas_voice() {
        let mut config = AppConfig::default();
        apply(
            &mut config,
            UPSERT_PERSONA,
            &json!({
                "name": "Reviewer",
                "systemPrompt": "Be terse.",
                "description": "Finds bugs",
                "tags": ["review", "rust"],
                "emoji": "🐛",
                "color": "violet",
                "favorite": true,
                "greeting": "What should I review?",
                "style": "Short sentences.",
                "rules": "Cite file and line.",
                "outputFormat": "One bullet per finding.",
                "examples": [{ "user": "look", "assistant": "found one" }],
            }),
        )
        .unwrap();

        let persona = &config.personas[0];
        assert_eq!(persona.description, "Finds bugs");
        assert_eq!(persona.tags, vec!["review".to_string(), "rust".to_string()]);
        assert_eq!(persona.emoji.as_deref(), Some("🐛"));
        assert_eq!(persona.color.as_deref(), Some("violet"));
        assert!(persona.favorite);
        assert_eq!(persona.greeting, "What should I review?");
        assert_eq!(persona.style, "Short sentences.");
        assert_eq!(persona.rules, "Cite file and line.");
        assert_eq!(persona.output_format, "One bullet per finding.");
        assert_eq!(persona.examples.len(), 1);
        assert_eq!(persona.examples[0].assistant, "found one");

        // An update leaves fields it was not given alone.
        let id = persona.id.clone();
        apply(
            &mut config,
            UPSERT_PERSONA,
            &json!({ "id": id, "name": "Reviewer", "systemPrompt": "Be terse.", "rules": "Be kind." }),
        )
        .unwrap();
        let persona = &config.personas[0];
        assert_eq!(persona.rules, "Be kind.");
        assert_eq!(persona.style, "Short sentences.");
        assert_eq!(persona.tags, vec!["review".to_string(), "rust".to_string()]);
    }

    /// Capability and memory fields are the settings UI's to write; the model
    /// path ignores them even when they are passed.
    #[test]
    fn atelier_never_writes_a_personas_powers() {
        let mut config = AppConfig::default();
        apply(
            &mut config,
            UPSERT_PERSONA,
            &json!({
                "name": "Reviewer",
                "systemPrompt": "Be terse.",
                "capabilities": {
                    "temperature": 0.2,
                    "permissionMode": "auto-all",
                    "agentMode": "build",
                    "tools": ["read_file"],
                    "mcpServers": ["github"]
                },
                "memory": { "enabled": true, "tokenBudget": 400 },
            }),
        )
        .unwrap();

        let persona = &config.personas[0];
        assert_eq!(persona.capabilities.temperature, None);
        assert_eq!(persona.capabilities.permission_mode, None);
        assert!(persona.capabilities.tools.is_empty());
        assert!(!persona.memory.enabled);
    }

    /// The new groups and casts: a deleted persona leaves their rosters.
    #[test]
    fn deleting_a_persona_drops_it_from_groups() {
        let mut config = AppConfig::default();
        for name in ["One", "Two"] {
            apply(
                &mut config,
                UPSERT_PERSONA,
                &json!({ "name": name, "systemPrompt": "x" }),
            )
            .unwrap();
        }
        let first = config.personas[0].id.clone();
        let second = config.personas[1].id.clone();
        config.persona_groups.push(crate::config::PersonaGroup {
            id: "g".into(),
            name: "Duo".into(),
            members: vec![first.clone(), second.clone()],
            cast: true,
        });

        apply(&mut config, DELETE_PERSONA, &json!({ "id": first })).unwrap();
        assert_eq!(config.persona_groups[0].members, vec![second]);
    }

    /// The UI path writes them, and still refuses to make Atelier a persona
    /// default.
    #[test]
    fn the_settings_ui_writes_a_personas_powers() {
        let mut config = AppConfig::default();
        apply_ui(
            &mut config,
            UPSERT_PERSONA,
            &json!({
                "name": "Coder",
                "systemPrompt": "Write Rust.",
                "capabilities": {
                    "temperature": 0.2,
                    "permissionMode": "auto-read-only",
                    "tools": ["read_file", "grep"],
                    "mcpServers": ["github"]
                },
                "memory": { "enabled": true, "tokenBudget": 400 },
            }),
        )
        .unwrap();

        let persona = &config.personas[0];
        assert_eq!(persona.capabilities.temperature, Some(0.2));
        assert_eq!(
            persona.capabilities.permission_mode,
            Some(crate::config::PermissionMode::AutoReadOnly)
        );
        assert_eq!(persona.capabilities.tools, vec!["read_file", "grep"]);
        assert!(persona.memory.enabled);
        assert_eq!(persona.memory.token_budget, 400);
        let id = persona.id.clone();

        apply_ui(
            &mut config,
            UPSERT_PERSONA,
            &json!({
                "id": id,
                "name": "Coder",
                "systemPrompt": "Write Rust.",
                "capabilities": { "permissionMode": "atelier" },
            }),
        )
        .unwrap();
        assert_eq!(config.personas[0].capabilities.permission_mode, None);
    }

    #[test]
    fn update_settings_is_deny_by_default() {
        let mut config = AppConfig::default();

        for args in [
            json!({ "interface": { "hotkey": "Ctrl+Alt+L" } }),
            json!({ "interface": { "hotkeyEnabled": false } }),
            json!({ "background": { "preset": "porcelain" } }),
            json!({ "interface": { "sidebarGrouping": "none" } }),
            json!({ "nonsense": true }),
        ] {
            assert!(
                apply(&mut config, UPDATE_SETTINGS, &args).is_err(),
                "{args} should be refused"
            );
        }

        // The refusal names the way around it.
        let error = apply(
            &mut config,
            UPDATE_SETTINGS,
            &json!({ "interface": { "hotkey": "Ctrl+Alt+L" } }),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("Settings"), "{error}");
        let error = apply(&mut config, UPDATE_SETTINGS, &json!({ "background": {} }))
            .unwrap_err()
            .to_string();
        assert!(error.contains("background"), "{error}");

        // Nothing was changed on the way to the error.
        assert_eq!(config.interface.hotkey, "Ctrl+Shift+Space");

        // The allowlisted keys do apply, with the documented clamps.
        let summary = apply(
            &mut config,
            UPDATE_SETTINGS,
            &json!({
                "theme": "light",
                "searchProvider": "jina",
                "interface": { "compact": true, "sidebarWidth": 300 },
                "chat": { "maxOutputTokens": 123, "autoTitle": false },
            }),
        )
        .unwrap();
        assert!(summary.contains("interface.compact"), "{summary}");
        assert_eq!(config.theme, Theme::Light);
        assert_eq!(config.search_provider, SearchProvider::Jina);
        assert!(config.interface.compact);
        assert_eq!(config.interface.sidebar_width, 300);
        assert_eq!(config.chat.max_output_tokens, 256);
        assert!(!config.chat.auto_title);
    }

    #[test]
    fn provider_validation_rejects_bad_input() {
        let mut config = AppConfig::default();

        assert!(apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "", "name": "X", "baseUrl": "https://x" })
        )
        .is_err());
        assert!(apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "x", "name": "X", "baseUrl": "ftp://x" })
        )
        .is_err());
        assert!(apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "x", "name": "X", "baseUrl": "https://x", "kind": "nope" })
        )
        .is_err());
        assert!(apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "x", "name": "X", "baseUrl": "https://x", "apiKey": "sk-secret" })
        )
        .is_err());
        assert!(config.providers.is_empty());

        apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "x", "name": "X", "baseUrl": "https://x", "keyRequired": false }),
        )
        .unwrap();
        assert_eq!(config.providers["x"].base_url, "https://x");
        assert!(!config.providers["x"].key_required);
    }

    #[test]
    fn the_last_enabled_provider_cannot_be_deleted() {
        let mut config = AppConfig::default();
        apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "x", "name": "X", "baseUrl": "https://x" }),
        )
        .unwrap();
        assert!(apply(&mut config, DELETE_PROVIDER, &json!({ "id": "x" })).is_err());

        apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "y", "name": "Y", "baseUrl": "https://y" }),
        )
        .unwrap();
        apply(&mut config, DELETE_PROVIDER, &json!({ "id": "x" })).unwrap();
        assert!(!config.providers.contains_key("x"));
    }

    #[test]
    fn mcp_ids_that_would_break_tool_names_are_refused() {
        for id in [
            "a__b",
            "Uppercase",
            "has space",
            "-leading",
            "trailing-",
            "",
        ] {
            let mut config = AppConfig::default();
            assert!(
                apply(
                    &mut config,
                    UPSERT_MCP_SERVER,
                    &json!({ "id": id, "command": "npx" })
                )
                .is_err(),
                "{id} should be refused"
            );
        }

        let mut config = AppConfig::default();
        apply(
            &mut config,
            UPSERT_MCP_SERVER,
            &json!({ "id": "filesystem", "command": "npx", "args": ["-y", "server"] }),
        )
        .unwrap();
        assert_eq!(config.mcp_servers["filesystem"].args.len(), 2);
    }

    #[test]
    fn model_updates_add_edit_and_remove() {
        let mut config = AppConfig::default();
        apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "p", "name": "P", "baseUrl": "https://p" }),
        )
        .unwrap();

        apply(
            &mut config,
            UPDATE_MODEL,
            &json!({ "providerId": "p", "modelId": "m", "context": 32000, "favorite": true }),
        )
        .unwrap();
        assert_eq!(config.providers["p"].models["m"].context, Some(32_000));
        assert!(config.providers["p"].models["m"].favorite);
        assert_eq!(
            config.providers["p"].models["m"].source,
            crate::provider::MetadataSource::User,
            "an edit is stamped as the user's"
        );

        apply(
            &mut config,
            UPDATE_MODEL,
            &json!({
                "providerId": "p",
                "modelId": "m",
                "inputModalities": ["text", "image"],
                "reasoning": { "enabled": true, "variants": ["low", "high"], "defaultVariant": "high" }
            }),
        )
        .unwrap();
        let edited = &config.providers["p"].models["m"];
        assert_eq!(
            edited.input_modalities,
            vec![Modality::Text, Modality::Image]
        );
        assert!(edited.reasoning.is_some());

        apply(
            &mut config,
            UPDATE_MODEL,
            &json!({ "providerId": "p", "modelId": "m", "context": null }),
        )
        .unwrap();
        assert_eq!(config.providers["p"].models["m"].context, None);

        apply(
            &mut config,
            UPDATE_MODEL,
            &json!({ "providerId": "p", "modelId": "m", "remove": true }),
        )
        .unwrap();
        assert!(!config.providers["p"].models.contains_key("m"));
    }

    #[test]
    fn model_reset_returns_to_detected_metadata() {
        let mut config = AppConfig::default();
        apply(
            &mut config,
            UPSERT_PROVIDER,
            &json!({ "id": "p", "name": "P", "baseUrl": "https://p" }),
        )
        .unwrap();
        apply(
            &mut config,
            UPDATE_MODEL,
            &json!({ "providerId": "p", "modelId": "gpt-4o", "context": 99, "favorite": true }),
        )
        .unwrap();
        assert_eq!(
            config.providers["p"].models["gpt-4o"].source,
            crate::provider::MetadataSource::User
        );

        let summary = apply(
            &mut config,
            UPDATE_MODEL,
            &json!({ "providerId": "p", "modelId": "gpt-4o", "reset": true }),
        )
        .unwrap();
        assert!(summary.contains("catalog@"), "{summary}");

        let spec = &config.providers["p"].models["gpt-4o"];
        assert_eq!(spec.context, Some(128_000));
        assert!(spec.input_modalities.contains(&Modality::Image));
        assert!(spec.favorite, "the favourite flag survives a reset");
        assert!(matches!(
            spec.source,
            crate::provider::MetadataSource::Catalog(_)
        ));
    }

    #[test]
    fn view_truncates_long_prompts_and_hides_secrets() {
        home(|| {
            let mut config = AppConfig::default();
            let long = "x".repeat(5_000);
            apply(
                &mut config,
                UPSERT_PERSONA,
                &json!({ "name": "Long", "systemPrompt": long }),
            )
            .unwrap();
            apply(
                &mut config,
                UPSERT_MCP_SERVER,
                &json!({ "id": "filesystem", "command": "npx", "env": { "TOKEN": "s3cret" } }),
            )
            .unwrap();
            apply(
                &mut config,
                UPSERT_PROVIDER,
                &json!({ "id": "p", "name": "P", "baseUrl": "https://p" }),
            )
            .unwrap();

            let viewed = view(&config).unwrap();
            let persona = &viewed["personas"][0];
            assert_eq!(persona["prompt"]["truncated"], true);
            assert_eq!(
                persona["prompt"]["text"].as_str().unwrap().chars().count(),
                VIEW_CHARS
            );
            assert_eq!(persona["prompt"]["fullLength"], 5_000);

            // Env values are secret; only the key names are shown.
            let rendered = viewed.to_string();
            assert!(rendered.contains("envKeys"), "{rendered}");
            assert!(!rendered.contains("s3cret"), "{rendered}");

            // And a sections filter is honored, with a readable error for a
            // section that does not exist.
            let only = view_sections(&config, &["prompts".to_string()]).unwrap();
            assert!(only.get("personas").is_none());
            assert!(only.get("prompts").is_some());
            assert!(view_sections(&config, &["nope".to_string()]).is_err());
        });
    }

    #[test]
    fn backups_are_written_and_pruned_to_ten() {
        home(|| {
            let directory = paths::backups_dir().unwrap();
            std::fs::create_dir_all(&directory).unwrap();
            for index in 0..12 {
                std::fs::write(
                    directory.join(format!("config-00010101-{index:06}Z.json")),
                    "{}",
                )
                .unwrap();
            }

            let mut config = AppConfig::default();
            config.theme = Theme::Light;
            let path = backup(&config).unwrap().unwrap();
            assert!(path.exists());

            let restored: AppConfig =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(restored, config);

            let files: Vec<String> = std::fs::read_dir(&directory)
                .unwrap()
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with("config-"))
                .collect();
            assert_eq!(files.len(), KEEP_BACKUPS, "{files:?}");
            assert!(files.iter().any(|name| directory.join(name) == path));
        });
    }

    #[test]
    fn skills_are_written_read_and_deleted() {
        home(|| {
            let mut config = AppConfig::default();
            apply(
                &mut config,
                WRITE_SKILL,
                &json!({ "id": "review", "name": "Review", "description": "d", "body": "Look for bugs." }),
            )
            .unwrap();

            let skill = skills::read("review").unwrap();
            assert_eq!(skill.name, "Review");
            assert_eq!(skill.prompt, "Look for bugs.");

            // The view lists it without the body.
            let viewed = view(&config).unwrap();
            assert_eq!(viewed["skills"][0]["id"], "review");

            apply(&mut config, DELETE_SKILL, &json!({ "id": "review" })).unwrap();
            assert!(skills::read("review").is_err());
        });
    }
}
