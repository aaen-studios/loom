//! One window, with no frame of Windows' own: rounded corners and a shadow from DWM,
//! everything inside drawn by hand each frame and put on screen with GDI, paced by
//! DwmFlush to the display's refresh. Anywhere that isn't a control drags the window.

use std::time::Instant;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::draw::Canvas;
use crate::install::wide;

/// Where a control is, in logical pixels, and what it does.
#[derive(Clone, Copy)]
pub struct Hit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub id: usize,
}

pub enum Key {
    Enter,
    Escape,
}

pub enum Next {
    Stay,
    Hide,
    Show,
    Quit,
}

/// What the window shows and does.
pub trait Screen {
    /// Draws a frame, `t` seconds after the window opened, with `hover` the control the
    /// pointer is over and `hovers` how far each control's hover has faded in. Gives the
    /// controls, for the pointer.
    fn draw(&mut self, c: &mut Canvas, t: f32, hovers: &[f32], pressed: Option<usize>) -> Vec<Hit>;
    fn click(&mut self, id: usize, t: f32) -> Next;
    fn key(&mut self, key: Key, t: f32) -> Next;
    /// Called each frame, and while hidden, to move along.
    fn tick(&mut self, t: f32) -> Next;
}

pub const W: f32 = 520.0;
pub const H: f32 = 360.0;
const HOVERS: usize = 16;

struct State {
    screen: Box<dyn Screen>,
    canvas: Canvas,
    bgra: Vec<u8>,
    start: Instant,
    hits: Vec<Hit>,
    hover: Option<usize>,
    pressed: Option<usize>,
    hovers: [f32; HOVERS],
    last: f32,
    tracking: bool,
    visible: bool,
    quit: bool,
}

static mut STATE: Option<State> = None;

#[allow(static_mut_refs)]
fn state() -> &'static mut State {
    // SAFETY: the window and its state live on the one thread that runs `run`.
    unsafe { STATE.as_mut().expect("the window's state") }
}

/// Under a test, LOOM_SETUP_NO_FOCUS opens the window off screen and without taking
/// focus, so it never gets in the way of what the person is doing.
fn no_focus() -> bool {
    std::env::var_os("LOOM_SETUP_NO_FOCUS").is_some()
}

/// Whether Windows is set to dark mode for apps, or LOOM_THEME says, as for the app.
pub fn dark_mode() -> bool {
    match std::env::var("LOOM_THEME").as_deref() {
        Ok("dark") => return true,
        Ok("light") => return false,
        _ => {}
    }
    let (k, n) = (
        wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
        wide("AppsUseLightTheme"),
    );
    let mut v = 1u32;
    let mut len = 4u32;
    // SAFETY: four bytes into `v`.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut v as *mut u32 as _,
            &mut len,
        )
    };
    r == 0 && v == 0
}

fn scale_of(hwnd: HWND) -> f32 {
    // SAFETY: any window handle.
    (unsafe { GetDpiForWindow(hwnd) }.max(96)) as f32 / 96.0
}

/// Opens the window, shown at once unless `hidden`, and runs until the screen says quit.
pub fn run(screen: Box<dyn Screen>, hidden: bool, dark: bool) {
    // SAFETY: plain Win32: a window class and a window, and their message loop, on
    // this thread alone.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let inst = GetModuleHandleW(std::ptr::null());
        let class = wide("loom-setup");
        // The light icon, or the dark one in a dark theme (build.rs).
        let id: usize = if dark { 2 } else { 1 };
        let icon = LoadIconW(inst, id as _);
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: inst,
            hIcon: icon,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        // On the monitor the pointer is on, in the middle of its work area.
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let scale = dx.max(96) as f32 / 96.0;
        let (w, h) = ((W * scale) as i32, (H * scale) as i32);
        let wa = mi.rcWork;
        // Under a test the window is drawn off every screen, where PrintWindow still
        // sees it and nobody else does.
        let (x, y) = if no_focus() {
            (-10000, -10000)
        } else {
            (
                wa.left + ((wa.right - wa.left) - w) / 2,
                wa.top + ((wa.bottom - wa.top) - h) / 2,
            )
        };
        STATE = Some(State {
            screen,
            canvas: Canvas::new(w as u32, h as u32, scale),
            bgra: Vec::new(),
            start: Instant::now(),
            hits: Vec::new(),
            hover: None,
            pressed: None,
            hovers: [0.0; HOVERS],
            last: 0.0,
            tracking: false,
            visible: !hidden,
            quit: false,
        });
        let title = wide("loom");
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            x,
            y,
            w,
            h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null(),
        );
        // Rounded like the app's window, with the system's shadow, in the theme's
        // frame colour.
        let round: u32 = 2; // DWMWCP_ROUND
        DwmSetWindowAttribute(
            hwnd,
            33, /* DWMWA_WINDOW_CORNER_PREFERENCE */
            &round as *const u32 as _,
            4,
        );
        let dark_frame: u32 = dark as u32;
        DwmSetWindowAttribute(
            hwnd,
            20, /* DWMWA_USE_IMMERSIVE_DARK_MODE */
            &dark_frame as *const u32 as _,
            4,
        );
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: 1,
            cyBottomHeight: 0,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
        if !hidden {
            ShowWindow(
                hwnd,
                if no_focus() {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_SHOW
                },
            );
        }
        let mut msg: MSG = std::mem::zeroed();
        loop {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let s = state();
            if s.quit {
                DestroyWindow(hwnd);
                continue;
            }
            let t = s.start.elapsed().as_secs_f32();
            let next = s.screen.tick(t);
            act(hwnd, next);
            if state().visible {
                frame(hwnd);
                DwmFlush();
            } else {
                // Hidden: nothing to draw, but the screen still moves along.
                MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 50, QS_ALLINPUT);
            }
        }
    }
}

fn act(hwnd: HWND, next: Next) {
    let s = state();
    // SAFETY: the window this thread made.
    unsafe {
        match next {
            Next::Stay => {}
            Next::Hide => {
                if s.visible {
                    ShowWindow(hwnd, SW_HIDE);
                    s.visible = false;
                }
            }
            Next::Show => {
                if !s.visible {
                    ShowWindow(
                        hwnd,
                        if no_focus() {
                            SW_SHOWNOACTIVATE
                        } else {
                            SW_SHOW
                        },
                    );
                    if !no_focus() {
                        SetForegroundWindow(hwnd);
                    }
                    s.visible = true;
                }
            }
            Next::Quit => s.quit = true,
        }
    }
}

fn frame(hwnd: HWND) {
    let s = state();
    let mut r: RECT = unsafe { std::mem::zeroed() };
    // SAFETY: the window this thread made.
    unsafe { GetClientRect(hwnd, &mut r) };
    let (w, h) = (
        (r.right - r.left).max(1) as u32,
        (r.bottom - r.top).max(1) as u32,
    );
    let scale = scale_of(hwnd);
    if s.canvas.pix.width() != w || s.canvas.pix.height() != h || s.canvas.scale != scale {
        s.canvas = Canvas::new(w, h, scale);
    }
    let t = s.start.elapsed().as_secs_f32();
    let dt = (t - s.last).clamp(0.0, 0.1);
    s.last = t;
    // Hovers fade in and out over about a tenth of a second, as the app's do.
    for (i, v) in s.hovers.iter_mut().enumerate() {
        let target = if s.hover == Some(i) { 1.0 } else { 0.0 };
        let step = dt / 0.12;
        *v = if *v < target {
            (*v + step).min(target)
        } else {
            (*v - step).max(target)
        };
    }
    s.hits = s.screen.draw(&mut s.canvas, t, &s.hovers, s.pressed);
    // tiny-skia gives RGBA; GDI takes BGRA.
    let src = s.canvas.pix.data();
    s.bgra.resize(src.len(), 0);
    for (d, p) in s
        .bgra
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        d[0] = p[2];
        d[1] = p[1];
        d[2] = p[0];
        d[3] = 255;
    }
    // SAFETY: the bitmap's header says its size, and the buffer holds that many pixels.
    unsafe {
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w as i32;
        bi.bmiHeader.biHeight = -(h as i32);
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        let dc = GetDC(hwnd);
        SetDIBitsToDevice(
            dc,
            0,
            0,
            w,
            h,
            0,
            0,
            0,
            h,
            s.bgra.as_ptr() as _,
            &bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(hwnd, dc);
    }
}

fn hit_at(x: f32, y: f32) -> Option<usize> {
    state()
        .hits
        .iter()
        .find(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h)
        .map(|h| h.id)
}

fn client_point(hwnd: HWND, lp: LPARAM, screen: bool) -> (f32, f32) {
    let mut p = POINT {
        x: (lp & 0xffff) as i16 as i32,
        y: ((lp >> 16) & 0xffff) as i16 as i32,
    };
    if screen {
        // SAFETY: converts a point for this window.
        unsafe { ScreenToClient(hwnd, &mut p) };
    }
    let scale = scale_of(hwnd);
    (p.x as f32 / scale, p.y as f32 / scale)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        // No frame of Windows' own: the whole window is ours to draw.
        WM_NCCALCSIZE if wp != 0 => 0,
        WM_NCHITTEST => {
            let (x, y) = client_point(hwnd, lp, true);
            if hit_at(x, y).is_some() {
                HTCLIENT as _
            } else {
                HTCAPTION as _
            }
        }
        WM_SETCURSOR => {
            if (lp & 0xffff) as u32 == HTCLIENT && state().hover.is_some() {
                SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND));
                return 1;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_MOUSEMOVE => {
            let s = state();
            let (x, y) = client_point(hwnd, lp, false);
            s.hover = hit_at(x, y);
            if !s.tracking {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tme);
                s.tracking = true;
            }
            0
        }
        WM_MOUSELEAVE | WM_NCMOUSEMOVE => {
            let s = state();
            s.hover = None;
            s.tracking = false;
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = client_point(hwnd, lp, false);
            state().pressed = hit_at(x, y);
            SetCapture(hwnd);
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            let s = state();
            let (x, y) = client_point(hwnd, lp, false);
            let was = s.pressed.take();
            if let Some(id) = was.filter(|&id| hit_at(x, y) == Some(id)) {
                let t = s.start.elapsed().as_secs_f32();
                let next = s.screen.click(id, t);
                act(hwnd, next);
            }
            0
        }
        WM_KEYDOWN => {
            let key = match wp as u16 {
                VK_RETURN => Some(Key::Enter),
                VK_ESCAPE => Some(Key::Escape),
                _ => None,
            };
            if let Some(k) = key {
                let s = state();
                let t = s.start.elapsed().as_secs_f32();
                let next = s.screen.key(k, t);
                act(hwnd, next);
            }
            0
        }
        WM_DPICHANGED => {
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            if state().visible {
                frame(hwnd);
            }
            0
        }
        WM_CLOSE => {
            let s = state();
            let t = s.start.elapsed().as_secs_f32();
            let next = s.screen.key(Key::Escape, t);
            act(hwnd, next);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
