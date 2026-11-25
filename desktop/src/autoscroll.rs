//! Middle-click scrolling, as Chrome and Edge have it. The middle button sets a marker
//! where it's pressed, and what's under the marker (the chat, the sidebar, a box of
//! details) then scrolls each frame, faster the further the pointer is above or below it.
//! The cursor shows which way. Moved out of the dead zone with the button held, it stops
//! on release; released in the dead zone, it stays on until the next click, key or wheel.
//!
//! The speed is Chromium's (`autoscroll_controller.cc`, `fixed_velocity_curve.cc`):
//! nothing within `DEAD` of the marker, then `RATE × distance ^ POWER` pixels a second,
//! with the distance the whole of it, not only the part past the dead zone.

use std::time::{Duration, Instant};

use gpui_kit::*;

/// How far from the marker, up or down, the pointer can be with nothing scrolling: as far
/// as the button can move, held, with letting go still a click.
pub const DEAD: f32 = 15.0;

/// Pixels a second for the pointer 1 px from the marker, before the power.
const RATE: f32 = 0.04;

/// How steeply the speed grows with the distance.
const POWER: f32 = 2.2;

/// The longest a frame counts as: a frame that comes late doesn't jump the chat.
const LONGEST: Duration = Duration::from_millis(50);

/// How long without a step before the steps count as lost, as for a frame that never
/// came (`crate::smooth` has the same).
const STALL: Duration = Duration::from_millis(250);

/// How fast it scrolls for the pointer `dy` below the marker (above when negative), in
/// pixels a second: down for a positive speed.
pub fn speed(dy: f32) -> f32 {
    if dy.abs() <= DEAD {
        return 0.0;
    }
    dy.signum() * RATE * dy.abs().powf(POWER)
}

/// The cursor for the pointer `dy` below the marker: at rest in the dead zone, and
/// otherwise the way it scrolls.
pub fn cursor(dy: f32) -> PanCursor {
    match speed(dy) {
        s if s < 0.0 => PanCursor::Up,
        s if s > 0.0 => PanCursor::Down,
        _ => PanCursor::UpDown,
    }
}

/// The middle button's scrolling.
#[derive(Default)]
pub struct Auto {
    /// Where the marker is, in the window, while it's on.
    at: Option<Point<Pixels>>,
    /// Where the pointer is.
    pointer: Point<Pixels>,
    /// Whether the middle button is still down from the press that set the marker, and
    /// whether the pointer has left the dead zone since then.
    held: bool,
    dragged: bool,
    /// Whether steps are being played, and when the last was.
    running: bool,
    stepped: Option<Instant>,
    /// Which run of steps is the one playing: steps of one started before, that come
    /// after all, stop, so two never scroll at once.
    run: u64,
}

impl Auto {
    /// Where the marker is, while it's on.
    pub fn at(&self) -> Option<Point<Pixels>> {
        self.at
    }

    /// Whether the middle button that set the marker is still down.
    pub fn held(&self) -> bool {
        self.at.is_some() && self.held
    }

    /// The cursor it shows, while it's on.
    pub fn cursor(&self) -> Option<PanCursor> {
        self.at.map(|at| cursor((self.pointer.y - at.y).as_f32()))
    }

    /// The middle button went down at `at`: the marker goes there.
    pub fn press(&mut self, at: Point<Pixels>) {
        *self = Self {
            at: Some(at),
            pointer: at,
            held: true,
            run: self.run,
            ..Self::default()
        };
    }

    /// The pointer moved to `to`. Out of the dead zone with the button held, letting go
    /// stops it.
    pub fn moved(&mut self, to: Point<Pixels>) {
        self.pointer = to;
        if let Some(at) = self.at.filter(|_| self.held) {
            let (dx, dy) = ((to.x - at.x).as_f32(), (to.y - at.y).as_f32());
            if dx.abs() > DEAD || dy.abs() > DEAD {
                self.dragged = true;
            }
        }
    }

    /// The middle button came up: after a drag that's the end of it, and otherwise it
    /// stays on. True when it's still on.
    pub fn release(&mut self) -> bool {
        self.held = false;
        if self.dragged {
            self.stop();
        }
        self.at.is_some()
    }

    /// Stops it, and the marker goes.
    pub fn stop(&mut self) {
        self.at = None;
        self.held = false;
        self.dragged = false;
    }

    /// Whether steps should start being played at `now`: it's on, and none are, or they
    /// stopped coming. Marks them started, and gives the run they're of.
    pub fn start(&mut self, now: Instant) -> Option<u64> {
        self.at?;
        if self.running && self.stepped.is_some_and(|t| now - t < STALL) {
            return None;
        }
        self.running = true;
        self.stepped = Some(now);
        self.run += 1;
        Some(self.run)
    }

    /// The pixels to scroll down for `since` since the last step of `run` (up when
    /// negative), with where the marker is, or none once it's off or another run took
    /// over. Inside the dead zone that's 0, and the steps go on.
    pub fn step(
        &mut self,
        run: u64,
        now: Instant,
        since: Duration,
    ) -> Option<(f32, Point<Pixels>)> {
        if run != self.run {
            return None;
        }
        let Some(at) = self.at else {
            self.running = false;
            self.stepped = Some(now);
            return None;
        };
        self.running = true;
        self.stepped = Some(now);
        let dy = (self.pointer.y - at.y).as_f32();
        Some((speed(dy) * since.min(LONGEST).as_secs_f32(), at))
    }
}

#[cfg(test)]
mod tests {
    use super::{cursor, speed, Auto, DEAD, STALL};
    use gpui_kit::{point, px, PanCursor};
    use std::time::{Duration, Instant};

    #[test]
    fn the_speed_is_chromes() {
        assert_eq!(speed(0.0), 0.0);
        assert_eq!(speed(DEAD), 0.0);
        assert_eq!(speed(-DEAD), 0.0);
        // Chromium's own numbers, in pixels a second: 0.04 × distance ^ 2.2.
        for (dy, want) in [
            (16.0, 17.83),
            (30.0, 71.08),
            (100.0, 1004.8),
            (200.0, 4617.0),
        ] {
            let got = speed(dy);
            assert!(
                (got / want - 1.0).abs() < 0.01,
                "{dy} px: {got}, not {want}"
            );
            assert_eq!(speed(-dy), -got, "up the same");
        }
        let (near, far) = (speed(50.0), speed(100.0));
        assert!(far > near * 4.0, "it grows faster than the distance");
    }

    #[test]
    fn the_cursor_shows_the_way_it_scrolls() {
        assert_eq!(cursor(0.0), PanCursor::UpDown);
        assert_eq!(cursor(DEAD), PanCursor::UpDown);
        assert_eq!(cursor(-DEAD - 1.0), PanCursor::Up);
        assert_eq!(cursor(DEAD + 1.0), PanCursor::Down);
        let mut a = Auto::default();
        assert_eq!(a.cursor(), None, "none while it's off");
        a.press(point(px(100.0), px(100.0)));
        assert_eq!(a.cursor(), Some(PanCursor::UpDown));
        a.moved(point(px(100.0), px(40.0)));
        assert_eq!(a.cursor(), Some(PanCursor::Up));
    }

    #[test]
    fn a_click_leaves_it_on_and_a_drag_stops_on_release() {
        let at = point(px(100.0), px(100.0));
        let mut a = Auto::default();
        a.press(at);
        a.moved(point(px(110.0), px(112.0)));
        assert!(a.release(), "let go in the dead zone, it stays on");
        a.moved(point(px(100.0), px(300.0)));
        assert_eq!(a.at(), Some(at), "moving after it doesn't make it a drag");

        a.press(at);
        a.moved(point(px(100.0), px(160.0)));
        a.moved(point(px(100.0), px(100.0)));
        assert!(a.held());
        assert!(
            !a.release(),
            "out of the dead zone held, it stops on release, wherever that is"
        );
        assert_eq!(a.at(), None);
    }

    #[test]
    fn steps_scroll_by_the_time_and_stop_when_it_does() {
        let at = point(px(100.0), px(100.0));
        let now = Instant::now();
        let mut a = Auto::default();
        assert!(a.start(now).is_none(), "off, nothing starts");
        a.press(at);
        a.moved(point(px(100.0), px(150.0)));
        let run = a.start(now).unwrap();
        let ms = Duration::from_millis(10);
        let (dy, from) = a.step(run, now, ms).unwrap();
        assert_eq!(from, at, "scrolls where the marker is");
        assert!((dy - speed(50.0) * 0.01).abs() < 0.001, "{dy}");
        let (late, _) = a.step(run, now, Duration::from_secs(2)).unwrap();
        assert!(
            (late - speed(50.0) * 0.05).abs() < 0.001,
            "a late frame doesn't jump"
        );
        a.moved(point(px(100.0), px(95.0)));
        assert_eq!(a.step(run, now, ms).unwrap().0, 0.0);
        a.stop();
        assert!(a.step(run, now, ms).is_none());
    }

    #[test]
    fn lost_steps_start_again_and_only_one_run_plays() {
        let now = Instant::now();
        let ms = Duration::from_millis(8);
        let mut a = Auto::default();
        a.press(point(px(0.0), px(0.0)));
        let first = a.start(now).unwrap();
        assert!(a.start(now + ms).is_none(), "steps are coming");
        assert!(a.step(first, now + ms, ms).is_some());
        // No step for a while, as when the frame they waited for never came.
        let second = a.start(now + STALL * 2).expect("they start again");
        assert!(
            a.step(first, now + STALL * 2, ms).is_none(),
            "the lost run, coming after all, stops"
        );
        assert!(a.step(second, now + STALL * 2, ms).is_some());
    }
}
