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
/// frames, no workers, no forms.
pub const CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; media-src 'self' data: blob:; connect-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'; manifest-src 'none'; form-action 'none'; base-uri 'none'";

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

/// Runs before a panel's own scripts, in every frame: turns off the web view's
/// storage (a panel's data is the instance's, through the door) and WebRTC, and
/// gives the panel `loom.send(op, args)`, the door, which answers with a promise.
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
            .with_initialization_script(PRELUDE)
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

/// A file of the panel's, with the policy, or 404.
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
        .header("Referrer-Policy", "no-referrer")
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
    fn reads_only_whole_messages() {
        let m = read_message(r#"{"id":3,"op":"storage.get","args":{"key":"a"}}"#).unwrap();
        assert_eq!((m.id, m.op.as_str()), (3, "storage.get"));
        assert!(read_message(r#"{"op":"x"}"#).is_none());
        assert!(read_message("not json").is_none());
    }
}
