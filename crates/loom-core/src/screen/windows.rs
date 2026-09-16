//! Windows capture: DXGI Desktop Duplication.
//!
//! The duplication session is created once per monitor and kept, so a capture
//! is a GPU copy plus a map — tens of milliseconds, not the hundreds a fresh
//! session costs. DXGI only offers a frame when the desktop changed; when it
//! has not, the previous frame is still the truth, so it is reused.
//!
//! The overlay marks itself `WDA_EXCLUDEFROMCAPTURE`, which removes it from
//! this capture path entirely: the shot shows what is *under* the window.

use std::sync::Mutex;

use image::RgbaImage;
use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, POINT, RECT, TRUE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION, DXGI_MODE_ROTATION_IDENTITY,
    DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270, DXGI_MODE_ROTATION_ROTATE90,
    DXGI_MODE_ROTATION_UNSPECIFIED, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC,
    GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, ReleaseDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, GetWindowRect, SetWindowDisplayAffinity, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WDA_EXCLUDEFROMCAPTURE,
};

use super::Monitor;
use crate::{Error, Result};

struct Duplicator {
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    /// Surface size before rotation, and how the display rotates it.
    width: u32,
    height: u32,
    rotation: DXGI_MODE_ROTATION,
    staging: Option<ID3D11Texture2D>,
    /// Reused when DXGI reports that nothing changed.
    last: Option<RgbaImage>,
    /// The monitor's rectangle in virtual-desktop coordinates.
    desktop: RECT,
}

static ACTIVE: Mutex<Option<Duplicator>> = Mutex::new(None);

pub fn capture_at(x: i32, y: i32) -> Result<RgbaImage> {
    let mut guard = ACTIVE
        .lock()
        .map_err(|_| Error::Other("screen capture is busy".into()))?;

    if guard
        .as_ref()
        .is_some_and(|duplicator| !contains(duplicator.desktop, x, y))
    {
        *guard = None;
    }

    if let Some(duplicator) = guard.as_mut() {
        match duplicator.grab() {
            Ok(frame) => return Ok(frame),
            // Sessions die on display-mode changes and driver resets; drop it
            // and build a fresh one below.
            Err(_) => *guard = None,
        }
    }

    let mut duplicator = Duplicator::create(x, y)?;
    let frame = duplicator.grab()?;
    *guard = Some(duplicator);
    Ok(frame)
}

/// Hides a window from every capture API, including this one.
pub fn exclude_from_capture(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe {
        SetWindowDisplayAffinity(HWND(hwnd as *mut core::ffi::c_void), WDA_EXCLUDEFROMCAPTURE)
            .is_ok()
    }
}

impl Duplicator {
    fn create(x: i32, y: i32) -> Result<Self> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()
                .map_err(|error| Error::Other(format!("DXGI is unavailable: {error}")))?;

            let mut adapter_index = 0;
            loop {
                let Ok(adapter) = factory.EnumAdapters(adapter_index) else {
                    break;
                };
                adapter_index += 1;

                let mut output_index = 0;
                loop {
                    let Ok(output) = adapter.EnumOutputs(output_index) else {
                        break;
                    };
                    output_index += 1;

                    let Ok(description) = output.GetDesc() else {
                        continue;
                    };
                    if !description.AttachedToDesktop.as_bool()
                        || !contains(description.DesktopCoordinates, x, y)
                    {
                        continue;
                    }

                    // The device must live on the adapter that drives the
                    // monitor, or duplication fails on hybrid-GPU machines.
                    let mut device = None;
                    let mut context = None;
                    D3D11CreateDevice(
                        Some(&adapter),
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        None,
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut context),
                    )
                    .map_err(|error| Error::Other(format!("D3D11 device: {error}")))?;

                    let device =
                        device.ok_or_else(|| Error::Other("D3D11 returned no device".into()))?;
                    let context =
                        context.ok_or_else(|| Error::Other("D3D11 returned no context".into()))?;
                    let output1: IDXGIOutput1 = output.cast().map_err(interface_error)?;
                    let duplication = output1
                        .DuplicateOutput(&device)
                        .map_err(|error| Error::Other(format!("desktop duplication: {error}")))?;
                    let duplicated = duplication.GetDesc();

                    return Ok(Self {
                        context,
                        duplication,
                        width: duplicated.ModeDesc.Width,
                        height: duplicated.ModeDesc.Height,
                        rotation: duplicated.Rotation,
                        staging: None,
                        last: None,
                        desktop: description.DesktopCoordinates,
                    });
                }
            }
        }

        Err(Error::Other(format!("no monitor contains ({x}, {y})")))
    }

    fn grab(&mut self) -> Result<RgbaImage> {
        // The compositor only hands over a frame when the desktop changes, and
        // a fresh session starts with no image at all. Probe first, then wait
        // a beat, then a little longer, before giving up.
        const WAITS: [u32; 3] = [0, 100, 400];
        let mut attempt = 0;

        loop {
            let timeout = WAITS[attempt];
            unsafe {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource: Option<IDXGIResource> = None;

                match self
                    .duplication
                    .AcquireNextFrame(timeout, &mut info, &mut resource)
                {
                    Ok(()) => {
                        // A frame without a present time carries only a
                        // pointer update; its desktop image is undefined —
                        // black on a fresh session. The previous frame is
                        // still the truth, so reuse it.
                        if !is_desktop_update(&info) {
                            let _ = self.duplication.ReleaseFrame();
                            if let Some(frame) = &self.last {
                                return Ok(frame.clone());
                            }
                        } else {
                            let frame = self.read_frame(resource);
                            let _ = self.duplication.ReleaseFrame();
                            let frame = frame?;

                            // A present can still hand over an uninitialised
                            // surface (all black) right after the session
                            // starts or the display wakes. Do not keep it.
                            if self.last.is_some() || !is_blank(&frame) {
                                self.last = Some(frame.clone());
                                return Ok(frame);
                            }
                        }
                    }
                    Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                        if let Some(frame) = &self.last {
                            return Ok(frame.clone());
                        }
                    }
                    Err(error) => {
                        return Err(Error::Other(format!("screen capture failed: {error}")));
                    }
                }
            }

            attempt += 1;
            if attempt >= WAITS.len() {
                return Err(Error::Other("the screen produced no frame".into()));
            }
        }
    }

    fn read_frame(&mut self, resource: Option<IDXGIResource>) -> Result<RgbaImage> {
        let resource = resource.ok_or_else(|| Error::Other("capture returned no frame".into()))?;
        let texture: ID3D11Texture2D = resource.cast().map_err(interface_error)?;
        let staging = self.staging_texture()?;

        unsafe {
            self.context.CopyResource(&staging, &texture);

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|error| Error::Other(format!("could not read the frame: {error}")))?;
            let pixels = self.to_rgba(&mapped);
            self.context.Unmap(&staging, 0);
            pixels
        }
    }

    fn staging_texture(&mut self) -> Result<ID3D11Texture2D> {
        if let Some(texture) = &self.staging {
            return Ok(texture.clone());
        }

        let description = D3D11_TEXTURE2D_DESC {
            Width: self.width,
            Height: self.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };

        unsafe {
            let device = self
                .context
                .GetDevice()
                .map_err(|error| Error::Other(format!("D3D11 device: {error}")))?;
            let mut texture = None;
            device
                .CreateTexture2D(&description, None, Some(&mut texture))
                .map_err(|error| Error::Other(format!("D3D11 staging texture: {error}")))?;
            let texture =
                texture.ok_or_else(|| Error::Other("D3D11 returned no texture".into()))?;
            self.staging = Some(texture.clone());
            Ok(texture)
        }
    }

    /// Copies the mapped surface into an RGBA image, undoing the BGRA layout
    /// and the display rotation, so callers see the desktop as the user does.
    fn to_rgba(&self, mapped: &D3D11_MAPPED_SUBRESOURCE) -> Result<RgbaImage> {
        let width = self.width as usize;
        let height = self.height as usize;
        let pitch = mapped.RowPitch as usize;

        if mapped.pData.is_null() || pitch < width * 4 {
            return Err(Error::Other("the captured frame was empty".into()));
        }

        let mut image = RgbaImage::new(self.width, self.height);
        let source =
            unsafe { std::slice::from_raw_parts(mapped.pData as *const u8, pitch * height) };

        for (row, target) in image.rows_mut().enumerate() {
            let start = row * pitch;
            let row_bytes = &source[start..start + width * 4];
            for (pixel, chunk) in target.zip(row_bytes.chunks_exact(4)) {
                pixel[0] = chunk[2];
                pixel[1] = chunk[1];
                pixel[2] = chunk[0];
                pixel[3] = 255;
            }
        }

        Ok(rotate(image, self.rotation))
    }
}

/// True when a frame actually carries a desktop image. A frame with no present
/// time is a pointer-only update, and its surface contents are undefined.
fn is_desktop_update(info: &DXGI_OUTDUPL_FRAME_INFO) -> bool {
    info.LastPresentTime != 0
}

/// True when a sampled grid of the frame is pure black all the way down: the
/// signature of an uninitialised surface, not of a dark desktop.
fn is_blank(image: &RgbaImage) -> bool {
    let step_x = (image.width() / 64).max(1) as usize;
    let step_y = (image.height() / 64).max(1) as usize;

    for y in (0..image.height()).step_by(step_y) {
        for x in (0..image.width()).step_by(step_x) {
            let pixel = image.get_pixel(x, y);
            if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                return false;
            }
        }
    }
    true
}

fn rotate(image: RgbaImage, rotation: DXGI_MODE_ROTATION) -> RgbaImage {
    if rotation == DXGI_MODE_ROTATION_ROTATE90 {
        image::imageops::rotate90(&image)
    } else if rotation == DXGI_MODE_ROTATION_ROTATE180 {
        image::imageops::rotate180(&image)
    } else if rotation == DXGI_MODE_ROTATION_ROTATE270 {
        image::imageops::rotate270(&image)
    } else if rotation == DXGI_MODE_ROTATION_IDENTITY || rotation == DXGI_MODE_ROTATION_UNSPECIFIED
    {
        image
    } else {
        image
    }
}

fn contains(rect: RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

fn interface_error(error: windows::core::Error) -> Error {
    Error::Other(format!("unexpected capture interface: {error}"))
}

// ------------------------------------------------------------------
// Computer-use capture: monitors, the whole desktop, one window, the cursor
// ------------------------------------------------------------------

thread_local! {
    static COLLECTED: std::cell::RefCell<Vec<Monitor>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// `MONITORINFOF_PRIMARY`; not exposed as a constant by windows-rs.
const MONITORINFOF_PRIMARY: u32 = 0x0000_0001;

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    _data: LPARAM,
) -> BOOL {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let bounds = info.rcMonitor;
        COLLECTED.with(|list| {
            list.borrow_mut().push(Monitor {
                index: 0,
                rect: (
                    bounds.left,
                    bounds.top,
                    (bounds.right - bounds.left).unsigned_abs(),
                    (bounds.bottom - bounds.top).unsigned_abs(),
                ),
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
                dpi: dpi_x,
            });
        });
    }
    TRUE
}

pub fn monitors() -> Result<Vec<Monitor>> {
    COLLECTED.with(|list| list.borrow_mut().clear());
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(0));
    }
    let mut list = COLLECTED.with(|list| list.borrow().clone());
    if list.is_empty() {
        return Err(Error::Other("no monitors were found".into()));
    }
    list.sort_by_key(|monitor| (monitor.rect.0, monitor.rect.1));
    for (index, monitor) in list.iter_mut().enumerate() {
        monitor.index = index as i32;
    }
    Ok(list)
}

pub fn desktop_rect() -> (i32, i32, u32, u32) {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).unsigned_abs().max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).unsigned_abs().max(1);
        (x, y, width, height)
    }
}

pub fn capture_monitor(index: i32) -> Result<RgbaImage> {
    let list = monitors()?;
    let monitor = list.get(index as usize).ok_or_else(|| {
        Error::Other(format!(
            "there is no monitor {index}; this machine has {}",
            list.len()
        ))
    })?;
    // A point two pixels in avoids the exact edge, where DXGI can disagree
    // about which output owns the pixel.
    capture_at(monitor.rect.0 + 2, monitor.rect.1 + 2)
}

pub fn capture_virtual() -> Result<RgbaImage> {
    let (virtual_x, virtual_y, virtual_w, virtual_h) = desktop_rect();
    let mut canvas = RgbaImage::new(virtual_w, virtual_h);
    for monitor in monitors()? {
        let frame = capture_monitor(monitor.index)?;
        image::imageops::overlay(
            &mut canvas,
            &frame,
            i64::from(monitor.rect.0 - virtual_x),
            i64::from(monitor.rect.1 - virtual_y),
        );
    }
    Ok(canvas)
}

/// `PW_RENDERFULLCONTENT`: capture DirectComposition surfaces (browsers,
/// Electron apps) instead of the black rectangle the old flag returns.
const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;

pub fn capture_window(hwnd: isize) -> Result<RgbaImage> {
    if hwnd == 0 {
        return Err(Error::Other("no window to capture".into()));
    }
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).map_err(|error| Error::Other(format!("window rect: {error}")))?;
        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;

        let screen_dc = GetDC(None);
        let memory_dc = CreateCompatibleDC(Some(screen_dc));
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };

        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(memory_dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|error| Error::Other(format!("capture bitmap: {error}")))?;
        let previous = SelectObject(memory_dc, bitmap.into());
        let drawn = PrintWindow(hwnd, memory_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT));
        let _ = SelectObject(memory_dc, previous);

        let result = if drawn.as_bool() && !bits.is_null() {
            let source =
                std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
            let mut image = RgbaImage::new(width, height);
            for (pixel, chunk) in image.pixels_mut().zip(source.chunks_exact(4)) {
                pixel[0] = chunk[2];
                pixel[1] = chunk[1];
                pixel[2] = chunk[0];
                pixel[3] = 255;
            }
            Ok(image)
        } else {
            Err(Error::Other(
                "window capture failed; the window may be protected".into(),
            ))
        };

        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_dc);
        ReleaseDC(None, screen_dc);
        result
    }
}

pub fn cursor_screen_pos() -> Result<(i32, i32)> {
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point).map_err(|error| Error::Other(format!("cursor: {error}")))?;
    }
    Ok((point.x, point.y))
}

/// The monitor whose rect contains a point, for `active` capture. Prefers the
/// monitor of a window when one is given.
pub fn monitor_index_for_window(hwnd: isize) -> Option<i32> {
    let list = monitors().ok()?;
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    let target = unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            (info.rcMonitor.left, info.rcMonitor.top)
        } else {
            return None;
        }
    };
    list.iter()
        .find(|monitor| (monitor.rect.0, monitor.rect.1) == target)
        .map(|monitor| monitor.index)
}

pub fn monitor_index_for_point(x: i32, y: i32) -> Option<i32> {
    let list = monitors().ok()?;
    list.iter()
        .find(|monitor| {
            let (mx, my, mw, mh) = monitor.rect;
            x >= mx && x < mx + mw as i32 && y >= my && y < my + mh as i32
        })
        .map(|monitor| monitor.index)
        .or_else(|| {
            let point = POINT { x, y };
            let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
                let target = (info.rcMonitor.left, info.rcMonitor.top);
                list.iter()
                    .find(|monitor| (monitor.rect.0, monitor.rect.1) == target)
                    .map(|monitor| monitor.index)
            } else {
                None
            }
        })
}
