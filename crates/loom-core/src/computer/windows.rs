//! Windows implementation of computer use.
//!
//! Everything here is `SendInput` and Win32 reads/writes; no new dependencies.
//! Coordinates arriving from [`super::mouse`] are already virtual-desktop
//! pixels. Input is checked against the foreground window's elevation so a
//! click into an elevated app fails loudly instead of silently vanishing
//! (UIPI).

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
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
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    SetClipboardData,
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
    CallNextHookEx, DispatchMessageW, EnumWindows, GetAncestor, GetForegroundWindow, GetMessageW,
    GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, PostMessageW, SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage,
    WindowFromPoint, GA_ROOT, HWND_NOTOPMOST, HWND_TOPMOST, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
    LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, SetWindowsHookExW, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_CLOSE,
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

fn own_input_target_blocked() -> Result<()> {
    if let Some(foreground) = foreground_window() {
        if window_is_elevated(foreground) {
            return Err(Error::Other(
                "that window is elevated; Loom must run elevated to control it".into(),
            ));
        }
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
            move_absolute(x, y);
            Ok(format!("moved to screen ({x},{y})"))
        }
        "click" | "double_click" | "right_click" | "middle_click" => {
            let button = match action {
                "right_click" => "right",
                "middle_click" => "middle",
                _ => button,
            };
            let clicks = if action == "double_click" { 2 } else { count };
            own_input_target_blocked()?;
            move_absolute(x, y);
            with_modifiers(modifiers, || click(button, clicks))?;
            Ok(format!(
                "{} {} at screen ({x},{y})",
                if clicks > 1 { "double-clicked" } else { "clicked" },
                button
            ))
        }
        "down" => {
            own_input_target_blocked()?;
            move_absolute(x, y);
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
            own_input_target_blocked()?;
            move_absolute(x, y);
            send(&[mouse_input(0, 0, 0, button_flags(button, false))]);
            let steps = steps.max(1);
            for step in 1..=steps {
                let progress = f64::from(step) / f64::from(steps);
                let ix = x + ((tx - x) as f64 * progress).round() as i32;
                let iy = y + ((ty - y) as f64 * progress).round() as i32;
                move_absolute(ix, iy);
                if duration_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(u64::from(
                        duration_ms / steps,
                    )));
                }
            }
            send(&[mouse_input(0, 0, 0, button_flags(button, true))]);
            Ok(format!("dragged {button} from ({x},{y}) to ({tx},{ty})"))
        }
        "scroll" => {
            let notches = amount.clamp(-100, 100);
            if notches == 0 {
                return Err(Error::Other("scroll needs a non-zero amount".into()));
            }
            let data = (notches * 120) as u32;
            let flag = if horizontal {
                MOUSEEVENTF_HWHEEL
            } else {
                MOUSEEVENTF_WHEEL
            };
            send(&[mouse_input(0, 0, data, flag)]);
            Ok(format!(
                "scrolled {} {notches} notches",
                if horizontal { "horizontally" } else { "vertically" }
            ))
        }
        other => Err(Error::Other(format!("unknown mouse action: {other}"))),
    }
}

pub fn cursor_pos() -> Result<(i32, i32)> {
    screen::cursor_screen_pos()
}

/// Refuses when the focused window is elevated (UIPI would drop the input).
pub fn check_input_target() -> Result<()> {
    own_input_target_blocked()
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
    own_input_target_blocked()?;
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
    own_input_target_blocked()?;
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

pub fn type_via_clipboard(text: &str) -> Result<String> {
    let previous = clipboard_text().ok();
    write_text(text)?;
    std::thread::sleep(std::time::Duration::from_millis(120));
    let pressed = combo(&["ctrl".into(), "v".into()], 1)?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    let restored = match previous {
        Some(previous) => write_text(&previous).is_ok(),
        None => false,
    };
    Ok(format!(
        "typed {} characters via clipboard ({pressed}); previous clipboard {}",
        text.chars().count(),
        if restored { "restored" } else { "not restored" }
    ))
}

// ------------------------------------------------------------------
// clipboard
// ------------------------------------------------------------------

const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;

fn clipboard_text() -> Result<String> {
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
            while *pointer.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length));
            let _ = GlobalUnlock(HGLOBAL(handle.0));
            Ok(text)
        })();
        let _ = CloseClipboard();
        result
    }
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

pub fn clipboard(arguments: &Value) -> Result<String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("clipboard needs an action".into()))?;
    match action {
        "read" => {
            if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) }.is_ok() {
                let mut text = clipboard_text()?;
                if text.chars().count() > 10_000 {
                    text = text.chars().take(10_000).collect::<String>() + "… [truncated]";
                }
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

pub fn ui_act(hwnd: isize, path: &[u32], action: &str, value: Option<&str>) -> Result<String> {
    if hwnd == 0 {
        return Err(Error::Other("run `ui tree` again".into()));
    }
    let automation = uia()?;
    let element = element_at(&automation, HWND(hwnd as *mut core::ffi::c_void), path)?;
    let name = unsafe { element.CurrentName() }
        .map(|value| value.to_string())
        .unwrap_or_default();

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

struct HookShared {
    armed: AtomicBool,
    tripped: AtomicBool,
    last_input_ms: AtomicU64,
    ignored: AtomicI64,
    thread: AtomicU32,
}

static WATCH: OnceLock<Arc<HookShared>> = OnceLock::new();

/// Watches for real (non-injected) mouse and keyboard input while a computer
/// turn runs, so the user touching the machine can pause it.
pub struct TakeoverWatch {
    shared: Arc<HookShared>,
    stop: Arc<AtomicBool>,
}

impl TakeoverWatch {
    pub fn start() -> Result<Self> {
        let shared = WATCH.get_or_init(|| {
            let shared = Arc::new(HookShared {
                armed: AtomicBool::new(false),
                tripped: AtomicBool::new(false),
                last_input_ms: AtomicU64::new(0),
                ignored: AtomicI64::new(0),
                thread: AtomicU32::new(0),
            });
            spawn_hook_thread(Arc::clone(&shared));
            shared
        });
        shared.armed.store(true, Ordering::Relaxed);
        shared.tripped.store(false, Ordering::Relaxed);
        shared.last_input_ms.store(0, Ordering::Relaxed);
        Ok(Self {
            shared: Arc::clone(shared),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn tripped(&self) -> bool {
        self.shared.tripped.load(Ordering::Relaxed)
    }

    pub fn idle_ms(&self) -> u64 {
        let last = self.shared.last_input_ms.load(Ordering::Relaxed);
        if last == 0 {
            return 0;
        }
        now_ms().saturating_sub(last)
    }

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
        self.shared.tripped.store(false, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Marks a window whose input is Loom's own UI (the control pill): clicking
/// Stop must not count as the user taking over.
pub fn set_ignored_window(hwnd: isize) {
    if let Some(shared) = WATCH.get() {
        shared.ignored.store(hwnd as i64, Ordering::Relaxed);
    } else {
        let _ = TakeoverWatch::start();
        if let Some(shared) = WATCH.get() {
            shared.ignored.store(hwnd as i64, Ordering::Relaxed);
        }
    }
}

fn spawn_hook_thread(shared: Arc<HookShared>) {
    std::thread::spawn(move || unsafe {
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), None, 0)
            .ok()
            .map(|hook| hook);
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0)
            .ok()
            .map(|hook| hook);
        if mouse.is_none() || keyboard.is_none() {
            eprintln!("[loom] input hooks are unavailable; takeover detection is off");
            return;
        }
        shared.thread.store(GetCurrentThreadId(), Ordering::Relaxed);

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        // Thread ending: remove the hooks (only reached if the message loop
        // is stopped, which the app does not currently do).
        if let Some(hook) = mouse {
            let _ = windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook);
        }
        if let Some(hook) = keyboard {
            let _ = windows::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook);
        }
    });
}

fn over_ignored_window(point: POINT) -> bool {
    let Some(shared) = WATCH.get() else {
        return false;
    };
    let ignored = shared.ignored.load(Ordering::Relaxed);
    if ignored == 0 {
        return false;
    }
    let window = unsafe { WindowFromPoint(point) };
    if window.0.is_null() {
        return false;
    }
    let root = unsafe { GetAncestor(window, GA_ROOT) };
    root.0 as i64 == ignored || window.0 as i64 == ignored
}

fn note_real_input(shared: &HookShared) {
    if shared.armed.load(Ordering::Relaxed) {
        shared.tripped.store(true, Ordering::Relaxed);
        shared
            .last_input_ms
            .store(now_ms(), Ordering::Relaxed);
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(shared) = WATCH.get() {
            if shared.armed.load(Ordering::Relaxed) {
                let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                let injected = info.flags & LLMHF_INJECTED != 0;
                if !injected && !over_ignored_window(info.pt) {
                    note_real_input(shared);
                }
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
                if !injected {
                    note_real_input(shared);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
