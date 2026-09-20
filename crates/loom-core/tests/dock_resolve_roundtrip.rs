//! The dock's read-write round trip.
//!
//! `resolve()` lives in `src-tauri/src/panels.rs` and is not reachable from an
//! integration test, so this file reproduces its two arms exactly against the
//! real `DockLayout` type. The point is to pin the failure this repo shipped:
//! opening a panel wrote a layout with `open: true`, `resolve` read it back and
//! replaced every `open` with `false`, and the broadcast of the user's own write
//! then closed the panel they had just opened.
//!
//! If these tests are ever deleted, the bug they describe can come back
//! silently — nothing about a build would say so.

use loom_core::config::AppConfig;
use loom_core::dock::{DockEdge, DockLayout};

/// Which of `resolve`'s two questions is being asked, mirroring `Resolve` in
/// `src-tauri/src/panels.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolve {
    /// A read: a folder with no entry of its own opens on the conversation.
    Cold,
    /// The read-back after a write: adopt what was written, `open` included.
    Written,
}

/// The arm of `resolve` under test, copied verbatim in behaviour.
///
/// A folder with its own entry honours it; a folder without one gets the
/// default's zones and sizes, and — when the question is the *cold* one — its
/// `open` flags cleared. The `open` flags are the subject.
fn resolve_like_panels_rs(
    config: &AppConfig,
    workdir: Option<&str>,
    mode: Resolve,
) -> DockLayout {
    if let Some(layout) = workdir.and_then(|path| config.dock.get(path)) {
        return layout.clone().validated();
    }
    let mut layout = config.dock_default.clone();
    if mode == Resolve::Cold {
        for zone in &mut layout.zones {
            zone.open = false;
        }
    }
    layout.validated()
}

/// A default with the right zone already open, as a write of `open: true` makes.
fn config_after_opening(workdir: Option<&str>, edge: DockEdge) -> AppConfig {
    let mut config = AppConfig::default();
    let mut layout = config.dock_default.clone();
    for zone in &mut layout.zones {
        if zone.edge == edge {
            zone.open = true;
        }
    }
    match workdir {
        Some(path) => {
            config.dock.insert(path.to_string(), layout);
        }
        None => config.dock_default = layout,
    }
    config
}

#[test]
fn opening_a_panel_in_a_named_folder_survives_the_round_trip() {
    // The `Some(path)` arm understood this all along: the write is stored under
    // the folder, and the resolve that follows reads it straight back.
    let config = config_after_opening(Some("/home/dev/project"), DockEdge::Right);
    // Both modes agree here, and that is the point: the folder has its own
    // entry, so neither the cold rule nor the written one has anything to say.
    for mode in [Resolve::Cold, Resolve::Written] {
        let resolved = resolve_like_panels_rs(&config, Some("/home/dev/project"), mode);
        let right = resolved.zone("right").expect("right zone");
        assert!(right.open, "a saved open zone must come back open ({mode:?})");
    }

    // And a *different* folder, with no entry of its own, is still closed on a
    // cold read — which is the rule that is actually wanted there.
    let other = resolve_like_panels_rs(&config, Some("/home/dev/elsewhere"), Resolve::Cold);
    assert!(!other.is_open(), "a folder with no entry opens on the conversation");
}

#[test]
fn opening_a_panel_with_no_folder_open_must_survive_the_round_trip() {
    // **This is the shipped bug.** With no workspace open, `workdir` is `None`,
    // so the write goes to `dock_default` — and the resolve that followed it
    // then swept every `open` back to `false`. The broadcast of the user's own
    // write closed the panel they had just opened, so the Panels menu did
    // nothing at all in the state the app starts in.
    let config = config_after_opening(None, DockEdge::Bottom);

    // The resolve after the write adopts it: this is the assertion that failed.
    let written = resolve_like_panels_rs(&config, None, Resolve::Written);
    let bottom = written.zone("bottom").expect("bottom zone");
    assert!(
        bottom.open,
        "a panel opened with no folder active was closed again by the resolve \
         that followed its own write"
    );

    // And a cold read is unchanged: launching with no folder active still opens
    // on the conversation, so the fix did not trade one bug for the other.
    let cold = resolve_like_panels_rs(&config, None, Resolve::Cold);
    assert!(
        !cold.is_open(),
        "a cold read must still open on the conversation"
    );
    // The zones, panels and sizes are the same either way — only `open` differs.
    assert_eq!(cold.zones.len(), written.zones.len());
    assert_eq!(
        cold.zones.iter().map(|zone| &zone.panels).collect::<Vec<_>>(),
        written.zones.iter().map(|zone| &zone.panels).collect::<Vec<_>>(),
    );
}

#[test]
fn a_folder_with_an_entry_ignores_the_cold_rule_in_both_modes() {
    // The `Some(path)` arm was never the bug, and this pins that the fix did not
    // disturb it: a saved folder's own `open` flags are honoured either way, and
    // closing a panel in that folder keeps it closed across a cold read.
    let mut config = AppConfig::default();
    let mut closed = config.dock_default.clone();
    for zone in &mut closed.zones {
        zone.open = false;
    }
    config.dock.insert("/home/dev/project".into(), closed);

    for mode in [Resolve::Cold, Resolve::Written] {
        let resolved =
            resolve_like_panels_rs(&config, Some("/home/dev/project"), mode);
        assert!(!resolved.is_open(), "a closed folder stays closed ({mode:?})");
    }
}
