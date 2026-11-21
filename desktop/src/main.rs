//! The LOO-4 spike: a GPUI window on Windows (GPUI Kit), with a transcript of
//! thousands of Markdown messages in a virtual list, and a reply streamed into
//! it by the Calx core (../core/spike.calx), linked in as a static library,
//! from the core's own thread through its C boundary.

mod core;

use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

const OLD_MESSAGES: usize = 3000;

struct Chat {
    messages: Vec<SharedString>,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    notified: bool,
    started: Instant,
    deltas: usize,
    done_after: Option<Duration>,
    _core: core::Core,
}

/// The `"text"` of a delta frame. A spike's parser: the real app reads the
/// core's frames with serde.
fn delta_text(frame: &str) -> Option<String> {
    if !frame.contains("\"t\":\"delta\"") {
        return None;
    }
    let start = frame.find("\"text\":\"")? + 8;
    let rest = &frame[start..];
    let end = rest.find("\"}").or_else(|| rest.rfind('"'))?;
    Some(rest[..end].replace("\\n", "\n").replace("\\\"", "\""))
}

fn old_message(i: usize) -> SharedString {
    match i % 4 {
        0 => format!("**Message {i}.** Did the build on `main` pass?").into(),
        1 => format!("Yes. Build {i} passed in 3 minutes. The slow step:\n\n```sh\ncargo build --release --locked\n```").into(),
        2 => format!("Message {i}: a longer reply, to give the list rows of different heights. It goes on for a while so that it wraps over several lines in a window of ordinary width, as real replies do.").into(),
        _ => format!("- item one of {i}\n- item two\n- item three").into(),
    }
}

/// The `"got"` of an echo frame: what the core was sent.
fn echo_text(frame: &str) -> Option<String> {
    if !frame.contains("\"t\":\"echo\"") {
        return None;
    }
    let start = frame.find("\"got\":\"")? + 7;
    let rest = &frame[start..];
    let end = rest.rfind('"')?;
    Some(
        rest[..end]
            .replace("\\n", "\n")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\"),
    )
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A desktop notification, as the app shows when a run finishes in the
/// background.
fn notify(body: &str) {
    let _ = notify_rust::Notification::new()
        .summary("loom")
        .body(body)
        .show();
}

impl Chat {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut messages: Vec<SharedString> = (0..OLD_MESSAGES).map(old_message).collect();
        messages.push(SharedString::default()); // the reply the core streams
        let count = messages.len();
        let scroller = cx.new(|cx| MessageScrollerState::new(count, cx));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true)
                .placeholder("Message loom")
        });
        cx.subscribe_in(
            &composer,
            window,
            |chat: &mut Chat, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = state.read(cx).value().trim().to_string();
                    if text.is_empty() {
                        return;
                    }
                    chat._core.send(&format!(
                        "{{\"t\":\"send\",\"text\":{}}}",
                        json_string(&text)
                    ));
                    state.update(cx, |s, cx| s.set_value("", window, cx));
                }
            },
        )
        .detach();

        let (core, events) = core::Core::start("{\"data\":\"spike\"}");
        core.send("{\"t\":\"hello\"}");
        // The core's events come on its thread; this task brings them to the UI.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(4))
                .await;
            let mut got = Vec::new();
            while let Ok(e) = events.try_recv() {
                got.push(e);
            }
            if got.is_empty() {
                continue;
            }
            let ok = this.update(cx, |chat, cx| {
                let last = OLD_MESSAGES;
                let mut text = chat.messages[last].to_string();
                let mut added = 0;
                for e in got {
                    if let Some(t) = delta_text(&e) {
                        text.push_str(&t);
                        chat.deltas += 1;
                    } else if let Some(sent) = echo_text(&e) {
                        // The spike's core echoes what it's sent: show it as a sent message.
                        chat.messages.push(format!("**You:** {sent}").into());
                        added += 1;
                    } else if e.contains("run.finished") {
                        chat.done_after = Some(chat.started.elapsed());
                        if !chat.notified {
                            chat.notified = true;
                            notify("The reply is done.");
                        }
                    }
                }
                chat.messages[last] = text.into();
                chat.scroller.update(cx, |s, cx| {
                    s.remeasure_items(last..last + 1, cx);
                    if added > 0 {
                        s.append(added, cx);
                    }
                });
                cx.notify();
            });
            if ok.is_err() {
                return;
            }
        })
        .detach();
        Chat {
            messages,
            scroller,
            composer,
            notified: false,
            started: Instant::now(),
            deltas: 0,
            done_after: None,
            _core: core,
        }
    }
}

impl Render for Chat {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status: SharedString = match self.done_after {
            Some(d) => format!(
                "{} old messages, and {} deltas from the Calx core in {} ms",
                OLD_MESSAGES,
                self.deltas,
                d.as_millis()
            )
            .into(),
            None => format!("{} deltas…", self.deltas).into(),
        };
        let messages = self.messages.clone();
        let list = MessageScroller::new("transcript", self.scroller.clone(), move |i, _, _| {
            div().px(px(32.)).py(px(10.)).max_w(px(780.)).child(
                TextView::markdown(ElementId::Integer(i as u64), messages[i].clone())
                    .selectable(true),
            )
        });
        div()
            .size_full()
            .bg(rgb(0x0b0b0c))
            .text_color(rgb(0xededee))
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(32.))
                    .py(px(12.))
                    .text_color(rgb(0x7c7c85))
                    .child(status),
            )
            .child(div().flex_1().child(list))
            .child(
                div()
                    .px(px(32.))
                    .py(px(12.))
                    .max_w(px(844.))
                    .child(Textarea::new(&self.composer)),
            )
            // Files dropped on the window are attached; the spike sends their paths.
            .on_drop(cx.listener(|chat, paths: &ExternalPaths, _, _| {
                for p in paths.paths() {
                    chat._core.send(&format!(
                        "{{\"t\":\"attach\",\"path\":{}}}",
                        json_string(&p.display().to_string())
                    ));
                }
            }))
    }
}

/// The tray icon: a plain square for the spike.
fn tray() -> tray_icon::TrayIcon {
    let size = 32u32;
    let rgba: Vec<u8> = (0..size * size)
        .flat_map(|_| [0xed, 0xed, 0xee, 0xff])
        .collect();
    let icon = tray_icon::Icon::from_rgba(rgba, size, size).expect("a tray icon");
    tray_icon::TrayIconBuilder::new()
        .with_tooltip("loom")
        .with_icon(icon)
        .build()
        .expect("a tray icon")
}

fn main() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| Chat::new(window, cx)),
        )
        .expect("failed to open the window");
        // The tray icon lives as long as the app; GPUI's message loop serves it.
        std::mem::forget(tray());
        cx.activate(true);
    });
}
