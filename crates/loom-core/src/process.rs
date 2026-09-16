//! Spawning processes without flashing console windows, and keeping long-running
//! commands alive across a turn.
//!
//! Two rules live here so they cannot drift apart again:
//!
//! * **Every child Loom starts is windowless.** On Windows each spawn is
//!   flagged `CREATE_NO_WINDOW`, so `npm test`, a `git log`, an MCP server, or
//!   the updater's swap script never throw a black rectangle over the user's
//!   desktop. `setup/` already did this; the app crate had four sites that did
//!   not.
//! * **A command can outlive the reply that started it.** [`Running`] owns a
//!   background process: its stdout/stderr are appended to a log file (in
//!   arrival order) and its last 64 KiB per stream are kept in memory, so a
//!   timed-out foreground command can be handed over to the tracker instead of
//!   being killed with its output thrown away.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{paths, Error, Result};

/// Windows process-creation flag: start the child with no console window.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How long a foreground command may run before the turn stops waiting.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// Bytes of each stream kept in memory (the log file keeps much more).
const TAIL_BYTES: usize = 64 * 1024;

/// A command's log stops growing here, so a runaway process cannot fill a disk.
pub const LOG_LIMIT_BYTES: u64 = 5 * 1024 * 1024;

/// The platform shell, and the flag that means "run this whole string".
pub fn shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    }
}

/// A `std::process::Command` that will not open a console window.
pub fn hidden_std(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// A `tokio::process::Command` that will not open a console window.
pub fn hidden_tokio(program: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

/// Hides a command built by hand (a program with arguments the caller sets).
pub fn hide(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(not(windows))]
    let _ = command;
}

/// `~/.loom/logs/cmd-<id>.log`, created if the folder is missing.
pub fn command_log_path(id: &str) -> Result<PathBuf> {
    let dir = paths::logs_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    Ok(dir.join(format!("cmd-{id}.log")))
}

// ------------------------------------------------------------------
// tracking a long-running process
// ------------------------------------------------------------------

/// Which stream a chunk came from.
#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

/// The log file plus the in-memory tails, written by both reader tasks.
struct LogSink {
    file: std::fs::File,
    out: Vec<u8>,
    err: Vec<u8>,
    written: u64,
    /// Set once the cap is hit, so the tail can say the log was cut short.
    truncated: bool,
}

impl LogSink {
    fn write(&mut self, stream: Stream, bytes: &[u8]) {
        if self.written < LOG_LIMIT_BYTES {
            match self.file.write_all(bytes) {
                Ok(()) => {
                    let _ = self.file.flush();
                    self.written += bytes.len() as u64;
                }
                Err(_) => {
                    // A closed or full log must never break the command.
                    self.written = LOG_LIMIT_BYTES;
                    self.truncated = true;
                }
            }
        } else {
            self.truncated = true;
        }

        let tail = match stream {
            Stream::Out => &mut self.out,
            Stream::Err => &mut self.err,
        };
        tail.extend_from_slice(bytes);
        if tail.len() > TAIL_BYTES {
            let excess = tail.len() - TAIL_BYTES;
            tail.drain(..excess);
        }
    }

    fn tails(&self) -> (String, String) {
        (
            String::from_utf8_lossy(&self.out).into_owned(),
            String::from_utf8_lossy(&self.err).into_owned(),
        )
    }
}

/// A process Loom started and still owns: a background command, or a
/// foreground one that outlived [`COMMAND_TIMEOUT`].
///
/// Dropping it does **not** kill the process — that is the point of a
/// background command. Use [`kill_tree`] to stop it.
pub struct Running {
    child: tokio::process::Child,
    pid: u32,
    log_path: PathBuf,
    sink: Arc<Mutex<LogSink>>,
    readers: Vec<tokio::task::JoinHandle<()>>,
}

/// What a wait saw happen.
///
/// Three states rather than "an exit, or not": a failed wait is not the same
/// as a live process. Reporting it as one made `run_command` tell the model
/// "still running" and adopt a command that had already been reaped.
pub enum Wait {
    /// The process ended, with this status.
    Exited(std::process::ExitStatus),
    /// The limit passed and the process is still going.
    Running,
    /// The wait itself failed, so whether it is alive is no longer known.
    Unknown,
}

impl Running {
    /// Spawns `command` through the platform shell, hidden, with both streams
    /// piped into `log_path`.
    ///
    /// Needs a Tokio runtime: the streams are pumped by spawned tasks, and
    /// `tokio::process` panics without a reactor. Checked here so the call
    /// fails with a sentence instead of aborting the process.
    pub fn spawn(command: &str, cwd: &Path, log_path: &Path) -> Result<Self> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(Error::other("running a command needs a Tokio runtime"));
        }
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_path)
            .map_err(|e| Error::io(log_path, e))?;

        let (shell, flag) = shell();
        let mut command_line = hidden_tokio(shell);
        command_line
            .arg(flag)
            .arg(command)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // On Unix the child gets its own process group, so `kill_tree` can
        // signal the whole tree instead of only the `sh -c` wrapper that
        // leaves everything it started reparented and still running. Windows
        // gets the equivalent from `taskkill /T`.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command_line.as_std_mut().process_group(0);
        }
        let mut child = command_line
            .spawn()
            .map_err(|e| Error::other(format!("failed to run command: {e}")))?;

        let pid = child.id().unwrap_or(0);
        let sink = Arc::new(Mutex::new(LogSink {
            file,
            out: Vec::new(),
            err: Vec::new(),
            written: 0,
            truncated: false,
        }));

        let mut readers = Vec::new();        if let Some(stdout) = child.stdout.take() {
            readers.push(tokio::spawn(pump(stdout, Arc::clone(&sink), Stream::Out)));
        }
        if let Some(stderr) = child.stderr.take() {
            readers.push(tokio::spawn(pump(stderr, Arc::clone(&sink), Stream::Err)));
        }

        Ok(Self {
            child,
            pid,
            log_path: log_path.to_path_buf(),
            sink,
            readers,
        })
    }

    /// The process id of the shell Loom spawned (its children share the tree).
    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// Whether the log hit [`LOG_LIMIT_BYTES`] and stopped recording.
    pub fn log_truncated(&self) -> bool {
        self.sink.lock().map(|sink| sink.truncated).unwrap_or(false)
    }

    /// Waits up to `limit`, and says which of the three things happened.
    pub async fn wait_timeout(&mut self, limit: Duration) -> Wait {
        match tokio::time::timeout(limit, self.child.wait()).await {
            Ok(Ok(status)) => Wait::Exited(status),
            Ok(Err(_)) => Wait::Unknown,
            Err(_) => Wait::Running,
        }
    }

    /// Waits however long it takes.
    pub async fn wait(&mut self) -> Option<std::process::ExitStatus> {
        self.child.wait().await.ok()
    }

    /// Everything captured so far: (stdout, stderr).
    pub fn tails(&self) -> (String, String) {
        self.sink
            .lock()
            .map(|sink| sink.tails())
            .unwrap_or_else(|poisoned| poisoned.into_inner().tails())
    }

    /// A handle to the captured output alone, without the child.
    ///
    /// Lets a caller read what a running command has produced while another
    /// task owns the process and is waiting on it — which is exactly what the
    /// engine's watcher does.
    pub fn tail_handle(&self) -> TailHandle {
        TailHandle {
            sink: Arc::clone(&self.sink),
        }
    }

    /// Waits for the reader tasks to drain — they end when the pipes close —
    /// then returns the captured streams. The wait is bounded, because a
    /// grandchild that inherited the pipe can hold it open past its parent.
    pub async fn finish(&mut self) -> (String, String) {
        for reader in self.readers.drain(..) {
            let _ = tokio::time::timeout(Duration::from_millis(500), reader).await;
        }
        self.tails()
    }
}

/// Read-only access to a command's captured output, independent of whoever is
/// waiting on the process.
#[derive(Clone)]
pub struct TailHandle {
    sink: Arc<Mutex<LogSink>>,
}

impl TailHandle {
    /// Everything captured so far: (stdout, stderr).
    pub fn tails(&self) -> (String, String) {
        self.sink
            .lock()
            .map(|sink| sink.tails())
            .unwrap_or_else(|poisoned| poisoned.into_inner().tails())
    }
}

/// Copies a child stream into the shared log until the pipe closes.
async fn pump<R>(mut reader: R, sink: Arc<Mutex<LogSink>>, stream: Stream)
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let mut sink = match sink.lock() {
                    Ok(sink) => sink,
                    Err(poisoned) => poisoned.into_inner(),
                };
                sink.write(stream, &buffer[..read]);
            }
        }
    }
}

// ------------------------------------------------------------------
// stopping and inspecting
// ------------------------------------------------------------------

/// Ends a process and everything it spawned, and says whether the signal
/// landed. `stop_command` reports that, so the panel never claims a stop that
/// did not happen.
///
/// Best effort either way: a process that deliberately reparents its workers
/// can still slip away, and the row then reads `orphaned` on the next launch.
pub fn kill_tree(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        return hidden_std("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }
    #[cfg(unix)]
    {
        // A negative pid means the *process group*, which is why `Running::spawn`
        // puts the child in one: killing the pid alone killed only the `sh -c`
        // wrapper and left `npm test`'s workers running with the log held open.
        let group = format!("-{pid}");
        let signalled = hidden_std("kill")
            .args(["-TERM", &group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        // A process that traps SIGTERM (a build tool with its own handler)
        // would otherwise survive the Stop button, so escalate once. The pause
        // costs nothing: this runs off the event loop.
        std::thread::sleep(Duration::from_millis(250));
        if is_alive(pid) {
            let _ = hidden_std("kill")
                .args(["-KILL", &group])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        signalled
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

/// Whether a pid is still running. Used to stop a command Loom tracked in an
/// earlier session, when there is no child handle left to wait on.
///
/// Best effort, and it errs towards "gone": `OpenProcess` fails for an elevated
/// or other-user process, which reports a live orphan as dead, and pids are
/// reused, so a pid recorded in an earlier session may now belong to something
/// else entirely. That is tolerable for the one log line it feeds
/// (`mark_interrupted_commands`) and would not be for anything that decides
/// whether to signal the process — which is why `stop_command` signals the pid
/// recorded on the row without asking this first.
#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// `STILL_ACTIVE` from the Windows SDK: the exit code a live process has.
    const STILL_ACTIVE: u32 = 259;

    if pid == 0 {
        return false;
    }
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut code = 0u32;
        let alive = GetExitCodeProcess(handle, &mut code).is_ok() && code == STILL_ACTIVE;
        let _ = CloseHandle(handle);
        alive
    }
}

#[cfg(not(windows))]
pub fn is_alive(pid: u32) -> bool {
    // `kill -0` asks "may I signal this process?" without signalling it: exit 0
    // means it exists, anything else means it does not (or is not ours).
    if pid == 0 {
        return false;
    }
    hidden_std("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// The last `lines` lines of a command's log, reading only the tail of the
/// file so a huge log stays cheap to inspect.
pub fn log_tail(path: &Path, lines: usize) -> Result<String> {
    /// How much of the end of a log is worth reading for a tail request.
    const WINDOW: u64 = 256 * 1024;

    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let length = file
        .metadata()
        .map_err(|e| Error::io(path, e))?
        .len();
    let start = length.saturating_sub(WINDOW);
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .map_err(|e| Error::io(path, e))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| Error::io(path, e))?;

    let text = String::from_utf8_lossy(&bytes);
    let wanted: Vec<&str> = text.lines().rev().take(lines.max(1)).collect();
    if wanted.is_empty() {
        return Ok("(no output yet)".to_string());
    }
    let mut out: Vec<&str> = wanted.into_iter().rev().collect();
    if start > 0 {
        out.insert(0, "… [earlier output not shown]");
    }
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command line that prints, waits, and finishes — portable enough for
    /// both shells. `ping` on Windows is the dependency-free sleep.
    fn sleeper(ms: u64) -> String {
        if cfg!(windows) {
            format!("echo hello && ping -n {} 127.0.0.1 >nul && echo bye", (ms / 900).max(1) + 1)
        } else {
            format!("echo hello && sleep {} && echo bye", ms as f64 / 1000.0)
        }
    }

    /// A current-thread runtime, because spawning and pumping a child needs a
    /// reactor and these tests are not async themselves.
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn shell_matches_the_platform() {
        let (shell, flag) = shell();
        if cfg!(windows) {
            assert_eq!((shell, flag), ("cmd", "/C"));
        } else {
            assert_eq!((shell, flag), ("sh", "-c"));
        }
    }

    #[test]
    fn log_paths_live_under_the_logs_folder() {
        let _guard = paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());
        let path = command_log_path("abc").unwrap();
        assert!(path.ends_with("logs/cmd-abc.log") || path.ends_with("logs\\cmd-abc.log"));
        std::env::remove_var("LOOM_HOME");
    }

    #[test]
    fn spawning_outside_a_runtime_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("logs/cmd-test.log");
        let error = match Running::spawn("echo loom", dir.path(), &log) {
            Ok(_) => panic!("spawning without a runtime should fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("Tokio runtime"), "{error}");
    }

    #[test]
    fn a_hidden_command_runs_and_reports_its_output() {
        runtime().block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let log = dir.path().join("logs/cmd-test.log");
            let mut running = Running::spawn("echo loom", dir.path(), &log).expect("spawn");

            let status = running.wait_timeout(Duration::from_secs(30)).await;
            assert!(
                matches!(status, Wait::Exited(status) if status.code() == Some(0)),
                "a command that finished should report its exit"
            );

            let (stdout, _) = running.finish().await;
            assert!(stdout.contains("loom"), "{stdout}");

            // The log is the durable copy, and it is what a later session reads.
            let text = std::fs::read_to_string(&log).unwrap();
            assert!(text.contains("loom"), "{text}");
        });
    }

    #[test]
    fn a_command_in_flight_times_out_without_being_killed() {
        runtime().block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let log = dir.path().join("logs/cmd-test.log");
            let mut running = Running::spawn(&sleeper(3_000), dir.path(), &log).expect("spawn");
            let pid = running.pid();
            assert!(pid != 0);

            // Past the cap: still running, and reported as such rather than as
            // an exit. This is the behaviour the user chose.
            let timed_out = running.wait_timeout(Duration::from_millis(50)).await;
            assert!(
                matches!(timed_out, Wait::Running),
                "a running command must not report an exit"
            );
            assert!(is_alive(pid), "the command must still be alive");

            assert!(kill_tree(pid), "the kill should be reported as delivered");
            let ended = running.wait_timeout(Duration::from_secs(30)).await;
            assert!(
                matches!(ended, Wait::Exited(_)),
                "the killed command should exit"
            );
            assert!(!is_alive(pid), "kill_tree should end the process");
        });
    }

    /// The tree, not just the shell: `sh -c "sleep 60"` puts the sleep in the
    /// same process group, and that is what Stop has to reach. Before the fix
    /// this left the grandchild running with the log still open.
    #[cfg(unix)]
    #[test]
    fn kill_tree_reaches_what_the_shell_started() {
        runtime().block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let log = dir.path().join("logs/cmd-tree.log");
            let mut running =
                Running::spawn("sleep 60 & echo $!; wait", dir.path(), &log).expect("spawn");
            let pid = running.pid();

            // Let the shell print its child's pid before killing anything.
            let mut waited = 0;
            let child_pid = loop {
                let text = std::fs::read_to_string(&log).unwrap_or_default();
                if let Some(line) = text.lines().next().and_then(|l| l.trim().parse::<u32>().ok()) {
                    break line;
                }
                assert!(waited < 5_000, "the shell never started its child");
                tokio::time::sleep(Duration::from_millis(50)).await;
                waited += 50;
            };
            assert!(is_alive(child_pid), "the grandchild should be running");

            kill_tree(pid);
            let _ = running.wait_timeout(Duration::from_secs(10)).await;
            // SIGTERM may take a moment to reap.
            let mut waited = 0;
            while is_alive(child_pid) && waited < 2_000 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                waited += 50;
            }
            assert!(
                !is_alive(child_pid),
                "kill_tree must end the shell's children too (pid {child_pid})"
            );
        });
    }

    #[test]
    fn log_tail_reads_the_end_of_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cmd.log");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert_eq!(log_tail(&path, 2).unwrap(), "two\nthree");

        std::fs::write(&path, "").unwrap();
        assert_eq!(log_tail(&path, 10).unwrap(), "(no output yet)");

        assert!(log_tail(&dir.path().join("missing.log"), 5).is_err());
    }

    #[test]
    fn the_log_cap_is_a_megabyte_scale_number() {
        // Guards against a typo turning the cap into 5 bytes or 5 GiB.
        assert_eq!(LOG_LIMIT_BYTES, 5 * 1024 * 1024);
        assert_eq!(COMMAND_TIMEOUT.as_secs(), 120);
    }
}
