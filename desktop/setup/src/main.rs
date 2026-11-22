//! loom-setup: loom's installer, updater and uninstaller for Windows, in one small
//! program (README.md).
//!
//! ```text
//! loom-setup.exe                               install, with a window
//! loom-setup.exe --silent                      install, without one
//! loom-setup.exe --update --pid N [--background]
//!                                              the app hands over an update and quits:
//!                                              wait for it, swap, start the new one,
//!                                              and go back to the old one if it fails
//! uninstall.exe --uninstall [--silent]         take loom off again
//! ```
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod draw;
mod install;
mod pack;
mod window;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use draw::tokens::*;
use draw::{ease_out, mix, smooth, Canvas, Face, Mood, Palette, Thread};
use window::{Hit, Key, Next, H, W};

#[derive(Default)]
struct Args {
    update: bool,
    uninstall: bool,
    silent: bool,
    background: bool,
    pid: Option<u32>,
    from: Option<PathBuf>,
}

fn args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(w) = it.next() {
        match w.as_str() {
            "--update" => a.update = true,
            "--uninstall" => a.uninstall = true,
            "--silent" => a.silent = true,
            "--background" => a.background = true,
            "--pid" => a.pid = it.next().and_then(|p| p.parse().ok()),
            "--from" => a.from = it.next().map(PathBuf::from),
            _ => {}
        }
    }
    a
}

fn main() {
    let a = args();
    // Only v0.1's updater ever names this program loom.exe (scripts/release.calx bridge).
    let bridged = std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().eq_ignore_ascii_case("loom.exe"))
        })
        .unwrap_or(false);
    let code = if a.uninstall {
        uninstall_main(a)
    } else if a.update {
        update_main(a)
    } else if bridged {
        bridge_main()
    } else {
        install_main(a)
    };
    std::process::exit(code);
}

fn bridge_main() -> i32 {
    let p = match pack::read_own() {
        Ok(p) => Arc::new(p),
        // Named loom.exe but carrying nothing: not v0.1's handover, so it's as if it
        // were opened by hand.
        Err(_) => return install_main(Args::default()),
    };
    let mut app = App::new(Mode::Update { background: false }, Ok(p.clone()));
    app.work = Some(start(move |w| do_bridge(w, &p)));
    app.go(Page::Working, 0.0);
    window::run(Box::new(app), false, window::dark_mode());
    0
}

/// A line in update.log in the app's data folder, so an update that went wrong while
/// no one watched can be looked into.
fn log(line: &str) {
    if let Ok(dir) = install::data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("update.log"))
        {
            let _ = writeln!(f, "{} {line}", unix_now());
        }
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// --- The work, on a thread of its own ---

#[derive(Default)]
struct Work {
    step: String,
    progress: f32,
    /// The work is over: how it went.
    done: Option<Result<(), String>>,
    /// The update didn't start, and the version before is back.
    rolled_back: Option<String>,
    /// The new loom is running and being watched, so the window can go.
    opened: bool,
}

type Shared = Arc<Mutex<Work>>;

fn report(w: &Shared, step: &str, progress: f32) {
    let mut g = w.lock().unwrap();
    g.step = step.to_string();
    g.progress = progress;
}

/// Installs what the pack carries, closing the loom programs in `close` first and
/// removing v0.1 if it's there.
fn do_install(
    w: &Shared,
    p: &pack::Pack,
    close: &[u32],
    old: Option<&install::Old>,
) -> Result<(), String> {
    let dir = install::install_dir()?;
    if !close.is_empty() {
        report(w, "Closing loom", 0.02);
        install::close(close);
    }
    report(w, "Unpacking", 0.1);
    let exe = p.unpack(&|k| report(w, "Unpacking", 0.1 + 0.65 * k))?;
    report(w, "Copying files", 0.8);
    install::place(&dir, &exe, &p.stub, &p.version)?;
    if let Some(o) = old {
        report(w, &format!("Removing loom {}", o.version), 0.9);
        install::remove_v01(o, &dir.join("loom.exe"));
    }
    report(w, "Done", 1.0);
    Ok(())
}

/// v0.1's own updater handing over: it put this setup in its folder as loom.exe and
/// started it. So it installs loom as usual, takes v0.1 off, starts the new loom, and
/// has this program removed once it has ended. There's no going back to v0.1, so no
/// watching the new loom start.
fn do_bridge(w: &Shared, p: &pack::Pack) -> Result<(), String> {
    let dir = install::install_dir()?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = dir.join("loom.exe");
    let old = install::find_v01();
    let still = install::running(&exe);
    if !still.is_empty() {
        install::close(&still);
    }
    report(w, "Unpacking", 0.1);
    let new = p.unpack(&|k| report(w, "Unpacking", 0.1 + 0.65 * k))?;
    report(w, "Copying files", 0.8);
    install::place(&dir, &new, &p.stub, &p.version)?;
    if let Some(o) = &old {
        install::remove_v01(o, &exe);
    }
    // In v0.1's folder, which is loom's own, this program is loom.old.exe now.
    if me
        .to_string_lossy()
        .eq_ignore_ascii_case(&exe.to_string_lossy())
    {
        install::remove_later(&dir.join("loom.old.exe"));
    }
    report(w, "Opening loom", 1.0);
    install::launch(&exe, &[])?;
    w.lock().unwrap().opened = true;
    log(&format!(
        "moved loom {} to {}",
        old.map(|o| o.version).unwrap_or_default(),
        p.version
    ));
    Ok(())
}

/// The update the app handed over: wait for the app to end, put the new version in,
/// start it, and watch it start. If it doesn't, the version before goes back.
fn do_update(w: &Shared, p: &pack::Pack, pid: Option<u32>, background: bool) -> Result<(), String> {
    let dir = install::install_dir()?;
    let exe = dir.join("loom.exe");
    let before = install::installed_version().unwrap_or_default();
    let launch_args: &[&str] = if background { &["--background"] } else { &[] };
    report(w, "Waiting for loom to close", 0.02);
    if let Some(pid) = pid {
        if !install::wait_for(pid, 30000) {
            return Err("loom didn't close, so the update waits for next time".into());
        }
    }
    let still = install::running(&exe);
    if !still.is_empty() {
        install::close(&still);
    }
    let put = (|| {
        report(w, "Unpacking", 0.1);
        let new = p.unpack(&|k| report(w, "Unpacking", 0.1 + 0.6 * k))?;
        report(w, "Copying files", 0.8);
        install::place(&dir, &new, &p.stub, &p.version)
    })();
    if let Err(e) = put {
        // Whatever happened, the person gets their loom back.
        let _ = install::launch(&exe, launch_args);
        return Err(e);
    }
    report(w, "Opening loom", 1.0);
    let mut child = match install::launch(&exe, launch_args) {
        Ok(c) => c,
        Err(e) => return roll_back(w, &dir, &before, &p.version, launch_args, &e),
    };
    w.lock().unwrap().opened = true;
    // A loom that's still running after ten seconds started.
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(Some(status)) = child.try_wait() {
            if !status.success() {
                let why = format!("it ended at once ({status})");
                return roll_back(w, &dir, &before, &p.version, launch_args, &why);
            }
            break;
        }
    }
    install::drop_old(&dir);
    log(&format!("updated loom {before} to {}", p.version));
    Ok(())
}

fn roll_back(
    w: &Shared,
    dir: &Path,
    before: &str,
    new: &str,
    launch_args: &[&str],
    why: &str,
) -> Result<(), String> {
    log(&format!(
        "loom {new} didn't start: {why}; going back to {before}"
    ));
    install::roll_back(dir, before)?;
    let _ = install::launch(&dir.join("loom.exe"), launch_args);
    w.lock().unwrap().rolled_back = Some(before.to_string());
    Ok(())
}

fn do_uninstall(w: &Shared, dir: &Path, data: bool) -> Result<(), String> {
    report(w, "Closing loom", 0.1);
    install::close(&install::running(&dir.join("loom.exe")));
    report(w, "Removing loom", 0.4);
    install::remove(dir, data)?;
    report(w, "Done", 1.0);
    Ok(())
}

fn start(work: impl FnOnce(&Shared) -> Result<(), String> + Send + 'static) -> Shared {
    let w: Shared = Arc::new(Mutex::new(Work::default()));
    let w2 = w.clone();
    std::thread::spawn(move || {
        let r = work(&w2);
        w2.lock().unwrap().done = Some(r);
    });
    w
}

// --- The modes ---

fn install_main(a: Args) -> i32 {
    let p = pack::read_own();
    if a.silent {
        let p = match p {
            Ok(p) => p,
            Err(e) => {
                log(&e);
                return 1;
            }
        };
        let dir = install::install_dir().unwrap_or_default();
        let close = install::running(&dir.join("loom.exe"));
        let old = install::find_v01();
        let w: Shared = Default::default();
        return match do_install(&w, &p, &close, old.as_ref()) {
            Ok(()) => 0,
            Err(e) => {
                log(&e);
                1
            }
        };
    }
    let app = App::new(Mode::Install, p.map(Arc::new));
    window::run(Box::new(app), false, window::dark_mode());
    0
}

fn update_main(a: Args) -> i32 {
    let p = match pack::read_own() {
        Ok(p) => Arc::new(p),
        Err(e) => {
            log(&format!("update refused: {e}"));
            // The app quit to hand over; give it back.
            if let Ok(dir) = install::install_dir() {
                let args: &[&str] = if a.background { &["--background"] } else { &[] };
                let _ = install::launch(&dir.join("loom.exe"), args);
            }
            return 1;
        }
    };
    let mut app = App::new(
        Mode::Update {
            background: a.background,
        },
        Ok(p.clone()),
    );
    let (pid, background) = (a.pid, a.background);
    app.work = Some(start(move |w| do_update(w, &p, pid, background)));
    app.go(Page::Working, 0.0);
    window::run(Box::new(app), a.background, window::dark_mode());
    0
}

fn uninstall_main(a: Args) -> i32 {
    let Some(dir) = a.from.clone() else {
        // Started from where loom is installed: go on from a copy in the temporary
        // folder, so everything here can be removed.
        let Ok(dir) = install::install_dir() else {
            return 1;
        };
        let Ok(copy) = install::copy_to_temp() else {
            return 1;
        };
        let mut c = std::process::Command::new(copy);
        c.arg("--uninstall").arg("--from").arg(&dir);
        if a.silent {
            c.arg("--silent");
        }
        return if c.spawn().is_ok() { 0 } else { 1 };
    };
    if a.silent {
        let w: Shared = Default::default();
        let r = do_uninstall(&w, &dir, false);
        install::remove_self_later();
        return if r.is_ok() { 0 } else { 1 };
    }
    let mut app = App::new(Mode::Uninstall, Err(String::new()));
    app.dir = dir;
    app.page = Page::AskUninstall;
    window::run(Box::new(app), false, window::dark_mode());
    install::remove_self_later();
    0
}

// --- The window's pages ---

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Install,
    Update { background: bool },
    Uninstall,
}

#[derive(Clone, PartialEq)]
enum Page {
    Welcome,
    CloseFirst(Vec<u32>),
    Working,
    Done,
    Failed(String),
    AskUninstall,
    Gone,
    RolledBack(String),
}

const CLOSE: usize = 0;
const PRIMARY: usize = 1;
const SECONDARY: usize = 2;
const TOGGLE: usize = 3;

struct App {
    mode: Mode,
    page: Page,
    page_at: f32,
    pal: Palette,
    thread: Thread,
    pack: Result<Arc<pack::Pack>, String>,
    work: Option<Shared>,
    installed: Option<String>,
    old: Option<install::Old>,
    dir: PathBuf,
    delete_data: bool,
    toggled_at: f32,
    gone_at: f32,
    ticked: f32,
    now: f32,
    elevated: bool,
}

impl App {
    fn new(mode: Mode, pack: Result<Arc<pack::Pack>, String>) -> App {
        let pal = Palette::new(window::dark_mode());
        App {
            mode,
            page: Page::Welcome,
            page_at: 0.0,
            pal,
            thread: Thread::new(pal.ink_3),
            pack,
            work: None,
            installed: install::installed_version(),
            old: if mode == Mode::Install {
                install::find_v01()
            } else {
                None
            },
            dir: install::install_dir().unwrap_or_default(),
            delete_data: false,
            toggled_at: -1.0,
            gone_at: -1.0,
            ticked: 0.0,
            now: 0.0,
            elevated: install::elevated(),
        }
    }

    fn version(&self) -> String {
        self.pack
            .as_ref()
            .map(|p| p.version.clone())
            .unwrap_or_default()
    }

    fn go(&mut self, page: Page, t: f32) {
        if self.page != page {
            self.page = page;
            self.page_at = t;
        }
    }

    fn begin_install(&mut self, close: Vec<u32>, t: f32) {
        let Ok(p) = self.pack.clone() else { return };
        let old = install::find_v01();
        self.work = Some(start(move |w| do_install(w, &p, &close, old.as_ref())));
        self.go(Page::Working, t);
    }

    fn primary(&mut self, t: f32) -> Next {
        match self.page.clone() {
            Page::Welcome => {
                if self.pack.is_err() {
                    return Next::Quit;
                }
                let running = install::running(&self.dir.join("loom.exe"));
                if running.is_empty() {
                    self.begin_install(Vec::new(), t);
                } else {
                    self.go(Page::CloseFirst(running), t);
                }
                Next::Stay
            }
            Page::CloseFirst(pids) => {
                self.begin_install(pids, t);
                Next::Stay
            }
            Page::Done => {
                if self.mode == Mode::Install && !self.elevated {
                    let _ = install::launch(&self.dir.join("loom.exe"), &[]);
                }
                Next::Quit
            }
            Page::Failed(_) if self.mode == Mode::Install => {
                self.go(Page::Welcome, t);
                Next::Stay
            }
            Page::AskUninstall => {
                let (dir, data) = (self.dir.clone(), self.delete_data);
                self.work = Some(start(move |w| do_uninstall(w, &dir, data)));
                self.go(Page::Working, t);
                Next::Stay
            }
            Page::Working => Next::Stay,
            _ => Next::Quit,
        }
    }

    fn back(&mut self, t: f32) -> Next {
        match self.page {
            Page::Working => Next::Stay,
            Page::CloseFirst(_) => {
                self.go(Page::Welcome, t);
                Next::Stay
            }
            _ => Next::Quit,
        }
    }

    /// The page's words: its title, and a line under it only when there's something the
    /// person needs to know: an error, what a step will stop, or what was put back.
    fn words(&self) -> (String, String) {
        let v = self.version();
        let none = String::new;
        match (&self.page, self.mode) {
            (Page::Welcome, _) => match (&self.pack, &self.installed) {
                (Err(e), _) => ("This setup can't install loom".into(), e.clone()),
                (Ok(_), Some(i)) if *i == v => (format!("loom {v} is already installed"), none()),
                (Ok(_), Some(i)) if is_newer(i, &v) => (
                    format!("loom {i} is installed"),
                    format!("This replaces it with {v}, an older version."),
                ),
                (Ok(_), Some(i)) => (format!("Update loom {i} to {v}"), none()),
                (Ok(_), None) => match &self.old {
                    Some(o) => (
                        format!("Install loom {v}"),
                        format!(
                            "loom {} will be uninstalled first. Its data in .loom is kept.",
                            o.version
                        ),
                    ),
                    None => (format!("Install loom {v}"), none()),
                },
            },
            (Page::CloseFirst(_), _) => (
                "loom is running".into(),
                "It has to close first. Agents running on this computer will stop.".into(),
            ),
            (Page::Working, Mode::Install) => ("Installing loom".into(), none()),
            (Page::Working, Mode::Update { .. }) => (format!("Updating loom to {v}"), none()),
            (Page::Working, Mode::Uninstall) => ("Uninstalling loom".into(), none()),
            (Page::Done, _) if self.elevated => (
                "loom is installed".into(),
                "Open it from the Start menu, so it doesn't run as administrator.".into(),
            ),
            (Page::Done, _) => ("loom is installed".into(), none()),
            (Page::Failed(e), Mode::Install) => ("Installation failed".into(), e.clone()),
            (Page::Failed(e), Mode::Update { .. }) => (
                "The update failed".into(),
                format!("{e}. The installed version is unchanged."),
            ),
            (Page::Failed(e), Mode::Uninstall) => ("Uninstall failed".into(), e.clone()),
            (Page::AskUninstall, _) => ("Uninstall loom?".into(), none()),
            (Page::Gone, _) => ("loom is uninstalled".into(), none()),
            (Page::RolledBack(before), _) => (
                format!("loom {v} failed to start"),
                format!("loom {before} has been restored."),
            ),
        }
    }

    fn buttons(&self) -> Vec<(usize, &'static str)> {
        let installed_same = self.installed.as_deref() == Some(self.version().as_str());
        match (&self.page, self.mode) {
            (Page::Welcome, _) if self.pack.is_err() => vec![(PRIMARY, "Close")],
            (Page::Welcome, _) if installed_same => vec![(PRIMARY, "Reinstall")],
            (Page::Welcome, _) if self.installed.is_some() => vec![(PRIMARY, "Update")],
            (Page::Welcome, _) => vec![(PRIMARY, "Install")],
            (Page::CloseFirst(_), _) => {
                vec![(SECONDARY, "Cancel"), (PRIMARY, "Close loom and continue")]
            }
            (Page::Working, _) => vec![],
            (Page::Done, _) if self.elevated => vec![(PRIMARY, "Close")],
            (Page::Done, _) => vec![(PRIMARY, "Open loom")],
            (Page::Failed(_), Mode::Install) => vec![(SECONDARY, "Close"), (PRIMARY, "Try again")],
            (Page::AskUninstall, _) => vec![(SECONDARY, "Cancel"), (PRIMARY, "Uninstall")],
            _ => vec![(PRIMARY, "Close")],
        }
    }
}

/// Whether version `a` is newer than `b`, as shared/release.calx compares them.
fn is_newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| {
        let v = v.trim_start_matches('v');
        let (main, pre) = v.split_once('-').unwrap_or((v, ""));
        let mut n: Vec<i64> = main.split('.').map(|x| x.parse().unwrap_or(0)).collect();
        n.resize(3, 0);
        (n, pre.to_string())
    };
    let ((an, ap), (bn, bp)) = (parts(a), parts(b));
    if an != bn {
        return an > bn;
    }
    if ap == bp {
        return false;
    }
    if ap.is_empty() {
        return true;
    }
    if bp.is_empty() {
        return false;
    }
    ap > bp
}

// Where things go, in logical pixels.
const MARK_Y: f32 = 96.0;
const THREAD_Y: f32 = 160.0;
const THREAD_ROOM: f32 = 22.0;
const SIDE: f32 = 56.0;
const TITLE_Y: f32 = 190.0;

impl window::Screen for App {
    fn tick(&mut self, t: f32) -> Next {
        let Some(w) = self.work.clone() else {
            return Next::Stay;
        };
        let (done, rolled, opened, progress) = {
            let g = w.lock().unwrap();
            (g.done.clone(), g.rolled_back.clone(), g.opened, g.progress)
        };
        // The loop follows the work, never ahead of it, and travels at most the whole
        // thread a second, so it's seen to go along even when the work is quick.
        let dt = (t - self.ticked).clamp(0.0, 0.1);
        self.ticked = t;
        self.thread.head += (progress - self.thread.head).clamp(0.0, dt * 1.1);
        let arrived = self.thread.head >= 0.995;
        if let Mode::Update { background } = self.mode {
            if let Some(before) = rolled {
                self.work = None;
                if background {
                    return Next::Quit;
                }
                self.go(Page::RolledBack(before), t);
                return Next::Show;
            }
            match done {
                Some(Ok(())) => return Next::Quit,
                Some(Err(e)) => {
                    log(&format!("update failed: {e}"));
                    self.work = None;
                    if background {
                        return Next::Quit;
                    }
                    self.go(Page::Failed(e), t);
                    return Next::Show;
                }
                None if opened => return Next::Hide,
                None => return Next::Stay,
            }
        }
        match done {
            // Done once the loop has reached the end of the thread too.
            Some(Ok(())) if !arrived => {}
            Some(Ok(())) => {
                self.work = None;
                if self.mode == Mode::Uninstall {
                    self.gone_at = t;
                    self.go(Page::Gone, t);
                } else {
                    self.go(Page::Done, t);
                }
            }
            Some(Err(e)) => {
                self.work = None;
                self.go(Page::Failed(e), t);
            }
            None => {}
        }
        Next::Stay
    }

    fn click(&mut self, id: usize, t: f32) -> Next {
        match id {
            CLOSE | SECONDARY => self.back(t),
            PRIMARY => self.primary(t),
            TOGGLE => {
                self.delete_data = !self.delete_data;
                self.toggled_at = t;
                Next::Stay
            }
            _ => Next::Stay,
        }
    }

    fn key(&mut self, key: Key, t: f32) -> Next {
        match key {
            Key::Enter => self.primary(t),
            Key::Escape => self.back(t),
        }
    }

    fn draw(&mut self, c: &mut Canvas, t: f32, hovers: &[f32], pressed: Option<usize>) -> Vec<Hit> {
        let p = self.pal;
        let mut hits = Vec::new();
        self.now = t;
        c.clear(p.bg);
        let span = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);

        // The thread: grey while it waits for the person, in loom's first colour once
        // it works, slack and red if it failed.
        let (mood, tint) = match self.page {
            Page::Working => (Mood::Working, p.thread),
            Page::Done => (Mood::Done, p.thread),
            Page::Failed(_) | Page::RolledBack(_) => (Mood::Failed, mix(p.bad, p.bg, 0.3)),
            _ => (Mood::Idle, p.ink_3),
        };
        if mood == Mood::Done {
            self.thread.head = 1.0;
        }
        self.thread.set(mood, tint, t);

        // Opening: the thread draws itself across, the mark settles onto it, and the
        // rest opens out from the thread's line, as the app opens.
        let reveal = ease_out(span(0.0, 0.6));
        let mark_in = ease_out(span(0.25, 0.7));
        let open = smooth(span(0.55, 1.0));
        // Uninstalled: the thread draws back the way it came.
        let reveal = if self.gone_at >= 0.0 {
            reveal * (1.0 - smooth((t - self.gone_at - 0.3) / 1.2))
        } else {
            reveal
        };

        // What's under the thread, unrolling down from it whenever the page changes.
        let (title, detail) = self.words();
        let cx = W / 2.0;
        c.text_centered(
            &title,
            cx,
            TITLE_Y,
            W - 2.0 * SIDE,
            size::COMPOSER,
            1.3,
            Face::Medium,
            p.ink,
            1.0,
        );
        let mut y = TITLE_Y + size::COMPOSER * 1.3 + space::XL;
        if !detail.is_empty() {
            let dh = c.text_centered(
                &detail,
                cx,
                TITLE_Y + 26.0,
                W - 2.0 * SIDE - 24.0,
                size::BODY,
                line::BODY,
                Face::Regular,
                p.ink_2,
                1.0,
            );
            y = TITLE_Y + 26.0 + dh + space::L;
        }
        if self.page == Page::AskUninstall {
            y = self.toggle(c, &mut hits, y, hovers);
        }
        // Buttons as the app draws them (app.rs, button_owned): the primary in the
        // button colour, which fades towards the background under the pointer; the
        // other an outline that fills with the hover colour. Pressed, a shade further.
        let buttons = self.buttons();
        if !buttons.is_empty() {
            let bh = control::BUTTON;
            let widths: Vec<f32> = buttons
                .iter()
                .map(|(_, l)| {
                    (c.text_width(l, size::SMALL, Face::Medium) + 2.0 * space::L).max(96.0)
                })
                .collect();
            let total: f32 = widths.iter().sum::<f32>() + space::S * (buttons.len() as f32 - 1.0);
            let mut bx = cx - total / 2.0;
            for ((id, label), w) in buttons.iter().zip(widths) {
                let hv = hovers.get(*id).copied().unwrap_or(0.0);
                let down = if pressed == Some(*id) { 1.0 } else { 0.0 };
                let ink = if *id == PRIMARY {
                    c.round_rect(
                        bx,
                        y,
                        w,
                        bh,
                        radius::BUTTON,
                        mix(p.button, p.bg, 0.18 * hv + 0.1 * down),
                        1.0,
                    );
                    p.button_ink
                } else {
                    c.round_rect(bx, y, w, bh, radius::BUTTON, p.line, 1.0);
                    c.round_rect(
                        bx + 1.0,
                        y + 1.0,
                        w - 2.0,
                        bh - 2.0,
                        radius::BUTTON - 1.0,
                        mix(p.bg, mix(p.hover, p.active, down), hv.max(down)),
                        1.0,
                    );
                    p.ink
                };
                let tw = c.text_width(label, size::SMALL, Face::Medium);
                c.text(
                    label,
                    bx + (w - tw) / 2.0,
                    y + (bh - size::SMALL * 1.25) / 2.0,
                    size::SMALL,
                    Face::Medium,
                    ink,
                    1.0,
                );
                hits.push(Hit {
                    x: bx,
                    y,
                    w,
                    h: bh,
                    id: *id,
                });
                bx += w + space::S;
            }
        }
        let below = THREAD_Y + THREAD_ROOM + 4.0;
        let unroll = ease_out(((t - self.page_at) / 0.45).clamp(0.0, 1.0));
        let covered = below + (H - below) * unroll;
        if covered < H {
            c.rect(0.0, covered, W, H - covered, p.bg, 1.0);
        }

        // The halves that open from the thread's line at the start.
        if open < 1.0 {
            c.rect(0.0, 0.0, W, THREAD_Y * (1.0 - open), p.bg, 1.0);
            let lower = H - THREAD_Y;
            c.rect(
                0.0,
                H - lower * (1.0 - open),
                W,
                lower * (1.0 - open),
                p.bg,
                1.0,
            );
        }

        // The mark, settling onto the thread.
        c.wordmark(
            cx,
            MARK_Y + 6.0 * (1.0 - mark_in),
            30.0,
            p.bg,
            p.ink,
            mark_in,
        );
        self.thread.draw(
            c,
            SIDE,
            THREAD_Y,
            W - 2.0 * SIDE,
            THREAD_ROOM,
            t,
            reveal,
            thread_stroke::LARGE,
            p.line,
        );

        // Close, unless something is being done.
        if self.page != Page::Working {
            let s = control::ICON;
            let (x, y) = (W - space::M - s, space::M);
            let hv = hovers.get(CLOSE).copied().unwrap_or(0.0);
            if hv > 0.0 {
                c.round_rect(x, y, s, s, radius::ROW, p.hover, hv);
            }
            let ink = mix(p.ink_3, p.ink, hv);
            let (m, k) = (x + s / 2.0, 4.5);
            c.line(
                &[(m - k, y + s / 2.0 - k), (m + k, y + s / 2.0 + k)],
                1.4,
                ink,
                open,
            );
            c.line(
                &[(m + k, y + s / 2.0 - k), (m - k, y + s / 2.0 + k)],
                1.4,
                ink,
                open,
            );
            hits.push(Hit {
                x,
                y,
                w: s,
                h: s,
                id: CLOSE,
            });
        }
        hits
    }
}

impl App {
    /// The switch for deleting the person's data too, the app's own switch (app.rs,
    /// `switch`): 38 by 22, a square-cornered knob that slides across in 0.16 s while
    /// the track fades from the strong line colour to the button colour. Gives where
    /// the next thing goes.
    fn toggle(&self, c: &mut Canvas, hits: &mut Vec<Hit>, y: f32, hovers: &[f32]) -> f32 {
        let p = self.pal;
        let label = "Delete agents and chats on this computer";
        let (sw, sh) = (38.0, 22.0);
        let lw = c.text_width(label, size::SMALL, Face::Regular);
        let total = sw + space::M + lw;
        let x = W / 2.0 - total / 2.0;
        let k = self.switch_level();
        let hv = hovers.get(TOGGLE).copied().unwrap_or(0.0);
        let off = mix(p.line, mix(p.line, p.ink_3, 0.35), hv);
        c.round_rect(x, y, sw, sh, radius::ROW, mix(off, p.button, k), 1.0);
        let kx = x + 3.0 + 16.0 * k;
        // The knob's small shadow, then the knob.
        c.round_rect(kx, y + 3.5, 16.0, 16.0, 4.0, 0x000000, 0.18);
        c.round_rect(
            kx,
            y + 3.0,
            16.0,
            16.0,
            4.0,
            mix(p.bg, p.button_ink, k),
            1.0,
        );
        c.text(
            label,
            x + sw + space::M,
            y + (sh - size::SMALL * 1.25) / 2.0,
            size::SMALL,
            Face::Regular,
            p.ink,
            1.0,
        );
        hits.push(Hit {
            x,
            y: y - 4.0,
            w: total,
            h: sh + 8.0,
            id: TOGGLE,
        });
        y + sh + space::L
    }

    /// How far the switch is from off to on, easing as the app's motion does: fast at
    /// first, gentle at the end, over 0.16 s.
    fn switch_level(&self) -> f32 {
        let to = if self.delete_data { 1.0 } else { 0.0 };
        if self.toggled_at < 0.0 {
            return to;
        }
        let k = ease_out((self.now - self.toggled_at) / 0.16);
        (1.0 - to) + (to - (1.0 - to)) * k
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_as_the_release_script_does() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("v0.2.1", "0.2.0-dev"));
        assert!(is_newer("0.2.0", "0.2.0-dev"));
        assert!(!is_newer("0.2.0-dev", "0.2.0"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.0"));
    }
}
