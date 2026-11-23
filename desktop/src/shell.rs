//! What the app reaches in the system: the tray (the menu bar on a Mac), notifications,
//! starting at login, and keeping the window in the tray when it's closed (PLAN.md, "An
//! instance in the desktop app": "It keeps running").

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// What the person did in the tray.
pub enum TrayAction {
    Open,
    Quit,
    StartAtLogin(bool),
    CheckUpdates,
}

static MENU_EVENTS: std::sync::Mutex<Vec<MenuEvent>> = std::sync::Mutex::new(Vec::new());
static TRAY_EVENTS: std::sync::Mutex<Vec<TrayIconEvent>> = std::sync::Mutex::new(Vec::new());

pub struct Tray {
    _icon: TrayIcon,
    open: MenuItem,
    quit: MenuItem,
    login: CheckMenuItem,
    updates: MenuItem,
}

fn icon() -> Option<tray_icon::Icon> {
    // The mark in light ink, for the taskbar's dark default. On a Mac it's a template
    // image, which the menu bar draws in its own ink, so only its shape counts.
    #[cfg(not(target_os = "macos"))]
    let bytes: &[u8] = include_bytes!("../../design/logo/png/dark-32.png");
    #[cfg(target_os = "macos")]
    let bytes: &[u8] = include_bytes!("../../design/logo/png/dark-64.png");
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
        // On Windows a click opens the window and the menu is the right button's; a Mac's
        // menu bar opens the menu on a click.
        let mut b = TrayIconBuilder::new()
            .with_tooltip("loom")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(cfg!(target_os = "macos"));
        if let Some(i) = icon() {
            #[cfg(target_os = "macos")]
            let with = b.with_icon_templated(i);
            #[cfg(not(target_os = "macos"))]
            let with = b.with_icon(i);
            b = with;
        }
        let icon = b.build().ok()?;
        // What happens in the tray waits here, and wakes the window's task to look.
        MenuEvent::set_event_handler(Some(|e: MenuEvent| {
            if let Ok(mut m) = MENU_EVENTS.lock() {
                m.push(e);
            }
            crate::wake::wake();
        }));
        TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
            if let TrayIconEvent::Click { .. } = e {
                if let Ok(mut t) = TRAY_EVENTS.lock() {
                    t.push(e);
                }
                crate::wake::wake();
            }
        }));
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
        let menu = MENU_EVENTS
            .lock()
            .map(|mut m| std::mem::take(&mut *m))
            .unwrap_or_default();
        let tray = TRAY_EVENTS
            .lock()
            .map(|mut t| std::mem::take(&mut *t))
            .unwrap_or_default();
        for e in menu {
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
        for e in tray {
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
            crate::wake::wake();
            Ok(())
        });
    if let Err(e) = toast.show() {
        eprintln!("loom: the notification didn't show: {e}");
    }
}

/// The notifications waiting for the window's task to post, which on a Mac goes through
/// GPUI's own (UNUserNotificationCenter), on the main thread.
#[cfg(target_os = "macos")]
static TO_POST: std::sync::Mutex<Vec<Note>> = std::sync::Mutex::new(Vec::new());

/// A notification waiting: its title, its body, what it's about and whether it sounds.
#[cfg(target_os = "macos")]
type Note = (String, String, Option<About>, bool);

/// A system notification, with the person's alert sound or without. A click on it opens
/// what it's about. It goes out from the window's task (`post_notifications`).
#[cfg(target_os = "macos")]
pub fn notify(title: &str, body: &str, about: Option<About>, sound: bool) {
    if let Ok(mut n) = TO_POST.lock() {
        n.push((title.to_string(), body.to_string(), about, sound));
    }
    crate::wake::wake();
}

/// A notification's tag, which comes back when it's clicked: a count, so a newer one
/// doesn't replace it, and what it's about.
#[cfg(any(target_os = "macos", test))]
fn tag_of(n: u64, about: &About) -> String {
    format!(
        "{n}\u{1f}{}\u{1f}{}\u{1f}{}",
        about.server, about.agent, about.conversation
    )
}

/// What a clicked notification's tag says it's about.
#[cfg(any(target_os = "macos", test))]
fn about_of(tag: &str) -> Option<About> {
    let mut parts = tag.split('\u{1f}').skip(1);
    Some(About {
        server: parts.next()?.to_string(),
        agent: parts.next()?.to_string(),
        conversation: parts.next()?.to_string(),
    })
}

/// Posts the notifications waiting, from the window's task.
#[cfg(target_os = "macos")]
pub fn post_notifications(cx: &gpui_kit::App) {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let waiting = TO_POST
        .lock()
        .map(|mut n| std::mem::take(&mut *n))
        .unwrap_or_default();
    for (title, body, about, sound) in waiting {
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        cx.show_system_notification(gpui_kit::SystemNotification {
            tag: tag_of(n, &about.unwrap_or_default()).into(),
            title: title.into(),
            body: body.into(),
            actions: Vec::new(),
        });
        if sound {
            crate::mac::beep();
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn post_notifications(_cx: &gpui_kit::App) {}

/// Hears each click on loom's notifications, for `clicked`.
#[cfg(target_os = "macos")]
pub fn hear_notifications(cx: &gpui_kit::App) {
    cx.on_system_notification_response(|r, _| {
        if let (Some(a), Ok(mut c)) = (about_of(&r.tag), CLICKED.lock()) {
            c.push(a);
        }
        crate::wake::wake();
    });
}

#[cfg(not(target_os = "macos"))]
pub fn hear_notifications(_cx: &gpui_kit::App) {}

/// A system notification.
#[cfg(all(not(windows), not(target_os = "macos")))]
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

/// The app icon in loom's theme: loom.exe carries the light one first and the dark
/// one second (build.rs). The window's and the taskbar's icon change at once; loom's
/// Start menu and desktop shortcuts are pointed at the matching one, which Explorer
/// shows from then on, on a thread of their own.
#[cfg(windows)]
pub fn follow_theme(hwnd: isize, dark: bool) {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let id: usize = if dark { 2 } else { 1 };
    // SAFETY: icons from this program's own resources, shared so they're never freed,
    // given to its own window.
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        for (which, side) in [
            (ICON_BIG, GetSystemMetrics(SM_CXICON)),
            (ICON_SMALL, GetSystemMetrics(SM_CXSMICON)),
        ] {
            let icon = LoadImageW(inst, id as _, IMAGE_ICON, side, side, LR_SHARED);
            if !icon.is_null() && hwnd != 0 {
                SendMessageW(hwnd as _, WM_SETICON, which as usize, icon as isize);
            }
        }
    }
    std::thread::spawn(move || {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut links = Vec::new();
        if let Ok(d) = std::env::var("APPDATA") {
            links.push(
                std::path::Path::new(&d).join(r"Microsoft\Windows\Start Menu\Programs\loom.lnk"),
            );
        }
        if let Ok(d) = std::env::var("USERPROFILE") {
            links.push(std::path::Path::new(&d).join(r"Desktop\Loom.lnk"));
            links.push(std::path::Path::new(&d).join(r"Desktop\loom.lnk"));
        }
        for lnk in links {
            if lnk.exists() {
                if let Err(e) = point_icon(&lnk, &exe, if dark { 1 } else { 0 }) {
                    eprintln!("loom: {} keeps its icon: {e}", lnk.display());
                }
            }
        }
    });
}

#[cfg(not(windows))]
pub fn follow_theme(_hwnd: isize, _dark: bool) {}

/// Points the shortcut at `lnk`, if it opens `exe`, at `exe`'s icon number `index`,
/// keeping everything else it has, such as its AppUserModelID.
#[cfg(windows)]
fn point_icon(
    lnk: &std::path::Path,
    exe: &std::path::Path,
    index: i32,
) -> windows::core::Result<()> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Shell::{
        IShellLinkW, SHChangeNotify, ShellLink, SHCNE_UPDATEITEM, SHCNF_PATHW,
    };
    // The same file, however each path is spelt: a shortcut keeps the long path, and a
    // program's own can have short 8.3 names in it, as C:\Users\RUNNER~1 does.
    let same = |a: &[u16], b: &std::path::Path| {
        let end = a.iter().position(|&c| c == 0).unwrap_or(a.len());
        let a = std::path::PathBuf::from(String::from_utf16_lossy(&a[..end]));
        match (std::fs::canonicalize(&a), std::fs::canonicalize(b)) {
            (Ok(x), Ok(y)) => x == y,
            _ => a
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.to_string_lossy()),
        }
    };
    // SAFETY: COM on this thread, for a shell link that's loaded, read, changed only
    // if it's loom's and its icon differs, saved and released.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let file: IPersistFile = link.cast()?;
        let path = HSTRING::from(lnk.as_os_str());
        file.Load(&path, STGM_READWRITE)?;
        let mut target = [0u16; 1024];
        link.GetPath(&mut target, std::ptr::null_mut(), 0)?;
        if !same(&target, exe) {
            return Ok(());
        }
        let mut icon = [0u16; 1024];
        let mut now = -1;
        link.GetIconLocation(&mut icon, &mut now)?;
        if now == index && same(&icon, exe) {
            return Ok(());
        }
        link.SetIconLocation(&HSTRING::from(exe.as_os_str()), index)?;
        file.Save(&path, true)?;
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = lnk.as_os_str().encode_wide().chain(Some(0)).collect();
        SHChangeNotify(
            SHCNE_UPDATEITEM,
            SHCNF_PATHW,
            Some(wide.as_ptr() as _),
            None,
        );
    }
    Ok(())
}

/// Hands the update at `setup`, the new release's loom-setup.exe that the core
/// downloaded and checked against the signed manifest, to that setup, for the app to
/// quit after. The setup waits for this program to end, checks its own signature,
/// swaps loom.exe, starts the new one (in the tray when `background`) and puts the old
/// one back if the new one doesn't start (desktop/setup).
#[cfg(not(target_os = "macos"))]
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

/// Hands the update at `dmg`, the new release's disk image that the core downloaded and
/// checked against the signed manifest, to the new loom in it, for the app to quit
/// after: the new loom puts itself in this one's place, starts, and puts this one back
/// if it doesn't (swap.rs).
#[cfg(target_os = "macos")]
pub fn apply_update(dmg: &std::path::Path, background: bool) -> std::io::Result<()> {
    let bundle = crate::mac::bundle()
        .ok_or_else(|| std::io::Error::other("this loom isn't in an app to swap"))?;
    if let Some(why) = crate::mac::cant_update(&bundle) {
        return Err(std::io::Error::other(why));
    }
    let staged = crate::swap::stage(dmg)?;
    crate::swap::hand_over(&staged, &bundle, background)
}

/// Why this app can't update itself where it runs, for the update icon to say, or none
/// when it can. Only a Mac's app can be somewhere it can't, such as its disk image.
pub fn cant_update() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    if let Some(bundle) = crate::mac::bundle() {
        return crate::mac::cant_update(&bundle);
    }
    None
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

#[cfg(target_os = "macos")]
pub fn keep_awake(on: bool) {
    crate::mac::keep_awake(on);
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn keep_awake(_on: bool) {}

/// Whether the window is on screen, rather than hidden in the tray.
#[cfg(windows)]
pub fn window_shown(hwnd: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    // SAFETY: IsWindowVisible only reads the window's state, and takes any handle.
    unsafe { IsWindowVisible(hwnd as _) != 0 }
}

#[cfg(target_os = "macos")]
pub fn window_shown(hwnd: isize) -> bool {
    crate::mac::window_shown(hwnd)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn window_shown(_hwnd: isize) -> bool {
    true
}

/// The name of loom's entry in HKCU's Run key, which the setup removes on uninstall.
#[cfg(not(target_os = "macos"))]
const LOGIN_NAME: &str = "loom";

/// The label of loom's LaunchAgent on a Mac, `~/Library/LaunchAgents/<it>.plist`: the
/// app's bundle identifier, so System Settings shows it under loom in Login Items.
#[cfg(target_os = "macos")]
const LOGIN_NAME: &str = BUNDLE_ID;

/// The Mac app's bundle identifier (scripts/mac.calx writes it into Info.plist).
#[cfg(target_os = "macos")]
pub const BUNDLE_ID: &str = "no.aaenz.loom.desktop";

/// Where the setup installs loom.exe: %LOCALAPPDATA%\Programs\loom (desktop/setup).
#[cfg(not(target_os = "macos"))]
fn installed_exe() -> Option<std::path::PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    std::env::var_os("LOCALAPPDATA")
        .filter(|d| !d.is_empty())
        .map(|d| std::path::Path::new(&d).join(r"Programs\loom\loom.exe"))
}

/// Where a Mac's loom is installed: dragged from its disk image into Applications, or
/// into the person's own ~/Applications. The one this program is, when it's either, and
/// otherwise the first that's there.
#[cfg(target_os = "macos")]
fn installed_exe() -> Option<std::path::PathBuf> {
    let inside = "loom.app/Contents/MacOS/loom";
    let mut places = vec![std::path::Path::new("/Applications").join(inside)];
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        places.push(
            std::path::Path::new(&home)
                .join("Applications")
                .join(inside),
        );
    }
    let exe = std::env::current_exe().ok();
    if let Some(here) = places
        .iter()
        .find(|p| exe.as_deref().is_some_and(|e| same_program(e, p)))
    {
        return Some(here.clone());
    }
    places
        .iter()
        .find(|p| p.is_file())
        .or(places.first())
        .cloned()
}

/// The program "Start at login" starts: the installed loom when there is one, so a dev
/// build ticking it never puts itself in the installed loom's place, and otherwise
/// this one, since the person asked.
fn login_exe() -> Option<std::path::PathBuf> {
    installed_exe()
        .filter(|p| p.is_file())
        .or_else(|| std::env::current_exe().ok())
}

/// The same program, however each path is spelt (case, 8.3 names).
fn same_program(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// Whether this build turns starting at login on by itself, on its first start: only a
/// release (no "-dev" in its version, not a `perf` build), not started for a test (with
/// LOOM_DATA or LOOM_SETUP_SANDBOX set), running from where the setup installs it.
/// Anything else would put itself in the installed loom's place at the next login.
fn turns_itself_on(
    version: &str,
    perf: bool,
    test: bool,
    exe: &std::path::Path,
    installed: Option<&std::path::Path>,
) -> bool {
    !version.contains("-dev") && !perf && !test && installed.is_some_and(|i| same_program(exe, i))
}

/// The program a Run entry starts: what's in its quotes, or up to its ".exe".
#[cfg(any(windows, test))]
fn run_program(entry: &str) -> &str {
    let entry = entry.trim();
    if let Some(rest) = entry.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest);
    }
    match entry.to_ascii_lowercase().find(".exe") {
        Some(i) => &entry[..i + 4],
        None => entry,
    }
}

/// loom's entry in HKCU's Run key, as written there.
#[cfg(windows)]
fn run_entry() -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = wide(LOGIN_NAME);
    let mut len = 0u32;
    // SAFETY: NUL-terminated wide strings; this call only asks for the value's size.
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut len,
        )
    };
    if found != 0 {
        return None;
    }
    let mut buf = vec![0u16; (len as usize).div_ceil(2)];
    // SAFETY: `buf` holds `len` bytes, which the call writes at most.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as _,
            &mut len,
        )
    };
    if read != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

fn launcher() -> Option<auto_launch::AutoLaunch> {
    let exe = login_exe()?;
    // Quoted on Windows, as the setup writes it, for a path with a space in it.
    let path = if cfg!(windows) {
        format!("\"{}\"", exe.display())
    } else {
        exe.to_string_lossy().into_owned()
    };
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(LOGIN_NAME)
        .set_app_path(&path)
        .set_args(&["--background"])
        // Under HKCU, as everything the setup writes, even when run as administrator.
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser)
        // A LaunchAgent in ~/Library/LaunchAgents, which needs no permission to ask for
        // and shows under loom in Login Items.
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        .set_bundle_identifiers(&[LOGIN_NAME])
        .build()
        .ok()
}

/// The program loom's LaunchAgent starts: the first of its ProgramArguments.
#[cfg(any(target_os = "macos", test))]
fn agent_program(plist: &str) -> Option<&str> {
    let args = &plist[plist.find("<key>ProgramArguments</key>")?..];
    let start = args.find("<string>")? + "<string>".len();
    let end = args[start..].find("</string>")? + start;
    Some(&args[start..end])
}

/// Whether loom starts when the person logs in: whether the Run entry (on a Mac, the
/// LaunchAgent) starts the program "Start at login" would put there, so an entry left by
/// another build shows as off, and ticking it points it here.
pub fn starts_at_login() -> bool {
    let enabled = launcher()
        .and_then(|l| l.is_enabled().ok())
        .unwrap_or(false);
    #[cfg(windows)]
    let enabled = enabled
        && match (run_entry(), login_exe()) {
            (Some(entry), Some(exe)) => {
                same_program(std::path::Path::new(run_program(&entry)), &exe)
            }
            _ => false,
        };
    #[cfg(target_os = "macos")]
    let enabled = enabled && {
        let plist = std::env::var_os("HOME")
            .map(|h| {
                std::path::Path::new(&h)
                    .join("Library/LaunchAgents")
                    .join(format!("{LOGIN_NAME}.plist"))
            })
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        match (agent_program(&plist), login_exe()) {
            (Some(program), Some(exe)) => same_program(std::path::Path::new(program), &exe),
            _ => false,
        }
    };
    enabled
}

pub fn set_start_at_login(on: bool) {
    let Some(l) = launcher() else { return };
    let done = if on { l.enable() } else { l.disable() };
    if let Err(e) = done {
        eprintln!("loom: starting at login couldn't be changed: {e}");
    }
}

/// The first start of an installed release turns starting at login on, as the plan has
/// it; after that it's the person's choice, kept by the system. Any other build leaves
/// it alone, and leaves the first start to the installed loom.
pub fn first_start(data: &str) {
    let marker = std::path::Path::new(data).join("started");
    if marker.exists() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let test = ["LOOM_DATA", "LOOM_SETUP_SANDBOX"]
        .iter()
        .any(|v| std::env::var_os(v).is_some());
    let installed = installed_exe();
    if turns_itself_on(
        crate::VERSION,
        cfg!(feature = "perf"),
        test,
        &exe,
        installed.as_deref(),
    ) {
        set_start_at_login(true);
        let _ = std::fs::write(marker, b"");
    }
}

/// Whether the window, put in its place while hidden (started in the tray), opens
/// maximized the first time it's shown.
#[cfg(windows)]
static MAXIMIZE_WHEN_SHOWN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Hides and shows the window, which GPUI can't do itself on Windows. Shown, it comes
/// forward, out of the tray or the taskbar.
#[cfg(windows)]
pub fn show_window(hwnd: isize, show: bool) {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsIconic, SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWMAXIMIZED,
        SW_SHOWNA,
    };
    // A build for measuring shows its window behind what the person is doing (main.rs).
    let measuring = cfg!(feature = "perf");
    // Shown, it no longer waits to be maximized, whichever way it's shown.
    let maximize = show && MAXIMIZE_WHEN_SHOWN.swap(false, Ordering::Relaxed);
    let how = if !show {
        SW_HIDE
    } else if measuring {
        SW_SHOWNA
    } else if maximize {
        SW_SHOWMAXIMIZED
    // SAFETY: IsIconic only reads the window's state.
    } else if unsafe { IsIconic(hwnd as _) } != 0 {
        SW_RESTORE
    } else {
        SW_SHOW
    };
    // SAFETY: hwnd is the app's own window, which lives as long as the app.
    unsafe {
        ShowWindow(hwnd as _, how);
        if show && !measuring {
            SetForegroundWindow(hwnd as _);
        }
    }
}

#[cfg(target_os = "macos")]
pub fn show_window(hwnd: isize, show: bool) {
    crate::mac::show_window(hwnd, show);
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn show_window(_hwnd: isize, _show: bool) {}

/// Where the window is now, as Windows keeps it: its normal rectangle and whether
/// it's maximized, also while it's minimized or hidden in the tray. Hidden since it
/// was put in a maximized place (`put_window`), it counts as maximized, though Windows
/// only maximizes it once it's shown, so a move while it waits doesn't lose that.
#[cfg(windows)]
pub fn window_place(hwnd: isize) -> Option<crate::place::Place> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, IsWindowVisible, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, WINDOWPLACEMENT,
        WPF_RESTORETOMAXIMIZED,
    };
    // SAFETY: a WINDOWPLACEMENT with its length set, for the app's own window.
    let p = unsafe {
        let mut p: WINDOWPLACEMENT = std::mem::zeroed();
        p.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        if GetWindowPlacement(hwnd as _, &mut p) == 0 {
            return None;
        }
        p
    };
    let r = p.rcNormalPosition;
    let show = p.showCmd as i32;
    // SAFETY: IsWindowVisible only reads the window's state.
    let waiting = unsafe { IsWindowVisible(hwnd as _) } == 0
        && MAXIMIZE_WHEN_SHOWN.load(std::sync::atomic::Ordering::Relaxed);
    Some(crate::place::Place {
        normal: crate::place::Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        },
        maximized: waiting
            || show == SW_SHOWMAXIMIZED
            || (show == SW_SHOWMINIMIZED && p.flags & WPF_RESTORETOMAXIMIZED != 0),
    })
}

#[cfg(target_os = "macos")]
pub fn window_place(hwnd: isize) -> Option<crate::place::Place> {
    crate::mac::window_place(hwnd)
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn window_place(_hwnd: isize) -> Option<crate::place::Place> {
    None
}

/// The screens' work areas, the main one first, in the workspace coordinates the
/// window's place is kept in (they start at the main screen's work area).
#[cfg(windows)]
fn screens() -> Vec<crate::place::Rect> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    const MONITORINFOF_PRIMARY: u32 = 1;
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
        // SAFETY: `out` is the Vec this call was given, and `info` has its size set.
        unsafe {
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(m, &mut info) != 0 {
                let list = &mut *(out as *mut Vec<(bool, RECT)>);
                list.push((info.dwFlags & MONITORINFOF_PRIMARY != 0, info.rcWork));
            }
        }
        1
    }
    let mut found: Vec<(bool, RECT)> = Vec::new();
    // SAFETY: the callback only runs during this call, while `found` lives.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(each),
            &mut found as *mut _ as LPARAM,
        );
    }
    found.sort_by_key(|(primary, _)| !primary);
    let Some((_, main)) = found.first().copied() else {
        return Vec::new();
    };
    found
        .iter()
        .map(|(_, r)| crate::place::Rect {
            left: r.left - main.left,
            top: r.top - main.top,
            right: r.right - main.left,
            bottom: r.bottom - main.top,
        })
        .collect()
}

/// Puts the window where it was, kept on the screens there are now. Shown, it opens
/// there at once (maximized if it was); hidden, as when loom starts in the tray, it
/// waits there and opens that way when it's first shown, hidden before it's moved so
/// it never shows there for a moment. Set twice, so a screen with another scale than
/// the one the window opened on doesn't change its size.
#[cfg(windows)]
pub fn put_window(hwnd: isize, place: crate::place::Place, shown: bool) {
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPlacement, ShowWindow, SW_HIDE, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE,
        WINDOWPLACEMENT,
    };
    if !shown {
        // SAFETY: hwnd is the app's own window.
        unsafe { ShowWindow(hwnd as _, SW_HIDE) };
    }
    let place = place.fit(&screens());
    let r = place.normal;
    let p = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        flags: 0,
        showCmd: if !shown {
            SW_HIDE
        } else if place.maximized {
            SW_SHOWMAXIMIZED
        } else {
            SW_SHOWNOACTIVATE
        } as u32,
        ptMinPosition: POINT { x: -1, y: -1 },
        ptMaxPosition: POINT { x: -1, y: -1 },
        rcNormalPosition: RECT {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        },
    };
    // SAFETY: a filled-in WINDOWPLACEMENT for the app's own window.
    unsafe {
        SetWindowPlacement(hwnd as _, &p);
        SetWindowPlacement(hwnd as _, &p);
    }
    MAXIMIZE_WHEN_SHOWN.store(
        !shown && place.maximized,
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[cfg(target_os = "macos")]
pub fn put_window(hwnd: isize, place: crate::place::Place, shown: bool) {
    crate::mac::put_window(hwnd, place, shown);
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn put_window(_hwnd: isize, _place: crate::place::Place, _shown: bool) {}

/// Keeps a build for measuring, which opens hidden (main.rs), out of the way: its
/// window can never be the one in front, and shown, it's far off every screen, `w`
/// by `h`.
#[cfg(windows)]
pub fn out_of_the_way(hwnd: isize, w: i32, h: i32, shown: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_NOACTIVATE,
        SWP_NOZORDER, SWP_SHOWWINDOW, WS_EX_NOACTIVATE,
    };
    // SAFETY: hwnd is the app's own window.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd as _, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd as _, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE as isize);
        if shown {
            SetWindowPos(
                hwnd as _,
                std::ptr::null_mut(),
                -10000,
                -10000,
                w,
                h,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
            );
        }
    }
}

#[cfg(not(windows))]
pub fn out_of_the_way(_hwnd: isize, _w: i32, _h: i32, _shown: bool) {}

/// The window's native handle, for `show_window`: its HWND on Windows, and its NSView on
/// a Mac.
pub fn hwnd(window: &gpui_kit::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        #[cfg(windows)]
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        #[cfg(target_os = "macos")]
        Ok(RawWindowHandle::AppKit(h)) => h.ns_view.as_ptr() as isize,
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
    /// Only an installed release, started for real, turns starting at login on by itself.
    #[test]
    fn only_an_installed_release_turns_itself_on() {
        use std::path::Path;
        let installed = Path::new(r"C:\Users\x\AppData\Local\Programs\loom\loom.exe");
        let on = |version, perf, test, exe: &str, installed: Option<&Path>| {
            super::turns_itself_on(version, perf, test, Path::new(exe), installed)
        };
        let here = r"C:\Users\x\AppData\Local\Programs\loom\loom.exe";
        assert!(on("0.3.0", false, false, here, Some(installed)));
        // However the path is spelt.
        assert!(on(
            "0.3.0",
            false,
            false,
            r"c:\users\X\appdata\local\programs\LOOM\LOOM.EXE",
            Some(installed)
        ));
        // A dev build, a perf build, or one started for a test.
        assert!(!on("0.2.0-dev", false, false, here, Some(installed)));
        assert!(!on("0.3.0", true, false, here, Some(installed)));
        assert!(!on("0.3.0", false, true, here, Some(installed)));
        // A release somewhere else: a test folder, a download, a build folder.
        let elsewhere = r"C:\Users\x\AppData\Local\Temp\loo233\loom-app.exe";
        assert!(!on("0.3.0", false, false, elsewhere, Some(installed)));
        assert!(!on(
            "0.3.0",
            false,
            false,
            r"C:\Users\x\Downloads\loom.exe",
            Some(installed)
        ));
        // Where nothing is installed.
        assert!(!on("0.3.0", false, false, here, None));
        // A Mac's: in Applications, and not run from its disk image.
        let app = Path::new("/Applications/loom.app/Contents/MacOS/loom");
        let on_mac =
            |exe: &str| super::turns_itself_on("0.3.0", false, false, Path::new(exe), Some(app));
        assert!(on_mac("/Applications/loom.app/Contents/MacOS/loom"));
        assert!(!on_mac("/Volumes/loom/loom.app/Contents/MacOS/loom"));
    }

    /// The program a Run entry starts, quoted as the setup writes it or not, as the
    /// auto-launch crate does.
    #[test]
    fn reads_the_program_a_run_entry_starts() {
        let exe = r"C:\Users\A B\AppData\Local\Programs\loom\loom.exe";
        assert_eq!(super::run_program(&format!("\"{exe}\" --background")), exe);
        assert_eq!(super::run_program(&format!("{exe} --background")), exe);
        assert_eq!(super::run_program(&format!("  {exe}  ")), exe);
        assert_eq!(
            super::run_program(r"C:\loom\LOOM.EXE --x"),
            r"C:\loom\LOOM.EXE"
        );
        assert_eq!(
            super::run_program("/Applications/loom"),
            "/Applications/loom"
        );
    }

    /// A Mac notification's tag carries what it's about back from a click.
    #[test]
    fn a_notification_s_tag_says_what_it_is_about() {
        let about = super::About {
            server: "servers/2".into(),
            agent: "a_1".into(),
            conversation: "c_9".into(),
        };
        let tag = super::tag_of(7, &about);
        assert_ne!(tag, super::tag_of(8, &about));
        assert_eq!(super::about_of(&tag), Some(about));
        assert_eq!(super::about_of("nothing"), None);
    }

    /// The program a Mac's LaunchAgent starts, as auto-launch writes it.
    #[test]
    fn reads_the_program_a_launch_agent_starts() {
        let plist = "<plist version=\"1.0\">\n  <dict>\n  <key>Label</key>\n  <string>no.aaenz.loom.desktop</string>\n  <key>AssociatedBundleIdentifiers</key>\n  <array><string>no.aaenz.loom.desktop</string></array>\n  <key>ProgramArguments</key>\n  <array><string>/Applications/loom.app/Contents/MacOS/loom</string><string>--background</string></array>\n  </dict>\n</plist>";
        assert_eq!(
            super::agent_program(plist),
            Some("/Applications/loom.app/Contents/MacOS/loom")
        );
        assert_eq!(super::agent_program(""), None);
    }

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

    /// loom.exe's icons, light then dark (build.rs), as this test program carries them too.
    #[cfg(windows)]
    #[test]
    fn carries_the_light_and_the_dark_icon() {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{LoadImageW, IMAGE_ICON, LR_SHARED};
        for id in [1usize, 2] {
            // SAFETY: an icon from this program's own resources, shared, never freed.
            let icon = unsafe {
                LoadImageW(
                    GetModuleHandleW(std::ptr::null()),
                    id as _,
                    IMAGE_ICON,
                    32,
                    32,
                    LR_SHARED,
                )
            };
            assert!(!icon.is_null(), "no icon {id}");
        }
    }

    /// A shortcut to loom is pointed at the icon for the theme and keeps the rest; one
    /// to another program is left alone.
    #[cfg(windows)]
    #[test]
    fn points_loom_s_shortcut_at_the_theme_s_icon() {
        use windows::core::{Interface, HSTRING};
        use windows::Win32::System::Com::*;
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
        let dir = std::env::temp_dir().join(format!("loom-icon-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("loom.exe");
        let other = dir.join("other.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        std::fs::write(&other, b"MZ").unwrap();
        let make = |lnk: &std::path::Path, target: &std::path::Path| unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            link.SetPath(&HSTRING::from(target.as_os_str())).unwrap();
            link.SetIconLocation(&HSTRING::from(target.as_os_str()), 0)
                .unwrap();
            link.SetDescription(&HSTRING::from("kept")).unwrap();
            let file: IPersistFile = link.cast().unwrap();
            file.Save(&HSTRING::from(lnk.as_os_str()), true).unwrap();
        };
        let read = |lnk: &std::path::Path| unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let file: IPersistFile = link.cast().unwrap();
            file.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ)
                .unwrap();
            let mut icon = [0u16; 1024];
            let mut index = -1;
            link.GetIconLocation(&mut icon, &mut index).unwrap();
            let mut said = [0u16; 1024];
            link.GetDescription(&mut said).unwrap();
            let end = said.iter().position(|&c| c == 0).unwrap_or(0);
            (index, String::from_utf16_lossy(&said[..end]))
        };
        let ours = dir.join("loom.lnk");
        let theirs = dir.join("other.lnk");
        make(&ours, &exe);
        make(&theirs, &other);
        super::point_icon(&ours, &exe, 1).unwrap();
        super::point_icon(&theirs, &exe, 1).unwrap();
        assert_eq!(read(&ours), (1, "kept".to_string()));
        assert_eq!(read(&theirs).0, 0);
        super::point_icon(&ours, &exe, 0).unwrap();
        assert_eq!(read(&ours).0, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A window put in its place to wait hidden, as when loom starts in the tray, is
    /// hidden before it moves, reads back in that place, maximized too, and is never
    /// given the focus. It starts shown, as GPUI opens the window before loom started at
    /// login sends it to the tray, but far off screen.
    #[cfg(windows)]
    #[test]
    fn a_window_keeps_the_place_it_is_put_in() {
        use crate::place::{Place, Rect};
        use std::cell::RefCell;
        use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindowVisible, RegisterClassW,
            SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
            SW_HIDE, SW_SHOWNOACTIVATE, WINDOWPOS, WM_WINDOWPOSCHANGED, WNDCLASSW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW,
        };
        thread_local! {
            // Each move the window made: whether it was visible, and its left edge.
            static MOVES: RefCell<Vec<(bool, i32)>> = const { RefCell::new(Vec::new()) };
        }
        unsafe extern "system" fn record(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            // SAFETY: WM_WINDOWPOSCHANGED's lparam is a WINDOWPOS, for this call.
            unsafe {
                if msg == WM_WINDOWPOSCHANGED {
                    let pos = &*(l as *const WINDOWPOS);
                    if pos.flags & SWP_NOMOVE == 0 {
                        let seen = IsWindowVisible(h) != 0;
                        MOVES.with(|m| m.borrow_mut().push((seen, pos.x)));
                    }
                }
                DefWindowProcW(h, msg, w, l)
            }
        }
        let class: Vec<u16> = "loom-place-test\0".encode_utf16().collect();
        // SAFETY: a window class of this test's own, and a window of it that never takes
        // the focus or a taskbar button, destroyed at the end.
        let hwnd = unsafe {
            let module = GetModuleHandleW(std::ptr::null());
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(record);
            wc.hInstance = module;
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_OVERLAPPEDWINDOW,
                -10000,
                -10000,
                400,
                300,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                module,
                std::ptr::null(),
            );
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            hwnd
        } as isize;
        assert_ne!(hwnd, 0);
        // SAFETY: the window made above.
        assert_ne!(unsafe { IsWindowVisible(hwnd as _) }, 0);
        // On the main screen wherever it is: its work area always holds this.
        let main = super::screens()[0];
        let normal = Rect {
            left: main.left + 40,
            top: main.top + 30,
            right: main.left + 40 + 700,
            bottom: main.top + 30 + 500,
        };
        for maximized in [false, true] {
            let place = Place { normal, maximized };
            super::put_window(hwnd, place, false);
            let back = super::window_place(hwnd).expect("a placement");
            assert_eq!(back.normal, normal);
            // Hidden, Windows maximizes it only once it's shown, but it reads as it
            // will open.
            assert_eq!(back.maximized, maximized);
            assert_eq!(
                super::MAXIMIZE_WHEN_SHOWN.load(std::sync::atomic::Ordering::Relaxed),
                maximized
            );
            // SAFETY: the window made above.
            assert_eq!(unsafe { IsWindowVisible(hwnd as _) }, 0);
        }
        // Moved while it waits hidden, as by a display change, it still reads maximized,
        // so the place kept then opens it maximized next time. Off screen again, then
        // shown there, not maximized, it reads as it is.
        // SAFETY: the window made above.
        unsafe {
            SetWindowPos(
                hwnd as _,
                std::ptr::null_mut(),
                -10000,
                -10000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        assert!(super::window_place(hwnd).expect("a placement").maximized);
        // SAFETY: the window made above.
        unsafe { ShowWindow(hwnd as _, SW_SHOWNOACTIVATE) };
        assert!(!super::window_place(hwnd).expect("a placement").maximized);
        // SAFETY: the window made above.
        unsafe { ShowWindow(hwnd as _, SW_HIDE) };
        // It came onto the screen, and was hidden each time it did.
        let moves = MOVES.with(|m| m.borrow().clone());
        assert!(moves.iter().any(|&(_, x)| x > -5000), "{moves:?}");
        assert!(
            moves.iter().all(|&(seen, x)| !seen || x <= -5000),
            "{moves:?}"
        );
        super::MAXIMIZE_WHEN_SHOWN.store(false, std::sync::atomic::Ordering::Relaxed);
        // SAFETY: the window made above.
        unsafe { DestroyWindow(hwnd as _) };
    }
}
