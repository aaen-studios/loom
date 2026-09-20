//! Screen capture for the quick-ask overlay.
//!
//! The overlay marks itself as excluded from capture, so the shot shows the
//! desktop underneath without hiding the window. On Windows that exclusion is
//! `WDA_EXCLUDEFROMCAPTURE`; on other platforms the caller hides the overlay
//! instead.
//!
//! The image is downscaled before it is stored: providers re-resize anything
//! larger, and the extra pixels only inflate the base64 payload sent on every
//! turn.

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::{Error, Result};

/// Longest edge handed to the model. Matches the common vision-model sweet
/// spot; anything larger is discarded detail.
const MAX_EDGE: u32 = 1568;
/// A screenshot at this size or above switches to JPEG to stay inside the
/// attachment budget.
const PNG_BUDGET: usize = 6 * 1024 * 1024;

/// A prepared screenshot: bytes plus the name it should be stored under.
pub struct Shot {
    pub bytes: Vec<u8>,
    pub name: String,
    /// Dimensions of the encoded image, i.e. after downscaling.
    pub width: u32,
    pub height: u32,
}

/// Captures the monitor containing the physical point `(x, y)`.
pub fn capture_at(x: i32, y: i32) -> Result<Shot> {
    prepare(imp::capture_at(x, y)?)
}

/// One display, in virtual-desktop coordinates.
#[derive(Debug, Clone)]
pub struct Monitor {
    pub index: i32,
    pub rect: (i32, i32, u32, u32),
    pub primary: bool,
    pub dpi: u32,
}

/// Every attached display, ordered left to right.
pub fn monitors() -> Result<Vec<Monitor>> {
    imp::monitors()
}

/// The whole virtual desktop: x, y, width, height.
pub fn desktop_rect() -> (i32, i32, u32, u32) {
    imp::desktop_rect()
}

/// Captures one monitor by index (from [`monitors`]).
pub fn capture_monitor(index: i32) -> Result<image::RgbaImage> {
    imp::capture_monitor(index)
}

/// Captures every monitor stitched into one virtual-desktop image.
pub fn capture_virtual() -> Result<image::RgbaImage> {
    imp::capture_virtual()
}

/// Captures a single window (Windows: `PrintWindow`).
pub fn capture_window(hwnd: isize) -> Result<image::RgbaImage> {
    imp::capture_window(hwnd)
}

/// The pointer's position in virtual-desktop coordinates.
pub fn cursor_screen_pos() -> Result<(i32, i32)> {
    imp::cursor_screen_pos()
}

/// The monitor index that owns the window, when one can be determined.
pub fn monitor_index_for_window(hwnd: isize) -> Option<i32> {
    imp::monitor_index_for_window(hwnd)
}

/// The monitor index containing a virtual-desktop point.
pub fn monitor_index_for_point(x: i32, y: i32) -> Option<i32> {
    imp::monitor_index_for_point(x, y)
}

/// Marks a window as invisible to screen capture (Windows only). Returns
/// `false` when the platform or the OS build cannot honour it, which tells the
/// caller to hide the window for the shot.
pub fn exclude_from_capture(hwnd: isize) -> bool {
    imp::exclude_from_capture(hwnd)
}

/// Downscales and encodes a raw frame, naming it for the current time.
pub fn prepare(image: RgbaImage) -> Result<Shot> {
    prepare_with_edge(image, MAX_EDGE)
}

/// Like [`prepare`], but with an explicit longest edge. `0` keeps the frame at
/// native resolution (computer use defaults to this, quality first); the
/// quick-ask overlay keeps the provider-honoured 1568.
pub fn prepare_with_edge(image: RgbaImage, edge: u32) -> Result<Shot> {
    let prepared = downscale(DynamicImage::ImageRgba8(image), edge);
    let width = prepared.width();
    let height = prepared.height();
    let png = encode_png(&prepared)?;
    if png.len() <= PNG_BUDGET {
        return Ok(Shot {
            bytes: png,
            name: format!("Screen {}.png", timestamp()),
            width,
            height,
        });
    }

    let jpeg = encode(&prepared, ImageFormat::Jpeg)?;
    Ok(Shot {
        bytes: jpeg,
        name: format!("Screen {}.jpg", timestamp()),
        width,
        height,
    })
}

/// Prepares a screenshot that arrived **already encoded** — a browser tab
/// capture, rather than one of the raw desktop frames above.
///
/// It is decoded and re-encoded rather than passed through, and that is the
/// point: a browser screenshot then goes down exactly the same
/// downscale-and-encode path as a desktop one, so `chat.browserScreenshotEdge`
/// means the same thing as `chat.computerScreenshotEdge` and a large capture
/// falls back to JPEG by the same rule. Two sources of pixels, one set of
/// decisions about their size and format — rather than a second policy to keep
/// in step.
pub fn prepare_encoded(bytes: &[u8], edge: u32, name: &str) -> Result<Shot> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| Error::Other(format!("could not decode the captured image: {error}")))?;
    let mut shot = prepare_with_edge(decoded.to_rgba8(), edge)?;
    shot.name = name.to_string();
    Ok(shot)
}

/// Scales the longest edge down to `edge`, preserving the aspect ratio.
/// `edge` of 0 means no scaling at all.
fn downscale(image: DynamicImage, edge: u32) -> DynamicImage {
    if edge == 0 {
        return image;
    }
    let (width, height) = (image.width(), image.height());
    let longest = width.max(height);
    if longest <= edge {
        return image;
    }

    let scale = f64::from(edge) / f64::from(longest);
    let target_width = ((f64::from(width) * scale).round() as u32).max(1);
    let target_height = ((f64::from(height) * scale).round() as u32).max(1);
    image.resize_exact(target_width, target_height, FilterType::Triangle)
}

/// Fast PNG: screenshots are taken in the middle of a send, so encode time
/// matters more than the last few kilobytes. Alpha is dropped — the desktop
/// is opaque and the planar RGB compresses better.
fn encode_png(image: &DynamicImage) -> Result<Vec<u8>> {
    use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
    use image::{ExtendedColorType, ImageEncoder};

    let rgb = image.to_rgb8();
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, PngFilter::Sub)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| Error::Other(format!("could not encode screenshot: {error}")))?;
    Ok(bytes)
}

fn encode(image: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    image
        .write_to(&mut cursor, format)
        .map_err(|error| Error::Other(format!("could not encode screenshot: {error}")))?;
    Ok(bytes)
}

/// `2026-09-15 1340` in UTC. A file name only needs to sort usefully.
fn timestamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let (year, month, day, hour, minute) = civil_from_unix(seconds);
    format!("{year:04}-{month:02}-{day:02} {hour:02}{minute:02}")
}

/// Days-to-civil conversion (Howard Hinnant's `civil_from_days`), so the
/// timestamp costs no dependency.
fn civil_from_unix(seconds: u64) -> (i64, i64, i64, u64, u64) {
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;

    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };

    (
        if month <= 2 { year + 1 } else { year },
        month,
        day,
        remainder / 3_600,
        (remainder % 3_600) / 60,
    )
}

/// Windows: DXGI Desktop Duplication, cached between captures.
#[cfg(windows)]
#[path = "screen/windows.rs"]
mod imp;
/// Everywhere else: xcap.
#[cfg(not(windows))]
#[path = "screen/fallback.rs"]
mod imp;

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba([20, 40, 60, 255]))
    }

    #[test]
    fn downscales_the_longest_edge_only() {
        let scaled = downscale(DynamicImage::ImageRgba8(frame(4000, 2000)), MAX_EDGE);
        assert_eq!((scaled.width(), scaled.height()), (1568, 784));
    }

    #[test]
    fn leaves_small_frames_untouched() {
        let scaled = downscale(DynamicImage::ImageRgba8(frame(800, 600)), MAX_EDGE);
        assert_eq!((scaled.width(), scaled.height()), (800, 600));
    }

    #[test]
    fn native_keeps_every_pixel() {
        let scaled = downscale(DynamicImage::ImageRgba8(frame(4000, 2000)), 0);
        assert_eq!((scaled.width(), scaled.height()), (4000, 2000));
    }

    #[test]
    fn prepares_a_png_with_a_readable_name() {
        let shot = prepare(frame(16, 16)).unwrap();
        assert!(shot.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(shot.name.starts_with("Screen "));
        assert!(shot.name.ends_with(".png"));
        assert_eq!((shot.width, shot.height), (16, 16));
    }

    #[test]
    fn civil_dates_land_on_known_days() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0));
        assert_eq!(civil_from_unix(1_700_000_000), (2023, 11, 14, 22, 13));
        assert_eq!(civil_from_unix(1_767_225_600), (2026, 1, 1, 0, 0));
    }
}
