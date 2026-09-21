//! The dock: the areas a panel can live in, how big they are, and which panels
//! are stacked inside them.
//!
//! The layout is configuration rather than view state, for one reason: a panel
//! torn off into its own window is a second webview with its own JavaScript
//! heap, so a `zustand` store cannot be the source of truth for both. Rust owns
//! the layout, the UI projects it, and every change is broadcast on `loom://dock`
//! so a torn-off window and the main window cannot disagree about where a panel
//! is or how wide it is.
//!
//! What deliberately is *not* here: the panel registry. Rust does not know what
//! a `terminal` or a `browser` is, so it never invents, drops, or reorders panel
//! ids. It enforces only the structural invariants a tab stack depends on -- a
//! panel appears in one place, the active index is in range, sizes are sane --
//! which means adding a panel is a frontend change and a stale config cannot
//! lose one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Smallest a zone may be dragged, on either axis.
///
/// Below about this the panel's own header stops fitting, and a dock narrower
/// than its tab strip is worse than a closed dock.
pub const MIN_ZONE_PX: u32 = 240;

/// The chat's comfortable minimum. A zone cannot grow past this without the
/// chat giving ground, and the chat gives ground by collapsing to a spine: a
/// title, a jump-to-latest and one button to bring it back.
pub const MIN_CHAT_PX: u32 = 460;

/// Absolute ceiling, so a hand-edited config cannot ask for a 100000px zone.
pub const MAX_ZONE_PX: u32 = 2400;

/// Which window edge a zone is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockEdge {
    Left,
    Right,
    Bottom,
}

impl DockEdge {
    /// Whether this edge competes for the window's width. A bottom zone takes
    /// height instead, so it never fights a side zone for the same pixels.
    pub fn is_vertical(self) -> bool {
        matches!(self, DockEdge::Left | DockEdge::Right)
    }
}

/// One docked area: an edge, a size, and the panels stacked in it as tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DockZone {
    /// Stable id, so the UI can address a zone across a reload.
    pub id: String,
    pub edge: DockEdge,
    /// Width for a side zone, height for the bottom one, in logical pixels.
    pub size: u32,
    /// Whether the zone is showing. A closed zone keeps its panels and its
    /// size, which is what makes closing and reopening cheap.
    pub open: bool,
    /// Panel ids in tab order. Ids the registry does not recognise are left
    /// alone: they render as nothing, and removing them here would make a
    /// downgrade destructive.
    pub panels: Vec<String>,
    /// Which tab is showing. Kept in range by [`DockLayout::validated`].
    pub active: usize,
}

impl Default for DockZone {
    fn default() -> Self {
        Self {
            id: String::new(),
            edge: DockEdge::Right,
            size: 420,
            open: false,
            panels: Vec::new(),
            active: 0,
        }
    }
}

/// A complete arrangement: where the zones are and what is stacked in them.
///
/// What is deliberately *not* here: whether the rail or the hot zone are on.
/// Those are ergonomic preferences about how the dock behaves everywhere, so
/// they live in `InterfaceConfig` beside the other global UI settings. Keeping
/// them per folder would mean a rail on one project and not another, which is
/// not a distinction anyone makes on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DockLayout {
    pub zones: Vec<DockZone>,
    /// Shell profile id for this folder's terminal, once one has been chosen.
    ///
    /// Kept beside the zones rather than in the terminal panel's own state
    /// because it *is* a property of the folder: reopening the same project
    /// should land in the same shell.
    pub shell: Option<String>,
}

impl Default for DockLayout {
    /// **Every zone closed**, and that is a correction rather than a shrug.
    ///
    /// This shipped with the left zone open, reasoned as "the chats list is the
    /// sidebar, so it should not have to be summoned". The reasoning was wrong
    /// in the way that matters: the app was deciding, on every launch, that the
    /// user wanted a panel — and a panel is a thing you *choose* to look at. The
    /// distinction the old comment missed is that a sidebar being *available* is
    /// not the same as it being *open*.
    ///
    /// It also had a cost the comment claimed it did not. Zones overlay the
    /// transcript rather than reflowing it, so an open panel never moved a word
    /// — true, and beside the point: it still covered the first 300px of the
    /// conversation on every launch, and the only way to read the beginning of a
    /// reply was to close it first.
    ///
    /// So the default is now "Loom opens on the conversation", and every panel
    /// is one click or one `Ctrl+`` away. `resolve()` in `panels.rs` enforces the
    /// same rule for configs already on disk, because this default was what got
    /// persisted into them.
    fn default() -> Self {
        Self {
            zones: vec![
                DockZone {
                    id: "left".into(),
                    edge: DockEdge::Left,
                    size: 300,
                    open: false,
                    // Git first, then the chats list. Both live here because the
                    // left edge is where a project's *state* belongs — what has
                    // changed, and what you are working on — while the right
                    // edge is where you watch something run.
                    //
                    // Git leads because it is the panel with a job to do that
                    // has no other home: the chats list is also reachable from
                    // the title bar and `Ctrl+K`, so anything that opens the
                    // left zone on it is one click from the list anyway.
                    //
                    // **The zone still opens closed**, and the test below is
                    // what holds that line. This changes *what* the first click
                    // shows, not whether the app decides to show it.
                    panels: vec!["git".into(), "sessions".into()],
                    active: 0,
                },
                DockZone {
                    id: "right".into(),
                    edge: DockEdge::Right,
                    size: 460,
                    open: false,
                    panels: vec!["terminal".into()],
                    active: 0,
                },
                DockZone {
                    id: "bottom".into(),
                    edge: DockEdge::Bottom,
                    size: 260,
                    open: false,
                    panels: vec!["runs".into()],
                    active: 0,
                },
            ],
            shell: None,
        }
    }
}

/// The widest a zone on `edge` may be, given the window and what else is
/// already docked on the same axis.
///
/// `reserved` is the space the other zones on that axis are taking, so two
/// side zones cannot between them squeeze the chat to nothing. The floor is
/// [`MIN_ZONE_PX`]: in a window too small for both the chat and a usable zone,
/// the zone wins and the chat collapses to its spine. That is the deliberate
/// choice -- someone dragging a dock open in a narrow window is asking for the
/// dock -- and it is why this can return a value that leaves the chat under
/// [`MIN_CHAT_PX`].
pub fn max_zone_size(window_extent: u32, reserved: u32) -> u32 {
    window_extent
        .saturating_sub(MIN_CHAT_PX.saturating_add(reserved))
        .max(MIN_ZONE_PX)
        .min(MAX_ZONE_PX)
}

/// A zone size the window can actually honour.
pub fn clamp_zone_size(desired: u32, window_extent: u32, reserved: u32) -> u32 {
    desired.clamp(MIN_ZONE_PX, max_zone_size(window_extent, reserved))
}

impl DockLayout {
    /// Repairs a layout that came from disk, from a hand edit, or from a
    /// frontend that was interrupted mid-drag.
    ///
    /// Every rule here exists to keep a tab stack renderable: a duplicate panel
    /// would mount twice, an out-of-range index would blank the zone, and a
    /// size outside the constants would make the splitter un-grabbable. The
    /// repair is silent because the alternative -- refusing the config -- would
    /// leave the user with a layout they cannot fix from the UI.
    pub fn validated(mut self) -> Self {
        let mut seen_zones: Vec<String> = Vec::new();
        let mut seen_panels: Vec<String> = Vec::new();

        self.zones.retain_mut(|zone| {
            if zone.id.is_empty() || seen_zones.contains(&zone.id) {
                return false;
            }
            seen_zones.push(zone.id.clone());

            // A panel belongs to exactly one zone. First zone in the list wins,
            // which keeps the result stable across reloads rather than
            // depending on hash order.
            zone.panels
                .retain(|panel| !panel.is_empty() && !seen_panels.contains(panel));
            for panel in &zone.panels {
                seen_panels.push(panel.clone());
            }

            zone.size = zone.size.clamp(MIN_ZONE_PX, MAX_ZONE_PX);
            zone.active = if zone.panels.is_empty() {
                0
            } else {
                zone.active.min(zone.panels.len() - 1)
            };
            true
        });

        self
    }

    /// Whether any zone is showing.
    pub fn is_open(&self) -> bool {
        self.zones.iter().any(|zone| zone.open)
    }

    /// The zone a plain toggle acts on: the first open one, or the right zone.
    ///
    /// One keystroke has to mean one thing, and "the panel dock" is the right
    /// zone far more often than not -- it is where the terminal lives.
    pub fn primary_index(&self) -> Option<usize> {
        if let Some(index) = self.zones.iter().position(|zone| zone.open) {
            return Some(index);
        }
        self.zones
            .iter()
            .position(|zone| zone.edge == DockEdge::Right)
            .or(if self.zones.is_empty() { None } else { Some(0) })
    }

    pub fn zone(&self, id: &str) -> Option<&DockZone> {
        self.zones.iter().find(|zone| zone.id == id)
    }

    /// Moves `panel` into `zone_id` at `index`, taking it out of wherever it
    /// was. Returns whether anything moved.
    ///
    /// A panel can only be in one zone, so a move is a remove and an insert.
    /// When the panel leaves a zone that now has nothing in it, that zone is
    /// closed: an empty open zone is a strip of chrome with nothing under it.
    pub fn move_panel(&mut self, panel: &str, zone_id: &str, index: usize) -> bool {
        if self.zone(zone_id).is_none() {
            return false;
        }
        let mut from: Option<String> = None;
        for zone in &mut self.zones {
            if let Some(position) = zone.panels.iter().position(|entry| entry == panel) {
                from = Some(zone.id.clone());
                let was_active = zone.active == position;
                zone.panels.remove(position);
                if was_active {
                    zone.active = zone.active.min(zone.panels.len().saturating_sub(1));
                } else if position < zone.active {
                    zone.active -= 1;
                }
            }
        }

        let target = self
            .zones
            .iter_mut()
            .find(|zone| zone.id == zone_id)
            .expect("zone existence checked above");
        let at = index.min(target.panels.len());
        target.panels.insert(at, panel.to_string());
        // Moving a panel to a zone is also asking to see it there.
        target.active = at;
        target.open = true;

        if let Some(from_id) = from {
            if from_id != target.id {
                if let Some(zone) = self.zones.iter_mut().find(|zone| zone.id == from_id) {
                    if zone.panels.is_empty() {
                        zone.open = false;
                    }
                }
            }
        }
        true
    }

    /// Closes a tab, closing the zone when it was the last one.
    pub fn close_panel(&mut self, zone_id: &str, panel: &str) -> bool {
        let Some(zone) = self.zones.iter_mut().find(|zone| zone.id == zone_id) else {
            return false;
        };
        let Some(position) = zone.panels.iter().position(|entry| entry == panel) else {
            return false;
        };
        zone.panels.remove(position);
        if zone.panels.is_empty() {
            zone.active = 0;
            zone.open = false;
        } else if position <= zone.active {
            zone.active = zone.active.saturating_sub(1).min(zone.panels.len() - 1);
        }
        true
    }
}

/// Terminal appearance.
///
/// Apart from the transcript, the terminal is the one surface where the user's
/// idea of readable beats ours: a shell is read for hours and the right size
/// depends on the monitor, the eyes, and the command. A bundled default, then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalConfig {
    pub font_family: String,
    /// Logical pixels.
    pub font_size: u32,
    /// Percentage, as CSS line-height. 130 is a shell's usual breathing room.
    pub line_height: u32,
    /// Only offered when the WebView reports WebGL2 and the user opts in.
    pub webgl: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono".to_string(),
            font_size: 13,
            line_height: 130,
            webgl: false,
        }
    }
}

/// The per-folder layouts, keyed by workspace path.
///
/// Keyed by path because that is how a chat already identifies its folder, and
/// a folder renamed in the UI keeps its layout. Entries for folders that no
/// longer exist are left in place rather than pruned: re-adding a project puts
/// the dock back the way it was.
pub type DockLayouts = BTreeMap<String, DockLayout>;

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(id: &str, edge: DockEdge, panels: &[&str]) -> DockZone {
        DockZone {
            id: id.into(),
            edge,
            panels: panels.iter().map(|p| p.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn default_opens_nothing_at_all() {
        let layout = DockLayout::default();
        // **No zone open.** This is the whole default, stated once: Loom opens on
        // the conversation, and every panel is one click or one `Ctrl+`` away.
        //
        // The assertion this replaces was `assert_eq!(open, vec!["left"])`, and
        // the history is worth keeping because the same argument got it wrong
        // twice. It was first `!is_open()` — correct while every panel was an
        // overlay you summoned. Then the dock landed and it became "the chats
        // list is open", on the reasoning that navigation should not have to be
        // summoned. That is where the reasoning broke: *available* navigation and
        // *open* navigation are different things, and the app was choosing the
        // second on the user's behalf on every launch.
        //
        // If someone wants this changed back, they should have to change this
        // test and delete this paragraph — not discover that a config default
        // quietly decided it.
        let open: Vec<&str> = layout
            .zones
            .iter()
            .filter(|zone| zone.open)
            .map(|zone| zone.id.as_str())
            .collect();
        assert!(open.is_empty(), "no zone should open by default, got {open:?}");
        assert_eq!(layout.zone("left").unwrap().edge, DockEdge::Left);
        // The left zone's first tab is git, which is a statement about *content*
        // and not about visibility: it decides what the first `Ctrl+`` shows,
        // not whether anything is shown.
        assert_eq!(layout.zone("left").unwrap().panels[0], "git");
        assert_eq!(layout.zone("left").unwrap().panels[1], "sessions");
        assert_eq!(layout.shell, None);
        // Every default zone carries a panel, so opening one shows something
        // rather than an empty frame. A zone with no panels renders as a strip
        // of chrome over a gap.
        for zone in &layout.zones {
            assert!(!zone.panels.is_empty(), "{} has no panels", zone.id);
        }
        assert_eq!(layout.zone("right").unwrap().edge, DockEdge::Right);
        assert_eq!(layout.zones.len(), 3);
    }

    #[test]
    fn toggle_targets_the_right_zone_when_nothing_is_open() {
        // What a toggle falls back to when no zone is showing. It no longer needs
        // to close everything first — that is the default now — so this asserts
        // the fallback against the shipped layout directly, which is stronger:
        // the fallback and the default can no longer drift apart without this
        // failing.
        let layout = DockLayout::default();
        assert!(!layout.is_open());
        let index = layout.primary_index().unwrap();
        assert_eq!(layout.zones[index].edge, DockEdge::Right);
    }

    #[test]
    fn toggle_prefers_a_zone_that_is_already_open() {
        // An open zone is preferred over the right-edge fallback. This has to
        // open `zones[0]` explicitly, because nothing is open by default.
        let mut layout = DockLayout::default();
        layout.zones[0].open = true;
        let index = layout.primary_index().unwrap();
        assert_eq!(layout.zones[index].edge, DockEdge::Left);
    }

    #[test]
    fn bottom_zones_do_not_compete_for_width() {
        assert!(DockEdge::Left.is_vertical());
        assert!(DockEdge::Right.is_vertical());
        assert!(!DockEdge::Bottom.is_vertical());
    }

    #[test]
    fn clamp_keeps_the_chat_room_when_the_window_is_generous() {
        // 1600 wide, nothing else docked: the chat keeps MIN_CHAT_PX, so the
        // zone may take the rest.
        assert_eq!(max_zone_size(1600, 0), 1140);
        assert_eq!(clamp_zone_size(900, 1600, 0), 900);
        // Above the ceiling, the ceiling holds.
        assert_eq!(clamp_zone_size(2000, 1600, 0), 1140);
    }

    #[test]
    fn clamp_accounts_for_the_other_zone_on_the_same_axis() {
        // A 300px left zone is already taking width, so the right one gets
        // 1600 - 460 - 300.
        assert_eq!(max_zone_size(1600, 300), 840);
        assert_eq!(clamp_zone_size(1000, 1600, 300), 840);
    }

    #[test]
    fn clamp_never_returns_less_than_a_usable_zone() {
        // A tiny window cannot honour both. The zone wins its minimum and the
        // chat collapses to a spine -- dragging a dock open in a narrow window
        // is a request for the dock.
        assert_eq!(clamp_zone_size(600, 700, 0), MIN_ZONE_PX);
        assert_eq!(max_zone_size(300, 0), MIN_ZONE_PX);
        assert!(clamp_zone_size(10, 700, 0) >= MIN_ZONE_PX);
    }

    #[test]
    fn validated_drops_a_zone_with_no_id() {
        let layout = DockLayout {
            zones: vec![
                DockZone {
                    id: String::new(),
                    ..Default::default()
                },
                zone("right", DockEdge::Right, &["terminal"]),
            ],
            ..Default::default()
        }
        .validated();
        assert_eq!(layout.zones.len(), 1);
        assert_eq!(layout.zones[0].id, "right");
    }

    #[test]
    fn validated_keeps_a_panel_in_exactly_one_zone() {
        // A drag interrupted mid-flight, or a hand-edited file, could name the
        // same panel twice. Mounting a terminal twice would open two shells.
        let layout = DockLayout {
            zones: vec![
                zone("left", DockEdge::Left, &["sessions", "terminal"]),
                zone("right", DockEdge::Right, &["terminal", "runs"]),
            ],
            ..Default::default()
        }
        .validated();
        assert_eq!(layout.zones[0].panels, vec!["sessions", "terminal"]);
        assert_eq!(layout.zones[1].panels, vec!["runs"]);
        // And the surviving tab is still the one that was showing.
        assert_eq!(layout.zones[0].active, 0);
    }

    #[test]
    fn validated_pulls_the_active_index_back_into_range() {
        let layout = DockLayout {
            zones: vec![DockZone {
                active: 7,
                ..zone("right", DockEdge::Right, &["terminal", "runs"])
            }],
            ..Default::default()
        }
        .validated();
        assert_eq!(layout.zones[0].active, 1);

        // An empty zone cannot have an active tab at all.
        let empty = DockLayout {
            zones: vec![DockZone {
                active: 3,
                ..zone("right", DockEdge::Right, &[])
            }],
            ..Default::default()
        }
        .validated();
        assert_eq!(empty.zones[0].active, 0);
    }

    #[test]
    fn validated_clamps_a_hand_edited_size() {
        let layout = DockLayout {
            zones: vec![
                DockZone {
                    size: 99_999,
                    ..zone("right", DockEdge::Right, &["terminal"])
                },
                DockZone {
                    size: 1,
                    ..zone("bottom", DockEdge::Bottom, &["runs"])
                },
            ],
            ..Default::default()
        }
        .validated();
        assert_eq!(layout.zones[0].size, MAX_ZONE_PX);
        assert_eq!(layout.zones[1].size, MIN_ZONE_PX);
    }

    #[test]
    fn validated_leaves_panels_it_does_not_recognise_alone() {
        // Rust has no registry, and dropping ids here would make a downgrade
        // quietly destructive for a panel a newer build knew about.
        let layout = DockLayout {
            zones: vec![zone("right", DockEdge::Right, &["browser", "terminal"])],
            ..Default::default()
        }
        .validated();
        assert_eq!(layout.zones[0].panels, vec!["browser", "terminal"]);
    }

    #[test]
    fn move_panel_takes_it_out_of_the_zone_it_was_in() {
        let mut layout = DockLayout {
            zones: vec![
                DockZone {
                    open: true,
                    ..zone("left", DockEdge::Left, &["sessions"])
                },
                DockZone {
                    open: true,
                    ..zone("right", DockEdge::Right, &["terminal", "runs"])
                },
            ],
            ..Default::default()
        };
        assert!(layout.move_panel("terminal", "left", 0));
        assert_eq!(layout.zones[0].panels, vec!["terminal", "sessions"]);
        assert_eq!(layout.zones[1].panels, vec!["runs"]);
        // Landing in a zone is also asking to see it.
        assert_eq!(layout.zones[0].active, 0);
    }

    #[test]
    fn move_panel_closes_a_zone_it_empties() {
        let mut layout = DockLayout {
            zones: vec![
                DockZone {
                    open: true,
                    ..zone("left", DockEdge::Left, &["sessions"])
                },
                DockZone {
                    open: true,
                    ..zone("right", DockEdge::Right, &["terminal"])
                },
            ],
            ..Default::default()
        };
        assert!(layout.move_panel("sessions", "right", 1));
        assert_eq!(layout.zones[0].panels, Vec::<String>::new());
        // An open zone with nothing in it is a strip of chrome over a gap.
        assert!(!layout.zones[0].open);
        assert_eq!(layout.zones[1].panels, vec!["terminal", "sessions"]);
        assert_eq!(layout.zones[1].active, 1);
    }

    #[test]
    fn a_panel_moved_between_zones_keeps_its_position_when_one_closes() {
        // Two panels in the right zone, the second showing. Moving the *first*
        // out must leave the second showing and still be the one that was
        // showing, not a neighbour that took its index.
        let mut layout = DockLayout {
            zones: vec![
                DockZone {
                    open: true,
                    active: 1,
                    ..zone("right", DockEdge::Right, &["terminal", "runs"])
                },
                DockZone {
                    open: true,
                    ..zone("bottom", DockEdge::Bottom, &["files"])
                },
            ],
            ..Default::default()
        };
        assert!(layout.move_panel("terminal", "bottom", 1));
        // "runs" was showing; it still is, now at index 0.
        assert_eq!(layout.zones[0].panels, vec!["runs"]);
        assert_eq!(layout.zones[0].active, 0);
        assert_eq!(layout.zones[1].panels, vec!["files", "terminal"]);
        // The moved panel is the one the bottom zone now shows.
        assert_eq!(layout.zones[1].active, 1);
    }

    #[test]
    fn validated_keeps_zone_order_so_ids_stay_comparable() {
        // Zone order is the order they were declared, and `primary_index` picks
        // the first open one — so a repair must not reorder, or "the zone I was
        // last in" changes across a reload.
        let layout = DockLayout {
            zones: vec![
                zone("right", DockEdge::Right, &["terminal"]),
                zone("left", DockEdge::Left, &["sessions"]),
                zone("bottom", DockEdge::Bottom, &["runs"]),
            ],
            ..Default::default()
        }
        .validated();
        let ids: Vec<&str> = layout.zones.iter().map(|z| z.id.as_str()).collect();
        assert_eq!(ids, vec!["right", "left", "bottom"]);
    }

    #[test]
    fn move_panel_refuses_an_unknown_zone() {
        let mut layout = DockLayout::default();
        assert!(!layout.move_panel("terminal", "nowhere", 0));
    }

    #[test]
    fn close_panel_closes_a_zone_that_runs_out_of_tabs() {
        let mut layout = DockLayout {
            zones: vec![DockZone {
                open: true,
                ..zone("right", DockEdge::Right, &["terminal", "runs"])
            }],
            ..Default::default()
        };
        assert!(layout.close_panel("right", "terminal"));
        assert_eq!(layout.zones[0].panels, vec!["runs"]);
        assert_eq!(layout.zones[0].active, 0);
        assert!(layout.close_panel("right", "runs"));
        assert!(!layout.zones[0].open);
        assert!(!layout.close_panel("right", "terminal"));
    }

    #[test]
    fn closing_a_tab_above_the_active_one_keeps_the_same_tab_showing() {
        let mut layout = DockLayout {
            zones: vec![DockZone {
                open: true,
                active: 2,
                ..zone("right", DockEdge::Right, &["terminal", "runs", "files"])
            }],
            ..Default::default()
        };
        assert!(layout.close_panel("right", "terminal"));
        // "files" was showing and still is, now one place to the left.
        assert_eq!(layout.zones[0].panels, vec!["runs", "files"]);
        assert_eq!(layout.zones[0].active, 1);
    }

    #[test]
    fn round_trips_through_camel_case_json() {
        let mut layout = DockLayout::default();
        layout.shell = Some("git-bash".into());
        layout.zones[1].open = true;
        let raw = serde_json::to_string(&layout).unwrap();
        // The frontend reads these names directly, so the wire format is a
        // contract and not an implementation detail.
        assert!(raw.contains("\"shell\""), "{raw}");
        assert!(raw.contains("\"size\""), "{raw}");
        assert!(raw.contains("\"active\""), "{raw}");
        let back: DockLayout = serde_json::from_str(&raw).unwrap();
        assert_eq!(back, layout);
    }

    #[test]
    fn a_partial_layout_fills_the_rest_from_defaults() {
        // A config written before a field existed must still load, which is
        // what `#[serde(default)]` on the struct is for.
        let layout: DockLayout = serde_json::from_str(r#"{ "shell": "pwsh" }"#).unwrap();
        assert_eq!(layout.shell.as_deref(), Some("pwsh"));
        assert_eq!(layout.zones.len(), DockLayout::default().zones.len());
    }

    #[test]
    fn a_layout_from_before_the_split_still_loads() {
        // `rail` and `hotZone` used to live here, then briefly on
        // `InterfaceConfig`, and are now gone entirely: the rail was removed and
        // the hover zone with it, in favour of one menu in the title bar. An
        // existing config.json still has them at this level, and the loader has
        // to ignore them rather than refuse the whole layout and silently reset
        // the user's dock.
        let layout: DockLayout = serde_json::from_str(
            r#"{ "rail": false, "hotZone": false, "zones": [ { "id": "right", "edge": "right", "size": 500, "open": true, "panels": ["terminal"], "active": 0 } ] }"#,
        )
        .unwrap();
        assert_eq!(layout.zones.len(), 1);
        assert_eq!(layout.zones[0].size, 500);
        assert!(layout.zones[0].open);
    }

    #[test]
    fn edges_serialise_lowercase() {
        let raw = serde_json::to_string(&DockEdge::Bottom).unwrap();
        assert_eq!(raw, "\"bottom\"");
    }
}
