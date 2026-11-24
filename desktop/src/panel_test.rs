//! `loom --panel-test OUT`: the panel break-out test (PLAN.md, "The panel sandbox").
//! It shows `panels/breakout`, a panel that tries every way out it can, while
//! listening where that panel aims: a TCP port and a UDP port on this machine. It
//! writes what happened to OUT as JSON, and exits 0 when nothing got out and every
//! attempt was stopped, or 1.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::net::{TcpListener, UdpSocket};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::*;
use serde_json::{json, Value};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::Rect;

use crate::panel::{self, Message, Panel, Source};

const PAGE: &str = include_str!("../panels/breakout/index.html");
const GIVE_UP: Duration = Duration::from_secs(90);
/// How many ways out the panel tries, each of which it reports.
const ATTEMPTS: usize = 30;

/// What reached the listeners: the first line of each TCP connection, and each UDP
/// datagram's sender.
#[derive(Default)]
struct Heard {
    tcp: Vec<String>,
    udp: Vec<String>,
}

struct BreakOut {
    panel: Option<Panel>,
    messages: Rc<RefCell<Vec<Message>>>,
    refused: Rc<RefCell<Vec<String>>>,
    reports: Vec<Value>,
    heard: Arc<Mutex<Heard>>,
    out: std::path::PathBuf,
    started: Instant,
}

impl Render for BreakOut {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

/// Runs the test, and exits.
pub fn run(out: std::path::PathBuf) {
    let heard = Arc::new(Mutex::new(Heard::default()));
    let tcp = TcpListener::bind("127.0.0.1:0").expect("a TCP port");
    let udp = UdpSocket::bind("127.0.0.1:0").expect("a UDP port");
    let tcp_port = tcp.local_addr().expect("its address").port();
    let udp_port = udp.local_addr().expect("its address").port();
    {
        let heard = heard.clone();
        std::thread::spawn(move || {
            for conn in tcp.incoming().flatten() {
                let mut first = [0u8; 256];
                let _ = conn.set_read_timeout(Some(Duration::from_millis(500)));
                let n = (&conn).read(&mut first).unwrap_or(0);
                let line = String::from_utf8_lossy(&first[..n])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                heard.lock().unwrap().tcp.push(line);
            }
        });
    }
    {
        let heard = heard.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok((_, from)) = udp.recv_from(&mut buf) {
                heard.lock().unwrap().udp.push(from.to_string());
            }
        });
    }
    // One folder for every run: each panel is private, so it keeps nothing there.
    let data = std::env::temp_dir().join("loom-panel-test");
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: false,
                show: false,
                ..Default::default()
            },
            cx,
            move |window, cx| {
                out_of_the_way(window);
                cx.new(|cx| {
                    let messages = Rc::new(RefCell::new(Vec::new()));
                    let refused = Rc::new(RefCell::new(Vec::new()));
                    let mut files = HashMap::new();
                    files.insert(String::from("index.html"), PAGE.as_bytes().to_vec());
                    let source = Source {
                        id: String::from("breakout"),
                        files: Arc::new(files),
                        entry: format!("index.html?c={tcp_port}&u={udp_port}"),
                    };
                    let Ok(parent) = raw_window_handle::HasWindowHandle::window_handle(&*window)
                        .map(|h| Parent(h.as_raw()))
                    else {
                        eprintln!("loom: the window has no handle");
                        std::process::exit(2);
                    };
                    let door = {
                        let messages = messages.clone();
                        move |m| messages.borrow_mut().push(m)
                    };
                    let stopped = {
                        let refused = refused.clone();
                        move |what| refused.borrow_mut().push(what)
                    };
                    let data = data.clone();
                    // Made from a task, with nothing borrowed, as the app makes a panel
                    // (app.rs, sync_panel): making one runs the main thread's loop.
                    cx.spawn(async move |this, cx| {
                        let mut context = panel::context(&data);
                        let bounds = Rect {
                            position: LogicalPosition::new(0.0, 0.0).into(),
                            size: LogicalSize::new(800.0, 600.0).into(),
                        };
                        match Panel::open(&parent, &mut context, &source, bounds, door, stopped) {
                            Ok(p) => {
                                let _ = this.update(cx, |t: &mut BreakOut, _| t.panel = Some(p));
                            }
                            Err(e) => {
                                eprintln!("loom: the panel didn't open: {e}");
                                std::process::exit(2);
                            }
                        }
                        loop {
                            cx.background_executor()
                                .timer(Duration::from_millis(50))
                                .await;
                            let finished = this
                                .update(cx, |t: &mut BreakOut, _| t.step())
                                .unwrap_or(true);
                            if finished {
                                break;
                            }
                        }
                    })
                    .detach();
                    BreakOut {
                        panel: None,
                        messages,
                        refused,
                        reports: Vec::new(),
                        heard: heard.clone(),
                        out: out.clone(),
                        started: Instant::now(),
                    }
                })
            },
        )
        .expect("a window");
    });
}

/// The test's window, by its handle, for the task that makes the panel.
struct Parent(raw_window_handle::RawWindowHandle);

impl raw_window_handle::HasWindowHandle for Parent {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // SAFETY: the handle is the test's one window, which lives until the test exits,
        // and the panel is made on its thread.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.0) })
    }
}

/// Shows the test's window far off every screen, without taking focus, as the test
/// often runs on a computer someone is working at. A Mac's loom has no Dock icon
/// while it runs.
fn out_of_the_way(window: &Window) {
    let handle = crate::shell::hwnd(window);
    #[cfg(windows)]
    {
        let scale = window.scale_factor();
        let (w, h) = ((800.0 * scale) as i32, (600.0 * scale) as i32);
        crate::shell::out_of_the_way(handle, w, h, true);
    }
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSView};
        use objc2_foundation::NSPoint;
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        if handle == 0 {
            return;
        }
        // SAFETY: the NSView of the test's own window, which lives until the test
        // exits, on the main thread.
        let view: &NSView = unsafe { &*(handle as *const NSView) };
        // macOS keeps part of a window on a screen, so it's also clear and lets clicks
        // through, behind every other window.
        if let Some(w) = view.window() {
            w.setAlphaValue(0.0);
            w.setIgnoresMouseEvents(true);
            w.setHasShadow(false);
            w.orderBack(None);
            w.setFrameOrigin(NSPoint::new(-10000.0, -10000.0));
        }
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let _ = handle;
}

impl BreakOut {
    /// Answers what came through the door. Gives whether the test is over.
    fn step(&mut self) -> bool {
        let came: Vec<Message> = self.messages.borrow_mut().drain(..).collect();
        let mut done = false;
        for m in came {
            let answer = match m.op.as_str() {
                "report" => {
                    self.reports.push(m.args.clone());
                    Ok(json!(true))
                }
                "done" => {
                    done = true;
                    Ok(json!(true))
                }
                // The only ops this panel may use. Anything else is refused, as a
                // plugin's would be when its manifest didn't ask for it.
                other => {
                    self.refused.borrow_mut().push(format!("op {other}"));
                    Err(format!("{other} isn't allowed for this panel"))
                }
            };
            if let Some(p) = &self.panel {
                p.answer(m.id, answer);
            }
        }
        if !done && self.started.elapsed() < GIVE_UP {
            return false;
        }
        // A moment for anything still on its way to the listeners.
        std::thread::sleep(Duration::from_millis(1500));
        let heard = self.heard.lock().unwrap();
        let got_out: Vec<&Value> = self
            .reports
            .iter()
            .filter(|r| r.get("blocked") != Some(&json!(true)))
            .collect();
        let refused = self.refused.borrow().clone();
        let navigation_stopped = refused.iter().any(|r| r.starts_with("navigate "));
        let passed = done
            && got_out.is_empty()
            && heard.tcp.is_empty()
            && heard.udp.is_empty()
            && navigation_stopped
            && self.reports.len() == ATTEMPTS;
        let result = json!({
            "passed": passed,
            "finished": done,
            "reports": self.reports,
            "got_out": got_out,
            "refused": refused,
            "heard": {"tcp": heard.tcp, "udp": heard.udp},
        });
        let _ = std::fs::write(
            &self.out,
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        );
        std::process::exit(if passed { 0 } else { 1 });
    }
}
