//! What the app reaches in the system: the tray, notifications, starting at login,
//! and keeping the window in the tray when it's closed (PLAN.md, "An instance in the
//! desktop app": "It keeps running").

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// What the person did in the tray.
pub enum TrayAction {
    Open,
    Quit,
    StartAtLogin(bool),
    CheckUpdates,
}

pub struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    quit: MenuItem,
    login: CheckMenuItem,
    updates: MenuItem,
}

fn icon() -> Option<tray_icon::Icon> {
    // The mark in light ink, for the taskbar's dark default.
    let bytes: &[u8] = include_bytes!("../../design/logo/png/dark-32.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba {
        return None;
    }
    tray_icon::Icon::from_rgba(buf[..info.buffer_size()].to_vec(), info.width, info.height).ok()
}

impl Tray {
    pub fn new(start_at_login: bool) -> Option<Tray> {
        let open = MenuItem::new("Open loom", true, None);
        let login = CheckMenuItem::new("Start at login", true, start_at_login, None);
        let updates = MenuItem::new("Check for updates", true, None);
        let quit = MenuItem::new("Quit loom", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &open,
            &PredefinedMenuItem::separator(),
            &login,
            &updates,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .ok()?;
        let mut b = TrayIconBuilder::new()
            .with_tooltip("loom")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false);
        if let Some(i) = icon() {
            b = b.with_icon(i);
        }
        let icon = b.build().ok()?;
        Some(Tray {
            _icon: icon,
            open,
            quit,
            login,
            updates,
        })
    }

    /// Ticks "Start at login" as it is, after Settings changed it.
    pub fn set_login(&self, on: bool) {
        self.login.set_checked(on);
    }

    /// What happened in the tray since the last look.
    pub fn actions(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if e.id == *self.open.id() {
                out.push(TrayAction::Open);
            } else if e.id == *self.quit.id() {
                out.push(TrayAction::Quit);
            } else if e.id == *self.login.id() {
                out.push(TrayAction::StartAtLogin(self.login.is_checked()));
            } else if e.id == *self.updates.id() {
                out.push(TrayAction::CheckUpdates);
            }
        }
        while let Ok(e) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = e
            {
                out.push(TrayAction::Open);
            }
        }
        out
    }
}

/// Starts the system's own dictation, which types what's said into the field that has
/// the focus: Windows voice typing, which Win+H opens. False where there's none to start.
#[cfg(windows)]
pub fn dictate() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LWIN,
    };
    let key = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let h = u16::from(b'H');
    let keys = [
        key(VK_LWIN, false),
        key(h, false),
        key(h, true),
        key(VK_LWIN, true),
    ];
    // SAFETY: `keys` is an array of initialised INPUTs that outlives the call, and its
    // length and element size are passed as SendInput expects.
    let sent = unsafe {
        SendInput(
            keys.len() as u32,
            keys.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    sent == keys.len() as u32
}

#[cfg(not(windows))]
pub fn dictate() -> bool {
    false
}

/// What a notification is about: the server's folder ("" for the one on screen first),
/// the agent and the conversation, which a click on it opens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct About {
    pub server: String,
    pub agent: String,
    pub conversation: String,
}

/// The notifications clicked since the app last looked, in the order they were.
static CLICKED: std::sync::Mutex<Vec<About>> = std::sync::Mutex::new(Vec::new());

/// The notifications clicked since the last call.
pub fn clicked() -> Vec<About> {
    CLICKED
        .lock()
        .map(|mut c| std::mem::take(&mut *c))
        .unwrap_or_default()
}

/// A system notification, with its sound or without. A click on it, while the app
/// runs, opens what it's about.
#[cfg(windows)]
pub fn notify(title: &str, body: &str, about: Option<About>, sound: bool) {
    let toast = tauri_winrt_notification::Toast::new(app_id())
        .title(title)
        .text1(body)
        .sound(sound.then_some(tauri_winrt_notification::Sound::Default))
        .on_activated(move |_| {
            if let (Some(a), Ok(mut c)) = (about.clone(), CLICKED.lock()) {
                c.push(a);
            }
            Ok(())
        });
    if let Err(e) = toast.show() {
        eprintln!("loom: the notification didn't show: {e}");
    }
}

/// A system notification.
#[cfg(not(windows))]
pub fn notify(title: &str, body: &str, _about: Option<About>, sound: bool) {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body);
    if sound {
        n.sound_name("message-new-instant");
    }
    if let Err(e) = n.show() {
        eprintln!("loom: the notification didn't show: {e}");
    }
}

/// The id loom-setup gives loom's Start menu shortcut (desktop/setup), which Windows
/// shows notifications under.
#[cfg(windows)]
const APP_ID: &str = "AaenStudios.Loom";

/// loom's own id once the installer has registered it, and otherwise PowerShell's, which
/// Windows still shows notifications for.
#[cfg(windows)]
fn app_id() -> &'static str {
    let installed = std::env::var("APPDATA").is_ok_and(|d| {
        std::path::Path::new(&d)
            .join(r"Microsoft\Windows\Start Menu\Programs\loom.lnk")
            .exists()
    });
    if installed {
        APP_ID
    } else {
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe"
    }
}

/// Hands the update at `setup`, the new release's loom-setup.exe that the core
/// downloaded and checked against the signed manifest, to that setup, for the app to
/// quit after. The setup waits for this program to end, checks its own signature,
/// swaps loom.exe, starts the new one (in the tray when `background`) and puts the old
/// one back if the new one doesn't start (desktop/setup).
pub fn apply_update(setup: &std::path::Path, background: bool) -> std::io::Result<()> {
    let mut started = std::process::Command::new(setup);
    started
        .arg("--update")
        .arg("--pid")
        .arg(std::process::id().to_string());
    if background {
        started.arg("--background");
    }
    started.spawn()?;
    Ok(())
}

/// Whether closing the window keeps loom in the tray, rather than quitting it.
static KEEP_IN_TRAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn keep_in_tray(on: bool) {
    KEEP_IN_TRAY.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn keeps_in_tray() -> bool {
    KEEP_IN_TRAY.load(std::sync::atomic::Ordering::Relaxed)
}

/// Keeps the computer from sleeping, while `on`. Called from the window's thread.
#[cfg(windows)]
pub fn keep_awake(on: bool) {
    use windows_sys::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    // SAFETY: it only sets this thread's execution state.
    unsafe {
        SetThreadExecutionState(if on {
            ES_CONTINUOUS | ES_SYSTEM_REQUIRED
        } else {
            ES_CONTINUOUS
        });
    }
}

#[cfg(not(windows))]
pub fn keep_awake(_on: bool) {}

/// Whether the window is on screen, rather than hidden in the tray.
#[cfg(windows)]
pub fn window_shown(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    // SAFETY: IsWindowVisible only reads the window's state, and takes any handle.
    unsafe { IsWindowVisible(hwnd as _) != 0 }
}

#[cfg(not(windows))]
pub fn window_shown(_hwnd: isize) -> bool {
    true
}

fn launcher() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name("loom")
        .set_app_path(&exe.to_string_lossy())
        .set_args(&["--background"])
        .build()
        .ok()
}

/// Whether loom starts when the person logs in.
pub fn starts_at_login() -> bool {
    launcher()
        .and_then(|l| l.is_enabled().ok())
        .unwrap_or(false)
}

pub fn set_start_at_login(on: bool) {
    let Some(l) = launcher() else { return };
    let done = if on { l.enable() } else { l.disable() };
    if let Err(e) = done {
        eprintln!("loom: starting at login couldn't be changed: {e}");
    }
}

/// The first start turns starting at login on, as the plan has it; after that it's the
/// person's choice, kept by the system.
pub fn first_start(data: &str) {
    let marker = std::path::Path::new(data).join("started");
    if !marker.exists() {
        set_start_at_login(true);
        let _ = std::fs::write(marker, b"");
    }
}

/// Hides and shows the window, which GPUI can't do itself on Windows.
#[cfg(windows)]
pub fn show_window(hwnd: isize, show: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_HIDE, SW_SHOW,
    };
    // SAFETY: hwnd is the app's own window, which lives as long as the app.
    unsafe {
        ShowWindow(hwnd as _, if show { SW_SHOW } else { SW_HIDE });
        if show {
            SetForegroundWindow(hwnd as _);
        }
    }
}

#[cfg(not(windows))]
pub fn show_window(_hwnd: isize, _show: bool) {}

/// The window's native handle, for `show_window`.
pub fn hwnd(window: &gpui_kit::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        #[cfg(windows)]
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}

/// An image the clipboard gave in another kind, such as a bitmap, as a PNG.
pub fn to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

/// The longest side a screenshot is sent at: what models read a picture at best.
const SHOT_SIDE: u32 = 1568;

/// A picture of the main screen, for an agent's `screenshot` device tool: a JPEG,
/// scaled down to `SHOT_SIDE` at most, with the size it was taken at.
pub fn screenshot() -> Result<(Vec<u8>, u32, u32), String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let main = monitors
        .iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or("This computer has no screen to take.")?;
    let shot = main.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = shot.dimensions();
    let longest = w.max(h).max(1);
    let shot = if longest > SHOT_SIDE {
        let scale = f64::from(SHOT_SIDE) / f64::from(longest);
        let (sw, sh) = (
            (f64::from(w) * scale).round() as u32,
            (f64::from(h) * scale).round() as u32,
        );
        image::imageops::resize(&shot, sw, sh, image::imageops::FilterType::Triangle)
    } else {
        shot
    };
    let rgb = image::DynamicImage::ImageRgba8(shot).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok((out, w, h))
}

#[cfg(test)]
mod tests {
    // Needs a screen, so CI leaves it out: `cargo test -- --ignored` runs it.
    #[test]
    #[ignore]
    fn takes_the_main_screen() {
        let (jpeg, w, h) = super::screenshot().expect("a screenshot");
        assert!(w > 0 && h > 0);
        assert_eq!(&jpeg[..3], &[0xFF, 0xD8, 0xFF]);
        let shot = image::load_from_memory(&jpeg).expect("a JPEG");
        assert!(shot.width().max(shot.height()) <= super::SHOT_SIDE);
    }
}
