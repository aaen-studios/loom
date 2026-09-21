//! The IDE surface: git, and the editor's file IO.
//!
//! Its own module rather than more of `commands.rs`, for the same reason
//! `panels.rs` exists: this is a self-contained surface with its own state and
//! its own event channel. Git operations and file writes are the two things in
//! Loom that can *change the user's disk*, so keeping them together makes the
//! complete list of such entry points readable in one file — which is worth
//! something when the question is "what can this app do to my working tree".
//!
//! ## The `loom://fs` channel
//!
//! Every command here that can alter a file emits on `loom://fs` afterwards,
//! carrying the workspace folder. The engine already emits `FilesChanged` on
//! `loom://event` when a *tool* touches something; this is the same signal for
//! changes the *user* makes through these panels — a checkout, a pull, a discard,
//! a commit that rewrites the index.
//!
//! It needs its own channel rather than reusing `loom://event` because that one
//! carries `EngineEvent`, which is the engine's vocabulary. A git checkout is not
//! an engine event and pretending it is would mean constructing engine state from
//! the shell layer to describe something the engine did not do.
//!
//! What listens: the editor, which restats its open buffers, and the git panel,
//! which refreshes status. Both are cheap and both need to be exact — a stale
//! editor after a checkout shows you the previous branch's file, which is the
//! kind of wrong that wastes an hour.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use loom_core::edit::{
    self, FileStat, LineEnding, SaveOutcome, TextFile, TreeEntry,
};
use loom_core::git::{
    self, Branch, Commit, CommitResult, GitStatus,
};

use crate::commands::AppState;

/// Payload for `loom://fs`.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FsWire {
    /// Which folder changed. A window showing a different one ignores it.
    workdir: String,
    /// What caused it, for the diagnostics line. Not branchable.
    reason: String,
}

/// Tells every window that a folder's files may have moved on.
fn tell_windows(app: &AppHandle, workdir: &str, reason: &str) {
    let _ = app.emit(
        "loom://fs",
        FsWire {
            workdir: workdir.to_string(),
            reason: reason.to_string(),
        },
    );
}

/* ---------------------------------------------------------------------------
   Git
--------------------------------------------------------------------------- */

/// Whether `git` can be run at all.
///
/// Asked once by the panel so a machine without git gets one sentence rather
/// than every button failing separately.
#[tauri::command]
pub fn git_available() -> bool {
    git::available()
}

#[tauri::command]
pub fn git_status(workdir: String) -> Result<GitStatus, String> {
    git::status(std::path::Path::new(&workdir)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_stage(
    app: AppHandle,
    workdir: String,
    paths: Vec<String>,
) -> Result<GitStatus, String> {
    let root = std::path::Path::new(&workdir);
    git::stage(root, &paths).map_err(|error| error.to_string())?;
    tell_windows(&app, &workdir, "stage");
    git::status(root).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_unstage(
    app: AppHandle,
    workdir: String,
    paths: Vec<String>,
) -> Result<GitStatus, String> {
    let root = std::path::Path::new(&workdir);
    git::unstage(root, &paths).map_err(|error| error.to_string())?;
    tell_windows(&app, &workdir, "unstage");
    git::status(root).map_err(|error| error.to_string())
}

/// Throws away uncommitted changes. Destructive, and the only command here that
/// can lose work that nothing can restore.
///
/// The confirmation is the **UI's**, not this function's: a Tauri command cannot
/// show a card, and silently relying on the caller to ask is exactly the kind of
/// implicit contract that breaks. So the panel always cards this one — the single
/// deliberate exception to Loom's usual "no card for something you asked for" —
/// and this function's job is to do precisely what it was told.
#[tauri::command]
pub fn git_discard(
    app: AppHandle,
    workdir: String,
    paths: Vec<String>,
) -> Result<GitStatus, String> {
    let root = std::path::Path::new(&workdir);
    git::discard(root, &paths).map_err(|error| error.to_string())?;
    tell_windows(&app, &workdir, "discard");
    git::status(root).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_commit(
    app: AppHandle,
    workdir: String,
    message: String,
) -> Result<CommitResult, String> {
    let root = std::path::Path::new(&workdir);
    let result = git::commit(root, &message).map_err(|error| error.to_string())?;
    // A commit rewrites the index, so the panel's staged list is stale and every
    // open buffer's notion of "changed" is too.
    tell_windows(&app, &workdir, "commit");
    Ok(result)
}

#[tauri::command]
pub fn git_branches(workdir: String) -> Result<Vec<Branch>, String> {
    git::branches(std::path::Path::new(&workdir)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_checkout(app: AppHandle, workdir: String, name: String) -> Result<GitStatus, String> {
    let root = std::path::Path::new(&workdir);
    git::checkout(root, &name).map_err(|error| error.to_string())?;
    // The most disruptive thing here: a checkout rewrites arbitrary files, and
    // an editor showing the old branch's contents is a real hazard.
    tell_windows(&app, &workdir, "checkout");
    git::status(root).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_create_branch(
    app: AppHandle,
    workdir: String,
    name: String,
    checkout: bool,
) -> Result<GitStatus, String> {
    let root = std::path::Path::new(&workdir);
    git::create_branch(root, &name, checkout).map_err(|error| error.to_string())?;
    if checkout {
        tell_windows(&app, &workdir, "branch");
    }
    git::status(root).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_fetch(workdir: String) -> Result<String, String> {
    git::fetch(std::path::Path::new(&workdir)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_pull(app: AppHandle, workdir: String) -> Result<String, String> {
    let summary = git::pull(std::path::Path::new(&workdir)).map_err(|error| error.to_string())?;
    tell_windows(&app, &workdir, "pull");
    Ok(summary)
}

#[tauri::command]
pub fn git_push(workdir: String) -> Result<String, String> {
    // No `fs` event: a push changes nothing on disk.
    git::push(std::path::Path::new(&workdir)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn git_log(workdir: String, count: Option<usize>) -> Result<Vec<Commit>, String> {
    git::log(
        std::path::Path::new(&workdir),
        count.unwrap_or(50).clamp(1, 500),
    )
    .map_err(|error| error.to_string())
}

/// The unified diff, which is what the commit-message pass reads and what the
/// panel shows as a fallback. The *editor's* diff view does not use this — it
/// compares two file versions, which is exact where a patch is a description.
#[tauri::command]
pub fn git_diff(workdir: String, staged: bool) -> Result<String, String> {
    git::diff(std::path::Path::new(&workdir), staged).map_err(|error| error.to_string())
}

/// A file's content at HEAD or in the index, for the diff view's "before" side.
///
/// `null` rather than an error when the path is absent from that revision: a new
/// file has no earlier version, and that is the ordinary case rather than a
/// failure.
#[tauri::command]
pub fn git_file_at(
    workdir: String,
    path: String,
    staged: bool,
) -> Result<Option<String>, String> {
    let root = std::path::Path::new(&workdir);
    let result = if staged {
        git::file_at_index(root, &path)
    } else {
        git::file_at_head(root, &path)
    };
    result.map_err(|error| error.to_string())
}

/// Writes a commit message for what is staged.
///
/// Takes the **session**, not a workdir, because the message has to come from
/// the chat's own model — its provider, its model id, its reasoning variant, its
/// API key — and a session id is the only thing that resolves all of those
/// consistently. An earlier signature took a workdir and read the *app's* default
/// model, so a chat switched to another model still had its messages written by
/// whatever the default happened to be.
///
/// Async because it makes a network call. A synchronous command would run on the
/// event-loop thread, and this repo has already been bitten by exactly that —
/// `send_message` was synchronous once, `tokio::spawn` panicked with "there is no
/// reactor running", and a panic in an IPC handler aborts the process. See the
/// note in `docs/spec.md`.
#[tauri::command]
pub async fn draft_commit_message(
    state: State<'_, AppState>,
    session_id: String,
    staged: Option<bool>,
) -> Result<String, String> {
    state
        .engine
        .draft_commit_message(&session_id, staged.unwrap_or(false))
        .await
        .map_err(|error| error.to_string())
}

/* ---------------------------------------------------------------------------
   Files
--------------------------------------------------------------------------- */

#[tauri::command]
pub fn file_read(
    workdir: Option<String>,
    path: String,
) -> Result<TextFile, String> {
    edit::read_text(workdir.as_deref(), &path).map_err(|error| error.to_string())
}

/// Saves a buffer.
///
/// `expected_hash` is the hash of what was loaded, and a mismatch comes back as
/// `SaveOutcome::Conflict` rather than an error — an expected outcome of
/// autosave that the UI branches on, not a failure. `None` means "write
/// unconditionally", which is what an explicit Save or a conflict's Overwrite
/// passes.
#[tauri::command]
pub fn file_save(
    app: AppHandle,
    workdir: Option<String>,
    path: String,
    text: String,
    expected_hash: Option<String>,
    eol: Option<LineEnding>,
    bom: Option<bool>,
) -> Result<SaveOutcome, String> {
    let outcome = edit::write_text(
        workdir.as_deref(),
        &path,
        &text,
        expected_hash.as_deref(),
        eol.unwrap_or(LineEnding::Lf),
        bom.unwrap_or(false),
    )
    .map_err(|error| error.to_string())?;

    if let (SaveOutcome::Written { .. }, Some(folder)) = (&outcome, workdir.as_deref()) {
        tell_windows(&app, folder, "save");
    }
    Ok(outcome)
}

/// Stats several paths at once, for the editor's freshness check.
///
/// Batched deliberately: autosave fires on a timer and an editor can have a
/// dozen tabs open, so one call per path would be a dozen IPC round trips per
/// keystroke pause.
#[tauri::command]
pub fn file_stat_many(
    workdir: Option<String>,
    paths: Vec<String>,
) -> Vec<FileStat> {
    paths
        .into_iter()
        .filter_map(|path| edit::stat(workdir.as_deref(), &path).ok())
        .collect()
}

#[tauri::command]
pub fn dir_list(
    workdir: Option<String>,
    path: Option<String>,
) -> Result<Vec<TreeEntry>, String> {
    edit::list_dir(workdir.as_deref(), path.as_deref().unwrap_or(""))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn file_create(
    app: AppHandle,
    workdir: Option<String>,
    path: String,
    is_dir: bool,
) -> Result<TreeEntry, String> {
    let entry = edit::create_entry(workdir.as_deref(), &path, is_dir)
        .map_err(|error| error.to_string())?;
    if let Some(folder) = workdir.as_deref() {
        tell_windows(&app, folder, "create");
    }
    Ok(entry)
}

#[tauri::command]
pub fn file_rename(
    app: AppHandle,
    workdir: Option<String>,
    from: String,
    to: String,
) -> Result<TreeEntry, String> {
    let entry = edit::rename_entry(workdir.as_deref(), &from, &to)
        .map_err(|error| error.to_string())?;
    if let Some(folder) = workdir.as_deref() {
        tell_windows(&app, folder, "rename");
    }
    Ok(entry)
}

/// Deletes a path through the same risk check the agent's `delete_path` uses.
///
/// Deliberately *not* a plain `remove_file`: deleting inside a repository is
/// recoverable through git and outside one it is not, and that distinction is
/// already encoded in `tools::delete_risk`. Reusing it means the panel and the
/// model make the same judgement about what is at stake, rather than the panel
/// being the careless path.
#[tauri::command]
pub fn file_delete(
    app: AppHandle,
    workdir: Option<String>,
    path: String,
) -> Result<(), String> {
    let Some(folder) = workdir.as_deref() else {
        return Err("this chat has no workspace folder set".into());
    };
    loom_core::tools::delete_in_workspace(folder, &path).map_err(|error| error.to_string())?;
    tell_windows(&app, folder, "delete");
    Ok(())
}

/// Whether a path exists, for the tree's refresh after a delete.
#[tauri::command]
pub fn file_exists(workdir: Option<String>, path: String) -> bool {
    edit::exists(workdir.as_deref(), &path).unwrap_or(false)
}
