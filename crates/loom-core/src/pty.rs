//! A real terminal: a pseudo-console, not a pipe.
//!
//! This is deliberately not built on [`crate::process`]. That module runs a
//! command and collects output, which is what the agent's `run_command` wants.
//! A terminal is the opposite: it *is* the shell, it stays open, it answers
//! keystrokes as they are typed, and the program on the other end expects a
//! tty -- without one, `git` loses its pager, `npm` its progress bar, and vim
//! and htop cannot start at all. So this uses a real pty (`ConPTY` on Windows,
//! `openpty` elsewhere) and speaks bytes both ways.
//!
//! Three things here are load-bearing and worth knowing before changing them.
//!
//! **The reader is a blocking read on its own thread.** A pty master has no
//! async story, and wrapping it in a runtime would only add a thread pool to
//! something that needs exactly one thread. Each session gets its own.
//!
//! **Output is coalesced before it leaves this module.** A shell repainting a
//! progress bar writes thousands of tiny chunks a second, and forwarding each
//! one as its own event would put thousands of messages a second through the
//! webview. One pump thread per process drains whatever is queued and emits it
//! as a single chunk, so the boundary between the pty and the UI is a frame
//! boundary rather than a syscall boundary. The pump blocking on `recv` and
//! then draining with `try_recv` is what does it: no polling, and it batches
//! exactly when there is a backlog.
//!
//! **The terminal is the user's, and nothing else may use it.** These sessions
//! are not exposed as agent tools, they are not registered with the command
//! tracker, and no engine code path may write to them. A shell the user is
//! typing into must not be reachable by the model: the permission card asks
//! before a tool touches the filesystem, and there is no card in front of
//! keystrokes typed into a live shell. Keeping the two surfaces separate is
//! what makes that true by construction rather than by review.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::{process, Error, Result};

/// Default pty geometry, used until the UI reports the real size.
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 80;

/// What happened in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyEvent {
    /// Bytes for the terminal to render. Not text: a pty stream contains
    /// partial escape sequences and partial UTF-8 codepoints, and splitting
    /// either would corrupt the display. The UI hands these to xterm, which
    /// owns the parser.
    Data(Vec<u8>),
    /// The shell ended, so the tab can say so instead of looking hung.
    Exit,
}

/// One batch of output, tagged with the session it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyChunk {
    pub id: String,
    pub event: PtyEvent,
}

/// Where a session's output goes.
///
/// A closure rather than a Tauri `Emitter` so this module stays free of the
/// shell, and so a test can collect output into a `Vec` and assert on it.
pub type Sink = Arc<dyn Fn(PtyChunk) + Send + Sync>;

/// A shell Loom can offer to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellProfile {
    /// Stable id, stored per workspace so a folder reopens in the same shell.
    pub id: String,
    /// What the tab is labelled.
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
}

impl ShellProfile {
    fn new(id: &str, name: &str, program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            program: program.into(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
        }
    }

    /// A profile from a bare path, named after the executable.
    fn from_path(id: &str, name: &str, path: &Path, args: &[&str]) -> Self {
        Self::new(id, name, path.to_string_lossy().into_owned(), args)
    }
}

/// Live session state.
struct Session {
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    profile: Option<String>,
    workdir: String,
    /// Cleared by the reader thread when the shell ends, so `list` can report
    /// a dead tab without waiting on a `try_wait` that may never reap.
    alive: Arc<AtomicBool>,
}

/// What the UI knows about a session. Deliberately no process id: the shell is
/// the user's, and a pid would invite code that reaches for it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyInfo {
    pub id: String,
    pub profile: Option<String>,
    pub workdir: String,
    pub alive: bool,
    pub rows: u16,
    pub cols: u16,
}

/// Every live shell in the process.
///
/// One map, so a torn-off terminal window and the main window address the same
/// shells by id. That is the whole reason sessions index by string rather than
/// living inside a component.
pub struct PtyManager {
    sessions: Mutex<HashMap<String, Session>>,
    sizes: Mutex<HashMap<String, (u16, u16)>>,
    next_id: AtomicU64,
    /// Held so the pump's channel never closes for the life of the process.
    _input: Sender<PtyChunk>,
    shutdown: AtomicBool,
}

impl PtyManager {
    /// Starts the output pump. Call once, at app start.
    pub fn new(sink: Sink) -> Arc<Self> {
        let (input, output) = mpsc::channel::<PtyChunk>();
        let manager = Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            _input: input.clone(),
            shutdown: AtomicBool::new(false),
        });

        // The pump. Owns the receiving half, so nothing else can consume a
        // chunk before it has been offered the chance to batch.
        std::thread::Builder::new()
            .name("loom-pty-pump".into())
            .spawn(move || pump(output, &sink))
            .expect("could not start the pty output pump");

        manager
    }

    /// Allocates an id. Monotonic, so a closed tab's id is never reused and a
    /// late event from a dying shell cannot land in a new one.
    pub fn next_id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Starts a shell. `id` is the caller's, so the UI can name a session after
    /// the workspace it belongs to.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        self: &Arc<Self>,
        id: &str,
        profile: &ShellProfile,
        workdir: Option<&Path>,
        rows: Option<u16>,
        cols: Option<u16>,
    ) -> Result<()> {
        if self
            .sessions
            .lock()
            .expect("pty map poisoned")
            .contains_key(id)
        {
            return Err(Error::other(format!("pty session already open: {id}")));
        }

        let rows = rows.unwrap_or(DEFAULT_ROWS).max(2);
        let cols = cols.unwrap_or(DEFAULT_COLS).max(2);

        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| Error::other(format!("could not open a pty: {e}")))?;

        let mut command = CommandBuilder::new(&profile.program);
        for arg in &profile.args {
            command.arg(arg);
        }
        // A folder that has since been deleted must not make the shell fail to
        // start; it just starts somewhere sensible instead.
        let cwd = workdir
            .filter(|dir| dir.is_dir())
            .map(|dir| dir.to_path_buf());
        if let Some(dir) = &cwd {
            command.cwd(dir);
        }
        // What every colour-capable program reads to decide how to paint
        // itself. Without these, `git diff` and `ls` come out monochrome.
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| Error::other(format!("could not start {}: {e}", profile.program)))?;
        // Dropped so the master sees EOF when the shell exits. Holding a slave
        // handle open would make `read` on the master hang forever after the
        // shell is gone, and the tab would never learn that it ended.
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| Error::other(format!("could not read the pty: {e}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| Error::other(format!("could not write to the pty: {e}")))?;

        let alive = Arc::new(AtomicBool::new(true));
        self.sessions.lock().expect("pty map poisoned").insert(
            id.to_string(),
            Session {
                child,
                writer,
                master: pair.master,
                profile: Some(profile.id.clone()),
                workdir: cwd
                    .as_ref()
                    .map(|dir| dir.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                alive: alive.clone(),
            },
        );
        self.sizes
            .lock()
            .expect("pty size map poisoned")
            .insert(id.to_string(), (rows, cols));

        // One thread per session, blocking on the read. See the module note.
        let id = id.to_string();
        let sender = self._input.clone();
        std::thread::Builder::new()
            .name(format!("loom-pty-{id}"))
            .spawn(move || read_loop(id, reader, sender, alive))
            .map_err(|e| Error::other(format!("could not start the pty reader: {e}")))?;

        Ok(())
    }

    /// Types into a shell. `data` is bytes, not text: a keystroke may be a
    /// control byte, and a bracketed paste may be several hundred of them.
    pub fn write(&self, id: &str, data: &[u8]) -> Result<()> {
        let mut sessions = self.sessions.lock().expect("pty map poisoned");
        let session = sessions
            .get_mut(id)
            .ok_or_else(|| Error::other(format!("no pty session {id}")))?;
        session
            .writer
            .write_all(data)
            .map_err(|e| Error::other(format!("could not write to {id}: {e}")))?;
        session
            .writer
            .flush()
            .map_err(|e| Error::other(format!("could not flush {id}: {e}")))?;
        Ok(())
    }

    /// Tells the shell how big the terminal is, which is what makes a
    /// full-screen program lay itself out correctly and reflow on a resize.
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        let rows = rows.max(2);
        let cols = cols.max(2);
        {
            let mut sessions = self.sessions.lock().expect("pty map poisoned");
            let session = sessions
                .get_mut(id)
                .ok_or_else(|| Error::other(format!("no pty session {id}")))?;
            session
                .master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| Error::other(format!("could not resize {id}: {e}")))?;
        }
        self.sizes
            .lock()
            .expect("pty size map poisoned")
            .insert(id.to_string(), (rows, cols));
        Ok(())
    }

    /// Ends a session, and the whole process tree under it.
    ///
    /// `kill` on the pty child only reaches the immediate process. A shell that
    /// has started a build has children of its own, and leaving them running
    /// after the tab is closed would leak exactly the processes a user closes a
    /// terminal to be rid of -- which is why this reuses [`process::kill_tree`].
    pub fn close(&self, id: &str) -> Result<()> {
        let session = {
            let mut sessions = self.sessions.lock().expect("pty map poisoned");
            sessions.remove(id)
        };
        self.sizes.lock().expect("pty size map poisoned").remove(id);

        let Some(mut session) = session else {
            return Ok(());
        };
        session.alive.store(false, Ordering::Relaxed);
        if let Some(pid) = session.child.process_id() {
            process::kill_tree(pid);
        }
        let _ = session.child.kill();
        // Reap, so the process does not linger as a zombie on Unix.
        let _ = session.child.wait();
        Ok(())
    }

    /// Ends every session. Called when the app is quitting: an orphaned shell
    /// holding a port or a lock file is a bad surprise on the next launch.
    pub fn close_all(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let ids: Vec<String> = self
            .sessions
            .lock()
            .expect("pty map poisoned")
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.close(&id);
        }
    }

    pub fn list(&self) -> Vec<PtyInfo> {
        let sessions = self.sessions.lock().expect("pty map poisoned");
        let sizes = self.sizes.lock().expect("pty size map poisoned");
        let mut out: Vec<PtyInfo> = sessions
            .iter()
            .map(|(id, session)| {
                let (rows, cols) = sizes
                    .get(id)
                    .copied()
                    .unwrap_or((DEFAULT_ROWS, DEFAULT_COLS));
                PtyInfo {
                    id: id.clone(),
                    profile: session.profile.clone(),
                    workdir: session.workdir.clone(),
                    alive: session.alive.load(Ordering::Relaxed),
                    rows,
                    cols,
                }
            })
            .collect();
        // Stable order, so tabs do not shuffle when one shell exits.
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    pub fn is_alive(&self, id: &str) -> bool {
        self.sessions
            .lock()
            .expect("pty map poisoned")
            .get(id)
            .map(|session| session.alive.load(Ordering::Relaxed))
            .unwrap_or(false)
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let sessions = std::mem::take(&mut *self.sessions.lock().expect("pty map poisoned"));
        for (_, mut session) in sessions {
            if let Some(pid) = session.child.process_id() {
                process::kill_tree(pid);
            }
            let _ = session.child.kill();
        }
    }
}

/// Reads a pty until it ends. One thread per session; blocking is the point.
fn read_loop(
    id: String,
    mut reader: Box<dyn Read + Send>,
    sender: Sender<PtyChunk>,
    alive: Arc<AtomicBool>,
) {
    // 16 KiB is a comfortable read: big enough that a `yes` flood is batched
    // by the read itself, small enough to stay off the large-object heap.
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let chunk = PtyChunk {
                    id: id.clone(),
                    event: PtyEvent::Data(buffer[..count].to_vec()),
                };
                // The pump is gone only if the app is tearing down.
                if sender.send(chunk).is_err() {
                    break;
                }
            }
        }
    }
    alive.store(false, Ordering::Relaxed);
    let _ = sender.send(PtyChunk {
        id,
        event: PtyEvent::Exit,
    });
}

/// Drains the queue and emits one chunk per burst.
///
/// Blocks on the first chunk so an idle terminal costs nothing, then takes
/// everything already queued. Consecutive chunks from the *same* session are
/// concatenated, because a repaint is many small writes and one message is
/// what the webview wants. A chunk from another session is flushed first, so
/// two shells never have their output spliced together.
fn pump(queue: Receiver<PtyChunk>, sink: &Sink) {
    loop {
        // Blocks until there is something to do, so an idle terminal costs
        // nothing. Everything after this drains a backlog rather than waiting.
        let Ok(first) = queue.recv() else {
            return;
        };

        let mut id = first.id.clone();
        let mut bytes: Vec<u8> = Vec::new();
        let mut next = Some(first);

        loop {
            let chunk = match next.take() {
                Some(chunk) => chunk,
                None => match queue.try_recv() {
                    Ok(chunk) => chunk,
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                },
            };

            if chunk.id != id {
                // A different shell spoke. Flush what we have first, so two
                // sessions can never have their output spliced into one chunk.
                emit_data(sink, &id, &mut bytes);
                id = chunk.id.clone();
            }

            match chunk.event {
                PtyEvent::Data(more) => bytes.extend_from_slice(&more),
                // Never coalesced with data: the UI has to see the end, and
                // the bytes that came before it are the shell's last words.
                PtyEvent::Exit => {
                    emit_data(sink, &id, &mut bytes);
                    sink(PtyChunk {
                        id: id.clone(),
                        event: PtyEvent::Exit,
                    });
                }
            }
        }

        emit_data(sink, &id, &mut bytes);
    }
}

fn emit_data(sink: &Sink, id: &str, bytes: &mut Vec<u8>) {
    if bytes.is_empty() {
        return;
    }
    sink(PtyChunk {
        id: id.to_string(),
        event: PtyEvent::Data(std::mem::take(bytes)),
    });
}

/// The shells Loom can offer, best first.
///
/// Runs `wsl -l -q` (a process), so the UI calls this once and caches it. The
/// static half is split out because profile *selection* is the part with the
/// decisions in it, and it should be testable without spawning anything.
pub fn profiles() -> Vec<ShellProfile> {
    let mut found = profiles_static();
    for distro in wsl_distros() {
        found.push(ShellProfile::new(
            &format!("wsl:{distro}"),
            &format!("WSL · {distro}"),
            "wsl.exe",
            &["-d", &distro],
        ));
    }
    found
}

/// The shells that can be found without asking a subsystem for a list.
pub fn profiles_static() -> Vec<ShellProfile> {
    let mut found: Vec<ShellProfile> = Vec::new();

    if cfg!(windows) {
        // PowerShell 7 first: it is the modern one, and it is only present if
        // the user deliberately installed it.
        if let Some(path) = on_path("pwsh.exe").or_else(|| program_files("PowerShell\\7\\pwsh.exe"))
        {
            found.push(ShellProfile::from_path(
                "pwsh",
                "PowerShell 7",
                &path,
                &["-NoLogo"],
            ));
        }
        if let Some(path) = on_path("powershell.exe") {
            found.push(ShellProfile::from_path(
                "powershell",
                "Windows PowerShell",
                &path,
                &["-NoLogo"],
            ));
        }
        // Git Bash, wherever the installer put it. `-i` is what gives it a
        // prompt and job control; without it the shell exits immediately.
        let bash = [
            program_files("Git\\bin\\bash.exe"),
            program_files("Git\\usr\\bin\\bash.exe"),
            local_app_data("Programs\\Git\\bin\\bash.exe"),
        ]
        .into_iter()
        .flatten()
        .next();
        if let Some(path) = bash {
            found.push(ShellProfile::from_path(
                "git-bash",
                "Git Bash",
                &path,
                &["--login", "-i"],
            ));
        }
        // cmd.exe as the floor: it is always there, so the terminal can never
        // be unusable on a machine where nothing else was found.
        let cmd = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        found.push(ShellProfile::new("cmd", "Command Prompt", cmd, &[]));
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let name = shell.rsplit('/').next().unwrap_or("sh").to_string();
        let id = match name.as_str() {
            "zsh" => "zsh",
            "fish" => "fish",
            "sh" => "sh",
            _ => "bash",
        };
        // `-l` for a login shell, which is what a terminal emulator starts and
        // what a user's PATH and aliases are set up for.
        found.push(ShellProfile::new(id, &name, shell, &["-l"]));
    }

    found
}

/// The profile to start when the workspace has not chosen one.
///
/// The first entry, which the ordering above already puts in the right place:
/// a modern PowerShell where it exists, otherwise the shell the platform
/// guarantees. Returns `None` only on a machine with no shell at all, which the
/// caller should treat as "the terminal cannot open" rather than a crash.
pub fn default_profile(profiles: &[ShellProfile]) -> Option<&ShellProfile> {
    // An explicit `SHELL` on Unix, or pwsh on Windows, is already first; but on
    // Unix a user with `SHELL=/bin/sh` would get `sh` over nothing, so the list
    // order is the decision and this is only a guard against an empty list.
    profiles.first()
}

/// Every WSL distribution, best effort.
///
/// Every failure mode returns an empty list. WSL is optional, often absent, and
/// asking for it must never be the reason the terminal does not open.
fn wsl_distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let output = match process::hidden_std("wsl.exe").args(["-l", "-q"]).output() {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };
    decode_wsl_listing(&output)
}

/// Parses `wsl.exe -l -q`.
///
/// `wsl.exe` writes **UTF-16LE**, which is why this is a function with a test
/// rather than three lines inline: read as UTF-8 it is a stream of interleaved
/// NULs, so the naive version produces mojibake that then becomes tab names.
/// `-q` suppresses the `Windows Subsystem for Linux Distributions:` header and
/// the `*` default marker, but both are stripped here anyway so a user on an
/// older WSL that ignores `-q` still gets clean names.
pub fn decode_wsl_listing(bytes: &[u8]) -> Vec<String> {
    let text = if looks_utf16(bytes) {
        // Step two bytes at a time, native-endian: Windows is little-endian on
        // every platform WSL exists for.
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };

    text.lines()
        .map(|line| line.trim().trim_start_matches('*').trim())
        .filter(|line| !line.is_empty())
        // The header `-q` should have removed, and the blank line that follows
        // it. Matched loosely so a localised Windows still gets filtered.
        .filter(|line| !line.to_lowercase().contains("windows subsystem for linux"))
        .map(|line| line.to_string())
        .collect()
}

/// Whether a byte stream is UTF-16LE rather than UTF-8.
///
/// ASCII in UTF-16LE has a NUL in every odd position, and `wsl.exe` output is
/// all ASCII, so this is unambiguous in practice. A UTF-8 stream containing a
/// NUL at every other byte is not something a distro list can be.
fn looks_utf16(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let odd_nul = bytes[1..]
        .iter()
        .step_by(2)
        .filter(|byte| **byte == 0)
        .count();
    let odd_total = bytes[1..].iter().step_by(2).count();
    odd_nul * 2 >= odd_total
}

/// The first `name` on `PATH`.
fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// An absolute path under `%ProgramFiles%`, if it exists.
fn program_files(relative: &str) -> Option<PathBuf> {
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(root) = std::env::var(key) {
            let candidate = Path::new(&root).join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// An absolute path under `%LOCALAPPDATA%`, if it exists.
fn local_app_data(relative: &str) -> Option<PathBuf> {
    let root = std::env::var("LOCALAPPDATA").ok()?;
    let candidate = Path::new(&root).join(relative);
    candidate.is_file().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Encodes text the way `wsl.exe` does, so the decoder can be tested on
    /// every platform rather than only where WSL is installed.
    fn utf16le(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    }

    #[test]
    fn decodes_a_utf16_distro_list() {
        let raw = utf16le("Ubuntu\r\nDebian\r\n");
        assert_eq!(decode_wsl_listing(&raw), vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn decodes_utf8_too() {
        // Some builds respect `--locale` and emit the machine's codepage; the
        // decoder must not turn that into garbage either.
        assert_eq!(
            decode_wsl_listing(b"Ubuntu\nDebian\n"),
            vec!["Ubuntu", "Debian"]
        );
    }

    #[test]
    fn strips_the_header_and_the_default_marker() {
        // `wsl -l` without `-q`, on a Windows whose text is localised.
        let raw = utf16le(
            "Windows Subsystem for Linux Distributions:\r\nUbuntu (Default)\r\n*Debian\r\n",
        );
        assert_eq!(decode_wsl_listing(&raw), vec!["Ubuntu (Default)", "Debian"]);
    }

    #[test]
    fn ignores_blank_lines_and_whitespace() {
        let raw = utf16le("\r\n  Ubuntu  \r\n\r\n");
        assert_eq!(decode_wsl_listing(&raw), vec!["Ubuntu"]);
    }

    #[test]
    fn an_empty_listing_is_not_an_error() {
        assert!(decode_wsl_listing(b"").is_empty());
        assert!(decode_wsl_listing(&utf16le("\r\n")).is_empty());
    }

    #[test]
    fn short_input_is_not_treated_as_utf16() {
        // Under four bytes there is not enough evidence to guess, and guessing
        // wrong would mangle a two-character name.
        assert!(!looks_utf16(b"ab"));
        assert!(!looks_utf16(b""));
        assert!(looks_utf16(&utf16le("Ubuntu")));
        assert!(!looks_utf16(b"Ubuntu"));
    }

    #[test]
    fn profiles_are_found_without_asking_wsl() {
        let found = profiles_static();
        assert!(
            !found.is_empty(),
            "there must always be a shell to fall back to"
        );
        // Ids are what the per-workspace setting stores, so they have to be
        // stable and free of the spaces a display name has.
        for profile in &found {
            assert!(!profile.id.is_empty(), "{profile:?}");
            assert!(profile.id == profile.id.to_lowercase(), "{profile:?}");
            assert!(!profile.name.is_empty(), "{profile:?}");
            assert!(!profile.program.is_empty(), "{profile:?}");
        }
    }

    #[test]
    fn a_platform_shell_is_always_offered() {
        let ids: Vec<String> = profiles_static().into_iter().map(|p| p.id).collect();
        if cfg!(windows) {
            // cmd.exe is the floor: the terminal must work on a machine with
            // nothing else installed.
            assert!(ids.contains(&"cmd".to_string()), "{ids:?}");
        } else {
            assert!(
                ids.iter()
                    .any(|id| ["bash", "zsh", "fish", "sh"].contains(&id.as_str())),
                "{ids:?}"
            );
        }
    }

    #[test]
    fn the_default_is_the_first_offer() {
        let found = profiles_static();
        let default = default_profile(&found).unwrap();
        assert_eq!(default.id, found[0].id);
    }

    #[test]
    fn the_default_survives_an_empty_list() {
        // A machine with no shell must not panic; the caller reports that the
        // terminal cannot open.
        assert!(default_profile(&[]).is_none());
    }

    #[test]
    fn ids_are_unique_so_a_stored_choice_is_unambiguous() {
        let found = profiles_static();
        let mut ids: Vec<&str> = found.iter().map(|p| p.id.as_str()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate profile id in {found:?}");
    }

    #[test]
    fn opens_writes_and_closes_a_real_shell() {
        // The only test that proves the pty works end to end. It runs a real
        // shell, so it asserts on bytes arriving rather than on their content:
        // a prompt differs by platform, by locale, and by profile.
        let received: Arc<Mutex<Vec<PtyChunk>>> = Arc::new(Mutex::new(Vec::new()));
        let sink: Sink = {
            let received = received.clone();
            Arc::new(move |chunk| received.lock().expect("sink poisoned").push(chunk))
        };
        let manager = PtyManager::new(sink);

        let found = profiles_static();
        let Some(profile) = default_profile(&found) else {
            return; // No shell on this machine; nothing to prove.
        };

        let dir = tempfile::tempdir().unwrap();
        manager
            .open("t-1", profile, Some(dir.path()), Some(24), Some(80))
            .expect("a pty should open");

        let info = manager.list();
        assert_eq!(info.len(), 1);
        assert!(info[0].alive);

        // Give the shell a moment to print a banner or a prompt.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let got_output =
            |received: &Arc<Mutex<Vec<PtyChunk>>>| {
                received.lock().expect("sink poisoned").iter().any(
                    |chunk| matches!(chunk.event, PtyEvent::Data(ref bytes) if !bytes.is_empty()),
                )
            };
        while !got_output(&received) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(got_output(&received), "the shell produced no output");

        // Typing must reach it. `exit` is the one command every shell has.
        manager.write("t-1", b"exit\r").expect("write should land");

        // And closing must end it, whether the `exit` already did or not.
        manager.close("t-1").expect("close should succeed");
        assert!(manager.list().is_empty());
    }

    #[test]
    fn resize_is_recorded_even_though_nothing_asserts_the_shell_noticed() {
        // The pty call itself is platform code; what is worth pinning is that
        // the reported geometry follows the resize, since the tab's own layout
        // depends on it.
        let sink: Sink = Arc::new(|_| {});
        let manager = PtyManager::new(sink);
        let found = profiles_static();
        let Some(profile) = default_profile(&found) else {
            return;
        };
        manager
            .open("t-2", profile, None, Some(24), Some(80))
            .unwrap();
        manager.resize("t-2", 40, 120).unwrap();
        let info = manager.list();
        assert_eq!(info[0].rows, 40);
        assert_eq!(info[0].cols, 120);
        manager.close("t-2").unwrap();
    }

    #[test]
    fn a_missing_session_is_an_error_not_a_panic() {
        // A stale tab can ask about a shell that has gone; the UI shows the
        // message rather than the app dying.
        let sink: Sink = Arc::new(|_| {});
        let manager = PtyManager::new(sink);
        assert!(manager.write("gone", b"x").is_err());
        assert!(manager.resize("gone", 10, 10).is_err());
        assert!(!manager.is_alive("gone"));
        // Closing something already gone is a no-op, because a close is often
        // raced by the shell exiting on its own.
        assert!(manager.close("gone").is_ok());
    }

    #[test]
    fn ids_do_not_repeat() {
        let sink: Sink = Arc::new(|_| {});
        let manager = PtyManager::new(sink);
        let first = manager.next_id("ws");
        let second = manager.next_id("ws");
        assert_ne!(first, second);
        assert!(first.starts_with("ws-"));
    }
}
