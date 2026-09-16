//! Tools the model can call, plus the permission gate.
//!
//! Every tool is a plain function over a `ToolContext` (the chat's workspace
//! folder). Tools declare whether they are read-only, which is what the
//! `AutoReadOnly` permission mode keys off.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::PermissionMode;
use crate::{Error, Result};

/// Largest file a tool will return.
const MAX_READ_BYTES: u64 = 400_000;

/// Name of the tool that pauses the turn to ask the user something.
///
/// It is dispatched by the engine (which has the UI to ask through), not by
/// [`execute`], and it is never gated behind a permission prompt: the question
/// card *is* the prompt.
pub const ASK_USER: &str = "ask_user";

/// The task-list tools. They only touch Loom's own state, so they are declared
/// read-only on purpose: they run without a permission card, and Plan mode may
/// still use them (planning is exactly what a task list is for). The engine
/// dispatches them; it owns the session and the database.
pub const TODO_WRITE: &str = "todo_write";
pub const TODO_READ: &str = "todo_read";

/// Deleting a path. Unlike the other destructive tools this is not gated by
/// name alone: `delete_risk` decides per call whether losing the path could be
/// undone. See that function for the rule.
pub const DELETE_PATH: &str = "delete_path";

/// Tracking the shell commands Loom has started. `run_command` itself is
/// dispatched by the engine (it owns the process handles and the log files),
/// and so are these three.
pub const LIST_COMMANDS: &str = "list_commands";
pub const COMMAND_OUTPUT: &str = "command_output";
pub const STOP_COMMAND: &str = "stop_command";

/// Most option buttons one question may show. Beyond this the model should be
/// asking a narrower question.
const MAX_QUESTION_OPTIONS: usize = 6;

/// Longest question/option text accepted, so a runaway model cannot push a
/// novel through the UI.
const MAX_QUESTION_CHARS: usize = 400;

/// One button the user can pick from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A normalized `ask_user` call: what the UI renders and what the model asked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskQuestion {
    pub question: String,
    /// Optional one-or-two-word label, e.g. "Database".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    /// The user may pick several options.
    #[serde(default)]
    pub allow_multiple: bool,
    /// The user may type an answer instead of (or as well as) picking.
    #[serde(default)]
    pub allow_free_text: bool,
}

/// The user's reply to an [`AskQuestion`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    /// Labels of the options the user picked, in the order they were shown.
    #[serde(default)]
    pub selected: Vec<String>,
    /// Free text the user typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The user dismissed the question instead of answering it.
    #[serde(default)]
    pub cancelled: bool,
}

impl AskQuestion {
    /// Reads an `ask_user` call, filling in defaults and rejecting the shapes
    /// that would leave the user with nothing to click.
    pub fn parse(arguments: &str) -> Result<Self> {
        let value: Value = if arguments.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(arguments)
                .map_err(|e| Error::Other(format!("invalid {ASK_USER} arguments: {e}")))?
        };

        let question = value
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if question.is_empty() {
            return Err(Error::Other(format!("{ASK_USER} needs a question")));
        }

        let mut options: Vec<QuestionOption> = Vec::new();
        if let Some(items) = value.get("options").and_then(Value::as_array) {
            for item in items {
                // Accept both `{"label": "..."}` and a bare `"..."`, because
                // models mix the two shapes freely.
                let (label, description) = match item {
                    Value::String(text) => (text.clone(), None),
                    other => (
                        other
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        other
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    ),
                };
                let label = truncate_chars(label.trim(), MAX_QUESTION_CHARS);
                // Skip blanks and duplicates rather than failing the whole call.
                if label.is_empty() || options.iter().any(|option| option.label == label) {
                    continue;
                }
                options.push(QuestionOption {
                    label,
                    description: description
                        .map(|text| truncate_chars(text.trim(), MAX_QUESTION_CHARS))
                        .filter(|text| !text.is_empty()),
                });
                if options.len() == MAX_QUESTION_OPTIONS {
                    break;
                }
            }
        }

        let allow_free_text = value
            .get("allow_free_text")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if options.is_empty() && !allow_free_text {
            return Err(Error::Other(format!(
                "{ASK_USER} needs options or a free-text answer"
            )));
        }

        Ok(AskQuestion {
            question: truncate_chars(&question, MAX_QUESTION_CHARS),
            header: value
                .get("header")
                .and_then(Value::as_str)
                .map(|header| truncate_chars(header.trim(), 40))
                .filter(|header| !header.is_empty()),
            options,
            allow_multiple: value
                .get("allow_multiple")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            allow_free_text,
        })
    }

    /// Tool output handed back to the model so the turn can continue.
    pub fn to_tool_output(&self, answer: &Answer) -> String {
        if answer.cancelled {
            return "The user dismissed the question without answering. Do not ask it again: continue with your best judgement, or say what you need from them."
                .to_string();
        }

        let mut parts: Vec<String> = Vec::new();
        if !answer.selected.is_empty() {
            parts.push(format!("selected {}", answer.selected.join(", ")));
        }
        let typed = answer
            .text
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if !typed.is_empty() {
            parts.push(format!("typed \"{typed}\""));
        }
        if parts.is_empty() {
            return "The user submitted an empty answer. Continue with your best judgement."
                .to_string();
        }

        format!(
            "The user answered \"{}\" — {}.",
            self.question,
            parts.join("; ")
        )
    }
}

/// Keeps a tool argument to a sane length without splitting a UTF-8 character.
fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
    pub read_only: bool,
    /// What the tool can reach. Omitted for workspace tools, which is the
    /// default the UI assumes; harness tools declare themselves so the
    /// permission card can say so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ToolScope>,
}

/// Where a tool's effects land. The permission card renders a different badge
/// for each non-workspace scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolScope {
    Workspace,
    Harness,
    Web,
    Mcp,
    /// Mouse, keyboard, windows, processes: the machine itself.
    Computer,
}

#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    /// Absolute path of the chat's workspace folder, when one is set.
    pub workdir: Option<PathBuf>,
    /// Computer use is armed for this chat (the composer's Computer chip).
    pub computer: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON string as produced by the model.
    pub arguments: String,
}

/// An image a tool produced (a screenshot): stored on disk, shown in the
/// transcript, and inlined into the wire for the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolImage {
    pub name: String,
    pub mime: String,
    /// Absolute path inside the Loom home directory.
    pub path: String,
    /// Encoded pixel dimensions; zero when unknown (a record written by an
    /// older build). The context budget counts tokens per pixel tile rather
    /// than per byte, because that is how providers meter an image.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub width: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub height: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub id: String,
    pub name: String,
    pub ok: bool,
    pub output: String,
    /// Images produced by the call, e.g. a screenshot. Empty for most tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ToolImage>,
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: ASK_USER,
            description: "Ask the user a question and wait for their answer. Use it when a choice or a missing detail genuinely changes what you do next — not for routine confirmations. Ask as many as you need; each call asks one question, and the user can pick an option, type their own answer, or dismiss it.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question, phrased plainly" },
                    "header": { "type": "string", "description": "Optional one-or-two-word label, e.g. \"Database\"" },
                    "options": {
                        "type": "array",
                        "description": "Choices to offer, each a label with an optional description",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "description": { "type": "string" }
                            },
                            "required": ["label"],
                            "additionalProperties": false
                        }
                    },
                    "allow_multiple": { "type": "boolean", "description": "Set true when several options may be picked" },
                    "allow_free_text": { "type": "boolean", "description": "Whether a typed answer is allowed (default true)" }
                },
                "required": ["question"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "datetime",
            description: "Current UTC date and time, with the unix timestamp. Use when the user asks about now, today, or relative dates.",
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "list_dir",
            description: "List files and folders inside the chat's workspace folder. Paths are relative to the workspace root.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative folder path, omit or use \".\" for the root" }
                },
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "read_file",
            description: "Read a UTF-8 text file from the chat's workspace folder. Paths are relative to the workspace root and may not escape it.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative file path" },
                    "start_line": { "type": "integer", "description": "First line to return (1-based, optional)" },
                    "max_lines": { "type": "integer", "description": "How many lines to return (optional)" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "grep",
            description: "Search the workspace folder for a literal string (case-insensitive) and return matching lines with file names.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Literal text to search for" },
                    "path": { "type": "string", "description": "Relative folder to search, defaults to the workspace root" },
                    "include": { "type": "string", "description": "Optional glob to limit files, e.g. \"*.rs\" or \"src/**/*.ts\"" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "find_files",
            description: "Find files in the workspace folder by glob pattern, e.g. \"**/*.rs\" or \"src/**/*.tsx\". Returns matching relative paths.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern relative to the workspace root" },
                    "limit": { "type": "integer", "description": "Maximum matches (1-200, default 80)" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "create_dir",
            description: "Create a folder (and any missing parents) inside the workspace.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative folder path" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: "move_path",
            description: "Move or rename a file or folder inside the workspace. The destination is a full relative path, not a folder.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Relative path to move" },
                    "to": { "type": "string", "description": "Relative destination path" }
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: "copy_path",
            description: "Copy a file or folder inside the workspace. The destination is a full relative path, not a folder.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Relative path to copy" },
                    "to": { "type": "string", "description": "Relative destination path" }
                },
                "required": ["from", "to"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: DELETE_PATH,
            description: "Delete a file or folder inside the workspace. On Windows it goes to the Recycle Bin, so it can be restored. The user is asked to confirm only when the loss would be real: content that is uncommitted or ignored, a nested repository, or a folder outside version control. Deleting committed files in a repository runs without asking, so check `git_status` first if you are unsure what is at stake.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path to delete" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: "git_status",
            description: "Show the git branch and working-tree status of the workspace folder.",
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "git_diff",
            description: "Show the workspace's uncommitted git diff (unstaged, or staged with staged=true).",
            parameters: json!({
                "type": "object",
                "properties": {
                    "staged": { "type": "boolean", "description": "Show the staged diff instead of the working-tree diff" },
                    "path": { "type": "string", "description": "Optional relative path to limit the diff to" }
                },
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "git_log",
            description: "Show the most recent commits in the workspace folder, one line each.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "count": { "type": "integer", "description": "How many commits (1-100, default 15)" }
                },
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: TODO_WRITE,
            description: "Replace this chat's live task list. This only edits Loom's own task panel — never the workspace — so it needs no permission and works in Plan mode too. Send the complete list every time; keep exactly one item in_progress while you work, and mark items completed as you finish them.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "description": "The full task list, replacing whatever was there",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "The task, in a few words" },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                            },
                            "required": ["content", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["todos"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: TODO_READ,
            description: "Read this chat's current task list.",
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "write_file",
            description: "Create or overwrite a file in the workspace folder with the given content. Paths are relative to the workspace root.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative file path" },
                    "content": { "type": "string", "description": "Full file contents" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace the first occurrence of old_string with new_string in a workspace file. Fails if old_string is missing or ambiguous.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative file path" },
                    "old_string": { "type": "string", "description": "Exact text to replace" },
                    "new_string": { "type": "string", "description": "Replacement text" }
                },
                "required": ["path", "old_string", "new_string"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: "run_command",
            description: "Run a shell command inside the workspace folder and return stdout, stderr, and the exit code. Use for tests, builds, and git status. It runs hidden — no terminal window opens — and is given 120s; if it is still going after that it is NOT killed, it keeps running in the background and you get its id so `command_output` can read it and `stop_command` can end it. Set `background: true` for anything long-lived you do not want to wait for (a dev server, a watcher): you get an id back immediately, the process survives this turn, and its output goes to a log.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command line to execute" },
                    "background": { "type": "boolean", "description": "Start it and return at once instead of waiting, for long-running processes (default false)" },
                    "label": { "type": "string", "description": "Short human label for this command, e.g. \"dev server\" (optional)" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: LIST_COMMANDS,
            description: "List the shell commands Loom has started, newest first, with their status, exit code, and id. Includes background commands and any that outlived their turn.",
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: COMMAND_OUTPUT,
            description: "Read what a command has produced so far (its log's last lines). Works for a running background command and for one that has finished.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Command id from run_command or list_commands" },
                    "tail": { "type": "integer", "description": "How many trailing lines to return (default 60)" }
                },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: STOP_COMMAND,
            description: "Stop a running command and everything it spawned (its whole process tree). Use it to end a dev server or a watcher you started.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Command id from run_command or list_commands" }
                },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: "search_workspace",
            description: "Semantic search over the indexed workspace. Returns the most relevant file chunks. Run `index_workspace` first (the user can do that from the workspace menu).",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What you are looking for" },
                    "limit": { "type": "integer", "description": "How many chunks to return (1-12, default 6)" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "generate_image",
            description: "Generate an image from a text prompt and show it in the chat. Returns the saved file path.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "What to draw" },
                    "size": { "type": "string", "description": "Optional size, e.g. 1024x1024" }
                },
                "required": ["prompt"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: "spawn_agent",
            description: "Delegate a self-contained task to a subagent and get its final answer back. Useful for research or parallel reading. Maximum depth is one. Set `background` to true to detach the work: it runs as its own task (visible in the Runs popup, survives this turn) and its result lands in this chat when it finishes.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "What the subagent should do" },
                    "system": { "type": "string", "description": "Optional role for the subagent" },
                    "background": { "type": "boolean", "description": "Detach the run instead of waiting for it (default false)" }
                },
                "required": ["task"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: None,
        },
        ToolSpec {
            name: RECALL,
            description: "Search long-term memory (durable facts about the user and the current project) for anything relevant. Use it when a remembered detail would change your answer.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for" },
                    "limit": { "type": "integer", "description": "How many facts to return (1-10, default 6)" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            read_only: true,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: REMEMBER_FACT,
            description: "Save a durable fact to long-term memory: who the user is, their preferences, standing instructions, or something about the current project. Facts persist across chats. Do not save secrets or one-off requests. Updating an existing fact replaces it.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "fact": { "type": "string", "description": "The fact, one sentence (max 2 KiB)" },
                    "scope": { "type": "string", "enum": ["global", "workspace"], "description": "global for facts about the user, workspace for facts about the current project (default global)" },
                    "pinned": { "type": "boolean", "description": "Pin it into every future prompt (default false)" }
                },
                "required": ["fact"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: FORGET_FACT,
            description: "Delete a long-term memory. Pass `id` when you have it from `recall`; otherwise pass `query` and an unambiguous match is deleted.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Memory id from recall" },
                    "query": { "type": "string", "description": "Text to match when no id is known" }
                },
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: SCHEDULE_JOB,
            description: "Create or update a scheduled job: a prompt Loom runs on a cron schedule, in the background, even while you are away. Use it when the user asks for something recurring (\"every weekday morning\", \"every hour\"). The user always sees a confirmation card before anything is scheduled.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Short job name, e.g. \"Morning inbox summary\"" },
                    "cron": { "type": "string", "description": "Five-field cron: minute hour day-of-month month weekday, e.g. \"0 8 * * 1-5\"" },
                    "prompt": { "type": "string", "description": "The instruction the job runs" },
                    "id": { "type": "string", "description": "Existing job id to update; omit to create" },
                    "workspace": { "type": "string", "description": "Workspace folder path (defaults to this chat's)" },
                    "permission_mode": { "type": "string", "enum": ["ask", "auto-read-only", "auto-all"], "description": "How much the job may do unattended (default auto-read-only)" },
                    "notify_on_success": { "type": "boolean", "description": "Notify even when a run succeeds (default false: failures and questions only)" }
                },
                "required": ["name", "cron", "prompt"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: LIST_JOBS,
            description: "List the scheduled jobs and their next run times.",
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            read_only: true,
            scope: Some(ToolScope::Harness),
        },
        ToolSpec {
            name: DELETE_JOB,
            description: "Delete a scheduled job by id. The user always sees a confirmation card.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Job id from list_jobs" }
                },
                "required": ["id"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: Some(ToolScope::Harness),
        },
    ]
}

/// Persistent persona memory: `remember` and `forget`, offered only when the
/// active persona has memory enabled.
pub const REMEMBER: &str = "remember";
pub const FORGET: &str = "forget";

/// Long-term memory (global + per-workspace facts), offered in every mode.
pub const RECALL: &str = "recall";
pub const REMEMBER_FACT: &str = "remember_fact";
pub const FORGET_FACT: &str = "forget_fact";

/// Scheduled jobs.
pub const SCHEDULE_JOB: &str = "schedule_job";
pub const LIST_JOBS: &str = "list_jobs";
pub const DELETE_JOB: &str = "delete_job";

/// Passes the turn to another persona in a multi-persona chat.
pub const HANDOFF: &str = "handoff";

/// Tools offered when the active persona has memory enabled.
pub fn memory_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: REMEMBER,
            description: "Save a durable fact to this persona's memory. Memory persists across every chat that uses the persona, so keep it short and general — preferences, ongoing projects, names — not a transcript.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Short stable label, e.g. \"preferred language\"" },
                    "value": { "type": "string", "description": "What to remember (max 2 KiB)" }
                },
                "required": ["key", "value"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
        ToolSpec {
            name: FORGET,
            description: "Delete one entry from this persona's memory by its key. Use when a remembered fact is wrong or no longer true.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The key passed to remember" }
                },
                "required": ["key"],
                "additionalProperties": false
            }),
            read_only: false,
            scope: None,
        },
    ]
}

/// The turn-passing tool, offered only in multi-persona chats.
pub fn handoff_spec(cast: &[String]) -> ToolSpec {
    ToolSpec {
        name: HANDOFF,
        description: "Pass the turn to another persona in this group chat and give them the floor. They will reply next, seeing the whole conversation including your handoff note.",
        parameters: json!({
            "type": "object",
            "properties": {
                "persona": {
                    "type": "string",
                    "description": format!("Persona name or id to hand the turn to. Cast: {}", cast.join(", "))
                },
                "note": { "type": "string", "description": "Why you are handing over and what they should address" }
            },
            "required": ["persona"],
            "additionalProperties": false
        }),
        read_only: false,
        scope: None,
    }
}

/// Memory and handoff specs together, for Settings → Tools and the permission
/// card's tool lookup. They are only *offered* to the model contextually.
pub fn persona_tool_specs() -> Vec<ToolSpec> {
    let mut specs = memory_specs();
    specs.push(handoff_spec(&[]));
    specs
}

pub fn spec(name: &str) -> Option<ToolSpec> {
    specs()
        .into_iter()
        .chain(persona_tool_specs())
        .find(|tool| tool.name == name)
}

/// The built-in tool list for a permission mode. Harness tools are listed only
/// in Atelier; everywhere else the model never sees them.
pub fn specs_for(mode: PermissionMode) -> Vec<ToolSpec> {
    let mut specs = specs();
    if mode == PermissionMode::Atelier {
        specs.extend(crate::harness::specs());
    }
    specs
}

/// Tools that `AutoReadOnly` may run without asking.
pub fn is_read_only(name: &str) -> bool {
    // Harness tools are not in this module's `specs()`, so they are mapped
    // explicitly: only `list_harness` is a read.
    if crate::harness::is_harness_read(name) {
        return true;
    }
    if crate::harness::is_harness_tool(name) {
        return false;
    }
    if crate::computer::is_computer_tool(name) {
        return crate::computer::is_read_only(name);
    }
    spec(name).map(|tool| tool.read_only).unwrap_or(false)
}

/// Calls that always show a confirmation card, even under Auto all: creating
/// or deleting a schedule changes what Loom does while nobody is watching, and
/// the user is not there to notice.
///
/// `delete_path` is deliberately *not* here. It used to be, which meant every
/// delete carded in every mode no matter how trivial. What makes a delete worth
/// a card is not its name but whether the user could get the bytes back, and
/// that needs the arguments — so it is judged by [`delete_risk`] instead.
pub fn always_asks(name: &str) -> bool {
    matches!(name, SCHEDULE_JOB | DELETE_JOB)
}

/// Whether a call may proceed under the current mode.
pub fn requires_confirmation(mode: PermissionMode, name: &str) -> bool {
    // The question card *is* the prompt, so `ask_user` is never gated: asking
    // the user to allow asking the user would be silly.
    if name == ASK_USER {
        return false;
    }
    if always_asks(name) {
        return true;
    }
    match mode {
        PermissionMode::Ask => true,
        PermissionMode::AutoReadOnly => !is_read_only(name),
        PermissionMode::AutoAll => false,
        // Auto all plus the harness tools — except that destruction always
        // asks. That is the only asymmetry.
        PermissionMode::Atelier => crate::harness::is_destructive(name),
    }
}

/// Whether deleting this path deserves a confirmation card even where the
/// mode would run it silently: the reason to show, or `None` to go ahead.
///
/// [`requires_confirmation`] answers a question about a tool's *name*. This
/// answers a different one — "could the user get it back?" — which needs the
/// arguments and the filesystem, so it cannot live in that match.
///
/// The rule is git, because git is the only undo Loom can actually check:
///
/// - a path with no repository above it cannot be restored, so it asks;
/// - anything uncommitted underneath it asks — `git restore` cannot bring that
///   back — ignored files included, since `.gitignore` happens to cover exactly
///   the folders that are expensive to rebuild;
/// - a nested repository asks whatever its own contents are, because `git
///   status` collapses one to a single line and says nothing about the work
///   inside it;
/// - anything else — a tracked, committed path in a repository — runs silently,
///   which is the common case and the one the user asked for.
///
/// [`delete_path`] also sends deletes to the Recycle Bin on Windows, but that
/// is deliberately not part of this decision: the bin is a user setting, it
/// empties on its own schedule, and other platforms have none. It is a safety
/// net for the calls that ran without asking, never a reason to stop asking.
pub fn delete_risk(arguments: &str, context: &ToolContext) -> Option<String> {
    let parsed: Value = serde_json::from_str(arguments).ok()?;
    let requested = parsed.get("path").and_then(Value::as_str)?;
    let root = context.workdir.clone()?;

    // A path the gate refuses anyway, or one that is not there, is not worth a
    // card: the call will come back with an error the model can read.
    let target = resolve(context, requested).ok()?;
    if target == root || !target.exists() {
        return None;
    }

    let relative = relative_path(&target, &root);
    let directory = target.is_dir();

    // Taking `.git` with it discards every commit the `git_*` tools could
    // revert to — the one loss that removing a working tree does not cover.
    if relative.split('/').any(|part| part == ".git") {
        return Some(format!(
            "{relative} is the repository's history — nothing Loom can reach would restore it"
        ));
    }

    // A directory can hold repositories of its own, and `git status` reports
    // badly on both kinds: a submodule collapses to one line, and a separate
    // clone inside the tree is not mentioned at all.
    if directory {
        match nested_repository(&target) {
            Nested::Repository => {
                return Some(format!(
                    "{relative} contains a git repository, whose commits are not part of \
                     this workspace's history — deleting it would lose them"
                ))
            }
            Nested::Submodule => {
                return Some(format!(
                    "{relative} contains a submodule's working tree; git records it as a \
                     commit id only, so anything uncommitted inside it would be lost"
                ))
            }
            Nested::None => {}
        }
    }

    match uncommitted_count(&root, &relative) {
        // Inside a repository, and git has a complete record of the path:
        // `git restore` brings it straight back, so no question is warranted.
        Some(0) => None,
        Some(changed) if directory => {
            let total = count_entries(&target);
            Some(format!(
                "{changed} of the {total} entries under {relative} are uncommitted — git \
                 cannot restore those, and the Recycle Bin empties on its own schedule"
            ))
        }
        Some(_) => Some(format!(
            "{relative} is uncommitted — git cannot restore it, and the Recycle Bin \
             empties on its own schedule"
        )),
        // Nowhere to ask, so nothing would bring the path back.
        None => Some(format!(
            "{relative} is not in version control — neither git nor the Recycle Bin \
             would bring it back"
        )),
    }
}

/// What a directory holds that git would not report on usefully.
#[derive(Debug, PartialEq, Eq)]
enum Nested {
    /// A `.git` directory in a child: a repository of its own.
    Repository,
    /// A `.git` file in a child: a submodule's working tree, whose contents
    /// exist only on this machine until they are committed and pushed.
    Submodule,
    None,
}

fn nested_repository(directory: &Path) -> Nested {
    let Ok(reader) = std::fs::read_dir(directory) else {
        return Nested::None;
    };
    for entry in reader.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let marker = entry.path().join(".git");
        if marker.is_dir() {
            return Nested::Repository;
        }
        if marker.is_file() {
            return Nested::Submodule;
        }
    }
    Nested::None
}

/// Uncommitted changes under `relative`, as git counts them. `Some(0)` means
/// git has a complete record of the path; `None` means there is no repository
/// to ask — or git is not installed, which comes to the same thing here.
///
/// Ignored files are included on purpose. They are untracked by definition, so
/// deleting them is final, and `.gitignore` happens to cover the folders that
/// are most expensive to rebuild (`node_modules`, `target`). Somewhere there
/// is no git at all, the whole path can only be judged as unrecorded.
///
/// The count says nothing about what is *inside* a nested repository: a
/// submodule comes back as a single changed line and a separate clone as none
/// at all. [`delete_risk`] checks for both before trusting this number.
fn uncommitted_count(root: &Path, relative: &str) -> Option<usize> {
    let output = run_git(
        root,
        &[
            "--no-pager",
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignored",
            "--",
            relative,
        ],
    )
    .ok()?;
    // A non-zero exit means "not a repository" (or a pathspec git rejected),
    // not "clean" — the distinction the caller depends on.
    if output.status.code() != Some(0) {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(stdout.lines().filter(|line| !line.trim().is_empty()).count())
}

/// Plan mode refuses anything that can change the workspace. Web tools are
/// fine (research is the point); MCP tools are refused because their side
/// effects are unknown. `ask_user` is never refused — the card is the prompt.
pub fn is_blocked_in_plan(name: &str) -> bool {
    if name == ASK_USER {
        return false;
    }
    if name == crate::web::SEARCH_TOOL || name == crate::web::FETCH_TOOL {
        return false;
    }
    !is_read_only(name) // write_file, edit_file, run_command, and every MCP tool
}

/// What the model is told when a read-only agent mode refuses a call. Phrased
/// as guidance, not just an error, so the next round produces the mode's
/// deliverable instead of a retry.
pub fn mode_refusal(mode: &str, name: &str) -> String {
    let guidance = if mode == "Review" {
        "report what you found — issues ranked by severity, with file and line, and a proposed \
         fix each. The user can switch to Build to have them applied."
    } else {
        "present a concrete plan (steps, files, risks) and wait for the user to switch to Build."
    };
    format!(
        "{mode} mode: `{name}` is unavailable. Do not call it again — read and \
         search as much as you need, then {guidance}"
    )
}

pub fn plan_refusal(name: &str) -> String {
    mode_refusal("Plan", name)
}

pub fn execute(call: &ToolCall, context: &ToolContext) -> ToolOutcome {
    let result = dispatch(call, context);
    match result {
        Ok(output) => ToolOutcome {
            id: call.id.clone(),
            name: call.name.clone(),
            ok: true,
            output,
            images: Vec::new(),
        },
        Err(error) => ToolOutcome {
            id: call.id.clone(),
            name: call.name.clone(),
            ok: false,
            output: error.to_string(),
            images: Vec::new(),
        },
    }
}

fn dispatch(call: &ToolCall, context: &ToolContext) -> Result<String> {
    let arguments: Value = if call.arguments.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&call.arguments)
            .map_err(|e| Error::Other(format!("invalid tool arguments: {e}")))?
    };

    match call.name.as_str() {
        ASK_USER => Err(Error::Other(format!(
            "{ASK_USER} is answered by the engine, not executed"
        ))),
        "datetime" => Ok(now_string()),
        "list_dir" => {
            let path = string_arg(&arguments, "path").unwrap_or_else(|| ".".to_string());
            let target = resolve(context, &path)?;
            list_dir(&target)
        }
        "read_file" => {
            let path = string_arg(&arguments, "path")
                .ok_or_else(|| Error::Other("read_file requires a path".into()))?;
            let target = resolve(context, &path)?;
            let start = int_arg(&arguments, "start_line").filter(|value| *value > 0);
            let lines = int_arg(&arguments, "max_lines").filter(|value| *value > 0);
            read_file(&target, start, lines)
        }
        "grep" => {
            let query = string_arg(&arguments, "query")
                .or_else(|| string_arg(&arguments, "pattern"))
                .ok_or_else(|| Error::Other("grep requires a query".into()))?;
            let path = string_arg(&arguments, "path").unwrap_or_else(|| ".".to_string());
            let target = resolve(context, &path)?;
            let include = string_arg(&arguments, "include");
            grep(&target, &query, include.as_deref())
        }
        "find_files" => {
            let pattern = string_arg(&arguments, "pattern")
                .ok_or_else(|| Error::Other("find_files requires a pattern".into()))?;
            let limit = int_arg(&arguments, "limit").unwrap_or(80).clamp(1, 200) as usize;
            find_files(context, &pattern, limit)
        }
        "create_dir" => {
            let path = string_arg(&arguments, "path")
                .ok_or_else(|| Error::Other("create_dir requires a path".into()))?;
            let target = resolve(context, &path)?;
            create_dir(&target)
        }
        "move_path" => {
            let from = string_arg(&arguments, "from")
                .ok_or_else(|| Error::Other("move_path requires from".into()))?;
            let to = string_arg(&arguments, "to")
                .ok_or_else(|| Error::Other("move_path requires to".into()))?;
            let source = resolve(context, &from)?;
            let destination = resolve(context, &to)?;
            let root = workspace_root(context)?;
            move_path(&source, &destination, &root)
        }
        "copy_path" => {
            let from = string_arg(&arguments, "from")
                .ok_or_else(|| Error::Other("copy_path requires from".into()))?;
            let to = string_arg(&arguments, "to")
                .ok_or_else(|| Error::Other("copy_path requires to".into()))?;
            let source = resolve(context, &from)?;
            let destination = resolve(context, &to)?;
            let root = workspace_root(context)?;
            copy_path(&source, &destination, &root)
        }
        DELETE_PATH => {
            let path = string_arg(&arguments, "path")
                .ok_or_else(|| Error::Other("delete_path requires a path".into()))?;
            let target = resolve(context, &path)?;
            let root = workspace_root(context)?;
            delete_path(&target, &root)
        }
        "git_status" => git_status(context),
        "git_diff" => {
            let staged = arguments
                .get("staged")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let path = string_arg(&arguments, "path");
            git_diff(context, staged, path.as_deref())
        }
        "git_log" => {
            let count = int_arg(&arguments, "count").unwrap_or(15).clamp(1, 100);
            git_log(context, count)
        }
        TODO_WRITE | TODO_READ | LIST_COMMANDS | COMMAND_OUTPUT | STOP_COMMAND => {
            Err(Error::Other(format!(
                "{} is handled by the engine, not executed",
                call.name
            )))
        }
        "write_file" => {
            let path = string_arg(&arguments, "path")
                .ok_or_else(|| Error::Other("write_file requires a path".into()))?;
            let content = string_arg(&arguments, "content")
                .ok_or_else(|| Error::Other("write_file requires content".into()))?;
            let target = resolve(context, &path)?;
            write_file(&target, &content)
        }
        "edit_file" => {
            let path = string_arg(&arguments, "path")
                .ok_or_else(|| Error::Other("edit_file requires a path".into()))?;
            let old = string_arg(&arguments, "old_string")
                .ok_or_else(|| Error::Other("edit_file requires old_string".into()))?;
            let new = string_arg(&arguments, "new_string")
                .ok_or_else(|| Error::Other("edit_file requires new_string".into()))?;
            let target = resolve(context, &path)?;
            edit_file(&target, &old, &new)
        }
        other => Err(Error::Other(format!("unknown tool: {other}"))),
    }
}

fn now_string() -> String {
    // Local time without a chrono dependency: system time + offset via libc-free
    // arithmetic is not portable, so report UTC plus the local offset when the
    // platform exposes it through the time crate's formatting is overkill here.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = now / 86_400;
    let seconds = now % 86_400;
    let (year, month, day) = crate::fsutil::civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC (unix {now})",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

fn int_arg(arguments: &Value, key: &str) -> Option<i64> {
    arguments.get(key).and_then(Value::as_i64)
}

fn workspace_root(context: &ToolContext) -> Result<PathBuf> {
    context
        .workdir
        .clone()
        .ok_or_else(|| Error::Other("this chat has no workspace folder set".into()))
}

fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Glob matcher for `find_files` and `grep --include`: `*` stays inside a path
/// segment, `**` crosses folders, `?` is one character.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    match_glob(&pattern, &text)
}

fn match_glob(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    match pattern[0] {
        '*' => {
            if pattern.len() > 1 && pattern[1] == '*' {
                let rest: &[char] = if pattern.len() > 2 && pattern[2] == '/' {
                    &pattern[3..]
                } else {
                    &pattern[2..]
                };
                (0..=text.len()).any(|skip| match_glob(rest, &text[skip..]))
            } else {
                let rest = &pattern[1..];
                for skip in 0..=text.len() {
                    if skip > 0 && text[skip - 1] == '/' {
                        break;
                    }
                    if match_glob(rest, &text[skip..]) {
                        return true;
                    }
                }
                false
            }
        }
        '?' => !text.is_empty() && text[0] != '/' && match_glob(&pattern[1..], &text[1..]),
        ch => !text.is_empty() && text[0] == ch && match_glob(&pattern[1..], &text[1..]),
    }
}

fn find_files(context: &ToolContext, pattern: &str, limit: usize) -> Result<String> {
    let root = workspace_root(context)?;
    let mut matches: Vec<String> = Vec::new();
    walk(&root, &mut |path| {
        if matches.len() >= limit {
            return;
        }
        let relative = relative_path(path, &root);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let hit = if pattern.contains('/') {
            glob_match(pattern, &relative)
        } else {
            glob_match(pattern, &name)
        };
        if hit {
            matches.push(relative);
        }
    })?;
    matches.sort();
    if matches.is_empty() {
        return Ok(format!("no files match \"{pattern}\""));
    }
    Ok(matches.join("\n"))
}

fn create_dir(path: &Path) -> Result<String> {
    std::fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
    Ok(format!("created folder {}", relative_name(path)))
}

fn relative_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn move_path(source: &Path, destination: &Path, root: &Path) -> Result<String> {
    if !source.exists() {
        return Err(Error::Other(format!(
            "{} does not exist",
            relative_path(source, root)
        )));
    }
    if source == root || destination == root {
        return Err(Error::Other(
            "refusing to move the workspace root itself".into(),
        ));
    }
    if destination.exists() {
        return Err(Error::Other(format!(
            "{} already exists",
            relative_path(destination, root)
        )));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    match std::fs::rename(source, destination) {
        Ok(()) => {}
        Err(_) if source.is_dir() => {
            copy_dir(source, destination)?;
            std::fs::remove_dir_all(source).map_err(|e| Error::io(source, e))?;
        }
        Err(_) => {
            std::fs::copy(source, destination).map_err(|e| Error::io(source, e))?;
            std::fs::remove_file(source).map_err(|e| Error::io(source, e))?;
        }
    }

    Ok(format!(
        "moved {} -> {}",
        relative_path(source, root),
        relative_path(destination, root)
    ))
}

fn copy_path(source: &Path, destination: &Path, root: &Path) -> Result<String> {
    if !source.exists() {
        return Err(Error::Other(format!(
            "{} does not exist",
            relative_path(source, root)
        )));
    }
    if source == root || destination == root {
        return Err(Error::Other(
            "refusing to copy the workspace root itself".into(),
        ));
    }
    if destination.exists() {
        return Err(Error::Other(format!(
            "{} already exists",
            relative_path(destination, root)
        )));
    }

    if source.is_dir() {
        copy_dir(source, destination)?;
    } else {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        std::fs::copy(source, destination).map_err(|e| Error::io(source, e))?;
    }

    Ok(format!(
        "copied {} -> {}",
        relative_path(source, root),
        relative_path(destination, root)
    ))
}

fn copy_dir(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination).map_err(|e| Error::io(destination, e))?;
    let reader = std::fs::read_dir(source).map_err(|e| Error::io(source, e))?;
    for entry in reader.flatten() {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        // Links are leaves here. Recursing through one would copy whatever it
        // points at — quite possibly outside the workspace — and a junction
        // aimed at its own parent would never finish.
        if from.is_dir() && !is_link(&from) {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| Error::io(&from, e))?;
        }
    }
    Ok(())
}

/// Deletes a path, preferring the Recycle Bin where there is one, and returns
/// a phrase naming where the bytes went so the model reports it accurately.
fn remove_path(target: &Path) -> Result<&'static str> {
    #[cfg(windows)]
    {
        // The shell owns the Recycle Bin, so anything that goes wrong here is
        // not fatal: fall through to the permanent delete the user asked for.
        // The returned phrase is what tells them which one happened.
        if recycle(target).is_ok() {
            return Ok("to the Recycle Bin");
        }
    }
    if target.is_dir() {
        std::fs::remove_dir_all(target).map_err(|e| Error::io(target, e))?;
    } else {
        std::fs::remove_file(target).map_err(|e| Error::io(target, e))?;
    }
    Ok("permanently")
}

/// Sends a path to the Recycle Bin.
///
/// `FOF_ALLOWUNDO` is the whole point: without it `SHFileOperation` erases the
/// way `remove_dir_all` does. `FOF_NORECURSEREPARSE` holds the shell to the
/// same rule as the rest of this module — never follow a junction.
#[cfg(windows)]
fn recycle(target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        SHFileOperationW, SHFILEOPSTRUCTW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
        FOF_NORECURSEREPARSE, FOF_SILENT, FO_DELETE,
    };

    // The shell wants a double-null-terminated list of paths, and it may be
    // reached from a worker thread where COM was never set up. Ignore the
    // result: RPC_E_CHANGED_MODE just means someone else chose an apartment
    // already.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

    let mut from: Vec<u16> = target.as_os_str().encode_wide().collect();
    from.push(0);
    from.push(0);

    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0
            | FOF_NOCONFIRMATION.0
            | FOF_NOERRORUI.0
            | FOF_SILENT.0
            | FOF_NORECURSEREPARSE.0) as u16,
        ..Default::default()
    };

    let code = unsafe { SHFileOperationW(&mut operation) };
    if code != 0 {
        return Err(Error::Other(format!(
            "the Recycle Bin refused {} (shell error {code})",
            target.display()
        )));
    }
    if operation.fAnyOperationsAborted.as_bool() {
        return Err(Error::Other(format!(
            "the Recycle Bin did not finish with {}",
            target.display()
        )));
    }
    Ok(())
}

/// Whether a path is a link — a symlink, or a Windows junction — and so must
/// not be walked or recursed through.
///
/// Two reasons this matters. Following one can leave the workspace entirely,
/// which is exactly what [`resolve`] exists to prevent. And a junction whose
/// target is its own parent makes any recursion unbounded: `count_entries`
/// would overflow the stack, and with `panic = "abort"` in the release profile
/// that does not unwind, it takes the whole process down.
fn is_link(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return true;
    }
    // Windows: treat any reparse point as a link, rather than trusting
    // `is_symlink` to cover junctions as well as symlinks.
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn count_entries(path: &Path) -> usize {
    let Ok(reader) = std::fs::read_dir(path) else {
        return 0;
    };
    reader
        .flatten()
        .map(|entry| {
            let child = entry.path();
            // A link counts as one entry and is never walked into: its target
            // is not part of this folder, and following it can recurse without
            // end.
            if child.is_dir() && !is_link(&child) {
                1 + count_entries(&child)
            } else {
                1
            }
        })
        .sum()
}

fn delete_path(target: &Path, root: &Path) -> Result<String> {
    if target == root {
        return Err(Error::Other(
            "refusing to delete the workspace root itself".into(),
        ));
    }
    if !target.exists() {
        return Err(Error::Other(format!(
            "{} does not exist",
            relative_path(target, root)
        )));
    }

    if target.is_dir() {
        let count = count_entries(target);
        let how = remove_path(target)?;
        Ok(format!(
            "deleted folder {} ({count} entries) {how}",
            relative_path(target, root)
        ))
    } else {
        let how = remove_path(target)?;
        Ok(format!("deleted {} {how}", relative_path(target, root)))
    }
}

fn run_git(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    // Hidden: `git` is a console program, and a visible child would flash a
    // window over whatever the user is doing.
    crate::process::hidden_std("git")
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| Error::Other(format!("could not run git: {e}")))
}

fn git_status(context: &ToolContext) -> Result<String> {
    let root = workspace_root(context)?;
    let output = run_git(&root, &["--no-pager", "status", "--porcelain=v1", "-b"])?;
    if output.status.code() != Some(0) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!("git: {}", stderr.trim())));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok("clean working tree".to_string());
    }
    Ok(truncate_output(&stdout))
}

fn git_diff(context: &ToolContext, staged: bool, path: Option<&str>) -> Result<String> {
    let root = workspace_root(context)?;
    let relative = match path {
        Some(path) => {
            let target = resolve(context, path)?;
            relative_path(&target, &root)
        }
        None => ".".to_string(),
    };

    let mut args: Vec<&str> = vec!["--no-pager", "diff", "--patch"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(&relative);

    let output = run_git(&root, &args)?;
    if output.status.code() != Some(0) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!("git: {}", stderr.trim())));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(if staged {
            "no staged changes".to_string()
        } else {
            "no uncommitted changes".to_string()
        });
    }
    Ok(truncate_output(&stdout))
}

fn git_log(context: &ToolContext, count: i64) -> Result<String> {
    let root = workspace_root(context)?;
    let limit = format!("-n{count}");
    let output = run_git(&root, &["--no-pager", "log", "--oneline", &limit])?;
    if output.status.code() != Some(0) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!("git: {}", stderr.trim())));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok("no commits yet".to_string());
    }
    Ok(truncate_output(&stdout))
}

/// Resolves a relative path inside the workspace and refuses escapes.
fn resolve(context: &ToolContext, relative: &str) -> Result<PathBuf> {
    let root = context
        .workdir
        .clone()
        .ok_or_else(|| Error::Other("this chat has no workspace folder set".into()))?;

    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(Error::Other("absolute paths are not allowed".into()));
    }

    let joined = root.join(candidate);
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }

    if !normalized.starts_with(&root) {
        return Err(Error::Other(format!(
            "\"{relative}\" is outside the workspace folder"
        )));
    }

    Ok(normalized)
}

fn list_dir(path: &Path) -> Result<String> {
    let mut entries: Vec<String> = Vec::new();
    let reader = std::fs::read_dir(path).map_err(|e| Error::io(path, e))?;
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "node_modules" || name == "target" {
            entries.push(format!("{name}/ (skipped)"));
            continue;
        }
        let kind = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            "dir "
        } else {
            "file"
        };
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        entries.push(format!("{kind} {name} ({size} bytes)"));
    }
    entries.sort();
    if entries.is_empty() {
        return Ok("(empty folder)".to_string());
    }
    Ok(entries.join("\n"))
}

fn read_file(path: &Path, start_line: Option<i64>, max_lines: Option<i64>) -> Result<String> {
    let metadata = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if metadata.is_dir() {
        return Err(Error::Other("that path is a folder, not a file".into()));
    }
    if metadata.len() > MAX_READ_BYTES {
        return Err(Error::Other(format!(
            "file is {} bytes, larger than the {} byte limit",
            metadata.len(),
            MAX_READ_BYTES
        )));
    }
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;

    if start_line.is_none() && max_lines.is_none() {
        return Ok(text);
    }

    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    if total == 0 {
        return Ok("(the file is empty)".to_string());
    }
    let first = start_line.unwrap_or(1).max(1) as usize;
    let start = (first - 1).min(total);
    let end = match max_lines {
        Some(count) => (start + count.max(1) as usize).min(total),
        None => total,
    };
    let body = lines[start..end].join("\n");
    Ok(format!(
        "lines {}-{} of {total}:\n{body}",
        start + 1,
        end
    ))
}

fn write_file(path: &Path, content: &str) -> Result<String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let existed = path.exists();
    std::fs::write(path, content).map_err(|e| Error::io(path, e))?;
    let bytes = content.len();
    Ok(format!(
        "{} {} ({} bytes)",
        if existed { "updated" } else { "created" },
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        bytes
    ))
}

fn edit_file(path: &Path, old: &str, new: &str) -> Result<String> {
    if old.is_empty() {
        return Err(Error::Other("old_string must not be empty".into()));
    }
    let current = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let occurrences = current.matches(old).count();
    if occurrences == 0 {
        return Err(Error::Other("old_string was not found in the file".into()));
    }
    if occurrences > 1 {
        return Err(Error::Other(format!(
            "old_string appears {occurrences} times; include more context to make it unique"
        )));
    }

    let updated = current.replacen(old, new, 1);
    std::fs::write(path, &updated).map_err(|e| Error::io(path, e))?;
    Ok(diff_preview(old, new))
}

/// Small +/- preview so the UI and the model can see what changed.
pub fn diff_preview(old: &str, new: &str) -> String {
    let mut out = String::new();
    for line in old.lines().take(12) {
        out.push_str("- ");
        out.push_str(line);
        out.push('\n');
    }
    if old.lines().count() > 12 {
        out.push_str("- …\n");
    }
    for line in new.lines().take(12) {
        out.push_str("+ ");
        out.push_str(line);
        out.push('\n');
    }
    if new.lines().count() > 12 {
        out.push_str("+ …\n");
    }
    out.trim_end().to_string()
}

/// Caps captured output at a size the model can use without drowning in it.
///
/// Shared with the engine, which reports both a finished command's streams and
/// what a still-running one has logged so far.
pub(crate) fn truncate_output(text: &str) -> String {
    const MAX: usize = 12_000;
    if text.len() <= MAX {
        return text.trim_end().to_string();
    }
    let mut cut = MAX;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}... [truncated]", &text[..cut])
}

fn grep(root: &Path, query: &str, include: Option<&str>) -> Result<String> {
    let needle = query.to_lowercase();
    let mut matches: Vec<String> = Vec::new();
    walk(root, &mut |path| {
        if matches.len() >= 200 {
            return;
        }
        if let Some(include) = include {
            let relative = relative_path(path, root);
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let hit = if include.contains('/') {
                glob_match(include, &relative)
            } else {
                glob_match(include, &name)
            };
            if !hit {
                return;
            }
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        for (index, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let relative = relative_path(path, root);
                matches.push(format!("{relative}:{}: {}", index + 1, line.trim()));
                if matches.len() >= 200 {
                    break;
                }
            }
        }
    })?;

    if matches.is_empty() {
        return Ok(format!("no matches for \"{query}\""));
    }
    Ok(matches.join("\n"))
}

fn walk(root: &Path, visit: &mut impl FnMut(&Path)) -> Result<()> {
    let reader = match std::fs::read_dir(root) {
        Ok(reader) => reader,
        Err(_) => return Ok(()),
    };
    for entry in reader.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), ".git" | "node_modules" | "target" | "dist") {
            continue;
        }
        if path.is_dir() {
            walk(&path, visit)?;
        } else {
            visit(&path);
        }
    }
    Ok(())
}

fn string_arg(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(root: &Path) -> ToolContext {
        ToolContext {
            workdir: Some(root.to_path_buf()),
            computer: false,
        }
    }

    /// A real repository, so the risk rule is exercised against git itself
    /// rather than against a mock of it.
    fn git(root: &Path, args: &[&str]) -> bool {
        crate::process::hidden_std("git")
            .args(args)
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    fn repo(root: &Path) {
        // `-b main` keeps the initial branch name out of the test's concerns.
        assert!(git(root, &["init", "-q", "-b", "main"]), "git init");
        assert!(git(root, &["config", "user.email", "t@example.com"]), "email");
        assert!(git(root, &["config", "user.name", "Test"]), "name");
    }

    #[test]
    fn delete_risk_asks_outside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "scratch").unwrap();

        let reason = delete_risk(r#"{"path":"notes.md"}"#, &context(dir.path()));
        let reason = reason.expect("nothing could restore this, so it must ask");
        assert!(reason.contains("notes.md"), "{reason}");
        assert!(reason.contains("version control"), "{reason}");
    }

    #[test]
    fn delete_risk_is_quiet_for_committed_content() {
        let dir = tempfile::tempdir().unwrap();
        repo(dir.path());
        std::fs::write(dir.path().join("kept.md"), "committed").unwrap();
        assert!(git(dir.path(), &["add", "kept.md"]), "add");
        assert!(git(dir.path(), &["commit", "-qm", "add kept.md"]), "commit");

        // `git restore` brings this straight back, so a card would be noise.
        assert_eq!(delete_risk(r#"{"path":"kept.md"}"#, &context(dir.path())), None);
    }

    #[test]
    fn delete_risk_asks_for_untracked_and_modified_content() {
        let dir = tempfile::tempdir().unwrap();
        repo(dir.path());
        std::fs::write(dir.path().join("tracked.md"), "one").unwrap();
        assert!(git(dir.path(), &["add", "tracked.md"]), "add");
        assert!(git(dir.path(), &["commit", "-qm", "add tracked.md"]), "commit");

        // Untracked: git has never seen these bytes.
        std::fs::write(dir.path().join("draft.md"), "unpublished").unwrap();
        let reason = delete_risk(r#"{"path":"draft.md"}"#, &context(dir.path()))
            .expect("untracked content must ask");
        assert!(reason.contains("draft.md"), "{reason}");

        // Modified: a commit exists, but not with these bytes in it.
        std::fs::write(dir.path().join("tracked.md"), "two").unwrap();
        let reason = delete_risk(r#"{"path":"tracked.md"}"#, &context(dir.path()))
            .expect("modified content must ask");
        assert!(reason.contains("tracked.md"), "{reason}");

        // A folder is judged the same way, and the count is the useful part.
        std::fs::write(dir.path().join("untouched.md"), "fine").unwrap();
        assert!(git(dir.path(), &["add", "untouched.md"]), "add");
        assert!(git(dir.path(), &["commit", "-qm", "add untouched.md"]), "commit");
        std::fs::create_dir_all(dir.path().join("mixed")).unwrap();
        std::fs::write(dir.path().join("mixed/clean.md"), "ok").unwrap();
        assert!(git(dir.path(), &["add", "mixed/clean.md"]), "add");
        assert!(git(dir.path(), &["commit", "-qm", "add mixed"]), "commit");
        std::fs::write(dir.path().join("mixed/dirty.md"), "uncommitted").unwrap();

        let reason = delete_risk(r#"{"path":"mixed"}"#, &context(dir.path()))
            .expect("a folder with uncommitted content must ask");
        assert!(reason.contains("uncommitted"), "{reason}");
        assert!(reason.contains("1 of the 2"), "{reason}");
    }

    #[test]
    fn delete_risk_asks_before_losing_repository_history() {
        let dir = tempfile::tempdir().unwrap();
        repo(dir.path());

        // `.git` is never "clean" in a useful sense: taking it away discards
        // every commit the git_* tools could revert to.
        let reason = delete_risk(r#"{"path":".git"}"#, &context(dir.path()))
            .expect("the repository's history must ask");
        assert!(reason.contains("history"), "{reason}");
    }

    #[test]
    fn nested_repositories_are_told_apart_from_ordinary_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("plain/deep")).unwrap();
        assert_eq!(nested_repository(&dir.path().join("plain")), Nested::None);

        // A child with a real `.git` directory: a repository of its own, whose
        // commits this workspace's history says nothing about.
        std::fs::create_dir_all(dir.path().join("clone/inner/.git")).unwrap();
        assert_eq!(nested_repository(&dir.path().join("clone")), Nested::Repository);

        // A child whose `.git` is a *file*: a submodule's work tree, recorded
        // in the parent only as a commit id.
        std::fs::create_dir_all(dir.path().join("sub/module")).unwrap();
        std::fs::write(dir.path().join("sub/module/.git"), "gitdir: ../.git/modules/x").unwrap();
        assert_eq!(nested_repository(&dir.path().join("sub")), Nested::Submodule);
    }

    #[test]
    fn delete_risk_does_not_follow_a_link_out_of_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "not yours").unwrap();

        // A junction needs no elevation on Windows, unlike a symlink.
        let link = dir.path().join("escape");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if !made {
            // A filesystem without reparse points cannot exercise this; the
            // walker's guard is still what `is_link` reports on below.
            eprintln!("skipping: this filesystem would not create a junction");
            return;
        }

        assert!(is_link(&link), "a junction must be recognised as a link");
        // Counting must terminate and must not descend into the target: the
        // link is one entry, and the file behind it is not in this folder.
        assert_eq!(count_entries(&dir.path()), 1);
    }

    #[test]
    fn plain_folders_are_not_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("real/inner")).unwrap();
        std::fs::write(dir.path().join("real/file.txt"), "x").unwrap();

        assert!(!is_link(&dir.path().join("real")));
        assert!(!is_link(&dir.path().join("real/file.txt")));
        assert_eq!(count_entries(&dir.path()), 1 + 2);
    }

    #[test]
    fn read_file_refuses_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let call = ToolCall {
            id: "1".into(),
            name: "read_file".into(),
            arguments: json!({ "path": "../secret.txt" }).to_string(),
        };
        let outcome = execute(&call, &context(dir.path()));
        assert!(!outcome.ok);
        assert!(outcome.output.contains("outside the workspace"));
    }

    #[test]
    fn read_file_reads_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "hello tools").unwrap();
        let call = ToolCall {
            id: "1".into(),
            name: "read_file".into(),
            arguments: json!({ "path": "notes.md" }).to_string(),
        };
        let outcome = execute(&call, &context(dir.path()));
        assert!(outcome.ok, "{}", outcome.output);
        assert_eq!(outcome.output, "hello tools");
    }

    #[test]
    fn tools_need_a_workspace() {
        let call = ToolCall {
            id: "1".into(),
            name: "list_dir".into(),
            arguments: "{}".into(),
        };
        let outcome = execute(&call, &ToolContext::default());
        assert!(!outcome.ok);
        assert!(outcome.output.contains("no workspace"));
    }

    #[test]
    fn grep_finds_lines_and_skips_heavy_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "alpha\nBETA\n").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        std::fs::write(dir.path().join("node_modules/b.txt"), "beta").unwrap();

        let call = ToolCall {
            id: "1".into(),
            name: "grep".into(),
            arguments: json!({ "query": "beta" }).to_string(),
        };
        let outcome = execute(&call, &context(dir.path()));
        assert!(outcome.ok);
        assert!(outcome.output.contains("a.txt:2"));
        assert!(!outcome.output.contains("node_modules"));
    }

    #[test]
    fn only_the_schedules_still_always_ask() {
        // Creating or deleting a schedule changes what Loom does while nobody
        // is watching, which is a different thing from deleting a file.
        assert!(always_asks(SCHEDULE_JOB));
        assert!(always_asks(DELETE_JOB));
        // A delete is judged by `delete_risk` instead, so the name alone no
        // longer forces a card.
        assert!(!always_asks(DELETE_PATH));
        assert!(!requires_confirmation(PermissionMode::AutoAll, DELETE_PATH));
        assert!(!requires_confirmation(PermissionMode::Atelier, DELETE_PATH));
    }

    #[test]
    fn permission_modes_gate_tools() {
        assert!(requires_confirmation(PermissionMode::Ask, "read_file"));
        assert!(!requires_confirmation(
            PermissionMode::AutoReadOnly,
            "read_file"
        ));
        assert!(requires_confirmation(
            PermissionMode::AutoReadOnly,
            "write_file"
        ));
        assert!(!requires_confirmation(
            PermissionMode::AutoAll,
            "write_file"
        ));
    }

    #[test]
    fn atelier_only_cards_its_five_deletes() {
        for name in [
            "run_command",
            "write_file",
            "upsert_persona",
            "upsert_prompt",
            "write_skill",
            "upsert_mcp_server",
            "test_mcp_server",
            "upsert_provider",
            "update_model",
            "update_settings",
            "list_harness",
        ] {
            assert!(
                !requires_confirmation(PermissionMode::Atelier, name),
                "{name} should run without a card in Atelier"
            );
        }
        for name in [
            "delete_persona",
            "delete_skill",
            "delete_prompt",
            "delete_provider",
            "delete_mcp_server",
        ] {
            assert!(
                requires_confirmation(PermissionMode::Atelier, name),
                "{name} must still ask in Atelier"
            );
        }
    }

    #[test]
    fn harness_tools_are_listed_only_in_atelier() {
        let harness: Vec<String> = crate::harness::specs()
            .iter()
            .map(|spec| spec.name.to_string())
            .collect();
        assert_eq!(harness.len(), 14);

        for name in &harness {
            assert!(
                !name.starts_with("mcp__"),
                "{name} must not collide with the MCP namespace"
            );
            assert!(
                !specs_for(PermissionMode::Ask)
                    .iter()
                    .any(|spec| spec.name == name),
                "{name} must not be listed outside Atelier"
            );
            assert!(
                specs_for(PermissionMode::Atelier)
                    .iter()
                    .any(|spec| spec.name == name),
                "{name} must be listed in Atelier"
            );
        }

        // And the harness tools declare their scope for the permission card.
        assert!(crate::harness::specs()
            .iter()
            .all(|spec| spec.scope == Some(ToolScope::Harness)));
    }

    #[test]
    fn plan_mode_refuses_harness_writers_and_allows_list_harness() {
        for name in [
            "upsert_persona",
            "delete_persona",
            "write_skill",
            "delete_skill",
            "update_settings",
        ] {
            assert!(is_blocked_in_plan(name), "{name} should be refused in Plan");
        }
        assert!(is_read_only("list_harness"));
        assert!(!is_read_only("upsert_persona"));
        assert!(!is_blocked_in_plan("list_harness"));
    }

    #[test]
    fn ask_user_is_never_gated() {
        for mode in [
            PermissionMode::Ask,
            PermissionMode::AutoReadOnly,
            PermissionMode::AutoAll,
            PermissionMode::Atelier,
        ] {
            assert!(!requires_confirmation(mode, ASK_USER));
        }
    }

    #[test]
    fn plan_mode_blocks_what_can_change_the_workspace() {
        for name in ["write_file", "edit_file", "run_command", STOP_COMMAND] {
            assert!(is_blocked_in_plan(name), "{name} should be refused");
        }
        for name in [
            "read_file",
            "grep",
            "list_dir",
            "search_workspace",
            "datetime",
            "generate_image",
            "spawn_agent",
            LIST_COMMANDS,
            COMMAND_OUTPUT,
            crate::web::SEARCH_TOOL,
            crate::web::FETCH_TOOL,
            ASK_USER,
        ] {
            assert!(!is_blocked_in_plan(name), "{name} should be allowed");
        }
    }

    #[test]
    fn the_command_tools_declare_what_they_do() {
        // Reading a log is a read (no card under Auto read-only, fine in Plan);
        // ending a process is not.
        for name in [LIST_COMMANDS, COMMAND_OUTPUT] {
            assert!(is_read_only(name), "{name} should be read-only");
            assert!(
                !requires_confirmation(PermissionMode::AutoReadOnly, name),
                "{name} should not need a card"
            );
        }
        assert!(!is_read_only(STOP_COMMAND));
        assert!(requires_confirmation(PermissionMode::AutoReadOnly, STOP_COMMAND));
        assert!(!requires_confirmation(PermissionMode::AutoAll, STOP_COMMAND));

        // Every new tool is offered, and none of them collides with the MCP
        // namespace ("mcp__server__tool").
        for name in ["run_command", LIST_COMMANDS, COMMAND_OUTPUT, STOP_COMMAND] {
            assert!(spec(name).is_some(), "{name} must be in specs()");
            assert!(!name.starts_with("mcp__"), "{name} collides with MCP");
        }
    }

    #[test]
    fn plan_mode_refuses_unknown_and_mcp_tools() {
        assert!(is_blocked_in_plan("mcp__files__write"));
        assert!(is_blocked_in_plan("some_future_tool"));
    }

    #[test]
    fn plan_refusal_names_the_tool_and_points_at_planning() {
        let refusal = plan_refusal("write_file");
        assert!(refusal.contains("Plan mode"), "{refusal}");
        assert!(refusal.contains("write_file"), "{refusal}");
        assert!(refusal.contains("Do not call it again"), "{refusal}");
    }

    #[test]
    fn ask_user_parses_options_and_shapes() {
        let question = AskQuestion::parse(
            &json!({
                "question": "Which database?",
                "header": "Database",
                "options": [
                    "Postgres",
                    { "label": "SQLite", "description": "single file" },
                    "Postgres"
                ]
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(question.question, "Which database?");
        assert_eq!(question.header.as_deref(), Some("Database"));
        // A bare string and an object both parse; the duplicate is dropped.
        assert_eq!(question.options.len(), 2);
        assert_eq!(
            question.options[1].description.as_deref(),
            Some("single file")
        );
        assert!(question.allow_free_text);
        assert!(!question.allow_multiple);
    }

    #[test]
    fn ask_user_rejects_questions_with_nothing_to_do() {
        assert!(AskQuestion::parse("{}").is_err());
        assert!(AskQuestion::parse(
            r#"{"question":"Pick one","options":[],"allow_free_text":false}"#
        )
        .is_err());
    }

    #[test]
    fn ask_user_output_carries_the_answer_back() {
        let question = AskQuestion::parse(r#"{"question":"Which database?"}"#).unwrap();
        let answered = question.to_tool_output(&Answer {
            selected: vec!["Postgres".into()],
            text: Some("not SQLite".into()),
            cancelled: false,
        });
        assert!(answered.contains("Postgres"), "{answered}");
        assert!(answered.contains("not SQLite"), "{answered}");

        let skipped = question.to_tool_output(&Answer {
            cancelled: true,
            ..Default::default()
        });
        assert!(skipped.contains("dismissed"), "{skipped}");
    }

    #[test]
    fn datetime_is_iso_like() {
        let call = ToolCall {
            id: "1".into(),
            name: "datetime".into(),
            arguments: "{}".into(),
        };
        let outcome = execute(&call, &ToolContext::default());
        assert!(outcome.ok);
        assert!(outcome.output.contains("UTC"));
        assert!(outcome.output.len() >= 20);
    }

    #[test]
    fn civil_dates_are_correct() {
        assert_eq!(crate::fsutil::civil_from_days(0), (1970, 1, 1));
        assert_eq!(crate::fsutil::civil_from_days(19_723), (2024, 1, 1));
    }
}
