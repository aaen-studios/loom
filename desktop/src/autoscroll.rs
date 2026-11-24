//! Middle-click scrolling in the chat, as a browser has it. The middle button sets a
//! marker where it's pressed, and the chat then scrolls each frame, faster the further the
//! pointer is above or below it. Released after a drag, it stops; released where it was
//! pressed, it stays on until the next click, key or wheel.

use std::time::{Duration, Instant};

use gpui_kit::*;

/// How far from the marker, up or down, the pointer can be with nothing scrolling.
pub const DEAD: f32 = 10.0;

/// Pixels a second for each pixel past `DEAD`.
const RATE: f32 = 8.0;

/// The fastest it scrolls, in pixels a second.
const MOST: f32 = 4000.0;

/// How far the pointer moves with the button held before letting go counts as the end of
/// a drag, not a click.
const SLOP: f32 = 6.0;

/// The longest a frame counts as: a frame that comes late doesn't jump the chat.
const LONGEST: Duration = Duration::from_millis(50);

/// How long without a step before the steps count as lost, as for a frame that never
/// came (`crate::smooth` has the same).
const STALL: Duration = Duration::from_millis(250);

/// How fast it scrolls for the pointer `dy` below the marker (above when negative), in
/// pixels a second: down for a positive speed.
pub fn speed(dy: f32) -> f32 {
    let past = dy.abs() - DEAD;
    if past <= 0.0 {
        return 0.0;
    }
    dy.signum() * (past * RATE).min(MOST)
}

/// The middle button's scrolling.
#[derive(Default)]
pub struct Auto {
    /// Where the marker is, in the window, while it's on.
    at: Option<Point<Pixels>>,
    /// Where the pointer is.
    pointer: Point<Pixels>,
    /// Whether the middle button is still down from the press that set the marker, and
    /// whether the pointer has moved since then.
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

    /// The pointer moved to `to`.
    pub fn moved(&mut self, to: Point<Pixels>) {
        self.pointer = to;
        if let Some(at) = self.at.filter(|_| self.held) {
            let (dx, dy) = ((to.x - at.x).as_f32(), (to.y - at.y).as_f32());
            if dx.hypot(dy) > SLOP {
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
    use super::{speed, Auto, DEAD, MOST, STALL};
    use gpui_kit::{point, px};
    use std::time::{Duration, Instant};

    #[test]
    fn the_speed_grows_with_the_distance_past_a_dead_zone() {
        assert_eq!(speed(0.0), 0.0);
        assert_eq!(speed(DEAD), 0.0);
        assert_eq!(speed(-DEAD), 0.0);
        let (near, far) = (speed(30.0), speed(120.0));
        assert!(
            near > 0.0 && far > near,
            "down, and faster further: {near} {far}"
        );
        assert!(
            (far / near - (120.0 - DEAD) / (30.0 - DEAD)).abs() < 0.001,
            "in proportion"
        );
        assert_eq!(speed(-30.0), -near, "up the same");
        assert_eq!(speed(100_000.0), MOST, "to a most");
        assert_eq!(speed(-100_000.0), -MOST);
    }

    #[test]
    fn a_click_leaves_it_on_and_a_drag_stops_on_release() {
        let at = point(px(100.0), px(100.0));
        let mut a = Auto::default();
        a.press(at);
        a.moved(point(px(102.0), px(101.0)));
        assert!(a.release(), "a click without moving leaves it on");
        a.moved(point(px(100.0), px(300.0)));
        assert_eq!(a.at(), Some(at), "moving after it doesn't make it a drag");

        a.press(at);
        a.moved(point(px(100.0), px(160.0)));
        assert!(a.held());
        assert!(!a.release(), "let go after a drag, it stops");
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
