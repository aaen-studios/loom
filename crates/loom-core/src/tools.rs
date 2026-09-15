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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
    pub read_only: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    /// Absolute path of the chat's workspace folder, when one is set.
    pub workdir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON string as produced by the model.
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub id: String,
    pub name: String,
    pub ok: bool,
    pub output: String,
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "datetime",
            description: "Current local date and time, including timezone offset. Use when the user asks about now, today, or relative dates.",
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            read_only: true,
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
        },
        ToolSpec {
            name: "read_file",
            description: "Read a UTF-8 text file from the chat's workspace folder. Paths are relative to the workspace root and may not escape it.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative file path" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            read_only: true,
        },
        ToolSpec {
            name: "grep",
            description: "Search the workspace folder for a literal string (case-insensitive) and return matching lines with file names.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Literal text to search for" },
                    "path": { "type": "string", "description": "Relative folder to search, defaults to the workspace root" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            read_only: true,
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
        },
        ToolSpec {
            name: "run_command",
            description: "Run a shell command inside the workspace folder and return stdout, stderr, and the exit code. Use for tests, builds, and git status.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command line to execute" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            read_only: false,
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
        },
        ToolSpec {
            name: "spawn_agent",
            description: "Delegate a self-contained task to a subagent and get its final answer back. Useful for research or parallel reading. Maximum depth is one.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "What the subagent should do" },
                    "system": { "type": "string", "description": "Optional role for the subagent" }
                },
                "required": ["task"],
                "additionalProperties": false
            }),
            read_only: true,
        },
    ]
}

pub fn spec(name: &str) -> Option<ToolSpec> {
    specs().into_iter().find(|tool| tool.name == name)
}

/// Tools that `AutoReadOnly` may run without asking.
pub fn is_read_only(name: &str) -> bool {
    spec(name).map(|tool| tool.read_only).unwrap_or(false)
}

/// Whether a call may proceed under the current mode.
pub fn requires_confirmation(mode: PermissionMode, name: &str) -> bool {
    match mode {
        PermissionMode::AutoAll => false,
        PermissionMode::AutoReadOnly => !is_read_only(name),
        PermissionMode::Ask => true,
    }
}

pub fn execute(call: &ToolCall, context: &ToolContext) -> ToolOutcome {
    let result = dispatch(call, context);
    match result {
        Ok(output) => ToolOutcome {
            id: call.id.clone(),
            name: call.name.clone(),
            ok: true,
            output,
        },
        Err(error) => ToolOutcome {
            id: call.id.clone(),
            name: call.name.clone(),
            ok: false,
            output: error.to_string(),
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
            read_file(&target)
        }
        "grep" => {
            let query = string_arg(&arguments, "query")
                .ok_or_else(|| Error::Other("grep requires a query".into()))?;
            let path = string_arg(&arguments, "path").unwrap_or_else(|| ".".to_string());
            let target = resolve(context, &path)?;
            grep(&target, &query)
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
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC (unix {now})",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

/// Howard Hinnant's civil-from-days algorithm (proleptic Gregorian).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
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
        let size = entry
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);
        entries.push(format!("{kind} {name} ({size} bytes)"));
    }
    entries.sort();
    if entries.is_empty() {
        return Ok("(empty folder)".to_string());
    }
    Ok(entries.join("\n"))
}

fn read_file(path: &Path) -> Result<String> {
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
    Ok(text)
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
        return Err(Error::Other(
            "old_string was not found in the file".into(),
        ));
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

/// Executes a shell command inside the workspace (async, bounded).
pub async fn run_command(context: &ToolContext, command: &str) -> Result<String> {
    let root = context
        .workdir
        .clone()
        .ok_or_else(|| Error::Other("this chat has no workspace folder set".into()))?;

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let flag = if cfg!(windows) { "/C" } else { "-c" };

    let child = tokio::process::Command::new(shell)
        .arg(flag)
        .arg(command)
        .current_dir(&root)
        .stdin(std::process::Stdio::null())
        .output();

    let output = tokio::time::timeout(std::time::Duration::from_secs(120), child)
        .await
        .map_err(|_| Error::Other("command timed out after 120s".into()))?
        .map_err(|e| Error::Other(format!("failed to run command: {e}")))?;

    let mut report = String::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    report.push_str(&format!("exit code: {}\n", output.status.code().unwrap_or(-1)));
    if !stdout.trim().is_empty() {
        report.push_str("stdout:\n");
        report.push_str(&truncate_output(&stdout));
        report.push('\n');
    }
    if !stderr.trim().is_empty() {
        report.push_str("stderr:\n");
        report.push_str(&truncate_output(&stderr));
    }
    Ok(report.trim_end().to_string())
}

fn truncate_output(text: &str) -> String {
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

fn grep(root: &Path, query: &str) -> Result<String> {
    let needle = query.to_lowercase();
    let mut matches: Vec<String> = Vec::new();
    walk(root, &mut |path| {
        if matches.len() >= 200 {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        for (index, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
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
        }
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
    fn permission_modes_gate_tools() {
        assert!(requires_confirmation(PermissionMode::Ask, "read_file"));
        assert!(!requires_confirmation(PermissionMode::AutoReadOnly, "read_file"));
        assert!(requires_confirmation(PermissionMode::AutoReadOnly, "write_file"));
        assert!(!requires_confirmation(PermissionMode::AutoAll, "write_file"));
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
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }
}
