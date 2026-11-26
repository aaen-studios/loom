//! Spools, the integrations catalog (PLAN.md, "The integrations catalog"): its entries
//! as `catalog.list` gives them; which of them the Find tab shows for the pill picked
//! and what's typed; an entry's detail and the steps of connecting one, as the
//! instance's `connect` frames move them; and the person's servers as the Installed
//! tab lists them, in its order, with how each stands. The screens only draw it.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};

use super::popup::{finds, Dot};
use super::{s, Badge, McpServer, McpStatus, McpTest, Model, SignIn};

/// The entries the Find tab shows first, as two wide cards: loom's own.
pub const FEATURED: [&str; 2] = ["acta", "t3"];

/// The category pills after All, in their order. A category the catalog has that
/// isn't one of them comes after them.
pub const PILLS: [&str; 6] = ["Work", "Code", "Mail & calendar", "Files", "Home", "Fun"];

/// A field an entry asks the person to fill in, such as T3's address. One from the
/// registry may have a default, used where it's left empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Field {
    pub name: String,
    pub label: String,
    pub placeholder: String,
    pub default: String,
}

/// A secret an entry asks for, and whether the person has one by its name already. One
/// from the registry may be optional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Key {
    pub name: String,
    pub label: String,
    pub placeholder: String,
    pub have: bool,
    pub optional: bool,
}

/// One of the catalog's entries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub maker: String,
    /// Its badge, with the icons the person's server from it gave of its own once it has.
    pub badge: Option<Badge>,
    /// Its mark, an SVG in currentColor, drawn on its badge until a server gives an icon.
    pub icon_svg: Option<String>,
    /// The name its server is added as, unless the person gives another.
    pub server: String,
    /// Reached over HTTP, or a program the instance starts.
    pub remote: bool,
    /// Its address, with a `{field}` for each field; or the program, for one the
    /// instance starts, with its arguments.
    pub url: String,
    pub command: String,
    pub fields: Vec<Field>,
    /// `oauth`, `secret` or `none`.
    pub sign_in: String,
    pub secrets: Vec<Key>,
    /// One line, only where the person would go wrong without it.
    pub note: String,
    /// Its tools, each with whether it changes something rather than only reading.
    pub tools: Vec<(String, bool)>,
    /// False where the tools' names couldn't be checked against the server.
    pub tools_checked: bool,
    /// The person's servers made from it.
    pub servers: Vec<String>,
    /// One of the MCP Registry's, from a search, by its name there and its version.
    pub community: bool,
    pub registry: String,
    pub version: String,
    /// Why loom can't connect it, for one from the registry it can't.
    pub unsupported: String,
}

impl Entry {
    fn of(v: &Value) -> Entry {
        let list = |key: &str| {
            v.get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut command = s(v, "command");
        for a in list("args").iter().filter_map(Value::as_str) {
            command.push(' ');
            command.push_str(a);
        }
        Entry {
            id: s(v, "id"),
            name: s(v, "name"),
            category: s(v, "category"),
            description: s(v, "description"),
            maker: s(v, "maker"),
            badge: v.get("badge").and_then(Badge::of),
            icon_svg: v
                .get("icon_svg")
                .and_then(Value::as_str)
                .filter(|svg| !svg.is_empty())
                .map(str::to_string),
            server: s(v, "server"),
            remote: s(v, "runs") != "instance",
            url: s(v, "url"),
            command,
            // A registry server's field or secret is labelled by its own name, so what
            // it's for, as the registry says, is its placeholder where it has none.
            fields: list("fields")
                .iter()
                .map(|f| Field {
                    name: s(f, "name"),
                    label: s(f, "label"),
                    placeholder: first_of(&[
                        s(f, "placeholder"),
                        s(f, "default"),
                        s(f, "description"),
                    ]),
                    default: s(f, "default"),
                })
                .collect(),
            sign_in: s(v, "sign_in"),
            secrets: list("secrets")
                .iter()
                .map(|k| Key {
                    name: s(k, "name"),
                    label: s(k, "label"),
                    placeholder: first_of(&[s(k, "placeholder"), s(k, "description")]),
                    have: k.get("have").and_then(Value::as_bool).unwrap_or(false),
                    optional: k.get("optional").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect(),
            note: s(v, "note"),
            tools: list("tools")
                .iter()
                .map(|t| (s(t, "name"), s(t, "kind") == "change"))
                .collect(),
            tools_checked: v
                .get("tools_checked")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            servers: list("servers")
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            community: v.get("community").and_then(Value::as_bool).unwrap_or(false),
            registry: s(v, "registry"),
            version: s(v, "version"),
            unsupported: s(v, "unsupported"),
        }
    }

    /// One of the two the Find tab shows first, as a wide card.
    pub fn featured(&self) -> bool {
        FEATURED.contains(&self.id.as_str())
    }

    pub fn connected(&self) -> bool {
        !self.servers.is_empty()
    }

    /// Whether the person can connect more than one: a curated entry reached at an
    /// address they give, such as T3 on each of their computers.
    pub fn several(&self) -> bool {
        !self.community && !self.fields.is_empty()
    }

    /// Whether connecting asks for something first: a field, or a secret the person
    /// doesn't have. Another of one they have always asks, for its address.
    pub fn asks(&self, another: bool) -> bool {
        another || !self.fields.is_empty() || self.secrets.iter().any(|k| !k.have)
    }

    /// Where it runs, in words: the host it's reached at, the address the person
    /// gives, or the program it is on their server.
    pub fn runs_words(&self) -> String {
        if !self.remote {
            let program = self
                .command
                .split_whitespace()
                .next()
                .unwrap_or("its program");
            return format!("On your server, with {program}");
        }
        if self.url.contains('{') {
            return "At the address you give".into();
        }
        host(&self.url)
    }

    /// How it signs in, in words.
    pub fn sign_in_words(&self) -> String {
        match self.sign_in.as_str() {
            "oauth" => format!("With your {} account", self.name),
            // A registry server's secrets are labelled by their own names.
            "secret" if self.community => "With a key".into(),
            "secret" => match self.secrets.first() {
                Some(k) if !k.label.is_empty() => {
                    let mut label = k.label.clone();
                    if let Some(first) = label.get(..1) {
                        let lower = first.to_lowercase();
                        label.replace_range(..1, &lower);
                    }
                    let a = if label.starts_with(['a', 'e', 'i', 'o', 'u']) {
                        "an"
                    } else {
                        "a"
                    };
                    format!("With {a} {label}")
                }
                _ => "With a key".into(),
            },
            // A remote from the registry with no keys: the registry doesn't say.
            "unknown" => "Known once it's connected".into(),
            _ => "No sign-in".into(),
        }
    }

    /// How `catalog.connect` names it: a curated entry by its id, one from the registry
    /// by its name there and its version.
    fn connect_args(&self) -> Value {
        if self.community {
            json!({"registry": self.registry, "version": self.version})
        } else {
            json!({"id": self.id})
        }
    }
}

/// The first of `texts` that isn't empty.
fn first_of(texts: &[String]) -> String {
    texts
        .iter()
        .find(|t| !t.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

/// The host of an address, as `https://acta.aaenz.no/api/mcp` is `acta.aaenz.no`.
pub fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// A step of connecting one, as the instance's `connect` frames say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Adding,
    SignIn,
    Agents,
    Failed,
    Done,
}

impl Step {
    fn of(step: &str) -> Step {
        match step {
            "sign_in" => Step::SignIn,
            "agents" => Step::Agents,
            "failed" => Step::Failed,
            "done" => Step::Done,
            _ => Step::Adding,
        }
    }
}

/// A connect under way: the entry, the server it's adding, its step, why it failed
/// or a sign-in didn't work, and the tools it found.
#[derive(Clone, Debug, PartialEq)]
pub struct Connect {
    pub entry: String,
    pub server: String,
    pub step: Step,
    pub said: String,
    pub tools: Vec<String>,
}

impl Connect {
    fn of(v: &Value) -> Option<Connect> {
        let server = s(v, "server");
        if server.is_empty() {
            return None;
        }
        Some(Connect {
            entry: s(v, "entry"),
            server,
            step: Step::of(&s(v, "step")),
            said: s(v, "said"),
            tools: v
                .get("tools")
                .and_then(Value::as_array)
                .map(|ts| {
                    ts.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// Where the detail pane of an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// What it is, with Connect, or how its servers stand once it has some.
    About,
    /// Its fields and secrets, to fill in before it's connected.
    Form,
    /// Connecting, at this step, as the instance last said.
    Going(Step),
}

/// The entry whose detail pane is open on the Find tab, and how far connecting it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Flow {
    pub entry: String,
    /// Another server from an entry the person has one of already.
    pub another: bool,
    /// The server it adds, once the instance has said.
    pub server: String,
    pub stage: Stage,
    /// What went wrong, in one line: a field left out, or the instance's why.
    pub said: String,
    /// The agents picked to get it, at its agents step.
    pub agents: Vec<String>,
    /// Every tool shown, after Show all.
    pub all_tools: bool,
}

impl Flow {
    fn new(entry: &str) -> Flow {
        Flow {
            entry: entry.to_string(),
            another: false,
            server: String::new(),
            stage: Stage::About,
            said: String::new(),
            agents: Vec::new(),
            all_tools: false,
        }
    }
}

/// How many tools a pane shows before Show all.
pub const TOOLS_SHOWN: usize = 8;

/// The tools a pane shows, by their place in `tools`: every one after Show all, or
/// for an entry with more, the first `TOOLS_SHOWN`, as many that change something as
/// that only read where it has enough of both, in the catalog's order.
pub fn tools_shown(tools: &[(String, bool)], all: bool) -> Vec<usize> {
    if all || tools.len() <= TOOLS_SHOWN {
        return (0..tools.len()).collect();
    }
    let reads: Vec<usize> = (0..tools.len()).filter(|&i| !tools[i].1).collect();
    let changes: Vec<usize> = (0..tools.len()).filter(|&i| tools[i].1).collect();
    let half = TOOLS_SHOWN / 2;
    let r = if changes.len() < half {
        TOOLS_SHOWN - changes.len()
    } else {
        half.min(reads.len())
    };
    let mut out: Vec<usize> = reads[..r].to_vec();
    out.extend(&changes[..(TOOLS_SHOWN - r).min(changes.len())]);
    out.sort_unstable();
    out
}

/// What can be done with one of the person's servers, from the Installed tab or its
/// entry's pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    SignIn,
    /// Tries one not tried yet (`mcp.test`).
    Try,
    Reconnect,
    Disconnect,
    Remove,
}

/// One of the person's servers as the Installed tab lists it: its place in the
/// model's servers, the entry it came from, its name, the line under it, and its dot.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub server: usize,
    pub entry: Option<usize>,
    pub title: String,
    pub line: String,
    pub dot: Option<Dot>,
}

/// Spools as the model keeps it.
#[derive(Default)]
pub struct Spools {
    pub entries: Vec<Entry>,
    pub categories: Vec<String>,
    /// Whether Spools has been opened, so the catalog is kept up to date as the
    /// person's servers change.
    pub loaded: bool,
    /// Whether the catalog has come, and why it couldn't, from an instance without it.
    pub came: bool,
    pub error: Option<String>,
    /// The category pill picked; none is All.
    pub pill: Option<String>,
    /// The entry whose pane is open on the Find tab.
    pub flow: Option<Flow>,
    /// The server picked on the Installed tab.
    pub picked: Option<String>,
    /// Each connect under way, by the server it adds.
    pub connects: HashMap<String, Connect>,
    /// Servers connected again by Reconnect: their agents step is answered with the
    /// agents they had.
    reconnecting: HashSet<String>,
    /// The agent Spools was opened for, from its composer or its settings, picked
    /// first at the agents step.
    pub for_agent: Option<String>,
    /// The search of the MCP Registry for what's typed, once it's long enough.
    pub search: Search,
}

/// How few letters the MCP Registry is searched for.
pub const SHORTEST: usize = 3;

/// A search of the MCP Registry (`catalog.search`): what it's for, the command asked,
/// and once answered, the registry's servers, the curated entries it found, or why the
/// registry couldn't say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Search {
    pub query: String,
    asked: String,
    pub answered: bool,
    pub entries: Vec<Entry>,
    pub curated: Vec<String>,
    pub error: Option<String>,
}

/// What the Find tab shows: loom's own that match as cards, the rest of the catalog's
/// that match and then the registry's as tiles, and under them the search's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    pub cards: Vec<Entry>,
    pub tiles: Vec<Entry>,
    /// Searching…, or why the registry couldn't be searched.
    pub line: Option<String>,
    /// Nothing matches, the registry included.
    pub nothing: bool,
}

/// The pills, All aside: the decided ones, then any other category the catalog has.
pub fn pills(categories: &[String]) -> Vec<String> {
    let mut out: Vec<String> = PILLS.iter().map(|c| c.to_string()).collect();
    for c in categories {
        if !out.contains(c) {
            out.push(c.clone());
        }
    }
    out
}

/// What the Find tab shows of the catalog for the pill picked (none is All) and what's
/// typed: the featured entries that match, as cards, and the rest that match, as tiles,
/// each by its place in `entries`. `also` are the ids of those the registry's search
/// found too, such as one whose address or package a registry server named is.
pub fn found(
    entries: &[Entry],
    pill: Option<&str>,
    query: &str,
    also: &[String],
) -> (Vec<usize>, Vec<usize>) {
    let mut cards = Vec::new();
    let mut tiles = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if pill.is_some_and(|p| p != e.category) {
            continue;
        }
        let text = format!("{} {} {} {}", e.name, e.description, e.category, e.maker);
        if !finds(query, &text) && !also.contains(&e.id) {
            continue;
        }
        if e.featured() {
            cards.push(i);
        } else {
            tiles.push(i);
        }
    }
    (cards, tiles)
}

/// How an entry's card stands: none until the person has a server from it, then the
/// worst of how its servers stand, in Installed's words: failing, needing a sign-in,
/// not tried yet, or else connected. Its thread is taut only while it's connected.
pub fn card_status(entry: &Entry, servers: &[McpServer]) -> Option<(String, Dot)> {
    let mine: Vec<&McpServer> = servers
        .iter()
        .filter(|m| entry.servers.contains(&m.name))
        .collect();
    if mine.is_empty() {
        return None;
    }
    for worst in [McpStatus::Failing, McpStatus::SignIn, McpStatus::Untested] {
        if let Some(m) = mine.iter().find(|m| m.status == worst) {
            return Some((status_words(m), status_dot(m)));
        }
    }
    Some(("Connected".into(), Dot::Ok))
}

/// How one of the person's servers stands, in words.
pub fn status_words(m: &McpServer) -> String {
    match m.status {
        McpStatus::Connected => match &m.sign_in {
            SignIn::SignedIn(who) if !who.is_empty() => format!("Connected as {who}"),
            _ => "Connected".into(),
        },
        McpStatus::SignIn => "Needs you to sign in".into(),
        McpStatus::Failing if m.why.is_empty() => "Not working".into(),
        McpStatus::Failing => m.why.clone(),
        McpStatus::Untested => "Not tried yet".into(),
    }
}

/// How it stands, as its pane says it: with how many tools it offers, once connected.
pub fn status_line(m: &McpServer) -> String {
    let words = status_words(m);
    match (m.status, m.tools.len()) {
        (McpStatus::Connected, 0) => words,
        (McpStatus::Connected, 1) => format!("{words} · 1 tool"),
        (McpStatus::Connected, n) => format!("{words} · {n} tools"),
        _ => words,
    }
}

/// The dot of how it stands: ok, warn for one that needs a sign-in, bad for one
/// failing, and quiet before it's been tried.
pub fn status_dot(m: &McpServer) -> Dot {
    match m.status {
        McpStatus::Connected => Dot::Ok,
        McpStatus::SignIn => Dot::Warn,
        McpStatus::Failing => Dot::Bad,
        McpStatus::Untested => Dot::Quiet,
    }
}

/// What can be done with it: Sign in where it needs one, Try it before it's been tried
/// and Reconnect once it has, Disconnect where it's signed in, and Remove.
pub fn acts(m: &McpServer) -> Vec<Act> {
    let mut out = Vec::new();
    if m.status == McpStatus::SignIn || m.sign_in == SignIn::Needed {
        out.push(Act::SignIn);
    }
    out.push(if m.status == McpStatus::Untested {
        Act::Try
    } else {
        Act::Reconnect
    });
    if matches!(m.sign_in, SignIn::SignedIn(_)) {
        out.push(Act::Disconnect);
    }
    out.push(Act::Remove);
    out
}

/// The person's servers as the Installed tab lists them: those from the catalog in its
/// order, each entry's by name, then the rest by name. Each is its entry's name, or
/// its own; the line under it is the host, for an entry with several (T3 on each
/// computer), or else how it stands; a connect under way says so.
pub fn installed(
    servers: &[McpServer],
    entries: &[Entry],
    connects: &HashMap<String, Connect>,
) -> Vec<Row> {
    let mut rows: Vec<Row> = servers
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let entry = entries
                .iter()
                .position(|e| !m.catalog.is_empty() && e.id == m.catalog);
            let e = entry.map(|at| &entries[at]);
            let going = connects
                .get(&m.name)
                .is_some_and(|c| matches!(c.step, Step::Adding));
            let line = if going {
                "Connecting…".to_string()
            } else if e.is_some_and(Entry::several) {
                host(&m.url)
            } else {
                status_words(m)
            };
            Row {
                server: i,
                entry,
                title: e.map_or_else(|| m.name.clone(), |e| e.name.clone()),
                line,
                dot: Some(if going { Dot::Quiet } else { status_dot(m) }),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        let key = |r: &Row| {
            (
                r.entry.unwrap_or(usize::MAX),
                servers[r.server].name.clone(),
            )
        };
        key(a).cmp(&key(b))
    });
    rows
}

/// The line under a server's title in Installed's pane: its own name only where it
/// isn't the one its entry gives (`t3-vps-2`, not `acta`), and its host where the person
/// gave it (T3's) or added it by hand.
pub fn installed_under(m: &McpServer, entry: Option<&Entry>) -> String {
    let mut parts = Vec::new();
    match entry {
        Some(e) => {
            if m.name != e.server {
                parts.push(m.name.clone());
            }
            if e.several() {
                parts.push(host(&m.url));
            }
        }
        None => parts.push(host(&m.url)),
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" · ")
}

/// The name of another server from `entry`, reached at `address`, for an entry the
/// person has one of already: its server's name and the first part of the host, as
/// `t3-vps` for `https://vps.tail1234.ts.net`, with a number after it where that's
/// taken.
pub fn another_name(entry: &Entry, address: &str, taken: &[&str]) -> String {
    let first = host(address.trim())
        .split(['.', ':'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let label: String = first
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let base = if label.is_empty() {
        format!("{}-2", entry.server)
    } else {
        format!("{}-{label}", entry.server)
    };
    let mut name = base.clone();
    let mut n = 2;
    while taken.contains(&name.as_str()) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

/// What's missing from a form before it can connect: the label of the first field
/// without a default, or of a secret the person doesn't have and isn't optional, left
/// empty.
pub fn missing(
    entry: &Entry,
    fields: &[(String, String)],
    secrets: &[(String, String)],
) -> Option<String> {
    let given = |list: &[(String, String)], name: &str| {
        list.iter().any(|(n, v)| n == name && !v.trim().is_empty())
    };
    for f in &entry.fields {
        if f.default.is_empty() && !given(fields, &f.name) {
            return Some(format!("Fill in {}", f.label));
        }
    }
    for k in &entry.secrets {
        if !k.have && !k.optional && !given(secrets, &k.name) {
            return Some(format!("Fill in {}", k.label));
        }
    }
    None
}

/// The agents a server goes to with `agent` turned on or off.
pub fn toggled(agents: &[String], agent: &str) -> Vec<String> {
    let mut out = agents.to_vec();
    match out.iter().position(|a| a == agent) {
        Some(i) => {
            out.remove(i);
        }
        None => out.push(agent.to_string()),
    }
    out
}

impl Model {
    /// Spools opens, for `agent` when it was opened from one's composer or settings:
    /// asks for the catalog and the person's servers, which are kept up to date from
    /// then on.
    pub fn open_spools(&mut self, agent: Option<String>) -> Vec<Value> {
        self.spools.loaded = true;
        self.spools.for_agent = agent;
        self.spools.flow = None;
        self.load_mcp()
    }

    /// Asks for the catalog, and the registry's search again where there is one, as
    /// the person's servers from both change.
    pub(super) fn load_catalog(&mut self) -> Vec<Value> {
        let id = self.id("app.spools_list");
        let mut out = vec![json!({"t": "cmd", "id": id, "name": "catalog.list"})];
        if !self.spools.search.query.is_empty() {
            let query = self.spools.search.query.clone();
            out.push(self.ask_search(&query));
        }
        out
    }

    /// An entry by its id: the catalog's, or one the registry's search found.
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        let sp = &self.spools;
        sp.entries
            .iter()
            .chain(&sp.search.entries)
            .find(|e| e.id == id)
    }

    /// What's typed in Find's search, once typing pauses: the MCP Registry is searched
    /// for it from `SHORTEST` letters, and fewer forget the last search.
    pub fn spool_search(&mut self, query: &str) -> Option<Value> {
        let query = query.trim();
        if query.chars().count() < SHORTEST {
            self.spools.search = Search::default();
            return None;
        }
        if self.spools.search.query == query {
            return None;
        }
        self.spools.search = Search {
            query: query.to_string(),
            ..Search::default()
        };
        Some(self.ask_search(query))
    }

    /// `catalog.search` for `query`, whose answer is the only one taken.
    fn ask_search(&mut self, query: &str) -> Value {
        let id = self.id("app.spools_search");
        self.spools.search.asked = id.clone();
        json!({"t": "cmd", "id": id, "name": "catalog.search", "args": {"query": query}})
    }

    /// What the Find tab shows for what's typed, `query`, with the pill picked: the
    /// catalog's that match, or that the registry found, then the registry's servers,
    /// which have no category, under All; and while the registry hasn't answered for
    /// it, or couldn't, a line that says so.
    pub fn spools_shown(&self, query: &str) -> Shown {
        let sp = &self.spools;
        let query = query.trim();
        let search =
            (query.chars().count() >= SHORTEST && sp.search.query == query).then_some(&sp.search);
        let curated = search.map_or(&[][..], |s| &s.curated[..]);
        let (cards, tiles) = found(&sp.entries, sp.pill.as_deref(), query, curated);
        let mut shown = Shown {
            cards: cards.iter().map(|&i| sp.entries[i].clone()).collect(),
            tiles: tiles.iter().map(|&i| sp.entries[i].clone()).collect(),
            ..Shown::default()
        };
        if let Some(s) = search {
            if sp.pill.is_none() {
                shown.tiles.extend(s.entries.iter().cloned());
            }
            shown.line = if !s.answered {
                Some("Searching…".into())
            } else {
                s.error.clone()
            };
        }
        shown.nothing = shown.cards.is_empty() && shown.tiles.is_empty() && shown.line.is_none();
        shown
    }

    /// The person's server `name`.
    pub fn mcp_server(&self, name: &str) -> Option<&McpServer> {
        self.mcp_servers.iter().find(|m| m.name == name)
    }

    /// What the Installed tab lists, in its order.
    pub fn installed_rows(&self) -> Vec<Row> {
        installed(
            &self.mcp_servers,
            &self.spools.entries,
            &self.spools.connects,
        )
    }

    /// The answers to Spools' commands.
    pub(super) fn spools_result(&mut self, id: &str, f: &Value) {
        let error = f.get("error").map(|e| s(e, "message"));
        if id.starts_with("app.spools_list") {
            match error {
                Some(e) => self.spools.error = Some(e),
                None => self.catalog_came(f.get("value").unwrap_or(&Value::Null)),
            }
        } else if id.starts_with("app.spools_search") {
            let search = &mut self.spools.search;
            // An answer to a search typed over since is left.
            if search.asked != id {
                return;
            }
            search.answered = true;
            let v = f.get("value").unwrap_or(&Value::Null);
            let list = |key: &str| v.get(key).and_then(Value::as_array).cloned();
            search.entries = list("entries")
                .unwrap_or_default()
                .iter()
                .map(Entry::of)
                .collect();
            search.curated = list("curated")
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            search.error =
                error.or_else(|| v.get("error").and_then(Value::as_str).map(str::to_string));
        } else if id.starts_with("app.spools_connect") {
            match (error, f.pointer("/value/connect").and_then(Connect::of)) {
                (Some(e), _) => {
                    if let Some(flow) = self.spools.flow.as_mut() {
                        if flow.stage == Stage::Going(Step::Adding) {
                            flow.stage = Stage::Going(Step::Failed);
                            flow.said = e;
                        }
                    } else {
                        self.notices.push(e);
                    }
                }
                (None, Some(c)) => {
                    // The steps after it may have come first, as frames.
                    let later = self
                        .spools
                        .connects
                        .get(&c.server)
                        .is_some_and(|had| had.step != Step::Adding && had.entry == c.entry);
                    if !later {
                        self.connect_came(c);
                    }
                }
                (None, None) => {}
            }
            self.mcp_stale = true;
        } else if id.starts_with("app.spools_agents") {
            match error {
                Some(e) => self.notices.push(e),
                None => {
                    let server = f
                        .pointer("/value/server")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if let Some(flow) = self.spools.flow.as_mut() {
                        if flow.server == server && flow.stage == Stage::Going(Step::Agents) {
                            flow.stage = Stage::Going(Step::Done);
                        }
                    }
                }
            }
            self.mcp_stale = true;
        }
    }

    /// The catalog came: its entries, its categories, and the connects under way.
    fn catalog_came(&mut self, v: &Value) {
        let entries = v
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.spools.entries = entries.iter().map(Entry::of).collect();
        self.spools.categories = v
            .get("categories")
            .and_then(Value::as_array)
            .map(|cs| {
                cs.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        self.spools.came = true;
        self.spools.error = None;
        for e in &entries {
            if let Some(c) = e.get("connecting").and_then(Connect::of) {
                self.spools.connects.entry(c.server.clone()).or_insert(c);
            }
        }
    }

    /// A `connect` frame: a server being connected moved to its next step, on this
    /// device or another of the person's.
    pub(super) fn connect_frame(&mut self, f: &Value) {
        if let Some(c) = f.get("connect").and_then(Connect::of) {
            self.connect_came(c);
            // The servers and the catalog change with it.
            self.mcp_stale = true;
        }
    }

    fn connect_came(&mut self, c: Connect) {
        let sp = &mut self.spools;
        // Reconnect waits on no one at its agents step: they're as they were.
        if c.step == Step::Agents && sp.reconnecting.remove(&c.server) {
            let agents = self
                .mcp_servers
                .iter()
                .find(|m| m.name == c.server)
                .map(|m| m.agents.clone())
                .unwrap_or_default();
            let id = format!("app.spools_agents_{}", c.server);
            self.to_send.push(
                json!({"t": "cmd", "id": id, "name": "mcp.agents", "args": {"name": c.server, "agents": agents}}),
            );
        }
        if matches!(c.step, Step::Failed | Step::Done) {
            sp.reconnecting.remove(&c.server);
        }
        if let Some(flow) = sp.flow.as_mut() {
            let ours = flow.entry == c.entry
                && (flow.server == c.server || flow.server.is_empty())
                && matches!(flow.stage, Stage::Going(_));
            if ours {
                flow.server = c.server.clone();
                flow.stage = Stage::Going(c.step);
                flow.said = c.said.clone();
                if c.step == Step::Agents && flow.agents.is_empty() {
                    flow.agents = sp.for_agent.iter().cloned().collect();
                }
            }
        }
        if c.step == Step::Done {
            sp.connects.remove(&c.server);
        } else {
            sp.connects.insert(c.server.clone(), c);
        }
    }

    /// Opens the pane of the entry `id`: where a connect of it is under way, at its step.
    pub fn spool_open(&mut self, id: &str) {
        let mut flow = Flow::new(id);
        if let Some(c) = self
            .spools
            .connects
            .values()
            .find(|c| c.entry == id && c.step != Step::Done)
        {
            flow.server = c.server.clone();
            flow.stage = Stage::Going(c.step);
            flow.said = c.said.clone();
            if c.step == Step::Agents {
                flow.agents = self.spools.for_agent.iter().cloned().collect();
            }
        }
        self.spools.flow = Some(flow);
    }

    pub fn spool_close(&mut self) {
        self.spools.flow = None;
    }

    /// Connect, or Connect another, in the open pane: its form, where it asks for
    /// something first, or else connecting at once.
    pub fn spool_connect(&mut self, another: bool) -> Option<Value> {
        let open = self.spools.flow.as_ref()?.entry.clone();
        let entry = self.entry(&open)?.clone();
        if !entry.unsupported.is_empty() {
            return None;
        }
        let flow = self.spools.flow.as_mut()?;
        flow.another = another;
        flow.said.clear();
        flow.agents.clear();
        if entry.asks(another) {
            flow.stage = Stage::Form;
            return None;
        }
        flow.server = entry.server.clone();
        flow.stage = Stage::Going(Step::Adding);
        let id = self.id("app.spools_connect");
        Some(json!({"t": "cmd", "id": id, "name": "catalog.connect", "args": entry.connect_args()}))
    }

    /// The form's Connect, with what was typed in its fields and secrets: what's
    /// missing, in one line, or the connect.
    pub fn spool_submit(
        &mut self,
        fields: &[(String, String)],
        secrets: &[(String, String)],
    ) -> Option<Value> {
        let flow = self.spools.flow.as_ref()?;
        let entry = self.entry(&flow.entry)?.clone();
        if let Some(said) = missing(&entry, fields, secrets) {
            if let Some(flow) = self.spools.flow.as_mut() {
                flow.said = said;
            }
            return None;
        }
        let pick = |list: &[(String, String)]| -> serde_json::Map<String, Value> {
            list.iter()
                .filter(|(_, v)| !v.trim().is_empty())
                .map(|(n, v)| (n.clone(), json!(v.trim())))
                .collect()
        };
        let mut args = entry.connect_args();
        args["fields"] = Value::Object(pick(fields));
        args["secrets"] = Value::Object(pick(secrets));
        let name = if flow.another {
            let address = fields.first().map(|(_, v)| v.as_str()).unwrap_or_default();
            let taken: Vec<&str> = self.mcp_servers.iter().map(|m| m.name.as_str()).collect();
            another_name(&entry, address, &taken)
        } else {
            entry.server.clone()
        };
        if flow.another {
            args["name"] = json!(name);
        }
        let flow = self.spools.flow.as_mut()?;
        flow.server = name;
        flow.said.clear();
        flow.stage = Stage::Going(Step::Adding);
        let id = self.id("app.spools_connect");
        Some(json!({"t": "cmd", "id": id, "name": "catalog.connect", "args": args}))
    }

    /// Back from a connect that failed, or from the form: to the form again, for one
    /// that asks, or to the entry.
    pub fn spool_back(&mut self) {
        let Some(flow) = self.spools.flow.as_ref() else {
            return;
        };
        let asks = self
            .entry(&flow.entry)
            .is_some_and(|e| e.asks(flow.another));
        let Some(flow) = self.spools.flow.as_mut() else {
            return;
        };
        flow.stage = if flow.stage == Stage::Going(Step::Failed) && asks {
            Stage::Form
        } else {
            Stage::About
        };
        flow.said.clear();
    }

    /// An agent's chip at the agents step, turned on or off.
    pub fn spool_pick_agent(&mut self, agent: &str) {
        if let Some(flow) = self.spools.flow.as_mut() {
            flow.agents = toggled(&flow.agents, agent);
        }
    }

    /// Done at the agents step: the server goes to the agents picked.
    pub fn spool_give(&mut self) -> Option<Value> {
        let flow = self.spools.flow.as_ref()?;
        if flow.stage != Stage::Going(Step::Agents) {
            return None;
        }
        let (server, agents) = (flow.server.clone(), flow.agents.clone());
        let id = self.id("app.spools_agents");
        Some(
            json!({"t": "cmd", "id": id, "name": "mcp.agents", "args": {"name": server, "agents": agents}}),
        )
    }

    /// An agent's chip on one of the person's servers, turned on or off, as it shows
    /// at once.
    pub fn spool_toggle_agent(&mut self, server: &str, agent: &str) -> Option<Value> {
        let m = self.mcp_servers.iter_mut().find(|m| m.name == server)?;
        m.agents = toggled(&m.agents, agent);
        let agents = m.agents.clone();
        let id = self.id("app.spools_agents");
        Some(
            json!({"t": "cmd", "id": id, "name": "mcp.agents", "args": {"name": server, "agents": agents}}),
        )
    }

    /// How one of the person's servers stands in its pane, in words and a dot: being
    /// connected or tried just now says so, or else how its last test found it.
    pub fn server_standing(&self, m: &McpServer) -> (String, Dot) {
        if self
            .spools
            .connects
            .get(&m.name)
            .is_some_and(|c| c.step == Step::Adding)
        {
            return ("Connecting…".into(), Dot::Quiet);
        }
        if matches!(self.mcp_tests.get(&m.name), Some(McpTest::Testing)) {
            return ("Trying…".into(), Dot::Quiet);
        }
        (status_line(m), status_dot(m))
    }

    /// Try it: one not tried yet is tested, as the MCP servers page tests one.
    pub fn spool_try(&mut self, server: &str) -> Value {
        self.test_mcp(server)
    }

    /// Reconnect: one from the catalog is connected again from its entry, as it is,
    /// keeping its agents; one added by hand is tried again.
    pub fn spool_reconnect(&mut self, server: &str) -> Option<Value> {
        let m = self.mcp_servers.iter().find(|m| m.name == server)?;
        if m.catalog.is_empty() || !self.spools.entries.iter().any(|e| e.id == m.catalog) {
            return Some(self.test_mcp(server));
        }
        let entry = m.catalog.clone();
        self.spools.reconnecting.insert(server.to_string());
        self.spools.connects.insert(
            server.to_string(),
            Connect {
                entry: entry.clone(),
                server: server.to_string(),
                step: Step::Adding,
                said: String::new(),
                tools: Vec::new(),
            },
        );
        let id = self.id("app.spools_connect");
        Some(
            json!({"t": "cmd", "id": id, "name": "catalog.connect", "args": {"id": entry, "name": server}}),
        )
    }

    /// Remove: the server goes, and is unpicked.
    pub fn spool_remove(&mut self, server: &str) -> Value {
        if self.spools.picked.as_deref() == Some(server) {
            self.spools.picked = None;
        }
        self.spools.connects.remove(server);
        self.delete_mcp(server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Value {
        json!({"entries": [
            {"id": "acta", "name": "Acta", "category": "Work", "description": "Boards, tasks and comments in Acta.",
             "maker": "Aaen Studios", "badge": {"name": "acta", "initial": "A", "color": "plum", "icon": "ab12"}, "icon_svg": "<svg/>", "server": "acta",
             "runs": "remote", "url": "https://acta.aaenz.no/api/mcp", "fields": [], "sign_in": "oauth", "secrets": [],
             "tools": [{"name": "list_tasks", "kind": "read"}, {"name": "create_tasks", "kind": "change"}],
             "tools_from": "x", "tools_checked": true, "servers": ["acta"], "connected": true},
            {"id": "t3", "name": "T3 Code", "category": "Code", "description": "Projects and coding threads in T3 Code.",
             "maker": "T3 Tools", "server": "t3", "runs": "remote", "url": "{address}/mcp",
             "fields": [{"name": "address", "label": "Address", "placeholder": "https://host.tailnet.ts.net"}],
             "sign_in": "oauth", "secrets": [], "note": "Use an address your browser reaches too.",
             "tools": [], "tools_checked": false, "servers": [], "connected": false},
            {"id": "github", "name": "GitHub", "category": "Code", "description": "Repositories, issues and pull requests on GitHub.",
             "maker": "GitHub", "server": "github", "runs": "remote", "url": "https://api.githubcopilot.com/mcp/",
             "fields": [], "sign_in": "secret",
             "secrets": [{"name": "GITHUB_TOKEN", "label": "Personal access token", "placeholder": "github_pat_…", "have": false}],
             "tools": [], "tools_checked": true, "servers": [], "connected": false},
            {"id": "playwright", "name": "Playwright", "category": "Code", "description": "Microsoft's Playwright browser, run on the instance.",
             "maker": "Microsoft", "server": "playwright", "runs": "instance", "command": "npx", "args": ["-y", "@playwright/mcp@latest"],
             "fields": [], "sign_in": "none", "secrets": [], "tools": [], "tools_checked": true, "servers": [], "connected": false},
            {"id": "home-assistant", "name": "Home Assistant", "category": "Home", "description": "Lights, devices and what's on at home.",
             "maker": "Open Home Foundation", "server": "home-assistant", "runs": "remote", "url": "{address}/api/mcp",
             "fields": [{"name": "address", "label": "Address"}], "sign_in": "secret",
             "secrets": [{"name": "HOME_ASSISTANT_TOKEN", "label": "Long-lived access token", "have": true}],
             "tools": [], "tools_checked": false, "servers": [], "connected": false}
        ], "categories": ["Work", "Code", "Home"]})
    }

    fn with_catalog() -> Model {
        let mut m = Model::new();
        m.take(&json!({"t": "result", "id": "app.spools_list_1", "ok": true, "value": catalog()}));
        m
    }

    fn server(name: &str, catalog: &str, status: McpStatus) -> McpServer {
        McpServer {
            name: name.into(),
            catalog: catalog.into(),
            status,
            ..McpServer::default()
        }
    }

    #[test]
    fn the_catalog_comes_as_entries() {
        let m = with_catalog();
        let acta = m.entry("acta").unwrap();
        assert_eq!(acta.name, "Acta");
        assert!(acta.featured() && acta.connected() && acta.remote);
        assert_eq!(acta.badge.as_ref().map(|b| b.initial.as_str()), Some("A"));
        assert_eq!(
            acta.tools,
            vec![("list_tasks".into(), false), ("create_tasks".into(), true)]
        );
        let t3 = m.entry("t3").unwrap();
        assert!(t3.featured() && !t3.connected() && t3.several());
        assert_eq!(t3.fields[0].placeholder, "https://host.tailnet.ts.net");
        let playwright = m.entry("playwright").unwrap();
        assert_eq!(playwright.command, "npx -y @playwright/mcp@latest");
        assert!(!playwright.remote);
        // Acta's mark comes with it; one without has none.
        assert_eq!(acta.icon_svg.as_deref(), Some("<svg/>"));
        assert_eq!(t3.icon_svg, None);
    }

    #[test]
    fn the_icons_servers_gave_are_fetched_for_their_badges() {
        let mut m = with_catalog();
        m.take(&json!({"t": "result", "id": "app.mcp_list_1", "ok": true, "value": {"servers": [
            {"name": "t3", "url": "https://pc.ts.net/mcp", "status": "connected", "catalog": "t3",
             "badge": {"name": "t3", "initial": "T", "color": "mint", "icon": "cd34", "icon_dark": "ef56"}}
        ]}}));
        assert_eq!(
            m.mcp_servers[0].badge.as_ref().map(|b| b.icon_for(true)),
            Some("ef56")
        );
        let wanted: Vec<String> = m
            .file_fetches()
            .iter()
            .map(|f| f["file"].as_str().unwrap_or_default().to_string())
            .collect();
        for icon in ["ab12", "cd34", "ef56"] {
            assert!(wanted.contains(&icon.to_string()), "{icon} isn't fetched");
        }
    }

    #[test]
    fn an_entry_says_where_it_runs_and_how_it_signs_in() {
        let m = with_catalog();
        let words = |id: &str| {
            let e = m.entry(id).unwrap();
            (e.runs_words(), e.sign_in_words())
        };
        assert_eq!(
            words("acta"),
            ("acta.aaenz.no".into(), "With your Acta account".into())
        );
        assert_eq!(words("t3").0, "At the address you give");
        assert_eq!(words("github").1, "With a personal access token");
        assert_eq!(
            words("playwright"),
            ("On your server, with npx".into(), "No sign-in".into())
        );
        assert_eq!(words("home-assistant").1, "With a long-lived access token");
    }

    #[test]
    fn find_shows_the_two_first_as_cards_and_filters_by_pill_and_words() {
        let m = with_catalog();
        let entries = &m.spools.entries;
        // All: Acta and T3 as cards, the rest as tiles in the catalog's order.
        assert_eq!(found(entries, None, "", &[]), (vec![0, 1], vec![2, 3, 4]));
        // A pill: only its category, cards too.
        assert_eq!(found(entries, Some("Code"), "", &[]), (vec![1], vec![2, 3]));
        assert_eq!(found(entries, Some("Fun"), "", &[]), (vec![], vec![]));
        // What's typed finds a name, a line or who makes it, in any case.
        assert_eq!(found(entries, None, "git", &[]), (vec![], vec![2]));
        assert_eq!(found(entries, None, "MICROSOFT", &[]), (vec![], vec![3]));
        assert_eq!(found(entries, None, "boards", &[]), (vec![0], vec![]));
        assert_eq!(found(entries, Some("Home"), "acta", &[]), (vec![], vec![]));
    }

    /// What `catalog.search` answers for "notion": two servers from the registry, one
    /// with an icon and one loom can't connect, and the curated GitHub, as if a registry
    /// server's address were its.
    fn searched() -> Value {
        json!({"entries": [
            {"id": "registry:com.example/notes", "name": "Notes", "category": "", "description": "Notes in Notion.",
             "maker": "example.com", "badge": {"name": "notes", "initial": "N", "color": "sky"}, "icon_svg": "<svg id=\"notes\"/>",
             "server": "notes", "runs": "remote", "url": "https://{region}.notes.example.com/mcp",
             "fields": [{"name": "region", "label": "region", "default": "eu", "description": "Where it runs"}],
             "sign_in": "secret",
             "secrets": [{"name": "notes.NOTES_KEY", "label": "NOTES_KEY", "description": "Your key", "have": false},
                         {"name": "notes.NOTES_TEAM", "label": "NOTES_TEAM", "have": false, "optional": true}],
             "tools": [], "tools_checked": false, "servers": [], "connected": false,
             "community": true, "registry": "com.example/notes", "version": "1.2.0"},
            {"id": "registry:com.example/boxed", "name": "Boxed", "category": "", "description": "Notion in a box.",
             "maker": "example.com", "server": "boxed", "runs": "remote", "fields": [], "sign_in": "none", "secrets": [],
             "tools": [], "tools_checked": false, "servers": [], "connected": false,
             "community": true, "registry": "com.example/boxed", "version": "0.1.0", "unsupported": "It only runs in Docker."}
        ], "curated": ["github"]})
    }

    /// Searches for `query` and answers with `value`.
    fn search(m: &mut Model, query: &str, value: Value) {
        let ask = m.spool_search(query).unwrap();
        assert_eq!(ask["name"], "catalog.search");
        assert_eq!(ask["args"]["query"], query);
        m.take(&json!({"t": "result", "id": ask["id"], "ok": true, "value": value}));
    }

    #[test]
    fn the_registry_is_searched_from_three_letters_and_only_its_last_answer_is_taken() {
        let mut m = with_catalog();
        assert_eq!(m.spool_search("no"), None);
        let first = m.spool_search("not").unwrap();
        // The same words again don't ask again.
        assert_eq!(m.spool_search(" not "), None);
        let shown = m.spools_shown("not");
        assert_eq!(shown.line.as_deref(), Some("Searching…"));
        assert!(!shown.nothing);
        let last = m.spool_search("notion").unwrap();
        // The answer to the first, typed over since, is left.
        m.take(&json!({"t": "result", "id": first["id"], "ok": true, "value": {"entries": [], "curated": []}}));
        assert!(!m.spools.search.answered);
        m.take(&json!({"t": "result", "id": last["id"], "ok": true, "value": searched()}));
        assert!(m.spools.search.answered);
        assert_eq!(m.spools_shown("notion").line, None);
        // Fewer letters again forget it.
        assert_eq!(m.spool_search("no"), None);
        assert_eq!(m.spools.search, Search::default());
    }

    #[test]
    fn the_registrys_servers_come_after_the_catalogs_marked_community() {
        let mut m = with_catalog();
        search(&mut m, "notion", searched());
        let shown = m.spools_shown("notion");
        let ids: Vec<&str> = shown.tiles.iter().map(|e| e.id.as_str()).collect();
        // GitHub, which the registry found, though its words don't have "notion".
        assert_eq!(
            ids,
            vec![
                "github",
                "registry:com.example/notes",
                "registry:com.example/boxed"
            ]
        );
        assert!(shown.cards.is_empty() && !shown.nothing);
        let notes = m.entry("registry:com.example/notes").unwrap();
        assert!(notes.community && !notes.several());
        assert_eq!(notes.sign_in_words(), "With a key");
        assert_eq!(notes.icon_svg.as_deref(), Some("<svg id=\"notes\"/>"));
        // A field or secret labelled by its own name has what it's for as its placeholder.
        assert_eq!(notes.fields[0].placeholder, "eu");
        assert_eq!(notes.secrets[0].placeholder, "Your key");
        assert!(!m.entry("github").unwrap().community);
        // With a pill, only the catalog's: the registry's have no category.
        m.spools.pill = Some("Code".into());
        let ids: Vec<String> = m
            .spools_shown("notion")
            .tiles
            .iter()
            .map(|e| e.id.clone())
            .collect();
        assert_eq!(ids, vec!["github".to_string()]);
        // What's typed since, until it's searched, shows only the catalog's.
        m.spools.pill = None;
        assert!(m.spools_shown("notio").tiles.is_empty());
    }

    #[test]
    fn a_registry_thats_down_says_why_in_place_of_nothing_found() {
        let mut m = with_catalog();
        search(
            &mut m,
            "zzzz",
            json!({"entries": [], "curated": [], "error": "The MCP Registry couldn't be reached."}),
        );
        let shown = m.spools_shown("zzzz");
        assert_eq!(
            shown.line.as_deref(),
            Some("The MCP Registry couldn't be reached.")
        );
        assert!(!shown.nothing);
        // A search that finds nothing anywhere says so.
        search(&mut m, "qqqq", json!({"entries": [], "curated": []}));
        let shown = m.spools_shown("qqqq");
        assert!(shown.nothing && shown.line.is_none());
    }

    #[test]
    fn a_registry_server_connects_by_its_name_and_version() {
        let mut m = with_catalog();
        m.open_spools(None);
        search(&mut m, "notion", searched());
        m.spool_open("registry:com.example/notes");
        // It asks for its key; its region has a default and its team is optional.
        assert_eq!(m.spool_connect(false), None);
        assert_eq!(m.spools.flow.as_ref().unwrap().stage, Stage::Form);
        assert_eq!(
            m.spool_submit(&[("region".into(), String::new())], &[]),
            None
        );
        assert_eq!(m.spools.flow.as_ref().unwrap().said, "Fill in NOTES_KEY");
        let f = m
            .spool_submit(
                &[("region".into(), String::new())],
                &[("notes.NOTES_KEY".into(), " k1 ".into())],
            )
            .unwrap();
        assert_eq!(f["name"], "catalog.connect");
        assert_eq!(
            f["args"],
            json!({"registry": "com.example/notes", "version": "1.2.0", "fields": {}, "secrets": {"notes.NOTES_KEY": "k1"}})
        );
        // Its connect frames name it as the registry's.
        m.take(&json!({"t": "connect", "connect": {"entry": "registry:com.example/notes", "server": "notes", "step": "agents", "tools": []}}));
        assert_eq!(
            m.spools.flow.as_ref().unwrap().stage,
            Stage::Going(Step::Agents)
        );
        // Once connected, the catalog and the search are asked for again, for its servers.
        let names: Vec<String> = m
            .load_mcp()
            .iter()
            .map(|f| f["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(names.contains(&"catalog.list".to_string()));
        assert!(names.contains(&"catalog.search".to_string()));
    }

    #[test]
    fn one_loom_cant_connect_has_no_connect() {
        let mut m = with_catalog();
        search(&mut m, "notion", searched());
        m.spool_open("registry:com.example/boxed");
        assert_eq!(m.spool_connect(false), None);
        assert_eq!(m.spools.flow.as_ref().unwrap().stage, Stage::About);
        assert_eq!(
            m.entry("registry:com.example/boxed").unwrap().unsupported,
            "It only runs in Docker."
        );
    }

    #[test]
    fn the_pills_are_the_decided_ones_then_any_other() {
        let pills = pills(&["Work".into(), "Money".into()]);
        assert_eq!(
            pills,
            [
                "Work",
                "Code",
                "Mail & calendar",
                "Files",
                "Home",
                "Fun",
                "Money"
            ]
        );
    }

    #[test]
    fn connecting_one_that_asks_nothing_goes_at_once() {
        let mut m = with_catalog();
        m.spool_open("playwright");
        let f = m.spool_connect(false).unwrap();
        assert_eq!(f["name"], "catalog.connect");
        assert_eq!(f["args"], json!({"id": "playwright"}));
        assert_eq!(
            m.spools.flow.as_ref().unwrap().stage,
            Stage::Going(Step::Adding)
        );
    }

    #[test]
    fn connecting_one_that_asks_fills_its_form_first() {
        let mut m = with_catalog();
        m.spool_open("github");
        assert!(m.spool_connect(false).is_none());
        assert_eq!(m.spools.flow.as_ref().unwrap().stage, Stage::Form);
        // A secret left empty: it says which, in one line.
        assert!(m
            .spool_submit(&[], &[("GITHUB_TOKEN".into(), " ".into())])
            .is_none());
        assert_eq!(
            m.spools.flow.as_ref().unwrap().said,
            "Fill in Personal access token"
        );
        let f = m
            .spool_submit(&[], &[("GITHUB_TOKEN".into(), "github_pat_1".into())])
            .unwrap();
        assert_eq!(
            f["args"],
            json!({"id": "github", "fields": {}, "secrets": {"GITHUB_TOKEN": "github_pat_1"}})
        );
        let flow = m.spools.flow.as_ref().unwrap();
        assert_eq!(
            (flow.stage, flow.said.as_str()),
            (Stage::Going(Step::Adding), "")
        );
        // A secret the person has isn't asked for again; its address is.
        assert_eq!(
            missing(m.entry("home-assistant").unwrap(), &[], &[]),
            Some("Fill in Address".into())
        );
        assert_eq!(
            missing(
                m.entry("home-assistant").unwrap(),
                &[("address".into(), "https://home.example.com".into())],
                &[]
            ),
            None
        );
    }

    #[test]
    fn a_connect_moves_through_its_steps_as_the_instance_says() {
        let mut m = with_catalog();
        m.open_spools(Some("a1".into()));
        m.spool_open("t3");
        m.spool_connect(false);
        let f = m
            .spool_submit(&[("address".into(), "https://pc.tail1.ts.net".into())], &[])
            .unwrap();
        assert_eq!(
            f["args"]["fields"],
            json!({"address": "https://pc.tail1.ts.net"})
        );
        assert!(f["args"].get("name").is_none());
        let id = f["id"].as_str().unwrap().to_string();
        m.take(
            &json!({"t": "result", "id": id, "ok": true, "value": {"connect":
            {"entry": "t3", "server": "t3", "step": "adding", "said": "", "tools": []}}}),
        );
        let frame = |step: &str, said: &str| json!({"t": "connect", "connect": {"entry": "t3", "server": "t3", "step": step, "said": said, "tools": ["t3_thread_list"]}});
        m.take(&frame("sign_in", ""));
        assert_eq!(
            m.spools.flow.as_ref().unwrap().stage,
            Stage::Going(Step::SignIn)
        );
        // The installed list says it's being connected while it adds.
        m.take(&frame("adding", ""));
        m.mcp_servers = vec![McpServer {
            url: "https://pc.tail1.ts.net/mcp".into(),
            ..server("t3", "t3", McpStatus::Untested)
        }];
        assert_eq!(m.installed_rows()[0].line, "Connecting…");
        // At its agents step, the agent Spools was opened for is picked.
        m.take(&frame("agents", ""));
        let flow = m.spools.flow.as_ref().unwrap();
        assert_eq!(flow.stage, Stage::Going(Step::Agents));
        assert_eq!(flow.agents, vec!["a1".to_string()]);
        m.spool_pick_agent("a1");
        m.spool_pick_agent("a2");
        let give = m.spool_give().unwrap();
        assert_eq!(give["name"], "mcp.agents");
        assert_eq!(give["args"], json!({"name": "t3", "agents": ["a2"]}));
        let id = give["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "ok": true, "value": {"server": "t3", "agents": ["a2"]}}));
        assert_eq!(
            m.spools.flow.as_ref().unwrap().stage,
            Stage::Going(Step::Done)
        );
        m.take(&frame("done", ""));
        assert!(m.spools.connects.is_empty());
        assert_eq!(m.installed_rows()[0].line, "pc.tail1.ts.net");
    }

    #[test]
    fn a_connect_that_fails_says_why_and_goes_back_to_its_form() {
        let mut m = with_catalog();
        m.spool_open("github");
        m.spool_connect(false);
        let f = m
            .spool_submit(&[], &[("GITHUB_TOKEN".into(), "bad".into())])
            .unwrap();
        m.take(&json!({"t": "connect", "connect": {"entry": "github", "server": "github", "step": "failed", "said": "It answered 401."}}));
        let flow = m.spools.flow.as_ref().unwrap();
        assert_eq!(flow.stage, Stage::Going(Step::Failed));
        assert_eq!(flow.said, "It answered 401.");
        m.spool_back();
        assert_eq!(m.spools.flow.as_ref().unwrap().stage, Stage::Form);
        // One the instance refused says why too.
        m.spool_connect(false);
        let f2 = m
            .spool_submit(&[], &[("GITHUB_TOKEN".into(), "x".into())])
            .unwrap();
        assert_ne!(f["id"], f2["id"]);
        m.take(&json!({"t": "result", "id": f2["id"], "ok": false, "error": {"code": "exists", "message": "There's an MCP server named github already."}}));
        let flow = m.spools.flow.as_ref().unwrap();
        assert_eq!(flow.stage, Stage::Going(Step::Failed));
        assert_eq!(flow.said, "There's an MCP server named github already.");
    }

    #[test]
    fn a_pane_opened_on_a_connect_under_way_shows_its_step() {
        let mut m = with_catalog();
        let mut list = catalog();
        list["entries"][1]["connecting"] =
            json!({"entry": "t3", "server": "t3", "step": "sign_in", "said": "", "tools": []});
        m.take(&json!({"t": "result", "id": "app.spools_list_2", "ok": true, "value": list}));
        m.spool_open("t3");
        let flow = m.spools.flow.as_ref().unwrap();
        assert_eq!(
            (flow.stage, flow.server.as_str()),
            (Stage::Going(Step::SignIn), "t3")
        );
        // One waiting on its agents has the agent Spools was opened for picked.
        m.spools.for_agent = Some("a1".into());
        m.take(&json!({"t": "connect", "connect": {"entry": "github", "server": "github", "step": "agents", "said": "", "tools": []}}));
        m.spool_open("github");
        let flow = m.spools.flow.as_ref().unwrap();
        assert_eq!(flow.stage, Stage::Going(Step::Agents));
        assert_eq!(flow.agents, ["a1"]);
    }

    #[test]
    fn another_t3_is_named_for_its_host() {
        let m = with_catalog();
        let t3 = m.entry("t3").unwrap();
        assert_eq!(
            another_name(t3, "https://vps.tail1234.ts.net", &["t3"]),
            "t3-vps"
        );
        assert_eq!(
            another_name(t3, "https://VPS.tail1234.ts.net", &["t3", "t3-vps"]),
            "t3-vps-2"
        );
        assert_eq!(another_name(t3, "http://192.168.1.4:3773", &[]), "t3-192");
        assert_eq!(another_name(t3, "https://my_pc.ts.net", &[]), "t3-my-pc");
        assert_eq!(another_name(t3, "", &[]), "t3-2");
    }

    #[test]
    fn connect_another_asks_for_its_address_and_names_it() {
        let mut m = with_catalog();
        m.mcp_servers = vec![server("t3", "t3", McpStatus::Connected)];
        m.spool_open("t3");
        assert!(m.spool_connect(true).is_none());
        let f = m
            .spool_submit(
                &[("address".into(), "https://vps.tail1.ts.net/".into())],
                &[],
            )
            .unwrap();
        assert_eq!(f["args"]["name"], "t3-vps");
        assert_eq!(m.spools.flow.as_ref().unwrap().server, "t3-vps");
    }

    #[test]
    fn installed_lists_the_catalogs_first_in_its_order_then_the_rest() {
        let mut m = with_catalog();
        m.mcp_servers = vec![
            server("notes", "", McpStatus::Untested),
            McpServer {
                url: "https://vps.ts.net/mcp".into(),
                ..server("t3-vps", "t3", McpStatus::SignIn)
            },
            server("github", "github", McpStatus::Failing),
            McpServer {
                url: "https://pc.ts.net/mcp".into(),
                sign_in: SignIn::SignedIn("Elliot".into()),
                ..server("t3", "t3", McpStatus::Connected)
            },
            McpServer {
                sign_in: SignIn::SignedIn("Elliot".into()),
                ..server("acta", "acta", McpStatus::Connected)
            },
            server("calendar", "", McpStatus::Connected),
        ];
        m.mcp_servers[2].why = "It answered 401.".into();
        let rows = m.installed_rows();
        let names: Vec<&str> = rows
            .iter()
            .map(|r| m.mcp_servers[r.server].name.as_str())
            .collect();
        assert_eq!(
            names,
            ["acta", "t3", "t3-vps", "github", "calendar", "notes"]
        );
        let shown: Vec<(&str, &str, Option<Dot>)> = rows
            .iter()
            .map(|r| (r.title.as_str(), r.line.as_str(), r.dot))
            .collect();
        assert_eq!(
            shown,
            [
                ("Acta", "Connected as Elliot", Some(Dot::Ok)),
                ("T3 Code", "pc.ts.net", Some(Dot::Ok)),
                ("T3 Code", "vps.ts.net", Some(Dot::Warn)),
                ("GitHub", "It answered 401.", Some(Dot::Bad)),
                ("calendar", "Connected", Some(Dot::Ok)),
                ("notes", "Not tried yet", Some(Dot::Quiet)),
            ]
        );
        assert_eq!(rows[0].entry, Some(0));
        assert_eq!(rows[4].entry, None);
    }

    #[test]
    fn a_card_says_the_worst_of_how_its_servers_stand() {
        let mut m = with_catalog();
        let t3 = m.entry("t3").unwrap().clone();
        assert_eq!(card_status(&t3, &[]), None);
        let mut list = catalog();
        list["entries"][1]["servers"] = json!(["t3", "t3-vps"]);
        m.take(&json!({"t": "result", "id": "app.spools_list_2", "ok": true, "value": list}));
        let t3 = m.entry("t3").unwrap().clone();
        let mut servers = vec![
            server("t3", "t3", McpStatus::Connected),
            server("t3-vps", "t3", McpStatus::Connected),
            server("other", "", McpStatus::Failing),
        ];
        assert_eq!(
            card_status(&t3, &servers),
            Some(("Connected".into(), Dot::Ok))
        );
        // One not tried yet says so, as Installed does, and isn't taut.
        servers[1].status = McpStatus::Untested;
        assert_eq!(
            card_status(&t3, &servers),
            Some(("Not tried yet".into(), Dot::Quiet))
        );
        assert_eq!(status_words(&servers[1]), "Not tried yet");
        servers[1].status = McpStatus::SignIn;
        assert_eq!(
            card_status(&t3, &servers),
            Some(("Needs you to sign in".into(), Dot::Warn))
        );
        servers[0].status = McpStatus::Failing;
        servers[0].why = "It didn't answer.".into();
        assert_eq!(
            card_status(&t3, &servers),
            Some(("It didn't answer.".into(), Dot::Bad))
        );
    }

    #[test]
    fn a_pane_shows_a_few_of_each_kind_of_tool_until_show_all() {
        let tools = |reads: usize, changes: usize| -> Vec<(String, bool)> {
            (0..reads)
                .map(|i| (format!("r{i}"), false))
                .chain((0..changes).map(|i| (format!("c{i}"), true)))
                .collect()
        };
        // Few enough: all of them.
        assert_eq!(tools_shown(&tools(2, 3), false), [0, 1, 2, 3, 4]);
        // Many of both: four of each.
        assert_eq!(
            tools_shown(&tools(18, 14), false),
            [0, 1, 2, 3, 18, 19, 20, 21]
        );
        // Few that change: all of those, and reads to make up eight.
        assert_eq!(
            tools_shown(&tools(10, 2), false),
            [0, 1, 2, 3, 4, 5, 10, 11]
        );
        // Few that read: all of those, and changes to make up eight.
        assert_eq!(tools_shown(&tools(1, 12), false), [0, 1, 2, 3, 4, 5, 6, 7]);
        // Show all.
        assert_eq!(tools_shown(&tools(18, 14), true).len(), 32);
    }

    #[test]
    fn a_servers_pane_says_how_many_tools_it_offers() {
        let mut m = McpServer {
            tools: vec!["a".into(), "b".into()],
            ..server("acta", "acta", McpStatus::Connected)
        };
        assert_eq!(status_line(&m), "Connected · 2 tools");
        m.tools.truncate(1);
        assert_eq!(status_line(&m), "Connected · 1 tool");
        m.status = McpStatus::Failing;
        m.why = "It didn't answer.".into();
        assert_eq!(status_line(&m), "It didn't answer.");
    }

    #[test]
    fn what_can_be_done_with_a_server_follows_how_it_stands() {
        let needs = McpServer {
            sign_in: SignIn::Needed,
            ..server("t3", "t3", McpStatus::SignIn)
        };
        assert_eq!(acts(&needs), [Act::SignIn, Act::Reconnect, Act::Remove]);
        let signed = McpServer {
            sign_in: SignIn::SignedIn(String::new()),
            ..server("acta", "acta", McpStatus::Connected)
        };
        assert_eq!(
            acts(&signed),
            [Act::Reconnect, Act::Disconnect, Act::Remove]
        );
        let keyed = server("github", "github", McpStatus::Connected);
        assert_eq!(acts(&keyed), [Act::Reconnect, Act::Remove]);
        let failing = server("notes", "", McpStatus::Failing);
        assert_eq!(acts(&failing), [Act::Reconnect, Act::Remove]);
        // One not tried yet is tried, not reconnected.
        let untried = server("acta", "acta", McpStatus::Untested);
        assert_eq!(acts(&untried), [Act::Try, Act::Remove]);
    }

    #[test]
    fn trying_a_server_tests_it_and_its_pane_says_so() {
        let mut m = with_catalog();
        m.mcp_servers = vec![server("acta", "acta", McpStatus::Untested)];
        let acta = m.mcp_servers[0].clone();
        assert_eq!(
            m.server_standing(&acta),
            ("Not tried yet".into(), Dot::Quiet)
        );
        let f = m.spool_try("acta");
        assert_eq!(f["name"], "mcp.test");
        assert_eq!(f["args"]["name"], "acta");
        assert_eq!(m.server_standing(&acta), ("Trying…".into(), Dot::Quiet));
    }

    #[test]
    fn a_servers_pane_names_it_only_where_its_title_doesnt() {
        let m = with_catalog();
        let acta = m.entry("acta").cloned();
        let t3 = m.entry("t3").cloned();
        // Acta's own name repeats its title.
        let own = server("acta", "acta", McpStatus::Connected);
        assert_eq!(installed_under(&own, acta.as_ref()), "");
        // T3's has its host, and another's its name too.
        let first = McpServer {
            url: "https://pc.ts.net/mcp".into(),
            ..server("t3", "t3", McpStatus::Connected)
        };
        assert_eq!(installed_under(&first, t3.as_ref()), "pc.ts.net");
        let another = McpServer {
            url: "https://vps.ts.net/mcp".into(),
            ..server("t3-vps-2", "t3", McpStatus::Connected)
        };
        assert_eq!(
            installed_under(&another, t3.as_ref()),
            "t3-vps-2 · vps.ts.net"
        );
        // One added by hand has its host, or nothing for a program.
        let hand = McpServer {
            url: "https://mcp.example.com/sse".into(),
            ..server("calendar", "", McpStatus::Connected)
        };
        assert_eq!(installed_under(&hand, None), "mcp.example.com");
        assert_eq!(
            installed_under(&server("notes", "", McpStatus::Failing), None),
            ""
        );
    }

    #[test]
    fn an_agents_chip_toggles_at_once() {
        let mut m = with_catalog();
        m.mcp_servers = vec![McpServer {
            agents: vec!["a1".into()],
            ..server("acta", "acta", McpStatus::Connected)
        }];
        let f = m.spool_toggle_agent("acta", "a2").unwrap();
        assert_eq!(f["args"], json!({"name": "acta", "agents": ["a1", "a2"]}));
        assert_eq!(m.mcp_servers[0].agents, ["a1", "a2"]);
        let f = m.spool_toggle_agent("acta", "a1").unwrap();
        assert_eq!(f["args"]["agents"], json!(["a2"]));
        assert!(m.spool_toggle_agent("gone", "a1").is_none());
    }

    #[test]
    fn reconnect_connects_one_from_the_catalog_again_and_keeps_its_agents() {
        let mut m = with_catalog();
        m.mcp_servers = vec![
            McpServer {
                agents: vec!["a1".into()],
                ..server("t3-vps", "t3", McpStatus::Failing)
            },
            server("notes", "", McpStatus::Failing),
        ];
        let f = m.spool_reconnect("t3-vps").unwrap();
        assert_eq!(f["name"], "catalog.connect");
        assert_eq!(f["args"], json!({"id": "t3", "name": "t3-vps"}));
        assert_eq!(m.installed_rows()[0].line, "Connecting…");
        // Its agents step is answered with the agents it had.
        m.take(&json!({"t": "connect", "connect": {"entry": "t3", "server": "t3-vps", "step": "agents", "said": "", "tools": []}}));
        let sent = m.to_send.pop().unwrap();
        assert_eq!(sent["name"], "mcp.agents");
        assert_eq!(sent["args"], json!({"name": "t3-vps", "agents": ["a1"]}));
        // Only once.
        m.take(&json!({"t": "connect", "connect": {"entry": "t3", "server": "t3-vps", "step": "agents", "said": "", "tools": []}}));
        assert!(m.to_send.iter().all(|f| f["name"] != "mcp.agents"));
        // One added by hand is tried again.
        let f = m.spool_reconnect("notes").unwrap();
        assert_eq!(f["name"], "mcp.test");
    }

    #[test]
    fn remove_deletes_the_server_and_unpicks_it() {
        let mut m = with_catalog();
        m.mcp_servers = vec![server("acta", "acta", McpStatus::Connected)];
        m.spools.picked = Some("acta".into());
        let f = m.spool_remove("acta");
        assert_eq!(f["name"], "mcp.delete");
        assert_eq!(m.spools.picked, None);
    }

    #[test]
    fn an_instance_without_spools_says_why() {
        let mut m = Model::new();
        m.take(&json!({"t": "result", "id": "app.spools_list_1", "ok": false, "error": {"code": "unknown_command", "message": "No such command."}}));
        assert_eq!(m.spools.error.as_deref(), Some("No such command."));
        assert!(!m.spools.came);
    }

    #[test]
    fn spools_keeps_the_catalog_up_to_date_once_opened() {
        let mut m = Model::new();
        assert!(m.load_mcp().iter().all(|f| f["name"] != "catalog.list"));
        let asked = m.open_spools(None);
        assert!(asked.iter().any(|f| f["name"] == "catalog.list"));
        assert!(m.load_mcp().iter().any(|f| f["name"] == "catalog.list"));
    }
}
