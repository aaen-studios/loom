//! A small ring that fills as something is used: an agent's context after the header's
//! model, and the plan's limits in the sidebar.

use std::f32::consts::{FRAC_PI_2, TAU};

use gpui_kit::*;

/// How full a meter reads: quiet, then warm, then red.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Quiet,
    Warn,
    Bad,
}

impl Level {
    /// A plan's limit: orange from 75%, red from 90%.
    pub fn of(percent: i64) -> Level {
        match percent {
            p if p >= 90 => Level::Bad,
            p if p >= 75 => Level::Warn,
            _ => Level::Quiet,
        }
    }

    /// An agent's context: warm from 70%, before its fresh session at 80%, and red from 90%.
    pub fn of_context(percent: i64) -> Level {
        match percent {
            p if p >= 90 => Level::Bad,
            p if p >= 70 => Level::Warn,
            _ => Level::Quiet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Level;

    #[test]
    fn a_context_warms_before_its_fresh_session_and_a_limit_later() {
        assert_eq!(Level::of_context(69), Level::Quiet);
        assert_eq!(Level::of_context(70), Level::Warn);
        assert_eq!(Level::of_context(90), Level::Bad);
        assert_eq!(Level::of(70), Level::Quiet);
        assert_eq!(Level::of(75), Level::Warn);
    }
}

/// A ring `size` across: `track` all round, and `ink` over `fraction` of it, clockwise
/// from the top.
pub fn ring(size: f32, fraction: f32, ink: Hsla, track: Hsla) -> impl IntoElement {
    let fraction = fraction.clamp(0.0, 1.0);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let line = (size / 8.0).max(1.5);
            let r = (size - line) / 2.0;
            let (cx, cy) = (
                bounds.origin.x.as_f32() + size / 2.0,
                bounds.origin.y.as_f32() + size / 2.0,
            );
            let mut arc = |to: f32, color: Hsla| {
                let steps = ((48.0 * to).ceil() as usize).max(2);
                let mut pb = PathBuilder::stroke(px(line));
                for i in 0..=steps {
                    let a = TAU * to * i as f32 / steps as f32 - FRAC_PI_2;
                    let p = point(px(cx + r * a.cos()), px(cy + r * a.sin()));
                    if i == 0 {
                        pb.move_to(p)
                    } else {
                        pb.line_to(p)
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, color);
                }
            };
            arc(1.0, track);
            if fraction > 0.0 {
                arc(fraction, ink);
            }
        },
    )
    .size(px(size))
    .flex_none()
}
