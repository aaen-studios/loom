//! What the screens show, as the core tells it. The model keeps no state of its own
//! beyond that: lists come from the core's cache (`core.get`), and are fetched again
//! when an event says they changed. Only the text of a reply still being written
//! lives here alone, from its deltas, until the finished message replaces it.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// What the app asks the core for, and when.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum List {
    Agents,
    Conversations,
    Runs,
    Tools,
    Approvals,
    Outbox,
    Messages,
    Devices,
}

impl List {
    pub fn name(self) -> &'static str {
        match self {
            List::Agents => "agents",
            List::Conversations => "conversations",
            List::Runs => "runs",
            List::Tools => "tools",
            List::Approvals => "approvals",
            List::Outbox => "outbox",
            List::Messages => "messages",
            List::Devices => "devices",
        }
    }

    fn from_name(name: &str) -> Option<List> {
        Some(match name {
            "agents" => List::Agents,
            "conversations" => List::Conversations,
            "runs" => List::Runs,
            "tools" => List::Tools,
            "approvals" => List::Approvals,
            "outbox" => List::Outbox,
            "messages" => List::Messages,
            "devices" => List::Devices,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub color: String,
    pub program: String,
    pub permission: String,
    pub instructions: String,
    /// The names of the person's MCP servers it has.
    pub mcp: Vec<String>,
}

/// One of the person's MCP servers.
#[derive(Clone, Debug)]
pub struct McpServer {
    pub name: String,
    /// The command line, for one the agent starts; empty for one over HTTP.
    pub command: String,
    pub url: String,
    /// Its env or headers: each name, and its text or the secret it names.
    pub settings: Vec<(String, Setting)>,
}

#[derive(Clone, Debug)]
pub enum Setting {
    Text(String),
    Secret(String),
}

impl McpServer {
    fn of(v: &Value) -> McpServer {
        let mut command = s(v, "command");
        if let Some(args) = v.get("args").and_then(Value::as_array) {
            for a in args.iter().filter_map(Value::as_str) {
                command.push(' ');
                command.push_str(a);
            }
        }
        let pairs = v
            .get("env")
            .or_else(|| v.get("headers"))
            .and_then(Value::as_object);
        let settings = pairs
            .map(|m| {
                m.iter()
                    .map(|(k, v)| {
                        let value = match v.get("secret").and_then(Value::as_str) {
                            Some(name) => Setting::Secret(name.to_string()),
                            None => Setting::Text(v.as_str().unwrap_or_default().to_string()),
                        };
                        (k.clone(), value)
                    })
                    .collect()
            })
            .unwrap_or_default();
        McpServer {
            name: s(v, "name"),
            command,
            url: s(v, "url"),
            settings,
        }
    }

    /// Its settings as the form shows them, one `NAME=value` or `NAME=secret:KEY` a line.
    pub fn settings_text(&self) -> String {
        self.settings
            .iter()
            .map(|(k, v)| match v {
                Setting::Text(t) => format!("{k}={t}"),
                Setting::Secret(n) => format!("{k}=secret:{n}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The `mcp.put` arguments for a server as the form has it: a command line, or an http(s)
/// address, and its settings, one `NAME=value` or `NAME=secret:KEY` a line.
pub fn server_args(name: &str, line: &str, settings: &str) -> Value {
    let mut pairs = serde_json::Map::new();
    for l in settings.lines() {
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() {
            continue;
        }
        let value = match v.strip_prefix("secret:") {
            Some(n) => json!({"secret": n.trim()}),
            None => json!(v),
        };
        pairs.insert(k.to_string(), value);
    }
    let line = line.trim();
    if line.starts_with("http://") || line.starts_with("https://") {
        return json!({"name": name.trim(), "url": line, "headers": pairs});
    }
    let mut words = line.split_whitespace();
    let command = words.next().unwrap_or_default();
    let args: Vec<&str> = words.collect();
    json!({"name": name.trim(), "command": command, "args": args, "env": pairs})
}

/// The tools a device offers agents, as the core names them.
pub const DEVICE_TOOLS: [&str; 7] = [
    "run_command",
    "read_file",
    "list_files",
    "open",
    "screenshot",
    "read_clipboard",
    "write_clipboard",
];

/// An agent program on the instance. Its settings' values stay there: only their names
/// come.
#[derive(Clone, Debug)]
pub struct Program {
    pub name: String,
    pub source: String,
    pub version: String,
    pub settings: Vec<String>,
    /// Its settings shared with everyone but the owner: tokens a person may use each
    /// day or month, and, for someone it applies to, how many are left.
    pub share: Option<Share>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub tokens: i64,
    pub per: String,
    pub left: Option<i64>,
}

/// An agent in the ACP Registry.
#[derive(Clone, Debug)]
pub struct Listed {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// How it would run on the instance (`binary`, `npx` or `uvx`), or none.
    pub how: Option<String>,
    /// The version installed, if it is.
    pub installed: Option<String>,
}

/// A note an agent keeps, or one shared by all of the person's agents.
#[derive(Clone, Debug)]
pub struct Note {
    pub id: i64,
    pub text: String,
    pub shared: bool,
    pub by: String,
    pub at: i64,
}

/// Someone with an account on the instance, for the People page.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub id: String,
    pub username: String,
    /// owner, admin or member.
    pub role: String,
}

/// An invite still open: its id, the role it gives, the uses it has left, and when it
/// expires (Unix ms).
#[derive(Clone, Debug, PartialEq)]
pub struct Invite {
    pub id: String,
    pub role: String,
    pub uses: i64,
    pub expires: i64,
}

/// One of the person's skills.
#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// One of the person's plugins (protocol.md, "Plugins").
#[derive(Clone, Debug, Default)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Its panel's first page, when it has a panel.
    pub entry: Option<String>,
    pub tools: bool,
    /// What it asks for that the person hasn't allowed yet.
    pub waiting: Vec<String>,
}

impl Plugin {
    fn of(p: &Value) -> Plugin {
        let list = |key: &str| -> Vec<String> {
            p.get(key)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        Plugin {
            id: s(p, "id"),
            name: s(p, "name"),
            version: s(p, "version"),
            description: s(p, "description"),
            entry: p
                .pointer("/panel/entry")
                .and_then(Value::as_str)
                .map(String::from),
            tools: p.get("tools").is_some_and(|t| !t.is_null()),
            waiting: list("waiting"),
        }
    }
}

/// A plugin's panel that's open: the plugin, and its files.
#[derive(Clone, Debug)]
pub struct OpenPanel {
    pub plugin: Plugin,
    pub files: Vec<(String, String)>,
}

/// What `asks` means, in words, for the plugins page.
pub fn asking(asks: &[String]) -> String {
    asks.iter()
        .map(|a| match a.as_str() {
            "tools" => String::from("run its tool server for your agents"),
            "agents" => String::from("message your agents"),
            other => match other.strip_prefix("cmd:") {
                Some(c) => format!("send {c}"),
                None => other.to_string(),
            },
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// One of the person's routines: a prompt an agent is sent on a schedule.
#[derive(Clone, Debug, Default)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub agent: String,
    pub prompt: String,
    /// How its schedule runs: "every" (minutes apart), "daily" or "weekdays" (at a time of
    /// day), or "" for a routine that only runs by webhook.
    pub kind: String,
    /// The schedule's value as the form shows it: "30", or "08:00".
    pub at: String,
    /// When it runs, in words: "every 30 minutes".
    pub when: String,
    pub notify: bool,
    pub on: bool,
    /// Whether a run missed while the instance was off happens once it starts again.
    pub catch_up: bool,
    /// Its webhook's id and secret, when it has one.
    pub hook: String,
    pub secret: String,
    /// When it runs next (Unix ms), or 0 while it's paused.
    pub next: i64,
    /// How its last run went: running, done, failed, skipped, or "" before the first.
    pub last_status: String,
    pub last_at: i64,
    pub last_error: String,
}

/// Which of the schedules routine `r` has: "every", "daily", "weekdays", or "".
fn schedule_kind(r: &Value) -> String {
    for k in ["every", "daily", "weekdays"] {
        if r.pointer(&format!("/schedule/{k}")).is_some() {
            return k.into();
        }
    }
    String::new()
}

/// Its schedule's value, as the form shows it: "30" or "08:00".
fn schedule_value(r: &Value) -> String {
    match r.get("schedule") {
        Some(Value::Object(m)) => m
            .values()
            .next()
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

impl Routine {
    fn of(r: &Value) -> Routine {
        let last = r.get("last").cloned().unwrap_or(Value::Null);
        Routine {
            id: s(r, "id"),
            name: s(r, "name"),
            agent: s(r, "agent"),
            prompt: s(r, "prompt"),
            kind: schedule_kind(r),
            at: schedule_value(r),
            when: s(r, "when"),
            notify: r.get("notify").and_then(Value::as_bool).unwrap_or(false),
            on: r.get("on").and_then(Value::as_bool).unwrap_or(true),
            catch_up: r.get("catch_up").and_then(Value::as_bool).unwrap_or(true),
            hook: s(r, "hook"),
            secret: s(r, "secret"),
            next: r.get("next").and_then(Value::as_i64).unwrap_or(0),
            last_status: s(&last, "status"),
            last_at: last.get("at").and_then(Value::as_i64).unwrap_or(0),
            last_error: s(&last, "error"),
        }
    }
}

/// What the routine form says, to save.
#[derive(Clone, Debug, Default)]
pub struct RoutineForm {
    /// Empty for a new routine.
    pub id: String,
    pub name: String,
    pub agent: String,
    pub prompt: String,
    /// "every", "daily" or "weekdays", as on `Routine`.
    pub kind: String,
    /// Minutes apart, or a time of day as HH:MM. Empty for a routine that only runs by
    /// webhook. The instance checks it.
    pub at: String,
    pub notify: bool,
    pub on: bool,
    pub catch_up: bool,
    pub webhook: bool,
}

/// One entry of the activity log.
#[derive(Clone, Debug)]
pub struct Activity {
    pub kind: String,
    pub text: String,
    /// Who did it: a device, an agent's id, "auto" for the permission mode, or "" for the
    /// agent itself.
    pub by: String,
    pub at: i64,
}

/// One revision of an agent, for the edit page.
#[derive(Clone, Debug)]
pub struct Revision {
    pub revision: i64,
    /// Who made it: a device or an agent's id.
    pub by: String,
    pub at: i64,
    pub instructions: String,
}

#[derive(Clone, Debug)]
pub struct Conversation {
    pub id: String,
    pub agent: String,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: String,
    pub text: String,
    pub call: Option<String>,
    pub file: Option<FileRef>,
}

impl Block {
    fn text(kind: &str, text: String) -> Block {
        Block {
            kind: kind.into(),
            text,
            call: None,
            file: None,
        }
    }
}

/// A file in a message, or one sent: its SHA-256, name, type and size.
#[derive(Clone, Debug, PartialEq)]
pub struct FileRef {
    pub file: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

impl FileRef {
    fn of(v: &Value) -> FileRef {
        FileRef {
            file: s(v, "file"),
            name: s(v, "name"),
            mime: s(v, "mime"),
            size: v.get("size").and_then(Value::as_u64).unwrap_or(0),
        }
    }

    /// Whether the chat shows it as a picture.
    pub fn image(&self) -> bool {
        matches!(
            self.mime.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        )
    }
}

/// A file picked to go with the next message.
#[derive(Clone, Debug)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    /// Once it's up.
    pub file: Option<FileRef>,
    pub error: Option<String>,
}

/// A message search found.
#[derive(Clone, Debug)]
pub struct Hit {
    pub agent: String,
    /// The words around the match, and where in them the matched words are.
    pub snippet: String,
    pub marks: Vec<std::ops::Range<usize>>,
    pub person: bool,
    pub at: i64,
}

/// A snippet without the instance's marks (U+0002 before a matched word, U+0003 after),
/// and the byte ranges of the matched words in what's left.
pub fn unmark(snippet: &str) -> (String, Vec<std::ops::Range<usize>>) {
    let mut out = String::new();
    let mut marks = Vec::new();
    let mut start = None;
    for c in snippet.chars() {
        match c {
            '\u{2}' => start = Some(out.len()),
            '\u{3}' => {
                if let Some(s) = start.take() {
                    marks.push(s..out.len());
                }
            }
            '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    (out, marks)
}

/// A size as people read it.
pub fn size_of(bytes: u64) -> String {
    match bytes {
        b if b < 1000 => format!("{b} B"),
        b if b < 1_000_000 => format!("{:.0} KB", b as f64 / 1000.0),
        b => format!("{:.1} MB", b as f64 / 1_000_000.0),
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub at: i64,
    pub blocks: Vec<Block>,
    /// Still being written, from deltas.
    pub live: bool,
    /// Written here and waiting in the outbox.
    pub queued: bool,
    /// For a message another agent sent with message_agent, that agent's name.
    pub from_agent: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub call: String,
    pub run: String,
    pub title: String,
    pub status: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Approval {
    pub call: String,
    pub run: String,
    pub title: String,
    pub detail: String,
    pub can_always: bool,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub id: String,
    pub conversation: String,
    pub agent: String,
    pub message: String,
}

/// What an agent's thread shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Idle,
    Thinking,
    Writing,
    Tool,
    Waiting,
    Failed,
}

/// Where the connection to the instance is.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Conn {
    Starting,
    Connecting,
    Live,
    Offline(u64),
    SignedOut,
    UpdateNeeded,
}

#[derive(Default)]
pub struct Model {
    pub conn: Option<Conn>,
    pub synced: bool,
    pub login_error: Option<String>,
    pub logging_in: bool,
    pub agents: Vec<Agent>,
    pub conversations: Vec<Conversation>,
    pub runs: Vec<Run>,
    pub tools: Vec<Tool>,
    pub approvals: Vec<Approval>,
    pub programs: Vec<String>,
    /// The selected agent's messages, oldest first, with the queued ones last.
    pub messages: Vec<Message>,
    pub more_messages: bool,
    pub selected: Option<String>,
    /// Agents with a reply that came while their chat wasn't open, until it is: their
    /// row in the sidebar is bold.
    pub unread: HashSet<String>,
    /// Replies being written, by message id: their blocks so far.
    live: HashMap<String, Vec<Block>>,
    last_delta: HashMap<String, Instant>,
    failed: HashMap<String, Instant>,
    finished: HashMap<String, Instant>,
    queued: Vec<Queued>,
    dirty: HashSet<List>,
    next_id: u64,
    /// The id of the agent the app just asked the instance to make.
    pub made_agent: Option<String>,
    pub notices: Vec<String>,
    /// Where the instance is moving to, while the owner's instance.move is under way.
    pub moving: Option<String>,
    /// Files for the next message.
    pub attachments: Vec<Attachment>,
    /// Files the core has fetched, by SHA-256, or SHA-256 and ".thumb": where they are.
    pub files: HashMap<String, String>,
    /// file.get requests on their way: what they fetch, and whether to open it.
    getting: HashMap<String, (String, bool)>,
    /// Files fetched to be opened, for the app to open.
    pub to_open: Vec<String>,
    /// What's typed in search, and what the newest search found.
    pub query: String,
    pub hits: Vec<Hit>,
    search_id: String,
    /// A newer release, downloaded and checked: its version and where it is.
    pub update: Option<(String, String)>,
    /// What a check the person asked for found, for the app to say.
    pub checked: Option<String>,
    /// The agent the edit page shows, and its revisions, newest first.
    pub editing: Option<String>,
    pub revisions: Vec<Revision>,
    revisions_stale: bool,
    /// The activity page's entries, newest first.
    pub activity: Vec<Activity>,
    /// The person's skills, by name, for the skills page.
    pub skills: Vec<Skill>,
    skills_stale: bool,
    /// The person's routines, oldest first.
    pub routines: Vec<Routine>,
    routines_stale: bool,
    /// The person's plugins, by id.
    pub plugins: Vec<Plugin>,
    plugins_stale: bool,
    /// The plugin whose panel is open, once its files have come.
    pub panel: Option<OpenPanel>,
    /// Answers for the open panel's door: the message's id, and its value or why not.
    pub door_answers: Vec<(i64, Result<Value, String>)>,
    /// The panel's messages sent on as commands: the command's id, the message's id, and
    /// where its answer is in the command's value.
    door_waiting: HashMap<String, (i64, &'static str)>,
    /// The QR code that adds a phone, while one is shown: its text, and when it expires
    /// (Unix ms).
    pub link: Option<(String, i64)>,
    /// The name of the device that joined with it, once one has.
    pub joined: Option<String>,
    /// Whether phones reach the instance in this app over the network, once it has said.
    pub phones: Option<bool>,
    /// The push relay the instance sends iPhone notifications through, once it has said:
    /// its address while it's on, or an empty one while it's off.
    pub relay: Option<String>,
    /// The People page: everyone with an account, the invites still open, and the newest
    /// invite made here, to pass on: its link (or code), the role it gives and when it expires.
    pub people: Vec<Person>,
    pub invites: Vec<Invite>,
    pub invite_made: Option<(String, String, i64)>,
    /// The newest password reset link made here: whose, the link, and when it expires.
    pub reset_made: Option<(String, String, i64)>,
    /// Why the People page shows nothing, when the instance said no.
    pub people_refused: Option<String>,
    pub people_open: bool,
    people_stale: bool,
    /// The person's devices, as the cache has them: id, name and kind.
    pub devices: Vec<(String, String, String)>,
    /// This device's id, once the instance has welcomed it.
    pub me: String,
    /// What the selected agent remembers, for the memory page, newest first.
    pub notes: Vec<Note>,
    notes_stale: bool,
    /// The person's profile: its text, revision and who wrote it, once it's come.
    pub profile: Option<(String, i64, String)>,
    /// The profile came, and the page's text should show it.
    pub profile_came: bool,
    /// The agent programs on the instance, for the programs page.
    pub installed: Vec<Program>,
    /// The ACP Registry's agents, once it's been read.
    pub registry: Vec<Listed>,
    pub registry_loading: bool,
    pub registry_error: Option<String>,
    /// The registry agent being installed, while it is.
    pub installing: Option<String>,
    programs_stale: bool,
    /// What this device offers agents, each tool with `off`, `ask` or `allow`.
    pub device_tools: Vec<(String, String)>,
    /// The person's MCP servers, and the names of their secrets.
    pub mcp_servers: Vec<McpServer>,
    pub secrets: Vec<String>,
    mcp_stale: bool,
}

#[derive(Clone, Debug)]
struct Queued {
    cid: String,
    agent: Option<String>,
    conversation: Option<String>,
    text: String,
    files: usize,
}

impl Queued {
    fn of(q: &Value) -> Queued {
        Queued {
            cid: s(q, "cid"),
            agent: q.get("agent").and_then(Value::as_str).map(str::to_string),
            conversation: q
                .get("conversation")
                .and_then(Value::as_str)
                .map(str::to_string),
            text: s(q, "text"),
            files: q.get("files").and_then(Value::as_array).map_or(0, Vec::len),
        }
    }
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn blocks_of(v: &Value) -> Vec<Block> {
    v.get("blocks")
        .and_then(Value::as_array)
        .map(|bs| {
            bs.iter()
                .map(|b| Block {
                    kind: s(b, "kind"),
                    text: s(b, "text"),
                    call: b.get("call").and_then(Value::as_str).map(str::to_string),
                    file: (s(b, "kind") == "file").then(|| FileRef::of(b)),
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Model {
    pub fn new() -> Model {
        let mut m = Model::default();
        for l in [
            List::Agents,
            List::Conversations,
            List::Runs,
            List::Tools,
            List::Approvals,
            List::Outbox,
        ] {
            m.dirty.insert(l);
        }
        m
    }

    fn id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}_{}", self.next_id)
    }

    /// The frames to send the core for the lists that changed.
    pub fn fetches(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        if std::mem::take(&mut self.revisions_stale) && self.editing.is_some() {
            out.push(self.revisions_cmd());
        }
        if std::mem::take(&mut self.mcp_stale) {
            out.extend(self.load_mcp());
        }
        if std::mem::take(&mut self.programs_stale) {
            out.push(self.programs());
            out.push(self.registry_cmd());
        }
        if std::mem::take(&mut self.notes_stale) {
            if let Some(f) = self.load_notes() {
                out.push(f);
            }
        }
        if std::mem::take(&mut self.skills_stale) {
            out.push(self.load_skills());
        }
        if std::mem::take(&mut self.routines_stale) {
            out.push(self.load_routines());
        }
        if std::mem::take(&mut self.plugins_stale) {
            out.push(self.load_plugins());
        }
        if std::mem::take(&mut self.people_stale) {
            out.extend(self.load_people());
        }
        let dirty: Vec<List> = self.dirty.drain().collect();
        for l in dirty {
            if l == List::Messages {
                let Some(conversation) = self.conversation() else {
                    self.messages.clear();
                    continue;
                };
                out.push(json!({"t": "core.get", "id": "app.messages", "what": "messages", "conversation": conversation, "limit": 200}));
            } else {
                out.push(
                    json!({"t": "core.get", "id": format!("app.{}", l.name()), "what": l.name()}),
                );
            }
        }
        out
    }

    /// The conversation the selected agent's chat shows: its newest.
    pub fn conversation(&self) -> Option<String> {
        let agent = self.selected.as_ref()?;
        self.conversations
            .iter()
            .find(|c| &c.agent == agent)
            .map(|c| c.id.clone())
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    pub fn select(&mut self, agent: Option<String>) {
        if let Some(a) = &agent {
            self.unread.remove(a);
        }
        if self.selected != agent {
            self.selected = agent;
            self.messages.clear();
            self.dirty.insert(List::Messages);
        }
    }

    /// Takes a frame from the core. Gives whether the screen should redraw.
    pub fn take(&mut self, f: &Value) -> bool {
        let t = s(f, "t");
        match t.as_str() {
            "core.conn" => {
                let state = s(f, "state");
                self.conn = Some(match state.as_str() {
                    "connecting" => Conn::Connecting,
                    "live" => Conn::Live,
                    "offline" => {
                        Conn::Offline(f.get("retry_ms").and_then(Value::as_u64).unwrap_or(0))
                    }
                    "signed_out" => Conn::SignedOut,
                    "update_needed" => Conn::UpdateNeeded,
                    _ => Conn::Starting,
                });
                if state != "live" {
                    self.synced = false;
                }
                if state == "signed_out" {
                    self.logging_in = false;
                }
            }
            "core.login" => {
                self.logging_in = false;
                if let Some(e) = f.get("error") {
                    self.login_error = Some(s(e, "message"));
                } else {
                    self.login_error = None;
                }
            }
            "synced" => {
                self.synced = true;
                self.routines_stale = true;
                for l in [
                    List::Agents,
                    List::Conversations,
                    List::Runs,
                    List::Tools,
                    List::Approvals,
                    List::Outbox,
                    List::Messages,
                ] {
                    self.dirty.insert(l);
                }
            }
            "welcome" => self.me = s(f, "device"),
            "core.data" => self.data(f),
            "core.queued" => {
                self.queued.push(Queued::of(f));
                self.place_queued();
            }
            "core.file" => self.file_came(f),
            "core.moved" => {
                self.moving = None;
                self.notices.push(format!(
                    "Your instance moved to {}. The app is connected there now.",
                    s(f, "address")
                ));
            }
            "core.device" => {
                if let Some(tools) = f.get("tools").and_then(Value::as_object) {
                    self.device_tools = DEVICE_TOOLS
                        .iter()
                        .map(|t| {
                            let mode = tools.get(*t).and_then(Value::as_str).unwrap_or("off");
                            (t.to_string(), mode.to_string())
                        })
                        .collect();
                }
            }
            "core.update" => {
                let asked = f.get("id").is_some();
                if let Some(path) = f.get("path").and_then(Value::as_str) {
                    let version = s(f, "version");
                    if asked {
                        self.checked = Some(format!(
                            "loom {version} is ready. Restart it from the sidebar."
                        ));
                    }
                    self.update = Some((version, path.to_string()));
                } else if asked {
                    self.checked = Some(match f.get("error") {
                        Some(e) => format!("The update check didn't work: {}", s(e, "message")),
                        None => format!("loom {} is the newest.", s(f, "version")),
                    });
                }
            }
            "ack" => {
                let cid = s(f, "cid");
                self.queued.retain(|q| q.cid != cid);
                self.dirty.insert(List::Messages);
            }
            "delta" => self.delta(f),
            "event" => self.event(f),
            "result" => self.result(f),
            "error" => {
                let code = s(f, "code");
                if code == "local" {
                    // The instance on this computer didn't start: the sign-in page says why.
                    self.login_error = Some(s(f, "message"));
                } else if code != "offline" {
                    self.notices.push(s(f, "message"));
                }
            }
            _ => return false,
        }
        true
    }

    fn event(&mut self, f: &Value) {
        let kind = s(f, "kind");
        let (family, _) = kind.split_once('.').unwrap_or((kind.as_str(), ""));
        match family {
            // A device that joined with the QR code being shown: the code is used up.
            "device" => {
                self.dirty.insert(List::Devices);
                if kind == "device.added" && self.link.is_some() {
                    self.link = None;
                    self.joined = Some(s(f.get("device").unwrap_or(&Value::Null), "name"));
                }
            }
            "agent" => {
                self.dirty.insert(List::Agents);
                // A change to the agent being edited is a new revision to list.
                if self.editing.is_some()
                    && f.pointer("/agent/id").and_then(Value::as_str) == self.editing.as_deref()
                {
                    self.revisions_stale = true;
                }
            }
            "conversation" => {
                self.dirty.insert(List::Conversations);
                self.dirty.insert(List::Messages);
            }
            "message" => {
                let message = f.get("message").cloned().unwrap_or(Value::Null);
                let id = s(&message, "id");
                self.live.remove(&id);
                let conversation = s(f, "conversation");
                if Some(conversation.clone()) == self.conversation() {
                    self.dirty.insert(List::Messages);
                } else if kind == "message.created" && self.synced && s(&message, "role") == "agent"
                {
                    // A reply in another chat, as it comes; catch-up is what was seen before.
                    let agent = self
                        .conversations
                        .iter()
                        .find(|c| c.id == conversation)
                        .map(|c| c.agent.clone())
                        .unwrap_or_else(|| s(&message, "agent"));
                    if !agent.is_empty() {
                        self.unread.insert(agent);
                    }
                }
                self.dirty.insert(List::Conversations);
            }
            "run" => {
                self.dirty.insert(List::Runs);
                if kind == "run.finished" {
                    let run = s(f, "run");
                    if let Some(r) = self.runs.iter().find(|r| r.id == run) {
                        let at = Instant::now();
                        if s(f, "status") == "failed" {
                            self.failed.insert(r.agent.clone(), at);
                        } else {
                            self.finished.insert(r.agent.clone(), at);
                        }
                    }
                    self.dirty.insert(List::Tools);
                    self.dirty.insert(List::Approvals);
                }
            }
            "tool" => {
                self.dirty.insert(List::Tools);
            }
            "approval" => {
                self.dirty.insert(List::Approvals);
            }
            "routine" => {
                self.routines_stale = true;
            }
            "plugin" if kind != "plugin.data" => {
                self.plugins_stale = true;
            }
            // Someone joined with an invite: the People page lists them, and the invite's uses.
            "user" if self.people_open => {
                self.people_stale = true;
            }
            _ => {}
        }
    }

    fn delta(&mut self, f: &Value) {
        let message = s(f, "message");
        let block = f.get("block").and_then(Value::as_u64).unwrap_or(0) as usize;
        let blocks = self.live.entry(message.clone()).or_default();
        while blocks.len() <= block {
            blocks.push(Block::text(&s(f, "kind"), String::new()));
        }
        // The core sends only what adds to the text, so the piece goes on the end.
        let pos = f.get("pos").and_then(Value::as_u64).unwrap_or(0) as usize;
        let text = s(f, "text");
        let b = &mut blocks[block];
        if pos <= b.text.len() {
            b.text.truncate(pos);
            b.text.push_str(&text);
        }
        if let Some(run) = self.runs.iter().find(|r| r.message == message) {
            self.last_delta.insert(run.agent.clone(), Instant::now());
        }
    }

    fn result(&mut self, f: &Value) {
        let id = s(f, "id");
        if id.starts_with("app.search") {
            if id != self.search_id {
                return;
            }
            let results = f
                .pointer("/value/results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            self.hits = results
                .iter()
                .filter_map(|r| {
                    let m = r.get("message")?;
                    let conversation = s(m, "conversation");
                    let person = s(m, "role") == "person";
                    // A person's message is its conversation's agent's.
                    let agent = if person {
                        self.conversations
                            .iter()
                            .find(|c| c.id == conversation)?
                            .agent
                            .clone()
                    } else {
                        s(m, "agent")
                    };
                    self.agent(&agent)?;
                    let (snippet, marks) = unmark(&s(r, "snippet"));
                    Some(Hit {
                        agent,
                        snippet,
                        marks,
                        person,
                        at: m.get("at").and_then(Value::as_i64).unwrap_or(0),
                    })
                })
                .collect();
        } else if id.starts_with("app.door") {
            let Some((message, at)) = self.door_waiting.remove(&id) else {
                return;
            };
            let answer = match f.get("error") {
                Some(e) => Err(s(e, "message")),
                None => Ok(f.pointer(at).cloned().unwrap_or(Value::Null)),
            };
            self.door_answers.push((message, answer));
        } else if id.starts_with("app.plugins") {
            self.plugins = f
                .pointer("/value/plugins")
                .and_then(Value::as_array)
                .map(|ps| ps.iter().map(Plugin::of).collect())
                .unwrap_or_default();
            // The open panel's plugin may have been allowed more, or less.
            if let Some(open) = &mut self.panel {
                match self.plugins.iter().find(|p| p.id == open.plugin.id) {
                    Some(p) => open.plugin = p.clone(),
                    None => self.panel = None,
                }
            }
        } else if id.starts_with("app.move") {
            // Once it's there, the core says so (core.moved); a failure is said now.
            if let Some(e) = f.get("error") {
                self.moving = None;
                self.notices
                    .push(format!("The move didn't happen: {}", s(e, "message")));
            }
        } else if id.starts_with("app.plugin_get") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                return;
            }
            let Some(v) = f.get("value") else { return };
            let plugin = Plugin::of(v.get("plugin").unwrap_or(&Value::Null));
            let files = v
                .get("files")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                        .collect()
                })
                .unwrap_or_default();
            self.panel = Some(OpenPanel { plugin, files });
        } else if id.starts_with("app.plugin_") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.plugins_stale = true;
        } else if id.starts_with("app.skills") {
            self.skills = f
                .pointer("/value/skills")
                .and_then(Value::as_array)
                .map(|ss| {
                    ss.iter()
                        .map(|k| Skill {
                            name: s(k, "name"),
                            description: s(k, "description"),
                            body: s(k, "body"),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.device_revoke") || id.starts_with("app.account_delete") {
            // A deleted account's sessions close with 4001 and the core signs out, so only
            // a refusal needs saying.
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
        } else if id.starts_with("app.phones") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            } else {
                self.phones = f.pointer("/value/on").and_then(Value::as_bool);
            }
        } else if id.starts_with("app.relay") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            } else if f.pointer("/value/on").and_then(Value::as_bool) == Some(true) {
                self.relay = Some(
                    f.pointer("/value/url")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                );
            } else {
                self.relay = Some(String::new());
            }
        } else if id.starts_with("app.people") {
            if let Some(e) = f.get("error") {
                self.people_refused = Some(s(e, "message"));
                self.people.clear();
            } else {
                self.people_refused = None;
                self.people = f
                    .pointer("/value/users")
                    .and_then(Value::as_array)
                    .map(|us| {
                        us.iter()
                            .map(|u| Person {
                                id: s(u, "user"),
                                username: s(u, "username"),
                                role: s(u, "role"),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
        } else if id.starts_with("app.invites") {
            self.invites = f
                .pointer("/value/invites")
                .and_then(Value::as_array)
                .map(|is| {
                    is.iter()
                        .map(|i| Invite {
                            id: s(i, "invite"),
                            role: s(i, "role"),
                            uses: i.get("uses").and_then(Value::as_i64).unwrap_or(0),
                            expires: i.get("expires").and_then(Value::as_i64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.invite_made") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            } else if let Some(v) = f.get("value") {
                // The core adds the link, for an instance at an http:// or https:// address.
                let link = v
                    .get("link")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| s(v, "code"));
                self.invite_made = Some((
                    link,
                    s(v, "role"),
                    v.get("expires").and_then(Value::as_i64).unwrap_or(0),
                ));
            }
            self.people_stale = true;
        } else if let Some(who) = id.strip_prefix("app.reset:") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            } else if let Some(v) = f.get("value") {
                let who = who.rsplit_once('_').map_or(who, |(w, _)| w);
                let link = v
                    .get("link")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| s(v, "code"));
                self.reset_made = Some((
                    who.to_string(),
                    link,
                    v.get("expires").and_then(Value::as_i64).unwrap_or(0),
                ));
            }
        } else if id.starts_with("app.person") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.people_stale = true;
        } else if id.starts_with("app.link") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            } else if let Some(v) = f.get("value") {
                self.link = Some((
                    s(v, "qr"),
                    v.get("expires").and_then(Value::as_i64).unwrap_or(0),
                ));
                self.joined = None;
            }
        } else if id.starts_with("app.routines") {
            self.routines = f
                .pointer("/value/routines")
                .and_then(Value::as_array)
                .map(|rs| rs.iter().map(Routine::of).collect())
                .unwrap_or_default();
            self.dirty.insert(List::Messages);
        } else if id.starts_with("app.routine_") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.routines_stale = true;
        } else if id.starts_with("app.skill_put") || id.starts_with("app.skill_delete") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.skills_stale = true;
        } else if id.starts_with("app.memory_list") {
            self.notes = f
                .pointer("/value/memories")
                .and_then(Value::as_array)
                .map(|ns| {
                    ns.iter()
                        .map(|n| Note {
                            id: n.get("id").and_then(Value::as_i64).unwrap_or(0),
                            text: s(n, "text"),
                            shared: n.get("shared").and_then(Value::as_bool).unwrap_or(false),
                            by: s(n, "by"),
                            at: n.get("at").and_then(Value::as_i64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.memory_") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.notes_stale = true;
        } else if id.starts_with("app.profile") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                return;
            }
            let pr = f.pointer("/value/profile").cloned().unwrap_or(Value::Null);
            self.profile = Some((
                s(&pr, "text"),
                pr.get("revision").and_then(Value::as_i64).unwrap_or(0),
                s(&pr, "by"),
            ));
            self.profile_came = true;
        } else if id.starts_with("app.activity") {
            self.activity = f
                .pointer("/value/activity")
                .and_then(Value::as_array)
                .map(|es| {
                    es.iter()
                        .map(|e| Activity {
                            kind: s(e, "kind"),
                            text: s(e, "text"),
                            by: s(e, "by"),
                            at: e.get("at").and_then(Value::as_i64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.revisions") {
            self.revisions = f
                .pointer("/value/revisions")
                .and_then(Value::as_array)
                .map(|rs| {
                    rs.iter()
                        .map(|r| Revision {
                            revision: r.get("revision").and_then(Value::as_i64).unwrap_or(0),
                            by: s(r, "by"),
                            at: r.get("at").and_then(Value::as_i64).unwrap_or(0),
                            instructions: r
                                .pointer("/agent/instructions")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.mcp_list") {
            self.mcp_servers = f
                .pointer("/value/servers")
                .and_then(Value::as_array)
                .map(|ss| ss.iter().map(McpServer::of).collect())
                .unwrap_or_default();
        } else if id.starts_with("app.secrets") {
            self.secrets = f
                .pointer("/value/secrets")
                .and_then(Value::as_array)
                .map(|ss| ss.iter().map(|k| s(k, "name")).collect())
                .unwrap_or_default();
        } else if id.starts_with("app.mcp_") || id.starts_with("app.secret_") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.mcp_stale = true;
        } else if id.starts_with("app.registry") {
            self.registry_loading = false;
            if let Some(e) = f.get("error") {
                self.registry_error = Some(s(e, "message"));
                return;
            }
            self.registry_error = None;
            self.registry = f
                .pointer("/value/agents")
                .and_then(Value::as_array)
                .map(|as_| {
                    as_.iter()
                        .map(|a| Listed {
                            id: s(a, "id"),
                            name: s(a, "name"),
                            version: s(a, "version"),
                            description: s(a, "description"),
                            how: a.get("how").and_then(Value::as_str).map(str::to_string),
                            installed: a
                                .get("installed")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.install") || id.starts_with("app.program") {
            if id.starts_with("app.install") {
                self.installing = None;
            }
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.programs_stale = true;
        } else if id.starts_with("app.acp") {
            self.programs = f
                .pointer("/value/programs")
                .and_then(Value::as_array)
                .map(|ps| ps.iter().map(|p| s(p, "name")).collect())
                .unwrap_or_default();
            self.installed = f
                .pointer("/value/programs")
                .and_then(Value::as_array)
                .map(|ps| {
                    ps.iter()
                        .map(|p| Program {
                            name: s(p, "name"),
                            source: s(p, "source"),
                            version: s(p, "version"),
                            settings: p
                                .get("settings")
                                .and_then(Value::as_array)
                                .map(|ks| {
                                    ks.iter()
                                        .filter_map(Value::as_str)
                                        .map(str::to_string)
                                        .collect()
                                })
                                .unwrap_or_default(),
                            share: p.get("share").filter(|v| v.is_object()).map(|sh| Share {
                                tokens: sh.get("tokens").and_then(Value::as_i64).unwrap_or(0),
                                per: s(sh, "per"),
                                left: sh.get("left").and_then(Value::as_i64),
                            }),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.new_agent") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            } else {
                self.made_agent = f
                    .pointer("/value/agent/id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        } else if let Some(e) = f.get("error") {
            self.notices.push(s(e, "message"));
        }
    }

    fn data(&mut self, f: &Value) {
        let Some(what) = f
            .get("what")
            .and_then(Value::as_str)
            .and_then(List::from_name)
        else {
            return;
        };
        let items: Vec<Value> = f
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        match what {
            List::Devices => {
                self.devices = items
                    .iter()
                    .map(|d| (s(d, "id"), s(d, "name"), s(d, "kind")))
                    .collect();
            }
            List::Agents => {
                self.agents = items
                    .iter()
                    .map(|a| Agent {
                        id: s(a, "id"),
                        name: s(a, "name"),
                        color: s(a, "color"),
                        program: s(a, "acp_agent"),
                        permission: s(a, "permission"),
                        instructions: s(a, "instructions"),
                        mcp: a
                            .get("mcp")
                            .and_then(Value::as_array)
                            .map(|ns| {
                                ns.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect();
                if let Some(made) = self.made_agent.take() {
                    if self.agent(&made).is_some() {
                        self.select(Some(made));
                    } else {
                        self.made_agent = Some(made);
                    }
                }
                if self
                    .selected
                    .as_ref()
                    .is_none_or(|id| self.agent(id).is_none())
                {
                    let first = self.agents.first().map(|a| a.id.clone());
                    self.select(first);
                }
            }
            List::Conversations => {
                let before = self.conversation();
                self.conversations = items
                    .iter()
                    .map(|c| Conversation {
                        id: s(c, "id"),
                        agent: s(c, "agent"),
                    })
                    .collect();
                if self.conversation() != before {
                    self.dirty.insert(List::Messages);
                }
            }
            List::Runs => {
                self.runs = items
                    .iter()
                    .map(|r| Run {
                        id: s(r, "run"),
                        conversation: s(r, "conversation"),
                        agent: s(r, "agent"),
                        message: s(r, "message"),
                    })
                    .collect();
            }
            List::Tools => {
                self.tools = items
                    .iter()
                    .map(|t| Tool {
                        call: s(t, "call"),
                        run: s(t, "run"),
                        title: s(t, "title"),
                        status: t.get("status").and_then(Value::as_str).map(str::to_string),
                    })
                    .collect();
            }
            List::Approvals => {
                self.approvals = items
                    .iter()
                    .map(|a| Approval {
                        call: s(a, "call"),
                        run: s(a, "run"),
                        title: s(a, "title"),
                        detail: match a.get("detail") {
                            Some(Value::String(d)) => d.clone(),
                            Some(Value::Null) | None => String::new(),
                            Some(other) => other.to_string(),
                        },
                        can_always: a
                            .get("can_always")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect();
            }
            List::Outbox => {
                self.queued = items.iter().map(Queued::of).collect();
                self.place_queued();
            }
            List::Messages => {
                self.more_messages = f.get("more").and_then(Value::as_bool).unwrap_or(false);
                let messages: Vec<Message> = items
                    .iter()
                    .map(|m| Message {
                        id: s(m, "id"),
                        role: s(m, "role"),
                        at: m.get("at").and_then(Value::as_i64).unwrap_or(0),
                        blocks: blocks_of(m),
                        live: false,
                        queued: false,
                        // `from` is a device, the agent that sent it, or a routine.
                        from_agent: m.get("from").and_then(Value::as_str).and_then(|f| {
                            if f.starts_with("a_") {
                                Some(
                                    self.agent(f)
                                        .map(|a| a.name.clone())
                                        .unwrap_or_else(|| "Another agent".into()),
                                )
                            } else if f.starts_with("rt_") {
                                Some(
                                    self.routines
                                        .iter()
                                        .find(|r| r.id == f)
                                        .map(|r| format!("the routine {}", r.name))
                                        .unwrap_or_else(|| "a routine".into()),
                                )
                            } else {
                                None
                            }
                        }),
                    })
                    .collect();
                self.messages = messages;
                self.place_queued();
            }
        }
    }

    /// The queued messages for the shown chat go after its stored ones.
    fn place_queued(&mut self) {
        self.messages.retain(|m| !m.queued);
        let conversation = self.conversation();
        let agent = self.selected.clone();
        for q in &self.queued {
            let here = match &q.conversation {
                Some(c) => Some(c) == conversation.as_ref(),
                None => q.agent.is_some() && q.agent == agent,
            };
            if here {
                let mut text = q.text.clone();
                if q.files > 0 {
                    let files = if q.files == 1 {
                        "1 file".to_string()
                    } else {
                        format!("{} files", q.files)
                    };
                    text = if text.is_empty() {
                        files
                    } else {
                        format!("{text}\n\n{files}")
                    };
                }
                self.messages.push(Message {
                    id: q.cid.clone(),
                    role: "person".into(),
                    at: i64::MAX,
                    blocks: vec![Block::text("text", text)],
                    live: false,
                    queued: true,
                    from_agent: None,
                });
            }
        }
    }

    /// The selected chat's messages, with the replies still being written at the end.
    pub fn shown(&self) -> Vec<Message> {
        let mut out = self.messages.clone();
        let conversation = self.conversation();
        for r in &self.runs {
            if Some(&r.conversation) != conversation.as_ref() {
                continue;
            }
            if out.iter().any(|m| m.id == r.message) {
                continue;
            }
            let mut blocks = self.live.get(&r.message).cloned().unwrap_or_default();
            for t in self.tools.iter().filter(|t| t.run == r.id) {
                if !blocks.iter().any(|b| b.call.as_ref() == Some(&t.call)) {
                    let mut b = Block::text("tool", String::new());
                    b.call = Some(t.call.clone());
                    blocks.push(b);
                }
            }
            out.push(Message {
                id: r.message.clone(),
                role: "agent".into(),
                at: i64::MAX,
                blocks,
                live: true,
                queued: false,
                from_agent: None,
            });
        }
        out
    }

    /// The questions waiting in the shown chat.
    pub fn shown_approvals(&self) -> Vec<Approval> {
        let conversation = self.conversation();
        self.approvals
            .iter()
            .filter(|a| {
                self.runs
                    .iter()
                    .any(|r| r.id == a.run && Some(&r.conversation) == conversation.as_ref())
            })
            .cloned()
            .collect()
    }

    /// What an agent's thread shows now.
    pub fn mood(&self, agent: &str) -> Mood {
        let runs: Vec<&Run> = self.runs.iter().filter(|r| r.agent == agent).collect();
        if runs
            .iter()
            .any(|r| self.approvals.iter().any(|a| a.run == r.id))
        {
            return Mood::Waiting;
        }
        if runs.iter().any(|r| {
            self.tools
                .iter()
                .any(|t| t.run == r.id && t.status.is_none())
        }) {
            return Mood::Tool;
        }
        if !runs.is_empty() {
            let writing = self
                .last_delta
                .get(agent)
                .is_some_and(|at| at.elapsed() < Duration::from_millis(700));
            return if writing {
                Mood::Writing
            } else {
                Mood::Thinking
            };
        }
        if self
            .failed
            .get(agent)
            .is_some_and(|at| at.elapsed() < Duration::from_secs(600))
        {
            return Mood::Failed;
        }
        Mood::Idle
    }

    /// When the agent last finished a run, for the thread's one-time snap.
    pub fn finished_at(&self, agent: &str) -> Option<Instant> {
        self.finished.get(agent).copied()
    }

    /// The newest text of an agent's chat, for the sidebar.
    pub fn preview(&self, agent: &Agent) -> String {
        let Some(r) = self.runs.iter().find(|r| r.agent == agent.id) else {
            return match self.mood(&agent.id) {
                Mood::Failed => "Couldn't finish".into(),
                _ => agent.program.clone(),
            };
        };
        if let Some(a) = self.approvals.iter().find(|a| a.run == r.id) {
            return format!("Wants to {}", a.title);
        }
        if let Some(t) = self
            .tools
            .iter()
            .rev()
            .find(|t| t.run == r.id && t.status.is_none())
        {
            return t.title.clone();
        }
        "Thinking".into()
    }

    // --- What the app sends ---

    /// The next message, with the attachments that are up. None while one is still going
    /// up, or with nothing to send.
    pub fn send(&mut self, text: &str) -> Option<Value> {
        let agent = self.selected.clone()?;
        let text = text.trim();
        if self.uploading() {
            return None;
        }
        let files: Vec<String> = self
            .attachments
            .iter()
            .filter_map(|a| a.file.as_ref().map(|f| f.file.clone()))
            .collect();
        if text.is_empty() && files.is_empty() {
            return None;
        }
        self.attachments.clear();
        let mut frame = json!({"t": "send", "agent": agent, "text": text});
        if !files.is_empty() {
            frame["files"] = json!(files);
        }
        Some(frame)
    }

    /// Starts sending a file for the next message.
    pub fn attach(&mut self, path: &str) -> Value {
        let id = self.id("app.up");
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        self.attachments.push(Attachment {
            id: id.clone(),
            name,
            file: None,
            error: None,
        });
        json!({"t": "file.upload", "id": id, "path": path})
    }

    pub fn unattach(&mut self, id: &str) {
        self.attachments.retain(|a| a.id != id);
    }

    /// Whether an attachment is still going up.
    pub fn uploading(&self) -> bool {
        self.attachments
            .iter()
            .any(|a| a.file.is_none() && a.error.is_none())
    }

    /// The thumbnails the shown chat needs and doesn't have yet.
    pub fn file_fetches(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        let wanted: Vec<String> = self
            .shown()
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| b.file.as_ref())
            .filter(|f| f.image())
            .map(|f| f.file.clone())
            .collect();
        for sha in wanted {
            let key = format!("{sha}.thumb");
            if self.files.contains_key(&key) || self.getting.values().any(|(k, _)| k == &key) {
                continue;
            }
            let id = self.id("app.get");
            self.getting.insert(id.clone(), (key, false));
            out.push(json!({"t": "file.get", "id": id, "file": sha, "thumb": true}));
        }
        out
    }

    /// Opens a file in its own program: at once if it's here, or once it's fetched.
    pub fn open_file(&mut self, f: &FileRef) -> Option<Value> {
        if let Some(path) = self.files.get(&f.file) {
            self.to_open.push(path.clone());
            return None;
        }
        let id = self.id("app.get");
        self.getting.insert(id.clone(), (f.file.clone(), true));
        Some(json!({"t": "file.get", "id": id, "file": f.file, "name": f.name}))
    }

    fn file_came(&mut self, f: &Value) {
        let id = s(f, "id");
        let error = f.get("error").map(|e| s(e, "message"));
        if let Some(a) = self.attachments.iter_mut().find(|a| a.id == id) {
            match f.get("file") {
                Some(file) if error.is_none() => a.file = Some(FileRef::of(file)),
                _ => a.error = Some(error.unwrap_or_else(|| "It didn't go up.".into())),
            }
            return;
        }
        let Some((key, open)) = self.getting.remove(&id) else {
            return;
        };
        match (error, f.get("path").and_then(Value::as_str)) {
            (None, Some(path)) => {
                if open {
                    self.to_open.push(path.to_string());
                }
                self.files.insert(key, path.to_string());
            }
            (e, _) => {
                if open {
                    self.notices
                        .push(e.unwrap_or_else(|| "The file couldn't be fetched.".into()));
                }
            }
        }
    }

    /// Searches every chat for `query`. None when it's too short to search.
    pub fn search(&mut self, query: &str) -> Option<Value> {
        self.query = query.to_string();
        if query.trim().chars().count() < 2 {
            self.hits.clear();
            self.search_id.clear();
            return None;
        }
        let id = self.id("app.search");
        self.search_id = id.clone();
        Some(
            json!({"t": "cmd", "id": id, "name": "messages.search", "args": {"query": query.trim(), "limit": 30}}),
        )
    }

    pub fn login(&mut self, server: &str, username: &str, password: &str, code: &str) -> Value {
        self.logging_in = true;
        self.login_error = None;
        let device = json!({"name": device_name(), "kind": "windows"});
        if code.trim().is_empty() {
            json!({"t": "login", "server": server.trim(), "username": username.trim(), "password": password, "device": device})
        } else {
            json!({"t": "setup", "server": server.trim(), "code": code.trim(), "username": username.trim(), "password": password, "device": device})
        }
    }

    pub fn programs(&mut self) -> Value {
        let id = self.id("app.acp");
        json!({"t": "cmd", "id": id, "name": "acp.list"})
    }

    pub fn new_agent(&mut self, name: &str, program: &str, instructions: &str) -> Value {
        let id = self.id("app.new_agent");
        json!({"t": "cmd", "id": id, "name": "agent.create", "args": {"name": name.trim(), "acp_agent": program, "instructions": instructions.trim()}})
    }

    pub fn start_fresh(&mut self) -> Option<Value> {
        let agent = self.selected.clone()?;
        let id = self.id("app.fresh");
        Some(json!({"t": "cmd", "id": id, "name": "conversation.create", "args": {"agent": agent}}))
    }

    pub fn delete_agent(&mut self) -> Option<Value> {
        let agent = self.selected.clone()?;
        let id = self.id("app.delete");
        Some(json!({"t": "cmd", "id": id, "name": "agent.delete", "args": {"agent": agent}}))
    }

    /// Opens the edit page for the selected agent: asks for its revisions.
    pub fn edit_agent(&mut self) -> Option<Value> {
        let agent = self.selected.clone()?;
        self.editing = Some(agent);
        self.revisions.clear();
        Some(self.revisions_cmd())
    }

    fn revisions_cmd(&mut self) -> Value {
        let id = self.id("app.revisions");
        json!({"t": "cmd", "id": id, "name": "agent.revisions", "args": {"agent": self.editing}})
    }

    /// Saves the edit page's name and instructions, as a new revision.
    pub fn save_agent(&mut self, name: &str, instructions: &str) -> Option<Value> {
        let agent = self.editing.clone()?;
        let id = self.id("app.save");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "name": name.trim(), "instructions": instructions}}),
        )
    }

    /// Goes back to `revision`, as a new revision.
    pub fn roll_back(&mut self, revision: i64) -> Option<Value> {
        let agent = self.editing.clone()?;
        let id = self.id("app.rollback");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.rollback", "args": {"agent": agent, "revision": revision}}),
        )
    }

    /// Asks for the person's MCP servers and their secrets' names.
    pub fn load_mcp(&mut self) -> Vec<Value> {
        let (a, b) = (self.id("app.mcp_list"), self.id("app.secrets"));
        vec![
            json!({"t": "cmd", "id": a, "name": "mcp.list"}),
            json!({"t": "cmd", "id": b, "name": "secret.list"}),
        ]
    }

    pub fn put_mcp(&mut self, args: Value) -> Value {
        let id = self.id("app.mcp_put");
        json!({"t": "cmd", "id": id, "name": "mcp.put", "args": args})
    }

    pub fn delete_mcp(&mut self, name: &str) -> Value {
        let id = self.id("app.mcp_delete");
        json!({"t": "cmd", "id": id, "name": "mcp.delete", "args": {"name": name}})
    }

    pub fn put_secret(&mut self, name: &str, value: &str) -> Value {
        let id = self.id("app.secret_put");
        json!({"t": "cmd", "id": id, "name": "secret.put", "args": {"name": name.trim(), "value": value}})
    }

    pub fn delete_secret(&mut self, name: &str) -> Value {
        let id = self.id("app.secret_delete");
        json!({"t": "cmd", "id": id, "name": "secret.delete", "args": {"name": name}})
    }

    /// Gives the agent being edited the MCP server `name`, or takes it away.
    pub fn toggle_mcp(&mut self, name: &str) -> Option<Value> {
        let agent = self.editing.clone()?;
        let mut names = self.agent(&agent)?.mcp.clone();
        if let Some(i) = names.iter().position(|n| n == name) {
            names.remove(i);
        } else {
            names.push(name.to_string());
        }
        let id = self.id("app.save");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "mcp": names}}),
        )
    }

    /// Asks the core what this device offers agents.
    pub fn device_get(&self) -> Value {
        json!({"t": "device.get"})
    }

    /// Sets how agents may use one of this device's tools: `off`, `ask` or `allow`.
    pub fn set_device_tool(&self, tool: &str, mode: &str) -> Value {
        json!({"t": "device.tools", "tools": {tool: mode}})
    }

    /// Asks for the programs page: the instance's programs, and the registry's.
    pub fn load_programs(&mut self) -> Vec<Value> {
        self.registry_loading = self.registry.is_empty();
        self.registry_error = None;
        vec![self.programs(), self.registry_cmd()]
    }

    fn registry_cmd(&mut self) -> Value {
        let id = self.id("app.registry");
        json!({"t": "cmd", "id": id, "name": "acp.registry"})
    }

    /// Installs an agent from the registry, or updates it to the registry's version.
    pub fn install(&mut self, registry_id: &str) -> Value {
        self.installing = Some(registry_id.to_string());
        let id = self.id("app.install");
        json!({"t": "cmd", "id": id, "name": "acp.install", "args": {"id": registry_id}})
    }

    /// Sets a program's setting, or removes it with `None`.
    pub fn set_setting(&mut self, program: &str, key: &str, value: Option<&str>) -> Value {
        let id = self.id("app.program");
        let mut env = serde_json::Map::new();
        env.insert(
            key.trim().to_string(),
            value.map_or(Value::Null, |v| json!(v)),
        );
        json!({"t": "cmd", "id": id, "name": "acp.settings", "args": {"name": program, "env": env}})
    }

    /// Shares a program's settings with everyone but the owner, `tokens` each `per` day
    /// or month; 0 stops sharing them.
    pub fn share_program(&mut self, program: &str, tokens: i64, per: &str) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.share", "args": {"name": program, "tokens": tokens, "per": per}})
    }

    pub fn remove_program(&mut self, program: &str) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.remove", "args": {"name": program}})
    }

    /// Asks for the person's plugins, for the plugins page.
    pub fn load_plugins(&mut self) -> Value {
        let id = self.id("app.plugins");
        json!({"t": "cmd", "id": id, "name": "plugin.list"})
    }

    /// Asks for a plugin's files, to open its panel.
    /// The owner's instance.move to `to`, with the move code `loom receive` printed there.
    pub fn move_instance(&mut self, to: &str, code: &str) -> Option<Value> {
        if to.is_empty() || code.is_empty() {
            self.notices
                .push("Give the new place's address and its move code.".into());
            return None;
        }
        self.moving = Some(to.to_string());
        let id = self.id("app.move");
        Some(
            json!({"t": "cmd", "id": id, "name": "instance.move", "args": {"to": to, "code": code}}),
        )
    }

    pub fn open_plugin(&mut self, plugin: &str) -> Value {
        self.panel = None;
        self.door_answers.clear();
        self.door_waiting.clear();
        let id = self.id("app.plugin_get");
        json!({"t": "cmd", "id": id, "name": "plugin.get", "args": {"id": plugin}})
    }

    pub fn close_panel(&mut self) {
        self.panel = None;
        self.door_answers.clear();
        self.door_waiting.clear();
    }

    /// Allows all a plugin asks for.
    pub fn allow_plugin(&mut self, plugin: &str) -> Value {
        let id = self.id("app.plugin_allow");
        json!({"t": "cmd", "id": id, "name": "plugin.allow", "args": {"id": plugin}})
    }

    pub fn delete_plugin(&mut self, plugin: &str) -> Value {
        if self.panel.as_ref().is_some_and(|p| p.plugin.id == plugin) {
            self.close_panel();
        }
        let id = self.id("app.plugin_delete");
        json!({"t": "cmd", "id": id, "name": "plugin.delete", "args": {"id": plugin}})
    }

    /// A message through the open panel's door (protocol.md, "Plugins"): the command it
    /// becomes, or why it's refused. Its answer comes in `door_answers`, by `message`.
    pub fn door(&mut self, message: i64, op: &str, args: &Value) -> Result<Value, String> {
        let Some(open) = &self.panel else {
            return Err(String::from("The panel isn't open."));
        };
        let plugin = open.plugin.id.clone();
        let key = args.get("key").cloned().unwrap_or(Value::Null);
        let (name, cmd_args, answer_at) = match op {
            "data.get" => (
                String::from("plugin.data.get"),
                json!({"plugin": plugin, "key": key}),
                "/value/value",
            ),
            "data.put" => (
                String::from("plugin.data.put"),
                json!({"plugin": plugin, "key": key, "value": args.get("value").cloned().unwrap_or(Value::Null)}),
                "/value",
            ),
            // The instance checks what the person allowed the plugin.
            "cmd" => (
                String::from("plugin.cmd"),
                json!({"plugin": plugin, "name": s(args, "name"), "args": args.get("args").cloned().unwrap_or(json!({}))}),
                "/value",
            ),
            "message_agent" => (
                String::from("plugin.message"),
                json!({"plugin": plugin, "agent": s(args, "agent"), "text": s(args, "text")}),
                "/value/reply",
            ),
            other => return Err(format!("A panel can't {other}.")),
        };
        let id = self.id("app.door");
        self.door_waiting.insert(id.clone(), (message, answer_at));
        Ok(json!({"t": "cmd", "id": id, "name": name, "args": cmd_args}))
    }

    /// Asks for the person's skills, with what each says, for the skills page.
    pub fn load_skills(&mut self) -> Value {
        let id = self.id("app.skills");
        json!({"t": "cmd", "id": id, "name": "skill.list", "args": {"bodies": true}})
    }

    /// Saves a skill, new or changed.
    pub fn save_skill(&mut self, name: &str, description: &str, body: &str) -> Value {
        let id = self.id("app.skill_put");
        json!({"t": "cmd", "id": id, "name": "skill.put", "args": {"name": name.trim(), "description": description.trim(), "body": body}})
    }

    pub fn delete_skill(&mut self, name: &str) -> Value {
        let id = self.id("app.skill_delete");
        json!({"t": "cmd", "id": id, "name": "skill.delete", "args": {"name": name}})
    }

    /// Asks the instance for a one-time code that adds a phone; the core gives it back with
    /// the QR code's text.
    pub fn create_link(&mut self) -> Value {
        let id = self.id("app.link");
        self.link = None;
        self.joined = None;
        json!({"t": "cmd", "id": id, "name": "link.create"})
    }

    /// Asks whether phones reach the instance in this app over the network, or with `on`,
    /// lets them or stops them. Turning them off takes the code away, since phones can't
    /// use it.
    pub fn set_phones(&mut self, on: Option<bool>) -> Value {
        let id = self.id("app.phones");
        let args = match on {
            Some(on) => {
                if !on {
                    self.link = None;
                }
                json!({"on": on})
            }
            None => json!({}),
        };
        json!({"t": "cmd", "id": id, "name": "instance.phones", "args": args})
    }

    /// Asks which push relay the instance sends iPhone notifications through, or with
    /// `url`, has it use that one (the owner only; an empty one stops it).
    pub fn set_relay(&mut self, url: Option<&str>) -> Value {
        let id = self.id("app.relay");
        let args = match url {
            Some(url) => json!({"url": url}),
            None => json!({}),
        };
        json!({"t": "cmd", "id": id, "name": "instance.relay", "args": args})
    }

    /// Reads everyone with an account and the invites still open, for the People page.
    pub fn load_people(&mut self) -> Vec<Value> {
        let people = self.id("app.people");
        let invites = self.id("app.invites");
        vec![
            json!({"t": "cmd", "id": people, "name": "user.list", "args": {}}),
            json!({"t": "cmd", "id": invites, "name": "invite.list", "args": {}}),
        ]
    }

    /// Makes an invite that gives `role` (member or admin), good for `uses` accounts.
    pub fn create_invite(&mut self, role: &str, uses: i64) -> Value {
        let id = self.id("app.invite_made");
        self.invite_made = None;
        json!({"t": "cmd", "id": id, "name": "invite.create", "args": {"role": role, "uses": uses}})
    }

    pub fn delete_invite(&mut self, invite: &str) -> Value {
        let id = self.id("app.person");
        json!({"t": "cmd", "id": id, "name": "invite.delete", "args": {"invite": invite}})
    }

    /// Makes someone an admin or a member again.
    pub fn set_role(&mut self, user: &str, role: &str) -> Value {
        let id = self.id("app.person");
        json!({"t": "cmd", "id": id, "name": "user.role", "args": {"user": user, "role": role}})
    }

    /// Makes a link that sets a new password for `user`, whose name is `username`.
    pub fn reset_password(&mut self, user: &str, username: &str) -> Value {
        let id = self.id(&format!("app.reset:{username}"));
        self.reset_made = None;
        json!({"t": "cmd", "id": id, "name": "password.reset", "args": {"user": user}})
    }

    /// Removes someone, and everything of theirs.
    pub fn remove_person(&mut self, user: &str) -> Value {
        let id = self.id("app.person");
        json!({"t": "cmd", "id": id, "name": "user.remove", "args": {"user": user}})
    }

    /// Deletes the person's own account and everything of theirs. The instance refuses
    /// the owner's.
    pub fn delete_account(&mut self) -> Value {
        let id = self.id("app.account_delete");
        json!({"t": "cmd", "id": id, "name": "account.delete", "args": {}})
    }

    /// Reads the person's devices from the cache.
    pub fn load_devices(&mut self) -> Value {
        json!({"t": "core.get", "id": "app.devices", "what": "devices"})
    }

    /// Signs one of the person's devices out, for good: its token stops working.
    pub fn revoke_device(&mut self, device: &str) -> Value {
        let id = self.id("app.device_revoke");
        json!({"t": "cmd", "id": id, "name": "device.revoke", "args": {"device": device}})
    }

    /// Asks for the person's routines.
    pub fn load_routines(&mut self) -> Value {
        let id = self.id("app.routines");
        json!({"t": "cmd", "id": id, "name": "routine.list"})
    }

    /// Saves a routine, new (`id` empty) or changed.
    pub fn save_routine(&mut self, f: &RoutineForm) -> Value {
        let cmd = self.id("app.routine_put");
        let at = f.at.trim();
        let schedule = if at.is_empty() {
            Value::Null
        } else if f.kind == "every" {
            json!({"every": at.parse::<i64>().map_or(json!(at), |m| json!(m))})
        } else {
            json!({ f.kind.as_str(): at })
        };
        let mut args = json!({"name": f.name.trim(), "agent": f.agent, "prompt": f.prompt, "schedule": schedule, "notify": f.notify, "on": f.on, "catch_up": f.catch_up, "webhook": f.webhook});
        if !f.id.is_empty() {
            args["id"] = json!(f.id);
        }
        json!({"t": "cmd", "id": cmd, "name": "routine.put", "args": args})
    }

    pub fn run_routine(&mut self, id: &str) -> Value {
        let cmd = self.id("app.routine_run");
        json!({"t": "cmd", "id": cmd, "name": "routine.run", "args": {"id": id}})
    }

    pub fn delete_routine(&mut self, id: &str) -> Value {
        let cmd = self.id("app.routine_delete");
        json!({"t": "cmd", "id": cmd, "name": "routine.delete", "args": {"id": id}})
    }

    /// Asks for the person's profile, for the About you page.
    pub fn load_profile(&mut self) -> Value {
        let id = self.id("app.profile_get");
        json!({"t": "cmd", "id": id, "name": "profile.get"})
    }

    pub fn save_profile(&mut self, text: &str) -> Value {
        let id = self.id("app.profile_put");
        json!({"t": "cmd", "id": id, "name": "profile.put", "args": {"text": text}})
    }

    /// Asks for what the selected agent remembers, for the memory page.
    pub fn load_notes(&mut self) -> Option<Value> {
        let agent = self.selected.clone()?;
        let id = self.id("app.memory_list");
        Some(
            json!({"t": "cmd", "id": id, "name": "memory.list", "args": {"agent": agent, "limit": 500}}),
        )
    }

    /// Adds a note for the selected agent, or for all of them.
    pub fn add_note(&mut self, text: &str, shared: bool) -> Option<Value> {
        let agent = if shared {
            String::new()
        } else {
            self.selected.clone()?
        };
        let id = self.id("app.memory_put");
        Some(
            json!({"t": "cmd", "id": id, "name": "memory.put", "args": {"text": text.trim(), "agent": agent}}),
        )
    }

    pub fn delete_note(&mut self, note: i64) -> Value {
        let id = self.id("app.memory_delete");
        json!({"t": "cmd", "id": id, "name": "memory.delete", "args": {"id": note}})
    }

    /// Asks for the selected agent's activity, for the activity page.
    pub fn load_activity(&mut self) -> Option<Value> {
        let agent = self.selected.clone()?;
        self.activity.clear();
        let id = self.id("app.activity");
        Some(
            json!({"t": "cmd", "id": id, "name": "activity.list", "args": {"agent": agent, "limit": 200}}),
        )
    }

    /// Who made a revision, as the edit page says it.
    pub fn made_by(&self, by: &str) -> String {
        if let Some(a) = self.agent(by) {
            return a.name.clone();
        }
        if by.starts_with("a_") {
            return "An agent".into();
        }
        "You".into()
    }

    pub fn set_permission(&mut self, permission: &str) -> Option<Value> {
        let agent = self.selected.clone()?;
        let id = self.id("app.permission");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "permission": permission}}),
        )
    }

    pub fn cancel(&self) -> Option<Value> {
        let conversation = self.conversation()?;
        let run = self.runs.iter().find(|r| r.conversation == conversation)?;
        Some(json!({"t": "cancel", "run": run.id}))
    }

    pub fn busy(&self) -> bool {
        let conversation = self.conversation();
        self.runs
            .iter()
            .any(|r| Some(&r.conversation) == conversation.as_ref())
    }
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Desktop".into())
}

pub fn approve(call: &str, allow: bool, always: bool) -> Value {
    json!({"t": "approve", "call": call, "allow": allow, "always": always})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_panel() -> Model {
        let mut m = Model::new();
        m.panel = Some(OpenPanel {
            plugin: Plugin {
                id: String::from("board"),
                ..Plugin::default()
            },
            files: Vec::new(),
        });
        m
    }

    #[test]
    fn the_door_sends_as_its_plugin() {
        let mut m = with_panel();
        let put = m
            .door(1, "data.put", &json!({"key": "k", "value": 3}))
            .unwrap();
        assert_eq!(put["name"], "plugin.data.put");
        assert_eq!(put["args"]["plugin"], "board");
        let cmd = m.door(2, "cmd", &json!({"name": "routine.list"})).unwrap();
        assert_eq!(cmd["name"], "plugin.cmd");
        assert_eq!(cmd["args"]["plugin"], "board");
        assert_eq!(cmd["args"]["name"], "routine.list");
        let said = m
            .door(3, "message_agent", &json!({"agent": "Main", "text": "hi"}))
            .unwrap();
        assert_eq!(said["name"], "plugin.message");
        assert_eq!(said["args"]["text"], "hi");
        assert!(m.door(4, "fetch", &json!({})).is_err());
    }

    #[test]
    fn a_message_is_answered_with_the_reply() {
        let mut m = with_panel();
        let said = m
            .door(8, "message_agent", &json!({"agent": "Main", "text": "hi"}))
            .unwrap();
        m.result(
            &json!({"t": "result", "id": said["id"], "ok": true, "value": {"reply": "hello"}}),
        );
        assert_eq!(m.door_answers, vec![(8, Ok(json!("hello")))]);
    }

    #[test]
    fn the_door_answers_by_message() {
        let mut m = with_panel();
        let get = m.door(7, "data.get", &json!({"key": "k"})).unwrap();
        m.result(&json!({"t": "result", "id": get["id"], "ok": true, "value": {"value": 41}}));
        assert_eq!(m.door_answers, vec![(7, Ok(json!(41)))]);
    }

    #[test]
    fn a_move_waits_for_the_core_and_a_failure_is_said() {
        let mut m = Model::new();
        assert!(m.move_instance("", "loom-move-1").is_none());
        let f = m
            .move_instance("https://new.example", "loom-move-1")
            .unwrap();
        assert_eq!(f["name"], "instance.move");
        assert_eq!(m.moving.as_deref(), Some("https://new.example"));
        m.result(&json!({"t": "result", "id": f["id"], "ok": false, "error": {"code": "move_failed", "message": "refused"}}));
        assert!(m.moving.is_none());
        assert!(m.notices.last().unwrap().contains("refused"));
        m.move_instance("https://new.example", "loom-move-1");
        m.take(&json!({"t": "core.moved", "address": "https://new.example"}));
        assert!(m.moving.is_none());
        assert!(m
            .notices
            .last()
            .unwrap()
            .contains("moved to https://new.example"));
    }

    #[test]
    fn the_push_relay_is_asked_set_and_turned_off() {
        let mut m = Model::new();
        let ask = m.set_relay(None);
        assert_eq!(ask["name"], "instance.relay");
        assert_eq!(ask["args"], json!({}));
        m.result(&json!({"t": "result", "id": ask["id"], "ok": true, "value": {"on": false}}));
        assert_eq!(m.relay.as_deref(), Some(""));
        let on = m.set_relay(Some("https://push.loom.example"));
        assert_eq!(on["args"], json!({"url": "https://push.loom.example"}));
        m.result(&json!({"t": "result", "id": on["id"], "ok": true, "value": {"on": true, "url": "https://push.loom.example", "key": "k"}}));
        assert_eq!(m.relay.as_deref(), Some("https://push.loom.example"));
        let off = m.set_relay(Some(""));
        m.result(&json!({"t": "result", "id": off["id"], "ok": false, "error": {"code": "not_allowed", "message": "Only the instance's owner"}}));
        assert_eq!(m.relay.as_deref(), Some("https://push.loom.example"));
        assert!(m
            .notices
            .last()
            .unwrap()
            .contains("Only the instance's owner"));
    }

    #[test]
    fn a_reply_in_another_chat_is_unread_until_it_is_opened() {
        let mut m = Model::new();
        m.conversations = vec![
            Conversation {
                id: "c_1".into(),
                agent: "a_1".into(),
            },
            Conversation {
                id: "c_2".into(),
                agent: "a_2".into(),
            },
        ];
        m.select(Some("a_1".into()));
        let reply = |conversation: &str, agent: &str| json!({"t": "event", "kind": "message.created", "conversation": conversation, "message": {"id": "msg_1", "conversation": conversation, "role": "agent", "agent": agent}});
        // During catch-up, replies were seen before.
        m.take(&reply("c_2", "a_2"));
        assert!(m.unread.is_empty());
        m.synced = true;
        m.take(&reply("c_1", "a_1"));
        m.take(&json!({"t": "event", "kind": "message.created", "conversation": "c_2", "message": {"id": "msg_2", "conversation": "c_2", "role": "person"}}));
        assert!(m.unread.is_empty());
        m.take(&reply("c_2", "a_2"));
        assert!(m.unread.contains("a_2"));
        m.select(Some("a_2".into()));
        assert!(m.unread.is_empty());
    }

    #[test]
    fn phones_are_asked_turned_on_and_off() {
        let mut m = Model::new();
        let ask = m.set_phones(None);
        assert_eq!(ask["name"], "instance.phones");
        assert_eq!(ask["args"], json!({}));
        m.result(&json!({"t": "result", "id": ask["id"], "ok": true, "value": {"on": false}}));
        assert_eq!(m.phones, Some(false));
        let on = m.set_phones(Some(true));
        m.result(&json!({"t": "result", "id": on["id"], "ok": true, "value": {"on": true, "port": 47420, "key": "k"}}));
        assert_eq!(m.phones, Some(true));
        m.link = Some(("loom://link?c=1".into(), 0));
        let off = m.set_phones(Some(false));
        assert!(m.link.is_none());
        m.result(&json!({"t": "result", "id": off["id"], "ok": false, "error": {"code": "not_allowed", "message": "Only the owner"}}));
        assert_eq!(m.phones, Some(true));
        assert!(m.notices.last().unwrap().contains("Only the owner"));
    }

    #[test]
    fn people_and_invites_are_listed_and_an_invite_is_kept_to_pass_on() {
        let mut m = Model::new();
        let [people, invites] = m.load_people().try_into().unwrap();
        assert_eq!(people["name"], "user.list");
        m.result(&json!({"t": "result", "id": people["id"], "ok": true, "value": {"users": [{"user": "u_1", "username": "elliot", "role": "owner"}, {"user": "u_2", "username": "ann", "role": "member"}]}}));
        m.result(&json!({"t": "result", "id": invites["id"], "ok": true, "value": {"invites": [{"invite": "inv_1", "role": "member", "uses": 2, "expires": 9}]}}));
        assert_eq!(m.people.len(), 2);
        assert_eq!(m.people[1].username, "ann");
        assert_eq!(m.invites[0].uses, 2);
        let made = m.create_invite("admin", 1);
        assert_eq!(made["args"], json!({"role": "admin", "uses": 1}));
        m.result(&json!({"t": "result", "id": made["id"], "ok": true, "value": {"invite": "inv_2", "code": "ab", "link": "https://loom.example.com/join/ab", "role": "admin", "uses": 1, "expires": 5}}));
        assert_eq!(
            m.invite_made,
            Some(("https://loom.example.com/join/ab".into(), "admin".into(), 5))
        );
        let reset = m.reset_password("u_2", "ann_b");
        assert_eq!(reset["name"], "password.reset");
        m.result(&json!({"t": "result", "id": reset["id"], "ok": true, "value": {"code": "cd", "link": "https://loom.example.com/reset/cd", "expires": 7}}));
        assert_eq!(
            m.reset_made,
            Some((
                "ann_b".into(),
                "https://loom.example.com/reset/cd".into(),
                7
            ))
        );
        // A member is told why there's nothing to see.
        let [people, _] = m.load_people().try_into().unwrap();
        m.result(&json!({"t": "result", "id": people["id"], "ok": false, "error": {"code": "not_allowed", "message": "Only the owner and admins"}}));
        assert!(m.people.is_empty());
        assert!(m
            .people_refused
            .as_deref()
            .unwrap()
            .contains("Only the owner"));
    }

    #[test]
    fn deleting_the_account_says_why_when_refused() {
        let mut m = Model::new();
        let f = m.delete_account();
        assert_eq!(f["name"], "account.delete");
        m.result(&json!({"t": "result", "id": f["id"], "ok": false, "error": {"code": "not_allowed", "message": "The owner's account goes with the instance."}}));
        assert!(m
            .notices
            .iter()
            .any(|n| n.contains("goes with the instance")));
    }

    #[test]
    fn a_program_shared_with_a_budget() {
        let mut m = Model::new();
        let list = m.programs();
        m.result(&json!({"t": "result", "id": list["id"], "ok": true, "value": {"programs": [
            {"name": "claude", "source": "", "version": "", "settings": ["ANTHROPIC_API_KEY"], "share": {"tokens": 5000, "per": "day", "left": 1200}},
            {"name": "codex", "source": "", "version": "", "settings": []}
        ]}}));
        assert_eq!(
            m.installed[0].share,
            Some(Share {
                tokens: 5000,
                per: "day".into(),
                left: Some(1200)
            })
        );
        assert_eq!(m.installed[1].share, None);
        let shared = m.share_program("codex", 100, "month");
        assert_eq!(shared["name"], "acp.share");
        assert_eq!(
            shared["args"],
            json!({"name": "codex", "tokens": 100, "per": "month"})
        );
    }

    #[test]
    fn no_door_without_a_panel() {
        let mut m = Model::new();
        assert!(m.door(1, "data.get", &json!({"key": "k"})).is_err());
    }
}
