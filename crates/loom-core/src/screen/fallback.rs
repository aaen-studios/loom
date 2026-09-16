//! Non-Windows capture via xcap. There is no `WDA_EXCLUDEFROMCAPTURE`
//! counterpart here, so the caller hides the overlay before this runs.

use image::RgbaImage;

use super::Monitor;
use crate::{Error, Result};

pub fn capture_at(x: i32, y: i32) -> Result<RgbaImage> {
    let monitor = xcap::Monitor::from_point(x, y)
        .map_err(|error| Error::Other(format!("no monitor at ({x}, {y}): {error}")))?;
    monitor
        .capture_image()
        .map_err(|error| Error::Other(format!("screen capture failed: {error}")))
}

pub fn exclude_from_capture(_hwnd: isize) -> bool {
    false
}

fn monitors_impl() -> Result<Vec<Monitor>> {
    let mut list: Vec<Monitor> = xcap::Monitor::all()
        .map_err(|error| Error::Other(format!("monitor enumeration failed: {error}")))?
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| Monitor {
            index: index as i32,
            rect: (
                monitor.x(),
                monitor.y(),
                monitor.width(),
                monitor.height(),
            ),
            primary: monitor.is_primary(),
            dpi: (monitor.scale_factor() * 96.0).round() as u32,
        })
        .collect();
    if list.is_empty() {
        return Err(Error::Other("no monitors were found".into()));
    }
    list.sort_by_key(|monitor| (monitor.rect.0, monitor.rect.1));
    for (index, monitor) in list.iter_mut().enumerate() {
        monitor.index = index as i32;
    }
    Ok(list)
}

pub fn monitors() -> Result<Vec<Monitor>> {
    monitors_impl()
}

pub fn desktop_rect() -> (i32, i32, u32, u32) {
    let Ok(list) = monitors_impl() else {
        return (0, 0, 1, 1);
    };
    let min_x = list.iter().map(|monitor| monitor.rect.0).min().unwrap_or(0);
    let min_y = list.iter().map(|monitor| monitor.rect.1).min().unwrap_or(0);
    let max_x = list
        .iter()
        .map(|monitor| monitor.rect.0 + monitor.rect.2 as i32)
        .max()
        .unwrap_or(1);
    let max_y = list
        .iter()
        .map(|monitor| monitor.rect.1 + monitor.rect.3 as i32)
        .max()
        .unwrap_or(1);
    (
        min_x,
        min_y,
        (max_x - min_x).unsigned_abs().max(1),
        (max_y - min_y).unsigned_abs().max(1),
    )
}

pub fn capture_monitor(index: i32) -> Result<RgbaImage> {
    let list = monitors_impl()?;
    let monitor = list
        .get(index as usize)
        .ok_or_else(|| Error::Other(format!("there is no monitor {index}")))?;
    capture_at(monitor.rect.0 + 2, monitor.rect.1 + 2)
}

pub fn capture_virtual() -> Result<RgbaImage> {
    let (virtual_x, virtual_y, virtual_w, virtual_h) = desktop_rect();
    let mut canvas = RgbaImage::new(virtual_w, virtual_h);
    for monitor in monitors_impl()? {
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

pub fn capture_window(_hwnd: isize) -> Result<RgbaImage> {
    Err(Error::Other(
        "window capture is Windows-only in this build".into(),
    ))
}

pub fn cursor_screen_pos() -> Result<(i32, i32)> {
    Err(Error::Other(
        "cursor position is Windows-only in this build".into(),
    ))
}

pub fn monitor_index_for_window(_hwnd: isize) -> Option<i32> {
    None
}

pub fn monitor_index_for_point(x: i32, y: i32) -> Option<i32> {
    monitors_impl()
        .ok()?
        .into_iter()
        .find(|monitor| {
            let (mx, my, mw, mh) = monitor.rect;
            x >= mx && x < mx + mw as i32 && y >= my && y < my + mh as i32
        })
        .map(|monitor| monitor.index)
}
