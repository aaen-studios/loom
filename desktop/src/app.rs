//! The window: the sidebar of agents, and the chat with the one picked. The look
//! follows design/explorations/round-4.html, with every value from design/tokens.toml.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName as Ico;
use gpui_kit::component::input::{Input, InputEvent, InputState, Paste, Textarea, TextareaState};
use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::component::{Icon, IconName, Sizable as _, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;

use crate::core::Core;
use crate::model::{
    self, Approval, Conn, ImportSource, Message, Model, Mood, Offer, ServerUpdate, SignIn,
};
use crate::motion::{self, SoftHover};
use crate::prefs::{Prefs, Text, Theme, Thoughts, Tools, Width};
use crate::shell;
use crate::thread::{self, thread, Spool};
use crate::tokens::*;

#[path = "settings.rs"]
mod settings;
use settings::{settings_found, settings_groups, system_dark};

#[path = "rules.rs"]
mod rules;

/// What dropping a dragged agent on a row does.
type DropAgent = Box<dyn Fn(&AgentDrag, &mut Window, &mut App) + 'static>;

/// An agent being dragged in the sidebar to a new place, with what its floating copy
/// under the pointer shows: its thread in its shape, and its name.
#[derive(Clone)]
pub struct AgentDrag {
    id: String,
    name: SharedString,
    tint: Hsla,
    outline: Option<Vec<(f32, f32)>>,
    p: Palette,
}

impl AgentDrag {
    fn of(a: &model::Agent, p: Palette) -> Self {
        AgentDrag {
            id: a.id.clone(),
            name: a.name.clone().into(),
            tint: thread::color(&a.color, p.dark),
            outline: a.outline().map(<[_]>::to_vec),
            p,
        }
    }

    fn ghost(d: &AgentDrag, _: Point<Pixels>, _: &mut Window, cx: &mut App) -> Entity<AgentDrag> {
        let d = d.clone();
        cx.new(|_| d)
    }
}

impl Render for AgentDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        div()
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .shadow_lg()
            .text_size(px(size::BODY))
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.ink)
            .child(thread::strand(
                format!("drag-{}", self.id),
                Mood::Idle,
                self.tint,
                self.outline.as_deref(),
                26.0,
                10.0,
                None,
            ))
            .child(self.name.clone())
    }
}

/// The colours of one theme.
#[derive(Clone, Copy, PartialEq)]
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
    Proposals,
    Memory,
    Profile,
    Defaults,
    Rules,
    Routines,
    Tasks,
    Programs,
    ThisDevice,
    Hosts,
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
    Import,
    Appearance,
    ChatPrefs,
    AppPrefs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Menu {
    Agent,
    /// The agent's brain: its program's settings, such as its model and effort.
    Model,
    Permission,
}

/// What a right-click opened a menu on, at the pointer. An agent's opens the agent menu
/// (`menu_at`) instead.
#[derive(Clone, PartialEq, Eq)]
enum Ctx {
    /// A message in the chat, by its id.
    Message(String),
    /// A server in the sidebar's foot, by its folder.
    Server(String),
    /// The sidebar's list of agents, under its last.
    Agents,
}

/// The composer's + menu, open at its first list or at one it leads to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Plus {
    Main,
    Skills,
    Routines,
}

/// One row of the chat's list.
#[derive(Clone, PartialEq)]
enum Row {
    Message(Message),
    Approval(Approval),
}

pub struct Loom {
    /// The server on screen: what it shows, its core, and the core's events.
    model: Model,
    /// How this device shows loom, as the person set it in Settings.
    prefs: Prefs,
    /// Whether loom keeps the computer awake now, for an agent at work.
    awake: bool,
    /// Whether loom starts at login, as the system had it last time it was asked.
    starts_at_login: bool,
    /// What the last update check said.
    update_said: Option<String>,
    core: Core,
    events: std::sync::mpsc::Receiver<Value>,
    /// Its folder ("" for the first server).
    here: String,
    /// The other servers the app is signed in to, each with its own core.
    away: Vec<Away>,
    /// The server to put on screen at the first draw: the one open when the app closed.
    reopen: Option<String>,
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
    /// Where the agent menu opens when an agent was right-clicked in the sidebar, in
    /// the window; under the header's name otherwise.
    menu_at: Option<Point<Pixels>>,
    /// A menu a right-click opened, where and on what.
    ctx: Option<(Point<Pixels>, Ctx)>,
    /// When the picture viewer opened, which it fades in from.
    viewer_at: Instant,
    /// The menu item that can't be undone and waits for its second click, until the
    /// menu closes.
    armed: Option<&'static str>,
    /// Start fresh on the agent's edit panel, waiting for its second click.
    fresh_armed: bool,
    /// Whether the colour wheel for an agent's own colour is open on its edit panel,
    /// and when a click outside last closed it.
    custom_color: bool,
    color_closed_at: Option<Instant>,
    /// Whether a press on the colour wheel is being dragged.
    wheel_drag: bool,
    /// Where the colour wheel is on screen, for a click on it.
    wheel_bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// An agent's own colour as #rrggbb, on its edit panel.
    color_in: Entity<InputState>,
    /// Whether the shapes popup on an agent's edit panel is open, when a click outside last
    /// closed it, whether a stroke is being drawn, the stroke so far in the pad's
    /// pixels, and where the pad is on screen.
    shape_open: bool,
    shape_closed_at: Option<Instant>,
    draw_drag: bool,
    draw_stroke: Vec<(f32, f32)>,
    draw_bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    plus: Option<Plus>,
    /// The settings or server popup over the window, open at this page.
    popup: Option<Screen>,
    /// The panel beside the chat on the right: an agent's page, or a plugin's panel.
    side: Option<Screen>,
    /// The settings page the popup opened at last, to open at again.
    settings_last: Screen,
    /// The update icon's popup: the pointer is on the icon, or on the popup, and when it
    /// last left both, so the popup stays a moment for the pointer to cross the gap.
    update_over: (bool, bool),
    update_left: Option<Instant>,
    rows: Rc<Vec<Row>>,
    /// How each row sits among those around it: its bubble joined to the next, a time
    /// line above it, its agent's name (`model::flow`).
    flow: Rc<Vec<model::Flow>>,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    /// Text to put in the composer at its cursor (true), or take out of it (false), on
    /// the next draw, which has the window: a file's `[name]` as it's attached and
    /// taken off.
    composer_edits: Vec<(bool, String)>,
    /// What's written and attached but not sent in each chat the person left, by
    /// `draft_key`, and the chat the composer holds now.
    drafts: HashMap<String, (String, Vec<String>)>,
    draft_for: String,
    server_in: Entity<InputState>,
    user_in: Entity<InputState>,
    pass_in: Entity<InputState>,
    secret_in: Entity<InputState>,
    /// A masked field for each secret an MCP server an agent proposed needs that the
    /// person doesn't have, by its card's call and the secret's name ("tc_4/ACTA_KEY").
    card_secrets: Rc<HashMap<String, Entity<InputState>>>,
    queue_in: Entity<InputState>,
    /// A side question being written.
    aside_in: Entity<InputState>,
    /// The queued message being edited, by id.
    queue_editing: Option<String>,
    code_in: Entity<InputState>,
    name_in: Entity<InputState>,
    /// A host to add to the list of the agent being edited, and whether its
    /// network is shown as a list, which starts empty, reaching nothing.
    host_in: Entity<InputState>,
    /// The name the person gave the server on screen, on its page.
    server_name_in: Entity<InputState>,
    /// The server whose name the field has in it, by `server_key`.
    server_name_for: Option<String>,
    listing_hosts: bool,
    /// One of the person's own hosts to add, by what ssh is given, on the Hosts page.
    ssh_host_in: Entity<InputState>,
    skill_name_in: Entity<InputState>,
    skill_desc_in: Entity<InputState>,
    skill_body_in: Entity<TextareaState>,
    /// On the skills page: none for the list, `Some(None)` for a new skill, `Some(Some(name))`
    /// for one being changed.
    skill_open: Option<Option<String>>,
    /// On the proposals page: the one open, as the person is changing it before keeping it.
    proposal_open: Option<model::Proposal>,
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
    /// The instructions every agent follows, on the Defaults page.
    every_in: Entity<TextareaState>,
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
    // The mouse wheel's scrolling, played out smoothly.
    smooth: crate::smooth::Smooth,
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
    /// A client of the person's own, for a server whose sign-in lets no app sign up.
    mcp_client_in: Entity<InputState>,
    mcp_client_secret_in: Entity<InputState>,
    /// The Rules page's form for a new rule, while it's open: what it does, what it's on,
    /// what it matches, what the action does, who started the turn, and whose it is.
    rule_open: bool,
    rule_then: &'static str,
    rule_on: &'static str,
    rule_effect: &'static str,
    rule_by: &'static str,
    rule_agent: String,
    rule_in: Entity<InputState>,
    rule_note_in: Entity<InputState>,
    /// The what-if: what it's on, who started the turn, and for which agent ("" for none).
    check_on: &'static str,
    check_by: &'static str,
    check_agent: String,
    check_in: Entity<InputState>,
    /// The program whose settings form is open, on the programs page.
    program_open: Option<String>,
    set_key_in: Entity<InputState>,
    set_value_in: Entity<InputState>,
    /// The open program's share: tokens each person may use, and whether each month.
    share_in: Entity<InputState>,
    share_month: bool,
    instructions_in: Entity<TextareaState>,
    search_in: Entity<InputState>,
    /// Search over the settings, at the top of the popup's pages.
    settings_in: Entity<InputState>,
    open_thoughts: HashSet<String>,
    tray: Option<shell::Tray>,
    hwnd: isize,
    /// Where the window was last kept (place.rs), so it's written only when it moves.
    place: Option<crate::place::Place>,
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
    /// Frames drawn before, let go of at the next render.
    old_pages: Vec<Arc<RenderImage>>,
    /// Each attached picture's preview for the composer, small, by the attachment's id:
    /// none while it's made. One that couldn't be made isn't here, and the picture
    /// itself is shown.
    previews: HashMap<String, Option<Arc<RenderImage>>>,
    page_decoding: bool,
    /// The newest frame a decode was started for.
    page_tried: u64,
    /// What the composer says while it's empty, as last set.
    composer_placeholder: String,
    /// Counts the search box's changes, so only the last one, once typing pauses, asks.
    search_gen: u64,
    /// A redraw is on its way for the agents' threads, which tick at 30 frames a second.
    thread_tick: bool,
    /// Counts the window's moves and resizes, so its place is kept once it stops.
    place_gen: u64,
    /// The core's frames changed what the chat shows: its rows are made again once,
    /// before the next frame, however many frames came.
    rows_dirty: bool,
    /// The wheel on the browser the person took over, summed until it goes, at most
    /// every 40 ms: where it was, how far, and whether it's waiting to go.
    wheel: ((f32, f32), f32, bool),
    /// The chat's list, as a view of its own (`ChatList`).
    chat_view: Option<Entity<ChatList>>,
    /// The sidebar, whole or as the rail, as a view of its own (`SidebarView`), and what
    /// it was last drawn from.
    side_view: Option<Entity<SidebarView>>,
    side_key: Option<SidebarKey>,
    /// Whether a Mac's traffic lights are in the window's top left, as they are unless
    /// it's full screen, where they leave with the title bar. Read at each frame.
    lights: bool,
    /// The pages whose rows are drawn as a list, only those on screen built
    /// (`list_page`), by name, and the frames drawn, for knowing a page just opened.
    lists: HashMap<&'static str, PageList>,
    frames: u64,
    /// The activity page's lines, and what they were made from: how many entries, the
    /// times of the first and the last, and the day.
    activity_lines: (ActivityKey, Rc<Vec<ActivityLine>>),
    /// Where the page is drawn, for turning a click on it into the page's own pixels.
    page_bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Whether the watched browser fills the panel beside the chat. Otherwise it's
    /// the small window at the chat's bottom right.
    browser_docked: bool,
    /// Whether the browser beside the chat is open big over the window instead.
    browser_big: bool,
    /// The agent whose small browser window the person closed, until its browser closes.
    browser_shut: Option<String>,
    /// The panel's width, as the person dragged it, kept in the data folder.
    side_w: Option<f32>,
    /// While the panel's edge is dragged: where the pointer and the width started.
    side_drag: Option<(f32, f32)>,
    /// The sidebar, whole or a rail (rail.rs).
    bar: crate::rail::Sidebar,
    /// Whether Ctrl+F showed the search box while the prefs hide it, until it's cleared.
    search_peek: bool,
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
    /// The newest thing to tell the person, shown for a while at the window's foot, over
    /// any popup: its words, since when, and whether it's good news rather than
    /// something that went wrong.
    notice: Option<(String, Instant, bool)>,
}

pub fn data_dir() -> String {
    // Worked out once: frames ask for it, and making the folder is a trip to the disk.
    static DIR: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if let Ok(dir) = std::env::var("LOOM_DATA") {
            return dir;
        }
        // %APPDATA%\Loom on Windows, ~/Library/Application Support/Loom on a Mac.
        let base = if cfg!(target_os = "macos") {
            std::env::var("HOME")
                .map(|h| format!("{h}/Library/Application Support"))
                .unwrap_or_else(|_| ".".into())
        } else {
            std::env::var("APPDATA").unwrap_or_else(|_| ".".into())
        };
        let dir = std::path::Path::new(&base).join("Loom");
        let _ = std::fs::create_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    })
    .clone()
}

/// A server the app is signed in to besides the one on screen: its own core, with a
/// data folder of its own, and what it shows, kept up to date while it's away.
struct Away {
    /// Its folder under the app's: "" for the first server, `servers/2` for another.
    dir: String,
    core: Core,
    events: std::sync::mpsc::Receiver<Value>,
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

/// The server that was on screen when the app last closed, kept in `last-server`: one
/// of the app's server folders, or None for the first.
fn last_server() -> Option<String> {
    let file = std::path::Path::new(&data_dir()).join("last-server");
    let dir = std::fs::read_to_string(file).ok()?.trim().to_string();
    is_server_dir(&dir).then_some(dir)
}

/// Keeps which server is on screen, for the next start.
fn keep_last_server(dir: &str) {
    let file = std::path::Path::new(&data_dir()).join("last-server");
    let _ = std::fs::write(file, dir);
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
/// Which server a name the person gave is for: its address, or empty for the instance
/// in the app.
fn server_key(model: &Model, dir: &str) -> String {
    if dir.is_empty() && local_mode() {
        return String::new();
    }
    model.server.clone().unwrap_or_default()
}

/// A server as it reads: its address, or what the app calls it when it has none. The
/// yarn's colour comes from this, so a name the person gives doesn't change it.
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
        let prefs = Prefs::load(&data_dir());
        let dark = prefs.dark(system_dark(window));
        let (core, events) = FIRST_CORE
            .lock()
            .ok()
            .and_then(|mut c| c.take())
            .unwrap_or_else(|| Core::start(&core_config("", None)));
        let (device_tx, device_rx) = std::sync::mpsc::channel();

        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 10)
                .submit_on_enter(!prefs.ctrl_enter)
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
        // An agent's own colour: each change that makes a whole #rrggbb sets it.
        let color_in = input(window, cx, "#5b8def", false);
        cx.subscribe_in(
            &color_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let typed = state.read(cx).value().trim().to_lowercase();
                    let typed = if typed.starts_with('#') {
                        typed
                    } else {
                        format!("#{typed}")
                    };
                    let same = this
                        .model
                        .editing
                        .as_ref()
                        .and_then(|id| this.model.agent(id))
                        .is_some_and(|a| a.color == typed);
                    if thread::hex(&typed).is_some() && !same {
                        if let Some(f) = this.model.set_color(&typed) {
                            this.send(f);
                        }
                        cx.notify();
                    }
                }
            },
        )
        .detach();
        // The server's own name on this device: each change keeps it, and an empty one
        // goes back to its address.
        let server_name_in = input(window, cx, "", false);
        cx.subscribe_in(
            &server_name_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let typed = state.read(cx).value().trim().to_string();
                    if let Some(key) = this.server_name_for.clone() {
                        this.prefs.rename_server(&key, &typed);
                        this.prefs.save(&data_dir());
                        cx.notify();
                    }
                }
            },
        )
        .detach();
        // A host for the agent's list: Enter adds it.
        let host_in = input(window, cx, "github.com", false);
        cx.subscribe_in(
            &host_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = state.read(cx).value().to_string();
                    if let Some(f) = this.model.add_host(&typed) {
                        this.send(f);
                    }
                    state.update(cx, |s, cx| s.set_value("", window, cx));
                    cx.notify();
                }
            },
        )
        .detach();
        // One of the person's hosts: Enter adds it.
        let ssh_host_in = input(window, cx, "vps, or root@vps", false);
        cx.subscribe_in(
            &ssh_host_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = state.read(cx).value().to_string();
                    if let Some(f) = this.model.add_ssh_host(&typed, "ssh") {
                        this.send(f);
                    }
                    state.update(cx, |s, cx| s.set_value("", window, cx));
                    cx.notify();
                }
            },
        )
        .detach();
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
        let every_in = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 16)
                .placeholder("What every agent should know or do, such as how to answer")
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
        let rule_in = input(window, cx, "git push", false);
        let rule_note_in = input(window, cx, "Why, for you later", false);
        let check_in = input(window, cx, "git push origin main", false);
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
        // A server over HTTP shows its client's fields as soon as its address is typed.
        cx.subscribe_in(
            &mcp_cmd_in,
            window,
            |_: &mut Loom, _, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    cx.notify();
                }
            },
        )
        .detach();
        let mcp_client_in = input(window, cx, "Only if its sign-in asks for one", false);
        let mcp_client_secret_in = input(window, cx, "secret:KEY, or the secret", true);
        let set_value_in = input(window, cx, "Its value", true);
        let share_in = input(window, cx, "Tokens", false);
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
        let settings_in = input(window, cx, "Search settings", false);
        cx.subscribe_in(
            &settings_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => cx.notify(),
                // Enter opens the first page it found.
                InputEvent::PressEnter { .. } => {
                    let query = state.read(cx).value().to_string();
                    if let Some((_, _, _, screen)) = settings_found(&query).first() {
                        this.open(*screen, cx);
                    }
                }
                _ => {}
            },
        )
        .detach();
        let search_in = input(window, cx, "Search", false);
        cx.subscribe_in(
            &search_in,
            window,
            |this: &mut Loom, state, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    // It asks once typing pauses, not at each letter: the instance ranks
                    // every match of a short prefix, which takes it tens of milliseconds.
                    let query = state.read(cx).value().to_string();
                    this.search_gen += 1;
                    let gen = this.search_gen;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(150))
                            .await;
                        let _ = this.update(cx, |loom, cx| {
                            if loom.search_gen != gen {
                                return;
                            }
                            if let Some(frame) = loom.model.search(&query) {
                                loom.send(frame);
                            }
                            cx.notify();
                        });
                    })
                    .detach();
                }
            },
        )
        .detach();

        // The core's events come on its thread; this task brings them to the window.
        // It also looks at the tray, and notices the system waking: a tick that comes
        // long after the last means the computer slept. What comes wakes it (wake.rs);
        // otherwise it looks four times a second.
        cx.spawn(async move |this, cx| {
            let mut last = Instant::now();
            loop {
                crate::wake::wait(cx.background_executor(), Duration::from_millis(250)).await;
                let woke = last.elapsed() > Duration::from_secs(10);
                last = Instant::now();
                let alive = this.update(cx, |loom, cx| {
                    let got: Vec<Value> =
                        std::iter::from_fn(|| loom.events.try_recv().ok()).collect();
                    if woke {
                        loom.send(serde_json::json!({"t": "app.wake"}));
                        for a in &loom.away {
                            a.core.send(r#"{"t":"app.wake"}"#);
                        }
                    }
                    loom.tray_actions(cx);
                    // loom was started again: the person wants this one (single.rs).
                    if crate::single::asked_to_show() {
                        shell::show_window(loom.hwnd, true);
                        crate::perf::once("shown for another start");
                    }
                    loom.tick_prefs(cx);
                    shell::post_notifications(cx);
                    loom.notification_clicks(cx);
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
                    loom.spread_release();
                    loom.route_updates(cx);
                    loom.watch_adding(cx);
                    // A server an agent offered, and the person said yes to here.
                    let mut offered = std::mem::take(&mut loom.model.offered);
                    for a in &mut loom.away {
                        offered.append(&mut a.model.offered);
                    }
                    for offer in offered {
                        if offer.code.is_empty() {
                            loom.add_server(Some(offer.address), cx);
                        } else {
                            loom.link_server(offer, cx);
                        }
                    }
                    // The newest picture of the watched browser, as the instance sent it,
                    // decoded off this thread and drawn once it is.
                    if let Some(page) = loom.core.newest_page() {
                        loom.model.take_page(&page.head, page.jpeg);
                    }
                    if got.is_empty() {
                        if std::mem::take(&mut loom.model.page_moved) {
                            loom.decode_frame(cx);
                        }
                        return;
                    }
                    let mut changed = false;
                    for v in got {
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
                            let about = shell::About {
                                server: loom.here.clone(),
                                agent: s("agent"),
                                conversation: s("conversation"),
                            };
                            loom.notify(&s("title"), &s("body"), about);
                        }
                        changed |= crate::perf::step("take", || loom.model.take(&v));
                    }
                    loom.spread_release();
                    loom.route_updates(cx);
                    loom.fetch();
                    loom.open_fetched(cx);
                    // A sign-in page, in the person's own browser.
                    let mut to_browse = std::mem::take(&mut loom.model.to_browse);
                    for a in &mut loom.away {
                        to_browse.append(&mut a.model.to_browse);
                    }
                    for url in to_browse {
                        cx.open_url(&url);
                    }
                    // A frame is decoded off this thread, and drawn once it is.
                    if std::mem::take(&mut loom.model.page_moved) {
                        loom.decode_frame(cx);
                    }
                    if changed {
                        loom.rows_dirty = true;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        })
        .detach();

        let hwnd = shell::hwnd(window);
        // The window opens where it was closed, and keeps its place as it moves and
        // resizes (place.rs). Started in the tray, it waits there hidden. A build for
        // measuring stays off screen, and takes its place only while it waits hidden.
        let background = std::env::args().any(|a| a == "--background");
        let place = crate::place::Place::load(&data_dir());
        if cfg!(feature = "perf") {
            let scale = window.scale_factor();
            let (w, h) = ((1120.0 * scale) as i32, (760.0 * scale) as i32);
            shell::out_of_the_way(hwnd, w, h, !background);
        }
        if let Some(p) = place.filter(|_| background || !cfg!(feature = "perf")) {
            shell::put_window(hwnd, p, !background);
        }
        // Kept once the window stops moving, not at each step of a drag.
        cx.observe_window_bounds(window, |this: &mut Loom, _, cx| {
            this.place_gen += 1;
            let gen = this.place_gen;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let _ = this.update(cx, |loom, _| {
                    if loom.place_gen == gen {
                        loom.keep_place();
                    }
                });
            })
            .detach();
        })
        .detach();
        // The window, taskbar and shortcuts show the icon in loom's theme.
        shell::follow_theme(hwnd, dark);
        // Closing the window keeps loom in the tray, so routines and replies go on.
        // Unless the person turned that off, in Settings: then it quits.
        shell::keep_in_tray(prefs.tray);
        window.on_window_should_close(cx, move |_, cx| {
            if !shell::keeps_in_tray() {
                cx.quit();
                return true;
            }
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
        // The tray icon takes 5–25 ms to make, so a window being opened gets it once its
        // first frame is up. Started at login, there's no window to show, only the tray.
        let tray = if background {
            shell::Tray::new(shell::starts_at_login())
        } else {
            let weak = cx.weak_entity();
            window.on_next_frame(move |_, cx| {
                weak.update(cx, |this, _| {
                    this.tray = shell::Tray::new(shell::starts_at_login());
                })
                .ok();
            });
            None
        };
        if background {
            // Started at login: straight to the tray.
            cx.defer(move |_| shell::show_window(hwnd, false));
        }

        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        // "System" follows the system as it changes.
        cx.observe_window_appearance(window, |this: &mut Loom, window, cx| {
            this.apply_theme(window, cx);
        })
        .detach();
        let mut loom = Loom {
            model: Model::new(),
            prefs,
            awake: false,
            starts_at_login: shell::starts_at_login(),
            update_said: None,
            core,
            events,
            here: String::new(),
            away: server_dirs()
                .into_iter()
                .map(|dir| Away::start(dir, None))
                .collect(),
            reopen: last_server(),
            fill_server: None,
            adding: None,
            device_tx,
            device_rx,
            p: Palette::new(dark),
            screen: Screen::Chat,
            menu: None,
            menu_at: None,
            ctx: None,
            viewer_at: Instant::now(),
            armed: None,
            fresh_armed: false,
            custom_color: false,
            color_closed_at: None,
            wheel_drag: false,
            wheel_bounds: Rc::new(std::cell::Cell::new(None)),
            color_in,
            shape_open: false,
            shape_closed_at: None,
            draw_drag: false,
            draw_stroke: Vec::new(),
            draw_bounds: Rc::new(std::cell::Cell::new(None)),
            plus: None,
            popup: None,
            side: None,
            settings_last: Screen::Profile,
            update_over: (false, false),
            update_left: None,
            rows: Rc::new(Vec::new()),
            flow: Rc::new(Vec::new()),
            scroller,
            composer,
            composer_edits: Vec::new(),
            drafts: HashMap::new(),
            draft_for: String::new(),
            server_in,
            user_in,
            pass_in,
            secret_in,
            card_secrets: Rc::default(),
            queue_in,
            aside_in,
            queue_editing: None,
            code_in,
            name_in,
            host_in,
            server_name_in,
            server_name_for: None,
            listing_hosts: false,
            ssh_host_in,
            skill_name_in,
            skill_desc_in,
            skill_body_in,
            skill_open: None,
            proposal_open: None,
            table_dropping: None,
            panel: None,
            panel_context: None,
            panel_opening: None,
            door_in: Rc::new(std::cell::RefCell::new(Vec::new())),
            note_in,
            profile_in,
            every_in,
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
            old_pages: Vec::new(),
            previews: HashMap::new(),
            page_decoding: false,
            page_tried: 0,
            composer_placeholder: String::new(),
            search_gen: 0,
            thread_tick: false,
            place_gen: 0,
            rows_dirty: false,
            wheel: ((0.0, 0.0), 0.0, false),
            chat_view: None,
            side_view: None,
            side_key: None,
            lights: false,
            lists: HashMap::new(),
            frames: 0,
            activity_lines: ((0, None, None, chrono::NaiveDate::MIN), Rc::default()),
            page_bounds: Rc::new(std::cell::Cell::new(None)),
            browser_docked: false,
            browser_big: false,
            browser_shut: None,
            side_w: std::fs::read_to_string(std::path::Path::new(&data_dir()).join("side-width"))
                .ok()
                .and_then(|w| w.trim().parse().ok()),
            side_drag: None,
            bar: crate::rail::Sidebar::load(&data_dir()),
            search_peek: false,
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
            smooth: Default::default(),
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
            mcp_client_in,
            mcp_client_secret_in,
            rule_open: false,
            rule_then: "deny",
            rule_on: "command",
            rule_effect: "",
            rule_by: "",
            rule_agent: String::new(),
            rule_in,
            rule_note_in,
            check_on: "command",
            check_by: "person",
            check_agent: String::new(),
            check_in,
            set_key_in,
            set_value_in,
            share_in,
            share_month: false,
            instructions_in,
            search_in,
            settings_in,
            open_thoughts: HashSet::new(),
            tray,
            hwnd,
            place,
            notice: None,
        };
        loom.fetch();
        window.focus(&loom.keys, cx);
        loom
    }

    /// Esc steps back: closes a menu, the server form, a page, or a step of making an
    /// agent, whichever is on top.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.viewing.is_some() {
            self.model.viewing = None;
        } else if self.browser_big {
            self.browser_big = false;
        } else if self.menu.is_some() || self.ctx.is_some() {
            self.menu = None;
            self.ctx = None;
        } else if self.popup.is_none() && (self.search_peek || !self.model.query.is_empty()) {
            self.end_search(window, cx);
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

    // --- The sidebar, whole or a rail (rail.rs) ---

    /// How wide what's open beside the chat is at the least, for whether the window has
    /// room for the whole sidebar too.
    fn beside(&self) -> f32 {
        let open = self.screen == Screen::Chat
            && (self.side.is_some() || self.browser_docked && self.model.watching.is_some());
        if open {
            SIDE_LEAST
        } else {
            0.0
        }
    }

    /// Whether the sidebar is a rail, in a window `window` wide.
    fn railed(&self, window: f32) -> bool {
        self.bar.railed(window, self.beside())
    }

    /// How wide the sidebar is, once it's done moving, in a window `window` wide.
    fn bar_width(&self, window: f32) -> f32 {
        self.bar.width(window, self.beside())
    }

    /// The person's collapse or open, from the sidebar's control or Ctrl+B.
    fn toggle_bar(&mut self, window: &Window, cx: &mut Context<Self>) {
        let w = window.viewport_size().width.as_f32();
        self.bar.toggle(w, self.beside());
        self.bar.save(&data_dir());
        cx.notify();
    }

    /// Ctrl+F: the search box, opening the sidebar if it's a rail and showing the box
    /// if the prefs hide it.
    fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let w = window.viewport_size().width.as_f32();
        if self.railed(w) {
            self.bar.open(w, self.beside());
            self.bar.save(&data_dir());
        }
        if !self.prefs.search {
            self.search_peek = true;
        }
        self.search_in.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Clears the search, and hides the box again if only Ctrl+F showed it.
    fn end_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_in
            .update(cx, |s, cx| s.set_value("", window, cx));
        self.model.search("");
        self.search_peek = false;
        window.focus(&self.keys, cx);
        cx.notify();
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
                    crate::wake::wake();
                });
            }
            _ => self.send(answer(false, "This app has no such tool.")),
        }
    }

    /// Runs the instance on this computer, or stops and goes back to signing in to one
    /// elsewhere: keeps the choice, and starts the core again with it.
    /// Opens a page, asking the instance for what it shows.
    fn open(&mut self, screen: Screen, cx: &mut Context<Self>) {
        // The Brains page's lists are slow to come the first time, so any settings
        // page asks for them behind it.
        let settings = self
            .popup_pages(false)
            .iter()
            .any(|(_, _, _, s)| *s == screen);
        if settings && screen != Screen::Programs {
            if let Some(frames) = self.model.warm_programs() {
                for f in frames {
                    self.send(f);
                }
            }
        }
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
            Screen::Rules => {
                let f = self.model.load_rules();
                self.send(f);
                self.rule_open = false;
                self.model.what_if = None;
            }
            Screen::Defaults => {
                // Shown as they were when the page opened, the instructions too.
                let f = self.model.load_defaults();
                self.send(f);
                let f = self.model.programs();
                self.send(f);
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
                _ = self.model.load_routine_runs(None);
            }
            Screen::Tasks => {
                let f = self.model.load_tasks();
                self.send(f);
            }
            Screen::Proposals => {
                let f = self.model.load_proposals();
                self.send(f);
                self.proposal_open = None;
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
            Screen::Import => {
                let f = self.model.load_imports();
                self.send(f);
            }
            Screen::Programs => {
                for f in self.model.load_programs() {
                    self.send(f);
                }
                // Who's here, for the owner to give each program to.
                if !self.local() {
                    for f in self.model.load_people() {
                        self.send(f);
                    }
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
            Screen::Hosts => {
                let f = self.model.device_get();
                self.send(f);
                let f = self.model.scan_hosts();
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
        // The browser goes back to its small window.
        self.browser_docked = false;
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
        cx.notify();
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
        LOCAL.store(
            if local { 1 } else { 2 },
            std::sync::atomic::Ordering::Relaxed,
        );
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
        let prefs = self.prefs.clone();
        for a in &mut self.away {
            let got: Vec<Value> = std::iter::from_fn(|| a.events.try_recv().ok()).collect();
            for v in got {
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
                        let about = shell::About {
                            server: a.dir.clone(),
                            agent: s("agent"),
                            conversation: s("conversation"),
                        };
                        settings::notify_as(&prefs, &s("title"), &s("body"), about);
                    }
                    _ => {}
                }
                changed |= a.model.take(&v);
            }
            if let Some(page) = a.core.newest_page() {
                a.model.take_page(&page.head, page.jpeg);
            }
            for f in a.model.fetches() {
                a.core.send(&f.to_string());
            }
        }
        changed
    }

    /// The newest release, as the core that looks for app updates found it, goes to
    /// every server's core, and each says whether its server is behind (`core.server`).
    fn spread_release(&mut self) {
        let mut found: Vec<String> = self.model.release.take().into_iter().collect();
        for a in &mut self.away {
            found.extend(a.model.release.take());
        }
        for version in found {
            let frame = serde_json::json!({"t": "release", "version": version}).to_string();
            self.core.send(&frame);
            for a in &self.away {
                a.core.send(&frame);
            }
        }
    }

    /// Hands a frame to the core of the server whose folder is `dir`, here or away.
    fn send_to(&self, dir: &str, frame: &Value) {
        if self.here == dir {
            self.core.send(&frame.to_string());
        } else if let Some(a) = self.away.iter().find(|a| a.dir == dir) {
            a.core.send(&frame.to_string());
        }
    }

    /// The first core's model, which says what's behind across the app.
    fn hub(&self) -> &Model {
        if self.here.is_empty() {
            return &self.model;
        }
        self.away
            .iter()
            .find(|a| a.dir.is_empty())
            .map(|a| &a.model)
            .unwrap_or(&self.model)
    }

    fn hub_mut(&mut self) -> &mut Model {
        if self.here.is_empty() {
            return &mut self.model;
        }
        match self.away.iter_mut().find(|a| a.dir.is_empty()) {
            Some(a) => &mut a.model,
            None => &mut self.model,
        }
    }

    /// Carries what the update icon needs between the cores, as the first core asks:
    /// each other server's `core.server` to it, what it relays to another server's
    /// core, and the app's own update once it says the app goes (core/updates.calx).
    fn route_updates(&mut self, cx: &mut Context<Self>) {
        let mut said: Vec<(String, Value)> = Vec::new();
        if let Some(f) = self.model.server_said.take() {
            said.push((self.here.clone(), f));
        }
        for a in &mut self.away {
            if let Some(f) = a.model.server_said.take() {
                said.push((a.dir.clone(), f));
            }
        }
        for (dir, mut f) in said {
            // The first core hears its own server itself.
            if dir.is_empty() {
                continue;
            }
            f["t"] = "server.state".into();
            f["dir"] = dir.into();
            self.send_to("", &f);
        }
        let relays = std::mem::take(&mut self.hub_mut().relays);
        for (dir, frame) in relays {
            self.send_to(&dir, &frame);
        }
        if let Some(path) = self.hub_mut().restart.take() {
            match shell::apply_update(std::path::Path::new(&path), false) {
                Ok(()) => cx.quit(),
                Err(e) => {
                    self.model
                        .notices
                        .push(format!("loom couldn't update itself: {e}"));
                    cx.notify();
                }
            }
        }
    }

    /// Updates `device` ("app", or a server's folder), or with none, everything behind
    /// that this person may update: the first core decides what that means.
    fn run_updates(&mut self, device: Option<&str>) {
        let mut f = serde_json::json!({"t": "updates.run"});
        if let Some(d) = device {
            f["device"] = d.into();
        }
        self.send_to("", &f);
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
        keep_last_server(&self.here);
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
            if self.show_known(a, cx) {
                return;
            }
        }
        self.fill_server = Some(address.unwrap_or_default());
        self.open(Screen::AddServer, cx);
    }

    /// Puts the server at `address` on screen if the app has it already, and says
    /// whether it did.
    fn show_known(&mut self, address: &str, cx: &mut Context<Self>) -> bool {
        let same = |m: &Model| {
            m.server.as_deref().map(|s| s.trim_end_matches('/'))
                == Some(address.trim_end_matches('/'))
        };
        if same(&self.model) {
            return true;
        }
        if let Some(dir) = self
            .away
            .iter()
            .find(|x| same(&x.model))
            .map(|x| x.dir.clone())
        {
            self.show_server(&dir, cx);
            return true;
        }
        false
    }

    /// A server an agent offered with a link code, which the person said yes to here:
    /// the popup to add it opens signing in with the code, and the server comes on
    /// screen once it's in. A code that didn't work leaves the form, with why.
    fn link_server(&mut self, offer: Offer, cx: &mut Context<Self>) {
        if self.show_known(&offer.address, cx) {
            return;
        }
        self.fill_server = Some(offer.address.clone());
        self.open(Screen::AddServer, cx);
        let dir = self.start_adding(&offer.address);
        if let Some(a) = self.away.iter_mut().find(|a| a.dir == dir) {
            if !a.model.logging_in {
                let frame = a.model.link_offered(&offer);
                a.core.send(&frame.to_string());
            }
        }
        cx.notify();
    }

    /// The folder of the server being added, with a core of its own beside the one on
    /// screen, started the first time.
    fn start_adding(&mut self, server: &str) -> String {
        if let Some(dir) = self.adding.clone() {
            return dir;
        }
        // The app keeps it once it's signed in. A folder left from one that never
        // was is started afresh.
        let dirs = server_dirs();
        let n = (2..)
            .find(|n| !dirs.contains(&format!("servers/{n}")))
            .unwrap_or(2);
        let dir = format!("servers/{n}");
        let _ = std::fs::remove_dir_all(server_folder(&dir));
        self.away.push(Away::start(dir.clone(), Some(server)));
        self.adding = Some(dir.clone());
        dir
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
        let dir = self.start_adding(&server);
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
            // Its core stops as it's dropped, and lets go of its folder; the first core
            // forgets whether it was behind.
            self.send_to(
                "",
                &serde_json::json!({"t": "server.state", "dir": dir, "gone": true}),
            );
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

    /// Whether the first server is out of the app: not the instance on this computer and
    /// signed in nowhere, while the app has others. It stays running, since it's the one
    /// that updates the app, but the app shows the others.
    fn first_out(&self) -> bool {
        let first = if self.here.is_empty() {
            Some(&self.model)
        } else {
            self.away
                .iter()
                .find(|a| a.dir.is_empty())
                .map(|a| &a.model)
        };
        !self.away.is_empty()
            && !local_mode()
            && first.is_some_and(|m| m.conn == Some(Conn::SignedOut))
    }

    /// At each draw: the server open when the app closed comes on screen once, and a
    /// first server that's out of the app gives the screen to another.
    fn settle_server(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = self.reopen.take() {
            if self.away.iter().any(|a| a.dir == dir) {
                self.show_server(&dir, cx);
            }
        }
        if self.here.is_empty() && self.first_out() {
            if let Some(dir) = self.away.first().map(|a| a.dir.clone()) {
                self.show_server(&dir, cx);
            }
        }
    }

    /// Every server, in the order the sidebar shows them: the first, then the rest as
    /// they were added. Each is its folder and what it shows. A first server out of the
    /// app isn't among them.
    fn servers(&self) -> Vec<(String, &Model)> {
        let out = self.first_out();
        let mut all: Vec<(String, &Model)> = vec![(self.here.clone(), &self.model)];
        all.extend(self.away.iter().map(|a| (a.dir.clone(), &a.model)));
        all.retain(|(d, _)| !(out && d.is_empty()));
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

    /// Shows the newest notice from the model, and hides one that's been up its time.
    fn take_notice(&mut self, cx: &mut Context<Self>) {
        if let Some(words) = self.hub_mut().checked.take() {
            // Settings shows it; away from the window, a notification does.
            if !shell::window_shown(self.hwnd) {
                shell::notify("loom", &words, None, self.prefs.sound);
            }
            self.update_said = Some(words);
            cx.notify();
        }
        // Something that went wrong wins over good news that came with it.
        let bad = self.model.notices.pop().map(|w| (w, false));
        let came = bad.or_else(|| self.model.said.pop().map(|w| (w, true)));
        if let Some((words, good)) = came {
            self.model.notices.clear();
            self.model.said.clear();
            self.notice = Some((words, Instant::now(), good));
            cx.notify();
        } else if self
            .notice
            .as_ref()
            .is_some_and(|(_, at, _)| at.elapsed().as_secs_f32() > NOTICE_SECONDS)
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

    /// Shows a picture from the chat big over the window.
    pub fn view(&mut self, f: &model::FileRef, cx: &mut Context<Self>) {
        if self.model.viewing.is_none() {
            self.viewer_at = Instant::now();
        }
        if let Some(frame) = self.model.view(f) {
            self.send(frame);
        }
        cx.notify();
    }

    /// Opens a stack of pictures from the chat big over the window, at its first.
    pub fn view_stack(&mut self, stack: &[model::FileRef], cx: &mut Context<Self>) {
        if self.model.viewing.is_none() {
            self.viewer_at = Instant::now();
        }
        if let Some(frame) = self.model.view_stack(stack) {
            self.send(frame);
        }
        cx.notify();
    }

    /// The viewer's next picture (1) or the one before (-1).
    fn view_step(&mut self, by: i32, cx: &mut Context<Self>) {
        if let Some(frame) = self.model.view_step(by) {
            self.send(frame);
        }
        cx.notify();
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
                if let Some(a) = self.model.attachments.last().filter(|a| a.image()) {
                    let (id, path) = (a.id.clone(), a.path.clone());
                    self.make_preview(id, path, cx);
                }
                // Its name goes in the words where the cursor is, as a link to it once
                // it's sent (`model::link_files`).
                if let Some(a) = self.model.attachments.last() {
                    self.composer_edits
                        .push((true, format!("{} ", model::file_mark(&a.name))));
                }
            }
        }
        cx.notify();
    }

    /// A small copy of an attached picture, for its chip in the composer: a photo drawn
    /// as itself there took about 48 MB of memory, and as much on the GPU, at 64 pixels.
    fn make_preview(&mut self, id: String, path: String, cx: &mut Context<Self>) {
        self.previews.insert(id.clone(), None);
        let task = cx.background_executor().spawn(async move {
            let full = picture_pixels(&std::fs::read(&path).ok()?)?;
            let small = image::DynamicImage::ImageRgba8(full)
                .thumbnail(128, 128)
                .into_rgba8();
            Some(Arc::new(RenderImage::new(vec![image::Frame::new(small)])))
        });
        cx.spawn(async move |this, cx| {
            let made = task.await;
            let _ = this.update(cx, |loom, cx| {
                match made {
                    Some(r) if loom.previews.contains_key(&id) => {
                        loom.previews.insert(id, Some(r));
                    }
                    _ => {
                        loom.previews.remove(&id);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn unattach(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(a) = self.model.attachments.iter().find(|a| a.id == id) {
            self.composer_edits.push((false, model::file_mark(&a.name)));
        }
        self.model.unattach(id);
        cx.notify();
    }

    /// Which chat the composer writes in: the agent's, or a task or branch of it, on
    /// this server.
    fn draft_key(&self) -> String {
        let agent = self.model.selected.clone().unwrap_or_default();
        let task = self.model.task_open().map(|t| t.id.clone());
        let side = task
            .or_else(|| self.model.open_branch.clone())
            .unwrap_or_default();
        format!("{}/{agent}/{side}", self.here)
    }

    /// Once the person opens another chat, keeps what they'd written and attached in
    /// the one they left, and puts back what they'd left in this one.
    fn swap_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.draft_key();
        if key == self.draft_for {
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let files = self.model.park_attachments();
        let left = std::mem::replace(&mut self.draft_for, key.clone());
        if !text.trim().is_empty() || !files.is_empty() {
            self.drafts.insert(left, (text, files));
        }
        let (text, files) = self.drafts.remove(&key).unwrap_or_default();
        self.model.unpark_attachments(&files);
        self.composer
            .update(cx, |c, cx| c.set_value(text, window, cx));
        cx.notify();
    }

    /// Puts in and takes out the composer's file names, now that there's a window.
    fn edit_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (put, words) in std::mem::take(&mut self.composer_edits) {
            self.composer.update(cx, |c, cx| {
                if put {
                    c.insert(words, window, cx);
                } else {
                    let text = c.value().to_string();
                    let next = text.replacen(&format!("{words} "), "", 1);
                    let next = if next == text {
                        text.replacen(&words, "", 1)
                    } else {
                        next
                    };
                    if next != text {
                        c.set_value(next, window, cx);
                    }
                }
            });
        }
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
                    let format = image.format;
                    let ext = match format {
                        ImageFormat::Jpeg => "jpg",
                        ImageFormat::Webp => "webp",
                        ImageFormat::Gif => "gif",
                        _ => "png",
                    };
                    // A short name, as it reads in the message, in a folder of its own so
                    // another paste can have it too.
                    let at = chrono::Local::now().format("%Y-%m-%d %H.%M.%S%.3f");
                    let dir = std::path::Path::new(&data_dir())
                        .join("pasted")
                        .join(at.to_string());
                    let name = model::free_name("image", ext, &self.model.attachments);
                    let path = dir.join(name);
                    // Making a big screenshot a PNG, and writing it, take a while: off this
                    // thread, and attached once it's done.
                    let raw = image.bytes.clone();
                    let task = cx.background_executor().spawn(async move {
                        let bytes = match format {
                            ImageFormat::Jpeg
                            | ImageFormat::Webp
                            | ImageFormat::Gif
                            | ImageFormat::Png => raw,
                            _ => shell::to_png(&raw)
                                .ok_or_else(|| "That image's kind can't be sent.".to_string())?,
                        };
                        std::fs::create_dir_all(&dir)
                            .and_then(|_| std::fs::write(&path, &bytes))
                            .map_err(|e| format!("The image couldn't be kept: {e}"))?;
                        Ok::<_, String>(path)
                    });
                    cx.spawn(async move |this, cx| {
                        let done = task.await;
                        let _ = this.update(cx, |loom, cx| match done {
                            Ok(path) => loom.attach(&[path], cx),
                            Err(e) => {
                                loom.model.notices.push(e);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Rebuilds the chat's rows, and tells the list what changed.
    fn refresh_rows(&mut self, cx: &mut Context<Self>) {
        crate::perf::step("rows", || self.rebuild_rows(cx));
        if self.rows.len() > 1 {
            crate::perf::once("chat shown");
        }
    }

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let mut rows: Vec<Row> = self.model.shown().into_iter().map(Row::Message).collect();
        rows.extend(self.model.shown_approvals().into_iter().map(Row::Approval));
        // What another chat's frames change leaves this one's rows as they were: the
        // list keeps them, so its cached drawing stays good. Compared from the end,
        // where a reply that's streaming differs at once.
        if rows.len() == self.rows.len() && rows.iter().rev().eq(self.rows.iter().rev()) {
            return;
        }
        let old = self.rows.clone();
        let same_start = rows.len() >= old.len()
            && old.iter().zip(rows.iter()).all(|(a, b)| match (a, b) {
                (Row::Message(a), Row::Message(b)) => a.id == b.id,
                (Row::Approval(a), Row::Approval(b)) => a.call == b.call,
                _ => false,
            });
        let (old_len, new_len) = (old.len(), rows.len());
        let messages: Vec<Option<&Message>> = rows
            .iter()
            .map(|r| match r {
                Row::Message(m) => Some(m),
                Row::Approval(_) => None,
            })
            .collect();
        // A zone's offset changes only on a quarter hour, so each quarter hour's day is
        // looked up once: Windows asks the system for the zone at every lookup.
        let days = std::cell::RefCell::new(HashMap::<i64, i64>::new());
        let day = |ms: i64| {
            *days
                .borrow_mut()
                .entry(ms.div_euclid(900_000))
                .or_insert_with(|| day_number(ms))
        };
        self.flow = Rc::new(model::flow(&messages, day));
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

    /// What dropping a dragged agent on the agent `onto` does: puts it in that one's
    /// place, and tells the instance the new order.
    fn drop_agent(&self, onto: &str, cx: &mut Context<Self>) -> DropAgent {
        let onto = onto.to_string();
        Box::new(cx.listener(move |this, d: &AgentDrag, _, cx| {
            if let Some(f) = this.model.move_agent(&d.id, &onto) {
                this.send(f);
            }
            cx.notify();
        }))
    }

    /// A right-click on an agent in the sidebar: its chat, with its menu open at the
    /// pointer, the same menu as the header's.
    fn agent_menu_at(&mut self, agent: String, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.select(agent, cx);
        self.ctx = None;
        self.menu = Some(Menu::Agent);
        self.menu_at = Some(at);
        cx.notify();
    }

    /// A server as the person sees it: the name they gave it on this device, or its
    /// address.
    fn label(&self, model: &Model, dir: &str) -> String {
        self.prefs
            .server_name(&server_key(model, dir))
            .unwrap_or_else(|| server_name(model, dir))
    }

    /// Puts the server on screen's name in its page's field, once for each server. A
    /// change made while it's set isn't kept, since the key is cleared until it's done.
    fn sync_server_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = server_key(&self.model, &self.here);
        if self.server_name_for.as_ref() == Some(&key) {
            return;
        }
        self.server_name_for = None;
        let given = self.prefs.server_name(&key).unwrap_or_default();
        let address = server_name(&self.model, &self.here);
        self.server_name_in.update(cx, |s, cx| {
            s.set_value(given, window, cx);
            s.set_placeholder(address, window, cx);
        });
        self.server_name_for = Some(key);
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
        let width = side_width(
            side == Screen::Plugins || side == Screen::Page,
            window.viewport_size().width.as_f32(),
            self.bar_width(window.viewport_size().width.as_f32()),
            self.side_w,
        );
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
            })
            .map(|hero| {
                hero.child(
                    div()
                        .pt(px(space::S))
                        .pb(px(space::M))
                        .border_b_1()
                        .border_color(p.line)
                        .child(self.look_picker(width - 2.0 * space::L, cx)),
                )
            });
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .relative()
            .border_l_1()
            .border_color(p.line)
            .child(self.side_edge(width, cx))
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
            .size(px(control::ICON))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .text_color(p.ink_3)
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
            .tooltip(tip("Close"))
            .child(Icon::new(Ico::X).small())
    }

    /// How the server's spool is now.
    fn spool_state(&self) -> Spool {
        spool_of(&self.model)
    }

    /// A server as its ball of yarn, `size` square: coloured and wound its own way from
    /// its name, so each server looks like itself, and showing how it is (`thread::yarn`).
    fn server_yarn(&self, state: Spool, name: &str, size: f32) -> AnyElement {
        let seed = thread::seed_of(name);
        let tint = thread::color(COLORS[(seed % COLORS.len() as u64) as usize], self.p.dark);
        thread::yarn(state, tint, self.p.ink_3, self.p.side, seed, size).into_any_element()
    }

    /// A server's yarn, with a small open ring at its foot while a newer loom is out
    /// than the server runs: quiet, in the yarn's own grey, for the server popup to say more.
    fn yarn_marked(&self, model: &Model, state: Spool, name: &str, size: f32) -> AnyElement {
        let p = self.p;
        let behind = model.version.as_ref().is_some_and(|v| v.behind);
        div()
            .relative()
            .flex_none()
            .size(px(size))
            .child(self.server_yarn(state, name, size))
            .when(behind, |d| {
                d.child(
                    div()
                        .absolute()
                        .right(px(-1.0))
                        .bottom(px(-1.0))
                        .size(px(8.0))
                        .rounded_full()
                        .border_2()
                        .border_color(p.side)
                        .bg(p.ink_3),
                )
            })
            .into_any_element()
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
            let name = server_name(model, &dir);
            let id = SharedString::from(format!("server-{dir}"));
            let mut button = div()
                .id(id)
                .role(Role::Button)
                .aria_label(SharedString::from(name.to_string()))
                .relative()
                .flex()
                .items_center()
                .gap(px(space::M))
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .on_mouse_down(MouseButton::Right, {
                    let dir = dir.clone();
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        this.open_ctx(e.position, Ctx::Server(dir.clone()), cx)
                    })
                });
            if here {
                button = button
                    .flex_1()
                    .min_w_0()
                    .h(px(control::SERVER))
                    .px(px(space::S))
                    .when(server_open, |d| d.bg(p.active))
                    .child(self.yarn_marked(model, state, &name, 32.0))
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
                                    .child(self.label(model, &dir)),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(if state == Spool::Loose {
                                        p.bad
                                    } else {
                                        p.ink_3
                                    })
                                    .truncate()
                                    .child(self.conn_words()),
                            ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.open(Screen::Server, cx)));
            } else {
                button = button.flex_none().p(px(space::S));
                let heard = !model.unread.is_empty();
                button = button
                    .child(self.yarn_marked(model, state, &name, 32.0))
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
                pages.push(("pop-hosts", Ico::Server, "Hosts", Screen::Hosts));
            }
            return pages;
        }
        settings_groups()
            .into_iter()
            .flat_map(|(_, pages)| pages)
            .collect()
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
            .w(px(layout::NAV))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(px(space::S))
            .bg(p.side)
            .border_r_1()
            .border_color(p.line);
        // Its head: the gear, or the server's yarn, as the sidebar shows them.
        let yarn = self.server_yarn(
            self.spool_state(),
            &server_name(&self.model, &self.here),
            20.0,
        );
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
                .when(server, |d| d.child(yarn).child("Server"))
                .when(!server, |d| {
                    d.child(Icon::new(Ico::Settings).small().text_color(p.ink_2))
                        .child("Settings")
                }),
        );
        // The person's pages under their headings, or what search found; the server's
        // in one list.
        let query = self.settings_in.read(cx).value().trim().to_string();
        let groups = if server {
            vec![(None, pages)]
        } else if query.is_empty() {
            settings_groups()
        } else {
            vec![(None, settings_found(&query))]
        };
        if !server {
            nav = nav.child(
                div()
                    .px(px(space::XS))
                    .pb(px(space::S))
                    .text_size(px(size::SMALL))
                    .child(
                        Input::new(&self.settings_in)
                            .prefix(Icon::new(IconName::Search).small().text_color(p.ink_3))
                            .cleanable(true),
                    ),
            );
        }
        let nothing = !server && !query.is_empty() && groups.iter().all(|(_, g)| g.is_empty());
        let mut list = div()
            .id("popup-nav")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(2.0));
        for (i, (heading, pages)) in groups.into_iter().enumerate() {
            if let Some(h) = heading {
                list = list.child(
                    div()
                        .px(px(space::M))
                        .pt(px(if i == 0 { 0.0 } else { space::M }))
                        .pb(px(space::XS))
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_3)
                        .child(h),
                );
            } else if i > 0 {
                list = list.child(div().h(px(space::S)));
            }
            for (id, icon, label, screen) in pages {
                list = list.child(
                    self.menu_item(id, icon, label, false)
                        .when(screen == open, |d| d.bg(p.active))
                        .on_click(cx.listener(move |this, _, _, cx| this.open(screen, cx))),
                );
            }
        }
        if nothing {
            list = list.child(
                div()
                    .px(px(space::M))
                    .py(px(space::S))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .child("Nothing found"),
            );
        }
        nav = nav.child(list);
        // What isn't a setting of this server, but something to do: at the bottom.
        if server {
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
            // With other servers in the app, it leaves the sidebar too (`first_out`).
            let others = !self.away.is_empty();
            nav = nav.child(
                self.menu_item(
                    "pop-sign-out",
                    if local { Ico::Power } else { Ico::LogOut },
                    match (local, others) {
                        (true, true) => "Stop and remove",
                        (true, false) => "Close this instance",
                        (false, true) => "Sign out and remove",
                        (false, false) => "Sign out",
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
            Screen::Defaults => self.defaults_page(cx),
            Screen::Rules => self.rules_page(cx),
            Screen::Skills => self.skills_page(cx),
            Screen::Proposals => self.proposals_page(cx),
            Screen::Routines => self.routines_page(cx),
            Screen::Tasks => self.tasks_page(cx),
            Screen::Programs => self.programs_page(cx),
            Screen::ThisDevice => self.device_page(cx),
            Screen::Hosts => self.hosts_page(cx),
            Screen::Move => self.move_page(cx),
            Screen::People => self.people_page(cx),
            Screen::AddPhone => self.add_phone_page(cx),
            Screen::Mcp => self.mcp_page(cx),
            Screen::Plugins => self.plugins_page(cx),
            Screen::Import => self.import_page(cx),
            Screen::Appearance => self.appearance_page(cx),
            Screen::ChatPrefs => self.chat_prefs_page(cx),
            Screen::AppPrefs => self.app_prefs_page(cx),
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

    /// The notice, floating at the window's foot over everything, a popup too, so what a
    /// click in Settings did shows where it was clicked: good news with a tick, and what
    /// went wrong in red. It fades in and out, and a click puts it away.
    fn toast(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let (words, at, good) = self.notice.clone()?;
        let p = self.p;
        let key = SharedString::from(format!("toast-{at:?}"));
        let shown = at.elapsed().as_secs_f32();
        let leaving = ((NOTICE_SECONDS - shown) / 0.3).clamp(0.0, 1.0);
        if leaving < 1.0 || shown > NOTICE_SECONDS - 0.4 {
            window.request_animation_frame();
        }
        let k = motion::arrive(&key, 0.2).min(leaving);
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(88.0 - space::S * (1.0 - k)))
                .flex()
                .justify_center()
                .px(px(space::XL))
                .child(
                    div()
                        .id("notice")
                        .occlude()
                        .max_w(px(space::COLUMN))
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .px(px(space::L))
                        .py(px(space::S + 2.0))
                        .rounded(px(radius::CARD))
                        .border_1()
                        .border_color(p.line_strong)
                        .bg(p.raised)
                        .shadow_lg()
                        .opacity(k)
                        .text_size(px(size::SMALL))
                        .text_color(if good { p.ink } else { p.bad })
                        .cursor_pointer()
                        .child(
                            Icon::new(if good {
                                Ico::CircleCheck
                            } else {
                                Ico::CircleAlert
                            })
                            .small()
                            .text_color(if good {
                                p.ok
                            } else {
                                p.bad
                            }),
                        )
                        .child(words)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                ),
        )
    }

    /// While a menu is open, a clear sheet over the window that takes the pointer: nothing
    /// behind it hovers, and a click anywhere, the button that opened it too, closes it.
    /// The menus themselves are drawn over it.
    fn menu_scrim(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if self.menu.is_none() && self.plus.is_none() && self.ctx.is_none() {
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
                    this.ctx = None;
                    cx.notify();
                })),
        )
    }

    /// The popup's first server page: where the instance is and how it is, and scripts.
    fn server_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let given = self.prefs.server_name(&server_key(&self.model, &self.here));
        let where_ = if let Some(given) = given {
            given
        } else if self.local() {
            "On this computer".to_string()
        } else {
            match &self.model.server {
                Some(_) => self.label(&self.model, &self.here),
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
            .child(words);
        // Its version, and quietly, a newer one that's out or an update under way, with
        // Update for an owner or admin.
        let (line, action) = self.version_line();
        let mut control = div().flex().items_center().gap(px(space::M)).child(status);
        if let Some((label, now)) = action {
            control = control.child(self.small_button("server-update", label, false).on_click(
                cx.listener(move |this, _, _, cx| {
                    let f = this.model.update_server(now);
                    this.send(f);
                    cx.notify();
                }),
            ));
        }
        let here = self
            .row(
                Some(self.yarn_marked(
                    &self.model,
                    state,
                    &server_name(&self.model, &self.here),
                    40.0,
                )),
                where_,
                Some(line),
                Some(control.into_any_element()),
            )
            .min_h(px(control::ROW_TALL));
        // A name of the person's own for it, on this device; empty is its address.
        let name = self.row(
            None,
            "Name",
            None,
            Some(
                div()
                    .w(px(240.0))
                    .child(Input::new(&self.server_name_in).small())
                    .into_any_element(),
            ),
        );
        let mut groups =
            vec![self.group(None, vec![here.into_any_element(), name.into_any_element()])];
        if let Some(g) = self.self_update_group(cx) {
            groups.push(g);
        }
        groups.push(self.scripts_group(cx));
        self.settings_page("server-page", "This server", None, groups)
    }

    /// The server's version in a line, as quiet as the rest of the row, and what the
    /// person may do about it: a label, and whether it stops what runs.
    fn version_line(&self) -> (String, Option<(&'static str, bool)>) {
        if self.local() {
            return (format!("loom {}", crate::VERSION), None);
        }
        let Some(v) = self.model.version.clone() else {
            return ("loom".into(), None);
        };
        let ours = format!("loom {}", v.loom);
        let may = self.model.updates.as_ref().is_some_and(|u| u.may && u.can);
        let next = |u: &ServerUpdate| {
            if u.version.is_empty() {
                v.latest.clone()
            } else {
                u.version.clone()
            }
        };
        match self.model.server_update.as_ref() {
            Some(u) if u.state == "downloading" => (format!("{ours} · getting the newest…"), None),
            Some(u) if u.state == "waiting" => (
                format!(
                    "{ours} · {} is ready, and goes in once nothing runs",
                    next(u)
                ),
                may.then_some(("Now", true)),
            ),
            Some(u) if u.state == "restarting" => {
                (format!("{ours} · restarting into {}…", next(u)), None)
            }
            Some(u) if u.state == "failed" => (
                format!("{ours} · the update didn't work: {}", u.why),
                may.then_some(("Try again", false)),
            ),
            Some(u) if u.state == "latest" => (format!("{ours} · the newest"), None),
            _ if v.behind => (
                format!("{ours} · {} is out", v.latest),
                may.then_some(("Update", false)),
            ),
            _ => (ours, None),
        }
    }

    /// For the owner of a server elsewhere: whether it updates itself, once nothing runs.
    fn self_update_group(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.local() {
            return None;
        }
        let u = self.model.updates.clone().filter(|u| u.owner)?;
        let on = u.auto;
        let toggle =
            self.switch("self-update-switch", on)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let f = this.model.set_auto_update(!on);
                    this.send(f);
                    cx.notify();
                }));
        let sub = if u.can {
            "Takes each new release once nothing runs, and tells you after".to_string()
        } else {
            u.why.clone()
        };
        let row = self.row(
            Some(self.tile(Ico::RotateCcw)),
            "Updates itself",
            Some(sub),
            u.can.then(|| toggle.into_any_element()),
        );
        Some(self.group(None, vec![row.into_any_element()]))
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
        // A status token, apart from scripts: it only reads the status, for a dashboard.
        if let Some(made) = self.model.status_token {
            let mut control = div().flex().items_center().gap(px(space::S));
            if made {
                control = control.child(self.small_button("status-off", "Revoke", false).on_click(
                    cx.listener(|this, _, _, cx| {
                        let f = this.model.set_status_token(false);
                        this.send(f);
                        cx.notify();
                    }),
                ));
            }
            control = control.child(
                self.small_button("status-new", if made { "New token" } else { "Make" }, false)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let f = this.model.set_status_token(true);
                        this.send(f);
                        cx.notify();
                    })),
            );
            // One an agent had saved to a file says where, and which agent asked.
            let about = match &self.model.status_saved {
                Some((path, by)) if made && by.is_empty() => format!("Saved to {path}"),
                Some((path, by)) if made => format!("Saved to {path} by {by}"),
                _ => "Only reads your agents and routines".to_string(),
            };
            rows.push(
                self.row(
                    None,
                    "Status token",
                    Some(about),
                    Some(control.into_any_element()),
                )
                .into_any_element(),
            );
        }
        if let Some(token) = self.model.new_status_token.clone() {
            let copied = token.clone();
            let copy =
                self.small_button("status-token", "Copy", false)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                    });
            rows.push(
                self.row(
                    None,
                    token,
                    Some("Your status token, shown this once".into()),
                    Some(copy.into_any_element()),
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
                    .max_w(px(self.prefs.column()))
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
                            .size(px(control::ICON))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
                    .max_w(px(self.prefs.column()))
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
                            .size(px(control::ICON))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
        let agents = self.model.agents.clone();
        for a in agents.iter() {
            let id = a.id.clone();
            let on = picked == Some(a.id.as_str());
            let tint = thread::color(&a.color, p.dark);
            chips = chips.child(
                div()
                    .id(SharedString::from(format!("{prefix}-{}", a.id)))
                    .h(px(control::CHIP))
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
                            .soft_ink(p.ink_3, p.ink)
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

    /// Everyone, then each person but the owner, as chips to pick who a program or its
    /// settings go to. `on` gets who it goes to once one is clicked.
    fn member_chips(
        &self,
        prefix: &str,
        now: Option<Vec<String>>,
        people: &[model::Person],
        cx: &mut Context<Self>,
        on: impl Fn(&mut Loom, Option<Vec<String>>) + Clone + 'static,
    ) -> Div {
        let p = self.p;
        let chip = |id: String, label: String, picked: bool| {
            div()
                .id(SharedString::from(id))
                .h(px(control::CHIP))
                .px(px(space::M))
                .flex()
                .items_center()
                .rounded(px(radius::CHIP))
                .border_1()
                .text_size(px(size::SMALL))
                .cursor_pointer()
                .when(picked, |d| {
                    d.bg(p.bg)
                        .border_color(p.line_strong)
                        .text_color(p.ink)
                        .font_weight(FontWeight::MEDIUM)
                })
                .when(!picked, |d| {
                    d.border_color(p.line)
                        .text_color(p.ink_3)
                        .soft_ink(p.ink_3, p.ink)
                })
                .child(label)
        };
        let everyone = on.clone();
        let mut chips = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(px(space::XS))
            .child(
                chip(
                    format!("{prefix}-everyone"),
                    "Everyone".into(),
                    now.is_none(),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    everyone(this, None);
                    cx.notify();
                })),
            );
        for person in people {
            let picked = now.as_ref().is_some_and(|l| l.contains(&person.id));
            let (on, now, id) = (on.clone(), now.clone(), person.id.clone());
            chips = chips.child(
                chip(
                    format!("{prefix}-{}", person.id),
                    person.username.clone(),
                    picked,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    on(this, model::toggle_member(now.as_deref(), Some(&id)));
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
                // One piece: who it's for, then what it is, with no line between.
                vec![div()
                        .p(px(space::L))
                        .flex()
                        .flex_col()
                        .gap(px(space::M))
                        .child(chips)
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
                .size(px(control::LEAD))
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
                .soft_hover(transparent_black(), p.hover, None)
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
        if self.model.tasks_more && !rows.is_empty() {
            rows.push(
                div()
                    .p(px(space::M))
                    .flex()
                    .justify_center()
                    .child(
                        self.small_button("older-tasks", "Older tasks", false)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(f) = this.model.load_older_tasks() {
                                    this.send(f);
                                }
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
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
        self.browser_docked = agent.is_some();
        if agent.is_some() {
            self.browser_shut = None;
        } else {
            self.browser_big = false;
        }
        // Opened beside the chat, the person watches it, which keeps it open.
        if let Some(f) = self.model.watch(agent, false) {
            self.send(f);
        }
        cx.notify();
    }

    /// Keeps the small browser window right for the agent on screen: following its
    /// browser while it's open and the person hasn't closed the window, and not
    /// once it's closed. The small window only follows, so the browser still
    /// closes when the agent's turn ends.
    fn follow_browser(&mut self, agent: Option<&model::Agent>) {
        let Some(a) = agent else { return };
        let open = self.model.browser(&a.id).open;
        let mine = self.model.watching.as_deref() == Some(a.id.as_str());
        if !open && self.browser_shut.as_deref() == Some(a.id.as_str()) {
            self.browser_shut = None;
        }
        let want = open && self.browser_shut.as_deref() != Some(a.id.as_str());
        if want && (!mine || (!self.browser_docked && !self.model.watch_follows)) {
            self.browser_docked = false;
            if let Some(f) = self.model.watch(Some(a.id.clone()), true) {
                self.send(f);
            }
        } else if !open && mine && !self.browser_docked {
            if let Some(f) = self.model.watch(None, true) {
                self.send(f);
            }
        }
    }

    /// Decodes the watched browser's newest frame off the UI thread, one at a time:
    /// frames that come while one decodes are skipped for the newest.
    fn decode_frame(&mut self, cx: &mut Context<Self>) {
        let Some(f) = &self.model.frame else {
            return;
        };
        if self.page_decoding || self.page_tried == f.seq {
            return;
        }
        let (seq, jpeg) = (f.seq, f.jpeg.clone());
        self.page_decoding = true;
        self.page_tried = seq;
        let task = cx.background_executor().spawn(async move {
            let bgra = jpeg_pixels(&jpeg)?;
            Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
        });
        cx.spawn(async move |this, cx| {
            let render = task.await;
            let _ = this.update(cx, |loom, cx| {
                loom.page_decoding = false;
                if let Some(render) = render {
                    if let Some((_, old)) = loom.page_image.replace((seq, render)) {
                        loom.old_pages.push(old);
                    }
                    cx.notify();
                }
                loom.decode_frame(cx);
            });
        })
        .detach();
    }

    /// The watched page is drawn `width` wide: the instance hears it in the screen's
    /// pixels, so its frames come no bigger. By the width alone, at the browser's
    /// shape, so the page's own height, which comes with its first frame, asks nothing
    /// again.
    fn page_shown(&mut self, width: f32, window: &Window) {
        let w = width * window.scale_factor();
        if let Some(f) = self.model.show_page_at(w, w * 800.0 / 1280.0) {
            self.send(f);
        }
    }

    /// Lets go of the frames drawn before, and of the page once nothing's watched.
    fn decode_page(&mut self, window: &mut Window) {
        for old in self.old_pages.drain(..) {
            let _ = window.drop_image(old);
        }
        if self.model.frame.is_none() {
            if let Some((_, old)) = self.page_image.take() {
                let _ = window.drop_image(old);
            }
        }
    }

    /// The agent's browser as a small window at the chat's bottom right, while it
    /// browses and the panel beside the chat shows something else. A click opens
    /// it in the panel.
    fn browser_mini(
        &mut self,
        agent: &model::Agent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.decode_page(window);
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        let state = self.model.browser(&agent.id);
        let (fw, fh) = self
            .model
            .frame
            .as_ref()
            .map_or((1280.0, 800.0), |f| (f.width.max(1.0), f.height.max(1.0)));
        let w = MINI_W;
        let h = mini_page_height(fw, fh);
        self.page_shown(w, window);
        let page = match self.page_image.clone() {
            Some((_, image)) => img(ImageSource::Render(image))
                .size_full()
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Loading the page…")
                .into_any_element(),
        };
        let words = if state.person {
            "You have it".to_string()
        } else if !state.asked.is_empty() {
            "Needs you".to_string()
        } else {
            format!("{} is browsing", agent.name)
        };
        let shown = motion::arrive(&SharedString::from("browser-mini"), 0.22);
        let id = agent.id.clone();
        div()
            .id("browser-mini")
            .absolute()
            .right(px(space::L))
            .bottom(px(space::S - 8.0 * (1.0 - shown)))
            .opacity(shown)
            .w(px(w))
            .flex()
            .flex_col()
            .rounded(px(radius::CARD))
            .overflow_hidden()
            .border_1()
            .border_color(p.line)
            .bg(p.raised)
            .shadow_lg()
            .cursor_pointer()
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| {
                let id = this.model.selected.clone();
                this.watch(id, cx);
            }))
            .child(
                div()
                    .h(px(control::CHIP))
                    .px(px(space::S))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_2)
                    .child(div().text_color(tint).child(Icon::new(Ico::Globe).xsmall()))
                    .child(div().flex_1().truncate().child(words))
                    .child(
                        div()
                            .id("browser-mini-shut")
                            .tooltip(tip("Hide until the browser opens again"))
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .text_color(p.ink_3)
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
                            .child(Icon::new(Ico::X).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.browser_shut = Some(id.clone());
                                this.watch(None, cx);
                            })),
                    ),
            )
            .child(div().w(px(w)).h(px(h)).bg(p.hover).child(page))
    }

    /// A handle over the panel's left edge, which drags to make the panel wider or
    /// narrower. It shows as a stronger line on hover and while dragged.
    fn side_edge(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.p;
        let dragging = self.side_drag.is_some();
        div()
            .id("side-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-3.0))
            .w(px(7.0))
            .flex()
            .justify_center()
            .cursor_col_resize()
            .occlude()
            .group("side-edge")
            .child(
                div()
                    .w(px(1.0))
                    .h_full()
                    .when(dragging, |d| d.bg(p.line_strong))
                    .group_hover("side-edge", |s| s.bg(p.line_strong)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.side_drag = Some((e.position.x.as_f32(), width));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    /// While the panel's edge is dragged, follows the pointer anywhere in the
    /// window, and keeps the width where it's let go. It draws nothing.
    fn side_dragger(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        let up = this.clone();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = this.update(cx, |this, cx| {
                        let Some((from, start)) = this.side_drag else {
                            return;
                        };
                        if !e.dragging() {
                            this.side_drag_end(cx);
                            return;
                        }
                        let w = window.viewport_size().width.as_f32();
                        let to = start + from - e.position.x.as_f32();
                        this.side_w = Some(side_width(true, w, this.bar_width(w), Some(to)));
                        cx.notify();
                    });
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let _ = up.update(cx, |this, cx| this.side_drag_end(cx));
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }

    fn side_drag_end(&mut self, cx: &mut Context<Self>) {
        if self.side_drag.take().is_none() {
            return;
        }
        if let Some(w) = self.side_w {
            let _ = std::fs::write(
                std::path::Path::new(&data_dir()).join("side-width"),
                format!("{}", w.round()),
            );
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

    /// The person's key, on the page they've taken over: a named key, the text it
    /// types, or what's on the clipboard for Ctrl+V (⌘V). Whether it was the page's.
    fn page_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let k = &e.keystroke;
        let paste = k.key == "v"
            && k.modifiers.secondary(cx)
            && !k.modifiers.alt
            && !k.modifiers.shift
            && !e.prefer_character_input;
        let input = if paste {
            let text = cx
                .read_from_clipboard()
                .and_then(|c| c.text())
                .unwrap_or_default();
            if text.is_empty() {
                return true;
            }
            serde_json::json!({"kind": "text", "text": text})
        } else {
            match page_input(k, e.prefer_character_input) {
                PageKey::Send(input) => input,
                PageKey::Keep => return true,
                PageKey::Pass => return false,
            }
        };
        for f in self.model.browser_input(input) {
            self.send(f);
        }
        true
    }

    /// The agent's browser beside its chat.
    fn browser_panel(
        &mut self,
        agent: &model::Agent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.p;
        let width = {
            let ww = window.viewport_size().width.as_f32();
            side_width(true, ww, self.bar_width(ww), self.side_w)
        };
        let (header, screen, footer) =
            self.browser_view(agent, width - 2.0 * space::L, None, window, cx);
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::L))
            .relative()
            .border_l_1()
            .border_color(p.line)
            .child(self.side_edge(width, cx))
            .child(header)
            .child(screen)
            .child(footer)
    }

    /// The agent's browser big over the window, from the panel's full screen control,
    /// its page as large as the window lets it be and asked for at that size. Esc, the
    /// same control or a click beside it puts it back beside the chat.
    fn browser_full(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !self.browser_big {
            return None;
        }
        let agent = self
            .model
            .selected
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .filter(|a| {
                self.screen == Screen::Chat
                    && self.browser_docked
                    && self.model.watching.as_deref() == Some(a.id.as_str())
            })
            .cloned();
        let Some(agent) = agent else {
            self.browser_big = false;
            return None;
        };
        let p = self.p;
        let k = motion::arrive(&SharedString::from("browser-full"), 0.15);
        if k < 1.0 {
            window.request_animation_frame();
        }
        // Under the title bar, which keeps the window's own controls.
        let size = window.viewport_size();
        let (w, h) = (size.width.as_f32(), size.height.as_f32() - layout::TITLEBAR);
        let card_w = (w - 2.0 * space::XL).max(320.0);
        let card_h = (h - 2.0 * space::XL).max(240.0);
        // What's left for the page under the header and over the line under it.
        let page_w = card_w - 2.0 * space::L;
        let page_h = card_h - 2.0 * space::L - 2.0 * space::M - BROWSER_HEAD - control::BUTTON;
        let (header, screen, footer) = self.browser_view(&agent, page_w, Some(page_h), window, cx);
        Some(
            div()
                .id("browser-full")
                .absolute()
                .top(px(layout::TITLEBAR))
                .left_0()
                .right_0()
                .bottom_0()
                .occlude()
                .opacity(k)
                .bg(black().opacity(0.45))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.browser_big = false;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(card_w))
                        .max_h(px(card_h))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(space::M))
                        .p(px(space::L))
                        .bg(p.bg)
                        .rounded(px(radius::WINDOW))
                        .border_1()
                        .border_color(p.line_strong)
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(header.w_full())
                        .child(screen)
                        .child(footer.w_full()),
                ),
        )
    }

    /// The agent's browser, beside the chat or full screen: over the page its name, the
    /// full screen control and the close; the page as it changes, at most `most_w` wide
    /// and `most_h` tall, at its own shape; and under it what's going on, with Take over
    /// or Hand back. A click on the page takes it over, so the person's clicks, scrolls
    /// and keys go to it.
    fn browser_view(
        &mut self,
        agent: &model::Agent,
        most_w: f32,
        most_h: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Div, Stateful<Div>, Div) {
        let p = self.p;
        let big = self.browser_big;
        let tint = thread::color(&agent.color, p.dark);
        let state = self.model.browser(&agent.id);
        self.decode_page(window);
        let (fw, fh) = self
            .model
            .frame
            .as_ref()
            .map_or((1280.0, 800.0), |f| (f.width.max(1.0), f.height.max(1.0)));
        let (pw, ph) = page_fit(fw, fh, most_w - 4.0, most_h.map(|h| h - 4.0));
        self.page_shown(pw, window);
        let bounds = self.page_bounds.clone();
        let page = match self.page_image.clone() {
            Some((_, image)) => img(ImageSource::Render(image))
                .size_full()
                .object_fit(ObjectFit::Fill)
                .rounded(px(radius::BUTTON - 2.0))
                .into_any_element(),
            None => div()
                .size_full()
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
        let open = state.open;
        let asked = !state.asked.is_empty();
        // Until it's theirs, a click on the page takes it over: said under the pointer,
        // and all the while the agent asks for them.
        let hint = (open && !person && self.page_image.is_some()).then(|| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(space::M))
                .flex()
                .justify_center()
                .when(!asked, |d| {
                    d.opacity(0.0)
                        .group_hover("browser-page", |s| s.opacity(1.0))
                })
                .child(
                    div()
                        .h(px(control::CHIP))
                        .px(px(space::M))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .bg(p.button)
                        .text_color(p.button_ink)
                        .text_size(px(size::SMALL))
                        .font_weight(FontWeight::MEDIUM)
                        .shadow_md()
                        .child("Click to take over"),
                )
        });
        let screen = div()
            .id("browser-page")
            .key_context(PAGE_KEYS)
            .track_focus(&self.browser_keys)
            .group("browser-page")
            .relative()
            .flex_none()
            .w(px(pw + 4.0))
            .h(px(ph + 4.0))
            .rounded(px(radius::BUTTON))
            .overflow_hidden()
            .border_2()
            .border_color(if person { tint } else { p.line })
            .when(open && !person, |d| d.cursor_pointer())
            .child(page)
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .children(hint)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    window.focus(&this.browser_keys, cx);
                    let Some((x, y)) = this.page_point(e.position) else {
                        return;
                    };
                    let click = serde_json::json!({"kind": "click", "x": x, "y": y});
                    for f in this.model.browser_input(click) {
                        this.send(f);
                    }
                    cx.notify();
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
                let agent = this.model.watching.clone().unwrap_or_default();
                if !this.model.browser(&agent).person {
                    return;
                }
                cx.stop_propagation();
                // Summed and sent at most every 40 ms: the smooth wheel moves a little
                // each frame, and each piece was a command and a DevTools call.
                this.wheel.0 = (x, y);
                this.wheel.1 += dy;
                if this.wheel.2 {
                    return;
                }
                this.wheel.2 = true;
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(40))
                        .await;
                    let _ = this.update(cx, |loom, _| {
                        let ((x, y), dy, _) = std::mem::take(&mut loom.wheel);
                        let wheel = serde_json::json!({"kind": "wheel", "x": x, "y": y, "dy": dy});
                        for f in loom.model.browser_input(wheel) {
                            loom.send(f);
                        }
                    });
                })
                .detach();
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                // Full screen, Esc puts the browser back beside the chat, as Esc
                // steps back everywhere, rather than going to the page.
                if this.browser_big && e.keystroke.key == "escape" {
                    return;
                }
                if this
                    .model
                    .browser(this.model.watching.as_deref().unwrap_or_default())
                    .person
                    && this.page_key(e, cx)
                {
                    cx.stop_propagation();
                }
            }));
        let words = if person && state.recording {
            match state.steps {
                0 | 1 => "Recording. Do the task, then turn Record off.".to_string(),
                n => format!("Recording, {n} steps. Do the task, then turn Record off."),
            }
        } else if person {
            format!(
                "You have control. {} waits until you hand it back.",
                agent.name
            )
        } else if asked {
            state.asked.clone()
        } else if open {
            format!("{} is browsing.", agent.name)
        } else {
            String::new()
        };
        let hand = if person {
            self.button("browser-give", "Hand back", true)
        } else {
            self.button("browser-take", "Take over", asked)
        }
        .flex_none()
        .when(!open, |d| d.opacity(0.4))
        .on_click(cx.listener(move |this, _, window, cx| {
            if let Some(f) = this.model.take_browser(!person) {
                this.send(f);
            }
            if !person {
                window.focus(&this.browser_keys, cx);
            }
            cx.notify();
        }));
        // Teaching a task: while the person has the browser, what they do is kept
        // as steps, and the agent writes them up as a skill for them to look at.
        let recording = state.recording;
        let record = person.then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(space::S))
                .child(
                    div()
                        .text_size(px(size::SMALL))
                        .text_color(if recording { p.ink } else { p.ink_2 })
                        .child("Record"),
                )
                .child(
                    self.switch("browser-record", recording)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(f) = this.model.record_browser(!recording) {
                                this.send(f);
                            }
                            window.focus(&this.browser_keys, cx);
                            cx.notify();
                        })),
                )
        });
        let corner = |id: &'static str, icon: Ico, tooltip: &'static str| {
            div()
                .id(id)
                .tooltip(tip(tooltip))
                .p(px(space::XS))
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .text_color(p.ink_3)
                .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
                .child(Icon::new(icon).small())
        };
        let header = div()
            .h(px(BROWSER_HEAD))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(space::S))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_2)
                    .child(Icon::new(Ico::Globe).small().text_color(tint))
                    .child(div().truncate().child(format!("{}'s browser", agent.name))),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(space::XS))
                    .child(
                        if big {
                            corner("browser-big", Ico::Minimize, "Back beside the chat")
                        } else {
                            corner("browser-big", Ico::Maximize, "Full screen")
                        }
                        // The click took the keys off the page: they go back to it.
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.browser_big = !this.browser_big;
                            window.focus(&this.browser_keys, cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        corner("browser-close", Ico::X, "Close")
                            // Back to the small window while it browses.
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.browser_big = false;
                                if this
                                    .model
                                    .browser(&this.model.watching.clone().unwrap_or_default())
                                    .open
                                {
                                    this.browser_docked = false;
                                    cx.notify();
                                } else {
                                    this.watch(None, cx);
                                }
                            })),
                    ),
            );
        // What's going on, wrapping in the room beside the controls, which go under it
        // when there's too little.
        let footer = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(space::M))
            .child(
                div()
                    .flex_grow(1.0)
                    .flex_basis(px(200.0))
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .when(person, |d| {
                        d.child(div().flex_none().size(px(8.0)).rounded_full().bg(tint))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(size::SMALL))
                            .text_color(if person || asked { p.ink } else { p.ink_3 })
                            .child(words),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .children(record)
                    .child(hand),
            );
        (header, screen, footer)
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

    /// The masked fields the chat's proposed MCP servers need: one for each secret a
    /// card still asking names that the person doesn't have, kept while it asks.
    fn card_secrets(
        &mut self,
        rows: &[Row],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Rc<HashMap<String, Entity<InputState>>> {
        let wanted: HashSet<String> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Message(m) => Some(m),
                Row::Approval(_) => None,
            })
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| match b.card.as_ref().map(|c| &c.body) {
                Some(model::CardBody::McpAdd {
                    call,
                    settings,
                    state,
                    ..
                }) if state == "asking" => Some((call, settings)),
                _ => None,
            })
            .flat_map(|(call, settings)| {
                settings
                    .iter()
                    .filter(|s| s.wanted())
                    .map(move |s| format!("{call}/{}", s.secret))
            })
            .collect();
        if wanted.len() != self.card_secrets.len()
            || !wanted.iter().all(|k| self.card_secrets.contains_key(k))
        {
            let mut kept = HashMap::new();
            for key in wanted {
                // Named as it's kept, the secret's name.
                let name = key.split_once('/').map_or("", |(_, n)| n).to_string();
                let input = match self.card_secrets.get(&key) {
                    Some(input) => input.clone(),
                    None => cx.new(|cx| InputState::new(window, cx).placeholder(name).masked(true)),
                };
                kept.insert(key, input);
            }
            self.card_secrets = Rc::new(kept);
        }
        self.card_secrets.clone()
    }

    /// The person's answer to the MCP server an agent proposed on card `call`, with
    /// the secrets typed on it for Add.
    fn answer_mcp_add(&mut self, call: &str, answer: &str, cx: &mut Context<Self>) {
        let prefix = format!("{call}/");
        let secrets: Vec<(String, String)> = if answer == "add" {
            self.card_secrets
                .iter()
                .filter_map(|(key, input)| {
                    key.strip_prefix(&prefix)
                        .map(|name| (name.to_string(), input.read(cx).value().to_string()))
                })
                .collect()
        } else {
            Vec::new()
        };
        let f = self.model.answer_mcp_add(call, answer, &secrets);
        self.send(f);
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
            .max_w(px(self.prefs.column()))
            .flex()
            .flex_col()
            .gap(px(space::XS));
        for (k, item) in items.iter().enumerate() {
            let editing = self.queue_editing.as_deref() == Some(item.id.as_str());
            let tool = |name: &str, icon: Icon| {
                let words = match name {
                    "up" => "Move up",
                    "steer" => "Send now, stopping the answer",
                    "save" => "Save",
                    "edit" => "Edit",
                    _ => "Remove",
                };
                div()
                    .id(SharedString::from(format!("queue-{name}-{}", item.id)))
                    .tooltip(tip(words))
                    .size(px(control::ICON_SMALL))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::ROW))
                    .text_color(p.ink_3)
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
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
                // Moving up comes first: the first can't go higher, so its empty place
                // sits by the words, and its other buttons line up with the rest.
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
                .when(k == 0, |d| {
                    d.child(div().size(px(control::ICON_SMALL)).flex_none())
                })
                .child(tool("steer", Icon::new(Ico::Zap)).on_click(cx.listener(
                    move |this, _, _, cx| {
                        let f = this.model.queue_cmd("queue.steer", &steer_id, None);
                        this.send(f);
                        cx.notify();
                    },
                )))
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

    /// The agent asking for something only the person should know, typed straight into
    /// its page: a card above the composer with a masked box.
    fn secret_ask(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.model.browser(&agent.id);
        if state.secret.is_empty() {
            return None;
        }
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        let card =
            CardFrame::new()
                .lead(card_icon(p, Ico::ShieldCheck, tint))
                .title(state.secret.clone())
                .row(Input::new(&self.secret_in))
                .row(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(format!(
                            "It goes into the page. {} never sees it.",
                            agent.name
                        )),
                )
                .action(card_button(p, "secret-give", "Fill it in", true).on_click(
                    cx.listener(|this, _, window, cx| this.give_secret(true, window, cx)),
                ))
                .action(card_button(p, "secret-skip", "Not now", false).on_click(
                    cx.listener(|this, _, window, cx| this.give_secret(false, window, cx)),
                ));
        Some(self.above_composer(card.draw(p, 1.0)))
    }

    /// The agent asking for the person in its browser, above the composer while
    /// its browser isn't showing. Take over opens it beside the chat, theirs.
    fn browser_ask(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Option<Div> {
        let state = self.model.browser(&agent.id);
        if state.asked.is_empty()
            || (self.browser_docked && self.model.watching.as_deref() == Some(agent.id.as_str()))
        {
            return None;
        }
        let p = self.p;
        let tint = thread::color(&agent.color, p.dark);
        let id = agent.id.clone();
        let card = CardFrame::new()
            .lead(card_icon(p, Ico::Globe, tint))
            .title(state.asked.clone())
            .side(card_button(p, "browser-open", "Take over", true).on_click(
                cx.listener(move |this, _, window, cx| {
                    this.watch(Some(id.clone()), cx);
                    if let Some(f) = this.model.take_browser(true) {
                        this.send(f);
                    }
                    window.focus(&this.browser_keys, cx);
                }),
            ));
        Some(self.above_composer(card.draw(p, 1.0)))
    }

    /// A card above the composer, as wide as the chat's column.
    fn above_composer(&self, card: Div) -> Div {
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(space::XL))
            .pb(px(space::S))
            .child(div().w_full().max_w(px(self.prefs.column())).child(card))
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
            .role(Role::Button)
            .aria_label(label.to_string())
            .px(px(space::L))
            .h(px(control::BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .text_size(px(size::SMALL))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .when(primary, |d| {
                d.text_color(p.button_ink).soft_hover(
                    p.button,
                    motion::mix(p.button, p.bg, 0.18),
                    None,
                )
            })
            .when(!primary, |d| {
                d.border_1()
                    .border_color(p.line_strong)
                    .text_color(p.ink)
                    .soft_hover(transparent_black(), p.hover, None)
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
                                    .h(px(control::CHOICE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(space::S))
                                    .rounded(px(radius::BUTTON))
                                    .soft_hover(p.button, motion::mix(p.button, p.bg, 0.18), None)
                                    .text_color(p.button_ink)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
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
                                    .soft_ink(p.ink_3, p.ink)
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
                            .size(px(control::ICON))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::BUTTON))
                            .text_color(p.ink_2)
                            .cursor_pointer()
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
                    .h(px(control::BUTTON_WIDE))
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
                        .soft_ink(p.ink_3, p.ink)
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
                    .soft_hover(transparent_black(), p.hover, None)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.end_search(window, cx);
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

    /// A pinned agent atop the sidebar: a tile with its thread drawn large in its shape and
    /// its name under it. It's what the agent's row would be: the thread moves as the agent
    /// works, a dot shows a reply not seen yet, and what it's on shows under the pointer. A
    /// click opens its chat, a right-click its menu, and it drags into a new place.
    fn pinned_tile(&self, a: &model::Agent, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let selected =
            self.model.selected.as_deref() == Some(a.id.as_str()) && self.screen == Screen::Chat;
        let unread = self.model.unread.contains(&a.id);
        let preview = self.model.preview(a);
        let id = a.id.clone();
        let bg = if selected { p.active } else { p.raised };
        div()
            .id(SharedString::from(format!("tile-{}", a.id)))
            .relative()
            .size(px(TILE))
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .rounded(px(radius::CARD))
            .cursor_pointer()
            .bg(bg)
            .when(!selected, move |d| d.soft_hover(bg, p.raised_2, None))
            .tooltip(tip_text(if preview.is_empty() {
                a.name.clone()
            } else {
                format!("{} · {preview}", a.name)
            }))
            .on_mouse_down(MouseButton::Right, {
                let id = id.clone();
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.agent_menu_at(id.clone(), e.position, cx);
                })
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
            .on_drag(AgentDrag::of(a, p), AgentDrag::ghost)
            .drag_over::<AgentDrag>(move |s, _, _, _| s.bg(p.raised_2))
            .on_drop(self.drop_agent(&a.id, cx))
            // The same key as its row's would be, so pinning or unpinning it while it
            // works carries on its motion.
            .child(thread::strand_in(
                format!("side-{}", a.id),
                self.model.mood(&a.id),
                thread::color(&a.color, p.dark),
                a.outline(),
                TILE_THREAD,
                16.0,
                self.model.finished_at(&a.id),
            ))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(size::SMALL))
                    .font_weight(if unread {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if selected || unread { p.ink } else { p.ink_2 })
                    .truncate()
                    .child(a.name.clone()),
            )
            .when(unread, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(space::S))
                        .right(px(space::S))
                        .size(px(6.0))
                        .rounded_full()
                        .bg(p.ink),
                )
            })
    }

    /// The whole sidebar: the knot and the wordmark, the + and the collapse, search when
    /// it's shown, the pinned agents as tiles, the rest of them each on one line, and at
    /// the foot the server, the update icon and the gear.
    fn whole_sidebar(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let searching = !self.model.query.trim().is_empty();
        let mut list = div().flex().flex_col().gap(px(2.0)).px(px(space::S));
        // The pinned agents, as tiles over the rest.
        let mut tiles = Vec::new();
        let agents = self.model.agents.clone();
        for a in agents.iter() {
            if a.pinned.is_some() {
                tiles.push(self.pinned_tile(a, cx));
                continue;
            }
            let selected = self.model.selected.as_deref() == Some(a.id.as_str())
                && self.screen == Screen::Chat;
            let mood = self.model.mood(&a.id);
            let preview = self.model.preview(a);
            // A reply that came while its chat wasn't open: the row is bold until it is.
            let unread = self.model.unread.contains(&a.id);
            let id = a.id.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("agent-{}", a.id)))
                    .role(Role::Button)
                    .aria_label(a.name.clone())
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::M))
                    .py(px(space::S))
                    .rounded(px(radius::ROW))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(p.active))
                    .when(!selected, move |d| {
                        d.soft_hover(transparent_black(), p.hover, None)
                    })
                    .on_mouse_down(MouseButton::Right, {
                        let id = id.clone();
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.agent_menu_at(id.clone(), e.position, cx);
                        })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
                    .on_drag(AgentDrag::of(a, p), AgentDrag::ghost)
                    .drag_over::<AgentDrag>(move |s, _, _, _| s.bg(p.hover))
                    .on_drop(self.drop_agent(&a.id, cx))
                    .child(thread::strand(
                        format!("side-{}", a.id),
                        mood,
                        thread::color(&a.color, p.dark),
                        a.outline(),
                        26.0,
                        10.0,
                        self.model.finished_at(&a.id),
                    ))
                    // One line whatever the agent is doing: what it's on sits quietly after
                    // its name, so starting or stopping work never moves a row.
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
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
                            .when(!preview.is_empty(), |d| {
                                d.child(
                                    div()
                                        .min_w_0()
                                        .max_w(relative(0.55))
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
                                        .child(preview),
                                )
                            }),
                    ),
            );
        }
        if !tiles.is_empty() {
            list = div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_center()
                        .gap(px(space::XS))
                        .px(px(space::S))
                        .children(tiles),
                )
                .child(list);
        }
        let server_open = self.popup.is_some_and(is_server);
        let settings_open = self.popup.is_some() && !server_open;
        let search_shown = self.prefs.search || self.search_peek || searching;
        div()
            .w(px(layout::SIDEBAR))
            .h_full()
            .relative()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("side-top")
                    .window_control_area(WindowControlArea::Drag)
                    .h(px(layout::TITLEBAR))
                    .flex()
                    .items_center()
                    .justify_between()
                    // On a Mac, after the traffic lights.
                    .pl(px(if self.lights { LIGHTS_END } else { space::L }))
                    .pr(px(space::M))
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
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(self.new_agent_button("new-agent", control::ICON_SMALL, cx))
                            .child(self.bar_button(false, cx)),
                    ),
            )
            .when(search_shown, |d| {
                d.child(
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
            })
            .child(
                div()
                    .px(px(space::L + space::XS))
                    .pt(px(space::S))
                    .pb(px(space::XS))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .when(searching && !self.model.hits.is_empty(), |d| {
                        d.child("Found")
                    }),
            )
            .child(
                div()
                    .id("side-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, e: &MouseDownEvent, _, cx| {
                            this.open_ctx(e.position, Ctx::Agents, cx)
                        }),
                    )
                    .child(if searching { self.hits(cx) } else { list }),
            )
            .when_some(self.update_pop(false, cx), |d, pop| d.child(pop))
            // The server, as its spool, on the left, and the settings on the right: each
            // opens the popup. While anything loom runs is behind, the update icon sits
            // just left of the gear.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .py(px(space::S))
                    .child(self.spools(server_open, cx))
                    .when_some(self.update_icon(cx), |d, icon| d.child(icon))
                    .child(self.gear(settings_open, cx)),
            )
    }

    /// The sidebar, whole or as a rail, its width easing between the two. What's in it is
    /// laid out at its own width and the sidebar reveals it, so nothing reflows while it
    /// moves.
    fn sidebar(&mut self, railed: bool, listening: bool, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let k = motion::level(
            &SharedString::from("sidebar-rail"),
            railed,
            crate::rail::SLIDE,
        );
        let width = layout::SIDEBAR + (layout::RAIL - layout::SIDEBAR) * k;
        // What's in it is a view of its own, drawn again only when what it shows changes
        // (`SidebarKey`), its threads move or a hover in it fades: a frame that only
        // changes the chat, such as a reply streaming, uses it as it was.
        let key = self.sidebar_key(railed);
        let view = match &self.side_view {
            Some(v) => v.clone(),
            None => {
                let this = cx.entity().downgrade();
                let v = cx.new(|_| SidebarView { this, railed });
                self.side_view = Some(v.clone());
                v
            }
        };
        if self.side_key.as_ref() != Some(&key) {
            view.update(cx, |v, cx| {
                v.railed = railed;
                cx.notify();
            });
            self.side_key = Some(key);
        }
        // Beside a Mac's traffic lights the rail draws its own colour, under the title
        // bar, which crosses the window over it (`rail`).
        let own = !(railed && self.lights);
        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .relative()
            .overflow_hidden()
            .when(own, |d| d.bg(p.side).border_r_1().border_color(p.line))
            // Its top drags the window. The view has this too, but what a view drawn from
            // its cache had in its window's title bar isn't kept, so it's here as well,
            // under the view, whose buttons keep the pointer from it.
            .child(
                div()
                    .id("side-drag")
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(layout::TITLEBAR))
                    .window_control_area(WindowControlArea::Drag),
            )
            // While a screen reader listens, it's built at every frame: a frame that uses
            // the view as it was leaves what's in it out of what the screen reader is told.
            .child(if listening {
                AnyView::from(view).into_any_element()
            } else {
                AnyView::from(view)
                    .cached(StyleRefinement::default().size_full())
                    .into_any_element()
            })
    }

    /// What the sidebar shows, for drawing it again only when that changes.
    fn sidebar_key(&self, railed: bool) -> SidebarKey {
        let model = &self.model;
        let behind = self.behind();
        SidebarKey {
            railed,
            lights: self.lights,
            p: self.p,
            agents: model
                .agents
                .iter()
                .map(|a| SidebarAgent {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    color: a.color.clone(),
                    pinned: a.pinned.is_some(),
                    outline: a.outline().map(<[(f32, f32)]>::to_vec),
                    mood: model.mood(&a.id),
                    finished: model.finished_at(&a.id),
                    preview: model.preview(a),
                    unread: model.unread.contains(&a.id),
                })
                .collect(),
            selected: model
                .selected
                .clone()
                .filter(|_| self.screen == Screen::Chat),
            query: model.query.clone(),
            hits: model
                .hits
                .iter()
                .map(|h| {
                    (
                        h.agent.clone(),
                        h.snippet.clone(),
                        h.marks.clone(),
                        h.person,
                        h.at,
                    )
                })
                .collect(),
            search: self.prefs.search || self.search_peek,
            popup: self.popup,
            servers: self
                .servers()
                .into_iter()
                .map(|(dir, m)| SidebarServer {
                    spool: spool_of(m),
                    name: server_name(m, &dir),
                    label: self.label(m, &dir),
                    heard: !m.unread.is_empty(),
                    behind: m.version.as_ref().is_some_and(|v| v.behind),
                    dir,
                })
                .collect(),
            here: self.here.clone(),
            conn: self.conn_words(),
            behind: behind.to_vec(),
            behind_all: self.hub().behind_all,
            update_open: !behind.is_empty() && self.update_pop_open(),
        }
    }

    /// The control that collapses the sidebar to the rail (`rail` false), or opens it
    /// from the rail: at the top of the rail, or beside a Mac's traffic lights in the
    /// title bar. It's always there, and says what it does out to its right, in the
    /// title bar, rather than under the pointer, over the chat.
    fn bar_button(&self, rail: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        let id = if rail { "bar-open" } else { "bar-collapse" };
        let icon = Icon::new(if rail {
            Ico::PanelLeftOpen
        } else {
            Ico::PanelLeftClose
        })
        .small();
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(if rail {
                "Open sidebar"
            } else {
                "Collapse sidebar"
            })
            .occlude()
            .size(px(control::ICON_SMALL))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
            .relative()
            .child(icon)
            .children(tip_beside(
                p,
                &motion::hover_key(id),
                if rail { BAR_OPEN } else { BAR_COLLAPSE }.into(),
                control::ICON_SMALL,
                control::ICON_SMALL,
            ))
            .on_click(cx.listener(|this, _, window, cx| this.toggle_bar(window, cx)))
    }

    /// The + that makes an agent, `size` square.
    fn new_agent_button(
        &self,
        id: &'static str,
        size: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label("New agent")
            .occlude()
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .text_color(p.ink_2)
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, None)
            .child(Icon::new(IconName::Plus).small())
            .on_click(cx.listener(|this, _, _, cx| this.open(Screen::NewAgent, cx)))
    }

    /// The settings gear, which opens the popup on the page last open.
    fn gear(&self, settings_open: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.p;
        div()
            .id("settings")
            .role(Role::Button)
            .aria_label("Settings")
            .size(px(control::ICON_LARGE))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW))
            .text_color(if settings_open { p.ink } else { p.ink_3 })
            .cursor_pointer()
            .when(settings_open, |d| d.bg(p.active))
            .soft_hover(
                if settings_open {
                    p.active
                } else {
                    transparent_black()
                },
                p.hover,
                Some((if settings_open { p.ink } else { p.ink_3 }, p.ink)),
            )
            .child(Icon::new(Ico::Settings))
            .on_click(cx.listener(|this, _, _, cx| {
                let last = this.settings_last;
                this.open(last, cx)
            }))
    }

    /// The sidebar collapsed: the control that opens it (beside a Mac's traffic lights,
    /// in the title bar), each agent as its thread, the pinned ones first and a short
    /// line under them, with a dot while a reply waits and its name out to its right
    /// under the pointer, and at the foot the update icon over the gear over the server's
    /// yarn.
    fn rail(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut list = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.0))
            .py(px(space::XS));
        let agents = self.model.agents.clone();
        for (n, a) in agents.iter().enumerate() {
            let selected = self.model.selected.as_deref() == Some(a.id.as_str())
                && self.screen == Screen::Chat;
            let unread = self.model.unread.contains(&a.id);
            let id = a.id.clone();
            let el = SharedString::from(format!("rail-{}", a.id));
            let hover = motion::hover_key(el.clone());
            // The pinned agents come first (model.rs): a line ends them.
            if n > 0 && agents[n - 1].pinned.is_some() && a.pinned.is_none() {
                list = list.child(
                    div()
                        .w(px(space::L))
                        .h(px(1.0))
                        .my(px(space::XS))
                        .flex_none()
                        .bg(p.line_strong),
                );
            }
            let tile = div()
                .id(el)
                .role(Role::Button)
                .aria_label(a.name.clone())
                .relative()
                .size(px(RAIL_ROW))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .when(selected, |d| d.bg(p.active))
                .when(!selected, move |d| {
                    d.soft_hover(transparent_black(), p.hover, None)
                })
                .children(tip_beside(
                    p,
                    &hover,
                    a.name.clone().into(),
                    RAIL_ROW,
                    RAIL_ROW,
                ))
                .on_mouse_down(MouseButton::Right, {
                    let id = id.clone();
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.agent_menu_at(id.clone(), e.position, cx);
                    })
                })
                .on_click(cx.listener(move |this, _, _, cx| this.select(id.clone(), cx)))
                .on_drag(AgentDrag::of(a, p), AgentDrag::ghost)
                .drag_over::<AgentDrag>(move |s, _, _, _| s.bg(p.hover))
                .on_drop(self.drop_agent(&a.id, cx))
                .child(thread::strand(
                    format!("side-{}", a.id),
                    self.model.mood(&a.id),
                    thread::color(&a.color, p.dark),
                    a.outline(),
                    26.0,
                    10.0,
                    self.model.finished_at(&a.id),
                ))
                .when(unread, |d| d.child(rail_dot(p)));
            // The one on screen has no hover to fade, so its name keeps the pointer itself.
            list = list.child(if selected {
                motion::tracks_hover(tile, hover)
            } else {
                tile
            });
        }
        let server_open = self.popup.is_some_and(is_server);
        let settings_open = self.popup.is_some() && !server_open;
        // The other servers' yarns over the one on screen, which is at the very bottom.
        let mut others = Vec::new();
        let mut mine = None;
        for (dir, model) in self.servers() {
            let here = dir == self.here;
            let state = spool_of(model);
            let name = server_name(model, &dir);
            let heard = !here && !model.unread.is_empty();
            let button = div()
                .id(SharedString::from(format!("rail-server-{dir}")))
                .on_mouse_down(MouseButton::Right, {
                    let dir = dir.clone();
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        this.open_ctx(e.position, Ctx::Server(dir.clone()), cx)
                    })
                })
                .relative()
                .size(px(control::SERVER))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .when(here && server_open, |d| d.bg(p.active))
                .soft_hover(
                    if here && server_open {
                        p.active
                    } else {
                        transparent_black()
                    },
                    p.hover,
                    None,
                )
                .tooltip(tip_text(if here {
                    format!("{} · {}", self.label(model, &dir), self.conn_words())
                } else {
                    self.label(model, &dir)
                }))
                .child(self.yarn_marked(model, state, &name, 32.0))
                .when(heard, |d| d.child(rail_dot(p)));
            let button = if here {
                button.on_click(cx.listener(|this, _, _, cx| this.open(Screen::Server, cx)))
            } else {
                button.on_click(cx.listener(move |this, _, _, cx| this.show_server(&dir, cx)))
            };
            if here {
                mine = Some(button);
            } else {
                others.push(button);
            }
        }
        let yarns = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(space::XS))
            .children(others)
            .children(mine);
        let lights = self.lights;
        let body = div()
            .id("rail")
            .w(px(layout::RAIL))
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            // Beside a Mac's traffic lights, the control is in the title bar, after them.
            .when(!lights, |d| {
                d.child(
                    div()
                        .id("rail-top")
                        .window_control_area(WindowControlArea::Drag)
                        .w_full()
                        .h(px(layout::TITLEBAR))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(self.bar_button(true, cx)),
                )
            })
            .child(
                div()
                    .id("rail-list")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(space::XS))
                    .pb(px(space::S))
                    .when_some(self.update_icon(cx), |d, icon| d.child(icon))
                    .child(self.gear(settings_open, cx))
                    .child(yarns),
            );
        if !lights {
            return div().h_full().child(body);
        }
        // A Mac's traffic lights reach past the rail, so the title bar crosses the window
        // over it, as a Mac's does when its sidebar is put away, with the lights and the
        // control at its start, and the rail begins under it, level with the chat.
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("rail-lights")
                    .window_control_area(WindowControlArea::Drag)
                    .w_full()
                    .h(px(layout::TITLEBAR))
                    .flex_none()
                    .bg(p.bg)
                    .border_b_1()
                    .border_color(p.line),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .bg(p.side)
                    .border_r_1()
                    .border_color(p.line)
                    .child(body),
            )
    }

    // --- The update icon (core/updates.calx decides; this draws it) ---

    /// What's behind across the app, as the first core says it.
    fn behind(&self) -> &[model::Behind] {
        &self.hub().behind
    }

    /// A device behind, by name: this app, or a server as the sidebar names it.
    fn behind_name(&self, b: &model::Behind) -> String {
        if b.app {
            return "This app".into();
        }
        self.servers()
            .into_iter()
            .find(|(dir, _)| *dir == b.device)
            .map(|(dir, m)| self.label(m, &dir))
            .unwrap_or_else(|| "A server".into())
    }

    /// Whether the update popup shows: the pointer is on the icon or the popup, or left
    /// them a moment ago, so it can cross between them.
    fn update_pop_open(&self) -> bool {
        let (icon, pop) = self.update_over;
        icon || pop
            || self
                .update_left
                .is_some_and(|t| t.elapsed() < Duration::from_millis(300))
    }

    /// Notes the pointer coming onto or leaving the icon (`which` 0) or the popup (1).
    fn update_hovered(&mut self, which: usize, over: bool, cx: &mut Context<Self>) {
        if which == 0 {
            self.update_over.0 = over;
        } else {
            self.update_over.1 = over;
        }
        if !over {
            self.update_left = Some(Instant::now());
            // Closes once the moment is up, unless the pointer came back.
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(320))
                    .await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        cx.notify();
    }

    /// The icon just left of the gear, while anything loom runs is behind: a click updates
    /// everything this person may update, and hovering shows each device.
    fn update_icon(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let behind = self.behind();
        if behind.is_empty() {
            return None;
        }
        let p = self.p;
        let busy = behind.iter().any(|b| {
            matches!(
                b.state.as_str(),
                "asked" | "downloading" | "waiting" | "restarting"
            )
        });
        let all = self.hub().behind_all;
        let open = self.update_pop_open();
        Some(
            div()
                .id("update")
                .size(px(control::ICON_LARGE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .when(open, |d| d.bg(p.hover))
                .soft_hover(
                    if open { p.hover } else { transparent_black() },
                    p.hover,
                    Some((if busy { p.ink_3 } else { p.ink_2 }, p.ink)),
                )
                .child(Icon::new(Ico::CircleFadingArrowUp))
                .on_hover(cx.listener(|this, over: &bool, _, cx| this.update_hovered(0, *over, cx)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if all {
                        this.run_updates(None);
                    }
                    cx.notify();
                })),
        )
    }

    /// The popup over the sidebar's foot: each device that's behind, with its versions and
    /// its own Update, or how its update stands, or plainly why it can't be updated here.
    /// From the rail (`rail`), it's beside the rail instead, level with the icon, and
    /// placed in the window rather than the sidebar.
    fn update_pop(&self, rail: bool, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let behind = self.behind().to_vec();
        if behind.is_empty() || !self.update_pop_open() {
            return None;
        }
        let p = self.p;
        let mut list = div().flex().flex_col().gap(px(2.0));
        for b in &behind {
            let name = self.behind_name(b);
            let versions = format!("{} → {}", b.version, b.latest);
            let (note, failed): (Option<String>, bool) = match b.state.as_str() {
                "asked" | "downloading" => (Some("Downloading…".into()), false),
                "waiting" => (Some("Updates once nothing runs".into()), false),
                "restarting" => (Some("Restarting…".into()), false),
                "failed" => (
                    Some(if b.error.is_empty() {
                        "Didn't update".into()
                    } else {
                        format!("Didn't update: {}", b.error)
                    }),
                    true,
                ),
                _ if !b.can => (
                    Some(if b.why.is_empty() {
                        "It can't update itself where it runs.".into()
                    } else {
                        b.why.clone()
                    }),
                    false,
                ),
                _ if !b.may => (Some("An owner or admin updates it".into()), false),
                _ => (None, false),
            };
            let offer = b.may && b.can && matches!(b.state.as_str(), "" | "failed");
            let device = b.device.clone();
            let id = SharedString::from(format!("update-{}", b.device));
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::S))
                    .py(px(space::S))
                    .rounded(px(radius::ROW))
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
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(size::CAPTION))
                                    .text_color(p.ink_3)
                                    .child(versions),
                            )
                            .when_some(note, |d, n| {
                                d.child(
                                    div()
                                        .text_size(px(size::CAPTION))
                                        .text_color(if failed { p.bad } else { p.ink_3 })
                                        .child(n),
                                )
                            }),
                    )
                    .when(offer, |d| {
                        d.child(self.small_button(id, "Update", false).flex_none().on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.run_updates(Some(&device));
                                cx.notify();
                            }),
                        ))
                    }),
            );
        }
        let offered = behind
            .iter()
            .filter(|b| b.may && b.can && matches!(b.state.as_str(), "" | "failed"))
            .count();
        let head = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(space::S))
            .pt(px(space::XS))
            .pb(px(space::XS))
            .child(
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child("Updates"),
            )
            .when(offered > 1, |d| {
                d.child(
                    div()
                        .id("update-all")
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_2)
                        .cursor_pointer()
                        .soft_ink(p.ink_2, p.ink)
                        .child("Update all")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_updates(None);
                            cx.notify();
                        })),
                )
            });
        let pop = div().id("update-pop").absolute();
        let pop = if rail {
            pop.left(px(layout::RAIL + space::XS))
                .w(px(layout::SIDEBAR - 2.0 * space::S))
                .bottom(px(rail_update_bottom(self.servers().len())))
        } else {
            pop.left(px(space::S))
                .right(px(space::S))
                .bottom(px(control::SERVER + space::S * 2.0 - space::XS))
        };
        Some(
            pop.p(px(space::XS))
                .flex()
                .flex_col()
                .rounded(px(radius::CARD))
                .border_1()
                .border_color(p.line)
                .bg(p.raised)
                .shadow_lg()
                .occlude()
                .on_hover(cx.listener(|this, over: &bool, _, cx| this.update_hovered(1, *over, cx)))
                .child(head)
                .child(list),
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
        let permission = model::mode_name(agent.mode());
        // The agent's browser, while it's open: a globe that shows it beside the chat.
        let browsing = self.model.browser(&agent.id).open;
        let watching =
            self.browser_docked && self.model.watching.as_deref() == Some(agent.id.as_str());
        let tint = thread::color(&agent.color, p.dark);
        let globe = div()
            .id("browser-toggle")
            .role(Role::Button)
            .aria_label("Browser")
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
            .soft_hover(
                if watching {
                    p.hover
                } else {
                    transparent_black()
                },
                p.hover,
                None,
            )
            .child(Icon::new(Ico::Globe).small())
            .child("Browser")
            .on_click(cx.listener(move |this, _, _, cx| {
                let id = this.model.selected.clone();
                if this.model.watching == id && this.browser_docked {
                    this.browser_docked = false;
                    cx.notify();
                } else {
                    this.watch(id, cx);
                }
            }));
        // The chat's pins, while it has some, or they're open beside it.
        let pinned = self.model.messages.iter().filter(|m| m.pinned).count();
        let pins_open = self.side == Some(Screen::Pins);
        let pins = div()
            .id("pins-toggle")
            .role(Role::Button)
            .aria_label(format!("Pins: {pinned}"))
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
            .soft_hover(
                if pins_open {
                    p.hover
                } else {
                    transparent_black()
                },
                p.hover,
                None,
            )
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
                .role(Role::Button)
                .aria_label(agent.name.clone())
                .occlude()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::ROW))
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .when(
                    self.menu == Some(Menu::Agent) && self.menu_at.is_none(),
                    |d| d.bg(p.hover),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.menu_at = None;
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
                .children(self.brain_chip(&agent, cx))
                .child(
                    div()
                        .id("permission-menu")
                        .role(Role::Button)
                        .aria_label(format!("Permission: {permission}"))
                        .occlude()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::ROW))
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_2)
                        .cursor_pointer()
                        .soft_hover(transparent_black(), p.hover, None)
                        .when(self.menu == Some(Menu::Permission), |d| d.bg(p.hover))
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
            .role(Role::MenuItem)
            .aria_label(title.to_string())
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .text_size(px(size::SMALL))
            .text_color(if danger { p.bad } else { p.ink })
            .soft_hover(transparent_black(), p.hover, None)
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
        let k = motion::arrive(&SharedString::from("plus-menu"), 0.16);
        let panel = div()
            .occlude()
            // It rises into place as it fades in.
            .opacity(k)
            .mb(px(space::S - 6.0 * (1.0 - k)))
            .w(px(layout::MENU_WIDE))
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

    /// The header's brain chip: the program's icon and what the agent is set to on it,
    /// such as "Opus 5.5 · Medium". It opens one menu just under it, with each setting
    /// the program offers (its model, then its effort) under its name.
    fn brain_chip(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let p = self.p;
        let picked = agent.picked()?;
        let listed = self.model.program_listed(&agent.program).cloned();
        let open = self.menu == Some(Menu::Model);
        let menu = open.then(|| self.brain_menu(agent, cx));
        Some(
            div()
                .id("brain-chip")
                .role(Role::Button)
                .aria_label(format!("Brain: {picked}"))
                .relative()
                .occlude()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::ROW))
                .text_size(px(size::CAPTION))
                .text_color(p.ink_2)
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
                .when(open, |d| d.bg(p.hover))
                .flex()
                .items_center()
                .gap(px(space::XS + 2.0))
                .child(self.brain_icon(listed.as_ref(), 12.0))
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
                }))
                .when_some(menu, |d, menu| {
                    d.child(
                        div().absolute().bottom_0().right_0().child(deferred(
                            anchored()
                                .anchor(Anchor::TopRight)
                                .snap_to_window_with_margin(px(space::S))
                                .child(div().pt(px(space::XS)).child(menu)),
                        )),
                    )
                }),
        )
    }

    /// The brain chip's menu: each setting under its name, with the person's default
    /// marked on its choice. Picking that one has the agent follow the default, and
    /// picking another gives it its own.
    fn brain_menu(&self, agent: &model::Agent, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut panel = div()
            .occlude()
            .min_w(px(220.0))
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
        for (n, o) in agent.picks().into_iter().enumerate() {
            let follows = agent.follows(&o.id);
            let on = agent.chosen(&o.id);
            let default = agent.followed(&o.id);
            panel = panel.child(
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
                let is_default = value == default;
                let picked = if follows { is_default } else { value == on };
                panel = panel.child(
                    self.pick_row(format!("pick-{}-{value}", o.id), name, picked, is_default)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = if is_default {
                                this.model.follow_config(&agent_id, &option)
                            } else {
                                this.model.set_config(&agent_id, &option, &value)
                            };
                            if let Some(f) = f {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        panel
    }

    /// One choice in a setting's menu, ticked when it's the one, and saying so beside
    /// the person's default.
    fn pick_row(&self, id: String, label: String, picked: bool, default: bool) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(SharedString::from(id))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::M))
            .py(px(space::S))
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .text_size(px(size::SMALL))
            .text_color(p.ink)
            .soft_hover(transparent_black(), p.hover, None)
            .child(div().flex_1().child(label))
            .when(default, |d| {
                d.child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child("default"),
                )
            })
            .when(picked, |d| {
                d.child(
                    div()
                        .text_color(p.ink_2)
                        .child(Icon::new(Ico::Check).small()),
                )
            })
    }

    /// Opens a right-click menu at the pointer, in place of any menu open.
    fn open_ctx(&mut self, at: Point<Pixels>, on: Ctx, cx: &mut Context<Self>) {
        self.menu = None;
        self.plus = None;
        self.ctx = Some((at, on));
        cx.notify();
    }

    /// The menu a right-click opened, at the pointer, with what can be done to what was
    /// clicked: the same things its buttons and the agent menu do.
    fn ctx_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.p;
        let (at, on) = self.ctx.clone()?;
        let close = |this: &mut Loom, cx: &mut Context<Loom>| {
            this.ctx = None;
            cx.notify();
        };
        let mut panel = div()
            .occlude()
            .w(px(layout::MENU))
            .p(px(space::XS))
            .flex()
            .flex_col()
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(move |this, _, _, cx| close(this, cx)));
        match on {
            Ctx::Message(id) => {
                let m = self.rows.iter().find_map(|r| match r {
                    Row::Message(m) if m.id == id => Some(m.clone()),
                    _ => None,
                })?;
                let text = model::copy_text(&m, &self.model.tools);
                if !text.is_empty() {
                    panel = panel.child(
                        self.menu_item("ctx-copy", Ico::Copy, "Copy", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                                this.notice = Some(("Copied.".into(), Instant::now(), true));
                                close(this, cx);
                            })),
                    );
                }
                let pinned = m.pinned;
                let pin_id = m.id.clone();
                panel = panel
                    .child(
                        self.menu_item(
                            "ctx-pin",
                            if pinned { Ico::PinOff } else { Ico::Pin },
                            if pinned { "Unpin" } else { "Pin" },
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.pin(&pin_id, !pinned);
                            this.send(f);
                            close(this, cx);
                        })),
                    )
                    .child(
                        self.menu_item("ctx-branch", Ico::GitBranch, "Branch from here", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.branch(&m.id);
                                this.send(f);
                                close(this, cx);
                            })),
                    );
            }
            Ctx::Server(dir) => {
                let name = self
                    .servers()
                    .into_iter()
                    .find(|(d, _)| *d == dir)
                    .map(|(_, m)| server_name(m, &dir))?;
                let here = dir == self.here;
                let rename_dir = dir.clone();
                panel = panel
                    .child(
                        self.menu_item(
                            "ctx-server",
                            Ico::Settings,
                            if here { "Server settings" } else { "Open" },
                            false,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.ctx = None;
                            if here {
                                this.open(Screen::Server, cx);
                            } else {
                                this.show_server(&dir, cx);
                            }
                            cx.notify();
                        })),
                    )
                    .child({
                        // Its page, on screen, with the name field ready to type in.
                        let dir = rename_dir;
                        self.menu_item("ctx-rename-server", Ico::Pencil, "Rename", false)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.ctx = None;
                                if dir != this.here {
                                    this.show_server(&dir, cx);
                                }
                                this.open(Screen::Server, cx);
                                this.server_name_in.update(cx, |s, cx| s.focus(window, cx));
                                cx.notify();
                            }))
                    })
                    .child(
                        self.menu_item("ctx-copy-server", Ico::Copy, "Copy address", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
                                this.notice = Some(("Copied.".into(), Instant::now(), true));
                                close(this, cx);
                            })),
                    );
            }
            Ctx::Agents => {
                panel = panel.child(
                    self.menu_item("ctx-new-agent", Ico::Plus, "New agent", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.ctx = None;
                            this.open(Screen::NewAgent, cx);
                            cx.notify();
                        })),
                );
            }
        }
        Some(
            deferred(
                anchored()
                    .position(at)
                    .snap_to_window_with_margin(px(space::S))
                    .child(panel),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// A picture from the chat, big over the window: whole once it's fetched, its
    /// thumbnail until then, with its name and where it is among them. A click beside it
    /// or Esc closes it, the arrows (or ← and →) go between the pictures of the stack it
    /// was opened on, or else the chat's, and Open hands it to its own program.
    fn viewer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let f = self.model.viewing.clone()?;
        let files = self.model.files.clone();
        let path = files
            .get(&f.file)
            .or_else(|| files.get(&format!("{}.thumb", f.file)))
            .cloned();
        let all = self.model.viewer_pictures();
        let at = all.iter().position(|x| x.file == f.file);
        let (before, after) = match at {
            Some(i) => (i > 0, i + 1 < all.len()),
            None => (false, false),
        };
        let key = SharedString::from(format!("viewer-{:?}", self.viewer_at));
        let k = motion::arrive(&key, 0.15);
        if k < 1.0 {
            window.request_animation_frame();
        }
        let size = window.viewport_size();
        let (w, h) = (size.width.as_f32(), size.height.as_f32());
        let round = |id: &'static str, icon: IconName| {
            div()
                .id(id)
                .size(px(36.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .text_color(white())
                .bg(white().opacity(0.08))
                .hover(|s| s.bg(white().opacity(0.18)))
                .child(Icon::new(icon).small())
        };
        let picture = match path {
            Some(path) => {
                // Sized here, as in the chat (`file_view`): a most alone let a tall
                // picture draw past the window.
                let (most_w, most_h) = ((w - 160.0).max(100.0), (h - 140.0).max(100.0));
                let (pw, ph) = match picture_size(&path) {
                    Some((iw, ih)) => fit(iw, ih, most_w, most_h),
                    None => (most_w, most_h),
                };
                img(std::path::PathBuf::from(path))
                    .image_cache(&kept_images(cx).1)
                    .w(px(pw))
                    .h(px(ph))
                    .rounded(px(radius::ROW))
                    .into_any_element()
            }
            None => div()
                .size(px(160.0))
                .rounded(px(radius::ROW))
                .bg(white().opacity(0.06))
                .into_any_element(),
        };
        let opened = f.clone();
        let count = match at {
            Some(i) if all.len() > 1 => Some(format!("{} / {}", i + 1, all.len())),
            _ => None,
        };
        let caption = div()
            .max_w(px((w - 2.0 * space::XL).max(100.0)))
            .flex()
            .items_center()
            .gap(px(space::M))
            .text_size(px(size::SMALL))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(white().opacity(0.75))
                    .child(f.name.clone()),
            )
            .when_some(count, |d, c| {
                d.child(div().flex_none().text_color(white().opacity(0.5)).child(c))
            });
        Some(
            div()
                .id("viewer")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .opacity(k)
                .bg(black().opacity(0.9))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(space::M))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.model.viewing = None;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(picture),
                )
                .child(caption)
                .child(
                    div()
                        .absolute()
                        .top(px(layout::TITLEBAR + space::S))
                        .right(px(space::M))
                        .flex()
                        .gap(px(space::S))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            round("viewer-open", IconName::ExternalLink)
                                .tooltip(tip("Open in its program"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.model.viewing = None;
                                    this.open_file(&opened, cx);
                                    cx.notify();
                                })),
                        )
                        .child(round("viewer-close", IconName::Close).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.model.viewing = None;
                                cx.notify();
                            },
                        ))),
                )
                .when(before, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left(px(space::L))
                            .top(px(h / 2.0 - 18.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                round("viewer-before", IconName::ChevronLeft)
                                    .on_click(cx.listener(|this, _, _, cx| this.view_step(-1, cx))),
                            ),
                    )
                })
                .when(after, |d| {
                    d.child(
                        div()
                            .absolute()
                            .right(px(space::L))
                            .top(px(h / 2.0 - 18.0))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(
                                round("viewer-after", IconName::ChevronRight)
                                    .on_click(cx.listener(|this, _, _, cx| this.view_step(1, cx))),
                            ),
                    )
                }),
        )
    }

    fn menu_panel(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.p;
        if self.menu != Some(Menu::Agent) {
            self.armed = None;
            self.menu_at = None;
        }
        let menu = self.menu?;
        let under_header = self.menu_at.is_none();
        // It takes the pointer, so a click on an item reaches it and not the sheet under
        // it, which would close the menu first.
        let panel = div()
            .occlude()
            .when(under_header, |d| {
                d.absolute().top(px(layout::TITLEBAR - 4.0))
            })
            .w(px(layout::MENU))
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
                .when(under_header, |d| d.left(px(space::S)))
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
                            let f = this.model.programs();
                            this.send(f);
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
                            this.fresh_armed = false;
                            this.custom_color = false;
                            this.show_side(Screen::EditAgent, cx);
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
                // Deleting can't be undone: the first click asks for a second.
                .child(
                    self.menu_item(
                        "delete",
                        Ico::Trash,
                        if self.armed == Some("delete") {
                            "Click again to delete"
                        } else {
                            "Delete agent"
                        },
                        true,
                    )
                    .when(self.armed == Some("delete"), |d| {
                        d.bg(motion::mix(p.raised, p.bad, 0.12))
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.armed != Some("delete") {
                            this.armed = Some("delete");
                            cx.notify();
                            return;
                        }
                        if let Some(f) = this.model.delete_agent() {
                            this.send(f);
                        }
                        this.menu = None;
                        cx.notify();
                    })),
                ),
            // It opens under the brain chip in the header (`brain_chip`).
            Menu::Model => return None,
            Menu::Permission => {
                let default_title = format!(
                    "Default: {}",
                    model::mode_name(&self.model.defaults.permission)
                );
                let mut list = panel.right(px(space::S)).child(
                    self.mode_item("mode-default", Ico::SlidersHorizontal, &default_title, "")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pick_mode(None, cx);
                        })),
                );
                let modes: [(&'static str, Ico, &'static str); 3] = [
                    ("ask", Ico::Hand, "ask"),
                    ("auto", Ico::ShieldCheck, "auto"),
                    ("allow", Ico::Zap, "allow"),
                ];
                // In the words the header and the agent's page use.
                for (id, icon, mode) in modes {
                    let title = model::mode_name(mode);
                    list = list.child(self.mode_item(id, icon, title, mode).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.pick_mode(Some(mode), cx);
                        },
                    )));
                }
                list
            }
        })
    }

    /// Sets the open agent's permission mode from the chat's menu, or with None, has it
    /// follow the default.
    fn pick_mode(&mut self, mode: Option<&str>, cx: &mut Context<Self>) {
        if let Some(agent) = self.model.selected.clone() {
            if let Some(f) = self.model.set_permission(&agent, mode) {
                self.send(f);
            }
        }
        self.menu = None;
        cx.notify();
    }

    /// How wide the chat's column is, to the pixel: the window less the sidebar and
    /// the panel beside the chat, at most the column's widest.
    ///
    /// A message's text has it in its id. GPUI answers "how wide would you be" with the
    /// size it last wrapped to, so a bubble that hugs its text would keep a narrow width
    /// after the chat widens (a panel closing, the window growing). A new id is a new
    /// text, which measures itself afresh.
    fn chat_room(&self, window: &Window) -> i32 {
        let w = window.viewport_size().width.as_f32();
        let side = match self.side {
            Some(side) => side_width(
                side == Screen::Plugins || side == Screen::Page,
                w,
                self.bar_width(w),
                self.side_w,
            ),
            // The agent's browser beside the chat, unless it's full screen.
            None if self.browser_docked && self.model.watching.is_some() && !self.browser_big => {
                side_width(true, w, self.bar_width(w), self.side_w)
            }
            None => 0.0,
        };
        (w - self.bar_width(w) - side)
            .clamp(0.0, self.prefs.column())
            .round() as i32
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
        let flow = self.flow.clone();
        let secrets = self.card_secrets(&rows, window, cx);
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
        self.follow_browser(agent.as_ref());
        // Beside the chat: the agent's browser, or an agent's page or a plugin's panel.
        let watched = agent
            .clone()
            .filter(|a| self.model.watching.as_deref() == Some(a.id.as_str()));
        let docked = watched.clone().filter(|_| self.browser_docked);
        // While it browses and the panel shows something else, or nothing: the small window.
        let mini = watched
            .filter(|a| !self.browser_docked && self.model.browser(&a.id).open)
            .map(|a| self.browser_mini(&a, window, cx));
        let panel = match (docked, self.side) {
            // Full screen, it's drawn over the window instead (`browser_full`).
            (Some(_), _) if self.browser_big => None,
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
        let show = Show {
            tools: self.prefs.tools,
            thoughts: self.prefs.thoughts,
            text_scale: self.prefs.text_scale(),
            bubbles: self.prefs.bubbles,
            vibrant: self.prefs.vibrant,
            newest: false,
            room: self.chat_room(window),
        };
        // The newest message, whose choices still answer.
        let newest = rows.iter().rev().find_map(|r| match r {
            Row::Message(m) if !m.queued => Some(m.id.clone()),
            _ => None,
        });
        let tools = self.model.tools.clone();
        let files = self.model.files.clone();
        let this = cx.entity().downgrade();
        let new_line = self.model.new_line.clone();
        let column = self.prefs.column();
        // Clear of the small browser window, while it shows.
        let pad = mini.is_some().then(|| {
            let (fw, fh) = self
                .model
                .frame
                .as_ref()
                .map_or((1280.0, 800.0), |f| (f.width.max(1.0), f.height.max(1.0)));
            28.0 + mini_page_height(fw, fh) + space::L
        });
        let inputs = ChatInputs {
            rows,
            flow,
            tools,
            files,
            secrets,
            branches,
            name,
            tint,
            mood,
            open,
            show,
            newest,
            new_line,
            column,
            p,
            pad,
        };
        // The list is a view of its own, built with the window, and each finished message
        // in it another, drawn from its cache until what it shows changes (`RowView`): a
        // frame that only moves a thread, here or elsewhere, lays out none of them again.
        // The list itself isn't drawn from a cache: in a view built again, every view
        // inside it is built again too.
        let view = match &self.chat_view {
            Some(v) => v.clone(),
            None => {
                kept_images(cx);
                let scroller = self.scroller.clone();
                let this = this.clone();
                let v = cx.new(|_| ChatList {
                    scroller,
                    this,
                    inputs: None,
                    views: Rc::default(),
                    pruned: None,
                });
                self.chat_view = Some(v.clone());
                v
            }
        };
        view.update(cx, |v, _| v.inputs = Some(inputs));
        let list = AnyView::from(view);

        // A chat whose messages are on their way from the cache, as just after switching
        // agents, isn't the empty chat: the hero would show for that moment.
        let empty = self.rows.is_empty() && self.model.chat_known();
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
        // Set only when it changes: setting it marks the composer to be drawn again.
        if self.composer_placeholder != placeholder {
            self.composer_placeholder = placeholder.clone();
            self.composer
                .update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
        }
        // Previews of pictures that aren't attached any more leave the GPU.
        let gone: Vec<String> = self
            .previews
            .keys()
            .filter(|k| !self.model.attachments.iter().any(|a| &a.id == *k))
            .cloned()
            .collect();
        for k in gone {
            if let Some(Some(r)) = self.previews.remove(&k) {
                let _ = window.drop_image(r);
            }
        }
        let mut attached = div().flex().flex_wrap().gap(px(space::S));
        for a in self.model.attachments.clone() {
            let id = a.id.clone();
            let (note, color) = match (&a.file, &a.error) {
                (_, Some(e)) => (e.clone(), p.bad),
                (Some(f), None) => (model::size_of(f.size), p.ink_3),
                (None, None) => ("Sending…".to_string(), p.ink_3),
            };
            // A picture as itself, its × in its corner, faint while it goes up.
            if a.image() && a.error.is_none() {
                let going_up = a.file.is_none();
                attached = attached.child(
                    div()
                        .relative()
                        .size(px(64.0))
                        .flex_none()
                        .rounded(px(radius::ROW))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.line)
                        .bg(p.bubble)
                        .when(going_up, |d| d.opacity(0.6))
                        .child(match self.previews.get(&a.id) {
                            Some(Some(small)) => img(ImageSource::Render(small.clone()))
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .into_any_element(),
                            Some(None) => div().into_any_element(),
                            None => img(std::path::PathBuf::from(&a.path))
                                .image_cache(&kept_images(cx).0)
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .into_any_element(),
                        })
                        .child(
                            div()
                                .id(SharedString::from(format!("unattach-{}", a.id)))
                                .absolute()
                                .top(px(3.0))
                                .right(px(3.0))
                                .size(px(18.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(black().opacity(0.6))
                                .text_color(white())
                                .cursor_pointer()
                                .tooltip(tip_text(a.name.clone()))
                                .child(Icon::new(IconName::Close).xsmall())
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.unattach(&id, cx)),
                                ),
                        ),
                );
                continue;
            }
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
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
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
                .max_w(px(self.prefs.column()))
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::M))
                .py(px(space::S))
                .bg(p.raised)
                .border_1()
                .border_color(p.line)
                .rounded(px(radius::COMPOSER))
                // The plus menu opens just above the composer, from its left edge.
                .relative()
                .when_some(plus_menu, |d, menu| {
                    d.child(div().absolute().top_0().left_0().child(deferred(
                        anchored().anchor(Anchor::BottomLeft).child(menu),
                    )))
                })
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
                                        .role(Role::Button)
                                        .aria_label("Attach, skills and routines")
                                        .size(px(control::ICON_COMPOSER))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(radius::BUTTON))
                                        .text_color(p.ink_2)
                                        .cursor_pointer()
                                        .soft_hover(transparent_black(), p.hover, None)
                                        .when(plus_open, |d| d.bg(p.hover))
                                        .when(!plus_open, |d| {
                                            d.tooltip(tip("Attach, skills and routines"))
                                        })
                                        .child(Icon::new(Ico::Plus).small())
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.plus = if this.plus.is_some() {
                                                None
                                            } else {
                                                Some(Plus::Main)
                                            };
                                            cx.notify();
                                        })),
                                ),
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
                        .when(cfg!(any(windows, target_os = "macos")), |d| {
                            let (name, didnt) = if cfg!(target_os = "macos") {
                                (
                                    "Dictation",
                                    "Dictation didn't start. Turn it on in System Settings → Keyboard.",
                                )
                            } else {
                                (
                                    "Voice typing",
                                    "Voice typing didn't start. Press Windows+H in the message box.",
                                )
                            };
                            d.child(
                                div()
                                    .id("dictate")
                                    .role(Role::Button)
                                    .aria_label(name)
                                    .size(px(control::ICON_COMPOSER))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(radius::BUTTON))
                                    .text_color(p.ink_2)
                                    .cursor_pointer()
                                    .soft_hover(transparent_black(), p.hover, None)
                                    .tooltip(tip(name))
                                    .child(Icon::new(Ico::Mic).small())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.composer.update(cx, |c, cx| c.focus(window, cx));
                                        if !shell::dictate() {
                                            this.model.notices.push(didnt.into());
                                            this.take_notice(cx);
                                        }
                                    })),
                            )
                        })
                        .child(
                            div()
                                .id("send")
                                .role(Role::Button)
                                .aria_label(if busy { "Stop" } else { "Send" })
                                .size(px(control::ICON_COMPOSER))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(radius::BUTTON))
                                .soft_hover(p.button, motion::mix(p.button, p.bg, 0.18), None)
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
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .relative()
                    .child(if empty && !in_task {
                        self.hero(agent.as_ref(), composer.take(), cx)
                            .into_any_element()
                    } else {
                        div().flex_1().min_h_0().child(list).into_any_element()
                    })
                    .when_some(mini, |d, m| d.child(m)),
            )
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
                        .size(px(control::ICON_LARGE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::BUTTON))
                        .text_color(p.ink_2)
                        .cursor_pointer()
                        .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
                    .size(px(control::ICON_LARGE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_2()
                    .border_color(if picked { dot } else { transparent_black() })
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, None)
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
                        // As wide as the die on the right, so the field sits in the
                        // middle, under the heading and over the colours.
                        .child(div().size(px(40.0)).flex_none())
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
                                .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
                    .h(px(control::BUTTON_WIDE))
                    .px(px(space::XXL))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .rounded(px(radius::BUTTON))
                    .soft_hover(tint, motion::mix(tint, p.bg, 0.15), None)
                    .text_color(motion::ink_on(tint, p.button_ink, p.button))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
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

    /// The opening, over everything for a moment on every launch: one thread draws
    /// itself across the middle, the mark settles on it, and the window opens along
    /// that line to show the app.
    fn splash(&self, h: f32) -> Option<Div> {
        let t = self.launched?.elapsed().as_secs_f32();
        if t >= SPLASH_SECONDS {
            return None;
        }
        let p = self.p;
        let span = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
        let ease = |x: f32| 1.0 - (1.0 - x).powi(3);
        let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
        let reveal = ease(span(0.0, 0.6));
        let word_in = ease(span(0.3, 0.75));
        let word_out = smooth(span(1.0, 1.25));
        let calm = 0.55 + 0.45 * smooth(span(0.8, 1.25));
        let close = smooth(span(1.1, SPLASH_SECONDS));
        let (mark, _) = wordmark(p, 34.0);
        const BAND: f32 = 64.0;
        let shown = word_in * (1.0 - word_out);
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
                    .child(thread::weave(vec![p.ink_3], calm, reveal, BAND)),
            )
            // The mark sits on the thread, which passes behind it, and settles into
            // place as it comes.
            .child(
                div()
                    .relative()
                    .top(px(6.0 * (1.0 - word_in)))
                    .px(px(space::L))
                    .bg(p.bg)
                    .opacity(shown)
                    .child(mark),
            );
        // It opens from the line outwards, each half drawing back to its edge, and
        // the thread goes as they part.
        let center = h / 2.0;
        let half = |top: bool| {
            let d = div().absolute().left_0().right_0().occlude().bg(p.bg);
            if top {
                d.top_0().h(px(center * (1.0 - close)))
            } else {
                d.bottom_0().h(px((h - center) * (1.0 - close)))
            }
        };
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(half(true))
                .child(half(false))
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(center - BAND / 2.0))
                        .h(px(BAND))
                        .opacity(1.0 - smooth(span(1.1, 1.35)))
                        .child(band),
                ),
        )
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
                    .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
                // An empty chat keeps its composer at the normal width, whatever the
                // chat width is.
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
                    .soft_hover(transparent_black(), p.hover, None)
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

    /// A choice in a row of them, filled when it's the one on, and saying so beside the
    /// person's default.
    fn chip(
        &self,
        id: impl Into<ElementId>,
        label: &str,
        on: bool,
        default: bool,
    ) -> Stateful<Div> {
        let p = self.p;
        div()
            .id(id.into())
            .role(Role::Button)
            .aria_label(label.to_string())
            .h(px(control::CHIP))
            .px(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::XS + 2.0))
            .rounded(px(radius::CHIP))
            .text_size(px(size::SMALL))
            .cursor_pointer()
            .when(on, |d| d.bg(p.button).text_color(p.button_ink))
            .when(!on, |d| {
                d.text_color(p.ink_2)
                    .soft_hover(p.hover, p.active, Some((p.ink_2, p.ink)))
            })
            .child(label.to_string())
            .when(default, |d| {
                d.child(
                    div()
                        .text_size(px(size::CAPTION))
                        .opacity(0.6)
                        .child("default"),
                )
            })
    }

    /// One of an agent's settings as a row of chips. The person's default is one of
    /// them, marked so: picking it has the agent follow the default, and picking another
    /// gives it its own. `own` is None while it follows; `default` is the default's
    /// value and name, put first when it isn't one of `values`.
    fn choices(
        &self,
        id: &str,
        mut values: Vec<(String, String)>,
        own: Option<String>,
        default: (String, String),
        pick: Pick,
        cx: &mut Context<Self>,
    ) -> Div {
        if !values.iter().any(|(v, _)| *v == default.0) {
            values.insert(0, default.clone());
        }
        let on = own.unwrap_or_else(|| default.0.clone());
        let mut row = div().flex().flex_wrap().gap(px(space::XS + 2.0));
        for (value, label) in values {
            let is_default = value == default.0;
            let chosen = (!is_default).then(|| value.clone());
            let pick = pick.clone();
            row = row.child(
                self.chip(format!("{id}-{value}"), &label, value == on, is_default)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = pick(this, chosen.as_deref()) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        row
    }

    /// The agent's colours and its shape, on one line `room` pixels wide under its
    /// thread at the top of its edit panel: as many of loom's colours as fit, always
    /// with the one it has, its own colour, a line, and at the end its shape.
    fn look_picker(&self, room: f32, cx: &mut Context<Self>) -> Div {
        // Each control is a square with a gap after it; the line sits in what's left
        // over between the colours and the shape.
        let slots = ((room + space::XS) / (control::ICON + space::XS)).floor() as usize;
        let colors = slots.saturating_sub(2).clamp(4, THREAD_COLORS.len());
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(self.color_picker(colors, cx))
            .child(
                div()
                    .w(px(1.0))
                    .h(px(18.0))
                    .flex_none()
                    .bg(self.p.line_strong),
            )
            .child(self.shape_picker(cx))
    }

    /// The agent's colours: `fit` of loom's, the one it has among them, and its own,
    /// which opens a strip of every hue and a field for one as #rrggbb.
    fn color_picker(&self, fit: usize, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let color = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .map(|a| a.color.clone())
            .unwrap_or_default();
        let own = thread::hex(&color).is_some();
        let swatch = |id: ElementId, picked: bool| {
            div()
                .id(id)
                .size(px(control::ICON))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::BUTTON))
                .border_2()
                .border_color(if picked { p.ink_2 } else { transparent_black() })
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, None)
        };
        let mut row = div().flex().items_center().gap(px(space::XS));
        // The first that fit, with the agent's own in place of the last when it's
        // further along.
        let mut shown: Vec<usize> = (0..fit).collect();
        if let Some(at) = THREAD_COLORS.iter().position(|(c, _)| *c == color) {
            if at >= fit {
                shown[fit - 1] = at;
            }
        }
        for i in shown {
            let (c, title) = THREAD_COLORS[i];
            let c: &'static str = c;
            row = row.child(
                swatch(("edit-swatch", i).into(), color == *c)
                    .child(
                        div()
                            .size(px(18.0))
                            .rounded(px(radius::ROW))
                            .bg(thread::color(c, p.dark)),
                    )
                    .tooltip(tip(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.custom_color = false;
                        if let Some(f) = this.model.set_color(c) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        // Its own colour: a small colour wheel, which opens a bigger one to pick from,
        // with a field for one as #rrggbb.
        let wheel_icon = swatch("edit-swatch-own".into(), own || self.custom_color)
            .child(thread::wheel(18.0, 48, p.dark, None))
            .when(!self.custom_color, |d| d.tooltip(tip("Its own colour")))
            .on_click(cx.listener(|this, _, window, cx| {
                // The click that closed it, outside it, doesn't open it again.
                if this
                    .color_closed_at
                    .is_some_and(|at| at.elapsed().as_millis() < 300)
                {
                    return;
                }
                this.custom_color = !this.custom_color;
                let color = this
                    .model
                    .editing
                    .as_ref()
                    .and_then(|id| this.model.agent(id))
                    .map(|a| a.color.clone())
                    .filter(|c| thread::hex(c).is_some())
                    .unwrap_or_default();
                this.color_in
                    .update(cx, |s, cx| s.set_value(color, window, cx));
                cx.notify();
            }));
        let mut own_slot = div().relative().child(wheel_icon);
        if self.custom_color {
            const WHEEL: f32 = 184.0;
            let bounds = self.wheel_bounds.clone();
            let pick = |this: &mut Loom,
                        at: Point<Pixels>,
                        window: &mut Window,
                        cx: &mut Context<Loom>| {
                let Some(b) = this.wheel_bounds.get() else {
                    return;
                };
                let (x, y) = ((at.x - b.origin.x).as_f32(), (at.y - b.origin.y).as_f32());
                let Some(hex) = thread::wheel_pick(x, y, WHEEL) else {
                    return;
                };
                if let Some(f) = this.model.set_color(&hex) {
                    this.send(f);
                }
                this.color_in
                    .update(cx, |s, cx| s.set_value(hex, window, cx));
                cx.notify();
            };
            let wheel = div()
                .id("edit-wheel")
                .relative()
                .size(px(WHEEL))
                .cursor_pointer()
                .child(thread::wheel(
                    WHEEL,
                    180,
                    p.dark,
                    own.then(|| color.clone()),
                ))
                .child(
                    canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                        this.wheel_drag = true;
                        pick(this, e.position, window, cx)
                    }),
                )
                // A drag picks as it goes, only when it started on the wheel.
                .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, window, cx| {
                    if this.wheel_drag && e.pressed_button == Some(MouseButton::Left) {
                        pick(this, e.position, window, cx)
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.wheel_drag = false),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.wheel_drag = false),
                );
            let popup = div()
                .occlude()
                .mt(px(space::XS))
                .p(px(space::M))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(space::M))
                .bg(p.raised)
                .border_1()
                .border_color(p.line_strong)
                .rounded(px(radius::CARD))
                .shadow_lg()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.custom_color = false;
                    this.color_closed_at = Some(Instant::now());
                    cx.notify();
                }))
                .child(wheel)
                .child(
                    div()
                        .w(px(WHEEL))
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            div()
                                .size(px(control::ICON_SMALL))
                                .flex_none()
                                .rounded(px(radius::ROW))
                                .bg(thread::color(&color, p.dark)),
                        )
                        .child(div().flex_1().child(Input::new(&self.color_in).small())),
                );
            own_slot = own_slot.child(
                div()
                    .absolute()
                    .top(px(control::ICON))
                    .right_0()
                    .child(deferred(anchored().anchor(Anchor::TopRight).child(popup))),
            );
        }
        row.child(own_slot)
    }

    /// The agent's shape (PLAN.md, "Its shape"): a button with its thread in the shape
    /// it has, moving as it does, which opens the shapes: the plain thread and the
    /// eight that come with loom, and under them a pad that shows its drawing, picks
    /// it with a click, and takes a new one in one stroke.
    fn shape_picker(&self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let Some(agent) = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .cloned()
        else {
            return div();
        };
        let tint = thread::color(&agent.color, p.dark);
        let mood = self.model.mood(&agent.id);
        let button = div()
            .id("edit-shape")
            .size(px(control::ICON))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .border_2()
            .border_color(if self.shape_open {
                p.ink_2
            } else {
                p.line_strong
            })
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, None)
            .child(thread::strand(
                format!("pick-{}", agent.id),
                mood,
                tint,
                agent.outline(),
                control::ICON - 8.0,
                6.0,
                self.model.finished_at(&agent.id),
            ))
            .when(!self.shape_open, |d| d.tooltip(tip("Shape")))
            .on_click(cx.listener(|this, _, _, cx| {
                // The click that closed it, outside it, doesn't open it again.
                if this
                    .shape_closed_at
                    .is_some_and(|at| at.elapsed().as_millis() < 300)
                {
                    return;
                }
                this.shape_open = !this.shape_open;
                this.draw_stroke.clear();
                cx.notify();
            }));
        let mut slot = div().relative().child(button);
        if !self.shape_open {
            return slot;
        }
        const TILE: f32 = 44.0;
        const COLUMNS: usize = 3;
        let wide = TILE * COLUMNS as f32 + space::XS * (COLUMNS - 1) as f32;
        let mut shapes: Vec<(&'static str, &'static str)> = vec![("", "Plain thread")];
        for (name, title) in crate::tokens::thread_shape::NAMES.iter().zip(SHAPE_TITLES) {
            shapes.push((name, title));
        }
        let mut grid = div().w(px(wide)).flex().flex_wrap().gap(px(space::XS));
        for (name, title) in shapes {
            let picked = agent.shape == name;
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("edit-shape-{name}")))
                    .size(px(TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::BUTTON))
                    .border_2()
                    .border_color(if picked { p.ink_2 } else { transparent_black() })
                    .cursor_pointer()
                    .soft_hover(transparent_black(), p.hover, None)
                    .child(thread::strand(
                        format!("pick-{}-{name}", agent.id),
                        Mood::Idle,
                        tint,
                        model::outline_of(name, &agent.drawing),
                        TILE - 14.0,
                        8.0,
                        None,
                    ))
                    .tooltip(tip(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.model.set_shape(name) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        // The pad: its drawing while nothing's being drawn, the stroke as it's drawn,
        // and a pencil while there's neither.
        const PAD_H: f32 = 96.0;
        let bounds = self.draw_bounds.clone();
        let stroke = self.draw_stroke.clone();
        let drawing = agent.drawing.clone();
        let picked = agent.shape == "drawn";
        let empty = stroke.is_empty() && drawing.is_empty();
        let add = |this: &mut Loom, at: Point<Pixels>, cx: &mut Context<Loom>| {
            let Some(b) = this.draw_bounds.get() else {
                return;
            };
            let (w, h) = (b.size.width.as_f32(), b.size.height.as_f32());
            let (x, y) = ((at.x - b.origin.x).as_f32(), (at.y - b.origin.y).as_f32());
            this.draw_stroke.push((x.clamp(0.0, w), y.clamp(0.0, h)));
            cx.notify();
        };
        // A stroke becomes its drawing; a click alone picks the one it has.
        let done = |this: &mut Loom, cx: &mut Context<Loom>| {
            if !this.draw_drag {
                return;
            }
            this.draw_drag = false;
            let stroke = std::mem::take(&mut this.draw_stroke);
            let f = if stroke.len() >= 8 {
                this.model.set_drawing(&stroke)
            } else if this
                .model
                .editing
                .as_ref()
                .and_then(|id| this.model.agent(id))
                .is_some_and(|a| !a.drawing.is_empty())
            {
                this.model.set_shape("drawn")
            } else {
                None
            };
            if let Some(f) = f {
                this.send(f);
            }
            cx.notify();
        };
        let pad = div()
            .id("edit-draw-pad")
            .relative()
            .w(px(wide))
            .h(px(PAD_H))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::BUTTON))
            .bg(p.bubble)
            .border_2()
            .border_color(if picked { p.ink_2 } else { transparent_black() })
            .cursor_crosshair()
            .when(empty, |d| {
                d.child(Icon::new(Ico::Pencil).text_color(p.ink_3))
            })
            .when(!empty, |d| d.tooltip(tip("Its drawing")))
            .when(empty, |d| d.tooltip(tip("Draw one")))
            .child(
                canvas(
                    move |b, _, _| bounds.set(Some(b)),
                    move |b, _, window, _| {
                        let (x0, y0) = (b.origin.x.as_f32(), b.origin.y.as_f32());
                        let (w, h) = (b.size.width.as_f32(), b.size.height.as_f32());
                        let (line, colour): (Vec<(f32, f32)>, Hsla) = if !stroke.is_empty() {
                            (stroke.iter().map(|(x, y)| (x0 + x, y0 + y)).collect(), tint)
                        } else {
                            // Its drawing, as big as the pad holds it.
                            let s = (h - 24.0) / 2.0;
                            let pts = drawing
                                .iter()
                                .map(|(x, y)| (x0 + w / 2.0 + x * s, y0 + h / 2.0 + y * s))
                                .collect();
                            (pts, if picked { tint } else { tint.opacity(0.55) })
                        };
                        if line.len() < 2 {
                            return;
                        }
                        let mut pb = PathBuilder::stroke(px(thread_stroke::LARGE));
                        for (i, (x, y)) in line.into_iter().enumerate() {
                            let at = point(px(x), px(y));
                            if i == 0 {
                                pb.move_to(at)
                            } else {
                                pb.line_to(at)
                            }
                        }
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, colour);
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.draw_drag = true;
                    this.draw_stroke.clear();
                    add(this, e.position, cx)
                }),
            )
            .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, _, cx| {
                if this.draw_drag && e.pressed_button == Some(MouseButton::Left) {
                    add(this, e.position, cx)
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| done(this, cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| done(this, cx)),
            );
        let popup = div()
            .occlude()
            .mt(px(space::XS))
            .p(px(space::S))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .bg(p.raised)
            .border_1()
            .border_color(p.line_strong)
            .rounded(px(radius::CARD))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.shape_open = false;
                this.draw_drag = false;
                this.shape_closed_at = Some(Instant::now());
                cx.notify();
            }))
            .child(grid)
            .child(pad);
        slot = slot.child(
            div()
                .absolute()
                .top(px(control::ICON))
                .right_0()
                .child(deferred(anchored().anchor(Anchor::TopRight).child(popup))),
        );
        slot
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
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(
                        div()
                            .text_size(px(size::CAPTION))
                            .text_color(p.ink_3)
                            .child("None yet. Set them up once, then pick them here."),
                    )
                    .child(
                        self.small_button("edit-mcp-setup", "Set up", false)
                            .on_click(cx.listener(|this, _, _, cx| this.open(Screen::Mcp, cx))),
                    ),
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
        // Code Mode: its tools reached through scripts, or each given plainly.
        let code_mode = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .is_none_or(|a| a.code_mode);
        let scripts = self.switch_row(
            "edit-code-mode",
            "Code Mode",
            "Uses its tools by writing small scripts, which keeps its context small",
            code_mode,
            cx,
            |this| {
                let on = this
                    .model
                    .editing
                    .as_ref()
                    .and_then(|id| this.model.agent(id))
                    .is_none_or(|a| a.code_mode);
                if let Some(f) = this.model.set_code_mode(!on) {
                    this.send(f);
                }
            },
        );
        // Its model and reasoning effort, as the chat's picker has them, once its
        // program has said what it offers; then its permission mode, what a message
        // while it's busy does, and its backup. Each row's default is one of its chips.
        let mut picks = Vec::new();
        if let Some(a) = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .cloned()
        {
            for o in a.picks() {
                let default = a.followed(&o.id);
                let own = (!a.follows(&o.id)).then(|| a.chosen(&o.id));
                let (agent_id, option) = (a.id.clone(), o.id.clone());
                let row = self.choices(
                    &format!("edit-pick-{}", o.id),
                    o.values.clone(),
                    own,
                    (default.clone(), value_name(&o, &default)),
                    Rc::new(move |this: &mut Loom, v: Option<&str>| match v {
                        None => this.model.follow_config(&agent_id, &option),
                        Some(v) => this.model.set_config(&agent_id, &option, v),
                    }),
                    cx,
                );
                picks.push(self.field(&o.name, row));
            }
            let default_mode = match self.model.defaults.permission.as_str() {
                "" => "ask".to_string(),
                m => m.to_string(),
            };
            let agent_id = a.id.clone();
            let modes = self.choices(
                "edit-mode",
                ["ask", "auto", "allow"]
                    .map(|m| (m.to_string(), model::mode_name(m).to_string()))
                    .to_vec(),
                (!a.permission.is_empty()).then(|| a.permission.clone()),
                (
                    default_mode.clone(),
                    model::mode_name(&default_mode).to_string(),
                ),
                Rc::new(move |this: &mut Loom, v: Option<&str>| {
                    this.model.set_permission(&agent_id, v)
                }),
                cx,
            );
            picks.push(self.field("Permission", modes));
            let default_busy = if self.model.defaults.busy == "steer" {
                "steer"
            } else {
                "queue"
            };
            let agent_id = a.id.clone();
            let busy = self.choices(
                "edit-busy",
                ["queue", "steer"]
                    .map(|b| (b.to_string(), busy_name(b).to_string()))
                    .to_vec(),
                (!a.busy.is_empty()).then(|| a.busy.clone()),
                (
                    default_busy.to_string(),
                    busy_name(default_busy).to_string(),
                ),
                Rc::new(move |this: &mut Loom, v: Option<&str>| this.model.set_busy(&agent_id, v)),
                cx,
            );
            picks.push(self.field("A message while it answers", busy));
            // Its backup program, which takes a turn over when its own runs out of usage
            // or doesn't start: none, or one of the other programs.
            let default_backup = if self.model.defaults.backup.is_empty() {
                "none".to_string()
            } else {
                self.model.defaults.backup.clone()
            };
            let mut programs = vec![("none".to_string(), "None".to_string())];
            for name in self.model.programs.iter().filter(|n| **n != a.program) {
                programs.push((name.clone(), program_title(name)));
            }
            let backup_name = if default_backup == "none" {
                "None".to_string()
            } else {
                program_title(&default_backup)
            };
            let agent_id = a.id.clone();
            let backups = self.choices(
                "edit-backup",
                programs,
                (!a.backup.is_empty()).then(|| a.backup.clone()),
                (default_backup, backup_name),
                Rc::new(move |this: &mut Loom, v: Option<&str>| {
                    this.model.set_backup(&agent_id, v)
                }),
                cx,
            );
            picks.push(self.field("Backup program", backups));
        }
        // Where it runs: in a sandbox or not, and what it reaches from there,
        // from everything, through a list of hosts, to nothing. A member's agents
        // always run in one; an instance without sandboxes can't isolate any.
        let (isolated, network) = self
            .model
            .editing
            .as_ref()
            .and_then(|id| self.model.agent(id))
            .map(|a| (a.isolated, a.network.clone()))
            .unwrap_or((false, None));
        let member = !self.model.programs_owner;
        let locked = member || (!self.model.sandboxes && !isolated);
        let isolate_how = if member {
            "In a sandbox of your own, as every member's agents are"
        } else if !self.model.sandboxes && !isolated {
            "Needs sandboxes, which this instance doesn't run. On Linux, start it with --sandboxes podman"
        } else {
            "Runs in a sandbox, apart from the instance's files and your other agents"
        };
        let isolate = if locked {
            self.row(
                None,
                "Isolated",
                Some(isolate_how.to_string()),
                Some(self.switch("edit-isolated", isolated).into_any_element()),
            )
            .into_any_element()
        } else {
            self.switch_row(
                "edit-isolated",
                "Isolated",
                isolate_how,
                isolated,
                cx,
                |this| {
                    let on = this
                        .model
                        .editing
                        .as_ref()
                        .and_then(|id| this.model.agent(id))
                        .is_some_and(|a| a.isolated);
                    this.listing_hosts = false;
                    if let Some(f) = this.model.set_isolated(!on) {
                        this.send(f);
                    }
                },
            )
        };
        let listing = match &network {
            None => false,
            Some(hosts) => self.listing_hosts || !hosts.is_empty(),
        };
        let mut reach = div().flex().flex_wrap().gap(px(space::S));
        let choices: [(&'static str, &'static str, bool); 3] = [
            ("all", "Everything", network.is_none()),
            ("list", "Only these hosts", listing),
            (
                "none",
                "Nothing",
                network.as_ref().is_some_and(|h| h.is_empty()) && !listing,
            ),
        ];
        for (id, label, on) in choices {
            reach = reach.child(
                self.button_owned(format!("edit-network-{id}"), label, on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let hosts = this
                            .model
                            .editing
                            .as_ref()
                            .and_then(|a| this.model.agent(a))
                            .and_then(|a| a.network.clone());
                        this.listing_hosts = id == "list";
                        let wanted = match id {
                            "all" => None,
                            "list" => Some(hosts.unwrap_or_default()),
                            _ => Some(Vec::new()),
                        };
                        if let Some(f) = this.model.set_network(wanted) {
                            this.send(f);
                        }
                        cx.notify();
                    })),
            );
        }
        let mut hosts_list = div().flex().flex_col().gap(px(space::S));
        if listing {
            let mut chips = div().flex().flex_wrap().gap(px(space::S));
            for h in network.clone().unwrap_or_default() {
                let host = h.clone();
                chips = chips.child(
                    self.button_owned(format!("edit-host-{h}"), &format!("{h}  ×"), false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(f) = this.model.remove_host(&host) {
                                this.send(f);
                            }
                            cx.notify();
                        })),
                );
            }
            hosts_list = hosts_list
                .child(chips)
                .child(
                    div()
                        .flex()
                        .gap(px(space::S))
                        .child(div().flex_1().child(Input::new(&self.host_in)))
                        .child(
                            self.button("edit-host-add", "Add", false)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    let typed = this.host_in.read(cx).value().to_string();
                                    if let Some(f) = this.model.add_host(&typed) {
                                        this.send(f);
                                    }
                                    this.host_in.update(cx, |s, cx| s.set_value("", window, cx));
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(
                            "A name, or *.example.com for every name under one. It reaches \
                             nothing else, and never the instance's own computer.",
                        ),
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
        let activity = self.stat_row(
            "edit-activity",
            "Activity",
            String::new(),
            true,
            cx.listener(|this, _, _, cx| {
                if let Some(f) = this.model.load_activity() {
                    this.send(f);
                }
                this.show_side(Screen::Activity, cx);
                cx.notify();
            }),
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
        // The open chat: the brain it runs on, to move it to another, which starts with
        // loom's summary of it, and Start fresh, which asks for a second click.
        let on = self.model.chat_program().unwrap_or_default();
        let mut brains = div().flex().flex_wrap().gap(px(space::XS + 2.0));
        for name in self.model.programs.clone() {
            let program = name.clone();
            brains = brains.child(
                self.chip(
                    format!("edit-move-{name}"),
                    &program_title(&name),
                    name == on,
                    false,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(f) = this.model.move_chat(&program) {
                        this.send(f);
                    }
                    cx.notify();
                })),
            );
        }
        let fresh = self
            .button_owned(
                "edit-fresh".to_string(),
                if self.fresh_armed {
                    "Click again to start fresh"
                } else {
                    "Start fresh"
                },
                false,
            )
            .when(self.fresh_armed, |d| {
                d.border_color(p.bad).text_color(p.bad)
            })
            .on_click(cx.listener(|this, _, _, cx| {
                if !this.fresh_armed {
                    this.fresh_armed = true;
                    cx.notify();
                    return;
                }
                this.fresh_armed = false;
                if let Some(f) = this.model.start_fresh() {
                    this.send(f);
                }
                this.close_side(cx);
                cx.notify();
            }));
        let group = |title: &'static str| {
            div()
                .mt(px(space::M))
                .pt(px(space::L))
                .border_t_1()
                .border_color(p.line)
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink)
                .child(title)
        };
        // The counted rows and the switches reach out past the column by their own
        // padding, so their words line up with the labels and their hover has room.
        let rows =
            |items: Vec<Stateful<Div>>| div().mx(px(-space::S)).flex().flex_col().children(items);
        let save = self
            .button("save-agent", "Save", true)
            .on_click(cx.listener(|this, _, _, cx| {
                let name = this.name_in.read(cx).value().to_string();
                let instructions = this.instructions_in.read(cx).value().to_string();
                if name.trim().is_empty() {
                    return;
                }
                if let Some(f) = this.model.save_agent(&name, &instructions) {
                    this.send(f);
                }
                this.close_side(cx);
                cx.notify();
            }));
        let cancel = self
            .button("cancel-edit", "Cancel", false)
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_side(cx);
                cx.notify();
            }));
        let page = div()
            .id("edit-agent")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(space::L))
            .px(px(space::XL))
            .pb(px(space::XXL))
            .child(
                div()
                    .w_full()
                    .max_w(px(layout::PAGE))
                    .flex()
                    .flex_col()
                    .gap(px(space::L))
                    // Name and instructions are what Save keeps; everything under them
                    // changes as it's picked.
                    .child(self.field("Name", Input::new(&self.name_in)))
                    .child(self.field("Instructions", Textarea::new(&self.instructions_in)))
                    .child(div().flex().gap(px(space::S)).child(save).child(cancel))
                    .when(!picks.is_empty(), |d| {
                        d.child(group("Brain")).children(picks)
                    })
                    .child(group("Knows"))
                    .child(rows(vec![memories, skills, tables, files]))
                    .child(self.field("MCP servers", servers))
                    .child(div().mx(px(-space::L)).child(scripts))
                    .child(group("Does"))
                    .child(rows(vec![activity, routines, usage]))
                    .child(group("Runs"))
                    .child(div().mx(px(-space::L)).child(isolate))
                    .when(isolated, |d| {
                        d.child(self.field("Network", reach))
                            .when(listing, |d| d.child(hosts_list))
                    })
                    .when(!self.model.revisions.is_empty(), |d| {
                        d.child(self.field("Versions", list))
                    })
                    .child(group("This chat"))
                    .when(!self.model.programs.is_empty(), |d| {
                        d.child(self.field("Runs on", brains))
                    })
                    .child(div().flex().child(fresh)),
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
        row.id(id)
            .cursor_pointer()
            .soft_hover(transparent_black(), p.hover, None)
    }

    /// Signs in to the person's MCP server `name` (PLAN.md, "Signing in to MCP servers"):
    /// a listener on loopback waits for the browser, the instance gives the server's
    /// sign-in page, which opens in the person's own browser, and what the browser brings
    /// back goes to the instance, which swaps it for tokens. The same server's core gets
    /// it, whichever is on screen by then.
    fn sign_in_mcp(&mut self, name: &str, cx: &mut Context<Self>) {
        let catcher = match crate::signin::listen() {
            Ok(c) => c,
            Err(e) => {
                self.model
                    .notices
                    .push(format!("Couldn't wait for the sign-in: {e}"));
                cx.notify();
                return;
            }
        };
        let f = self.model.sign_in_mcp(name, &catcher.redirect);
        self.send(f);
        // A sign-in card says Signing in… meanwhile.
        self.rows_dirty = true;
        let dir = self.here.clone();
        let back = catcher.back;
        cx.spawn(async move |this, cx| loop {
            match back.try_recv() {
                Ok(query) => {
                    let _ = this.update(cx, |loom, cx| {
                        loom.send_to(&dir, &model::signed_in_frame(&query));
                        cx.notify();
                    });
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Ten minutes and the browser didn't come back.
                    let _ = this.update(cx, |loom, cx| {
                        loom.model.mcp_signing.clear();
                        loom.rows_dirty = true;
                        cx.notify();
                    });
                    return;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Sign in, for an MCP server that needs it: "Signing in…" while the browser is away.
    fn sign_in_button(&self, name: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let signing = self.model.mcp_signing.contains(name);
        let server = name.to_string();
        self.small_button(
            format!("sign-in-mcp-{name}"),
            if signing { "Signing in…" } else { "Sign in" },
            true,
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.sign_in_mcp(&server, cx);
        }))
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
                    let client = this.mcp_client_in.read(cx).value().to_string();
                    let secret = this.mcp_client_secret_in.read(cx).value().to_string();
                    let f = this.model.put_mcp(crate::model::with_client(
                        crate::model::server_args(&name, &line, &settings),
                        &client,
                        &secret,
                    ));
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
            let mut fields = vec![
                self.field_row("Name", Input::new(&self.mcp_name_in)),
                self.field_row("Command or URL", Input::new(&self.mcp_cmd_in)),
                self.text_row(
                    "Settings",
                    Some("NAME=value, one a line. NAME=secret:KEY uses one of your keys"),
                    Textarea::new(&self.mcp_settings_in),
                ),
            ];
            if self.mcp_cmd_in.read(cx).value().trim().starts_with("http") {
                fields.push(self.field_row("Client ID", Input::new(&self.mcp_client_in)));
                fields
                    .push(self.field_row("Client secret", Input::new(&self.mcp_client_secret_in)));
            }
            let groups = vec![
                self.group(None, fields),
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
                    for s in [
                        &this.mcp_name_in,
                        &this.mcp_cmd_in,
                        &this.mcp_client_in,
                        &this.mcp_client_secret_in,
                    ] {
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
                // Its last test, in place of what it runs, once there's been one, or
                // whom it's signed in as.
                let tested = self.model.mcp_tests.get(&m.name).cloned();
                let signed = match &m.sign_in {
                    SignIn::SignedIn(who) if who.is_empty() => Some("Signed in".to_string()),
                    SignIn::SignedIn(who) => Some(format!("Signed in as {who}")),
                    _ => None,
                };
                let sub = tested.as_ref().map(|t| t.said()).or(signed).unwrap_or(what);
                let testing = tested == Some(crate::model::McpTest::Testing);
                let test_name = m.name.clone();
                let test = self
                    .small_button(
                        format!("test-mcp-{}", m.name),
                        if testing { "Testing…" } else { "Test" },
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if this.model.mcp_tests.get(&test_name)
                            == Some(&crate::model::McpTest::Testing)
                        {
                            return;
                        }
                        let f = this.model.test_mcp(&test_name);
                        this.send(f);
                        cx.notify();
                    }));
                // Sign in, for a server that needs it, in place of the error; Sign out
                // once signed in.
                let needs = m.sign_in == SignIn::Needed
                    || tested == Some(crate::model::McpTest::NeedsSignIn);
                let account = match &m.sign_in {
                    SignIn::SignedIn(_) => {
                        let out_name = m.name.clone();
                        Some(
                            self.small_button(
                                format!("sign-out-mcp-{}", m.name),
                                "Sign out",
                                false,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let f = this.model.sign_out_mcp(&out_name);
                                    this.send(f);
                                    cx.notify();
                                },
                            )),
                        )
                    }
                    _ if needs => Some(self.sign_in_button(&m.name, cx)),
                    _ => None,
                };
                let actions = div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .children(account)
                    .child(test);
                let sub = if needs && tested.is_none() {
                    "Needs you to sign in".to_string()
                } else {
                    sub
                };
                let row = self.row(
                    Some(self.tile(Ico::Plug)),
                    m.name.clone(),
                    Some(sub),
                    Some(actions.into_any_element()),
                );
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
                            let (client, secret) = (
                                m.client_id.clone(),
                                m.client_secret.clone().unwrap_or_default(),
                            );
                            this.mcp_client_in
                                .update(cx, |s, cx| s.set_value(client, window, cx));
                            this.mcp_client_secret_in
                                .update(cx, |s, cx| s.set_value(secret, window, cx));
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
        if self.model.skills.is_empty() {
            let action = self.new_skill_button(cx);
            let groups = vec![self.empty(
                Ico::BookOpen,
                "No skills yet",
                "How you like something done, which every agent reads when a job fits it",
                Some(action),
            )];
            return self.settings_page("skills-page", "Skills", None, groups);
        }
        // Its title over the skills, each a row of the one group, as a list.
        let n = self.model.skills.len();
        self.list_page(
            "skills-page",
            n + 1,
            layout::SETTINGS,
            |this, ix, cx| {
                if ix == 0 {
                    let action = Some(this.new_skill_button(cx));
                    return this.list_head("Skills", action);
                }
                let n = this.model.skills.len();
                match this.model.skills.get(ix - 1).cloned() {
                    Some(k) => {
                        let row = this.skill_row(k, cx);
                        this.group_item(ix - 1, n, row)
                    }
                    None => div().into_any_element(),
                }
            },
            cx,
        )
    }

    /// The skills page's New skill, the page's own button while it has none.
    fn new_skill_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.small_button("new-skill", "New skill", self.model.skills.is_empty())
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
    }

    /// One skill, as a row of the skills page, which opens it to change.
    fn skill_row(&self, k: model::Skill, cx: &mut Context<Self>) -> AnyElement {
        let row = self.row(
            Some(self.tile(Ico::BookOpen)),
            k.name.clone(),
            Some(k.description.clone()),
            None,
        );
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
            .into_any_element()
    }

    /// What agents propose from past chats, and skills written from a task the person
    /// recorded in an agent's browser: each kept or dropped here, and nothing applied
    /// without the person's yes. One opens to be read, and changed, before it's kept.
    fn proposals_page(&mut self, cx: &mut Context<Self>) -> Div {
        if let Some(open) = self.proposal_open.clone() {
            let skill = open.kind == "skill";
            let keep = self
                .small_button("keep-proposal", "Keep", true)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let Some(mut p) = this.proposal_open.take() else {
                        return;
                    };
                    if p.kind == "skill" {
                        p.name = this.skill_name_in.read(cx).value().to_string();
                        p.description = this.skill_desc_in.read(cx).value().to_string();
                        p.body = this.skill_body_in.read(cx).value().to_string();
                        if p.name.trim().is_empty() || p.description.trim().is_empty() {
                            this.model
                                .notices
                                .push("Needs a name, and when it's for".into());
                            this.proposal_open = Some(p);
                            cx.notify();
                            return;
                        }
                    } else {
                        p.text = this.note_in.read(cx).value().to_string();
                    }
                    let f = this.model.keep_proposal(&p);
                    this.send(f);
                    cx.notify();
                }));
            let cancel = self
                .small_button("cancel-proposal", "Cancel", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.proposal_open = None;
                    cx.notify();
                }));
            let id = open.id.clone();
            let drop = self
                .small_button("drop-proposal", "Drop", false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let f = this.model.drop_proposal(&id);
                    this.send(f);
                    this.proposal_open = None;
                    cx.notify();
                }));
            let p = self.p;
            let from = format!(
                "From {}{}",
                self.who(&open.agent),
                if open.why.is_empty() {
                    String::new()
                } else {
                    format!(": {}", open.why)
                }
            );
            let mut groups = vec![self.group(None, vec![self.note_row(from, p.ink_2)])];
            if skill {
                groups.push(self.group(
                    None,
                    vec![
                        self.field_row("Name", Input::new(&self.skill_name_in)),
                        self.field_row("When it's for", Input::new(&self.skill_desc_in)),
                        self.text_row("How to do it", None, Textarea::new(&self.skill_body_in)),
                    ],
                ));
                if !open.steps.is_empty() {
                    let steps = open
                        .steps
                        .iter()
                        .enumerate()
                        .map(|(k, line)| self.note_row(format!("{}. {line}", k + 1), p.ink_2))
                        .collect();
                    groups.push(self.group(Some("What you did"), steps));
                }
            } else {
                let mut rows = vec![self.field_row("Note", Input::new(&self.note_in))];
                if !open.site.is_empty() || open.shared {
                    let mut how = Vec::new();
                    if !open.site.is_empty() {
                        how.push(format!("Shown on {}", open.site));
                    }
                    if open.shared {
                        how.push("every agent reads it".to_string());
                    }
                    rows.push(self.note_row(how.join(", "), p.ink_3));
                }
                groups.push(self.group(None, rows));
            }
            groups.push(self.form_actions(Some(drop), cancel, keep));
            let title = if skill {
                "Proposed skill"
            } else {
                "Proposed note"
            };
            return self.settings_page("proposals-page", title, None, groups);
        }
        // Asking the agent on screen to look over its chats for more.
        let ask = self.model.selected.clone().map(|agent| {
            let name = self.who(&agent);
            self.small_button("ask-proposals", &format!("Ask {name}"), false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let f = this.model.ask_proposals(&agent);
                    this.send(f);
                    this.model.said.push(format!(
                        "{name} is looking over your chats. What it proposes comes here."
                    ));
                    cx.notify();
                }))
                .into_any_element()
        });
        if self.model.proposals.is_empty() {
            let groups = vec![self.empty(
                Ico::Lightbulb,
                "Nothing to look at",
                "Skills and notes your agents propose from your chats, and tasks you teach them in their browser, wait here for your yes",
                ask,
            )];
            return self.settings_page("proposals-page", "Proposals", None, groups);
        }
        let mut rows = Vec::new();
        for k in self.model.proposals.clone() {
            let kind = if k.kind == "skill" { "Skill" } else { "Note" };
            let sub = format!("{kind} from {}", self.who(&k.agent));
            let (keep_id, drop_id) = (k.clone(), k.id.clone());
            let controls = div()
                .flex()
                .gap(px(space::S))
                .child(
                    self.small_button(SharedString::from(format!("drop-{}", k.id)), "Drop", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            let f = this.model.drop_proposal(&drop_id);
                            this.send(f);
                            cx.notify();
                        })),
                )
                .child(
                    self.small_button(SharedString::from(format!("keep-{}", k.id)), "Keep", true)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            let f = this.model.keep_proposal(&keep_id);
                            this.send(f);
                            cx.notify();
                        })),
                )
                .into_any_element();
            let icon = if k.kind == "skill" {
                Ico::BookOpen
            } else {
                Ico::Lightbulb
            };
            let row = self.row(Some(self.tile(icon)), k.title(), Some(sub), Some(controls));
            rows.push(
                self.link_row(SharedString::from(format!("proposal-{}", k.id)), row)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if k.kind == "skill" {
                            let (name, description, body) =
                                (k.name.clone(), k.description.clone(), k.body.clone());
                            this.skill_name_in
                                .update(cx, |s, cx| s.set_value(name, window, cx));
                            this.skill_desc_in
                                .update(cx, |s, cx| s.set_value(description, window, cx));
                            this.skill_body_in
                                .update(cx, |s, cx| s.set_value(body, window, cx));
                        } else {
                            let text = k.text.clone();
                            this.note_in
                                .update(cx, |s, cx| s.set_value(text, window, cx));
                        }
                        this.proposal_open = Some(k.clone());
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        self.settings_page(
            "proposals-page",
            "Proposals",
            ask,
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
                .size(px(control::ICON))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .text_color(p.ink_2)
                .cursor_pointer()
                .soft_hover(transparent_black(), p.hover, Some((p.ink_2, p.ink)))
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
                                .font_family(crate::prefs::mono_font())
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

    /// The Import page: OpenBot and Grok Bot, each with what the instance's computer has of
    /// it and a button to bring it over, then how the last import went.
    fn import_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let here = if self.local() {
            "this computer"
        } else {
            "the server's computer"
        };
        let mut sources = self.model.import_sources.clone();
        if sources.is_empty() {
            for (from, name) in [("openbot", "OpenBot"), ("grokbot", "Grok Bot")] {
                sources.push(ImportSource {
                    from: from.into(),
                    name: name.into(),
                    ..ImportSource::default()
                });
            }
        }
        let mut rows = Vec::new();
        for src in sources {
            let busy = self.model.importing.is_some();
            let sub = if self.model.import_loading && !src.found {
                "Looking…".to_string()
            } else if src.found {
                src.text.clone()
            } else {
                format!("Not found on {here}")
            };
            let control = src.found.then(|| {
                let from = src.from.clone();
                let label = if self.model.importing.as_deref() == Some(src.from.as_str()) {
                    "Importing…"
                } else {
                    "Import"
                };
                self.small_button(format!("import-{}", src.from), label, false)
                    .when(!busy, |b| {
                        b.on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.run_import(&from);
                            this.send(f);
                            cx.notify();
                        }))
                    })
                    .into_any_element()
            });
            rows.push(
                self.row(
                    Some(self.tile(Ico::Import)),
                    src.name.clone(),
                    Some(sub),
                    control,
                )
                .into_any_element(),
            );
        }
        let mut groups = vec![self.group(Some("From another app"), rows)];
        groups.push(
            div()
                .px(px(space::XS))
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child("Agents, chats, memories, routines, skills and tables come over. Keys and logins don't.")
                .into_any_element(),
        );
        if let Some(done) = self.model.import_done.clone() {
            let mut lines =
                vec![self.note_row(done.text.clone(), if done.error { p.bad } else { p.ink })];
            for n in &done.not {
                lines.push(self.note_row(n.clone(), p.ink_3));
            }
            groups.push(self.group(Some("Last import"), lines));
        }
        self.settings_page("import-page", "Import", None, groups)
    }

    /// The plugins page: each plugin, what it asks for that waits for the person's yes, and
    /// its panel to open. An open panel lies over the content area in a web view of its own.
    fn plugins_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let ask = |this: &Self, cx: &mut Context<Self>, primary: bool| {
            this.small_button("plugin-prompt", "Copy prompt", primary)
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(PLUGIN_PROMPT.to_string()));
                    this.notice = Some((
                        "Copied. Paste it to one of your agents.".into(),
                        Instant::now(),
                        true,
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
        let side = self.side.unwrap_or(Screen::Plugins);
        let w = side_width(
            side == Screen::Plugins || side == Screen::Page,
            f32::from(size.width),
            self.bar_width(f32::from(size.width)),
            self.side_w,
        );
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
        // On a Mac, only a private WebKit setting stops a clicked link connecting to an
        // address on this computer or its network (panel.rs, `no_hosts`). A macOS
        // without it opens no panels or pages.
        if !crate::panel::can_open() {
            self.model
                .notices
                .push("Panels and pages don't open on this version of macOS.".into());
            if key.starts_with("page:") {
                self.model.page = None;
            } else {
                self.model.close_panel();
            }
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
                move |m| {
                    door.borrow_mut().push(m);
                    crate::wake::wake();
                },
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
        _ = self.model.load_routine_runs(None);
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
        if self.model.routines.is_empty() {
            let groups = vec![self.empty(
                Ico::Clock,
                "No routines yet",
                "Something an agent does on a schedule, such as a summary each morning or a check every hour",
                Some(self.new_routine_button(cx)),
            )];
            return self.settings_page("routines-page", "Routines", None, groups);
        }
        // Its title over the routines, each a row of the one group, as a list.
        let n = self.model.routines.len();
        self.list_page(
            "routines-page",
            n + 1,
            layout::SETTINGS,
            |this, ix, cx| {
                if ix == 0 {
                    let action = Some(this.new_routine_button(cx));
                    return this.list_head("Routines", action);
                }
                let n = this.model.routines.len();
                match this.model.routines.get(ix - 1).cloned() {
                    Some(r) => {
                        let row = this.routine_row(r, cx);
                        this.group_item(ix - 1, n, row)
                    }
                    None => div().into_any_element(),
                }
            },
            cx,
        )
    }

    /// One routine, as a row of the routines page: when it runs next and how its last
    /// run went, or its flow's steps while it runs, which opens it to change.
    fn routine_row(&self, r: model::Routine, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
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
            "waiting" => " · waiting for the chat".into(),
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
            .size(px(control::LEAD))
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
        self.row(
            Some(lead),
            r.name.clone(),
            Some(sub),
            Some(controls.into_any_element()),
        )
        .id(SharedString::from(format!("routine-{}", r.id)))
        .cursor_pointer()
        .soft_hover(transparent_black(), p.hover, None)
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
            if let Some(f) = this.model.load_routine_runs(Some(&r.id)) {
                this.send(f);
            }
            this.routine_open = Some(Some(r.id.clone()));
            cx.notify();
        }))
        .into_any_element()
    }

    /// The routines page's New routine, the page's own button while it has none.
    fn new_routine_button(&self, cx: &mut Context<Self>) -> AnyElement {
        self.small_button("new-routine", "New routine", self.model.routines.is_empty())
            .on_click(cx.listener(|this, _, window, cx| this.new_routine(window, cx)))
            .into_any_element()
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
        // Its runs, which leave nothing in the chat but what they post: how each ended,
        // and what its agent posted or answered.
        if open.is_some() && !self.model.routine_runs.is_empty() {
            let now = chrono::Local::now();
            let runs = self
                .model
                .routine_runs
                .iter()
                .map(|r| {
                    let words = r.words();
                    let when = div()
                        .text_size(px(size::CAPTION))
                        .text_color(p.ink_3)
                        .child(when_sent(r.at, now))
                        .into_any_element();
                    self.row(
                        None,
                        r.outcome(),
                        (!words.is_empty()).then_some(words),
                        Some(when),
                    )
                    .into_any_element()
                })
                .collect();
            groups.push(self.group(Some("Runs"), runs));
        }
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
            let agents = self.model.agents.clone();
            for a in agents.iter() {
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

    /// The Hosts page: the person's own servers, which this computer reaches with ssh,
    /// each with what agents may do there, and the ones found in the ssh config and on
    /// the tailnet, to add. The core keeps them and runs the commands.
    fn hosts_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut rows = Vec::new();
        for h in self.model.hosts.clone() {
            let mut choices = Vec::new();
            for (choice, label) in [("off", "Off"), ("ask", "Ask"), ("allow", "Allow")] {
                let name = h.name.clone();
                choices.push(
                    self.segment(format!("host-{}-{choice}", h.name), label, h.mode == choice)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.set_ssh_host(&name, choice);
                            this.send(f);
                            cx.notify();
                        })),
                );
            }
            let name = h.name.clone();
            let remove = self
                .small_button(format!("host-{}-remove", h.name), "Remove", false)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let f = this.model.remove_ssh_host(&name);
                    this.send(f);
                    cx.notify();
                }));
            let via = if h.via == "tailscale" {
                "Tailscale"
            } else {
                "SSH"
            };
            rows.push(
                self.row(
                    Some(self.tile(Ico::Server)),
                    h.name.clone(),
                    Some(format!("{} · {via}", h.target)),
                    Some(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .child(self.segments(choices))
                            .child(remove)
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
            if h.mode == "allow" {
                rows.push(self.note_row(
                    format!("Agents can run any command on {} without asking", h.name),
                    p.bad,
                ));
            }
        }
        let add = div()
            .flex()
            .gap(px(space::S))
            .child(div().flex_1().child(Input::new(&self.ssh_host_in)))
            .child(self.button("host-add", "Add", false).on_click(cx.listener(
                |this, _, window, cx| {
                    let typed = this.ssh_host_in.read(cx).value().to_string();
                    if let Some(f) = this.model.add_ssh_host(&typed, "ssh") {
                        this.send(f);
                    }
                    this.ssh_host_in
                        .update(cx, |s, cx| s.set_value("", window, cx));
                    cx.notify();
                },
            )));
        rows.push(
            div()
                .px(px(space::L))
                .py(px(space::S + 2.0))
                .child(add)
                .into_any_element(),
        );
        let mut groups = vec![self.group(Some("Hosts"), rows)];
        if !self.model.hosts_found.is_empty() {
            let mut found = Vec::new();
            for h in self.model.hosts_found.clone() {
                let (target, via) = (h.target.clone(), h.via.clone());
                let from = if h.via == "tailscale" {
                    "On your tailnet"
                } else {
                    "In your ssh config"
                };
                found.push(
                    self.row(
                        Some(self.tile(Ico::Server)),
                        h.name.clone(),
                        Some(from.into()),
                        Some(
                            self.small_button(format!("found-{}", h.name), "Add", false)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(f) = this.model.add_ssh_host(&target, &via) {
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
            groups.push(self.group(Some("Found"), found));
        }
        self.settings_page("hosts-page", "Hosts", None, groups)
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
                    .soft_ink(p.ink_3, p.ink)
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
        let mut groups = vec![
            self.group(None, rows),
            actions.into_any_element(),
            self.agent_group("add-server-prompt", ADD_SERVER_PROMPT, cx),
        ];
        // While the instance on this computer is out of the app, it can come back, with
        // the agents and chats it had.
        if self.first_out() {
            let start = self
                .small_button("add-server-local", "Run here", false)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.popup = None;
                    this.set_local(true, cx);
                }));
            groups.push(self.group(
                None,
                vec![self
                    .row(
                        Some(self.tile(Ico::Monitor)),
                        "This computer",
                        Some("Run loom here, with the agents it had".into()),
                        Some(start.into_any_element()),
                    )
                    .into_any_element()],
            ));
        }
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

    /// A link to pass on, as a row: the link, how long it works, and Copy.
    fn copy_row(&self, id: &'static str, link: String, sub: String) -> AnyElement {
        let copied = link.clone();
        self.row(
            Some(self.tile(Ico::Copy)),
            link,
            Some(sub),
            Some(
                self.small_button(id, "Copy", true)
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()));
                    })
                    .into_any_element(),
            ),
        )
        .into_any_element()
    }

    /// The People page, for owners and admins of an instance on a server: invite someone,
    /// with a link to pass on, and see who's in, to make them an admin or remove them.
    fn people_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        if let Some(why) = self.model.people_refused.clone() {
            let groups = vec![self.empty(Ico::Users, "Not for you", &why, None)];
            return self.settings_page("people-page", "People", None, groups);
        }
        let mut groups = Vec::new();
        // Invite someone: the role it gives, and the link to pass on once it's made.
        let admin = self.invite_admin;
        let role = self.segments(vec![
            self.segment("invite-member", "Member", !admin)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invite_admin = false;
                    cx.notify();
                })),
            self.segment("invite-admin", "Admin", admin)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invite_admin = true;
                    cx.notify();
                })),
        ]);
        let mut invite = vec![self
            .row(
                Some(self.tile(Ico::Users)),
                "Invite someone",
                Some("Works once, for a week".into()),
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(role)
                        .child(
                            self.small_button("make-invite", "Make an invite", true)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let role = if this.invite_admin { "admin" } else { "member" };
                                    let f = this.model.create_invite(role, 1);
                                    this.send(f);
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                ),
            )
            .into_any_element()];
        if let Some((link, role, expires)) = self.model.invite_made.clone() {
            invite.push(self.copy_row(
                "copy-invite",
                link,
                format!(
                    "One {role}, until {}. Send it as you'd send a password",
                    moment_of(expires)
                ),
            ));
        }
        for inv in self.model.invites.clone() {
            let id = inv.id.clone();
            invite.push(
                self.row(
                    None,
                    format!("An open invite for a {}", inv.role),
                    Some(format!(
                        "{} {} left, until {}",
                        inv.uses,
                        if inv.uses == 1 { "use" } else { "uses" },
                        moment_of(inv.expires)
                    )),
                    Some(
                        self.small_button(format!("invite-{}", inv.id), "Delete", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.delete_invite(&id);
                                this.send(f);
                                cx.notify();
                            }))
                            .into_any_element(),
                    ),
                )
                .into_any_element(),
            );
        }
        groups.push(self.group(None, invite));
        // Who's in.
        let mut everyone = Vec::new();
        for person in self.model.people.clone() {
            let initial = person
                .username
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            let lead = div()
                .size(px(control::LEAD))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::ROW))
                .bg(p.raised_2)
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.ink_2)
                .child(initial)
                .into_any_element();
            let mut controls = div().flex().gap(px(space::XS));
            if person.role != "owner" {
                let (user, to, label) = if person.role == "admin" {
                    (person.id.clone(), "member", "Make member")
                } else {
                    (person.id.clone(), "admin", "Make admin")
                };
                let gone = person.id.clone();
                let (reset_id, reset_name) = (person.id.clone(), person.username.clone());
                controls = controls
                    .child(
                        self.small_button(format!("reset-{}", person.id), "Reset password", false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.reset_password(&reset_id, &reset_name);
                                this.send(f);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.small_button(format!("role-{}", person.id), label, false)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.set_role(&user, to);
                                this.send(f);
                                cx.notify();
                            })),
                    )
                    .child(
                        self.small_button(format!("remove-{}", person.id), "Remove", false)
                            .text_color(p.bad)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.remove_person(&gone);
                                this.send(f);
                                cx.notify();
                            })),
                    );
            }
            everyone.push(
                self.row(
                    Some(lead),
                    person.username.clone(),
                    Some(person.role.clone()),
                    Some(controls.into_any_element()),
                )
                .into_any_element(),
            );
        }
        if let Some((who, link, expires)) = self.model.reset_made.clone() {
            everyone.push(self.copy_row(
                "copy-reset",
                link,
                format!(
                    "A new password for {who}, once, until {}",
                    moment_of(expires)
                ),
            ));
        }
        if !everyone.is_empty() {
            groups.push(self.group(Some("Everyone"), everyone));
        }
        // What happened to people here, newest first.
        let mut log = Vec::new();
        for e in self.model.people_log.clone() {
            let who = if e.by.is_empty() {
                "from the server".to_string()
            } else {
                format!("by {}", e.by)
            };
            log.push(
                self.row(
                    None,
                    e.text.clone(),
                    Some(format!("{}, {who}", moment_of(e.at))),
                    None,
                )
                .into_any_element(),
            );
        }
        if !log.is_empty() {
            groups.push(self.group(Some("What happened"), log));
        }
        self.settings_page("people-page", "People", None, groups)
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
                .soft_hover(
                    if close {
                        p.bad.opacity(0.0)
                    } else {
                        transparent_black()
                    },
                    if close { p.bad } else { p.hover },
                    Some((p.ink_3, if close { gpui::white() } else { p.ink })),
                )
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

    /// Catches the mouse wheel's notches before anything scrolls, and plays each out in
    /// pixels over the next frames, so every list and page scrolls smoothly
    /// (crate::smooth). It draws nothing.
    fn wheel_catcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let this = this.clone();
                window.on_mouse_event(move |e: &ScrollEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let line = window.line_height().as_f32();
                    let _ = this.update(cx, |this, cx| {
                        if !this.smooth.catch(e, line) {
                            return;
                        }
                        cx.stop_propagation();
                        if this.smooth.running {
                            return;
                        }
                        this.smooth.running = true;
                        // One step a frame the window shows, not on a timer of its own, so
                        // each frame moves as far as the time it took says.
                        let weak = cx.entity().downgrade();
                        let first = Instant::now() - crate::smooth::FRAME;
                        window
                            .on_next_frame(move |window, cx| smooth_step(weak, first, window, cx));
                    });
                });
            },
        )
        .absolute()
        .size_0()
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
            .max_w(px(layout::SETTINGS))
            .flex()
            .flex_col()
            .gap(px(space::XL))
            .child(
                div()
                    .min_h(px(control::BUTTON))
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

    /// A page whose rows are drawn as a list, `count` of them, by `item`: only the rows
    /// on screen are built each frame, however many there are. It reads as a page
    /// laid out as `settings_page` lays one out, its column `width` wide at most, and
    /// opens at its top.
    fn list_page(
        &mut self,
        name: &'static str,
        count: usize,
        width: f32,
        item: fn(&mut Loom, usize, &mut Context<Loom>) -> AnyElement,
        cx: &mut Context<Self>,
    ) -> Div {
        let frames = self.frames;
        // Every row is measured once as the page opens, so it scrolls as far and as fast as
        // a page laid out whole: a list that knew only the rows near the screen would stop
        // a quick scroll at the last of them.
        let page = self.lists.entry(name).or_insert_with(|| PageList {
            state: ListState::new(0, ListAlignment::Top, px(PAGE_OVERDRAW)).measure_all(),
            count: 0,
            drawn: 0,
        });
        // Just opened: at its top, as a page that's made anew is.
        if page.drawn + 1 != frames {
            page.state.reset(count);
            page.count = count;
        } else if page.count != count {
            // Rows came or went: the list keeps its place.
            let common = page.count.min(count);
            page.state.splice(common..page.count, count - common);
            page.count = count;
        }
        page.drawn = frames;
        let state = page.state.clone();
        let this = cx.entity().downgrade();
        // The page's padding rides on its rows: a list measures how far it scrolls from
        // its rows alone.
        let rows = list(state, move |ix, _, cx| {
            this.update(cx, |loom, cx| {
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .px(px(space::XL))
                    .when(ix == 0, |d| d.pt(px(space::XL)))
                    .when(ix + 1 == count, |d| d.pb(px(space::XXL)))
                    .child(div().w_full().max_w(px(width)).child(item(loom, ix, cx)))
                    .into_any_element()
            })
            .unwrap_or_else(|_| div().into_any_element())
        })
        .size_full();
        div().flex_1().min_h_0().child(rows)
    }

    /// A list page's head: its title, and its action beside it, as `settings_page` has
    /// them over its groups.
    fn list_head(&self, title: &str, action: Option<AnyElement>) -> AnyElement {
        let p = self.p;
        div()
            .pb(px(space::XL))
            .child(
                div()
                    .min_h(px(control::BUTTON))
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
            .into_any_element()
    }

    /// Row `i` of `n` in a group's box, on a list page: its piece of the box, as
    /// `group` draws the whole box with a hairline between each row.
    fn group_item(&self, i: usize, n: usize, row: AnyElement) -> AnyElement {
        let p = self.p;
        div()
            .flex()
            .flex_col()
            .border_x_1()
            .border_t_1()
            .border_color(p.line)
            .bg(self.card_fill())
            .overflow_hidden()
            .when(i == 0, |d| {
                d.rounded_tl(px(radius::CARD)).rounded_tr(px(radius::CARD))
            })
            .when(i + 1 == n, |d| {
                d.border_b_1()
                    .rounded_bl(px(radius::CARD))
                    .rounded_br(px(radius::CARD))
            })
            .child(row)
            .into_any_element()
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
            .size(px(control::LEAD))
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
            .min_h(px(control::ROW))
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
                    // Up to two lines, so a wide control beside it doesn't cut off
                    // what the row is for.
                    .when_some(sub, |d, s| {
                        d.child(
                            div()
                                .text_size(px(size::CAPTION))
                                .text_color(p.ink_3)
                                .line_clamp(2)
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
                    .w(px(layout::FIELD_LABEL))
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
            .h(px(control::BUTTON_SMALL))
            .px(px(space::M))
    }

    /// An on and off switch, whose knob slides across and whose colour fades as it
    /// changes.
    fn switch(&self, id: impl Into<SharedString>, on: bool) -> Stateful<Div> {
        let p = self.p;
        let id: SharedString = id.into();
        let k = motion::level(&id, on, motion::SLIDE);
        let off_hover = motion::mix(p.line_strong, p.ink_3, 0.35);
        div()
            .id(id.clone())
            .relative()
            .w(px(38.0))
            .h(px(22.0))
            .flex_none()
            .rounded(px(radius::ROW))
            .cursor_pointer()
            .soft_hover(
                motion::mix(p.line_strong, p.button, k),
                motion::mix(off_hover, p.button, k),
                None,
            )
            .child(
                div()
                    .absolute()
                    .top(px(3.0))
                    .left(px(3.0 + 16.0 * k))
                    .size(px(16.0))
                    .rounded(px(4.0))
                    .bg(motion::mix(p.bg, p.button_ink, k))
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
            .h(px(control::SEGMENT))
            .px(px(space::M))
            .flex()
            .items_center()
            .rounded(px(radius::ROW))
            .text_size(px(size::SMALL))
            .cursor_pointer()
            // The picked one stands up off the track: white on light, a lighter step on
            // dark, where the page's own black would sink into it.
            .when(on, |d| {
                d.bg(if p.dark { p.line_strong } else { p.bg })
                    .text_color(p.ink)
                    .font_weight(FontWeight::MEDIUM)
                    .shadow_sm()
            })
            .when(!on, |d| d.soft_ink(p.ink_2, p.ink))
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
                    true,
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

    /// A brain's icon: its ACP Registry icon, drawn in the app's ink, or the brain for one
    /// the registry doesn't have.
    fn brain_icon(&self, listed: Option<&model::Listed>, size: f32) -> AnyElement {
        let p = self.p;
        match listed.and_then(|l| l.icon.as_ref().map(|svg| (l.id.clone(), svg))) {
            Some((id, svg)) => {
                let path = crate::runtime_svg(&format!("brains/{id}.svg"), svg);
                svg_icon(path, size, p.ink)
            }
            None => Icon::new(Ico::Brain)
                .size(px(size))
                .text_color(p.ink_2)
                .into_any_element(),
        }
    }

    /// A brain's icon in a square of its own, as a row's.
    fn brain_tile(&self, listed: Option<&model::Listed>, size: f32) -> AnyElement {
        let p = self.p;
        div()
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(radius::ROW + 2.0))
            .border_1()
            .border_color(p.line)
            .bg(p.bg)
            .child(self.brain_icon(listed, size * 0.5))
            .into_any_element()
    }

    /// The Brains page: the agent programs on the instance, each an agent's brain, with
    /// their settings and sharing, and the ACP Registry's to install.
    fn programs_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let mut groups = Vec::new();
        let mut mine = Vec::new();
        for prog in self.model.installed.clone() {
            let open = self.program_open.as_deref() == Some(prog.name.as_str());
            let listed = self.model.program_listed(&prog.name).cloned();
            let from_registry = (!prog.source.is_empty())
                .then(|| self.model.registry.iter().find(|l| l.id == prog.source))
                .flatten()
                .cloned();
            let newer = from_registry
                .as_ref()
                .filter(|l| l.version != prog.version)
                .map(|l| l.version.clone());
            let shown = from_registry
                .as_ref()
                .or(listed.as_ref())
                .map_or_else(|| program_title(&prog.name), |l| l.name.clone());
            let settings_words = match (&prog.share, prog.settings.len()) {
                (Some(sh), _) => share_words(sh),
                (None, 0) => "No settings".to_string(),
                (None, 1) => "1 setting".to_string(),
                (None, n) => format!("{n} settings"),
            };
            let what = if prog.source.is_empty() {
                format!("Added on the instance · {settings_words}")
            } else {
                format!("Version {} · {settings_words}", prog.version)
            };
            let name = prog.name.clone();
            let chevron = div()
                .text_color(p.ink_3)
                .child(
                    Icon::new(if open {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .small(),
                )
                .into_any_element();
            let row = self.row(
                Some(self.brain_tile(from_registry.as_ref().or(listed.as_ref()), 34.0)),
                shown,
                Some(what),
                Some(chevron),
            );
            mine.push(
                self.link_row(SharedString::from(format!("program-{}", prog.name)), row)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.program_open = if this.program_open.as_deref() == Some(name.as_str()) {
                            None
                        } else {
                            Some(name.clone())
                        };
                        for s in [&this.set_key_in, &this.set_value_in] {
                            s.update(cx, |s, cx| s.set_value("", window, cx));
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if !open {
                continue;
            }
            // Its settings, which stay on the instance: only their names come here.
            for key in prog.settings.clone() {
                let (program, k) = (prog.name.clone(), key.clone());
                mine.push(
                    self.row(
                        Some(div().w(px(34.0)).flex_none().into_any_element()),
                        key.clone(),
                        Some("••••••••".into()),
                        Some(
                            self.small_button(
                                format!("unset-{}-{key}", prog.name),
                                "Remove",
                                false,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let f = this.model.set_setting(&program, &k, None);
                                this.send(f);
                                cx.notify();
                            }))
                            .into_any_element(),
                        ),
                    )
                    .into_any_element(),
                );
            }
            let program = prog.name.clone();
            mine.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pl(px(space::L + 34.0 + space::M))
                    .pr(px(space::L))
                    .py(px(space::M))
                    .child(div().flex_1().child(Input::new(&self.set_key_in)))
                    .child(div().flex_1().child(Input::new(&self.set_value_in)))
                    .child(
                        self.small_button(format!("set-{}", prog.name), "Add setting", false)
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
                    )
                    .into_any_element(),
            );
            // Sharing its settings with the people invited, a number of tokens each.
            let month = self.share_month;
            let program = prog.name.clone();
            let mut share = div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(div().w(px(96.0)).child(Input::new(&self.share_in).small()))
                .child(self.segments(vec![
                    self.segment(format!("share-day-{}", prog.name), "a day", !month)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.share_month = false;
                            cx.notify();
                        })),
                    self.segment(format!("share-month-{}", prog.name), "a month", month)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.share_month = true;
                            cx.notify();
                        })),
                ]))
                .child(
                    self.small_button(format!("share-{}", prog.name), "Share", false)
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
                share = share.child(
                    self.small_button(format!("unshare-{}", prog.name), "Stop", false)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let f = this.model.share_program(&program, 0, "day");
                            this.send(f);
                            cx.notify();
                        })),
                );
            }
            mine.push(
                self.row(
                    Some(div().w(px(34.0)).flex_none().into_any_element()),
                    "Share it",
                    Some("A budget for each".into()),
                    Some(share.into_any_element()),
                )
                .into_any_element(),
            );
            // Who has it, and who gets the settings shared: the owner's to say.
            let people: Vec<model::Person> = self
                .model
                .people
                .iter()
                .filter(|person| person.role != "owner")
                .cloned()
                .collect();
            if self.model.programs_owner && !people.is_empty() {
                let program = prog.name.clone();
                let chips = self.member_chips(
                    &format!("has-{}", prog.name),
                    prog.members.clone(),
                    &people,
                    cx,
                    move |this, members| {
                        let f = this.model.program_members(&program, members);
                        this.send(f);
                    },
                );
                mine.push(
                    self.row(
                        Some(div().w(px(34.0)).flex_none().into_any_element()),
                        "Who has it",
                        Some(who_words(prog.members.as_deref(), &people)),
                        Some(chips.into_any_element()),
                    )
                    .into_any_element(),
                );
                if let Some(sh) = prog.share.clone() {
                    let program = prog.name.clone();
                    let given = sh.members.clone();
                    let chips = self.member_chips(
                        &format!("keys-{}", prog.name),
                        given.clone(),
                        &people,
                        cx,
                        move |this, members| {
                            let f = this.model.share_to(&program, &sh, members);
                            this.send(f);
                        },
                    );
                    mine.push(
                        self.row(
                            Some(div().w(px(34.0)).flex_none().into_any_element()),
                            "Who gets your settings",
                            Some(who_words(given.as_deref(), &people)),
                            Some(chips.into_any_element()),
                        )
                        .into_any_element(),
                    );
                }
            }
            let mut actions = div().flex().gap(px(space::S));
            if let Some(version) = newer {
                let source = prog.source.clone();
                actions = actions.child(
                    self.small_button(
                        format!("update-{}", prog.name),
                        &format!("Update to {version}"),
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.install(&source);
                        this.send(f);
                        cx.notify();
                    })),
                );
            }
            let program = prog.name.clone();
            actions = actions.child(
                self.small_button(format!("remove-{}", prog.name), "Remove", false)
                    .text_color(p.bad)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.remove_program(&program);
                        this.send(f);
                        this.program_open = None;
                        cx.notify();
                    })),
            );
            mine.push(
                div()
                    .flex()
                    .justify_end()
                    .px(px(space::L))
                    .py(px(space::M))
                    .child(actions)
                    .into_any_element(),
            );
        }
        if mine.is_empty() {
            groups.push(self.empty(
                Ico::Brain,
                "No brains yet",
                "The program an agent thinks with, such as Claude or Codex. Install one below",
                None,
            ));
        } else {
            groups.push(self.group(Some("Your brains"), mine));
        }
        // The registry's, not yet installed, as cards with their icons.
        let mut cards = Vec::new();
        for l in self.model.registry.clone() {
            if l.installed.is_some() || self.model.program_ids.values().any(|id| *id == l.id) {
                continue;
            }
            let action: AnyElement = if self.model.installing.as_deref() == Some(l.id.as_str()) {
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child("Installing…")
                    .into_any_element()
            } else if l.how.is_some() {
                let id = l.id.clone();
                self.small_button(format!("install-{}", l.id), "Install", false)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let f = this.model.install(&id);
                        this.send(f);
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                div()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child("Not for this computer")
                    .into_any_element()
            };
            cards.push(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::M))
                    .p(px(space::L))
                    .rounded(px(radius::CARD))
                    .border_1()
                    .border_color(p.line)
                    .bg(self.card_fill())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(space::S))
                            .child(self.brain_tile(Some(&l), 40.0))
                            .child(action),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(size::BODY))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.ink)
                                    .truncate()
                                    .child(l.name.clone()),
                            )
                            .child(
                                div()
                                    .h(px(32.0))
                                    .overflow_hidden()
                                    .text_size(px(size::CAPTION))
                                    .line_height(px(16.0))
                                    .text_color(p.ink_3)
                                    .child(l.description.clone()),
                            ),
                    )
                    .into_any_element(),
            );
        }
        let get_more: AnyElement = if self.model.registry_loading {
            self.group(
                None,
                vec![self.note_row("Reading the ACP Registry…", p.ink_3)],
            )
        } else if let Some(e) = self.model.registry_error.clone() {
            self.group(None, vec![self.note_row(e, p.bad)])
        } else {
            div()
                .grid()
                .grid_cols(2)
                .gap(px(space::S))
                .children(cards)
                .into_any_element()
        };
        groups.push(
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(
                    div()
                        .px(px(space::XS))
                        .text_size(px(size::CAPTION))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_3)
                        .child("More brains, from the ACP Registry"),
                )
                .child(get_more)
                .into_any_element(),
        );
        self.settings_page("programs-page", "Brains", None, groups)
    }

    /// The Agent defaults page: what every agent follows where it has nothing of its
    /// own, changed live for every agent that follows it.
    fn defaults_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let d = self.model.defaults.clone();
        let put = |args: Value| {
            cx.listener(move |this: &mut Loom, _: &ClickEvent, _: &mut Window, cx| {
                let f = this.model.put_defaults(args.clone());
                this.send(f);
                cx.notify();
            })
        };
        // What a new agent runs on.
        let programs = self.model.programs.clone();
        let brain: AnyElement = if programs.is_empty() {
            div()
                .text_size(px(size::SMALL))
                .text_color(p.ink_3)
                .child("None installed")
                .into_any_element()
        } else {
            let mut items = Vec::new();
            for name in programs.clone() {
                items.push(
                    self.segment(
                        format!("default-brain-{name}"),
                        &program_title(&name),
                        d.brain == name,
                    )
                    .on_click(put(serde_json::json!({"brain": name}))),
                );
            }
            self.segments(items).into_any_element()
        };
        // What takes an agent's turn over when its own program runs out of usage or
        // doesn't start, for agents without a backup of their own.
        let backup: Option<AnyElement> = (!programs.is_empty()).then(|| {
            let mut items = vec![self
                .segment("default-backup-none", "None", d.backup.is_empty())
                .on_click(put(serde_json::json!({"backup": null})))];
            for name in programs.clone() {
                items.push(
                    self.segment(
                        format!("default-backup-{name}"),
                        &program_title(&name),
                        d.backup == name,
                    )
                    .on_click(put(serde_json::json!({"backup": name}))),
                );
            }
            self.segments(items).into_any_element()
        });
        let brain_how = if d.brain.is_empty() {
            "None picked yet: what an agent your agents make runs on"
        } else {
            "The program a new agent runs on"
        };
        let mut rows = vec![self
            .row(None, "Brain", Some(brain_how.into()), Some(brain))
            .into_any_element()];
        if let Some(backup) = backup {
            rows.push(
                self.row(
                    None,
                    "Backup program",
                    Some(
                        "Takes a turn over when an agent's own runs out of usage or doesn't start"
                            .into(),
                    ),
                    Some(backup),
                )
                .into_any_element(),
            );
        }
        let new_agents = self.group(Some("Brains"), rows);
        // Each program's model and effort, from what an agent on it has said it offers.
        let mut picks = Vec::new();
        for name in programs {
            let Some(agent) = self
                .model
                .agents
                .iter()
                .find(|a| a.program == name && !a.picks().is_empty())
                .cloned()
            else {
                picks.push(self.note_row(
                    format!(
                        "{}: its models show here once an agent on it has started.",
                        program_title(&name)
                    ),
                    p.ink_3,
                ));
                continue;
            };
            for o in agent.picks() {
                let picked = d.pick(&name, &o.id);
                let mut row = div().flex().flex_wrap().gap(px(space::S)).child(
                    self.small_button(
                        format!("default-pick-{name}-{}-own", o.id),
                        "What it starts on",
                        picked.is_none(),
                    )
                    .on_click(put(
                        serde_json::json!({"config": {name.clone(): {o.id.clone(): null}}}),
                    )),
                );
                for (value, label) in o.values.clone() {
                    row = row.child(
                        self.small_button(
                            format!("default-pick-{name}-{}-{value}", o.id),
                            &label,
                            picked.as_deref() == Some(value.as_str()),
                        )
                        .on_click(put(
                            serde_json::json!({"config": {name.clone(): {o.id.clone(): value}}}),
                        )),
                    );
                }
                picks.push(self.text_row(
                    &format!("{} · {}", program_title(&name), o.name),
                    None,
                    row,
                ));
            }
        }
        let models = self.group(
            Some("Model and effort, for agents that don't pick their own"),
            picks,
        );
        // How agents ask, and what a message while one answers does.
        let mut mode_items = Vec::new();
        for mode in ["ask", "auto", "allow"] {
            mode_items.push(
                self.segment(
                    format!("default-mode-{mode}"),
                    model::mode_name(mode),
                    d.permission == mode,
                )
                .on_click(put(serde_json::json!({"permission": mode}))),
            );
        }
        let mut busy_items = Vec::new();
        for (busy, label) in [("queue", "Waits its turn"), ("steer", "Steers at once")] {
            busy_items.push(
                self.segment(format!("default-busy-{busy}"), label, d.busy == busy)
                    .on_click(put(serde_json::json!({"busy": busy}))),
            );
        }
        let ways = self.group(
            Some("How they work"),
            vec![
                self.row(
                    None,
                    "Permission",
                    Some("For agents without a mode of their own".into()),
                    Some(self.segments(mode_items).into_any_element()),
                )
                .into_any_element(),
                self.row(
                    None,
                    "A message while it answers",
                    Some("Waits in the queue, or stops the turn and goes now".into()),
                    Some(self.segments(busy_items).into_any_element()),
                )
                .into_any_element(),
            ],
        );
        let instructions = self.group(
            Some("Instructions every agent follows"),
            vec![
                div()
                    .p(px(space::L))
                    .child(Textarea::new(&self.every_in))
                    .into_any_element(),
                div()
                    .px(px(space::L))
                    .py(px(space::S + 2.0))
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child("Told to every agent beside About you, under their own heading")
                    .into_any_element(),
            ],
        );
        let save = self
            .small_button("save-defaults", "Save", true)
            .on_click(cx.listener(|this, _, _, cx| {
                let text = this.every_in.read(cx).value().to_string();
                let f = this
                    .model
                    .put_defaults(serde_json::json!({"instructions": text.trim()}));
                this.send(f);
                cx.notify();
            }))
            .into_any_element();
        self.settings_page(
            "defaults-page",
            "Agent defaults",
            Some(save),
            vec![new_agents, ways, models, instructions],
        )
    }

    /// The About you page: the person's profile, which every agent's session starts with.
    fn profile_page(&mut self, cx: &mut Context<Self>) -> Div {
        let p = self.p;
        let said = match &self.model.profile {
            Some((_, revision, by)) if *revision > 0 => {
                format!("Revision {revision}, by {}", self.model.made_by(by))
            }
            Some(_) => "Not written yet. Every agent starts with what you put here".to_string(),
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
            list = list.child(div().text_size(px(size::SMALL)).text_color(p.ink_3).child(
                "Nothing remembered yet. It keeps notes here as it learns, and you \
                         can add one above: Add for all shares it with every agent.",
            ));
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
        // Its entries under a line for each day, made again only when they change, or
        // the day does, when Today becomes Yesterday.
        let activity = &self.model.activity;
        let key = (
            activity.len(),
            activity.first().map(|e| e.at),
            activity.last().map(|e| e.at),
            chrono::Local::now().date_naive(),
        );
        if self.activity_lines.0 != key {
            let mut lines = Vec::new();
            let mut day = String::new();
            for (i, e) in activity.iter().enumerate() {
                let this_day = day_of(e.at);
                if this_day != day {
                    lines.push(ActivityLine::Day(this_day.clone()));
                    day = this_day;
                }
                lines.push(ActivityLine::Entry(i));
            }
            self.activity_lines = (key, Rc::new(lines));
        }
        // The export, then the entries, or a word that there are none yet.
        let n = self.activity_lines.1.len().max(1);
        self.list_page(
            "activity-page",
            n + 1,
            ACTIVITY_WIDTH,
            |this, ix, cx| {
                if ix == 0 {
                    return div()
                        .flex()
                        .justify_end()
                        .child(
                            this.button("export-activity", "Export as CSV", false)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(f) = this.model.export_activity() {
                                        this.send(f);
                                    }
                                    cx.notify();
                                })),
                        )
                        .into_any_element();
                }
                let line = this.activity_lines.1.get(ix - 1).cloned();
                this.activity_line(line)
            },
            cx,
        )
    }

    /// A line of the activity page: a day's name over its entries, an entry, or, with
    /// nothing to show, a word that there's nothing yet.
    fn activity_line(&self, line: Option<ActivityLine>) -> AnyElement {
        let p = self.p;
        let e = match line {
            None => {
                return div()
                    .text_size(px(size::SMALL))
                    .pt(px(space::L))
                    .text_color(p.ink_3)
                    .child(
                        "Nothing yet. What it runs, what it's allowed and what changes \
                         show here as it works.",
                    )
                    .into_any_element()
            }
            Some(ActivityLine::Day(day)) => {
                return div()
                    .pt(px(space::L))
                    .pb(px(space::XS))
                    .text_size(px(size::CAPTION))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.ink_3)
                    .child(day)
                    .into_any_element()
            }
            Some(ActivityLine::Entry(i)) => match self.model.activity.get(i) {
                Some(e) => e,
                None => return div().into_any_element(),
            },
        };
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
        // Its time and kind sit on its first line, however many lines it takes.
        div()
            .flex()
            .items_start()
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
                            .child(readable_name(&e.text)),
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
            })
            .into_any_element()
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
                let written = std::fs::write(&path, csv.as_bytes());
                let _ = this.update(cx, |loom, cx| {
                    match written {
                        Ok(()) => loom
                            .model
                            .said
                            .push(format!("Saved the activity log to {}", path.display())),
                        Err(e) => loom
                            .model
                            .notices
                            .push(format!("The activity log couldn't be saved: {e}")),
                    }
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
                .filter(|b| b.kind == "text" && b.parent.is_none())
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
                    .soft_hover(transparent_black(), p.hover, None)
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
                                    .tooltip(tip("Unpin"))
                                    .size(px(22.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(radius::ROW))
                                    .soft_hover(
                                        transparent_black(),
                                        p.active,
                                        Some((p.ink_3, p.ink)),
                                    )
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

    /// An answer picked on an agent's choices card: it goes as the person's message.
    pub fn choose(&mut self, answer: &str, cx: &mut Context<Self>) {
        if let Some(frame) = self.model.send_choice(answer) {
            self.send(frame);
        }
        cx.notify();
    }

    /// Opens what each clicked notification was about, with the window up: its server,
    /// its agent and its conversation. The person clicked it, so the window comes forward.
    fn notification_clicks(&mut self, cx: &mut Context<Self>) {
        for about in shell::clicked() {
            shell::show_window(self.hwnd, true);
            self.show_server(&about.server, cx);
            self.popup = None;
            self.menu = None;
            self.screen = Screen::Chat;
            self.model.open_about(&about.agent, &about.conversation);
            self.fetch();
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    fn tray_actions(&mut self, cx: &mut Context<Self>) {
        let Some(tray) = &self.tray else { return };
        for a in tray.actions() {
            match a {
                shell::TrayAction::Open => shell::show_window(self.hwnd, true),
                shell::TrayAction::StartAtLogin(on) => {
                    shell::set_start_at_login(on);
                    self.starts_at_login = shell::starts_at_login();
                }
                shell::TrayAction::CheckUpdates => {
                    self.send_to(
                        "",
                        &serde_json::json!({"t": "update.check", "id": "app.update"}),
                    );
                }
                shell::TrayAction::Quit => {
                    self.send(serde_json::json!({"t": "app.active", "active": false}));
                    cx.quit();
                }
            }
        }
    }

    /// The person's answer to a question: its card says how at once, until the instance
    /// takes the question away.
    pub fn answer(&mut self, call: String, allow: bool, always: bool, cx: &mut Context<Self>) {
        let f = self.model.answer(&call, allow, always);
        self.send(f);
        self.refresh_rows(cx);
        cx.notify();
    }
}

/// The person's bubble in an agent's chat. With Vibrant bubbles, the default, it's the
/// agent's colour itself, as saturated as it is, made light in the light theme and deep in
/// the dark, so the words read on it whichever colour it is (`motion::ink_on` makes sure).
/// Without, it's the bubble grey with a breath of the agent's colour, a little more in the
/// dark.
fn person_bubble(p: Palette, tint: Hsla, vibrant: bool) -> Hsla {
    if vibrant {
        return Hsla {
            l: if p.dark { 0.28 } else { 0.8 },
            ..tint
        };
    }
    motion::mix(p.bubble, tint, if p.dark { 0.12 } else { 0.07 })
}

#[allow(clippy::too_many_arguments)]
fn person(
    p: Palette,
    m: &Message,
    tint: Hsla,
    vibrant: bool,
    files: &HashMap<String, String>,
    scale: f32,
    room: i32,
    joined: model::Joined,
    this: WeakEntity<Loom>,
) -> Div {
    let text: String = m
        .blocks
        .iter()
        .filter(|b| b.kind == "text")
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
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
    // Its files over its words, two or more pictures as one stack.
    let mut stacked: Vec<model::FileRef> = Vec::new();
    for part in model::file_parts(m) {
        match part {
            model::Part::Pictures(run) => {
                let stack: Vec<model::FileRef> = run
                    .iter()
                    .filter_map(|&i| m.blocks[i].file.clone())
                    .collect();
                out = out.child(picture_stack(
                    p,
                    &stack,
                    files,
                    format!("{}/{}", m.id, run[0]),
                    room,
                    this.clone(),
                ));
                stacked.extend(stack);
            }
            model::Part::Block(i) => {
                if let Some(f) = &m.blocks[i].file {
                    out = out.child(file_view(
                        p,
                        f,
                        files,
                        format!("{}/{i}", m.id),
                        room,
                        this.clone(),
                    ));
                }
            }
            _ => {}
        }
    }
    // Each file reads as a link where it was attached, which opens it: a picture big over
    // the window, anything else in its own program. A stack's pictures only listed by
    // name are left out, since the stack shows them and the viewer their names.
    // What a person typed keeps its line breaks, which Markdown would join.
    let text = model::unmark_stacked(&text, &stacked).replace('\n', "  \n");
    let attached: Vec<model::FileRef> = m.blocks.iter().filter_map(|b| b.file.clone()).collect();
    let words = model::link_files(&model::literal_html(&text), &attached);
    let opener = this.clone();
    let on_link = move |url: &SharedString, _: &ClickEvent, _: &mut Window, cx: &mut App| match url
        .strip_prefix("loom-file:")
    {
        Some(sha) => {
            if let Some(f) = attached.iter().find(|f| f.file == sha) {
                let f = f.clone();
                let _ = opener.update(cx, |loom, cx| {
                    if f.image() {
                        loom.view(&f, cx);
                    } else {
                        loom.open_file(&f, cx);
                    }
                });
            }
        }
        None => cx.open_url(url),
    };
    // Its corners on the right smaller where it joins the person's messages either side.
    // What the person wrote takes the agent's colour; what a routine or another agent
    // sent is plain grey.
    let c = model::corners(true, joined);
    let bg = if m.from_agent.is_some() {
        p.bubble
    } else {
        person_bubble(p, tint, vibrant)
    };
    let ink = motion::ink_on(bg, p.ink, p.bg);
    out.when(!text.trim().is_empty(), |d| {
        d.child(
            div()
                .max_w(px(520.0))
                .px(px(space::L))
                .py(px(space::S + 2.0))
                .rounded_tl(px(c.tl))
                .rounded_tr(px(c.tr))
                .rounded_br(px(c.br))
                .rounded_bl(px(c.bl))
                .bg(bg)
                .text_size(px(size::BODY * scale))
                .text_color(ink)
                .when(m.queued, |d| d.opacity(0.6))
                .child(KeptText::new(
                    SharedString::from(format!("m-{}-{room}", m.id)),
                    words,
                    move |v| v.selectable(true).on_link_click(on_link),
                )),
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
    room: i32,
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
            .soft_hover(p.bubble, p.hover, None)
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
    if f.image() {
        // A picture opens big over the window, inside loom.
        let open = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |loom, cx| loom.view(&file, cx));
        };
        // Its own place in the message, sized before it's drawn: scaled down to fit the
        // column and PICTURE_H, its shape kept. A size given only as a most (max_w,
        // max_h) let a tall one draw past its place, over the words under it.
        let most_w = PICTURE_W
            .min(room as f32 - 2.0 * space::XL)
            .max(PICTURE_LEAST);
        let picture = match files.get(&format!("{}.thumb", f.file)) {
            Some(path) => {
                let (w, h) = match picture_size(path) {
                    Some((w, h)) => fit(w, h, most_w, PICTURE_H),
                    None => PICTURE_UNKNOWN,
                };
                img(std::path::PathBuf::from(path))
                    .when_some(chat_images(), |i, c| i.image_cache(&c))
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(radius::BUBBLE))
                    .into_any_element()
            }
            // Until the thumbnail is here, a place of about its size.
            None => div()
                .w(px(PICTURE_UNKNOWN.0))
                .h(px(PICTURE_UNKNOWN.1))
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
    let open = move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
        let _ = this.update(cx, |loom, cx| loom.open_file(&file, cx));
    };
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
        .soft_hover(transparent_black(), p.hover, None)
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

/// Two or more pictures in a message as one stack of a fixed size, however many there
/// are: the first on top, filling its place, up to two more peeking out behind it, and
/// how many there are in its corner. Clicking it opens the viewer at the first, to step
/// through them.
fn picture_stack(
    p: Palette,
    stack: &[model::FileRef],
    files: &HashMap<String, String>,
    key: String,
    room: i32,
    this: WeakEntity<Loom>,
) -> AnyElement {
    let w = STACK_W
        .min(room as f32 - 2.0 * space::XL - 2.0 * STACK_STEP)
        .max(PICTURE_LEAST);
    let h = (w * STACK_H / STACK_W).round();
    let behind = stack.len().saturating_sub(1).min(2);
    let card = |f: &model::FileRef, layer: usize| {
        let shift = STACK_STEP * layer as f32;
        div()
            .absolute()
            .left(px(shift))
            .top(px(STACK_STEP * behind as f32 - shift))
            .w(px(w))
            .h(px(h))
            .rounded(px(radius::BUBBLE))
            .bg(p.bubble)
            // Each layer rounded itself: a parent's corners don't clip what's in it. The
            // picture comes cropped to the card's shape (`CardLoader`), so it fills it.
            .when_some(
                files.get(&format!("{}.thumb", f.file)).zip(card_images()),
                |d, (path, cards)| {
                    d.child(
                        img(std::path::PathBuf::from(path))
                            .image_cache(&cards)
                            .size_full()
                            .rounded(px(radius::BUBBLE))
                            .object_fit(ObjectFit::Fill),
                    )
                },
            )
            // The ones behind sit a step back, a little toward the page.
            .when(layer > 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .rounded(px(radius::BUBBLE))
                        .bg(p.bg.opacity(0.35)),
                )
            })
            // A fine edge over the picture, so the layers read apart.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .rounded(px(radius::BUBBLE))
                    .border_1()
                    .border_color(p.line),
            )
    };
    let mut out = div()
        .id(SharedString::from(format!("stack-{key}")))
        .relative()
        .flex_none()
        .w(px(w + STACK_STEP * behind as f32))
        .h(px(h + STACK_STEP * behind as f32))
        .cursor_pointer();
    for layer in (1..=behind).rev() {
        out = out.child(card(&stack[layer], layer));
    }
    let opened = stack.to_vec();
    out.child(
        card(&stack[0], 0).child(
            div()
                .absolute()
                .right(px(space::S))
                .bottom(px(space::S))
                .h(px(20.0))
                .min_w(px(20.0))
                .px(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(black().opacity(0.6))
                .text_color(white())
                .text_size(px(size::CAPTION))
                .font_weight(FontWeight::MEDIUM)
                .child(stack.len().to_string()),
        ),
    )
    .on_click(move |_, _, cx| {
        let _ = this.update(cx, |loom, cx| loom.view_stack(&opened, cx));
    })
    .into_any_element()
}

/// A stack's place, wide and tall, at most, and how far each picture behind peeks out.
const STACK_W: f32 = 240.0;
const STACK_H: f32 = 180.0;
const STACK_STEP: f32 = 6.0;

/// The most room a picture in a message takes, wide and tall.
const PICTURE_W: f32 = 360.0;
const PICTURE_H: f32 = 400.0;
/// The least it's given across, in a narrow column.
const PICTURE_LEAST: f32 = 120.0;
/// The place a picture takes while its size isn't known.
const PICTURE_UNKNOWN: (f32, f32) = (240.0, 160.0);

/// A JPEG as the BGRA pixels GPUI draws, turned as its EXIF says; None for what
/// jpeg-decoder doesn't read (not a JPEG, or CMYK). Every JPEG loom shows comes through
/// here: Chrome on Linux, and some other encoders, write a JPEG as three scans, one for
/// each of Y, Cb and Cr, and zune-jpeg 0.5.15, image's decoder and so GPUI's, reads those
/// as bright green stripes over a faint picture (LOO-252, LOO-255). jpeg-decoder reads
/// both.
fn jpeg_pixels(jpeg: &[u8]) -> Option<image::RgbaImage> {
    let mut decoder = jpeg_decoder::Decoder::new(jpeg);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    let bgra: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [b, g, r, 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        _ => return None,
    };
    let bgra = image::RgbaImage::from_raw(w, h, bgra)?;
    // A phone's photo is kept as its camera saw it, with the way up in its EXIF.
    match decoder
        .exif_data()
        .and_then(image::metadata::Orientation::from_exif_chunk)
    {
        Some(turn) if turn != image::metadata::Orientation::NoTransforms => {
            let mut turned = image::DynamicImage::ImageRgba8(bgra);
            turned.apply_orientation(turn);
            Some(turned.into_rgba8())
        }
        _ => Some(bgra),
    }
}

/// A picture file's pixels as GPUI draws them, BGRA: a JPEG through `jpeg_pixels`, any
/// other kind through image.
fn picture_pixels(bytes: &[u8]) -> Option<image::RgbaImage> {
    if let Some(bgra) = jpeg_pixels(bytes) {
        return Some(bgra);
    }
    let mut rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    for px in rgba.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(rgba)
}

/// A JPEG file, for `PictureLoader`: None for any other kind of file, which GPUI's own
/// loader takes.
fn jpeg_file(path: &std::path::Path) -> Option<Arc<RenderImage>> {
    let bgra = jpeg_pixels(&std::fs::read(path).ok()?)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
}

/// How the pictures in chats, the composer and the viewer load (`KeptImages`): GPUI's
/// own loader, but with a JPEG file decoded by `jpeg_pixels`.
enum PictureLoader {}

impl Asset for PictureLoader {
    type Source = Resource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(
        source: Resource,
        cx: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let path = match &source {
            Resource::Path(path) => Some(path.clone()),
            _ => None,
        };
        let gpui = ImageAssetLoader::load(source, cx);
        async move {
            match path.and_then(|p| jpeg_file(&p)) {
                Some(image) => Ok(image),
                None => gpui.await,
            }
        }
    }
}

/// How a stack's cards load (`KeptImages` with `cards`): the picture file cropped from its
/// middle to the stack's shape, so it's drawn whole, filling its card, with its own
/// corners rounded. GPUI clips only to rectangles, so a picture drawn to cover its card
/// either spilled out of it or had square corners.
enum CardLoader {}

impl Asset for CardLoader {
    type Source = Resource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    // An async fn would hold on to `App`, and the future must outlive it.
    #[allow(clippy::manual_async_fn)]
    fn load(
        source: Resource,
        _: &mut App,
    ) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        async move {
            let failed = |why: &str| ImageCacheError::Io(Arc::new(std::io::Error::other(why)));
            let Resource::Path(path) = source else {
                return Err(failed("a card is a file"));
            };
            let bytes = std::fs::read(&*path).map_err(|e| ImageCacheError::Io(Arc::new(e)))?;
            let full = picture_pixels(&bytes).ok_or_else(|| failed("not a picture"))?;
            let (x, y, w, h) = card_crop(full.width(), full.height());
            let card = image::imageops::crop_imm(&full, x, y, w, h).to_image();
            Ok(Arc::new(RenderImage::new(vec![image::Frame::new(card)])))
        }
    }
}

/// The middle of a `w` × `h` picture in a stack card's shape: its left, top, width and
/// height.
fn card_crop(w: u32, h: u32) -> (u32, u32, u32, u32) {
    let (w, h) = (w.max(1), h.max(1));
    let shape = STACK_W / STACK_H;
    if w as f32 / h as f32 > shape {
        let cw = ((h as f32 * shape).round() as u32).clamp(1, w);
        ((w - cw) / 2, 0, cw, h)
    } else {
        let ch = ((w as f32 / shape).round() as u32).clamp(1, h);
        (0, (h - ch) / 2, w, ch)
    }
}

/// The size to draw a `w` × `h` picture at, at most `most_w` × `most_h`: scaled down to
/// fit both, its shape kept, and never up.
fn fit(w: u32, h: u32, most_w: f32, most_h: f32) -> (f32, f32) {
    let (w, h) = (w.max(1) as f32, h.max(1) as f32);
    let k = (most_w / w).min(most_h / h).min(1.0);
    ((w * k).round().max(1.0), (h * k).round().max(1.0))
}

/// A picture's size in pixels, from its file's header, kept by path: a file's path is
/// its SHA-256, so what's there doesn't change shape.
fn picture_size(path: &str) -> Option<(u32, u32)> {
    thread_local! {
        static SIZES: std::cell::RefCell<HashMap<String, Option<(u32, u32)>>> =
            std::cell::RefCell::new(HashMap::new());
    }
    if let Some(kept) = SIZES.with(|s| s.borrow().get(path).copied()) {
        return kept;
    }
    let size = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .ok()
        .and_then(|r| r.into_dimensions().ok());
    SIZES.with(|s| s.borrow_mut().insert(path.to_string(), size));
    size
}

/// What a reply shows as it's drawn, from the person's prefs.
#[derive(Clone, Copy, PartialEq)]
struct Show {
    tools: Tools,
    thoughts: Thoughts,
    text_scale: f32,
    bubbles: bool,
    vibrant: bool,
    /// Whether this is the chat's newest message, whose choices still answer.
    newest: bool,
    /// How wide the chat's column is, to the pixel (`Loom::chat_room`), in the text's
    /// id, so text measures itself again when the column changes.
    room: i32,
}

/// The widest a card in the chat grows: past that, its few lines would sit far apart
/// from its buttons in a wide column.
const CARD_MOST: f32 = 600.0;

/// A card's fill: the window's own in the light theme, outlined, and a step lighter in
/// the dark, where an outline alone sinks.
fn card_fill(p: Palette) -> Hsla {
    if p.dark {
        p.raised
    } else {
        p.bg
    }
}

/// What sits inside a card on a fill of its own (its icon's square, a command, the
/// Details), a step from the card's.
fn card_inset(p: Palette) -> Hsla {
    if p.dark {
        p.bg
    } else {
        p.raised
    }
}

/// The frame every card in the chat is drawn in (PLAN.md, "Cards in the chat"): loom's
/// asks and its sign-in, an agent's `show_card`, the install's plan, and the asks above
/// the composer. A box with a small square at its start (an icon or an initial), and
/// beside it, from the top, the title with what goes beside it, the rows, and the
/// actions, one of them the main one. Answered, a card is one quiet line (`card_done`).
#[derive(Default)]
struct CardFrame {
    lead: Option<AnyElement>,
    title: Option<SharedString>,
    side: Option<AnyElement>,
    rows: Vec<AnyElement>,
    actions: Vec<AnyElement>,
}

impl CardFrame {
    fn new() -> Self {
        Self::default()
    }

    /// The small square at its start: `card_icon` or `card_initial`.
    fn lead(mut self, lead: impl IntoElement) -> Self {
        self.lead = Some(lead.into_any_element());
        self
    }

    /// Its title, which an empty one leaves out.
    fn title(mut self, title: impl Into<SharedString>) -> Self {
        let title = title.into();
        if !title.is_empty() {
            self.title = Some(title);
        }
        self
    }

    /// What goes at the title's end: how far along it is, or its one button.
    fn side(mut self, side: impl IntoElement) -> Self {
        self.side = Some(side.into_any_element());
        self
    }

    fn row(mut self, row: impl IntoElement) -> Self {
        self.rows.push(row.into_any_element());
        self
    }

    /// A button under the rows, the main one first (`card_button`).
    fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    fn draw(self, p: Palette, scale: f32) -> Div {
        let head = (self.title.is_some() || self.side.is_some()).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .min_h(px(control::ICON_SMALL))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(size::BODY * scale))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink)
                        .children(self.title),
                )
                .children(self.side)
        });
        let actions = (!self.actions.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(space::S))
                .pt(px(space::XS))
                .children(self.actions)
        });
        div()
            .w_full()
            .flex()
            .items_start()
            .gap(px(space::M))
            .px(px(space::L))
            .py(px(space::M))
            .rounded(px(radius::CARD))
            .border_1()
            .border_color(if p.dark { p.line } else { p.line_strong })
            .bg(card_fill(p))
            .children(self.lead)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .children(head)
                    .children(self.rows)
                    .children(actions),
            )
    }
}

/// The small square at a card's start.
fn card_lead(p: Palette) -> Div {
    div()
        .size(px(control::ICON_SMALL))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::ROW))
        .bg(card_inset(p))
}

/// A card's icon, in `ink`: an ask's in its agent's colour, the one hint of it.
fn card_icon(p: Palette, icon: Ico, ink: Hsla) -> Div {
    card_lead(p).text_color(ink).child(Icon::new(icon).small())
}

/// A card's initial, for something with a name and no icon of its own, such as an MCP
/// server.
fn card_initial(p: Palette, name: &str) -> Div {
    let initial = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".into());
    card_lead(p)
        .text_size(px(size::SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.ink_2)
        .child(initial)
}

/// A labelled line in a card: what it is, quiet, then its value, each cut to the room
/// it has, so a long one never runs past the card.
fn card_line(
    p: Palette,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    scale: f32,
) -> Div {
    div()
        .flex()
        .gap(px(space::M))
        .text_size(px(size::SMALL * scale))
        .child(
            div()
                .w(px(layout::FIELD_LABEL))
                .flex_none()
                .truncate()
                .text_color(p.ink_3)
                .child(label.into()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(p.ink)
                .child(value.into()),
        )
}

/// Labelled lines in a card, close together, as an approval's call and the facts card
/// show them.
fn card_lines(p: Palette, lines: &[(String, String)], scale: f32) -> Div {
    let mut out = div().flex().flex_col().gap(px(space::XS));
    for (label, value) in lines {
        out = out.child(card_line(p, label.clone(), value.clone(), scale));
    }
    out
}

/// A command or a script in a card, as it's written, wrapped to the card.
fn card_code(p: Palette, text: impl Into<SharedString>, scale: f32) -> Div {
    div()
        .w_full()
        .px(px(space::M))
        .py(px(space::S))
        .rounded(px(radius::INPUT))
        .bg(card_inset(p))
        .font_family(crate::prefs::mono_font())
        .text_size(px(size::SMALL * scale))
        .text_color(p.ink_2)
        .child(text.into())
}

/// Details, folded under its line until it's clicked: all of what a card shows only
/// some of, in a box of its own that scrolls past a few lines.
fn card_details(
    p: Palette,
    key: String,
    open: bool,
    raw: &str,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Div {
    let k = key.clone();
    let toggle = div()
        .id(SharedString::from(format!("t-{key}")))
        .flex()
        .items_center()
        .gap(px(space::XS))
        .text_size(px(size::CAPTION))
        .text_color(p.ink_3)
        .cursor_pointer()
        .soft_ink(p.ink_3, p.ink)
        .child(if open { "⌄" } else { "›" })
        .child("Details")
        .on_click(move |_, _, cx| {
            let _ = this.update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
        });
    div()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .child(div().flex().child(toggle))
        .when(open, |d| {
            d.child(
                card_code(p, raw.to_string(), scale)
                    .id(SharedString::from(format!("r-{key}")))
                    .max_h(px(280.0))
                    .overflow_y_scroll(),
            )
        })
}

/// A card's button, as tall as an approval's: the main one filled, the rest outlined.
fn card_button(
    p: Palette,
    id: impl Into<SharedString>,
    label: &str,
    primary: bool,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .role(Role::Button)
        .aria_label(label.to_string())
        .px(px(space::M))
        .h(px(control::BUTTON_SMALL))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(radius::BUTTON))
        .text_size(px(size::SMALL))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(primary, |d| {
            d.text_color(p.button_ink)
                .soft_hover(p.button, motion::mix(p.button, p.bg, 0.18), None)
        })
        .when(!primary, |d| {
            d.border_1()
                .border_color(p.line_strong)
                .text_color(p.ink)
                .soft_hover(transparent_black(), p.hover, None)
        })
        .child(label.to_string())
}

/// A card once it's answered: one quiet line saying how.
fn card_done(p: Palette, icon: Ico, words: impl Into<SharedString>, scale: f32) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .text_size(px(size::SMALL * scale))
        .text_color(p.ink_3)
        .child(Icon::new(icon).xsmall())
        .child(div().min_w_0().truncate().child(words.into()))
}

/// A card an agent put in its reply (PLAN.md, "Cards in the chat"), or loom's own, in
/// the card frame: choices as buttons that answer, and the rest in the frame.
fn card_view(
    p: Palette,
    card: &model::Card,
    key: &str,
    tint: Hsla,
    show: Show,
    secrets: &HashMap<String, Entity<InputState>>,
    this: WeakEntity<Loom>,
) -> Div {
    use model::CardBody;
    let scale = show.text_scale;
    // Choices stand on their own: the question, then a row of buttons under it, live
    // while it's the newest message. Once the chat has gone on, the one the person
    // sent stays marked, and the rest go quiet.
    if let CardBody::Choices { options, chosen } = &card.body {
        let mut row = div().flex().flex_wrap().gap(px(space::S));
        for (i, option) in options.iter().enumerate() {
            let chip = div()
                .id(SharedString::from(format!("choice-{key}-{i}")))
                .flex()
                .items_center()
                .gap(px(space::XS + 2.0))
                .px(px(space::M))
                .h(px(control::BUTTON_SMALL))
                .rounded(px(radius::BUTTON))
                .border_1()
                .text_size(px(size::SMALL * scale));
            row = row.child(if show.newest {
                let this = this.clone();
                let answer = option.clone();
                chip.border_color(tint.opacity(0.55))
                    .text_color(p.ink)
                    .cursor_pointer()
                    .soft_hover(p.bg, motion::mix(p.bg, tint, 0.14), None)
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |loom, cx| loom.choose(&answer, cx));
                    })
                    .child(option.clone())
            } else if *chosen == Some(i) {
                chip.border_color(p.line_strong)
                    .bg(card_inset(p))
                    .text_color(p.ink_2)
                    .child(Icon::new(Ico::Check).xsmall())
                    .child(option.clone())
            } else {
                chip.border_color(p.line)
                    .text_color(p.ink_3)
                    .child(option.clone())
            });
        }
        return div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .when(!card.title.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(size::SMALL * scale))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.ink_2)
                        .child(card.title.clone()),
                )
            })
            .child(row);
    }
    // Loom's own card for an MCP server that needs the person to sign in: the server
    // by its initial, and Sign in, which opens its sign-in page in their browser. Once
    // they have, one quiet line says so, as whom when the sign-in said; and once the
    // chat has gone on without it, one quiet line keeps a small Sign in.
    if let CardBody::SignIn {
        server,
        signed,
        signing,
        passed,
    } = &card.body
    {
        if let Some(who) = signed {
            let words = if who.is_empty() {
                format!("Signed in to {server}")
            } else {
                format!("Signed in to {server} as {who}")
            };
            return div().child(card_done(p, Ico::Check, words, scale));
        }
        let go = {
            let this = this.clone();
            let name = server.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |loom, cx| loom.sign_in_mcp(&name, cx));
            }
        };
        if *passed {
            return div().child(
                card_done(p, Ico::KeyRound, card.title.clone(), scale).child(
                    div()
                        .id(SharedString::from(format!("sign-in-{key}")))
                        .role(Role::Button)
                        .flex_none()
                        .text_color(p.ink_2)
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .soft_ink(p.ink_2, p.ink)
                        .child(if *signing { "Signing in…" } else { "Sign in" })
                        .on_click(go),
                ),
            );
        }
        return div().max_w(px(CARD_MOST)).child(
            CardFrame::new()
                .lead(card_initial(p, server))
                .title(card.title.clone())
                .side(
                    card_button(
                        p,
                        format!("sign-in-{key}"),
                        if *signing { "Signing in…" } else { "Sign in" },
                        true,
                    )
                    .on_click(go),
                )
                .draw(p, scale),
        );
    }
    // Loom's own card for an MCP server an agent proposed, in the card frame: what would
    // be added, with a masked field for each key the person doesn't have yet, and Add or
    // Cancel; then Sign in, for one that needs it. Once it's settled, one quiet line says
    // how it went.
    if let CardBody::McpAdd {
        call,
        server,
        url,
        command,
        args,
        settings,
        state,
        said,
        signed,
        signing,
    } = &card.body
    {
        let done = |icon: Ico, words: String| div().child(card_done(p, icon, words, scale));
        match state.as_str() {
            "ready" if said.is_empty() => return done(Ico::Check, format!("Added {server}")),
            "ready" => return done(Ico::Check, format!("Added {server} · {said}")),
            "added" => return done(Ico::Check, format!("Added {server}")),
            "later" if *signed => return done(Ico::Check, format!("Added {server}, signed in")),
            "cancelled" => return done(Ico::X, format!("Not added · {server}")),
            _ => {}
        }
        // A labelled line whose value is an element: a field, or quiet words.
        let line = |label: String, value: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .text_size(px(size::SMALL * scale))
                .child(
                    div()
                        .w(px(layout::FIELD_LABEL))
                        .flex_none()
                        .truncate()
                        .text_color(p.ink_3)
                        .child(label),
                )
                .child(div().flex_1().min_w_0().text_color(p.ink).child(value))
        };
        let mut list = div().flex().flex_col().gap(px(space::XS));
        list = list.child(if url.is_empty() {
            let words = std::iter::once(command)
                .chain(args)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            line(
                "Command".into(),
                div().truncate().child(words).into_any_element(),
            )
        } else {
            line(
                "Address".into(),
                div().truncate().child(url.clone()).into_any_element(),
            )
        });
        let asking = state == "asking";
        // A secret two settings name is typed in once, at the first.
        let mut typed = HashSet::new();
        for s in settings {
            let value = if s.secret.is_empty() {
                div().truncate().child(s.value.clone()).into_any_element()
            } else {
                match secrets.get(&format!("{call}/{}", s.secret)) {
                    Some(input) if asking && s.wanted() && typed.insert(&s.secret) => div()
                        .w_full()
                        .child(Input::new(input).small())
                        .into_any_element(),
                    _ => div()
                        .truncate()
                        .text_color(p.ink_3)
                        .child(if s.have {
                            format!("Your secret {}", s.secret)
                        } else {
                            format!("Secret {}", s.secret)
                        })
                        .into_any_element(),
                }
            };
            list = list.child(line(s.name.clone(), value));
        }
        let mut frame = CardFrame::new()
            .lead(card_initial(p, server))
            .title(card.title.clone())
            .row(list);
        // How it stands, in a line: the trouble in the colour that says so.
        let (words, bad) = match state.as_str() {
            "adding" => (format!("Starting {server}"), false),
            "sign_in" => (said.clone(), true),
            "later" => (format!("Added {server}. Sign in to use it."), false),
            "failed" => (format!("It didn't start: {said}"), true),
            _ => (String::new(), false),
        };
        if !words.is_empty() {
            frame = frame.row(
                div()
                    .text_size(px(size::SMALL * scale))
                    .text_color(if bad { p.bad } else { p.ink_3 })
                    .child(words),
            );
        }
        let answer = |label: &str, answer: &'static str, primary: bool| {
            let this = this.clone();
            let call = call.clone();
            card_button(p, format!("mcp-add-{answer}-{key}"), label, primary).on_click(
                move |_, _, cx| {
                    let _ = this.update(cx, |loom, cx| loom.answer_mcp_add(&call, answer, cx));
                },
            )
        };
        let sign_in = || {
            let this = this.clone();
            let server = server.clone();
            card_button(
                p,
                format!("mcp-add-sign-in-{key}"),
                if *signing { "Signing in…" } else { "Sign in" },
                true,
            )
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |loom, cx| loom.sign_in_mcp(&server, cx));
            })
        };
        match state.as_str() {
            "asking" => {
                frame = frame
                    .action(answer("Add", "add", true))
                    .action(answer("Cancel", "cancel", false));
            }
            "sign_in" => {
                frame = frame
                    .action(sign_in())
                    .action(answer("Later", "later", false));
            }
            "later" => frame = frame.action(sign_in()),
            _ => {}
        }
        return div().max_w(px(CARD_MOST)).child(frame.draw(p, scale));
    }
    let mut frame = CardFrame::new().title(card.title.clone());
    match &card.body {
        CardBody::Checklist(items) => {
            let done = items.iter().filter(|(_, d)| *d).count();
            frame = frame.side(
                div()
                    .flex_none()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(format!("{done} of {}", items.len())),
            );
            let mut list = div().flex().flex_col().gap(px(space::XS + 2.0));
            for (text, done) in items {
                let mark = div()
                    .size(px(16.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .text_size(px(size::SMALL * scale))
                        .child(if *done {
                            mark.border_color(tint)
                                .bg(tint)
                                .text_color(motion::ink_on(tint, p.button_ink, p.button))
                                .child(Icon::new(Ico::Check).xsmall())
                        } else {
                            mark.border_color(p.line_strong)
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .when(*done, |d| d.text_color(p.ink_3))
                                .when(!*done, |d| d.text_color(p.ink))
                                .child(text.clone()),
                        ),
                );
            }
            frame = frame.row(list);
        }
        CardBody::Progress { label, value } => {
            let pct = (value * 100.0).round() as u32;
            frame = frame.side(
                div()
                    .flex_none()
                    .text_size(px(size::CAPTION))
                    .text_color(p.ink_3)
                    .child(format!("{pct}%")),
            );
            // With no title, what's going on is the title.
            frame = if card.title.is_empty() || label.is_empty() {
                frame.title(label.clone())
            } else {
                frame.row(
                    div()
                        .truncate()
                        .text_size(px(size::SMALL * scale))
                        .text_color(p.ink_2)
                        .child(label.clone()),
                )
            };
            frame = frame.row(
                div()
                    .h(px(6.0))
                    .w_full()
                    .rounded_full()
                    .bg(card_inset(p))
                    .child(div().h_full().w(relative(*value)).rounded_full().bg(tint)),
            );
        }
        CardBody::Facts(items) => {
            frame = frame.row(card_lines(p, items, scale));
        }
        CardBody::Table { columns, rows } => {
            // Each column as wide as its longest cell asks, between a few characters and
            // forty, so a number's column doesn't take a title's room.
            let share: Vec<f32> = (0..columns.len())
                .map(|c| {
                    std::iter::once(columns.get(c))
                        .chain(rows.iter().map(|r| r.get(c)))
                        .flatten()
                        .map(|s| s.chars().count())
                        .max()
                        .unwrap_or(0)
                        .clamp(3, 40) as f32
                })
                .collect();
            let all: f32 = share.iter().sum::<f32>().max(1.0);
            let line = |cells: &[String], head: bool| {
                let mut r = div()
                    .flex()
                    .gap(px(space::M))
                    .py(px(space::XS + 2.0))
                    .text_size(px(if head {
                        size::CAPTION
                    } else {
                        size::SMALL * scale
                    }));
                for (k, c) in cells.iter().enumerate() {
                    r = r.child(
                        div()
                            .flex_basis(relative(share.get(k).copied().unwrap_or(3.0) / all))
                            .flex_shrink(1.0)
                            .min_w_0()
                            .truncate()
                            .when(head, |d| {
                                d.font_weight(FontWeight::MEDIUM).text_color(p.ink_3)
                            })
                            .when(!head, |d| d.text_color(p.ink))
                            .child(c.clone()),
                    );
                }
                r
            };
            let mut table = div()
                .flex()
                .flex_col()
                .child(line(columns, true).border_b_1().border_color(p.line_strong));
            let last = rows.len().saturating_sub(1);
            for (i, row) in rows.iter().enumerate() {
                let r = line(row, false);
                table = table.child(if i < last {
                    r.border_b_1().border_color(p.line)
                } else {
                    r
                });
            }
            frame = frame.row(table);
        }
        CardBody::Choices { .. } | CardBody::SignIn { .. } | CardBody::McpAdd { .. } => {}
    }
    // A table has columns to fit, and takes the reply's room.
    let wide = matches!(card.body, CardBody::Table { .. });
    div()
        .w_full()
        .when(wide, |d| d.max_w(relative(0.94)))
        .when(!wide, |d| d.max_w(px(CARD_MOST)))
        .child(frame.draw(p, scale))
}

/// One tool call: its title, and how it went. `live` is a reply still being written,
/// where a call nothing finished yet is running.
fn tool_line(p: Palette, b: &model::Block, tools: &model::Tools, live: bool) -> Div {
    let t = b.call.as_ref().and_then(|c| tools.get(c));
    let (title, mark, color) = match t {
        Some(t) => {
            let (mark, color) = match model::tool_went(t, live) {
                model::Went::Done => ("✓", p.ok),
                model::Went::Failed => ("✕", p.bad),
                model::Went::Denied => ("denied", p.bad),
                model::Went::Stopped => ("stopped", p.ink_3),
                model::Went::Running => ("…", p.ink_3),
            };
            (t.title.clone(), mark, color)
        }
        None => ("Used a tool".to_string(), "", p.ink_3),
    };
    div()
        .flex()
        .items_center()
        .gap(px(space::S))
        .text_size(px(size::SMALL))
        .child(div().text_color(p.ink_3).child("›_"))
        .child(
            div()
                .font_family(crate::prefs::mono_font())
                .text_color(p.ink_2)
                .truncate()
                .child(title),
        )
        .child(div().text_color(color).child(mark))
}

/// How many message texts stay parsed while out of view.
const KEPT_TEXTS: usize = 300;

/// Each kept text state by its key, with when it was last drawn.
type KeptStates = HashMap<SharedString, (Entity<TextViewState>, u64)>;

thread_local! {
    /// Each message text's parsed state, by its key, with when it was last drawn. A
    /// TextView's own state goes once its row isn't drawn, so a row scrolled back into
    /// view was parsed again from the start, and a long one drew empty first.
    static TEXTS: std::cell::RefCell<(KeptStates, u64)> =
        std::cell::RefCell::new((HashMap::new(), 0));
    /// The pictures the chat and the composer show, and the one the viewer shows
    /// (`kept_images`).
    static IMAGES: std::cell::RefCell<Option<(Entity<KeptImages>, Entity<KeptImages>)>> =
        const { std::cell::RefCell::new(None) };
    /// The pictures of stacks, cropped to a card's shape (`CardLoader`).
    static CARDS: std::cell::RefCell<Option<Entity<KeptImages>>> =
        const { std::cell::RefCell::new(None) };
}

/// Pictures kept decoded: the `most` drawn last. GPUI's own cache keeps every picture
/// it has loaded, in memory and on the GPU, until the app ends; one that leaves this
/// one goes from there too, and loads again if it's drawn again. With `cards`, they're
/// a stack's, loaded by `CardLoader`.
struct KeptImages {
    most: usize,
    tick: u64,
    items: HashMap<Resource, u64>,
    cards: bool,
}

impl KeptImages {
    fn new(most: usize, cards: bool, cx: &mut App) -> Entity<Self> {
        let e = cx.new(|_| KeptImages {
            most,
            tick: 0,
            items: HashMap::new(),
            cards,
        });
        cx.observe_release(&e, |c, cx| {
            for (resource, _) in c.items.drain() {
                forget_picture(&resource, c.cards, None, cx);
            }
        })
        .detach();
        e
    }
}

/// Lets a picture `KeptImages` had go: off the GPU and out of GPUI's cache, or its load
/// stopped.
fn forget_picture(resource: &Resource, card: bool, window: Option<&mut Window>, cx: &mut App) {
    if card {
        if let Some(Ok(image)) = cx.fetch_asset::<CardLoader>(resource) {
            cx.drop_image(image, window);
        }
        cx.remove_asset::<CardLoader>(resource);
        return;
    }
    if let Some(Ok(image)) = cx.fetch_asset::<PictureLoader>(resource) {
        cx.drop_image(image, window);
    }
    cx.remove_asset::<PictureLoader>(resource);
}

impl ImageCache for KeptImages {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.tick += 1;
        if self.items.insert(resource.clone(), self.tick).is_none() && self.items.len() > self.most
        {
            let oldest = self
                .items
                .iter()
                .min_by_key(|(_, t)| **t)
                .map(|(r, _)| r.clone());
            if let Some(oldest) = oldest {
                self.items.remove(&oldest);
                forget_picture(&oldest, self.cards, Some(window), cx);
            }
        }
        if self.cards {
            return window.use_asset::<CardLoader>(resource, cx);
        }
        window.use_asset::<PictureLoader>(resource, cx)
    }
}

/// The chat's pictures (up to 40 thumbnails, about 25 MB) and the viewer's (the last 3),
/// made the first time.
fn kept_images(cx: &mut App) -> (Entity<KeptImages>, Entity<KeptImages>) {
    if let Some(both) = IMAGES.with(|c| c.borrow().clone()) {
        return both;
    }
    let both = (
        KeptImages::new(40, false, cx),
        KeptImages::new(3, false, cx),
    );
    IMAGES.with(|c| *c.borrow_mut() = Some(both.clone()));
    // A stack's cards: three each, so as many as the chat's pictures.
    let cards = KeptImages::new(40, true, cx);
    CARDS.with(|c| *c.borrow_mut() = Some(cards));
    both
}

/// The pictures of stacks, once `kept_images` has made them.
fn card_images() -> Option<Entity<KeptImages>> {
    CARDS.with(|c| c.borrow().clone())
}

/// The chat's pictures, once `kept_images` has made them.
fn chat_images() -> Option<Entity<KeptImages>> {
    IMAGES.with(|c| c.borrow().as_ref().map(|(chat, _)| chat.clone()))
}

/// A message's text, drawn from the state kept for `key`.
#[derive(IntoElement)]
struct KeptText {
    key: SharedString,
    text: SharedString,
    style: Box<dyn FnOnce(TextView) -> TextView>,
}

impl KeptText {
    fn new(
        key: SharedString,
        text: impl Into<SharedString>,
        style: impl FnOnce(TextView) -> TextView + 'static,
    ) -> Self {
        KeptText {
            key,
            text: text.into(),
            style: Box::new(style),
        }
    }
}

impl RenderOnce for KeptText {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let kept = TEXTS.with(|t| {
            let mut t = t.borrow_mut();
            t.1 += 1;
            let now = t.1;
            t.0.get_mut(&self.key).map(|(state, used)| {
                *used = now;
                state.clone()
            })
        });
        let state = match kept {
            Some(state) => {
                state.update(cx, |s, cx| s.set_text(&self.text, cx));
                state
            }
            None => {
                let text = self.text.clone();
                let state = cx.new(|cx| TextViewState::markdown(&text, cx));
                TEXTS.with(|t| {
                    let mut t = t.borrow_mut();
                    let now = t.1;
                    t.0.insert(self.key.clone(), (state.clone(), now));
                    // The least recently drawn go, a batch at a time.
                    if t.0.len() > KEPT_TEXTS + KEPT_TEXTS / 4 {
                        let mut by_use: Vec<(u64, SharedString)> =
                            t.0.iter().map(|(k, (_, u))| (*u, k.clone())).collect();
                        by_use.sort_unstable();
                        for (_, k) in by_use.into_iter().take(KEPT_TEXTS / 4) {
                            t.0.remove(&k);
                        }
                    }
                });
                state
            }
        };
        (self.style)(TextView::new(&state))
    }
}

/// What drawing one reply's parts needs (`model::parts`).
struct ReplyDraw<'a> {
    p: Palette,
    m: &'a Message,
    tint: Hsla,
    open: &'a HashSet<String>,
    tools: &'a model::Tools,
    files: &'a HashMap<String, String>,
    secrets: &'a HashMap<String, Entity<InputState>>,
    show: Show,
    this: &'a WeakEntity<Loom>,
}

impl ReplyDraw<'_> {
    fn key(&self, i: usize) -> String {
        format!("{}/{}", self.m.id, i)
    }

    fn part(&self, part: &model::Part) -> AnyElement {
        use model::Part;
        match part {
            Part::Bubbles(run) => self.bubbles(run),
            Part::Tools(run) => self.tool_run(run),
            Part::Block(i) => self.block(*i),
            Part::Pictures(run) => {
                let stack: Vec<model::FileRef> = run
                    .iter()
                    .filter_map(|&i| self.m.blocks[i].file.clone())
                    .collect();
                picture_stack(
                    self.p,
                    &stack,
                    self.files,
                    self.key(run[0]),
                    self.show.room,
                    self.this.clone(),
                )
            }
            Part::Fold {
                words,
                working,
                parts,
            } => self.fold(words, *working, parts),
            Part::Quiet(words) => div()
                .text_size(px(size::SMALL))
                .text_color(self.p.ink_3)
                .truncate()
                .child(words.clone())
                .into_any_element(),
        }
    }

    /// Words one after another, a bubble each, joined at their corners as one group.
    fn bubbles(&self, run: &[usize]) -> AnyElement {
        let (p, show) = (self.p, self.show);
        let mut group =
            div()
                .flex()
                .flex_col()
                .gap(px(if show.bubbles { space::XS } else { space::S }));
        for (k, &i) in run.iter().enumerate() {
            let words = div()
                .text_size(px(size::REPLY * show.text_scale))
                .line_height(relative(line::REPLY))
                .text_color(p.ink)
                .child(KeptText::new(
                    SharedString::from(format!("r-{}-{}", self.key(i), show.room)),
                    self.m.blocks[i].text.clone(),
                    |v| v.selectable(true),
                ));
            group = group.child(if show.bubbles {
                // A grey bubble, never the agent's colour, which is the person's: its
                // corner by the thread squared off as a speech bubble's tail, and
                // smaller where it joins the next.
                let c = model::corners(false, model::Joined::at(k, run.len()));
                div().flex().child(
                    words
                        .max_w(relative(0.94))
                        .px(px(space::L))
                        .py(px(space::S + 2.0))
                        .rounded_tl(px(c.tl))
                        .rounded_tr(px(c.tr))
                        .rounded_br(px(c.br))
                        .rounded_bl(px(c.bl))
                        .bg(p.bubble),
                )
            } else {
                words
            });
        }
        group.into_any_element()
    }

    /// A run of tool calls, as the person's prefs show them.
    fn tool_run(&self, run: &[usize]) -> AnyElement {
        let (p, tools) = (self.p, self.tools);
        let blocks: Vec<&model::Block> = run.iter().map(|&i| &self.m.blocks[i]).collect();
        let status = |b: &model::Block| {
            b.call
                .as_ref()
                .and_then(|c| tools.get(c))
                .and_then(|t| t.status.clone())
        };
        match self.show.tools {
            Tools::All | Tools::Hidden => {
                let mut list = div().flex().flex_col().gap(px(space::XS));
                for &i in run {
                    list = list.child(self.tool_entry(i));
                }
                list.into_any_element()
            }
            Tools::Folded => {
                let key = self.key(run[0]);
                let fold = format!("fold/{key}");
                let opened = self.open.contains(&fold);
                let running = blocks
                    .iter()
                    .rev()
                    .find(|b| matches!(status(b).as_deref(), None | Some("running")))
                    .filter(|_| self.m.live);
                let failed = blocks
                    .iter()
                    .filter(|b| matches!(status(b).as_deref(), Some("failed" | "denied")))
                    .count();
                let title = |b: &model::Block| model::tool_title(b.call.as_ref(), tools);
                let words = match running {
                    Some(b) => format!("{}…", title(b)),
                    None if blocks.len() == 1 => format!("Used {}", title(blocks[0])),
                    None => format!("Used {} tools", blocks.len()),
                };
                let this = self.this.clone();
                let line = div()
                    .id(SharedString::from(format!("f-{key}")))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .text_size(px(size::SMALL))
                    .text_color(p.ink_3)
                    .cursor_pointer()
                    .soft_ink(p.ink_3, p.ink)
                    .child(if opened { "⌄" } else { "›" })
                    .child(div().truncate().child(words))
                    .when(failed > 0, |d| {
                        d.child(div().text_color(p.bad).child(format!("{failed} failed")))
                    })
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |loom, cx| loom.toggle_thought(fold.clone(), cx));
                    });
                let mut out = div().flex().flex_col().gap(px(space::S)).child(line);
                if opened {
                    let mut list = div()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .pl(px(space::M))
                        .border_l_1()
                        .border_color(p.line_strong);
                    for &i in run {
                        list = list.child(self.tool_entry(i));
                    }
                    out = out.child(list);
                }
                out.into_any_element()
            }
        }
    }

    /// One tool call. A subagent's call holds what its subagent did: its line says how
    /// many steps, and opens with a click to show them inside it.
    fn tool_entry(&self, i: usize) -> AnyElement {
        let p = self.p;
        let b = &self.m.blocks[i];
        let shows = model::Shows {
            tools: true,
            thoughts: self.show.thoughts != Thoughts::Hidden,
        };
        let (parts, n) = model::inside(self.m, i, shows);
        if n == 0 {
            return tool_line(p, b, self.tools, self.m.live).into_any_element();
        }
        let key = format!("sub/{}", self.key(i));
        let opened = self.open.contains(&key);
        let this = self.this.clone();
        let k = key.clone();
        let line = tool_line(p, b, self.tools, self.m.live)
            .id(SharedString::from(format!("u-{}", self.key(i))))
            .cursor_pointer()
            .child(div().flex_none().text_color(p.ink_3).child(if n == 1 {
                "1 step".to_string()
            } else {
                format!("{n} steps")
            }))
            .child(
                div()
                    .flex_none()
                    .text_color(p.ink_3)
                    .child(if opened { "⌄" } else { "›" }),
            )
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
            });
        let mut out = div().flex().flex_col().gap(px(space::XS)).child(line);
        if opened {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .pl(px(space::M))
                .border_l_1()
                .border_color(p.line_strong);
            for part in &parts {
                list = list.child(match part {
                    model::Part::Bubbles(run) => self.said_on_the_way(run),
                    model::Part::Tools(run) => {
                        let mut calls = div().flex().flex_col().gap(px(space::XS));
                        for &j in run {
                            calls = calls.child(self.tool_entry(j));
                        }
                        calls.into_any_element()
                    }
                    _ => self.part(part),
                });
            }
            out = out.child(list);
        }
        out.into_any_element()
    }

    /// The turn's steps under one quiet line, which opens to show them: while it runs,
    /// the step it's on, and once it's done, how many there were.
    fn fold(&self, words: &str, working: bool, parts: &[model::Part]) -> AnyElement {
        let p = self.p;
        let key = format!("steps/{}", self.m.id);
        let can_open = !parts.is_empty();
        let opened = can_open && self.open.contains(&key);
        let this = self.this.clone();
        let k = key.clone();
        let line = div()
            .id(SharedString::from(format!("s-{}", self.m.id)))
            .flex()
            .items_center()
            .gap(px(space::S))
            .min_w_0()
            .text_size(px(size::SMALL))
            .text_color(p.ink_3)
            .when(can_open, |d| {
                d.cursor_pointer()
                    .soft_ink(p.ink_3, p.ink)
                    .child(if opened { "⌄" } else { "›" })
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
                    })
            })
            .child(div().min_w_0().truncate().child(words.to_string()))
            .when(working && !words.ends_with('…'), |d| d.child("…"));
        let mut out = div().flex().flex_col().gap(px(space::S)).child(line);
        if opened {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .pl(px(space::M))
                .border_l_1()
                .border_color(p.line_strong);
            for part in parts {
                list = list.child(match part {
                    model::Part::Bubbles(run) => self.said_on_the_way(run),
                    _ => self.part(part),
                });
            }
            out = out.child(list);
        }
        out.into_any_element()
    }

    /// Words the agent wrote on its way to the answer, among the steps: smaller and quieter
    /// than the answer, and never a bubble whatever the person's prefs, so they don't read
    /// as a message in the chat, its or the person's.
    fn said_on_the_way(&self, run: &[usize]) -> AnyElement {
        let (p, show) = (self.p, self.show);
        let mut group = div().flex().flex_col().gap(px(space::XS));
        for &i in run {
            group = group.child(
                div()
                    .text_size(px(size::SMALL * show.text_scale))
                    .line_height(relative(line::REPLY))
                    .text_color(p.ink_2)
                    .child(KeptText::new(
                        SharedString::from(format!("w-{}-{}", self.key(i), show.room)),
                        self.m.blocks[i].text.clone(),
                        |v| v.selectable(true),
                    )),
            );
        }
        group.into_any_element()
    }

    /// A block on its own: a thought, a plan, a notice, a file or a card.
    fn block(&self, i: usize) -> AnyElement {
        let (p, show, tint) = (self.p, self.show, self.tint);
        let b = &self.m.blocks[i];
        let key = self.key(i);
        match b.kind.as_str() {
            "thought" => {
                // A key in `open` turns it the other way from the person's prefs.
                let shown = self.open.contains(&key) != (show.thoughts == Thoughts::Open);
                let this = self.this.clone();
                let k = key.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .id(SharedString::from(format!("t-{key}")))
                            .text_size(px(size::CAPTION))
                            .soft_ink(p.ink_3, p.ink)
                            .cursor_pointer()
                            .child(if shown { "⌄ Thought" } else { "› Thought" })
                            .on_click(move |_, _, cx| {
                                let _ =
                                    this.update(cx, |loom, cx| loom.toggle_thought(k.clone(), cx));
                            }),
                    )
                    .when(shown, |d| {
                        d.child(
                            div()
                                .pl(px(space::M))
                                .border_l_1()
                                .border_color(p.line_strong)
                                .text_size(px(size::SMALL * show.text_scale))
                                .text_color(p.ink_3)
                                .child(b.text.clone()),
                        )
                    })
                    .into_any_element()
            }
            "plan" => {
                // Its steps, a line each, marked as the checklist card marks them: done
                // in the agent's colour, the one it's on in ink, the rest waiting.
                let mut steps = div().flex().flex_col().gap(px(space::XS));
                for line in b.text.lines().filter(|l| !l.trim().is_empty()) {
                    let (state, what) = line.split_once(' ').unwrap_or(("todo", line));
                    let mark = div()
                        .size(px(14.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1();
                    let mark = match state {
                        "done" => mark
                            .border_color(tint)
                            .bg(tint)
                            .text_color(motion::ink_on(tint, p.button_ink, p.button))
                            .child(Icon::new(Ico::Check).xsmall()),
                        "doing" => mark
                            .border_color(tint)
                            .child(div().size(px(6.0)).rounded_full().bg(tint)),
                        _ => mark.border_color(p.line_strong),
                    };
                    steps = steps.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .text_size(px(size::SMALL * show.text_scale))
                            .text_color(match state {
                                "done" => p.ink_3,
                                "doing" => p.ink,
                                _ => p.ink_2,
                            })
                            .child(mark)
                            .child(div().flex_1().min_w_0().child(what.to_string())),
                    );
                }
                steps.into_any_element()
            }
            // One quiet line from loom, such as that another program took the chat
            // over. It isn't the reply, so what it's doing still shows under it.
            "notice" => div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child(Icon::new(Ico::ArrowRightLeft).small())
                .child(b.text.clone())
                .into_any_element(),
            "file" => match &b.file {
                Some(f) => file_view(p, f, self.files, key, show.room, self.this.clone()),
                None => div().into_any_element(),
            },
            "card" => match &b.card {
                Some(c) => card_view(p, c, &key, tint, show, self.secrets, self.this.clone())
                    .into_any_element(),
                None => div().into_any_element(),
            },
            _ => div().into_any_element(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reply(
    p: Palette,
    m: &Message,
    name: &str,
    tint: Hsla,
    mood: Mood,
    head: bool,
    open: &HashSet<String>,
    tools: &model::Tools,
    files: &HashMap<String, String>,
    secrets: &HashMap<String, Entity<InputState>>,
    show: Show,
    this: WeakEntity<Loom>,
) -> Div {
    let mut body = div().flex().flex_col().gap(px(space::S)).flex_1().min_w_0();
    // Its agent's name, over the first reply after a time line (`model::flow`).
    if head {
        body = body.child(
            div()
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::MEDIUM)
                .text_color(p.ink)
                .child(name.to_string()),
        );
    }
    // A routine's post: which routine it's from, as a person's message from another agent
    // says who sent it.
    if let Some(from) = m.routine_line() {
        body = body.child(
            div()
                .text_size(px(size::CAPTION))
                .text_color(p.ink_3)
                .child(from),
        );
    }
    // What shows in the reply, and how it reads: its answer, and its steps folded
    // under one line (`model::parts`).
    let shows = model::Shows {
        tools: show.tools != Tools::Hidden,
        thoughts: show.thoughts != Thoughts::Hidden,
    };
    let drawing = ReplyDraw {
        p,
        m,
        tint,
        open,
        tools,
        files,
        secrets,
        show,
        this: &this,
    };
    for part in model::parts(m, tools, shows) {
        body = body.child(drawing.part(&part));
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
                    .soft_hover(p.raised, p.hover, None)
                    .hover(move |s| s.border_color(p.line_strong))
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
    // Its thread beside the name, or else level with the first line of words, plain or
    // in a bubble.
    let first_line = (size::REPLY * show.text_scale * line::REPLY - 10.0) / 2.0;
    let thread_at = match (head, show.bubbles) {
        (true, _) => 4.0,
        (false, true) => 16.0,
        (false, false) => first_line,
    };
    div()
        .w_full()
        .flex()
        .gap(px(space::M))
        .child(div().pt(px(thread_at)).child(thread(
            format!("msg-{}", m.id),
            mood,
            tint,
            14.0,
            10.0,
            None,
        )))
        .child(body)
}

/// The icon of a question's card: what kind of call it is, as the core says (Code Mode's
/// script, a tool of an MCP server or a plugin, one of the person's devices or hosts),
/// or else the kind of tool.
fn ask_icon(a: &Approval) -> Ico {
    match (a.shown.what.as_str(), a.kind.as_str()) {
        ("script", _) | (_, "execute") => Ico::SquareTerminal,
        ("mcp", _) => Ico::Plug,
        ("plugin", _) => Ico::Puzzle,
        ("device", _) => Ico::Laptop,
        ("install", _) => Ico::Server,
        (_, "delete") => Ico::Trash,
        (_, "edit") => Ico::Pencil,
        (_, "move") => Ico::Truck,
        (_, "fetch") => Ico::Globe,
        (_, "read") => Ico::FileText,
        (_, "search") => Ico::Search,
        _ => Ico::ShieldCheck,
    }
}

/// A question the agent waits on (PLAN.md, "Permissions"), in the card frame: what it
/// would do as the core put it in words (`core/asks.calx`), with Details for all of it,
/// then Allow once, Always allow where it's offered, and Deny. Answered here, it's one
/// quiet line saying how, until the instance takes the question away.
fn approval(
    p: Palette,
    a: &Approval,
    tint: Hsla,
    open: &HashSet<String>,
    scale: f32,
    this: WeakEntity<Loom>,
) -> Div {
    let title = if a.title.is_empty() {
        "Can it go ahead?".to_string()
    } else {
        format!("{}?", a.title)
    };
    let place = div().ml(px(26.0)).max_w(px(CARD_MOST + 26.0));
    if let Some((allow, always)) = a.answered {
        let (icon, how) = match (allow, always) {
            (false, _) => (Ico::X, "Denied"),
            (true, true) => (Ico::Check, "Always allowed"),
            (true, false) => (Ico::Check, "Allowed once"),
        };
        let words = if a.title.is_empty() {
            how.to_string()
        } else {
            format!("{how} · {}", a.title)
        };
        return place.child(card_done(p, icon, words, scale));
    }
    let call = a.call.clone();
    let to_answer = this.clone();
    let answer = move |allow: bool, always: bool| {
        let this = to_answer.clone();
        let call = call.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |loom, cx| loom.answer(call.clone(), allow, always, cx));
        }
    };
    let mut card = CardFrame::new()
        .lead(card_icon(p, ask_icon(a), tint))
        .title(title);
    if !a.shown.lines.is_empty() {
        card = card.row(card_lines(p, &a.shown.lines, scale));
    }
    if !a.shown.code.is_empty() {
        card = card.row(card_code(p, a.shown.code.clone(), scale));
    }
    if !a.shown.raw.is_empty() {
        let key = format!("details/{}", a.call);
        let opened = open.contains(&key);
        card = card.row(card_details(
            p,
            key,
            opened,
            &a.shown.raw,
            scale,
            this.clone(),
        ));
    }
    card = card.action(
        card_button(p, format!("allow-{}", a.call), "Allow once", true)
            .on_click(answer(true, false)),
    );
    if a.can_always {
        card = card.action(
            card_button(p, format!("always-{}", a.call), "Always allow", false)
                .on_click(answer(true, true)),
        );
    }
    card = card.action(
        card_button(p, format!("deny-{}", a.call), "Deny", false).on_click(answer(false, false)),
    );
    place.child(card.draw(p, scale))
}

/// A button's name, shown under the pointer after a moment, for buttons that are only an icon.
fn tip(words: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(words).build(window, cx)
}

/// As `tip`, for words made as the app runs, such as an agent's name on the rail.
fn tip_text(words: String) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let words = SharedString::from(words);
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(words.clone()).build(window, cx)
}

/// The dot on the rail at a thread's or a yarn's corner: something said there that
/// wasn't seen yet.
fn rail_dot(p: Palette) -> Div {
    div()
        .absolute()
        .top(px(3.0))
        .right(px(3.0))
        .size(px(6.0))
        .rounded_full()
        .bg(p.ink)
}

/// The day of `ms`, as the activity page heads its entries: "Today", "Yesterday", or a date.
/// The day `ms` falls on in the person's time zone, as a number of days.
fn day_number(ms: i64) -> i64 {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms) {
        chrono::LocalResult::Single(t) => i64::from(chrono::Datelike::num_days_from_ce(&t)),
        _ => 0,
    }
}

/// A time line's words: "Today 14:05", "Yesterday 09:30", "21 Nov 14:05".
fn line_of(ms: i64, now: chrono::DateTime<chrono::Local>) -> String {
    let when = when_sent(ms, now);
    if day_number(ms) == day_number(now.timestamp_millis()) {
        format!("Today {when}")
    } else {
        when
    }
}

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

/// What a message to a busy agent does, in words.
fn busy_name(busy: &str) -> &'static str {
    if busy == "steer" {
        "Steers at once"
    } else {
        "Waits its turn"
    }
}

/// A value of an option by its name, or as it is when the option doesn't name it.
fn value_name(o: &model::ConfigOption, value: &str) -> String {
    o.values
        .iter()
        .find(|(v, _)| v == value)
        .map_or_else(|| value.to_string(), |(_, name)| name.clone())
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
mod bubble_tests {
    use super::{person_bubble, Palette};
    use crate::{motion, thread};

    #[test]
    fn the_persons_words_read_on_their_bubble_in_every_colour() {
        let names = [
            "periwinkle",
            "mint",
            "apricot",
            "pink",
            "sky",
            "gold",
            "lavender",
            "grey",
            "coral",
            "moss",
            "teal",
            "plum",
            "#ffee00",
            "#000080",
        ];
        let contrast = |a, b| {
            let (x, y) = (motion::luminance(a), motion::luminance(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        };
        for dark in [false, true] {
            let p = Palette::new(dark);
            for name in names {
                for vibrant in [false, true] {
                    let bg = person_bubble(p, thread::color(name, dark), vibrant);
                    let ink = motion::ink_on(bg, p.ink, p.bg);
                    let c = contrast(bg, ink);
                    assert!(c >= 4.5, "{name}, dark {dark}, vibrant {vibrant}: {c}");
                }
            }
        }
    }
}
#[cfg(test)]
mod command_tests {
    #[test]
    fn a_tools_name_reads_as_words() {
        use super::readable_name;
        assert_eq!(readable_name("show_card"), "Show card");
        assert_eq!(readable_name("browser_open"), "Browser open");
        assert_eq!(
            readable_name("Clean out the old build folder"),
            "Clean out the old build folder"
        );
        assert_eq!(readable_name("notes.md"), "notes.md");
        assert_eq!(readable_name("Made Button"), "Made Button");
    }
}

#[cfg(test)]
mod picture_tests {
    use super::{card_crop, fit, picture_size, PICTURE_H, PICTURE_W};

    #[test]
    fn a_stacks_card_is_the_middle_of_its_picture() {
        // 4:3 already: all of it.
        assert_eq!(card_crop(512, 384), (0, 0, 512, 384));
        // Wide: its middle across. Tall: its middle down.
        assert_eq!(card_crop(1000, 300), (300, 0, 400, 300));
        assert_eq!(card_crop(300, 800), (0, 287, 300, 225));
        assert_eq!(card_crop(0, 0), (0, 0, 1, 1));
    }

    #[test]
    fn a_picture_keeps_its_shape_inside_its_place() {
        // A tall screenshot, as a 512 px thumbnail: as tall as the place lets it be.
        assert_eq!(fit(307, 512, PICTURE_W, PICTURE_H), (240.0, 400.0));
        assert_eq!(fit(1170, 2532, PICTURE_W, PICTURE_H), (185.0, 400.0));
        // A wide one fills the width, and a small one keeps its own size.
        assert_eq!(fit(2000, 600, PICTURE_W, PICTURE_H), (360.0, 108.0));
        assert_eq!(fit(60, 40, PICTURE_W, PICTURE_H), (60.0, 40.0));
        // A narrow column takes it down further, and a broken size still has a place.
        assert_eq!(fit(512, 512, 200.0, PICTURE_H), (200.0, 200.0));
        assert_eq!(fit(0, 0, PICTURE_W, PICTURE_H), (1.0, 1.0));
    }

    #[test]
    fn a_pictures_size_is_read_from_its_file() {
        let dir = std::env::temp_dir().join(format!("loom-picture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Kept as a thumbnail is, with no extension to say what it is.
        let path = dir.join("tall.thumb");
        image::RgbImage::new(30, 50)
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(picture_size(&path.to_string_lossy()), Some((30, 50)));
        assert_eq!(picture_size(&dir.join("gone").to_string_lossy()), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod frame_tests {
    use super::{jpeg_file, jpeg_pixels, picture_pixels, picture_size};

    /// The pixel at (x, y) of a frame, as red, green and blue.
    fn rgb(frame: &image::RgbaImage, x: u32, y: u32) -> [u8; 3] {
        let [b, g, r, a] = frame.get_pixel(x, y).0;
        assert_eq!(a, 255);
        [r, g, b]
    }

    fn near(got: [u8; 3], want: [u8; 3]) -> bool {
        got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 12)
    }

    /// Frames of the same page, a dark bar over red, green, blue and white boxes on
    /// #fafaf8, from the screencast of Chrome for Testing 159 on Linux with --headless=new,
    /// as a server runs it, in three scans, and of Chromium on Windows, in one.
    #[test]
    fn a_browser_frame_shows_the_page() {
        for (name, jpeg, size) in [
            (
                "linux",
                &include_bytes!("../testdata/frame-chrome-linux.jpg")[..],
                (1280, 657),
            ),
            (
                "windows",
                &include_bytes!("../testdata/frame-chrome-windows.jpg")[..],
                (1264, 649),
            ),
        ] {
            let frame = jpeg_pixels(jpeg).unwrap_or_else(|| panic!("the {name} frame decodes"));
            shows_the_page(name, &frame, size);
        }
    }

    /// Whether `frame` is the page in the test frames, `size` big.
    fn shows_the_page(name: &str, frame: &image::RgbaImage, size: (u32, u32)) {
        assert_eq!(frame.dimensions(), size, "{name}");
        let mut want = vec![
            ("bar", 600, 60, [0x20, 0x21, 0x24]),
            ("red", 70, 140, [255, 0, 0]),
            ("green", 200, 140, [0, 255, 0]),
            ("blue", 330, 140, [0, 0, 255]),
            ("white", 460, 140, [255, 255, 255]),
        ];
        // Every third row of the page below the boxes, where the stripes were.
        want.extend(
            (260..size.1 - 8)
                .step_by(3)
                .map(|y| ("page", 1000, y, [0xfa, 0xfa, 0xf8])),
        );
        for (what, x, y, color) in want {
            let got = rgb(frame, x, y);
            assert!(near(got, color), "{name}: {what} at ({x}, {y}) is {got:?}");
        }
    }

    /// A screenshot an agent took with Chrome on Linux, sent in a chat: the picture in the
    /// message, its thumbnail, the viewer's and the composer's all draw it whole.
    #[test]
    fn a_picture_in_a_chat_written_in_three_scans_shows_whole() {
        let jpeg = include_bytes!("../testdata/frame-chrome-linux.jpg");
        let dir = std::env::temp_dir().join(format!("loom-jpeg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Kept as a chat's file is, by its SHA-256 with no extension.
        let path = dir.join("3a1f.thumb");
        std::fs::write(&path, jpeg).unwrap();
        let drawn = jpeg_file(&path).expect("a JPEG file loads as a JPEG");
        let at = drawn.size(0);
        let (w, h) = (at.width.0 as u32, at.height.0 as u32);
        let pixels = image::RgbaImage::from_raw(w, h, drawn.as_bytes(0).unwrap().to_vec());
        shows_the_page("in a chat", &pixels.unwrap(), (1280, 657));
        assert_eq!(picture_size(&path.to_string_lossy()), Some((1280, 657)));
        // Attached in the composer, for its chip.
        shows_the_page("attached", &picture_pixels(jpeg).unwrap(), (1280, 657));
        // Any other kind of picture is GPUI's loader's.
        let png = dir.join("small.thumb");
        image::RgbImage::new(4, 4)
            .save_with_format(&png, image::ImageFormat::Png)
            .unwrap();
        assert!(jpeg_file(&png).is_none());
        assert_eq!(
            picture_pixels(&std::fs::read(&png).unwrap())
                .unwrap()
                .dimensions(),
            (4, 4)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A phone's photo, kept on its side with EXIF saying to turn it, is drawn upright,
    /// as GPUI's loader drew it.
    #[test]
    fn a_photo_is_turned_as_its_exif_says() {
        use image::ImageEncoder as _;
        // Red then blue, side by side, 16 pixels each, and "turn 90° clockwise".
        let mut wide = image::RgbImage::from_pixel(32, 16, image::Rgb([255, 0, 0]));
        for x in 16..32 {
            for y in 0..16 {
                wide.put_pixel(x, y, image::Rgb([0, 0, 255]));
            }
        }
        let exif = vec![
            0x49, 0x49, 0x2a, 0, 8, 0, 0, 0, 1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0,
            0, 0,
        ];
        let mut jpeg = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95);
        encoder.set_exif_metadata(exif).unwrap();
        encoder
            .write_image(&wide, 32, 16, image::ExtendedColorType::Rgb8)
            .unwrap();
        let upright = jpeg_pixels(&jpeg).unwrap();
        assert_eq!(upright.dimensions(), (16, 32));
        // Red on top, blue under it.
        assert!(
            near(rgb(&upright, 8, 4), [255, 0, 0]),
            "{:?}",
            rgb(&upright, 8, 4)
        );
        assert!(
            near(rgb(&upright, 8, 28), [0, 0, 255]),
            "{:?}",
            rgb(&upright, 8, 28)
        );
    }

    #[test]
    fn what_isnt_a_jpeg_is_skipped() {
        assert!(jpeg_pixels(b"not a picture").is_none());
        assert!(picture_pixels(b"not a picture").is_none());
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
        crate::perf::frame_start();
        self.frames += 1;
        let p = self.p;
        // Before anything is drawn from it: which server is on screen.
        self.settle_server(cx);
        self.finish_weaving(window, cx);
        if std::mem::take(&mut self.rows_dirty) {
            self.refresh_rows(cx);
        }
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
        if std::mem::take(&mut self.model.defaults_came) {
            let text = self.model.defaults.instructions.clone();
            self.every_in
                .update(cx, |s, cx| s.set_value(text, window, cx));
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
        let threads_move = self
            .model
            .agents
            .iter()
            .any(|a| thread::moves(self.model.mood(&a.id), self.model.finished_at(&a.id)));
        if self.model.conn == Some(Conn::SignedOut)
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
            // The agent's edit panel beside the chat, whose thread fidgets at its top.
            // The other panels have no thread, so they draw only when something changes.
            || self.side == Some(Screen::EditAgent) && self.screen == Screen::Chat
            // Someone typing to an agent, whose thread thinks along: only on the empty
            // chat's hero, where that thread is drawn.
            || self.screen == Screen::Chat
                && self.rows.is_empty()
                && self.model.task_open().is_none()
                && self.model.open_branch.is_none()
                && !self.composer.read(cx).value().trim().is_empty()
        {
            crate::perf::count("want:app");
            window.request_animation_frame();
        }
        if threads_move && !self.thread_tick {
            // Agents' threads move, small ones beside their names and the live reply's:
            // 30 frames a second is as smooth for them, and the window isn't drawn again
            // at the screen's 144–240 Hz while an agent works. The sidebar, drawn from a
            // cache, is drawn again with them.
            self.thread_tick = true;
            crate::perf::count("want:threads");
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                let _ = this.update(cx, |loom, cx| {
                    loom.thread_tick = false;
                    if let Some(v) = loom.side_view.clone() {
                        v.update(cx, |_, cx| cx.notify());
                    }
                    cx.notify();
                });
            })
            .detach();
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
            // While a picture is open big, ← and → go between the chat's pictures, before
            // the composer can take them.
            .capture_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                if this.model.viewing.is_some() {
                    match e.keystroke.key.as_str() {
                        "left" => this.view_step(-1, cx),
                        "right" => this.view_step(1, cx),
                        _ => return,
                    }
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
                let k = &e.keystroke;
                // Ctrl+B and Ctrl+F (Cmd on a Mac), only where the sidebar is.
                let chord = k.modifiers.secondary(cx)
                    && !k.modifiers.alt
                    && !k.modifiers.shift
                    && this.scene.0 == "app";
                if k.key == "escape" {
                    this.escape(window, cx);
                } else if chord && k.key == "b" {
                    this.toggle_bar(window, cx);
                    cx.stop_propagation();
                } else if chord && k.key == "f" {
                    this.find(window, cx);
                    cx.stop_propagation();
                }
            }))
            .relative()
            .size_full()
            .bg(p.bg)
            .text_color(p.ink)
            .font_family(crate::prefs::ui_font())
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
            let toast = (scene != "blank" && !signed_out)
                .then(|| self.toast(window, cx))
                .flatten();
            return root
                .flex()
                .flex_col()
                .child(self.title_bar(window, div(), transparent_black()))
                .child(div().flex_1().min_h_0().flex().flex_col().child(page))
                .when_some(toast, |d, t| d.child(t))
                .when_some(splash, |d, s| d.child(s))
                .child(motion_ticker());
        }
        // The main area is the chat, unless an agent is being made.
        let main = match self.screen {
            Screen::NewAgent => self.new_agent(cx),
            Screen::Chat if self.model.agents.is_empty() && self.model.synced => {
                self.first_agent(cx)
            }
            _ => crate::perf::step("chat", || self.chat(window, cx)),
        };
        self.sync_panel(window, cx);
        self.swap_draft(window, cx);
        self.edit_composer(window, cx);
        self.sync_server_name(window, cx);
        let ww = window.viewport_size().width.as_f32();
        if self.bar.fit(ww, self.beside()) {
            cx.notify();
        }
        let railed = self.railed(ww);
        self.lights = cfg!(target_os = "macos") && !lights_gone(window);
        // Beside the rail, a Mac's traffic lights reach past it, so the title bar has the
        // control that opens the sidebar after them, its icon where the knot is in the
        // whole sidebar, and then the header.
        let header = if railed && self.lights {
            let inset = (control::ICON_SMALL - ICON_GLYPH) / 2.0;
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .pl(px(LIGHTS_END - inset - layout::RAIL))
                .child(self.bar_button(true, cx))
                .child(self.header(cx).pl(px(space::XS)))
        } else {
            self.header(cx)
        };
        // Drawn last, over the sheet that takes the pointer while it's open.
        // From a right-click in the sidebar, it opens at the pointer instead.
        let menu = self.menu_panel(cx).map(|m| match self.menu_at {
            Some(at) => deferred(
                anchored()
                    .position(at)
                    .snap_to_window_with_margin(px(space::S))
                    .child(m),
            )
            .into_any_element(),
            None => deferred(m).into_any_element(),
        });
        let menu_scrim = self.menu_scrim(cx);
        let ctx_menu = self.ctx_menu(cx);
        let popup = self.popup_view(window, cx);
        let browser_full = self.browser_full(window, cx);
        let viewer = self.viewer(window, cx);
        let toast = self.toast(window, cx);
        let out = root
            .flex()
            .child(self.wheel_catcher(cx))
            .when(self.side_drag.is_some(), |d| {
                d.child(self.side_dragger(cx)).cursor_col_resize()
            })
            .child(self.sidebar(railed, window.is_a11y_active(), cx))
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
            // From the rail, the update popup comes out beside it, past the rail's edge.
            .when_some(
                railed.then(|| self.update_pop(true, cx)).flatten(),
                |d, pop| d.child(pop),
            )
            .when_some(popup, |d, p| d.child(p))
            .when_some(browser_full, |d, b| d.child(b))
            .when_some(viewer, |d, v| d.child(v))
            .when_some(toast, |d, t| d.child(t))
            .when_some(menu_scrim, |d, s| d.child(s))
            .when_some(ctx_menu, |d, m| d.child(m))
            .when_some(whole, |d, c| d.child(c))
            .when_some(splash, |d, s| d.child(s))
            .child(motion_ticker());
        crate::perf::render_end();
        out
    }
}

/// Drawn last of all, after every list has drawn its rows: while a switch is sliding
/// or a hover fading, it asks for the next frame, with the cached views it's in built
/// again.
fn motion_ticker() -> impl IntoElement {
    canvas(
        |_, _, _| {},
        |_, _, window, _| {
            crate::perf::frame_end();
            let moving = crate::motion::moving();
            if moving.any {
                crate::perf::count("want:motion");
                window.request_animation_frame();
            }
            if !moving.views.is_empty() {
                window.on_next_frame(move |_, cx| {
                    for view in moving.views {
                        cx.notify(view);
                    }
                });
            }
        },
    )
    .absolute()
    .size_0()
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

/// Who a program or its settings go to, in words: everyone, only the owner, or names.
fn who_words(members: Option<&[String]>, people: &[model::Person]) -> String {
    let Some(ids) = members else {
        return "Everyone you invited".into();
    };
    let names: Vec<&str> = people
        .iter()
        .filter(|person| ids.contains(&person.id))
        .map(|person| person.username.as_str())
        .collect();
    match names.as_slice() {
        [] => "Only you".into(),
        [one] => format!("Only {one}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
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
const MOVE_PROMPT: &str = "Move my loom instance to another server for me. Ask me where it should go (a server I can reach, or one to rent) if I haven't said. If loom isn't on it yet, use install_server: with the host, if it's one of my hosts (list_devices, kind host), which installs it on my one yes, and otherwise to get the command that puts it there, and run it or walk me through it. Then use move_instance to move this instance there, and tell me when it's done and what changed for me.";

/// What "Copy prompt" gives for adding a server, for an agent in loom to set it up with
/// its tools (install_server, add_server).
const ADD_SERVER_PROMPT: &str = "Set up a new loom server for me and add it to my app. Ask me where it should run (a server I can reach, or a VPS to rent) if I haven't said. If it's one of my hosts (list_devices, kind host), use install_server with that host, and import openbot if OpenBot runs there: it shows me the plan, and my one yes installs loom, brings OpenBot over and signs my app in. Otherwise use install_server to get the command that installs loom there, run it or walk me through it, and use add_server with its address and the link code loom install --json gave. Tell me what's done and what to do next.";

/// What "Copy prompt" gives on the Plugins page, for an agent in loom to build one with
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

/// What picking one of an agent's choices does: the frame for it, given the value, or
/// None to follow the person's default.
type Pick = Rc<dyn Fn(&mut Loom, Option<&str>) -> Option<Value>>;

/// What each of the eight shapes is called on an agent's edit panel, in the order of
/// `thread_shape::NAMES`.
const SHAPE_TITLES: [&str; 8] = [
    "Loop", "Coil", "Bow", "Heart", "Star", "Knot", "Snail", "Yarn",
];

/// Every colour an agent's thread can have (design/tokens.toml), with its name, for
/// its edit panel. An agent can also have its own, as #rrggbb.
const THREAD_COLORS: [(&str, &str); 12] = [
    ("periwinkle", "Periwinkle"),
    ("lavender", "Lavender"),
    ("plum", "Plum"),
    ("pink", "Pink"),
    ("coral", "Coral"),
    ("apricot", "Apricot"),
    ("gold", "Gold"),
    ("moss", "Moss"),
    ("mint", "Mint"),
    ("teal", "Teal"),
    ("sky", "Sky"),
    ("grey", "Grey"),
];

/// A small thing coming in, `k` of the way (0 to 1): a button, a tile, a line of text.
/// Whole screens never fade; they change through the threads.
fn appear<E: Styled>(d: E, k: f32) -> E {
    d.opacity(k)
}

/// How long the opening weave lasts on every launch, in seconds.
const SPLASH_SECONDS: f32 = 1.6;

/// How long a notice stays, unless it's clicked away.
const NOTICE_SECONDS: f32 = 8.0;
/// How far past a message its hover reaches, for its time on the left and its buttons on
/// the right.
const HOVER_REACH: f32 = 34.0;

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
/// A tool's name as the person reads it: `show_card` is "Show card". Anything already in
/// words, with a space in it or no underscore, is left as it is.
fn readable_name(text: &str) -> String {
    let id = !text.is_empty()
        && text.contains('_')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
    if !id {
        return text.to_string();
    }
    let words = text
        .split(['_', '.'])
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    program_title(&words)
}

fn program_title(name: &str) -> String {
    let mut c = name.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Whether this computer runs its own instance, as the person chose on the first start.
/// Whether the first server is the instance on this computer: the `mode` file, read
/// once, since a frame asks several times. 0 is not read yet, 1 local, 2 not.
static LOCAL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn local_mode() -> bool {
    use std::sync::atomic::Ordering;
    match LOCAL.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let local = std::fs::read_to_string(std::path::Path::new(&data_dir()).join("mode"))
                .is_ok_and(|m| m.trim() == "local");
            LOCAL.store(if local { 1 } else { 2 }, Ordering::Relaxed);
            local
        }
    }
}

/// The core's config (core/README.md, "Config"), for the server whose folder is `dir`
/// ("" for the first), and for a new one, its address. Only the first updates the
/// app, and only it can be the instance on this computer.
/// The first server's core, started before the window is made, so it boots (and an
/// instance in the app opens) while GPUI starts, rather than after.
static FIRST_CORE: std::sync::Mutex<Option<(Core, std::sync::mpsc::Receiver<Value>)>> =
    std::sync::Mutex::new(None);

pub fn start_first_core() {
    crate::perf::once("core starting");
    let started = Core::start(&core_config("", None));
    crate::perf::once("core started");
    if let Ok(mut c) = FIRST_CORE.lock() {
        *c = Some(started);
    }
}

/// `loom mcp` alone (server/mcp_main.calx), built with the app by build.rs.
static MCP_PROGRAM: &[u8] = include_bytes!(env!("LOOM_MCP_PROGRAM"));

/// Where the instance in the app finds the small `loom mcp` it gives each agent session in
/// place of the app itself, which takes twice as long to start: written into `folder`'s
/// bin/ once for each build of it, with the older builds' let go. None if it can't be.
fn mcp_program(folder: &std::path::Path) -> Option<String> {
    let name = format!(
        "loom-mcp-{}{}",
        env!("LOOM_MCP_HASH"),
        std::env::consts::EXE_SUFFIX
    );
    let dir = folder.join("bin");
    let path = dir.join(&name);
    if !path.exists() {
        std::fs::create_dir_all(&dir).ok()?;
        let part = dir.join(format!("{name}.part"));
        std::fs::write(&part, MCP_PROGRAM).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).ok()?;
        }
        std::fs::rename(&part, &path).ok()?;
        // An older build's, unless a session still runs it.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let other = e.file_name().to_string_lossy().into_owned();
                if other.starts_with("loom-mcp-") && other != name {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    Some(path.to_string_lossy().into_owned())
}

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
    // The first core gathers what's behind across the app, for the update icon.
    config["hub"] = true.into();
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
    // For trying things in a build from source: the key a test's releases are signed
    // with, and a device token to start with. A release never reads them.
    if crate::VERSION.contains("-dev") {
        if let Ok(key) = std::env::var("LOOM_UPDATE_KEY") {
            config["update_key"] = key.into();
        }
        if let Ok(token) = std::env::var("LOOM_TOKEN") {
            config["token"] = token.into();
        }
    }
    // What the core fetches for an update: the release's installer, which swaps loom.exe
    // in once the app has quit, and can put the old one back (desktop/setup); on a Mac,
    // the release's disk image, whose loom puts itself in place (swap.rs).
    config["platform"] = if cfg!(target_os = "macos") {
        "macos-desktop"
    } else {
        "windows-installer"
    }
    .into();
    // Where this app can't update itself, as from its disk image, the update icon says why.
    if let Some(why) = shell::cant_update() {
        config["app_why"] = why.into();
    }
    if local_mode() {
        config["mode"] = "local".into();
        if let Some(mcp) = mcp_program(&folder) {
            config["mcp"] = mcp.into();
        }
        let user = if cfg!(windows) { "USERNAME" } else { "USER" };
        config["username"] = std::env::var(user).unwrap_or_else(|_| "me".into()).into();
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
            | Screen::Hosts
            | Screen::Move
            | Screen::AddServer
    )
}

/// The height of the head of the panel beside the chat, and of the popup's page.
const SIDE_HEAD: f32 = 48.0;

/// The narrowest the panel beside the chat drags to, and the least it leaves the chat.
const SIDE_LEAST: f32 = 320.0;
pub(crate) const CHAT_LEAST: f32 = 420.0;

/// Where a Mac draws the window's traffic lights: the close button's top left in the
/// title bar (main.rs), the three, each 14 px, in the middle of its height.
pub(crate) const LIGHTS: (f32, f32) = (16.0, 17.0);
/// Where what's beside the traffic lights starts, past the three (23 px apart) and a
/// gap as wide as the one before them: the knot in the sidebar's top, and with the
/// sidebar put away, the control that opens it. Windows draws its buttons at the other
/// end, so nothing moves there.
const LIGHTS_END: f32 = LIGHTS.0 + 60.0 + LIGHTS.0;
/// How wide a small icon is drawn, in its button.
const ICON_GLYPH: f32 = 14.0;

/// Whether a Mac's traffic lights are gone from the window's corner: full screen, where
/// they leave with the title bar.
fn lights_gone(window: &Window) -> bool {
    window.is_fullscreen()
}

/// What a control on the sidebar's edge says while the pointer rests on it: out to its
/// right and level with it, so it lands in the title bar or just past the rail rather
/// than under the pointer, over the chat. It comes after a moment, as a tooltip does,
/// so a pointer passing over doesn't flash it. `key` is the control's hover key, and
/// `w` and `h` its size.
fn tip_beside(
    p: Palette,
    key: &SharedString,
    words: SharedString,
    w: f32,
    h: f32,
) -> Option<AnyElement> {
    let on = motion::hovered(key);
    // Kept with the control's hover, which is forgotten once it isn't drawn.
    motion::level(key, on, motion::HOVER);
    let k = motion::level(&SharedString::from(format!("{key}-tip")), on, TIP_WAIT);
    // From 0.85 (about half a second in) to 0.95, easing out as `level` does; going
    // away it's gone at once.
    let shown = ((k - 0.85) / 0.1).clamp(0.0, 1.0);
    if shown <= 0.0 {
        return None;
    }
    // Placed from the control's top left corner, wherever its own children sit.
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                deferred(
                    anchored()
                        .position_mode(AnchoredPositionMode::Local)
                        .position(point(px(w + space::XS), px(0.0)))
                        .child(
                            div().h(px(h)).flex().items_center().opacity(shown).child(
                                div()
                                    .px(px(space::S))
                                    .py(px(2.0))
                                    .rounded(px(radius::INPUT))
                                    .bg(p.raised)
                                    .border_1()
                                    .border_color(p.line_strong)
                                    .shadow_md()
                                    .text_size(px(size::SMALL))
                                    .text_color(p.ink)
                                    .whitespace_nowrap()
                                    .child(words),
                            ),
                        ),
                )
                .with_priority(1),
            )
            .into_any_element(),
    )
}

/// How long `tip_beside`'s level takes to come all the way on, in seconds.
const TIP_WAIT: f32 = 1.2;

/// An agent's square on the rail.
const RAIL_ROW: f32 = 40.0;
/// A pinned agent's tile: three across the sidebar, inside its sides, a little apart.
const TILE: f32 = ((layout::SIDEBAR - 2.0 * space::S - 2.0 * space::XS) / 3.0) as i32 as f32;
/// The box a tile draws the agent's thread in.
const TILE_THREAD: (f32, f32) = (52.0, 36.0);

/// How far the rail's update icon is from the window's bottom, over the gear over the
/// yarns of `servers` servers, for the update popup to sit level with it.
fn rail_update_bottom(servers: usize) -> f32 {
    let yarns = servers.max(1) as f32;
    space::S + yarns * control::SERVER + yarns * space::XS + control::ICON_LARGE + space::XS
}
/// The sidebar's collapse and open, under the pointer.
#[cfg(target_os = "macos")]
const BAR_COLLAPSE: &str = "Collapse sidebar  ⌘B";
#[cfg(not(target_os = "macos"))]
const BAR_COLLAPSE: &str = "Collapse sidebar  Ctrl+B";
#[cfg(target_os = "macos")]
const BAR_OPEN: &str = "Open sidebar  ⌘B";
#[cfg(not(target_os = "macos"))]
const BAR_OPEN: &str = "Open sidebar  Ctrl+B";
/// The small browser window's width.
const MINI_W: f32 = 280.0;

/// The height of the page in the small browser window, for a page `fw` by `fh`.
/// How tall the row over the agent's browser is: its name and its controls.
const BROWSER_HEAD: f32 = 26.0;

/// The agent's browser's page, `fw` × `fh` pixels, drawn at its own shape as big as fits
/// `most_w` wide and, when it's given, `most_h` tall.
fn page_fit(fw: f32, fh: f32, most_w: f32, most_h: Option<f32>) -> (f32, f32) {
    let (fw, fh) = (fw.max(1.0), fh.max(1.0));
    let k = (most_w.max(1.0) / fw).min(most_h.map_or(f32::MAX, |h| h.max(1.0) / fh));
    ((fw * k).floor().max(1.0), (fh * k).floor().max(1.0))
}

/// What a key pressed on the page the person took over does.
#[derive(Debug, PartialEq)]
enum PageKey {
    /// Goes to the page, as `browser.input`'s args.
    Send(Value),
    /// The page's, though `browser.input` has no way to give it: Shift+Tab, which
    /// would otherwise take the app's focus off the page.
    Keep,
    /// The window's own, such as Ctrl+B.
    Pass,
}

/// A keystroke on the page, as `browser.input` takes it: a named key, or the text it
/// types. A character typed with modifiers is text too, as AltGr+2 types `@` on a
/// Norwegian keyboard, which Windows gives as Ctrl+Alt, and Option+2 on a Mac.
fn page_input(k: &Keystroke, prefer_character_input: bool) -> PageKey {
    let m = &k.modifiers;
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
    if let Some(key) = named {
        if k.key == "tab" && m.shift {
            return PageKey::Keep;
        }
        return PageKey::Send(serde_json::json!({"kind": "key", "key": key}));
    }
    let modified = m.control || m.platform || (m.alt && !cfg!(target_os = "macos"));
    match k.key_char.as_deref().filter(|t| !t.is_empty()) {
        Some(text) if prefer_character_input || !modified => {
            PageKey::Send(serde_json::json!({"kind": "text", "text": text}))
        }
        _ => PageKey::Pass,
    }
}

/// The page's own keys: Tab and Shift+Tab move the window's focus everywhere else
/// (GPUI Kit's root binds them), and here they're the page's, so they reach its key
/// handler. Bound for its key context, `BrowserPage`.
pub fn bind_page_keys(cx: &mut App) {
    cx.bind_keys(page_key_bindings());
}

fn page_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("tab", NoAction, Some(PAGE_KEYS)),
        KeyBinding::new("shift-tab", NoAction, Some(PAGE_KEYS)),
    ]
}

/// The key context of the page the person can type into.
const PAGE_KEYS: &str = "BrowserPage";

// LOO-279: Tab on the page moved the window's focus to the composer, so what was
// typed after it went into the chat, and AltGr's characters never reached the page.
#[cfg(test)]
mod page_key_tests {
    use super::{page_fit, page_input, page_key_bindings, PageKey, PAGE_KEYS};
    use gpui_kit::{actions, KeyBinding, KeyContext, Keymap, Keystroke};
    use serde_json::json;

    actions!(page_key_tests, [MoveFocus]);

    fn key(s: &'static str, prefer_character_input: bool) -> PageKey {
        page_input(&Keystroke::parse(s).unwrap(), prefer_character_input)
    }

    fn named(key: &str) -> PageKey {
        PageKey::Send(json!({"kind": "key", "key": key}))
    }

    fn text(text: &str) -> PageKey {
        PageKey::Send(json!({"kind": "text", "text": text}))
    }

    #[test]
    fn keys_on_the_page_go_to_it() {
        assert_eq!(key("tab", false), named("Tab"));
        assert_eq!(key("enter", false), named("Enter"));
        assert_eq!(key("backspace", false), named("Backspace"));
        assert_eq!(key("escape", false), named("Escape"));
        assert_eq!(key("a->a", false), text("a"));
        assert_eq!(key("shift-a->A", false), text("A"));
        // AltGr+2 on a Norwegian keyboard, which Windows gives as Ctrl+Alt.
        assert_eq!(key("ctrl-alt-2->@", true), text("@"));
        // The window's own keys stay the window's.
        assert_eq!(key("ctrl-b->b", false), PageKey::Pass);
        // Shift+Tab has no way to the page, but doesn't move the focus off it.
        assert_eq!(key("shift-tab", false), PageKey::Keep);
    }

    #[test]
    fn tab_on_the_page_is_the_pages() {
        // As GPUI Kit's root binds them, to move the focus.
        let mut bindings = vec![
            KeyBinding::new("tab", MoveFocus, Some("Root")),
            KeyBinding::new("shift-tab", MoveFocus, Some("Root")),
        ];
        bindings.extend(page_key_bindings());
        let keymap = Keymap::new(bindings);
        let root = KeyContext::parse("Root").unwrap();
        let page = KeyContext::parse(PAGE_KEYS).unwrap();
        for k in ["tab", "shift-tab"] {
            let input = [Keystroke::parse(k).unwrap()];
            let (elsewhere, _) = keymap.bindings_for_input(&input, std::slice::from_ref(&root));
            assert_eq!(elsewhere.len(), 1, "{k} moves the focus elsewhere");
            let (on_page, _) = keymap.bindings_for_input(&input, &[root.clone(), page.clone()]);
            assert!(
                on_page.is_empty(),
                "{k} on the page reaches its key handler"
            );
        }
    }

    #[test]
    fn the_page_is_drawn_at_its_own_shape() {
        assert_eq!(page_fit(1264.0, 705.0, 632.0, None), (632.0, 352.0));
        // Full screen, what's tall enough decides.
        assert_eq!(
            page_fit(1264.0, 705.0, 2000.0, Some(600.0)),
            (1075.0, 600.0)
        );
        assert_eq!(page_fit(1264.0, 705.0, 900.0, Some(900.0)), (900.0, 501.0));
    }
}

fn mini_page_height(fw: f32, fh: f32) -> f32 {
    (MINI_W * fh / fw).clamp(90.0, 200.0)
}

/// How wide the panel beside the chat is, in a window `window` wide: as the person
/// dragged it, or else a plugin's panel or the browser (`wide`) wider than an
/// agent's page.
fn side_width(wide: bool, window: f32, bar: f32, dragged: Option<f32>) -> f32 {
    let most = (window - bar - CHAT_LEAST).max(SIDE_LEAST);
    match dragged {
        Some(w) => w.clamp(SIDE_LEAST, most),
        None if wide => (window * 0.45).clamp(360.0, 820.0).min(most),
        None => (window * 0.36).clamp(360.0, 560.0).min(most),
    }
}

fn panel_bounds(r: [f32; 4]) -> wry::Rect {
    wry::Rect {
        position: wry::dpi::LogicalPosition::new(r[0] as f64, r[1] as f64).into(),
        size: wry::dpi::LogicalSize::new(r[2].max(0.0) as f64, r[3].max(0.0) as f64).into(),
    }
}

/// An SVG icon by its asset path, `size` square, drawn in `ink`.
fn svg_icon(path: SharedString, size: f32, ink: Hsla) -> AnyElement {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(ink)
        .into_any_element()
}

#[cfg(test)]
mod row_tests {
    use super::{model, Entity, InputState, Message, Mood, Palette, Row, RowInputs, Show};
    use crate::prefs::{Thoughts, Tools};
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    fn reply(id: &str, text: &str, call: Option<&str>) -> Message {
        let block = |kind: &str, text: &str, call: Option<&str>| model::Block {
            kind: kind.into(),
            text: text.into(),
            call: call.map(String::from),
            file: None,
            card: None,
            parent: None,
        };
        let mut blocks = vec![block("text", text, None)];
        if let Some(c) = call {
            blocks.insert(0, block("tool", "", Some(c)));
        }
        Message {
            id: id.into(),
            role: "agent".into(),
            at: 1_700_000_000_000,
            blocks,
            live: false,
            queued: false,
            from_agent: None,
            routine: String::new(),
            routine_name: String::new(),
            pinned: false,
            mentions: Vec::new(),
            status: "done".into(),
        }
    }

    fn tool(call: &str, status: Option<&str>) -> model::Tool {
        model::Tool {
            call: call.into(),
            run: "r1".into(),
            title: "ls".into(),
            status: status.map(String::from),
            block: None,
            parent: None,
        }
    }

    thread_local! {
        // The chat's files, made once, as the chat keeps them until they change.
        static FILES: Rc<HashMap<String, String>> = Rc::new(HashMap::new());
        static SECRETS: Rc<HashMap<String, Entity<InputState>>> = Rc::new(HashMap::new());
    }

    fn inputs(rows: Vec<Message>, ix: usize, tools: Vec<model::Tool>) -> RowInputs {
        let rows = Rc::new(rows.into_iter().map(Row::Message).collect::<Vec<_>>());
        let tools = Rc::new(model::Tools::new(tools));
        let called = match &rows[ix] {
            Row::Message(m) => m
                .blocks
                .iter()
                .filter_map(|b| b.call.as_ref())
                .map(|c| tools.get(c).cloned())
                .collect(),
            Row::Approval(_) => Vec::new(),
        };
        RowInputs {
            rows,
            ix,
            here: model::Flow::default(),
            tools,
            called,
            files: FILES.with(Rc::clone),
            secrets: SECRETS.with(Rc::clone),
            gone_on: Vec::new(),
            name: "Fake".into(),
            tint: gpui_kit::hsla(0.6, 0.5, 0.5, 1.0),
            mood: Mood::Idle,
            open: HashSet::new(),
            show: Show {
                tools: Tools::Folded,
                thoughts: Thoughts::Hidden,
                text_scale: 1.0,
                bubbles: true,
                vibrant: false,
                newest: false,
                room: 720,
            },
            is_new: false,
            column: 720.0,
            p: Palette::new(false),
            line: None,
            when: Some("14:05".into()),
        }
    }

    #[test]
    fn a_finished_row_is_drawn_again_only_when_what_it_shows_changes() {
        let a = reply("m1", "Hello", Some("c1"));
        let b = reply("m2", "There", Some("c2"));
        let was = inputs(
            vec![a.clone(), b.clone()],
            0,
            vec![tool("c1", Some("done"))],
        );
        // The rows made again, the same: as it was, though another reply's tool changed.
        let same = inputs(
            vec![a.clone(), b.clone()],
            0,
            vec![tool("c1", Some("done")), tool("c2", None)],
        );
        assert!(was.same(&same));
        // Its own tool call changed.
        let failed = inputs(
            vec![a.clone(), b.clone()],
            0,
            vec![tool("c1", Some("failed"))],
        );
        assert!(!was.same(&failed));
        // Its words changed.
        let edited = inputs(
            vec![reply("m1", "Hello!", Some("c1")), b.clone()],
            0,
            vec![tool("c1", Some("done"))],
        );
        assert!(!was.same(&edited));
        // A thought in it opened, and the time on hover moved on.
        let mut opened = same.clone();
        opened.open.insert("m1/1".into());
        assert!(!was.same(&opened));
        let mut later = same.clone();
        later.when = Some("Yesterday 14:05".into());
        assert!(!was.same(&later));
        // Another row at its place.
        let moved = inputs(vec![b, a], 1, vec![tool("c1", Some("done"))]);
        assert!(!was.same(&moved));
    }
}

#[cfg(test)]
mod side_tests {
    use super::{control, layout, rail_update_bottom, side_width, CHAT_LEAST, SIDE_LEAST};

    #[test]
    fn the_rail_gives_the_panel_the_room_the_sidebar_had() {
        let window = 1000.0;
        let whole = side_width(true, window, layout::SIDEBAR, Some(5000.0));
        let rail = side_width(true, window, layout::RAIL, Some(5000.0));
        assert_eq!(rail - whole, layout::SIDEBAR - layout::RAIL);
        // The update popup from the rail sits level with the icon, over the gear and
        // however many yarns there are.
        assert!(rail_update_bottom(1) > control::SERVER + control::ICON_LARGE);
        assert_eq!(
            rail_update_bottom(2) - rail_update_bottom(1),
            control::SERVER + super::space::XS
        );
    }

    #[test]
    fn the_panel_keeps_its_dragged_width_and_leaves_the_chat_room() {
        let window = 1400.0;
        assert_eq!(
            side_width(true, window, layout::SIDEBAR, Some(500.0)),
            500.0
        );
        assert_eq!(
            side_width(false, window, layout::SIDEBAR, Some(100.0)),
            SIDE_LEAST
        );
        let most = window - layout::SIDEBAR - CHAT_LEAST;
        assert_eq!(
            side_width(true, window, layout::SIDEBAR, Some(5000.0)),
            most
        );
        // Undragged, it's a share of the window, never past what leaves the chat room.
        assert!(
            side_width(true, 1120.0, layout::SIDEBAR, None)
                <= 1120.0 - layout::SIDEBAR - CHAT_LEAST
        );
        assert!(
            side_width(false, window, layout::SIDEBAR, None)
                < side_width(true, window, layout::SIDEBAR, None)
        );
    }
}

/// How far past its edges a list page builds rows, so a scroll doesn't show them coming.
const PAGE_OVERDRAW: f32 = 200.0;

/// How wide the activity page's column is at most.
const ACTIVITY_WIDTH: f32 = 640.0;

/// What the activity page's lines were made from (`Loom::activity_lines`).
type ActivityKey = (usize, Option<i64>, Option<i64>, chrono::NaiveDate);

/// A line of the activity page.
#[derive(Clone)]
enum ActivityLine {
    /// A day's name, over its entries.
    Day(String),
    /// The entry at this place in the activity.
    Entry(usize),
}

/// A page whose rows are drawn as a list (`Loom::list_page`).
struct PageList {
    state: ListState,
    count: usize,
    /// The frame it was last drawn in.
    drawn: u64,
}

/// A message search found, as the sidebar shows it: its agent, its snippet and the
/// matched words in it, whether the person wrote it, and when.
type SidebarHit = (String, String, Vec<std::ops::Range<usize>>, bool, i64);

/// What the sidebar is drawn from (`Loom::sidebar_key`): everything it shows, but how
/// far its threads and hovers have moved, which draw it again as they move.
#[derive(PartialEq)]
struct SidebarKey {
    railed: bool,
    lights: bool,
    p: Palette,
    agents: Vec<SidebarAgent>,
    /// The agent whose chat is on screen.
    selected: Option<String>,
    query: String,
    hits: Vec<SidebarHit>,
    search: bool,
    popup: Option<Screen>,
    servers: Vec<SidebarServer>,
    here: String,
    conn: String,
    behind: Vec<model::Behind>,
    behind_all: bool,
    update_open: bool,
}

/// An agent, as the sidebar shows it.
#[derive(PartialEq)]
struct SidebarAgent {
    id: String,
    name: String,
    color: String,
    pinned: bool,
    outline: Option<Vec<(f32, f32)>>,
    mood: Mood,
    finished: Option<Instant>,
    preview: String,
    unread: bool,
}

/// A server, as the sidebar shows it.
#[derive(PartialEq)]
struct SidebarServer {
    dir: String,
    spool: Spool,
    name: String,
    label: String,
    heard: bool,
    behind: bool,
}

/// The sidebar, whole or as the rail, as a view of its own.
struct SidebarView {
    this: WeakEntity<Loom>,
    railed: bool,
}

impl Render for SidebarView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::perf::count("sidebar");
        let railed = self.railed;
        let id = cx.entity_id();
        self.this
            .update(cx, |loom, cx| {
                motion::scoped(id, || {
                    if railed {
                        loom.rail(cx)
                    } else {
                        loom.whole_sidebar(cx)
                    }
                })
            })
            .map(IntoElement::into_any_element)
            .unwrap_or_else(|_| div().into_any_element())
    }
}

/// What the chat's list is drawn from (`chat_list`).
#[derive(Clone)]
struct ChatInputs {
    rows: Rc<Vec<Row>>,
    flow: Rc<Vec<model::Flow>>,
    tools: Rc<model::Tools>,
    files: Rc<HashMap<String, String>>,
    /// The masked fields of the proposed MCP servers' cards (`Loom::card_secrets`).
    secrets: Rc<HashMap<String, Entity<InputState>>>,
    branches: Rc<HashMap<String, Vec<(String, String)>>>,
    name: String,
    tint: Hsla,
    mood: Mood,
    open: HashSet<String>,
    show: Show,
    newest: Option<String>,
    new_line: Option<String>,
    column: f32,
    p: Palette,
    pad: Option<f32>,
}

/// The chat's list, as a view of its own.
struct ChatList {
    scroller: Entity<MessageScrollerState>,
    this: WeakEntity<Loom>,
    inputs: Option<ChatInputs>,
    /// A view for each finished message it has drawn, by id (`RowView`).
    views: Rc<RefCell<HashMap<String, Entity<RowView>>>>,
    /// The rows `views` was last pruned to.
    pruned: Option<Rc<Vec<Row>>>,
}

impl Render for ChatList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(i) = self.inputs.clone() else {
            return div().into_any_element();
        };
        // A message that's gone from the chat takes its view with it.
        if !self.pruned.as_ref().is_some_and(|r| Rc::ptr_eq(r, &i.rows)) {
            let here: HashSet<&str> = i
                .rows
                .iter()
                .filter_map(|r| match r {
                    Row::Message(m) => Some(m.id.as_str()),
                    Row::Approval(_) => None,
                })
                .collect();
            self.views.borrow_mut().retain(|id, v| {
                let keep = here.contains(id.as_str());
                if !keep {
                    motion::forget_view(v.entity_id());
                }
                keep
            });
            self.pruned = Some(i.rows.clone());
        }
        crate::perf::count("list");
        chat_list(
            i,
            self.scroller.clone(),
            self.this.clone(),
            cx.entity_id(),
            self.views.clone(),
        )
        .into_any_element()
    }
}

/// What one of the chat's rows is drawn from (`chat_row`).
#[derive(Clone)]
struct RowInputs {
    rows: Rc<Vec<Row>>,
    ix: usize,
    here: model::Flow,
    tools: Rc<model::Tools>,
    /// The tool calls its blocks name, as they are: what it shows of `tools`.
    called: Vec<Option<model::Tool>>,
    files: Rc<HashMap<String, String>>,
    secrets: Rc<HashMap<String, Entity<InputState>>>,
    /// The branches that go on from it.
    gone_on: Vec<(String, String)>,
    name: String,
    tint: Hsla,
    mood: Mood,
    /// What's open in it, of the chat's open thoughts, folds and steps.
    open: HashSet<String>,
    show: Show,
    /// The first reply that came while the chat was shut, with NEW over it.
    is_new: bool,
    column: f32,
    p: Palette,
    /// The time line over it, and its time on hover.
    line: Option<String>,
    when: Option<String>,
}

impl RowInputs {
    fn row(&self) -> &Row {
        &self.rows[self.ix]
    }

    /// Whether the row would be drawn the same from `o`.
    fn same(&self, o: &Self) -> bool {
        self.ix == o.ix
            && (Rc::ptr_eq(&self.rows, &o.rows) || self.row() == o.row())
            && self.here == o.here
            && self.called == o.called
            && Rc::ptr_eq(&self.files, &o.files)
            && Rc::ptr_eq(&self.secrets, &o.secrets)
            && self.gone_on == o.gone_on
            && self.name == o.name
            && self.tint == o.tint
            && self.mood == o.mood
            && self.open == o.open
            && self.show == o.show
            && self.is_new == o.is_new
            && self.column == o.column
            && self.p == o.p
            && self.line == o.line
            && self.when == o.when
    }
}

/// A finished message in the chat, as a view of its own: drawn from its cache, at the
/// height it was last laid out at, until what it shows changes. So a frame that only
/// moves the live reply's thread doesn't lay out every reply on screen again.
struct RowView {
    inputs: Option<RowInputs>,
    this: WeakEntity<Loom>,
    /// The height it was last laid out at, once it has been.
    height: Rc<Cell<Option<Pixels>>>,
}

impl Render for RowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(i) = &self.inputs else {
            return div().into_any_element();
        };
        crate::perf::count("row");
        let height = self.height.clone();
        motion::scoped(cx.entity_id(), || chat_row(i, self.this.clone()))
            .relative()
            // Measures it as it's laid out, for drawing it from the cache. Should it come
            // out another height than the cache had it at, it's laid out again.
            .child(
                canvas(
                    move |bounds, window, _| {
                        let was = height.replace(Some(bounds.size.height));
                        if was.is_some_and(|h| h != bounds.size.height) {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// Plays the wheel's next step, sized by the time since `last`, and the one after on
/// the next frame, until it's played out (crate::smooth).
fn smooth_step(this: WeakEntity<Loom>, last: Instant, window: &mut Window, cx: &mut App) {
    let now = Instant::now();
    let step = this
        .update(cx, |this, _| {
            let step = this.smooth.step(now - last);
            this.smooth.running = step.is_some();
            step
        })
        .ok()
        .flatten();
    let Some(step) = step else {
        return;
    };
    window.dispatch_event(PlatformInput::Scroll(step), cx);
    window.on_next_frame(move |window, cx| smooth_step(this, now, window, cx));
}

/// The chat's messages, as its list draws them: each finished one from its own view in
/// `views`, drawn from its cache while it's the same (`RowView`), and the reply being
/// written and the approvals as they are, as part of the list's view, `list`.
fn chat_list(
    i: ChatInputs,
    scroller: Entity<MessageScrollerState>,
    this: WeakEntity<Loom>,
    list: EntityId,
    views: Rc<RefCell<HashMap<String, Entity<RowView>>>>,
) -> MessageScroller {
    let ChatInputs {
        rows,
        flow,
        tools,
        files,
        secrets,
        branches,
        name,
        tint,
        mood,
        open,
        show,
        newest,
        new_line,
        column,
        p,
        pad,
    } = i;
    let now = chrono::Local::now();
    MessageScroller::new("chat", scroller, move |ix, window, cx| {
        let Some(row) = rows.get(ix) else {
            return div().into_any_element();
        };
        let here = flow.get(ix).copied().unwrap_or_default();
        let m = match row {
            Row::Message(m) => Some(m),
            Row::Approval(_) => None,
        };
        let inputs = RowInputs {
            rows: rows.clone(),
            ix,
            here,
            tools: tools.clone(),
            called: m.map_or_else(Vec::new, |m| {
                m.blocks
                    .iter()
                    .filter_map(|b| b.call.as_ref())
                    .map(|c| tools.get(c).cloned())
                    .collect()
            }),
            files: files.clone(),
            secrets: secrets.clone(),
            gone_on: m
                .and_then(|m| branches.get(&m.id).cloned())
                .unwrap_or_default(),
            name: name.clone(),
            tint,
            mood: if m.is_some_and(|m| m.live) {
                mood
            } else {
                Mood::Idle
            },
            // What's open in it, named by its message, or a question's by its call.
            open: {
                let of = match row {
                    Row::Message(m) => m.id.as_str(),
                    Row::Approval(a) => a.call.as_str(),
                };
                open.iter().filter(|k| k.contains(of)).cloned().collect()
            },
            show: Show {
                newest: m.is_some_and(|m| newest.as_deref() == Some(m.id.as_str())),
                ..show
            },
            is_new: m.is_some_and(|m| new_line.as_deref() == Some(m.id.as_str())),
            column,
            p,
            // Across the chat where it paused, or on a new day: when it went on.
            line: m.filter(|_| here.line).map(|m| line_of(m.at, now)),
            // Its time, just outside its left edge while the pointer is over it.
            when: m
                .filter(|m| !m.live && !m.queued)
                .map(|m| when_sent(m.at, now))
                .filter(|w| !w.is_empty()),
        };
        let Some(m) = m.filter(|m| !m.live) else {
            return motion::scoped(list, || chat_row(&inputs, this.clone())).into_any_element();
        };
        let view = views
            .borrow_mut()
            .entry(m.id.clone())
            .or_insert_with(|| {
                cx.new(|_| RowView {
                    inputs: None,
                    this: this.clone(),
                    height: Rc::new(Cell::new(None)),
                })
            })
            .clone();
        let (same, height) = {
            let v = view.read(cx);
            (
                v.inputs.as_ref().is_some_and(|o| o.same(&inputs)),
                v.height.get(),
            )
        };
        if !same {
            view.update(cx, |v, cx| {
                v.inputs = Some(inputs);
                v.height.set(None);
                cx.notify();
            });
        }
        // Drawn from its cache at the height it was laid out at, or, until it has been,
        // built and measured. While a screen reader listens it's built at every frame: a
        // frame that uses a row as it was leaves it out of what the screen reader is told.
        match height.filter(|_| same && !window.is_a11y_active()) {
            Some(h) => AnyView::from(view)
                .cached(StyleRefinement::default().w_full().h(h))
                .into_any_element(),
            None => AnyView::from(view).into_any_element(),
        }
    })
    .with_row_style(StyleRefinement::default().pb(px(0.0)))
    .with_bottom_fade(p.bg)
    // The last message scrolls up clear of the small browser window.
    .when_some(pad, |l, pad| {
        let mut s = StyleRefinement::default();
        s.padding.bottom = Some(px(pad).into());
        l.with_list_style(s)
    })
}

/// One of the chat's rows: a message, with its time line, its pin and its branches, or
/// an approval.
fn chat_row(i: &RowInputs, this: WeakEntity<Loom>) -> Stateful<Div> {
    let RowInputs {
        ix,
        here,
        tools,
        files,
        secrets,
        name,
        tint,
        mood,
        open,
        show,
        is_new,
        column,
        p,
        ..
    } = i;
    let (ix, here, tint, mood, show, is_new, column, p) =
        (*ix, *here, *tint, *mood, *show, *is_new, *column, *p);
    let row = i.row();
    // Above the first reply that came while the chat was shut: a line, NEW
    // in the agent's colour.
    let divider = div()
        .w_full()
        .max_w(px(column))
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
    let joined = here.joined;
    // Across the chat where it paused, or on a new day: when it went on.
    let time_line = i.line.clone().map(|line| {
        div()
            .w_full()
            .max_w(px(column))
            .flex()
            .justify_center()
            .pt(px(if ix == 0 { space::S } else { space::L }))
            .pb(px(space::XS))
            .text_size(px(size::CAPTION))
            .text_color(p.ink_3)
            .child(line)
    });
    let inner = match row {
        Row::Message(m) if m.role == "person" => person(
            p,
            m,
            tint,
            show.vibrant,
            files,
            show.text_scale,
            show.room,
            joined,
            this.clone(),
        ),
        Row::Message(m) => reply(
            p,
            m,
            name,
            tint,
            mood,
            here.head,
            open,
            tools,
            files,
            secrets,
            show,
            this.clone(),
        ),
        Row::Approval(a) => approval(p, a, tint, open, show.text_scale, this.clone()),
    };
    // A message's pin, just outside its corner: there while it's pinned, and
    // on hover to pin it.
    let pin = match row {
        Row::Message(m) if !m.live && !m.queued => Some((m.id.clone(), m.pinned)),
        _ => None,
    };
    // Under it, a link to each branch that goes on from it; and beside it, on
    // hover, a way to branch there.
    let gone_on = i.gone_on.clone();
    let branch_at = pin.as_ref().map(|(id, _)| id.clone());
    // Its time, just outside its left edge while the pointer is over it. Time
    // lines show only where the chat paused.
    let when = i.when.clone();
    let to_branch = this.clone();
    let mut links =
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .pt(px(if joined.below { 0.0 } else { space::XS }));
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
                .soft_ink(p.ink_3, p.ink)
                .child(Icon::new(Ico::GitBranch).xsmall())
                .child(title)
                .on_click(move |_, _, cx| {
                    let _ = this.update(cx, |loom, cx| loom.open_branch(Some(id.clone()), cx));
                }),
        );
    }
    // A right-click on a kept message: Copy, Pin and Branch from here.
    let on_right = branch_at.clone().map(|id| {
        let this = this.clone();
        move |e: &MouseDownEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |loom, cx| {
                loom.open_ctx(e.position, Ctx::Message(id.clone()), cx)
            });
        }
    });
    // To a screen reader, a message is an item in the chat, named by who said
    // what.
    let (row_id, said) = match row {
        Row::Message(m) => {
            let who = if m.role == "person" {
                "You"
            } else {
                name.as_str()
            };
            let words = model::spoken(&model::copy_text(m, tools));
            (format!("msg-{}", m.id), Some(format!("{who}: {words}")))
        }
        Row::Approval(_) => (format!("row-{ix}"), None),
    };
    div()
        .id(SharedString::from(row_id))
        .when_some(said, |d, said| d.role(Role::ListItem).aria_label(said))
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .px(px(space::XL))
        .when_some(on_right, |d, f| d.on_mouse_down(MouseButton::Right, f))
        .when(is_new, |d| d.child(divider))
        .children(time_line)
        .child(
            div()
                .w_full()
                // The hover reaches the strip either side where its time and
                // buttons are, so moving on to them keeps them there.
                .max_w(px(column + 2.0 * HOVER_REACH))
                .mx(px(-HOVER_REACH))
                .px(px(HOVER_REACH))
                // Messages stand a little apart, and bubbles that join sit
                // close, as they do in a reply. The list's own gap under each
                // row is taken off (`with_row_style`), so it's here instead.
                .pt(px(if joined.above { 0.0 } else { space::XS }))
                .pb(px(if joined.below { space::XS } else { space::S }))
                .relative()
                .group("row")
                .when_some(when, |d, when| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(if joined.above { 0.0 } else { space::XS } + 4.0))
                            .right(relative(1.0))
                            .mr(px(-HOVER_REACH))
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
                            .role(Role::Button)
                            .aria_label("Branch from here")
                            .absolute()
                            .top(px(space::XS + 28.0))
                            .right(px(HOVER_REACH - 30.0))
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::ROW))
                            .cursor_pointer()
                            .text_color(p.ink_3)
                            .opacity(0.0)
                            .group_hover("row", |s| s.opacity(1.0))
                            .soft_hover(transparent_black(), p.hover, Some((p.ink_3, p.ink)))
                            .tooltip(tip("Branch from here"))
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
                            .role(Role::Button)
                            .aria_label(if pinned { "Unpin" } else { "Pin" })
                            .absolute()
                            .top(px(space::XS))
                            .right(px(HOVER_REACH - 30.0))
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
                            .soft_hover(
                                transparent_black(),
                                p.hover,
                                Some((if pinned { p.ink_2 } else { p.ink_3 }, p.ink)),
                            )
                            .tooltip(tip(if pinned { "Unpin" } else { "Pin" }))
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
}
