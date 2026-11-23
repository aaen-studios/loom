//! Smooth wheel scrolling. A mouse wheel's notch, which the system gives in lines, is
//! caught before anything scrolls and played out in pixels over the next frames, easing
//! out, as a browser does. Trackpads and touch, already in pixels, pass as they are.

use std::time::Duration;

use gpui_kit::*;

/// How much of what's left each 8 ms takes: a notch is mostly there in 80 ms and done
/// in about 200, at any frame rate.
const TAKE: f32 = 0.2;

/// The time `TAKE` is for.
pub const FRAME: Duration = Duration::from_millis(8);

/// The wheel's scrolling still to play out.
#[derive(Default)]
pub struct Smooth {
    /// The pixels still to scroll, across and down.
    left: (f32, f32),
    /// Where the pointer was, and the keys held, at the last notch.
    at: Point<Pixels>,
    modifiers: Modifiers,
    /// Whether steps are being played out.
    pub running: bool,
}

impl Smooth {
    /// Takes a wheel's notch, `line` pixels a line: true when it's caught, and the caller
    /// stops it reaching anything. A notch the other way drops what was left.
    pub fn catch(&mut self, e: &ScrollEvent, line: f32) -> bool {
        let ScrollDelta::Lines(lines) = e.delta else {
            return false;
        };
        if e.source != ScrollSource::Pointer {
            return false;
        }
        let (x, y) = (lines.x * line, lines.y * line);
        if x * self.left.0 < 0.0 {
            self.left.0 = 0.0;
        }
        if y * self.left.1 < 0.0 {
            self.left.1 = 0.0;
        }
        self.left = (self.left.0 + x, self.left.1 + y);
        self.at = e.position;
        self.modifiers = e.modifiers;
        true
    }

    /// The next step to scroll, in pixels, for `since` since the last one, or none once
    /// it's played out.
    pub fn step(&mut self, since: Duration) -> Option<ScrollEvent> {
        let frames = (since.as_secs_f32() / FRAME.as_secs_f32()).clamp(0.25, 8.0);
        let part = 1.0 - (1.0 - TAKE).powf(frames);
        let take = |v: f32| {
            if v.abs() <= 1.0 {
                v
            } else {
                v.signum() * (v.abs() * part).max(1.0)
            }
        };
        let (dx, dy) = (take(self.left.0), take(self.left.1));
        if dx == 0.0 && dy == 0.0 {
            return None;
        }
        self.left = (self.left.0 - dx, self.left.1 - dy);
        Some(ScrollEvent {
            position: self.at,
            delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
            modifiers: self.modifiers,
            touch_phase: TouchPhase::Moved,
            source: ScrollSource::Pointer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Smooth;
    use gpui_kit::{point, px, ScrollDelta, ScrollEvent, ScrollSource};

    fn notch(lines: f32) -> ScrollEvent {
        ScrollEvent {
            delta: ScrollDelta::Lines(point(0.0, lines)),
            source: ScrollSource::Pointer,
            ..Default::default()
        }
    }

    fn played(s: &mut Smooth) -> Vec<f32> {
        let mut steps = Vec::new();
        while let Some(e) = s.step(super::FRAME) {
            let ScrollDelta::Pixels(d) = e.delta else {
                panic!("steps are in pixels")
            };
            steps.push(d.y.as_f32());
        }
        steps
    }

    #[test]
    fn a_notch_plays_out_whole_and_eases_out() {
        let mut s = Smooth::default();
        assert!(s.catch(&notch(-3.0), 20.0));
        let steps = played(&mut s);
        let total: f32 = steps.iter().sum();
        assert!((total + 60.0).abs() < 0.001, "all of it, {total}");
        assert!(steps.len() > 5, "over several frames");
        assert!(
            steps[0].abs() > steps[steps.len() / 2].abs(),
            "quick, then slower"
        );
    }

    #[test]
    fn a_notch_the_other_way_drops_the_rest() {
        let mut s = Smooth::default();
        s.catch(&notch(-3.0), 20.0);
        s.step(super::FRAME);
        s.catch(&notch(1.0), 20.0);
        let total: f32 = played(&mut s).iter().sum();
        assert!((total - 20.0).abs() < 0.001, "only the new notch, {total}");
    }

    #[test]
    fn pixels_pass_as_they_are() {
        let mut s = Smooth::default();
        let pad = ScrollEvent {
            delta: ScrollDelta::Pixels(point(px(0.0), px(-4.0))),
            ..Default::default()
        };
        assert!(!s.catch(&pad, 20.0));
        let touch = ScrollEvent {
            source: ScrollSource::Touch,
            ..notch(-1.0)
        };
        assert!(!s.catch(&touch, 20.0));
        assert!(s.step(super::FRAME).is_none());
    }
}
