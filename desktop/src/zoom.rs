//! The picture viewer's zoom and pan (`Loom::viewer`). A picture's place in the viewer is
//! its fit: its whole, as big as the window lets it be, and never bigger than itself.
//! Zoomed, it's that place `scale` times over, its middle `pan` from the viewer's middle.
//! The wheel, the keys and a double-click set where the zoom is going (`target`), and it
//! eases there over the next frames about a point that stays put under the pointer; a
//! pinch goes there at once. Dragged, the picture follows the pointer, and past its edges
//! it gives a little, springing back once it's let go.

use std::time::Instant;

/// The most it zooms in, as many times its fit, for a picture `actual` times its fit
/// across: 8, or four times its own size where that's more.
pub fn most(actual: f32) -> f32 {
    (4.0 * actual).max(8.0)
}

/// How far its middle may be from the viewer's along one side, `fit` long at `scale` in
/// a viewer `view` long: up to its edge meeting the viewer's, and nowhere while it fits.
fn limit(fit: f32, scale: f32, view: f32) -> f32 {
    ((fit * scale - view) / 2.0).max(0.0)
}

/// A pan `v` past its limit `l` given only a little, the further the less: the picture
/// pulled past its edge.
pub fn band(v: f32, l: f32) -> f32 {
    let over = v.abs() - l;
    if over <= 0.0 {
        return v;
    }
    v.signum() * (l + over * 0.55 / (1.0 + over / 240.0))
}

/// A press on the viewer: where the pointer went down, the pan then, whether it was on
/// the picture, and whether it's moved far enough to be a drag rather than a click.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub from: (f32, f32),
    pub pan: (f32, f32),
    pub on_picture: bool,
    pub moved: bool,
}

/// One picture's zoom, while the viewer shows it. Points are from the viewer's middle.
#[derive(Clone, Debug)]
pub struct Zoom {
    /// The picture's SHA-256: another picture starts at its fit.
    pub file: String,
    /// How many times its fit it's drawn, and where that's easing to.
    pub scale: f32,
    pub target: f32,
    /// The point that stays put while it eases.
    pub anchor: (f32, f32),
    /// Its middle.
    pub pan: (f32, f32),
    pub drag: Option<Drag>,
    /// Its fit and the viewer's size, wide and tall, and the most it zooms in, as the
    /// last frame had them (`frame`).
    pub fit: (f32, f32),
    pub view: (f32, f32),
    pub most: f32,
    /// The last frame it was moved for; when its zoom last changed, which the zoom's
    /// level shows for a moment after; and when a pinch last moved it, which holds it
    /// where the fingers put it until they stop.
    pub stepped: Instant,
    pub changed: Option<Instant>,
    pub pinched: Option<Instant>,
    /// Whether its whole was already there when it first showed, so it isn't faded in
    /// over its thumbnail; none until it's shown.
    pub sharp_at_first: Option<bool>,
}

impl Zoom {
    pub fn new(file: &str) -> Zoom {
        Zoom {
            file: file.to_string(),
            scale: 1.0,
            target: 1.0,
            anchor: (0.0, 0.0),
            pan: (0.0, 0.0),
            drag: None,
            fit: (1.0, 1.0),
            view: (1.0, 1.0),
            most: 8.0,
            stepped: Instant::now(),
            changed: None,
            pinched: None,
            sharp_at_first: None,
        }
    }

    /// What this frame lays it out by.
    pub fn frame(&mut self, fit: (f32, f32), view: (f32, f32), most: f32) {
        self.fit = fit;
        self.view = view;
        self.most = most.max(1.0);
    }

    /// Whether it's zoomed in past its fit, or on its way there.
    pub fn zoomed(&self) -> bool {
        self.scale > 1.001 || self.target > 1.001
    }

    fn limits(&self) -> (f32, f32) {
        (
            limit(self.fit.0, self.scale, self.view.0),
            limit(self.fit.1, self.scale, self.view.1),
        )
    }

    /// Eases to `to`, kept between its fit and its most, keeping `at` still.
    pub fn go(&mut self, to: f32, at: (f32, f32)) {
        let to = to.clamp(1.0, self.most);
        if (to - self.target).abs() > 1e-4 {
            self.changed = Some(Instant::now());
        }
        self.target = to;
        self.anchor = at;
    }

    /// Goes to `to` at once, keeping `at` still, as a pinch does.
    pub fn set(&mut self, to: f32, at: (f32, f32)) {
        let to = to.clamp(1.0, self.most);
        if (to - self.scale).abs() > 1e-4 {
            self.changed = Some(Instant::now());
        }
        self.about(to, at);
        self.target = to;
        self.anchor = at;
        self.pinched = Some(Instant::now());
    }

    /// Scales to `to` about `at`: the point of the picture under it stays under it.
    fn about(&mut self, to: f32, at: (f32, f32)) {
        let k = to / self.scale.max(1e-4);
        self.pan = (
            at.0 - (at.0 - self.pan.0) * k,
            at.1 - (at.1 - self.pan.1) * k,
        );
        self.scale = to;
    }

    /// A double-click at `at`: back to its fit when zoomed, else in to its own size
    /// (`actual` times its fit), or twice its fit for one that fits whole, with `at` kept
    /// under the pointer.
    pub fn toggle(&mut self, at: (f32, f32), actual: f32) {
        if self.zoomed() {
            self.go(1.0, at);
        } else {
            self.go(actual.max(2.0), at);
        }
    }

    /// Moves it on to `now`: the scale eased toward its target, and once it's let go and
    /// done zooming, its middle back within its edges, so the point under the pointer
    /// stays put while it zooms. True while it's still moving.
    pub fn step(&mut self, now: Instant) -> bool {
        let dt = now
            .saturating_duration_since(self.stepped)
            .as_secs_f32()
            .min(0.1);
        self.stepped = now;
        // Most of the way in about 120 ms, at any frame rate.
        let take = 1.0 - (-dt * 18.0).exp();
        let mut moving = false;
        if (self.target - self.scale).abs() > 1e-3 {
            let to = self.scale + (self.target - self.scale) * take;
            self.about(to, self.anchor);
            moving = true;
        } else if self.scale != self.target {
            self.about(self.target, self.anchor);
        }
        let pinching = self
            .pinched
            .is_some_and(|t| now.saturating_duration_since(t).as_secs_f32() < 0.15);
        if self.drag.is_none() && !moving && !pinching {
            let (lx, ly) = self.limits();
            let goal = (self.pan.0.clamp(-lx, lx), self.pan.1.clamp(-ly, ly));
            let (dx, dy) = (goal.0 - self.pan.0, goal.1 - self.pan.1);
            if dx.abs() > 0.3 || dy.abs() > 0.3 {
                self.pan = (self.pan.0 + dx * take, self.pan.1 + dy * take);
                moving = true;
            } else {
                self.pan = goal;
            }
        }
        moving || pinching
    }

    /// The pointer went down at `at`, on the picture or beside it.
    pub fn grab(&mut self, at: (f32, f32), on_picture: bool) {
        self.drag = Some(Drag {
            from: at,
            pan: self.pan,
            on_picture,
            moved: false,
        });
    }

    /// Whether it's being dragged.
    pub fn dragged(&self) -> bool {
        self.drag.is_some_and(|d| d.moved)
    }

    /// The pointer moved to `at` while down: a drag once it's gone 4 pixels, which pans
    /// it while it's zoomed in. Past its edges it gives a little.
    pub fn pull(&mut self, at: (f32, f32)) {
        let zoomed = self.zoomed();
        let (lx, ly) = self.limits();
        let Some(d) = self.drag.as_mut() else {
            return;
        };
        let (dx, dy) = (at.0 - d.from.0, at.1 - d.from.1);
        if !d.moved && dx.hypot(dy) < 4.0 {
            return;
        }
        d.moved = true;
        if zoomed {
            self.pan = (band(d.pan.0 + dx, lx), band(d.pan.1 + dy, ly));
        }
    }

    /// The pointer came up: the press that ended, if there was one.
    pub fn let_go(&mut self) -> Option<Drag> {
        self.drag.take()
    }

    /// Pans by `by`, as a trackpad's two fingers do, kept within its edges.
    pub fn scroll(&mut self, by: (f32, f32)) {
        let (lx, ly) = self.limits();
        self.pan = (
            (self.pan.0 + by.0).clamp(-lx, lx),
            (self.pan.1 + by.1).clamp(-ly, ly),
        );
    }

    /// How big it's shown, as a share of its own size, rounded: what its level says.
    pub fn percent(&self, actual: f32) -> u32 {
        (self.scale / actual.max(1e-4) * 100.0).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn zoom(fit: (f32, f32), view: (f32, f32), most: f32) -> Zoom {
        let mut z = Zoom::new("a");
        z.frame(fit, view, most);
        z
    }

    fn settle(z: &mut Zoom) {
        let mut at = Instant::now();
        for _ in 0..200 {
            at += Duration::from_millis(16);
            z.step(at);
        }
    }

    #[test]
    fn the_point_under_the_pointer_stays_put() {
        let mut z = zoom((800.0, 600.0), (1000.0, 800.0), 8.0);
        // A point 100 right and 50 down of the middle, on a picture at its fit.
        let at = (100.0, 50.0);
        let under = ((at.0 - z.pan.0) / z.scale, (at.1 - z.pan.1) / z.scale);
        z.go(3.0, at);
        let mut t = Instant::now();
        for _ in 0..200 {
            t += Duration::from_millis(16);
            z.step(t);
            let now = ((at.0 - z.pan.0) / z.scale, (at.1 - z.pan.1) / z.scale);
            assert!(
                (now.0 - under.0).abs() < 0.5 && (now.1 - under.1).abs() < 0.5,
                "{now:?} at {}",
                z.scale
            );
        }
        assert!((z.scale - 3.0).abs() < 1e-3);
    }

    #[test]
    fn it_stays_between_its_fit_and_its_most() {
        let mut z = zoom((800.0, 600.0), (1000.0, 800.0), 8.0);
        z.go(0.2, (0.0, 0.0));
        assert_eq!(z.target, 1.0);
        z.go(50.0, (0.0, 0.0));
        assert_eq!(z.target, 8.0);
        // A 4000-pixel picture fitted to 800 is 5 times smaller than itself: up to 20.
        assert_eq!(most(5.0), 20.0);
        assert_eq!(most(0.5), 8.0);
    }

    #[test]
    fn zoomed_out_again_it_comes_back_to_the_middle() {
        let mut z = zoom((800.0, 600.0), (1000.0, 800.0), 8.0);
        z.go(4.0, (300.0, 200.0));
        settle(&mut z);
        assert!(z.pan.0.abs() > 10.0);
        z.go(1.0, (300.0, 200.0));
        settle(&mut z);
        assert_eq!(z.scale, 1.0);
        assert!(z.pan.0.abs() < 0.5 && z.pan.1.abs() < 0.5, "{:?}", z.pan);
    }

    #[test]
    fn a_drag_pans_and_gives_a_little_past_the_edge() {
        let mut z = zoom((800.0, 600.0), (1000.0, 800.0), 8.0);
        z.go(2.0, (0.0, 0.0));
        settle(&mut z);
        // 1600 wide in 1000: its middle goes up to 300 either way.
        z.grab((500.0, 400.0), true);
        z.pull((600.0, 400.0));
        assert!((z.pan.0 - 100.0).abs() < 1e-3);
        z.pull((1400.0, 400.0));
        assert!(z.pan.0 > 300.0 && z.pan.0 < 600.0, "{}", z.pan.0);
        assert!(z.let_go().is_some_and(|d| d.moved), "a drag isn't a click");
        settle(&mut z);
        assert!((z.pan.0 - 300.0).abs() < 0.5, "sprung back, {}", z.pan.0);
    }

    #[test]
    fn at_its_fit_a_press_is_a_click_and_doesnt_pan() {
        let mut z = zoom((800.0, 600.0), (1000.0, 800.0), 8.0);
        z.grab((500.0, 400.0), false);
        z.pull((502.0, 401.0));
        let d = z.let_go().unwrap();
        assert!(!d.moved && !d.on_picture);
        z.grab((500.0, 400.0), true);
        z.pull((700.0, 400.0));
        assert_eq!(z.pan, (0.0, 0.0));
        assert!(z.let_go().unwrap().moved);
    }

    #[test]
    fn a_double_click_goes_to_its_own_size_and_back() {
        let mut z = zoom((800.0, 800.0), (1000.0, 900.0), most(5.0));
        z.toggle((0.0, 0.0), 5.0);
        assert_eq!(z.target, 5.0);
        settle(&mut z);
        assert_eq!(z.percent(5.0), 100);
        z.toggle((0.0, 0.0), 5.0);
        assert_eq!(z.target, 1.0);
        // One that fits whole goes to twice its fit.
        let mut small = zoom((300.0, 200.0), (1000.0, 900.0), most(1.0));
        small.toggle((0.0, 0.0), 1.0);
        assert_eq!(small.target, 2.0);
    }

    #[test]
    fn a_pinch_goes_at_once_and_a_trackpad_pans_within_the_edges() {
        let mut z = zoom((800.0, 600.0), (1000.0, 800.0), 8.0);
        z.set(2.5, (40.0, -20.0));
        assert_eq!(z.scale, 2.5);
        assert!((z.pan.0 - (40.0 - 40.0 * 2.5)).abs() < 1e-3);
        // 2000 by 1500 in 1000 by 800: up to 500 and 350.
        z.scroll((-5000.0, 5000.0));
        assert_eq!(z.pan, (-500.0, 350.0));
    }
}
