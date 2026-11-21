//! The window: the sidebar of agents, and the chat with the one picked. The look
//! follows design/explorations/round-4.html, with every value from design/tokens.toml.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName as Ico;
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
use crate::thread::{self, spool, thread, Spool};
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
    Server,
    NewAgent,
    EditAgent,
    Activity,
    Skills,
    Memory,
    Profile,
    Routines,
    Tasks,
    Programs,
    ThisDevice,
    AddPhone,
    Mcp,
    Plugins,
    Move,
    People,
    AddServer,
    Pins,
    Aside,
    Page,
    Tables,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Agent,
    Model,
    Permission,
}

/// The composer's + menu, open at its first list or at one it leads to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Plus {
    Main,
    Skills,
    Routines,
}

/// One row of the chat's list.
#[derive(Clone)]
enum Row {
    Message(Message),
    Approval(Approval),
}

pub struct Loom {
    /// The server on screen: what it shows, its core, and the core's events.
    model: Model,
    core: Core,
    events: std::sync::mpsc::Receiver<String>,
    /// Its folder ("" for the first server).
    here: String,
    /// The other servers the app is signed in to, each with its own core.
    away: Vec<Away>,
    /// An address for the server form, put in once there's a window to do it with.
    fill_server: Option<String>,
    /// The server being added in the popup, by folder, until it signs in.
    adding: Option<String>,
    /// Answers to the core's device asks that finish on another thread, such as a
    /// screenshot, sent on from the window's task.
    device_tx: std::sync::mpsc::Sender<Value>,
    device_rx: std::sync::mpsc::Receiver<Value>,
    p: Palette,
    screen: Screen,
    menu: Option<Menu>,
    plus: Option<Plus>,
    /// The settings or server popup over the window, open at this page.
    popup: Option<Screen>,
    /// The panel beside the chat on the right: an agent's page, or a plugin's panel.
    side: Option<Screen>,
    /// The settings page the popup opened at last, to open at again.
    settings_last: Screen,
    rows: Rc<Vec<Row>>,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    server_in: Entity<InputState>,
    user_in: Entity<InputState>,
    pass_in: Entity<InputState>,
    secret_in: Entity<InputState>,
    queue_in: Entity<InputState>,
    /// A side question being written.
    aside_in: Entity<InputState>,
    /// The queued message being edited, by id.
    queue_editing: Option<String>,
    code_in: Entity<InputState>,
    name_in: Entity<InputState>,
    skill_name_in: Entity<InputState>,
    skill_desc_in: Entity<InputState>,
    skill_body_in: Entity<TextareaState>,
    /// On the skills page: none for the list, `Some(None)` for a new skill, `Some(Some(name))`
    /// for one being changed.
    skill_open: Option<Option<String>>,
    /// The table the tables page is asking whether to drop.
    table_dropping: Option<String>,
    /// The open plugin panel: its plugin, the web view, and where it is (x, y, w, h).
    panel: Option<(String, crate::panel::Panel, [f32; 4])>,
    panel_context: Option<wry::WebContext>,
    /// The plugin whose panel is being made, while it is.
    panel_opening: Option<String>,
    /// What came through the open panel's door, waiting for the window's task.
    door_in: Rc<std::cell::RefCell<Vec<crate::panel::Message>>>,
    note_in: Entity<InputState>,
    profile_in: Entity<TextareaState>,
    routine_name_in: Entity<InputState>,
    move_to_in: Entity<InputState>,
    relay_in: Entity<InputState>,
    move_code_in: Entity<InputState>,
    /// Whether the People page's next invite makes an admin rather than a member.
    invite_admin: bool,
    /// Whether the Add a phone page is asking "delete my account?" before it does.
    confirm_delete: bool,
    routine_at_in: Entity<InputState>,
    routine_prompt_in: Entity<TextareaState>,
    /// On the routines page: none for the list, `Some(None)` for a new routine,
    /// `Some(Some(id))` for one being changed. The form's agent and switches go with it.
    routine_open: Option<Option<String>>,
    routine_agent: Option<String>,
    /// The new task form: what it is, and the agent it goes to.
    task_prompt_in: Entity<TextareaState>,
    task_agent: Option<String>,
    // Whether the Tasks page shows a new task being written.
    task_new: bool,
    routine_notify: bool,
    routine_on: bool,
    routine_catch_up: bool,
    /// Whether the routine in the form posts into a thread of its own, not its agent's chat.
    routine_thread: bool,
    routine_webhook: bool,
    /// The flow the form edits, one step after another: each step's agent and prompt.
    routine_flow: Vec<(Option<String>, Entity<TextareaState>)>,
    /// A flow an agent made with steps side by side, which the form shows but doesn't
    /// edit: kept as it is unless the person takes it off.
    routine_flow_kept: Vec<model::FlowStep>,
    /// The schedule the form shows: "every", "daily" or "weekdays".
    routine_kind: String,
    /// The MCP server form: none, a new server, or the one with this name.
    mcp_open: Option<Option<String>>,
    mcp_name_in: Entity<InputState>,
    mcp_cmd_in: Entity<InputState>,
    mcp_settings_in: Entity<TextareaState>,
    /// The program whose settings form is open, on the programs page.
    program_open: Option<String>,
    set_key_in: Entity<InputState>,
    set_value_in: Entity<InputState>,
    /// The open program's share: tokens each person may use, and whether each month.
    share_in: Entity<InputState>,
    share_month: bool,
    instructions_in: Entity<TextareaState>,
    search_in: Entity<InputState>,
    open_thoughts: HashSet<String>,
    tray: Option<shell::Tray>,
    hwnd: isize,
    /// On the welcome screen: the server sign-in instead of Start, and with it the setup
    /// code, for a server that has no owner yet.
    server_form: bool,
    show_code: bool,
    /// Whether the agent programs were asked for, for the first agent's picker.
    asked_programs: bool,
    make: Making,
    /// When the window opened, for the opening weave.
    launched: Option<Instant>,
    /// The window's own focus, so keys like Esc reach it when no field has focus.
    keys: FocusHandle,
    /// The watched browser's page, decoded, and the frame it's from.
    page_image: Option<(u64, Arc<RenderImage>)>,
    /// Where the page is drawn, for turning a click on it into the page's own pixels.
    page_bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The browser panel's focus, so keys go to the page while the person has it.
    browser_keys: FocusHandle,
    /// The page beside the sidebar (its screen and agent), and when it last changed.
    page: ((Screen, Option<String>), Instant),
    /// What the window shows, and since when, so each new view eases in.
    scene: (&'static str, Instant),
    /// The window closing on a new agent's thread: since when, its colour, and whether it
    /// was the first agent, made full-window.
    closing: Option<(Instant, Hsla, bool)>,
    /// When the welcome last swapped between its buttons and the server form.
    form_at: Instant,
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

/// A server the app is signed in to besides the one on screen: its own core, with a
/// data folder of its own, and what it shows, kept up to date while it's away.
struct Away {
    /// Its folder under the app's: "" for the first server, `servers/2` for another.
    dir: String,
    core: Core,
    events: std::sync::mpsc::Receiver<String>,
    model: Model,
}

impl Away {
    fn start(dir: String, server: Option<&str>) -> Away {
        let (core, events) = Core::start(&core_config(&dir, server));
        Away {
            dir,
            core,
            events,
            model: Model::new(),
        }
    }
}

/// The servers besides the first, by folder, as the app keeps them in `server-list`.
fn server_dirs() -> Vec<String> {
    let file = std::path::Path::new(&data_dir()).join("server-list");
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| is_server_dir(l))
        .map(String::from)
        .collect()
}

/// Whether `dir` is a folder the app made for a server: `servers/` and a number.
fn is_server_dir(dir: &str) -> bool {
    dir.strip_prefix("servers/")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn keep_server_dirs(dirs: &[String]) -> std::io::Result<()> {
    let file = std::path::Path::new(&data_dir()).join("server-list");
    std::fs::write(file, dirs.join("\n"))
}

/// A server's data folder: the app's own for the first, and one under it for another.
fn server_folder(dir: &str) -> std::path::PathBuf {
    let base = std::path::PathBuf::from(data_dir());
    if dir.is_empty() {
        base
    } else {
        base.join(dir)
    }
}

/// How a server's spool is: wound while it's live, turning while it connects, loose
/// while it can't, and empty signed out.
fn spool_of(model: &Model) -> Spool {
    match &model.conn {
        Some(Conn::Live) if model.synced => Spool::Wound,
        Some(Conn::Offline(_)) | Some(Conn::UpdateNeeded) => Spool::Loose,
        Some(Conn::SignedOut) => Spool::Empty,
        _ => Spool::Turning,
    }
}

/// A server's name as the sidebar shows it: its host, or where it is.
fn server_name(model: &Model, dir: &str) -> String {
    if dir.is_empty() && local_mode() {
        return "This computer".into();
    }
    match &model.server {
        Some(address) => address
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string(),
        None => "New server".into(),
    }
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
        let (core, events) = Core::start(&core_config("", None));
        let (device_tx, device_rx) = std::sync::mpsc::channel();

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
                match event {
                    InputEvent::PressEnter { shift: false, .. } => {
                        let text = state.read(cx).value().to_string();
                        if let Some(frame) = this.model.send(&text) {
                            this.core.send(&frame.to_string());
                            state.update(cx, |s, cx| s.set_value("", window, cx));
                        }
                    }
                    // The agent's thread thinks along while there's something typed.
                    InputEvent::Change => cx.notify(),
                    _ => {}
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
        let server_in = input(window, cx, "Server address", false);
        let user_in = input(window, cx, "Username", false);
        let pass_in = input(window, cx, "Password", true);
        let code_in = input(window, cx, "Setup code", false);
        let name_in = input(window, cx, "Name", false);
        // Naming a new agent: each change twangs its thread, and Enter weaves it.
        cx.subscribe_in(
            &name_in,
            window,
            |this: &mut Loom, _, event: &InputEvent, _, cx| {
                if this.make.step != Step::Name {
                    return;
                }
                match event {
                    InputEvent::Change => {
                        this.make.snapped = Some(Instant::now());
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.weave_agent(cx),
                    _ => {}
                }
            },
        )
        .detach();
        // What an agent asked for with browser_ask_secret: Enter sends it to the page.
        let secret_in = input(window, cx, "Type it here", true);
        cx.subscribe_in(
            &secret_in,
            window,
            |this: &mut Loom, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.give_secret(true, window, cx);
                }
            },
        )
        .detach();
        // A queued message being edited: Enter saves it.
        let queue_in = input(window, cx, "", false);
        cx.subscribe_in(
            &queue_in,
            window,
            |this: &mut Loom, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.save_queued(window, cx);
                }
            },
        )
        .detach();
        // A side question: Enter asks it.
        let aside_in = input(window, cx, "Ask about this chat", false);
        cx.subscribe_in(
            &aside_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let question = state.read(cx).value().to_string();
                    if let Some(f) = this.model.ask_aside(&question) {
                        this.send(f);
                        state.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                    cx.notify();
                }
            },
        )
        .detach();
        for field in [&pass_in, &code_in] {
            cx.subscribe_in(
                field,
                window,
                |this: &mut Loom, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        // In the popup, the same fields add a server beside this one.
                        if this.popup == Some(Screen::AddServer) {
                            this.sign_in_added(cx);
                        } else {
                            this.sign_in_server(cx);
                        }
                    }
                },
            )
            .detach();
        }
        let skill_name_in = input(window, cx, "release-notes", false);
        let note_in = input(window, cx, "Something to remember", false);
        let profile_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 20)
                .placeholder("Who you are, and how you like answers")
        });
        let routine_name_in = input(window, cx, "Morning brief", false);
        let move_to_in = input(window, cx, "https://loom.example.com", false);
        let relay_in = input(window, cx, "https://push.loom.example", false);
        let move_code_in = input(window, cx, "loom-move-…", false);
        let routine_at_in = input(window, cx, "30", false);
        let task_prompt_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 14)
                .placeholder("Everything it needs: what to do, and what done looks like.")
        });
        let routine_prompt_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 14)
                .placeholder("What the agent is asked each time.")
        });
        let set_key_in = input(window, cx, "ANTHROPIC_API_KEY", false);
        let mcp_name_in = input(window, cx, "acta", false);
        let mcp_cmd_in = input(
            window,
            cx,
            "npx -y acta-mcp, or https://mcp.example.com/mcp",
            false,
        );
        let mcp_settings_in = cx.new(|cx| {
            TextareaState::new(window, cx).auto_grow(3, 12).placeholder(
                "ACTA_URL=https://acta.example.com
ACTA_KEY=secret:ACTA_KEY",
            )
        });
        let set_value_in = input(window, cx, "Its value", true);
        let share_in = input(window, cx, "Tokens for each person", false);
        let skill_desc_in = input(window, cx, "When it's for, in one sentence", false);
        let skill_body_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(8, 24)
                .placeholder("How to do it, in Markdown.")
        });
        let instructions_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 14)
                .placeholder("How should it work?")
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
                        for a in &loom.away {
                            a.core.send(r#"{"t":"app.wake"}"#);
                        }
                    }
                    loom.tray_actions(cx);
                    loom.take_notice(cx);
                    if let Some(csv) = loom.model.exported.take() {
                        loom.save_export(csv, cx);
                    }
                    while let Ok(answer) = loom.device_rx.try_recv() {
                        loom.send(answer);
                    }
                    loom.pump_door();
                    if loom.pump_away() {
                        cx.notify();
                    }
                    loom.watch_adding(cx);
                    // A server an agent offered, and the person said yes to here.
                    let mut offered = std::mem::take(&mut loom.model.offered);
                    for a in &mut loom.away {
                        offered.append(&mut a.model.offered);
                    }
                    for address in offered {
                        loom.add_server(Some(address), cx);
                    }
                    if got.is_empty() {
                        return;
                    }
                    let mut changed = false;
                    for e in got {
                        if let Ok(v) = serde_json::from_str::<Value>(&e) {
                            if v.get("t").and_then(Value::as_str) == Some("core.device_ask") {
                                loom.device_ask(&v, cx);
                                continue;
                            }
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
        // The servers away aren't on screen, so they always may.
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
            here: String::new(),
            away: server_dirs()
                .into_iter()
                .map(|dir| Away::start(dir, None))
                .collect(),
            fill_server: None,
            adding: None,
            device_tx,
            device_rx,
            p: Palette::new(dark),
            screen: Screen::Chat,
            menu: None,
            plus: None,
            popup: None,
            side: None,
            settings_last: Screen::Profile,
            rows: Rc::new(Vec::new()),
            scroller,
            composer,
            server_in,
            user_in,
            pass_in,
            secret_in,
            queue_in,
            aside_in,
            queue_editing: None,
            code_in,
            name_in,
            skill_name_in,
            skill_desc_in,
            skill_body_in,
            skill_open: None,
            table_dropping: None,
            panel: None,
            panel_context: None,
            panel_opening: None,
            door_in: Rc::new(std::cell::RefCell::new(Vec::new())),
            note_in,
            profile_in,
            routine_name_in,
            move_to_in,
            relay_in,
            move_code_in,
            invite_admin: false,
            server_form: false,
            show_code: false,
            asked_programs: false,
            make: Making::default(),
            launched: None,
            keys: cx.focus_handle(),
            page_image: None,
            page_bounds: Rc::new(std::cell::Cell::new(None)),
            browser_keys: cx.focus_handle(),
            page: ((Screen::Chat, None), Instant::now()),
            scene: ("", Instant::now()),
            closing: None,
            form_at: Instant::now(),
            confirm_delete: false,
            routine_at_in,
            routine_prompt_in,
            routine_open: None,
            routine_agent: None,
            task_prompt_in,
            task_agent: None,
            task_new: false,
            routine_notify: true,
            routine_on: true,
            routine_catch_up: true,
            routine_thread: false,
            routine_webhook: false,
            routine_kind: "every".into(),
            routine_flow: Vec::new(),
            routine_flow_kept: Vec::new(),
            program_open: None,
            mcp_open: None,
            mcp_name_in,
            mcp_cmd_in,
            mcp_settings_in,
            set_key_in,
            set_value_in,
            share_in,
            share_month: false,
            instructions_in,
            search_in,
            open_thoughts: HashSet::new(),
            tray,
            hwnd,
            notice: None,
        };
        loom.fetch();
        window.focus(&loom.keys, cx);
        loom
    }

    /// Esc steps back: closes a menu, the server form, a page, or a step of making an
    /// agent, whichever is on top.
    fn escape(&mut self, cx: &mut Context<Self>) {
        if self.menu.is_some() {
            self.menu = None;
        } else if self.popup.is_some() {
            self.popup = None;
        } else if self.side.is_some() && self.screen == Screen::Chat {
            self.close_side(cx);
        } else if self.model.conn == Some(Conn::SignedOut) {
            if self.server_form {
                self.server_form = false;
                self.model.login_error = None;
                self.form_at = Instant::now();
            }
        } else if self.make.step == Step::Name {
            self.make.step = Step::Brain;
        } else if self.make.step == Step::Weaving {
            // It's being made; there's nothing to step back to.
        } else if self.screen != Screen::Chat {
            self.model.editing = None;
            self.screen = Screen::Chat;
        } else if self.model.open_branch.is_some() {
            self.open_branch(None, cx);
            return;
        }
        cx.notify();
    }

    fn send(&self, frame: Value) {
        self.core.send(&frame.to_string());
    }

    /// Does what the core asks of this computer for an agent, once it has checked the
    /// call is allowed: the clipboard here, and a screenshot on a thread of its own,
    /// answered with `device.answer`.
    fn device_ask(&mut self, ask: &Value, cx: &mut Context<Self>) {
        let call = ask
            .get("call")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let answer = |ok: bool, output: &str| serde_json::json!({"t": "device.answer", "call": call, "ok": ok, "output": output});
        match ask.get("tool").and_then(Value::as_str).unwrap_or_default() {
            "read_clipboard" => {
                let text = cx.read_from_clipboard().and_then(|c| c.text());
                self.send(match text {
                    Some(t) => answer(true, &t),
                    None => answer(true, "The clipboard has no text."),
                });
            }
            "write_clipboard" => {
                let text = ask
                    .pointer("/args/text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                self.send(answer(true, "Copied."));
            }
            "screenshot" => {
                let tx = self.device_tx.clone();
                std::thread::spawn(move || {
                    use base64::Engine as _;
                    let frame = match shell::screenshot() {
                        Ok((jpeg, w, h)) => serde_json::json!({
                            "t": "device.answer", "call": call, "ok": true,
                            "output": format!("The main screen, taken at {w}×{h}."),
                            "image": base64::engine::general_purpose::STANDARD.encode(jpeg),
                            "mime": "image/jpeg",
                        }),
                        Err(e) => serde_json::json!({
                            "t": "device.answer", "call": call, "ok": false,
                            "output": format!("The screenshot didn't work: {e}"),
                        }),
                    };
                    let _ = tx.send(frame);
                });
            }
            _ => self.send(answer(false, "This app has no such tool.")),
        }
    }

    /// Runs the instance on this computer, or stops and goes back to signing in to one
    /// elsewhere: keeps the choice, and starts the core again with it.
    /// Opens a page, asking the instance for what it shows.
    fn open(&mut self, screen: Screen, cx: &mut Context<Self>) {
        match screen {
            Screen::Server => {
                let f = self.model.load_scripts();
                self.send(f);
            }
            Screen::Tables => {
                self.table_dropping = None;
                for f in self.model.open_table(None) {
                    self.send(f);
                }
            }
            Screen::Profile => {
                let f = self.model.load_profile();
                self.send(f);
                self.model.profile = None;
            }
            Screen::Routines => {
                let f = self.model.load_routines();
                self.send(f);
                self.routine_open = None;
            }
            Screen::Tasks => {
                let f = self.model.load_tasks();
                self.send(f);
            }
            Screen::Skills => {
                let f = self.model.load_skills();
                self.send(f);
                self.skill_open = None;
            }
            Screen::Plugins => {
                let f = self.model.load_plugins();
                self.send(f);
            }
            Screen::Programs => {
                for f in self.model.load_programs() {
                    self.send(f);
                }
                self.program_open = None;
            }
            Screen::Mcp => {
                for f in self.model.load_mcp() {
                    self.send(f);
                }
                self.mcp_open = None;
            }
            Screen::ThisDevice => {
                let f = self.model.device_get();
                self.send(f);
            }
            Screen::People => {
                self.model.people_open = true;
                for f in self.model.load_people() {
                    self.send(f);
                }
            }
            Screen::AddPhone => {
                // An instance in this app reaches phones only once they're on.
                if self.local() {
                    let f = self.model.set_phones(None);
                    self.send(f);
                }
                let f = self.model.create_link();
                self.send(f);
                let d = self.model.load_devices();
                self.send(d);
                let r = self.model.set_relay(None);
                self.send(r);
            }
            Screen::NewAgent => {
                self.make = Making::default();
                let f = self.model.programs();
                self.send(f);
            }
            Screen::AddServer => {
                self.show_code = false;
                if self.fill_server.is_none() {
                    self.fill_server = Some(String::new());
                }
            }
            _ => {}
        }
        self.menu = None;
        // Only making an agent takes the main area from the chat. An agent's own pages
        // go beside the chat, and everything else in the popup over the window.
        match screen {
            Screen::Chat | Screen::NewAgent => {
                self.screen = screen;
                self.popup = None;
                self.side = None;
            }
            Screen::EditAgent
            | Screen::Activity
            | Screen::Memory
            | Screen::Pins
            | Screen::Tables => self.show_side(screen, cx),
            _ => {
                if !is_server(screen) {
                    self.settings_last = screen;
                }
                self.popup = Some(screen);
            }
        }
        cx.notify();
    }

    /// Opens `screen` in the panel beside the chat, in place of the browser if it showed.
    fn show_side(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.model.watching.is_some() {
            self.watch(None, cx);
        }
        self.screen = Screen::Chat;
        self.popup = None;
        self.side = Some(screen);
        self.model.pins_open = screen == Screen::Pins;
        self.model.tables_open = screen == Screen::Tables;
        if screen == Screen::Pins {
            if let Some(f) = self.model.load_pins() {
                self.send(f);
            }
        }
    }

    /// Closes the panel beside the chat, and a plugin's panel with it.
    fn close_side(&mut self, cx: &mut Context<Self>) {
        if self.side == Some(Screen::Plugins) {
            self.model.close_panel();
        }
        self.model.page = None;
        self.model.pins_open = false;
        self.model.tables_open = false;
        self.side = None;
        self.model.editing = None;
        cx.notify();
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        // Another server signs out and goes from the app.
        if !self.here.is_empty() {
            self.remove_server(cx);
            return;
        }
        if local_mode() {
            // Stops the instance here; its agents and chats stay for next time.
            self.set_local(false, cx);
            return;
        }
        self.send(serde_json::json!({"t": "logout"}));
        cx.notify();
    }

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
        // The choice is the first server's.
        self.show_server("", cx);
        self.model = Model::new();
        self.screen = Screen::Chat;
        self.menu = None;
        // The old core stops, and its instance with it, as it's dropped.
        let (core, events) = Core::start(&core_config("", None));
        self.core = core;
        self.events = events;
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    // --- Servers (PLAN.md, "Several servers in one app") ---

    /// Brings in what the servers away said. Gives whether the window should redraw.
    fn pump_away(&mut self) -> bool {
        let mut changed = false;
        for a in &mut self.away {
            let got: Vec<String> = std::iter::from_fn(|| a.events.try_recv().ok()).collect();
            for e in got {
                let Ok(v) = serde_json::from_str::<Value>(&e) else {
                    continue;
                };
                match v.get("t").and_then(Value::as_str) {
                    // This PC's tools answer the server on screen alone.
                    Some("core.device_ask") => {
                        let call = v.get("call").and_then(Value::as_str).unwrap_or_default();
                        a.core.send(&serde_json::json!({"t": "device.answer", "call": call, "ok": false, "output": "This server isn't the one open in loom on this PC. Ask your person to open it there."}).to_string());
                        continue;
                    }
                    Some("core.notify") => {
                        let s = |k: &str| {
                            v.get(k)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string()
                        };
                        shell::notify(&s("title"), &s("body"));
                    }
                    _ => {}
                }
                changed |= a.model.take(&v);
            }
            for f in a.model.fetches() {
                a.core.send(&f.to_string());
            }
        }
        changed
    }

    /// Puts the server whose folder is `dir` on screen, and the one there away.
    fn show_server(&mut self, dir: &str, cx: &mut Context<Self>) {
        if self.here == dir {
            return;
        }
        let Some(i) = self.away.iter().position(|a| a.dir == dir) else {
            return;
        };
        let a = &mut self.away[i];
        std::mem::swap(&mut self.core, &mut a.core);
        std::mem::swap(&mut self.events, &mut a.events);
        std::mem::swap(&mut self.model, &mut a.model);
        std::mem::swap(&mut self.here, &mut a.dir);
        // The one away notifies whatever the window does; the one here, only while it's
        // not looked at, and it's looked at now.
        a.core.send(r#"{"t":"app.active","active":false}"#);
        self.send(serde_json::json!({"t": "app.active", "active": true}));
        self.popup = None;
        self.side = None;
        self.menu = None;
        self.panel = None;
        self.screen = Screen::Chat;
        self.server_form = self.model.conn == Some(Conn::SignedOut) && !self.here.is_empty();
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Opens the popup to add a server, at `address` when an agent offered one. One the
    /// app has already is put on screen instead.
    fn add_server(&mut self, address: Option<String>, cx: &mut Context<Self>) {
        if let Some(a) = &address {
            let same = |m: &Model| {
                m.server.as_deref().map(|s| s.trim_end_matches('/'))
                    == Some(a.trim_end_matches('/'))
            };
            if same(&self.model) {
                return;
            }
            if let Some(dir) = self
                .away
                .iter()
                .find(|x| same(&x.model))
                .map(|x| x.dir.clone())
            {
                self.show_server(&dir, cx);
                return;
            }
        }
        self.fill_server = Some(address.unwrap_or_default());
        self.open(Screen::AddServer, cx);
    }

    /// Signs in to the server in the add-a-server form: with a core of its own beside
    /// the one on screen, started the first time.
    fn sign_in_added(&mut self, cx: &mut Context<Self>) {
        let server = self.server_in.read(cx).value().trim().to_string();
        let user = self.user_in.read(cx).value().to_string();
        let pass = self.pass_in.read(cx).value().to_string();
        let code = if self.show_code {
            self.code_in.read(cx).value().to_string()
        } else {
            String::new()
        };
        let dir = match self.adding.clone() {
            Some(dir) => dir,
            None => {
                // The app keeps it once it's signed in. A folder left from one that never
                // was is started afresh.
                let dirs = server_dirs();
                let n = (2..)
                    .find(|n| !dirs.contains(&format!("servers/{n}")))
                    .unwrap_or(2);
                let dir = format!("servers/{n}");
                let _ = std::fs::remove_dir_all(server_folder(&dir));
                self.away.push(Away::start(dir.clone(), Some(&server)));
                self.adding = Some(dir.clone());
                dir
            }
        };
        if let Some(a) = self.away.iter_mut().find(|a| a.dir == dir) {
            if a.model.logging_in {
                return;
            }
            let frame = a.model.login(&server, &user, &pass, &code);
            a.core.send(&frame.to_string());
        }
        cx.notify();
    }

    /// The server being added: on screen once it's signed in, and gone again if the
    /// popup closed before it was.
    fn watch_adding(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.adding.clone() else {
            return;
        };
        let live = self
            .away
            .iter()
            .find(|a| a.dir == dir)
            .is_some_and(|a| matches!(a.model.conn, Some(Conn::Live)));
        if live {
            self.adding = None;
            self.popup = None;
            let mut dirs = server_dirs();
            dirs.push(dir.clone());
            if let Err(e) = keep_server_dirs(&dirs) {
                self.model
                    .notices
                    .push(format!("loom couldn't keep the new server: {e}"));
            }
            self.show_server(&dir, cx);
        } else if self.popup != Some(Screen::AddServer) {
            self.adding = None;
            self.forget_server(&dir);
            cx.notify();
        }
    }

    /// Signs out of the server on screen, takes it from the app, and puts the first
    /// server on screen. The first server isn't taken.
    fn remove_server(&mut self, cx: &mut Context<Self>) {
        let dir = self.here.clone();
        if !is_server_dir(&dir) {
            return;
        }
        self.send(serde_json::json!({"t": "logout"}));
        self.show_server("", cx);
        self.forget_server(&dir);
        cx.notify();
    }

    /// Stops a server away, and takes it from the app's list, with its folder.
    fn forget_server(&mut self, dir: &str) {
        if !is_server_dir(dir) {
            return;
        }
        if let Some(i) = self.away.iter().position(|a| a.dir == dir) {
            // Its core stops as it's dropped, and lets go of its folder.
            drop(self.away.remove(i));
        }
        let dirs: Vec<String> = server_dirs().into_iter().filter(|d| d != dir).collect();
        if let Err(e) = keep_server_dirs(&dirs) {
            self.model
                .notices
                .push(format!("loom couldn't forget the server: {e}"));
            return;
        }
        let folder = server_folder(dir);
        if folder.starts_with(data_dir()) && folder != data_dir() {
            let _ = std::fs::remove_dir_all(folder);
        }
    }

    /// Whether the server on screen is the instance on this computer.
    fn local(&self) -> bool {
        self.here.is_empty() && local_mode()
    }

    /// Every server, in the order the sidebar shows them: the first, then the rest as
    /// they were added. Each is its folder and what it shows.
    fn servers(&self) -> Vec<(String, &Model)> {
        let mut all: Vec<(String, &Model)> = vec![(self.here.clone(), &self.model)];
        all.extend(self.away.iter().map(|a| (a.dir.clone(), &a.model)));
        let key = |d: &str| {
            d.strip_prefix("servers/")
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
        };
        all.sort_by_key(|(d, _)| key(d));
        all
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
        for frame in std::mem::take(&mut self.model.to_send) {
            self.send(frame);
        }
        for path in std::mem::take(&mut self.model.to_open) {
            cx.open_with_system(std::path::Path::new(&path));
        }
    }

    /// Opens a page from the chat beside it, walled off as a plugin's panel is.
    pub fn open_page(&mut self, f: &model::FileRef, cx: &mut Context<Self>) {
        if let Some(frame) = self.model.open_page(f) {
            self.send(frame);
        }
        self.show_side(Screen::Page, cx);
        cx.notify();
    }

    /// Opens a file a reply named, from its agent's folder, in its own program.
    pub fn open_mention(&mut self, m: &model::Mention) {
        let frame = self.model.open_mention(m);
        self.send(frame);
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
        self.popup = None;
        // An agent's own page beside the chat is the old agent's: it closes.
        if self.side.is_some() && self.side != Some(Screen::Plugins) {
            self.side = None;
        }
        self.model.editing = None;
        self.menu = None;
        self.model.select(Some(agent));
        self.model.show_task(None);
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Shows a task's conversation in the chat, or with None, the agent's chat again.
    fn open_task(&mut self, task: Option<String>, cx: &mut Context<Self>) {
        self.screen = Screen::Chat;
        self.popup = None;
        self.model.editing = None;
        self.menu = None;
        self.model.show_task(task);
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    // --- Beside the chat, and the popup ---

    /// The panel beside the chat on the right: an agent's page, or a plugin's panel,
    /// whose web view sits over the space left for it under the head.
    fn side_panel(&mut self, side: Screen, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let width = side_width(side, window.viewport_size().width.as_f32());
        let name = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let (icon, title) = match side {
            Screen::EditAgent => (Ico::Pencil, format!("Edit {name}")),
            Screen::Activity => (Ico::Activity, format!("What {name} did")),
            Screen::Memory => (Ico::Brain, format!("What {name} remembers")),
            Screen::Pins => (Ico::Pin, "Pinned".to_string()),
            Screen::Tables => (Ico::Sheet, format!("{name}'s tables")),
            Screen::Aside => (
                Ico::MessageCircleQuestionMark,
                format!("Ask {name} on the side"),
            ),
            Screen::Page => (
                Ico::AppWindow,
                self.model
                    .page
                    .as_ref()
                    .map(model::FileRef::title)
                    .unwrap_or_else(|| "Page".into()),
            ),
            _ => (
                Ico::Puzzle,
                self.model
                    .panel
                    .as_ref()
                    .map(|o| o.plugin.name.clone())
                    .unwrap_or_else(|| "Plugin".into()),
            ),
        };
        let body = match side {
            Screen::EditAgent => self.edit_agent(cx),
            Screen::Activity => self.activity_page(cx),
            Screen::Memory => self.memory_page(cx),
            Screen::Pins => self.pins_page(&name, cx),
            Screen::Tables => self.tables_panel(cx),
            Screen::Aside => self.aside_page(&name),
            // The page's web view sits over this, once it's here.
            Screen::Page if self.model.page_path().is_none() => div()
                .flex_1()
                .p(px(space::L))
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child("Opening the page…"),
            _ => div().flex_1(),
        };
        // The agent's edit panel opens under its thread, across the panel, so a new
        // colour shows on it at once. Other panels don't have it.
        let hero = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .filter(|_| side == Screen::EditAgent)
            .map(|a| {
                let (mood, snapped) = fidget(&a.id, self.model.mood(&a.id));
                div().px(px(space::L)).pt(px(space::S)).child(thread(
                    format!("side-thread-{}", a.id),
                    mood,
                    thread::color(&a.color, p.dark),
                    width - 2.0 * space::L,
                    40.0,
                    snapped.or(self.model.finished_at(&a.id)),
                ))
            });
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(p.line)
            .child(
                div()
                    .h(px(SIDE_HEAD))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(space::L))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_2)
                            .child(Icon::new(icon).small())
                            .child(title),
                    )
                    .child(
                        self.close_button("side-close")
                            .on_click(cx.listener(|this, _, _, cx| this.close_side(cx))),
                    ),
            )
            .when_some(hero, |d, h| d.child(h))
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
    }

    /// A small round X.
    fn close_button(&self, id: &'static str) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .size(px(28.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .text_color(p.ink_3)
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover).text_color(p.ink))
            .child(Icon::new(Ico::X).small())
    }

    /// How the server's spool is now.
    fn spool_state(&self) -> Spool {
        spool_of(&self.model)
    }

    /// The spool's colour: the app's ink, or the colour for trouble.
    fn spool_ink(&self) -> Hsla {
        self.ink_of(self.spool_state())
    }

    fn ink_of(&self, state: Spool) -> Hsla {
        match state {
            Spool::Loose => self.p.bad,
            Spool::Empty => self.p.ink_3,
            _ => self.p.ink_2,
        }
    }

    /// Each server the app is signed in to, in a row. The one on screen is a box with its
    /// spool, its name and how it is, which opens the server popup; another is its spool
    /// alone, which comes on screen, with a dot when one of its agents said something there.
    fn spools(&self, server_open: bool, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut row = div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .min_w_0();
        for (dir, model) in self.servers() {
            let here = dir == self.here;
            let state = spool_of(model);
            let ink = self.ink_of(state);
            let id = SharedString::from(format!("server-{dir}"));
            let mut button = div()
                .id(id)
                .relative()
                .flex()
                .items_center()
                .gap(px(space::S))
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover));
            if here {
                let dot = match state {
                    Spool::Wound => p.ok,
                    Spool::Loose => p.bad,
                    _ => p.ink_3,
                };
                button = button
                    .flex_1()
                    .min_w_0()
                    .px(px(space::S))
                    .py(px(6.0))
                    .border_1()
                    .border_color(if server_open { p.line_strong } else { p.line })
                    .when(server_open, |d| d.bg(p.active))
                    .child(div().flex_none().child(spool(state, ink, 30.0, 14.0)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink)
                                    .truncate()
                                    .child(server_name(model, &dir)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.0))
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(div().size(px(6.0)).flex_none().rounded(px(2.0)).bg(dot))
                                    .child(div().truncate().child(self.conn_words())),
                            ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.open(Screen::Server, cx)));
            } else {
                button = button.flex_none().px(px(space::XS)).py(px(space::S));
                let heard = !model.unread.is_empty();
                button = button
                    .child(spool(state, ink, 26.0, 13.0))
                    .when(heard, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(3.0))
                                .right(px(3.0))
                                .size(px(6.0))
                                .rounded_full()
                                .bg(p.ink),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.show_server(&dir, cx)));
            }
            row = row.child(button);
        }
        row
    }

    /// The connection in words.
    fn conn_words(&self) -> String {
        match &self.model.conn {
            Some(Conn::Live) if self.model.synced => "Connected".into(),
            Some(Conn::Offline(ms)) => format!("Offline, trying again in {}s", ms.div_ceil(1000)),
            Some(Conn::SignedOut) => "Signed out".into(),
            Some(Conn::UpdateNeeded) => "Needs an update".into(),
            _ => "Connecting…".into(),
        }
    }

    /// The popup's pages: the person's settings, or the server's.
    fn popup_pages(&self, server: bool) -> Vec<(&'static str, Ico, &'static str, Screen)> {
        if server {
            let mut pages = vec![
                ("pop-server", Ico::Monitor, "This server", Screen::Server),
                (
                    "pop-phone",
                    Ico::Smartphone,
                    "Add a phone",
                    Screen::AddPhone,
                ),
            ];
            // An instance somewhere else has other people, and reaches this PC as a device.
            if !self.local() {
                pages.push(("pop-people", Ico::Users, "People", Screen::People));
                pages.push(("pop-device", Ico::Monitor, "This PC", Screen::ThisDevice));
            }
            return pages;
        }
        vec![
            (
                "pop-profile",
                Ico::CircleUserRound,
                "About you",
                Screen::Profile,
            ),
            ("pop-routines", Ico::Clock, "Routines", Screen::Routines),
            ("pop-tasks", Ico::ListTodo, "Tasks", Screen::Tasks),
            ("pop-skills", Ico::BookOpen, "Skills", Screen::Skills),
            ("pop-programs", Ico::Bot, "Agent programs", Screen::Programs),
            ("pop-mcp", Ico::Plug, "MCP servers", Screen::Mcp),
            ("pop-plugins", Ico::Puzzle, "Plugins", Screen::Plugins),
        ]
    }

    /// The popup over the window: its pages down the left, the open one on the right.
    /// A click outside it, or Esc, closes it.
    fn popup_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let open = self.popup?;
        let p = self.p;
        let server = is_server(open);
        let pages = self.popup_pages(server);
        let size = window.viewport_size();
        let w = (size.width.as_f32() - 2.0 * space::XXL).clamp(320.0, 900.0);
        let h = (size.height.as_f32() - 2.0 * space::XXL).clamp(320.0, 640.0);
        let mut nav = div()
            .w(px(210.0))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(space::S))
            .bg(p.side)
            .border_r_1()
            .border_color(p.line);
        // Its head: the gear, or the server's spool, as the sidebar shows them.
        nav = nav.child(
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::M))
                .pt(px(space::S))
                .pb(px(space::M))
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink)
                .when(server, |d| {
                    d.child(spool(self.spool_state(), self.spool_ink(), 30.0, 14.0))
                        .child("Server")
                })
                .when(!server, |d| {
                    d.child(Icon::new(Ico::Settings).small().text_color(p.ink_2))
                        .child("Settings")
                }),
        );
        for (id, icon, label, screen) in pages {
            nav = nav.child(
                self.menu_item(id, icon, label, false)
                    .when(screen == open, |d| d.bg(p.active))
                    .on_click(cx.listener(move |this, _, _, cx| this.open(screen, cx))),
            );
        }
        // What isn't a setting of this server, but something to do: at the bottom.
        if server {
            nav = nav.child(div().flex_1());
            let actions = [
                ("pop-move", Ico::Truck, "Move", Screen::Move),
                (
                    "pop-add-server",
                    Ico::Plus,
                    "Add a server",
                    Screen::AddServer,
                ),
            ];
            for (id, icon, label, screen) in actions {
                nav = nav.child(
                    self.menu_item(id, icon, label, false)
                        .when(screen == open, |d| d.bg(p.active))
                        .on_click(cx.listener(move |this, _, _, cx| this.open(screen, cx))),
                );
            }
        }
        if server && !self.here.is_empty() {
            nav = nav.child(
                self.menu_item(
                    "pop-remove-server",
                    Ico::LogOut,
                    "Sign out and remove",
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.popup = None;
                    this.remove_server(cx)
                })),
            );
        } else if server {
            let local = self.local();
            nav = nav.child(
                self.menu_item(
                    "pop-sign-out",
                    if local { Ico::Power } else { Ico::LogOut },
                    if local {
                        "Close this instance"
                    } else {
                        "Sign out"
                    },
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.popup = None;
                    this.sign_out(cx)
                })),
            );
        }
        let page = match open {
            Screen::Server => self.server_page(cx),
            Screen::Profile => self.profile_page(cx),
            Screen::Skills => self.skills_page(cx),
            Screen::Routines => self.routines_page(cx),
            Screen::Tasks => self.tasks_page(cx),
            Screen::Programs => self.programs_page(cx),
            Screen::ThisDevice => self.device_page(cx),
            Screen::Move => self.move_page(cx),
            Screen::People => self.people_page(cx),
            Screen::AddPhone => self.add_phone_page(cx),
            Screen::Mcp => self.mcp_page(cx),
            Screen::Plugins => self.plugins_page(cx),
            Screen::AddServer => self.add_server_page(cx),
            _ => div(),
        };
        let card = div()
            .id("popup")
            .occlude()
            .w(px(w))
            .h(px(h))
            .flex()
            .bg(p.bg)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .overflow_hidden()
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                // A menu of the page's own (a dropdown) closes first.
                if this.menu.is_none() {
                    this.popup = None;
                    cx.notify();
                }
            }))
            .child(nav)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .pt(px(space::L))
                    .child(div().flex_1().min_h_0().flex().flex_col().child(page))
                    // The page has its own heading; the popup only its X.
                    .child(
                        div()
                            .absolute()
                            .top(px(space::S))
                            .right(px(space::S))
                            .child(self.close_button("popup-close").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.popup = None;
                                    cx.notify();
                                },
                            ))),
                    ),
            );
        let mut scrim = p.bg;
        scrim.a = 0.7;
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                // It takes the pointer: nothing behind it hovers or takes a click, so
                // the button that opened it doesn't open it again.
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(scrim)
                .child(card),
        )
    }

    /// While a menu is open, a clear sheet over the window that takes the pointer: nothing
    /// behind it hovers, and a click anywhere, the button that opened it too, closes it.
    /// The menus themselves are drawn over it.
    fn menu_scrim(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.menu.is_none() && self.plus.is_none() {
            return None;
        }
        Some(
            div()
                .id("menu-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_any_mouse_down(cx.listener(|this, _, _, cx| {
                    this.menu = None;
                    this.plus = None;
                    cx.notify();
                })),
        )
    }

    /// The popup's first server page: where the instance is and how it is, and scripts.
    fn server_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let where_ = if self.local() {
            "On this computer".to_string()
        } else {
            match &self.model.server {
                Some(_) => server_name(&self.model, &self.here),
                None => "On a server".into(),
            }
        };
        let state = self.spool_state();
        let words = self.conn_words();
        let status = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .text_size(px(size::SMALL))
            .text_color(if state == Spool::Loose {
                p.bad
            } else {
                p.ink_2
            })
            .child(div().size(px(7.0)).rounded(px(2.0)).bg(match state {
                Spool::Wound => p.ok,
                Spool::Loose => p.bad,
                _ => p.ink_3,
            }))
            .child(words);
        let here = self
            .row(
                Some(
                    div()
                        .w(px(52.0))
                        .flex_none()
                        .flex()
                        .justify_center()
                        .child(spool(state, self.spool_ink(), 46.0, 20.0))
                        .into_any_element(),
                ),
                where_,
                Some(format!("loom {}", crate::VERSION)),
                Some(status.into_any_element()),
            )
            .min_h(px(68.0));
        let groups = vec![
            self.group(None, vec![here.into_any_element()]),
            self.scripts_group(cx),
        ];
        self.settings_page("server-page", "This server", None, groups)
    }

    /// Scripts on the instance's computer (PLAN.md, "Scripts"): off until turned on,
    /// with a token shown once, and how to use it.
    fn scripts_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let (on, port) = self.model.scripts.unwrap_or((false, 0));
        let toggle =
            self.switch("scripts-switch", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let f = this.model.set_scripts(!on);
                    this.send(f);
                    cx.notify();
                }));
        let mut rows = vec![self
            .row(
                Some(self.tile(Ico::SquareTerminal)),
                "Scripts",
                Some("Your scripts on that computer can use your agents".into()),
                self.model
                    .scripts
                    .is_some()
                    .then(|| toggle.into_any_element()),
            )
            .into_any_element()];
        if let Some(token) = self.model.script_token.clone() {
            let agent = self
                .model
                .agents
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Bean".into());
            let copied = token.clone();
            let copy =
                self.small_button("script-token", "Copy", false)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                    });
            rows.push(
                self.row(
                    None,
                    token.clone(),
                    Some("Your token, shown this once".into()),
                    Some(copy.into_any_element()),
                )
                .into_any_element(),
            );
            rows.push(self.note_row(
                format!("loom ask {agent} \"hello\" --port {port} --token {token}"),
                p.ink_2,
            ));
        } else if on {
            rows.push(
                self.row(
                    None,
                    "Token",
                    Some("Shown once, when it's made".into()),
                    Some(
                        self.small_button("scripts-new", "New token", false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let f = this.model.set_scripts(true);
                                this.send(f);
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        self.group(None, rows)
    }

    // --- Tasks ---

    /// What a task is doing, in a few words.
    fn task_status(task: &model::Task) -> String {
        match task.status.as_str() {
            "running" => "Working on it".into(),
            "paused" => "Paused".into(),
            "done" => "Done".into(),
            "cancelled" => "Cancelled".into(),
            _ if task.error.is_empty() => "Couldn't finish".into(),
            _ => format!("Couldn't finish: {}", task.error),
        }
    }

    /// The buttons for what a task can do next: pause or resume it, cancel it, or try it again.
    fn task_actions(&self, task: &model::Task, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().gap(px(space::XS));
        let add = |row: Div, name: &'static str, label: &str, primary: bool| {
            let id = task.id.clone();
            row.child(
                self.small_button(format!("task-{name}-{}", task.id), label, primary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this
                            .model
                            .task_cmd(&format!("task.{name}"), serde_json::json!({"id": id}));
                        this.send(f);
                        cx.notify();
                    })),
            )
        };
        match task.status.as_str() {
            "running" => {
                row = add(row, "pause", "Pause", false);
                row = add(row, "cancel", "Cancel", false);
            }
            "paused" => {
                row = add(row, "resume", "Resume", true);
                row = add(row, "cancel", "Cancel", false);
            }
            "failed" | "cancelled" => {
                row = add(row, "retry", "Try again", true);
            }
            _ => {}
        }
        row
    }

    /// Over the chat while it shows a task: back to the agent's chat, the task, and what it can do.
    fn task_strip(&self, task: &model::Task, tint: Hsla, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(space::XL))
            .pt(px(space::S))
            .child(
                div()
                    .w_full()
                    .max_w(px(space::COLUMN))
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::M))
                    .py(px(space::S))
                    .rounded(px(radius::BUTTON))
                    .border_1()
                    .border_color(p.line)
                    .child(
                        div()
                            .id("task-back")
                            .size(px(28.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover).text_color(p.ink))
                            .child(Icon::new(Ico::ArrowLeft).small())
                            .on_click(cx.listener(|this, _, _, cx| this.open_task(None, cx))),
                    )
                    .child(Icon::new(Ico::ListTodo).small().text_color(tint))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(size::SMALL))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink)
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(size::CAPTION))
                                    .text_color(if task.status == "failed" {
                                        p.bad
                                    } else {
                                        p.ink_3
                                    })
                                    .child(Loom::task_status(task)),
                            ),
                    )
                    .child(self.task_actions(task, cx)),
            )
    }

    /// Over a branch, or a routine's thread, in the chat: its title, and the way back to the
    /// agent's chat.
    fn branch_strip(
        &self,
        title: String,
        routine: bool,
        tint: Hsla,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(space::XL))
            .pt(px(space::S))
            .child(
                div()
                    .w_full()
                    .max_w(px(space::COLUMN))
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::M))
                    .py(px(space::S))
                    .rounded(px(radius::BUTTON))
                    .border_1()
                    .border_color(p.line)
                    .child(
                        div()
                            .id("branch-back")
                            .size(px(28.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover).text_color(p.ink))
                            .child(Icon::new(Ico::ArrowLeft).small())
                            .on_click(cx.listener(|this, _, _, cx| this.open_branch(None, cx))),
                    )
                    .child(
                        Icon::new(if routine { Ico::Clock } else { Ico::GitBranch })
                            .small()
                            .text_color(tint),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(size::SMALL))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(if routine {
                                        "A routine's thread: each run posts here, not in the chat"
                                    } else {
                                        "A branch: the chat goes on as it was"
                                    }),
                            ),
                    ),
            )
    }

    /// Shows a routine's thread of its own in place of its agent's chat, closing the page
    /// that asked for it.
    fn open_thread(&mut self, agent: &str, conversation: &str, cx: &mut Context<Self>) {
        self.popup = None;
        self.screen = Screen::Chat;
        self.model.select(Some(agent.to_string()));
        self.open_branch(Some(conversation.to_string()), cx);
    }

    /// Shows a branch in the chat, or the chat again.
    fn open_branch(&mut self, branch: Option<String>, cx: &mut Context<Self>) {
        self.model.show_branch(branch);
        self.fetch();
        self.refresh_rows(cx);
        cx.notify();
    }

    /// The agents to pick one from, as chips with their thread's colour.
    fn agent_chips(
        &self,
        prefix: &'static str,
        picked: Option<&str>,
        cx: &mut Context<Self>,
        pick: fn(&mut Loom, String),
    ) -> Div {
        let p = self.p;
        let mut chips = div().flex().flex_wrap().gap(px(space::XS));
        for a in self.model.agents.clone() {
            let id = a.id.clone();
            let on = picked == Some(a.id.as_str());
            let tint = thread::color(&a.color, p.dark);
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("{prefix}-{}", a.id)))
                    .h(px(28.0))
                    .px(px(space::M))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .rounded(px(radius::CHIP))
                    .border_1()
                    .text_size(px(size::SMALL))
                    .cursor_pointer()
                    .when(on, |d| {
                        d.bg(p.bg)
                            .border_color(p.line_strong)
                            .text_color(p.ink)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!on, |d| {
                        d.border_color(transparent_black())
                            .text_color(p.ink_3)
                            .hover(move |s| s.text_color(p.ink))
                    })
                    .child(div().w(px(12.0)).h(px(2.0)).rounded(px(1.0)).bg(tint))
                    .child(a.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        pick(this, id.clone());
                        cx.notify();
                    })),
            );
        }
        chips
    }

    /// The Tasks page: every task, newest first, and a new one for an agent.
    fn tasks_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        if self
            .task_agent
            .as_ref()
            .is_none_or(|a| self.model.agent(a).is_none())
        {
            self.task_agent = self
                .model
                .selected
                .clone()
                .or_else(|| self.model.agents.first().map(|a| a.id.clone()));
        }
        let mut groups = Vec::new();
        if self.task_new {
            let start = self
                .small_button("start-task", "Start", true)
                .on_click(cx.listener(|this, _, window, cx| {
                    let prompt = this.task_prompt_in.read(cx).value().trim().to_string();
                    let Some(agent) = this.task_agent.clone() else {
                        return;
                    };
                    if prompt.is_empty() {
                        return;
                    }
                    let f = this.model.task_cmd(
                        "task.start",
                        serde_json::json!({"agent": agent, "prompt": prompt}),
                    );
                    this.send(f);
                    this.task_prompt_in
                        .update(cx, |s, cx| s.set_value("", window, cx));
                    this.task_new = false;
                    cx.notify();
                }));
            let cancel = self
                .small_button("cancel-task", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.task_new = false;
                    cx.notify();
                }));
            let chips = self.agent_chips(
                "task-agent",
                self.task_agent.clone().as_deref(),
                cx,
                |this, id| this.task_agent = Some(id),
            );
            groups.push(self.group(
                Some("New task"),
                vec![
                    div()
                        .px(px(space::L))
                        .pt(px(space::M))
                        .child(chips)
                        .into_any_element(),
                    div()
                        .p(px(space::L))
                        .flex()
                        .flex_col()
                        .gap(px(space::M))
                        .child(Textarea::new(&self.task_prompt_in))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(space::S))
                                .child(cancel)
                                .child(start),
                        )
                        .into_any_element(),
                ],
            ));
        }
        let mut rows = Vec::new();
        for t in self.model.tasks.clone() {
            let Some(agent) = self.model.agent(&t.agent).cloned() else {
                continue;
            };
            let tint = thread::color(&agent.color, p.dark);
            let id = t.id.clone();
            let mut words = format!("{} · {}", agent.name, Loom::task_status(&t));
            if t.status == "done" && !t.result.is_empty() {
                words = format!("{words} · {}", t.result.lines().next().unwrap_or_default());
            }
            let lead = div()
                .size(px(30.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .border_1()
                .border_color(p.line)
                .bg(p.bg)
                .text_color(tint)
                .child(Icon::new(Ico::ListTodo).small())
                .into_any_element();
            rows.push(
                self.row(
                    Some(lead),
                    t.title.clone(),
                    Some(words),
                    Some(self.task_actions(&t, cx).into_any_element()),
                )
                .id(SharedString::from(format!("task-{}", t.id)))
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover))
                .on_click(cx.listener(move |this, _, _, cx| this.open_task(Some(id.clone()), cx)))
                .into_any_element(),
            );
        }
        let new_task = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("new-task", "New task", primary)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.task_new = true;
                    cx.notify();
                }))
                .into_any_element()
        };
        if !rows.is_empty() {
            groups.push(self.group(None, rows));
        } else if !self.task_new {
            let action = new_task(self, cx, true);
            groups.push(self.empty(
                Ico::ListTodo,
                "No tasks yet",
                "Hand an agent a job to work on by itself, and come back to what it did",
                Some(action),
            ));
        }
        let action =
            (!self.task_new && !self.model.tasks.is_empty()).then(|| new_task(self, cx, false));
        self.settings_page("tasks-page", "Tasks", action, groups)
    }

    // --- The parts ---

    /// Watches an agent's browser beside the chat, or stops with None.
    fn watch(&mut self, agent: Option<String>, cx: &mut Context<Self>) {
        // The browser takes the place beside the chat.
        if agent.is_some() && self.side.is_some() {
            self.close_side(cx);
        }
        if let Some(f) = self.model.watch(agent) {
            self.send(f);
        }
        cx.notify();
    }

    /// A click or a scroll on the page as drawn, in the page's own pixels.
    fn page_point(&self, at: Point<Pixels>) -> Option<(f32, f32)> {
        let b = self.page_bounds.get()?;
        let f = self.model.frame.as_ref()?;
        let w = b.size.width.as_f32();
        let h = b.size.height.as_f32();
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let x = (at.x - b.origin.x).as_f32() / w * f.width;
        let y = (at.y - b.origin.y).as_f32() / h * f.height;
        (x >= 0.0 && y >= 0.0 && x <= f.width && y <= f.height).then_some((x, y))
    }

    /// The person's key, on the page they've taken over: a named key, or the text it types.
    fn page_key(&mut self, e: &KeyDownEvent) -> bool {
        let k = &e.keystroke;
        let named = match k.key.as_str() {
            "enter" => Some("Enter"),
            "tab" => Some("Tab"),
            "escape" => Some("Escape"),
            "backspace" => Some("Backspace"),
            "delete" => Some("Delete"),
            "up" => Some("ArrowUp"),
            "down" => Some("ArrowDown"),
            "left" => Some("ArrowLeft"),
            "right" => Some("ArrowRight"),
            "home" => Some("Home"),
            "end" => Some("End"),
            "pageup" => Some("PageUp"),
            "pagedown" => Some("PageDown"),
            _ => None,
        };
        let input = if let Some(key) = named {
            serde_json::json!({"kind": "key", "key": key})
        } else if k.modifiers.control || k.modifiers.alt || k.modifiers.platform {
            return false;
        } else if let Some(text) = k.key_char.clone().filter(|t| !t.is_empty()) {
            serde_json::json!({"kind": "text", "text": text})
        } else {
            return false;
        };
        if let Some(f) = self.model.browser_input(input) {
            self.send(f);
        }
        true
    }

    /// The agent's browser beside its chat: its page as it changes, and the
    /// person's way in for what only they should do.
    fn browser_panel(
        &mut self,
        agent: &model::Agent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        let state = self.model.browser(&agent.id);
        // A new frame is decoded once; the one it replaces leaves the GPU.
        if let Some(f) = &self.model.frame {
            if self.page_image.as_ref().map(|(seq, _)| *seq) != Some(f.seq) {
                if let Ok(decoded) =
                    image::load_from_memory_with_format(&f.jpeg, image::ImageFormat::Jpeg)
                {
                    let mut rgba = decoded.to_rgba8();
                    for px in rgba.pixels_mut() {
                        px.0.swap(0, 2);
                    }
                    let render = Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]));
                    if let Some((_, old)) = self.page_image.replace((f.seq, render)) {
                        let _ = window.drop_image(old);
                    }
                }
            }
        } else if let Some((_, old)) = self.page_image.take() {
            let _ = window.drop_image(old);
        }
        let width = (window.viewport_size().width.as_f32() * 0.45).clamp(360.0, 820.0);
        let inner = width - 2.0 * space::L;
        let (fw, fh) = self
            .model
            .frame
            .as_ref()
            .map_or((1280.0, 800.0), |f| (f.width.max(1.0), f.height.max(1.0)));
        let height = inner * fh / fw;
        let bounds = self.page_bounds.clone();
        let page = match self.page_image.clone() {
            Some((_, image)) => img(ImageSource::Render(image))
                .size_full()
                .rounded(px(radius::BUTTON - 2.0))
                .into_any_element(),
            None => div()
                .w(px(inner))
                .h(px(height))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child(if state.open {
                    "Loading the page…"
                } else {
                    "The browser is closed."
                })
                .into_any_element(),
        };
        let person = state.person;
        let screen = div()
            .id("browser-page")
            .track_focus(&self.browser_keys)
            .relative()
            .w(px(inner))
            .h(px(height))
            .rounded(px(radius::BUTTON))
            .overflow_hidden()
            .border_2()
            .border_color(if person { tint } else { p.line })
            .when(person, |d| d.cursor_pointer())
            .child(page)
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.browser_keys, cx);
                    let Some((x, y)) = this.page_point(e.position) else {
                        return;
                    };
                    if let Some(f) = this
                        .model
                        .browser_input(serde_json::json!({"kind": "click", "x": x, "y": y}))
                    {
                        this.send(f);
                    }
                }),
            )
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| {
                let Some((x, y)) = this.page_point(e.position) else {
                    return;
                };
                let dy = match e.delta {
                    ScrollDelta::Pixels(d) => -d.y.as_f32(),
                    ScrollDelta::Lines(d) => -d.y * 40.0,
                };
                if dy.abs() < 1.0 {
                    return;
                }
                if let Some(f) = this
                    .model
                    .browser_input(serde_json::json!({"kind": "wheel", "x": x, "y": y, "dy": dy}))
                {
                    this.send(f);
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if this
                    .model
                    .browser(this.model.watching.as_deref().unwrap_or_default())
                    .person
                    && this.page_key(e)
                {
                    cx.stop_propagation();
                }
            }));
        let words = if person {
            "You have it. Click and type on the page.".to_string()
        } else if !state.asked.is_empty() {
            state.asked.clone()
        } else {
            format!("{} is browsing.", agent.name)
        };
        let hand = if person {
            self.button("browser-give", "Hand back", true)
        } else {
            self.button("browser-take", "Take over", !state.asked.is_empty())
        }
        .when(!state.open, |d| d.opacity(0.4))
        .on_click(cx.listener(move |this, _, window, cx| {
            if let Some(f) = this.model.take_browser(!person) {
                this.send(f);
            }
            if !person {
                window.focus(&this.browser_keys, cx);
            }
            cx.notify();
        }));
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::L))
            .border_l_1()
            .border_color(p.line)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_2)
                            .child(Icon::new(Ico::Globe).small().text_color(tint))
                            .child(format!("{}'s browser", agent.name)),
                    )
                    .child(
                        div()
                            .id("browser-close")
                            .p(px(space::XS))
                            .rounded(px(radius::ROW))
                            .cursor_pointer()
                            .text_color(p.ink_3)
                            .hover(move |s| s.bg(p.hover).text_color(p.ink))
                            .child(Icon::new(Ico::X).small())
                            .on_click(cx.listener(|this, _, _, cx| this.watch(None, cx))),
                    ),
            )
            .child(screen)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::M))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(size::SMALL))
                            .text_color(if person || !state.asked.is_empty() {
                                p.ink
                            } else {
                                p.ink_3
                            })
                            .child(words),
                    )
                    .child(hand),
            )
    }

    /// Sends what the person typed for the agent's browser_ask_secret, or says
    /// they won't, and clears the box either way.
    fn give_secret(&mut self, give: bool, window: &mut Window, cx: &mut Context<Self>) {
        let value = if give {
            self.secret_in.read(cx).value().to_string()
        } else {
            String::new()
        };
        if give && value.is_empty() {
            return;
        }
        if let Some(f) = self.model.give_secret(&value) {
            self.send(f);
        }
        self.secret_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Starts editing a queued message in place.
    fn edit_queued(
        &mut self,
        item: &model::QueueItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.queue_editing = Some(item.id.clone());
        let text = item.text.clone();
        self.queue_in.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Saves the queued message being edited; empty text keeps what it was.
    fn save_queued(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.queue_editing.take() else {
            return;
        };
        let text = self.queue_in.read(cx).value().trim().to_string();
        if !text.is_empty() {
            let f = self.model.queue_cmd("queue.edit", &id, Some(&text));
            self.send(f);
        }
        cx.notify();
    }

    /// What waits for the agent in this conversation, above the composer:
    /// each one can go now (steer), move up, be edited or be dropped.
    fn queue_view(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Option<Div> {
        let items = self.model.queue();
        if items.is_empty() {
            return None;
        }
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        let mut list = div()
            .w_full()
            .max_w(px(space::COLUMN))
            .flex()
            .flex_col()
            .gap(px(space::XS));
        for (k, item) in items.iter().enumerate() {
            let editing = self.queue_editing.as_deref() == Some(item.id.as_str());
            let tool = |name: &str, icon: Icon| {
                div()
                    .id(SharedString::from(format!("queue-{name}-{}", item.id)))
                    .size(px(26.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::ROW))
                    .text_color(p.ink_3)
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover).text_color(p.ink))
                    .child(icon.xsmall())
            };
            let (steer_id, up_id, edit_item, drop_id) = (
                item.id.clone(),
                item.id.clone(),
                item.clone(),
                item.id.clone(),
            );
            let text = if item.files > 0 {
                let files = if item.files == 1 {
                    "1 file".to_string()
                } else {
                    format!("{} files", item.files)
                };
                if item.text.is_empty() {
                    files
                } else {
                    format!("{} · {files}", item.text)
                }
            } else {
                item.text.clone()
            };
            let row = div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(space::S))
                .pl(px(space::M))
                .pr(px(space::XS))
                .py(px(space::XS))
                .rounded(px(radius::ROW))
                .bg(p.bubble)
                .text_size(px(size::SMALL))
                .child(Icon::new(Ico::Clock).xsmall().text_color(tint))
                .child(if editing {
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.queue_in))
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(p.ink)
                        .child(text)
                        .into_any_element()
                })
                .child(tool("steer", Icon::new(Ico::Zap)).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let f = this.model.queue_cmd("queue.steer", &steer_id, None);
                        this.send(f);
                        cx.notify();
                    },
                )))
                .when(k > 0, |d| {
                    d.child(
                        tool("up", Icon::new(IconName::ArrowUp)).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(f) = this.model.queue_up(&up_id) {
                                    this.send(f);
                                }
                                cx.notify();
                            },
                        )),
                    )
                })
                // The first can't go higher; its buttons still line up.
                .when(k == 0, |d| d.child(div().size(px(26.0)).flex_none()))
                .child(if editing {
                    tool("save", Icon::new(Ico::Check))
                        .on_click(cx.listener(|this, _, window, cx| this.save_queued(window, cx)))
                } else {
                    tool("edit", Icon::new(Ico::Pencil)).on_click(cx.listener(
                        move |this, _, window, cx| this.edit_queued(&edit_item, window, cx),
                    ))
                })
                .child(
                    tool("drop", Icon::new(IconName::Close)).on_click(cx.listener(
                        move |this, _, _, cx| {
                            let f = this.model.queue_cmd("queue.drop", &drop_id, None);
                            this.send(f);
                            cx.notify();
                        },
                    )),
                );
            list = list.child(row);
        }
        Some(
            div()
                .w_full()
                .flex()
                .justify_center()
                .px(px(space::XL))
                .pb(px(space::XS))
                .child(list),
        )
    }

    /// The agent asking for something only the person should know, typed
    /// straight into its page: a masked box above the composer.
    fn secret_ask(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.model.browser(&agent.id);
        if state.secret.is_empty() {
            return None;
        }
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        Some(
            div()
                .w_full()
                .flex()
                .justify_center()
                .px(px(space::XL))
                .pb(px(space::S))
                .child(
                    div()
                        .w_full()
                        .max_w(px(space::COLUMN))
                        .flex()
                        .flex_col()
                        .gap(px(space::S))
                        .px(px(space::L))
                        .py(px(space::M))
                        .rounded(px(radius::BUTTON))
                        .border_1()
                        .border_color(tint)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(space::S))
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(Icon::new(Ico::ShieldCheck).small().text_color(tint))
                                .child(state.secret.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(space::S))
                                .child(div().flex_1().child(Input::new(&self.secret_in)))
                                .child(self.button("secret-skip", "Not now", false).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.give_secret(false, window, cx)
                                    }),
                                ))
                                .child(self.button("secret-give", "Fill it in", true).on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.give_secret(true, window, cx)
                                    }),
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(format!(
                                    "It goes into the page. {} never sees it.",
                                    agent.name
                                )),
                        ),
                ),
        )
    }

    /// The agent asking for the person in its browser, above the composer while
    /// its browser isn't showing.
    fn browser_ask(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.model.browser(&agent.id);
        if state.asked.is_empty() || self.model.watching.as_deref() == Some(agent.id.as_str()) {
            return None;
        }
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        let id = agent.id.clone();
        Some(
            div()
                .w_full()
                .flex()
                .justify_center()
                .px(px(space::XL))
                .pb(px(space::S))
                .child(
                    div()
                        .w_full()
                        .max_w(px(space::COLUMN))
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .px(px(space::L))
                        .py(px(space::M))
                        .rounded(px(radius::BUTTON))
                        .border_1()
                        .border_color(tint)
                        .child(Icon::new(Ico::Globe).small().text_color(tint))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(state.asked.clone()),
                        )
                        .child(self.button("browser-open", "Open browser", true).on_click(
                            cx.listener(move |this, _, _, cx| this.watch(Some(id.clone()), cx)),
                        )),
                ),
        )
    }

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

    /// The first screen: Start runs loom on this computer, and a server is one step aside.
    fn sign_in(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let column = div().w(px(340.0)).flex().flex_col().items_center();
        // Every thread colour, woven through the name. The threads draw in from the left,
        // then the name settles among them, then the rest. The band stays put when the
        // server form takes the place of what's under it.
        let threads: Vec<Hsla> = COLORS.iter().map(|c| thread::color(c, p.dark)).collect();
        let over = threads.clone();
        let (mark, half) = wordmark(p, 72.0);
        let reveal = self.enter(0.15, 1.3);
        let word = self.enter(0.7, 0.6);
        let line = self.enter(1.1, 0.5);
        let buttons = self.enter(1.3, 0.5);
        let swapped = self.form_at.elapsed().as_secs_f32();
        let swap = |delay: f32| {
            let x = ((swapped - delay) / 0.35).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let under = if self.server_form {
            appear(self.server_sign_in(column, cx), swap(0.0))
        } else {
            column
                .gap(px(space::XL))
                .child(appear(
                    div()
                        .text_size(px(size::HEADING))
                        .text_color(p.ink_2)
                        .child("Little agents that live with you"),
                    line.min(swap(0.0)),
                ))
                .child(
                    appear(
                        div()
                            .w_full()
                            .pt(px(space::L))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(space::L))
                            .child(
                                div()
                                    .id("local")
                                    .w_full()
                                    .h(px(50.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(space::S))
                                    .rounded(px(radius::BUTTON))
                                    .bg(p.button)
                                    .text_color(p.button_ink)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .child("Let's begin")
                                    .child(Icon::new(IconName::ArrowRight).small())
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.set_local(true, cx)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("use-server")
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink_3)
                                    .cursor_pointer()
                                    .hover(move |s| s.text_color(p.ink))
                                    .child("I have a server")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.server_form = true;
                                        this.form_at = Instant::now();
                                        cx.notify();
                                    })),
                            ),
                        buttons.min(swap(0.0)),
                    )
                    .w_full(),
                )
                .when_some(self.model.login_error.clone(), |d, e| {
                    d.child(div().text_size(px(size::SMALL)).text_color(p.bad).child(e))
                })
        };
        let woven = div()
            .relative()
            .w_full()
            .h(px(190.0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(190.0))
                    .child(thread::weave(threads, 0.0, reveal, 190.0)),
            )
            .child(appear(mark, word))
            // The threads that cross over the mark, so they weave through it.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(190.0))
                    .opacity(word)
                    .child(thread::weave_over(over, 0.0, reveal, 190.0, half)),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(p.bg)
            .child(woven)
            // A fixed height under the band, so swapping what's in it doesn't move it.
            .child(div().h(px(340.0)).pt(px(space::L)).child(under))
    }

    /// Signs in to the server in the form, unless that's already under way.
    fn sign_in_server(&mut self, cx: &mut Context<Self>) {
        if self.model.logging_in {
            return;
        }
        let server = self.server_in.read(cx).value().to_string();
        let user = self.user_in.read(cx).value().to_string();
        let pass = self.pass_in.read(cx).value().to_string();
        let code = if self.show_code {
            self.code_in.read(cx).value().to_string()
        } else {
            String::new()
        };
        let frame = self.model.login(&server, &user, &pass, &code);
        self.send(frame);
        cx.notify();
    }

    /// Signing in to an instance on a server: its address, the account, and for a new
    /// server, the setup code it printed.
    fn server_sign_in(&mut self, column: Div, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let busy = self.model.logging_in;
        let input = |s: &Entity<InputState>| div().w_full().child(Input::new(s).large());
        column
            .gap(px(space::M))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pb(px(space::M))
                    .child(
                        div()
                            .id("back")
                            .size(px(30.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::BUTTON))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover).text_color(p.ink))
                            .child(Icon::new(Ico::ArrowLeft).small())
                            .on_click(cx.listener(|this, _, _, cx| {
                                // A server being added goes again, back to the one before.
                                if !this.here.is_empty() {
                                    this.remove_server(cx);
                                    return;
                                }
                                this.server_form = false;
                                this.form_at = Instant::now();
                                this.model.login_error = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(size::HEADING))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .child("Your server"),
                    ),
            )
            .child(input(&self.server_in))
            .child(input(&self.user_in))
            .child(input(&self.pass_in))
            .when(self.show_code, |d| d.child(input(&self.code_in)))
            .when_some(self.model.login_error.clone(), |d, e| {
                d.child(
                    div()
                        .w_full()
                        .text_size(px(size::SMALL))
                        .text_color(p.bad)
                        .child(e),
                )
            })
            .child(
                div().w_full().pt(px(space::S)).child(
                    self.button(
                        "sign-in",
                        if busy { "Signing in…" } else { "Sign in" },
                        true,
                    )
                    .w_full()
                    .h(px(46.0))
                    .on_click(cx.listener(|this, _, _, cx| this.sign_in_server(cx))),
                ),
            )
            .when(!self.show_code, |d| {
                d.child(
                    div()
                        .id("new-server")
                        .pt(px(space::S))
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_3)
                        .cursor_pointer()
                        .hover(move |s| s.text_color(p.ink))
                        .child("New server? Add its setup code")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_code = true;
                            cx.notify();
                        })),
                )
            })
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
                                format!("hit-{i}"),
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
        // With several servers, the agents are under their server's name.
        if !self.away.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::M))
                    .pt(px(space::XS))
                    .pb(px(space::S))
                    .text_size(px(size::CAPTION))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink_3)
                    .child(spool(self.spool_state(), self.spool_ink(), 22.0, 11.0))
                    .child(div().truncate().child(server_name(&self.model, &self.here))),
            );
        }
        for a in self.model.agents.clone() {
            let selected = self.model.selected.as_deref() == Some(a.id.as_str())
                && self.screen == Screen::Chat;
            let mood = self.model.mood(&a.id);
            let preview = self.model.preview(&a);
            // A reply that came while its chat wasn't open: the row is bold until it is.
            let unread = self.model.unread.contains(&a.id);
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
                        format!("side-{}", a.id),
                        mood,
                        thread::color(&a.color, p.dark),
                        26.0,
                        10.0,
                        self.model.finished_at(&a.id),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(size::BODY))
                                    .font_weight(if unread {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    })
                                    .text_color(p.ink)
                                    .truncate()
                                    .child(a.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .when(unread, |d| d.font_weight(FontWeight::MEDIUM))
                                    .text_color(if mood == Mood::Failed {
                                        p.bad
                                    } else if unread {
                                        p.ink
                                    } else {
                                        p.ink_3
                                    })
                                    .truncate()
                                    .when(!preview.is_empty(), |d| d.child(preview)),
                            ),
                    )
                    .when(a.pinned.is_some(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(p.ink_3)
                                .child(Icon::new(Ico::Pin).xsmall()),
                        )
                    }),
            );
        }
        let server_open = self.popup.is_some_and(is_server);
        let settings_open = self.popup.is_some() && !server_open;
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
            .relative()
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
                    // The knot, and the wordmark beside it (design/logo/README.md).
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                svg()
                                    .path("logo/knot.svg")
                                    .size(px(20.0))
                                    .flex_none()
                                    .text_color(p.ink),
                            )
                            .child(
                                div()
                                    .text_size(px(size::BODY))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.ink)
                                    .child("loom"),
                            ),
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
                            .on_click(
                                cx.listener(|this, _, _, cx| this.open(Screen::NewAgent, cx)),
                            ),
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
                    .when(searching, |d| d.child("Found")),
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
            // The server, as its spool, on the left, and the settings on the right: each
            // opens the popup.
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .justify_between()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .py(px(space::S))
                    .child(self.spools(server_open, cx))
                    .child(
                        div()
                            .id("settings")
                            .w(px(40.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .border_1()
                            .border_color(if settings_open { p.line_strong } else { p.line })
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .when(settings_open, |d| d.bg(p.active))
                            .hover(move |s| s.bg(p.hover).text_color(p.ink))
                            .child(Icon::new(Ico::Settings).small())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let last = this.settings_last;
                                this.open(last, cx)
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
            return bar;
        }
        let permission = match agent.permission.as_str() {
            "allow" => "Doesn't ask",
            "auto" => "Asks when it's risky",
            _ => "Asks first",
        };
        // The agent's browser, while it's open: a globe that shows it beside the chat.
        let browsing = self.model.browser(&agent.id).open;
        let watching = self.model.watching.as_deref() == Some(agent.id.as_str());
        let tint = thread::color(&agent.color, p.dark);
        let globe = div()
            .id("browser-toggle")
            .occlude()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::ROW))
            .text_size(px(size::CAPTION))
            .text_color(if watching { tint } else { p.ink_2 })
            .when(watching, |d| d.bg(p.hover))
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(Icon::new(Ico::Globe).small())
            .child("Browser")
            .on_click(cx.listener(move |this, _, _, cx| {
                let id = this.model.selected.clone();
                let next = if this.model.watching == id { None } else { id };
                this.watch(next, cx);
            }));
        // The chat's pins, while it has some, or they're open beside it.
        let pinned = self.model.messages.iter().filter(|m| m.pinned).count();
        let pins_open = self.side == Some(Screen::Pins);
        let pins = div()
            .id("pins-toggle")
            .occlude()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::ROW))
            .text_size(px(size::CAPTION))
            .text_color(if pins_open { p.ink } else { p.ink_2 })
            .when(pins_open, |d| d.bg(p.hover))
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .child(Icon::new(Ico::Pin).small())
            .when(pinned > 0, |d| d.child(pinned.to_string()))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.side == Some(Screen::Pins) {
                    this.close_side(cx);
                } else {
                    this.open(Screen::Pins, cx);
                }
            }));
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
                    "head",
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
                .when(pinned > 0 || pins_open, |d| d.child(pins))
                .when(browsing || watching, |d| d.child(globe))
                .when_some(agent.picked(), |d, picked| {
                    d.child(
                        div()
                            .id("model-menu")
                            .occlude()
                            .px(px(space::S))
                            .py(px(space::XS))
                            .rounded(px(radius::ROW))
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .hover(move |s| s.bg(p.hover))
                            .when(self.menu == Some(Menu::Model), |d| d.bg(p.hover))
                            .flex()
                            .items_center()
                            .gap(px(space::XS))
                            .child(picked)
                            .child(
                                Icon::new(IconName::ChevronDown)
                                    .xsmall()
                                    .text_color(p.ink_3),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.menu = if this.menu == Some(Menu::Model) {
                                    None
                                } else {
                                    Some(Menu::Model)
                                };
                                cx.notify();
                            })),
                    )
                })
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

    /// A menu entry: its icon and its title.
    fn menu_item(
        &self,
        id: impl Into<SharedString>,
        icon: Ico,
        title: &str,
        danger: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id.into())
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .text_size(px(size::SMALL))
            .text_color(if danger { p.bad } else { p.ink })
            .hover(move |s| s.bg(p.hover))
            .child(
                div()
                    .text_color(if danger { p.bad } else { p.ink_2 })
                    .child(Icon::new(icon).small()),
            )
            .child(div().flex_1().child(title.to_string()))
    }

    /// The composer's + menu: attach files, start the message with one of the
    /// person's skills, or run one of this agent's routines now.
    fn plus_menu(&self, which: Plus, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let panel = div()
            .occlude()
            .mb(px(space::S))
            .w(px(240.0))
            .max_h(px(320.0))
            .p(px(space::XS))
            .flex()
            .flex_col()
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.plus = None;
                cx.notify();
            }));
        let back = |this: &Self, cx: &mut Context<Self>| {
            this.menu_item("plus-back", Ico::ArrowLeft, "Back", false)
                .text_color(this.p.ink_2)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.plus = Some(Plus::Main);
                    cx.notify();
                }))
        };
        let none = |text: &'static str| {
            div()
                .px(px(space::M))
                .py(px(space::S))
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child(text)
        };
        match which {
            Plus::Main => panel
                .child(
                    self.menu_item("plus-attach", Ico::Paperclip, "Attach files", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.plus = None;
                            this.pick_files(cx);
                            cx.notify();
                        })),
                )
                .child(
                    self.menu_item("plus-skill", Ico::BookOpen, "Use a skill", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let f = this.model.load_skills();
                            this.send(f);
                            this.plus = Some(Plus::Skills);
                            cx.notify();
                        })),
                )
                .child(
                    self.menu_item("plus-routine", Ico::Clock, "Run a routine", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let f = this.model.load_routines();
                            this.send(f);
                            this.plus = Some(Plus::Routines);
                            cx.notify();
                        })),
                ),
            Plus::Skills => {
                let mut list = div()
                    .id("plus-skills")
                    .flex()
                    .flex_col()
                    .overflow_y_scroll();
                if self.model.skills.is_empty() {
                    list = list.child(none("No skills yet"));
                }
                for s in self.model.skills.clone() {
                    let name = s.name.clone();
                    list = list.child(
                        self.menu_item(
                            format!("plus-skill-{}", s.name),
                            Ico::BookOpen,
                            &s.name,
                            false,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let text = this.composer.read(cx).value().to_string();
                                let next = model::with_skill(&name, &text);
                                this.composer.update(cx, |c, cx| {
                                    c.set_value(next, window, cx);
                                    c.focus(window, cx);
                                });
                                this.plus = None;
                                cx.notify();
                            },
                        )),
                    );
                }
                panel.child(back(self, cx)).child(list)
            }
            Plus::Routines => {
                let agent = self.model.selected.clone().unwrap_or_default();
                let mine: Vec<model::Routine> = self
                    .model
                    .routines
                    .iter()
                    .filter(|r| r.agent == agent)
                    .cloned()
                    .collect();
                let mut list = div()
                    .id("plus-routines")
                    .flex()
                    .flex_col()
                    .overflow_y_scroll();
                if mine.is_empty() {
                    list = list.child(none("No routines for this agent"));
                }
                for r in mine {
                    let id = r.id.clone();
                    list = list.child(
                        self.menu_item(
                            format!("plus-routine-{}", r.id),
                            Ico::Clock,
                            &r.name,
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.run_routine(&id);
                            this.send(f);
                            this.plus = None;
                            cx.notify();
                        })),
                    );
                }
                panel.child(back(self, cx)).child(list)
            }
        }
    }

    /// A menu entry for one of the permission modes, ticked when it's the agent's.
    fn mode_item(&self, id: &'static str, icon: Ico, title: &str, mode: &str) -> Stateful<Div> {
        let p = self.p;
        let on = self
            .model
            .selected
            .as_ref()
            .and_then(|a| self.model.agent(a))
            .is_some_and(|a| a.permission == mode);
        self.menu_item(id, icon, title, false).when(on, |d| {
            d.child(
                div()
                    .text_color(p.ink_2)
                    .child(Icon::new(Ico::Check).small()),
            )
        })
    }

    /// The model picker's list: each option the agent's program offers (its model,
    /// then its reasoning effort) under its name, the value it's on ticked.
    fn model_picks(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col();
        for (n, o) in agent.picks().into_iter().enumerate() {
            let on = agent.chosen(&o.id);
            list = list.child(
                div()
                    .px(px(space::M))
                    .pt(px(if n == 0 { space::XS } else { space::M }))
                    .pb(px(space::XS))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(o.name.clone()),
            );
            for (value, name) in o.values.clone() {
                let (agent_id, option) = (agent.id.clone(), o.id.clone());
                let picked = value == on;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("pick-{}-{value}", o.id)))
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .px(px(space::M))
                        .py(px(space::S))
                        .rounded(px(radius::ROW))
                        .cursor_pointer()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink)
                        .hover(move |s| s.bg(p.hover))
                        .child(div().flex_1().child(name))
                        .when(picked, |d| {
                            d.child(
                                div()
                                    .text_color(p.ink_2)
                                    .child(Icon::new(Ico::Check).small()),
                            )
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(f) = this.model.set_config(&agent_id, &option, &value) {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        list
    }

    fn menu_panel(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.p;
        let menu = self.menu?;
        let panel = div()
            .absolute()
            .top(px(layout::TITLEBAR - 4.0))
            .w(px(230.0))
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
        let pinned = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .is_some_and(|a| a.pinned.is_some());
        Some(match menu {
            Menu::Agent => panel
                .left(px(space::S))
                .child(
                    self.menu_item(
                        "pin-agent",
                        if pinned { Ico::PinOff } else { Ico::Pin },
                        if pinned { "Unpin" } else { "Pin to top" },
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.model.pin_agent(!pinned) {
                            this.send(f);
                        }
                        this.menu = None;
                        cx.notify();
                    })),
                )
                .child(
                    self.menu_item("edit", Ico::Pencil, "Edit agent", false)
                        .on_click(cx.listener(|this, _, window, cx| {
                            for f in this.model.load_mcp() {
                                this.send(f);
                            }
                            for f in this.model.edit_agent() {
                                this.send(f);
                            }
                            if let Some(a) = this
                                .model
                                .editing
                                .as_ref()
                                .and_then(|id| this.model.agent(id))
                            {
                                let (name, instructions) = (a.name.clone(), a.instructions.clone());
                                this.name_in
                                    .update(cx, |s, cx| s.set_value(name, window, cx));
                                this.instructions_in
                                    .update(cx, |s, cx| s.set_value(instructions, window, cx));
                            }
                            this.show_side(Screen::EditAgent, cx);
                            this.menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.menu_item("activity", Ico::Activity, "Activity", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.load_activity() {
                                this.send(f);
                            }
                            this.show_side(Screen::Activity, cx);
                            this.menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.menu_item("memory", Ico::Brain, "Memory", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.model.notes.clear();
                            if let Some(f) = this.model.load_notes() {
                                this.send(f);
                            }
                            this.show_side(Screen::Memory, cx);
                            this.menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.menu_item(
                        "aside",
                        Ico::MessageCircleQuestionMark,
                        "Ask on the side",
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_side(Screen::Aside, cx);
                        this.menu = None;
                        cx.notify();
                    })),
                )
                .child(
                    self.menu_item("fresh", Ico::RotateCcw, "Start fresh", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.start_fresh() {
                                this.send(f);
                            }
                            this.menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.menu_item("delete", Ico::Trash, "Delete agent", true)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.delete_agent() {
                                this.send(f);
                            }
                            this.menu = None;
                            cx.notify();
                        })),
                ),
            Menu::Model => {
                let agent = self
                    .model
                    .selected
                    .as_ref()
                    .and_then(|id| self.model.agent(id))
                    .cloned()?;
                panel
                    .right(px(space::S))
                    .child(self.model_picks(&agent, cx))
            }
            Menu::Permission => panel
                .right(px(space::S))
                .child(
                    self.mode_item("ask", Ico::Hand, "Ask first", "ask")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.set_permission("ask") {
                                this.send(f);
                            }
                            this.menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.mode_item("auto", Ico::ShieldCheck, "Ask when it's risky", "auto")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(f) = this.model.set_permission("auto") {
                                this.send(f);
                            }
                            this.menu = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.mode_item("allow", Ico::Zap, "Don't ask", "allow")
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
        // A new account starts by picking who to talk to, once the instance has said it has no agents.
        if self.model.agents.is_empty() || self.make.step == Step::Weaving {
            if self.model.synced {
                return self.first_agent(cx);
            }
            return div().flex_1();
        }
        let rows = self.rows.clone();
        let agent = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .cloned();
        // A browser shows beside its own agent's chat only.
        if self.model.watching.is_some() && self.model.watching != self.model.selected {
            self.watch(None, cx);
        }
        // Beside the chat: the agent's browser, or an agent's page or a plugin's panel.
        let watched = agent
            .clone()
            .filter(|a| self.model.watching.as_deref() == Some(a.id.as_str()));
        let panel = match (watched, self.side) {
            (Some(a), _) => Some(self.browser_panel(&a, window, cx)),
            (None, Some(side)) => Some(self.side_panel(side, window, cx)),
            (None, None) => None,
        };
        let ask = agent.as_ref().and_then(|a| self.browser_ask(a, cx));
        let secret = agent.as_ref().and_then(|a| self.secret_ask(a, cx));
        let queue = agent.as_ref().and_then(|a| self.queue_view(a, cx));
        // Named "Branch" until the list of conversations has it.
        let branch_title = self.model.open_branch.as_ref().map(|_| {
            self.model
                .branch_open()
                .map(|c| c.title.clone())
                .unwrap_or_else(|| "Branch".into())
        });
        let strip = match (
            self.model.task_open().cloned(),
            agent.as_ref(),
            branch_title,
        ) {
            (Some(t), Some(a), _) => Some(self.task_strip(&t, thread::color(&a.color, p.dark), cx)),
            (None, Some(a), Some(title)) => {
                let routine = self
                    .model
                    .branch_open()
                    .is_some_and(|c| c.routine.is_some());
                Some(self.branch_strip(title, routine, thread::color(&a.color, p.dark), cx))
            }
            _ => None,
        };
        // Where each message's branches go, for the links under it.
        let branches = Rc::new(self.model.branches());
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
        let new_line = self.model.new_line.clone();
        let list = MessageScroller::new("chat", self.scroller.clone(), move |ix, _, _| {
            let Some(row) = rows.get(ix) else {
                return div().into_any_element();
            };
            // Above the first reply that came while the chat was shut: a line, NEW
            // in the agent's colour.
            let is_new =
                matches!(row, Row::Message(m) if new_line.as_deref() == Some(m.id.as_str()));
            let divider = div()
                .w_full()
                .max_w(px(space::COLUMN))
                .flex()
                .items_center()
                .gap(px(space::S))
                .pt(px(space::S))
                .child(div().flex_1().h(px(1.0)).bg(tint.opacity(0.5)))
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(tint)
                        .child("NEW"),
                );
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
            // A message's pin, just outside its corner: there while it's pinned, and
            // on hover to pin it.
            let pin = match row {
                Row::Message(m) if !m.live && !m.queued => Some((m.id.clone(), m.pinned)),
                _ => None,
            };
            // Under it, a link to each branch that goes on from it; and beside it, on
            // hover, a way to branch there.
            let gone_on = match row {
                Row::Message(m) => branches.get(&m.id).cloned().unwrap_or_default(),
                _ => Vec::new(),
            };
            let branch_at = pin.as_ref().map(|(id, _)| id.clone());
            // Its time, just outside its left edge while the pointer is over it: the
            // chat has no time separators.
            let when = match row {
                Row::Message(m) if !m.live && !m.queued => {
                    Some(when_sent(m.at, chrono::Local::now())).filter(|w| !w.is_empty())
                }
                _ => None,
            };
            let this = this.clone();
            let to_branch = this.clone();
            let mut links = div().flex().flex_col().gap(px(2.0)).pt(px(space::XS));
            for (i, (id, title)) in gone_on.into_iter().enumerate() {
                let this = this.clone();
                links = links.child(
                    div()
                        .id(SharedString::from(format!("branch-link-{ix}-{i}")))
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .cursor_pointer()
                        .hover(move |s| s.text_color(p.ink))
                        .child(Icon::new(Ico::GitBranch).xsmall())
                        .child(title)
                        .on_click(move |_, _, cx| {
                            let _ =
                                this.update(cx, |loom, cx| loom.open_branch(Some(id.clone()), cx));
                        }),
                );
            }
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_center()
                .px(px(space::XL))
                .when(is_new, |d| d.child(divider))
                .child(
                    div()
                        .w_full()
                        .max_w(px(space::COLUMN))
                        .py(px(space::S))
                        .relative()
                        .group("row")
                        .when_some(when, |d, when| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(space::S + 4.0))
                                    .right(relative(1.0))
                                    .pr(px(space::M))
                                    .whitespace_nowrap()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .opacity(0.0)
                                    .group_hover("row", |s| s.opacity(1.0))
                                    .child(when),
                            )
                        })
                        .child(inner)
                        .child(links)
                        .when_some(branch_at, |d, id| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("branch-row-{id}")))
                                    .absolute()
                                    .top(px(space::S + 28.0))
                                    .right(px(-30.0))
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(radius::ROW))
                                    .cursor_pointer()
                                    .text_color(p.ink_3)
                                    .opacity(0.0)
                                    .group_hover("row", |s| s.opacity(1.0))
                                    .hover(move |s| s.bg(p.hover).text_color(p.ink))
                                    .child(Icon::new(Ico::GitBranch).xsmall())
                                    .on_click(move |_, _, cx| {
                                        let _ = to_branch.update(cx, |loom, _| {
                                            let f = loom.model.branch(&id);
                                            loom.send(f);
                                        });
                                    }),
                            )
                        })
                        .when_some(pin, |d, (id, pinned)| {
                            d.child(
                                div()
                                    .id(SharedString::from(format!("pin-row-{id}")))
                                    .absolute()
                                    .top(px(space::S))
                                    .right(px(-30.0))
                                    .size(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(radius::ROW))
                                    .cursor_pointer()
                                    .text_color(if pinned { p.ink_2 } else { p.ink_3 })
                                    .when(!pinned, |d| {
                                        d.opacity(0.0).group_hover("row", |s| s.opacity(1.0))
                                    })
                                    .hover(move |s| s.bg(p.hover).text_color(p.ink))
                                    .child(Icon::new(Ico::Pin).xsmall())
                                    .on_click(move |_, _, cx| {
                                        let _ = this.update(cx, |loom, _| {
                                            let f = loom.model.pin(&id, !pinned);
                                            loom.send(f);
                                        });
                                    }),
                            )
                        }),
                )
                .into_any_element()
        })
        .with_bottom_fade(p.bg);

        let empty = self.rows.is_empty();
        // A task or a branch shows its strip over the list, even before it has messages.
        let in_task = self.model.task_open().is_some() || self.model.open_branch.is_some();
        // While the agent answers, the button stops it, unless there's something to send: that waits in the queue.
        let busy = self.model.busy()
            && self.composer.read(cx).value().trim().is_empty()
            && self.model.attachments.is_empty();
        let who = agent
            .as_ref()
            .map(|a| a.name.as_str())
            .unwrap_or("the agent");
        let placeholder = if empty {
            format!("Ask {who} anything")
        } else {
            format!("Reply to {who}")
        };
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
                    .child(Icon::new(Ico::FileText).small().text_color(p.ink_3))
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
        let plus_open = self.plus.is_some();
        let plus_menu = self.plus.map(|which| self.plus_menu(which, cx));
        let mut composer = Some(
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
                                .flex_none()
                                .child(
                                    div()
                                        .id("plus")
                                        .size(px(32.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(radius::BUTTON))
                                        .text_color(p.ink_2)
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(p.hover))
                                        .when(plus_open, |d| d.bg(p.hover))
                                        .child(Icon::new(Ico::Plus).small())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.plus = if this.plus.is_some() {
                                                None
                                            } else {
                                                Some(Plus::Main)
                                            };
                                            cx.notify();
                                        })),
                                )
                                .when_some(plus_menu, |d, menu| {
                                    d.child(deferred(
                                        anchored().anchor(Anchor::BottomLeft).child(menu),
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(size::COMPOSER))
                                // Before the textarea's own Paste, which would
                                // paste a copied file as its path.
                                .capture_action(cx.listener(|this, _: &Paste, _, cx| {
                                    if this.paste_files(cx) {
                                        cx.stop_propagation();
                                    }
                                }))
                                .child(Textarea::new(&self.composer).appearance(false)),
                        )
                        // Dictation: the system's own voice typing, into the composer.
                        .when(cfg!(windows), |d| {
                            d.child(
                                div()
                                    .id("dictate")
                                    .size(px(32.0))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(radius::BUTTON))
                                    .text_color(p.ink_2)
                                    .cursor_pointer()
                                    .hover(move |s| s.bg(p.hover))
                                    .child(Icon::new(Ico::Mic).small())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.composer.update(cx, |c, cx| c.focus(window, cx));
                                        if !shell::dictate() {
                                            this.model.notices.push(
                                                "Voice typing didn't start. Press Windows+H in the message box."
                                                    .into(),
                                            );
                                            this.take_notice(cx);
                                        }
                                    })),
                            )
                        })
                        .child(
                            div()
                                .id("send")
                                .size(px(32.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(radius::BUTTON))
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
                                    let text = this.composer.read(cx).value().to_string();
                                    if let Some(frame) = this.model.send(&text) {
                                        this.send(frame);
                                        this.composer
                                            .update(cx, |s, cx| s.set_value("", window, cx));
                                    }
                                })),
                        ),
                ),
        );
        let column = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.attach(paths.paths(), cx);
            }))
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(p.hover))
            .when_some(strip, |d, s| d.child(s))
            .child(if empty && !in_task {
                self.hero(agent.as_ref(), composer.take(), cx)
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
            .when_some(ask, |d, a| d.child(a))
            .when_some(secret, |d, a| d.child(a))
            .when_some(queue, |d, q| d.child(q))
            .when_some(composer, |d, c| {
                d.child(
                    div()
                        .w_full()
                        .flex()
                        .justify_center()
                        .px(px(space::XL))
                        .pb(px(space::L))
                        .pt(px(space::XS))
                        .child(c),
                )
            });
        match panel {
            Some(panel) => div().flex_1().min_h_0().flex().child(column).child(panel),
            None => column,
        }
    }

    /// The first screen of a new account: making its first agent.
    fn first_agent(&mut self, cx: &mut Context<Self>) -> Div {
        if !self.asked_programs && self.model.synced {
            self.asked_programs = true;
            let f = self.model.programs();
            self.send(f);
        }
        self.making(true, cx)
    }

    /// Making an agent, in three steps: a brain (the agent program it runs on), then a
    /// name and a colour, then a moment while the threads weave it together.
    fn making(&mut self, first: bool, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let step = self.make.step;
        if step != Step::Weaving {
            self.make.first = first;
        }
        let back = step == Step::Name || (step == Step::Brain && !first);
        let body = match step {
            Step::Brain => self.brain_step(cx),
            Step::Name => self.name_step(cx),
            Step::Weaving => self.weaving_step(),
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::XXL))
            .when(step != Step::Weaving, |d| d.pb(px(60.0)))
            .when(back, |d| {
                d.child(
                    div()
                        .id("make-back")
                        .absolute()
                        .top(px(space::L))
                        .left(px(space::L))
                        .size(px(34.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::BUTTON))
                        .text_color(p.ink_2)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover).text_color(p.ink))
                        .child(Icon::new(IconName::ArrowLeft).small())
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.make.step == Step::Name {
                                this.make.step = Step::Brain;
                            } else {
                                this.screen = Screen::Chat;
                            }
                            cx.notify();
                        })),
                )
            })
            .child(body)
            .when(step != Step::Weaving, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom(px(48.0))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(self.step_marks(step)),
                )
            })
    }

    /// Where making an agent is up to: a short thread per step, moving on the current one.
    fn step_marks(&self, step: Step) -> Div {
        let p = self.p;
        let tint = thread::color(self.make.color(), p.dark);
        let grey = thread::color("grey", p.dark);
        let marks = [Step::Brain, Step::Name];
        let mut row = div().flex().items_center().gap(px(space::M));
        for (i, s) in marks.iter().enumerate() {
            let now = *s == step;
            let done = i < marks.iter().position(|m| *m == step).unwrap_or(0);
            let (mood, color) = if now {
                (Mood::Writing, tint)
            } else if done {
                (Mood::Idle, tint)
            } else {
                (Mood::Idle, grey)
            };
            row = row.child(thread(format!("mark-{i}"), mood, color, 34.0, 10.0, None));
        }
        row
    }

    /// The first step: which agent program it runs on, as a tile for each, whose thread
    /// comes alive under the pointer.
    fn brain_step(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let column = div().flex().flex_col().items_center().gap(px(space::XXL));
        let heading = |text: &str| {
            div()
                .text_size(px(size::HERO))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink)
                .child(text.to_string())
        };
        if !self.model.programs_known {
            return column.child(thread(
                "brain-wait",
                Mood::Thinking,
                thread::color("periwinkle", p.dark),
                160.0,
                16.0,
                None,
            ));
        }
        if self.model.programs.is_empty() {
            let greys = (0..4).map(|_| thread::color("grey", p.dark)).collect();
            return column
                .child(
                    div()
                        .w(px(360.0))
                        .child(thread::weave(greys, 0.0, 1.0, 70.0)),
                )
                .child(heading("It needs a brain first"))
                .child(
                    div()
                        .flex()
                        .gap(px(space::M))
                        .child(self.button("get-node", "Get Node.js", true).on_click(
                            cx.listener(|_, _, _, cx| cx.open_url("https://nodejs.org/")),
                        ))
                        .child(
                            self.button("install-program", "Install one", false)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.open(Screen::Programs, cx)),
                                ),
                        ),
                );
        }
        let mut tiles = div().flex().flex_wrap().justify_center().gap(px(space::L));
        // Once one's picked, the rest and the heading go, and the picked one's thread keeps
        // going, to grow into the next step's.
        let leaving = self.make.picked_at.map_or(0.0, |at| {
            let x = (at.elapsed().as_secs_f32() / PICK_SECONDS).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        });
        let chosen = self.make.picked_at.and(self.make.program.clone());
        for (i, name) in self.model.programs.clone().into_iter().enumerate() {
            let color = COLORS[i % COLORS.len()];
            let tint = thread::color(color, p.dark);
            let is_chosen = chosen.as_deref() == Some(name.as_str());
            let lit = is_chosen || self.make.hover.as_deref() == Some(name.as_str());
            let hovered = name.clone();
            let picked = name.clone();
            // The tiles come in one after another.
            let k = if is_chosen {
                1.0
            } else {
                self.enter(0.2 + 0.1 * i as f32, 0.5) * (1.0 - leaving)
            };
            tiles = tiles.child(appear(
                div()
                    .id(SharedString::from(format!("brain-{name}")))
                    .w(px(200.0))
                    .h(px(176.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(space::M))
                    .rounded(px(radius::CARD))
                    .border_1()
                    .cursor_pointer()
                    // The picked tile's card and words go too, leaving its thread.
                    .when(lit, |d| {
                        let stay = if is_chosen { 1.0 - leaving } else { 1.0 };
                        d.bg(p.raised_2.opacity(stay))
                            .border_color(tint.opacity(0.7 * stay))
                    })
                    .when(!lit, |d| d.bg(p.raised).border_color(p.line))
                    .child(thread(
                        format!("brain-{name}"),
                        if lit { Mood::Writing } else { Mood::Idle },
                        tint,
                        120.0,
                        20.0,
                        None,
                    ))
                    .child(appear(
                        div()
                            .pt(px(space::S))
                            .text_size(px(size::HEADING))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .child(program_title(&name)),
                        if is_chosen { 1.0 - leaving } else { 1.0 },
                    ))
                    .child(appear(
                        div()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(program_maker(&name)),
                        if is_chosen { 1.0 - leaving } else { 1.0 },
                    ))
                    .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                        if *over {
                            this.make.hover = Some(hovered.clone());
                        } else if this.make.hover.as_deref() == Some(hovered.as_str()) {
                            this.make.hover = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.make.picked_at.is_some() {
                            return;
                        }
                        this.make.program = Some(picked.clone());
                        this.make.color = color;
                        this.make.picked_at = Some(Instant::now());
                        let current = this.name_in.read(cx).value().to_string();
                        this.name_in.update(cx, |s, cx| {
                            s.set_value(whimsical_name(&current), window, cx);
                            s.focus(window, cx);
                        });
                        cx.notify();
                    })),
                k,
            ));
        }
        column
            .child(appear(
                heading("Pick a brain"),
                self.enter(0.0, 0.5) * (1.0 - leaving),
            ))
            .child(tiles)
    }

    /// The second step: its name, with a die for one, and its colour, shown on its own
    /// thread, which twangs at every change.
    fn name_step(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let tint = thread::color(self.make.color(), p.dark);
        let twang = self
            .make
            .snapped
            .is_some_and(|at| at.elapsed().as_secs_f32() < 1.2);
        let grow = self.enter(0.0, 0.6);
        let mut swatches = div().flex().items_center().gap(px(space::S));
        for (i, color) in COLORS.iter().enumerate() {
            let picked = self.make.color() == *color;
            let dot = thread::color(color, p.dark);
            swatches = swatches.child(
                div()
                    .id(("swatch", i))
                    .size(px(34.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_2()
                    .border_color(if picked { dot } else { transparent_black() })
                    .cursor_pointer()
                    .child(
                        div()
                            .size(px(if picked { 20.0 } else { 18.0 }))
                            .rounded_full()
                            .bg(dot),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.make.color = color;
                        this.make.snapped = Some(Instant::now());
                        cx.notify();
                    })),
            );
        }
        div()
            .w(px(380.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(space::XL))
            // The picked brain's thread, grown to full size; it's still writing as it
            // arrives, and settles into thinking.
            .child(div().h(px(36.0)).flex().items_center().child(thread(
                "name-preview",
                if twang {
                    Mood::Idle
                } else if grow < 1.0 {
                    Mood::Writing
                } else {
                    Mood::Thinking
                },
                tint,
                120.0 + 160.0 * grow,
                20.0 + 16.0 * grow,
                self.make.snapped,
            )))
            .child(appear(
                div()
                    .text_size(px(size::HERO))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink)
                    .child("Name it"),
                self.enter(0.25, 0.4),
            ))
            .child(
                appear(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(div().flex_1().child(Input::new(&self.name_in).large()))
                        .child(
                            div()
                                .id("dice")
                                .size(px(40.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(radius::BUTTON))
                                .text_color(p.ink_2)
                                .cursor_pointer()
                                .hover(move |s| s.bg(p.hover).text_color(p.ink))
                                .child(Icon::new(Ico::Dices))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let current = this.name_in.read(cx).value().to_string();
                                    this.name_in.update(cx, |s, cx| {
                                        s.set_value(whimsical_name(&current), window, cx)
                                    });
                                    this.make.snapped = Some(Instant::now());
                                    cx.notify();
                                })),
                        ),
                    self.enter(0.35, 0.4),
                )
                .w_full(),
            )
            .child(appear(swatches, self.enter(0.45, 0.4)))
            .child(appear(
                div()
                    .id("weave-it")
                    .mt(px(space::S))
                    .h(px(46.0))
                    .px(px(space::XXL))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .rounded(px(radius::BUTTON))
                    .bg(tint)
                    .text_color(p.bg)
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.88))
                    .child("Weave it")
                    .on_click(cx.listener(|this, _, _, cx| this.weave_agent(cx))),
                self.enter(0.55, 0.4),
            ))
    }

    /// Makes the agent and lets the threads weave it while the instance does.
    fn weave_agent(&mut self, cx: &mut Context<Self>) {
        let Some(program) = self.make.program.clone() else {
            return;
        };
        self.model.make_failed = false;
        let typed = self.name_in.read(cx).value().trim().to_string();
        let name = if typed.is_empty() {
            program_title(&program)
        } else {
            typed
        };
        let f = self.model.new_agent(&name, &program, "", self.make.color());
        self.send(f);
        self.make.step = Step::Weaving;
        self.make.started = Some(Instant::now());
        self.make.agents_before = self.model.agents.len();
        self.make.name = name;
        cx.notify();
    }

    /// The last step: every colour's thread pulled in to the new agent's one.
    fn weaving_step(&self) -> Div {
        let p = self.p;
        let chosen = self.make.color();
        let mut colors = vec![thread::color(chosen, p.dark)];
        colors.extend(
            COLORS
                .iter()
                .filter(|c| **c != chosen)
                .map(|c| thread::color(c, p.dark)),
        );
        let t = self
            .make
            .started
            .map_or(0.0, |s| s.elapsed().as_secs_f32() / WEAVE_SECONDS);
        let gather = 1.0 - (1.0 - t.min(1.0)).powi(3);
        // The words sink away as the threads finish meeting, before the window closes.
        let x = ((t * WEAVE_SECONDS - (WEAVE_SECONDS - 0.2)) / 0.45).clamp(0.0, 1.0);
        let leave = x * x * (3.0 - 2.0 * x);
        // The threads sit exactly in the middle of the page, where the curtain that
        // opens the app closes on them (`curtain`); the words hang under them.
        div()
            .relative()
            .w_full()
            .child(thread::weave(
                colors,
                gather,
                self.enter(0.0, 0.7),
                WEAVE_BAND,
            ))
            .child(
                div()
                    .absolute()
                    .top(px(WEAVE_BAND + space::XL))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(appear(
                        div()
                            .text_size(px(size::HEADING))
                            .text_color(p.ink_2)
                            .child(format!("Weaving {}…", self.make.name)),
                        self.enter(0.25, 0.5) - leave,
                    )),
            )
    }

    /// How far something has come in (0 to 1, eased), starting `delay` seconds after the
    /// view it's in appeared and taking `dur`.
    fn enter(&self, delay: f32, dur: f32) -> f32 {
        let t = self.scene.1.elapsed().as_secs_f32();
        let x = ((t - delay) / dur).clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    }

    /// A screen over the window, `h` high, closing on the line `center` down from its top,
    /// `close` of the way (0 to 1): its top and bottom edges meet there, showing what's
    /// under it as they come. `band`, `band_h` high, stays centred on that line.
    fn curtain(&self, h: f32, center: f32, close: f32, band: Div, band_h: f32) -> Div {
        let top = center * close;
        let bottom = (h - center) * close;
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(top))
            .bottom(px(bottom))
            .occlude()
            .overflow_hidden()
            .bg(self.p.bg)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(center - top - band_h / 2.0))
                    .h(px(band_h))
                    .child(band),
            )
    }

    /// The opening weave, over everything for a moment on every launch: the threads come
    /// in from the left, the name rises among them and sinks away, they draw together
    /// into one, and the window closes on that line to open the app.
    fn splash(&self, h: f32) -> Option<Div> {
        let t = self.launched?.elapsed().as_secs_f32();
        if t >= SPLASH_SECONDS {
            return None;
        }
        let p = self.p;
        let span = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
        let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
        let reveal = smooth(span(0.05, 1.2));
        let word_in = smooth(span(0.55, 1.1));
        let word_out = smooth(span(1.35, 1.75));
        let gather = smooth(span(1.45, 2.15));
        let close = smooth(span(2.1, SPLASH_SECONDS));
        let threads: Vec<Hsla> = COLORS.iter().map(|c| thread::color(c, p.dark)).collect();
        let over = threads.clone();
        let (mark, half) = wordmark(p, 80.0);
        const BAND: f32 = 220.0;
        const WORD: f32 = 130.0;
        let band = div()
            .relative()
            .w_full()
            .h(px(BAND))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(BAND))
                    .child(thread::weave(threads, gather, reveal, BAND)),
            )
            // The mark, in a slot it rises out of and sinks back into, with the threads
            // weaving over and under it.
            .child(
                div()
                    .overflow_hidden()
                    .child(mark.relative().top(px(WORD * ((1.0 - word_in) + word_out)))),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(BAND))
                    .opacity(word_in * (1.0 - word_out))
                    .child(thread::weave_over(over, gather, reveal, BAND, half)),
            );
        Some(self.curtain(h, h / 2.0, close, band, BAND))
    }

    /// When the page beside the sidebar was last seen arriving: as it unrolled, or as the
    /// opening weave or a new agent's curtain uncovered it. It can be a moment ahead.
    fn opened(&self) -> Instant {
        let mut at = self.page.1;
        if let Some((closed, _, _)) = self.closing {
            at = at.max(closed + Duration::from_secs_f32(CLOSE_SECONDS * 0.6));
        }
        if let Some(launched) = self.launched {
            at = at.max(launched + Duration::from_secs_f32(SPLASH_SECONDS * 0.85));
        }
        at
    }

    /// A page unrolling beside the sidebar: the background still over what's below a
    /// thread passing down it, `h` being the window's height.
    fn unroll(&self, h: f32) -> Option<Div> {
        let t = self.page.1.elapsed().as_secs_f32() / UNROLL_SECONDS;
        if t >= 1.0 {
            return None;
        }
        let p = self.p;
        let k = 1.0 - (1.0 - t).powi(3);
        let area = h - layout::TITLEBAR;
        let top = layout::TITLEBAR + area * k;
        let tint = match self.screen {
            Screen::Chat => self
                .model
                .selected
                .as_ref()
                .and_then(|id| self.model.agent(id))
                .map(|a| thread::color(&a.color, p.dark)),
            _ => None,
        }
        .unwrap_or_else(|| thread::color("grey", p.dark));
        const BAND: f32 = 28.0;
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(top - BAND / 2.0))
                .bottom_0()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(BAND / 2.0))
                        .bottom_0()
                        .bg(p.bg),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top_0()
                        .h(px(BAND))
                        .child(thread::weave(vec![tint], 0.6, 1.0, BAND)),
                ),
        )
    }

    /// The app opening under a new agent's weave: the window closes on its thread.
    fn closing(&self, h: f32) -> Option<Div> {
        let (at, tint, _) = self.closing?;
        let t = at.elapsed().as_secs_f32() / CLOSE_SECONDS;
        if t >= 1.0 {
            return None;
        }
        let close = t * t * (3.0 - 2.0 * t);
        let center = layout::TITLEBAR + (h - layout::TITLEBAR) / 2.0;
        let band = div()
            .w_full()
            .child(thread::weave(vec![tint], 1.0, 1.0, WEAVE_BAND));
        Some(self.curtain(h, center, close, band, WEAVE_BAND))
    }

    /// Once the woven agent is there and the threads have met, its chat opens.
    fn finish_weaving(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A picked brain moves on to naming once the other tiles have gone.
        if self.make.step == Step::Brain
            && self
                .make
                .picked_at
                .is_some_and(|at| at.elapsed().as_secs_f32() >= PICK_SECONDS)
        {
            self.make.picked_at = None;
            self.make.step = Step::Name;
        }
        if self.make.step != Step::Weaving {
            return;
        }
        let t = self.make.started.map_or(0.0, |s| s.elapsed().as_secs_f32());
        let made =
            self.model.agents.len() > self.make.agents_before && self.model.made_agent.is_none();
        if made && t > WEAVE_SECONDS + 0.3 {
            let tint = thread::color(self.make.color(), self.p.dark);
            self.closing = Some((Instant::now(), tint, self.make.first));
            self.name_in.update(cx, |s, cx| s.set_value("", window, cx));
            self.make = Making::default();
            self.screen = Screen::Chat;
            self.refresh_rows(cx);
        } else if std::mem::take(&mut self.model.make_failed) || !made && t > 20.0 {
            // The instance said no (its notice says why), or never answered: back to the
            // name, to try again.
            self.make.step = Step::Name;
        }
    }

    /// A chat with nothing in it yet: the agent's thread and name, the composer in the
    /// middle, and a few things to ask it.
    fn hero(
        &mut self,
        agent: Option<&model::Agent>,
        composer: Option<Div>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let Some(a) = agent else {
            return div().flex_1();
        };
        let tint = thread::color(&a.color, p.dark);
        // While the person types to an idle agent, its thread leans in and thinks along.
        let typing = !self.composer.read(cx).value().trim().is_empty();
        let mood = match self.model.mood(&a.id) {
            Mood::Idle if typing => Mood::Thinking,
            m => m,
        };
        // It twangs as its chat opens, like a hello, and after each run.
        let opened = self.opened();
        let snapped = match self.model.finished_at(&a.id) {
            Some(done) if done > opened => Some(done),
            _ => Some(opened),
        };
        let since = opened
            .checked_duration_since(Instant::now())
            .map_or(opened.elapsed().as_secs_f32(), |wait| -wait.as_secs_f32());
        let mut chips = div().flex().flex_wrap().justify_center().gap(px(space::S));
        for (i, ask) in SUGGESTIONS.iter().enumerate() {
            // The suggestions come in one after another.
            let x = ((since - 0.35 - 0.1 * i as f32) / 0.35).clamp(0.0, 1.0);
            chips = chips.child(appear(
                div()
                    .id(("ask", i))
                    .px(px(space::L))
                    .py(px(space::S))
                    .rounded(px(radius::CHIP))
                    .border_1()
                    .border_color(p.line_strong)
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover).text_color(p.ink))
                    .child(*ask)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.composer.update(cx, |s, cx| {
                            s.set_value(*ask, window, cx);
                            s.focus(window, cx);
                        });
                    })),
                x * x * (3.0 - 2.0 * x),
            ));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::XL))
            .px(px(space::XL))
            .pb(px(60.0))
            .child(thread("hero", mood, tint, 140.0, 16.0, snapped))
            .child(
                div()
                    .text_size(px(size::HERO))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink)
                    .child(a.name.clone()),
            )
            .when_some(composer, |d, c| {
                d.child(div().w_full().max_w(px(space::COLUMN)).child(c))
            })
            .child(chips)
    }

    /// A new agent: the same steps as the first one, with a way back.
    fn new_agent(&mut self, cx: &mut Context<Self>) -> Div {
        self.making(false, cx)
    }

    /// The edit page: an agent's name and instructions, and its earlier versions to go
    /// back to. Each save is a new revision, so nothing is lost.
    /// A line of the agent's settings: what's counted, and how many; one that opens
    /// where they're kept has a chevron.
    fn stat_row(
        &self,
        id: &'static str,
        label: &str,
        value: String,
        opens: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::S))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .text_size(px(size::SMALL))
            .when(opens, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .on_click(on_click)
            })
            .child(div().flex_1().text_color(p.ink).child(label.to_string()))
            .child(div().text_color(p.ink_2).child(value))
            .when(opens, |d| {
                d.child(
                    Icon::new(IconName::ChevronRight)
                        .xsmall()
                        .text_color(p.ink_3),
                )
            })
    }

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
                            this.close_side(cx);
                            cx.notify();
                        })),
                );
            }
            list = list.child(row);
        }
        // The person's MCP servers, each on or off for this agent.
        let has: Vec<String> = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .map(|a| a.mcp.clone())
            .unwrap_or_default();
        let mut servers = div().flex().flex_wrap().gap(px(space::S));
        if self.model.mcp_servers.is_empty() {
            servers = servers.child(
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child("None yet"),
            );
        }
        for m in self.model.mcp_servers.clone() {
            let name = m.name.clone();
            servers = servers.child(
                self.button_owned(
                    format!("agent-mcp-{}", m.name),
                    &m.name,
                    has.contains(&m.name),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(f) = this.model.toggle_mcp(&name) {
                        this.send(f);
                    }
                    cx.notify();
                })),
            );
        }
        // Its model and reasoning effort, as the chat's picker has them, once
        // its program has said what it offers.
        let mut picks = Vec::new();
        if let Some(a) = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .cloned()
        {
            for o in a.picks() {
                let on = a.chosen(&o.id);
                let mut row = div().flex().flex_wrap().gap(px(space::S));
                for (value, name) in o.values.clone() {
                    let (agent_id, option) = (a.id.clone(), o.id.clone());
                    row = row.child(
                        self.button_owned(
                            format!("edit-pick-{}-{value}", o.id),
                            &name,
                            value == on,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(f) = this.model.set_config(&agent_id, &option, &value) {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                    );
                }
                picks.push(self.field(&o.name, row));
            }
        }
        // Its thread's colour.
        let color = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .map(|a| a.color.clone())
            .unwrap_or_default();
        let mut swatches = div().flex().flex_wrap().items_center().gap(px(space::S));
        for (i, c) in COLORS.iter().enumerate() {
            let picked = color == *c;
            let dot = thread::color(c, p.dark);
            swatches = swatches.child(
                div()
                    .id(("edit-swatch", i))
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_2()
                    .border_color(if picked { dot } else { transparent_black() })
                    .cursor_pointer()
                    .child(div().size(px(16.0)).rounded_full().bg(dot))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.model.set_color(c) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        // What it knows and does, counted, each opening where it's kept.
        let summary = self.model.summary.clone();
        let count = |n: Option<i64>| n.map_or(String::from("…"), |n| n.to_string());
        let memories = self.stat_row(
            "edit-memories",
            "Memories",
            count(summary.as_ref().map(|s| s.memories)),
            true,
            cx.listener(|this, _, _, cx| {
                this.model.notes.clear();
                if let Some(f) = this.model.load_notes() {
                    this.send(f);
                }
                this.show_side(Screen::Memory, cx);
                cx.notify();
            }),
        );
        let skills = self.stat_row(
            "edit-skills",
            "Skills",
            count(summary.as_ref().map(|s| s.skills)),
            true,
            cx.listener(|this, _, _, cx| this.open(Screen::Skills, cx)),
        );
        let tables = self.stat_row(
            "edit-tables",
            "Tables",
            count(summary.as_ref().map(|s| s.tables)),
            true,
            cx.listener(|this, _, _, cx| this.open(Screen::Tables, cx)),
        );
        let files = self.stat_row(
            "edit-files",
            "Files",
            summary.as_ref().map_or(String::from("…"), |s| {
                format!("{} · {}", s.files, model::size_of(s.bytes.max(0) as u64))
            }),
            false,
            |_, _, _| {},
        );
        let routines = self.stat_row(
            "edit-routines",
            "Routines",
            count(summary.as_ref().map(|s| s.routines)),
            true,
            cx.listener(|this, _, _, cx| this.open(Screen::Routines, cx)),
        );
        let usage = self.stat_row(
            "edit-usage",
            "Tokens",
            summary.as_ref().map_or(String::from("…"), |s| {
                format!(
                    "{} today · {} this month",
                    model::tokens_text(s.tokens_today),
                    model::tokens_text(s.tokens_month)
                )
            }),
            false,
            |_, _, _| {},
        );
        let group = |title: &'static str| {
            div()
                .pt(px(space::M))
                .border_t_1()
                .border_color(p.line)
                .text_size(px(size::CAPTION))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink_3)
                .child(title)
        };
        let page = div()
            .id("edit-agent")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(
                div()
                    .w_full()
                    .max_w(px(520.0))
                    .flex()
                    .flex_col()
                    .gap(px(space::L))
                    .child(group("Brain"))
                    .child(self.field("Name", Input::new(&self.name_in)))
                    .children(picks)
                    .child(self.field("Colour", swatches))
                    .child(group("Knows"))
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
                                        this.close_side(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(self.button("cancel-edit", "Cancel", false).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.close_side(cx);
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(memories)
                            .child(skills)
                            .child(tables)
                            .child(files),
                    )
                    .child(self.field("MCP servers", servers))
                    .child(group("Does"))
                    .child(div().flex().flex_col().child(routines).child(usage))
                    .when(!self.model.revisions.is_empty(), |d| {
                        d.child(self.field("Versions", list))
                    }),
            );
        div().flex_1().min_h_0().child(page)
    }

    /// A form's buttons: Delete on the left when it's one that exists, and Cancel and Save
    /// on the right.
    fn form_actions(
        &self,
        delete: Option<Stateful<Div>>,
        cancel: Stateful<Div>,
        save: Stateful<Div>,
    ) -> AnyElement {
        let p = self.p;
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(div().children(delete.map(|d| d.text_color(p.bad))))
            .child(div().flex().gap(px(space::S)).child(cancel).child(save))
            .into_any_element()
    }

    /// A row a long text fills, under its label and a line of how.
    fn text_row(&self, label: &str, how: Option<&str>, input: impl IntoElement) -> AnyElement {
        let p = self.p;
        div()
            .p(px(space::L))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .text_color(p.ink)
                            .child(label.to_string()),
                    )
                    .when_some(how, |d, h| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(h.to_string()),
                        )
                    }),
            )
            .child(input)
            .into_any_element()
    }

    /// A row that opens what it names.
    fn link_row(&self, id: SharedString, row: Div) -> Stateful<Div> {
        let p = self.p;
        row.id(id).cursor_pointer().hover(move |s| s.bg(p.hover))
    }

    /// The MCP servers page: the person's servers, which each agent gets if it has them,
    /// and the keys their settings name.
    fn mcp_page(&mut self, cx: &mut Context<Self>) -> Div {
        if let Some(open) = self.mcp_open.clone() {
            let save = self
                .small_button("save-mcp", "Save", true)
                .on_click(cx.listener(|this, _, _, cx| {
                    let name = this.mcp_name_in.read(cx).value().to_string();
                    let line = this.mcp_cmd_in.read(cx).value().to_string();
                    let settings = this.mcp_settings_in.read(cx).value().to_string();
                    if name.trim().is_empty() || line.trim().is_empty() {
                        this.model
                            .notices
                            .push("Needs a name, and a command or an address".into());
                        cx.notify();
                        return;
                    }
                    let f = this
                        .model
                        .put_mcp(crate::model::server_args(&name, &line, &settings));
                    this.send(f);
                    this.mcp_open = None;
                    cx.notify();
                }));
            let cancel = self
                .small_button("cancel-mcp", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.mcp_open = None;
                    cx.notify();
                }));
            let delete = open.clone().map(|name| {
                self.small_button("delete-mcp", "Delete", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.delete_mcp(&name);
                        this.send(f);
                        this.mcp_open = None;
                        cx.notify();
                    }))
            });
            let groups = vec![
                self.group(
                    None,
                    vec![
                        self.field_row("Name", Input::new(&self.mcp_name_in)),
                        self.field_row("Command or URL", Input::new(&self.mcp_cmd_in)),
                        self.text_row(
                            "Settings",
                            Some("NAME=value, one a line. NAME=secret:KEY uses one of your keys"),
                            Textarea::new(&self.mcp_settings_in),
                        ),
                    ],
                ),
                self.form_actions(delete, cancel, save),
            ];
            let title = if open.is_some() {
                "Change MCP server"
            } else {
                "New MCP server"
            };
            return self.settings_page("mcp-page", title, None, groups);
        }
        let new_server = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("new-mcp", "New server", primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    for s in [&this.mcp_name_in, &this.mcp_cmd_in] {
                        s.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                    this.mcp_settings_in
                        .update(cx, |s, cx| s.set_value("", window, cx));
                    this.mcp_open = Some(None);
                    cx.notify();
                }))
        };
        // A browser the agent drives, through Microsoft's Playwright MCP server. It needs
        // Node on the instance's computer.
        let add_browser = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("add-browser", "Add a browser", primary)
                .on_click(cx.listener(|this, _, _, cx| {
                    let f = this.model.put_mcp(crate::model::server_args(
                        "browser",
                        "npx -y @playwright/mcp@latest",
                        "",
                    ));
                    this.send(f);
                    cx.notify();
                }))
        };
        let mut groups = Vec::new();
        let servers = self.model.mcp_servers.clone();
        let has_browser = servers.iter().any(|m| m.name == "browser");
        if servers.is_empty() {
            let actions = div()
                .flex()
                .gap(px(space::S))
                .child(add_browser(self, cx, true))
                .child(new_server(self, cx, false))
                .into_any_element();
            groups.push(self.empty(
                Ico::Plug,
                "No MCP servers yet",
                "Tools your agents can use, such as a browser, GitHub or your calendar",
                Some(actions),
            ));
        } else {
            let mut rows = Vec::new();
            for m in servers {
                let what = if m.url.is_empty() {
                    m.command.clone()
                } else {
                    m.url.clone()
                };
                let row = self.row(Some(self.tile(Ico::Plug)), m.name.clone(), Some(what), None);
                rows.push(
                    self.link_row(SharedString::from(format!("mcp-{}", m.name)), row)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let (name, line, settings) = (
                                m.name.clone(),
                                if m.url.is_empty() {
                                    m.command.clone()
                                } else {
                                    m.url.clone()
                                },
                                m.settings_text(),
                            );
                            this.mcp_name_in
                                .update(cx, |s, cx| s.set_value(name.clone(), window, cx));
                            this.mcp_cmd_in
                                .update(cx, |s, cx| s.set_value(line, window, cx));
                            this.mcp_settings_in
                                .update(cx, |s, cx| s.set_value(settings, window, cx));
                            this.mcp_open = Some(Some(name));
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            if !has_browser {
                rows.push(
                    self.row(
                        Some(self.tile(Ico::Globe)),
                        "A browser",
                        Some("Your agents open and use web pages. Needs Node".into()),
                        Some(add_browser(self, cx, false).into_any_element()),
                    )
                    .into_any_element(),
                );
            }
            groups.push(self.group(None, rows));
        }
        // The keys settings name, sealed on the instance: only their names come here.
        let mut keys = Vec::new();
        for k in self.model.secrets.clone() {
            let name = k.clone();
            keys.push(
                self.row(
                    Some(self.tile(Ico::KeyRound)),
                    k.clone(),
                    Some("••••••••".into()),
                    Some(
                        self.small_button(format!("secret-{k}"), "Remove", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.delete_secret(&name);
                                this.send(f);
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        keys.push(
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::L))
                .py(px(space::M))
                .child(div().flex_1().child(Input::new(&self.set_key_in)))
                .child(div().flex_1().child(Input::new(&self.set_value_in)))
                .child(
                    self.small_button("save-secret", "Add key", false)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let name = this.set_key_in.read(cx).value().to_string();
                            let value = this.set_value_in.read(cx).value().to_string();
                            if name.trim().is_empty() {
                                return;
                            }
                            let f = this.model.put_secret(&name, &value);
                            this.send(f);
                            for s in [&this.set_key_in, &this.set_value_in] {
                                s.update(cx, |s, cx| s.set_value("", window, cx));
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        );
        groups.push(self.group(Some("Keys, sealed on the instance"), keys));
        let action = (!self.model.mcp_servers.is_empty())
            .then(|| new_server(self, cx, false).into_any_element());
        self.settings_page("mcp-page", "MCP servers", action, groups)
    }

    /// The skills page: the person's skills, which every agent reads when a task fits one,
    /// and the form for a new or changed one.
    fn skills_page(&mut self, cx: &mut Context<Self>) -> Div {
        if let Some(open) = self.skill_open.clone() {
            let save = self
                .small_button("save-skill", "Save", true)
                .on_click(cx.listener(|this, _, _, cx| {
                    let name = this.skill_name_in.read(cx).value().to_string();
                    let description = this.skill_desc_in.read(cx).value().to_string();
                    let text = this.skill_body_in.read(cx).value().to_string();
                    if name.trim().is_empty() || description.trim().is_empty() {
                        this.model
                            .notices
                            .push("Needs a name, and when it's for".into());
                        cx.notify();
                        return;
                    }
                    let f = this.model.save_skill(&name, &description, &text);
                    this.send(f);
                    this.skill_open = None;
                    cx.notify();
                }));
            let cancel = self
                .small_button("cancel-skill", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.skill_open = None;
                    cx.notify();
                }));
            let delete = open.clone().map(|name| {
                self.small_button("delete-skill", "Delete", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.delete_skill(&name);
                        this.send(f);
                        this.skill_open = None;
                        cx.notify();
                    }))
            });
            let groups = vec![
                self.group(
                    None,
                    vec![
                        self.field_row("Name", Input::new(&self.skill_name_in)),
                        self.field_row("When it's for", Input::new(&self.skill_desc_in)),
                        self.text_row("How to do it", None, Textarea::new(&self.skill_body_in)),
                    ],
                ),
                self.form_actions(delete, cancel, save),
            ];
            let title = if open.is_some() {
                "Change skill"
            } else {
                "New skill"
            };
            return self.settings_page("skills-page", title, None, groups);
        }
        let new_skill = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("new-skill", "New skill", primary)
                .on_click(cx.listener(|this, _, window, cx| {
                    for s in [&this.skill_name_in, &this.skill_desc_in] {
                        s.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                    this.skill_body_in
                        .update(cx, |s, cx| s.set_value("", window, cx));
                    this.skill_open = Some(None);
                    cx.notify();
                }))
                .into_any_element()
        };
        if self.model.skills.is_empty() {
            let action = new_skill(self, cx, true);
            let groups = vec![self.empty(
                Ico::BookOpen,
                "No skills yet",
                "How you like something done, which every agent reads when a job fits it",
                Some(action),
            )];
            return self.settings_page("skills-page", "Skills", None, groups);
        }
        let mut rows = Vec::new();
        for k in self.model.skills.clone() {
            let row = self.row(
                Some(self.tile(Ico::BookOpen)),
                k.name.clone(),
                Some(k.description.clone()),
                None,
            );
            rows.push(
                self.link_row(SharedString::from(format!("skill-{}", k.name)), row)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let (name, description, text) =
                            (k.name.clone(), k.description.clone(), k.body.clone());
                        this.skill_name_in
                            .update(cx, |s, cx| s.set_value(name.clone(), window, cx));
                        this.skill_desc_in
                            .update(cx, |s, cx| s.set_value(description, window, cx));
                        this.skill_body_in
                            .update(cx, |s, cx| s.set_value(text, window, cx));
                        this.skill_open = Some(Some(name));
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        let action = new_skill(self, cx, false);
        self.settings_page(
            "skills-page",
            "Skills",
            Some(action),
            vec![self.group(None, rows)],
        )
    }

    /// Who an agent's id is, as the tables say it: its name, or "you" for the person.
    fn who(&self, id: &str) -> String {
        if id.is_empty() {
            return "you".into();
        }
        self.model
            .agent(id)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "an agent that's gone".into())
    }

    /// The tables beside the chat: the ones this agent made and the ones it shares with
    /// the person's other agents, and one open, with its newest rows. What an agent other
    /// than the owner wrote is marked, and dropping a table asks once more.
    fn tables_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut groups: Vec<AnyElement> = Vec::new();
        let open = self
            .model
            .table_open
            .clone()
            .and_then(|name| self.model.tables.iter().find(|t| t.name == name).cloned());
        if let Some(t) = open {
            let name = t.name.clone();
            let dropping = self.table_dropping.as_deref() == Some(name.as_str());
            let back = div()
                .id("tables-back")
                .size(px(28.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .text_color(p.ink_2)
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover).text_color(p.ink))
                .child(Icon::new(Ico::ArrowLeft).small())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.table_dropping = None;
                    for f in this.model.open_table(None) {
                        this.send(f);
                    }
                    cx.notify();
                }));
            let drop = self
                .small_button(
                    "drop-table",
                    if dropping { "Drop it" } else { "Drop" },
                    dropping,
                )
                .when(dropping, |d| d.bg(p.bad))
                .when(!dropping, |d| d.text_color(p.bad))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.table_dropping.as_deref() == Some(name.as_str()) {
                        let f = this.model.drop_table(&name);
                        this.send(f);
                        this.table_dropping = None;
                    } else {
                        this.table_dropping = Some(name.clone());
                    }
                    cx.notify();
                }));
            let sub = if dropping {
                format!("Drop it, with its {} rows, for good?", t.rows)
            } else if t.description.is_empty() {
                format!("Made by {}, {} rows", self.who(&t.owner), t.rows)
            } else {
                format!(
                    "{} · by {}, {} rows",
                    t.description,
                    self.who(&t.owner),
                    t.rows
                )
            };
            groups.push(self.group(
                None,
                vec![self
                    .row(
                        Some(back.into_any_element()),
                        t.name.clone(),
                        Some(sub),
                        Some(drop.into_any_element()),
                    )
                    .into_any_element()],
            ));
            let mut rows = Vec::new();
            for r in self.model.table_rows.clone() {
                rows.push(
                    div()
                        .px(px(space::L))
                        .py(px(space::M))
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .child(
                            div()
                                .flex()
                                .gap(px(space::S))
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(format!("#{} · {}", r.id, self.who(&r.by)))
                                .when(r.untrusted, |d| {
                                    d.child(div().text_color(p.bad).child("from another agent"))
                                }),
                        )
                        .child(
                            div()
                                .font_family(font::MONO)
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(r.data.clone()),
                        )
                        .into_any_element(),
                );
            }
            if rows.is_empty() {
                rows.push(self.note_row("No rows yet", p.ink_3));
            }
            if self.model.table_more {
                rows.push(self.note_row("Older rows aren't shown", p.ink_3));
            }
            groups.push(self.group(Some("Newest first"), rows));
        } else if self.model.tables.is_empty() {
            groups.push(self.empty(
                Ico::Sheet,
                "No tables yet",
                "Agents keep lists and records in tables, and share them with each other",
                None,
            ));
        } else {
            let agent = self.model.selected.clone().unwrap_or_default();
            let (mine, shared): (Vec<model::SharedTable>, Vec<model::SharedTable>) = self
                .model
                .tables
                .clone()
                .into_iter()
                .partition(|t| t.owner == agent);
            for (title, tables, by) in [
                ("Made by this agent", mine, false),
                ("Shared with it", shared, true),
            ] {
                if tables.is_empty() {
                    continue;
                }
                let mut rows = Vec::new();
                for t in tables {
                    let name = t.name.clone();
                    let mut sub = format!("{} rows", t.rows);
                    if by {
                        sub = format!("{sub}, by {}", self.who(&t.owner));
                    }
                    if !t.description.is_empty() {
                        sub = format!("{} · {sub}", t.description);
                    }
                    let row =
                        self.row(Some(self.tile(Ico::Sheet)), t.name.clone(), Some(sub), None);
                    rows.push(
                        self.link_row(SharedString::from(format!("table-{}", t.name)), row)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.table_dropping = None;
                                for f in this.model.open_table(Some(name.clone())) {
                                    this.send(f);
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
                groups.push(self.group(Some(title), rows));
            }
        }
        div().flex_1().min_h_0().child(
            div()
                .id("tables-panel")
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(space::L))
                .p(px(space::L))
                .children(groups),
        )
    }

    /// The plugins page: each plugin, what it asks for that waits for the person's yes, and
    /// its panel to open. An open panel lies over the content area in a web view of its own.
    fn plugins_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let ask = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("plugin-prompt", "Copy a prompt", primary)
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(PLUGIN_PROMPT.to_string()));
                    this.notice = Some((
                        "Copied. Paste it to one of your agents.".into(),
                        Instant::now(),
                    ));
                    cx.notify();
                }))
                .into_any_element()
        };
        if self.model.plugins.is_empty() {
            let action = ask(self, cx, true);
            let groups = vec![self.empty(
                Ico::Puzzle,
                "No plugins yet",
                "Your agents build them: a panel beside the chat, or new tools for every agent",
                Some(action),
            )];
            return self.settings_page("plugins-page", "Plugins", None, groups);
        }
        let mut rows = Vec::new();
        for pl in self.model.plugins.clone() {
            let mut buttons = div().flex().gap(px(space::XS));
            if pl.entry.is_some() {
                let id = pl.id.clone();
                buttons = buttons.child(
                    self.small_button(format!("open-{}", pl.id), "Open", true)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.open_plugin(&id);
                            this.send(f);
                            // Its panel opens beside the chat.
                            this.show_side(Screen::Plugins, cx);
                            cx.notify();
                        })),
                );
            }
            if !pl.waiting.is_empty() {
                let id = pl.id.clone();
                buttons = buttons.child(
                    self.small_button(format!("allow-{}", pl.id), "Allow", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.allow_plugin(&id);
                            this.send(f);
                            cx.notify();
                        })),
                );
            }
            let id = pl.id.clone();
            buttons = buttons.child(
                self.small_button(format!("delete-{}", pl.id), "Delete", false)
                    .text_color(p.bad)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.delete_plugin(&id);
                        this.send(f);
                        cx.notify();
                    })),
            );
            let has = match (pl.entry.is_some(), pl.tools) {
                (true, true) => "A panel and tools",
                (true, false) => "A panel",
                (false, true) => "Tools for your agents",
                (false, false) => "Nothing to show yet",
            };
            let sub = if pl.description.is_empty() {
                format!("{has} · {}", pl.version)
            } else {
                format!("{} · {}", pl.description, pl.version)
            };
            rows.push(
                self.row(
                    Some(self.tile(Ico::Puzzle)),
                    pl.name.clone(),
                    Some(sub),
                    Some(buttons.into_any_element()),
                )
                .into_any_element(),
            );
            if !pl.waiting.is_empty() {
                rows.push(self.note_row(
                    format!(
                        "{} asks to {}. Until you allow it, it can't.",
                        pl.name,
                        model::asking(&pl.waiting)
                    ),
                    p.ink_2,
                ));
            }
        }
        let action = ask(self, cx, false);
        self.settings_page(
            "plugins-page",
            "Plugins",
            Some(action),
            vec![self.group(None, rows)],
        )
    }

    /// What the web view beside the chat shows, by key: the open plugin's panel (by
    /// the plugin's id), or the open page once it's fetched ("page:" and its file).
    /// None when it shows neither.
    fn panel_key(&self) -> Option<String> {
        if self.screen != Screen::Chat {
            return None;
        }
        match self.side {
            Some(Screen::Plugins) => self
                .model
                .panel
                .as_ref()
                .filter(|o| o.plugin.entry.is_some())
                .map(|o| o.plugin.id.clone()),
            Some(Screen::Page) => {
                self.model.page_path()?;
                self.model
                    .page
                    .as_ref()
                    .map(|f| format!("page:{}", f.file.get(..16).unwrap_or(&f.file)))
            }
            _ => None,
        }
    }

    /// What the web view loads for `key`: the plugin's files, or the page alone.
    fn panel_source(&mut self, key: &str) -> Option<crate::panel::Source> {
        if key.starts_with("page:") {
            let path = self.model.page_path()?.clone();
            let html = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    self.model
                        .notices
                        .push(format!("The page couldn't be read: {e}"));
                    self.model.page = None;
                    return None;
                }
            };
            return Some(crate::panel::Source {
                id: key.to_string(),
                files: std::sync::Arc::new(HashMap::from([("index.html".to_string(), html)])),
                entry: "index.html".into(),
            });
        }
        let open = self.model.panel.clone()?;
        let entry = open.plugin.entry.clone()?;
        Some(crate::panel::Source {
            id: open.plugin.id.clone(),
            files: std::sync::Arc::new(
                open.files
                    .into_iter()
                    .map(|(k, v)| (k, v.into_bytes()))
                    .collect(),
            ),
            entry,
        })
    }

    /// Shows the open plugin's panel or the open page beside the chat, or takes it
    /// away when there's none to show, and keeps it in its place as the window
    /// changes size.
    fn sync_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.panel_key() else {
            self.panel = None;
            return;
        };
        // In the panel beside the chat, under its head. A web view draws over everything
        // in the window, so while the popup is open it shrinks to nothing.
        let size = window.viewport_size();
        let w = side_width(self.side.unwrap_or(Screen::Plugins), f32::from(size.width));
        let rect = if self.popup.is_some() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            [
                f32::from(size.width) - w + 1.0,
                layout::TITLEBAR + SIDE_HEAD,
                w - 1.0,
                f32::from(size.height) - layout::TITLEBAR - SIDE_HEAD,
            ]
        };
        if let Some((id, view, at)) = &mut self.panel {
            if *id == key {
                if *at != rect {
                    view.set_bounds(panel_bounds(rect));
                    *at = rect;
                }
                return;
            }
        }
        self.panel = None;
        if self.panel_opening.is_some() {
            return;
        }
        // Making a web view runs a message loop of its own until WebView2 is ready, and
        // the window's tasks run in it. So it's made from a task, with nothing borrowed,
        // not here, where the window is.
        let Ok(parent) =
            raw_window_handle::HasWindowHandle::window_handle(&*window).map(|h| Parent(h.as_raw()))
        else {
            return;
        };
        let Some(source) = self.panel_source(&key) else {
            return;
        };
        let mut context = self
            .panel_context
            .take()
            .unwrap_or_else(|| crate::panel::context(std::path::Path::new(&data_dir())));
        let door = self.door_in.clone();
        let id = key;
        self.panel_opening = Some(id.clone());
        cx.spawn(async move |this, cx| {
            let made = crate::panel::Panel::open(
                &parent,
                &mut context,
                &source,
                panel_bounds(rect),
                move |m| door.borrow_mut().push(m),
                |what| eprintln!("loom: a panel was refused: {what}"),
            );
            let _ = this.update(cx, |this, cx| {
                this.panel_opening = None;
                this.panel_context = Some(context);
                match made {
                    // Unless the person went elsewhere while it was being made.
                    Ok(view) => {
                        if this.panel_key().as_ref() == Some(&id) {
                            this.panel = Some((id, view, rect));
                        }
                    }
                    Err(e) => {
                        this.model
                            .notices
                            .push(format!("The panel didn't open: {e}"));
                        if id.starts_with("page:") {
                            this.model.page = None;
                        } else {
                            this.model.close_panel();
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The open panel's door: what came in goes to the instance as commands, or is
    /// refused, and the answers that came back go to the panel. A page has no door:
    /// everything through it is refused.
    fn pump_door(&mut self) {
        let came: Vec<crate::panel::Message> = self.door_in.borrow_mut().drain(..).collect();
        let page = self
            .panel
            .as_ref()
            .is_some_and(|(id, _, _)| id.starts_with("page:"));
        for m in came {
            if page {
                self.model
                    .door_answers
                    .push((m.id, Err("A page has no way to loom.".into())));
                continue;
            }
            match self.model.door(m.id, &m.op, &m.args) {
                Ok(frame) => self.send(frame),
                Err(why) => self.model.door_answers.push((m.id, Err(why))),
            }
        }
        if let Some((_, view, _)) = &self.panel {
            for (id, answer) in self.model.door_answers.drain(..) {
                view.answer(id, answer);
            }
        }
    }

    /// Opens the routine form, empty, for a new one.
    fn new_routine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.routine_name_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.routine_at_in
            .update(cx, |s, cx| s.set_value("30", window, cx));
        self.routine_kind = "every".into();
        self.routine_prompt_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.routine_agent = self.model.selected.clone();
        self.routine_notify = true;
        self.routine_on = true;
        self.routine_catch_up = true;
        self.routine_thread = false;
        self.routine_webhook = false;
        self.set_routine_flow(&[], window, cx);
        self.routine_open = Some(None);
        cx.notify();
    }

    /// A row with a switch, for one of a form's yes-or-no choices.
    fn switch_row(
        &self,
        id: &'static str,
        title: &str,
        sub: &str,
        on: bool,
        cx: &mut Context<Self>,
        flip: fn(&mut Loom),
    ) -> AnyElement {
        let toggle = self
            .switch(id, on)
            .on_click(cx.listener(move |this, _, _, cx| {
                flip(this);
                cx.notify();
            }));
        self.row(
            None,
            title.to_string(),
            Some(sub.to_string()),
            Some(toggle.into_any_element()),
        )
        .into_any_element()
    }

    /// The routines page: prompts the person's agents are sent on a schedule, each in a
    /// conversation of its own, and the form for a new or changed one.
    fn routines_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.routine_open.is_some() {
            return self.routine_form(cx);
        }
        let p = self.p;
        let mut rows = Vec::new();
        for r in self.model.routines.clone() {
            let (agent, tint) = match self.model.agent(&r.agent) {
                Some(a) => (a.name.clone(), thread::color(&a.color, p.dark)),
                None => ("a deleted agent".to_string(), p.ink_3),
            };
            let next = if !r.on || r.next == 0 {
                "paused".to_string()
            } else if day_of(r.next) == "Today" {
                format!("next at {}", time_of(r.next))
            } else {
                format!("next {} at {}", day_of(r.next), time_of(r.next))
            };
            let last = match r.last_status.as_str() {
                "" | "done" | "quiet" => String::new(),
                "running" => " · running now".into(),
                "skipped" => format!(" · skipped at {}", time_of(r.last_at)),
                _ => format!(" · failed at {}: {}", time_of(r.last_at), r.last_error),
            };
            let mut sub = format!("{agent} · {} · {next}{last}", r.when);
            // Its flow, while it runs: how far each step has got, and a Stop.
            let running = self
                .model
                .flows
                .iter()
                .find(|f| f.routine == r.id)
                .map(|f| {
                    let steps: Vec<String> = f
                        .steps
                        .iter()
                        .skip(1)
                        .map(|(name, _, status)| format!("step {name} {status}"))
                        .collect();
                    (steps.join(" · "), f.stopped)
                });
            let mut controls = div().flex().gap(px(space::XS));
            if let Some((text, stopped)) = running {
                sub = text;
                if !stopped {
                    let routine = r.id.clone();
                    controls = controls.child(
                        self.small_button(format!("stop-flow-{}", r.id), "Stop", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let f = this.model.stop_flow(&routine);
                                this.send(f);
                                cx.notify();
                            })),
                    );
                }
            }
            // A routine with a thread of its own: the way there.
            if r.post == "thread" && !r.conversation.is_empty() {
                let conversation = r.conversation.clone();
                let agent = r.agent.clone();
                controls = controls.child(
                    self.small_button(format!("thread-{}", r.id), "Thread", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_thread(&agent, &conversation, cx);
                        })),
                );
            }
            let lead = div()
                .size(px(30.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .border_1()
                .border_color(p.line)
                .bg(p.bg)
                .text_color(if r.on { tint } else { p.ink_3 })
                .child(Icon::new(Ico::Clock).small())
                .into_any_element();
            rows.push(
                self.row(
                    Some(lead),
                    r.name.clone(),
                    Some(sub),
                    Some(controls.into_any_element()),
                )
                .id(SharedString::from(format!("routine-{}", r.id)))
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.routine_name_in
                        .update(cx, |s, cx| s.set_value(r.name.clone(), window, cx));
                    this.routine_at_in
                        .update(cx, |s, cx| s.set_value(r.at.clone(), window, cx));
                    this.routine_kind = if r.kind.is_empty() {
                        "every".into()
                    } else {
                        r.kind.clone()
                    };
                    this.routine_prompt_in
                        .update(cx, |s, cx| s.set_value(r.prompt.clone(), window, cx));
                    this.routine_agent = Some(r.agent.clone());
                    this.routine_notify = r.notify;
                    this.routine_on = r.on;
                    this.routine_catch_up = r.catch_up;
                    this.routine_thread = r.post == "thread";
                    this.routine_webhook = !r.hook.is_empty();
                    this.set_routine_flow(&r.flow, window, cx);
                    this.routine_open = Some(Some(r.id.clone()));
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        let empty = rows.is_empty();
        let new = self
            .small_button("new-routine", "New routine", empty)
            .on_click(cx.listener(|this, _, window, cx| this.new_routine(window, cx)))
            .into_any_element();
        if empty {
            let groups = vec![self.empty(
                Ico::Clock,
                "No routines yet",
                "Something an agent does on a schedule, such as a summary each morning or a check every hour",
                Some(new),
            )];
            return self.settings_page("routines-page", "Routines", None, groups);
        }
        self.settings_page(
            "routines-page",
            "Routines",
            Some(new),
            vec![self.group(None, rows)],
        )
    }

    /// The form for a new routine, or one being changed.
    fn routine_form(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let open = self.routine_open.clone().unwrap_or_default();
        let mut groups = Vec::new();
        let chips = self.agent_chips(
            "routine-agent",
            self.routine_agent.clone().as_deref(),
            cx,
            |this, id| this.routine_agent = Some(id),
        );
        groups.push(self.group(
            None,
            vec![
                self.field_row("Name", Input::new(&self.routine_name_in)),
                self.field_row("Agent", chips),
                div()
                    .p(px(space::L))
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .text_color(p.ink)
                            .child("What it's asked each time"),
                    )
                    .child(Textarea::new(&self.routine_prompt_in))
                    .into_any_element(),
            ],
        ));
        let mut kinds = Vec::new();
        for (kind, label) in [
            ("every", "Every few minutes"),
            ("daily", "Daily"),
            ("weekdays", "Weekdays"),
        ] {
            kinds.push(
                self.segment(
                    format!("routine-kind-{kind}"),
                    label,
                    self.routine_kind == kind,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    // From minutes to a time of day or back, the box gets an example of
                    // the other kind, so it isn't left saying "30" for a daily routine.
                    let was_minutes = this.routine_kind == "every";
                    if was_minutes != (kind == "every") {
                        let example = if kind == "every" { "30" } else { "08:00" };
                        this.routine_at_in
                            .update(cx, |s, cx| s.set_value(example, window, cx));
                    }
                    this.routine_kind = kind.into();
                    cx.notify();
                })),
            );
        }
        let at_label = if self.routine_kind == "every" {
            "Minutes"
        } else {
            "At (HH:MM)"
        };
        groups.push(self.group(
            Some("When"),
            vec![
                self.row(
                    None,
                    "Repeats",
                    None,
                    Some(self.segments(kinds).into_any_element()),
                )
                .into_any_element(),
                self.field_row(at_label, Input::new(&self.routine_at_in)),
            ],
        ));
        let mut options = vec![
            self.switch_row(
                "routine-on",
                "On",
                "A paused routine doesn't run",
                self.routine_on,
                cx,
                |this| this.routine_on = !this.routine_on,
            ),
            self.switch_row(
                "routine-notify",
                "Notify me",
                "When it has something to say",
                self.routine_notify,
                cx,
                |this| this.routine_notify = !this.routine_notify,
            ),
            self.switch_row(
                "routine-catch-up",
                "Catch up",
                "Run once for what it missed while off",
                self.routine_catch_up,
                cx,
                |this| this.routine_catch_up = !this.routine_catch_up,
            ),
            self.switch_row(
                "routine-post",
                "A thread of its own",
                "Posts there instead of in the chat",
                self.routine_thread,
                cx,
                |this| this.routine_thread = !this.routine_thread,
            ),
            self.switch_row(
                "routine-webhook",
                "Webhook",
                "GitHub or anything else can start it",
                self.routine_webhook,
                cx,
                |this| this.routine_webhook = !this.routine_webhook,
            ),
        ];
        // Where to post, once it has a webhook: to give GitHub or anything else.
        let hook = open
            .as_ref()
            .and_then(|id| self.model.routines.iter().find(|r| &r.id == id))
            .filter(|r| !r.hook.is_empty() && self.routine_webhook)
            .map(|r| (format!("/hooks/{}", r.hook), r.secret.clone()));
        if let Some((path, secret)) = hook {
            let copy_path = path.clone();
            options.push(
                self.row(
                    None,
                    path,
                    Some("Bearer the secret, or signed as GitHub signs".into()),
                    Some(
                        div()
                            .flex()
                            .gap(px(space::XS))
                            .child(
                                self.small_button("copy-hook", "Copy address", false)
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_path.clone(),
                                        ));
                                    }),
                            )
                            .child(
                                self.small_button("copy-secret", "Copy secret", false)
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            secret.clone(),
                                        ));
                                    }),
                            )
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        groups.push(self.group(Some("Options"), options));
        groups.push(self.routine_flow_group(cx));
        let editing = open.clone().unwrap_or_default();
        let save = self
            .small_button("save-routine", "Save", true)
            .on_click(cx.listener(move |this, _, _, cx| {
                let name = this.routine_name_in.read(cx).value().to_string();
                let prompt = this.routine_prompt_in.read(cx).value().to_string();
                let at = this.routine_at_in.read(cx).value().to_string();
                let Some(agent) = this.routine_agent.clone() else {
                    this.model.notices.push("Pick the agent it runs.".into());
                    cx.notify();
                    return;
                };
                if at.trim().is_empty() && !this.routine_webhook {
                    this.model
                        .notices
                        .push("Needs a schedule or a webhook".into());
                    cx.notify();
                    return;
                }
                // The flow, one step after another; one an agent made
                // with steps side by side stays as it is.
                let flow = if this.routine_flow_kept.is_empty() {
                    let mut steps = Vec::new();
                    let mut before = "routine".to_string();
                    for (k, (agent, prompt_in)) in this.routine_flow.iter().enumerate() {
                        let Some(agent) = agent.clone() else {
                            this.model
                                .notices
                                .push(format!("Pick step {}'s agent.", k + 1));
                            cx.notify();
                            return;
                        };
                        let name = (k + 1).to_string();
                        steps.push(model::FlowStep {
                            name: name.clone(),
                            agent,
                            prompt: prompt_in.read(cx).value().to_string(),
                            after: vec![before],
                        });
                        before = name;
                    }
                    Some(steps)
                } else {
                    None
                };
                let form = model::RoutineForm {
                    id: editing.clone(),
                    name,
                    agent,
                    prompt,
                    kind: this.routine_kind.clone(),
                    at,
                    notify: this.routine_notify,
                    on: this.routine_on,
                    catch_up: this.routine_catch_up,
                    post: if this.routine_thread {
                        "thread"
                    } else {
                        "chat"
                    }
                    .into(),
                    webhook: this.routine_webhook,
                    flow,
                };
                let f = this.model.save_routine(&form);
                this.send(f);
                this.routine_open = None;
                cx.notify();
            }));
        let cancel = self
            .small_button("cancel-routine", "Cancel", false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.routine_open = None;
                cx.notify();
            }));
        let mut left = div().flex().gap(px(space::S));
        if let Some(id) = open.clone() {
            let run = id.clone();
            left = left
                .child(
                    self.small_button("delete-routine", "Delete", false)
                        .text_color(p.bad)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.delete_routine(&id);
                            this.send(f);
                            this.routine_open = None;
                            cx.notify();
                        })),
                )
                .child(
                    self.small_button("run-routine", "Run now", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.run_routine(&run);
                            this.send(f);
                            this.routine_open = None;
                            cx.notify();
                        })),
                );
        }
        groups.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(left)
                .child(div().flex().gap(px(space::S)).child(cancel).child(save))
                .into_any_element(),
        );
        let title = if open.is_some() {
            "Change routine"
        } else {
            "New routine"
        };
        self.settings_page("routines-page", title, None, groups)
    }

    /// Fills the routine form's flow from `flow`: steps it edits when they go one after
    /// another, or kept as they are when an agent put some side by side.
    fn set_routine_flow(
        &mut self,
        flow: &[model::FlowStep],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.routine_flow.clear();
        self.routine_flow_kept.clear();
        if !model::flow_is_chain(flow) {
            self.routine_flow_kept = flow.to_vec();
            return;
        }
        for step in flow {
            let prompt = step.prompt.clone();
            let input = cx.new(|cx| {
                let mut t = TextareaState::new(window, cx)
                    .auto_grow(2, 10)
                    .placeholder("What this agent is asked, with the answer before it.");
                t.set_value(prompt, window, cx);
                t
            });
            self.routine_flow.push((Some(step.agent.clone()), input));
        }
    }

    /// The routine form's flow (PLAN.md, "Routine flows"): who its answer goes to next,
    /// step by step.
    fn routine_flow_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let name_of = |id: &str| {
            self.model
                .agent(id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "a deleted agent".into())
        };
        let mut rows = Vec::new();
        if !self.routine_flow_kept.is_empty() {
            let lines: Vec<String> = self
                .routine_flow_kept
                .iter()
                .map(|s| {
                    format!(
                        "{} ({}), after {}: {}",
                        s.name,
                        name_of(&s.agent),
                        s.after.join(" and "),
                        s.prompt
                    )
                })
                .collect();
            rows.push(
                self.row(
                    None,
                    "Steps side by side",
                    Some("An agent set this flow up, so it stays as it is".into()),
                    Some(
                        self.small_button("flow-off", "Take it off", false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.routine_flow_kept.clear();
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
            rows.push(self.note_row(lines.join("\n"), p.ink_2));
            return self.group(Some("Then"), rows);
        }
        for (k, (picked, prompt_in)) in self.routine_flow.iter().enumerate() {
            let from = if k == 0 {
                "the routine's answer".to_string()
            } else {
                format!("step {k}'s answer")
            };
            let remove = self
                .small_button(format!("flow-{k}-remove"), "Remove", false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if k < this.routine_flow.len() {
                        this.routine_flow.remove(k);
                    }
                    cx.notify();
                }));
            let mut pick = div().flex().flex_wrap().gap(px(space::XS));
            for a in self.model.agents.clone() {
                let id = a.id.clone();
                let on = picked.as_deref() == Some(a.id.as_str());
                pick = pick.child(
                    self.segment(format!("flow-{k}-agent-{}", a.id), &a.name, on)
                        .when(!on, |d| d.border_1().border_color(p.line))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(step) = this.routine_flow.get_mut(k) {
                                step.0 = Some(id.clone());
                            }
                            cx.notify();
                        })),
                );
            }
            rows.push(
                div()
                    .p(px(space::L))
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(size::BODY))
                                    .text_color(p.ink)
                                    .child(format!("Step {}, with {from}", k + 1)),
                            )
                            .child(remove),
                    )
                    .child(pick)
                    .child(Textarea::new(prompt_in))
                    .into_any_element(),
            );
        }
        if self.routine_flow.len() < 20 {
            rows.push(
                self.row(
                    None,
                    "Hand the answer on",
                    Some("To another agent, and its answer to the next".into()),
                    Some(
                        self.small_button("flow-add", "Add a step", false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let input = cx.new(|cx| {
                                    TextareaState::new(window, cx).auto_grow(2, 10).placeholder(
                                        "What this agent is asked, with the answer before it.",
                                    )
                                });
                                this.routine_flow.push((None, input));
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        self.group(Some("Then"), rows)
    }

    /// iPhone notifications through the push relay (PLAN.md, "The push relay"): whether the
    /// instance uses one, and for the owner, a place to turn it on or off.
    fn relay_group(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let row = match self.model.relay.clone() {
            None => return None,
            Some(url) if !url.is_empty() => self.row(
                Some(self.tile(Ico::Bell)),
                "Push relay",
                Some(format!("{url}, sealed end to end")),
                Some(
                    self.small_button("relay-off", "Stop", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let f = this.model.set_relay(Some(""));
                            this.send(f);
                            cx.notify();
                        }))
                        .into_any_element(),
                ),
            ),
            Some(_) => self.row(
                Some(self.tile(Ico::Bell)),
                "Push relay",
                Some("Notifications while loom is closed".into()),
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(div().w(px(210.0)).child(Input::new(&self.relay_in).small()))
                        .child(
                            self.small_button("relay-on", "Use", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let url = this.relay_in.read(cx).value().trim().to_string();
                                    if !url.is_empty() {
                                        let f = this.model.set_relay(Some(&url));
                                        this.send(f);
                                    }
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                ),
            ),
        };
        Some(self.group(
            Some("Notifications on iPhone"),
            vec![row.into_any_element()],
        ))
    }

    /// The Add a phone page: a QR code the phone's loom app scans to join this account,
    /// with a token of its own. The core makes what's in it; this draws it, dark on white
    /// in either theme so a camera reads it. Then the devices signed in.
    fn add_phone_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut groups = Vec::new();
        // The instance in this app listens only on this computer until phones are on.
        let local = self.local();
        let phones_on = !local || self.model.phones == Some(true);
        let mut pair = Vec::new();
        if local {
            pair.push(
                self.row(
                    Some(self.tile(Ico::Smartphone)),
                    "Phones can reach this computer",
                    Some("Over Wi-Fi or Tailscale, encrypted end to end".into()),
                    Some(
                        self.switch("phones-switch", phones_on)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.set_phones(Some(!phones_on));
                                this.send(f);
                                if !phones_on {
                                    let f = this.model.create_link();
                                    this.send(f);
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        if phones_on {
            let caption = |text: String| {
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .text_center()
                    .child(text)
            };
            let mut code = div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(space::M))
                .py(px(space::XL))
                .px(px(space::L));
            if let Some(name) = self.model.joined.clone() {
                code = code
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.ink)
                            .child(format!("{name} joined")),
                    )
                    .child(caption(
                        "It's signed in below, where you can sign it out".into(),
                    ));
            } else if let Some((text, expires)) = self.model.link.clone() {
                code = code
                    .child(
                        div()
                            .p(px(space::S))
                            .rounded(px(radius::ROW))
                            .bg(gpui::white())
                            .child(qr_view(&text)),
                    )
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.ink)
                            .child("Scan it with loom on your phone"),
                    )
                    .child(caption(format!("Works once, until {}", time_of(expires))));
            } else {
                code = code.child(caption("Making a code…".into()));
            }
            code = code.child(self.small_button("new-link", "New code", false).on_click(
                cx.listener(|this, _, _, cx| {
                    let f = this.model.create_link();
                    this.send(f);
                    cx.notify();
                }),
            ));
            pair.push(code.into_any_element());
        }
        groups.push(self.group(None, pair));
        if let Some(relay) = self.relay_group(cx) {
            groups.push(relay);
        }
        // Every device signed in to this account, so one that shouldn't be can be signed out.
        let mut devices = Vec::new();
        for (id, name, kind) in self.model.devices.clone() {
            let (what, icon) = match kind.as_str() {
                "ios" => ("iPhone", Ico::Smartphone),
                "macos" => ("Mac", Ico::Laptop),
                "windows" => ("Windows PC", Ico::Monitor),
                _ => ("Command line", Ico::SquareTerminal),
            };
            let this_one = id == self.model.me;
            let control = if this_one {
                div()
                    .px(px(space::S))
                    .py(px(2.0))
                    .rounded(px(radius::CHIP))
                    .bg(p.raised_2)
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_2)
                    .child("This one")
                    .into_any_element()
            } else {
                self.small_button(format!("revoke-{id}"), "Sign out", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.revoke_device(&id);
                        this.send(f);
                        cx.notify();
                    }))
                    .into_any_element()
            };
            devices.push(
                self.row(
                    Some(self.tile(icon)),
                    name,
                    Some(what.into()),
                    Some(control),
                )
                .into_any_element(),
            );
        }
        if !devices.is_empty() {
            groups.push(self.group(Some("Signed in"), devices));
        }
        // Leaving an instance on a server: the account and everything of it. The instance
        // refuses the owner's, whose account goes with the instance.
        if !local {
            let control = if self.confirm_delete {
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        self.small_button("keep-account", "Keep it", false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.small_button("delete-account", "Delete it", true)
                            .bg(p.bad)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete = false;
                                let f = this.model.delete_account();
                                this.send(f);
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            } else {
                self.small_button("ask-delete-account", "Delete", false)
                    .text_color(p.bad)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_delete = true;
                        cx.notify();
                    }))
                    .into_any_element()
            };
            groups.push(self.group(
                Some("Your account"),
                vec![self
                    .row(
                        None,
                        "Delete my account",
                        Some(if self.confirm_delete {
                            "Every agent, chat, routine and secret of yours, for good".into()
                        } else {
                            "With everything of yours on this server".into()
                        }),
                        Some(control),
                    )
                    .into_any_element()],
            ));
        }
        self.settings_page("add-phone-page", "Add a phone", None, groups)
    }

    /// The This PC page: what agents may do on this computer, tool by tool. The core keeps
    /// the choice, and checks it before it acts.
    fn device_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut rows = Vec::new();
        for (tool, mode) in self.model.device_tools.clone() {
            let (name, what, icon) = match tool.as_str() {
                "run_command" => ("Run commands", "In PowerShell, as you", Ico::SquareTerminal),
                "read_file" => ("Read files", "Any file you can read", Ico::FileText),
                "list_files" => ("List folders", "What's in a folder", Ico::Folder),
                "screenshot" => ("Take screenshots", "Of the main screen", Ico::Monitor),
                "read_clipboard" => ("Read the clipboard", "The text you last copied", Ico::Copy),
                "write_clipboard" => ("Copy to the clipboard", "Text for you to paste", Ico::Copy),
                _ => (
                    "Open files and links",
                    "With their usual program",
                    Ico::ExternalLink,
                ),
            };
            let mut choices = Vec::new();
            for (choice, label) in [("off", "Off"), ("ask", "Ask"), ("allow", "Allow")] {
                let t = tool.clone();
                choices.push(
                    self.segment(format!("{tool}-{choice}"), label, mode == choice)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.set_device_tool(&t, choice);
                            this.send(f);
                            cx.notify();
                        })),
                );
            }
            rows.push(
                self.row(
                    Some(self.tile(icon)),
                    name,
                    Some(what.into()),
                    Some(self.segments(choices).into_any_element()),
                )
                .into_any_element(),
            );
        }
        let allowed = self
            .model
            .device_tools
            .iter()
            .any(|(t, m)| t == "run_command" && m == "allow");
        if allowed {
            rows.push(self.note_row("Agents can run any command here without asking", p.bad));
        }
        let groups = vec![self.group(
            Some("What agents may do here, Ask waits for your yes"),
            rows,
        )];
        self.settings_page("device-page", "This PC", None, groups)
    }

    /// Adding a server to the app: its address, the account, and for a new server, its
    /// setup code. It signs in beside the server on screen, and comes on screen once it's in.
    fn add_server_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let pending = self
            .adding
            .as_ref()
            .and_then(|dir| self.away.iter().find(|a| &a.dir == dir));
        let busy = pending.is_some_and(|a| a.model.logging_in);
        let error = pending.and_then(|a| a.model.login_error.clone());
        let mut rows = vec![
            self.field_row("Address", Input::new(&self.server_in)),
            self.field_row("Username", Input::new(&self.user_in)),
            self.field_row("Password", Input::new(&self.pass_in)),
        ];
        if self.show_code {
            rows.push(self.field_row("Setup code", Input::new(&self.code_in)));
        }
        if let Some(e) = error {
            rows.push(self.note_row(e, p.bad));
        }
        let actions = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(space::M))
            .child(if self.show_code {
                div().into_any_element()
            } else {
                div()
                    .id("add-server-code")
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(p.ink))
                    .child("New server? Add its setup code")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_code = true;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .child(
                self.button(
                    "add-server",
                    if busy { "Signing in…" } else { "Sign in" },
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.sign_in_added(cx))),
            );
        let groups = vec![
            self.group(None, rows),
            actions.into_any_element(),
            self.agent_group("add-server-prompt", ADD_SERVER_PROMPT, cx),
        ];
        self.settings_page("add-server-page", "Add a server", None, groups)
    }

    /// Moving the instance (PLAN.md, "Moving an instance"): the new place's address and the
    /// code its `loom receive` printed. The apps follow by themselves once it's there.
    fn move_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut groups = Vec::new();
        if let Some(to) = self.model.moving.clone() {
            groups.push(self.group(
                None,
                vec![self
                    .row(Some(self.tile(Ico::Truck)), format!("Moving to {to}…"), None, None)
                    .into_any_element()],
            ));
        } else {
            groups.push(self.group(
                None,
                vec![
                    self.field_row("New place", Input::new(&self.move_to_in)),
                    self.field_row("Move code", Input::new(&self.move_code_in)),
                ],
            ));
            groups.push(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::M))
                    .child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink_3)
                            .child("loom receive on the new server prints the code"),
                    )
                    .child(
                        self.button("move-instance", "Move", true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let to = this.move_to_in.read(cx).value().trim().to_string();
                                let code = this.move_code_in.read(cx).value().trim().to_string();
                                if let Some(f) = this.model.move_instance(&to, &code) {
                                    this.send(f);
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        groups.push(self.agent_group("move-prompt", MOVE_PROMPT, cx));
        self.settings_page("move-page", "Move this instance", None, groups)
    }

    /// The People page, for owners and admins of an instance on a server: invite someone,
    /// with a link to pass on, and see who's in, to make them an admin or remove them.
    fn people_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let caption = |text: String| {
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child(text)
        };
        let heading = |text: &'static str| {
            div()
                .text_size(px(size::BODY))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink)
                .child(text)
        };
        let mut body = div()
            .w_full()
            .max_w(px(560.0))
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .child(
                div()
                    .text_size(px(size::HEADING))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.ink)
                    .child("People"),
            );
        if let Some(why) = self.model.people_refused.clone() {
            return self.page("people-page", body.child(caption(why)));
        }
        // Invite someone: the role it gives, and the link to pass on once it's made.
        let admin = self.invite_admin;
        let mut invite = div()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(heading("Invite someone"))
            .child(caption("Works once, for a week".into()))
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        self.button("invite-member", "Member", !admin)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.invite_admin = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button("invite-admin", "Admin", admin)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.invite_admin = true;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(self.button("make-invite", "Make an invite", true).on_click(
                        cx.listener(|this, _, _, cx| {
                            let role = if this.invite_admin { "admin" } else { "member" };
                            let f = this.model.create_invite(role, 1);
                            this.send(f);
                            cx.notify();
                        }),
                    )),
            );
        if let Some((link, role, expires)) = self.model.invite_made.clone() {
            let copied = link.clone();
            invite = invite
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            div()
                                .flex_1()
                                .px(px(space::M))
                                .py(px(space::S))
                                .rounded(px(radius::BUTTON))
                                .border_1()
                                .border_color(p.line_strong)
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(link),
                        )
                        .child(self.button("copy-invite", "Copy", false).on_click(
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                            }),
                        )),
                )
                .child(caption(format!(
                    "Makes one {role}, until {}. It shows only now: send it the way you'd send a password.",
                    moment_of(expires)
                )));
        }
        body = body.child(invite);
        // Who's in.
        let mut everyone = div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(heading("Everyone"));
        for person in self.model.people.clone() {
            let mut row = div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .py(px(space::XS))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(size::SMALL))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.ink)
                                .child(person.username.clone()),
                        )
                        .child(caption(person.role.clone())),
                );
            if person.role != "owner" {
                let (user, to, label) = if person.role == "admin" {
                    (person.id.clone(), "member", "Make member")
                } else {
                    (person.id.clone(), "admin", "Make admin")
                };
                let gone = person.id.clone();
                let (reset_id, reset_name) = (person.id.clone(), person.username.clone());
                row = row
                    .child(
                        self.button_owned(format!("reset-{}", person.id), "Reset password", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.reset_password(&reset_id, &reset_name);
                                this.send(f);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button_owned(format!("role-{}", person.id), label, false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.set_role(&user, to);
                                this.send(f);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button_owned(format!("remove-{}", person.id), "Remove", false)
                            .text_color(p.bad)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.remove_person(&gone);
                                this.send(f);
                                cx.notify();
                            })),
                    );
            }
            everyone = everyone.child(row);
        }
        if let Some((who, link, expires)) = self.model.reset_made.clone() {
            let copied = link.clone();
            everyone = everyone
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            div()
                                .flex_1()
                                .px(px(space::M))
                                .py(px(space::S))
                                .rounded(px(radius::BUTTON))
                                .border_1()
                                .border_color(p.line_strong)
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(link),
                        )
                        .child(self.button("copy-reset", "Copy", false).on_click(
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                            }),
                        )),
                )
                .child(caption(format!(
                    "Sets a new password for {who}, once, until {}. It signs them out everywhere else.",
                    moment_of(expires)
                )));
        }
        body = body.child(everyone);
        // Invites still open.
        if !self.model.invites.is_empty() {
            let mut open = div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(heading("Open invites"));
            for inv in self.model.invites.clone() {
                let id = inv.id.clone();
                open = open.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .py(px(space::XS))
                        .child(div().flex_1().child(caption(format!(
                            "{}, {} {} left, until {}",
                            inv.role,
                            inv.uses,
                            if inv.uses == 1 { "use" } else { "uses" },
                            moment_of(inv.expires)
                        ))))
                        .child(
                            self.button_owned(format!("invite-{}", inv.id), "Delete", false)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let f = this.model.delete_invite(&id);
                                    this.send(f);
                                    cx.notify();
                                })),
                        ),
                );
            }
            body = body.child(open);
        }
        // What happened to people here, newest first.
        if !self.model.people_log.is_empty() {
            let mut log = div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(heading("What happened"));
            for e in self.model.people_log.clone() {
                let when = moment_of(e.at);
                let who = if e.by.is_empty() {
                    "from the server".to_string()
                } else {
                    format!("by {}", e.by)
                };
                log = log.child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(size::SMALL))
                                .text_color(p.ink)
                                .child(e.text.clone()),
                        )
                        .child(caption(format!("{when}, {who}"))),
                );
            }
            body = body.child(log);
        }
        self.page("people-page", body)
    }

    /// A page's body, centred and scrolling, under the header.
    fn page(&self, id: &'static str, body: Div) -> Div {
        let page = div()
            .id(id)
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(body);
        div().flex_1().min_h_0().child(page)
    }

    /// The window's title bar: it drags the window, with what's on it, and ends in loom's
    /// own minimise, maximise and close. On Windows they're the system's hit areas, so
    /// they behave as the system's do, Snap layouts and all. macOS keeps its traffic
    /// lights, through GPUI Kit's bar.
    fn title_bar(&self, window: &Window, content: Div, line: Hsla) -> AnyElement {
        let p = self.p;
        if cfg!(target_os = "macos") {
            return TitleBar::new()
                .h(px(layout::TITLEBAR))
                .pl(px(0.0))
                .bg(p.bg)
                .border_color(line)
                .child(content)
                .into_any_element();
        }
        let control = |id: &'static str, icon: Ico, area: WindowControlArea, close: bool| {
            div()
                .id(id)
                .w(px(34.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .text_color(p.ink_3)
                .hover(move |s| {
                    if close {
                        s.bg(p.bad).text_color(gpui::white())
                    } else {
                        s.bg(p.hover).text_color(p.ink)
                    }
                })
                .when(cfg!(target_os = "windows"), |d| d.window_control_area(area))
                .when(!cfg!(target_os = "windows"), |d| {
                    d.on_click(move |_, window, _| match area {
                        WindowControlArea::Min => window.minimize_window(),
                        WindowControlArea::Max => window.zoom_window(),
                        _ => window.remove_window(),
                    })
                })
                .child(Icon::new(icon).small())
        };
        let maximized = window.is_maximized();
        div()
            .h(px(layout::TITLEBAR))
            .flex_none()
            .flex()
            .items_center()
            .bg(p.bg)
            .border_b_1()
            .border_color(line)
            .child(
                div()
                    .id("title-drag")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .window_control_area(WindowControlArea::Drag)
                    .when(!cfg!(target_os = "windows"), |d| {
                        d.on_mouse_down(MouseButton::Left, |_, window, _| {
                            window.start_window_move()
                        })
                    })
                    .child(content),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .px(px(space::S))
                    .child(control(
                        "win-min",
                        Ico::Minus,
                        WindowControlArea::Min,
                        false,
                    ))
                    .child(control(
                        "win-max",
                        if maximized { Ico::Copy } else { Ico::Square },
                        WindowControlArea::Max,
                        false,
                    ))
                    .child(control("win-close", Ico::X, WindowControlArea::Close, true)),
            )
            .into_any_element()
    }

    // --- The settings kit ---
    //
    // Every page of the settings and server popups is built from these, the way the
    // system settings of macOS, iOS and Windows are: a title with the page's one
    // action beside it, then groups, each a box of rows with what it is on the left
    // and its control on the right, and one short line under a name at most.

    /// A settings page: its title, with its one action on the right, then its groups,
    /// in a column that scrolls.
    fn settings_page(
        &self,
        id: &'static str,
        title: &str,
        action: Option<AnyElement>,
        groups: Vec<AnyElement>,
    ) -> Div {
        let p = self.p;
        let column = div()
            .w_full()
            .max_w(px(600.0))
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .child(
                div()
                    .min_h(px(34.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::M))
                    .child(
                        div()
                            .text_size(px(size::HEADING - 2.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.ink)
                            .child(title.to_string()),
                    )
                    .children(action),
            )
            .children(groups);
        let page = div()
            .id(id)
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(column);
        div().flex_1().min_h_0().child(page)
    }

    /// The fill of a group's box: a step off the page.
    fn card_fill(&self) -> Hsla {
        if self.p.dark {
            self.p.raised
        } else {
            self.p.side
        }
    }

    /// A group: its name over a box of rows, with a hairline between each.
    fn group(&self, title: Option<&str>, rows: Vec<AnyElement>) -> AnyElement {
        let p = self.p;
        let mut boxed = div()
            .flex()
            .flex_col()
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(p.line)
            .bg(self.card_fill())
            .overflow_hidden();
        for (i, row) in rows.into_iter().enumerate() {
            boxed = boxed.child(
                div()
                    .when(i > 0, |d| d.border_t_1().border_color(p.line))
                    .child(row),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .when_some(title, |d, t| {
                d.child(
                    div()
                        .px(px(space::XS))
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_3)
                        .child(t.to_string()),
                )
            })
            .child(boxed)
            .into_any_element()
    }

    /// A row's icon, in a small square of its own.
    fn tile(&self, icon: Ico) -> AnyElement {
        let p = self.p;
        div()
            .size(px(30.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .border_1()
            .border_color(p.line)
            .bg(p.bg)
            .text_color(p.ink_2)
            .child(Icon::new(icon).small())
            .into_any_element()
    }

    /// A row: what it is on the left, with one short line under it, and its control
    /// on the right.
    fn row(
        &self,
        lead: Option<AnyElement>,
        title: impl Into<SharedString>,
        sub: Option<String>,
        control: Option<AnyElement>,
    ) -> Div {
        let p = self.p;
        div()
            .min_h(px(54.0))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::L))
            .py(px(space::S + 2.0))
            .children(lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(size::BODY))
                            .text_color(p.ink)
                            .truncate()
                            .child(title.into()),
                    )
                    .when_some(sub, |d, s| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .truncate()
                                .child(s),
                        )
                    }),
            )
            .when_some(control, |d, c| d.child(div().flex_none().child(c)))
    }

    /// A row of words alone, wrapping, such as a warning or how to use something.
    fn note_row(&self, text: impl Into<SharedString>, ink: Hsla) -> AnyElement {
        div()
            .px(px(space::L))
            .py(px(space::M))
            .text_size(px(size::SMALL))
            .line_height(rems(1.45))
            .text_color(ink)
            .child(text.into())
            .into_any_element()
    }

    /// A row a field fills, under its label.
    fn field_row(&self, label: &str, input: impl IntoElement) -> AnyElement {
        let p = self.p;
        div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::L))
            .py(px(space::S + 2.0))
            .child(
                div()
                    .w(px(120.0))
                    .flex_none()
                    .text_size(px(size::BODY))
                    .text_color(p.ink)
                    .child(label.to_string()),
            )
            .child(div().flex_1().min_w_0().child(input))
            .into_any_element()
    }

    /// A small button, for a row.
    fn small_button(
        &self,
        id: impl Into<SharedString>,
        label: &str,
        primary: bool,
    ) -> Stateful<Div> {
        self.button_owned(id, label, primary)
            .h(px(30.0))
            .px(px(space::M))
    }

    /// An on and off switch.
    fn switch(&self, id: impl Into<SharedString>, on: bool) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id.into())
            .w(px(38.0))
            .h(px(22.0))
            .flex_none()
            .flex()
            .items_center()
            .when(on, |d| d.justify_end())
            .p(px(3.0))
            .rounded(px(radius::ROW))
            .bg(if on { p.button } else { p.line_strong })
            .cursor_pointer()
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(4.0))
                    .bg(if on { p.button_ink } else { p.bg })
                    .shadow_sm(),
            )
    }

    /// A choice of a few, side by side, as one control.
    fn segments(&self, items: Vec<Stateful<Div>>) -> Div {
        let p = self.p;
        div()
            .flex()
            .p(px(2.0))
            .gap(px(2.0))
            .rounded(px(radius::ROW + 2.0))
            .bg(p.raised_2)
            .children(items)
    }

    /// One choice of `segments`.
    fn segment(&self, id: impl Into<SharedString>, label: &str, on: bool) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id.into())
            .h(px(26.0))
            .px(px(space::M))
            .flex()
            .items_center()
            .rounded(px(radius::ROW))
            .text_size(px(size::SMALL))
            .cursor_pointer()
            .when(on, |d| {
                d.bg(p.bg)
                    .text_color(p.ink)
                    .font_weight(FontWeight::MEDIUM)
                    .shadow_sm()
            })
            .when(!on, |d| {
                d.text_color(p.ink_3).hover(move |s| s.text_color(p.ink))
            })
            .child(label.to_string())
    }

    /// What a page shows with nothing on it yet: what would be here, in a line, and
    /// the way to make the first.
    fn empty(&self, icon: Ico, title: &str, line: &str, action: Option<AnyElement>) -> AnyElement {
        let p = self.p;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(space::S))
            .py(px(space::XXL + space::S))
            .px(px(space::XL))
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(p.line)
            .bg(self.card_fill())
            .child(
                div()
                    .size(px(44.0))
                    .mb(px(space::XS))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::BUTTON))
                    .border_1()
                    .border_color(p.line)
                    .bg(p.bg)
                    .text_color(p.ink_2)
                    .child(Icon::new(icon)),
            )
            .child(
                div()
                    .text_size(px(size::BODY))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink)
                    .child(title.to_string()),
            )
            .child(
                div()
                    .max_w(px(340.0))
                    .text_center()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child(line.to_string()),
            )
            .when_some(action, |d, a| d.child(div().pt(px(space::M)).child(a)))
            .into_any_element()
    }

    /// A technical job an agent can do instead, as a group of its own at the end of a
    /// page: a row saying so, and a button that copies the prompt to give it.
    fn agent_group(
        &self,
        id: &'static str,
        prompt: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let copy = self
            .small_button(id, "Copy prompt", false)
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(prompt.to_string()));
                this.notice = Some((
                    "Copied. Paste it to one of your agents.".into(),
                    Instant::now(),
                ));
                cx.notify();
            }));
        let row = self.row(
            Some(self.tile(Ico::Bot)),
            "Let an agent do it",
            Some("A prompt to paste to one of your agents".into()),
            Some(copy.into_any_element()),
        );
        self.group(None, vec![row.into_any_element()])
    }

    /// The programs page: the agent programs on the instance, with their settings, and the
    /// ACP Registry's agents to install.
    fn programs_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let heading = |text: &'static str| {
            div()
                .text_size(px(size::HEADING))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink)
                .child(text)
        };
        let caption = |text: String| {
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child(text)
        };
        let mut mine = div().flex().flex_col().gap(px(space::XS));
        if self.model.installed.is_empty() {
            mine = mine.child(caption("None yet".into()));
        }
        for prog in self.model.installed.clone() {
            let open = self.program_open.as_deref() == Some(prog.name.as_str());
            let newer = self
                .model
                .registry
                .iter()
                .find(|l| l.id == prog.source && !prog.source.is_empty())
                .filter(|l| l.version != prog.version)
                .map(|l| l.version.clone());
            let shown = self
                .model
                .registry
                .iter()
                .find(|l| l.id == prog.source && !prog.source.is_empty())
                .map_or(prog.name.clone(), |l| l.name.clone());
            let what = if prog.source.is_empty() {
                "Added on the instance".to_string()
            } else {
                format!("From the ACP Registry, version {}", prog.version)
            };
            let name = prog.name.clone();
            let mut row = div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .py(px(space::S))
                .px(px(space::S))
                .rounded(px(radius::BUTTON))
                .child(
                    div()
                        .id(SharedString::from(format!("program-{}", prog.name)))
                        .flex()
                        .items_center()
                        .justify_between()
                        .cursor_pointer()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(size::SMALL))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(p.ink)
                                        .child(shown),
                                )
                                .child(caption(what)),
                        )
                        .child(caption(match (&prog.share, prog.settings.len()) {
                            (Some(sh), _) => share_words(sh),
                            (None, 0) => "No settings".to_string(),
                            (None, 1) => "1 setting".to_string(),
                            (None, n) => format!("{n} settings"),
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.program_open =
                                if this.program_open.as_deref() == Some(name.as_str()) {
                                    None
                                } else {
                                    Some(name.clone())
                                };
                            for s in [&this.set_key_in, &this.set_value_in] {
                                s.update(cx, |s, cx| s.set_value("", window, cx));
                            }
                            cx.notify();
                        })),
                );
            if open {
                let mut settings = div().flex().flex_col().gap(px(space::XS));
                for key in prog.settings.clone() {
                    let (program, k) = (prog.name.clone(), key.clone());
                    settings = settings.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink_2)
                                    .child(format!("{key} = ••••")),
                            )
                            .child(
                                self.button_owned(
                                    format!("unset-{}-{key}", prog.name),
                                    "Remove",
                                    false,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let f = this.model.set_setting(&program, &k, None);
                                        this.send(f);
                                        cx.notify();
                                    },
                                )),
                            ),
                    );
                }
                let program = prog.name.clone();
                let mut buttons = div().flex().gap(px(space::S)).child(
                    self.button_owned(format!("set-{}", prog.name), "Save setting", true)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let key = this.set_key_in.read(cx).value().to_string();
                            let value = this.set_value_in.read(cx).value().to_string();
                            if key.trim().is_empty() {
                                this.model.notices.push(
                                    "A setting needs a name, such as ANTHROPIC_API_KEY.".into(),
                                );
                                cx.notify();
                                return;
                            }
                            let f = this.model.set_setting(&program, &key, Some(&value));
                            this.send(f);
                            for s in [&this.set_key_in, &this.set_value_in] {
                                s.update(cx, |s, cx| s.set_value("", window, cx));
                            }
                            cx.notify();
                        })),
                );
                if let Some(version) = newer {
                    let source = prog.source.clone();
                    buttons = buttons.child(
                        self.button_owned(
                            format!("update-{}", prog.name),
                            &format!("Update to {version}"),
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.install(&source);
                            this.send(f);
                            cx.notify();
                        })),
                    );
                }
                let program = prog.name.clone();
                buttons = buttons.child(
                    self.button_owned(format!("remove-{}", prog.name), "Remove", false)
                        .text_color(p.bad)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.remove_program(&program);
                            this.send(f);
                            this.program_open = None;
                            cx.notify();
                        })),
                );
                // Sharing the settings with everyone else, a number of tokens each.
                let month = self.share_month;
                let program = prog.name.clone();
                let mut sharing = div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(div().w(px(180.0)).child(Input::new(&self.share_in)))
                    .child(
                        self.button_owned(format!("share-day-{}", prog.name), "a day", !month)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.share_month = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button_owned(format!("share-month-{}", prog.name), "a month", month)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.share_month = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        self.button_owned(format!("share-{}", prog.name), "Share", false)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let given = this.share_in.read(cx).value().replace([',', ' '], "");
                                let Ok(tokens) = given.parse::<i64>() else {
                                    this.model.notices.push(
                                        "Give the tokens each person may use, as a number.".into(),
                                    );
                                    cx.notify();
                                    return;
                                };
                                let per = if this.share_month { "month" } else { "day" };
                                let f = this.model.share_program(&program, tokens, per);
                                this.send(f);
                                this.share_in
                                    .update(cx, |s, cx| s.set_value("", window, cx));
                                cx.notify();
                            })),
                    );
                if prog.share.is_some() {
                    let program = prog.name.clone();
                    sharing = sharing.child(
                        self.button_owned(format!("unshare-{}", prog.name), "Stop sharing", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.share_program(&program, 0, "day");
                                this.send(f);
                                cx.notify();
                            })),
                    );
                }
                row = row
                    .bg(p.hover)
                    .child(settings)
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(div().flex_1().child(Input::new(&self.set_key_in)))
                            .child(div().flex_1().child(Input::new(&self.set_value_in))),
                    )
                    .child(buttons)
                    .child(caption("Share with the people you've invited".into()))
                    .child(sharing);
            }
            mine = mine.child(row);
        }

        let mut registry = div().flex().flex_col().gap(px(space::XS));
        if self.model.registry_loading {
            registry = registry.child(caption("Reading the registry…".into()));
        } else if let Some(e) = self.model.registry_error.clone() {
            registry = registry.child(caption(e));
        }
        for l in self.model.registry.clone() {
            if l.installed.is_some() {
                continue;
            }
            let action: AnyElement = if self.model.installing.as_deref() == Some(l.id.as_str()) {
                caption("Installing…".into()).into_any_element()
            } else if l.how.is_some() {
                let id = l.id.clone();
                self.button_owned(format!("install-{}", l.id), "Install", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.install(&id);
                        this.send(f);
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                caption("Not for this computer".into()).into_any_element()
            };
            registry = registry.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::M))
                    .py(px(space::S))
                    .px(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink)
                                    .child(l.name.clone()),
                            )
                            .child(caption(l.description.clone())),
                    )
                    .child(div().flex_none().child(action)),
            );
        }
        let body = div()
            .w_full()
            .max_w(px(560.0))
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .child(heading("On this instance"))
            .child(mine)
            .child(heading("From the ACP Registry"))
            .child(registry);
        let page = div()
            .id("programs-page")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(body);
        div().flex_1().min_h_0().child(page)
    }

    /// The About you page: the person's profile, which every agent's session starts with.
    fn profile_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let said = match &self.model.profile {
            Some((_, revision, by)) if *revision > 0 => {
                format!("Revision {revision}, by {}", self.model.made_by(by))
            }
            Some(_) => "Nothing yet".to_string(),
            None => "Loading…".to_string(),
        };
        let save = self
            .small_button("save-profile", "Save", true)
            .on_click(cx.listener(|this, _, _, cx| {
                let text = this.profile_in.read(cx).value().to_string();
                let f = this.model.save_profile(&text);
                this.send(f);
                cx.notify();
            }))
            .into_any_element();
        let groups = vec![self.group(
            Some("What every agent knows about you"),
            vec![
                div()
                    .p(px(space::L))
                    .child(Textarea::new(&self.profile_in))
                    .into_any_element(),
                div()
                    .px(px(space::L))
                    .py(px(space::S + 2.0))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(said)
                    .into_any_element(),
            ],
        )];
        self.settings_page("profile-page", "About you", Some(save), groups)
    }

    /// The memory page: the selected agent's notes and the shared ones, newest first, to
    /// add to or delete from.
    fn memory_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col();
        if self.model.notes.is_empty() {
            list = list.child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("Nothing remembered yet"),
            );
        }
        for n in self.model.notes.clone() {
            let who = if n.shared {
                "Shared".to_string()
            } else {
                self.model.made_by(&n.by)
            };
            let note = n.id;
            list = list.child(
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
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink)
                                    .child(n.text.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(format!("{who} · {} {}", day_of(n.at), time_of(n.at))),
                            ),
                    )
                    .child(
                        self.button_owned(format!("forget-{note}"), "Delete", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.delete_note(note);
                                this.send(f);
                                cx.notify();
                            })),
                    ),
            );
        }
        let add = |shared: bool| {
            cx.listener(
                move |this: &mut Self,
                      _: &ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<Self>| {
                    let text = this.note_in.read(cx).value().to_string();
                    if text.trim().is_empty() {
                        return;
                    }
                    if let Some(f) = this.model.add_note(&text, shared) {
                        this.send(f);
                    }
                    this.note_in.update(cx, |s, cx| s.set_value("", window, cx));
                    cx.notify();
                },
            )
        };
        let body = div()
            .w_full()
            .max_w(px(560.0))
            .flex()
            .flex_col()
            .gap(px(space::L))
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(div().flex_1().child(Input::new(&self.note_in)))
                    .child(self.button("note-own", "Add", true).on_click(add(false)))
                    .child(
                        self.button("note-shared", "Add for all", false)
                            .on_click(add(true)),
                    ),
            )
            .child(list);
        let page = div()
            .id("memory-page")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(body);
        div().flex_1().min_h_0().child(page)
    }

    /// The activity page: what the selected agent did, and what was done to it, newest
    /// first.
    fn activity_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col();
        if self.model.activity.is_empty() {
            list = list.child(
                div()
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("Nothing yet."),
            );
        }
        let mut day = String::new();
        for e in self.model.activity.clone() {
            let this_day = day_of(e.at);
            if this_day != day {
                list = list.child(
                    div()
                        .pt(px(space::L))
                        .pb(px(space::XS))
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_3)
                        .child(this_day.clone()),
                );
                day = this_day;
            }
            let who = match e.by.as_str() {
                "" => String::new(),
                "auto" => "its permission mode".into(),
                by => self.model.made_by(by),
            };
            let what = match e.kind.as_str() {
                "tool" => "Ran",
                "approval" => "Asked",
                "message" => "Handed over",
                "notify" => "Notified",
                "memory" => "Remembered",
                "routine" => "Routine",
                "device" => "On a device",
                _ => "Changed",
            };
            list = list.child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(space::M))
                    .py(px(space::XS + 2.0))
                    .border_b_1()
                    .border_color(p.line)
                    .child(
                        div()
                            .w(px(44.0))
                            .flex_none()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(time_of(e.at)),
                    )
                    .child(
                        div()
                            .w(px(88.0))
                            .flex_none()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(what),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink)
                                    .child(e.text),
                            )
                            .when(
                                matches!(e.status.as_str(), "started" | "failed" | "refused"),
                                |d| {
                                    let (said, colour) = match e.status.as_str() {
                                        "started" => ("Not finished yet".to_string(), p.ink_3),
                                        "refused" => (format!("Refused: {}", e.why), p.bad),
                                        _ => (format!("Failed: {}", e.why), p.bad),
                                    };
                                    d.child(
                                        div()
                                            .text_size(px(size::CAPTION))
                                            .text_color(colour)
                                            .child(said),
                                    )
                                },
                            ),
                    )
                    .when(!who.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .child(who),
                        )
                    }),
            );
        }
        let page = div()
            .id("activity")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::XL))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(
                div()
                    .w_full()
                    .max_w(px(640.0))
                    .flex()
                    .flex_col()
                    .child(
                        div().flex().justify_end().child(
                            self.button("export-activity", "Export as CSV", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(f) = this.model.export_activity() {
                                        this.send(f);
                                    }
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(list),
            );
        div().flex_1().min_h_0().child(page)
    }

    /// Asks where to save the activity log `activity.export` gave, and saves it there.
    fn save_export(&mut self, csv: String, cx: &mut Context<Self>) {
        let downloads = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .map(|h| std::path::Path::new(&h).join("Downloads"))
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        let name = format!(
            "loom activity {}.csv",
            chrono::Local::now().format("%Y-%m-%d")
        );
        let picked = cx.prompt_for_new_path(&downloads, Some(&name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = picked.await {
                let said = match std::fs::write(&path, csv.as_bytes()) {
                    Ok(()) => format!("Saved the activity log to {}", path.display()),
                    Err(e) => format!("The activity log couldn't be saved: {e}"),
                };
                let _ = this.update(cx, |loom, cx| {
                    loom.model.notices.push(said);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The open chat's pinned messages, beside it: who, when and the start of each, to
    /// jump to, or to take the pin off.
    fn pins_page(&mut self, agent: &str, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(space::S));
        if self.model.pins.is_empty() {
            list = list.child(
                div()
                    .pt(px(space::L))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("Nothing pinned yet. Pin a message from its corner."),
            );
        }
        for m in self.model.pins.clone() {
            let who = if m.role == "person" {
                "You".to_string()
            } else {
                agent.to_string()
            };
            let words: String = m
                .blocks
                .iter()
                .filter(|b| b.kind == "text")
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let words = if words.trim().is_empty() {
                "A file".to_string()
            } else {
                let short: String = words.chars().take(240).collect();
                if short.len() < words.len() {
                    format!("{}…", short.trim_end())
                } else {
                    short
                }
            };
            let id = m.id.clone();
            let unpin = m.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("pin-{}", m.id)))
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .p(px(space::M))
                    .rounded(px(radius::CARD))
                    .border_1()
                    .border_color(p.line)
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.jump_to(&id, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink_2)
                                    .child(who),
                            )
                            .child(format!("{} {}", day_of(m.at), time_of(m.at)))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id(SharedString::from(format!("unpin-{}", m.id)))
                                    .size(px(22.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(radius::ROW))
                                    .hover(move |s| s.bg(p.active).text_color(p.ink))
                                    .child(Icon::new(Ico::PinOff).xsmall())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let f = this.model.pin(&unpin, false);
                                        this.send(f);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(size::SMALL))
                            .text_color(p.ink)
                            .child(words),
                    ),
            );
        }
        let page = div()
            .id("pins")
            .size_full()
            .overflow_y_scroll()
            .px(px(space::L))
            .pt(px(space::M))
            .pb(px(space::XXL))
            .child(list);
        div().flex_1().min_h_0().child(page)
    }

    /// Side questions about the open chat, beside it: what was asked and the answers,
    /// and a box to ask another. None of it goes in the chat.
    fn aside_page(&mut self, agent: &str) -> Div {
        let p = self.p;
        let selected = self.model.selected.clone().unwrap_or_default();
        let asides: Vec<model::Aside> = self
            .model
            .asides
            .iter()
            .filter(|a| a.agent == selected)
            .cloned()
            .collect();
        let mut list = div().flex().flex_col().gap(px(space::L));
        if asides.is_empty() {
            list = list.child(
                div()
                    .pt(px(space::L))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child(format!(
                        "Ask {agent} about this chat. The question and the answer stay out of it."
                    )),
            );
        }
        for a in asides {
            list = list
                .child(
                    div().flex().justify_end().child(
                        div()
                            .max_w(px(300.0))
                            .px(px(space::M))
                            .py(px(space::S))
                            .rounded(px(radius::CARD))
                            .bg(p.raised)
                            .text_size(px(size::SMALL))
                            .text_color(p.ink)
                            .child(a.question.clone()),
                    ),
                )
                .child(match (&a.answer, &a.error) {
                    (Some(answer), _) => div().text_size(px(size::SMALL)).text_color(p.ink).child(
                        if answer.trim().is_empty() {
                            "(Nothing to say.)".to_string()
                        } else {
                            answer.clone()
                        },
                    ),
                    (None, Some(e)) => div()
                        .text_size(px(size::SMALL))
                        .text_color(p.bad)
                        .child(e.clone()),
                    (None, None) => div()
                        .text_size(px(size::SMALL))
                        .text_color(p.ink_3)
                        .child("Thinking…"),
                });
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .id("asides")
                        .size_full()
                        .overflow_y_scroll()
                        .px(px(space::L))
                        .pt(px(space::M))
                        .pb(px(space::L))
                        .child(list),
                ),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(space::L))
                    .pb(px(space::L))
                    .child(Input::new(&self.aside_in).large()),
            )
    }

    /// Scrolls the chat to a message, when it's among those loaded.
    fn jump_to(&mut self, message: &str, cx: &mut Context<Self>) {
        let at = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Message(m) if m.id == message));
        match at {
            Some(ix) => {
                self.scroller.update(cx, |s, cx| {
                    s.scroll_to_item(ix, cx);
                });
            }
            None => self
                .model
                .notices
                .push("That one is further back than the chat has loaded.".into()),
        }
        cx.notify();
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
                    TextView::markdown(
                        SharedString::from(format!("m-{}", m.id)),
                        model::literal_html(&text),
                    )
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
    let id = SharedString::from(format!("file-{key}"));
    // A page an agent made opens beside the chat, walled off, not in a browser.
    if f.page() {
        return div()
            .id(id)
            .w_full()
            .max_w(px(320.0))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::M))
            .rounded(px(radius::ROW))
            .border_1()
            .border_color(p.line)
            .bg(p.bubble)
            .cursor_pointer()
            .hover(move |s| s.bg(p.hover))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |loom, cx| loom.open_page(&file, cx));
            })
            .child(Icon::new(Ico::AppWindow).text_color(p.ink_2))
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
                            .child(f.title()),
                    )
                    .child(
                        div()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child("Page · opens beside the chat"),
                    ),
            )
            .into_any_element();
    }
    let open = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        let _ = this.update(cx, |loom, cx| loom.open_file(&file, cx));
    };
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
        .child(Icon::new(Ico::FileText).small().text_color(p.ink_3))
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
    // The files it names, each a chip with its type, which opens it.
    if !m.mentions.is_empty() {
        let mut chips = div().flex().flex_wrap().gap(px(space::S));
        for (i, mention) in m.mentions.iter().enumerate() {
            let this = this.clone();
            let opened = mention.clone();
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("mention-{}-{i}", m.id)))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::M))
                    .py(px(space::XS))
                    .rounded(px(radius::ROW))
                    .border_1()
                    .border_color(p.line)
                    .bg(p.raised)
                    .text_size(px(size::SMALL))
                    .cursor_pointer()
                    .hover(move |s| s.bg(p.hover).border_color(p.line_strong))
                    .child(
                        div()
                            .text_color(p.ink_3)
                            .child(Icon::new(Ico::FileText).xsmall()),
                    )
                    .child(div().text_color(p.ink).child(mention.name.clone()))
                    .child(
                        div()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child(mention.kind()),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |loom, _| loom.open_mention(&opened));
                    }),
            );
        }
        body = body.child(chips);
    }
    div()
        .w_full()
        .flex()
        .gap(px(space::M))
        .child(div().pt(px(4.0)).child(thread(
            format!("msg-{}", m.id),
            mood,
            tint,
            14.0,
            10.0,
            None,
        )))
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

/// The day of `ms`, as the activity page heads its entries: "Today", "Yesterday", or a date.
fn day_of(ms: i64) -> String {
    use chrono::TimeZone;
    let chrono::LocalResult::Single(t) = chrono::Local.timestamp_millis_opt(ms) else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    let day = t.date_naive();
    if day == today {
        "Today".into()
    } else if Some(day) == today.pred_opt() {
        "Yesterday".into()
    } else {
        day.format("%-d %B %Y").to_string()
    }
}

/// When `ms` is: its time today, or its day and time, such as for an invite a week on.
fn moment_of(ms: i64) -> String {
    let day = day_of(ms);
    if day == "Today" {
        time_of(ms)
    } else {
        format!("{day} {}", time_of(ms))
    }
}

/// The side of one module of a drawn QR code, in pixels.
const QR_CELL: f32 = 6.0;

/// A QR code of `text`, dark on white with the quiet zone around it, each row's runs of
/// dark modules drawn as one box.
fn qr_view(text: &str) -> Div {
    let white: Hsla = rgb(0xffffff).into();
    let black: Hsla = rgb(0x000000).into();
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else {
        return div().child("This code is too long to draw.");
    };
    let width = code.width();
    let colors = code.to_colors();
    let mut grid = div()
        .flex()
        .flex_col()
        .p(px(QR_CELL * 4.0))
        .bg(white)
        .rounded(px(radius::BUTTON));
    for row in colors.chunks(width) {
        let mut line = div().flex().h(px(QR_CELL));
        let mut x = 0;
        while x < width {
            let dark = row[x] == qrcode::Color::Dark;
            let mut n = 1;
            while x + n < width && (row[x + n] == qrcode::Color::Dark) == dark {
                n += 1;
            }
            let cell = div().w(px(QR_CELL * n as f32)).h(px(QR_CELL));
            line = line.child(if dark { cell.bg(black) } else { cell });
            x += n;
        }
        grid = grid.child(line);
    }
    div().flex().child(grid)
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

/// When a message was sent, as its hover shows it: "14:05" today, "Yesterday 14:05",
/// "21 Nov 14:05" this year, and "21 Nov 2025 14:05" before.
fn when_sent(ms: i64, now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::{Datelike, TimeZone};
    if ms <= 0 || ms == i64::MAX {
        return String::new();
    }
    let chrono::LocalResult::Single(t) = chrono::Local.timestamp_millis_opt(ms) else {
        return String::new();
    };
    let (day, today) = (t.date_naive(), now.date_naive());
    if day == today {
        t.format("%H:%M").to_string()
    } else if today.pred_opt() == Some(day) {
        t.format("Yesterday %H:%M").to_string()
    } else if day.year() == today.year() {
        t.format("%-d %b %H:%M").to_string()
    } else {
        t.format("%-d %b %Y %H:%M").to_string()
    }
}

#[cfg(test)]
mod when_tests {
    use super::when_sent;
    use chrono::TimeZone;

    #[test]
    fn a_messages_time_says_the_day_when_it_isnt_today() {
        let at = |y, mo, d, h, mi| {
            chrono::Local
                .with_ymd_and_hms(y, mo, d, h, mi, 0)
                .single()
                .unwrap()
        };
        let now = at(2026, 11, 21, 20, 0);
        let ms = |t: chrono::DateTime<chrono::Local>| t.timestamp_millis();
        assert_eq!(when_sent(ms(at(2026, 11, 21, 14, 5)), now), "14:05");
        assert_eq!(
            when_sent(ms(at(2026, 11, 20, 9, 30)), now),
            "Yesterday 09:30"
        );
        assert_eq!(when_sent(ms(at(2026, 3, 2, 8, 0)), now), "2 Mar 08:00");
        assert_eq!(
            when_sent(ms(at(2025, 12, 31, 23, 59)), now),
            "31 Dec 2025 23:59"
        );
        assert_eq!(when_sent(0, now), "");
    }
}

impl Render for Loom {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        self.finish_weaving(window, cx);
        if let Some(address) = self.fill_server.take() {
            self.server_in
                .update(cx, |s, cx| s.set_value(address, window, cx));
        }
        let signed_out = self.model.conn == Some(Conn::SignedOut);
        // The opening weave plays once the app knows it's signed in. Signed out, the
        // welcome comes in on its own instead.
        if self.launched.is_none() {
            if signed_out {
                self.launched = Instant::now().checked_sub(Duration::from_secs(60));
            } else if self.model.conn.is_some() {
                self.launched = Some(Instant::now());
            }
        }
        // With other servers to go back to, it's made beside the sidebar instead.
        let first = (self.screen == Screen::Chat && self.model.agents.is_empty()
            || self.make.step == Step::Weaving && self.make.first)
            && self.away.is_empty();
        let scene = if self.model.conn.is_none() {
            "blank"
        } else if signed_out {
            "welcome"
        } else if first || self.screen == Screen::NewAgent {
            match self.make.step {
                // The tiles come in once the programs are known, not before.
                Step::Brain if !self.model.programs_known => "brain-wait",
                Step::Brain => "brain",
                Step::Name => "name",
                Step::Weaving => "weaving",
            }
        } else {
            "app"
        };
        if self.scene.0 != scene {
            self.scene = (scene, Instant::now());
        }
        if std::mem::take(&mut self.model.profile_came) {
            let text = self
                .model
                .profile
                .as_ref()
                .map(|p| p.0.clone())
                .unwrap_or_default();
            self.profile_in
                .update(cx, |s, cx| s.set_value(text, window, cx));
        }
        if self
            .model
            .agents
            .iter()
            .any(|a| thread::moves(self.model.mood(&a.id), self.model.finished_at(&a.id)))
            || self.model.conn == Some(Conn::SignedOut)
            || (self.screen == Screen::Chat && self.model.agents.is_empty())
            || self.screen == Screen::NewAgent
            || self.make.step == Step::Weaving
            || self
                .launched
                .is_some_and(|at| at.elapsed().as_secs_f32() < SPLASH_SECONDS)
            || self.scene.1.elapsed().as_secs_f32() < 2.5
            || self
                .closing
                .is_some_and(|c| c.0.elapsed().as_secs_f32() < CLOSE_SECONDS)
            // A page arriving: its thread unrolls it, then twangs hello.
            || self
                .opened()
                .checked_duration_since(Instant::now())
                .is_some()
            || self.opened().elapsed().as_secs_f32() < 1.5
            // An agent's page beside the chat, whose thread fidgets at its top.
            || matches!(
                self.side,
                Some(
                    Screen::EditAgent
                        | Screen::Activity
                        | Screen::Memory
                        | Screen::Pins
                        | Screen::Aside
                )
            ) && self.screen == Screen::Chat
            // Someone typing to an agent, whose thread thinks along.
            || self.screen == Screen::Chat && !self.composer.read(cx).value().trim().is_empty()
        {
            window.request_animation_frame();
        }
        let h = window.viewport_size().height.as_f32();
        let splash = self.splash(h);
        // A new agent's curtain covers what its weave covered: the whole window for the
        // first, the content beside the sidebar for another.
        let closing = self.closing(h);
        let first_closing = self.closing.is_some_and(|c| c.2);
        let (whole, beside) = if first_closing {
            (closing, None)
        } else {
            (None, closing)
        };
        // A new page in the app unrolls under a thread that passes down it, like a shuttle,
        // in the agent's colour for a chat. Not when the app has only just opened: the
        // opening weave or a new agent's curtain does that.
        let page = (self.screen, self.model.selected.clone());
        if self.page.0 != page {
            let settled = self.scene.0 == "app" && self.scene.1.elapsed().as_secs_f32() > 0.2;
            if settled {
                window.request_animation_frame();
            }
            self.page = (
                page,
                if settled {
                    Instant::now()
                } else {
                    Instant::now()
                        .checked_sub(Duration::from_secs(10))
                        .unwrap_or_else(Instant::now)
                },
            );
        }
        let unroll = self.unroll(h);
        let root = div()
            .track_focus(&self.keys)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.escape(cx);
                }
            }))
            .relative()
            .size_full()
            .bg(p.bg)
            .text_color(p.ink)
            .font_family(font::UI)
            .text_size(px(size::BODY));
        // Until the core says how things are, only the window's background: nothing to
        // pop in and out.
        // The welcome and making the first agent fill the window, with no app around them.
        if scene == "blank" || signed_out || first {
            let page = if scene == "blank" {
                div()
            } else if signed_out {
                self.sign_in(cx)
            } else {
                self.first_agent(cx)
            };
            return root
                .flex()
                .flex_col()
                .child(self.title_bar(window, div(), transparent_black()))
                .child(div().flex_1().min_h_0().flex().flex_col().child(page))
                .when_some(splash, |d, s| d.child(s));
        }
        // The main area is the chat, unless an agent is being made.
        let main = match self.screen {
            Screen::NewAgent => self.new_agent(cx),
            Screen::Chat if self.model.agents.is_empty() && self.model.synced => {
                self.first_agent(cx)
            }
            _ => self.chat(window, cx),
        };
        self.sync_panel(window, cx);
        let header = self.header(cx);
        // Drawn last, over the sheet that takes the pointer while it's open.
        let menu = self.menu_panel(cx).map(deferred);
        let menu_scrim = self.menu_scrim(cx);
        let popup = self.popup_view(window, cx);
        root.flex()
            .child(self.sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .relative()
                    .child(self.title_bar(window, header, p.line))
                    .child(main)
                    .when_some(menu, |d, m| d.child(m))
                    .when_some(beside, |d, c| d.child(c))
                    .when_some(unroll, |d, u| d.child(u)),
            )
            .when_some(popup, |d, p| d.child(p))
            .when_some(menu_scrim, |d, s| d.child(s))
            .when_some(whole, |d, c| d.child(c))
            .when_some(splash, |d, s| d.child(s))
    }
}

/// loom's mark, the knot, with the wordmark beside it (design/logo/README.md), "loom" at
/// `text` pixels, in a box of a set width, so threads woven through it know where it is.
/// Gives it, and half its width.
fn wordmark(p: Palette, text: f32) -> (Div, f32) {
    let knot = text * 1.2;
    let gap = text * 0.16;
    let word = text * 2.3;
    let width = knot + gap + word;
    let mark = div()
        .w(px(width))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(gap))
        .child(
            svg()
                .path("logo/knot.svg")
                .size(px(knot))
                .flex_none()
                .text_color(p.ink),
        )
        .child(
            div()
                .text_size(px(text))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink)
                .child("loom"),
        );
    (mark, width / 2.0)
}

/// A program's share, in words: what each person gets, and what's left of the
/// reader's own, when it applies to them.
fn share_words(sh: &model::Share) -> String {
    let per = if sh.per == "month" { "month" } else { "day" };
    match sh.left {
        Some(left) => format!(
            "Shared: {} of {} tokens left this {per}",
            thousands(left),
            thousands(sh.tokens)
        ),
        None => format!("Shared: {} tokens a {per} each", thousands(sh.tokens)),
    }
}

/// 12345 as 12,345.
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// What "Copy prompt" gives for moving the instance, for an agent in loom to do it with
/// its tools (move_instance, install_server), asking the person what it needs.
const MOVE_PROMPT: &str = "Move my loom instance to another server for me. Ask me where it should go (a server I can reach, or one to rent) if I haven't said. If loom isn't on it yet, use install_server to get the command that puts it there, and run it or walk me through it. Then use move_instance to move this instance there, and tell me when it's done and what changed for me.";

/// What "Copy prompt" gives for adding a server, for an agent in loom to set it up with
/// its tools (install_server, add_server).
const ADD_SERVER_PROMPT: &str = "Set up a new loom server for me and add it to my app. Ask me where it should run (a server I can reach, or a VPS to rent) if I haven't said. Use install_server to get the command that installs loom there, and run it or walk me through it. Then use add_server so my app offers to add it, and tell me what to do next.";

/// What "Copy a prompt" gives on the Plugins page, for an agent in loom to build one with
/// its tools (save_plugin), asking the person what it should do.
const PLUGIN_PROMPT: &str = "Make me a loom plugin. Ask me what it should do if I haven't said: a panel beside the chat, tools for my agents, or both. Build it with save_plugin, then tell me how to open it and what it asks me to allow.";

/// The thread colours a new agent can have (design/tokens.toml), in the order the brain
/// tiles take them. Grey is left for agents that are off.
const COLORS: [&str; 7] = [
    "periwinkle",
    "apricot",
    "mint",
    "pink",
    "sky",
    "gold",
    "lavender",
];

/// A small thing coming in, `k` of the way (0 to 1): a button, a tile, a line of text.
/// Whole screens never fade; they change through the threads.
fn appear<E: Styled>(d: E, k: f32) -> E {
    d.opacity(k)
}

/// How long the opening weave lasts on every launch, in seconds.
const SPLASH_SECONDS: f32 = 2.8;

/// How long picking a brain takes to move on to naming it, in seconds.
const PICK_SECONDS: f32 = 0.45;

/// How long the window takes to close on a new agent's thread, in seconds.
const CLOSE_SECONDS: f32 = 0.7;

/// How long a new page takes to unroll under its thread, in seconds.
const UNROLL_SECONDS: f32 = 0.45;

/// How high the band the weaving threads move in is.
const WEAVE_BAND: f32 = 150.0;

/// How long the threads take to weave a new agent together, in seconds.
const WEAVE_SECONDS: f32 = 2.2;

/// What the die names a new agent.
const NAMES: [&str; 32] = [
    "Pip",
    "Juniper",
    "Biscuit",
    "Moss",
    "Clementine",
    "Otto",
    "Wren",
    "Maple",
    "Pickle",
    "Nova",
    "Sprout",
    "Bramble",
    "Fig",
    "Comet",
    "Pebble",
    "Tansy",
    "Waffles",
    "Marmalade",
    "Basil",
    "Noodle",
    "Hazel",
    "Quill",
    "Bean",
    "Ziggy",
    "Clover",
    "Puddle",
    "Sorrel",
    "Button",
    "Thistle",
    "Mochi",
    "Ember",
    "Tuppence",
];

/// A name from the die, never the one it has now.
fn whimsical_name(now: &str) -> String {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as usize / 1000);
    let mut i = seed % NAMES.len();
    if NAMES[i] == now {
        i = (i + 1) % NAMES.len();
    }
    NAMES[i].to_string()
}

/// Who makes an agent program, under its name on its tile.
fn program_maker(name: &str) -> &'static str {
    match name {
        "claude" => "Anthropic",
        "codex" => "OpenAI",
        "gemini" => "Google",
        "goose" => "Block",
        _ => "",
    }
}

/// How far making an agent has got.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Step {
    #[default]
    Brain,
    Name,
    Weaving,
}

/// An agent being made: its brain, its colour, and the weaving at the end.
#[derive(Default)]
struct Making {
    step: Step,
    program: Option<String>,
    color: &'static str,
    /// The brain tile under the pointer, whose thread moves.
    hover: Option<String>,
    /// When the name or colour last changed, so its thread twangs.
    snapped: Option<Instant>,
    /// When the weaving started, and how many agents there were then.
    started: Option<Instant>,
    agents_before: usize,
    name: String,
    /// When a brain was picked: the step moves on once the others have gone.
    picked_at: Option<Instant>,
    /// Whether it's the person's first agent, made full-window before the app opens.
    first: bool,
}

impl Making {
    fn color(&self) -> &'static str {
        if self.color.is_empty() {
            "periwinkle"
        } else {
            self.color
        }
    }
}

/// What a new chat offers to ask, under the composer.
const SUGGESTIONS: [&str; 3] = [
    "What can you do?",
    "Plan my week with me",
    "Check something for me every morning",
];

/// An agent program's name as a person reads it: claude as Claude.
fn program_title(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Whether this computer runs its own instance, as the person chose on the first start.
fn local_mode() -> bool {
    std::fs::read_to_string(std::path::Path::new(&data_dir()).join("mode"))
        .is_ok_and(|m| m.trim() == "local")
}

/// The core's config (core/README.md, "Config"), for the server whose folder is `dir`
/// ("" for the first), and for a new one, its address. Only the first updates the
/// app, and only it can be the instance on this computer.
fn core_config(dir: &str, server: Option<&str>) -> String {
    let folder = server_folder(dir);
    let _ = std::fs::create_dir_all(&folder);
    let mut config = serde_json::json!({"data": folder.to_string_lossy()});
    // The system's time zone, which the core tells the instance, so routines at a time of
    // day keep this clock.
    if let Ok(tz) = iana_time_zone::get_timezone() {
        config["tz"] = tz.into();
    }
    if !dir.is_empty() {
        config["update"] = "".into();
        if let Some(server) = server {
            config["server"] = server.into();
        }
        return config.to_string();
    }
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

/// The window a panel is made in, by its handle, for the task that makes it.
struct Parent(raw_window_handle::RawWindowHandle);

impl raw_window_handle::HasWindowHandle for Parent {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        // SAFETY: the handle is the app's one window, which lives as long as the app, and
        // the panel is made on its thread.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.0) })
    }
}

/// Where a panel goes, from [x, y, width, height] in the window's logical pixels.
/// What an agent's thread does at the top of its page: what the agent is doing, or
/// while it's idle, a flourish every few seconds, a thinking loop, a knot or a twang, picked
/// at random for each agent and each few seconds. The thread eases between them.
fn fidget(agent: &str, mood: Mood) -> (Mood, Option<Instant>) {
    if mood != Mood::Idle {
        return (mood, None);
    }
    const BEAT: f32 = 3.5;
    let t = thread::now();
    let slot = (t / BEAT) as u64;
    let mut h = std::hash::DefaultHasher::new();
    std::hash::Hash::hash(&(agent, slot), &mut h);
    let roll = std::hash::Hasher::finish(&h) % 12;
    let pick = match roll {
        // Not writing or a tool: their waves are sized for a short thread.
        0..=3 => Mood::Thinking,
        4 | 5 => Mood::Waiting,
        _ => Mood::Idle,
    };
    // Some idle beats start with a twang.
    let snapped = (roll >= 7).then(|| {
        let into = Duration::from_secs_f32(t - slot as f32 * BEAT);
        Instant::now()
            .checked_sub(into)
            .unwrap_or_else(Instant::now)
    });
    (pick, snapped)
}

/// Whether a popup page is the server's, not the person's settings.
fn is_server(s: Screen) -> bool {
    matches!(
        s,
        Screen::Server
            | Screen::AddPhone
            | Screen::People
            | Screen::ThisDevice
            | Screen::Move
            | Screen::AddServer
    )
}

/// The height of the head of the panel beside the chat, and of the popup's page.
const SIDE_HEAD: f32 = 48.0;

/// How wide the panel beside the chat is, in a window `window` wide: a plugin's panel
/// as wide as the browser's, an agent's page narrower.
fn side_width(side: Screen, window: f32) -> f32 {
    if side == Screen::Plugins || side == Screen::Page {
        (window * 0.45).clamp(360.0, 820.0)
    } else {
        (window * 0.36).clamp(360.0, 560.0)
    }
}

fn panel_bounds(r: [f32; 4]) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(r[0] as f64, r[1] as f64).into(),
        size: wry::dpi::LogicalSize::new(r[2].max(0.0) as f64, r[3].max(0.0) as f64).into(),
    }
}
