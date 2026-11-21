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
}

pub struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    quit: MenuItem,
    login: CheckMenuItem,
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
        let quit = MenuItem::new("Quit loom", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &open,
            &PredefinedMenuItem::separator(),
            &login,
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
        })
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

/// A system notification.
pub fn notify(title: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body);
    #[cfg(windows)]
    n.app_id(APP_ID);
    if let Err(e) = n.show() {
        eprintln!("loom: the notification didn't show: {e}");
    }
}

/// The id Windows shows notifications under. Without an installer that registers one,
/// Windows falls back to PowerShell's, which still shows them.
#[cfg(windows)]
const APP_ID: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";

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
