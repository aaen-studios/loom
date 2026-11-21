//! The window: the sidebar of agents, and the chat with the one picked. The look
//! follows design/explorations/round-4.html, with every value from design/tokens.toml.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{Input, InputEvent, InputState, Paste, Textarea, TextareaState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Icon, IconName, Sizable as _, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;

use crate::core::Core;
use crate::model::{self, Approval, Conn, Message, Model, Mood};
use crate::shell;
use crate::thread::{self, thread};
use crate::tokens::*;

/// The colours of one theme.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Hsla,
    pub side: Hsla,
    pub raised: Hsla,
    pub raised_2: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub line: Hsla,
    pub line_strong: Hsla,
    pub ink: Hsla,
    pub ink_2: Hsla,
    pub ink_3: Hsla,
    pub button: Hsla,
    pub button_ink: Hsla,
    pub bubble: Hsla,
    pub ok: Hsla,
    pub bad: Hsla,
}

impl Palette {
    pub fn new(dark: bool) -> Palette {
        let c = |d: u32, l: u32| -> Hsla { rgb(if dark { d } else { l }).into() };
        use color_dark as d;
        use color_light as l;
        Palette {
            dark,
            bg: c(d::BG, l::BG),
            side: c(d::SIDE, l::SIDE),
            raised: c(d::RAISED, l::RAISED),
            raised_2: c(d::RAISED_2, l::RAISED_2),
            hover: c(d::HOVER, l::HOVER),
            active: c(d::ACTIVE, l::ACTIVE),
            line: c(d::LINE, l::LINE),
            line_strong: c(d::LINE_STRONG, l::LINE_STRONG),
            ink: c(d::INK, l::INK),
            ink_2: c(d::INK_2, l::INK_2),
            ink_3: c(d::INK_3, l::INK_3),
            button: c(d::BUTTON, l::BUTTON),
            button_ink: c(d::BUTTON_INK, l::BUTTON_INK),
            bubble: c(d::BUBBLE, l::BUBBLE),
            ok: c(d::OK, l::OK),
            bad: c(d::BAD, l::BAD),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Chat,
    NewAgent,
    EditAgent,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Agent,
    Permission,
}

/// One row of the chat's list.
#[derive(Clone)]
enum Row {
    Message(Message),
    Approval(Approval),
}

pub struct Loom {
    model: Model,
    core: Core,
    events: std::sync::mpsc::Receiver<String>,
    p: Palette,
    screen: Screen,
    menu: Option<Menu>,
    rows: Rc<Vec<Row>>,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    server_in: Entity<InputState>,
    user_in: Entity<InputState>,
    pass_in: Entity<InputState>,
    code_in: Entity<InputState>,
    name_in: Entity<InputState>,
    instructions_in: Entity<TextareaState>,
    search_in: Entity<InputState>,
    program: Option<String>,
    open_thoughts: HashSet<String>,
    tray: Option<shell::Tray>,
    hwnd: isize,
    /// The newest thing that went wrong, shown for a while above the composer.
    notice: Option<(String, Instant)>,
}

fn data_dir() -> String {
    if let Ok(dir) = std::env::var("LOOM_DATA") {
        return dir;
    }
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    let dir = std::path::Path::new(&base).join("Loom");
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().into_owned()
}

impl Loom {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dark = match std::env::var("LOOM_THEME").as_deref() {
            Ok("dark") => true,
            Ok("light") => false,
            _ => !matches!(
                window.appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
        };
        let (core, events) = Core::start(&core_config());

        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 10)
                .submit_on_enter(true)
                .placeholder("Reply")
        });
        cx.subscribe_in(
            &composer,
            window,
            |this: &mut Loom, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    let text = state.read(cx).value().to_string();
                    if let Some(frame) = this.model.send(&text) {
                        this.core.send(&frame.to_string());
                        state.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                }
            },
        )
        .detach();

        let input = |window: &mut Window,
                     cx: &mut Context<Self>,
                     placeholder: &'static str,
                     masked: bool| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .masked(masked)
            })
        };
        let server_in = input(window, cx, "https://loom.example.com", false);
        let user_in = input(window, cx, "Username", false);
        let pass_in = input(window, cx, "Password", true);
        let code_in = input(window, cx, "Setup code, for a new instance", false);
        let name_in = input(window, cx, "Scribe", false);
        let instructions_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 14)
                .placeholder("What it does, and how. It reads this before every chat.")
        });
        if let Ok(server) = std::env::var("LOOM_SERVER") {
            server_in.update(cx, |s, cx| s.set_value(server, window, cx));
        }
        let search_in = input(window, cx, "Search", false);
        cx.subscribe_in(
            &search_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let query = state.read(cx).value().to_string();
                    if let Some(frame) = this.model.search(&query) {
                        this.send(frame);
                    }
                    cx.notify();
                }
            },
        )
        .detach();

        // The core's events come on its thread; this task brings them to the window.
        // It also looks at the tray, and notices the system waking: a tick that comes
        // long after the last means the computer slept.
        cx.spawn(async move |this, cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                let woke = last.elapsed() > Duration::from_secs(10);
                last = Instant::now();
                let alive = this.update(cx, |loom, cx| {
                    let got: Vec<String> =
                        std::iter::from_fn(|| loom.events.try_recv().ok()).collect();
                    if woke {
                        loom.send(serde_json::json!({"t": "app.wake"}));
                    }
                    loom.tray_actions(cx);
                    loom.take_notice(cx);
                    if got.is_empty() {
                        return;
                    }
                    let mut changed = false;
                    for e in got {
                        if let Ok(v) = serde_json::from_str::<Value>(&e) {
                            if v.get("t").and_then(Value::as_str) == Some("core.notify") {
                                let s = |k: &str| {
                                    v.get(k)
                                        .and_then(Value::as_str)
                                        .unwrap_or_default()
                                        .to_string()
                                };
                                shell::notify(&s("title"), &s("body"));
                            }
                            changed |= loom.model.take(&v);
                        }
                    }
                    loom.fetch();
                    loom.open_fetched(cx);
                    if changed {
                        loom.refresh_rows(cx);
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        })
        .detach();

        // Closing the window keeps loom in the tray, so routines and replies go on.
        let hwnd = shell::hwnd(window);
        window.on_window_should_close(cx, move |_, _| {
            shell::show_window(hwnd, false);
            false
        });
        // The core notifies only while no one is looking at the app.
        cx.observe_window_activation(window, |this: &mut Loom, window, _| {
            this.send(serde_json::json!({"t": "app.active", "active": window.is_window_active()}));
        })
        .detach();
        let data = data_dir();
        shell::first_start(&data);
        let tray = shell::Tray::new(shell::starts_at_login());
        if std::env::args().any(|a| a == "--background") {
            // Started at login: straight to the tray.
            cx.defer(move |_| shell::show_window(hwnd, false));
        }

        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        let mut loom = Loom {
            model: Model::new(),
            core,
            events,
            p: Palette::new(dark),
            screen: Screen::Chat,
            menu: None,
            rows: Rc::new(Vec::new()),
            scroller,
            composer,
            server_in,
            user_in,
            pass_in,
            code_in,
            name_in,
            instructions_in,
            search_in,
            program: None,
            open_thoughts: HashSet::new(),
            tray,
            hwnd,
            notice: None,
        };
        loom.fetch();
        loom
    }

    fn send(&self, frame: Value) {
        self.core.send(&frame.to_string());
    }

    /// Runs the instance on this computer, or stops and goes back to signing in to one
    /// elsewhere: keeps the choice, and starts the core again with it.
    fn set_local(&mut self, local: bool, cx: &mut Context<Self>) {
        let file = std::path::Path::new(&data_dir()).join("mode");
        let kept = if local {
            std::fs::write(&file, "local")
        } else {
            std::fs::remove_file(&file).or_else(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(e)
                }
            })
        };
        if let Err(e) = kept {
            self.model.login_error = Some(format!("loom couldn't keep the choice: {e}"));
            cx.notify();
            return;
        }
        self.model = Model::new();
        self.screen = Screen::Chat;
        self.menu = None;
        // The old core stops, and its instance with it, as it's dropped.
        let (core, events) = Core::start(&core_config());
        self.core = core;
        self.events = events;
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    fn fetch(&mut self) {
        for f in self.model.fetches() {
            self.core.send(&f.to_string());
        }
        for f in self.model.file_fetches() {
            self.core.send(&f.to_string());
        }
    }

    /// Shows the newest notice from the model, and hides one that's been up 8 seconds.
    fn take_notice(&mut self, cx: &mut Context<Self>) {
        if let Some(words) = self.model.checked.take() {
            shell::notify("loom", &words);
        }
        if let Some(words) = self.model.notices.pop() {
            self.model.notices.clear();
            self.notice = Some((words, Instant::now()));
            cx.notify();
        } else if self
            .notice
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(8))
        {
            self.notice = None;
            cx.notify();
        }
    }

    /// Opens the files that came to be opened, each in its own program.
    fn open_fetched(&mut self, cx: &mut Context<Self>) {
        for path in std::mem::take(&mut self.model.to_open) {
            cx.open_with_system(std::path::Path::new(&path));
        }
    }

    /// Opens a file from the chat in its own program.
    pub fn open_file(&mut self, f: &model::FileRef, cx: &mut Context<Self>) {
        if let Some(frame) = self.model.open_file(f) {
            self.send(frame);
        }
        self.open_fetched(cx);
    }

    /// Sends files for the next message.
    fn attach(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        for path in paths {
            if path.is_file() {
                let frame = self.model.attach(&path.to_string_lossy());
                self.send(frame);
            }
        }
        cx.notify();
    }

    pub fn unattach(&mut self, id: &str, cx: &mut Context<Self>) {
        self.model.unattach(id);
        cx.notify();
    }

    /// Asks the system for files to attach.
    fn pick_files(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                let _ = this.update(cx, |loom, cx| loom.attach(&paths, cx));
            }
        })
        .detach();
    }

    /// Ctrl+V in the composer: an image, or files copied in Explorer, become attachments.
    /// Gives whether it took the paste, so text still pastes as text.
    fn paste_files(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        for entry in item.entries() {
            match entry {
                ClipboardEntry::ExternalPaths(paths) => {
                    self.attach(paths.paths(), cx);
                    return true;
                }
                ClipboardEntry::Image(image) => {
                    // Most Windows programs copy a bitmap, which agents don't take: it goes
                    // as a PNG.
                    let (ext, bytes) = match image.format {
                        ImageFormat::Jpeg => ("jpg", image.bytes.clone()),
                        ImageFormat::Webp => ("webp", image.bytes.clone()),
                        ImageFormat::Gif => ("gif", image.bytes.clone()),
                        ImageFormat::Png => ("png", image.bytes.clone()),
                        _ => match shell::to_png(&image.bytes) {
                            Some(png) => ("png", png),
                            None => {
                                self.model
                                    .notices
                                    .push("That image's kind can't be sent.".into());
                                return true;
                            }
                        },
                    };
                    let dir = std::path::Path::new(&data_dir()).join("pasted");
                    let at = chrono::Local::now().format("%Y-%m-%d %H.%M.%S");
                    let path = dir.join(format!("Pasted {at}.{ext}"));
                    let written =
                        std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes));
                    match written {
                        Ok(()) => self.attach(&[path], cx),
                        Err(e) => self
                            .model
                            .notices
                            .push(format!("The image couldn't be kept: {e}")),
                    }
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Rebuilds the chat's rows, and tells the list what changed.
    fn refresh_rows(&mut self, cx: &mut Context<Self>) {
        let mut rows: Vec<Row> = self.model.shown().into_iter().map(Row::Message).collect();
        rows.extend(self.model.shown_approvals().into_iter().map(Row::Approval));
        let old = self.rows.clone();
        let same_start = rows.len() >= old.len()
            && old.iter().zip(rows.iter()).all(|(a, b)| match (a, b) {
                (Row::Message(a), Row::Message(b)) => a.id == b.id,
                (Row::Approval(a), Row::Approval(b)) => a.call == b.call,
                _ => false,
            });
        let (old_len, new_len) = (old.len(), rows.len());
        self.rows = Rc::new(rows);
        self.scroller.update(cx, |s, cx| {
            if same_start && new_len >= old_len {
                if new_len > old_len {
                    s.append(new_len - old_len, cx);
                }
                if old_len > 0 {
                    s.remeasure_items(old_len.saturating_sub(2)..old_len, cx);
                }
            } else {
                s.reset(new_len, cx);
                s.scroll_to_end(cx);
            }
        });
    }

    fn select(&mut self, agent: String, cx: &mut Context<Self>) {
        self.screen = Screen::Chat;
        self.menu = None;
        self.model.select(Some(agent));
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    // --- The parts ---

    fn button(&self, id: &'static str, label: &str, primary: bool) -> Stateful<Div> {
        self.button_owned(id, label, primary)
    }

    /// A button whose id is made at run time, such as one per row.
    fn button_owned(
        &self,
        id: impl Into<SharedString>,
        label: &str,
        primary: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id.into())
            .px(px(space::L))
            .h(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .text_size(px(size::SMALL))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .when(primary, |d| d.bg(p.button).text_color(p.button_ink))
            .when(!primary, |d| {
                d.border_1()
                    .border_color(p.line_strong)
                    .text_color(p.ink)
                    .hover(move |s| s.bg(p.hover))
            })
            .child(label.to_string())
    }

    fn field(&self, label: &str, input: impl IntoElement) -> Div {
        let p = self.p;
        div()
            .flex()
            .flex_col()
            .gap(px(space::XS + 2.0))
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .font_weight(FontWeight::MEDIUM)
                    .child(label.to_string()),
            )
            .child(input)
    }

    fn sign_in(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let busy = self.model.logging_in;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.bg)
            .child(
                div()
                    .w(px(360.0))
                    .flex()
                    .flex_col()
                    .gap(px(space::L))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(space::S))
                            .child(thread(
                                Mood::Writing,
                                thread::color("periwinkle", p.dark),
                                120.0,
                                14.0,
                                None,
                            ))
                            .child(
                                div()
                                    .text_size(px(size::HERO))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.ink)
                                    .child("loom"),
                            )
                            .child(
                                div()
                                    .text_size(px(size::BODY))
                                    .text_color(p.ink_3)
                                    .child("Sign in to your instance."),
                            ),
                    )
                    .child(self.field("Instance", Input::new(&self.server_in)))
                    .child(self.field("Username", Input::new(&self.user_in)))
                    .child(self.field("Password", Input::new(&self.pass_in)))
                    .child(self.field(
                        "Setup code (only the first time)",
                        Input::new(&self.code_in),
                    ))
                    .when_some(self.model.login_error.clone(), |d, e| {
                        d.child(div().text_size(px(size::SMALL)).text_color(p.bad).child(e))
                    })
                    .child(
                        self.button(
                            "sign-in",
                            if busy { "Signing in…" } else { "Sign in" },
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.model.logging_in {
                                return;
                            }
                            let server = this.server_in.read(cx).value().to_string();
                            let user = this.user_in.read(cx).value().to_string();
                            let pass = this.pass_in.read(cx).value().to_string();
                            let code = this.code_in.read(cx).value().to_string();
                            let frame = this.model.login(&server, &user, &pass, &code);
                            this.send(frame);
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(space::S))
                            .pt(px(space::M))
                            .border_t_1()
                            .border_color(p.line)
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink_3)
                                    .child("No instance yet? Run one here. Your agents work while this computer is on."),
                            )
                            .child(
                                self.button("local", "Run one on this computer", false)
                                    .on_click(cx.listener(|this, _, _, cx| this.set_local(true, cx))),
                            ),
                    ),
            )
    }

    /// What search found, in place of the agents.
    fn hits(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(2.0)).px(px(space::S));
        if self.model.hits.is_empty() {
            let words = if self.model.query.trim().chars().count() < 2 {
                "Type a little more"
            } else {
                "Nothing found"
            };
            return list.child(
                div()
                    .px(px(space::M))
                    .py(px(space::S))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child(words),
            );
        }
        for (i, h) in self.model.hits.clone().into_iter().enumerate() {
            let Some(a) = self.model.agent(&h.agent).cloned() else {
                continue;
            };
            let id = h.agent.clone();
            let marked = HighlightStyle {
                color: Some(p.ink),
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            };
            let snippet = StyledText::new(h.snippet.clone())
                .with_highlights(h.marks.iter().map(|r| (r.clone(), marked)));
            list = list.child(
                div()
                    .id(SharedString::from(format!("hit-{i}")))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(space::M))
                    .py(px(space::S))
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.search_in
                            .update(cx, |s, cx| s.set_value("", window, cx));
                        this.model.search("");
                        this.select(id.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(thread(
                                Mood::Idle,
                                thread::color(&a.color, p.dark),
                                14.0,
                                8.0,
                                None,
                            ))
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink_2)
                                    .child(if h.person {
                                        format!("You, to {}", a.name)
                                    } else {
                                        a.name.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(time_of(h.at)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_3)
                            .line_clamp(2)
                            .child(snippet),
                    ),
            );
        }
        list
    }

    fn sidebar(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let searching = !self.model.query.trim().is_empty();
        let mut list = div().flex().flex_col().gap(px(2.0)).px(px(space::S));
        for a in self.model.agents.clone() {
            let selected = self.model.selected.as_deref() == Some(a.id.as_str())
                && self.screen == Screen::Chat;
            let mood = self.model.mood(&a.id);
            let preview = self.model.preview(&a);
            let id = a.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("agent-{}", a.id)))
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::M))
                    .py(px(space::S))
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(p.active))
                    .when(!selected, move |d| d.hover(move |s| s.bg(p.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
                    .child(thread(
                        mood,
                        thread::color(&a.color, p.dark),
                        26.0,
                        10.0,
                        self.model.finished_at(&a.id),
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(size::BODY))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink)
                                    .truncate()
                                    .child(a.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(if mood == Mood::Failed { p.bad } else { p.ink_3 })
                                    .truncate()
                                    .child(preview),
                            ),
                    ),
            );
        }
        let status: (String, Hsla) = match &self.model.conn {
            Some(Conn::Live) if self.model.synced => ("Connected".into(), p.ok),
            Some(Conn::Live) | Some(Conn::Connecting) | Some(Conn::Starting) | None => {
                ("Connecting…".into(), p.ink_3)
            }
            Some(Conn::Offline(ms)) => (
                format!("Offline, trying again in {}s", ms.div_ceil(1000)),
                p.bad,
            ),
            Some(Conn::SignedOut) => ("Signed out".into(), p.ink_3),
            Some(Conn::UpdateNeeded) => ("Needs an update".into(), p.bad),
        };
        // A newer loom, downloaded and checked, waits for a restart.
        let update = self.model.update.clone().map(|(version, path)| {
            div()
                .id("update")
                .mx(px(space::S))
                .mb(px(space::S))
                .px(px(space::M))
                .py(px(space::S))
                .rounded(px(radius::ROW))
                .bg(p.hover)
                .cursor_pointer()
                .hover(move |s| s.bg(p.active))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(size::SMALL))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink)
                        .child(format!("loom {version} is ready")),
                )
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child("Restart to update"),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    match shell::apply_update(std::path::Path::new(&path)) {
                        Ok(()) => cx.quit(),
                        Err(e) => {
                            this.model
                                .notices
                                .push(format!("loom couldn't update itself: {e}"));
                            cx.notify();
                        }
                    }
                }))
        });
        div()
            .w(px(layout::SIDEBAR))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(p.side)
            .border_r_1()
            .border_color(p.line)
            .child(
                div()
                    .id("side-top")
                    .window_control_area(WindowControlArea::Drag)
                    .h(px(layout::TITLEBAR))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(space::L))
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .child("loom"),
                    )
                    .child(
                        div()
                            .id("new-agent")
                            .occlude()
                            .size(px(26.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover))
                            .child(Icon::new(IconName::Plus).small())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.screen = Screen::NewAgent;
                                this.menu = None;
                                let f = this.model.programs();
                                this.send(f);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .px(px(space::M))
                    .pb(px(space::S))
                    .text_size(px(size::SMALL))
                    .child(
                        Input::new(&self.search_in)
                            .prefix(Icon::new(IconName::Search).small().text_color(p.ink_3))
                            .cleanable(true),
                    ),
            )
            .child(
                div()
                    .px(px(space::L + space::XS))
                    .pt(px(space::S))
                    .pb(px(space::XS))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(if searching { "Found" } else { "Agents" }),
            )
            .child(
                div()
                    .id("side-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(if searching { self.hits(cx) } else { list }),
            )
            .when_some(update, |d, row| d.child(row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(space::L))
                    .py(px(space::M))
                    .border_t_1()
                    .border_color(p.line)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(div().size(px(6.0)).rounded_full().bg(status.1))
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(status.0),
                            ),
                    )
                    .child(
                        div()
                            .id("sign-out")
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .cursor_pointer()
                            .hover(move |s| s.text_color(p.ink))
                            .child("Sign out")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if local_mode() {
                                    // Stops the instance here; its agents and chats stay for next time.
                                    this.set_local(false, cx);
                                    return;
                                }
                                this.send(serde_json::json!({"t": "logout"}));
                                cx.notify();
                            })),
                    ),
            )
    }

    fn header(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        // The window's own title bar: it drags the window, and the system's buttons sit at its end.
        let bar = div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(space::L));
        let Some(agent) = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .cloned()
        else {
            return bar;
        };
        if self.screen == Screen::NewAgent {
            return bar.child(
                div()
                    .text_size(px(size::BODY))
                    .text_color(p.ink_3)
                    .child("New agent"),
            );
        }
        if self.screen == Screen::EditAgent {
            return bar.child(
                div()
                    .text_size(px(size::BODY))
                    .text_color(p.ink_3)
                    .child(format!("Edit {}", agent.name)),
            );
        }
        let permission = match agent.permission.as_str() {
            "allow" => "Doesn't ask",
            "auto" => "Asks when it's risky",
            _ => "Asks first",
        };
        bar.child(
            div()
                .id("agent-menu")
                .occlude()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.menu = if this.menu == Some(Menu::Agent) {
                        None
                    } else {
                        Some(Menu::Agent)
                    };
                    cx.notify();
                }))
                .child(thread(
                    self.model.mood(&agent.id),
                    thread::color(&agent.color, p.dark),
                    26.0,
                    10.0,
                    self.model.finished_at(&agent.id),
                ))
                .child(
                    div()
                        .text_size(px(size::BODY))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink)
                        .child(agent.name.clone()),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .xsmall()
                        .text_color(p.ink_3),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::L))
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(agent.program.clone()),
                )
                .child(
                    div()
                        .id("permission-menu")
                        .occlude()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::ROW))
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_2)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover))
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .child(permission)
                        .child(
                            Icon::new(IconName::ChevronDown)
                                .xsmall()
                                .text_color(p.ink_3),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.menu = if this.menu == Some(Menu::Permission) {
                                None
                            } else {
                                Some(Menu::Permission)
                            };
                            cx.notify();
                        })),
                ),
        )
    }

    fn menu_item(
        &self,
        id: &'static str,
        title: &str,
        detail: &str,
        danger: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if danger { p.bad } else { p.ink })
                    .child(title.to_string()),
            )
            .when(!detail.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(detail.to_string()),
                )
            })
    }

    fn menu_panel(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.p;
        let menu = self.menu?;
        let panel = div()
            .absolute()
            .top(px(layout::TITLEBAR - 4.0))
            .w(px(300.0))
            .p(px(space::XS))
            .flex()
            .flex_col()
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.menu = None;
                cx.notify();
            }));
        Some(match menu {
            Menu::Agent => panel
                .left(px(space::S))
                .child(
                    self.menu_item(
                        "edit",
                        "Edit agent",
                        "Its name and instructions, and every earlier version.",
                        false,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(f) = this.model.edit_agent() {
                            this.send(f);
                        }
                        if let Some(a) = this.model.editing.as_ref().and_then(|id| this.model.agent(id)) {
                            let (name, instructions) = (a.name.clone(), a.instructions.clone());
                            this.name_in.update(cx, |s, cx| s.set_value(name, window, cx));
                            this.instructions_in
                                .update(cx, |s, cx| s.set_value(instructions, window, cx));
                        }
                        this.screen = Screen::EditAgent;
                        this.menu = None;
                        cx.notify();
                    })),
                )
                .child(
                    self.menu_item(
                        "fresh",
                        "Start fresh",
                        "A new conversation, so it forgets this one's context.",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.model.start_fresh() {
                            this.send(f);
                        }
                        this.menu = None;
                        cx.notify();
                    })),
                )
                .child(
                    self.menu_item("delete", "Delete agent", "", true)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.delete_agent() {
                                this.send(f);
                            }
                            this.menu = None;
                            cx.notify();
                        })),
                ),
            Menu::Permission => panel
                .right(px(space::S))
                .child(
                    self.menu_item(
                        "ask",
                        "Ask first",
                        "Asks before every command and every edit.",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.model.set_permission("ask") {
                            this.send(f);
                        }
                        this.menu = None;
                        cx.notify();
                    })),
                )
                .child(
                    self.menu_item(
                        "auto",
                        "Ask when it's risky",
                        "Reads, searches and looks things up on its own. Asks before commands and edits.",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.model.set_permission("auto") {
                            this.send(f);
                        }
                        this.menu = None;
                        cx.notify();
                    })),
                )
                .child(
                    self.menu_item(
                        "allow",
                        "Don't ask",
                        "Everything runs on its own. Only for agents you trust.",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.model.set_permission("allow") {
                            this.send(f);
                        }
                        this.menu = None;
                        cx.notify();
                    })),
                ),
        })
    }

    fn chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let rows = self.rows.clone();
        let agent = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .cloned();
        let name = agent.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let tint = agent
            .as_ref()
            .map(|a| thread::color(&a.color, p.dark))
            .unwrap_or(p.ink_3);
        let mood = agent
            .as_ref()
            .map(|a| self.model.mood(&a.id))
            .unwrap_or(Mood::Idle);
        let open = self.open_thoughts.clone();
        let tools: Vec<model::Tool> = self.model.tools.clone();
        let files = Rc::new(self.model.files.clone());
        let this = cx.entity().downgrade();
        let list = MessageScroller::new("chat", self.scroller.clone(), move |ix, _, _| {
            let Some(row) = rows.get(ix) else {
                return div().into_any_element();
            };
            let inner = match row {
                Row::Message(m) if m.role == "person" => person(p, m, &files, this.clone()),
                Row::Message(m) => reply(
                    p,
                    m,
                    &name,
                    tint,
                    if m.live { mood } else { Mood::Idle },
                    &open,
                    &tools,
                    &files,
                    this.clone(),
                ),
                Row::Approval(a) => approval(p, a, this.clone()),
            };
            div()
                .w_full()
                .flex()
                .justify_center()
                .px(px(space::XL))
                .child(
                    div()
                        .w_full()
                        .max_w(px(space::COLUMN))
                        .py(px(space::S))
                        .child(inner),
                )
                .into_any_element()
        })
        .with_bottom_fade(p.bg);

        let empty = self.rows.is_empty();
        let busy = self.model.busy();
        let placeholder = format!(
            "Reply to {}",
            agent
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or("the agent")
        );
        self.composer
            .update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
        let mut attached = div().flex().flex_wrap().gap(px(space::S));
        for a in self.model.attachments.clone() {
            let id = a.id.clone();
            let (note, color) = match (&a.file, &a.error) {
                (_, Some(e)) => (e.clone(), p.bad),
                (Some(f), None) => (model::size_of(f.size), p.ink_3),
                (None, None) => ("Sending…".to_string(), p.ink_3),
            };
            attached = attached.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pl(px(space::M))
                    .pr(px(space::XS))
                    .py(px(space::XS))
                    .rounded(px(radius::ROW))
                    .bg(p.bubble)
                    .text_size(px(size::SMALL))
                    .child(
                        Icon::new(gpui_kit::assets::IconName::FileText)
                            .small()
                            .text_color(p.ink_3),
                    )
                    .child(
                        div()
                            .max_w(px(220.0))
                            .truncate()
                            .text_color(p.ink)
                            .child(a.name.clone()),
                    )
                    .child(div().text_color(color).truncate().child(note))
                    .child(
                        div()
                            .id(SharedString::from(format!("unattach-{}", a.id)))
                            .size(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover).text_color(p.ink))
                            .child(Icon::new(IconName::Close).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| this.unattach(&id, cx))),
                    ),
            );
        }
        let has_attachments = !self.model.attachments.is_empty();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.attach(paths.paths(), cx);
            }))
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(p.hover))
            .child(if empty {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(size::BODY))
                    .text_color(p.ink_3)
                    .child(if agent.is_some() {
                        "Nothing here yet. Say hello."
                    } else {
                        "Make an agent with + to start."
                    })
                    .into_any_element()
            } else {
                div().flex_1().min_h_0().child(list).into_any_element()
            })
            .when_some(self.notice.clone(), |d, (words, _)| {
                d.child(
                    div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .px(px(space::XL))
                        .child(
                            div()
                                .id("notice")
                                .w_full()
                                .max_w(px(space::COLUMN))
                                .px(px(space::L))
                                .text_size(px(size::SMALL))
                                .text_color(p.bad)
                                .cursor_pointer()
                                .child(words)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notice = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .px(px(space::XL))
                    .pb(px(space::L))
                    .pt(px(space::XS))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(space::COLUMN))
                            .flex()
                            .flex_col()
                            .gap(px(space::S))
                            .px(px(space::M))
                            .py(px(space::S))
                            .bg(p.raised)
                            .border_1()
                            .border_color(p.line)
                            .rounded(px(radius::COMPOSER))
                            .when(has_attachments, |d| d.child(attached))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_end()
                                    .gap(px(space::S))
                                    .child(
                                        div()
                                            .id("attach")
                                            .size(px(32.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .text_color(p.ink_2)
                                            .cursor_pointer()
                                            .hover(move |s| s.bg(p.hover))
                                            .child(
                                                Icon::new(gpui_kit::assets::IconName::Paperclip)
                                                    .small(),
                                            )
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.pick_files(cx)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(size::COMPOSER))
                                            // Before the textarea's own Paste, which would
                                            // paste a copied file as its path.
                                            .capture_action(cx.listener(
                                                |this, _: &Paste, _, cx| {
                                                    if this.paste_files(cx) {
                                                        cx.stop_propagation();
                                                    }
                                                },
                                            ))
                                            .child(Textarea::new(&self.composer).appearance(false)),
                                    )
                                    .child(
                                        div()
                                            .id("send")
                                            .size(px(32.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .rounded_full()
                                            .bg(p.button)
                                            .text_color(p.button_ink)
                                            .cursor_pointer()
                                            .child(
                                                Icon::new(if busy {
                                                    IconName::Square
                                                } else {
                                                    IconName::ArrowUp
                                                })
                                                .small(),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                if busy {
                                                    if let Some(f) = this.model.cancel() {
                                                        this.send(f);
                                                    }
                                                    return;
                                                }
                                                let text =
                                                    this.composer.read(cx).value().to_string();
                                                if let Some(frame) = this.model.send(&text) {
                                                    this.send(frame);
                                                    this.composer.update(cx, |s, cx| {
                                                        s.set_value("", window, cx)
                                                    });
                                                }
                                            })),
                                    ),
                            ),
                    ),
            )
    }

    fn new_agent(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut programs = div().flex().flex_wrap().gap(px(space::S));
        if self.model.programs.is_empty() {
            programs = programs.child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("No agent programs are installed on the instance yet. Add one there with `loom acp add`."),
            );
        }
        for name in self.model.programs.clone() {
            let picked = self.program.as_deref() == Some(name.as_str());
            let n = name.clone();
            programs = programs.child(
                div()
                    .id(SharedString::from(format!("program-{name}")))
                    .px(px(space::M))
                    .py(px(space::XS + 2.0))
                    .rounded(px(radius::CHIP))
                    .border_1()
                    .text_size(px(size::SMALL))
                    .cursor_pointer()
                    .when(picked, |d| {
                        d.bg(p.button)
                            .text_color(p.button_ink)
                            .border_color(p.button)
                    })
                    .when(!picked, |d| {
                        d.border_color(p.line_strong)
                            .text_color(p.ink)
                            .hover(move |s| s.bg(p.hover))
                    })
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.program = Some(n.clone());
                        cx.notify();
                    })),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .justify_center()
            .pt(px(space::XXL))
            .child(
                div()
                    .w(px(520.0))
                    .flex()
                    .flex_col()
                    .gap(px(space::XL))
                    .child(
                        div()
                            .text_size(px(size::HEADING))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .child("New agent"),
                    )
                    .child(self.field("Name", Input::new(&self.name_in)))
                    .child(self.field("Runs on", programs))
                    .child(self.field("Instructions", Textarea::new(&self.instructions_in)))
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(
                                self.button("create", "Make agent", true)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let name = this.name_in.read(cx).value().to_string();
                                        let instructions =
                                            this.instructions_in.read(cx).value().to_string();
                                        let Some(program) = this.program.clone() else {
                                            return;
                                        };
                                        if name.trim().is_empty() {
                                            return;
                                        }
                                        let f =
                                            this.model.new_agent(&name, &program, &instructions);
                                        this.send(f);
                                        this.name_in
                                            .update(cx, |s, cx| s.set_value("", window, cx));
                                        this.instructions_in
                                            .update(cx, |s, cx| s.set_value("", window, cx));
                                        this.screen = Screen::Chat;
                                        cx.notify();
                                    })),
                            )
                            .child(self.button("cancel-new", "Cancel", false).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.screen = Screen::Chat;
                                    cx.notify();
                                }),
                            )),
                    ),
            )
    }

    /// The edit page: an agent's name and instructions, and its earlier versions to go
    /// back to. Each save is a new revision, so nothing is lost.
    fn edit_agent(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col();
        let newest = self.model.revisions.first().map_or(0, |r| r.revision);
        for r in self.model.revisions.clone() {
            let who = self.model.made_by(&r.by);
            let first_line = r
                .instructions
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("No instructions")
                .to_string();
            let mut row =
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .py(px(space::S))
                    .border_b_1()
                    .border_color(p.line)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(size::SMALL)).text_color(p.ink).child(
                                format!("Revision {} · {who} · {}", r.revision, time_of(r.at)),
                            ))
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .truncate()
                                    .child(first_line),
                            ),
                    );
            if r.revision == newest {
                row = row.child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child("Now"),
                );
            } else {
                let revision = r.revision;
                row = row.child(
                    self.button_owned(format!("restore-{revision}"), "Go back to this", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(f) = this.model.roll_back(revision) {
                                this.send(f);
                            }
                            this.screen = Screen::Chat;
                            this.model.editing = None;
                            cx.notify();
                        })),
                );
            }
            list = list.child(row);
        }
        let page = div()
            .id("edit-agent")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .pt(px(space::XXL))
            .pb(px(space::XXL))
            .child(
                div()
                    .w(px(520.0))
                    .flex()
                    .flex_col()
                    .gap(px(space::XL))
                    .child(self.field("Name", Input::new(&self.name_in)))
                    .child(self.field("Instructions", Textarea::new(&self.instructions_in)))
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(
                                self.button("save-agent", "Save", true)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let name = this.name_in.read(cx).value().to_string();
                                        let instructions =
                                            this.instructions_in.read(cx).value().to_string();
                                        if name.trim().is_empty() {
                                            return;
                                        }
                                        if let Some(f) = this.model.save_agent(&name, &instructions)
                                        {
                                            this.send(f);
                                        }
                                        this.screen = Screen::Chat;
                                        this.model.editing = None;
                                        cx.notify();
                                    })),
                            )
                            .child(self.button("cancel-edit", "Cancel", false).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.screen = Screen::Chat;
                                    this.model.editing = None;
                                    cx.notify();
                                }),
                            )),
                    )
                    .when(!self.model.revisions.is_empty(), |d| {
                        d.child(self.field("Versions", list))
                    }),
            );
        div().flex_1().min_h_0().child(page)
    }

    pub fn toggle_thought(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.open_thoughts.remove(&key) {
            self.open_thoughts.insert(key);
        }
        self.refresh_rows(cx);
        self.scroller.update(cx, |s, cx| s.remeasure(cx));
        cx.notify();
    }

    fn tray_actions(&mut self, cx: &mut Context<Self>) {
        let Some(tray) = &self.tray else { return };
        for a in tray.actions() {
            match a {
                shell::TrayAction::Open => shell::show_window(self.hwnd, true),
                shell::TrayAction::StartAtLogin(on) => shell::set_start_at_login(on),
                shell::TrayAction::CheckUpdates => {
                    self.send(serde_json::json!({"t": "update.check", "id": "app.update"}));
                }
                shell::TrayAction::Quit => {
                    self.send(serde_json::json!({"t": "app.active", "active": false}));
                    cx.quit();
                }
            }
        }
    }

    pub fn answer(&mut self, call: String, allow: bool, always: bool) {
        self.send(model::approve(&call, allow, always));
    }
}

fn person(p: Palette, m: &Message, files: &HashMap<String, String>, this: WeakEntity<Loom>) -> Div {
    let text: String = m
        .blocks
        .iter()
        .filter(|b| b.kind == "text")
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    // What a person typed keeps its line breaks, which Markdown would join.
    let text = text.replace('\n', "  \n");
    let mut out = div()
        .w_full()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(space::XS));
    // Another agent wrote this one, handing work over.
    if let Some(from) = &m.from_agent {
        out = out.child(
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child(format!("From {from}")),
        );
    }
    for (i, b) in m.blocks.iter().enumerate() {
        if let Some(f) = &b.file {
            out = out.child(file_view(
                p,
                f,
                files,
                format!("{}/{i}", m.id),
                this.clone(),
            ));
        }
    }
    out.when(!text.trim().is_empty(), |d| {
        d.child(
            div()
                .max_w(px(520.0))
                .px(px(space::L))
                .py(px(space::S + 2.0))
                .rounded(px(radius::BUBBLE))
                .bg(p.bubble)
                .text_size(px(size::BODY))
                .text_color(p.ink)
                .when(m.queued, |d| d.opacity(0.6))
                .child(
                    TextView::markdown(SharedString::from(format!("m-{}", m.id)), text)
                        .selectable(true),
                ),
        )
    })
    .when(m.queued, |d| {
        d.child(
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Waiting to send"),
        )
    })
}

/// A file in a message: a picture for an image, its name and size for anything else.
/// Clicking it opens it in its own program.
fn file_view(
    p: Palette,
    f: &model::FileRef,
    files: &HashMap<String, String>,
    key: String,
    this: WeakEntity<Loom>,
) -> AnyElement {
    let file = f.clone();
    let open = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        let _ = this.update(cx, |loom, cx| loom.open_file(&file, cx));
    };
    let id = SharedString::from(format!("file-{key}"));
    if f.image() {
        let picture = match files.get(&format!("{}.thumb", f.file)) {
            Some(path) => img(std::path::PathBuf::from(path))
                .max_w(px(320.0))
                .max_h(px(240.0))
                .rounded(px(radius::BUBBLE))
                .into_any_element(),
            // Until the thumbnail is here, a place of about its size.
            None => div()
                .w(px(240.0))
                .h(px(160.0))
                .rounded(px(radius::BUBBLE))
                .bg(p.bubble)
                .into_any_element(),
        };
        return div()
            .id(id)
            .cursor_pointer()
            .on_click(open)
            .child(picture)
            .into_any_element();
    }
    div()
        .id(id)
        .max_w(px(320.0))
        .flex()
        .items_center()
        .gap(px(space::M))
        .px(px(space::M))
        .py(px(space::S))
        .rounded(px(radius::ROW))
        .border_1()
        .border_color(p.line)
        .cursor_pointer()
        .hover(move |s| s.bg(p.hover))
        .on_click(open)
        .child(
            Icon::new(gpui_kit::assets::IconName::FileText)
                .small()
                .text_color(p.ink_3),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink)
                        .truncate()
                        .child(f.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(model::size_of(f.size)),
                ),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn reply(
    p: Palette,
    m: &Message,
    name: &str,
    tint: Hsla,
    mood: Mood,
    open: &HashSet<String>,
    tools: &[model::Tool],
    files: &HashMap<String, String>,
    this: WeakEntity<Loom>,
) -> Div {
    let when = if m.live {
        "now".to_string()
    } else {
        time_of(m.at)
    };
    let mut body = div().flex().flex_col().gap(px(space::S)).flex_1().min_w_0();
    body = body.child(
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .text_size(px(size::SMALL))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink)
                    .child(name.to_string()),
            )
            .child(
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(when),
            ),
    );
    for (i, b) in m.blocks.iter().enumerate() {
        let key = format!("{}/{}", m.id, i);
        match b.kind.as_str() {
            "text" => {
                body = body.child(
                    div()
                        .text_size(px(size::REPLY))
                        .line_height(relative(line::REPLY))
                        .text_color(p.ink)
                        .child(
                            TextView::markdown(
                                SharedString::from(format!("r-{key}")),
                                b.text.clone(),
                            )
                            .selectable(true),
                        ),
                );
            }
            "thought" => {
                let shown = open.contains(&key);
                let this = this.clone();
                let k = key.clone();
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .child(
                            div()
                                .id(SharedString::from(format!("t-{key}")))
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .cursor_pointer()
                                .child(if shown { "⌄ Thought" } else { "› Thought" })
                                .on_click(move |_, _, cx| {
                                    let _ = this
                                        .update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
                                }),
                        )
                        .when(shown, |d| {
                            d.child(
                                div()
                                    .pl(px(space::M))
                                    .border_l_1()
                                    .border_color(p.line_strong)
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink_3)
                                    .child(b.text.clone()),
                            )
                        }),
                );
            }
            "tool" => {
                let t = b
                    .call
                    .as_ref()
                    .and_then(|c| tools.iter().find(|t| &t.call == c));
                let (title, mark, color) = match t {
                    Some(t) => {
                        let (mark, color) = match t.status.as_deref() {
                            Some("done") => ("✓", p.ok),
                            Some("failed") => ("✕", p.bad),
                            Some("denied") => ("denied", p.bad),
                            _ => ("…", p.ink_3),
                        };
                        (t.title.clone(), mark, color)
                    }
                    None => ("Used a tool".to_string(), "", p.ink_3),
                };
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .text_size(px(size::SMALL))
                        .child(div().text_color(p.ink_3).child("›_"))
                        .child(
                            div()
                                .font_family(font::MONO)
                                .text_color(p.ink_2)
                                .truncate()
                                .child(title),
                        )
                        .child(div().text_color(color).child(mark)),
                );
            }
            "plan" => {
                body = body.child(
                    div()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_2)
                        .child(b.text.clone()),
                );
            }
            "file" => {
                if let Some(f) = &b.file {
                    body = body.child(file_view(p, f, files, key, this.clone()));
                }
            }
            _ => {}
        }
    }
    if m.live && m.blocks.is_empty() {
        body = body.child(
            div()
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child("Thinking…"),
        );
    }
    div()
        .w_full()
        .flex()
        .gap(px(space::M))
        .child(
            div()
                .pt(px(4.0))
                .child(thread(mood, tint, 14.0, 10.0, None)),
        )
        .child(body)
}

fn approval(p: Palette, a: &Approval, this: WeakEntity<Loom>) -> Div {
    let call = a.call.clone();
    let detail = command_of(&a.detail);
    let answer = move |allow: bool, always: bool| {
        let this = this.clone();
        let call = call.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |loom, _| loom.answer(call.clone(), allow, always));
        }
    };
    let btn = |id: String, label: &'static str, primary: bool| {
        div()
            .id(SharedString::from(id))
            .px(px(space::M))
            .h(px(30.0))
            .flex()
            .items_center()
            .rounded(px(radius::BUTTON))
            .text_size(px(size::SMALL))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .when(primary, |d| d.bg(p.button).text_color(p.button_ink))
            .when(!primary, |d| {
                d.border_1()
                    .border_color(p.line_strong)
                    .text_color(p.ink)
                    .hover(move |s| s.bg(p.hover))
            })
            .child(label)
    };
    div()
        .ml(px(26.0))
        .p(px(space::L))
        .flex()
        .flex_col()
        .gap(px(space::M))
        .bg(p.raised)
        .border_1()
        .border_color(p.line)
        .rounded(px(radius::CARD))
        .child(
            div()
                .text_size(px(size::BODY))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink)
                .child(if a.title.is_empty() {
                    "Can it go ahead?".to_string()
                } else {
                    format!("{}?", a.title)
                }),
        )
        .when(!detail.is_empty(), |d| {
            d.child(
                div()
                    .px(px(space::M))
                    .py(px(space::S))
                    .bg(p.bg)
                    .border_1()
                    .border_color(p.line)
                    .rounded(px(radius::INPUT))
                    .font_family(font::MONO)
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .child(detail),
            )
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(
                    btn(format!("allow-{}", a.call), "Allow once", true)
                        .on_click(answer(true, false)),
                )
                .when(a.can_always, |d| {
                    d.child(
                        btn(format!("always-{}", a.call), "Always allow", false)
                            .on_click(answer(true, true)),
                    )
                })
                .child(
                    btn(format!("deny-{}", a.call), "Deny", false).on_click(answer(false, false)),
                ),
        )
}

/// What an approval would run, from the tool's input: its command if it has one.
fn command_of(detail: &str) -> String {
    match serde_json::from_str::<Value>(detail) {
        Ok(v) => match v.get("command") {
            Some(Value::String(c)) => c.clone(),
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
            _ => match v {
                Value::Null => String::new(),
                Value::String(s) => s,
                other => serde_json::to_string_pretty(&other).unwrap_or_default(),
            },
        },
        Err(_) => detail.to_string(),
    }
}

fn time_of(ms: i64) -> String {
    use chrono::TimeZone;
    if ms <= 0 || ms == i64::MAX {
        return String::new();
    }
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => t.format("%H:%M").to_string(),
        _ => String::new(),
    }
}

impl Render for Loom {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        if self
            .model
            .agents
            .iter()
            .any(|a| thread::moves(self.model.mood(&a.id), self.model.finished_at(&a.id)))
            || self.model.conn == Some(Conn::SignedOut)
        {
            window.request_animation_frame();
        }
        let root = div()
            .size_full()
            .bg(p.bg)
            .text_color(p.ink)
            .font_family(font::UI)
            .text_size(px(size::BODY));
        if self.model.conn == Some(Conn::SignedOut) {
            return root.child(self.sign_in(cx));
        }
        let main = match self.screen {
            Screen::Chat => self.chat(window, cx),
            Screen::NewAgent => self.new_agent(cx),
            Screen::EditAgent => self.edit_agent(cx),
        };
        let header = self.header(cx);
        let menu = self.menu_panel(cx);
        root.flex().child(self.sidebar(cx)).child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .relative()
                .child(
                    TitleBar::new()
                        .h(px(layout::TITLEBAR))
                        .pl(px(0.0))
                        .bg(p.bg)
                        .border_color(p.line)
                        .child(header),
                )
                .child(main)
                .when_some(menu, |d, m| d.child(m)),
        )
    }
}

/// Whether this computer runs its own instance, as the person chose on the first start.
fn local_mode() -> bool {
    std::fs::read_to_string(std::path::Path::new(&data_dir()).join("mode"))
        .is_ok_and(|m| m.trim() == "local")
}

/// The core's config (core/README.md, "Config").
fn core_config() -> String {
    let mut config = serde_json::json!({"data": data_dir()});
    // Releases update themselves. A build from source doesn't, unless LOOM_UPDATE names a
    // manifest to try.
    match std::env::var("LOOM_UPDATE") {
        Ok(url) => {
            config["version"] = crate::VERSION.into();
            config["update"] = url.into();
        }
        Err(_) if !crate::VERSION.contains("-dev") => config["version"] = crate::VERSION.into(),
        Err(_) => {}
    }
    if local_mode() {
        config["mode"] = "local".into();
        config["username"] = std::env::var("USERNAME")
            .unwrap_or_else(|_| "me".into())
            .into();
        config["kind"] = if cfg!(target_os = "macos") {
            "macos"
        } else {
            "windows"
        }
        .into();
    } else if let Ok(server) = std::env::var("LOOM_SERVER") {
        config["server"] = Value::String(server);
    }
    config.to_string()
}
