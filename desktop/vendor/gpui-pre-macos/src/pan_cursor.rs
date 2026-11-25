//! loom: the cursors of a browser's middle-button scrolling, which macOS hasn't got. They
//! are drawn here as Windows draws its own (user32's pan cursors): on a 32-point square,
//! black triangles and a dot, each with a 1-point white edge. Each is an image with a
//! drawing at 1× and at 2×, so it's sharp on any display, and made once.

use std::sync::atomic::{AtomicPtr, Ordering};

use gpui::PanCursor;
use objc2::{class, msg_send, runtime::AnyObject, runtime::Bool};
use objc2_foundation::{NSInteger, NSPoint, NSSize, NSString, NSUInteger};

/// The side of a cursor's square, in points.
const SIDE: f32 = 32.0;

/// How wide the white edge is, in points.
const EDGE: f32 = 1.0;

/// The samples a pixel is made of, each way.
const SAMPLES: usize = 4;

/// How far the point (`x`, `y`) is from the black of `pan`, in points: less than 0 inside.
fn distance(pan: PanCursor, x: f32, y: f32) -> f32 {
    let p = (x, y);
    let up = triangle(p, (16.0, 3.5), (8.5, 11.0), (23.5, 11.0));
    let down = triangle(p, (16.0, 28.5), (23.5, 21.0), (8.5, 21.0));
    let dot = ((x - 16.0).powi(2) + (y - 16.0).powi(2)).sqrt() - 2.0;
    match pan {
        PanCursor::UpDown => up.min(down).min(dot),
        PanCursor::Up => up.min(dot),
        PanCursor::Down => down.min(dot),
    }
}

/// How far `p` is from the triangle `a`, `b`, `c`: less than 0 inside.
fn triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let edge = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
    };
    let side = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
    let (s1, s2, s3) = (side(a, b), side(b, c), side(c, a));
    let inside = (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0);
    let d = edge(a, b).min(edge(b, c)).min(edge(c, a));
    if inside { -d } else { d }
}

/// `pan` at `scale` pixels a point, as RGBA without the colour multiplied by the alpha,
/// the top row first.
pub(crate) fn pixels(pan: PanCursor, scale: usize) -> Vec<u8> {
    let n = SIDE as usize * scale;
    let mut out = Vec::with_capacity(n * n * 4);
    let step = 1.0 / (SAMPLES * scale) as f32;
    for row in 0..n {
        for col in 0..n {
            let (mut black, mut any) = (0usize, 0usize);
            for i in 0..SAMPLES {
                for j in 0..SAMPLES {
                    let x = (col * SAMPLES + j) as f32 * step + step / 2.0;
                    let y = (row * SAMPLES + i) as f32 * step + step / 2.0;
                    let d = distance(pan, x, y);
                    black += usize::from(d <= 0.0);
                    any += usize::from(d <= EDGE);
                }
            }
            let grey = if any == 0 {
                0
            } else {
                (255.0 * (1.0 - black as f32 / any as f32)).round() as u8
            };
            let alpha = (255.0 * any as f32 / (SAMPLES * SAMPLES) as f32).round() as u8;
            out.extend_from_slice(&[grey, grey, grey, alpha]);
        }
    }
    out
}

/// The `NSCursor` for `pan`, made the first time it's asked for and kept.
///
/// # Safety
///
/// Called on the AppKit main thread.
pub(crate) unsafe fn ns_cursor(pan: PanCursor) -> *mut AnyObject {
    static CURSORS: [AtomicPtr<AnyObject>; 3] = [
        AtomicPtr::new(std::ptr::null_mut()),
        AtomicPtr::new(std::ptr::null_mut()),
        AtomicPtr::new(std::ptr::null_mut()),
    ];
    let slot = &CURSORS[match pan {
        PanCursor::UpDown => 0,
        PanCursor::Up => 1,
        PanCursor::Down => 2,
    }];
    let kept = slot.load(Ordering::Acquire);
    if !kept.is_null() {
        return kept;
    }
    // SAFETY: on the main thread, as the caller guarantees; each object is made and used
    // here, and the cursor is kept for as long as the app runs.
    unsafe {
        let side = NSSize::new(SIDE as f64, SIDE as f64);
        let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
        let image: *mut AnyObject = msg_send![image, initWithSize: side];
        let rgb = NSString::from_str("NSDeviceRGBColorSpace");
        for scale in [1usize, 2] {
            let n = (SIDE as usize * scale) as NSInteger;
            let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
            let planes: *mut *mut u8 = std::ptr::null_mut();
            // NSBitmapFormatAlphaNonpremultiplied.
            let format: NSUInteger = 1 << 1;
            let rep: *mut AnyObject = msg_send![
                rep,
                initWithBitmapDataPlanes: planes,
                pixelsWide: n,
                pixelsHigh: n,
                bitsPerSample: 8 as NSInteger,
                samplesPerPixel: 4 as NSInteger,
                hasAlpha: Bool::YES,
                isPlanar: Bool::NO,
                colorSpaceName: &*rgb,
                bitmapFormat: format,
                bytesPerRow: n * 4,
                bitsPerPixel: 32 as NSInteger
            ];
            if rep.is_null() {
                continue;
            }
            let data: *mut u8 = msg_send![rep, bitmapData];
            let drawn = pixels(pan, scale);
            if !data.is_null() {
                std::ptr::copy_nonoverlapping(drawn.as_ptr(), data, drawn.len());
            }
            let _: () = msg_send![rep, setSize: side];
            let _: () = msg_send![image, addRepresentation: rep];
            let _: () = msg_send![rep, release];
        }
        let cursor: *mut AnyObject = msg_send![class!(NSCursor), alloc];
        let hot = NSPoint::new(SIDE as f64 / 2.0, SIDE as f64 / 2.0);
        let cursor: *mut AnyObject = msg_send![cursor, initWithImage: image, hotSpot: hot];
        let _: () = msg_send![image, release];
        slot.store(cursor, Ordering::Release);
        cursor
    }
}

#[cfg(test)]
mod tests {
    use super::{SIDE, pixels};
    use gpui::PanCursor;

    /// The pixel at (`x`, `y`) of `p`, `n` wide: its grey and alpha.
    fn at(p: &[u8], n: usize, x: usize, y: usize) -> (u8, u8) {
        let i = (y * n + x) * 4;
        (p[i], p[i + 3])
    }

    #[test]
    fn its_drawn_as_windows_draws_it() {
        let n = SIDE as usize;
        let p = pixels(PanCursor::UpDown, 1);
        assert_eq!(p.len(), n * n * 4);
        assert_eq!(at(&p, n, 16, 8), (0, 255), "the up arrow is black");
        assert_eq!(at(&p, n, 16, 24), (0, 255), "the down arrow is black");
        assert_eq!(at(&p, n, 16, 16).0, 0, "the dot is black");
        assert_eq!(
            at(&p, n, 16, 11),
            (255, 255),
            "a white edge under the arrow"
        );
        assert_eq!(at(&p, n, 2, 2).1, 0, "clear around it");
        let up = pixels(PanCursor::Up, 2);
        assert_eq!(
            at(&up, n * 2, 32, 48).1,
            0,
            "going up, there's no down arrow"
        );
        assert_eq!(at(&up, n * 2, 32, 16), (0, 255));
    }
}
