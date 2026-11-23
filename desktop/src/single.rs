//! One loom to a data folder. Started again while it runs (from the tray, after a
//! background update, from a second shortcut), loom asks the one running to show its
//! window and ends, so two never write the same cache.
//!
//! On Windows the loom running owns a named mutex for its data folder in the session's
//! namespace (`Local\loom-<hash>`), on a thread of its own that also waits for
//! `Local\loom-<hash>-show`. Another start sets that event, and the owner answers on
//! `…-shown` and brings its window forward. While it quits, the owner lets go of the
//! mutex and stops answering, so a loom started then waits for the mutex and runs. A
//! loom started with `--background` (at login, or by the updater) asks for nothing to be
//! shown: it waits for the folder and runs, or ends if another loom keeps it.
//!
//! Elsewhere a lock on `loom.lock` in the data folder keeps the one loom, and the
//! system brings the running app forward itself (macOS reopens it).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a loom started again waits for the one before it to quit, as one quitting
/// from the tray stops its cores and the instance in the app.
pub const WAIT: Duration = Duration::from_secs(20);

/// The name the data folder `data` is known by, in the session: the same however the
/// folder is spelt, and another for every other folder, such as a test's.
pub fn name(data: &Path) -> String {
    let full = std::fs::canonicalize(data).unwrap_or_else(|_| data.to_path_buf());
    let mut key = full.to_string_lossy().into_owned();
    while key.len() > 1 && (key.ends_with('\\') || key.ends_with('/')) {
        key.pop();
    }
    if cfg!(windows) {
        key = key.to_lowercase();
    }
    // FNV-1a: small, and the same in every build.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("loom-{h:016x}")
}

/// What `claim` found.
pub enum Start {
    /// This loom has the folder, for as long as it holds this.
    First(Owner),
    /// Another loom has it, and was asked to show itself if `show`. This one ends.
    Running,
}

/// The folder, held by this loom.
pub struct Owner {
    shown: Arc<AtomicBool>,
    held: imp::Held,
}

impl Owner {
    /// Whether another start asked for the window since the last look.
    pub fn asked_to_show(&self) -> bool {
        self.shown.swap(false, Ordering::Relaxed)
    }

    /// Lets go of the folder, as loom quits, for a loom started while it stops.
    pub fn let_go(self) {
        self.held.let_go();
    }
}

/// Takes the data folder `data` for this loom, or, when another loom has it, asks that
/// one to show its window (if `show`) and gives `Running`. It waits up to `wait` for a
/// loom that's quitting.
pub fn claim(data: &Path, show: bool, wait: Duration) -> Start {
    let _ = std::fs::create_dir_all(data);
    let shown = Arc::new(AtomicBool::new(false));
    match imp::claim(data, &name(data), show, wait, shown.clone()) {
        Some(held) => Start::First(Owner { shown, held }),
        None => Start::Running,
    }
}

/// This loom's hold on its folder, while the app runs.
static OWNER: Mutex<Option<Owner>> = Mutex::new(None);

/// Keeps `owner` for the app: `asked_to_show` and `let_go` use it.
pub fn keep(owner: Owner) {
    if let Ok(mut o) = OWNER.lock() {
        *o = Some(owner);
    }
}

/// Whether another start asked for the window since the last look.
pub fn asked_to_show() -> bool {
    OWNER
        .lock()
        .ok()
        .and_then(|o| o.as_ref().map(Owner::asked_to_show))
        .unwrap_or(false)
}

/// Lets go of the folder, as loom quits.
pub fn let_go() {
    if let Some(o) = OWNER.lock().ok().and_then(|mut o| o.take()) {
        o.let_go();
    }
}

#[cfg(windows)]
mod imp {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, ReleaseMutex, SetEvent, WaitForMultipleObjects,
        WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};

    /// How long a start waits for the loom running to answer that it's showing.
    const ANSWER: Duration = Duration::from_secs(2);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// An auto-reset event, made or opened by its name. Null if it couldn't be.
    fn event(name: &str) -> HANDLE {
        let n = wide(name);
        // SAFETY: a null-terminated name, no security attributes.
        unsafe { CreateEventW(std::ptr::null(), 0, 0, n.as_ptr()) }
    }

    fn close(h: HANDLE) {
        if !h.is_null() {
            // SAFETY: a handle this module opened, closed once.
            unsafe { CloseHandle(h) };
        }
    }

    fn millis(d: Duration) -> u32 {
        u32::try_from(d.as_millis()).unwrap_or(INFINITE - 1)
    }

    /// The owner thread's: the event that tells it to let go, and the thread.
    pub struct Held {
        quit: isize,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Held {
        pub fn let_go(mut self) {
            // SAFETY: the owner thread's quit event, alive until it's joined below.
            unsafe { SetEvent(self.quit as HANDLE) };
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
            close(self.quit as HANDLE);
        }
    }

    /// The mutex is owned by the thread that took it, so a thread of its own takes it
    /// and keeps it, answering other starts, until it's told to let go.
    pub fn claim(
        _data: &Path,
        name: &str,
        show: bool,
        wait: Duration,
        shown: Arc<AtomicBool>,
    ) -> Option<Held> {
        // SAFETY: an unnamed manual-reset event, no security attributes.
        let quit = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) } as isize;
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let name = name.to_string();
        let thread = std::thread::Builder::new()
            .name("loom-single".into())
            .spawn(move || own(&name, show, wait, shown, quit, tx))
            .ok()?;
        // A thread that couldn't say is a loom that runs: one loom too many beats none.
        if rx.recv().unwrap_or(true) {
            Some(Held {
                quit,
                thread: Some(thread),
            })
        } else {
            let _ = thread.join();
            close(quit as HANDLE);
            None
        }
    }

    fn own(
        name: &str,
        show: bool,
        wait: Duration,
        shown: Arc<AtomicBool>,
        quit: isize,
        tx: std::sync::mpsc::Sender<bool>,
    ) {
        let started = Instant::now();
        let show_ev = event(&format!("Local\\{name}-show"));
        let answer_ev = event(&format!("Local\\{name}-shown"));
        let n = wide(&format!("Local\\{name}"));
        // SAFETY: a null-terminated name, no security attributes, not owned yet.
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, n.as_ptr()) };
        let done = |owned: bool| {
            close(show_ev);
            close(answer_ev);
            close(mutex);
            let _ = tx.send(owned);
        };
        if mutex.is_null() || show_ev.is_null() || answer_ev.is_null() {
            done(true);
            return;
        }
        // Abandoned, the loom before ended without letting go: it's this one's now.
        // SAFETY: the mutex opened above.
        let take = |ms: u32| {
            matches!(
                unsafe { WaitForSingleObject(mutex, ms) },
                WAIT_OBJECT_0 | WAIT_ABANDONED
            )
        };
        if !take(0) {
            if show {
                // SAFETY: lets the loom running come to the front, as this start was the
                // person's own; then the events opened above.
                let answered = unsafe {
                    AllowSetForegroundWindow(ASFW_ANY);
                    SetEvent(show_ev);
                    WaitForSingleObject(answer_ev, millis(ANSWER)) == WAIT_OBJECT_0
                };
                if answered {
                    done(false);
                    return;
                }
            }
            // No answer: the loom running is quitting. Wait for it to let go.
            if !take(millis(wait.saturating_sub(started.elapsed()))) {
                done(false);
                return;
            }
        }
        let _ = tx.send(true);
        // The quit event first, so a loom letting go doesn't answer a start it won't show.
        let handles = [quit as HANDLE, show_ev];
        loop {
            // SAFETY: two live handles, waiting for either.
            let r = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) };
            if r == WAIT_OBJECT_0 + 1 {
                shown.store(true, Ordering::Relaxed);
                crate::wake::wake();
                // SAFETY: the answer event opened above.
                unsafe { SetEvent(answer_ev) };
                continue;
            }
            break;
        }
        // SAFETY: the mutex this thread owns.
        unsafe { ReleaseMutex(mutex) };
        close(show_ev);
        close(answer_ev);
        close(mutex);
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// The lock on `loom.lock`, held until it's let go or loom ends.
    pub struct Held {
        _file: Option<std::fs::File>,
    }

    impl Held {
        pub fn let_go(self) {}
    }

    pub fn claim(
        data: &Path,
        _name: &str,
        show: bool,
        wait: Duration,
        _shown: Arc<AtomicBool>,
    ) -> Option<Held> {
        // Nothing here asks the loom running to show itself, so a start the person made
        // ends at once rather than waiting; the system brings that loom forward.
        let wait = if show { Duration::ZERO } else { wait };
        let opened = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(data.join("loom.lock"));
        // A folder that can't be locked runs: one loom too many beats none.
        let Ok(file) = opened else {
            return Some(Held { _file: None });
        };
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < wait => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(std::fs::TryLockError::WouldBlock) => return None,
                _ => return Some(Held { _file: Some(file) }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn folder(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("loom-single-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_folder_has_one_name_however_it_is_spelt() {
        let d = folder("name");
        let plain = name(&d);
        let mut slash = d.clone().into_os_string();
        slash.push(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(name(Path::new(&slash)), plain);
        assert_eq!(name(&d.join("sub").join("..")), plain);
        if cfg!(windows) {
            let upper = d.to_string_lossy().to_uppercase();
            assert_eq!(name(Path::new(&upper)), plain);
        }
        assert_ne!(name(&folder("other")), plain);
        assert!(plain.starts_with("loom-") && plain.len() == 21);
    }

    /// A second start asks the first to show itself and ends; the first hears it once.
    #[test]
    fn a_second_start_asks_the_first_to_show_itself() {
        let d = folder("show");
        let Start::First(first) = claim(&d, true, WAIT) else {
            panic!("the folder wasn't free");
        };
        assert!(!first.asked_to_show());
        let t = Instant::now();
        assert!(matches!(claim(&d, true, WAIT), Start::Running));
        assert!(t.elapsed() < Duration::from_secs(2));
        if cfg!(windows) {
            assert!(first.asked_to_show());
            assert!(!first.asked_to_show());
        }
        // Started in the tray, it asks for nothing.
        assert!(matches!(
            claim(&d, false, Duration::from_millis(200)),
            Start::Running
        ));
        assert!(!first.asked_to_show());
        first.let_go();
    }

    /// A loom started while the one before quits waits for it, as after an update, and
    /// runs once it has let go.
    #[test]
    fn a_start_waits_for_the_loom_that_is_quitting() {
        let d = folder("quit");
        let Start::First(first) = claim(&d, true, WAIT) else {
            panic!("the folder wasn't free");
        };
        let quitting = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            first.let_go();
        });
        let t = Instant::now();
        let Start::First(second) = claim(&d, false, WAIT) else {
            panic!("the folder wasn't let go");
        };
        assert!(t.elapsed() >= Duration::from_millis(250));
        assert!(t.elapsed() < Duration::from_secs(5));
        quitting.join().unwrap();
        second.let_go();
        // And with nothing held, the next start has it at once.
        let Start::First(third) = claim(&d, true, WAIT) else {
            panic!("the folder wasn't let go");
        };
        third.let_go();
    }
}
