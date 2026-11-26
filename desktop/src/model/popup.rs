//! The settings popup's shape (PLAN.md, "The settings popup"): how big it is, as it
//! opens or as the person resized it, and what a list page shows: its list, the one
//! picked from it beside the list, or one of them at a time where there isn't room.

use super::{McpServer, McpTest, Routine, SignIn};
use crate::tokens::{layout, space};

/// How much of the window the popup opens at, up to its largest.
const SHARE: f32 = 0.9;
/// The smallest the popup is in a window too small even for its least size.
const TINY: f32 = 320.0;

/// How a page lays out in the popup.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A column of rows, at a comfortable reading width, in the middle.
    Column,
    /// Tiles or cards across the whole width, such as Emoji.
    Grid,
    /// A list down the left, and the one picked from it beside it.
    List,
}

/// How big the popup is in a window `window` big: as the person last resized it
/// (`kept`), or else most of the window up to its largest. Never bigger than the window
/// leaves room for, and never smaller than its least size (what it was before it grew),
/// which a small window gets.
pub fn size(window: (f32, f32), kept: Option<(f32, f32)>) -> (f32, f32) {
    let room = (window.0 - 2.0 * space::XXL, window.1 - 2.0 * space::XXL);
    let least = (
        room.0.clamp(TINY, layout::POPUP_LEAST_W),
        room.1.clamp(TINY, layout::POPUP_LEAST_H),
    );
    let wanted = kept.unwrap_or((
        (window.0 * SHARE).min(layout::POPUP_W),
        (window.1 * SHARE).min(layout::POPUP_H),
    ));
    (
        wanted.0.min(room.0).max(least.0).round(),
        wanted.1.min(room.1).max(least.1).round(),
    )
}

/// The popup's size while its bottom-right corner is dragged from `from` to `to`,
/// having been `start`: it stays in the middle of the window, so it grows by twice as
/// much as the pointer moves, and the corner stays under the pointer. Then it's held
/// to the window as `size` holds it.
pub fn dragged(
    window: (f32, f32),
    start: (f32, f32),
    from: (f32, f32),
    to: (f32, f32),
) -> (f32, f32) {
    let wanted = (
        start.0 + 2.0 * (to.0 - from.0),
        start.1 + 2.0 * (to.1 - from.1),
    );
    size(window, Some(wanted))
}

/// How wide a list page's list is beside the one picked, in a page `width` wide, or
/// `None` where there isn't room for both, and they show one at a time.
pub fn list_width(width: f32) -> Option<f32> {
    if width < layout::LIST_LEAST + layout::DETAIL_LEAST {
        return None;
    }
    Some((width - layout::DETAIL_LEAST).clamp(layout::LIST_LEAST, layout::LIST))
}

/// What the pane beside a list page's list shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    /// The page's overview, such as Routines' week.
    Overview,
    /// The page's other pane: a new one's form, or Brains' ACP Registry.
    New,
    /// The item at this place in the list.
    Item(usize),
    /// Nothing: the list is empty, and the page has no overview.
    Nothing,
}

/// What a list page shows: its list or not, and what's beside it or in its place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shows {
    pub list: bool,
    pub pane: Pane,
}

/// What a list page shows, with the list `keys` (each item's name or id) and what the
/// person picked: `open` is `None` for nothing yet, `Some(None)` for the page's other
/// pane, and `Some(Some(key))` for an item. Side by side (`split`), nothing picked shows
/// the overview, for a page that has one, or else the first item, and an item that's
/// gone does the same. One at a time, nothing picked, or an item that's gone, shows the
/// list.
pub fn shows(split: bool, open: Option<Option<&str>>, keys: &[&str], overview: bool) -> Shows {
    let picked = match open {
        None => None,
        Some(None) => Some(Pane::New),
        Some(Some(key)) => keys.iter().position(|k| *k == key).map(Pane::Item),
    };
    match (split, picked) {
        (true, Some(pane)) => Shows { list: true, pane },
        (true, None) => Shows {
            list: true,
            pane: if overview {
                Pane::Overview
            } else if keys.is_empty() {
                Pane::Nothing
            } else {
                Pane::Item(0)
            },
        },
        (false, Some(pane)) => Shows { list: false, pane },
        (false, None) => Shows {
            list: true,
            pane: Pane::Nothing,
        },
    }
}

/// Whether a list page's find field, holding `query`, finds an item that says `text`:
/// each word typed is somewhere in it, in any case.
pub fn finds(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| text.contains(word))
}

/// How many columns of cards at least `least` wide, `gap` apart, fit in `width`: one
/// at the least.
pub fn columns(width: f32, least: f32, gap: f32) -> usize {
    (((width + gap) / (least + gap)).floor() as usize).max(1)
}

/// The dot at the end of a list row, in its state's colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dot {
    Ok,
    Warn,
    Bad,
    Quiet,
}

/// An MCP server's row in the list: the short line under its name and its dot. Its
/// last test, once there's been one, says how many tools it found or that it failed;
/// before that, whom it's signed in as, that it needs a sign-in, or what it runs.
pub fn mcp_line(m: &McpServer, test: Option<&McpTest>) -> (String, Option<Dot>) {
    let count = |n: usize| match n {
        0 => "No tools".to_string(),
        1 => "1 tool".to_string(),
        n => format!("{n} tools"),
    };
    match (test, &m.sign_in) {
        (Some(McpTest::Testing), _) => ("Testing…".into(), Some(Dot::Quiet)),
        (Some(McpTest::Works(tools, _)), _) => (count(tools.len()), Some(Dot::Ok)),
        (Some(McpTest::NeedsSignIn), _) | (None, SignIn::Needed) => {
            ("Needs you to sign in".into(), Some(Dot::Warn))
        }
        (Some(McpTest::Failed(why)), _) => (why.clone(), Some(Dot::Bad)),
        (None, SignIn::SignedIn(who)) if who.is_empty() => ("Signed in".into(), Some(Dot::Ok)),
        (None, SignIn::SignedIn(who)) => (format!("Signed in as {who}"), Some(Dot::Ok)),
        (None, SignIn::No) if m.url.is_empty() => (m.command.clone(), None),
        (None, SignIn::No) => (m.url.clone(), None),
    }
}

/// A routine's dot in the list: on, paused (none), or how its last run went wrong.
pub fn routine_dot(r: &Routine) -> Option<Dot> {
    match r.last_status.as_str() {
        "failed" => Some(Dot::Bad),
        "running" | "waiting" => Some(Dot::Warn),
        _ if !r.on || r.next == 0 => None,
        _ => Some(Dot::Ok),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_opens_at_most_of_the_window_up_to_its_largest() {
        // A big screen: its largest.
        assert_eq!(size((1920.0, 1080.0), None), (1280.0, 900.0));
        // A laptop's: nine tenths of it.
        assert_eq!(size((1280.0, 800.0), None), (1152.0, 720.0));
        // A small window: the size it was before it grew, as near the window's edges
        // as it was.
        assert_eq!(size((900.0, 700.0), None), (836.0, 636.0));
        assert_eq!(size((1000.0, 760.0), None), (900.0, 684.0));
        // A tiny one: no smaller than it ever was.
        assert_eq!(size((476.0, 480.0), None), (412.0, 416.0));
    }

    #[test]
    fn it_keeps_the_size_the_person_gave_it_as_the_window_lets_it() {
        let kept = Some((1400.0, 860.0));
        // Bigger than it opens, where the window has room.
        assert_eq!(size((2560.0, 1440.0), kept), (1400.0, 860.0));
        // A smaller window holds it in, and gives it back when it's big again.
        assert_eq!(size((1280.0, 800.0), kept), (1216.0, 736.0));
        // Never smaller than its least.
        assert_eq!(size((1920.0, 1080.0), Some((500.0, 300.0))), (900.0, 640.0));
    }

    #[test]
    fn its_corner_follows_the_pointer() {
        let window = (1920.0, 1080.0);
        // The popup stays in the middle, so a corner moved 50 makes it 100 bigger.
        assert_eq!(
            dragged(window, (1280.0, 900.0), (1600.0, 990.0), (1650.0, 960.0)),
            (1380.0, 840.0)
        );
        // Out past the window, it stops at the window's edge.
        assert_eq!(
            dragged(window, (1280.0, 900.0), (1600.0, 990.0), (1920.0, 1080.0)),
            (1856.0, 1016.0)
        );
        // And in past its least, at its least.
        assert_eq!(
            dragged(window, (1280.0, 900.0), (1600.0, 990.0), (1000.0, 400.0)),
            (900.0, 640.0)
        );
    }

    #[test]
    fn a_list_sits_beside_the_one_picked_where_there_is_room() {
        // The popup at its largest: the list at its width.
        assert_eq!(list_width(1280.0 - layout::NAV), Some(layout::LIST));
        // Less room: a narrower list, so the one picked keeps its least.
        assert_eq!(list_width(640.0), Some(260.0));
        assert_eq!(list_width(600.0), Some(layout::LIST_LEAST));
        // Too little: one at a time.
        assert_eq!(list_width(590.0), None);
    }

    #[test]
    fn side_by_side_the_first_shows_until_one_is_picked() {
        let keys = ["github", "acta", "gmail"];
        let shown = |open| shows(true, open, &keys, false);
        assert_eq!(
            shown(None),
            Shows {
                list: true,
                pane: Pane::Item(0)
            }
        );
        assert_eq!(shown(Some(Some("acta"))).pane, Pane::Item(1));
        assert_eq!(shown(Some(None)).pane, Pane::New);
        // One that's gone, deleted elsewhere or renamed: the first.
        assert_eq!(shown(Some(Some("vercel"))).pane, Pane::Item(0));
        // Nothing in the list yet.
        assert_eq!(shows(true, None, &[], false).pane, Pane::Nothing);
        assert_eq!(shows(true, Some(None), &[], false).pane, Pane::New);
    }

    #[test]
    fn a_page_with_an_overview_shows_it_until_one_is_picked() {
        let keys = ["r1", "r2"];
        assert_eq!(shows(true, None, &keys, true).pane, Pane::Overview);
        assert_eq!(
            shows(true, Some(Some("r2")), &keys, true).pane,
            Pane::Item(1)
        );
        assert_eq!(
            shows(true, Some(Some("gone")), &keys, true).pane,
            Pane::Overview
        );
        assert_eq!(shows(true, None, &[], true).pane, Pane::Overview);
    }

    #[test]
    fn one_at_a_time_the_list_shows_until_one_is_picked() {
        let keys = ["github", "acta"];
        let shown = |open| shows(false, open, &keys, false);
        assert_eq!(
            shown(None),
            Shows {
                list: true,
                pane: Pane::Nothing
            }
        );
        assert_eq!(
            shown(Some(Some("acta"))),
            Shows {
                list: false,
                pane: Pane::Item(1)
            }
        );
        assert_eq!(
            shown(Some(None)),
            Shows {
                list: false,
                pane: Pane::New
            }
        );
        // One that's gone goes back to the list.
        assert_eq!(
            shown(Some(Some("vercel"))),
            Shows {
                list: true,
                pane: Pane::Nothing
            }
        );
        // An overview doesn't take the list's place.
        assert_eq!(shows(false, None, &keys, true).pane, Pane::Nothing);
    }

    #[test]
    fn find_finds_every_word_typed() {
        assert!(finds("", "GitHub"));
        assert!(finds("git", "GitHub 12 tools"));
        assert!(finds("HUB tools", "GitHub 12 tools"));
        assert!(!finds("gitlab", "GitHub 12 tools"));
    }

    #[test]
    fn an_mcp_row_says_what_its_last_test_found() {
        let server = |url: &str, sign_in: SignIn| McpServer {
            name: "acta".into(),
            command: "npx acta-mcp".into(),
            url: url.into(),
            settings: Vec::new(),
            sign_in,
            client_id: String::new(),
            client_secret: None,
        };
        let local = server("", SignIn::No);
        assert_eq!(mcp_line(&local, None), ("npx acta-mcp".into(), None));
        let web = server("https://acta.example/mcp", SignIn::No);
        assert_eq!(mcp_line(&web, None).0, "https://acta.example/mcp");
        let tools = McpTest::Works(vec!["list_tasks".into(), "get_task".into()], None);
        assert_eq!(
            mcp_line(&web, Some(&tools)),
            ("2 tools".into(), Some(Dot::Ok))
        );
        let one = McpTest::Works(vec!["read".into()], None);
        assert_eq!(mcp_line(&web, Some(&one)).0, "1 tool");
        let failed = McpTest::Failed("It didn't start".into());
        assert_eq!(
            mcp_line(&web, Some(&failed)),
            ("It didn't start".into(), Some(Dot::Bad))
        );
        let needs = server("https://acta.example/mcp", SignIn::Needed);
        assert_eq!(mcp_line(&needs, None).1, Some(Dot::Warn));
        assert_eq!(
            mcp_line(&web, Some(&McpTest::NeedsSignIn)).1,
            Some(Dot::Warn)
        );
        let signed = server(
            "https://acta.example/mcp",
            SignIn::SignedIn("Elliot".into()),
        );
        assert_eq!(
            mcp_line(&signed, None),
            ("Signed in as Elliot".into(), Some(Dot::Ok))
        );
        assert_eq!(
            mcp_line(&signed, Some(&McpTest::Testing)).1,
            Some(Dot::Quiet)
        );
    }

    #[test]
    fn a_routine_row_dots_how_it_is() {
        let on = Routine {
            on: true,
            next: 1,
            last_status: "done".into(),
            ..Routine::default()
        };
        assert_eq!(routine_dot(&on), Some(Dot::Ok));
        let paused = Routine {
            on: false,
            ..on.clone()
        };
        assert_eq!(routine_dot(&paused), None);
        let failed = Routine {
            last_status: "failed".into(),
            ..paused.clone()
        };
        assert_eq!(routine_dot(&failed), Some(Dot::Bad));
        let running = Routine {
            last_status: "running".into(),
            ..on
        };
        assert_eq!(routine_dot(&running), Some(Dot::Warn));
    }

    #[test]
    fn cards_take_as_many_columns_as_fit() {
        assert_eq!(columns(560.0, 220.0, 8.0), 2);
        assert_eq!(columns(770.0, 220.0, 8.0), 3);
        assert_eq!(columns(100.0, 220.0, 8.0), 1);
    }
}
