//! Where the window was: loom opens where it was closed, the same size, maximized if
//! it was. The place is the window's normal rectangle as Windows keeps it (in physical
//! pixels, in workspace coordinates), kept in window.json in the data folder each time
//! the window moves or resizes, so a crash or an update keeps it too.

use serde_json::{json, Value};

/// A window rectangle, right and bottom past the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// How much of `self` and `other` overlap, as an area.
    fn overlap(&self, other: &Rect) -> i64 {
        let w = (self.right.min(other.right) - self.left.max(other.left)).max(0);
        let h = (self.bottom.min(other.bottom) - self.top.max(other.top)).max(0);
        w as i64 * h as i64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    /// The window's size and place when it isn't maximized.
    pub normal: Rect,
    pub maximized: bool,
}

/// Smaller than this, a kept place is taken for a mistake and the default is used.
const SMALLEST: (i32, i32) = (320, 240);
/// How much of the window's top edge, where it's dragged from, has to be on a screen
/// for it to stay where it was.
const GRIP: (i32, i32) = (120, 32);

fn file(dir: &str) -> std::path::PathBuf {
    std::path::Path::new(dir).join("window.json")
}

impl Place {
    pub fn load(dir: &str) -> Option<Place> {
        let text = std::fs::read_to_string(file(dir)).ok()?;
        Place::from_json(&serde_json::from_str(&text).ok()?)
    }

    /// Kept for next time, except by a build for measuring, whose window is off every
    /// screen: it leaves the place the app opens at alone.
    pub fn save(self, dir: &str) {
        if cfg!(feature = "perf") {
            return;
        }
        let _ = std::fs::write(file(dir), self.to_json().to_string());
    }

    pub fn from_json(v: &Value) -> Option<Place> {
        let n = |k: &str| v.get(k)?.as_i64().and_then(|n| i32::try_from(n).ok());
        let normal = Rect {
            left: n("left")?,
            top: n("top")?,
            right: n("right")?,
            bottom: n("bottom")?,
        };
        if normal.width() < SMALLEST.0 || normal.height() < SMALLEST.1 {
            return None;
        }
        Some(Place {
            normal,
            maximized: v.get("maximized").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    pub fn to_json(self) -> Value {
        json!({
            "left": self.normal.left,
            "top": self.normal.top,
            "right": self.normal.right,
            "bottom": self.normal.bottom,
            "maximized": self.maximized,
        })
    }

    /// The place, kept on the screens there are now (their work areas, the main one
    /// first). If enough of the window's top edge is on one, it stays where it was;
    /// otherwise, as when a monitor was unplugged, it's centered on the screen it
    /// overlapped most, or the main one, made to fit.
    pub fn fit(self, screens: &[Rect]) -> Place {
        let Some(main) = screens.first() else {
            return self;
        };
        let r = self.normal;
        let grip = Rect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.top + GRIP.1,
        };
        let held = screens.iter().any(|s| {
            let w = grip.right.min(s.right) - grip.left.max(s.left);
            let h = grip.bottom.min(s.bottom) - grip.top.max(s.top);
            w >= GRIP.0.min(r.width()) && h >= GRIP.1.min(r.height()) / 2
        });
        if held {
            return self;
        }
        let screen = screens
            .iter()
            .max_by_key(|s| s.overlap(&r))
            .filter(|s| s.overlap(&r) > 0)
            .unwrap_or(main);
        let w = r.width().min(screen.width());
        let h = r.height().min(screen.height());
        let left = screen.left + (screen.width() - w) / 2;
        let top = screen.top + (screen.height() - h) / 2;
        Place {
            normal: Rect {
                left,
                top,
                right: left + w,
                bottom: top + h,
            },
            maximized: self.maximized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, w: i32, h: i32) -> Rect {
        Rect {
            left,
            top,
            right: left + w,
            bottom: top + h,
        }
    }

    const MAIN: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    #[test]
    fn a_place_comes_back_as_it_was_kept() {
        let p = Place {
            normal: rect(100, 80, 1200, 800),
            maximized: true,
        };
        assert_eq!(Place::from_json(&p.to_json()), Some(p));
    }

    #[test]
    fn a_broken_or_tiny_place_is_no_place() {
        assert_eq!(Place::from_json(&json!({"left": 1})), None);
        assert_eq!(Place::from_json(&json!("x")), None);
        let tiny = Place {
            normal: rect(0, 0, 10, 10),
            maximized: false,
        };
        assert_eq!(Place::from_json(&tiny.to_json()), None);
    }

    #[test]
    fn a_window_on_a_screen_stays_where_it_was() {
        let p = Place {
            normal: rect(300, 200, 1100, 700),
            maximized: false,
        };
        assert_eq!(p.fit(&[MAIN]), p);
        // Hanging off the right edge, with its top still in reach.
        let off = Place {
            normal: rect(1600, 100, 1100, 700),
            maximized: false,
        };
        assert_eq!(off.fit(&[MAIN]), off);
    }

    #[test]
    fn a_window_on_a_monitor_that_is_gone_comes_to_the_main_one() {
        let second = rect(1920, 0, 2560, 1400);
        let p = Place {
            normal: rect(2200, 150, 1600, 1000),
            maximized: true,
        };
        // With the second monitor there, it stays.
        assert_eq!(p.fit(&[MAIN, second]), p);
        // Unplugged: centered on the main screen, at its size, still maximized.
        let back = p.fit(&[MAIN]);
        assert_eq!(back.normal, rect(160, 20, 1600, 1000));
        assert!(back.maximized);
    }

    #[test]
    fn a_window_bigger_than_its_screen_is_made_to_fit() {
        let p = Place {
            normal: rect(-3000, -2000, 2560, 1400),
            maximized: false,
        };
        assert_eq!(p.fit(&[MAIN]).normal, rect(0, 0, 1920, 1040));
    }

    #[test]
    fn a_window_whose_top_is_above_every_screen_comes_back() {
        // Its body is on the screen but its title bar, where it's dragged from, isn't.
        let p = Place {
            normal: rect(200, -300, 1000, 900),
            maximized: false,
        };
        let back = p.fit(&[MAIN]);
        assert!(back.normal.top >= 0, "{back:?}");
        assert_eq!(back.normal.width(), 1000);
    }
}
