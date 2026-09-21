//! Structured git, over the system CLI.
//!
//! ## Why the CLI and not a library
//!
//! The agent's `git_status` / `git_diff` / `git_log` tools already shell out to
//! `git`, so this shares that path rather than adding a second implementation of
//! the same idea. What the CLI buys beyond "less code": Git Credential Manager
//! handles push and pull authentication for free, and the user's own config,
//! hooks, aliases, SSH keys, signing and LFS all simply work, because it *is*
//! their git.
//!
//! What it costs: `git` has to be on PATH. [`available`] asks once so the UI can
//! say so plainly instead of failing strangely, and every command here reports
//! git's own stderr rather than inventing a message.
//!
//! ## Why `--porcelain=v2 -z`
//!
//! The v1 porcelain format cannot express an unmerged entry honestly, and it
//! separates rename pairs with a tab inside a single record — so a path
//! containing a newline, a tab, or a quote is either mangled or requires the
//! `-z` NUL form to survive. v2 with `-z` is the only shape that gives all
//! three: real conflict codes, machine-readable rename pairs, and paths that
//! arrive as the bytes they actually are. Parsing anything else would mean
//! quietly losing files whose names contain the wrong character.
//!
//! Every process is spawned through [`crate::process::hidden_std`], so no git
//! command ever throws a console window over the desktop.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{process, workspace, Error, Result};

/// How long a network operation may take before it is killed.
///
/// `fetch`, `pull` and `push` can block on a credential prompt. With no console
/// and stdin closed, a prompt cannot be answered, so the honest outcome is a
/// timeout with a message rather than a turn that hangs until the app is closed.
/// A minute and a half is far longer than a real transfer and far shorter than
/// "the UI has stopped responding".
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(90);

/// Local operations are fast; a hang means something is actually wrong.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(20);

/// One path's state, as the UI needs to draw it.
///
/// The four booleans are deliberately not a single enum. An entry with changes
/// in *both* the index and the work tree is ordinary — stage a file, edit it
/// again — and a `ChangeKind` would force that to be either "staged" or
/// "modified", which is exactly the simplification that makes such a file
/// disappear from one of the two lists a commit box shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFile {
    /// Path relative to the repository root, always `/`-separated.
    pub path: String,
    /// For a rename or copy, where it came from. `None` otherwise.
    pub from: Option<String>,
    /// The index differs from HEAD.
    pub staged: bool,
    /// The work tree differs from the index.
    pub unstaged: bool,
    /// git has never seen this path.
    pub untracked: bool,
    /// An unmerged entry — a conflict that must be resolved before committing.
    pub conflicted: bool,
}

impl GitFile {
    fn new(path: String) -> Self {
        Self {
            path,
            from: None,
            staged: false,
            unstaged: false,
            untracked: false,
            conflicted: false,
        }
    }
}

/// The whole working-tree state, in one call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub is_repo: bool,
    /// The repository root, which may be above the chat's own folder.
    pub root: Option<String>,
    pub branch: Option<String>,
    /// True when HEAD is a commit rather than a branch.
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// `rebase`, `merge`, `cherry-pick` or `revert` while one is in progress.
    /// A commit box over a half-finished rebase deserves to say so.
    pub operation: Option<String>,
    pub files: Vec<GitFile>,
}

impl GitStatus {
    /// Not a repository: the state a folder with no `.git` reports.
    pub fn none() -> Self {
        Self {
            is_repo: false,
            root: None,
            branch: None,
            detached: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            operation: None,
            files: Vec::new(),
        }
    }

    /// Entries with something staged, in path order.
    pub fn staged(&self) -> Vec<&GitFile> {
        self.files.iter().filter(|file| file.staged).collect()
    }

    /// Entries with an unstaged or untracked change.
    pub fn unstaged(&self) -> Vec<&GitFile> {
        self.files
            .iter()
            .filter(|file| file.unstaged || file.untracked)
            .collect()
    }
}

/// A local branch, for the branch picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub current: bool,
    /// Tracking a remote branch of the same name.
    pub upstream: Option<String>,
}

/// One line of history, for the panel's log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    /// Abbreviated.
    pub id: String,
    pub subject: String,
    pub author: String,
    /// Unix seconds.
    pub at: i64,
}

/// Whether `git` can be run at all.
///
/// Asked once and cached by the caller. A machine without git gets one clear
/// sentence in the panel instead of every button failing separately.
pub fn available() -> bool {
    match process::hidden_std("git")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// The repository root at or above `workdir`, if there is one.
pub fn root_of(workdir: &Path) -> Option<PathBuf> {
    workspace::repo_root(workdir)
}

fn root(workdir: &Path) -> Result<PathBuf> {
    workspace::repo_root(workdir)
        .ok_or_else(|| Error::Other("this folder is not a git repository".into()))
}

/// Runs git and returns stdout, treating a non-zero exit as an error.
///
/// stderr is the payload on failure: git's own words describe the problem far
/// better than anything this module could invent, and re-wording it is how a
/// "nothing to commit" becomes a mystery.
fn run(root: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let output = run_raw(root, args, timeout)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let message = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(Error::Other(format!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            if message.is_empty() {
                format!("exited with {:?}", output.status.code())
            } else {
                message
            }
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs git and returns raw bytes, so `-z` output is not lossily decoded.
fn run_bytes(root: &Path, args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    Ok(run_raw(root, args, timeout)?.stdout)
}

/// The one place a git process is actually spawned.
///
/// Spawned rather than `output()`-ed because `output()` cannot be given a
/// deadline, and a `push` waiting on a credential prompt with no console and no
/// stdin is a process that never returns. stdout and stderr are drained on their
/// own threads — reading them only after the child exits is the classic deadlock,
/// since a pipe that fills blocks the writer and the writer is what would have
/// been waited for.
fn run_raw(
    root: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output> {
    use std::process::Stdio;

    let mut child = process::hidden_std("git")
        .args(args)
        .current_dir(root)
        // Closed stdin: a credential prompt fails immediately instead of
        // blocking on a console that does not exist.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| Error::Other(format!("could not run git: {error}")))?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let out_handle = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });
    let err_handle = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() > timeout {
                    // Best effort: the process tree goes, and the caller gets a
                    // sentence that says what happened rather than a hang.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Other(
                        "git timed out — a credential or passphrase prompt cannot be \
                         answered from inside Loom. Run the command in the terminal \
                         panel once, or configure a credential helper."
                            .into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(Error::Other(format!("git could not be waited on: {error}")));
            }
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/* ---------------------------------------------------------------------------
   Reading state
--------------------------------------------------------------------------- */

/// The working tree, the branch, and how far it has diverged.
pub fn status(workdir: &Path) -> Result<GitStatus> {
    let Some(root) = root_of(workdir) else {
        return Ok(GitStatus::none());
    };

    // `--porcelain=v2 -z -b`: branch headers, then one NUL-terminated record
    // per path. `--untracked-files=all` because a directory collapsed to
    // `dir/` cannot be staged file by file from the panel, and the panel is the
    // point.
    let raw = run_bytes(
        &root,
        &[
            "--no-pager",
            "status",
            "--porcelain=v2",
            "-z",
            "-b",
            "--untracked-files=all",
        ],
        LOCAL_TIMEOUT,
    )?;

    let mut status = parse_status(&raw);
    status.is_repo = true;
    status.root = Some(root.to_string_lossy().into_owned());
    status.operation = operation_in_progress(&root);
    Ok(status)
}

/// The branch, without the file list. Cheap enough to call on a timer.
pub fn branch(workdir: &Path) -> Result<Option<String>> {
    let Some(root) = root_of(workdir) else {
        return Ok(None);
    };
    let text = run(
        &root,
        &["--no-pager", "rev-parse", "--abbrev-ref", "HEAD"],
        LOCAL_TIMEOUT,
    )?;
    let name = text.trim();
    // A detached HEAD does not report "HEAD" as a branch.
    Ok(if name.is_empty() || name == "HEAD" {
        None
    } else {
        Some(name.to_string())
    })
}

/// Whether a merge, rebase, cherry-pick or revert is half-finished.
///
/// Read from `.git` directly rather than from git, because there is no plumbing
/// command that reports "an operation is in progress" — and a commit box that
/// does not know about a rebase will happily offer to commit into one.
fn operation_in_progress(root: &Path) -> Option<String> {
    // A worktree's `.git` is a file pointing elsewhere, so ask git where the
    // real directory is rather than guessing.
    let git_dir = run(root, &["rev-parse", "--git-dir"], LOCAL_TIMEOUT)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .map(|text| {
            let path = PathBuf::from(&text);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join(".git"));

    for (marker, name) in [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
    ] {
        if git_dir.join(marker).exists() {
            return Some(name.to_string());
        }
    }
    None
}

/// Parses `git status --porcelain=v2 -z -b`.
///
/// Split out from [`status`] so it can be tested against fixtures — the wire
/// format is the part that can be quietly wrong, and a unit test is the only way
/// to assert a rename pair without building a repository that produces one.
fn parse_status(raw: &[u8]) -> GitStatus {
    let mut status = GitStatus::none();
    status.is_repo = true;

    // NUL-separated, so a path containing a newline survives intact. Fields are
    // walked with an index rather than iterated, because a rename record owns
    // the *next* field: the original path.
    let fields: Vec<&[u8]> = raw.split(|byte| *byte == 0).collect();
    let mut index = 0;

    while index < fields.len() {
        let field = fields[index];
        index += 1;
        if field.is_empty() {
            continue;
        }
        // Anything that is not valid UTF-8 in the leading byte is not a record
        // header; skip it rather than guessing.
        let Ok(line) = std::str::from_utf8(field) else {
            continue;
        };

        if let Some(header) = line.strip_prefix("# ") {
            parse_header(header, &mut status);
            continue;
        }

        let mut chars = line.chars();
        match chars.next() {
            // Ordinary change: `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`
            Some('1') => {
                let parts: Vec<&str> = line.splitn(9, ' ').collect();
                if parts.len() < 9 {
                    continue;
                }
                let mut file = GitFile::new(normalise_path(parts[8]));
                apply_xy(&mut file, parts[1]);
                status.files.push(file);
            }
            // Rename or copy: `2 <XY> ... <X><score> <path>` then the original.
            Some('2') => {
                let parts: Vec<&str> = line.splitn(10, ' ').collect();
                if parts.len() < 10 {
                    continue;
                }
                let mut file = GitFile::new(normalise_path(parts[9]));
                apply_xy(&mut file, parts[1]);
                // The original path is the record's *own* field when `-z` is in
                // use, not a tab-separated tail.
                if let Some(origin) = fields.get(index) {
                    index += 1;
                    if let Ok(origin) = std::str::from_utf8(origin) {
                        if !origin.is_empty() {
                            file.from = Some(normalise_path(origin));
                        }
                    }
                }
                status.files.push(file);
            }
            // Unmerged: `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
            Some('u') => {
                let parts: Vec<&str> = line.splitn(11, ' ').collect();
                if parts.len() < 11 {
                    continue;
                }
                let mut file = GitFile::new(normalise_path(parts[10]));
                file.conflicted = true;
                status.files.push(file);
            }
            // Untracked.
            Some('?') => {
                let path = line[2.min(line.len())..].to_string();
                let mut file = GitFile::new(normalise_path(&path));
                file.untracked = true;
                status.files.push(file);
            }
            // Ignored: not shown, deliberately. A commit box listing every
            // build artefact is a commit box nobody reads.
            _ => {}
        }
    }

    status.files.sort_by(|a, b| a.path.cmp(&b.path));
    status
}

fn parse_header(header: &str, status: &mut GitStatus) {
    if let Some(value) = header.strip_prefix("branch.head ") {
        let value = value.trim();
        if value == "(detached)" {
            status.detached = true;
            status.branch = None;
        } else {
            status.branch = Some(value.to_string());
        }
    } else if let Some(value) = header.strip_prefix("branch.upstream ") {
        status.upstream = Some(value.trim().to_string());
    } else if let Some(value) = header.strip_prefix("branch.ab ") {
        // `+<ahead> -<behind>`
        for part in value.split_whitespace() {
            if let Some(count) = part.strip_prefix('+') {
                status.ahead = count.parse().unwrap_or(0);
            } else if let Some(count) = part.strip_prefix('-') {
                status.behind = count.parse().unwrap_or(0);
            }
        }
    }
}

/// Reads a two-character `XY` into the flags.
///
/// `X` is the index against HEAD and `Y` the work tree against the index, so
/// both can be set and both mean different lists in the panel.
fn apply_xy(file: &mut GitFile, xy: &str) {
    let mut chars = xy.chars();
    let index = chars.next().unwrap_or('.');
    let tree = chars.next().unwrap_or('.');
    file.staged = index != '.' && index != ' ';
    file.unstaged = tree != '.' && tree != ' ';
}

/// git reports paths with `/` on every platform; keep that, so a stored path is
/// the same string on both and only the filesystem call converts.
fn normalise_path(path: &str) -> String {
    path.replace('\\', "/")
}

/* ---------------------------------------------------------------------------
   Staging
--------------------------------------------------------------------------- */

/// Stages paths, or everything when the list is empty.
pub fn stage(workdir: &Path, paths: &[String]) -> Result<()> {
    let root = root(workdir)?;
    let mut args: Vec<&str> = vec!["add", "--"];
    // `-A` so a *deletion* can be staged from the panel. `git add <path>` alone
    // stages a modification but silently ignores the file being gone, which
    // would make a deleted file impossible to commit through the UI.
    if paths.is_empty() {
        args = vec!["add", "-A"];
    } else {
        for path in paths {
            args.push(path);
        }
    }
    run(&root, &args, LOCAL_TIMEOUT)?;
    Ok(())
}

/// Removes paths from the index, leaving the work tree alone.
///
/// `git restore --staged` rather than `git reset HEAD --`: `reset` is the one
/// command whose meaning depends on the shape of its arguments, and in the
/// no-commit case it fails instead of un-staging. `restore` says what it does.
pub fn unstage(workdir: &Path, paths: &[String]) -> Result<()> {
    let root = root(workdir)?;
    if paths.is_empty() {
        // No HEAD (a brand-new repo) means there is nothing to restore *from*,
        // so the index is emptied instead.
        let has_head = run(&root, &["rev-parse", "--verify", "HEAD"], LOCAL_TIMEOUT).is_ok();
        if has_head {
            run(&root, &["restore", "--staged", "--", "."], LOCAL_TIMEOUT)?;
        } else {
            run(&root, &["rm", "--cached", "-r", "--", "."], LOCAL_TIMEOUT)?;
        }
        return Ok(());
    }
    // A repository with no commits has no HEAD for `restore --staged` to
    // restore *from*, and it says so with `fatal: could not resolve HEAD`. That
    // is not an edge case: staging the first file in a fresh repo and changing
    // your mind is the first thing anyone does. With no HEAD the index is
    // emptied instead, which means removing entries rather than restoring them —
    // and only the ones that are actually staged, since `rm --cached` on a path
    // that is not in the index is an error of its own.
    let has_head = run(&root, &["rev-parse", "--verify", "HEAD"], LOCAL_TIMEOUT).is_ok();
    if !has_head {
        let status = status(workdir)?;
        let staged: Vec<&String> = paths
            .iter()
            .filter(|path| {
                status
                    .files
                    .iter()
                    .any(|file| &file.path == *path && file.staged)
            })
            .collect();
        if staged.is_empty() {
            return Ok(());
        }
        let mut args: Vec<&str> = vec!["rm", "--cached", "-r", "--"];
        for path in staged {
            args.push(path.as_str());
        }
        run(&root, &args, LOCAL_TIMEOUT)?;
        return Ok(());
    }

    let mut args: Vec<&str> = vec!["restore", "--staged", "--"];
    for path in paths {
        args.push(path);
    }
    run(&root, &args, LOCAL_TIMEOUT)?;
    Ok(())
}

/// Throws away uncommitted changes to paths. **Destructive.**
///
/// Untracked files are removed as well, because "discard" on a new file means
/// exactly that — but only the ones named, never a directory sweep. The caller
/// confirms first; nothing here asks, since a Rust module cannot show a card.
pub fn discard(workdir: &Path, paths: &[String]) -> Result<()> {
    let root = root(workdir)?;
    let status = status(workdir)?;
    let mut tracked: Vec<&String> = Vec::new();
    let mut untracked: Vec<&String> = Vec::new();
    for path in paths {
        match status.files.iter().find(|file| &file.path == path) {
            Some(file) if file.untracked => untracked.push(path),
            Some(_) => tracked.push(path),
            None => {}
        }
    }

    if !tracked.is_empty() {
        let mut args: Vec<&str> = vec!["restore", "--worktree", "--"];
        for path in &tracked {
            args.push(path.as_str());
        }
        run(&root, &args, LOCAL_TIMEOUT)?;
    }
    // `clean` takes no path list in the same shape: it wants `--` and then the
    // paths, and it refuses to remove anything not matched by `-f`.
    if !untracked.is_empty() {
        let mut args: Vec<&str> = vec!["clean", "-f", "--"];
        for path in &untracked {
            args.push(path.as_str());
        }
        run(&root, &args, LOCAL_TIMEOUT)?;
    }
    Ok(())
}

/* ---------------------------------------------------------------------------
   Committing
--------------------------------------------------------------------------- */

/// What a commit did, for the panel's confirmation line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub id: String,
    pub subject: String,
    pub files: usize,
}

/// Commits what is staged.
///
/// The message goes in through `-F -` rather than `-m`, because `-m` on Windows
/// has to survive `cmd`'s quoting rules and a message containing a quote or a
/// newline is exactly what a Conventional Commits body is. Reading from stdin
/// sidesteps the shell entirely.
///
/// **Nothing is committed when nothing is staged.** git would happily make an
/// empty commit with `--allow-empty`, and a button that can do that is a button
/// that can litter a history.
///
/// Hooks run, because this is the user's git. A failing pre-commit hook is
/// surfaced as its own error, output and all.
pub fn commit(workdir: &Path, message: &str) -> Result<CommitResult> {
    let root = root(workdir)?;
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(Error::Other("a commit needs a message".into()));
    }

    let status = status(workdir)?;
    if status.files.iter().all(|file| !file.staged) {
        return Err(Error::Other(
            "nothing is staged — stage a file before committing".into(),
        ));
    }
    if status.files.iter().any(|file| file.conflicted) {
        return Err(Error::Other(
            "this repository has unresolved conflicts — resolve them first".into(),
        ));
    }

    commit_with_message(&root, trimmed)?;

    let id = run(&root, &["rev-parse", "--short", "HEAD"], LOCAL_TIMEOUT)?;
    let id = id.trim().to_string();
    let subject = run(
        &root,
        &["--no-pager", "log", "-1", "--pretty=%s"],
        LOCAL_TIMEOUT,
    )?;
    let files = status.files.iter().filter(|file| file.staged).count();

    Ok(CommitResult {
        id,
        subject: subject.trim().to_string(),
        files,
    })
}

/// Writes the message to git's stdin and commits.
fn commit_with_message(root: &Path, message: &str) -> Result<()> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = process::hidden_std("git")
        .args(["commit", "--file", "-"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| Error::Other(format!("could not run git: {error}")))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(message.as_bytes())
            .map_err(|error| Error::Other(format!("could not write the message: {error}")))?;
    }
    // Dropping stdin is what tells git the message is complete.
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .map_err(|error| Error::Other(format!("git commit failed: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(Error::Other(if text.is_empty() {
            "git commit failed".into()
        } else {
            text
        }));
    }
    Ok(())
}

/* ---------------------------------------------------------------------------
   Branches
--------------------------------------------------------------------------- */

/// Local branches, current first, then most recently committed.
pub fn branches(workdir: &Path) -> Result<Vec<Branch>> {
    let root = root(workdir)?;
    // `%(upstream:short)` is empty when there is none, which is exactly the
    // absence the UI wants to render as "not tracking anything".
    let text = run(
        &root,
        &[
            "--no-pager",
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)%09%(upstream:short)%09%(HEAD)",
            "refs/heads",
        ],
        LOCAL_TIMEOUT,
    )?;

    let mut branches = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let name = parts.next().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let upstream = parts.next().unwrap_or("").trim();
        let head = parts.next().unwrap_or("").trim();
        branches.push(Branch {
            name: name.to_string(),
            current: head == "*",
            upstream: if upstream.is_empty() {
                None
            } else {
                Some(upstream.to_string())
            },
        });
    }
    branches.sort_by(|a, b| b.current.cmp(&a.current).then(a.name.cmp(&b.name)));
    Ok(branches)
}

/// Switches branches.
pub fn checkout(workdir: &Path, name: &str) -> Result<()> {
    let root = root(workdir)?;
    run(&root, &["checkout", name], LOCAL_TIMEOUT)?;
    Ok(())
}

/// Creates a branch, optionally switching to it.
pub fn create_branch(workdir: &Path, name: &str, checkout_after: bool) -> Result<()> {
    let root = root(workdir)?;
    if checkout_after {
        run(&root, &["checkout", "-b", name], LOCAL_TIMEOUT)?;
    } else {
        run(&root, &["branch", name], LOCAL_TIMEOUT)?;
    }
    Ok(())
}

/* ---------------------------------------------------------------------------
   Remotes
--------------------------------------------------------------------------- */

/// Whether the repository has any remote configured.
///
/// Asked before push/pull so the panel can say "this repo has no remote" rather
/// than surfacing git's `fatal: No configured push destination`, which reads
/// like a Loom bug.
pub fn has_remote(workdir: &Path) -> Result<bool> {
    let root = root(workdir)?;
    let text = run(&root, &["remote"], LOCAL_TIMEOUT)?;
    Ok(!text.trim().is_empty())
}

/// Fetches from the default remote, pruning deleted branches.
pub fn fetch(workdir: &Path) -> Result<String> {
    let root = root(workdir)?;
    let text = run(&root, &["fetch", "--prune"], NETWORK_TIMEOUT)?;
    Ok(summarise(&text, "fetched"))
}

/// Pulls, rebasing local commits on top rather than creating a merge.
///
/// `--rebase` because this is a panel with a *commit* button: a user who commits
/// locally and then pulls should not be handed a merge commit for their trouble,
/// and a linear history is what a small repo wants. A repository with a real
/// merge workflow still merges — `pull.rebase` is the user's own config to set,
/// and passing the flag only covers the case where nothing has decided.
pub fn pull(workdir: &Path) -> Result<String> {
    let root = root(workdir)?;
    let text = run(&root, &["pull", "--rebase"], NETWORK_TIMEOUT)?;
    Ok(summarise(&text, "already up to date"))
}

/// Pushes the current branch, setting its upstream on the first push.
pub fn push(workdir: &Path) -> Result<String> {
    let root = root(workdir)?;
    let status = status(workdir)?;
    let branch = status
        .branch
        .clone()
        .ok_or_else(|| Error::Other("HEAD is detached, so there is no branch to push".into()))?;

    let text = if status.upstream.is_some() {
        run(&root, &["push"], NETWORK_TIMEOUT)?
    } else {
        // First push of a new branch. Without `--set-upstream` git prints an
        // instruction to run the command again, which is a poor thing to do to
        // someone who just pressed a button.
        run(&root, &["push", "--set-upstream", "origin", &branch], NETWORK_TIMEOUT)?
    };
    Ok(summarise(&text, "pushed"))
}

/// git writes progress to stderr and little to stdout; a one-line summary beats
/// dumping a transfer table into a panel.
fn summarise(text: &str, fallback: &str) -> String {
    let interesting: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                // The progress rows: `Receiving objects: 42% (123/456)`.
                && !line.contains('%')
                && !line.starts_with("remote: Counting")
                && !line.starts_with("remote: Compressing")
        })
        .collect();
    if interesting.is_empty() {
        fallback.to_string()
    } else {
        interesting.join("\n")
    }
}

/* ---------------------------------------------------------------------------
   History and diffs
--------------------------------------------------------------------------- */

/// Recent commits, newest first.
pub fn log(workdir: &Path, count: usize) -> Result<Vec<Commit>> {
    let root = root(workdir)?;
    let limit = format!("-n{}", count.clamp(1, 500));
    // `%x1f` is a unit separator: a subject cannot contain one, so splitting on
    // it cannot break on a commit message that happens to hold a pipe or a tab.
    let text = run(
        &root,
        &[
            "--no-pager",
            "log",
            &limit,
            "--pretty=%h%x1f%s%x1f%an%x1f%at",
        ],
        LOCAL_TIMEOUT,
    )?;

    let mut commits = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\u{1f}').collect();
        if parts.len() < 4 {
            continue;
        }
        commits.push(Commit {
            id: parts[0].to_string(),
            subject: parts[1].to_string(),
            author: parts[2].to_string(),
            at: parts[3].parse().unwrap_or(0),
        });
    }
    Ok(commits)
}

/// The unified diff of the work tree, or of the index when `staged`.
///
/// This is what feeds the commit-message pass, and what the editor's diff view
/// does *not* need — that is built from two file versions, which is exact where
/// a patch is only a description of the difference.
///
/// `--no-color` because a patch with ANSI escapes in it is a patch no model
/// should be reading.
pub fn diff(workdir: &Path, staged: bool) -> Result<String> {
    let root = root(workdir)?;
    let mut args: Vec<&str> = vec![
        "--no-pager",
        "diff",
        "--patch",
        "--no-color",
        // A rename's content should show as a rename, not as a delete plus an
        // add, or a generated message describes a file being replaced.
        "--find-renames",
    ];
    if staged {
        args.push("--cached");
    }
    let text = run(&root, &args, LOCAL_TIMEOUT)?;
    // Untracked files are invisible to `git diff`. Include them so a first
    // commit of new work is describable rather than "no changes".
    if !staged {
        let untracked = run(
            &root,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
            ],
            LOCAL_TIMEOUT,
        )?;
        let mut combined = text;
        for path in untracked.lines().map(str::trim).filter(|p| !p.is_empty()) {
            combined.push_str(&format!(
                "\n--- /dev/null\n+++ b/{path}\n@@ new file @@\n"
            ));
            combined.push_str(&preview_untracked(&root, path));
        }
        return Ok(combined);
    }
    Ok(text)
}

/// A few lines of a new file, so the message pass can tell one from another.
fn preview_untracked(root: &Path, path: &str) -> String {
    let full = root.join(path);
    let Ok(bytes) = std::fs::read(&full) else {
        return String::new();
    };
    // Binary files would only add noise to a prompt.
    if bytes.iter().take(4096).any(|byte| *byte == 0) {
        return "(binary file)".to_string();
    }
    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        .take(40)
        .map(|line| format!("+{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The committed content of a path, or `None` when it did not exist.
///
/// This is what the editor's diff view uses as its "before" side. Reading the
/// blob is exact; reconstructing it from a patch would be a parser, and a parser
/// is a thing that can be subtly wrong about a file with no trailing newline.
pub fn file_at_head(workdir: &Path, path: &str) -> Result<Option<String>> {
    file_at(workdir, "HEAD", path)
}

/// The staged content of a path, or `None` when it is not in the index.
pub fn file_at_index(workdir: &Path, path: &str) -> Result<Option<String>> {
    file_at(workdir, ":0", path)
}

fn file_at(workdir: &Path, rev: &str, path: &str) -> Result<Option<String>> {
    let root = root(workdir)?;
    let spec = format!("{rev}:{path}");
    let output = run_raw(&root, &["show", &spec], LOCAL_TIMEOUT)?;
    if !output.status.success() {
        // A path that is not in that revision is the ordinary "new file" case,
        // not an error worth surfacing.
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs git in a temp dir, asserting it worked.
    fn git(root: &Path, args: &[&str]) -> bool {
        process::hidden_std("git")
            .args(args)
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        assert!(git(dir.path(), &["init", "-q", "-b", "main"]), "git init");
        assert!(
            git(dir.path(), &["config", "user.email", "t@example.com"]),
            "email"
        );
        assert!(git(dir.path(), &["config", "user.name", "Test"]), "name");
        assert!(
            git(dir.path(), &["config", "commit.gpgsign", "false"]),
            "no signing"
        );
        dir
    }

    fn write(root: &Path, name: &str, body: &str) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn a_folder_outside_a_repository_reports_none() {
        let dir = tempfile::tempdir().unwrap();
        let status = status(dir.path()).unwrap();
        assert!(!status.is_repo);
        assert!(status.files.is_empty());
        assert_eq!(status.root, None);
    }

    #[test]
    fn untracked_files_are_listed() {
        let dir = repo();
        write(dir.path(), "new.txt", "hello");

        let status = status(dir.path()).unwrap();
        assert!(status.is_repo);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.files.len(), 1);
        assert!(status.files[0].untracked);
        assert_eq!(status.files[0].path, "new.txt");
    }

    #[test]
    fn a_file_can_be_staged_and_committed() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");

        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        let staged = status(dir.path()).unwrap();
        assert!(staged.files[0].staged, "{staged:?}");
        assert!(!staged.files[0].unstaged);

        let result = commit(dir.path(), "feat: add a\n\nBecause it was needed.").unwrap();
        assert_eq!(result.subject, "feat: add a");
        assert_eq!(result.files, 1);
        assert!(!result.id.is_empty());

        // And the tree is clean afterwards.
        let after = status(dir.path()).unwrap();
        assert!(after.files.is_empty(), "{after:?}");
    }

    #[test]
    fn a_staged_file_edited_again_is_both_staged_and_unstaged() {
        // The case that a single-kind enum would lose.
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        write(dir.path(), "a.txt", "two");

        let status = status(dir.path()).unwrap();
        assert_eq!(status.files.len(), 1);
        assert!(status.files[0].staged);
        assert!(status.files[0].unstaged);
        assert_eq!(status.staged().len(), 1);
        assert_eq!(status.unstaged().len(), 1);
    }

    #[test]
    fn unstaging_leaves_the_work_tree_alone() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        unstage(dir.path(), &["a.txt".to_string()]).unwrap();

        let status = status(dir.path()).unwrap();
        assert!(!status.files[0].staged);
        assert!(status.files[0].untracked);
        // The bytes are still there — that is the whole difference from discard.
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one"
        );
    }

    #[test]
    fn unstaging_everything_works_in_a_repo_with_no_commits() {
        // `git restore --staged` needs a HEAD to restore from; this is the
        // brand-new-repository path, which is where a naive `reset` fails.
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        unstage(dir.path(), &[]).unwrap();

        let status = status(dir.path()).unwrap();
        assert!(status.files.iter().all(|file| !file.staged), "{status:?}");
    }

    #[test]
    fn discarding_a_tracked_edit_restores_the_committed_bytes() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();
        write(dir.path(), "a.txt", "changed");

        discard(dir.path(), &["a.txt".to_string()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one"
        );
    }

    #[test]
    fn discarding_an_untracked_file_removes_it() {
        let dir = repo();
        write(dir.path(), "scratch.txt", "temporary");
        discard(dir.path(), &["scratch.txt".to_string()]).unwrap();
        assert!(!dir.path().join("scratch.txt").exists());
    }

    #[test]
    fn committing_with_nothing_staged_is_refused() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        // Untracked, but not staged.
        let error = commit(dir.path(), "feat: nothing").unwrap_err();
        assert!(error.to_string().contains("nothing is staged"), "{error}");
    }

    #[test]
    fn an_empty_message_is_refused() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        assert!(commit(dir.path(), "   ").is_err());
    }

    #[test]
    fn a_message_containing_quotes_and_newlines_survives() {
        // The reason the message goes in over stdin rather than `-m`.
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();

        let message = "fix: handle \"quoted\" input\n\nIt broke on `a\"b` and on\nnewlines.";
        commit(dir.path(), message).unwrap();

        let subject = run(dir.path(), &["log", "-1", "--pretty=%s"], LOCAL_TIMEOUT).unwrap();
        assert_eq!(subject.trim(), "fix: handle \"quoted\" input");
        let body = run(dir.path(), &["log", "-1", "--pretty=%b"], LOCAL_TIMEOUT).unwrap();
        assert!(body.contains("newlines."), "{body}");
    }

    #[test]
    fn branches_are_listed_with_the_current_one_marked() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();
        create_branch(dir.path(), "feature", false).unwrap();

        let branches = branches(dir.path()).unwrap();
        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"main"), "{names:?}");
        assert!(names.contains(&"feature"), "{names:?}");
        // Created without switching, so `main` is still current.
        assert!(branches.iter().find(|b| b.name == "main").unwrap().current);
    }

    #[test]
    fn creating_a_branch_and_switching_moves_head() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();

        create_branch(dir.path(), "feature", true).unwrap();
        assert_eq!(branch(dir.path()).unwrap().as_deref(), Some("feature"));
    }

    #[test]
    fn a_deleted_file_can_be_staged_and_committed() {
        // `git add <path>` alone ignores a deletion, which would make it
        // impossible to commit one from the panel.
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();

        std::fs::remove_file(dir.path().join("a.txt")).unwrap();
        let gone = status(dir.path()).unwrap();
        assert!(gone.files.iter().any(|file| file.path == "a.txt"));

        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        let staged = status(dir.path()).unwrap();
        assert!(staged.files[0].staged, "{staged:?}");
        commit(dir.path(), "chore: remove a").unwrap();
    }

    #[test]
    fn log_reports_commits_newest_first() {
        let dir = repo();
        for (name, message) in [("a.txt", "first"), ("b.txt", "second")] {
            write(dir.path(), name, "x");
            stage(dir.path(), &[name.to_string()]).unwrap();
            commit(dir.path(), message).unwrap();
        }
        let commits = log(dir.path(), 10).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "second");
        assert_eq!(commits[1].subject, "first");
        assert!(commits[0].at > 0);
    }

    #[test]
    fn file_at_head_is_none_for_a_path_that_does_not_exist_there() {
        let dir = repo();
        write(dir.path(), "seed.txt", "x");
        stage(dir.path(), &["seed.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();

        assert_eq!(file_at_head(dir.path(), "seed.txt").unwrap().as_deref(), Some("x"));
        assert_eq!(file_at_head(dir.path(), "missing.txt").unwrap(), None);
    }

    #[test]
    fn file_at_index_reflects_the_staged_version_not_the_work_tree() {
        // What a staged diff has to compare against.
        let dir = repo();
        write(dir.path(), "a.txt", "committed");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();

        write(dir.path(), "a.txt", "staged");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        write(dir.path(), "a.txt", "work tree");

        assert_eq!(
            file_at_index(dir.path(), "a.txt").unwrap().as_deref(),
            Some("staged")
        );
        assert_eq!(
            file_at_head(dir.path(), "a.txt").unwrap().as_deref(),
            Some("committed")
        );
    }

    #[test]
    fn a_repository_with_no_remote_says_so() {
        let dir = repo();
        assert!(!has_remote(dir.path()).unwrap());
    }

    #[test]
    fn a_conflicted_file_is_reported_as_conflicted() {
        let dir = repo();
        write(dir.path(), "a.txt", "base\n");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();

        git(dir.path(), &["checkout", "-q", "-b", "other"]);
        write(dir.path(), "a.txt", "other\n");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: other").unwrap();

        git(dir.path(), &["checkout", "-q", "main"]);
        write(dir.path(), "a.txt", "main\n");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: main").unwrap();

        // A merge that must conflict.
        git(dir.path(), &["merge", "other"]);
        let status = status(dir.path()).unwrap();
        assert!(
            status.files.iter().any(|file| file.conflicted),
            "expected a conflict, got {status:?}"
        );
        // And committing over one is refused rather than half-done.
        assert!(commit(dir.path(), "chore: broken").is_err());
        assert_eq!(status.operation.as_deref(), Some("merge"));
    }

    #[test]
    fn a_detached_head_reports_no_branch() {
        let dir = repo();
        write(dir.path(), "a.txt", "one");
        stage(dir.path(), &["a.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();
        let id = run(dir.path(), &["rev-parse", "HEAD"], LOCAL_TIMEOUT).unwrap();
        git(dir.path(), &["checkout", "-q", id.trim()]);

        let status = status(dir.path()).unwrap();
        assert!(status.detached, "{status:?}");
        assert_eq!(status.branch, None);
        // A push has nothing to push, and says so rather than failing obscurely.
        assert!(push(dir.path()).is_err());
    }

    #[test]
    fn diff_includes_an_untracked_file() {
        let dir = repo();
        write(dir.path(), "fresh.txt", "brand new\n");
        let patch = diff(dir.path(), false).unwrap();
        assert!(patch.contains("fresh.txt"), "{patch}");
        assert!(patch.contains("+brand new"), "{patch}");
    }

    #[test]
    fn a_binary_untracked_file_is_not_dumped_into_the_diff() {
        let dir = repo();
        std::fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 3, 0, 9]).unwrap();
        let patch = diff(dir.path(), false).unwrap();
        assert!(patch.contains("(binary file)"), "{patch}");
    }

    #[test]
    fn the_wire_format_parses_a_path_with_a_newline_in_it() {
        // Tested against a synthetic payload rather than a real file, for two
        // reasons. The practical one: Windows cannot create a filename
        // containing a newline, so the integration test is not writable here at
        // all. The better one: `parse_status` is a pure function and the *wire
        // format* is the thing that can be quietly wrong, so this asserts the
        // format directly instead of hoping a repository produces it.
        //
        // A v1 porcelain parser splits this record into two entries, one of
        // which does not exist — which is exactly why `-z` is used.
        let raw = b"# branch.head main\0\
                    1 .M N... 100644 100644 100644 aaa bbb odd\nname.txt\0";

        let status = parse_status(raw);
        assert_eq!(status.files.len(), 1, "{status:?}");
        assert_eq!(status.files[0].path, "odd\nname.txt");
        // `X` is `.` and `Y` is `M`, so this is a work-tree change only.
        assert!(!status.files[0].staged);
        assert!(status.files[0].unstaged);
    }

    #[test]
    fn the_wire_format_parses_every_record_kind() {
        // One payload covering all four record types, so a regression in any of
        // the field offsets shows up in one place.
        let raw = b"# branch.oid abc1234\0\
                    # branch.head main\0\
                    # branch.upstream origin/main\0\
                    # branch.ab +2 -1\0\
                    1 M. N... 100644 100644 100644 aaa bbb src/staged.ts\0\
                    1 .M N... 100644 100644 100644 aaa bbb src/modified.ts\0\
                    2 R. N... 100644 100644 100644 aaa bbb R100 dst/moved.ts\0src/old name.ts\0\
                    u UU N... 100644 100644 100644 100644 a1 b2 c3 clash.txt\0\
                    ? brand new.txt\0";

        let status = parse_status(raw);
        assert!(status.is_repo);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert!(!status.detached);
        assert_eq!(status.files.len(), 5, "{status:?}");

        // Sorted by path, which is what makes the panel's two lists stable.
        let by_path = |needle: &str| {
            status
                .files
                .iter()
                .find(|file| file.path == needle)
                .unwrap_or_else(|| panic!("no entry for {needle} in {status:?}"))
        };

        let staged = by_path("src/staged.ts");
        assert!(staged.staged && !staged.unstaged);

        let modified = by_path("src/modified.ts");
        assert!(!modified.staged && modified.unstaged);

        // The rename's original path is a *separate* field in `-z`, and it keeps
        // the space in its name.
        let moved = by_path("dst/moved.ts");
        assert_eq!(moved.from.as_deref(), Some("src/old name.ts"));
        assert!(moved.staged);

        let clash = by_path("clash.txt");
        assert!(clash.conflicted);

        let fresh = by_path("brand new.txt");
        assert!(fresh.untracked);
        // An untracked path's leading space must not survive into the name.
        assert!(!fresh.path.starts_with(' '));
    }

    #[test]
    fn the_wire_format_reads_a_detached_head_and_ignores_ignored_files() {
        let raw = b"# branch.head (detached)\0\
                    ! build/artifact.bin\0\
                    ? kept.txt\0";

        let status = parse_status(raw);
        assert!(status.detached);
        assert_eq!(status.branch, None);
        // An ignored file is not listed: a commit box full of build artefacts is
        // one nobody reads.
        let paths: Vec<&str> = status.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["kept.txt"]);
    }

    #[test]
    fn the_wire_format_survives_a_truncated_payload() {
        // A record cut short — which is what a killed process produces — must not
        // panic, and must not invent an entry with a nonsense path.
        for raw in [
            &b"1 M. N... 100644\0"[..],
            &b"2 R. N... 100644 100644 100644 aaa bbb R100 dst.ts\0"[..],
            &b"u UU N... 100644\0"[..],
            &b"? \0"[..],
        ] {
            let status = parse_status(raw);
            assert!(
                status.files.iter().all(|file| !file.path.contains("100644")),
                "invented an entry from {raw:?}: {status:?}"
            );
        }
    }

    #[test]
    fn a_path_with_non_ascii_characters_round_trips() {
        let dir = repo();
        write(dir.path(), "café-λ.txt", "x");
        let status = status(dir.path()).unwrap();
        assert_eq!(status.files[0].path, "café-λ.txt");
    }

    #[test]
    fn a_rename_reports_both_paths() {
        let dir = repo();
        write(dir.path(), "before.txt", "content\n");
        stage(dir.path(), &["before.txt".to_string()]).unwrap();
        commit(dir.path(), "chore: seed").unwrap();

        git(dir.path(), &["mv", "before.txt", "after.txt"]);
        let status = status(dir.path()).unwrap();
        let file = status
            .files
            .iter()
            .find(|file| file.path == "after.txt")
            .unwrap_or_else(|| panic!("no rename entry in {status:?}"));
        assert_eq!(file.from.as_deref(), Some("before.txt"));
        assert!(file.staged);
    }

    #[test]
    fn ignored_files_are_left_out() {
        // A commit box listing every build artefact is a commit box nobody reads.
        let dir = repo();
        write(dir.path(), ".gitignore", "build/\n");
        write(dir.path(), "build/artifact.bin", "x");
        write(dir.path(), "kept.txt", "x");

        let status = status(dir.path()).unwrap();
        let paths: Vec<&str> = status.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"kept.txt"), "{paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with("build/")), "{paths:?}");
    }

    #[test]
    fn nested_folders_are_listed_file_by_file() {
        // `--untracked-files=all`, so a new folder can be staged per file.
        let dir = repo();
        write(dir.path(), "src/deep/thing.ts", "export {}");
        let status = status(dir.path()).unwrap();
        assert!(
            status.files.iter().any(|f| f.path == "src/deep/thing.ts"),
            "{status:?}"
        );
    }
}
