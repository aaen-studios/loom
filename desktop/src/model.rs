//! What the screens show, as the core tells it. The model keeps no state of its own
//! beyond that: lists come from the core's cache (`core.get`), and are fetched again
//! when an event says they changed. Only the text of a reply still being written
//! lives here alone, from its deltas, until the finished message replaces it.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
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
    /// Its own permission mode, or "" when it follows the person's default.
    pub permission: String,
    /// Its own answer to a message while it's busy (queue or steer), or "" when it
    /// follows the person's default.
    pub busy: String,
    /// The program it runs on.
    pub program: String,
    /// Its own backup program, which takes over when its own runs out of usage or
    /// doesn't start: "" when it follows the person's default, "none" for none.
    pub backup: String,
    pub instructions: String,
    /// The names of the person's MCP servers it has.
    pub mcp: Vec<String>,
    /// What its program offers to pick: its model, reasoning effort and the like.
    pub options: Vec<ConfigOption>,
    /// What the person picked, by option id; the rest follow the defaults.
    pub config: Vec<(String, String)>,
    /// The person's defaults it follows where it has nothing of its own: the picks
    /// for its program, and the permission mode.
    pub defaulted: Vec<(String, String)>,
    pub default_mode: String,
    /// When it was pinned to the top of the list, if it is.
    pub pinned: Option<i64>,
    /// Whether it reaches tools through scripts, search_tools and run_code (Code
    /// Mode, the default), rather than each tool plainly.
    pub code_mode: bool,
    /// Whether it runs in a sandbox: every member's agent does, and the owner's marked
    /// isolated.
    pub isolated: bool,
    /// The hosts it may reach from its sandbox, or None for everything; empty is none.
    pub network: Option<Vec<String>>,
}

/// One of the things an agent's program lets the person pick, such as its model.
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigOption {
    pub id: String,
    pub name: String,
    /// "model", "thought_level", "mode" or another the program names.
    pub category: String,
    /// What a new session starts on.
    pub default: String,
    /// Each value: what's sent, and its name.
    pub values: Vec<(String, String)>,
}

impl Agent {
    /// The value option `id` is on: the agent's own pick, or the person's default for
    /// its program where the program offers it, or what the program starts on.
    pub fn chosen(&self, id: &str) -> String {
        if let Some((_, v)) = self.config.iter().find(|(k, _)| k == id) {
            return v.clone();
        }
        self.followed(id)
    }

    /// The value option `id` is on while it follows the default.
    pub fn followed(&self, id: &str) -> String {
        let option = self.options.iter().find(|o| o.id == id);
        if let Some((_, v)) = self.defaulted.iter().find(|(k, _)| k == id) {
            if option.is_some_and(|o| o.values.iter().any(|(value, _)| value == v)) {
                return v.clone();
            }
        }
        option.map(|o| o.default.clone()).unwrap_or_default()
    }

    /// Whether option `id` follows the default, with no pick of the agent's own.
    pub fn follows(&self, id: &str) -> bool {
        !self.config.iter().any(|(k, _)| k == id)
    }

    /// The permission mode it works in: its own, or the default's.
    pub fn mode(&self) -> &str {
        if self.permission.is_empty() {
            &self.default_mode
        } else {
            &self.permission
        }
    }

    /// The options the picker shows: the model, then the reasoning effort.
    pub fn picks(&self) -> Vec<ConfigOption> {
        let mut out: Vec<ConfigOption> = Vec::new();
        for category in ["model", "thought_level"] {
            out.extend(
                self.options
                    .iter()
                    .filter(|o| o.category == category && !o.values.is_empty())
                    .cloned(),
            );
        }
        out
    }

    /// What the picker button says, as "Opus · High"; None when there's nothing to pick.
    pub fn picked(&self) -> Option<String> {
        let names: Vec<String> = self
            .picks()
            .iter()
            .map(|o| {
                let v = self.chosen(&o.id);
                o.values
                    .iter()
                    .find(|(value, _)| *value == v)
                    .map_or(v, |(_, name)| name.clone())
            })
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            None
        } else {
            Some(names.join(" · "))
        }
    }
}

fn config_options(a: &Value) -> Vec<ConfigOption> {
    a.get("options")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|o| ConfigOption {
                    id: s(o, "id"),
                    name: s(o, "name"),
                    category: s(o, "category"),
                    default: s(o, "value"),
                    values: o
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|vs| vs.iter().map(|v| (s(v, "value"), s(v, "name"))).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn config_values(a: &Value) -> Vec<(String, String)> {
    a.get("config")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// The person's agent defaults: what every agent of theirs follows where it has
/// nothing of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Defaults {
    /// The program a new agent runs on, or "" for none.
    pub brain: String,
    /// The program an agent's turns move to when its own runs out of usage or
    /// doesn't start, for agents without a backup of their own; "" for none.
    pub backup: String,
    /// Per program, the picks its agents start on, by option id.
    pub config: Vec<(String, Vec<(String, String)>)>,
    pub permission: String,
    pub instructions: String,
    /// "queue" or "steer".
    pub busy: String,
    pub revision: i64,
}

impl Defaults {
    pub fn of(d: &Value) -> Defaults {
        let config = d
            .get("config")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(program, picks)| {
                        (program.clone(), config_values(&json!({"config": picks})))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let or = |v: String, fallback: &str| {
            if v.is_empty() {
                fallback.to_string()
            } else {
                v
            }
        };
        Defaults {
            brain: s(d, "brain"),
            backup: s(d, "backup"),
            config,
            permission: or(s(d, "permission"), "ask"),
            instructions: s(d, "instructions"),
            busy: or(s(d, "busy"), "queue"),
            revision: d.get("revision").and_then(Value::as_i64).unwrap_or(0),
        }
    }

    /// The picks for `program`'s agents, by option id.
    pub fn picks(&self, program: &str) -> Vec<(String, String)> {
        self.config
            .iter()
            .find(|(p, _)| p == program)
            .map(|(_, picks)| picks.clone())
            .unwrap_or_default()
    }

    /// The pick for `program`'s option `option`, if there is one.
    pub fn pick(&self, program: &str, option: &str) -> Option<String> {
        self.picks(program)
            .into_iter()
            .find(|(k, _)| k == option)
            .map(|(_, v)| v)
    }
}

/// One of the person's rules on what agents touch, as the instance gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rule {
    pub id: String,
    /// The agent it's for, or "" for every agent.
    pub agent: String,
    /// allow, ask or deny.
    pub then: String,
    /// The rule in words, as the instance says it.
    pub said: String,
    pub note: String,
}

impl Rule {
    pub fn of(r: &Value) -> Rule {
        Rule {
            id: s(r, "id"),
            agent: s(r, "agent"),
            then: s(r, "then"),
            said: s(r, "said"),
            note: s(r, "note"),
        }
    }

    pub fn all(list: &Value) -> Vec<Rule> {
        list.as_array()
            .map(|l| l.iter().map(Rule::of).collect())
            .unwrap_or_default()
    }
}

/// What the what-if said an action would get.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WhatIf {
    /// allow, ask or deny, or loom when loom's own rules decide.
    pub outcome: String,
    /// rule, mode or loom: what decided.
    pub decided: String,
    pub why: String,
}

impl WhatIf {
    pub fn of(v: &Value) -> WhatIf {
        WhatIf {
            outcome: s(v, "outcome"),
            decided: s(v, "decided"),
            why: s(v, "why"),
        }
    }

    /// The outcome in a word or two.
    pub fn verdict(&self) -> &'static str {
        match self.outcome.as_str() {
            "allow" => "Allowed",
            "ask" => "Asks you",
            "deny" => "Denied",
            _ => "Loom's own rules",
        }
    }
}

/// What a permission mode is, in words.
pub fn mode_name(mode: &str) -> &'static str {
    match mode {
        "allow" => "Doesn't ask",
        "auto" => "Asks when it's risky",
        "ask" | "" => "Asks first",
        _ => "Allows some kinds",
    }
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
    /// To the owner, the people it's kept to, by id; none is everyone.
    pub members: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub tokens: i64,
    pub per: String,
    pub left: Option<i64>,
    /// To the owner, the people the settings go to, by id; none is everyone.
    pub members: Option<Vec<String>>,
}

/// The ids in a JSON list, or none when it isn't one: everyone.
fn ids(v: Option<&Value>) -> Option<Vec<String>> {
    v.and_then(Value::as_array).map(|list| {
        list.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// Who a program or its settings go to once `who` is clicked: everyone for none,
/// otherwise that person added or taken away. From everyone, it's that person alone.
pub fn toggle_member(now: Option<&[String]>, who: Option<&str>) -> Option<Vec<String>> {
    let who = who?;
    let Some(list) = now else {
        return Some(vec![who.to_string()]);
    };
    let mut list = list.to_vec();
    match list.iter().position(|id| id == who) {
        Some(at) => {
            list.remove(at);
        }
        None => list.push(who.to_string()),
    }
    Some(list)
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
    /// Its icon, an SVG in currentColor, as the instance fetched it.
    pub icon: Option<String>,
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

/// An entry of the people log: what happened, who did it ("" for the command line), and
/// when (Unix ms).
#[derive(Clone, Debug, PartialEq)]
pub struct PeopleEntry {
    pub text: String,
    pub by: String,
    pub at: i64,
}

/// What an agent has, counted, for its settings panel (`agent.summary`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub memories: i64,
    pub skills: i64,
    pub tables: i64,
    pub files: i64,
    pub bytes: i64,
    pub routines: i64,
    pub tokens_today: i64,
    pub tokens_month: i64,
}

/// A count of tokens as the panel shows it: "850", "12.4k", "1.2M".
pub fn tokens_text(n: i64) -> String {
    match n {
        n if n < 1000 => n.to_string(),
        n if n < 1_000_000 => format!("{:.1}k", n as f64 / 1000.0),
        n => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

/// One of the person's skills.
#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub body: String,
}

/// The composer's text with a skill picked from the + menu: the message
/// starts by asking for it, before what was written already.
pub fn with_skill(name: &str, text: &str) -> String {
    let ask = format!("Use the {name} skill.");
    let rest = text.trim_start();
    if rest.starts_with(&ask) {
        return text.to_string();
    }
    if rest.is_empty() {
        format!("{ask} ")
    } else {
        format!("{ask} {rest}")
    }
}

/// One of the person's shared tables (PLAN.md, "Shared tables").
#[derive(Clone, Debug, PartialEq)]
pub struct SharedTable {
    pub name: String,
    pub description: String,
    /// The agent that made it, or empty for the person.
    pub owner: String,
    pub rows: u64,
}

/// A row of the open shared table: its fields as JSON, who last wrote it, and whether
/// that was another agent than the table's owner.
#[derive(Clone, Debug, PartialEq)]
pub struct TableRow {
    pub id: i64,
    pub by: String,
    pub untrusted: bool,
    pub data: String,
}

/// A row's fields as a person reads them: `title: Emma · read: false`, a string as it
/// is and anything else as JSON.
fn fields_of(data: &Value) -> String {
    let Some(fields) = data.as_object() else {
        return data.to_string();
    };
    fields
        .iter()
        .map(|(k, v)| match v.as_str() {
            Some(text) => format!("{k}: {text}"),
            None => format!("{k}: {v}"),
        })
        .collect::<Vec<_>>()
        .join(" · ")
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
    /// How its last run went: running, done, quiet (nothing new), failed, skipped, or "" before
    /// the first.
    pub last_status: String,
    pub last_at: i64,
    pub last_error: String,
    /// Where it posts: "chat", its agent's chat, or "thread", a thread of its own.
    pub post: String,
    /// The conversation it last posted in: its thread, for one that posts into one.
    pub conversation: String,
    /// What happens to its answer (PLAN.md, "Routine flows"): the steps after it.
    pub flow: Vec<FlowStep>,
}

/// A step of a routine's flow: its agent is asked `prompt` once the steps it comes
/// `after` have answered ("routine" is the routine itself).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowStep {
    pub name: String,
    pub agent: String,
    pub prompt: String,
    pub after: Vec<String>,
}

impl FlowStep {
    fn of(v: &Value) -> FlowStep {
        FlowStep {
            name: s(v, "name"),
            agent: s(v, "agent"),
            prompt: s(v, "prompt"),
            after: v
                .get("after")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Whether `flow` goes one step after another, each after the one before it: what the
/// routine form edits. A flow an agent made with steps side by side is shown, not edited.
pub fn flow_is_chain(flow: &[FlowStep]) -> bool {
    let mut before = "routine";
    for step in flow {
        if step.after != [before.to_string()] {
            return false;
        }
        before = &step.name;
    }
    true
}

/// A routine's flow while it runs: how far each step has got.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunningFlow {
    pub id: String,
    pub routine: String,
    pub stopped: bool,
    /// Each step's name, agent and status: waiting, running, done, failed or skipped.
    /// The routine's own run is first, named "routine".
    pub steps: Vec<(String, String, String)>,
}

impl RunningFlow {
    fn of(v: &Value) -> RunningFlow {
        RunningFlow {
            id: s(v, "id"),
            routine: s(v, "routine"),
            stopped: v.get("stopped").and_then(Value::as_bool).unwrap_or(false),
            steps: v
                .get("steps")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| (s(x, "name"), s(x, "agent"), s(x, "status")))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
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
            post: r
                .get("post")
                .and_then(Value::as_str)
                .unwrap_or("chat")
                .to_string(),
            conversation: s(r, "conversation"),
            flow: r
                .get("flow")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(FlowStep::of).collect())
                .unwrap_or_default(),
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
    /// "chat" or "thread". Empty leaves it as it is.
    pub post: String,
    /// The flow after it, or none to leave it as it is. An empty one takes it off.
    pub flow: Option<Vec<FlowStep>>,
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
    /// For an action recorded before it happened: started, done, failed or refused. Empty for
    /// an entry that only says what happened.
    pub status: String,
    /// Why it failed or was refused; a refusal's starts with its rule.
    pub why: String,
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
    /// The task it's the conversation of, if it's one: not the agent's chat.
    pub task: Option<String>,
    /// Where side questions about the agent's chat are asked: not its chat either.
    pub side: bool,
    /// For a branch: the chat it goes on from, and the message it branches at.
    pub branch_of: Option<String>,
    pub branch_at: Option<String>,
    /// For a routine's thread of its own: the routine. Not the agent's chat either.
    pub routine: Option<String>,
    /// The program it was moved to, by hand or to its agent's backup, in place of
    /// its agent's own.
    pub program: Option<String>,
    pub title: String,
}

/// A side question about an agent's chat, and its answer once it's come
/// (protocol.md, "Side questions"). Kept while the app runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aside {
    pub id: String,
    pub agent: String,
    pub question: String,
    pub answer: Option<String>,
    pub error: Option<String>,
}

/// A long job an agent works on in a conversation of its own (protocol.md, "Tasks").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Task {
    pub id: String,
    pub agent: String,
    pub conversation: String,
    pub title: String,
    /// running, paused, done, failed or cancelled.
    pub status: String,
    pub result: String,
    pub error: String,
    pub attempts: i64,
    pub updated: i64,
}

impl Task {
    fn of(t: &Value) -> Task {
        Task {
            id: s(t, "id"),
            agent: s(t, "agent"),
            conversation: s(t, "conversation"),
            title: s(t, "title"),
            status: s(t, "status"),
            result: s(t, "result"),
            error: s(t, "error"),
            attempts: t.get("attempts").and_then(Value::as_i64).unwrap_or(1),
            updated: t.get("updated").and_then(Value::as_i64).unwrap_or(0),
        }
    }
}

/// A skill or a note an agent proposes, which waits for the person to keep or drop it
/// (protocol.md, "Teaching a task, and proposals").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub agent: String,
    /// skill or note.
    pub kind: String,
    /// A skill's name, when it's for, and how; or a note's text, its site and whether
    /// every agent reads it.
    pub name: String,
    pub description: String,
    pub body: String,
    pub text: String,
    pub site: String,
    pub shared: bool,
    pub why: String,
    /// What the person did, a line a step, for a skill written from a recording.
    pub steps: Vec<String>,
}

impl Proposal {
    fn of(p: &Value) -> Proposal {
        let skill = p.get("skill").cloned().unwrap_or(Value::Null);
        let note = p.get("note").cloned().unwrap_or(Value::Null);
        let steps = p
            .get("steps")
            .and_then(Value::as_array)
            .map(|ss| ss.iter().filter_map(step_line).collect())
            .unwrap_or_default();
        Proposal {
            id: s(p, "id"),
            agent: s(p, "agent"),
            kind: s(p, "kind"),
            name: s(&skill, "name"),
            description: s(&skill, "description"),
            body: s(&skill, "body"),
            text: s(&note, "text"),
            site: s(&note, "site"),
            shared: note.get("shared").and_then(Value::as_bool).unwrap_or(false),
            why: s(p, "why"),
            steps,
        }
    }

    /// What it's called in the list: a skill's name, or the note.
    pub fn title(&self) -> String {
        if self.kind == "skill" {
            self.name.clone()
        } else {
            self.text.clone()
        }
    }
}

/// One step of a recording, as the person reads it.
fn step_line(step: &Value) -> Option<String> {
    let what = match s(step, "kind").as_str() {
        "page" => format!("On {}", s(step, "title")),
        "click" => format!("Clicked {}", s(step, "on")),
        "type" if step.get("secret").and_then(Value::as_bool) == Some(true) => {
            format!("Typed something secret into {}", s(step, "into"))
        }
        "type" => format!(
            "Typed \u{201c}{}\u{201d} into {}",
            s(step, "text"),
            s(step, "into")
        ),
        "key" => format!("Pressed {}", s(step, "key")),
        "scroll" => format!("Scrolled {}", s(step, "direction")),
        _ => return None,
    };
    Some(what)
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: String,
    pub text: String,
    pub call: Option<String>,
    pub file: Option<FileRef>,
    pub card: Option<Card>,
}

impl Block {
    fn text(kind: &str, text: String) -> Block {
        Block {
            kind: kind.into(),
            text,
            call: None,
            file: None,
            card: None,
        }
    }

    fn of(b: &Value) -> Block {
        let kind = s(b, "kind");
        Block {
            text: if kind == "plan" {
                plan_text(b)
            } else {
                s(b, "text")
            },
            call: b.get("call").and_then(Value::as_str).map(str::to_string),
            file: (kind == "file").then(|| FileRef::of(b)),
            card: if kind == "card" {
                b.get("card").and_then(Card::of)
            } else {
                None
            },
            kind,
        }
    }
}

/// A title as it reads after other words: "Clean out the build" goes on "Wants to clean
/// out the build". A first word in capitals, such as "README" or "PR", stays as it is.
fn mid_sentence(title: &str) -> String {
    let mut c = title.chars();
    match (c.next(), c.next()) {
        (Some(first), Some(second)) if first.is_uppercase() && !second.is_uppercase() => {
            first.to_lowercase().chain(title.chars().skip(1)).collect()
        }
        _ => title.to_string(),
    }
}

/// A plan's entries (ACP's `plan`, kept in its block as `entries`), a line each, starting
/// with how far it is: `done`, `doing` or `todo`, then a space and what it is.
fn plan_text(b: &Value) -> String {
    let Some(entries) = b.get("entries").and_then(Value::as_array) else {
        return s(b, "text");
    };
    entries
        .iter()
        .map(|e| {
            let state = match s(e, "status").as_str() {
                "completed" => "done",
                "in_progress" => "doing",
                _ => "todo",
            };
            format!("{state} {}", s(e, "content").trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A card an agent put in its reply with show_card (PLAN.md, "Cards in the chat"),
/// as the instance tidied it.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub body: CardBody,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CardBody {
    /// Answers, each a button that sends it as the person's next message.
    Choices(Vec<String>),
    Checklist(Vec<(String, bool)>),
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// How far along, 0 to 1, and what's going on.
    Progress {
        label: String,
        value: f32,
    },
    Facts(Vec<(String, String)>),
}

impl Card {
    pub fn of(v: &Value) -> Option<Card> {
        let strings = |v: Option<&Value>| -> Vec<String> {
            v.and_then(Value::as_array)
                .map(|xs| {
                    xs.iter()
                        .map(|x| x.as_str().unwrap_or_default().to_string())
                        .collect()
                })
                .unwrap_or_default()
        };
        let items = v.get("items").and_then(Value::as_array);
        let body = match v.get("type").and_then(Value::as_str)? {
            "choices" => CardBody::Choices(strings(v.get("options"))),
            "checklist" => CardBody::Checklist(
                items?
                    .iter()
                    .map(|i| (s(i, "text"), i.get("done") == Some(&Value::Bool(true))))
                    .collect(),
            ),
            "facts" => CardBody::Facts(
                items?
                    .iter()
                    .map(|i| (s(i, "label"), s(i, "value")))
                    .collect(),
            ),
            "table" => CardBody::Table {
                columns: strings(v.get("columns")),
                rows: v
                    .get("rows")
                    .and_then(Value::as_array)
                    .map(|rs| rs.iter().map(|r| strings(Some(r))).collect())
                    .unwrap_or_default(),
            },
            "progress" => CardBody::Progress {
                label: s(v, "label"),
                value: v
                    .get("value")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0) as f32,
            },
            _ => return None,
        };
        Some(Card {
            id: s(v, "id"),
            title: s(v, "title"),
            body,
        })
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

    /// Whether it's a page, as an agent's html_render makes: the chat opens it beside
    /// itself, walled off, not in a browser.
    pub fn page(&self) -> bool {
        self.mime.split(';').next().map(str::trim) == Some("text/html")
    }

    /// What a page is called: its name without ".html".
    pub fn title(&self) -> String {
        let t = self.name.strip_suffix(".html").unwrap_or(&self.name);
        if t.is_empty() {
            "Page".into()
        } else {
            t.to_string()
        }
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

/// What a person typed, for the Markdown view to show as they typed it: each `<` outside
/// code is escaped, so HTML shows as its tags instead of being dropped. Fenced code blocks
/// and code spans keep their text, since Markdown shows those as they are already.
pub fn literal_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // The open fence's character and length, inside a fenced code block.
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indented = line.len() - trimmed.len() >= 4;
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map_or(0, |m| trimmed.chars().take_while(|c| *c == m).count());
        if let Some((m, n)) = fence {
            out.push_str(line);
            if !indented && marker == Some(m) && run >= n && trimmed[run..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if let Some(m) = marker.filter(|_| !indented && run >= 3) {
            fence = Some((m, run));
            out.push_str(line);
            continue;
        }
        escape_tags(line, &mut out);
    }
    out
}

/// One line of `literal_html` outside a fence: `<` escaped, but not in a code span or after
/// a backslash.
fn escape_tags(line: &str, out: &mut String) {
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        match c {
            '\\' => {
                // An escape the person wrote stays theirs, with what it escapes.
                let n = 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
                out.push_str(&rest[..n]);
                rest = &rest[n..];
            }
            '`' => {
                let n = rest.chars().take_while(|c| *c == '`').count();
                // A code span runs to the next run of exactly as many backticks.
                let mut end = None;
                let mut at = n;
                while let Some(found) = rest[at..].find('`') {
                    let start = at + found;
                    let len = rest[start..].chars().take_while(|c| *c == '`').count();
                    if len == n {
                        end = Some(start + len);
                        break;
                    }
                    at = start + len;
                }
                let n = end.unwrap_or(n);
                out.push_str(&rest[..n]);
                rest = &rest[n..];
            }
            '<' => {
                out.push_str("\\<");
                rest = &rest[1..];
            }
            c => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
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
    /// Pinned in its conversation.
    pub pinned: bool,
    /// Files the reply names that are in its agent's folder, shown as chips.
    pub mentions: Vec<Mention>,
}

impl Message {
    /// A finished reply of an agent's with nothing to show: no words, thoughts, tool
    /// calls, plan, files or cards, and nothing it names or pins.
    pub fn is_empty_reply(&self) -> bool {
        self.role == "agent"
            && !self.live
            && !self.pinned
            && self.mentions.is_empty()
            && self
                .blocks
                .iter()
                .all(|b| matches!(b.kind.as_str(), "text" | "thought") && b.text.trim().is_empty())
    }
}

/// A file a reply names, as `notes/hours.md`, in its agent's folder or the shared one.
#[derive(Clone, Debug, PartialEq)]
pub struct Mention {
    pub agent: String,
    /// As the reply wrote it.
    pub path: String,
    pub name: String,
    pub size: u64,
}

impl Mention {
    /// Its type, as the chip shows it: "MD" for hours.md.
    pub fn kind(&self) -> String {
        self.name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_uppercase())
            .unwrap_or_default()
    }
}

fn mentions_of(m: &Value) -> Vec<Mention> {
    let agent = s(m, "agent");
    m.get("mentions")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|v| Mention {
                    agent: agent.clone(),
                    path: s(v, "path"),
                    name: s(v, "name"),
                    size: v.get("size").and_then(Value::as_u64).unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
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

/// A message waiting for its agent to finish (protocol.md, "The queue").
#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub id: String,
    pub text: String,
    pub files: usize,
    pub steered: bool,
}

/// An agent's browser, as `browser.state` says it.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Browser {
    pub open: bool,
    /// Whether the person has taken it over.
    pub person: bool,
    /// What the agent asked the person for, while it waits.
    pub asked: String,
    /// What the agent asked the person to type into its page, while it waits.
    pub secret: String,
    /// Whether what the person does in it is recorded, to teach the agent a task, and
    /// how many steps the recording has.
    pub recording: bool,
    pub steps: i64,
}

/// The newest picture of the watched browser's page.
#[derive(Clone, Debug)]
pub struct PageFrame {
    pub jpeg: Vec<u8>,
    pub width: f32,
    pub height: f32,
    /// Counts up with each frame, so the window decodes only a new one.
    pub seq: u64,
}

/// The instance's version against loom's newest release, as the core says it
/// (`core.server`): the core decides whether it's behind.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerVersion {
    pub loom: String,
    pub latest: String,
    pub behind: bool,
}

/// An update of the instance under way, as `instance.update` says it: its state
/// (downloading, waiting, restarting, failed or latest), the release and why it failed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerUpdate {
    pub state: String,
    pub version: String,
    pub why: String,
}

impl ServerUpdate {
    fn of(v: &Value) -> ServerUpdate {
        ServerUpdate {
            state: s(v, "state"),
            version: s(v, "version"),
            why: s(v, "why"),
        }
    }
}

/// What `instance.updates` said: whether the instance updates itself, whether this
/// person may update it (an owner or admin) or turn that on (the owner), and whether
/// it can, with why not.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Updates {
    pub auto: bool,
    pub may: bool,
    pub owner: bool,
    pub can: bool,
    pub why: String,
}

#[derive(Default)]
pub struct Model {
    /// The instance's version against the newest release, once the core has said.
    pub version: Option<ServerVersion>,
    /// An update of the instance under way, while there's one.
    pub server_update: Option<ServerUpdate>,
    /// How the instance's updates are set, once asked.
    pub updates: Option<Updates>,
    updates_stale: bool,
    /// The newest release's version, as this server's core found it, for the app to
    /// hand to its other servers' cores.
    pub release: Option<String>,
    /// Each agent's browser, by agent id.
    pub browsers: HashMap<String, Browser>,
    /// The agent whose browser this device watches.
    pub watching: Option<String>,
    pub frame: Option<PageFrame>,
    rewatch: bool,
    /// Each conversation's queue, by conversation id.
    pub queues: HashMap<String, Vec<QueueItem>>,
    queue_stale: bool,
    pub conn: Option<Conn>,
    /// The instance's address, as the core says it.
    pub server: Option<String>,
    /// Servers an agent offered and the person said yes to on this device, for the
    /// app to add.
    pub offered: Vec<String>,
    pub synced: bool,
    pub login_error: Option<String>,
    pub logging_in: bool,
    pub agents: Vec<Agent>,
    pub conversations: Vec<Conversation>,
    pub runs: Vec<Run>,
    pub tools: Rc<Vec<Tool>>,
    pub approvals: Vec<Approval>,
    pub programs: Vec<String>,
    /// Whether the instance has said which programs it has, since the last ask.
    pub programs_known: bool,
    /// The selected agent's messages, oldest first, with the queued ones last.
    pub messages: Vec<Message>,
    pub more_messages: bool,
    pub selected: Option<String>,
    /// Agents with a reply that came while their chat wasn't open, until it is: their
    /// row in the sidebar is bold.
    pub unread: HashSet<String>,
    /// Each unread agent's first reply that came while its chat was shut.
    first_unread: HashMap<String, String>,
    /// The message the open chat's NEW line is above, while that chat stays open.
    pub new_line: Option<String>,
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
    /// The instance refused to make the agent asked for last.
    pub make_failed: bool,
    /// What went wrong, for the window to show.
    pub notices: Vec<String>,
    /// Good news for the window to show, such as that something was done.
    pub said: Vec<String>,
    /// Where the instance is moving to, while the owner's instance.move is under way.
    pub moving: Option<String>,
    /// Files for the next message.
    pub attachments: Vec<Attachment>,
    /// Files the core has fetched, by SHA-256, or SHA-256 and ".thumb": where they are.
    pub files: Rc<HashMap<String, String>>,
    /// file.get requests on their way: what they fetch, and whether to open it.
    getting: HashMap<String, (String, bool)>,
    /// Files fetched to be opened, for the app to open.
    pub to_open: Vec<String>,
    /// Frames an answer led to, for the app to send.
    pub to_send: Vec<Value>,
    /// The page shown beside the chat, from an agent's html_render.
    pub page: Option<FileRef>,
    /// Whether the person's scripts are on (PLAN.md, "Scripts"), and the port the
    /// instance listens on, once asked; and a new script token, shown once.
    pub scripts: Option<(bool, u64)>,
    pub script_token: Option<String>,
    /// What's typed in search, and what the newest search found.
    pub query: String,
    pub hits: Vec<Hit>,
    search_id: String,
    /// A newer release, downloaded and checked: its version and where it is.
    pub update: Option<(String, String)>,
    /// What a check the person asked for found, for the app to say.
    pub checked: Option<String>,
    /// The activity log as CSV, once `activity.export` gives it, for the app to save.
    pub exported: Option<String>,
    /// The agent the edit page shows, and its revisions, newest first.
    pub editing: Option<String>,
    pub revisions: Vec<Revision>,
    revisions_stale: bool,
    /// What the agent the edit page shows has, counted.
    pub summary: Option<Summary>,
    /// The activity page's entries, newest first.
    pub activity: Vec<Activity>,
    /// The person's skills, by name, for the skills page.
    pub skills: Vec<Skill>,
    skills_stale: bool,
    /// What agents propose that waits for the person, newest first.
    pub proposals: Vec<Proposal>,
    proposals_stale: bool,
    /// The person's shared tables, for the tables page, and the one open on it, with
    /// its newest rows and whether there are more.
    pub tables: Vec<SharedTable>,
    pub table_open: Option<String>,
    pub table_rows: Vec<TableRow>,
    pub table_more: bool,
    /// Whether the tables page is showing, so a change to a table reloads it.
    pub tables_open: bool,
    tables_stale: bool,
    /// The person's routines, oldest first.
    pub routines: Vec<Routine>,
    routines_stale: bool,
    /// The routine flows running.
    pub flows: Vec<RunningFlow>,
    /// The person's tasks, newest first.
    pub tasks: Vec<Task>,
    /// The open conversation's pinned messages, the newest pin first, while the pins
    /// are open beside the chat, and whether they changed since.
    pub pins: Vec<Message>,
    pub pins_open: bool,
    /// Side questions asked while the app runs, oldest first.
    pub asides: Vec<Aside>,
    pins_stale: bool,
    tasks_stale: bool,
    /// The task whose conversation the chat shows instead of the agent's chat.
    pub open_task: Option<String>,
    /// The branch the chat shows, by conversation, instead of the agent's chat.
    pub open_branch: Option<String>,
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
    /// The people log, newest first (PLAN.md, "Accounts and invites").
    pub people_log: Vec<PeopleEntry>,
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
    /// The person's agent defaults, once they've come, asked for on each connect and
    /// kept up to date by `defaults.updated`.
    pub defaults: Defaults,
    defaults_stale: bool,
    /// The defaults came anew, and the Defaults page's instructions should show them.
    pub defaults_came: bool,
    /// The person's rules on what agents touch, asked for on each connect and kept up
    /// to date by `rules.updated`.
    pub rules: Vec<Rule>,
    rules_stale: bool,
    /// What the Rules page's what-if last said, until it asks again.
    pub what_if: Option<WhatIf>,
    /// The agent programs on the instance, for the programs page.
    pub installed: Vec<Program>,
    /// Whether this person is the instance's owner, who gives programs and their settings
    /// to people, as acp.list said.
    pub programs_owner: bool,
    /// Whether the instance runs sandboxes, which an isolated agent needs, as acp.list said.
    pub sandboxes: bool,
    /// The ACP Registry's agents, once it's been read.
    pub registry: Vec<Listed>,
    /// Which registry agent each program on the instance is, by the program's name.
    pub program_ids: HashMap<String, String>,
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
    /// The Import page: what each other app has on the instance's computer, once it has
    /// said, the app being imported from while it is, and how the last import went.
    pub import_sources: Vec<ImportSource>,
    pub import_loading: bool,
    pub importing: Option<String>,
    pub import_done: Option<ImportDone>,
}

/// Another agent app the instance can import from, as import.scan says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportSource {
    /// openbot or grokbot.
    pub from: String,
    pub name: String,
    pub found: bool,
    /// What it holds, in words, such as "3 agents, 120 messages".
    pub text: String,
}

impl ImportSource {
    fn of(v: &Value) -> ImportSource {
        ImportSource {
            from: s(v, "from"),
            name: s(v, "name"),
            found: v.get("found").and_then(Value::as_bool).unwrap_or(false),
            text: s(v, "text"),
        }
    }
}

/// How an import went: from where, what came in words, and what couldn't.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportDone {
    pub from: String,
    pub text: String,
    pub not: Vec<String>,
    pub error: bool,
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

/// A host as it's typed, as the list keeps it: `https://GitHub.com/x` is
/// `github.com`. The instance says whether it's a host at all.
fn host_entry(typed: &str) -> Option<String> {
    let mut h = typed.trim().to_lowercase();
    for scheme in ["https://", "http://"] {
        if let Some(rest) = h.strip_prefix(scheme) {
            h = rest.to_string();
        }
    }
    if let Some(cut) = h.find('/') {
        h.truncate(cut);
    }
    let h = h.trim_end_matches('.').to_string();
    (!h.is_empty()).then_some(h)
}

fn blocks_of(v: &Value) -> Vec<Block> {
    v.get("blocks")
        .and_then(Value::as_array)
        .map(|bs| bs.iter().map(Block::of).collect())
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
        if std::mem::take(&mut self.proposals_stale) {
            out.push(self.load_proposals());
        }
        if std::mem::take(&mut self.tables_stale) && self.tables_open {
            out.extend(self.load_tables());
        }
        if std::mem::take(&mut self.routines_stale) {
            out.push(self.load_routines());
            out.push(self.load_flows());
        }
        if std::mem::take(&mut self.plugins_stale) {
            out.push(self.load_plugins());
        }
        if self.pins_stale && self.pins_open {
            out.extend(self.load_pins());
        }
        if std::mem::take(&mut self.people_stale) {
            out.extend(self.load_people());
        }
        if std::mem::take(&mut self.defaults_stale) {
            out.push(self.load_defaults());
        }
        if std::mem::take(&mut self.rules_stale) {
            out.push(self.load_rules());
        }
        if std::mem::take(&mut self.tasks_stale) {
            out.push(self.load_tasks());
        }
        if std::mem::take(&mut self.updates_stale) {
            out.push(self.load_updates());
        }
        if std::mem::take(&mut self.queue_stale) {
            if let Some(conversation) = self.conversation() {
                out.push(json!({"t": "cmd", "id": format!("app.queue:{conversation}"), "name": "queue.list", "args": {"conversation": conversation}}));
            }
        }
        // After a reconnect the instance has forgotten what this device watched.
        if std::mem::take(&mut self.rewatch) {
            if let Some(agent) = self.watching.clone() {
                let id = self.id("browser.watch");
                out.push(json!({"t": "cmd", "id": id, "name": "browser.watch", "args": {"agent": agent, "on": true}}));
            }
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

    /// The conversation the chat shows: the open task's, or else the selected
    /// agent's chat, its newest that isn't a task's.
    pub fn conversation(&self) -> Option<String> {
        if let Some(task) = self.task_open() {
            return Some(task.conversation.clone());
        }
        // By its id, even before the list of conversations has it.
        if let Some(branch) = &self.open_branch {
            return Some(branch.clone());
        }
        let agent = self.selected.as_ref()?;
        self.conversations
            .iter()
            .find(|c| {
                &c.agent == agent
                    && c.task.is_none()
                    && !c.side
                    && c.branch_of.is_none()
                    && c.routine.is_none()
            })
            .map(|c| c.id.clone())
    }

    /// The task the chat shows, if it shows one.
    pub fn task_open(&self) -> Option<&Task> {
        let id = self.open_task.as_ref()?;
        self.tasks
            .iter()
            .find(|t| &t.id == id && Some(&t.agent) == self.selected.as_ref())
    }

    /// Shows a task's conversation in the chat, or the agent's chat again.
    pub fn show_task(&mut self, task: Option<String>) {
        if let Some(agent) = task
            .as_ref()
            .and_then(|id| self.tasks.iter().find(|t| &t.id == id))
            .map(|t| t.agent.clone())
        {
            self.select(Some(agent));
        }
        if self.open_task != task {
            self.open_task = task;
            self.open_branch = None;
            self.messages.clear();
            self.dirty.insert(List::Messages);
            self.queue_stale = true;
            self.pins.clear();
            self.pins_stale = true;
        }
    }

    /// The branch the chat shows, if it shows one.
    pub fn branch_open(&self) -> Option<&Conversation> {
        let id = self.open_branch.as_ref()?;
        self.conversations
            .iter()
            .find(|c| &c.id == id && Some(&c.agent) == self.selected.as_ref())
    }

    /// Shows a branch in the chat, or the agent's chat again.
    pub fn show_branch(&mut self, branch: Option<String>) {
        if self.open_branch != branch {
            self.open_task = None;
            self.open_branch = branch;
            self.messages.clear();
            self.dirty.insert(List::Messages);
            self.queue_stale = true;
            self.pins.clear();
            self.pins_stale = true;
        }
    }

    /// Opens what a notification was about: the agent, and the conversation in the chat's
    /// place when it's a task's, a branch or a routine's thread rather than the chat itself.
    /// A side question opens the agent's chat, which it's about.
    pub fn open_about(&mut self, agent: &str, conversation: &str) {
        if self.agent(agent).is_none() {
            return;
        }
        self.select(Some(agent.to_string()));
        let about = self
            .conversations
            .iter()
            .find(|c| c.id == conversation && c.agent == agent)
            .cloned();
        match about {
            Some(c) if c.task.is_some() => {
                self.show_branch(None);
                self.open_task = c.task;
                self.dirty.insert(List::Messages);
            }
            Some(c) if c.branch_of.is_some() || c.routine.is_some() => {
                self.show_branch(Some(c.id));
            }
            _ => self.show_branch(None),
        }
    }

    /// Branches the chat at a message: once it's made, the chat shows it.
    pub fn branch(&mut self, message: &str) -> Value {
        json!({"t": "cmd", "id": self.id("app.branch"), "name": "conversation.branch", "args": {"message": message}})
    }

    /// The branches that go on from each message of the open chat, by message: their
    /// ids and titles.
    pub fn branches(&self) -> HashMap<String, Vec<(String, String)>> {
        let mut out: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let Some(chat) = self.conversation() else {
            return out;
        };
        for c in &self.conversations {
            if c.branch_of.as_deref() == Some(chat.as_str()) {
                if let Some(at) = &c.branch_at {
                    out.entry(at.clone())
                        .or_default()
                        .push((c.id.clone(), c.title.clone()));
                }
            }
        }
        out
    }

    /// Asks for the person's tasks.
    pub fn load_tasks(&mut self) -> Value {
        json!({"t": "cmd", "id": "app.tasks", "name": "task.list"})
    }

    /// task.start, task.pause, task.resume, task.cancel or task.retry.
    pub fn task_cmd(&mut self, name: &str, args: Value) -> Value {
        let id = self.id("app.task_cmd");
        json!({"t": "cmd", "id": id, "name": name, "args": args})
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    pub fn select(&mut self, agent: Option<String>) {
        if let Some(a) = &agent {
            self.unread.remove(a);
            // The NEW line goes above the first reply that came while the chat was
            // shut, and stays while it's open.
            if let Some(first) = self.first_unread.remove(a) {
                self.new_line = Some(first);
            } else if self.selected != agent {
                self.new_line = None;
            }
        } else {
            self.new_line = None;
        }
        if self.selected != agent {
            self.selected = agent;
            self.open_task = None;
            self.open_branch = None;
            self.messages.clear();
            self.dirty.insert(List::Messages);
            self.queue_stale = true;
            self.pins.clear();
            self.pins_stale = true;
        }
    }

    /// What waits for the open conversation's agent, first first.
    pub fn queue(&self) -> Vec<QueueItem> {
        self.conversation()
            .and_then(|c| self.queues.get(&c).cloned())
            .unwrap_or_default()
    }

    fn queue_of(v: &Value) -> Vec<QueueItem> {
        v.get("queue")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|q| QueueItem {
                        id: s(q, "id"),
                        text: s(q, "text"),
                        files: q.get("files").and_then(Value::as_array).map_or(0, Vec::len),
                        steered: q.get("steered").and_then(Value::as_bool).unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A command on one item of the open conversation's queue:
    /// `queue.drop`, `queue.steer` or `queue.edit` with its new text.
    pub fn queue_cmd(&mut self, name: &str, item: &str, text: Option<&str>) -> Value {
        let id = self.id("app.queue_cmd");
        let mut args = json!({"id": item});
        if let Some(t) = text {
            args["text"] = json!(t);
        }
        json!({"t": "cmd", "id": id, "name": name, "args": args})
    }

    /// Moves an item of the open conversation's queue one place up.
    pub fn queue_up(&mut self, item: &str) -> Option<Value> {
        let conversation = self.conversation()?;
        let mut ids: Vec<String> = self.queue().into_iter().map(|q| q.id).collect();
        let k = ids.iter().position(|i| i == item)?;
        if k == 0 {
            return None;
        }
        ids.swap(k, k - 1);
        let id = self.id("app.queue_cmd");
        Some(
            json!({"t": "cmd", "id": id, "name": "queue.order", "args": {"conversation": conversation, "ids": ids}}),
        )
    }

    /// A stored message, as the instance keeps it.
    fn message_of(&self, m: &Value) -> Message {
        Message {
            id: s(m, "id"),
            role: s(m, "role"),
            at: m.get("at").and_then(Value::as_i64).unwrap_or(0),
            blocks: blocks_of(m),
            live: false,
            queued: false,
            pinned: m.get("pinned").is_some_and(|p| !p.is_null()),
            mentions: mentions_of(m),
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
        }
    }

    /// The open conversation's pinned messages, for the pins beside the chat.
    pub fn load_pins(&mut self) -> Option<Value> {
        let conversation = self.conversation()?;
        self.pins_stale = false;
        Some(
            json!({"t": "cmd", "id": "app.pins", "name": "messages.pinned", "args": {"conversation": conversation}}),
        )
    }

    /// Asks the selected agent a side question about its chat.
    pub fn ask_aside(&mut self, question: &str) -> Option<Value> {
        let question = question.trim();
        if question.is_empty() {
            return None;
        }
        let conversation = self.conversation()?;
        let agent = self.selected.clone()?;
        let id = self.id("app.aside");
        self.asides.push(Aside {
            id: id.clone(),
            agent,
            question: question.to_string(),
            answer: None,
            error: None,
        });
        Some(
            json!({"t": "cmd", "id": id, "name": "chat.ask", "args": {"conversation": conversation, "question": question}}),
        )
    }

    /// Pins a message, or takes its pin off.
    pub fn pin(&mut self, message: &str, pinned: bool) -> Value {
        json!({"t": "cmd", "id": self.id("app.pin"), "name": "message.pin", "args": {"message": message, "pinned": pinned}})
    }

    /// Takes a frame from the core. Gives whether the screen should redraw.
    pub fn take(&mut self, f: &Value) -> bool {
        let t = s(f, "t");
        match t.as_str() {
            "core.conn" => {
                if let Some(server) = f.get("server").and_then(Value::as_str) {
                    self.server = Some(server.to_string());
                }
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
                self.defaults_stale = true;
                self.rules_stale = true;
                self.rewatch = true;
                self.queue_stale = true;
                self.tasks_stale = true;
                self.updates_stale = true;
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
            "welcome" => {
                self.me = s(f, "device");
                // An update under way, to an owner or admin; none once it's done.
                self.server_update = f.get("update").map(ServerUpdate::of);
            }
            "core.server" => {
                self.version = Some(ServerVersion {
                    loom: s(f, "loom"),
                    latest: s(f, "latest"),
                    behind: f.get("behind").and_then(Value::as_bool) == Some(true),
                });
            }
            "core.release" => self.release = Some(s(f, "version")),
            "instance.update" => self.server_update = Some(ServerUpdate::of(f)),
            "core.data" => self.data(f),
            "core.queued" => {
                self.queued.push(Queued::of(f));
                self.place_queued();
            }
            "core.file" => self.file_came(f),
            "core.moved" => {
                self.moving = None;
                self.said.push(format!(
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
            "card" => self.card(f),
            "event" => self.event(f),
            "result" => self.result(f),
            "browser.state" => self.browser_state(f),
            "browser.frame" => {
                let agent = s(f, "agent");
                if self.watching.as_deref() != Some(agent.as_str()) {
                    return true;
                }
                use base64::Engine as _;
                let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(s(f, "data"))
                else {
                    return true;
                };
                let seq = self.frame.as_ref().map_or(0, |p| p.seq) + 1;
                let size = |k: &str| f.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                self.frame = Some(PageFrame {
                    jpeg,
                    width: size("width"),
                    height: size("height"),
                    seq,
                });
            }
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
            "table" => {
                if s(f, "table") == self.table_open.clone().unwrap_or_default()
                    && f.get("dropped").and_then(Value::as_bool) == Some(true)
                {
                    self.table_open = None;
                    self.table_rows.clear();
                }
                self.tables_stale = true;
            }
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
                    // A pin put on or taken off here, or a pinned message gone.
                    if kind == "message.updated" || kind == "message.deleted" {
                        self.pins_stale = true;
                    }
                } else if kind == "message.created" && self.synced && s(&message, "role") == "agent"
                    // A side question's answer is shown where it was asked.
                    && !self.conversations.iter().any(|c| c.id == conversation && c.side)
                {
                    // A reply in another chat, as it comes; catch-up is what was seen before.
                    let agent = self
                        .conversations
                        .iter()
                        .find(|c| c.id == conversation)
                        .map(|c| c.agent.clone())
                        .unwrap_or_else(|| s(&message, "agent"));
                    if !agent.is_empty() {
                        self.first_unread.entry(agent.clone()).or_insert(id);
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
            "proposal" => {
                self.proposals_stale = true;
            }
            // A flow that finished goes from the list.
            "flow" => {
                let flow = RunningFlow::of(f.get("flow").unwrap_or(&Value::Null));
                let finished = f
                    .pointer("/flow/finished")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.flows.retain(|x| x.id != flow.id);
                if !finished {
                    self.flows.push(flow);
                }
            }
            // An agent offered another server, and the person said yes here.
            "server" if kind == "server.offered" && s(f, "by") == self.me => {
                self.offered.push(s(f, "address"));
            }
            "defaults" => {
                if let Some(d) = f.get("defaults") {
                    self.take_defaults(d, false);
                }
            }
            "rules" => {
                if let Some(list) = f.get("rules") {
                    self.rules = Rule::all(list);
                    // What the what-if said may be stale now.
                    self.what_if = None;
                }
            }
            "queue" => {
                self.queues.insert(s(f, "conversation"), Model::queue_of(f));
            }
            // An agent offered another server, and the person said yes here.
            "server" if kind == "server.offered" && s(f, "by") == self.me => {
                self.offered.push(s(f, "address"));
            }
            "task" => {
                let task = Task::of(f.get("task").unwrap_or(&Value::Null));
                match self.tasks.iter_mut().find(|t| t.id == task.id) {
                    Some(t) => *t = task,
                    None => self.tasks.insert(0, task),
                }
            }
            "plugin" if kind != "plugin.data" => {
                self.plugins_stale = true;
            }
            // Someone joined with an invite: the People page lists them, and the invite's uses;
            // and the people log grew.
            "user" | "people" if self.people_open => {
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

    /// A card in a reply that's still being written, new or changed: it takes its
    /// block's place whole.
    fn card(&mut self, f: &Value) {
        let Some(card) = f.get("card").and_then(Card::of) else {
            return;
        };
        let message = s(f, "message");
        let block = f.get("block").and_then(Value::as_u64).unwrap_or(0) as usize;
        let blocks = self.live.entry(message).or_default();
        while blocks.len() <= block {
            blocks.push(Block::text("", String::new()));
        }
        blocks[block] = Block {
            card: Some(card),
            ..Block::text("card", String::new())
        };
    }

    fn browser_state(&mut self, f: &Value) {
        let agent = s(f, "agent");
        let b = Browser {
            open: f.get("open").and_then(Value::as_bool).unwrap_or(false),
            person: f.get("person").and_then(Value::as_bool).unwrap_or(false),
            asked: s(f, "asked"),
            secret: s(f, "secret"),
            recording: f.get("recording").and_then(Value::as_bool).unwrap_or(false),
            steps: f.get("steps").and_then(Value::as_i64).unwrap_or(0),
        };
        if !b.open && self.watching.as_deref() == Some(agent.as_str()) {
            self.frame = None;
        }
        self.browsers.insert(agent, b);
    }

    /// The agent's browser, as last heard.
    pub fn browser(&self, agent: &str) -> Browser {
        self.browsers.get(agent).cloned().unwrap_or_default()
    }

    /// Watches `agent`'s browser, or stops watching with None.
    pub fn watch(&mut self, agent: Option<String>) -> Option<Value> {
        if self.watching == agent {
            return None;
        }
        let id = self.id("browser.watch");
        let frame = match (&agent, &self.watching) {
            (Some(a), _) => {
                json!({"t": "cmd", "id": id, "name": "browser.watch", "args": {"agent": a, "on": true}})
            }
            (None, Some(old)) => {
                json!({"t": "cmd", "id": id, "name": "browser.watch", "args": {"agent": old, "on": false}})
            }
            (None, None) => return None,
        };
        self.watching = agent;
        self.frame = None;
        Some(frame)
    }

    /// What the person typed for the selected agent's browser_ask_secret, or
    /// "" for no.
    pub fn give_secret(&mut self, value: &str) -> Option<Value> {
        let agent = self.selected.clone()?;
        if let Some(b) = self.browsers.get_mut(&agent) {
            b.secret.clear();
        }
        let id = self.id("browser.hand");
        Some(
            json!({"t": "cmd", "id": id, "name": "browser.secret", "args": {"agent": agent, "value": value}}),
        )
    }

    /// Starts or stops recording what the person does in the watched browser, which
    /// they've taken over, to teach its agent the task.
    pub fn record_browser(&mut self, on: bool) -> Option<Value> {
        let agent = self.watching.clone()?;
        let id = self.id("browser.hand");
        Some(
            json!({"t": "cmd", "id": id, "name": "browser.record", "args": {"agent": agent, "on": on}}),
        )
    }

    /// Takes over the watched browser, or hands it back.
    pub fn take_browser(&mut self, take: bool) -> Option<Value> {
        let agent = self.watching.clone()?;
        let id = self.id("browser.hand");
        let name = if take { "browser.take" } else { "browser.give" };
        Some(json!({"t": "cmd", "id": id, "name": name, "args": {"agent": agent}}))
    }

    /// The person's click, wheel, key or text, on the browser they've taken over.
    pub fn browser_input(&mut self, mut input: Value) -> Option<Value> {
        let agent = self.watching.clone()?;
        if !self.browser(&agent).person {
            return None;
        }
        input["agent"] = json!(agent);
        let id = self.id("browser.input");
        Some(json!({"t": "cmd", "id": id, "name": "browser.input", "args": input}))
    }

    fn result(&mut self, f: &Value) {
        let id = s(f, "id");
        if id.starts_with("browser.watch") || id.starts_with("browser.hand") {
            if let Some(v) = f.get("value") {
                if v.get("agent").is_some() {
                    self.browser_state(v);
                }
            }
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            return;
        }
        if id.starts_with("browser.input") {
            return;
        }
        // A file a reply named, kept as the person's: it opens once it's here.
        if id.starts_with("app.agent_file") {
            match (f.get("error"), f.pointer("/value/file")) {
                (Some(e), _) => self.notices.push(s(e, "message")),
                (None, Some(file)) => {
                    if let Some(frame) = self.open_file(&FileRef::of(file)) {
                        self.to_send.push(frame);
                    }
                }
                _ => {}
            }
            return;
        }
        if let Some(conversation) = id.strip_prefix("app.queue:") {
            if let Some(v) = f.get("value") {
                self.queues
                    .insert(conversation.to_string(), Model::queue_of(v));
            }
            return;
        }
        if id == "app.pins" {
            let pins: Vec<Message> = f
                .pointer("/value/messages")
                .and_then(Value::as_array)
                .map(|ms| ms.iter().map(|m| self.message_of(m)).collect())
                .unwrap_or_default();
            self.pins = pins;
            return;
        }
        if id.starts_with("app.branch_") {
            match f.get("error") {
                Some(e) => self.notices.push(s(e, "message")),
                None => {
                    let made = f
                        .pointer("/value/conversation")
                        .and_then(Value::as_str)
                        .map(String::from);
                    self.show_branch(made);
                }
            }
            return;
        }
        if id.starts_with("app.aside_") {
            if let Some(a) = self.asides.iter_mut().find(|a| a.id == id) {
                match f.get("error") {
                    Some(e) => a.error = Some(s(e, "message")),
                    None => {
                        a.answer = Some(
                            f.pointer("/value/reply")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        )
                    }
                }
            }
            return;
        }
        if id.starts_with("app.pin_") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            return;
        }
        if id.starts_with("app.import_scan") {
            self.import_loading = false;
            self.import_sources = f
                .pointer("/value/sources")
                .and_then(Value::as_array)
                .map(|ss| ss.iter().map(ImportSource::of).collect())
                .unwrap_or_default();
            if let Some(e) = f.get("error") {
                self.import_done = Some(ImportDone {
                    text: s(e, "message"),
                    error: true,
                    ..ImportDone::default()
                });
            }
            return;
        }
        if id.starts_with("app.import_run") {
            let from = self.importing.take().unwrap_or_default();
            self.import_done = Some(match f.get("error") {
                Some(e) => ImportDone {
                    from,
                    text: s(e, "message"),
                    not: Vec::new(),
                    error: true,
                },
                None => ImportDone {
                    from,
                    text: f
                        .pointer("/value/text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    not: f
                        .pointer("/value/not")
                        .and_then(Value::as_array)
                        .map(|ns| {
                            ns.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    error: false,
                },
            });
            return;
        }
        if id == "app.tasks" {
            self.tasks = f
                .pointer("/value/tasks")
                .and_then(Value::as_array)
                .map(|ts| ts.iter().map(Task::of).collect())
                .unwrap_or_default();
            return;
        }
        if id.starts_with("app.task_cmd") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            return;
        }
        if id.starts_with("app.queue_cmd") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            return;
        }
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
        } else if id.starts_with("app.updates") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                return;
            }
            if let Some(v) = f.get("value") {
                let yes = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
                self.updates = Some(Updates {
                    auto: yes("auto"),
                    may: yes("may"),
                    owner: yes("owner"),
                    can: yes("can"),
                    why: s(v, "why"),
                });
                if let Some(u) = v.get("update").filter(|u| !s(u, "state").is_empty()) {
                    self.server_update = Some(ServerUpdate::of(u));
                }
            }
        } else if id.starts_with("app.update_now") {
            match (f.get("error"), f.get("value")) {
                (Some(e), _) => self.notices.push(s(e, "message")),
                (None, Some(v)) => self.server_update = Some(ServerUpdate::of(v)),
                _ => {}
            }
        } else if id.starts_with("app.scripts_") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                return;
            }
            let port = f
                .pointer("/value/port")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let on = if id.starts_with("app.scripts_get") {
                f.pointer("/value/on").and_then(Value::as_bool) == Some(true)
            } else {
                id.starts_with("app.scripts_on")
            };
            self.script_token = f
                .pointer("/value/token")
                .and_then(Value::as_str)
                .map(String::from);
            self.scripts = Some((on, port));
        } else if id.starts_with("app.tables") {
            self.tables = f
                .pointer("/value/tables")
                .and_then(Value::as_array)
                .map(|ts| {
                    ts.iter()
                        .map(|t| SharedTable {
                            name: s(t, "name"),
                            description: s(t, "description"),
                            owner: s(t, "owner"),
                            rows: t.get("rows").and_then(Value::as_u64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.table_rows") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                self.table_open = None;
                return;
            }
            self.table_more = f.pointer("/value/more").and_then(Value::as_bool) == Some(true);
            self.table_rows = f
                .pointer("/value/rows")
                .and_then(Value::as_array)
                .map(|rs| {
                    rs.iter()
                        .map(|r| TableRow {
                            id: r.get("id").and_then(Value::as_i64).unwrap_or(0),
                            by: s(r, "by"),
                            untrusted: r.get("untrusted").and_then(Value::as_bool) == Some(true),
                            data: fields_of(r.get("data").unwrap_or(&Value::Null)),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.table_drop") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
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
        } else if id.starts_with("app.plog") {
            self.people_log = f
                .pointer("/value/entries")
                .and_then(Value::as_array)
                .map(|es| {
                    es.iter()
                        .map(|e| PeopleEntry {
                            text: s(e, "text"),
                            by: s(e, "by"),
                            at: e.get("at").and_then(Value::as_i64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default();
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
        } else if id.starts_with("app.flows") {
            self.flows = f
                .pointer("/value/flows")
                .and_then(Value::as_array)
                .map(|fs| fs.iter().map(RunningFlow::of).collect())
                .unwrap_or_default();
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
        } else if id.starts_with("app.proposals") {
            self.proposals = f
                .pointer("/value/proposals")
                .and_then(Value::as_array)
                .map(|ps| ps.iter().map(Proposal::of).collect())
                .unwrap_or_default();
        } else if id.starts_with("app.proposal_") {
            // A kept skill or note is in its list now; a refusal says why.
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
            }
            self.proposals_stale = true;
            self.skills_stale = true;
            self.notes_stale = true;
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
        } else if id.starts_with("app.rule") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                return;
            }
            if let Some(list) = f.pointer("/value/rules") {
                self.rules = Rule::all(list);
            }
            if id.starts_with("app.rule_check") {
                if let Some(v) = f.get("value") {
                    self.what_if = Some(WhatIf::of(v));
                }
            }
        } else if id.starts_with("app.defaults") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                return;
            }
            if let Some(d) = f.pointer("/value/defaults") {
                self.take_defaults(d, id.starts_with("app.defaults_get"));
            }
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
                            status: s(e, "status"),
                            why: s(e, "why"),
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if id.starts_with("app.actexport") {
            if let Some(csv) = f.pointer("/value/csv").and_then(Value::as_str) {
                self.exported = Some(csv.to_string());
            }
        } else if id.starts_with("app.summary") {
            self.summary = f.get("value").map(|v| {
                let n = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
                Summary {
                    memories: n("memories"),
                    skills: n("skills"),
                    tables: n("tables"),
                    files: n("files"),
                    bytes: n("bytes"),
                    routines: n("routines"),
                    tokens_today: n("tokens_today"),
                    tokens_month: n("tokens_month"),
                }
            });
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
                            icon: a
                                .get("icon_svg")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.program_ids = f
                .pointer("/value/programs")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|id| (k.clone(), id.to_string())))
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
            self.programs_known = true;
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
                                members: ids(sh.get("members")),
                            }),
                            members: ids(p.get("members")),
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.programs_owner = f
                .pointer("/value/owner")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            self.sandboxes = f
                .pointer("/value/sandboxes")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        } else if id.starts_with("app.place") {
            // Where it runs was refused, as on an instance without sandboxes, or
            // while it answers: the list as the instance has it again.
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                self.dirty.insert(List::Agents);
            }
        } else if id.starts_with("app.new_agent") {
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                self.make_failed = true;
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
                        permission: s(a, "permission"),
                        busy: s(a, "busy"),
                        program: s(a, "acp_agent"),
                        backup: s(a, "backup"),
                        defaulted: self.defaults.picks(&s(a, "acp_agent")),
                        default_mode: self.defaults.permission.clone(),
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
                        options: config_options(a),
                        config: config_values(a),
                        pinned: a.get("pinned").and_then(Value::as_i64),
                        code_mode: a.get("code_mode").and_then(Value::as_bool).unwrap_or(true),
                        isolated: a.get("isolated").and_then(Value::as_bool).unwrap_or(false),
                        network: a.get("network").and_then(Value::as_array).map(|hs| {
                            hs.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        }),
                    })
                    .collect();
                // Pinned agents first, in the order they were pinned; the rest as they were.
                self.agents
                    .sort_by_key(|a| (a.pinned.is_none(), a.pinned.unwrap_or(0)));
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
                        task: c.get("task").and_then(Value::as_str).map(String::from),
                        side: c.get("side").and_then(Value::as_bool).unwrap_or(false),
                        branch_of: c.get("branch_of").and_then(Value::as_str).map(String::from),
                        branch_at: c.get("branch_at").and_then(Value::as_str).map(String::from),
                        routine: c.get("routine").and_then(Value::as_str).map(String::from),
                        program: c.get("program").and_then(Value::as_str).map(String::from),
                        title: s(c, "title"),
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
                self.tools = Rc::new(
                    items
                        .iter()
                        .map(|t| Tool {
                            call: s(t, "call"),
                            run: s(t, "run"),
                            title: s(t, "title"),
                            status: t.get("status").and_then(Value::as_str).map(str::to_string),
                        })
                        .collect(),
                );
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
                let messages: Vec<Message> = items.iter().map(|m| self.message_of(m)).collect();
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
                    mentions: Vec::new(),
                    pinned: false,
                });
            }
        }
    }

    /// The selected chat's messages, with the replies still being written at the end.
    pub fn shown(&self) -> Vec<Message> {
        // A finished reply with nothing in it, such as a turn that only changed a
        // setting, would be the agent's name and time over nothing: it's left out.
        let mut out: Vec<Message> = self
            .messages
            .iter()
            .filter(|m| !m.is_empty_reply())
            .cloned()
            .collect();
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
                mentions: Vec::new(),
                pinned: false,
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
                _ => String::new(),
            };
        };
        if let Some(a) = self.approvals.iter().find(|a| a.run == r.id) {
            return format!("Wants to {}", mid_sentence(&a.title));
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
        let mut frame = self.send_frame(agent, text);
        if !files.is_empty() {
            frame["files"] = json!(files);
        }
        Some(frame)
    }

    /// An answer the person picked on an agent's choices card, sent as their next
    /// message, without what waits attached in the composer.
    pub fn send_choice(&mut self, text: &str) -> Option<Value> {
        let agent = self.selected.clone()?;
        let text = text.trim();
        (!text.is_empty()).then(|| self.send_frame(agent, text))
    }

    fn send_frame(&self, agent: String, text: &str) -> Value {
        let mut frame = json!({"t": "send", "agent": agent, "text": text});
        // Into the task's or the branch's conversation when the chat shows one; the core finds
        // the agent's chat otherwise.
        if let Some(task) = self.task_open() {
            frame["conversation"] = json!(task.conversation);
        } else if let Some(branch) = &self.open_branch {
            frame["conversation"] = json!(branch);
        }
        frame
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

    /// Opens a file a reply named: the instance keeps it as the person's first.
    pub fn open_mention(&mut self, m: &Mention) -> Value {
        let id = self.id("app.agent_file");
        json!({"t": "cmd", "id": id, "name": "agent.file", "args": {"agent": m.agent, "path": m.path}})
    }

    /// Shows a page beside the chat: fetched first, unless it's here already.
    pub fn open_page(&mut self, f: &FileRef) -> Option<Value> {
        self.page = Some(f.clone());
        if self.files.contains_key(&f.file) {
            return None;
        }
        let id = self.id("app.get");
        self.getting.insert(id.clone(), (f.file.clone(), false));
        Some(json!({"t": "file.get", "id": id, "file": f.file, "name": f.name}))
    }

    /// Where the open page is on this computer, once it's fetched.
    pub fn page_path(&self) -> Option<&String> {
        self.files.get(&self.page.as_ref()?.file)
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
                Rc::make_mut(&mut self.files).insert(key, path.to_string());
            }
            (e, _) => {
                let page = self.page.as_ref().is_some_and(|p| p.file == key);
                if page {
                    self.page = None;
                }
                if open || page {
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

    pub fn new_agent(
        &mut self,
        name: &str,
        program: &str,
        instructions: &str,
        color: &str,
    ) -> Value {
        let id = self.id("app.new_agent");
        let mut args =
            json!({"name": name.trim(), "acp_agent": program, "instructions": instructions.trim()});
        if !color.is_empty() {
            args["color"] = json!(color);
        }
        json!({"t": "cmd", "id": id, "name": "agent.create", "args": args})
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
    /// Gives the agent on the edit page another thread colour.
    /// Turns Code Mode on or off for the agent being edited, from its next session.
    pub fn set_code_mode(&mut self, on: bool) -> Option<Value> {
        let agent = self.editing.clone()?;
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent) {
            a.code_mode = on;
        }
        let id = self.id("app.code_mode");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "code_mode": on}}),
        )
    }

    /// Runs the agent being edited in a sandbox, or not. Out of one, it has the
    /// network of the instance's computer, so its own list goes.
    pub fn set_isolated(&mut self, on: bool) -> Option<Value> {
        let agent = self.editing.clone()?;
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent) {
            a.isolated = on;
            if !on {
                a.network = None;
            }
        }
        let id = self.id("app.place");
        let args = if on {
            json!({"agent": agent, "isolated": true})
        } else {
            json!({"agent": agent, "isolated": false, "network": null})
        };
        Some(json!({"t": "cmd", "id": id, "name": "agent.update", "args": args}))
    }

    /// Sets what the agent being edited reaches from its sandbox: None for
    /// everything, or the hosts on a list, none when it's empty.
    pub fn set_network(&mut self, hosts: Option<Vec<String>>) -> Option<Value> {
        let agent = self.editing.clone()?;
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent) {
            a.network = hosts.clone();
        }
        let id = self.id("app.place");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "network": hosts}}),
        )
    }

    /// Adds `host` to the list of the agent being edited, as the instance keeps
    /// it: lowercase, without a scheme, a path or a trailing dot. None when it's
    /// empty, or on the list already.
    pub fn add_host(&mut self, host: &str) -> Option<Value> {
        let agent = self.agent(self.editing.as_ref()?)?;
        let host = host_entry(host)?;
        let mut hosts = agent.network.clone().unwrap_or_default();
        if hosts.contains(&host) {
            return None;
        }
        hosts.push(host);
        self.set_network(Some(hosts))
    }

    pub fn remove_host(&mut self, host: &str) -> Option<Value> {
        let agent = self.agent(self.editing.as_ref()?)?;
        let mut hosts = agent.network.clone().unwrap_or_default();
        hosts.retain(|h| h != host);
        self.set_network(Some(hosts))
    }

    pub fn set_color(&mut self, color: &str) -> Option<Value> {
        let agent = self.editing.clone()?;
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == agent) {
            a.color = color.to_string();
        }
        let id = self.id("app.color");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "color": color}}),
        )
    }

    /// Opens the agent's settings: its revisions, and what it has, counted.
    pub fn edit_agent(&mut self) -> Vec<Value> {
        let Some(agent) = self.selected.clone() else {
            return Vec::new();
        };
        self.editing = Some(agent.clone());
        self.revisions.clear();
        self.summary = None;
        let id = self.id("app.summary");
        vec![
            self.revisions_cmd(),
            json!({"t": "cmd", "id": id, "name": "agent.summary", "args": {"agent": agent}}),
            // Whether it can be isolated here, and whether it's the owner's.
            self.programs(),
        ]
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

    /// Asks for the Brains page's lists ahead of it, once, when the settings open on
    /// another page, so they're there by the time it's opened. None once they're asked.
    pub fn warm_programs(&mut self) -> Option<Vec<Value>> {
        if !self.registry.is_empty() || self.registry_loading {
            return None;
        }
        Some(self.load_programs())
    }

    fn registry_cmd(&mut self) -> Value {
        let id = self.id("app.registry");
        json!({"t": "cmd", "id": id, "name": "acp.registry"})
    }

    /// The registry agent a program on the instance is, for its icon: the one the instance
    /// matched it to, or the one with its name.
    pub fn program_listed(&self, program: &str) -> Option<&Listed> {
        let id = self
            .program_ids
            .get(program)
            .map(String::as_str)
            .unwrap_or(program);
        self.registry.iter().find(|l| l.id == id)
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

    /// Gives a program's shared settings to `members` (by id), or everyone, on the
    /// budget they have.
    pub fn share_to(
        &mut self,
        program: &str,
        share: &Share,
        members: Option<Vec<String>>,
    ) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.share", "args": {"name": program, "tokens": share.tokens, "per": share.per, "members": members}})
    }

    /// Gives a program to `members` (by id), or everyone.
    pub fn program_members(&mut self, program: &str, members: Option<Vec<String>>) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.members", "args": {"name": program, "members": members}})
    }

    pub fn remove_program(&mut self, program: &str) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.remove", "args": {"name": program}})
    }

    /// Asks what OpenBot and Grok Bot keep on the instance's computer, for the Import page.
    pub fn load_imports(&mut self) -> Value {
        self.import_loading = true;
        let id = self.id("app.import_scan");
        json!({"t": "cmd", "id": id, "name": "import.scan"})
    }

    /// Imports from `from` (openbot or grokbot): the answer says what came.
    pub fn run_import(&mut self, from: &str) -> Value {
        self.importing = Some(from.to_string());
        self.import_done = None;
        let id = self.id("app.import_run");
        json!({"t": "cmd", "id": id, "name": "import.run", "args": {"from": from}})
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

    /// Asks how the instance's updates stand.
    pub fn load_updates(&mut self) -> Value {
        let id = self.id("app.updates");
        json!({"t": "cmd", "id": id, "name": "instance.updates", "args": {}})
    }

    /// Lets the instance update itself, or stops it: the owner's alone.
    pub fn set_auto_update(&mut self, on: bool) -> Value {
        let id = self.id("app.updates_set");
        json!({"t": "cmd", "id": id, "name": "instance.updates", "args": {"auto": on}})
    }

    /// Updates the instance to the newest release: once nothing runs, or with `now`,
    /// stopping what runs.
    pub fn update_server(&mut self, now: bool) -> Value {
        let id = self.id("app.update_now");
        json!({"t": "cmd", "id": id, "name": "instance.update", "args": {"now": now}})
    }

    /// Asks whether the person's scripts are on.
    pub fn load_scripts(&mut self) -> Value {
        self.script_token = None;
        let id = self.id("app.scripts_get");
        json!({"t": "cmd", "id": id, "name": "scripts.get", "args": {}})
    }

    /// Turns scripts on, or off, or with `on` again, makes a new token.
    pub fn set_scripts(&mut self, on: bool) -> Value {
        self.script_token = None;
        let (id, name) = if on {
            (self.id("app.scripts_on"), "scripts.on")
        } else {
            (self.id("app.scripts_off"), "scripts.off")
        };
        json!({"t": "cmd", "id": id, "name": name, "args": {}})
    }

    /// Asks for the person's shared tables, and the open one's newest rows.
    pub fn load_tables(&mut self) -> Vec<Value> {
        let mut out = vec![
            json!({"t": "cmd", "id": self.id("app.tables"), "name": "table.list", "args": {}}),
        ];
        if let Some(table) = self.table_open.clone() {
            out.push(json!({"t": "cmd", "id": self.id("app.table_rows"), "name": "table.rows", "args": {"table": table, "desc": true, "limit": 200}}));
        }
        out
    }

    /// Opens a table on the tables page, or with None, the list again.
    pub fn open_table(&mut self, table: Option<String>) -> Vec<Value> {
        self.table_open = table;
        self.table_rows.clear();
        self.table_more = false;
        self.load_tables()
    }

    pub fn drop_table(&mut self, table: &str) -> Value {
        let id = self.id("app.table_drop");
        json!({"t": "cmd", "id": id, "name": "table.drop", "args": {"table": table}})
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

    /// Asks for what agents propose that waits for the person.
    pub fn load_proposals(&mut self) -> Value {
        let id = self.id("app.proposals");
        json!({"t": "cmd", "id": id, "name": "proposal.list", "args": {"status": "waiting"}})
    }

    /// Keeps a proposal, with what the person changed in it: a skill's name, when it's
    /// for and how, or a note's text.
    pub fn keep_proposal(&mut self, p: &Proposal) -> Value {
        let id = self.id("app.proposal_keep");
        self.proposals.retain(|q| q.id != p.id);
        let changed = if p.kind == "skill" {
            json!({"skill": {"name": p.name.trim(), "description": p.description.trim(), "body": p.body}})
        } else {
            json!({"note": {"text": p.text.trim()}})
        };
        let mut args = changed;
        args["id"] = json!(p.id);
        json!({"t": "cmd", "id": id, "name": "proposal.keep", "args": args})
    }

    pub fn drop_proposal(&mut self, proposal: &str) -> Value {
        let id = self.id("app.proposal_drop");
        self.proposals.retain(|q| q.id != proposal);
        json!({"t": "cmd", "id": id, "name": "proposal.drop", "args": {"id": proposal}})
    }

    /// Asks an agent to look over its past chats and propose skills and notes, as a task.
    pub fn ask_proposals(&mut self, agent: &str) -> Value {
        let id = self.id("app.proposal_ask");
        json!({"t": "cmd", "id": id, "name": "proposal.ask", "args": {"agent": agent}})
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

    /// Reads everyone with an account, the invites still open and the newest of the people
    /// log, for the People page.
    pub fn load_people(&mut self) -> Vec<Value> {
        let people = self.id("app.people");
        let invites = self.id("app.invites");
        // Not "app.people…", which is user.list's.
        let log = self.id("app.plog");
        vec![
            json!({"t": "cmd", "id": people, "name": "user.list", "args": {}}),
            json!({"t": "cmd", "id": invites, "name": "invite.list", "args": {}}),
            json!({"t": "cmd", "id": log, "name": "people.log", "args": {"limit": 30}}),
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
        if !f.post.is_empty() {
            args["post"] = json!(f.post);
        }
        if !f.id.is_empty() {
            args["id"] = json!(f.id);
        }
        if let Some(flow) = &f.flow {
            args["flow"] = flow
                .iter()
                .map(|s| json!({"name": s.name, "agent": s.agent, "prompt": s.prompt, "after": s.after}))
                .collect();
        }
        json!({"t": "cmd", "id": cmd, "name": "routine.put", "args": args})
    }

    /// Asks for the routine flows running.
    pub fn load_flows(&mut self) -> Value {
        let id = self.id("app.flows");
        json!({"t": "cmd", "id": id, "name": "flow.list"})
    }

    /// Stops routine `id`'s running flow.
    pub fn stop_flow(&mut self, id: &str) -> Value {
        let cmd = self.id("app.routine_stop");
        json!({"t": "cmd", "id": cmd, "name": "routine.stop", "args": {"id": id}})
    }

    pub fn run_routine(&mut self, id: &str) -> Value {
        let cmd = self.id("app.routine_run");
        json!({"t": "cmd", "id": cmd, "name": "routine.run", "args": {"id": id}})
    }

    pub fn delete_routine(&mut self, id: &str) -> Value {
        let cmd = self.id("app.routine_delete");
        json!({"t": "cmd", "id": cmd, "name": "routine.delete", "args": {"id": id}})
    }

    /// Asks for the person's rules on what agents touch.
    pub fn load_rules(&mut self) -> Value {
        let id = self.id("app.rules_list");
        json!({"t": "cmd", "id": id, "name": "rule.list"})
    }

    /// Adds a rule, or changes one with an `id` in `args`. `rules.updated` brings the list.
    pub fn put_rule(&mut self, args: Value) -> Value {
        let id = self.id("app.rule_put");
        json!({"t": "cmd", "id": id, "name": "rule.put", "args": args})
    }

    pub fn delete_rule(&mut self, rule: &str) -> Value {
        self.rules.retain(|r| r.id != rule);
        let id = self.id("app.rule_delete");
        json!({"t": "cmd", "id": id, "name": "rule.delete", "args": {"id": rule}})
    }

    /// Asks what an action would get, for the what-if: `args` as rule.check takes them.
    pub fn check_rule(&mut self, args: Value) -> Value {
        self.what_if = None;
        let id = self.id("app.rule_check");
        json!({"t": "cmd", "id": id, "name": "rule.check", "args": args})
    }

    /// Asks for the person's agent defaults.
    pub fn load_defaults(&mut self) -> Value {
        let id = self.id("app.defaults_get");
        json!({"t": "cmd", "id": id, "name": "defaults.get"})
    }

    /// Changes the person's agent defaults: the fields in `args`, the rest as they are.
    pub fn put_defaults(&mut self, args: Value) -> Value {
        let id = self.id("app.defaults_put");
        json!({"t": "cmd", "id": id, "name": "defaults.put", "args": args})
    }

    /// The defaults came: every agent follows them anew. `fresh` shows their
    /// instructions on the Defaults page, as they also are when they changed.
    fn take_defaults(&mut self, d: &Value, fresh: bool) {
        let now = Defaults::of(d);
        if fresh || now.instructions != self.defaults.instructions {
            self.defaults_came = true;
        }
        if now != self.defaults {
            self.defaults = now;
            self.dirty.insert(List::Agents);
        }
    }

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

    /// Asks for the selected agent's activity log as CSV, to save.
    pub fn export_activity(&mut self) -> Option<Value> {
        let agent = self.selected.clone()?;
        let id = self.id("app.actexport");
        Some(json!({"t": "cmd", "id": id, "name": "activity.export", "args": {"agent": agent}}))
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

    /// Pins the open agent to the top of the list, or takes its pin off.
    pub fn pin_agent(&mut self, pinned: bool) -> Option<Value> {
        let agent = self.selected.clone()?;
        let id = self.id("app.pin_agent");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.pin", "args": {"agent": agent, "pinned": pinned}}),
        )
    }

    /// Sets an agent's permission mode, or with None, has it follow the default.
    pub fn set_permission(&mut self, agent: &str, permission: Option<&str>) -> Option<Value> {
        let a = self.agents.iter_mut().find(|a| a.id == agent)?;
        a.permission = permission.unwrap_or_default().to_string();
        let id = self.id("app.permission");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "permission": permission}}),
        )
    }

    /// Sets what a message to an agent while it's busy does, queue or steer, or with
    /// None, has it follow the default.
    pub fn set_busy(&mut self, agent: &str, busy: Option<&str>) -> Option<Value> {
        let a = self.agents.iter_mut().find(|a| a.id == agent)?;
        a.busy = busy.unwrap_or_default().to_string();
        let id = self.id("app.busy");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "busy": busy}}),
        )
    }

    /// Sets an agent's backup program: a program, "none" for none, or with None, the
    /// person's default.
    pub fn set_backup(&mut self, agent: &str, backup: Option<&str>) -> Option<Value> {
        let a = self.agents.iter_mut().find(|a| a.id == agent)?;
        a.backup = backup.unwrap_or_default().to_string();
        let id = self.id("app.backup");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "backup": backup}}),
        )
    }

    /// The program the open chat runs on: the one it was moved to, or its agent's own.
    pub fn chat_program(&self) -> Option<String> {
        let agent = self.agent(self.selected.as_ref()?)?;
        let moved = self
            .conversation()
            .and_then(|id| self.conversations.iter().find(|c| c.id == id))
            .and_then(|c| c.program.clone());
        Some(moved.unwrap_or_else(|| agent.program.clone()))
    }

    /// Moves the open chat to another program, which starts with loom's summary of it.
    pub fn move_chat(&mut self, program: &str) -> Option<Value> {
        let conversation = self.conversation()?;
        if self.chat_program().as_deref() == Some(program) {
            return None;
        }
        let id = self.id("app.move");
        Some(
            json!({"t": "cmd", "id": id, "name": "conversation.move", "args": {"conversation": conversation, "program": program}}),
        )
    }

    /// Has one of an agent's options, such as its model, follow the default again.
    pub fn follow_config(&mut self, agent: &str, option: &str) -> Option<Value> {
        let a = self.agents.iter_mut().find(|a| a.id == agent)?;
        a.config.retain(|(k, _)| k != option);
        let id = self.id("app.config");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "config": {option: null}}}),
        )
    }

    /// Picks a value of one of an agent's options, such as its model. It shows
    /// at once; the instance sets each session to it before its next prompt.
    pub fn set_config(&mut self, agent: &str, option: &str, value: &str) -> Option<Value> {
        let a = self.agents.iter_mut().find(|a| a.id == agent)?;
        a.config.retain(|(k, _)| k != option);
        a.config.push((option.to_string(), value.to_string()));
        let id = self.id("app.config");
        Some(
            json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "config": {option: value}}}),
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
    fn a_server_behind_and_its_update() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.server", "loom": "0.2.0", "latest": "0.2.1", "behind": true}));
        let v = m.version.clone().unwrap();
        assert!(v.behind && v.loom == "0.2.0" && v.latest == "0.2.1");
        m.take(&json!({"t": "core.release", "version": "0.2.1"}));
        assert_eq!(m.release.take().as_deref(), Some("0.2.1"));
        // How updates stand comes with each connection.
        m.take(&json!({"t": "synced", "seq": 0}));
        let asked = m.fetches();
        let ask = asked.iter().find(|f| f["name"] == "instance.updates").unwrap();
        m.take(&json!({"t": "result", "id": ask["id"], "ok": true, "value": {"version": "0.2.0", "auto": false, "may": true, "owner": true, "can": true, "update": {"state": ""}}}));
        let u = m.updates.clone().unwrap();
        assert!(u.may && u.owner && u.can && !u.auto);
        assert!(m.server_update.is_none(), "no update under way");
        let go = m.update_server(false);
        assert_eq!(go["name"], "instance.update");
        assert_eq!(go["args"]["now"], false);
        m.take(&json!({"t": "instance.update", "state": "waiting", "version": "0.2.1"}));
        assert_eq!(m.server_update.clone().unwrap().state, "waiting");
        // After the restart, welcome has no update under way.
        m.take(&json!({"t": "welcome", "device": "d_1", "loom": "0.2.1"}));
        assert!(m.server_update.is_none());
        let auto = m.set_auto_update(true);
        assert_eq!(auto["args"]["auto"], true);
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
    fn an_empty_reply_is_left_out() {
        let reply = |blocks: Vec<Block>| Message {
            id: "msg_1".into(),
            role: "agent".into(),
            at: 1,
            blocks,
            live: false,
            queued: false,
            from_agent: None,
            mentions: Vec::new(),
            pinned: false,
        };
        assert!(reply(Vec::new()).is_empty_reply());
        assert!(reply(vec![Block::text("text", "  \n".into())]).is_empty_reply());
        assert!(!reply(vec![Block::text("text", "Done.".into())]).is_empty_reply());
        assert!(!reply(vec![Block::text("tool", String::new())]).is_empty_reply());
        let live = Message {
            live: true,
            ..reply(Vec::new())
        };
        assert!(!live.is_empty_reply());
    }

    #[test]
    fn a_card_takes_its_blocks_place() {
        let mut m = Model::new();
        let progress = |v: f64| json!({"id": "card_1", "type": "progress", "title": "", "label": "Copying", "value": v});
        m.card(&json!({"t": "card", "message": "msg_1", "block": 2, "card": progress(0.2)}));
        m.card(&json!({"t": "card", "message": "msg_1", "block": 2, "card": progress(0.7)}));
        let blocks = &m.live["msg_1"];
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[2].kind, "card");
        assert_eq!(
            blocks[2].card.as_ref().map(|c| &c.body),
            Some(&CardBody::Progress {
                label: "Copying".into(),
                value: 0.7
            })
        );
        let kept = Block::of(
            &json!({"kind": "card", "card": {"id": "card_2", "type": "choices", "title": "Which?", "options": ["Red", "Blue"]}}),
        );
        assert_eq!(
            kept.card.map(|c| c.body),
            Some(CardBody::Choices(vec!["Red".into(), "Blue".into()]))
        );
        assert!(Card::of(&json!({"type": "chart"})).is_none());
    }

    #[test]
    fn proposals_wait_and_are_kept_as_changed() {
        let mut m = Model::new();
        let list = m.load_proposals();
        m.result(&json!({"t": "result", "id": list["id"], "ok": true, "value": {"proposals": [
            {"id": "prop_2", "agent": "a_1", "kind": "skill", "why": "You showed me.",
             "skill": {"name": "shop-search", "description": "Searching.", "body": "1. Search."},
             "recording": "rec_1", "steps": [
                {"kind": "page", "title": "Shop", "url": "https://shop.example"},
                {"kind": "type", "into": "textbox \"Password\"", "secret": true},
                {"kind": "click", "on": "button \"Go\""}]},
            {"id": "prop_1", "agent": "a_1", "kind": "note", "note": {"text": "Ann likes wool.", "site": "", "shared": false}}
        ]}}));
        assert_eq!(m.proposals.len(), 2);
        assert_eq!(m.proposals[0].title(), "shop-search");
        assert_eq!(
            m.proposals[0].steps,
            vec![
                "On Shop".to_string(),
                "Typed something secret into textbox \"Password\"".to_string(),
                "Clicked button \"Go\"".to_string()
            ]
        );
        assert_eq!(m.proposals[1].title(), "Ann likes wool.");
        let mut changed = m.proposals[0].clone();
        changed.description = "Searching the shop.".into();
        let keep = m.keep_proposal(&changed);
        assert_eq!(keep["name"], "proposal.keep");
        assert_eq!(keep["args"]["id"], "prop_2");
        assert_eq!(keep["args"]["skill"]["description"], "Searching the shop.");
        assert_eq!(m.proposals.len(), 1);
        let dropped = m.drop_proposal("prop_1");
        assert_eq!(dropped["args"]["id"], "prop_1");
        assert!(m.proposals.is_empty());
    }

    #[test]
    fn the_browser_says_when_it_records() {
        let mut m = Model::new();
        m.watching = Some("a_1".into());
        m.browser_state(&json!({"agent": "a_1", "open": true, "person": true, "asked": "", "secret": "", "recording": true, "steps": 4}));
        let b = m.browser("a_1");
        assert!(b.recording && b.steps == 4);
        let f = m.record_browser(false).unwrap();
        assert_eq!(f["name"], "browser.record");
        assert_eq!(f["args"]["on"], false);
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
        // Good news, said as such rather than as something that went wrong.
        assert!(m
            .said
            .last()
            .unwrap()
            .contains("moved to https://new.example"));
    }

    #[test]
    fn imports_are_scanned_run_and_said() {
        let mut m = Model::new();
        let scan = m.load_imports();
        assert_eq!(scan["name"], "import.scan");
        assert!(m.import_loading);
        m.result(&json!({"t": "result", "id": scan["id"], "ok": true, "value": {"sources": [
            {"from": "openbot", "name": "OpenBot", "found": true, "text": "2 agents, 3 messages"},
            {"from": "grokbot", "name": "Grok Bot", "found": false, "why": "There's no Grok Bot data here"}
        ]}}));
        assert!(!m.import_loading);
        assert_eq!(m.import_sources.len(), 2);
        assert!(m.import_sources[0].found && !m.import_sources[1].found);
        let run = m.run_import("openbot");
        assert_eq!(run["args"], json!({"from": "openbot"}));
        assert_eq!(m.importing.as_deref(), Some("openbot"));
        m.result(&json!({"t": "result", "id": run["id"], "ok": true, "value": {"text": "Brought over from OpenBot: 2 agents.", "not": ["Keys stay in OpenBot."]}}));
        let done = m.import_done.clone().unwrap();
        assert!(m.importing.is_none() && !done.error && done.from == "openbot");
        assert_eq!(done.not, vec!["Keys stay in OpenBot.".to_string()]);
        let again = m.run_import("grokbot");
        m.result(&json!({"t": "result", "id": again["id"], "ok": false, "error": {"code": "not_allowed", "message": "Only the instance's owner imports"}}));
        assert!(m.import_done.unwrap().error);
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
                task: None,
                side: false,
                branch_of: None,
                branch_at: None,
                routine: None,
                program: None,
                title: String::new(),
            },
            Conversation {
                id: "c_2".into(),
                agent: "a_2".into(),
                task: None,
                side: false,
                branch_of: None,
                branch_at: None,
                routine: None,
                program: None,
                title: String::new(),
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
        m.take(&json!({"t": "event", "kind": "message.created", "conversation": "c_2", "message": {"id": "msg_3", "conversation": "c_2", "role": "agent", "agent": "a_2"}}));
        m.select(Some("a_2".into()));
        assert!(m.unread.is_empty());
        // The NEW line is above the first of them, while the chat stays open.
        assert_eq!(m.new_line.as_deref(), Some("msg_1"));
        m.select(Some("a_2".into()));
        assert_eq!(m.new_line.as_deref(), Some("msg_1"));
        m.select(Some("a_1".into()));
        assert_eq!(m.new_line, None);
    }

    #[test]
    fn a_routines_thread_is_shown_only_when_opened() {
        let mut m = Model::new();
        let conversation = |id: &str, routine: Option<&str>| Conversation {
            id: id.into(),
            agent: "a_1".into(),
            task: None,
            side: false,
            branch_of: None,
            branch_at: None,
            routine: routine.map(String::from),
            program: None,
            title: String::new(),
        };
        // The thread first, as if it were newer than the chat.
        m.conversations = vec![
            conversation("c_thread", Some("rt_1")),
            conversation("c_chat", None),
        ];
        m.select(Some("a_1".into()));
        assert_eq!(
            m.conversation().as_deref(),
            Some("c_chat"),
            "a routine's thread isn't the chat"
        );
        m.show_branch(Some("c_thread".into()));
        assert_eq!(m.conversation().as_deref(), Some("c_thread"));
        assert!(m
            .branch_open()
            .is_some_and(|c| c.routine.as_deref() == Some("rt_1")));
    }

    #[test]
    fn rules_come_with_each_change_and_the_what_if_answers() {
        let mut m = Model::new();
        m.take(&json!({"t": "event", "kind": "rules.updated", "rules": [
            {"id": "r_1", "agent": "", "then": "deny", "command": "rm", "said": "Deny commands like rm"},
            {"id": "r_2", "agent": "a_1", "then": "ask", "site": "github.com", "said": "Ask before the site github.com", "note": "Mine"}
        ]}));
        assert_eq!(m.rules.len(), 2);
        assert_eq!(m.rules[1].agent, "a_1");
        assert_eq!(m.rules[1].note, "Mine");
        let f = m.check_rule(json!({"command": "rm -rf x"}));
        assert_eq!(f["name"], "rule.check");
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "value": {"outcome": "deny", "decided": "rule", "why": "Denied by rule r_1."}}));
        let w = m.what_if.clone().unwrap();
        assert_eq!((w.verdict(), w.decided.as_str()), ("Denied", "rule"));
        let f = m.delete_rule("r_1");
        assert_eq!(f["args"]["id"], "r_1");
        assert_eq!(m.rules.len(), 1);
        let f = m.put_rule(json!({"then": "allow", "command": "ls"}));
        let id = f["id"].as_str().unwrap().to_string();
        m.take(
            &json!({"t": "result", "id": id, "error": {"code": "bad_request", "message": "no"}}),
        );
        assert!(m.notices.iter().any(|n| n == "no"));
    }

    #[test]
    fn an_agents_model_and_effort_are_picked() {
        let mut m = Model::new();
        let agent = json!({"id": "a_1", "name": "Main", "options": [
            {"id": "mode", "name": "Mode", "category": "mode", "value": "default", "options": [{"value": "default", "name": "Default"}]},
            {"id": "effort", "name": "Effort", "category": "thought_level", "value": "medium", "options": [{"value": "medium", "name": "Medium"}, {"value": "high", "name": "High"}]},
            {"id": "model", "name": "Model", "category": "model", "value": "opus", "options": [{"value": "opus", "name": "Opus"}, {"value": "sonnet", "name": "Sonnet"}]}
        ], "config": {"effort": "high"}});
        m.take(&json!({"t": "core.data", "what": "agents", "items": [agent]}));
        let a = m.agent("a_1").cloned().unwrap();
        let ids: Vec<String> = a.picks().iter().map(|o| o.id.clone()).collect();
        assert_eq!(ids, ["model", "effort"]);
        assert_eq!(a.picked().as_deref(), Some("Opus · High"));
        let f = m.set_config("a_1", "model", "sonnet").unwrap();
        assert_eq!(f["args"]["config"]["model"], "sonnet");
        assert_eq!(
            m.agent("a_1").unwrap().picked().as_deref(),
            Some("Sonnet · High")
        );
        // A program that offers nothing to pick shows no picker.
        m.take(
            &json!({"t": "core.data", "what": "agents", "items": [{"id": "a_1", "name": "Main"}]}),
        );
        assert_eq!(m.agent("a_1").unwrap().picked(), None);
    }

    #[test]
    fn an_agent_follows_the_defaults_it_has_nothing_of_its_own_for() {
        let mut m = Model::new();
        let options = json!([
            {"id": "effort", "name": "Effort", "category": "thought_level", "value": "medium", "options": [{"value": "medium", "name": "Medium"}, {"value": "high", "name": "High"}]},
            {"id": "model", "name": "Model", "category": "model", "value": "opus", "options": [{"value": "opus", "name": "Opus"}, {"value": "sonnet", "name": "Sonnet"}]}
        ]);
        let agents = json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Main", "acp_agent": "claude", "options": options, "config": {"effort": "high"}},
            {"id": "a_2", "name": "Careful", "acp_agent": "claude", "options": options, "permission": "ask"}
        ]});
        m.take(&agents);
        assert_eq!(
            m.agent("a_1").unwrap().mode(),
            "",
            "before the defaults come"
        );
        assert!(m.take(&json!({"t": "event", "kind": "defaults.updated", "defaults": {
            "brain": "claude", "permission": "auto", "busy": "steer", "instructions": "Be brief.",
            "config": {"claude": {"model": "sonnet", "effort": "medium"}, "codex": {"model": "gpt"}}
        }})));
        assert!(m.defaults_came && m.defaults.busy == "steer");
        // The instance says the agents changed by sending them again.
        m.take(&agents);
        let main = m.agent("a_1").cloned().unwrap();
        assert_eq!(main.mode(), "auto");
        assert_eq!(main.picked().as_deref(), Some("Sonnet · High"));
        assert!(main.follows("model") && !main.follows("effort"));
        assert_eq!(m.agent("a_2").unwrap().mode(), "ask");
        // A default its program doesn't offer leaves what the program starts on.
        m.take(&json!({"t": "event", "kind": "defaults.updated", "defaults": {"config": {"claude": {"model": "haiku"}}}}));
        m.take(&agents);
        assert_eq!(m.agent("a_1").unwrap().followed("model"), "opus");
        assert_eq!(m.agent("a_1").unwrap().mode(), "ask");
        // Null sends a field back to following the default.
        let f = m.set_permission("a_2", None).unwrap();
        assert!(f["args"]["permission"].is_null());
        assert_eq!(m.agent("a_2").unwrap().permission, "");
        let f = m.follow_config("a_1", "effort").unwrap();
        assert!(f["args"]["config"]["effort"].is_null());
        assert!(m.agent("a_1").unwrap().follows("effort"));
        let f = m.set_busy("a_1", Some("queue")).unwrap();
        assert_eq!(f["args"]["busy"], "queue");
    }

    #[test]
    fn a_chat_moves_to_another_program_and_an_agent_has_a_backup() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Main", "acp_agent": "claude", "backup": "none"}
        ]}));
        m.take(
            &json!({"t": "core.data", "what": "conversations", "items": [
                {"id": "c_1", "agent": "a_1", "title": "Chat"}
            ]}),
        );
        m.select(Some("a_1".into()));
        assert_eq!(m.agent("a_1").unwrap().backup, "none");
        m.take(&json!({"t": "event", "kind": "defaults.updated", "defaults": {"backup": "codex"}}));
        assert_eq!(m.defaults.backup, "codex");
        let f = m.set_backup("a_1", None).unwrap();
        assert!(f["args"]["backup"].is_null());
        assert_eq!(m.agent("a_1").unwrap().backup, "");
        let f = m.set_backup("a_1", Some("opencode")).unwrap();
        assert_eq!(f["args"]["backup"], "opencode");
        // The chat runs on its agent's program until it's moved.
        assert_eq!(m.chat_program().as_deref(), Some("claude"));
        assert!(m.move_chat("claude").is_none(), "it's there already");
        let f = m.move_chat("codex").unwrap();
        assert_eq!(f["name"], "conversation.move");
        assert_eq!(f["args"]["conversation"], "c_1");
        assert_eq!(f["args"]["program"], "codex");
        m.take(
            &json!({"t": "core.data", "what": "conversations", "items": [
                {"id": "c_1", "agent": "a_1", "title": "Chat", "program": "codex"}
            ]}),
        );
        assert_eq!(m.chat_program().as_deref(), Some("codex"));
    }

    #[test]
    fn a_skill_from_the_plus_menu_starts_the_message() {
        assert_eq!(with_skill("review", ""), "Use the review skill. ");
        assert_eq!(
            with_skill("review", "  the login page"),
            "Use the review skill. the login page"
        );
        assert_eq!(
            with_skill("review", "Use the review skill. again"),
            "Use the review skill. again"
        );
    }

    #[test]
    fn a_file_a_reply_names_is_a_chip_that_opens_it() {
        let mut m = Model::new();
        let reply = m.message_of(
            &json!({"id": "msg_1", "role": "agent", "agent": "a_1", "blocks": [],
            "mentions": [{"path": "notes/hours.md", "name": "hours.md", "size": 7}]}),
        );
        assert_eq!(reply.mentions.len(), 1);
        let chip = reply.mentions[0].clone();
        assert_eq!((chip.agent.as_str(), chip.kind().as_str()), ("a_1", "MD"));
        let asked = m.open_mention(&chip);
        assert_eq!(asked["name"], "agent.file");
        assert_eq!(asked["args"]["path"], "notes/hours.md");
        // Kept as the person's file, it's fetched to be opened.
        m.result(&json!({"t": "result", "id": asked["id"], "ok": true, "value": {"file": {"kind": "file", "file": "ab12", "name": "hours.md", "mime": "text/markdown", "size": 7}}}));
        assert_eq!(m.to_send.len(), 1);
        assert_eq!(m.to_send[0]["t"], "file.get");
        assert_eq!(m.to_send[0]["file"], "ab12");
    }

    #[test]
    fn a_side_question_is_answered_beside_the_chat_and_not_in_it() {
        let mut m = Model::new();
        m.conversations = vec![
            Conversation {
                id: "c_side".into(),
                agent: "a_1".into(),
                task: None,
                side: true,
                branch_of: None,
                branch_at: None,
                routine: None,
                program: None,
                title: String::new(),
            },
            Conversation {
                id: "c_chat".into(),
                agent: "a_1".into(),
                task: None,
                side: false,
                branch_of: None,
                branch_at: None,
                routine: None,
                program: None,
                title: String::new(),
            },
        ];
        m.select(Some("a_1".into()));
        assert_eq!(
            m.conversation().as_deref(),
            Some("c_chat"),
            "the side questions' conversation isn't the chat"
        );
        let f = m
            .ask_aside("what was the word?")
            .expect("a question for the chat");
        assert_eq!(f["name"], "chat.ask");
        assert_eq!(f["args"]["conversation"], "c_chat");
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "ok": true, "value": {"reply": "teapot"}}));
        assert_eq!(m.asides[0].answer.as_deref(), Some("teapot"));
        m.synced = true;
        m.take(&json!({"t": "event", "kind": "message.created", "conversation": "c_side", "message": {"id": "msg_9", "conversation": "c_side", "role": "agent", "agent": "a_1"}}));
        assert!(
            !m.unread.contains("a_1"),
            "its answer doesn't mark the agent unread"
        );
    }

    #[test]
    fn scripts_are_turned_on_with_a_token_shown_once() {
        let mut m = Model::new();
        let f = m.load_scripts();
        assert_eq!(f["name"], "scripts.get");
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {"on": false, "since": null, "port": 8790}}));
        assert_eq!(m.scripts, Some((false, 8790)));
        let f = m.set_scripts(true);
        assert_eq!(f["name"], "scripts.on");
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {"token": "loom-script-ab", "port": 8790}}));
        assert_eq!(m.scripts, Some((true, 8790)));
        assert_eq!(m.script_token.as_deref(), Some("loom-script-ab"));
        // Coming back to the page, it's on, and the token isn't shown again.
        let f = m.load_scripts();
        assert!(m.script_token.is_none());
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {"on": true, "since": 1, "port": 8790}}));
        assert_eq!(m.scripts, Some((true, 8790)));
        let f = m.set_scripts(false);
        assert_eq!(f["name"], "scripts.off");
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {}}));
        assert_eq!(m.scripts, Some((false, 0)));
    }

    #[test]
    fn tables_are_listed_opened_and_reloaded_when_they_change() {
        let mut m = Model::new();
        m.tables_open = true;
        let asked = m.open_table(None);
        assert_eq!(asked.len(), 1, "just the list, with no table open");
        m.take(&json!({"t": "result", "id": asked[0]["id"], "ok": true, "value": {"tables": [{"name": "reading-list", "description": "Books", "owner": "a_1", "rows": 2}]}}));
        assert_eq!(m.tables[0].name, "reading-list");
        let asked = m.open_table(Some("reading-list".into()));
        assert_eq!(asked[1]["name"], "table.rows");
        assert_eq!(asked[1]["args"]["desc"], true);
        m.take(&json!({"t": "result", "id": asked[1]["id"], "ok": true, "value": {"owner": "a_1", "more": false, "rows": [
            {"id": 2, "by": "a_2", "untrusted": true, "data": {"title": "Emma"}},
            {"id": 1, "by": "a_1", "data": {"title": "Dune"}}
        ]}}));
        assert_eq!(m.table_rows.len(), 2);
        assert!(m.table_rows[0].untrusted && !m.table_rows[1].untrusted);
        assert_eq!(m.table_rows[1].data, "title: Dune");
        // A change reloads it, and dropping it closes it.
        m.take(
            &json!({"t": "event", "kind": "table.changed", "table": "reading-list", "by": "a_2"}),
        );
        let again = m.fetches();
        assert!(again.iter().any(|f| f["name"] == "table.rows"));
        m.take(&json!({"t": "event", "kind": "table.changed", "table": "reading-list", "by": "d_1", "dropped": true}));
        assert!(m.table_open.is_none() && m.table_rows.is_empty());
        assert_eq!(m.drop_table("x")["name"], "table.drop");
    }

    #[test]
    fn a_page_is_fetched_and_shown_beside_the_chat() {
        let page = FileRef::of(
            &json!({"file": "ab12", "name": "Sales this week.html", "mime": "text/html", "size": 90}),
        );
        assert!(page.page() && !page.image());
        assert_eq!(page.title(), "Sales this week");
        let mut m = Model::new();
        let f = m.open_page(&page).expect("it's fetched first");
        assert_eq!(f["t"], "file.get");
        assert!(m.page_path().is_none(), "not until it's here");
        m.take(&json!({"t": "core.file", "id": f["id"], "path": "C:/cache/ab12.html"}));
        assert_eq!(
            m.page_path().map(String::as_str),
            Some("C:/cache/ab12.html")
        );
        assert!(m.to_open.is_empty(), "it isn't opened in a browser");
        assert!(m.open_page(&page).is_none(), "and isn't fetched again");
        // One that can't be fetched closes, and says why.
        let gone = FileRef {
            file: "cd34".into(),
            ..page
        };
        let f = m.open_page(&gone).unwrap();
        m.take(&json!({"t": "core.file", "id": f["id"], "error": {"message": "gone"}}));
        assert!(m.page.is_none());
        assert_eq!(m.notices.last().map(String::as_str), Some("gone"));
    }

    #[test]
    fn html_a_person_typed_shows_as_they_typed_it() {
        assert_eq!(
            literal_html("<style>body{}</style><h1>Hi</h1>"),
            r"\<style>body{}\</style>\<h1>Hi\</h1>"
        );
        assert_eq!(
            literal_html("a `<b>` and ``x ` <i>`` c <d>"),
            r"a `<b>` and ``x ` <i>`` c \<d>",
            "code spans keep theirs"
        );
        assert_eq!(
            literal_html("see\n```html\n<p>hi</p>\n```\n<p>"),
            "see\n```html\n<p>hi</p>\n```\n\\<p>",
            "so does a fenced block, up to its end"
        );
        assert_eq!(literal_html(r"\<b> 1 < 2 ✓"), r"\<b> 1 \< 2 ✓");
        assert_eq!(literal_html("an open `tick <b>"), r"an open `tick \<b>");
    }

    #[test]
    fn a_routine_flow_is_saved_shown_while_it_runs_and_stopped() {
        let mut m = Model::new();
        let chain = vec![
            FlowStep {
                name: "1".into(),
                agent: "a_2".into(),
                prompt: "Sum it up".into(),
                after: vec!["routine".into()],
            },
            FlowStep {
                name: "2".into(),
                agent: "a_3".into(),
                prompt: "Post it".into(),
                after: vec!["1".into()],
            },
        ];
        assert!(flow_is_chain(&chain));
        let mut side_by_side = chain.clone();
        side_by_side[1].after = vec!["routine".into(), "1".into()];
        assert!(!flow_is_chain(&side_by_side));
        let form = RoutineForm {
            name: "Morning".into(),
            agent: "a_1".into(),
            prompt: "news".into(),
            kind: "every".into(),
            at: "30".into(),
            flow: Some(chain),
            ..RoutineForm::default()
        };
        let f = m.save_routine(&form);
        assert_eq!(f["args"]["flow"][1]["after"], json!(["1"]));
        let kept = RoutineForm { flow: None, ..form };
        assert!(
            m.save_routine(&kept)["args"].get("flow").is_none(),
            "no flow leaves it as it is"
        );
        let status = |finished: bool| json!({"t": "event", "kind": "flow.updated", "flow": {"id": "fl_1", "routine": "rt_1", "stopped": false, "finished": finished, "steps": [{"name": "routine", "agent": "a_1", "status": "done"}, {"name": "1", "agent": "a_2", "status": "running"}]}});
        m.take(&status(false));
        m.take(&status(false));
        assert_eq!(m.flows.len(), 1, "a flow is listed once while it runs");
        assert_eq!(
            m.flows[0].steps[1],
            ("1".into(), "a_2".into(), "running".into())
        );
        let stop = m.stop_flow("rt_1");
        assert_eq!(stop["name"], "routine.stop");
        assert_eq!(stop["args"]["id"], "rt_1");
        m.take(&status(true));
        assert!(m.flows.is_empty(), "a finished flow goes");
    }

    #[test]
    fn the_people_page_reads_the_people_log_and_reads_it_again_when_it_grows() {
        let mut m = Model::new();
        m.people_open = true;
        let frames = m.load_people();
        assert_eq!(frames[2]["name"], "people.log");
        let id = frames[2]["id"].as_str().unwrap().to_string();
        m.take(
            &json!({"t": "result", "id": id, "ok": true, "value": {"entries": [
                {"id": 2, "kind": "role", "text": "Made ann an admin", "by": "elliot", "at": 5},
                {"id": 1, "kind": "joined", "text": "ann joined as a member", "by": "ann", "at": 4},
            ]}}),
        );
        assert_eq!(m.people_log.len(), 2);
        assert_eq!(m.people_log[0].text, "Made ann an admin");
        assert!(
            m.people.is_empty(),
            "the log's answer isn't taken for user.list's"
        );
        m.take(&json!({"t": "event", "kind": "people.logged", "entry": {"kind": "invited", "text": "Took back an invite", "by": "elliot", "at": 6}}));
        assert!(
            m.fetches().iter().any(|f| f["name"] == "people.log"),
            "a new entry reads the log again"
        );
    }

    #[test]
    fn a_branch_opens_once_made_and_is_listed_under_its_message() {
        let chat = Conversation {
            id: "c_chat".into(),
            agent: "a_1".into(),
            task: None,
            side: false,
            branch_of: None,
            branch_at: None,
            routine: None,
            program: None,
            title: String::new(),
        };
        let mut m = Model::new();
        m.conversations = vec![chat.clone()];
        m.select(Some("a_1".into()));
        let f = m.branch("msg_3");
        assert_eq!(f["name"], "conversation.branch");
        let id = f["id"].as_str().unwrap().to_string();
        m.take(
            &json!({"t": "result", "id": id, "ok": true, "value": {"conversation": "c_branch"}}),
        );
        assert_eq!(
            m.conversation().as_deref(),
            Some("c_branch"),
            "the branch shows before the list has it"
        );
        assert!(m.branch_open().is_none());
        m.conversations.push(Conversation {
            id: "c_branch".into(),
            branch_of: Some("c_chat".into()),
            branch_at: Some("msg_3".into()),
            routine: None,
            program: None,
            title: "Socks, again".into(),
            ..chat
        });
        assert_eq!(m.branch_open().unwrap().title, "Socks, again");
        assert_eq!(m.send("hi").unwrap()["conversation"], "c_branch");
        m.show_branch(None);
        assert_eq!(m.conversation().as_deref(), Some("c_chat"));
        let under = m.branches();
        assert_eq!(
            under.get("msg_3"),
            Some(&vec![("c_branch".to_string(), "Socks, again".to_string())])
        );
        // Another agent closes it.
        m.show_branch(Some("c_branch".into()));
        m.select(Some("a_2".into()));
        assert!(m.open_branch.is_none());
    }

    #[test]
    fn a_server_says_where_it_is_and_an_offered_one_is_kept_for_this_device() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.conn", "state": "live", "server": "https://loom.example.com"}));
        assert_eq!(m.server.as_deref(), Some("https://loom.example.com"));
        m.take(&json!({"t": "welcome", "device": "d_1"}));
        m.take(&json!({"t": "event", "kind": "server.offered", "address": "https://two.example.com", "by": "d_2"}));
        assert!(
            m.offered.is_empty(),
            "one said yes to on another device is that one's"
        );
        m.take(&json!({"t": "event", "kind": "server.offered", "address": "https://two.example.com", "by": "d_1"}));
        assert_eq!(m.offered, vec!["https://two.example.com".to_string()]);
    }

    #[test]
    fn a_task_has_its_own_conversation_beside_the_chat() {
        let mut m = Model::new();
        m.conversations = vec![
            Conversation {
                id: "c_task".into(),
                agent: "a_1".into(),
                task: Some("task_1".into()),
                side: false,
                branch_of: None,
                branch_at: None,
                routine: None,
                program: None,
                title: String::new(),
            },
            Conversation {
                id: "c_chat".into(),
                agent: "a_1".into(),
                task: None,
                side: false,
                branch_of: None,
                branch_at: None,
                routine: None,
                program: None,
                title: String::new(),
            },
        ];
        m.select(Some("a_1".into()));
        // The newest conversation is the task's, but the chat is the chat.
        assert_eq!(m.conversation().as_deref(), Some("c_chat"));
        m.take(&json!({"t": "event", "kind": "task.updated", "task": {"id": "task_1", "agent": "a_1", "conversation": "c_task", "title": "Count", "status": "running"}}));
        assert_eq!(m.tasks.len(), 1);
        m.show_task(Some("task_1".into()));
        assert_eq!(m.conversation().as_deref(), Some("c_task"));
        let sent = m.send("more socks").unwrap();
        assert_eq!(
            sent["conversation"], "c_task",
            "a message goes into the task"
        );
        m.take(&json!({"t": "event", "kind": "task.updated", "task": {"id": "task_1", "agent": "a_1", "conversation": "c_task", "title": "Count", "status": "failed", "error": "no socks"}}));
        assert_eq!(m.tasks.len(), 1, "an update replaces the task");
        assert_eq!(m.tasks[0].error, "no socks");
        // Another agent closes it, and the chat's send names no conversation.
        m.select(Some("a_2".into()));
        assert!(m.task_open().is_none());
        m.select(Some("a_1".into()));
        assert_eq!(m.conversation().as_deref(), Some("c_chat"));
        assert!(m.send("hi").unwrap().get("conversation").is_none());
    }

    #[test]
    fn the_queue_is_heard_asked_and_reordered() {
        let mut m = Model::new();
        m.conversations = vec![Conversation {
            id: "c_1".into(),
            agent: "a_1".into(),
            task: None,
            side: false,
            branch_of: None,
            branch_at: None,
            routine: None,
            program: None,
            title: String::new(),
        }];
        m.select(Some("a_1".into()));
        // Opening the chat asks for its queue.
        let asked = m.fetches();
        assert!(asked
            .iter()
            .any(|f| f["name"] == "queue.list" && f["args"]["conversation"] == "c_1"));
        m.take(&json!({"t": "result", "id": "app.queue:c_1", "value": {"queue": [{"id": "q_1", "text": "first"}]}}));
        assert_eq!(m.queue().len(), 1);
        m.take(
            &json!({"t": "event", "kind": "queue.changed", "conversation": "c_1", "queue": [
                {"id": "q_1", "text": "first"},
                {"id": "q_2", "text": "", "files": ["f_1", "f_2"], "steered": true},
            ]}),
        );
        let q = m.queue();
        assert_eq!(q[1].files, 2);
        assert!(q[1].steered);
        assert!(m.queue_up("q_1").is_none(), "the first can't go higher");
        let up = m.queue_up("q_2").unwrap();
        assert_eq!(up["name"], "queue.order");
        assert_eq!(up["args"]["ids"], json!(["q_2", "q_1"]));
        let edit = m.queue_cmd("queue.edit", "q_1", Some("changed"));
        assert_eq!(edit["args"], json!({"id": "q_1", "text": "changed"}));
        // Another conversation's queue isn't this chat's.
        m.take(&json!({"t": "event", "kind": "queue.changed", "conversation": "c_9", "queue": []}));
        assert_eq!(m.queue().len(), 2);
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
        let [people, invites, _log] = m.load_people().try_into().unwrap();
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
        let [people, _, _] = m.load_people().try_into().unwrap();
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
                left: Some(1200),
                members: None,
            })
        );
        assert_eq!(m.installed[1].share, None);
        assert!(!m.programs_owner);
        let shared = m.share_program("codex", 100, "month");
        assert_eq!(shared["name"], "acp.share");
        assert_eq!(
            shared["args"],
            json!({"name": "codex", "tokens": 100, "per": "month"})
        );
    }

    #[test]
    fn the_owner_gives_a_program_to_some_people() {
        let mut m = Model::new();
        let list = m.programs();
        m.result(&json!({"t": "result", "id": list["id"], "ok": true, "value": {"owner": true, "programs": [
            {"name": "claude", "source": "", "version": "", "settings": ["ANTHROPIC_API_KEY"], "members": ["u_ann"], "share": {"tokens": 5000, "per": "day", "members": []}},
            {"name": "codex", "source": "", "version": "", "settings": []}
        ]}}));
        assert!(m.programs_owner);
        assert_eq!(m.installed[0].members, Some(vec!["u_ann".to_string()]));
        assert_eq!(m.installed[0].share.as_ref().unwrap().members, Some(vec![]));
        assert_eq!(m.installed[1].members, None);

        // From everyone, a person clicked is the only one; then each click adds or takes away.
        let ann = "u_ann".to_string();
        let bo = "u_bo".to_string();
        assert_eq!(toggle_member(None, Some("u_ann")), Some(vec![ann.clone()]));
        assert_eq!(
            toggle_member(Some(std::slice::from_ref(&ann)), Some("u_bo")),
            Some(vec![ann.clone(), bo.clone()])
        );
        assert_eq!(
            toggle_member(Some(&[ann.clone(), bo]), Some("u_ann")),
            Some(vec!["u_bo".to_string()])
        );
        assert_eq!(toggle_member(Some(std::slice::from_ref(&ann)), None), None);

        let given = m.program_members("claude", Some(vec![ann.clone()]));
        assert_eq!(given["name"], "acp.members");
        assert_eq!(
            given["args"],
            json!({"name": "claude", "members": ["u_ann"]})
        );
        let everyone = m.program_members("claude", None);
        assert_eq!(everyone["args"], json!({"name": "claude", "members": null}));
        let share = m.installed[0].share.clone().unwrap();
        let keys = m.share_to("claude", &share, Some(vec![ann]));
        assert_eq!(
            keys["args"],
            json!({"name": "claude", "tokens": 5000, "per": "day", "members": ["u_ann"]})
        );
    }

    #[test]
    fn a_brain_has_its_registry_icon() {
        let mut m = Model::new();
        let asked = m.load_programs();
        let reg = asked.iter().find(|f| f["name"] == "acp.registry").unwrap();
        m.result(&json!({"t": "result", "id": reg["id"], "ok": true, "value": {
            "agents": [
                {"id": "claude-acp", "name": "Claude Agent", "version": "1", "description": "", "how": "npx", "icon_svg": "<svg/>"},
                {"id": "amp-acp", "name": "Amp", "version": "1", "description": "", "how": null}
            ],
            "programs": {"claude": "claude-acp"},
            "platform": "windows-x86_64"
        }}));
        assert_eq!(m.registry[0].icon.as_deref(), Some("<svg/>"));
        assert_eq!(m.registry[1].icon, None);
        // A program added by hand has the icon of the registry agent it is.
        assert_eq!(
            m.program_listed("claude").map(|l| l.name.as_str()),
            Some("Claude Agent")
        );
        assert!(m.program_listed("codex").is_none());
    }

    #[test]
    fn the_settings_panel_counts_what_an_agent_has() {
        let mut m = Model::new();
        m.select(Some("a_1".into()));
        let asked = m.edit_agent();
        assert_eq!(asked.len(), 3);
        assert_eq!(asked[1]["name"], "agent.summary");
        assert_eq!(asked[2]["name"], "acp.list");
        assert_eq!(m.summary, None);
        m.result(
            &json!({"t": "result", "id": asked[1]["id"], "ok": true, "value": {
            "memories": 3, "skills": 2, "tables": 1, "files": 9, "bytes": 2048,
            "routines": 4, "tokens_today": 850, "tokens_month": 12400}}),
        );
        let s = m.summary.clone().unwrap();
        assert_eq!((s.memories, s.routines, s.bytes), (3, 4, 2048));
        assert_eq!(tokens_text(850), "850");
        assert_eq!(tokens_text(12_400), "12.4k");
        assert_eq!(tokens_text(1_200_000), "1.2M");
        let f = m.set_color("teal").unwrap();
        assert_eq!(f["args"]["color"], "teal");
    }

    #[test]
    fn an_agent_runs_isolated_with_a_list_of_hosts() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "One", "isolated": true, "network": ["github.com"]},
            {"id": "a_2", "name": "Two"},
        ]}));
        assert_eq!(m.agents[0].network, Some(vec!["github.com".to_string()]));
        assert!(!m.agents[1].isolated && m.agents[1].network.is_none());
        m.select(Some("a_1".into()));
        let asked = m.edit_agent();
        m.result(&json!({"t": "result", "id": asked[2]["id"], "ok": true, "value": {"programs": [], "owner": true, "sandboxes": true}}));
        assert!(m.sandboxes);
        let f = m.add_host(" https://Registry.NPMJS.org/x ").unwrap();
        assert_eq!(
            f["args"]["network"],
            json!(["github.com", "registry.npmjs.org"])
        );
        assert!(m.add_host("github.com").is_none());
        let f = m.remove_host("github.com").unwrap();
        assert_eq!(f["args"]["network"], json!(["registry.npmjs.org"]));
        let f = m.set_isolated(false).unwrap();
        assert_eq!(f["args"]["isolated"], false);
        assert_eq!(f["args"]["network"], Value::Null);
        assert!(m.agents[0].network.is_none());
        // Refused, as without sandboxes: the list as the instance has it comes again.
        m.result(&json!({"t": "result", "id": f["id"], "ok": false, "error": {"code": "not_here", "message": "No sandboxes."}}));
        assert_eq!(m.notices.last().map(String::as_str), Some("No sandboxes."));
        assert!(m.dirty.contains(&List::Agents));
    }

    #[test]
    fn a_plan_shows_its_entries_and_how_far_each_is() {
        let b = Block::of(&json!({"kind": "plan", "entries": [
            {"content": "Read the notes", "status": "completed", "priority": "high"},
            {"content": " Check the files ", "status": "in_progress"},
            {"content": "Write a summary", "status": "pending"},
        ]}));
        assert_eq!(
            b.text,
            "done Read the notes\ndoing Check the files\ntodo Write a summary"
        );
    }

    #[test]
    fn a_title_reads_on_after_other_words() {
        assert_eq!(
            mid_sentence("Clean out the old build"),
            "clean out the old build"
        );
        assert_eq!(mid_sentence("README check"), "README check");
        assert_eq!(mid_sentence("run git push"), "run git push");
        assert_eq!(mid_sentence(""), "");
    }

    #[test]
    fn no_door_without_a_panel() {
        let mut m = Model::new();
        assert!(m.door(1, "data.get", &json!({"key": "k"})).is_err());
    }

    #[test]
    fn pinned_agents_come_first_in_the_order_they_were_pinned() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "One"},
            {"id": "a_2", "name": "Two", "pinned": 200},
            {"id": "a_3", "name": "Three"},
            {"id": "a_4", "name": "Four", "pinned": 100}
        ]}));
        let order: Vec<&str> = m.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(order, ["a_4", "a_2", "a_1", "a_3"]);
        m.select(Some("a_1".into()));
        let f = m.pin_agent(true).unwrap();
        assert_eq!(f["name"], "agent.pin");
        assert_eq!(f["args"]["agent"], "a_1");
    }

    #[test]
    fn a_clicked_notification_opens_what_it_was_about() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "One"},
            {"id": "a_2", "name": "Two"}
        ]}));
        let chat = Conversation {
            id: "c_chat".into(),
            agent: "a_2".into(),
            task: None,
            side: false,
            branch_of: None,
            branch_at: None,
            routine: None,
            program: None,
            title: String::new(),
        };
        m.conversations = vec![
            chat.clone(),
            Conversation {
                id: "c_thread".into(),
                routine: Some("r_1".into()),
                ..chat.clone()
            },
            Conversation {
                id: "c_task".into(),
                task: Some("t_1".into()),
                ..chat.clone()
            },
        ];
        m.select(Some("a_1".into()));
        m.open_about("a_2", "c_chat");
        assert_eq!(m.selected.as_deref(), Some("a_2"));
        assert_eq!(m.conversation().as_deref(), Some("c_chat"));
        m.open_about("a_2", "c_thread");
        assert_eq!(
            m.conversation().as_deref(),
            Some("c_thread"),
            "a routine's thread"
        );
        m.open_about("a_2", "c_task");
        assert_eq!(m.open_task.as_deref(), Some("t_1"), "a task");
        m.open_about("a_9", "c_chat");
        assert_eq!(
            m.selected.as_deref(),
            Some("a_2"),
            "an agent that's gone opens nothing"
        );
    }
}
