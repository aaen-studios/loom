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

/// The thread colour named `name`, for the theme. An agent's own colour, `#rrggbb`,
/// keeps its hue and is made light or dark enough to read on the theme, as loom's
/// own are.
pub fn color(name: &str, dark: bool) -> Hsla {
    if let Some(c) = hex(name) {
        let mut c: Hsla = rgb(c).into();
        c.l = if dark {
            c.l.clamp(0.62, 0.8)
        } else {
            c.l.clamp(0.3, 0.52)
        };
        return c;
    }
    let c = if dark {
        match name {
            "mint" => thread_color_dark::MINT,
            "apricot" => thread_color_dark::APRICOT,
            "pink" => thread_color_dark::PINK,
            "sky" => thread_color_dark::SKY,
            "gold" => thread_color_dark::GOLD,
            "lavender" => thread_color_dark::LAVENDER,
            "grey" => thread_color_dark::GREY,
            "coral" => thread_color_dark::CORAL,
            "moss" => thread_color_dark::MOSS,
            "teal" => thread_color_dark::TEAL,
            "plum" => thread_color_dark::PLUM,
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
            "coral" => thread_color_light::CORAL,
            "moss" => thread_color_light::MOSS,
            "teal" => thread_color_light::TEAL,
            "plum" => thread_color_light::PLUM,
            _ => thread_color_light::PERIWINKLE,
        }
    };
    rgb(c).into()
}

/// An agent's own colour, `#rrggbb`, as a number, or none when `s` isn't one.
pub fn hex(s: &str) -> Option<u32> {
    let digits = s.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// How full the wheel's colours are at its edge.
const WHEEL_FULL: f32 = 0.72;

/// The colour at `hue` (0 to 1) and `sat` (0 to 1) on the colour wheel, as an agent's
/// own, `#rrggbb`.
fn wheel_hex(hue: f32, sat: f32) -> String {
    let c: Rgba = hsla(hue.rem_euclid(1.0), sat.clamp(0.0, 1.0), 0.5, 1.0).into();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

/// What's at `(x, y)` on a colour wheel `size` across, from its top left: the hue
/// round it, and how full from its middle out. Outside the wheel is nothing.
pub fn wheel_pick(x: f32, y: f32, size: f32) -> Option<String> {
    let r = size / 2.0;
    let (dx, dy) = (x - r, y - r);
    let d = (dx * dx + dy * dy).sqrt() / r;
    if d > 1.04 {
        return None;
    }
    let hue = dy.atan2(dx).rem_euclid(TAU) / TAU;
    Some(wheel_hex(hue, d.min(1.0) * WHEEL_FULL))
}

/// Where an agent's own colour sits on a colour wheel: its hue as the angle, and how
/// full it is as the distance out, 0 to 1.
fn wheel_place(color: &str) -> Option<(f32, f32)> {
    let c: Hsla = rgb(hex(color)?).into();
    Some((c.h * TAU, (c.s / WHEEL_FULL).min(1.0)))
}

/// A colour wheel, `size` across: every hue round it, in `wedges` slices, grey in the
/// middle and full at its edge, as the theme shows it. With `mark`, an agent's own
/// colour, a small ring sits where it is.
pub fn wheel(size: f32, wedges: usize, dark: bool, mark: Option<String>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (x0, y0) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            let r = size / 2.0;
            let (cx, cy) = (x0 + r, y0 + r);
            let at = |a: f32, d: f32| point(px(cx + d * a.cos()), px(cy + d * a.sin()));
            // Grey under it all, so nothing light shows between two slices.
            let mut under = PathBuilder::fill();
            for i in 0..=48 {
                let p = at(TAU * i as f32 / 48.0, r - 0.5);
                if i == 0 {
                    under.move_to(p)
                } else {
                    under.line_to(p)
                }
            }
            under.close();
            if let Ok(path) = under.build() {
                window.paint_path(path, color(&wheel_hex(0.0, 0.0), dark));
            }
            // Thin slices from the middle out, each grey at the middle and full at the
            // edge: at a lightness of a half, a colour's channels run straight from
            // grey to full as its saturation does, so a gradient is the wheel exactly.
            for w in 0..wedges {
                // A little past each edge, so no seam shows between them.
                let a0 = TAU * w as f32 / wedges as f32 - 0.02;
                let a1 = TAU * (w + 1) as f32 / wedges as f32 + 0.02;
                let mut pb = PathBuilder::fill();
                pb.move_to(at(0.0, 0.0));
                for i in 0..=2 {
                    pb.line_to(at(a0 + (a1 - a0) * i as f32 / 2.0, r));
                }
                pb.close();
                let hue = (w as f32 + 0.5) / wedges as f32;
                // Its way out, as CSS turns it: 0 up, 90 to the right.
                let toward = 90.0 + (a0 + a1) / 2.0 * 180.0 / PI;
                if let Ok(path) = pb.build() {
                    window.paint_path(
                        path,
                        linear_gradient(
                            toward,
                            linear_color_stop(color(&wheel_hex(hue, 0.0), dark), 0.0),
                            linear_color_stop(color(&wheel_hex(hue, WHEEL_FULL), dark), 1.0),
                        ),
                    );
                }
            }
            if let Some((a, d)) = mark.as_deref().and_then(wheel_place) {
                let out = (d * r).min(r - 4.0);
                let (mx, my) = (cx + out * a.cos(), cy + out * a.sin());
                for (width, ink) in [(4.0, rgb(0x000000)), (2.0, rgb(0xffffff))] {
                    let mut ring = PathBuilder::stroke(px(width));
                    for i in 0..=24 {
                        let t = TAU * i as f32 / 24.0;
                        let p = point(px(mx + 6.0 * t.cos()), px(my + 6.0 * t.sin()));
                        if i == 0 {
                            ring.move_to(p)
                        } else {
                            ring.line_to(p)
                        }
                    }
                    if let Ok(path) = ring.build() {
                        window.paint_path(
                            path,
                            Hsla::from(ink).opacity(if width > 3.0 { 0.25 } else { 1.0 }),
                        );
                    }
                }
            }
        },
    )
    .size(px(size))
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

/// The loop a thread curls into, `r` pixels round, centred at `c` along it: how far the
/// point at `u` is pushed along and up, in pixels. Outside the loop, nothing. The line
/// goes up, back over itself and down again, smooth at both feet, and stays one line.
fn curl(u: f32, c: f32, r: f32, width: f32) -> (f32, f32) {
    if r <= 0.0 || width <= 0.0 {
        return (0.0, 0.0);
    }
    // Room for the loop along the line: tight enough that it crosses itself.
    let span = (r * 2.2 / width).min(0.9);
    let s = (u - c) / span + 0.5;
    if !(0.0..=1.0).contains(&s) {
        return (0.0, 0.0);
    }
    let e = (PI * s).sin();
    let back = r * (TAU * s).sin() * e;
    let up = r * (1.0 - (TAU * s).cos()) * e * 0.5;
    (back, up)
}

/// Where a thread in `mood` is at `u` (0..1 along it): pushed along it and up, in
/// pixels, for a thread `width` wide with `room` pixels above and below its middle.
/// Every mood is the one line bending, twisting or curling; none adds anything to it.
fn bend(mood: Mood, u: f32, width: f32, room: f32, t: f32, since_snap: f32) -> (f32, f32) {
    let x = u * width;
    // The loop's size: as big as the thread's height allows, and no bigger than a
    // short thread can hold.
    let loop_r = (room * 0.9).min(width * 0.18);
    let up = match mood {
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
            // A slow loose wave, and a loop the line curls into, sliding along it.
            let head = 0.15 + 0.7 * (t * thread_motion_thinking::LOOP_SPEED).fract();
            let (back, lift) = curl(u, head, loop_r * 0.8, width);
            let wave = (TAU * (u * 2.0 - t * 0.35)).sin() * thread_motion_thinking::AMPLITUDE * 4.0;
            return (back, wave * room + lift);
        }
        Mood::Writing => {
            (TAU * (x / thread_motion_writing::WAVELENGTH - t * thread_motion_writing::SPEED)).sin()
                * thread_motion_writing::AMPLITUDE
                * 2.4
        }
        Mood::Tool => {
            // A twist running along it: a tight wave whose swell travels, as a cord
            // turns when it's worked.
            let swell = (PI * (u - t * thread_motion_tool::SLIDE_SPEED * 0.5).fract()).sin();
            let wavelength = (room * 1.2).max(7.0);
            (TAU * (x / wavelength - t * thread_motion_tool::SLIDE_SPEED * 2.0)).sin()
                * swell
                * thread_motion_tool::AMPLITUDE
                * 1.6
        }
        Mood::Waiting => {
            // Still, curled into a loop in the middle that breathes, and draws in now
            // and then, as if tugged.
            let tug = (t % thread_motion_waiting::TUG_EVERY) < 0.25;
            let breath = 0.85 + 0.15 * (TAU * t * thread_motion_waiting::BOB_SPEED / 4.0).sin();
            let r = loop_r * breath * if tug { 0.8 } else { 1.0 };
            return curl(u, 0.5, r, width);
        }
        // Slack: it sags, its far end hanging lowest.
        Mood::Failed => -0.55 * u * u,
    };
    (0.0, up * room)
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
                crate::perf::count("want:thread");
                window.request_animation_frame();
            }
            let x0 = bounds.origin.x.as_f32();
            let mid = bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0;
            let width = bounds.size.width.as_f32();
            let half = bounds.size.height.as_f32() / 2.0;
            let since_snap = snapped
                .map(|at| at.elapsed().as_secs_f32())
                .unwrap_or(f32::MAX);
            let room = (half - stroke).max(1.0);
            // Fine enough that a loop a few pixels round is still round.
            let steps = (width / 1.5).max(48.0).max(width / 0.75) as usize;
            let at = |u: f32| {
                let (ax, ay) = bend(from, u, width, room, t, since_snap);
                let (bx, by) = bend(to, u, width, room, t, since_snap);
                (ax + (bx - ax) * k, ay + (by - ay) * k)
            };
            // A failed thread is slack and a little shorter; nothing is added to it.
            let failed = weight(Mood::Failed, from, to, k);
            let end = 1.0 - 0.2 * failed;
            let mut pb = PathBuilder::stroke(px(stroke));
            for i in 0..=steps {
                let u = end * i as f32 / steps as f32;
                let (dx, up) = at(u);
                let p = point(px(x0 + u * width + dx), px(mid - up));
                if i == 0 {
                    pb.move_to(p)
                } else {
                    pb.line_to(p)
                }
            }
            if let Ok(path) = pb.build() {
                window.paint_path(path, tint);
            }
        },
    )
    .w(px(w))
    .h(px(h))
    .flex_none()
}

/// How the server is: what the connection to the instance is doing, as the dot on its
/// tile shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spool {
    /// Connected.
    Wound,
    /// Connecting.
    Turning,
    /// Can't be reached.
    Loose,
    /// Signed out.
    Empty,
}

/// A number from a server's name, the same every time, for its yarn's colour and winding.
pub fn seed_of(name: &str) -> u64 {
    // FNV-1a.
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The next of a little sequence of numbers from 0 to 1, from `state`.
fn next(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u64 << 24) as f32
}

/// A server's mark, `size` square: a soft ball of yarn, pale `tint` on `base` with a few
/// strands of `tint` wound round it, each tilted its own way by `seed`. Connected, it
/// rests; connecting, its strands turn; when it can't be reached, it's `grey`; signed out,
/// it's only a dashed outline.
pub fn yarn(
    state: Spool,
    tint: Hsla,
    grey: Hsla,
    base: Hsla,
    seed: u64,
    size: f32,
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let t = now();
            if state == Spool::Turning {
                crate::perf::count("want:yarn");
                window.request_animation_frame();
            }
            let s = bounds.size.height.as_f32();
            let (x0, y0) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            let (cx, cy) = (x0 + s / 2.0, y0 + s / 2.0);
            let r = s * 0.39;
            let at = |x: f32, y: f32| point(px(x), px(y));
            let color = match state {
                Spool::Loose | Spool::Empty => grey,
                _ => tint,
            };
            let line = (s * 0.06).max(1.2);
            let paint = |pb: PathBuilder, c: Hsla, window: &mut Window| {
                if let Ok(path) = pb.build() {
                    window.paint_path(path, c);
                }
            };
            if state == Spool::Empty {
                // A dashed ring where the ball would be.
                let mut dashes = PathBuilder::stroke(px(line * 0.8));
                for k in 0..12 {
                    let a0 = TAU * k as f32 / 12.0;
                    let a1 = a0 + TAU / 24.0;
                    dashes.move_to(at(cx + r * a0.cos(), cy + r * a0.sin()));
                    for i in 1..=4 {
                        let a = a0 + (a1 - a0) * i as f32 / 4.0;
                        dashes.line_to(at(cx + r * a.cos(), cy + r * a.sin()));
                    }
                }
                paint(dashes, mix(color, base, 0.3), window);
                return;
            }
            let mut ball = PathBuilder::fill();
            for i in 0..=48 {
                let a = TAU * i as f32 / 48.0;
                let p = at(cx + r * 1.04 * a.cos(), cy + r * 1.04 * a.sin());
                if i == 0 {
                    ball.move_to(p)
                } else {
                    ball.line_to(p)
                }
            }
            ball.close();
            paint(ball, mix(color, base, 0.78), window);
            // Its strands: each the front half of a turn round the ball, tilted and
            // squashed its own way, turning while it connects.
            let roll = if state == Spool::Turning {
                t * 1.6
            } else {
                0.0
            };
            let mut rng = seed | 1;
            let mut strands = PathBuilder::stroke(px(line));
            for _ in 0..4 {
                let tilt = next(&mut rng) * TAU + roll;
                let squash = 0.2 + next(&mut rng) * 0.6;
                let (c, sn) = (tilt.cos(), tilt.sin());
                for i in 0..=24 {
                    let u = PI * i as f32 / 24.0;
                    let (lx, ly) = (r * 0.92 * u.cos(), r * 0.92 * squash * u.sin());
                    let p = at(cx + lx * c - ly * sn, cy + lx * sn + ly * c);
                    if i == 0 {
                        strands.move_to(p)
                    } else {
                        strands.line_to(p)
                    }
                }
            }
            paint(strands, color, window);
        },
    )
    .size(px(size))
    .flex_none()
}

/// Several threads woven across the full width, `h` high: each its own wave, so they
/// cross over and under one another. `gather` (0 to 1) pulls them all on to the first
/// one's line, which calms as they come, until there is only that one. `reveal` (0 to 1)
/// is how far across they've come in from the left, each a little behind the one before.
pub fn weave(colors: Vec<Hsla>, gather: f32, reveal: f32, h: f32) -> impl IntoElement {
    weave_layer(colors, gather, reveal, h, None)
}

/// The part of `weave` that passes over a word laid across its middle, `half` either side
/// of the centre: each thread goes over a stretch and under the next, neighbours taking
/// turns. Drawn over the word, with `weave` behind it, the threads weave through it.
pub fn weave_over(
    colors: Vec<Hsla>,
    gather: f32,
    reveal: f32,
    h: f32,
    half: f32,
) -> impl IntoElement {
    weave_layer(colors, gather, reveal, h, Some(half))
}

/// Whether thread `k` is over the word at `u`, 0 to 1 across it: four stretches, one a
/// letter of "loom", with neighbouring threads over in turn.
pub fn over_word(k: usize, u: f32) -> bool {
    if !(0.0..1.0).contains(&u) {
        return false;
    }
    ((u * 4.0).floor() as usize + k).is_multiple_of(2)
}

fn weave_layer(
    colors: Vec<Hsla>,
    gather: f32,
    reveal: f32,
    h: f32,
    over: Option<f32>,
) -> impl IntoElement {
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
                let mut pen = false;
                let mut drawn = false;
                for s in 0..=last.max(1) {
                    let u = s as f32 / steps as f32;
                    let first = loose(0.0, u) - (0.0 - (n - 1.0) / 2.0) / n * 0.9;
                    let own = loose(k, u);
                    let y = (own + (first - own) * g) * (1.0 - g * 0.85);
                    // Eased in at the edges of its band, never cut flat.
                    let p = point(px(x0 + u * width), px(mid - y.tanh() * half));
                    // Over a word, only the stretches this thread is on top for.
                    let on = over.is_none_or(|w| {
                        let x = u * width - width / 2.0;
                        over_word(i, (x + w) / (2.0 * w))
                    });
                    if !on {
                        pen = false;
                        continue;
                    }
                    if pen {
                        pb.line_to(p)
                    } else {
                        pb.move_to(p)
                    }
                    pen = true;
                    drawn = true;
                }
                if !drawn {
                    continue;
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

#[cfg(test)]
mod tests {
    use super::{bend, color, hex, wheel_hex, wheel_pick, wheel_place, WHEEL_FULL};
    use crate::model::Mood;

    const MOODS: [Mood; 6] = [
        Mood::Idle,
        Mood::Thinking,
        Mood::Writing,
        Mood::Tool,
        Mood::Waiting,
        Mood::Failed,
    ];

    /// The thread's points, as `thread` draws them, for a thread `width` by twice `room`.
    fn points(mood: Mood, width: f32, room: f32, t: f32) -> Vec<(f32, f32)> {
        let steps = (width / 1.5).max(48.0).max(width / 0.75) as usize;
        (0..=steps)
            .map(|i| {
                let u = i as f32 / steps as f32;
                let (dx, up) = bend(mood, u, width, room, t, f32::MAX);
                (u * width + dx, up)
            })
            .collect()
    }

    #[test]
    fn a_thread_is_one_unbroken_line_in_every_mood() {
        // No jumps from one point to the next: nothing is drawn apart from the line.
        for (width, room) in [(26.0, 11.0), (14.0, 4.0), (360.0, 18.0)] {
            for mood in MOODS {
                for t in [0.0, 1.3, 2.7, 4.1] {
                    let pts = points(mood, width, room, t);
                    for w in pts.windows(2) {
                        let (a, b) = (w[0], w[1]);
                        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                        assert!(d < 4.0, "{mood:?} jumps {d} at width {width}, t {t}");
                    }
                    for (_, up) in &pts {
                        assert!(up.abs() <= room * 1.2, "{mood:?} leaves its height");
                    }
                }
            }
        }
    }

    #[test]
    fn threads_take_turns_over_the_word() {
        use super::over_word;
        // Off the word, nothing is over it.
        assert!(!over_word(0, -0.1) && !over_word(1, 1.0));
        // Each thread is over every other stretch, and its neighbour over the rest.
        for k in 0..7 {
            let stretches: Vec<bool> = (0..4)
                .map(|s| over_word(k, s as f32 * 0.25 + 0.1))
                .collect();
            assert_eq!(stretches.iter().filter(|o| **o).count(), 2);
            for (s, over) in stretches.iter().enumerate() {
                assert_ne!(*over, over_word(k + 1, s as f32 * 0.25 + 0.1));
            }
        }
    }

    #[test]
    fn waiting_curls_into_a_loop() {
        // A loop: somewhere the line runs back on itself.
        let pts = points(Mood::Waiting, 26.0, 11.0, 0.5);
        assert!(pts.windows(2).any(|w| w[1].0 < w[0].0));
        // And it's straight away from the loop.
        assert!(pts[0].1.abs() < 0.01 && pts[pts.len() - 1].1.abs() < 0.01);
        let idle = points(Mood::Idle, 26.0, 11.0, 0.5);
        assert!(idle.windows(2).all(|w| w[1].0 > w[0].0));
    }

    #[test]
    fn an_agent_s_own_colour_reads_in_both_themes() {
        assert_eq!(hex("#5B8def"), Some(0x5b8def));
        assert_eq!(hex("teal"), None);
        assert_eq!(hex("#5b8de"), None);
        // A pale yellow is made dark enough on light, and a deep one light enough on dark.
        assert!(color("#fff5b0", false).l <= 0.52);
        assert!(color("#3a3000", true).l >= 0.62);
        // Every hue the picker gives is an agent's own colour.
        for i in 0..36 {
            assert!(hex(&wheel_hex(i as f32 / 36.0, WHEEL_FULL)).is_some());
        }
        assert_eq!(wheel_hex(0.0, WHEEL_FULL), "#db2424");
        // The wheel: grey in its middle, full red at its right edge, nothing outside.
        assert_eq!(wheel_pick(50.0, 50.0, 100.0).as_deref(), Some("#808080"));
        assert_eq!(wheel_pick(100.0, 50.0, 100.0).as_deref(), Some("#db2424"));
        assert_eq!(wheel_pick(0.0, 0.0, 100.0), None);
        let (a, d) = wheel_place("#db2424").unwrap();
        assert!(a.abs() < 0.01 && (d - 1.0).abs() < 0.02);
    }
}
