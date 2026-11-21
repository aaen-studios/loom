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

/// A system notification.
pub fn notify(title: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body);
    #[cfg(windows)]
    n.app_id(app_id());
    if let Err(e) = n.show() {
        eprintln!("loom: the notification didn't show: {e}");
    }
}

/// The id the installer gives loom's Start menu shortcut (installer.iss), which Windows
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

/// The program as it was before an update, kept beside it until the next start.
fn old_exe() -> Option<std::path::PathBuf> {
    Some(std::env::current_exe().ok()?.with_extension("old.exe"))
}

/// Puts the update at `new` in this program's place and starts it, for the app to quit
/// after. The old program is kept as loom.old.exe, and put back if the swap fails.
pub fn apply_update(new: &std::path::Path) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let old = old_exe().unwrap_or_else(|| exe.with_extension("old.exe"));
    let _ = std::fs::remove_file(&old);
    // Windows lets a running program be renamed, not replaced.
    std::fs::rename(&exe, &old)?;
    if let Err(e) = std::fs::copy(new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e);
    }
    std::process::Command::new(&exe)
        .arg("--updated")
        .arg(std::process::id().to_string())
        .spawn()?;
    Ok(())
}

/// After an update: waits for the old program to end, so two never run at once, and
/// removes it.
pub fn after_update() {
    let mut args = std::env::args().skip_while(|a| a != "--updated").skip(1);
    if let Some(pid) = args.next().and_then(|p| p.parse::<u32>().ok()) {
        wait_for_exit(pid);
    }
    if let Some(old) = old_exe() {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(windows)]
fn wait_for_exit(pid: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    // SAFETY: the handle is checked, waited on for at most 15 s, and closed.
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, 15000);
            CloseHandle(h);
        }
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32) {
    std::thread::sleep(std::time::Duration::from_secs(2));
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
