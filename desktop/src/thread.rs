//! An agent's thread: the only colour in the app, a line whose movement says what
//! the agent is doing (design/tokens.toml, [thread.motion.*], and
//! design/explorations/threads-playground.html). Drawn as one path, its phase taken
//! from the time, so a window that's animating just asks for the next frame.

use std::f32::consts::TAU;
use std::time::Instant;

use gpui_kit::*;

use crate::model::Mood;
use crate::tokens::*;

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

/// Whether a thread in this state moves, so the window keeps drawing frames.
pub fn moves(mood: Mood, snapped: Option<Instant>) -> bool {
    mood != Mood::Idle && mood != Mood::Failed
        || snapped.is_some_and(|at| at.elapsed().as_secs_f32() < 1.2)
}

/// A thread `w` by `h` pixels.
pub fn thread(
    mood: Mood,
    tint: Hsla,
    w: f32,
    h: f32,
    snapped: Option<Instant>,
) -> impl IntoElement {
    let stroke = if h > 12.0 {
        thread_stroke::LARGE
    } else {
        thread_stroke::SMALL
    };
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let t = now();
            let x0 = bounds.origin.x.as_f32();
            let mid = bounds.origin.y.as_f32() + bounds.size.height.as_f32() / 2.0;
            let width = bounds.size.width.as_f32();
            let half = bounds.size.height.as_f32() / 2.0;
            let since_snap = snapped
                .map(|at| at.elapsed().as_secs_f32())
                .unwrap_or(f32::MAX);
            let steps = (width / 1.5).max(8.0) as usize;
            // The height of the line at x (0..1 along it), as a fraction of the half height.
            let y_at = |u: f32| -> f32 {
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
                                * (std::f32::consts::PI * u).sin()
                        } else {
                            0.0
                        };
                        base + snap
                    }
                    Mood::Thinking => {
                        // A slow loose wave, with a loop travelling along it.
                        let head = (t * thread_motion_thinking::LOOP_SPEED).fract();
                        let d = (u - head).abs().min(1.0 - (u - head).abs());
                        let bump =
                            (1.0 - (d / thread_motion_thinking::LOOP_WIDTH).min(1.0)).powi(2);
                        (TAU * (u * 2.0 - t * 0.35)).sin() * thread_motion_thinking::AMPLITUDE * 4.0
                            + bump * 0.55
                    }
                    Mood::Writing => {
                        (TAU * (x / thread_motion_writing::WAVELENGTH
                            - t * thread_motion_writing::SPEED))
                            .sin()
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
            };
            let mut pb = PathBuilder::stroke(px(stroke));
            let end = if mood == Mood::Failed { 0.62 } else { 1.0 };
            for i in 0..=steps {
                let u = end * i as f32 / steps as f32;
                let p = point(px(x0 + u * width), px(mid - y_at(u) * (half - stroke)));
                if i == 0 {
                    pb.move_to(p)
                } else {
                    pb.line_to(p)
                }
            }
            if mood == Mood::Failed {
                // Frayed at the end.
                let fx = x0 + end * width;
                for (dx, dy) in [(0.22, -0.8), (0.26, -0.2), (0.24, 0.35), (0.18, 0.9)] {
                    pb.move_to(point(px(fx), px(mid)));
                    pb.line_to(point(px(fx + dx * width), px(mid + dy * (half - stroke))));
                }
            }
            if let Ok(path) = pb.build() {
                window.paint_path(path, tint);
            }
            if mood == Mood::Waiting {
                let r = (half * 0.55).max(2.5);
                let mut ring = PathBuilder::stroke(px(stroke + 0.2));
                let cx0 = x0 + width / 2.0;
                let cy0 = mid - y_at(0.5) * (half - stroke) * 0.0;
                for i in 0..=24 {
                    let a = TAU * i as f32 / 24.0;
                    let p = point(px(cx0 + a.cos() * r), px(cy0 + a.sin() * r));
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

/// Several threads woven across the full width, `h` high: each its own wave, so they
/// cross over and under one another. `gather` (0 to 1) pulls them in to one line in the
/// middle, the first thread on top; at 1 only the first is left.
pub fn weave(colors: Vec<Hsla>, gather: f32, h: f32) -> impl IntoElement {
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
            // Drawn back to front, so the first colour is the one on top.
            for (i, tint) in colors.iter().enumerate().rev() {
                let k = i as f32;
                let lane = (k - (n - 1.0) / 2.0) / n * 0.9 * (1.0 - g);
                let amp = (0.55 + 0.25 * (k * 1.7).sin()) * (1.0 - g * 0.85);
                let wave = 0.9 + 0.35 * (k * 0.9).cos();
                let speed = 0.10 + 0.04 * (k * 2.3).sin().abs();
                let phase = k * 1.37;
                let fade = if i == 0 { 1.0 } else { 1.0 - g };
                if fade <= 0.01 {
                    continue;
                }
                let mut pb = PathBuilder::stroke(px(thread_stroke::LARGE));
                for s in 0..=steps {
                    let u = s as f32 / steps as f32;
                    // Calmer at the edges, so the threads come in from nowhere.
                    let edge = (std::f32::consts::PI * u).sin().max(0.0).powf(0.6);
                    let y = lane + amp * edge * (TAU * (u * wave - t * speed) + phase).sin();
                    let p = point(px(x0 + u * width), px(mid - y.clamp(-1.0, 1.0) * half));
                    if s == 0 {
                        pb.move_to(p)
                    } else {
                        pb.line_to(p)
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, tint.opacity(fade));
                }
            }
        },
    )
    .w_full()
    .h(px(h))
    .flex_none()
}
