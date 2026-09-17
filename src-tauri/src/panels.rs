//! The dock's own commands: where the zones are, and the terminal that lives
//! in one of them.
//!
//! Separated from `commands.rs` because it is a self-contained surface with its
//! own state (the [`PtyManager`]) and its own event stream (`loom://pty`, which
//! is far higher rate than `loom://event` and must not be multiplexed with it).
//! `commands.rs` stays the thin layer over `loom-core`'s engine; this is the
//! layer over its terminal.

use std::sync::Arc;

use base64::Engine as _;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use loom_core::config::AppConfig;
use loom_core::dock::{DockLayout, TerminalConfig};
use loom_core::pty::{PtyChunk, PtyEvent, PtyManager, ShellProfile};

use crate::commands::AppState;

/// Whose base64. Unqualified `encode` would be ambiguous with the trait.
const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

fn to_string(error: loom_core::Error) -> String {
    error.to_string()
}

/// Starts the terminal backend and the thread that forwards its output.
///
/// The sink broadcasts rather than targeting a window, and that is the whole
/// reason the manager lives in Rust: a terminal torn off into its own window is
/// a second webview talking to the same shells, so output has to reach whoever
/// is listening rather than the window that happened to open the session.
pub fn pty_manager(app: &AppHandle) -> Arc<PtyManager> {
    let handle = app.clone();
    PtyManager::new(Arc::new(move |chunk: PtyChunk| {
        let payload = match chunk.event {
            // Base64 rather than a string, because a pty stream is bytes: it
            // carries partial escape sequences and partial UTF-8 codepoints,
            // and a lossy decode here would corrupt both. xterm is handed the
            // bytes and owns the parser.
            PtyEvent::Data(bytes) => PtyWire {
                id: chunk.id,
                data: Some(B64.encode(&bytes)),
                exit: false,
            },
            PtyEvent::Exit => PtyWire {
                id: chunk.id,
                data: None,
                exit: true,
            },
        };
        let _ = handle.emit("loom://pty", payload);
    }))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PtyWire {
    id: String,
    /// Base64 pty bytes, or absent on exit.
    data: Option<String>,
    exit: bool,
}

/// A shell the terminal can start. Just the identity: the program and its
/// arguments are the backend's business, and sending them would invite the
/// frontend to invent a command line.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PtyProfile {
    pub id: String,
    pub name: String,
    /// True for the profile Loom would start when nothing has been chosen.
    pub default: bool,
}

/// The shells installed on this machine, best first.
///
/// Probes the filesystem and spawns `wsl.exe`, so the UI calls it once and
/// caches the answer rather than asking per render.
#[tauri::command]
pub fn pty_profiles() -> Vec<PtyProfile> {
    let found = loom_core::pty::profiles();
    let default = loom_core::pty::default_profile(&found).map(|profile| profile.id.clone());
    found
        .into_iter()
        .map(|profile| PtyProfile {
            default: default.as_deref() == Some(profile.id.as_str()),
            id: profile.id,
            name: profile.name,
        })
        .collect()
}

/// Resolves a stored profile id to something startable.
///
/// Falls back to the default rather than failing: a workspace remembers its
/// shell, and uninstalling Git Bash must not leave the folder's terminal
/// permanently broken. A stale preference degrading to a working shell is the
/// only acceptable outcome.
fn resolve_profile(id: &str) -> Option<ShellProfile> {
    let found = loom_core::pty::profiles();
    found
        .iter()
        .find(|profile| profile.id == id)
        .or_else(|| loom_core::pty::default_profile(&found))
        .cloned()
}

#[tauri::command]
pub fn pty_open(
    state: State<'_, AppState>,
    id: String,
    profile: String,
    workdir: Option<String>,
    rows: Option<u16>,
    cols: Option<u16>,
) -> Result<(), String> {
    let Some(profile) = resolve_profile(&profile) else {
        return Err("No shell is available on this machine.".into());
    };
    let dir = workdir.map(std::path::PathBuf::from);
    state
        .pty
        .open(&id, &profile, dir.as_deref(), rows, cols)
        .map_err(to_string)
}

/// Types into a shell.
///
/// The string is the user's own keystrokes, arriving through the webview. This
/// is deliberately not reachable from the model: it is not in `list_tools`, and
/// nothing in `loom-core` calls it. A live shell has no permission card in
/// front of it, so the only safe arrangement is that the agent cannot type.
#[tauri::command]
pub fn pty_write(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    state.pty.write(&id, data.as_bytes()).map_err(to_string)
}

#[tauri::command]
pub fn pty_resize(
    state: State<'_, AppState>,
    id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    state.pty.resize(&id, rows, cols).map_err(to_string)
}

#[tauri::command]
pub fn pty_close(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.pty.close(&id).map_err(to_string)
}

#[tauri::command]
pub fn pty_list(state: State<'_, AppState>) -> Vec<loom_core::pty::PtyInfo> {
    state.pty.list()
}

/* ---------------------------------------------------------------------------
   The dock layout

   Config is the source of truth, so the UI asks for a layout rather than
   keeping one. Reads resolve the folder's own entry, falling back to the
   default; writes store it under the folder and broadcast the resolved result,
   so every window -- including a torn-off one -- converges on the same answer
   instead of each applying its own optimistic edit.
--------------------------------------------------------------------------- */

/// The layout for a folder: its own if it has one, the default otherwise.
fn resolve(config: &AppConfig, workdir: Option<&str>) -> DockLayout {
    workdir
        .and_then(|path| config.dock.get(path))
        .cloned()
        .unwrap_or_else(|| config.dock_default.clone())
        // Repaired on the way out as well as the way in: a config older than
        // this build could hold a size this one would refuse to draw.
        .validated()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DockWire {
    /// Which folder this is for; `null` is the default layout, which every
    /// folder without its own arrangement follows.
    workdir: Option<String>,
    layout: DockLayout,
}

#[tauri::command]
pub fn dock_layout(state: State<'_, AppState>, workdir: Option<String>) -> DockLayout {
    resolve(&state.snapshot(), workdir.as_deref())
}

/// Stores a folder's arrangement and tells every window about it.
///
/// `workdir: None` writes the default. That is the deliberate one: rearranging
/// the dock with no workspace open, or an "apply to all folders" action, is
/// editing the default rather than a folder.
#[tauri::command]
pub fn set_dock_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    workdir: Option<String>,
    layout: DockLayout,
) -> Result<AppConfig, String> {
    let layout = layout.validated();
    let key = workdir.clone();
    let updated = state.mutate(move |config| match key {
        Some(path) => {
            config.dock.insert(path, layout);
        }
        None => config.dock_default = layout,
    })?;

    let resolved = resolve(&updated, workdir.as_deref());
    let _ = app.emit(
        "loom://dock",
        DockWire {
            workdir,
            layout: resolved,
        },
    );
    Ok(updated)
}

#[tauri::command]
pub fn set_terminal_settings(
    state: State<'_, AppState>,
    terminal: TerminalConfig,
) -> Result<AppConfig, String> {
    state.mutate(move |config| config.terminal = terminal)
}

/* ---------------------------------------------------------------------------
   Tear-off
--------------------------------------------------------------------------- */

/// Opens a panel in its own window, or focuses it if it is already out.
///
/// The window is just the panel: a second full Loom would mean a second chat
/// store, a second session list, and two windows that can disagree about which
/// chat is active. One panel is the honest unit of "I want that over there".
///
/// Reuses the query-parameter routing `?computer` and `?ask` already use, so
/// this needs no new entry point in the frontend.
#[tauri::command]
pub fn open_panel_window(
    app: AppHandle,
    panel: String,
    title: String,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<(), String> {
    let label = format!("panel-{panel}");
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }

    let url = format!("index.html?panel={panel}");
    let window = tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(width.unwrap_or(760.0), height.unwrap_or(520.0))
        .min_inner_size(320.0, 200.0)
        .build()
        .map_err(|e| e.to_string())?;

    // A torn-off panel is Loom's own UI, so input over it is the user using
    // Loom, not taking the machine over. Without this, clicking in a docked
    // terminal during a computer turn would look like a takeover and cancel it.
    if let Ok(hwnd) = window.hwnd() {
        loom_core::computer::set_ignored_window(hwnd.0 as isize);
    }
    Ok(())
}
