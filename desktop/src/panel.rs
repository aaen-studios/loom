//! Panels (PLAN.md, "The panel sandbox"): a plugin's small web app, in a web view
//! of its own laid over the window. A panel is code an agent wrote, or someone
//! else did, so it's walled off: it loads from a scheme and origin of its own,
//! reaches no network, keeps nothing in the web view, and talks to the app only
//! through one door, whose messages the app checks.

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use raw_window_handle::HasWindowHandle;
use serde_json::{json, Value};
use wry::http::{header, Request, Response, StatusCode};
use wry::{NewWindowResponse, PermissionResponse, Rect, WebContext, WebView, WebViewBuilder};

/// A panel's files, by path, as its plugin has them.
pub type Files = HashMap<String, Vec<u8>>;

/// The content security policy every page of a panel comes with: its own files,
/// its inline scripts and styles, and nothing from anywhere else. No fetch, no
/// frames, no workers, no forms. And sandboxed, with scripts: the page's origin is
/// no one's, and so is each blank frame's it makes, which it can't reach into, so
/// a frame the prelude doesn't run in (WKWebView's) lends it nothing.
pub const CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; media-src 'self' data: blob:; connect-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'; manifest-src 'none'; form-action 'none'; base-uri 'none'; sandbox allow-scripts";

/// The browser's own settings, for every panel. Past the policy and the request
/// filter, HTTP gets nowhere either: every request goes to a proxy that isn't
/// there, and no host name is looked up. wry's own defaults come first. WebRTC
/// ignores the proxy, and WebView2 has no switch for it (Chrome's
/// --force-webrtc-ip-handling-policy does nothing there), so the prelude takes it
/// away, in every frame.
#[cfg(windows)]
const BROWSER_ARGS: &str = concat!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    " --proxy-server=http://127.0.0.1:9 --proxy-bypass-list=<-loopback>",
    " --host-resolver-rules=\"MAP * ~NOTFOUND\"",
    " --disable-background-networking --no-pings --dns-prefetch-disable",
);

/// Runs before a panel's own scripts, in every frame that loads (WKWebView leaves out
/// a blank frame, which the policy's sandbox keeps the page out of): turns off the web
/// view's storage (a panel's data is the instance's, through the door) and WebRTC,
/// and gives the panel `loom.send(op, args)`, the door, which answers with a promise.
const PRELUDE: &str = r#"(() => {
  const off = (o, k) => { try { Object.defineProperty(o, k, { get() { throw new Error("A panel keeps its data through loom.send."); }, configurable: false }); } catch (e) {} };
  for (const k of ["localStorage", "sessionStorage", "indexedDB", "caches", "openDatabase", "RTCPeerConnection", "webkitRTCPeerConnection", "RTCDataChannel"]) off(window, k);
  off(navigator, "serviceWorker");
  const post = (window.chrome && window.chrome.webview) ? (m) => window.chrome.webview.postMessage(m) : (m) => window.ipc.postMessage(m);
  const waiting = new Map();
  let next = 1;
  Object.defineProperty(window, "__loom_answer", { value: (id, ok, value) => {
    const w = waiting.get(id);
    if (!w) return;
    waiting.delete(id);
    if (ok) w.resolve(value); else w.reject(new Error(value));
  }, configurable: false, writable: false });
  Object.defineProperty(window, "loom", { value: Object.freeze({
    send(op, args) {
      const id = next++;
      post(JSON.stringify({ id, op: String(op), args: args === undefined ? {} : args }));
      return new Promise((resolve, reject) => waiting.set(id, { resolve, reject }));
    },
  }), configurable: false, writable: false });
})();"#;

/// Runs after the prelude: a click on a link out of the panel does nothing, since
/// WKWebView connects to the link's host as it's clicked, before the policy, the
/// sandbox or the rule list see anything, and around the proxy for this computer and
/// its network. A capturing listener cancels it: on the window, on every shadow root
/// the page attaches, and around a click the page sends into a tree of its own.
/// Everything it uses is taken before the page's scripts run. `HOME` is the panel's
/// origin and a slash.
const LINKS: &str = r#"(() => {
  const home = HOME;
  const apply = Reflect.apply;
  const getter = (o, k) => Object.getOwnPropertyDescriptor(o, k).get;
  const pathOf = Event.prototype.composedPath;
  const cancel = Event.prototype.preventDefault;
  const starts = String.prototype.startsWith;
  const hrefs = [getter(HTMLAnchorElement.prototype, "href"), getter(HTMLAreaElement.prototype, "href")];
  const listen = EventTarget.prototype.addEventListener;
  const unlisten = EventTarget.prototype.removeEventListener;
  const rootOf = Node.prototype.getRootNode;
  const click = HTMLElement.prototype.click;
  const dispatch = EventTarget.prototype.dispatchEvent;
  const attach = Element.prototype.attachShadow;
  const internalsRoot = window.ElementInternals ? getter(ElementInternals.prototype, "shadowRoot") : null;
  const doc = document;
  const out = (node) => {
    for (let i = 0; i < 2; i++) {
      let url;
      try { url = apply(hrefs[i], node, []); } catch (e) { continue; }
      return url !== "" && !apply(starts, url, [home]) && !apply(starts, url, ["javascript:"]);
    }
    return false;
  };
  const guard = (e) => {
    let stop = true;
    try {
      const path = apply(pathOf, e, []);
      stop = false;
      for (let i = 0; i < path.length; i++) if (out(path[i])) stop = true;
    } catch (x) { stop = true; }
    if (stop) apply(cancel, e, []);
  };
  const watch = (t) => { apply(listen, t, ["click", guard, true]); apply(listen, t, ["auxclick", guard, true]); };
  const unwatch = (t) => { apply(unlisten, t, ["click", guard, true]); apply(unlisten, t, ["auxclick", guard, true]); };
  watch(window);
  const around = (target, f) => {
    let own = null, top = null;
    try { own = apply(rootOf, target, []); top = apply(rootOf, target, [{ composed: true }]); } catch (e) {}
    if (own === doc) own = null;
    if (top === doc || top === own) top = null;
    if (own) watch(own);
    if (top) watch(top);
    try { return f(); } finally { if (own) unwatch(own); if (top) unwatch(top); }
  };
  const put = (o, k, v) => Object.defineProperty(o, k, { value: v, writable: false, configurable: false });
  put(HTMLElement.prototype, "click", function click_() { return around(this, () => apply(click, this, [])); });
  put(EventTarget.prototype, "dispatchEvent", function dispatchEvent(ev) { return around(this, () => apply(dispatch, this, [ev])); });
  put(Element.prototype, "attachShadow", function attachShadow(init) { const r = apply(attach, this, [init]); watch(r); return r; });
  if (internalsRoot) Object.defineProperty(ElementInternals.prototype, "shadowRoot", { get() { const r = apply(internalsRoot, this, []); if (r) watch(r); return r; }, configurable: false });
})();"#;

/// The prelude and the link guard for a panel whose origin is `origin`.
fn prelude(origin: &str) -> String {
    let home = serde_json::to_string(&format!("{origin}/")).unwrap_or_default();
    format!("{PRELUDE}\n{}", LINKS.replacen("HOME", &home, 1))
}

/// A message through the door: what the panel asks the app for.
#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub op: String,
    pub args: Value,
}

/// A panel on screen. It goes when this is dropped.
pub struct Panel {
    view: WebView,
}

/// The origin a panel's pages have: its scheme's, which no other panel shares.
pub fn origin(scheme: &str) -> String {
    if cfg!(windows) {
        format!("http://{scheme}.localhost")
    } else {
        format!("{scheme}://localhost")
    }
}

/// A panel's scheme, from its plugin's id: `panel-` and the id's letters and digits.
pub fn scheme(id: &str) -> String {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    format!("panel-{safe}")
}

/// The web views' own folder: in the app's data folder, where it can write. Every
/// panel shares it, and keeps nothing in it, as each one is in private mode.
pub fn context(data: &std::path::Path) -> WebContext {
    WebContext::new(Some(data.join("panels")))
}

/// What a panel shows: its plugin's id, its files, and the page it starts at (which
/// may have a query).
pub struct Source {
    pub id: String,
    pub files: Arc<Files>,
    pub entry: String,
}

impl Panel {
    /// Shows `source` over `window`, at `bounds`. Each message through the door comes
    /// to `door`; anything refused (a request outside the panel, a navigation, a
    /// window, a permission, a download, a message that isn't one) comes to
    /// `refused`, said in a few words.
    pub fn open(
        window: &impl HasWindowHandle,
        context: &mut WebContext,
        source: &Source,
        bounds: Rect,
        door: impl Fn(Message) + 'static,
        refused: impl Fn(String) + 'static,
    ) -> Result<Panel, String> {
        let scheme = scheme(&source.id);
        let origin = origin(&scheme);
        #[cfg(target_os = "macos")]
        let config = mac::configuration(context.data_directory())?;
        let refused: Rc<dyn Fn(String)> = Rc::new(refused);
        let entry_path = source.entry.trim_start_matches('/').to_string();
        let serve = {
            let files = source.files.clone();
            let entry_file = entry_path.split('?').next().unwrap_or("").to_string();
            move |_: wry::WebViewId, req: Request<Vec<u8>>| {
                serve(&files, &entry_file, req.uri().path())
            }
        };
        let navigated = {
            let refused = refused.clone();
            let origin = origin.clone();
            move |url: String| {
                let ok = within(&url, &origin);
                if !ok {
                    refused(format!("navigate {url}"));
                }
                ok
            }
        };
        let windows = {
            let refused = refused.clone();
            move |url: String, _| {
                refused(format!("window {url}"));
                NewWindowResponse::Deny
            }
        };
        let downloads = {
            let refused = refused.clone();
            move |url: String, _: &mut std::path::PathBuf| {
                refused(format!("download {url}"));
                false
            }
        };
        let messages = {
            let refused = refused.clone();
            let origin = origin.clone();
            move |req: Request<String>| {
                let from = req.uri().to_string();
                if !within(&from, &origin) {
                    refused(format!("message from {from}"));
                    return;
                }
                match read_message(req.body()) {
                    Some(m) => door(m),
                    None => refused(String::from("message that isn't one")),
                }
            }
        };
        let builder = WebViewBuilder::new_with_web_context(context)
            .with_custom_protocol(scheme.clone(), serve)
            .with_url(format!("{origin}/{entry_path}"))
            .with_bounds(bounds)
            .with_initialization_script_for_main_only(prelude(&origin), false)
            .with_navigation_handler(navigated)
            .with_new_window_req_handler(windows)
            .with_download_started_handler(downloads)
            .with_permission_handler(|_| PermissionResponse::Deny)
            .with_drag_drop_handler(|_| true)
            .with_ipc_handler(messages)
            .with_incognito(true)
            .with_devtools(false)
            .with_clipboard(false)
            .with_hotkeys_zoom(false)
            .with_autoplay(false);
        #[cfg(windows)]
        let builder = {
            use wry::WebViewBuilderExtWindows;
            builder
                .with_additional_browser_args(BROWSER_ARGS)
                .with_default_context_menus(false)
                .with_browser_accelerator_keys(false)
        };
        #[cfg(target_os = "macos")]
        let builder = {
            use wry::WebViewBuilderExtMacos;
            builder.with_webview_configuration(config)
        };
        let view = builder.build_as_child(window).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        refuse_the_rest(&view, origin, refused).map_err(|e| e.to_string())?;
        Ok(Panel { view })
    }

    /// Answers the message `id`: its value, or why it's refused.
    pub fn answer(&self, id: i64, answer: Result<Value, String>) {
        let (ok, value) = match answer {
            Ok(v) => (true, v),
            Err(why) => (false, Value::String(why)),
        };
        let _ = self
            .view
            .evaluate_script(&format!("window.__loom_answer({id}, {ok}, {value});"));
    }

    /// Moves the panel, as the window changes size.
    pub fn set_bounds(&self, bounds: Rect) {
        let _ = self.view.set_bounds(bounds);
    }
}

/// Whether `url` is the panel's own: its origin, then a path or nothing.
fn within(url: &str, origin: &str) -> bool {
    url == origin || url.starts_with(&format!("{origin}/"))
}

/// The door's message in `body`: `{"id", "op", "args"}`, or none.
fn read_message(body: &str) -> Option<Message> {
    let v: Value = serde_json::from_str(body).ok()?;
    Some(Message {
        id: v.get("id")?.as_i64()?,
        op: v.get("op")?.as_str()?.to_string(),
        args: v.get("args").cloned().unwrap_or(json!({})),
    })
}

/// A file of the panel's, with the policy, or 404. Any origin may read it, as the
/// sandboxed page's own is no one's: its fonts and modules come with CORS. Only the
/// panel's own web view has its scheme.
fn serve(files: &Files, entry: &str, path: &str) -> Response<Cow<'static, [u8]>> {
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { entry } else { path };
    let Some(bytes) = files.get(path) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_SECURITY_POLICY, CSP)
            .body(Cow::Borrowed(&b""[..]))
            .expect("a response");
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime(path))
        .header(header::CONTENT_SECURITY_POLICY, CSP)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header("Referrer-Policy", "no-referrer")
        .header("X-DNS-Prefetch-Control", "off")
        .body(Cow::Owned(bytes.clone()))
        .expect("a response")
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

/// WebView2 answers every request that isn't the panel's own with 403, before it
/// reaches the network: images, scripts, frames and the rest, whatever the page
/// does to get past its policy.
#[cfg(windows)]
fn refuse_the_rest(
    view: &WebView,
    origin: String,
    refused: Rc<dyn Fn(String)>,
) -> windows::core::Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_22, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
    };
    use webview2_com::{take_pwstr, WebResourceRequestedEventHandler};
    use windows::core::{Interface, HSTRING, PWSTR};
    use wry::WebViewExtWindows;

    let core = view.webview();
    let env = view.environment();
    let all = HSTRING::from("*");
    unsafe {
        match core.cast::<ICoreWebView2_22>() {
            Ok(core_22) => core_22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                &all,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?,
            Err(_) => {
                core.AddWebResourceRequestedFilter(&all, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?
            }
        }
        let mut token = 0i64;
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut uri = PWSTR::null();
                args.Request()?.Uri(&mut uri)?;
                let uri = take_pwstr(uri);
                if within(&uri, &origin) {
                    return Ok(());
                }
                refused(format!("request {uri}"));
                let answer = env.CreateWebResourceResponse(
                    None,
                    403,
                    &HSTRING::from("Forbidden"),
                    &HSTRING::new(),
                )?;
                args.SetResponse(&answer)
            })),
            &mut token,
        )?;
    }
    Ok(())
}

/// What WKWebView gives a panel's web view on a Mac, past what wry sets: a data store
/// that keeps nothing, a content rule list that blocks every load but the panel's own
/// (the Mac's `refuse_the_rest`), no windows opened by a script, no AirPlay, and no
/// Safe Browsing lookups of where it goes.
#[cfg(target_os = "macos")]
mod mac {
    use std::cell::RefCell;
    use std::ffi::c_char;
    use std::path::Path;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, sel, MainThreadMarker};
    use objc2_foundation::{
        NSArray, NSDate, NSDefaultRunLoopMode, NSError, NSObjectProtocol, NSRunLoop, NSString,
        NSURL,
    };
    use objc2_web_kit::{
        WKContentRuleList, WKContentRuleListStore, WKWebViewConfiguration, WKWebsiteDataStore,
    };

    /// Blocks every load, then lets through the panel's own scheme and the data: and
    /// blob: URLs its policy allows, which never leave the web view. A panel's web view
    /// has its own scheme and no other panel's, so the one rule list does for all.
    const RULES: &str = r#"[
  {"trigger": {"url-filter": ".*"}, "action": {"type": "block"}},
  {"trigger": {"url-filter": "^panel-[a-z0-9]*://localhost/"}, "action": {"type": "ignore-previous-rules"}},
  {"trigger": {"url-filter": "^data:"}, "action": {"type": "ignore-previous-rules"}},
  {"trigger": {"url-filter": "^blob:"}, "action": {"type": "ignore-previous-rules"}}
]"#;

    thread_local! {
        static COMPILED: RefCell<Option<Retained<WKContentRuleList>>> = const { RefCell::new(None) };
    }

    /// What WebKit's compiler answers: the rule list, or why not.
    type Compiled = Result<Retained<WKContentRuleList>, String>;

    /// A web view's configuration for a panel. The first one waits for WebKit to compile
    /// the rule list, into `dir` (the web views' folder), running the main thread's
    /// loop meanwhile, as WebView2 does while it starts.
    pub fn configuration(dir: Option<&Path>) -> Result<Retained<WKWebViewConfiguration>, String> {
        let mtm = MainThreadMarker::new().ok_or("a panel is made on the main thread")?;
        let rules = rules(dir, mtm)?;
        // SAFETY: WebKit's own setters, on the main thread, with objects it made.
        unsafe {
            let config = WKWebViewConfiguration::new(mtm);
            let store = WKWebsiteDataStore::nonPersistentDataStore(mtm);
            dead_end(&store)?;
            config.setWebsiteDataStore(&store);
            config.userContentController().addContentRuleList(&rules);
            config.setAllowsAirPlayForMediaPlayback(false);
            let prefs = config.preferences();
            prefs.setJavaScriptCanOpenWindowsAutomatically(false);
            prefs.setFraudulentWebsiteWarningEnabled(false);
            Ok(config)
        }
    }

    #[link(name = "Network", kind = "framework")]
    extern "C" {
        fn nw_endpoint_create_host(host: *const c_char, port: *const c_char) -> *mut AnyObject;
        fn nw_proxy_config_create_http_connect(
            endpoint: *mut AnyObject,
            tls: *mut AnyObject,
        ) -> *mut AnyObject;
        fn nw_proxy_config_set_failover_allowed(config: *mut AnyObject, allowed: bool);
    }

    /// Sends whatever gets past the rules to a proxy that isn't there, and never around
    /// it when it doesn't answer, as WebView2's does: from macOS 14, which takes one.
    /// macOS never sends this computer's addresses or its network's to a proxy, so
    /// these get no further than the rules and the prelude's link guard let them.
    fn dead_end(store: &WKWebsiteDataStore) -> Result<(), String> {
        if !store.respondsToSelector(sel!(setProxyConfigurations:)) {
            return Ok(());
        }
        // SAFETY: Network's own constructors, each of which hands over a new object or
        // null, and WebKit's setter for its proxies, which takes an array of them.
        unsafe {
            let endpoint = Retained::from_raw(nw_endpoint_create_host(
                c"127.0.0.1".as_ptr(),
                c"9".as_ptr(),
            ))
            .ok_or("Network made no endpoint")?;
            let proxy = Retained::from_raw(nw_proxy_config_create_http_connect(
                Retained::as_ptr(&endpoint) as *mut AnyObject,
                std::ptr::null_mut(),
            ))
            .ok_or("Network made no proxy")?;
            nw_proxy_config_set_failover_allowed(Retained::as_ptr(&proxy) as *mut AnyObject, false);
            let proxies = NSArray::from_retained_slice(&[proxy]);
            let _: () = msg_send![store, setProxyConfigurations: &*proxies];
        }
        Ok(())
    }

    fn rules(
        dir: Option<&Path>,
        mtm: MainThreadMarker,
    ) -> Result<Retained<WKContentRuleList>, String> {
        if let Some(compiled) = COMPILED.with(|c| c.borrow().clone()) {
            return Ok(compiled);
        }
        // SAFETY: WebKit's stores, on the main thread.
        let store = unsafe {
            match dir {
                Some(dir) => {
                    let url = NSURL::fileURLWithPath(&NSString::from_str(
                        &dir.join("rules").to_string_lossy(),
                    ));
                    WKContentRuleListStore::storeWithURL(Some(&url), mtm)
                }
                None => WKContentRuleListStore::defaultStore(mtm),
            }
        }
        .ok_or("WebKit has no store for content rule lists")?;
        let got: Rc<RefCell<Option<Compiled>>> = Rc::default();
        let done = {
            let got = got.clone();
            RcBlock::new(move |list: *mut WKContentRuleList, error: *mut NSError| {
                // SAFETY: what WebKit hands the handler: a rule list or an error, either
                // of which may be null.
                let answer = match unsafe { Retained::retain(list) } {
                    Some(list) => Ok(list),
                    None => Err(unsafe { error.as_ref() }
                        .map(|e| e.localizedDescription().to_string())
                        .unwrap_or_else(|| String::from("no rule list"))),
                };
                *got.borrow_mut() = Some(answer);
            })
        };
        // SAFETY: the identifier, the rules and a handler WebKit calls on the main thread.
        unsafe {
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&NSString::from_str("loom-panels")),
                Some(&NSString::from_str(RULES)),
                Some(&done),
            );
        }
        let until = Instant::now() + Duration::from_secs(20);
        while got.borrow().is_none() && Instant::now() < until {
            let soon = NSDate::dateWithTimeIntervalSinceNow(0.02);
            // SAFETY: the default mode, a constant of Foundation's.
            NSRunLoop::currentRunLoop().runMode_beforeDate(unsafe { NSDefaultRunLoopMode }, &soon);
        }
        let compiled = got
            .borrow_mut()
            .take()
            .unwrap_or_else(|| Err(String::from("WebKit didn't compile the rule list in time")))
            .map_err(|e| format!("the panel's rule list didn't compile: {e}"))?;
        COMPILED.with(|c| *c.borrow_mut() = Some(compiled.clone()));
        Ok(compiled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_has_an_origin_of_its_own() {
        assert_eq!(scheme("Build-Board 2"), "panel-buildboard2");
        let o = origin("panel-a");
        assert!(within(&format!("{o}/index.html"), &o));
        assert!(!within(&format!("{o}.evil.com/x"), &o));
        assert!(!within("http://127.0.0.1:9/x", &o));
    }

    #[test]
    fn serves_its_files_with_the_policy() {
        let mut files = Files::new();
        files.insert("index.html".into(), b"<p>hi</p>".to_vec());
        let r = serve(&files, "index.html", "/");
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_SECURITY_POLICY], CSP);
        assert_eq!(
            serve(&files, "index.html", "/../secret").status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn the_link_guard_knows_the_panel_s_own_pages() {
        let p = prelude("panel-a://localhost");
        assert!(p.contains(r#"const home = "panel-a://localhost/";"#));
        assert!(!p.contains("HOME"));
        assert!(CSP.ends_with("sandbox allow-scripts"));
    }

    #[test]
    fn reads_only_whole_messages() {
        let m = read_message(r#"{"id":3,"op":"storage.get","args":{"key":"a"}}"#).unwrap();
        assert_eq!((m.id, m.op.as_str()), (3, "storage.get"));
        assert!(read_message(r#"{"op":"x"}"#).is_none());
        assert!(read_message("not json").is_none());
    }
}
