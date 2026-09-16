//! Windows implementation of computer use.
//!
//! Everything here is `SendInput` and Win32 reads/writes; no new dependencies.
//! Coordinates arriving from [`super::mouse`] are already virtual-desktop
//! pixels. Input is checked against the foreground window's elevation so a
//! click into an elevated app fails loudly instead of silently vanishing
//! (UIPI).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use windows::core::{BSTR, BOOL, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, FALSE, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM,
};
use windows::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IDataObject, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Ole::{
    OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess, OpenProcessToken,
    TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomationElement, IUIAutomationExpandCollapsePattern,
    IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern,
    IUIAutomationTreeWalker, IUIAutomationValuePattern, UIA_ExpandCollapsePatternId,
    UIA_InvokePatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEINPUT,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSE_EVENT_FLAGS,
    VIRTUAL_KEY, VK_APPS, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
    VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_MENU, VK_NEXT, VK_NUMLOCK, VK_OEM_1, VK_OEM_2,
    VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS,
    VK_OEM_PERIOD, VK_OEM_PLUS, VK_PAUSE, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SCROLL, VK_SHIFT,
    VK_SNAPSHOT, VK_SPACE, VK_TAB, VK_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, EnumWindows, GetAncestor, GetForegroundWindow,
    GetWindowLongPtrW, GetMessageW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, SetWindowsHookExW, ShowWindow, TranslateMessage,
    UnhookWindowsHookEx, WindowFromPoint, GA_ROOT, GWL_EXSTYLE, HHOOK, HWND_NOTOPMOST, HWND_TOPMOST,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, SW_MAXIMIZE, SW_MINIMIZE,
    SW_RESTORE, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_CLOSE, WM_MOUSEMOVE, WS_EX_NOACTIVATE,
};

use super::{Capture, CaptureResult, UiNode};
use crate::{screen, Error, Result};

// ------------------------------------------------------------------
// small helpers
// ------------------------------------------------------------------

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn window_title(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; (length + 1) as usize];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
}

fn window_rect(hwnd: HWND) -> Option<(i32, i32, u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect).ok()? };
    Some((
        rect.left,
        rect.top,
        (rect.right - rect.left).unsigned_abs(),
        (rect.bottom - rect.top).unsigned_abs(),
    ))
}

fn hwnd_from_hex(text: &str) -> Option<HWND> {
    let trimmed = text.trim();
    let value = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .and_then(|hex| usize::from_str_radix(hex, 16).ok())
        .or_else(|| trimmed.parse::<usize>().ok())?;
    Some(HWND(value as *mut core::ffi::c_void))
}

/// Resolves a window argument: an `0x…` hwnd, or a title substring (exact
/// title beats substring, and the focused window is preferred).
fn resolve_window(spec: &str) -> Result<HWND> {
    if let Some(hwnd) = hwnd_from_hex(spec) {
        return Ok(hwnd);
    }
    if spec.trim().is_empty() {
        let foreground = unsafe { GetForegroundWindow() };
        if !foreground.0.is_null() {
            return Ok(foreground);
        }
        return Err(Error::Other("no window is focused".into()));
    }
    find_window_impl(spec)?.ok_or_else(|| Error::Other(format!("no window matching \"{spec}\"")))
}

fn find_window_impl(title: &str) -> Result<Option<HWND>> {
    let needle = title.to_lowercase();
    let mut found: Vec<(bool, HWND)> = Vec::new();
    unsafe {
        EnumWindows(
            Some(collect_window),
            LPARAM(&mut found as *mut Vec<(bool, HWND)> as isize),
        )
        .map_err(|error| Error::Other(format!("window enumeration failed: {error}")))?;
    }
    let exact = found
        .iter()
        .find(|(_, hwnd)| window_title(*hwnd).to_lowercase() == needle)
        .map(|(_, hwnd)| *hwnd);
    if exact.is_some() {
        return Ok(exact);
    }
    // Prefer windows that are not the owner-less background helpers: the
    // first topmost match is the one the user thinks of.
    Ok(found
        .iter()
        .find(|(_, hwnd)| window_title(*hwnd).to_lowercase().contains(&needle))
        .map(|(_, hwnd)| *hwnd))
}

unsafe extern "system" fn collect_window(hwnd: HWND, data: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }
    let title = window_title(hwnd);
    if title.is_empty() {
        return TRUE;
    }
    let list = &mut *(data.0 as *mut Vec<(bool, HWND)>);
    list.push((true, hwnd));
    TRUE
}

pub fn find_window(title: &str) -> Result<Option<isize>> {
    Ok(find_window_impl(title)?.map(|hwnd| hwnd.0 as isize))
}

fn foreground_window() -> Option<HWND> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(foreground, Some(&mut pid)) };
    if pid == unsafe { GetCurrentProcessId() } {
        return None;
    }
    if !unsafe { IsWindowVisible(foreground) }.as_bool() {
        return None;
    }
    Some(foreground)
}

/// True when the process owning `hwnd` runs elevated: input sent to it is
/// dropped by UIPI, so saying so beats silently missing.
fn window_is_elevated(hwnd: HWND) -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let process = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(process) => process,
            Err(_) => return false,
        };
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &mut token).is_ok();
        let _ = CloseHandle(process);
        if !opened {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let read = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        read && elevation.TokenIsElevated != 0
    }
}

/// Refuses when the focused process is elevated: UIPI drops the input, so
/// saying so beats a click that silently vanishes.
fn elevation_blocked() -> Result<()> {
    if let Some(foreground) = foreground_window() {
        if window_is_elevated(foreground) {
            return Err(Error::Other(
                "that window is elevated; Loom must run elevated to control it".into(),
            ));
        }
    }
    Ok(())
}

/// Refuses keyboard input when one of Loom's own windows has focus. A click
/// can be aimed at a window the user is not focused on, so it is only subject
/// to [`elevation_blocked`]; a keystroke goes wherever focus is, and typing a
/// message into Loom's own composer is never what the model meant.
fn keyboard_target_blocked() -> Result<()> {
    elevation_blocked()?;
    if is_own_window(unsafe { GetForegroundWindow() }) {
        return Err(Error::Other(
            "Loom's own window has focus, so keystrokes would land in Loom. Focus the window \
             you want to type into first (window action focus), then send the keys."
                .into(),
        ));
    }
    Ok(())
}

/// Refuses pointer input aimed at a window the input cannot reach.
///
/// The elevation check has to look at the window **under the point**, not at
/// whichever window happens to have focus. Mouse input goes to the window the
/// pointer is over, so foreground-based checks were both wrong in each
/// direction: they refused a click on an ordinary window because a *different*
/// elevated window had focus, and allowed a click that landed on an elevated
/// window merely because focus was elsewhere. UIPI drops the latter silently,
/// so the model would believe a click had happened.
fn cursor_target_blocked(x: i32, y: i32) -> Result<()> {
    let window = unsafe { WindowFromPoint(POINT { x, y }) };
    if window.0.is_null() {
        return Err(Error::Other(format!(
            "there is no window at screen ({x},{y})"
        )));
    }
    if window_is_elevated(window) {
        return Err(Error::Other(
            "the window at that point is elevated; Loom must run elevated to control it"
                .into(),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------
// capture
// ------------------------------------------------------------------

pub fn capture(request: &Capture) -> Result<CaptureResult> {
    let (image, rect, monitor) = match request.target.as_str() {
        "window" => {
            let spec = request
                .window
                .as_deref()
                .ok_or_else(|| Error::Other("target window needs a window".into()))?;
            let hwnd = resolve_window(spec)?;
            let rect = window_rect(hwnd)
                .ok_or_else(|| Error::Other("could not read the window rectangle".into()))?;
            let index = screen::monitor_index_for_window(hwnd.0 as isize).unwrap_or(0);
            (screen::capture_window(hwnd.0 as isize)?, rect, index)
        }
        "monitor" => {
            let index = request.monitor.unwrap_or(0);
            let monitors = screen::monitors()?;
            let monitor = monitors.get(index as usize).ok_or_else(|| {
                Error::Other(format!("there is no monitor {index}"))
            })?;
            (screen::capture_monitor(index)?, monitor.rect, index)
        }
        "all" => (screen::capture_virtual()?, screen::desktop_rect(), 0),
        "region" => {
            let (x, y, width, height) = request
                .region
                .ok_or_else(|| Error::Other("target region needs a region".into()))?;
            let index = screen::monitor_index_for_point(x + width as i32 / 2, y + height as i32 / 2)
                .unwrap_or(0);
            let monitors = screen::monitors()?;
            let monitor = monitors
                .iter()
                .find(|monitor| monitor.index == index)
                .ok_or_else(|| Error::Other("region is not on any monitor".into()))?;
            let (mx, my, mw, mh) = monitor.rect;
            let inside = x >= mx
                && y >= my
                && x + width as i32 <= mx + mw as i32
                && y + height as i32 <= my + mh as i32;
            let frame = if inside {
                let full = screen::capture_monitor(index)?;
                let ox = (x - mx) as u32;
                let oy = (y - my) as u32;
                image::imageops::crop_imm(&full, ox, oy, width, height).to_image()
            } else {
                let full = screen::capture_virtual()?;
                let (vx, vy, vw, vh) = screen::desktop_rect();
                let fits = x >= vx
                    && y >= vy
                    && x + width as i32 <= vx + vw as i32
                    && y + height as i32 <= vy + vh as i32;
                if !fits {
                    return Err(Error::Other(format!(
                        "region ({x},{y},{width}x{height}) is outside the virtual desktop \
                         ({vx},{vy},{vw}x{vh})"
                    )));
                }
                image::imageops::crop_imm(
                    &full,
                    (x - vx) as u32,
                    (y - vy) as u32,
                    width,
                    height,
                )
                .to_image()
            };
            let frame = if request.scale > 1 {
                let target_w = frame.width() * request.scale;
                let target_h = frame.height() * request.scale;
                image::imageops::resize(
                    &frame,
                    target_w,
                    target_h,
                    image::imageops::FilterType::Triangle,
                )
            } else {
                frame
            };
            ((frame), (x, y, width, height), index)
        }
        _ => {
            // `active`: the focused window's monitor, else the cursor's.
            let index = foreground_window()
                .and_then(|hwnd| screen::monitor_index_for_window(hwnd.0 as isize))
                .or_else(|| {
                    screen::cursor_screen_pos()
                        .ok()
                        .and_then(|(x, y)| screen::monitor_index_for_point(x, y))
                })
                .unwrap_or(0);
            let monitors = screen::monitors()?;
            let monitor = monitors
                .iter()
                .find(|monitor| monitor.index == index)
                .ok_or_else(|| Error::Other("no monitor to capture".into()))?;
            (screen::capture_monitor(index)?, monitor.rect, index)
        }
    };

    let mut image = image;
    let cursor = if request.cursor {
        match screen::cursor_screen_pos() {
            Ok((x, y)) => {
                draw_cursor(&mut image, rect, x, y);
                Some((x, y))
            }
            Err(_) => None,
        }
    } else {
        None
    };

    let frame_hash = frame_hash(&image);
    let monitors = screen::monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|display| {
            let primary = if display.primary { " primary" } else { "" };
            let current = if display.index == monitor { " *" } else { "" };
            format!(
                "{}: ({},{}) {}x{} @{}%{primary}{current}",
                display.index, display.rect.0, display.rect.1, display.rect.2, display.rect.3,
                display.dpi,
            )
        })
        .collect();

    Ok(CaptureResult {
        image,
        rect,
        monitor,
        monitors,
        cursor,
        frame_hash,
    })
}

/// A crosshair so the model can see where the pointer is. Drawn at the raw
/// capture's scale; the arm grows with the frame so the marker survives a
/// provider-side downscale of a 4K screenshot.
fn draw_cursor(image: &mut image::RgbaImage, rect: (i32, i32, u32, u32), x: i32, y: i32) {
    let cx = x - rect.0;
    let cy = y - rect.1;
    if cx < 0 || cy < 0 || cx >= image.width() as i32 || cy >= image.height() as i32 {
        return;
    }
    let (cx, cy) = (cx as i32, cy as i32);
    let arm = ((image.width() / 120).clamp(14, 48)) as i32;
    let thickness = ((image.width() / 960).clamp(1, 4)) as i32;
    let paint = |image: &mut image::RgbaImage, x: i32, y: i32| {
        for dx in 0..thickness {
            for dy in 0..thickness {
                let px = x + dx;
                let py = y + dy;
                if px >= 0 && py >= 0 && px < image.width() as i32 && py < image.height() as i32 {
                    image.put_pixel(px as u32, py as u32, image::Rgba([255, 40, 40, 255]));
                }
            }
        }
    };
    for step in -arm..=arm {
        paint(image, cx + step, cy);
        paint(image, cx, cy + step);
    }
}

/// A cheap content fingerprint: a 64x64 sample of pixels, FNV-1a.
fn frame_hash(image: &image::RgbaImage) -> u64 {
    let step_x = (image.width() / 64).max(1) as usize;
    let step_y = (image.height() / 64).max(1) as usize;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for y in (0..image.height()).step_by(step_y) {
        for x in (0..image.width()).step_by(step_x) {
            let pixel = image.get_pixel(x, y);
            for channel in [pixel[0], pixel[1], pixel[2]] {
                hash ^= u64::from(channel);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    hash
}

// ------------------------------------------------------------------
// input: mouse
// ------------------------------------------------------------------

/// One wheel notch, in the units `mouseData` expects.
const WHEEL_DELTA: i32 = 120;
/// Pause between wheel events. Enough for a queued event to be processed,
/// short enough that a 20-notch scroll still feels immediate.
const SCROLL_STEP_MS: u64 = 12;
/// Settling time after a drag's final move and before the button release. Apps
/// that update their target on a move timer need to see the final position
/// before the drop, or the drop lands where the pointer was a frame ago.
const DRAG_SETTLE_MS: u64 = 60;

/// Moves the pointer and reports whether it actually arrived. `SendInput`
/// reports the number of events *inserted into the queue*, not delivered, so it
/// says nothing about whether the movement happened. Comparing against
/// `GetCursorPos` is the only way to know.
fn move_verified(x: i32, y: i32) -> bool {
    move_absolute(x, y);
    // A click or a keystroke can move the pointer between the two reads, which
    // would read as a failure; one retry and a tight matching window keeps that
    // from turning into a false alarm.
    for _ in 0..2 {
        match screen::cursor_screen_pos() {
            Ok((cx, cy)) if (cx - x).abs() <= 2 && (cy - y).abs() <= 2 => return true,
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                move_absolute(x, y);
            }
        }
    }
    matches!(screen::cursor_screen_pos(), Ok((cx, cy)) if (cx - x).abs() <= 2 && (cy - y).abs() <= 2)
}

/// Why a `move_verified` failure matters, as one sentence.
const CURSOR_STUCK: &str = "the pointer did not move to that position — another program is \
    holding the mouse (a game, a remote-desktop session, or a pointer-locked window), or the \
    coordinates are outside the desktop. `position` reports where it actually is.";

fn mouse_input(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode_input(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

/// Absolute move over the virtual desktop, so negative-origin multi-monitor
/// setups work the way the captured rectangles say they should.
fn move_absolute(x: i32, y: i32) {
    let (vx, vy, vw, vh) = screen::desktop_rect();
    let width = (vw.max(2) - 1) as f64;
    let height = (vh.max(2) - 1) as f64;
    let nx = ((f64::from(x - vx) / width) * 65535.0).round() as i32;
    let ny = ((f64::from(y - vy) / height) * 65535.0).round() as i32;
    send(&[mouse_input(
        nx,
        ny,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )]);
}

fn modifier_key(name: &str) -> Option<VIRTUAL_KEY> {
    Some(match name {
        "ctrl" | "control" => VK_CONTROL,
        "alt" => VK_MENU,
        "shift" => VK_SHIFT,
        "win" | "super" | "meta" => VK_LWIN,
        _ => return None,
    })
}

fn with_modifiers(modifiers: &[String], action: impl FnOnce()) -> Result<()> {
    let keys: Vec<VIRTUAL_KEY> = modifiers
        .iter()
        .filter_map(|name| modifier_key(name))
        .collect();
    if keys.len() != modifiers.len() {
        let unknown: Vec<&str> = modifiers
            .iter()
            .filter(|name| modifier_key(name).is_none())
            .map(String::as_str)
            .collect();
        return Err(Error::Other(format!(
            "unknown modifier(s): {}",
            unknown.join(", ")
        )));
    }
    let downs: Vec<INPUT> = keys
        .iter()
        .map(|key| key_input(*key, KEYBD_EVENT_FLAGS(0)))
        .collect();
    let ups: Vec<INPUT> = keys
        .iter()
        .rev()
        .map(|key| key_input(*key, KEYEVENTF_KEYUP))
        .collect();
    send(&downs);
    action();
    send(&ups);
    Ok(())
}

fn button_flags(button: &str, up: bool) -> MOUSE_EVENT_FLAGS {
    match (button, up) {
        ("right", false) => MOUSEEVENTF_RIGHTDOWN,
        ("right", true) => MOUSEEVENTF_RIGHTUP,
        ("middle", false) => MOUSEEVENTF_MIDDLEDOWN,
        ("middle", true) => MOUSEEVENTF_MIDDLEUP,
        (_, false) => MOUSEEVENTF_LEFTDOWN,
        (_, true) => MOUSEEVENTF_LEFTUP,
    }
}

fn click(button: &str, count: u32) {
    for _ in 0..count {
        send(&[mouse_input(0, 0, 0, button_flags(button, false))]);
        send(&[mouse_input(0, 0, 0, button_flags(button, true))]);
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

pub fn mouse(
    action: &str,
    x: i32,
    y: i32,
    to: Option<(i32, i32)>,
    button: &str,
    count: u32,
    modifiers: &[String],
    amount: i32,
    horizontal: bool,
    duration_ms: u32,
    steps: u32,
) -> Result<String> {
    match action {
        "move" => {
            if !move_verified(x, y) {
                return Err(Error::Other(CURSOR_STUCK.into()));
            }
            Ok(format!("moved to screen ({x},{y})"))
        }
        "click" | "double_click" | "right_click" | "middle_click" => {
            let button = match action {
                "right_click" => "right",
                "middle_click" => "middle",
                _ => button,
            };
            let clicks = if action == "double_click" { 2 } else { count };
            // The window under the point is what receives the click, so it is
            // the one that has to be reachable.
            cursor_target_blocked(x, y)?;
            if !move_verified(x, y) {
                return Err(Error::Other(CURSOR_STUCK.into()));
            }
            with_modifiers(modifiers, || click(button, clicks))?;
            Ok(format!(
                "{} {} at screen ({x},{y})",
                if clicks > 1 { "double-clicked" } else { "clicked" },
                button
            ))
        }
        "down" => {
            cursor_target_blocked(x, y)?;
            if !move_verified(x, y) {
                return Err(Error::Other(CURSOR_STUCK.into()));
            }
            with_modifiers(modifiers, || {
                send(&[mouse_input(0, 0, 0, button_flags(button, false))])
            })?;
            Ok(format!("{button} button down at screen ({x},{y})"))
        }
        "up" => {
            send(&[mouse_input(0, 0, 0, button_flags(button, true))]);
            Ok(format!("{button} button up"))
        }
        "drag" => {
            let (tx, ty) = to.ok_or_else(|| {
                Error::Other("drag needs to_x and to_y (image coordinates)".into())
            })?;
            // Both ends matter: the button goes down on the source and the drop
            // lands on the destination, so an elevated window at either end
            // means the gesture cannot work.
            cursor_target_blocked(x, y)?;
            cursor_target_blocked(tx, ty)?;

            // The press has to happen *over the source*, so a failure to get
            // there is fatal — dragging from wherever the pointer happened to
            // be picks up something else entirely, which is worse than not
            // dragging at all.
            if !move_verified(x, y) {
                return Err(Error::Other(format!("{CURSOR_STUCK} The drag was not started.")));
            }
            send(&[mouse_input(0, 0, 0, button_flags(button, false))]);
            // Let the target register the press before anything moves: a
            // drag-detection threshold and a pressed-state message both need a
            // frame to land.
            std::thread::sleep(std::time::Duration::from_millis(DRAG_SETTLE_MS));

            // Interpolated in screen space, one step per requested step, with
            // the time spread evenly across them. Real (non-injected) motion is
            // a stream of small moves; a single teleport is ignored by drop
            // targets that track the pointer.
            let steps = steps.max(1);
            let per_step = if duration_ms == 0 {
                0
            } else {
                // At least a millisecond, so a fast drag is still a sequence
                // rather than a coalesced jump.
                (duration_ms / steps).max(1)
            };
            for step in 1..=steps {
                let progress = f64::from(step) / f64::from(steps);
                let ix = x + ((tx - x) as f64 * progress).round() as i32;
                let iy = y + ((ty - y) as f64 * progress).round() as i32;
                move_absolute(ix, iy);
                if per_step > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(u64::from(per_step)));
                }
            }
            // Arrive exactly, and give the target a moment to see it: a drop at
            // the position from the previous frame lands in the wrong place.
            move_absolute(tx, ty);
            std::thread::sleep(std::time::Duration::from_millis(DRAG_SETTLE_MS));
            send(&[mouse_input(0, 0, 0, button_flags(button, true))]);

            let landed = screen::cursor_screen_pos()
                .map(|(cx, cy)| (cx - tx).abs() <= 2 && (cy - ty).abs() <= 2)
                .unwrap_or(true);
            Ok(if landed {
                format!("dragged {button} from ({x},{y}) to ({tx},{ty})")
            } else {
                format!(
                    "dragged {button} from ({x},{y}) toward ({tx},{ty}), but the pointer did \
                     not end up there — the drop may have landed elsewhere"
                )
            })
        }
        "scroll" => {
            let notches = amount.clamp(-100, 100);
            if notches == 0 {
                return Err(Error::Other(
                    "scroll needs a non-zero amount (negative is up/left)".into(),
                ));
            }
            // The wheel goes to whatever is under the pointer, so the move is
            // part of the action, not a nicety: without it the event scrolls
            // whichever window the cursor happens to be over.
            if !move_verified(x, y) {
                return Err(Error::Other(CURSOR_STUCK.into()));
            }
            let flag = if horizontal {
                MOUSEEVENTF_HWHEEL
            } else {
                MOUSEEVENTF_WHEEL
            };
            for data in super::wheel_deltas(notches, WHEEL_DELTA) {
                send(&[mouse_input(0, 0, data as u32, flag)]);
                std::thread::sleep(std::time::Duration::from_millis(SCROLL_STEP_MS));
            }
            Ok(format!(
                "scrolled {} {notches} notches at screen ({x},{y})",
                if horizontal { "horizontally" } else { "vertically" }
            ))
        }
        other => Err(Error::Other(format!("unknown mouse action: {other}"))),
    }
}

pub fn cursor_pos() -> Result<(i32, i32)> {
    screen::cursor_screen_pos()
}

/// Refuses keyboard input that would land somewhere it cannot reach or should
/// not go: an elevated target, or Loom's own focused window.
pub fn check_input_target() -> Result<()> {
    keyboard_target_blocked()
}

/// The virtual-key code for a key name, for held-key bookkeeping.
pub fn named_key_code(name: &str) -> Option<u16> {
    named_key(name).map(|key| key.0)
}

pub fn release(buttons: [bool; 3], keys: &[u16]) {
    if buttons[0] {
        send(&[mouse_input(0, 0, 0, MOUSEEVENTF_LEFTUP)]);
    }
    if buttons[1] {
        send(&[mouse_input(0, 0, 0, MOUSEEVENTF_RIGHTUP)]);
    }
    if buttons[2] {
        send(&[mouse_input(0, 0, 0, MOUSEEVENTF_MIDDLEUP)]);
    }
    let ups: Vec<INPUT> = keys
        .iter()
        .map(|vk| key_input(VIRTUAL_KEY(*vk), KEYEVENTF_KEYUP))
        .collect();
    send(&ups);
}

// ------------------------------------------------------------------
// input: keyboard
// ------------------------------------------------------------------

fn named_key(name: &str) -> Option<VIRTUAL_KEY> {
    let upper = name.trim().to_ascii_uppercase();
    let simple = match upper.as_str() {
        "ENTER" | "RETURN" => VK_RETURN,
        "ESC" | "ESCAPE" => VK_ESCAPE,
        "TAB" => VK_TAB,
        "SPACE" => VK_SPACE,
        "BACKSPACE" => VK_BACK,
        "DELETE" | "DEL" => VK_DELETE,
        "INSERT" | "INS" => VK_INSERT,
        "HOME" => VK_HOME,
        "END" => VK_END,
        "PAGEUP" | "PGUP" => VK_PRIOR,
        "PAGEDOWN" | "PGDN" => VK_NEXT,
        "UP" => VK_UP,
        "DOWN" => VK_DOWN,
        "LEFT" => VK_LEFT,
        "RIGHT" => VK_RIGHT,
        "PRINTSCREEN" | "PRTSC" => VK_SNAPSHOT,
        "CAPSLOCK" => VK_CAPITAL,
        "NUMLOCK" => VK_NUMLOCK,
        "SCROLLLOCK" => VK_SCROLL,
        "PAUSE" => VK_PAUSE,
        "MENU" | "APPS" => VK_APPS,
        "WIN" | "SUPER" | "META" => VK_LWIN,
        "CTRL" | "CONTROL" => VK_CONTROL,
        "ALT" => VK_MENU,
        "SHIFT" => VK_SHIFT,
        "VOLUMEUP" => VK_VOLUME_UP,
        "VOLUMEDOWN" => VK_VOLUME_DOWN,
        "VOLUMEMUTE" | "MUTE" => VK_VOLUME_MUTE,
        "PLAYPAUSE" | "MEDIAPLAYPAUSE" => VK_MEDIA_PLAY_PAUSE,
        "NEXTTRACK" | "MEDIANEXT" => VK_MEDIA_NEXT_TRACK,
        "PREVTRACK" | "MEDIAPREV" => VK_MEDIA_PREV_TRACK,
        "STOP" => VK_MEDIA_STOP,
        ";" => VK_OEM_1,
        "/" => VK_OEM_2,
        "`" => VK_OEM_3,
        "[" => VK_OEM_4,
        "\\" => VK_OEM_5,
        "]" => VK_OEM_6,
        "'" => VK_OEM_7,
        "," => VK_OEM_COMMA,
        "-" => VK_OEM_MINUS,
        "." => VK_OEM_PERIOD,
        "=" => VK_OEM_PLUS,
        _ => {
            if upper.starts_with('F') && upper.len() <= 3 {
                if let Ok(number) = upper[1..].parse::<u16>() {
                    if (1..=24).contains(&number) {
                        return Some(VIRTUAL_KEY(VK_F1.0 + number - 1));
                    }
                }
            }
            if upper.len() == 1 {
                let character = upper.chars().next().unwrap();
                if character.is_ascii_alphanumeric() {
                    return Some(VIRTUAL_KEY(character as u16));
                }
            }
            return None;
        }
    };
    Some(simple)
}

/// Keys that need the extended-key flag to reach the right physical key.
fn is_extended(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_INSERT | VK_DELETE | VK_HOME | VK_END | VK_NEXT | VK_UP | VK_DOWN | VK_LEFT | VK_RIGHT
    )
}

fn key_flags(vk: VIRTUAL_KEY, up: bool) -> KEYBD_EVENT_FLAGS {
    let mut flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    flags
}

pub fn key(action: &str, name: &str, repeat: u32) -> Result<String> {
    let vk = named_key(name)
        .ok_or_else(|| Error::Other(format!("unknown key name \"{name}\"")))?;
    keyboard_target_blocked()?;
    for _ in 0..repeat {
        match action {
            "down" => send(&[key_input(vk, key_flags(vk, false))]),
            "up" => send(&[key_input(vk, key_flags(vk, true))]),
            _ => {
                send(&[key_input(vk, key_flags(vk, false))]);
                std::thread::sleep(std::time::Duration::from_millis(20));
                send(&[key_input(vk, key_flags(vk, true))]);
            }
        }
        if repeat > 1 {
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }
    Ok(format!(
        "{action} {} ×{repeat}",
        name.trim().to_ascii_lowercase()
    ))
}

pub fn combo(keys: &[String], repeat: u32) -> Result<String> {
    let vks: Vec<VIRTUAL_KEY> = keys
        .iter()
        .map(|name| {
            named_key(name)
                .ok_or_else(|| Error::Other(format!("unknown key name \"{name}\"")))
        })
        .collect::<Result<_>>()?;
    keyboard_target_blocked()?;
    let downs: Vec<INPUT> = vks
        .iter()
        .map(|vk| key_input(*vk, key_flags(*vk, false)))
        .collect();
    let ups: Vec<INPUT> = vks
        .iter()
        .rev()
        .map(|vk| key_input(*vk, key_flags(*vk, true)))
        .collect();
    for _ in 0..repeat {
        send(&downs);
        std::thread::sleep(std::time::Duration::from_millis(30));
        send(&ups);
        if repeat > 1 {
            std::thread::sleep(std::time::Duration::from_millis(60));
        }
    }
    Ok(format!(
        "pressed {} ×{repeat}",
        keys.join("+").to_ascii_lowercase()
    ))
}

pub fn type_unicode(text: &str) -> Result<String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let inputs: Vec<INPUT> = units
        .iter()
        .flat_map(|unit| {
            [
                unicode_input(*unit, KEYEVENTF_UNICODE),
                unicode_input(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        })
        .collect();
    send(&inputs);
    Ok(format!("typed {} characters", text.chars().count()))
}

/// The clipboard as it was before Loom touched it.
enum Previous {
    /// The whole thing, as an OLE data object: every format, including the
    /// private ones Loom has no idea how to copy (an image, a file set from
    /// Explorer, an app's own flavour of a document).
    Object(IDataObject),
    /// Only text could be read, so only text can go back.
    Text(String),
    /// Nothing could be read. Loom will not clobber what it cannot restore.
    Nothing,
}

/// Types `text` by pasting it through the clipboard, then puts the clipboard
/// back exactly as it was.
///
/// Restoring text alone is not enough: an image, a set of copied files, or a
/// rich-text fragment would be destroyed by the paste and lost for good. The
/// previous contents are therefore held as an OLE data object and set back
/// afterwards. When they cannot be held — OLE unavailable, or the clipboard
/// unreadable — the text is typed out with `SendInput` instead: slower, but
/// nothing is lost, which is the whole point.
pub fn type_via_clipboard(text: &str) -> Result<String> {
    // The OLE clipboard calls are apartment-bound, and the engine's threads are
    // already multithreaded (UI Automation initialises them that way), where
    // `OleInitialize` fails outright. So the whole read-paste-restore dance
    // runs on a thread of its own, initialised single-threaded.
    let payload = text.to_string();
    std::thread::spawn(move || paste_via_clipboard(&payload))
        .join()
        .map_err(|_| Error::Other("the clipboard thread panicked".into()))?
}

fn paste_via_clipboard(text: &str) -> Result<String> {
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let ole = OleInitialize(None).is_ok();
        let result = paste_via_clipboard_inner(text, ole);
        if ole {
            OleUninitialize();
        }
        if com {
            CoUninitialize();
        }
        result
    }
}

fn paste_via_clipboard_inner(text: &str, ole: bool) -> Result<String> {
    let previous = if ole {
        match unsafe { OleGetClipboard() } {
            Ok(object) => Previous::Object(object),
            Err(_) => match clipboard_text() {
                Ok(text) => Previous::Text(text),
                Err(_) => Previous::Nothing,
            },
        }
    } else {
        match clipboard_text() {
            Ok(text) => Previous::Text(text),
            Err(_) => Previous::Nothing,
        }
    };

    // Nothing to restore is not a licence to clobber: typing is slower but
    // lossless, and the alternative is destroying whatever is on there.
    if matches!(previous, Previous::Nothing) && clipboard_holds_something() {
        return type_unicode(text).map(|summary| {
            format!("{summary} (the clipboard held something Loom could not read, so it was left alone)")
        });
    }

    if let Err(error) = write_text(text) {
        return type_unicode(text).map(|summary| format!("{summary} ({error})"));
    }
    std::thread::sleep(std::time::Duration::from_millis(120));
    let pressed = combo(&["ctrl".into(), "v".into()], 1)?;
    // Long enough for a slow app to read the clipboard before it changes back:
    // a target that reads late would otherwise paste the restored contents.
    std::thread::sleep(std::time::Duration::from_millis(400));

    let restored = match previous {
        Previous::Object(object) => unsafe {
            let placed = OleSetClipboard(Some(&object)).is_ok();
            if placed {
                // Put it back in static form, so it survives this thread and
                // Loom releasing the object.
                let _ = OleFlushClipboard();
                clipboard_holds_something()
            } else {
                false
            }
        },
        Previous::Text(previous) => write_text(&previous).is_ok(),
        Previous::Nothing => false,
    };

    Ok(format!(
        "typed {} characters via clipboard ({pressed}); previous clipboard {}",
        text.chars().count(),
        if restored {
            "restored"
        } else {
            "NOT restored — copy it again if you needed it"
        }
    ))
}

// ------------------------------------------------------------------
// clipboard
// ------------------------------------------------------------------

const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

/// The clipboard's text, in full. Used where the exact contents have to go
/// back afterwards, so it is deliberately uncapped.
fn clipboard_text() -> Result<String> {
    clipboard_text_capped(usize::MAX).map(|(text, _)| text)
}

/// The clipboard's text, stopping after `max` characters. Returns the text and
/// whether it was cut short: a clipboard can hold hundreds of megabytes, and
/// building that string to then trim it is the whole cost.
fn clipboard_text_capped(max: usize) -> Result<(String, bool)> {
    unsafe {
        OpenClipboard(None).map_err(|error| Error::Other(format!("clipboard busy: {error}")))?;
        let result = (|| {
            let handle = GetClipboardData(CF_UNICODETEXT)
                .map_err(|error| Error::Other(format!("clipboard read: {error}")))?;
            let pointer = GlobalLock(HGLOBAL(handle.0)) as *const u16;
            if pointer.is_null() {
                return Err(Error::Other("clipboard text was locked".into()));
            }
            let mut length = 0usize;
            while length <= max && *pointer.add(length) != 0 {
                length += 1;
            }
            let truncated = length > max;
            let length = length.min(max);
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            let _ = GlobalUnlock(HGLOBAL(handle.0));
            Ok((text, truncated))
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Whether the clipboard holds anything at all. Used to check that a restore
/// actually landed rather than assuming it did.
fn clipboard_holds_something() -> bool {
    clipboard_text().is_ok()
        || unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_ok()
        || unsafe { IsClipboardFormatAvailable(CF_DIB) }.is_ok()
        || unsafe { IsClipboardFormatAvailable(CF_BITMAP) }.is_ok()
}

fn write_bytes(format: u32, bytes: &[u8], utf16_terminator: bool) -> Result<()> {
    unsafe {
        OpenClipboard(None).map_err(|error| Error::Other(format!("clipboard busy: {error}")))?;
        let result = (|| {
            EmptyClipboard().map_err(|error| Error::Other(format!("clipboard clear: {error}")))?;
            let size = bytes.len() + if utf16_terminator { 2 } else { 0 };
            let memory = GlobalAlloc(GMEM_MOVEABLE, size.max(1))
                .map_err(|error| Error::Other(format!("clipboard alloc: {error}")))?;
            let pointer = GlobalLock(memory) as *mut u8;
            if pointer.is_null() {
                return Err(Error::Other("clipboard memory was locked".into()));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
            if utf16_terminator {
                *(pointer.add(bytes.len()) as *mut u16) = 0;
            }
            let _ = GlobalUnlock(memory);
            SetClipboardData(format, Some(HANDLE(memory.0)))
                .map_err(|error| Error::Other(format!("clipboard set: {error}")))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

fn write_text(text: &str) -> Result<()> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let bytes = unsafe {
        std::slice::from_raw_parts(units.as_ptr() as *const u8, units.len() * 2)
    };
    write_bytes(CF_UNICODETEXT, bytes, true)
}

fn write_files(paths: &[String]) -> Result<()> {
    // DROPFILES header + a double-null-terminated wide path list.
    let mut file_list: Vec<u16> = Vec::new();
    for path in paths {
        file_list.extend(path.encode_utf16());
        file_list.push(0);
    }
    file_list.push(0);

    #[repr(C)]
    struct DropFiles {
        p_files: u32,
        pt: POINT,
        f_nc: BOOL,
        f_wide: BOOL,
    }
    let header = DropFiles {
        p_files: std::mem::size_of::<DropFiles>() as u32,
        pt: POINT { x: 0, y: 0 },
        f_nc: FALSE,
        f_wide: TRUE,
    };
    let header_bytes = unsafe {
        std::slice::from_raw_parts(&header as *const DropFiles as *const u8, std::mem::size_of::<DropFiles>())
    };
    let list_bytes = unsafe {
        std::slice::from_raw_parts(file_list.as_ptr() as *const u8, file_list.len() * 2)
    };
    let mut payload = Vec::with_capacity(header_bytes.len() + list_bytes.len());
    payload.extend_from_slice(header_bytes);
    payload.extend_from_slice(list_bytes);
    write_bytes(CF_HDROP, &payload, false)
}

pub fn clipboard(arguments: &Value, read_chars: usize) -> Result<String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("clipboard needs an action".into()))?;
    match action {
        "read" => {
            if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_ok() {
                let (text, truncated) = clipboard_text_capped(read_chars)?;
                let text = if truncated {
                    format!("{text}… [truncated at {read_chars} characters]")
                } else {
                    text
                };
                Ok(format!("clipboard text:\n{text}"))
            } else if unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_ok() {
                Ok("clipboard holds files, not text".to_string())
            } else {
                Ok("clipboard is empty (or holds a format Loom cannot read)".to_string())
            }
        }
        "write" => {
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Other("clipboard write needs text".into()))?;
            write_text(text)?;
            Ok(format!("clipboard now holds {} characters", text.chars().count()))
        }
        "write_files" => {
            let paths: Vec<String> = arguments
                .get("paths")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if paths.is_empty() || paths.len() > 50 {
                return Err(Error::Other(
                    "clipboard write_files needs 1-50 paths".into(),
                ));
            }
            for path in &paths {
                if !std::path::Path::new(path).exists() {
                    return Err(Error::Other(format!("\"{path}\" does not exist")));
                }
            }
            write_files(&paths)?;
            Ok(format!(
                "clipboard now holds {} file(s); focus the target window and press ctrl+v",
                paths.len()
            ))
        }
        other => Err(Error::Other(format!("unknown clipboard action: {other}"))),
    }
}

// ------------------------------------------------------------------
// windows
// ------------------------------------------------------------------

unsafe extern "system" fn list_window_cb(hwnd: HWND, data: LPARAM) -> BOOL {
    let (include_hidden, filter, rows) = &mut *(data.0 as *mut (bool, Option<String>, Vec<String>));
    let visible = IsWindowVisible(hwnd).as_bool();
    if !visible && !*include_hidden {
        return TRUE;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == GetCurrentProcessId() {
        return TRUE;
    }
    let title = window_title(hwnd);
    if title.is_empty() {
        return TRUE;
    }
    if let Some(filter) = filter {
        if !title.to_lowercase().contains(&filter.to_lowercase()) {
            return TRUE;
        }
    }
    let rect = window_rect(hwnd).unwrap_or((0, 0, 0, 0));
    let focused = GetForegroundWindow() == hwnd;
    let minimized = IsIconic(hwnd).as_bool();
    rows.push(format!(
        "hwnd=0x{:08X} pid={pid} focused={focused} min={minimized} rect=({},{},{}x{}) \"{}\"",
        hwnd.0 as usize, rect.0, rect.1, rect.2, rect.3, title
    ));
    TRUE
}

pub fn list_windows(arguments: &Value) -> Result<String> {
    let filter = arguments
        .get("filter")
        .and_then(Value::as_str)
        .map(str::to_string);
    let include_hidden = arguments
        .get("include_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut context: (bool, Option<String>, Vec<String>) = (include_hidden, filter, Vec::new());
    unsafe {
        EnumWindows(
            Some(list_window_cb),
            LPARAM(&mut context as *mut (bool, Option<String>, Vec<String>) as isize),
        )
        .map_err(|error| Error::Other(format!("window enumeration failed: {error}")))?;
    }
    let mut rows = context.2;
    if rows.is_empty() {
        return Ok("(no matching windows)".to_string());
    }
    let truncated = rows.len() > 100;
    rows.truncate(100);
    if truncated {
        rows.push("… more windows not shown".to_string());
    }
    Ok(rows.join("\n"))
}

pub fn focus_window_handle(hwnd: isize) -> Result<String> {
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    focus(hwnd)
}

fn focus(hwnd: HWND) -> Result<String> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        if GetForegroundWindow() != hwnd {
            // The foreground lock: borrow the current thread's input queue,
            // which is how taskbars and launchers legitimately steal focus.
            let foreground = GetForegroundWindow();
            let target_thread = GetWindowThreadProcessId(foreground, None);
            let current = GetCurrentThreadId();
            if target_thread != current {
                let _ = AttachThreadInput(current, target_thread, true);
                let _ = SetForegroundWindow(hwnd);
                let _ = AttachThreadInput(current, target_thread, false);
            }
        }
        if GetForegroundWindow() != hwnd {
            return Err(Error::Other(
                "Windows refused to focus that window (it may be elevated or the foreground \
                 lock is active)"
                    .into(),
            ));
        }
        Ok(format!(
            "focused \"{}\" (hwnd 0x{:08X})",
            window_title(hwnd),
            hwnd.0 as usize
        ))
    }
}

pub fn window(arguments: &Value) -> Result<String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("window needs an action".into()))?;
    let spec = arguments
        .get("window")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("window needs a window (hwnd or title)".into()))?;
    let hwnd = resolve_window(spec)?;
    let title = window_title(hwnd);

    unsafe {
        match action {
            "focus" => focus(hwnd),
            "close" => {
                PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))
                    .map_err(|error| Error::Other(format!("close failed: {error}")))?;
                Ok(format!("asked \"{title}\" to close"))
            }
            "minimize" => {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
                Ok(format!("minimised \"{title}\""))
            }
            "maximize" => {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
                Ok(format!("maximised \"{title}\""))
            }
            "restore" => {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                Ok(format!("restored \"{title}\""))
            }
            "move" | "resize" => {
                let mut rect = RECT::default();
                GetWindowRect(hwnd, &mut rect)
                    .map_err(|error| Error::Other(format!("window rect: {error}")))?;
                let x = arguments
                    .get("x")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32);
                let y = arguments
                    .get("y")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32);
                let width = arguments
                    .get("width")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32);
                let height = arguments
                    .get("height")
                    .and_then(Value::as_i64)
                    .map(|value| value as i32);
                let mut flags = SWP_NOACTIVATE;
                if x.is_none() && y.is_none() {
                    flags |= SWP_NOMOVE;
                }
                if width.is_none() && height.is_none() {
                    flags |= SWP_NOSIZE;
                }
                let new_x = x.unwrap_or(rect.left);
                let new_y = y.unwrap_or(rect.top);
                let new_w = width.unwrap_or(rect.right - rect.left);
                let new_h = height.unwrap_or(rect.bottom - rect.top);
                SetWindowPos(hwnd, None, new_x, new_y, new_w.max(1), new_h.max(1), flags)
                    .map_err(|error| Error::Other(format!("window move failed: {error}")))?;
                Ok(format!(
                    "{action}d \"{title}\" to ({new_x},{new_y}) {}x{}",
                    new_w.max(1),
                    new_h.max(1)
                ))
            }
            "pin" => {
                let pin = arguments
                    .get("pin")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                let insert_after = if pin { HWND_TOPMOST } else { HWND_NOTOPMOST };
                SetWindowPos(
                    hwnd,
                    Some(insert_after),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
                .map_err(|error| Error::Other(format!("pin failed: {error}")))?;
                Ok(format!(
                    "{} \"{title}\"",
                    if pin { "pinned" } else { "unpinned" }
                ))
            }
            other => Err(Error::Other(format!("unknown window action: {other}"))),
        }
    }
}

// ------------------------------------------------------------------
// processes
// ------------------------------------------------------------------

const PROTECTED: [&str; 16] = [
    "system",
    "registry",
    "memory compression",
    "smss",
    "csrss",
    "wininit",
    "services",
    "lsass",
    "winlogon",
    "dwm",
    "svchost",
    "explorer",
    "audiodg",
    "fontdrvhost",
    "sihost",
    "loom",
];

fn process_snapshot() -> Result<Vec<(u32, String, u64)>> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .map_err(|error| Error::Other(format!("process snapshot: {error}")))?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut rows = Vec::new();
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|unit| *unit == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                let mut memory = 0u64;
                if let Ok(process) =
                    OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, entry.th32ProcessID)
                {
                    let mut counters = PROCESS_MEMORY_COUNTERS {
                        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
                        ..Default::default()
                    };
                    if GetProcessMemoryInfo(process, &mut counters, counters.cb).is_ok() {
                        memory = counters.WorkingSetSize as u64;
                    }
                    let _ = CloseHandle(process);
                }
                rows.push((entry.th32ProcessID, name, memory));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        Ok(rows)
    }
}

pub fn list_processes(arguments: &Value) -> Result<String> {
    let filter = arguments
        .get("filter")
        .and_then(Value::as_str)
        .map(|value| value.to_lowercase());
    let with_windows_only = arguments
        .get("with_windows_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let window_pids = window_pids()?;
    let mut rows: Vec<(u64, u32, String)> = process_snapshot()?
        .into_iter()
        .filter(|(pid, name, _)| {
            if with_windows_only && !window_pids.contains(pid) {
                return false;
            }
            match &filter {
                Some(filter) => name.to_lowercase().contains(filter),
                None => true,
            }
        })
        .map(|(pid, name, memory)| (memory, pid, name))
        .collect();
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    if rows.is_empty() {
        return Ok("(no matching processes)".to_string());
    }
    let truncated = rows.len() > 100;
    rows.truncate(100);
    let text = rows
        .iter()
        .map(|(memory, pid, name)| {
            format!("{name} pid={pid} mem={}MB", memory / (1024 * 1024))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(if truncated {
        format!("{text}\n… more processes not shown")
    } else {
        text
    })
}

fn window_pids() -> Result<Vec<u32>> {
    let mut pids: Vec<u32> = Vec::new();
    unsafe {
        EnumWindows(
            Some(collect_pid),
            LPARAM(&mut pids as *mut Vec<u32> as isize),
        )
        .map_err(|error| Error::Other(format!("window enumeration failed: {error}")))?;
    }
    Ok(pids)
}

unsafe extern "system" fn collect_pid(hwnd: HWND, data: LPARAM) -> BOOL {
    if IsWindowVisible(hwnd).as_bool() {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (*(data.0 as *mut Vec<u32>)).push(pid);
    }
    TRUE
}

pub fn launch_app(arguments: &Value) -> Result<String> {
    let target = arguments
        .get("target")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("launch_app needs a target".into()))?;
    let args: Vec<String> = arguments
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let cwd = arguments
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::to_string);
    let wait_for = arguments
        .get("wait_for_window")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timeout_ms = arguments
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(10_000)
        .min(30_000);

    let file = wide(target);
    let parameters = if args.is_empty() {
        None
    } else {
        Some(wide(&args.join(" ")))
    };
    let directory = cwd.as_deref().map(wide);

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: parameters
            .as_ref()
            .map(|value| PCWSTR(value.as_ptr()))
            .unwrap_or(PCWSTR::null()),
        lpDirectory: directory
            .as_ref()
            .map(|value| PCWSTR(value.as_ptr()))
            .unwrap_or(PCWSTR::null()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut info)
            .map_err(|error| Error::Other(format!("could not open \"{target}\": {error}")))?;
    }

    let mut pid = 0u32;
    if !info.hProcess.is_invalid() {
        pid = unsafe { windows::Win32::System::Threading::GetProcessId(info.hProcess) };
        let _ = unsafe { CloseHandle(info.hProcess) };
    }

    let mut text = format!("launched \"{target}\"");
    if pid != 0 {
        text.push_str(&format!(" (pid {pid})"));
    }
    if let Some(title) = wait_for {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if let Some(hwnd) = find_window_impl(&title)? {
                text.push_str(&format!(
                    "; window \"{}\" appeared (hwnd 0x{:08X})",
                    window_title(hwnd),
                    hwnd.0 as usize
                ));
                break;
            }
            if std::time::Instant::now() >= deadline {
                text.push_str(&format!("; no window matching \"{title}\" appeared"));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    Ok(text)
}

pub fn kill_process(arguments: &Value) -> Result<String> {
    let pid = arguments.get("pid").and_then(Value::as_i64).map(|v| v as u32);
    let name = arguments.get("name").and_then(Value::as_str);

    let target_pid = match (pid, name) {
        (Some(pid), _) => pid,
        (None, Some(name)) => {
            let wanted = name
                .trim()
                .to_ascii_lowercase()
                .trim_end_matches(".exe")
                .to_string();
            let matches: Vec<(u32, String)> = process_snapshot()?
                .into_iter()
                .filter(|(_, process, _)| {
                    process
                        .to_ascii_lowercase()
                        .trim_end_matches(".exe")
                        == wanted
                })
                .map(|(pid, process, _)| (pid, process))
                .collect();
            match matches.len() {
                0 => return Err(Error::Other(format!("no process named \"{name}\""))),
                1 => matches[0].0,
                _ => {
                    return Err(Error::Other(format!(
                        "\"{name}\" matches {} processes ({}); give a pid instead",
                        matches.len(),
                        matches
                            .iter()
                            .map(|(pid, _)| pid.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )))
                }
            }
        }
        (None, None) => return Err(Error::Other("kill_process needs a pid or name".into())),
    };

    if target_pid == unsafe { GetCurrentProcessId() } {
        return Err(Error::Other("refusing to kill Loom itself".into()));
    }
    let process_name = process_snapshot()?
        .into_iter()
        .find(|(pid, _, _)| *pid == target_pid)
        .map(|(_, name, _)| name)
        .unwrap_or_default();
    let bare = process_name.to_ascii_lowercase().trim_end_matches(".exe").to_string();
    if PROTECTED.contains(&bare.as_str()) {
        return Err(Error::Other(format!(
            "refusing to kill \"{process_name}\": it is a protected system process"
        )));
    }

    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE, false, target_pid)
            .map_err(|error| Error::Other(format!("could not open pid {target_pid}: {error}")))?;
        let result = TerminateProcess(process, 1);
        let _ = CloseHandle(process);
        result.map_err(|error| Error::Other(format!("could not terminate: {error}")))?;
    }
    Ok(format!(
        "terminated {} (pid {target_pid})",
        if process_name.is_empty() {
            "process".to_string()
        } else {
            process_name
        }
    ))
}

// ------------------------------------------------------------------
// UI Automation
// ------------------------------------------------------------------

fn control_type_name(id: i32) -> &'static str {
    match id {
        50000 => "Button",
        50001 => "Calendar",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50009 => "Menu",
        50010 => "MenuBar",
        50011 => "MenuItem",
        50012 => "ProgressBar",
        50013 => "RadioButton",
        50014 => "ScrollBar",
        50015 => "Slider",
        50016 => "Spinner",
        50017 => "StatusBar",
        50018 => "Tab",
        50019 => "TabItem",
        50020 => "Text",
        50021 => "ToolBar",
        50022 => "ToolTip",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "Custom",
        50026 => "Group",
        50027 => "Thumb",
        50028 => "DataGrid",
        50029 => "DataItem",
        50030 => "Document",
        50031 => "SplitButton",
        50032 => "Window",
        50033 => "Pane",
        50034 => "Header",
        _ => "Element",
    }
}

fn uia() -> Result<windows::Win32::UI::Accessibility::IUIAutomation> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| Error::Other(format!("UI Automation unavailable: {error}")))
    }
}

fn element_patterns(element: &IUIAutomationElement) -> Vec<&'static str> {
    let mut patterns = Vec::new();
    if unsafe { element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        .is_ok()
    {
        patterns.push("invoke");
    }
    if unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .is_ok()
    {
        patterns.push("value");
    }
    if unsafe { element.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) }
        .is_ok()
    {
        patterns.push("toggle");
    }
    if unsafe {
        element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
    }
    .is_ok()
    {
        patterns.push("select");
    }
    if unsafe {
        element.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
            UIA_ExpandCollapsePatternId,
        )
    }
    .is_ok()
    {
        patterns.push("expand");
    }
    patterns
}

fn element_text(element: &IUIAutomationElement) -> String {
    if let Ok(pattern) =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
    {
        if let Ok(value) = unsafe { pattern.CurrentValue() } {
            return value.to_string();
        }
    }
    unsafe { element.CurrentName() }
        .map(|name| name.to_string())
        .unwrap_or_default()
}

fn walk(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    path: &mut Vec<u32>,
    depth: u32,
    max_depth: u32,
    max_nodes: usize,
    nodes: &mut Vec<UiNode>,
) {
    if depth > max_depth || nodes.len() >= max_nodes {
        return;
    }
    let mut index = 0u32;
    let mut child = unsafe { walker.GetFirstChildElement(element) }.ok();
    while let Some(current) = child {
        if nodes.len() >= max_nodes {
            return;
        }
        path.push(index);
        let id = path
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join(".");
        let name = unsafe { current.CurrentName() }
            .map(|value| value.to_string())
            .unwrap_or_default();
        let control = unsafe { current.CurrentControlType() }
            .map(|value| value.0)
            .unwrap_or(0);
        let bounds = unsafe { current.CurrentBoundingRectangle() }
            .map(|rect| {
                (
                    rect.left,
                    rect.top,
                    (rect.right - rect.left).unsigned_abs(),
                    (rect.bottom - rect.top).unsigned_abs(),
                )
            })
            .unwrap_or((0, 0, 0, 0));
        let enabled = unsafe { current.CurrentIsEnabled() }
            .map(|value| value.as_bool())
            .unwrap_or(true);
        let offscreen = unsafe { current.CurrentIsOffscreen() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        nodes.push(UiNode {
            id,
            path: path.clone(),
            name,
            role: control_type_name(control).to_string(),
            bounds,
            enabled,
            offscreen,
            patterns: element_patterns(&current)
                .into_iter()
                .map(str::to_string)
                .collect(),
        });
        walk(
            walker,
            &current,
            path,
            depth + 1,
            max_depth,
            max_nodes,
            nodes,
        );
        path.pop();
        index += 1;
        child = unsafe { walker.GetNextSiblingElement(&current) }.ok();
    }
}

pub fn ui_tree(
    window: Option<&str>,
    depth: u32,
    max_nodes: usize,
) -> Result<(isize, Vec<UiNode>)> {
    let hwnd = match window {
        Some(spec) => resolve_window(spec)?,
        None => unsafe { GetForegroundWindow() },
    };
    if hwnd.0.is_null() {
        return Err(Error::Other("no window is focused".into()));
    }
    let automation = uia()?;
    let root = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|error| Error::Other(format!("no accessibility tree: {error}")))?;
    let walker = unsafe { automation.ControlViewWalker() }
        .map_err(|error| Error::Other(format!("no control view: {error}")))?;

    // The root itself is worth showing; children walk under it.
    let root_name = unsafe { root.CurrentName() }
        .map(|value| value.to_string())
        .unwrap_or_default();
    let root_control = unsafe { root.CurrentControlType() }
        .map(|value| value.0)
        .unwrap_or(0);
    let mut nodes = vec![UiNode {
        id: "root".to_string(),
        path: Vec::new(),
        name: root_name,
        role: control_type_name(root_control).to_string(),
        bounds: window_rect(hwnd).unwrap_or((0, 0, 0, 0)),
        enabled: true,
        offscreen: false,
        patterns: element_patterns(&root)
            .into_iter()
            .map(str::to_string)
            .collect(),
    }];
    let mut path = Vec::new();
    walk(&walker, &root, &mut path, 1, depth, max_nodes, &mut nodes);
    Ok((hwnd.0 as isize, nodes))
}

fn element_at(
    automation: &windows::Win32::UI::Accessibility::IUIAutomation,
    hwnd: HWND,
    path: &[u32],
) -> Result<IUIAutomationElement> {
    let root = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|error| Error::Other(format!("no accessibility tree: {error}")))?;
    let walker = unsafe { automation.ControlViewWalker() }
        .map_err(|error| Error::Other(format!("no control view: {error}")))?;
    let mut current = root;
    for index in path {
        let mut child = unsafe { walker.GetFirstChildElement(&current) }
            .map_err(|error| Error::Other(format!("element moved: {error}")))?;
        for _ in 0..*index {
            child = unsafe { walker.GetNextSiblingElement(&child) }
                .map_err(|error| Error::Other(format!("element moved: {error}")))?;
        }
        current = child;
    }
    Ok(current)
}

pub fn ui_act(
    hwnd: isize,
    path: &[u32],
    action: &str,
    value: Option<&str>,
    expected: Option<&str>,
) -> Result<String> {
    if hwnd == 0 {
        return Err(Error::Other("run `ui tree` again".into()));
    }
    let automation = uia()?;
    let element = element_at(&automation, HWND(hwnd as *mut core::ffi::c_void), path)?;
    let name = unsafe { element.CurrentName() }
        .map(|value| value.to_string())
        .unwrap_or_default();

    // The path is a position, not an identity: the window may have been
    // rebuilt since the tree was read, in which case this element is a
    // different one and acting on it would press the wrong button.
    if let Some(expected) = expected {
        if name != expected {
            return Err(Error::Other(format!(
                "the element at that id is now \"{name}\", not \"{expected}\"; run `ui tree` \
                 again"
            )));
        }
    }

    match action {
        "invoke" => {
            let pattern =
                unsafe { element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
                    .map_err(|_| Error::Other(format!("\"{name}\" cannot be invoked")))?;
            unsafe { pattern.Invoke() }
                .map_err(|error| Error::Other(format!("invoke failed: {error}")))?;
            Ok(format!("invoked \"{name}\""))
        }
        "set_value" => {
            let value = value.ok_or_else(|| Error::Other("set_value needs a value".into()))?;
            let pattern =
                unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
                    .map_err(|_| Error::Other(format!("\"{name}\" has no value pattern")))?;
            unsafe { pattern.SetValue(&BSTR::from(value)) }
                .map_err(|error| Error::Other(format!("set_value failed: {error}")))?;
            Ok(format!("set \"{name}\" to \"{value}\""))
        }
        "toggle" => {
            let pattern =
                unsafe { element.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) }
                    .map_err(|_| Error::Other(format!("\"{name}\" cannot be toggled")))?;
            unsafe { pattern.Toggle() }
                .map_err(|error| Error::Other(format!("toggle failed: {error}")))?;
            Ok(format!("toggled \"{name}\""))
        }
        "select" => {
            let pattern = unsafe {
                element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                    UIA_SelectionItemPatternId,
                )
            }
            .map_err(|_| Error::Other(format!("\"{name}\" cannot be selected")))?;
            unsafe { pattern.Select() }
                .map_err(|error| Error::Other(format!("select failed: {error}")))?;
            Ok(format!("selected \"{name}\""))
        }
        "expand" | "collapse" => {
            let pattern = unsafe {
                element.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                    UIA_ExpandCollapsePatternId,
                )
            }
            .map_err(|_| Error::Other(format!("\"{name}\" cannot expand or collapse")))?;
            if action == "expand" {
                unsafe { pattern.Expand() }
            } else {
                unsafe { pattern.Collapse() }
            }
            .map_err(|error| Error::Other(format!("{action} failed: {error}")))?;
            Ok(format!("{action}ed \"{name}\""))
        }
        "focus" => {
            unsafe { element.SetFocus() }
                .map_err(|error| Error::Other(format!("focus failed: {error}")))?;
            Ok(format!("focused \"{name}\""))
        }
        "get_text" => Ok(format!("text of \"{name}\": {}", element_text(&element))),
        other => Err(Error::Other(format!("unknown ui action: {other}"))),
    }
}

// ------------------------------------------------------------------
// takeover watch
// ------------------------------------------------------------------

/// The hooks are installed and working.
const HOOKS_INSTALLED: u8 = 1;
/// Neither hook could be installed: takeover detection is off.
const HOOKS_UNAVAILABLE: u8 = 2;

/// What the low-level hooks report, shared with the app.
#[derive(Default)]
struct HookShared {
    /// Whether real input counts right now (a computer turn is running).
    armed: AtomicBool,
    /// Set by a hook, consumed by [`TakeoverWatch::take_trip`]. Edge-triggered
    /// on purpose: a level flag that nothing cleared is why every resume was
    /// undone 150 ms later.
    trip: AtomicBool,
    /// When real input last arrived (epoch ms), movement included.
    last_input_ms: AtomicU64,
    /// Id of the hook thread, once it is running.
    thread: AtomicU32,
    /// [`HOOKS_INSTALLED`], [`HOOKS_UNAVAILABLE`], or 0 while the hook thread
    /// has not tried yet. The distinction matters: "not yet tried" is not a
    /// failure and must not be reported as one.
    hooks: AtomicU8,
}

static WATCH: OnceLock<Arc<HookShared>> = OnceLock::new();

/// Windows that are Loom's own. Kept apart from the hook state so the main
/// window can register itself before any computer turn has ever run.
static OWN_WINDOWS: OnceLock<RwLock<Vec<isize>>> = OnceLock::new();

fn own_windows() -> &'static RwLock<Vec<isize>> {
    OWN_WINDOWS.get_or_init(|| RwLock::new(Vec::new()))
}

/// True when `hwnd` is one of Loom's own windows, or a child of one.
fn is_own_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    let Ok(list) = own_windows().read() else {
        return false;
    };
    if list.is_empty() {
        return false;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    list.iter()
        .any(|own| *own == hwnd.0 as isize || *own == root.0 as isize)
}

/// Watches for real (non-injected) mouse and keyboard input while a computer
/// turn runs, so the user touching the machine can pause it.
pub struct TakeoverWatch {
    shared: Arc<HookShared>,
    stop: Arc<AtomicBool>,
}

impl TakeoverWatch {
    /// Arms the watch, installing the hooks on first use.
    ///
    /// Returns an error when the hooks cannot be installed. Silently carrying
    /// on would leave takeover protection off with no sign of it, which is
    /// exactly the sort of quiet failure that makes the user distrust the
    /// thing; the caller records the reason and the pill shows it.
    pub fn start() -> Result<Self> {
        let shared = WATCH.get_or_init(|| {
            let shared = Arc::new(HookShared::default());
            spawn_hook_thread(Arc::clone(&shared));
            shared
        });

        // The hook thread installs asynchronously, so wait briefly the first
        // time to find out whether it worked.
        for _ in 0..50 {
            match shared.hooks.load(Ordering::Acquire) {
                HOOKS_INSTALLED => break,
                HOOKS_UNAVAILABLE => {
                    return Err(Error::Other(
                        "the low-level input hooks could not be installed, so Loom cannot tell \
                         when you take the machine back"
                            .into(),
                    ))
                }
                _ => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
        if shared.hooks.load(Ordering::Acquire) != HOOKS_INSTALLED {
            return Err(Error::Other(
                "the input hooks did not install in time".into(),
            ));
        }

        shared.armed.store(true, Ordering::Relaxed);
        shared.trip.store(false, Ordering::Relaxed);
        shared.last_input_ms.store(0, Ordering::Relaxed);
        Ok(Self {
            shared: Arc::clone(shared),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Consumes a pending takeover: true at most once per real input event.
    /// Consuming rather than sampling is what makes a single click produce a
    /// single pause.
    pub fn take_trip(&self) -> bool {
        self.shared.trip.swap(false, Ordering::SeqCst)
    }

    /// Drops a pending trip without pausing. Resume calls this, so the click
    /// that asked for it cannot be read back as a fresh takeover.
    pub fn clear_trip(&self) {
        self.shared.trip.store(false, Ordering::SeqCst);
    }

    pub fn idle_ms(&self) -> u64 {
        let last = self.shared.last_input_ms.load(Ordering::Relaxed);
        if last == 0 {
            return 0;
        }
        now_ms().saturating_sub(last)
    }

    /// Starts the idle clock here. Used when a pause begins, so the countdown
    /// to auto-resume measures silence since the pause, not since the last
    /// keystroke of a turn that has been running a while.
    pub fn mark_input_now(&self) {
        self.shared
            .last_input_ms
            .store(now_ms(), Ordering::Relaxed);
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop)
    }

    pub fn stop(&self) {
        self.shared.armed.store(false, Ordering::Relaxed);
        self.shared.trip.store(false, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Registers a window as Loom's own: input over it is the user driving Loom,
/// not taking the machine over. Called for every window Loom shows — the main
/// window, the quick-ask overlay, and the control pill.
///
/// Additive on purpose. The old version stored a single hwnd, so clicking
/// anywhere in Loom's main window counted as a takeover.
pub fn set_ignored_window(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    if let Ok(mut list) = own_windows().write() {
        if !list.contains(&hwnd) {
            list.push(hwnd);
        }
    }
}

/// Marks a window as never taking focus. The control pill is a button the user
/// presses while Loom is driving another app; clicking it must not steal focus
/// from that app, or the key the model sends next goes somewhere else.
pub fn make_window_non_activating(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let window = HWND(hwnd as *mut core::ffi::c_void);
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            window,
            GWL_EXSTYLE,
            (style | WS_EX_NOACTIVATE.0) as isize,
        );
    }
}

/// Trips the latch exactly as a real key press does, for the debug-only
/// command the probe uses. Does nothing when no computer turn is watching.
pub fn trip_takeover_for_debug() {
    if let Some(shared) = WATCH.get() {
        if shared.armed.load(Ordering::Relaxed) {
            shared.trip.store(true, Ordering::SeqCst);
        }
    }
}

/// Removes both hooks when the thread that owns them ends.
struct HookGuard {
    mouse: HHOOK,
    keyboard: HHOOK,
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.mouse);
            let _ = UnhookWindowsHookEx(self.keyboard);
        }
    }
}

fn spawn_hook_thread(shared: Arc<HookShared>) {
    std::thread::spawn(move || unsafe {
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0).ok();
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0).ok();
        let (mouse, keyboard) = match (mouse, keyboard) {
            (Some(mouse), Some(keyboard)) => (mouse, keyboard),
            _ => {
                // One hook without the other is not protection. Drop the one
                // that worked rather than leaking it, and say so.
                if let Some(hook) = mouse {
                    let _ = UnhookWindowsHookEx(hook);
                }
                if let Some(hook) = keyboard {
                    let _ = UnhookWindowsHookEx(hook);
                }
                shared.hooks.store(HOOKS_UNAVAILABLE, Ordering::Release);
                eprintln!("[loom] input hooks are unavailable; takeover detection is off");
                return;
            }
        };
        shared.hooks.store(HOOKS_INSTALLED, Ordering::Release);
        shared.thread.store(GetCurrentThreadId(), Ordering::Relaxed);
        let _guard = HookGuard { mouse, keyboard };

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        // The thread only ends when the message loop does; the guard unhooks.
    });
}

/// Whether a point is over one of Loom's own windows.
fn over_own_window(point: POINT) -> bool {
    is_own_window(unsafe { WindowFromPoint(point) })
}

/// Records a real input event. Movement only refreshes the idle clock; a
/// deliberate act trips the latch, which the bridge consumes, so one physical
/// event produces exactly one pause.
fn note_real_input(shared: &HookShared, kind: super::InputKind, injected: bool, over_own: bool) {
    if injected {
        return;
    }
    shared.last_input_ms.store(now_ms(), Ordering::Relaxed);
    if super::takeover_input(kind, false, over_own) {
        shared.trip.store(true, Ordering::Relaxed);
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(shared) = WATCH.get() {
            if shared.armed.load(Ordering::Relaxed) {
                let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                let injected = info.flags & LLMHF_INJECTED != 0;
                // Everything except bare movement is a deliberate act: button
                // down and up, a wheel notch, a horizontal wheel. Movement only
                // refreshes the idle clock, because reaching for the Resume
                // button is itself movement.
                let kind = if wparam.0 as u32 == WM_MOUSEMOVE {
                    super::InputKind::Move
                } else {
                    super::InputKind::Press
                };
                note_real_input(shared, kind, injected, over_own_window(info.pt));
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(shared) = WATCH.get() {
            if shared.armed.load(Ordering::Relaxed) {
                let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                let injected = info.flags.0 & LLKHF_INJECTED.0 != 0;
                // A key event carries no point: the window it will reach is
                // whichever one has focus.
                let own = is_own_window(GetForegroundWindow());
                note_real_input(shared, super::InputKind::Press, injected, own);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
