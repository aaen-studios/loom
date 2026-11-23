//! Small motion for controls: a switch that slides, a hover that fades in and out.
//!
//! GPUI's own hover style swaps in a single frame. Here each thing that moves has a key,
//! and `level` says how far it is between off (0) and on (1), easing from wherever it was
//! when it last changed. While anything is still on its way, `moving` says so, and the
//! window asks for another frame.
//!
//! A view that's drawn from its cache (`AnyView::cached`) isn't built again, so what's
//! in it is drawn `scoped` to it: a key in a view that wasn't built this frame is kept,
//! and while one is on its way, `moving` names its view, to be drawn again.

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
    /// row that went away while the pointer was on it doesn't come back lit, unless the
    /// view it's in wasn't built this frame.
    static SEEN: RefCell<HashSet<SharedString>> = RefCell::new(HashSet::new());
    /// The view being built now, when it's one drawn from a cache.
    static SCOPE: Cell<Option<EntityId>> = const { Cell::new(None) };
    /// The view each key was last built in.
    static SCOPES: RefCell<HashMap<SharedString, EntityId>> = RefCell::new(HashMap::new());
    /// The views built this frame.
    static BUILT: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
    /// The views with something on its way, to be built again next frame.
    static MOVING_IN: RefCell<HashSet<EntityId>> = RefCell::new(HashSet::new());
}

/// Builds what `f` makes as part of `view`, one that's drawn from its cache.
pub fn scoped<T>(view: EntityId, f: impl FnOnce() -> T) -> T {
    BUILT.with(|b| {
        b.borrow_mut().insert(view);
    });
    let outer = SCOPE.with(|s| s.replace(Some(view)));
    let out = f();
    SCOPE.with(|s| s.set(outer));
    out
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
    let scope = SCOPE.with(|s| s.get());
    SCOPES.with(|s| {
        let mut s = s.borrow_mut();
        match scope {
            Some(view) => {
                s.insert(key.clone(), view);
            }
            None => {
                s.remove(key);
            }
        }
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
            if let Some(view) = scope {
                MOVING_IN.with(|m| {
                    m.borrow_mut().insert(view);
                });
            }
        }
        v
    })
}

/// How far the thing under `key` has come in, from 0 when it first shows to 1 `secs`
/// later. Once a frame goes by without it, it comes in from 0 again.
pub fn arrive(key: &SharedString, secs: f32) -> f32 {
    LEVELS.with(|levels| {
        levels.borrow_mut().entry(key.clone()).or_insert(Level {
            on: true,
            from: 0.0,
            at: Instant::now(),
            secs,
        });
    });
    level(key, true, secs)
}

/// Whether the pointer is over the thing under `key`.
pub fn hovered(key: &SharedString) -> bool {
    HOVERED.with(|h| h.borrow().get(key).copied().unwrap_or(false))
}

/// `d`, keeping whether the pointer is anywhere over it under `key`, for `hovered`.
pub fn tracks_hover(d: Stateful<Div>, key: SharedString) -> Stateful<Div> {
    d.on_hover(move |on, window, _| {
        set_hovered(key.clone(), *on);
        window.refresh();
    })
}

fn set_hovered(key: SharedString, on: bool) {
    HOVERED.with(|h| {
        h.borrow_mut().insert(key, on);
    });
}

/// What was still moving in the frame just drawn, which the window asks once per frame,
/// at its end: it needs another while anything is.
pub struct Moving {
    /// Anything at all.
    pub any: bool,
    /// The cached views it's in, which are to be built again for it.
    pub views: Vec<EntityId>,
}

/// What was still moving in the frame just drawn. What wasn't drawn in it is forgotten,
/// except what's in a cached view that wasn't built this frame.
pub fn moving() -> Moving {
    let seen = SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()));
    let built = BUILT.with(|b| std::mem::take(&mut *b.borrow_mut()));
    let kept = SCOPES.with(|s| {
        let mut s = s.borrow_mut();
        s.retain(|k, view| seen.contains(k) || !built.contains(view));
        s.keys().cloned().collect::<HashSet<_>>()
    });
    let keep = |k: &SharedString| seen.contains(k) || kept.contains(k);
    LEVELS.with(|l| l.borrow_mut().retain(|k, _| keep(k)));
    HOVERED.with(|h| h.borrow_mut().retain(|k, _| keep(k)));
    Moving {
        any: MOVING.with(|m| m.replace(false)),
        views: MOVING_IN.with(|m| m.borrow_mut().drain().collect()),
    }
}

/// Forgets what was built in `view`, which is gone.
pub fn forget_view(view: EntityId) {
    let gone: Vec<SharedString> = SCOPES.with(|s| {
        let mut s = s.borrow_mut();
        let gone = s
            .iter()
            .filter(|(_, v)| **v == view)
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for k in &gone {
            s.remove(k);
        }
        gone
    });
    LEVELS.with(|l| {
        let mut l = l.borrow_mut();
        for k in &gone {
            l.remove(k);
        }
    });
    HOVERED.with(|h| {
        let mut h = h.borrow_mut();
        for k in &gone {
            h.remove(k);
        }
    });
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

/// How light a colour looks (WCAG's relative luminance, 0 to 1).
pub fn luminance(c: Hsla) -> f32 {
    let c = c.to_rgb();
    let lin = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// Of two inks, the one that reads best on `fill`: words on an agent's colour stay
/// readable whichever colour it is, in either theme.
pub fn ink_on(fill: Hsla, a: Hsla, b: Hsla) -> Hsla {
    let contrast = |ink: Hsla| {
        let (x, y) = (luminance(fill), luminance(ink));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    };
    if contrast(a) >= contrast(b) {
        a
    } else {
        b
    }
}

/// What a hovered background turns while it's pressed: a step darker on a light one,
/// a step lighter on a dark one, so every button answers the click the same way.
pub fn pressed(c: Hsla) -> Hsla {
    let mut c = c;
    if c.l > 0.5 {
        c.l -= 0.05;
    } else {
        c.l += 0.06;
    }
    c
}

/// A hover that fades: the element's background goes from `bg` to `hover_bg`, and its
/// text from the first of `ink` to the second, when given, over `HOVER` seconds each
/// way, and a step further while it's pressed. It's kept under the element's id.
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

/// The key a soft hover keeps the pointer's being on the element with `id` under, for
/// `hovered`.
pub fn hover_key(id: impl Into<ElementId>) -> SharedString {
    format!("{:?}", Some(id.into())).into()
}

impl SoftHover for Stateful<Div> {
    fn soft(mut self, bg: Option<(Hsla, Hsla)>, ink: Option<(Hsla, Hsla)>) -> Self {
        let key: SharedString = format!("{:?}", self.interactivity().element_id).into();
        let k = level(&key, hovered(&key), HOVER);
        let mut d = self;
        if let Some((from, to)) = bg {
            d = d.bg(mix(from, to, k));
            if to.a > 0.0 {
                let down = pressed(to);
                d = d.active(move |s| s.bg(down));
            }
        }
        if let Some((from, to)) = ink {
            d = d.text_color(mix(from, to, k));
        }
        tracks_hover(d, key)
    }
}

#[cfg(test)]
mod tests {
    use super::{arrive, forget_view, hover_key, level, mix, moving, scoped, SLIDE};
    use gpui_kit::{
        div, hsla, transparent_black, EntityId, Hsla, InteractiveElement, SharedString,
    };

    #[test]
    fn a_hover_key_is_the_one_a_soft_hover_keeps() {
        let mut d = div().id("bar-open");
        let kept = format!("{:?}", d.interactivity().element_id);
        assert_eq!(hover_key("bar-open"), SharedString::from(kept));
    }

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
        assert!(moving().any);
    }

    #[test]
    fn what_a_cached_view_drew_is_kept_until_it_draws_again() {
        let view = EntityId::from(7u64);
        let key: SharedString = "test-cached-hover".into();
        // Built in the view, on its way in: the view is to be built again.
        scoped(view, || {
            level(&key, false, SLIDE);
            level(&key, true, SLIDE)
        });
        let m = moving();
        assert!(m.any);
        assert_eq!(m.views, vec![view]);
        // A frame the view is drawn from its cache keeps where it was: still on its way.
        let _ = moving();
        assert!(scoped(view, || level(&key, true, SLIDE)) < 1.0);
        // A frame it's built without it forgets it: it comes back as it is, at once.
        let _ = moving();
        scoped(view, || ());
        let _ = moving();
        assert_eq!(level(&key, false, SLIDE), 0.0);
        let _ = moving();
    }

    #[test]
    fn a_gone_view_takes_its_keys_with_it() {
        let view = EntityId::from(8u64);
        let key: SharedString = "test-gone".into();
        scoped(view, || level(&key, false, SLIDE));
        let _ = moving();
        forget_view(view);
        let _ = moving();
        assert_eq!(scoped(view, || level(&key, true, SLIDE)), 1.0);
        let _ = moving();
    }

    #[test]
    fn words_on_a_colour_take_the_ink_that_reads() {
        let white = hsla(0.0, 0.0, 1.0, 1.0);
        let black = hsla(0.0, 0.0, 0.04, 1.0);
        // The light theme's gold and mint want dark words; its periwinkle wants white.
        let gold: Hsla = gpui_kit::rgb(0xb8961a).into();
        let periwinkle: Hsla = gpui_kit::rgb(0x5763e0).into();
        assert_eq!(super::ink_on(gold, white, black), black);
        assert_eq!(super::ink_on(periwinkle, white, black), white);
    }

    #[test]
    fn pressing_steps_away_from_the_page() {
        let light = hsla(0.0, 0.0, 0.94, 1.0);
        let dark = hsla(0.0, 0.0, 0.09, 1.0);
        assert!(super::pressed(light).l < light.l);
        assert!(super::pressed(dark).l > dark.l);
    }

    #[test]
    fn a_new_thing_comes_in_from_nothing() {
        let key: SharedString = "test-arrive".into();
        assert!(arrive(&key, 0.2) < 0.2);
        assert!(moving().any);
    }
}
