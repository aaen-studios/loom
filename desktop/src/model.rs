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
    /// Replies being written, by message id: their blocks so far.
    live: HashMap<String, Vec<Block>>,
    last_delta: HashMap<String, Instant>,
    failed: HashMap<String, Instant>,
    finished: HashMap<String, Instant>,
    queued: Vec<(String, Option<String>, Option<String>, String)>, // cid, agent, conversation, text
    dirty: HashSet<List>,
    next_id: u64,
    /// The id of the agent the app just asked the instance to make.
    pub made_agent: Option<String>,
    pub notices: Vec<String>,
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
            "core.data" => self.data(f),
            "core.queued" => {
                self.queued.push((
                    s(f, "cid"),
                    f.get("agent").and_then(Value::as_str).map(str::to_string),
                    f.get("conversation")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    s(f, "text"),
                ));
                self.place_queued();
            }
            "ack" => {
                let cid = s(f, "cid");
                self.queued.retain(|q| q.0 != cid);
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
            "agent" => {
                self.dirty.insert(List::Agents);
            }
            "conversation" => {
                self.dirty.insert(List::Conversations);
                self.dirty.insert(List::Messages);
            }
            "message" => {
                let message = f.get("message").cloned().unwrap_or(Value::Null);
                let id = s(&message, "id");
                self.live.remove(&id);
                if Some(s(f, "conversation")) == self.conversation() {
                    self.dirty.insert(List::Messages);
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
            _ => {}
        }
    }

    fn delta(&mut self, f: &Value) {
        let message = s(f, "message");
        let block = f.get("block").and_then(Value::as_u64).unwrap_or(0) as usize;
        let blocks = self.live.entry(message.clone()).or_default();
        while blocks.len() <= block {
            blocks.push(Block {
                kind: s(f, "kind"),
                text: String::new(),
                call: None,
            });
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
        if id.starts_with("app.acp") {
            self.programs = f
                .pointer("/value/programs")
                .and_then(Value::as_array)
                .map(|ps| ps.iter().map(|p| s(p, "name")).collect())
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
            List::Agents => {
                self.agents = items
                    .iter()
                    .map(|a| Agent {
                        id: s(a, "id"),
                        name: s(a, "name"),
                        color: s(a, "color"),
                        program: s(a, "acp_agent"),
                        permission: s(a, "permission"),
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
                self.queued = items
                    .iter()
                    .map(|q| {
                        (
                            s(q, "cid"),
                            q.get("agent").and_then(Value::as_str).map(str::to_string),
                            q.get("conversation")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            s(q, "text"),
                        )
                    })
                    .collect();
                self.place_queued();
            }
            List::Messages => {
                self.more_messages = f.get("more").and_then(Value::as_bool).unwrap_or(false);
                self.messages = items
                    .iter()
                    .map(|m| Message {
                        id: s(m, "id"),
                        role: s(m, "role"),
                        at: m.get("at").and_then(Value::as_i64).unwrap_or(0),
                        blocks: blocks_of(m),
                        live: false,
                        queued: false,
                    })
                    .collect();
                self.place_queued();
            }
        }
    }

    /// The queued messages for the shown chat go after its stored ones.
    fn place_queued(&mut self) {
        self.messages.retain(|m| !m.queued);
        let conversation = self.conversation();
        let agent = self.selected.clone();
        for (cid, q_agent, q_conversation, text) in &self.queued {
            let here = match q_conversation {
                Some(c) => Some(c) == conversation.as_ref(),
                None => q_agent.is_some() && q_agent == &agent,
            };
            if here {
                self.messages.push(Message {
                    id: cid.clone(),
                    role: "person".into(),
                    at: i64::MAX,
                    blocks: vec![Block {
                        kind: "text".into(),
                        text: text.clone(),
                        call: None,
                    }],
                    live: false,
                    queued: true,
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
                    blocks.push(Block {
                        kind: "tool".into(),
                        text: String::new(),
                        call: Some(t.call.clone()),
                    });
                }
            }
            out.push(Message {
                id: r.message.clone(),
                role: "agent".into(),
                at: i64::MAX,
                blocks,
                live: true,
                queued: false,
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

    pub fn send(&mut self, text: &str) -> Option<Value> {
        let agent = self.selected.clone()?;
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        Some(json!({"t": "send", "agent": agent, "text": text}))
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
