//! The built-in browser's shell: a child webview per tab, inside the panel's
//! own window.
//!
//! # Why child webviews, and not windows
//!
//! A tab was a borderless *window* positioned over the rectangle the panel
//! reported. That is the arrangement this file used to implement, and it was
//! wrong, for reasons that are structural rather than fixable:
//!
//! - **The panel lives inside a window.** Laying a second window over part of
//!   the first is a fight the second one loses: any click on the chat raises it
//!   over the page, and the page has no idea it was covered.
//! - **Every move is two coordinates away from being wrong.** Screen pixels,
//!   the window's outer position, and a per-monitor scale factor all had to be
//!   right, and any one of them being stale put the page in the wrong place.
//! - **Tauri refuses the obvious fix.** `Webview::reparent` errors with
//!   `CannotReparentWebviewWindow` for exactly this shape, so a page could never
//!   follow its panel between the dock and a torn-off window.
//!
//! A **child webview** has none of those problems. `Window::add_child` puts it
//! *inside* the window that shows the panel, so it is clipped by that window,
//! moves with it, and is positioned in the same coordinate space the panel
//! already measures in — CSS pixels from `getBoundingClientRect`, which are the
//! logical units `set_bounds` wants. There is no scale factor to get wrong and
//! no z-order to maintain, because it is not a window at all.
//!
//! Two costs, both worth stating. `add_child` needs Tauri's `unstable` feature,
//! and a child webview cannot be rounded — it is a rectangle inside the panel's
//! padding, not a squircle.
//!
//! # Why the tabs are not in the panel's store
//!
//! A panel can be torn off, so two webviews can be showing the same browser and
//! two JavaScript heaps cannot share a `zustand` store. The tabs live here, in
//! the process, and every change is broadcast on `loom://browser` — the same
//! arrangement as the dock layout and the terminal's output, and for the same
//! reason.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Position, Rect, Size, Webview,
    WebviewBuilder, WebviewUrl,
};

use loom_core::browser::filters::Verdict;
use loom_core::browser::{BrowserHost, Profile};

use crate::blocker::Blocker;
use crate::commands::AppState;

/// How long a collector call may take before it is abandoned.
///
/// Generous, because a call may follow a navigation: `browser_open` waits for
/// the page, and a slow page is still a page.
const CALL_TIMEOUT: Duration = Duration::from_secs(25);

/// How long a `CapturePreview` may take. Shorter than a collector call: a
/// capture reads a frame that already exists, so a slow one means something is
/// wrong rather than that the page is large.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// Ceiling on a capture held in memory, checked before allocating.
const MAX_CAPTURE_BYTES: usize = 48 * 1024 * 1024;

/// Smallest slot worth drawing a page into. Below this the panel is collapsed,
/// hidden, or mid-layout, and a page squeezed into a sliver would reflow into a
/// layout no one asked for — so it is parked instead.
const MIN_SLOT: f64 = 48.0;

/// Where a tab that is not being shown is put.
///
/// **Parked, not hidden**, and the distinction matters: `hide()` sets WebView2's
/// `IsVisible` to false, which suspends rendering and throttles timers, so a
/// page the model is working in would stall while the user reads another tab.
/// Moving it outside the window's client area gets it clipped out of sight while
/// leaving it fully live.
fn park_slot() -> Slot {
    Slot {
        x: -20_000.0,
        y: 0.0,
        width: 1280.0,
        height: 800.0,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Where the page goes, in **logical pixels relative to the host window's
/// client area** — which is exactly what `getBoundingClientRect` returns for an
/// element in that window's own webview.
///
/// That equivalence is the whole point of using a child webview: the panel
/// measures in the same units the shell positions in, so there is no scale
/// factor, no window position, and nothing that can be stale.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Slot {
    fn rect(self) -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(self.x, self.y)),
            size: Size::Logical(LogicalSize::new(self.width, self.height)),
        }
    }

    /// Whether this is too small to be a page: the panel is collapsed, hidden,
    /// or between layouts.
    fn degenerate(self) -> bool {
        !self.width.is_finite()
            || !self.height.is_finite()
            || self.width < MIN_SLOT
            || self.height < MIN_SLOT
    }

    /// Rounded to whole pixels, so a sub-pixel jitter in a layout does not
    /// count as a change and move a real webview.
    fn quantised(self) -> Slot {
        Slot {
            x: self.x.round(),
            y: self.y.round(),
            width: self.width.round(),
            height: self.height.round(),
        }
    }
}

/// One open tab, as the UI and the model both see it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabWire {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub profile: String,
    /// The active tab *in its own host window*. Two windows can each have one.
    pub active: bool,
    /// Whether the chat's current tool call is working in it.
    pub driving: bool,
    pub loading: bool,
    /// Which window's panel is showing it, so the panel can pick its own.
    pub host: String,
    pub session_id: Option<String>,
}

/// A download Loom saw, for the panel's rail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRow {
    pub url: String,
    pub path: String,
    pub ok: bool,
    pub at: u64,
}

struct Tab {
    id: u32,
    /// The child webview. Cloned on every use because `Webview` is a handle, not
    /// the contents, and holding the lock across a call would deadlock the
    /// handlers that need it.
    webview: Webview,
    /// The window whose panel is showing it.
    host: String,
    profile: Profile,
    session_id: Option<String>,
    title: String,
    url: String,
    active: bool,
    /// The bounds last applied, so a resize that changes nothing does not move a
    /// real webview.
    bounds: Option<Slot>,
    loading: bool,
}

/// The browser's own state: the tabs, which chat is working in which, and which
/// window is showing what.
pub struct BrowserState {
    next_id: AtomicU32,
    tabs: Mutex<Vec<Tab>>,
    /// The tab each host window's panel is showing.
    active_by_host: Mutex<HashMap<String, u32>>,
    /// The tab each chat is working in.
    tab_by_session: Mutex<HashMap<String, u32>>,
    /// The window a panel last reported from, so a tool call made while the
    /// panel is closed still knows where to put a page.
    last_host: Mutex<Option<String>>,
    downloads: Mutex<Vec<DownloadRow>>,
    /// The content blocker, shared by every tab's request filter.
    pub blocker: crate::blocker::Blocker,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            // Starts at 1 so a tab id is never 0, which is what `Option::None`
            // means wherever ids cross the wire.
            next_id: AtomicU32::new(1),
            tabs: Mutex::new(Vec::new()),
            active_by_host: Mutex::new(HashMap::new()),
            tab_by_session: Mutex::new(HashMap::new()),
            last_host: Mutex::new(None),
            downloads: Mutex::new(Vec::new()),
            blocker: crate::blocker::Blocker::new(),
        }
    }
}

impl BrowserState {
    fn allocate_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn snapshot(&self) -> Vec<TabWire> {
        let driving: Vec<u32> = lock(&self.tab_by_session).values().copied().collect();
        lock(&self.tabs)
            .iter()
            .map(|tab| TabWire {
                id: tab.id,
                title: tab.title.clone(),
                url: tab.url.clone(),
                profile: tab.profile.as_str().to_string(),
                active: tab.active,
                driving: driving.contains(&tab.id),
                loading: tab.loading,
                host: tab.host.clone(),
                session_id: tab.session_id.clone(),
            })
            .collect()
    }

    /// The tab a chat is working in, or the one its host is showing.
    ///
    /// The two are deliberately different questions. The model works in a tab it
    /// opened; the panel shows the tab the user last looked at. Resolving in
    /// that order means a tool call never acts on a tab the user just clicked
    /// away to, which is the browser equivalent of typing into the wrong window.
    fn resolve(&self, session_id: &str, requested: Option<u32>) -> Option<u32> {
        if let Some(id) = requested {
            return Some(id);
        }
        let tabs = lock(&self.tabs);
        if let Some(id) = lock(&self.tab_by_session).get(session_id).copied() {
            if tabs.iter().any(|tab| tab.id == id) {
                return Some(id);
            }
        }
        tabs.iter()
            .find(|tab| tab.active)
            .or_else(|| tabs.last())
            .map(|tab| tab.id)
    }

    fn webview(&self, id: u32) -> Option<Webview> {
        lock(&self.tabs)
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.webview.clone())
    }

    fn host_of(&self, id: u32) -> Option<String> {
        lock(&self.tabs)
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.host.clone())
    }

    /// Records what a tab is doing, without holding the lock across a call that
    /// might need it.
    fn with_tab(&self, id: u32, change: impl FnOnce(&mut Tab)) -> bool {
        let mut tabs = lock(&self.tabs);
        match tabs.iter_mut().find(|tab| tab.id == id) {
            Some(tab) => {
                change(tab);
                true
            }
            None => false,
        }
    }

    fn remove(&self, id: u32) -> Option<Tab> {
        let mut tabs = lock(&self.tabs);
        let position = tabs.iter().position(|tab| tab.id == id)?;
        let tab = tabs.remove(position);
        // A closed tab must not stay the active one anywhere, or the panel would
        // render an empty slot while a live tab exists beside it.
        lock(&self.active_by_host).retain(|_, value| *value != id);
        lock(&self.tab_by_session).retain(|_, value| *value != id);
        Some(tab)
    }

    /// The window to put a new page in: the one a panel last reported from, the
    /// main window, or — if all else fails — whatever exists.
    fn host_for_new_tab(&self, app: &AppHandle, preferred: Option<&str>) -> String {
        if let Some(label) = preferred {
            if app.get_window(label).is_some() {
                return label.to_string();
            }
        }
        if let Some(label) = lock(&self.last_host).clone() {
            if app.get_window(&label).is_some() {
                return label;
            }
        }
        if app.get_window("main").is_some() {
            return "main".to_string();
        }
        app.webview_windows()
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "main".to_string())
    }

    fn record_download(&self, row: DownloadRow) -> Vec<DownloadRow> {
        let mut downloads = lock(&self.downloads);
        downloads.insert(0, row);
        downloads.truncate(20);
        downloads.clone()
    }
}

/* ---------------------------------------------------------------------------
   What the panel is told
--------------------------------------------------------------------------- */

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWire {
    pub tabs: Vec<TabWire>,
    pub downloads: Vec<DownloadRow>,
}

fn wire(state: &BrowserState) -> BrowserWire {
    BrowserWire {
        tabs: state.snapshot(),
        downloads: lock(&state.downloads).clone(),
    }
}

pub fn broadcast(app: &AppHandle, state: &BrowserState) {
    let _ = app.emit("loom://browser", wire(state));
}

/* ---------------------------------------------------------------------------
   The collector bridge
--------------------------------------------------------------------------- */

/// The expression that asks the collector to run one operation.
///
/// Wrapped in a guard rather than assuming the script landed: a page with a
/// strict CSP, an error page, or a navigation mid-flight can all leave the
/// collector absent, and the model should be told which of those it is instead
/// of reading a JavaScript error.
fn collector_expression(op: &str, args: &Value) -> String {
    let op = serde_json::to_string(op).unwrap_or_else(|_| "\"\"".to_string());
    let args = serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string());
    // `args` is already a JSON string; embedding it as a JS string and letting
    // the collector parse it keeps one round of escaping out of the picture.
    let args_literal = serde_json::to_string(&args).unwrap_or_else(|_| "\"{}\"".to_string());
    format!(
        "(function () {{ \
           if (!window.__loomBrowser) {{ \
             return JSON.stringify({{ error: \"the page collector is not installed on this \
               frame (it may be an error page, or the page may have navigated while the call \
               was in flight) — take a fresh browser_snapshot\" }}); \
           }} \
           return window.__loomBrowser.handle({op}, {args_literal}); \
         }})()"
    )
}

/// Evaluates an expression in a tab and returns the collector's JSON.
///
/// Blocks the calling thread, which is deliberate: this is called from
/// [`TauriBrowserHost::call`], which the engine already runs on a blocking
/// thread, and the alternative — an async round trip — would need a channel the
/// engine does not have.
fn collect(webview: &Webview, op: &str, args: &Value) -> Result<Value, String> {
    let (tx, rx) = mpsc::channel::<String>();
    let expression = collector_expression(op, args);

    webview
        .eval_with_callback(expression, move |result| {
            // A send failure means the caller timed out. Nothing to do about it,
            // and certainly not a panic.
            let _ = tx.send(result);
        })
        .map_err(|error| format!("could not reach the page: {error}"))?;

    let raw = rx
        .recv_timeout(CALL_TIMEOUT)
        .map_err(|_| format!("the page did not answer within {}s", CALL_TIMEOUT.as_secs()))?;

    Ok(decode_collector_reply(&raw))
}

/// Unwraps what the callback handed back.
///
/// The callback receives the *serialized* result, so a JSON string produced by
/// the collector arrives as a JSON string *containing* JSON. Parsed once to get
/// the string, then again to get the object — with the single-parse case handled
/// too, because which of the two shapes arrives depends on how the runtime
/// serializes a return value.
fn decode_collector_reply(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return json!({ "error": "the page returned nothing" });
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::String(inner)) => match serde_json::from_str::<Value>(&inner) {
            Ok(parsed) => parsed,
            // A string that is not JSON is the answer, not a wrapper.
            Err(_) => json!({ "value": inner }),
        },
        Ok(value) => value,
        Err(_) => json!({ "value": trimmed }),
    }
}

/* ---------------------------------------------------------------------------
   Tab lifecycle
--------------------------------------------------------------------------- */

/* ---------------------------------------------------------------------------
   Content blocking
--------------------------------------------------------------------------- */

/// Installs the blocker on a tab's webview.
///
/// Two halves, and both are needed for the result to look like an ad blocker
/// rather than a page full of holes:
///
/// 1. **The network filter.** `AddWebResourceRequestedFilter` asks WebView2 to
///    hand over every request *before it goes out*, and a rule match answers it
///    with a synthetic `403`. The request is therefore never made — which is
///    better for the user than blocking a response, and is what uBlock's own
///    network layer does.
/// 2. **The cosmetic sheet.** A blocked ad usually leaves its slot behind,
///    so the elements are hidden too. Injected through
///    `AddScriptToExecuteOnDocumentCreated` on each *navigation*, because a
///    sheet added after the page paints shows the ad for a frame — the flicker
///    every blocker exists to remove.
///
/// The whole thing is skipped when nothing is loaded, so a browser with blocking
/// switched off pays nothing for this existing.
#[cfg(windows)]
fn install_blocking(webview: &Webview, blocker: Blocker) {
    if !blocker.is_active() {
        return;
    }
    let net = blocker;

    if let Err(error) = webview.with_webview(move |platform| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2WebResourceRequestedEventArgs2, COREWEBVIEW2_WEB_RESOURCE_CONTEXT,
            COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        };
        use webview2_com::WebResourceRequestedEventHandler;
        use windows::core::{w, Interface, PWSTR};
        use windows::Win32::System::Com::IStream;

        let Ok(core) = platform.controller().CoreWebView2() else {
            return;
        };
        // The response factory lives on the *environment*, not the webview.
        let environment = platform.environment();

        // Watch every context. A list names types per rule, so narrowing the
        // filter here would make `$image` and `$script` rules unreachable.
        if let Err(error) =
            core.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
        {
            eprintln!("[loom] blocking: could not watch requests: {error}");
            return;
        }

        let filters = net.clone();
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |webview, args| {
            let (Some(webview), Some(args)) = (webview, args) else {
                return Ok(());
            };
            let Ok(request) = args.Request() else {
                return Ok(());
            };

            let mut raw = PWSTR::null();
            if request.Uri(&mut raw).is_err() {
                return Ok(());
            }
            let url = raw.to_string().unwrap_or_default();
            if url.is_empty() {
                return Ok(());
            }

            // The page the request came from, which is what `$third-party` and
            // `$domain=` are judged against. Falling back to the request's own
            // host makes such a rule fail closed rather than fire wrongly.
            let mut source = PWSTR::null();
            let _ = webview.Source(&mut source);
            let document = source.to_string().unwrap_or_default();
            let document_host = if document.is_empty() {
                loom_core::browser::filters::host_of(&url).to_string()
            } else {
                loom_core::browser::filters::host_of(&document).to_string()
            };

            // `ResourceContext` lives on the **version-2** interface, not the
            // base one, and writes through an out-parameter. Where the cast
            // fails — an older runtime — the type is reported as 0, which the
            // mapper turns into `other`. That makes a type-specific rule not
            // fire, and it is *under*-blocking deliberately: reporting every
            // request as matching every type would let an `$image` rule block
            // scripts, which is how a blocker breaks a site.
            let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT(0);
            let context = args
                .cast::<ICoreWebView2WebResourceRequestedEventArgs2>()
                .ok()
                .filter(|args2| args2.ResourceContext(&mut context).is_ok())
                .map(|_| context.0)
                .unwrap_or(0);
            let resource_type = crate::blocker::context_to_type(context);

            if filters.verdict(&url, &document_host, resource_type) != Verdict::Block {
                return Ok(());
            }

            // An empty body with `403` is the standard refusal: a page's own
            // error handling sees a failed request rather than a corrupt one,
            // and there are no bytes to download.
            let Ok(response) = environment.CreateWebResourceResponse(
                None::<&IStream>,
                403,
                w!("Blocked"),
                w!("Content-Type: text/plain"),
            ) else {
                return Ok(());
            };
            let _ = args.SetResponse(&response);
            Ok(())
        }));

        // The token is dropped rather than kept: a tab's webview lives exactly
        // as long as the tab, so there is nothing to unsubscribe from — the
        // filter goes away with the page.
        let mut token = 0i64;
        if let Err(error) = core.add_WebResourceRequested(&handler, &mut token) {
            eprintln!("[loom] blocking: could not install the request filter: {error}");
        }
    }) {
        eprintln!("[loom] blocking: could not reach the tab's webview: {error}");
    }
}

/// Everywhere else: no WebView2, so no request filter. The browser itself is
/// Windows-only for the same reason.
#[cfg(not(windows))]
fn install_blocking(_webview: &Webview, _blocker: Blocker) {}

/// How a finished download is named and where it goes.
fn download_destination(profile: Profile, suggested: &std::path::Path) -> PathBuf {
    // A ghost tab's downloads must not land in the user's own folder: the whole
    // point of a private tab is that it leaves no trace, and a file in
    // `~/Downloads` is a trace.
    if profile == Profile::Ghost {
        if let Ok(dir) = loom_core::paths::browser_downloads_dir() {
            let _ = std::fs::create_dir_all(&dir);
            if let Some(name) = suggested.file_name() {
                return dir.join(name);
            }
        }
    }
    suggested.to_path_buf()
}

/// Creates a tab's child webview inside `host`, and records it.
///
/// **Async or off the event loop, never synchronous.** Building a webview from a
/// synchronous command or an event handler deadlocks on Windows (wry#583) — the
/// same trap that already cost this codebase a process abort when `send_message`
/// was synchronous, so it is worth naming twice. `add_child` also blocks until
/// the webview exists, so the callers are all either async commands or threads.
#[allow(clippy::too_many_arguments)]
fn create_tab(
    app: &AppHandle,
    state: &Arc<BrowserState>,
    id: Option<u32>,
    host: &str,
    url: &str,
    profile: Profile,
    session_id: Option<String>,
    title: String,
) -> Result<u32, String> {
    let window = app
        .get_window(host)
        .ok_or_else(|| format!("the window {host} is not open, so there is nowhere to draw the page"))?;
    let id = id.unwrap_or_else(|| state.allocate_id());
    let label = format!("loom-tab-{id}");
    let parsed = url
        .parse()
        .map_err(|error| format!("`{url}` is not a URL the browser can open: {error}"))?;

    // Handlers run on the event loop and must not block, so the two that need to
    // do real work spawn threads rather than doing it inline.
    let title_app = app.clone();
    let title_state = Arc::clone(state);
    let load_app = app.clone();
    let load_state = Arc::clone(state);
    let window_app = app.clone();
    let window_state = Arc::clone(state);
    let download_app = app.clone();
    let download_state = Arc::clone(state);
    // Only when there is something to hide, so a browser with blocking off
    // injects nothing at all on any page.
    let cosmetic = state
        .blocker
        .is_active()
        .then(|| state.blocker.clone());

    let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(parsed))
        // The collector, installed before the page's own scripts in every frame.
        // This is what lets it patch `console` and observe real input rather than
        // inferring either, and it is the only reason a `[n]` index from a
        // snapshot still means something by the time the model clicks it.
        .initialization_script_for_all_frames(loom_core::browser::COLLECTOR_JS)
        // A ghost tab is in-private: no cookies, no storage, nothing left behind.
        .incognito(profile == Profile::Ghost)
        // Ctrl+wheel zoom, as in any browser.
        .zoom_hotkeys_enabled(true)
        // Never steal focus: opening a tab from the model's side must not pull
        // the caret out of the composer.
        .focused(false)
        .on_document_title_changed(move |_webview, new_title| {
            if title_state.with_tab(id, |tab| tab.title = new_title.clone()) {
                broadcast(&title_app, &title_state);
            }
        })
        .on_page_load(move |webview, payload| {
            let url = webview.url().map(|url| url.to_string()).unwrap_or_default();
            let loading = matches!(payload.event(), tauri::webview::PageLoadEvent::Started);
            if load_state.with_tab(id, |tab| {
                tab.url = url.clone();
                tab.loading = loading;
            }) {
                broadcast(&load_app, &load_state);
            }

            // The cosmetic half of content blocking: hide what the network
            // filter blocked, so a page shows no holes where the ads were.
            //
            // Injected here rather than through
            // `AddScriptToExecuteOnDocumentCreated` because the sheet depends on
            // which *host* is loading, and Tauri's page-load event is where that
            // is known — without a second stream of COM callbacks to keep alive.
            // `Started` is the useful moment: the document is being parsed and
            // nothing has painted, so the elements never appear at all.
            //
            // The script carries its own `__loomCosmetic` guard, so the second
            // injection at `Finished` is inert — which is why both events can
            // run it without producing two sheets.
            let host = loom_core::browser::filters::host_of(&url).to_string();
            if let Some(script) = cosmetic.as_ref().and_then(|b| b.cosmetic_script(&host)) {
                let _ = webview.eval(script);
            }
        })
        // `target="_blank"` and `window.open` become Loom tabs. Returning
        // `Deny` and opening one ourselves is what keeps the browser *one* tab
        // set: letting Tauri create a window would produce a page with no tab,
        // no strip entry and no way back.
        .on_new_window(move |url, _features| {
            let opener = window_state.host_of(id).unwrap_or_else(|| "main".to_string());
            let app = window_app.clone();
            let state = Arc::clone(&window_state);
            let target = url.to_string();
            std::thread::spawn(move || {
                // A thread, not this handler: creating a webview on the event
                // loop thread deadlocks on Windows. The result is dropped
                // deliberately — a page that opens a popup and fails to get one
                // has lost nothing the user was looking at.
                let _ = open_tab(&app, &state, Some(&opener), &target, Profile::Normal, None);
            });
            tauri::webview::NewWindowResponse::Deny
        })
        .on_download(move |_webview, event| {
            match event {
                tauri::webview::DownloadEvent::Requested { destination, .. } => {
                    // A normal tab downloads where any browser would; a ghost tab
                    // writes inside Loom instead, because a private tab that
                    // leaves a file in the user's Downloads folder has left
                    // exactly the trace it exists to avoid.
                    *destination = download_destination(profile, destination);
                }
                tauri::webview::DownloadEvent::Finished { url, path, success } => {
                    let path = path
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    download_state.record_download(DownloadRow {
                        url: url.to_string(),
                        path,
                        ok: success,
                        at: loom_core::db::now_ms() as u64,
                    });
                    broadcast(&download_app, &download_state);
                }
                // A variant for progress or cancellation, depending on the
                // platform. Neither needs recording: `Finished` is the only one
                // that says where the bytes ended up.
                _ => {}
            }
            true
        });

    // The per-origin deny list, enforced here rather than in a tool, so no tool
    // call can route around it. Empty by default: this is a setting, not a
    // permission card.
    if let Ok(config) = loom_core::config::load() {
        let blocked = config.browser.blocked_origins.clone();
        if !blocked.is_empty() {
            let state = Arc::clone(state);
            builder = builder.on_navigation(move |url| {
                let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
                let full = url.as_str().to_ascii_lowercase();
                let hit = blocked.iter().any(|entry| {
                    let entry = entry.trim().to_ascii_lowercase();
                    !entry.is_empty()
                        && (host == entry
                            || host.ends_with(&format!(".{entry}"))
                            || full.starts_with(&entry))
                });
                if hit {
                    eprintln!("[loom] browser: refused a blocked origin");
                    let _ = &state;
                }
                !hit
            });
        }
    }

    // Its own profile directory, so browsing cannot touch Loom's own storage and
    // clearing the browser's cookies cannot reset the app.
    if let Ok(dir) = loom_core::paths::browser_dir() {
        builder = builder.data_directory(dir);
    }

    let webview = window
        .add_child(
            builder,
            LogicalPosition::new(0.0, 0.0),
            // Created out of the way and immediately parked below, so a page
            // never flashes at the window's origin for a frame.
            LogicalSize::new(1.0, 1.0),
        )
        .map_err(|error| format!("could not open a browser tab: {error}"))?;

    // Content blocking, before the page gets a chance to load anything. Doing
    // it after `add_child` returns is not too late: the first request only
    // happens once the webview is sized and shown, and neither has happened yet.
    install_blocking(&webview, state.blocker.clone());

    {
        let mut tabs = lock(&state.tabs);
        // Opening a tab shows it: a tab nobody can see is not what opening one
        // means. The previous active tab in the same host stands down.
        for tab in tabs.iter_mut() {
            if tab.host == host {
                tab.active = false;
            }
        }
        tabs.push(Tab {
            id,
            webview: webview.clone(),
            host: host.to_string(),
            profile,
            session_id,
            title,
            url: url.to_string(),
            active: true,
            bounds: None,
            loading: true,
        });
    }
    lock(&state.active_by_host).insert(host.to_string(), id);
    lock(&state.last_host).replace(host.to_string());

    let _ = webview.set_bounds(park_slot().rect());
    Ok(id)
}

/// Opens a tab, showing it in the window whose panel is asking.
pub fn open_tab(
    app: &AppHandle,
    state: &Arc<BrowserState>,
    host: Option<&str>,
    url: &str,
    profile: Profile,
    session_id: Option<&str>,
) -> Result<u32, String> {
    let host = state.host_for_new_tab(app, host);
    let id = create_tab(
        app,
        state,
        None,
        &host,
        url,
        profile,
        session_id.map(str::to_string),
        String::new(),
    )?;
    if let Some(session_id) = session_id {
        lock(&state.tab_by_session).insert(session_id.to_string(), id);
    }
    broadcast(app, state);
    Ok(id)
}

/// Moves a tab's webview into another window.
///
/// The panel can be docked in `main` and then torn off into its own window, and
/// the page has to follow. `reparent` is the right answer when it works — the
/// page keeps its scroll position, its form state and its history — and it is
/// attempted first for that reason. When it refuses, the tab is recreated in the
/// new host at the same URL, which is a real loss (an in-page form is emptied)
/// and so is only ever the fallback.
fn migrate(app: &AppHandle, state: &Arc<BrowserState>, id: u32, host: &str) -> Result<(), String> {
    let Some(webview) = state.webview(id) else {
        return Err(format!("tab {id} is not open"));
    };
    let Some(window) = app.get_window(host) else {
        return Err(format!("the window {host} is not open"));
    };

    if webview.reparent(&window).is_ok() {
        state.with_tab(id, |tab| {
            tab.host = host.to_string();
            tab.bounds = None;
        });
        lock(&state.active_by_host).insert(host.to_string(), id);
        return Ok(());
    }

    // The fallback: same tab id — the panel and the model both refer to it — a
    // fresh page at the same URL.
    let Some(tab) = state.remove(id) else {
        return Err(format!("tab {id} disappeared while it was being moved"));
    };
    let url = tab.url.clone();
    let profile = tab.profile;
    let session = tab.session_id.clone();
    let title = tab.title.clone();
    let _ = tab.webview.close();

    let new_id = create_tab(app, state, Some(id), host, &url, profile, session, title)?;
    debug_assert_eq!(new_id, id);
    Ok(())
}

/// Lays out every tab a host is showing: the active one in its slot, the rest
/// parked.
///
/// This is the only place a tab's bounds change, so "which page is on screen" is
/// answered in one function rather than inferred from several.
fn apply_host_layout(
    app: &AppHandle,
    state: &Arc<BrowserState>,
    host: &str,
    active: Option<u32>,
    slot: Option<Slot>,
) {
    lock(&state.last_host).replace(host.to_string());

    // A slot too small to be a page means the panel is collapsed or hidden, and
    // then every tab parks — including the active one, because a page squeezed
    // into a sliver reflows into a layout nobody asked for.
    let target = slot.filter(|slot| !slot.degenerate()).map(Slot::quantised);

    // Collect first, act second: `set_bounds` crosses into the runtime, and
    // holding the tabs lock across it would deadlock the callbacks that need it.
    let mut plan: Vec<(u32, Webview, Option<Slot>)> = Vec::new();
    {
        let mut tabs = lock(&state.tabs);
        for tab in tabs.iter_mut() {
            if tab.host != host {
                continue;
            }
            let showing = target.is_some() && active == Some(tab.id);
            tab.active = showing;
            let wanted = if showing { target } else { Some(park_slot()) };
            if tab.bounds != wanted {
                tab.bounds = wanted;
                plan.push((tab.id, tab.webview.clone(), wanted));
            }
        }
    }

    for (_, webview, wanted) in plan {
        if let Some(slot) = wanted {
            if let Err(error) = webview.set_bounds(slot.rect()) {
                eprintln!("[loom] browser: could not place a page: {error}");
            }
            // Never hidden, only moved: see `park_slot`. The `show` is not
            // redundant even though nothing here hides a webview — a page that
            // navigated to a download, or one WebView2 decided to stop
            // compositing, comes back with it.
            if let Err(error) = webview.show() {
                eprintln!("[loom] browser: could not show a page: {error}");
            }
        }
    }

    broadcast(app, state);
}

/// Moves every tab a closing window was showing into the main window.
///
/// A torn-off browser window going away must not take the pages with it — the
/// tabs are the app's, not the window's — so they are migrated rather than
/// closed. Runs on its own thread because it creates webviews, which must not
/// happen on the event loop.
fn migrate_host_tabs(app: &AppHandle, state: &Arc<BrowserState>, host: &str) {
    let ids: Vec<u32> = lock(&state.tabs)
        .iter()
        .filter(|tab| tab.host == host)
        .map(|tab| tab.id)
        .collect();
    if ids.is_empty() {
        return;
    }
    for id in ids {
        if let Err(error) = migrate(app, state, id, "main") {
            eprintln!("[loom] browser: could not bring tab {id} back: {error}");
        }
    }
    broadcast(app, state);
}

/// Called from the window-event handler when a window is destroyed.
pub fn on_host_destroyed(app: &AppHandle, host: &str) {
    if host == "main" {
        return;
    }
    let Some(state) = app.try_state::<AppState>().map(|state| Arc::clone(&state.browser)) else {
        return;
    };
    let app = app.clone();
    let host = host.to_string();
    // A thread, not the caller: this creates webviews, and a webview created on
    // the event loop thread deadlocks.
    std::thread::spawn(move || migrate_host_tabs(&app, &state, &host));
}

/* ---------------------------------------------------------------------------
   The host
--------------------------------------------------------------------------- */

/// The shell's implementation of the engine's browser seam.
pub struct TauriBrowserHost {
    app: AppHandle,
    state: Arc<BrowserState>,
}

impl TauriBrowserHost {
    pub fn new(app: AppHandle, state: Arc<BrowserState>) -> Self {
        Self { app, state }
    }

    /// Runs a collector operation in a tab.
    fn run(&self, session_id: &str, op: &str, args: &Value) -> Result<Value, String> {
        let requested = args.get("tab").and_then(Value::as_u64).map(|id| id as u32);
        let Some(id) = self.state.resolve(session_id, requested) else {
            // Nothing open yet: `browser_open` is the tool that fixes that, and
            // saying so beats a bare "no tab".
            return Ok(json!({
                "error": "No browser tab is open in this chat yet. Call `browser_open` with a \
                          URL first."
            }));
        };
        let Some(webview) = self.state.webview(id) else {
            return Ok(json!({
                "error": format!("tab {id} is no longer open. Call `browser_tabs` to see what is.")
            }));
        };
        collect(&webview, op, args)
    }
}

impl BrowserHost for TauriBrowserHost {
    fn call(&self, session_id: &str, op: &str, args: &Value) -> loom_core::Result<Value> {
        let outcome = match op {
            // The one operation that is not a collector call: it creates a page
            // rather than talking to one.
            "browser_open" => self.open(session_id, args),
            "browser_tabs" => Ok(json!({ "tabs": self.state.snapshot() })),
            "browser_screenshot" => screenshot(&self.state, session_id, args),
            _ => self.run(session_id, op, args),
        };
        Ok(outcome.unwrap_or_else(|error| json!({ "error": error })))
    }

    fn release(&self, session_id: &str) {
        let released = lock(&self.state.tab_by_session).remove(session_id).is_some();
        if released {
            broadcast(&self.app, &self.state);
        }
    }

    fn available(&self) -> bool {
        true
    }
}

impl TauriBrowserHost {
    /// `browser_open`: create a tab, wait for it to settle, and answer with the
    /// same shaped page view every other See-tier tool returns.
    fn open(&self, session_id: &str, args: &Value) -> Result<Value, String> {
        let url = args
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| "browser_open needs a `url`".to_string())?;
        let profile = args
            .get("profile")
            .and_then(Value::as_str)
            .and_then(Profile::parse)
            .unwrap_or_default();

        let id = open_tab(
            &self.app,
            &self.state,
            None,
            url,
            profile,
            Some(session_id),
        )?;
        lock(&self.state.tab_by_session).insert(session_id.to_string(), id);

        // A fresh page is usually not ready at the moment the webview exists, so
        // waiting for it is the difference between a useful first answer and "no
        // actionable elements found".
        let webview = self
            .state
            .webview(id)
            .ok_or_else(|| format!("tab {id} closed as soon as it opened"))?;
        let _ = collect(
            &webview,
            "wait",
            &json!({ "condition": { "idle": 250 }, "timeout_ms": 6000 }),
        );

        let mut page = collect(&webview, "snapshot", &json!({ "tab": id }))?;
        if let Some(object) = page.as_object_mut() {
            object.insert("tab".into(), json!(id));
        }
        Ok(page)
    }
}

/// Screenshots go through WebView2's capture API.
///
/// Kept separate from the collector path because it is the only operation that
/// needs the COM surface: the page cannot read its own composited output, and
/// Tauri exposes no capture API. See [`capture_png`].
fn screenshot(state: &BrowserState, session_id: &str, args: &Value) -> Result<Value, String> {
    let requested = args.get("tab").and_then(Value::as_u64).map(|id| id as u32);
    let Some(id) = state.resolve(session_id, requested) else {
        return Ok(json!({
            "error": "No browser tab is open in this chat yet. Call `browser_open` with a URL \
                      first."
        }));
    };
    let Some(webview) = state.webview(id) else {
        return Ok(json!({
            "error": format!("tab {id} is no longer open. Call `browser_tabs` to see what is.")
        }));
    };

    let edge = args
        .get("max_edge")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .clamp(0, 4096) as u32;
    let mode = args
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("viewport")
        .to_string();

    // Bring the tab forward first: a capture of a parked page would show
    // whatever the compositor last had, which for an offscreen webview is not
    // guaranteed to be the current frame.
    let _ = webview.show();

    let png = match capture_png(&webview) {
        Ok(png) => png,
        Err(error) => return Ok(json!({ "error": error })),
    };

    // Named like a computer screenshot and stored the same way, so the
    // transcript, the retention cap and the newest-image-only rule in the wire
    // builder all treat it as one thing rather than two.
    let name = format!("Browser Tab {}.png", loom_core::db::now_ms());
    let shot = loom_core::screen::prepare_encoded(&png, edge, &name)
        .map_err(|error| error.to_string())?;

    use base64::Engine as _;
    let mime = if shot.name.ends_with(".jpg") {
        "image/jpeg"
    } else {
        "image/png"
    };
    let mut out = json!({
        "shot": {
            "name": shot.name,
            "mime": mime,
            "data": base64::engine::general_purpose::STANDARD.encode(&shot.bytes),
            "width": shot.width,
            "height": shot.height,
        },
        "width": shot.width,
        "height": shot.height,
        "mode": mode,
    });

    // `CapturePreview` is the composited viewport and nothing else. Saying so is
    // the difference between a caller scrolling for the rest and a caller
    // believing it had the whole page.
    if mode != "viewport" {
        out["note"] = json!(format!(
            "`{mode}` is not available from this capture API, so this is the visible viewport \
             ({}×{}). Scroll and capture again for the rest, or use `browser_read` for the whole \
             document — which is cheaper and usually what is actually wanted.",
            shot.width, shot.height
        ));
    }

    Ok(out)
}

/// Captures a tab's visible area as PNG.
///
/// `CapturePreview` is the only route to a page's composited pixels, and it runs
/// through `with_webview`, which hands the closure a `PlatformWebview` on the
/// **main thread** — the only thread a WebView2 call may happen on — and returns
/// immediately, so the result comes back over a channel rather than a return
/// value. This function blocks until it does, which is correct here: it is
/// called from [`TauriBrowserHost::call`], which the engine already runs on a
/// blocking thread.
#[cfg(windows)]
fn capture_png(webview: &Webview) -> Result<Vec<u8>, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use webview2_com::CapturePreviewCompletedHandler;
    use windows::Win32::System::Com::IStream;

    let (tx, rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let sink: CaptureSink = Arc::new(Mutex::new(Some(tx)));

    let dispatch = Arc::clone(&sink);
    webview
        .with_webview(move |platform| unsafe {
            let controller = platform.controller();
            let core = match controller.CoreWebView2() {
                Ok(core) => core,
                Err(error) => {
                    finish(&dispatch, Err(format!("could not reach the page: {error}")));
                    return;
                }
            };
            let stream: IStream = match create_stream() {
                Ok(stream) => stream,
                Err(error) => {
                    finish(&dispatch, Err(error));
                    return;
                }
            };
            // The handler outlives this closure, so it needs its own handle on
            // the stream. An `IStream` is refcounted, so this clones the
            // interface rather than the bytes.
            let reader = stream.clone();
            let done = Arc::clone(&dispatch);
            let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                match result {
                    Ok(()) => finish(&done, read_stream(&reader)),
                    Err(error) => finish(
                        &done,
                        Err(format!("the page could not be captured: {error}")),
                    ),
                }
                Ok(())
            }));

            if let Err(error) = core.CapturePreview(
                COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                &stream,
                &handler,
            ) {
                finish(
                    &dispatch,
                    Err(format!("the capture could not be started: {error}")),
                );
            }
        })
        .map_err(|error| format!("could not reach the tab's webview: {error}"))?;

    // The tab may have been closed between the call and the capture, in which
    // case nothing will ever arrive. A timeout is what stops that being a hang.
    rx.recv_timeout(CAPTURE_TIMEOUT).map_err(|_| {
        format!(
            "the page did not finish capturing within {}s — it may have been closed or \
             navigated while the capture was running",
            CAPTURE_TIMEOUT.as_secs()
        )
    })?
}

/// Where a capture's result is handed back. `None` once it has been sent, so a
/// second call — the error path firing after a successful handler, or the other
/// way round — cannot send twice.
#[cfg(windows)]
type CaptureSink = Arc<Mutex<Option<mpsc::Sender<Result<Vec<u8>, String>>>>>;

#[cfg(windows)]
fn finish(sink: &CaptureSink, value: Result<Vec<u8>, String>) {
    if let Ok(mut guard) = sink.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(value);
        }
    }
}

/// A stream for `CapturePreview` to write into.
///
/// `CreateStreamOnHGlobal` with a null handle gives an OLE-managed in-memory
/// stream, and `fdeleteonrelease: true` means it frees itself. There is no
/// simpler sink the API accepts.
#[cfg(windows)]
unsafe fn create_stream() -> Result<windows::Win32::System::Com::IStream, String> {
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    CreateStreamOnHGlobal(Default::default(), true)
        .map_err(|error| format!("could not create a capture buffer: {error}"))
}

/// Reads a completed capture out of its stream.
///
/// Sized from `Stat` rather than by reading until exhausted, because a single
/// `Read` on an OLE stream is allowed to return less than was asked for — so
/// this loops until full, and a short read ends it. The size is capped before
/// allocating, so a broken stream cannot ask for a gigabyte.
#[cfg(windows)]
unsafe fn read_stream(stream: &windows::Win32::System::Com::IStream) -> Result<Vec<u8>, String> {
    use windows::Win32::System::Com::{STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};

    // `Read` is declared on `ISequentialStream`, which `IStream` inherits, and
    // windows-rs does not repeat an inherited vtable method on the derived
    // interface — so the cast is required rather than incidental. `Interface` is
    // the trait providing `cast`, and it lives in `windows::core`, not `Win32`.
    use windows::core::Interface;
    let sequential: windows::Win32::System::Com::ISequentialStream = stream
        .cast()
        .map_err(|error| format!("could not read the capture stream: {error}"))?;

    // The capture was written from the start, so the position is at the end.
    // The third argument is the resulting position, which nothing needs.
    stream
        .Seek(0, STREAM_SEEK_SET, None)
        .map_err(|error| format!("could not rewind the capture: {error}"))?;

    let mut stat: STATSTG = std::mem::zeroed();
    stream
        .Stat(&mut stat, STATFLAG_NONAME)
        .map_err(|error| format!("could not measure the capture: {error}"))?;

    let total = stat.cbSize as usize;
    if total == 0 {
        return Err("the page came back empty — it may still have been loading".to_string());
    }
    if total > MAX_CAPTURE_BYTES {
        return Err(format!(
            "the capture is {} MB, more than Loom will hold in memory",
            total / (1024 * 1024)
        ));
    }

    let mut bytes = vec![0u8; total];
    let mut filled = 0usize;
    while filled < total {
        let mut got = 0u32;
        let want = (total - filled).min(u32::MAX as usize) as u32;
        // `Read` is the raw HRESULT form, not a `Result`: `.ok()` turns it into
        // one.
        sequential
            .Read(bytes[filled..].as_mut_ptr() as *mut _, want, Some(&mut got))
            .ok()
            .map_err(|error| format!("could not read the capture: {error}"))?;
        if got == 0 {
            break;
        }
        filled += got as usize;
    }
    bytes.truncate(filled);
    Ok(bytes)
}

/// Everywhere else. The browser is a WebView2 feature, the same call Loom
/// already makes for computer use: other platforms compile against this and
/// report a reason rather than failing to build.
#[cfg(not(windows))]
fn capture_png(_webview: &Webview) -> Result<Vec<u8>, String> {
    Err("Screenshots need the WebView2 capture API, which is Windows only. Use \
         `browser_snapshot` for the page's structure or `browser_read` for its text."
        .to_string())
}

/* ---------------------------------------------------------------------------
   Commands
--------------------------------------------------------------------------- */

fn state_of(state: &tauri::State<'_, AppState>) -> Arc<BrowserState> {
    Arc::clone(&state.browser)
}

/// Opens a tab in a window's panel. **Async**, because building a webview from a
/// synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn browser_open_tab(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    url: String,
    host: Option<String>,
    profile: Option<String>,
    session_id: Option<String>,
) -> Result<TabWire, String> {
    let browser = state_of(&state);
    let profile = profile.as_deref().and_then(Profile::parse).unwrap_or_default();
    let id = open_tab(
        &app,
        &browser,
        host.as_deref(),
        &url,
        profile,
        session_id.as_deref(),
    )?;
    browser
        .snapshot()
        .into_iter()
        .find(|tab| tab.id == id)
        .ok_or_else(|| "the tab closed before it could be listed".to_string())
}

#[tauri::command]
pub fn browser_tabs(state: tauri::State<'_, AppState>) -> BrowserWire {
    wire(&state_of(&state))
}

#[tauri::command]
pub fn browser_close_tab(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: u32,
) -> Result<(), String> {
    let browser = state_of(&state);
    if let Some(tab) = browser.remove(id) {
        // Closing the webview is what actually ends the page; the state is
        // changed first so a handler firing on the close finds nothing to do.
        let _ = tab.webview.close();
    }
    broadcast(&app, &browser);
    Ok(())
}

/// Lays the browser out in a window: the active tab in the slot, the rest
/// parked, and — if the panel has moved to a different window — the page follows
/// it.
///
/// Called on every panel resize and window move, so it is written to do nothing
/// at all when nothing changed: the bounds are compared before a webview is
/// touched, because moving a real window 60 times a second during a drag is how
/// a panel starts to feel like it is being dragged through treacle.
#[tauri::command]
pub fn browser_set_slot(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    host: String,
    active: Option<u32>,
    slot: Option<Slot>,
) -> Result<(), String> {
    let browser = state_of(&state);

    // The panel has just moved into this window, so the tab it is showing has to
    // come with it. Done before layout, because a tab in the wrong window cannot
    // be positioned from here at all.
    if let Some(id) = active {
        match browser.host_of(id) {
            Some(current) if current == host => {}
            Some(_) => {
                migrate(&app, &browser, id, &host)?;
            }
            None => {}
        }
    }

    apply_host_layout(&app, &browser, &host, active, slot);
    Ok(())
}

/// Focuses a tab: raises it within its window and gives it the keyboard.
#[tauri::command]
pub fn browser_focus_tab(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: u32,
    session_id: Option<String>,
) -> Result<(), String> {
    let browser = state_of(&state);
    let Some(webview) = browser.webview(id) else {
        return Err(format!("tab {id} is not open"));
    };
    let _ = webview.show();
    let _ = webview.set_focus();

    // Two pieces of bookkeeping, both done under one lock so the tab list cannot
    // be broadcast mid-change with two tabs claiming to be the visible one.
    let host = {
        let mut tabs = lock(&browser.tabs);
        let Some(host) = tabs.iter().find(|tab| tab.id == id).map(|tab| tab.host.clone()) else {
            return Ok(());
        };
        for tab in tabs.iter_mut() {
            if tab.host == host {
                tab.active = tab.id == id;
            }
        }
        host
    };
    lock(&browser.active_by_host).insert(host, id);
    if let Some(session_id) = session_id {
        lock(&browser.tab_by_session).insert(session_id, id);
    }
    broadcast(&app, &browser);
    Ok(())
}

/// Navigates a tab, or moves through its history.
#[tauri::command]
pub fn browser_navigate(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    id: u32,
    action: String,
    url: Option<String>,
) -> Result<(), String> {
    let browser = state_of(&state);
    let Some(webview) = browser.webview(id) else {
        return Err(format!("tab {id} is not open"));
    };

    match action.as_str() {
        "goto" => {
            let target = url.ok_or_else(|| "goto needs a url".to_string())?;
            let parsed = target
                .parse()
                .map_err(|error| format!("`{target}` is not a URL: {error}"))?;
            webview
                .navigate(parsed)
                .map_err(|error| format!("could not navigate: {error}"))?;
        }
        "reload" => {
            webview
                .reload()
                .map_err(|error| format!("could not reload: {error}"))?;
        }
        // `history.go` is the honest route for these two: it is the browser's own
        // history, which is the same history WebView2 keeps, and Tauri exposes no
        // back/forward of its own.
        "back" => {
            let _ = webview.eval("history.back()");
        }
        "forward" => {
            let _ = webview.eval("history.forward()");
        }
        "stop" => {
            let _ = webview.eval("window.stop()");
        }
        other => return Err(format!("{other} is not a navigation action")),
    }
    broadcast(&app, &browser);
    Ok(())
}

/// Runs a browser tool from the UI rather than from the model.
///
/// The panel uses this for its own chrome, so the UI path and the model path go
/// through one implementation — two ways to move a tab is two ways to disagree
/// about where it is.
#[tauri::command]
pub async fn browser_call(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
    op: String,
    args: Value,
) -> Result<Value, String> {
    let browser = state_of(&state);
    let host = TauriBrowserHost::new(app, browser);
    Ok(host.call(&session_id, &op, &args).unwrap_or_default())
}

/// Whether the collector reached a tab, so a probe can tell a wiring fault from
/// a page fault.
#[tauri::command]
pub async fn browser_ping(state: tauri::State<'_, AppState>, id: u32) -> Result<Value, String> {
    let browser = state_of(&state);
    let Some(webview) = browser.webview(id) else {
        return Err(format!("tab {id} is not open"));
    };
    collect(&webview, "page", &json!({}))
}

/// Opens the browser panel in the window that asked. The composer's chip and the
/// Panels menu both land here.
#[tauri::command]
pub fn browser_show_panel(app: AppHandle) -> Result<(), String> {
    let _ = app.emit("loom://browser-show-panel", ());
    Ok(())
}

/// Sets or clears the per-origin deny list.
#[tauri::command]
pub fn browser_set_blocked_origins(
    state: tauri::State<'_, AppState>,
    origins: Vec<String>,
) -> Result<loom_core::config::AppConfig, String> {
    state.mutate(move |config| config.browser.blocked_origins = origins)
}

/// Reloads every open tab.
///
/// This is how a blocking change takes effect. The request filter is installed on
/// a webview when that webview is *created*, and re-installing it on a live tab
/// would need a per-tab token the config change has no way to reach — so a list
/// change applies to the next tab, and this makes "the next tab" mean "the ones
/// already open" instead of leaving the user to close and reopen them by hand.
#[tauri::command]
pub fn browser_reload_tabs(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<u32, String> {
    let browser = state_of(&state);
    let webviews: Vec<Webview> = lock(&browser.tabs)
        .iter()
        .map(|tab| tab.webview.clone())
        .collect();
    let mut reloaded = 0u32;
    for webview in webviews {
        match webview.reload() {
            Ok(()) => reloaded += 1,
            // A tab closed between the list being taken and the reload is not a
            // failure worth reporting; the user asked for the others to reload.
            Err(error) => eprintln!("[loom] browser: could not reload a tab: {error}"),
        }
    }
    broadcast(&app, &browser);
    Ok(reloaded)
}

/* ---------------------------------------------------------------------------
   Content blocking
--------------------------------------------------------------------------- */

/// What the blocker is doing, so Settings can be honest rather than reassuring.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockingStatus {
    enabled: bool,
    /// Whether a filter is actually installed. False while the lists are still
    /// downloading, and false when every list failed — which is a different
    /// thing from "off" and has to read differently.
    installed: bool,
    rules: u32,
    blocked: u64,
    /// Sources currently fetched, with what went wrong where it did.
    sources: Vec<BlockingSource>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockingSource {
    id: String,
    name: String,
    rules: u32,
    error: Option<String>,
}

/// Reports the blocker's state.
#[tauri::command]
pub fn browser_blocking_status(state: tauri::State<'_, AppState>) -> BlockingStatus {
    let blocker = &state.browser.blocker;
    let config = state.snapshot();
    BlockingStatus {
        enabled: config.browser.blocking.enabled,
        installed: blocker.is_active(),
        rules: blocker.rule_count(),
        blocked: blocker.blocked_count(),
        sources: lock(&state.blocking_sources)
            .iter()
            .map(|entry| BlockingSource {
                id: entry.id.clone(),
                name: entry.name.clone(),
                rules: entry.rules,
                error: entry.error.clone(),
            })
            .collect(),
    }
}

/// The lists Loom offers, for Settings to list with their state.
#[tauri::command]
pub fn browser_blocking_presets() -> Vec<BlockingPreset> {
    loom_core::browser::lists::LIST_PRESETS
        .iter()
        .map(|(id, name, url)| BlockingPreset {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
        })
        .collect()
}

/// One offered filter list. `pub` because a command returns it, so it is part of
/// the IPC surface rather than an internal shape.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockingPreset {
    id: String,
    name: String,
    url: String,
}

/// Rebuilds the blocker from the config.
///
/// Takes effect on the **next tab**, and that is deliberate rather than lazy: the
/// request filter is installed per webview at creation, and re-installing it on
/// every open tab would need a token per tab that the config change has no way to
/// reach. A note in Settings says so, and `browser_reload_tabs` reloads them.
#[tauri::command]
pub async fn browser_refresh_blocking(
    state: tauri::State<'_, AppState>,
) -> Result<BlockingStatus, String> {
    let config = state.snapshot().browser.blocking;
    let client = state.engine.http_client();
    refresh_blocking(&state, &client, &config).await;
    Ok(browser_blocking_status(state))
}

/// Sets the blocking config, then rebuilds.
///
/// Returns the whole config rather than just the status, matching every other
/// settings command: the write is on the backend, and the UI's copy has to be
/// replaced or the switch would snap back to where it was on the next render.
#[tauri::command]
pub async fn browser_set_blocking(
    state: tauri::State<'_, AppState>,
    blocking: loom_core::config::BlockingConfig,
) -> Result<loom_core::config::AppConfig, String> {
    let updated = state.mutate({
        let blocking = blocking.clone();
        move |config| config.browser.blocking = blocking
    })?;
    let client = state.engine.http_client();
    refresh_blocking(&state, &client, &blocking).await;
    Ok(updated)
}

/// Fetches every configured list and installs the result.
///
/// A failure here is never fatal, and never silent: a blocker that quietly runs
/// on one list instead of three is the kind of thing nobody notices until an ad
/// appears, so each list's state is recorded and reported.
async fn refresh_blocking(
    state: &tauri::State<'_, AppState>,
    client: &reqwest::Client,
    config: &loom_core::config::BlockingConfig,
) {
    if !config.enabled {
        state.browser.blocker.set(
            loom_core::browser::filters::Filters::new(),
            Vec::new(),
        );
        *lock(&state.blocking_sources) = Vec::new();
        return;
    }
    let loaded = loom_core::browser::lists::load(client, config).await;
    state
        .browser
        .blocker
        .set(loaded.filters, config.allow.clone());
    *lock(&state.blocking_sources) = loaded.sources;
}
