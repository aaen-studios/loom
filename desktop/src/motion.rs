//! Small motion for controls: a switch that slides, a hover that fades in and out.
//!
//! GPUI's own hover style swaps in a single frame. Here each thing that moves has a key,
//! and `level` says how far it is between off (0) and on (1), easing from wherever it was
//! when it last changed. While anything is still on its way, `moving` says so, and the
//! window asks for another frame.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use gpui_kit::*;

/// How long a hover takes to fade, in seconds.
pub const HOVER: f32 = 0.12;
/// How long a switch takes to slide, in seconds.
pub const SLIDE: f32 = 0.16;

struct Level {
    on: bool,
    from: f32,
    at: Instant,
    secs: f32,
}

thread_local! {
    static LEVELS: RefCell<HashMap<SharedString, Level>> = RefCell::new(HashMap::new());
    static HOVERED: RefCell<HashMap<SharedString, bool>> = RefCell::new(HashMap::new());
    static MOVING: Cell<bool> = const { Cell::new(false) };
    /// The keys drawn this frame. Anything not drawn is forgotten at the end of it, so a
    /// row that went away while the pointer was on it doesn't come back lit.
    static SEEN: RefCell<HashSet<SharedString>> = RefCell::new(HashSet::new());
}

/// Fast at first, gentle at the end.
fn ease(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

fn value(l: &Level) -> f32 {
    let k = ease(l.at.elapsed().as_secs_f32() / l.secs);
    let to = if l.on { 1.0 } else { 0.0 };
    l.from + (to - l.from) * k
}

/// How far the thing under `key` is from off (0) to on (1), taking `secs` to get there
/// from wherever it was when `on` last changed. The first time it's asked, it's there.
pub fn level(key: &SharedString, on: bool, secs: f32) -> f32 {
    SEEN.with(|s| {
        s.borrow_mut().insert(key.clone());
    });
    LEVELS.with(|levels| {
        let mut levels = levels.borrow_mut();
        let l = levels.entry(key.clone()).or_insert(Level {
            on,
            from: if on { 1.0 } else { 0.0 },
            at: Instant::now(),
            secs,
        });
        if l.on != on {
            l.from = value(l);
            l.on = on;
            l.at = Instant::now();
            l.secs = secs;
        }
        let v = value(l);
        if l.at.elapsed().as_secs_f32() < l.secs {
            MOVING.with(|m| m.set(true));
        }
        v
    })
}

/// Whether the pointer is over the thing under `key`.
pub fn hovered(key: &SharedString) -> bool {
    HOVERED.with(|h| h.borrow().get(key).copied().unwrap_or(false))
}

fn set_hovered(key: SharedString, on: bool) {
    HOVERED.with(|h| {
        h.borrow_mut().insert(key, on);
    });
}

/// Whether anything was still moving in the frame just drawn, which the window asks
/// once per frame, at its end: it needs another while it is. What wasn't drawn in it
/// is forgotten.
pub fn moving() -> bool {
    let seen = SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()));
    LEVELS.with(|l| l.borrow_mut().retain(|k, _| seen.contains(k)));
    HOVERED.with(|h| h.borrow_mut().retain(|k, _| seen.contains(k)));
    MOVING.with(|m| m.replace(false))
}

/// `a` to `b`, `k` of the way. A colour with nothing showing takes the other's hue, so
/// fading in from nothing never passes through grey.
pub fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let (mut a, mut b) = (a.to_rgb(), b.to_rgb());
    if a.a == 0.0 {
        a = Rgba { a: 0.0, ..b };
    } else if b.a == 0.0 {
        b = Rgba { a: 0.0, ..a };
    }
    let l = |x: f32, y: f32| x + (y - x) * k;
    Rgba {
        r: l(a.r, b.r),
        g: l(a.g, b.g),
        b: l(a.b, b.b),
        a: l(a.a, b.a),
    }
    .into()
}

/// A hover that fades: the element's background goes from `bg` to `hover_bg`, and its
/// text from the first of `ink` to the second, when given, over `HOVER` seconds each
/// way. It's kept under the element's id.
pub trait SoftHover: Sized {
    /// The background from the first of `bg` to the second, and the text from the
    /// first of `ink` to the second, each when given.
    fn soft(self, bg: Option<(Hsla, Hsla)>, ink: Option<(Hsla, Hsla)>) -> Self;

    fn soft_hover(self, bg: Hsla, hover_bg: Hsla, ink: Option<(Hsla, Hsla)>) -> Self {
        self.soft(Some((bg, hover_bg)), ink)
    }

    /// Only the text fades, to `to` from `from`.
    fn soft_ink(self, from: Hsla, to: Hsla) -> Self {
        self.soft(None, Some((from, to)))
    }
}

impl SoftHover for Stateful<Div> {
    fn soft(mut self, bg: Option<(Hsla, Hsla)>, ink: Option<(Hsla, Hsla)>) -> Self {
        let key: SharedString = format!("{:?}", self.interactivity().element_id).into();
        let k = level(&key, hovered(&key), HOVER);
        let mut d = self;
        if let Some((from, to)) = bg {
            d = d.bg(mix(from, to, k));
        }
        if let Some((from, to)) = ink {
            d = d.text_color(mix(from, to, k));
        }
        d.on_hover(move |on, window, _| {
            set_hovered(key.clone(), *on);
            window.refresh();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{level, mix, moving, SLIDE};
    use gpui_kit::{hsla, transparent_black, SharedString};

    #[test]
    fn fading_in_from_nothing_never_greys() {
        let light = hsla(0.0, 0.0, 0.95, 1.0);
        let half = mix(transparent_black(), light, 0.5).to_rgb();
        assert!(half.r > 0.9 && (half.a - 0.5).abs() < 0.01);
    }

    #[test]
    fn a_level_starts_where_it_is_and_eases_when_it_changes() {
        let key: SharedString = "test-switch".into();
        assert_eq!(level(&key, false, SLIDE), 0.0);
        let moved = level(&key, true, SLIDE);
        assert!(moved < 0.5);
        assert!(moving());
    }
}
