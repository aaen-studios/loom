//! An agent's thread: the only colour in the app, a line whose movement says what
//! the agent is doing (design/tokens.toml, [thread.motion.*], and
//! design/explorations/threads-playground.html). Drawn as one path, its phase taken
//! from the time, so a window that's animating just asks for the next frame.
//!
//! A thread never jumps: each remembers, under its key, the mood and colour it last
//! showed, and eases from those to new ones.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::time::Instant;

use gpui_kit::*;

use crate::model::Mood;
use crate::tokens::*;

/// How long a thread takes to ease from one mood or colour to the next, in seconds.
const EASE: f32 = 0.7;

/// One clock for every thread, so they move together and never jump.
pub fn now() -> f32 {
    use std::sync::OnceLock;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}

/// The thread colour named `name`, for the theme.
pub fn color(name: &str, dark: bool) -> Hsla {
    let c = if dark {
        match name {
            "mint" => thread_color_dark::MINT,
            "apricot" => thread_color_dark::APRICOT,
            "pink" => thread_color_dark::PINK,
            "sky" => thread_color_dark::SKY,
            "gold" => thread_color_dark::GOLD,
            "lavender" => thread_color_dark::LAVENDER,
            "grey" => thread_color_dark::GREY,
            _ => thread_color_dark::PERIWINKLE,
        }
    } else {
        match name {
            "mint" => thread_color_light::MINT,
            "apricot" => thread_color_light::APRICOT,
            "pink" => thread_color_light::PINK,
            "sky" => thread_color_light::SKY,
            "gold" => thread_color_light::GOLD,
            "lavender" => thread_color_light::LAVENDER,
            "grey" => thread_color_light::GREY,
            _ => thread_color_light::PERIWINKLE,
        }
    };
    rgb(c).into()
}

/// Whether a thread in this state moves, so the window keeps drawing frames. A thread
/// still easing between moods asks for its own frames.
pub fn moves(mood: Mood, snapped: Option<Instant>) -> bool {
    mood != Mood::Idle && mood != Mood::Failed
        || snapped.is_some_and(|at| at.elapsed().as_secs_f32() < 1.2)
}

/// Where a thread is between two moods and two colours.
struct Blend {
    from: Mood,
    to: Mood,
    at: f32,
    from_tint: Hsla,
    to_tint: Hsla,
    tint_at: f32,
    seen: f32,
}

thread_local! {
    static BLENDS: RefCell<HashMap<SharedString, Blend>> = RefCell::new(HashMap::new());
}

/// Slow at both ends.
fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn mix(a: Hsla, b: Hsla, k: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let l = |x: f32, y: f32| x + (y - x) * k;
    Rgba {
        r: l(a.r, b.r),
        g: l(a.g, b.g),
        b: l(a.b, b.b),
        a: l(a.a, b.a),
    }
    .into()
}

/// The moods the thread under `key` is between, how far along (0 to 1), and its colour
/// now. A new key starts where it's asked to be.
fn blend(key: &SharedString, mood: Mood, tint: Hsla, t: f32) -> (Mood, Mood, f32, Hsla) {
    BLENDS.with(|b| {
        let mut b = b.borrow_mut();
        // Threads that haven't been drawn for a while (a closed chat's) are forgotten.
        if b.len() > 400 {
            b.retain(|_, v| t - v.seen < 30.0);
        }
        let e = b.entry(key.clone()).or_insert(Blend {
            from: mood,
            to: mood,
            at: t - EASE,
            from_tint: tint,
            to_tint: tint,
            tint_at: t - EASE,
            seen: t,
        });
        e.seen = t;
        if e.to != mood {
            // From whichever mood showed most, so a change midway doesn't jump far.
            let k = (t - e.at) / EASE;
            e.from = if k < 0.5 { e.from } else { e.to };
            e.to = mood;
            e.at = t;
        }
        if e.to_tint != tint {
            let shown = mix(e.from_tint, e.to_tint, ease((t - e.tint_at) / EASE));
            e.from_tint = shown;
            e.to_tint = tint;
            e.tint_at = t;
        }
        (
            e.from,
            e.to,
            ease((t - e.at) / EASE),
            mix(e.from_tint, e.to_tint, ease((t - e.tint_at) / EASE)),
        )
    })
}

/// The height of a thread in `mood` at `u` (0..1 along it), as a fraction of its half
/// height.
fn y_at(mood: Mood, u: f32, width: f32, t: f32, since_snap: f32) -> f32 {
    let x = u * width;
    match mood {
        Mood::Idle => {
            let base = (TAU * (u * 1.3 + t * thread_motion_idle::SPEED)).sin()
                * thread_motion_idle::AMPLITUDE
                * 2.0;
            let snap = if since_snap < 1.2 {
                let a = thread_motion_snap::AMPLITUDE
                    * 2.0
                    * (-thread_motion_snap::DAMPING * since_snap).exp();
                a * (TAU * (thread_motion_snap::FREQUENCY / 10.0) * since_snap).sin()
                    * (PI * u).sin()
            } else {
                0.0
            };
            base + snap
        }
        Mood::Thinking => {
            // A slow loose wave, with a loop travelling along it.
            let head = (t * thread_motion_thinking::LOOP_SPEED).fract();
            let d = (u - head).abs().min(1.0 - (u - head).abs());
            // Smooth at its top and its feet, so the loop never kinks.
            let x = (d / thread_motion_thinking::LOOP_WIDTH).min(1.0);
            let bump = (1.0 - x * x).powi(2);
            (TAU * (u * 2.0 - t * 0.35)).sin() * thread_motion_thinking::AMPLITUDE * 4.0
                + bump * 0.55
        }
        Mood::Writing => {
            (TAU * (x / thread_motion_writing::WAVELENGTH - t * thread_motion_writing::SPEED)).sin()
                * thread_motion_writing::AMPLITUDE
                * 2.4
        }
        Mood::Tool => {
            // Short stitches sliding along.
            let phase = (x / 8.0 - t * thread_motion_tool::SLIDE_SPEED * 2.0).fract();
            let lift = if phase < 0.5 { 1.0 } else { -1.0 };
            lift * thread_motion_tool::AMPLITUDE * 1.4
        }
        Mood::Waiting => {
            // Straight, with a knot in the middle that bobs, and a tug now and then.
            let tug = (t % thread_motion_waiting::TUG_EVERY) < 0.25;
            let bob = (TAU * t * thread_motion_waiting::BOB_SPEED / 4.0).sin() * 0.25;
            let d = (u - 0.5).abs();
            let knot = if d < thread_motion_waiting::KNOT_WIDTH * 2.0 {
                1.0 - d / (thread_motion_waiting::KNOT_WIDTH * 2.0)
            } else {
                0.0
            };
            knot * (0.8 + bob) * if tug { 1.3 } else { 1.0 }
        }
        Mood::Failed => 0.0,
    }
}

/// How much of `mood` shows when easing from `from` to `to`, `k` of the way.
fn weight(mood: Mood, from: Mood, to: Mood, k: f32) -> f32 {
    (if from == mood { 1.0 - k } else { 0.0 }) + if to == mood { k } else { 0.0 }
}

/// A thread `w` by `h` pixels. `key` names it, so it eases when its mood or colour
/// changes: the agent's id with where it's drawn, for instance.
pub fn thread(
    key: impl Into<SharedString>,
    mood: Mood,
    tint: Hsla,
    w: f32,
    h: f32,
    snapped: Option<Instant>,
) -> impl IntoElement {
    let key = key.into();
    let stroke = if h > 12.0 {
        thread_stroke::LARGE
    } else {
        thread_stroke::SMALL
    };
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let t = now();
            let (from, to, k, tint) = blend(&key, mood, tint, t);
            if k < 1.0 {
                window.request_animation_frame();
            }
            let x0 = bounds.origin.x.as_f32();
            let mid = bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0;
            let width = bounds.size.width.as_f32();
            let half = bounds.size.height.as_f32() / 2.0;
            let since_snap = snapped
                .map(|at| at.elapsed().as_secs_f32())
                .unwrap_or(f32::MAX);
            let steps = (width / 1.5).max(8.0) as usize;
            let y = |u: f32| {
                let a = y_at(from, u, width, t, since_snap);
                let b = y_at(to, u, width, t, since_snap);
                a + (b - a) * k
            };
            let failed = weight(Mood::Failed, from, to, k);
            let end = 1.0 - 0.38 * failed;
            let mut pb = PathBuilder::stroke(px(stroke));
            for i in 0..=steps {
                let u = end * i as f32 / steps as f32;
                let p = point(px(x0 + u * width), px(mid - y(u) * (half - stroke)));
                if i == 0 {
                    pb.move_to(p)
                } else {
                    pb.line_to(p)
                }
            }
            if let Ok(path) = pb.build() {
                window.paint_path(path, tint);
            }
            if failed > 0.01 {
                // Frayed at the end.
                let fx = x0 + end * width;
                let mut fray = PathBuilder::stroke(px(stroke));
                for (dx, dy) in [(0.22, -0.8), (0.26, -0.2), (0.24, 0.35), (0.18, 0.9)] {
                    fray.move_to(point(px(fx), px(mid)));
                    fray.line_to(point(
                        px(fx + dx * width * failed),
                        px(mid + dy * failed * (half - stroke)),
                    ));
                }
                if let Ok(path) = fray.build() {
                    window.paint_path(path, tint);
                }
            }
            let waiting = weight(Mood::Waiting, from, to, k);
            if waiting > 0.01 {
                let r = (half * 0.55).max(2.5) * waiting;
                let mut ring = PathBuilder::stroke(px(stroke + 0.2));
                let cx0 = x0 + width / 2.0;
                for i in 0..=24 {
                    let a = TAU * i as f32 / 24.0;
                    let p = point(px(cx0 + a.cos() * r), px(mid + a.sin() * r));
                    if i == 0 {
                        ring.move_to(p)
                    } else {
                        ring.line_to(p)
                    }
                }
                if let Ok(path) = ring.build() {
                    window.paint_path(path, tint);
                }
            }
        },
    )
    .w(px(w))
    .h(px(h))
    .flex_none()
}

/// How the server's spool is: what the connection to the instance is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spool {
    /// Connected: its thread wound on and still.
    Wound,
    /// Connecting: it turns, and its thread waves as it's drawn off.
    Turning,
    /// Can't be reached: half unwound, its end hanging loose and frayed.
    Loose,
    /// Signed out: an empty spool.
    Empty,
}

/// The server's mark, `w` by `h` pixels: a spool, with its thread coming off to the
/// right, the way an agent is its thread. The spool takes `h` of the width; the thread
/// the rest.
pub fn spool(state: Spool, ink: Hsla, w: f32, h: f32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let t = now();
            if state == Spool::Turning {
                window.request_animation_frame();
            }
            let x0 = bounds.origin.x.as_f32();
            let y0 = bounds.origin.y.as_f32();
            let bw = bounds.size.width.as_f32();
            let bh = bounds.size.height.as_f32();
            let s = if bh > 24.0 {
                thread_stroke::LARGE
            } else {
                thread_stroke::SMALL
            };
            let paint = |pb: PathBuilder, window: &mut Window| {
                if let Ok(path) = pb.build() {
                    window.paint_path(path, ink);
                }
            };
            // The flanges, either end of the spool.
            let left = x0 + s;
            let right = x0 + bh * 1.1 - s;
            let top = y0 + s;
            let bottom = y0 + bh - s;
            let mid = y0 + bh / 2.0;
            let mut flanges = PathBuilder::stroke(px(s));
            for x in [left, right] {
                flanges.move_to(point(px(x), px(top)));
                flanges.line_to(point(px(x), px(bottom)));
            }
            paint(flanges, window);
            // The thread wound on the barrel between them: close slanted turns, filling
            // most of the flanges' height, that slide along while the spool turns.
            let (bt, bb) = (mid - bh * 0.3, mid + bh * 0.3);
            let (inner_l, inner_r) = (left + s * 1.3, right - s * 1.3);
            let gap = (bh * 0.15).max(2.2);
            let slant = gap * 1.3;
            let wound_to = match state {
                Spool::Empty => inner_l,
                Spool::Loose => inner_l + (inner_r - inner_l) * 0.5,
                _ => inner_r,
            };
            let shift = if state == Spool::Turning {
                (t * 1.4 * gap) % gap
            } else {
                0.0
            };
            let mut turns = PathBuilder::stroke(px(s * 0.7));
            let mut x = inner_l - slant - gap + shift;
            while x < wound_to {
                let u0 = ((inner_l - x) / slant).max(0.0);
                let u1 = ((wound_to - x) / slant).min(1.0);
                if u1 > u0 {
                    let at = |u: f32| point(px(x + slant * u), px(bb + (bt - bb) * u));
                    turns.move_to(at(u0));
                    turns.line_to(at(u1));
                }
                x += gap;
            }
            paint(turns, window);
            if state == Spool::Empty {
                return;
            }
            // Its end, off the spool to the right at the middle, as an agent's thread
            // is: still while it's connected, waving as it's drawn off while it
            // connects, and hanging down, frayed, while it can't be reached.
            let from_x = right + s;
            let end_x = x0 + bw - s;
            let span = (end_x - from_x).max(1.0);
            let steps = (span / 1.5).max(8.0) as usize;
            let point_at = |u: f32| match state {
                Spool::Loose => {
                    let x = from_x + span * 0.45 * u;
                    point(px(x), px(mid + (bottom - mid) * u.powf(1.6)))
                }
                Spool::Turning => {
                    let wave = (bh * 0.2) * u * (TAU * (u * 1.2 - t * 0.9)).sin();
                    point(px(from_x + span * u), px(mid + wave))
                }
                _ => point(px(from_x + span * u), px(mid)),
            };
            let mut end = PathBuilder::stroke(px(s));
            for i in 0..=steps {
                let p = point_at(i as f32 / steps as f32);
                if i == 0 {
                    end.move_to(p)
                } else {
                    end.line_to(p)
                }
            }
            paint(end, window);
            if state == Spool::Loose {
                // Frayed where it ends.
                let tip = point_at(1.0);
                let mut fray = PathBuilder::stroke(px(s * 0.8));
                for (dx, dy) in [(-0.14, 0.14), (0.0, 0.2), (0.14, 0.14)] {
                    fray.move_to(tip);
                    fray.line_to(point(tip.x + px(dx * bh), tip.y + px(dy * bh)));
                }
                paint(fray, window);
            }
        },
    )
    .w(px(w))
    .h(px(h))
    .flex_none()
}

/// Several threads woven across the full width, `h` high: each its own wave, so they
/// cross over and under one another. `gather` (0 to 1) pulls them all on to the first
/// one's line, which calms as they come, until there is only that one. `reveal` (0 to 1)
/// is how far across they've come in from the left, each a little behind the one before.
pub fn weave(colors: Vec<Hsla>, gather: f32, reveal: f32, h: f32) -> impl IntoElement {
    let g = gather.clamp(0.0, 1.0);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let t = now();
            let x0 = bounds.origin.x.as_f32();
            let width = bounds.size.width.as_f32();
            let mid = bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0;
            let half = bounds.size.height.as_f32() / 2.0 - thread_stroke::LARGE;
            let n = colors.len().max(1) as f32;
            let steps = (width / 3.0).max(16.0) as usize;
            // Where thread `k` is at `u`, loose, before any gathering.
            let loose = |k: f32, u: f32| {
                let lane = (k - (n - 1.0) / 2.0) / n * 0.9;
                let amp = 0.55 + 0.25 * (k * 1.7).sin();
                let wave = 0.9 + 0.35 * (k * 0.9).cos();
                let speed = 0.10 + 0.04 * (k * 2.3).sin().abs();
                // Calmer at the edges, so the threads come in from nowhere.
                let edge = (PI * u).sin().max(0.0).powf(0.6);
                lane + amp * edge * (TAU * (u * wave - t * speed) + k * 1.37).sin()
            };
            // Drawn back to front, so the first colour is the one on top.
            for (i, tint) in colors.iter().enumerate().rev() {
                let k = i as f32;
                // Each comes in a little after the one before.
                let shown = ease((reveal * 1.5 - k / n * 0.5).clamp(0.0, 1.0));
                if shown <= 0.0 {
                    continue;
                }
                let last = ((steps as f32) * shown) as usize;
                let mut pb = PathBuilder::stroke(px(thread_stroke::LARGE));
                for s in 0..=last.max(1) {
                    let u = s as f32 / steps as f32;
                    let first = loose(0.0, u) - (0.0 - (n - 1.0) / 2.0) / n * 0.9;
                    let own = loose(k, u);
                    let y = (own + (first - own) * g) * (1.0 - g * 0.85);
                    // Eased in at the edges of its band, never cut flat.
                    let p = point(px(x0 + u * width), px(mid - y.tanh() * half));
                    if s == 0 {
                        pb.move_to(p)
                    } else {
                        pb.line_to(p)
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, *tint);
                }
            }
        },
    )
    .w_full()
    .h(px(h))
    .flex_none()
}
