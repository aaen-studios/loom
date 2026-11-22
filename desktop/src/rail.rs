//! The sidebar, whole or collapsed to a rail: as the person left it, or a rail on its
//! own while the window is too narrow for the sidebar and the chat. The person's own
//! choice is kept in `sidebar` in the app's data folder, as the panel's width is kept
//! in `side-width`.

use crate::app::CHAT_LEAST;
use crate::tokens::layout;

/// How long the sidebar takes to open or close, in seconds.
pub const SLIDE: f32 = 0.2;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Sidebar {
    /// Whether the person collapsed it. Kept.
    pub collapsed: bool,
    /// Whether the person opened it while the window was too narrow for it, until the
    /// window has room again. Not kept: the next narrow window collapses it again.
    pub opened_narrow: bool,
}

fn file(dir: &str) -> std::path::PathBuf {
    std::path::Path::new(dir).join("sidebar")
}

/// Whether a window `window` wide is too narrow for the whole sidebar beside the chat
/// and whatever else is open beside it, `beside` wide at the least.
pub fn narrow(window: f32, beside: f32) -> bool {
    window < layout::SIDEBAR + CHAT_LEAST + beside
}

impl Sidebar {
    /// As the person left it in `dir`, or whole if they never collapsed it.
    pub fn load(dir: &str) -> Sidebar {
        Sidebar::from_kept(&std::fs::read_to_string(file(dir)).unwrap_or_default())
    }

    pub fn save(&self, dir: &str) {
        let _ = std::fs::write(file(dir), self.kept());
    }

    fn from_kept(text: &str) -> Sidebar {
        Sidebar {
            collapsed: text.trim() == "rail",
            opened_narrow: false,
        }
    }

    fn kept(&self) -> &'static str {
        if self.collapsed {
            "rail"
        } else {
            "whole"
        }
    }

    /// Whether it's a rail now, in a window `window` wide with `beside` open beside the
    /// chat.
    pub fn railed(&self, window: f32, beside: f32) -> bool {
        self.collapsed || narrow(window, beside) && !self.opened_narrow
    }

    /// The person's collapse or open, from the control or Ctrl+B.
    pub fn toggle(&mut self, window: f32, beside: f32) {
        if self.railed(window, beside) {
            self.collapsed = false;
            self.opened_narrow = narrow(window, beside);
        } else {
            self.collapsed = true;
            self.opened_narrow = false;
        }
    }

    /// Opens it, if it's a rail, as the person asked for something only the whole
    /// sidebar shows (search).
    pub fn open(&mut self, window: f32, beside: f32) {
        if self.railed(window, beside) {
            self.toggle(window, beside);
        }
    }

    /// Follows the window: once it has room again, opening it while it was narrow is
    /// forgotten. Says whether anything changed.
    pub fn fit(&mut self, window: f32, beside: f32) -> bool {
        if self.opened_narrow && !narrow(window, beside) {
            self.opened_narrow = false;
            return true;
        }
        false
    }

    /// How wide it is: the whole sidebar's width, or the rail's.
    pub fn width(&self, window: f32, beside: f32) -> f32 {
        if self.railed(window, beside) {
            layout::RAIL
        } else {
            layout::SIDEBAR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{narrow, Sidebar};
    use crate::app::CHAT_LEAST;
    use crate::tokens::layout;

    const WIDE: f32 = 1400.0;
    const NARROW: f32 = 560.0;

    #[test]
    fn it_is_whole_until_the_person_collapses_it_and_that_is_kept() {
        assert_eq!(Sidebar::from_kept(""), Sidebar::default());
        assert!(!Sidebar::default().railed(WIDE, 0.0));
        let mut s = Sidebar::default();
        s.toggle(WIDE, 0.0);
        assert!(s.collapsed && s.railed(WIDE, 0.0));
        assert_eq!(Sidebar::from_kept(s.kept()), s);
        s.toggle(WIDE, 0.0);
        assert!(!s.railed(WIDE, 0.0));
        assert_eq!(Sidebar::from_kept(s.kept()), s);
        assert_eq!(s.width(WIDE, 0.0), layout::SIDEBAR);
    }

    #[test]
    fn a_window_too_narrow_for_the_sidebar_and_the_chat_collapses_it() {
        let least = layout::SIDEBAR + CHAT_LEAST;
        assert!(narrow(least - 1.0, 0.0));
        assert!(!narrow(least, 0.0));
        // A panel open beside the chat needs room too.
        assert!(narrow(least + 100.0, 320.0));
        let s = Sidebar::default();
        assert!(s.railed(NARROW, 0.0));
        assert_eq!(s.width(NARROW, 0.0), layout::RAIL);
        // And it comes back once there's room again.
        assert!(!s.railed(WIDE, 0.0));
    }

    #[test]
    fn the_persons_own_choice_wins() {
        // Collapsed by the person: it stays a rail when the window widens.
        let mut s = Sidebar::default();
        s.toggle(WIDE, 0.0);
        assert!(s.railed(NARROW, 0.0) && s.railed(WIDE, 0.0));
        // Opened by the person in a narrow window: it stays open while it's narrow,
        // and isn't kept collapsed.
        let mut s = Sidebar::default();
        s.toggle(NARROW, 0.0);
        assert!(!s.railed(NARROW, 0.0) && !s.collapsed);
        // Once there's room, that's forgotten, and the next narrow window collapses it.
        assert!(s.fit(WIDE, 0.0));
        assert!(s.railed(NARROW, 0.0));
        assert!(!s.fit(WIDE, 0.0));
    }

    #[test]
    fn asking_for_search_opens_a_rail() {
        let mut s = Sidebar::default();
        s.toggle(WIDE, 0.0);
        s.open(WIDE, 0.0);
        assert!(!s.railed(WIDE, 0.0));
        s.open(WIDE, 0.0);
        assert!(!s.railed(WIDE, 0.0));
    }
}
