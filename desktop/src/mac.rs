//! What loom reaches in macOS that GPUI doesn't give it: hiding the window to the menu
//! bar and bringing it back, where the window was, keeping the Mac awake while an agent
//! works, the alert sound under a notification, and where the app's bundle is.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSScreen, NSView, NSWindow};
use objc2_foundation::{
    NSActivityOptions, NSObjectProtocol, NSPoint, NSProcessInfo, NSRect, NSSize, NSString,
};

use crate::place::{Place, Rect};

/// The window that holds the view `view`, the pointer GPUI's window handle gives.
fn window_of(view: isize) -> Option<Retained<NSWindow>> {
    if view == 0 {
        return None;
    }
    // SAFETY: `view` is the NSView of loom's one window, which lives as long as the app,
    // and this runs on the main thread, where AppKit's views are used.
    let view: &NSView = unsafe { &*(view as *const NSView) };
    view.window()
}

/// Shows the window and brings loom forward, out of the menu bar or another Space, or
/// hides it, leaving loom running in the menu bar.
pub fn show_window(view: isize, show: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(window) = window_of(view) else {
        return;
    };
    if show {
        let app = NSApplication::sharedApplication(mtm);
        app.unhide(None);
        if window.isMiniaturized() {
            window.deminiaturize(None);
        }
        window.makeKeyAndOrderFront(None);
        if ZOOM_WHEN_SHOWN.swap(false, Ordering::Relaxed) && !window.isZoomed() {
            window.zoom(None);
        }
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    } else {
        window.orderOut(None);
    }
}

/// Whether the window, put in its place while hidden (started in the menu bar), is
/// zoomed the first time it's shown.
static ZOOM_WHEN_SHOWN: AtomicBool = AtomicBool::new(false);

/// The window's place while it isn't zoomed, which a zoomed window's frame doesn't say.
static NORMAL: Mutex<Option<Rect>> = Mutex::new(None);

/// The height of the main screen, the one with the menu bar, which AppKit's frames
/// count up from the bottom of.
fn main_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map(|s| s.frame().size.height)
        .unwrap_or(0.0)
}

/// An AppKit frame as a place's rectangle, in points from the main screen's top left.
fn to_rect(f: NSRect, main: f64) -> Rect {
    let left = f.origin.x.round() as i32;
    let top = (main - (f.origin.y + f.size.height)).round() as i32;
    Rect {
        left,
        top,
        right: left + f.size.width.round() as i32,
        bottom: top + f.size.height.round() as i32,
    }
}

fn to_frame(r: Rect, main: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(f64::from(r.left), main - f64::from(r.bottom)),
        NSSize::new(f64::from(r.width()), f64::from(r.height())),
    )
}

/// The screens' visible areas, the main one first, as a place's rectangles.
fn screens(mtm: MainThreadMarker) -> Vec<Rect> {
    let main = main_height(mtm);
    NSScreen::screens(mtm)
        .iter()
        .map(|s| to_rect(s.visibleFrame(), main))
        .collect()
}

/// Where the window is now: its frame, or while it's zoomed, the frame it had before.
pub fn window_place(view: isize) -> Option<Place> {
    let mtm = MainThreadMarker::new()?;
    let window = window_of(view)?;
    let now = to_rect(window.frame(), main_height(mtm));
    let zoomed =
        window.isZoomed() || (!window.isVisible() && ZOOM_WHEN_SHOWN.load(Ordering::Relaxed));
    let mut normal = NORMAL.lock().ok()?;
    if !zoomed {
        *normal = Some(now);
    }
    Some(Place {
        normal: normal.unwrap_or(now),
        maximized: zoomed,
    })
}

/// Puts the window where it was, kept on the screens there are now, zoomed if it was:
/// at once when it's shown, or when it's first shown, for one started in the menu bar.
pub fn put_window(view: isize, place: Place, shown: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(window) = window_of(view) else {
        return;
    };
    let place = place.fit(&screens(mtm));
    window.setFrame_display(to_frame(place.normal, main_height(mtm)), shown);
    if let Ok(mut normal) = NORMAL.lock() {
        *normal = Some(place.normal);
    }
    if place.maximized && shown && !window.isZoomed() {
        window.zoom(None);
    }
    ZOOM_WHEN_SHOWN.store(place.maximized && !shown, Ordering::Relaxed);
}

/// Whether the window is on screen: shown, and loom not hidden.
pub fn window_shown(view: isize) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return true;
    };
    let Some(window) = window_of(view) else {
        return true;
    };
    window.isVisible() && !NSApplication::sharedApplication(mtm).isHidden()
}

/// The activity that keeps the Mac from sleeping while an agent works, while it's held.
struct Awake(Retained<ProtocolObject<dyn NSObjectProtocol>>);

// SAFETY: the token is only handed back to NSProcessInfo, which may be called from any
// thread, and it's kept behind a mutex.
unsafe impl Send for Awake {}

static AWAKE: std::sync::Mutex<Option<Awake>> = std::sync::Mutex::new(None);

/// Keeps the Mac from going to sleep on its own while `on`, as the system's own
/// `caffeinate -i` does: the screen may still sleep.
pub fn keep_awake(on: bool) {
    let Ok(mut held) = AWAKE.lock() else { return };
    let info = NSProcessInfo::processInfo();
    if on && held.is_none() {
        let reason = NSString::from_str("An agent is working");
        let token = info
            .beginActivityWithOptions_reason(NSActivityOptions::IdleSystemSleepDisabled, &reason);
        *held = Some(Awake(token));
    } else if !on {
        if let Some(Awake(token)) = held.take() {
            // SAFETY: the token beginActivity gave, ended once.
            unsafe { info.endActivity(&token) };
        }
    }
}

/// What's between the markers in the login shell's answer.
fn between_markers(out: &str) -> Option<&str> {
    const MARK: &str = "<<loom-path>>";
    let start = out.find(MARK)? + MARK.len();
    let end = out[start..].find(MARK)? + start;
    Some(&out[start..end]).filter(|p| !p.is_empty())
}

/// `login`'s folders first, then those of `now` it doesn't have.
fn merged_path(login: &str, now: &str) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for dir in login.split(':').chain(now.split(':')) {
        if !dir.is_empty() && !seen.contains(&dir) {
            seen.push(dir);
        }
    }
    seen.join(":")
}

/// Takes the PATH the person's login shell has. An app opened from the Finder or the Dock
/// gets launchd's, only /usr/bin:/bin:/usr/sbin:/sbin, so the instance in it wouldn't find
/// agent programs installed with Homebrew or npm, nor `npx` to run them. Started from a
/// terminal, it has the shell's already. Called first thing, before any thread starts.
pub fn take_login_path() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| s.starts_with('/'))
        .unwrap_or_else(|| "/bin/zsh".into());
    let Ok(mut child) = Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            r#"printf '\n<<loom-path>>%s<<loom-path>>\n' "$PATH""#,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    // A shell that waits for something is left behind after a few seconds.
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
    }
    if let Some(login) = between_markers(&out) {
        let now = std::env::var("PATH").unwrap_or_default();
        // First thing in main, before any other thread reads the environment.
        std::env::set_var("PATH", merged_path(login, &now));
    }
}

/// The person's alert sound, for a notification that has one.
pub fn beep() {
    objc2_app_kit::NSBeep();
}

/// The `.app` bundle the program at `exe` is in: `…/loom.app` for
/// `…/loom.app/Contents/MacOS/loom`. None for a program outside one, as a build from
/// source is.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let named = |p: &Path, n: &str| p.file_name().is_some_and(|f| f == n);
    (named(macos, "MacOS")
        && named(contents, "Contents")
        && bundle.extension().is_some_and(|e| e == "app"))
    .then(|| bundle.to_path_buf())
}

/// This program's bundle.
pub fn bundle() -> Option<PathBuf> {
    bundle_of(&std::env::current_exe().ok()?)
}

/// Why loom can't update itself where it runs, or none when it can: its bundle must be
/// one it can swap, so not run from the disk image, from where macOS runs a downloaded
/// app it hasn't moved yet (App Translocation), or from a folder it can't write to.
pub fn cant_update(bundle: &Path) -> Option<&'static str> {
    let path = bundle.to_string_lossy();
    if path.starts_with("/Volumes/") {
        return Some("loom is running from its disk image. Drag it to Applications, open it from there, and it updates itself.");
    }
    if path.contains("/AppTranslocation/") {
        return Some("macOS is running loom from a copy of its own. Move loom to Applications, open it from there, and it updates itself.");
    }
    let parent = bundle.parent()?;
    let dir = std::ffi::CString::new(parent.to_string_lossy().as_bytes()).ok()?;
    // SAFETY: a NUL-terminated path; access only reads.
    if unsafe { libc::access(dir.as_ptr(), libc::W_OK) } != 0 {
        return Some("loom can't change the folder it's in. Move it to Applications, or ask whoever runs this Mac to update it.");
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn finds_the_bundle_a_program_is_in() {
        assert_eq!(
            super::bundle_of(Path::new("/Applications/loom.app/Contents/MacOS/loom")).as_deref(),
            Some(Path::new("/Applications/loom.app"))
        );
        assert_eq!(
            super::bundle_of(Path::new("/Users/e/loom/desktop/target/release/loom")),
            None
        );
        assert_eq!(super::bundle_of(Path::new("/loom")), None);
    }

    #[test]
    fn takes_the_login_shell_s_path_first() {
        let said = "Last login: today\n\n<<loom-path>>/opt/homebrew/bin:/usr/bin<<loom-path>>\n";
        assert_eq!(
            super::between_markers(said),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(super::between_markers("nothing"), None);
        assert_eq!(
            super::merged_path(
                "/opt/homebrew/bin:/usr/bin",
                "/usr/bin:/bin:/usr/sbin:/sbin"
            ),
            "/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        );
    }

    #[test]
    fn says_why_it_cant_update_from_the_disk_image() {
        let why = super::cant_update(Path::new("/Volumes/loom/loom.app"));
        assert!(why.is_some_and(|w| w.contains("disk image")));
        let why = super::cant_update(Path::new(
            "/private/var/folders/x/AppTranslocation/1234/d/loom.app",
        ));
        assert!(why.is_some_and(|w| w.contains("Applications")));
        let temp = std::env::temp_dir().join("loom.app");
        assert_eq!(super::cant_update(&temp), None);
    }
}
