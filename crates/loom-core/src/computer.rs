//! Computer use: seeing the screen and driving the machine.
//!
//! Eleven tools, offered only in chats whose Computer chip is on: `screenshot`
//! (eyes), `mouse` / `keyboard` / `clipboard` (hands), `window` / `list_windows`
//! / `launch_app` / `list_processes` / `kill_process` (the desktop), `ui`
//! (accessibility), and `wait`. The chip is the standing consent, so these run
//! without per-call cards; the user's own input pauses the whole turn.
//!
//! Implemented on Windows in `computer/windows.rs`. Other platforms compile
//! against a fallback that refuses every action.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tools::{ToolCall, ToolImage, ToolOutcome, ToolScope, ToolSpec};
use crate::{Error, Result};

/// Milliseconds since the Unix epoch, matching the engine's clock.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// What the model is told when it reaches for a computer tool in a chat whose
/// Computer chip is off (it should not even see the tool, but old plans do).
pub const DISABLED_NOTE: &str = "Computer control is off for this chat. Ask the user to enable \
    the Computer chip in the composer; do not call computer tools until then.";

/// How long a screenshot stays young enough to click on. Screen layouts and
/// windows move; stale pixels are worse than no pixels.
const SHOT_TTL_MS: u64 = 120_000;

/// Longest text `keyboard::type` hands to the clipboard staging path before
/// typing it directly. Typing is layout-independent but slower per character.
const CLIPBOARD_STAGE_CHARS: usize = 400;

#[cfg(windows)]
#[path = "computer/windows.rs"]
mod imp;
#[cfg(not(windows))]
#[path = "computer/fallback.rs"]
mod imp;

pub use imp::{set_ignored_window, TakeoverWatch};

/// Where the last screenshot came from, so image pixels can be mapped back to
/// screen pixels for the next click.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotMeta {
    /// Virtual-desktop rect captured: x, y, width, height.
    pub rect: (i32, i32, u32, u32),
    pub image_w: u32,
    pub image_h: u32,
    /// Image pixels per screen pixel (post-downscale).
    pub scale: f64,
    /// Monitor index this shot mostly came from.
    pub monitor: i32,
    pub taken_ms: u64,
}

impl ShotMeta {
    fn stale(&self) -> bool {
        now_ms().saturating_sub(self.taken_ms) > SHOT_TTL_MS
    }
}

/// One node of the UI Automation tree, cached between calls of the same turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiNode {
    /// Path id, e.g. `0.2.1`, resolvable until the tree is re-read.
    pub id: String,
    /// Child indices from the window root.
    pub path: Vec<u32>,
    pub name: String,
    pub role: String,
    /// Screen-pixel rectangle: x, y, width, height.
    pub bounds: (i32, i32, u32, u32),
    pub enabled: bool,
    pub offscreen: bool,
    /// Pattern names the element supports, e.g. `invoke`, `value`, `toggle`.
    pub patterns: Vec<String>,
}

/// Per-chat computer state. One mouse, many chats.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerState {
    pub last_shot: Option<ShotMeta>,
    pub last_click: Option<(i32, i32)>,
    pub held_buttons: [bool; 3],
    pub held_keys: Vec<u16>,
    pub last_ui: Option<Vec<UiNode>>,
    pub last_ui_ms: u64,
    /// Window (hwnd) the cached UI tree came from, so actions re-walk the same
    /// window even if focus moved.
    pub last_ui_window: Option<isize>,
    /// The last encoded screenshot, reused when frame and pointer are
    /// unchanged: at native resolution, encoding is the slow part of a shot.
    #[serde(skip)]
    pub cached: Option<CachedFrame>,
}

/// One encoded frame, kept only in memory.
#[derive(Debug, Clone)]
pub struct CachedFrame {
    pub hash: u64,
    pub cursor: Option<(i32, i32)>,
    pub bytes: Vec<u8>,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// Everything a capture request can say.
#[derive(Debug, Clone)]
pub struct Capture {
    /// `active`, `monitor`, `window`, `all`, `region`.
    pub target: String,
    pub monitor: Option<i32>,
    pub window: Option<String>,
    /// Screen pixels.
    pub region: Option<(i32, i32, u32, u32)>,
    /// Region zoom, 1-3.
    pub scale: u32,
    /// Wait until two consecutive frames match, up to this long.
    pub settle_ms: u64,
    pub cursor: bool,
}

/// Per-turn knobs the engine hands to the tools.
pub struct ComputerOptions {
    /// Longest edge of a screenshot; `0` keeps native resolution.
    pub screenshot_edge: u32,
    /// The turn's cancellation flag, so `wait` stops with the turn.
    pub cancel: Option<crate::providers::stream::Cancellation>,
}

impl Default for ComputerOptions {
    fn default() -> Self {
        Self {
            screenshot_edge: 0,
            cancel: None,
        }
    }
}

/// A prepared capture plus the coordinate contract handed to the model.
pub struct CaptureResult {
    pub image: image::RgbaImage,
    /// Virtual-desktop rect the image covers.
    pub rect: (i32, i32, u32, u32),
    pub monitor: i32,
    /// One human-readable line per monitor, for the tool output.
    pub monitors: Vec<String>,
    pub cursor: Option<(i32, i32)>,
    pub frame_hash: u64,
}

pub fn is_computer_tool(name: &str) -> bool {
    matches!(
        name,
        "screenshot"
            | "ui"
            | "mouse"
            | "keyboard"
            | "clipboard"
            | "list_windows"
            | "window"
            | "list_processes"
            | "launch_app"
            | "kill_process"
            | "wait"
    )
}

pub fn is_read_only(name: &str) -> bool {
    matches!(
        name,
        "screenshot" | "list_windows" | "list_processes" | "wait"
    )
}

fn tool_spec(
    name: &'static str,
    description: &'static str,
    parameters: Value,
    read_only: bool,
) -> ToolSpec {
    ToolSpec {
        name,
        description,
        parameters,
        read_only,
        scope: Some(ToolScope::Computer),
    }
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "screenshot",
            "See the screen. Returns an image plus the numbers needed to click on it: the image size, the virtual-desktop rect it covers, and the image-pixels-per-screen-pixel scale. Mouse coordinates are always image pixels from the most recent screenshot. Take one before you act, and again to verify what happened. `active` (the default) is the monitor with the focused window; `monitor` takes an index from the list in the result; `window` captures one window by hwnd or title; `all` is every monitor stitched; `region` crops screen pixels and can zoom 1-3x to read small text.",
            json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "enum": ["active", "monitor", "window", "all", "region"], "description": "What to capture (default active)" },
                    "monitor": { "type": "integer", "description": "Monitor index from a previous screenshot's monitor list" },
                    "window": { "type": "string", "description": "Window hwnd (0x...) or title substring, for target window" },
                    "region": {
                        "type": "object",
                        "description": "Screen-pixel rectangle, for target region",
                        "properties": {
                            "x": { "type": "integer" },
                            "y": { "type": "integer" },
                            "width": { "type": "integer" },
                            "height": { "type": "integer" }
                        },
                        "required": ["x", "y", "width", "height"],
                        "additionalProperties": false
                    },
                    "scale": { "type": "integer", "description": "Region zoom, 1-3 (default 1)" },
                    "settle_ms": { "type": "integer", "description": "Wait until the screen stops changing, up to this many ms (0-2000, default 0)" },
                    "cursor": { "type": "boolean", "description": "Draw the pointer into the image (default true)" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        tool_spec(
            "ui",
            "Drive the focused window through Windows UI Automation instead of pixels: `tree` lists its accessible elements (name, role, bounds, id, patterns); use ids with invoke/set_value/toggle/select/expand/collapse/focus/get_text. Prefer this over clicking pixels when the element has a name. Element ids expire when the tree is re-read — run `tree` again after the UI changes. `click_element` falls back to a real mouse click at the element's centre when the element itself is not invokable.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["tree", "invoke", "set_value", "toggle", "select", "expand", "collapse", "focus", "click_element", "get_text"], "description": "What to do" },
                    "window": { "type": "string", "description": "Window hwnd or title substring; defaults to the focused window" },
                    "id": { "type": "string", "description": "Element id (path) from the last tree" },
                    "value": { "type": "string", "description": "Text for set_value" },
                    "depth": { "type": "integer", "description": "Tree depth, 1-8 (default 4)" },
                    "max_nodes": { "type": "integer", "description": "Node cap, 50-500 (default 250)" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "mouse",
            "Move, click, drag, and scroll the mouse. Coordinates are image pixels from the most recent screenshot (the result tells you their scale). `drag` goes from x,y to to_x,to_y; `scroll` uses wheel notches (negative is up/left); `modifiers` are held around the action (e.g. ctrl+click). After each action, take a fresh screenshot to verify.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["move", "click", "double_click", "right_click", "middle_click", "down", "up", "drag", "scroll", "position"], "description": "What to do" },
                    "x": { "type": "integer", "description": "Image-space x" },
                    "y": { "type": "integer", "description": "Image-space y" },
                    "to_x": { "type": "integer", "description": "Image-space drag end x" },
                    "to_y": { "type": "integer", "description": "Image-space drag end y" },
                    "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "Mouse button (default left)" },
                    "count": { "type": "integer", "description": "Click count, 1-3 (default 1)" },
                    "modifiers": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["ctrl", "alt", "shift", "win"] },
                        "description": "Keys held around the action"
                    },
                    "amount": { "type": "integer", "description": "Scroll notches, -100..100 (positive is down/right)" },
                    "horizontal": { "type": "boolean", "description": "Scroll sideways" },
                    "duration_ms": { "type": "integer", "description": "Drag duration, 0-5000 (default 250)" },
                    "steps": { "type": "integer", "description": "Drag interpolation steps, 1-60 (default 12)" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "keyboard",
            "Type text and press keys. `type` handles arbitrary Unicode (long text is pasted through the clipboard, then your clipboard text is restored); `press` presses one named key (enter, esc, tab, arrows, f1-f24, media keys, letters, digits...); `combo` presses several at once (`ctrl+shift+t` or [\"ctrl\",\"shift\",\"t\"]); `down`/`up` hold keys for drags and games. Keyboard shortcuts are usually the fastest, most reliable way to act.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["type", "press", "combo", "down", "up"], "description": "What to do" },
                    "text": { "type": "string", "description": "Text to type (for action type)" },
                    "key": { "type": "string", "description": "Key name (for press/down/up)" },
                    "keys": { "type": "array", "items": { "type": "string" }, "description": "Keys to hold together (for combo), e.g. [\"ctrl\",\"c\"]" },
                    "repeat": { "type": "integer", "description": "Repeat count, 1-50 (default 1)" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "clipboard",
            "Read or write the clipboard. `write_files` puts a list of file paths on the clipboard as if you had copied them in Explorer: focus the target window and press ctrl+v to drop them into an app.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["read", "write", "write_files"], "description": "What to do" },
                    "text": { "type": "string", "description": "Text for write" },
                    "paths": { "type": "array", "items": { "type": "string" }, "description": "File paths for write_files" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "list_windows",
            "List visible top-level windows, topmost first, with their hwnd, pid, title, rectangle, and whether they are focused. Use the hwnd with `window` and `screenshot`.",
            json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Only titles containing this text" },
                    "include_hidden": { "type": "boolean", "description": "Include windows that are hidden or minimised" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        tool_spec(
            "window",
            "Act on a window by hwnd (0x...) or title substring: focus/restore it, close it (WM_CLOSE), minimise, maximise, restore, move, resize, or pin it on top. Focusing is verified: if Windows refuses (elevated target, foreground lock) you are told so instead of typing into the wrong window.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["focus", "close", "minimize", "maximize", "restore", "move", "resize", "pin"], "description": "What to do" },
                    "window": { "type": "string", "description": "Window hwnd (0x...) or title substring" },
                    "x": { "type": "integer", "description": "Screen x for move" },
                    "y": { "type": "integer", "description": "Screen y for move" },
                    "width": { "type": "integer", "description": "Width for resize" },
                    "height": { "type": "integer", "description": "Height for resize" },
                    "pin": { "type": "boolean", "description": "True to pin on top, false to unpin" }
                },
                "required": ["action", "window"],
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "list_processes",
            "List processes with pid, name, memory, and window title when they own one, largest first.",
            json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Only names containing this text" },
                    "with_windows_only": { "type": "boolean", "description": "Only processes that own a visible window" }
                },
                "additionalProperties": false
            }),
            true,
        ),
        tool_spec(
            "launch_app",
            "Open an application, document, folder, or URL (like typing it into Start). Optionally wait until a window whose title contains `wait_for_window` appears, and report its hwnd so you can focus it.",
            json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Executable path, app name, document path, folder, or URL" },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Command-line arguments" },
                    "cwd": { "type": "string", "description": "Working directory" },
                    "wait_for_window": { "type": "string", "description": "Title substring to wait for" },
                    "timeout_ms": { "type": "integer", "description": "Wait limit in ms, 0-30000 (default 10000)" }
                },
                "required": ["target"],
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "kill_process",
            "Force-stop a process by pid or exact name (case-insensitive, `.exe` optional). This can lose unsaved work. System-critical processes are refused, and names that match several processes are refused: give a pid.",
            json!({
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "description": "Process id" },
                    "name": { "type": "string", "description": "Exact process name" }
                },
                "additionalProperties": false
            }),
            false,
        ),
        tool_spec(
            "wait",
            "Wait without hammering the screen: pause for a few seconds, or until a window whose title contains `for_window` appears. Use it after launching something or while a page loads.",
            json!({
                "type": "object",
                "properties": {
                    "seconds": { "type": "number", "description": "How long to wait, 0.1-60" },
                    "for_window": { "type": "string", "description": "Title substring to wait for" },
                    "timeout_seconds": { "type": "number", "description": "Limit for for_window, 0.1-120" }
                },
                "required": ["seconds"],
                "additionalProperties": false
            }),
            true,
        ),
    ]
}

pub fn spec(name: &str) -> Option<ToolSpec> {
    specs().into_iter().find(|tool| tool.name == name)
}

/// Executes one computer tool call.
pub async fn run(
    session_id: &str,
    call: &ToolCall,
    state: &Mutex<HashMap<String, ComputerState>>,
    options: &ComputerOptions,
) -> ToolOutcome {
    let arguments: Value = if call.arguments.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&call.arguments) {
            Ok(value) => value,
            Err(error) => {
                return outcome(call, false, format!("invalid arguments: {error}"), Vec::new())
            }
        }
    };

    let result = match call.name.as_str() {
        "screenshot" => screenshot(session_id, &arguments, state, options).await,
        "ui" => ui(session_id, &arguments, state).await,
        "mouse" => mouse(session_id, &arguments, state),
        "keyboard" => keyboard(session_id, &arguments, state),
        "clipboard" => imp::clipboard(&arguments).map(Output::text),
        "list_windows" => imp::list_windows(&arguments).map(Output::text),
        "window" => imp::window(&arguments).map(Output::text),
        "list_processes" => imp::list_processes(&arguments).map(Output::text),
        "launch_app" => imp::launch_app(&arguments).map(Output::text),
        "kill_process" => imp::kill_process(&arguments).map(Output::text),
        "wait" => wait(&arguments, options).await,
        other => Err(Error::Other(format!("unknown computer tool: {other}"))),
    };

    match result {
        Ok(output) => outcome(call, true, output.text, output.images),
        Err(error) => outcome(call, false, error.to_string(), Vec::new()),
    }
}

struct Output {
    text: String,
    images: Vec<ToolImage>,
}

impl Output {
    fn text(text: String) -> Self {
        Self {
            text,
            images: Vec::new(),
        }
    }
}

fn outcome(call: &ToolCall, ok: bool, output: String, images: Vec<ToolImage>) -> ToolOutcome {
    ToolOutcome {
        id: call.id.clone(),
        name: call.name.clone(),
        ok,
        output,
        images,
    }
}

// ------------------------------------------------------------------
// screenshot
// ------------------------------------------------------------------

async fn screenshot(
    session_id: &str,
    arguments: &Value,
    state: &Mutex<HashMap<String, ComputerState>>,
    options: &ComputerOptions,
) -> Result<Output> {
    let target = arguments
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("active")
        .to_string();
    let capture = Capture {
        target,
        monitor: arguments.get("monitor").and_then(Value::as_i64).map(|v| v as i32),
        window: arguments
            .get("window")
            .and_then(Value::as_str)
            .map(str::to_string),
        region: arguments
            .get("region")
            .and_then(|region| {
                Some((
                    region.get("x")?.as_i64()? as i32,
                    region.get("y")?.as_i64()? as i32,
                    region.get("width")?.as_u64()? as u32,
                    region.get("height")?.as_u64()? as u32,
                ))
            })
            .filter(|(_, _, w, h)| *w > 0 && *h > 0),
        scale: arguments
            .get("scale")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 3) as u32,
        settle_ms: arguments
            .get("settle_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(2_000),
        cursor: arguments
            .get("cursor")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    };

    let captured = capture_frame(&capture)?;

    // Identical frame and pointer: reuse the encoded bytes (and the file
    // already written for the previous call stays valid — a new copy is
    // written below either way, but encoding is what costs).
    let cached = state
        .lock()
        .ok()
        .and_then(|states| states.get(session_id).and_then(|entry| entry.cached.clone()))
        .filter(|cached| {
            cached.hash == captured.frame_hash && cached.cursor == captured.cursor
        });
    let shot = match cached {
        Some(cached) => crate::screen::Shot {
            bytes: cached.bytes,
            name: cached.name,
            width: cached.width,
            height: cached.height,
        },
        None => {
            let shot = if options.screenshot_edge == 0 {
                crate::screen::prepare(captured.image)?
            } else {
                crate::screen::prepare_with_edge(captured.image, options.screenshot_edge)?
            };
            if let Ok(mut states) = state.lock() {
                if let Some(entry) = states.get_mut(session_id) {
                    entry.cached = Some(CachedFrame {
                        hash: captured.frame_hash,
                        cursor: captured.cursor,
                        bytes: shot.bytes.clone(),
                        name: shot.name.clone(),
                        width: shot.width,
                        height: shot.height,
                    });
                }
            }
            shot
        }
    };
    let name = format!("Computer {}", shot.name);
    let image = store_shot(session_id, &name, &shot.bytes)?;

    let scale = if captured.rect.2 == 0 {
        1.0
    } else {
        f64::from(shot.width) / f64::from(captured.rect.2)
    };
    let cursor_note = match captured.cursor {
        Some((x, y)) => {
            let ix = ((f64::from(x - captured.rect.0)) * scale).round() as i64;
            let iy = ((f64::from(y - captured.rect.1)) * scale).round() as i64;
            format!("cursor screen ({x},{y}) -> image ({ix},{iy})")
        }
        None => "cursor off-screen".to_string(),
    };
    let monitors = if captured.monitors.is_empty() {
        String::new()
    } else {
        format!(" · monitors: {}", captured.monitors.join(" "))
    };
    let text = format!(
        "Screenshot {}x{} · source rect ({},{},{}x{}) · scale {:.4} image-px per screen-px · {} \
         · frame {:08x}{monitors}",
        shot.width,
        shot.height,
        captured.rect.0,
        captured.rect.1,
        captured.rect.2,
        captured.rect.3,
        scale,
        cursor_note,
        captured.frame_hash as u32,
    );

    if let Ok(mut states) = state.lock() {
        let entry = states.entry(session_id.to_string()).or_default();
        entry.last_shot = Some(ShotMeta {
            rect: captured.rect,
            image_w: shot.width,
            image_h: shot.height,
            scale,
            monitor: captured.monitor,
            taken_ms: now_ms(),
        });
    }

    Ok(Output {
        text,
        images: vec![image],
    })
}

/// Captures once, or polls until two frames match when `settle_ms` asks.
fn capture_frame(capture: &Capture) -> Result<CaptureResult> {
    let first = imp::capture(capture)?;
    if capture.settle_ms == 0 {
        return Ok(first);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(capture.settle_ms);
    let mut last = first;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(120));
        let next = imp::capture(capture)?;
        if next.frame_hash == last.frame_hash {
            return Ok(next);
        }
        last = next;
    }
    Ok(last)
}

fn store_shot(session_id: &str, name: &str, bytes: &[u8]) -> Result<ToolImage> {
    let root = crate::paths::loom_home()?.join("attachments");
    let stored = crate::attachments::store_bytes_in(&root, session_id, name, bytes)?;
    Ok(ToolImage {
        name: name.to_string(),
        mime: stored.mime,
        path: stored.path,
    })
}

// ------------------------------------------------------------------
// mouse / keyboard
// ------------------------------------------------------------------

fn mouse(
    session_id: &str,
    arguments: &Value,
    state: &Mutex<HashMap<String, ComputerState>>,
) -> Result<Output> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("mouse needs an action".into()))?;
    let button = arguments
        .get("button")
        .and_then(Value::as_str)
        .unwrap_or("left")
        .to_string();
    let count = arguments
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 3) as u32;
    let modifiers: Vec<String> = arguments
        .get("modifiers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let amount = arguments
        .get("amount")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let horizontal = arguments
        .get("horizontal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let duration_ms = arguments
        .get("duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or(250)
        .min(5_000) as u32;
    let steps = arguments
        .get("steps")
        .and_then(Value::as_u64)
        .unwrap_or(12)
        .clamp(1, 60) as u32;
    let x = arguments.get("x").and_then(Value::as_i64).map(|v| v as i32);
    let y = arguments.get("y").and_then(Value::as_i64).map(|v| v as i32);

    let mut states = state.lock().map_err(|_| Error::Other("state busy".into()))?;

    match action {
        "position" => {
            let (x, y) = imp::cursor_pos()?;
            return Ok(Output::text(format!("cursor screen ({x},{y})")));
        }
        // Releasing the button needs no coordinates: it is the safety valve
        // for a drag that was interrupted (pause, stop, a crashed turn).
        "up" => {
            let text = imp::mouse(
                action, 0, 0, None, &button, count, &modifiers, amount, horizontal, duration_ms,
                steps,
            )?;
            if let Some(entry) = states.get_mut(session_id) {
                match button.as_str() {
                    "right" => entry.held_buttons[1] = false,
                    "middle" => entry.held_buttons[2] = false,
                    _ => entry.held_buttons[0] = false,
                }
            }
            return Ok(Output::text(text));
        }
        _ => {}
    }

    // Coordinate actions need a screenshot to map from.
    let (screen_x, screen_y) = match (x, y) {
        (Some(x), Some(y)) => {
            let entry = states.get(session_id).ok_or_else(no_shot_error)?;
            image_to_screen(entry, x, y)?
        }
        _ => {
            return Err(Error::Other(format!(
                "mouse action `{action}` needs x and y (image coordinates from the last screenshot)"
            )))
        }
    };
    let to = match (
        arguments.get("to_x").and_then(Value::as_i64),
        arguments.get("to_y").and_then(Value::as_i64),
    ) {
        (Some(tx), Some(ty)) => {
            let entry = states.get(session_id).ok_or_else(no_shot_error)?;
            Some(image_to_screen(entry, tx as i32, ty as i32)?)
        }
        _ => None,
    };

    let text = imp::mouse(
        action,
        screen_x,
        screen_y,
        to,
        &button,
        count,
        &modifiers,
        amount,
        horizontal,
        duration_ms,
        steps,
    )?;

    // Remember where we clicked and keep held-button bookkeeping honest.
    if let Some(entry) = states.get_mut(session_id) {
        if matches!(
            action,
            "click" | "double_click" | "right_click" | "middle_click"
        ) {
            entry.last_click = Some((screen_x, screen_y));
        }
        match (action, button.as_str()) {
            ("down", "left") => entry.held_buttons[0] = true,
            ("down", "right") => entry.held_buttons[1] = true,
            ("down", "middle") => entry.held_buttons[2] = true,
            ("up", "left") => entry.held_buttons[0] = false,
            ("up", "right") => entry.held_buttons[1] = false,
            ("up", "middle") => entry.held_buttons[2] = false,
            ("drag", "left") => entry.held_buttons[0] = false,
            ("drag", "right") => entry.held_buttons[1] = false,
            ("drag", "middle") => entry.held_buttons[2] = false,
            _ => {}
        }
    }

    Ok(Output::text(text))
}

fn no_shot_error() -> Error {
    Error::Other(
        "take a screenshot first — mouse coordinates are image pixels from the most recent \
         screenshot"
            .into(),
    )
}

/// Maps image pixels from the latest screenshot to virtual-desktop pixels.
fn image_to_screen(state: &ComputerState, x: i32, y: i32) -> Result<(i32, i32)> {
    let shot = state.last_shot.as_ref().ok_or_else(no_shot_error)?;
    if shot.stale() {
        return Err(Error::Other(
            "the last screenshot is stale; take a new one".into(),
        ));
    }
    let scale = if shot.scale <= 0.0 { 1.0 } else { shot.scale };
    let sx = f64::from(shot.rect.0) + (f64::from(x) / scale).round();
    let sy = f64::from(shot.rect.1) + (f64::from(y) / scale).round();
    Ok((sx as i32, sy as i32))
}

fn keyboard(
    session_id: &str,
    arguments: &Value,
    state: &Mutex<HashMap<String, ComputerState>>,
) -> Result<Output> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("keyboard needs an action".into()))?;
    let repeat = arguments
        .get("repeat")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 50) as u32;

    match action {
        "type" => {
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Other("keyboard type needs text".into()))?;
            if text.is_empty() {
                return Err(Error::Other("keyboard type needs non-empty text".into()));
            }
            imp::check_input_target()?;
            if text.chars().count() > CLIPBOARD_STAGE_CHARS {
                imp::type_via_clipboard(text).map(Output::text)
            } else {
                imp::type_unicode(text).map(Output::text)
            }
        }
        "press" | "down" | "up" => {
            let key = arguments
                .get("key")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Other(format!("keyboard {action} needs a key name")))?;
            let text = imp::key(action, key, repeat).map(Output::text)?;
            // Track held keys so a pause or a stopped turn can release them.
            if let Some(code) = imp::named_key_code(key) {
                if let Ok(mut states) = state.lock() {
                    if let Some(entry) = states.get_mut(session_id) {
                        if action == "down" {
                            if !entry.held_keys.contains(&code) {
                                entry.held_keys.push(code);
                            }
                        } else if action == "up" {
                            entry.held_keys.retain(|held| *held != code);
                        }
                    }
                }
            }
            Ok(text)
        }
        "combo" => {
            let keys: Vec<String> = match arguments.get("keys") {
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(Value::String(spec)) => spec
                    .split('+')
                    .map(|key| key.trim().to_string())
                    .filter(|key| !key.is_empty())
                    .collect(),
                _ => return Err(Error::Other("keyboard combo needs keys".into())),
            };
            if keys.len() < 2 {
                return Err(Error::Other(
                    "keyboard combo needs at least two keys, e.g. [\"ctrl\",\"c\"]".into(),
                ));
            }
            imp::combo(&keys, repeat).map(Output::text)
        }
        other => Err(Error::Other(format!("unknown keyboard action: {other}"))),
    }
}

// ------------------------------------------------------------------
// ui
// ------------------------------------------------------------------

async fn ui(
    session_id: &str,
    arguments: &Value,
    state: &Mutex<HashMap<String, ComputerState>>,
) -> Result<Output> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other("ui needs an action".into()))?;
    let window = arguments
        .get("window")
        .and_then(Value::as_str)
        .map(str::to_string);
    let depth = arguments
        .get("depth")
        .and_then(Value::as_u64)
        .unwrap_or(4)
        .clamp(1, 8) as u32;
    let max_nodes = arguments
        .get("max_nodes")
        .and_then(Value::as_u64)
        .unwrap_or(250)
        .clamp(50, 500) as usize;

    if action == "tree" {
        let (hwnd, nodes) = imp::ui_tree(window.as_deref(), depth, max_nodes)?;
        let mut text = String::new();
        for node in &nodes {
            let patterns = if node.patterns.is_empty() {
                String::new()
            } else {
                format!(" [{}]", node.patterns.join(","))
            };
            let value = if node.enabled { "" } else { " disabled" };
            text.push_str(&format!(
                "{} {} \"{}\" bounds=({},{},{}x{}){}{}\n",
                node.id,
                node.role,
                node.name.replace('"', "'"),
                node.bounds.0,
                node.bounds.1,
                node.bounds.2,
                node.bounds.3,
                value,
                patterns,
            ));
        }
        if text.is_empty() {
            text.push_str("(no accessible elements found)");
        }

        let mut states = state.lock().map_err(|_| Error::Other("state busy".into()))?;
        if let Some(entry) = states.get_mut(session_id) {
            entry.last_ui = Some(nodes);
            entry.last_ui_ms = now_ms();
            entry.last_ui_window = Some(hwnd);
        }
        return Ok(Output::text(text.trim_end().to_string()));
    }

    let id = arguments
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Other(format!("ui {action} needs an element id from the tree")))?
        .to_string();
    let value = arguments
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string);

    let (hwnd, path, bounds) = {
        let states = state.lock().map_err(|_| Error::Other("state busy".into()))?;
        let entry = states
            .get(session_id)
            .ok_or_else(|| Error::Other("run `ui tree` first".into()))?;
        let nodes = entry
            .last_ui
            .as_ref()
            .ok_or_else(|| Error::Other("run `ui tree` first".into()))?;
        let node = nodes
            .iter()
            .find(|node| node.id == id)
            .ok_or_else(|| {
                Error::Other("unknown element id; run `ui tree` again — ids expire when the tree is re-read".into())
            })?;
        (
            entry.last_ui_window.unwrap_or(0),
            node.path.clone(),
            node.bounds,
        )
    };

    if action == "click_element" {
        let cx = bounds.0 + bounds.2 as i32 / 2;
        let cy = bounds.1 + bounds.3 as i32 / 2;
        if hwnd != 0 {
            let _ = imp::focus_window_handle(hwnd);
        }
        let text = imp::mouse("click", cx, cy, None, "left", 1, &[], 0, false, 0, 1)?;
        return Ok(Output::text(text));
    }

    imp::ui_act(hwnd, &path, action, value.as_deref()).map(Output::text)
}

// ------------------------------------------------------------------
// wait
// ------------------------------------------------------------------

async fn wait(arguments: &Value, options: &ComputerOptions) -> Result<Output> {
    let seconds = arguments
        .get("seconds")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.1, 60.0);
    let for_window = arguments
        .get("for_window")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timeout = arguments
        .get("timeout_seconds")
        .and_then(Value::as_f64)
        .unwrap_or(30.0)
        .clamp(0.1, 120.0);

    if let Some(title) = for_window {
        let started = std::time::Instant::now();
        let deadline = started + std::time::Duration::from_secs_f64(timeout);
        loop {
            if cancelled(options) {
                return Ok(Output::text("wait interrupted: the turn was stopped".into()));
            }
            if let Some(hwnd) = imp::find_window(&title)? {
                return Ok(Output::text(format!(
                    "window \"{title}\" appeared after {:.1}s (hwnd 0x{hwnd:X})",
                    started.elapsed().as_secs_f64()
                )));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(Output::text(format!(
                    "window \"{title}\" did not appear within {timeout:.0}s"
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
    while std::time::Instant::now() < deadline {
        if cancelled(options) {
            return Ok(Output::text("wait interrupted: the turn was stopped".into()));
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    Ok(Output::text(format!("waited {seconds:.1}s")))
}

/// True when the turn this call belongs to has been stopped.
fn cancelled(options: &ComputerOptions) -> bool {
    options
        .cancel
        .as_ref()
        .is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed))
}

// ------------------------------------------------------------------
// session plumbing
// ------------------------------------------------------------------

/// Releases any mouse buttons and keys still held for a session: called when
/// a turn pauses or ends so a half-finished drag cannot stick down.
pub fn release_held(state: &Mutex<HashMap<String, ComputerState>>, session_id: &str) {
    let Ok(mut states) = state.lock() else {
        return;
    };
    let Some(entry) = states.get_mut(session_id) else {
        return;
    };
    let buttons = entry.held_buttons;
    let keys = std::mem::take(&mut entry.held_keys);
    entry.held_buttons = [false; 3];
    drop(states);
    imp::release(buttons, &keys);
}

/// Deletes the oldest computer screenshots of a session, keeping the newest
/// [`KEEP_SHOTS`]. Returns the paths that were removed.
pub const KEEP_SHOTS: usize = 200;

pub fn prune_screenshots(session_id: &str) -> Result<Vec<String>> {
    let Ok(home) = crate::paths::loom_home() else {
        return Ok(Vec::new());
    };
    let directory = home.join("attachments").join(sanitize(session_id));
    let Ok(reader) = std::fs::read_dir(&directory) else {
        return Ok(Vec::new());
    };

    let mut screenshots: Vec<(std::time::SystemTime, std::path::PathBuf)> = reader
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains("Computer Screen")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .collect();

    if screenshots.len() <= KEEP_SHOTS {
        return Ok(Vec::new());
    }
    screenshots.sort_by(|a, b| b.0.cmp(&a.0));
    let mut removed = Vec::new();
    for (_, path) in screenshots.into_iter().skip(KEEP_SHOTS) {
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(removed)
}

/// The same file-name the attachment store uses for a session.
fn sanitize(session_id: &str) -> String {
    session_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect()
}
