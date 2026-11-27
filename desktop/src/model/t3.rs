//! T3 Code's threads in the chat (PLAN.md, "T3 threads"): a thread a message names, or
//! one an agent's T3 tool call started, as the instance keeps it on the message
//! (`threads`), and what its card shows (LOO-435): a band across its top with where it
//! is in words, its run, its steps and its time; T3's mark and its title; where it runs,
//! its project, branch, model and effort; what it's doing in plain words, what it asks,
//! its summary or why it failed; its pull requests; and its buttons. And
//! the form that starts a thread. The instance looks threads up, follows them and
//! answers them; this reads what it kept and says what to send.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use super::{s, Message, Model};

/// How long a thread's id can sit before the start of its link in a message's words
/// for the two to be read together: `t3-thread://v2/ENV/` comes first.
const LINK: &str = "t3-thread://v2/";

/// Where a thread is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ThreadState {
    #[default]
    Working,
    /// A question or an approval waits on the person.
    Waiting,
    Done,
    Failed,
}

/// A pull request the thread opened: its number, title, state (`open`, `merged`,
/// `closed`) and checks (`success`, `failure`, `pending`), as T3 gives them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pr {
    pub number: i64,
    pub url: String,
    pub title: String,
    pub state: String,
    pub checks: String,
}

impl Pr {
    /// How it stands, in a word or two: its checks while it's open, else its state.
    pub fn standing(&self) -> String {
        match (self.state.as_str(), self.checks.as_str()) {
            ("merged", _) => "Merged".into(),
            ("closed", _) => "Closed".into(),
            (_, "success") => "✓ Checks pass".into(),
            (_, "failure") => "✕ Checks fail".into(),
            (_, "pending") => "● Checks running".into(),
            ("open", _) => "Open".into(),
            _ => String::new(),
        }
    }
}

/// What a thread waits on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    /// `question` or `approval`.
    pub kind: String,
    pub request: String,
    pub text: String,
    pub choices: Vec<String>,
    /// An approval loom couldn't answer: the person answers it in T3.
    pub in_t3: bool,
}

/// A thread as a message keeps it (protocol.md, "T3 threads").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThreadCard {
    pub thread: String,
    /// The T3 server it's on, the person's spool (`t3-vps`).
    pub server: String,
    pub title: String,
    pub project: String,
    pub branch: String,
    pub model: String,
    pub effort: String,
    pub state: ThreadState,
    /// Its latest step as it came, and in plain words while it works.
    pub step: String,
    pub doing: String,
    /// Its last few steps in plain words, the newest last, and how many it took.
    pub steps: Vec<String>,
    pub step_count: i64,
    /// Which run it's on, as T3 counts them, or 0.
    pub run: i64,
    /// Its last message, once it's done.
    pub summary: String,
    /// When its latest run started and ended, in Unix ms, or 0.
    pub started: i64,
    pub ended: i64,
    pub prs: Vec<Pr>,
    pub ask: Option<Ask>,
    pub reason: String,
    /// Its page in T3's app.
    pub url: String,
    /// None of the person's T3 servers has it.
    pub missing: bool,
}

/// What a button on a card does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Answers its question with this choice.
    Answer(String),
    /// Answers its approval: `accept` or `decline`.
    Decide(&'static str),
    /// Opens its page in T3's app.
    Open,
    /// Asks it to try again.
    Retry,
    /// Sends it a message the person wrote on the card.
    Send(String),
    /// Stops it.
    Stop,
}

/// A button on a card, the main one filled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub act: Act,
    pub main: bool,
}

/// The band across a card's top: where it is, in words, its run and steps, and its
/// time; `ink` fills it while the thread waits on the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    pub state: &'static str,
    pub run: String,
    pub time: String,
    pub ink: bool,
}

/// What a card says under its head: what it's doing in plain words with the step as it
/// came, what it asks, its summary, or why it failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Doing { words: String, raw: String },
    Asks(String),
    Summary(String),
    Failed(String),
}

/// The icon buttons at a card's foot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Open,
    Send,
    Stop,
}

impl ThreadCard {
    pub fn of(v: &Value) -> ThreadCard {
        let state = match s(v, "state").as_str() {
            "waiting" => ThreadState::Waiting,
            "done" => ThreadState::Done,
            "failed" => ThreadState::Failed,
            _ => ThreadState::Working,
        };
        let ask = v.get("ask").filter(|a| a.is_object()).map(|a| Ask {
            kind: s(a, "kind"),
            request: s(a, "request"),
            text: s(a, "text"),
            choices: strings(a.get("choices")),
            in_t3: a.get("in_t3").and_then(Value::as_bool).unwrap_or(false),
        });
        let int = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        ThreadCard {
            thread: s(v, "thread"),
            server: s(v, "server"),
            title: s(v, "title"),
            project: s(v, "project"),
            branch: s(v, "branch"),
            model: s(v, "model"),
            effort: s(v, "effort"),
            state,
            step: s(v, "step"),
            doing: s(v, "doing"),
            steps: strings(v.get("steps")),
            step_count: int("step_count"),
            run: int("run"),
            summary: s(v, "summary"),
            started: int("started"),
            ended: int("ended"),
            prs: v
                .get("prs")
                .and_then(Value::as_array)
                .map(|ps| {
                    ps.iter()
                        .map(|p| Pr {
                            number: p.get("number").and_then(Value::as_i64).unwrap_or(0),
                            url: s(p, "url"),
                            title: s(p, "title"),
                            state: s(p, "state"),
                            checks: s(p, "checks"),
                        })
                        .filter(|p| p.number > 0)
                        .collect()
                })
                .unwrap_or_default(),
            ask: if state == ThreadState::Waiting {
                ask
            } else {
                None
            },
            reason: s(v, "reason"),
            url: s(v, "url"),
            missing: v.get("missing").and_then(Value::as_bool).unwrap_or(false),
        }
    }

    /// Its band: "Working", "Waiting on you", "Done" or "Failed", in words and in its
    /// shape, so it never rests on colour alone; "Run 2 · 18 steps"; how long it has
    /// run, or how long ago it ended; and filled with ink while it waits on the person.
    pub fn band(&self, now: i64) -> Band {
        let state = match self.state {
            ThreadState::Working => "Working",
            ThreadState::Waiting => "Waiting on you",
            ThreadState::Done => "Done",
            ThreadState::Failed => "Failed",
        };
        let mut run = Vec::new();
        if self.run > 0 {
            run.push(format!("Run {}", self.run));
        }
        match self.step_count {
            0 => {}
            1 => run.push("1 step".into()),
            n => run.push(format!("{n} steps")),
        }
        let time = match self.state {
            ThreadState::Working | ThreadState::Waiting if self.started > 0 => {
                took(now - self.started)
            }
            ThreadState::Done | ThreadState::Failed if self.ended > 0 => ago(now - self.ended),
            _ => String::new(),
        };
        Band {
            state,
            run: run.join(" · "),
            time,
            ink: self.state == ThreadState::Waiting,
        }
    }

    /// The quiet line under its title: where it runs (its spool), its project, branch,
    /// and model · effort.
    pub fn meta(&self) -> Vec<String> {
        let model = [self.model.as_str(), self.effort.as_str()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        [&self.server, &self.project, &self.branch, &model]
            .into_iter()
            .filter(|p| !p.is_empty())
            .cloned()
            .collect()
    }

    /// What it says under its head, as where it is has it.
    pub fn body(&self) -> Option<Body> {
        match self.state {
            ThreadState::Working => {
                let words = if self.doing.is_empty() {
                    self.step.clone()
                } else {
                    self.doing.clone()
                };
                let raw = if self.step == words {
                    String::new()
                } else {
                    self.step.clone()
                };
                (!words.is_empty()).then_some(Body::Doing { words, raw })
            }
            ThreadState::Waiting => self
                .ask
                .as_ref()
                .map(|a| a.text.clone())
                .filter(|t| !t.is_empty())
                .map(Body::Asks),
            ThreadState::Done => {
                (!self.summary.is_empty()).then(|| Body::Summary(self.summary.clone()))
            }
            ThreadState::Failed => Some(Body::Failed(if self.reason.is_empty() {
                "Failed".into()
            } else {
                self.reason.clone()
            })),
        }
    }

    /// The buttons that answer it, the main one first: a question's choices, an
    /// approval's Approve and Decline, Open where the person approves in T3 itself, Try
    /// again once it failed.
    pub fn buttons(&self) -> Vec<Button> {
        let b = |label: &str, act: Act, main: bool| Button {
            label: label.into(),
            act,
            main,
        };
        match (&self.state, &self.ask) {
            (ThreadState::Waiting, Some(a)) if a.kind == "question" => a
                .choices
                .iter()
                .enumerate()
                .map(|(i, c)| b(c, Act::Answer(c.clone()), i == 0))
                .collect(),
            (ThreadState::Waiting, Some(a)) if a.in_t3 || a.request.is_empty() => {
                vec![b("Open in T3", Act::Open, true)]
            }
            (ThreadState::Waiting, Some(_)) => vec![
                b("Approve", Act::Decide("accept"), true),
                b("Decline", Act::Decide("decline"), false),
            ],
            (ThreadState::Failed, _) => vec![b("Try again", Act::Retry, true)],
            _ => Vec::new(),
        }
    }

    /// The words beside Open, for an approval the person answers in T3.
    pub fn in_t3(&self) -> bool {
        self.ask
            .as_ref()
            .is_some_and(|a| a.kind == "approval" && (a.in_t3 || a.request.is_empty()))
    }

    /// Its icon buttons: Open in T3 where it has a page, Send a message while it isn't
    /// failed, and Stop while it works or waits.
    pub fn tools(&self) -> Vec<Tool> {
        let mut out = Vec::new();
        if !self.url.is_empty() {
            out.push(Tool::Open);
        }
        if self.state != ThreadState::Failed {
            out.push(Tool::Send);
        }
        if self.moving() {
            out.push(Tool::Stop);
        }
        out
    }

    /// Whether it's working or waiting, so its time counts up.
    pub fn moving(&self) -> bool {
        matches!(self.state, ThreadState::Working | ThreadState::Waiting)
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|c| {
            c.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// How long ago something was, as a card's band says it: "just now", "3 min ago", "2 h
/// ago", "4 d ago".
pub fn ago(ms: i64) -> String {
    let mins = (ms / 60_000).max(0);
    match mins {
        0 => "just now".into(),
        1..=59 => format!("{mins} min ago"),
        60..=1439 => format!("{} h ago", mins / 60),
        _ => format!("{} d ago", mins / 1440),
    }
}

/// How long a run took, as a card says it: "45 s", "18 m", "1 h 5 m".
pub fn took(ms: i64) -> String {
    let secs = (ms / 1000).max(0);
    if secs < 60 {
        return format!("{secs} s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins} m");
    }
    let (h, m) = (mins / 60, mins % 60);
    if m == 0 {
        format!("{h} h")
    } else {
        format!("{h} h {m} m")
    }
}

/// A message's threads, in the order its words link them, then the rest by when they
/// started.
pub fn threads_of(m: &Value) -> Arc<Vec<ThreadCard>> {
    let Some(o) = m.get("threads").and_then(Value::as_object) else {
        return Arc::default();
    };
    let words: String = m
        .get("blocks")
        .and_then(Value::as_array)
        .map(|bs| {
            bs.iter()
                .filter(|b| b.get("kind").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let mut cards: Vec<ThreadCard> = o
        .values()
        .map(ThreadCard::of)
        .filter(|c| !c.missing)
        .collect();
    let place = |c: &ThreadCard| {
        words
            .find(&format!("/{}", c.thread))
            .filter(|at| words[..*at].rfind(LINK).is_some())
            .unwrap_or(usize::MAX)
    };
    cards.sort_by_key(|c| (place(c), c.started));
    Arc::new(cards)
}

/// The id of the thread a link in a message's words points at, `t3-thread://v2/ENV/ID`.
pub fn link_thread(url: &str) -> Option<&str> {
    let rest = url.strip_prefix(LINK)?;
    let (_, id) = rest.split_once('/')?;
    (!id.is_empty()).then_some(id)
}

/// A model, as the start form offers it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelChoice {
    pub id: String,
    pub name: String,
    pub efforts: Vec<String>,
    /// The effort it starts on.
    pub effort: String,
}

/// A provider T3 runs, with its models.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub models: Vec<ModelChoice>,
}

/// The start form in the right panel (PLAN.md, "T3 threads"): Server when the person
/// has several, Project, Model, Effort and Task, and Start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StartForm {
    pub servers: Vec<String>,
    pub server: String,
    /// Each project's id and title.
    pub projects: Vec<(String, String)>,
    pub project: String,
    pub providers: Vec<Provider>,
    pub provider: String,
    pub model: String,
    pub effort: String,
    /// Waiting for t3.form's answer.
    pub loading: bool,
    /// Waiting for t3.start's answer.
    pub starting: bool,
    /// Why it couldn't load or start, or empty.
    pub error: String,
}

/// An effort as the form's segments say it.
pub fn effort_label(e: &str) -> String {
    match e {
        "low" => "Low".into(),
        "medium" => "Medium".into(),
        "high" => "High".into(),
        "xhigh" => "Extra high".into(),
        "max" => "Max".into(),
        other => other.to_string(),
    }
}

impl StartForm {
    /// What t3.form answered: the servers, the projects of the one picked and the
    /// models. What the person picked stays where it still can.
    pub fn take(&mut self, v: &Value) {
        let list = |k: &str| {
            v.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        self.servers = list("servers")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        self.server = s(v, "server");
        self.projects = list("projects")
            .iter()
            .map(|p| (s(p, "id"), s(p, "title")))
            .filter(|(id, _)| !id.is_empty())
            .collect();
        self.providers = list("providers")
            .iter()
            .map(|p| Provider {
                id: s(p, "id"),
                name: s(p, "name"),
                models: p
                    .get("models")
                    .and_then(Value::as_array)
                    .map(|ms| {
                        ms.iter()
                            .map(|m| ModelChoice {
                                id: s(m, "id"),
                                name: s(m, "name"),
                                efforts: m
                                    .get("efforts")
                                    .and_then(Value::as_array)
                                    .map(|es| {
                                        es.iter()
                                            .filter_map(Value::as_str)
                                            .map(str::to_string)
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                effort: s(m, "effort"),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        if !self.projects.iter().any(|(id, _)| *id == self.project) {
            self.project = self
                .projects
                .first()
                .map(|(id, _)| id.clone())
                .unwrap_or_default();
        }
        if self.chosen().is_none() {
            let first = self
                .providers
                .iter()
                .find_map(|p| p.models.first().map(|m| (p.id.clone(), m.id.clone())));
            if let Some((p, m)) = first {
                self.pick_model(&p, &m);
            }
        }
        self.loading = false;
    }

    /// The model picked, and its provider.
    pub fn chosen(&self) -> Option<(&Provider, &ModelChoice)> {
        let p = self.providers.iter().find(|p| p.id == self.provider)?;
        let m = p.models.iter().find(|m| m.id == self.model)?;
        Some((p, m))
    }

    /// Picks a model: its effort stays when the model has it, or else it's the
    /// model's own.
    pub fn pick_model(&mut self, provider: &str, model: &str) {
        self.provider = provider.into();
        self.model = model.into();
        let (efforts, own) = match self.chosen() {
            Some((_, m)) => (m.efforts.clone(), m.effort.clone()),
            None => (Vec::new(), String::new()),
        };
        if !efforts.contains(&self.effort) {
            self.effort = if efforts.contains(&own) {
                own
            } else {
                efforts.first().cloned().unwrap_or_default()
            };
        }
    }

    /// The efforts the model picked has, each its id and its label: none for one with
    /// none, and then Effort isn't shown.
    pub fn efforts(&self) -> Vec<(String, String)> {
        self.chosen()
            .map(|(_, m)| {
                m.efforts
                    .iter()
                    .map(|e| (e.clone(), effort_label(e)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every model, as the Model menu lists them: its provider, its id and how it's
    /// shown, `Opus 5.5` with Claude's, `GPT-6 Luna · Codex` with the others.
    pub fn model_list(&self) -> Vec<(String, String, String)> {
        let several = self.providers.len() > 1;
        let mut out = Vec::new();
        for p in &self.providers {
            for m in &p.models {
                let label = if several {
                    format!("{} · {}", m.name, p.name)
                } else {
                    m.name.clone()
                };
                out.push((p.id.clone(), m.id.clone(), label));
            }
        }
        out
    }

    /// How the picked model is shown on its field.
    pub fn model_label(&self) -> String {
        let list = self.model_list();
        list.into_iter()
            .find(|(p, m, _)| *p == self.provider && *m == self.model)
            .map(|(_, _, l)| l)
            .unwrap_or_default()
    }

    /// How the picked project is shown on its field.
    pub fn project_label(&self) -> String {
        self.projects
            .iter()
            .find(|(id, _)| *id == self.project)
            .map(|(_, t)| t.clone())
            .unwrap_or_default()
    }

    /// Server shows only when the person has several T3 servers.
    pub fn shows_server(&self) -> bool {
        self.servers.len() > 1
    }

    /// Whether Start can go: a project, a model and a task, and nothing on its way.
    pub fn can_start(&self, task: &str) -> bool {
        !self.loading
            && !self.starting
            && !self.project.is_empty()
            && self.chosen().is_some()
            && !task.trim().is_empty()
    }
}

/// What the T3 cards keep in the model.
#[derive(Debug, Default)]
pub struct T3 {
    /// The messages last said to be shown (`t3.view`), once said since the app caught up.
    view_sent: Option<Vec<String>>,
    /// Answers on their way, by thread: which button was pressed.
    pub busy: HashMap<String, String>,
    /// The start form, while the panel is open.
    pub form: Option<StartForm>,
    /// The person's T3 servers, as the + menu last asked: with none, it has no "Start a
    /// T3 thread".
    pub servers: Vec<String>,
    /// t3.answer and t3.retry asked, by the command's id: the thread.
    asked: HashMap<String, String>,
}

impl Model {
    /// The frames the T3 cards need (PLAN.md, "T3 threads"): which messages of the shown
    /// chat have threads, so the instance follows those still going, said again when
    /// they change and after the app catches up.
    pub fn t3_fetches(&mut self) -> Vec<Value> {
        if !self.synced {
            self.t3.view_sent = None;
            return Vec::new();
        }
        // The newest 50 messages with threads, newest first.
        let shown: Vec<String> = self
            .messages
            .iter()
            .rev()
            .filter(|m| !m.threads.is_empty())
            .take(50)
            .map(|m| m.id.clone())
            .collect();
        if self.t3.view_sent.as_ref() == Some(&shown)
            || (self.t3.view_sent.is_none() && shown.is_empty())
        {
            return Vec::new();
        }
        self.t3.view_sent = Some(shown.clone());
        let id = self.id("app.t3_view");
        vec![json!({"t": "cmd", "id": id, "name": "t3.view", "args": {"messages": shown}})]
    }

    /// Asks which T3 servers the person has, for the + menu.
    pub fn ask_t3_servers(&mut self) -> Value {
        let id = self.id("app.t3_servers");
        json!({"t": "cmd", "id": id, "name": "t3.servers"})
    }

    /// What a card's button sends: the answer to a question or an approval, or Try
    /// again. None for Open, which the app opens itself, and while an answer to the
    /// thread is on its way.
    pub fn t3_act(&mut self, card: &ThreadCard, act: &Act, label: &str) -> Option<Value> {
        if self.t3.busy.contains_key(&card.thread) {
            return None;
        }
        let (name, args) = match act {
            Act::Answer(choice) => (
                "t3.answer",
                json!({"thread": card.thread, "answer": choice}),
            ),
            Act::Decide(d) => ("t3.answer", json!({"thread": card.thread, "decision": d})),
            Act::Retry => ("t3.retry", json!({"thread": card.thread})),
            Act::Send(text) if !text.trim().is_empty() => (
                "t3.send",
                json!({"thread": card.thread, "text": text.trim()}),
            ),
            Act::Stop => ("t3.stop", json!({"thread": card.thread})),
            Act::Open | Act::Send(_) => return None,
        };
        let id = self.id("app.t3_act");
        self.t3.busy.insert(card.thread.clone(), label.into());
        self.t3.asked.insert(id.clone(), card.thread.clone());
        Some(json!({"t": "cmd", "id": id, "name": name, "args": args}))
    }

    /// Opens the start form, and asks for what it offers: the projects of `server`, or
    /// of the first of the person's T3 servers.
    pub fn open_t3_form(&mut self, server: Option<&str>) -> Value {
        let mut form = self.t3.form.take().unwrap_or_default();
        form.loading = true;
        form.error.clear();
        self.t3.form = Some(form);
        let id = self.id("app.t3_form");
        let args = match server {
            Some(s) => json!({"server": s}),
            None => json!({}),
        };
        json!({"t": "cmd", "id": id, "name": "t3.form", "args": args})
    }

    /// Starts the thread the form describes, with `task`, in the chat shown.
    pub fn t3_start(&mut self, task: &str) -> Option<Value> {
        let conversation = self.conversation().unwrap_or_default();
        let form = self.t3.form.as_mut()?;
        if !form.can_start(task) {
            return None;
        }
        form.starting = true;
        form.error.clear();
        let args = json!({
            "server": form.server,
            "project": form.project,
            "provider": form.provider,
            "model": form.model,
            "effort": form.effort,
            "task": task.trim(),
            "conversation": conversation,
        });
        let id = self.id("app.t3_start");
        Some(json!({"t": "cmd", "id": id, "name": "t3.start", "args": args}))
    }

    /// The answers to what the T3 cards and the form asked. Gives whether `id` was one.
    pub(super) fn take_t3_answer(&mut self, id: &str, f: &Value) -> bool {
        let refused = f.get("error").map(|e| s(e, "message"));
        if let Some(thread) = self.t3.asked.remove(id) {
            self.t3.busy.remove(&thread);
            if let Some(why) = refused {
                self.notices.push(why);
            }
            return true;
        }
        if id.starts_with("app.t3_form") {
            if let Some(form) = self.t3.form.as_mut() {
                match (&refused, f.get("value")) {
                    (None, Some(v)) => form.take(v),
                    _ => {
                        form.loading = false;
                        form.error = refused.unwrap_or_else(|| "T3 didn't answer.".into());
                    }
                }
            }
            return true;
        }
        if id.starts_with("app.t3_start") {
            match refused {
                // Its card lands in the chat: the form's done.
                None => self.t3.form = None,
                Some(why) => {
                    if let Some(form) = self.t3.form.as_mut() {
                        form.starting = false;
                        form.error = why;
                    }
                }
            }
            return true;
        }
        if id.starts_with("app.t3_servers") {
            if refused.is_none() {
                self.t3.servers = f
                    .pointer("/value/servers")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            return true;
        }
        id.starts_with("app.t3_view")
    }
}

/// Whether a message is the line from loom a thread the person started lands in, once
/// its card has come: the card shows in the line's place.
pub fn is_start_line(m: &Message) -> bool {
    m.role == "loom" && !m.threads.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(v: Value) -> ThreadCard {
        ThreadCard::of(&v)
    }

    #[test]
    fn a_card_reads_its_state() {
        let working = card(
            json!({"thread": "mcp:1", "server": "t3-vps", "title": "Fix the flaky sign-in test", "project": "loom", "branch": "loo-434-t3", "model": "Opus 5.5", "effort": "high", "state": "working", "step": "Read server/catalog.calx", "doing": "Checking how Spools matches servers to the catalog", "steps": ["Read a file", "Ran a command"], "step_count": 18, "run": 2, "started": 1_000, "url": "https://pc/env/mcp:1", "prs": [{"number": 353, "url": "u", "title": "Spools knows T3", "state": "open", "checks": "pending"}], "at": 1}),
        );
        assert_eq!(
            working.band(241_000),
            Band {
                state: "Working",
                run: "Run 2 · 18 steps".into(),
                time: "4 m".into(),
                ink: false
            }
        );
        assert_eq!(
            working.meta(),
            vec!["t3-vps", "loom", "loo-434-t3", "Opus 5.5 · high"]
        );
        assert_eq!(
            working.body(),
            Some(Body::Doing {
                words: "Checking how Spools matches servers to the catalog".into(),
                raw: "Read server/catalog.calx".into()
            })
        );
        assert_eq!(working.steps, vec!["Read a file", "Ran a command"]);
        assert_eq!(working.prs[0].standing(), "● Checks running");
        assert_eq!(working.prs[0].title, "Spools knows T3");
        assert!(working.buttons().is_empty() && working.moving());
        assert_eq!(working.tools(), vec![Tool::Open, Tool::Send, Tool::Stop]);
        // Without plain words, the step as it came, once.
        let plain = card(json!({"state": "working", "step": "git push"}));
        assert_eq!(
            plain.body(),
            Some(Body::Doing {
                words: "git push".into(),
                raw: String::new()
            })
        );
        assert_eq!(plain.band(0).run, "");

        let done = card(
            json!({"thread": "mcp:1", "title": "T", "project": "loom", "model": "Opus 5.5", "state": "done", "summary": "Opened #353.", "step_count": 1, "started": 1_000, "ended": 1_081_000, "prs": [{"number": 353, "url": "https://github.com/a/b/pull/353", "state": "open", "checks": "success"}]}),
        );
        let band = done.band(1_081_000 + 180_000);
        assert_eq!(
            (band.state, band.run.as_str(), band.time.as_str()),
            ("Done", "1 step", "3 min ago")
        );
        assert_eq!(done.body(), Some(Body::Summary("Opened #353.".into())));
        assert_eq!(done.meta(), vec!["loom", "Opus 5.5"]);
        assert_eq!(done.prs[0].standing(), "✓ Checks pass");
        assert_eq!(done.tools(), vec![Tool::Send]);

        let failed = card(
            json!({"thread": "mcp:1", "state": "failed", "reason": "Usage limit reached · resets 15:30", "started": 1, "ended": 60_001, "url": "u"}),
        );
        assert_eq!(failed.band(60_001).state, "Failed");
        assert_eq!(failed.band(60_001).time, "just now");
        assert_eq!(
            failed.body(),
            Some(Body::Failed("Usage limit reached · resets 15:30".into()))
        );
        assert_eq!(
            failed.buttons(),
            vec![Button {
                label: "Try again".into(),
                act: Act::Retry,
                main: true
            }]
        );
        assert!(!failed.moving());
        assert_eq!(failed.tools(), vec![Tool::Open]);
    }

    #[test]
    fn a_waiting_card_offers_its_answers() {
        let q = card(
            json!({"thread": "mcp:1", "state": "waiting", "started": 0, "ask": {"kind": "question", "request": "r1", "text": "Keep the retry, or make the test wait for the port instead?", "choices": ["Wait for the port", "Keep the retry"]}}),
        );
        let band = q.band(0);
        assert!(band.ink && band.state == "Waiting on you");
        assert_eq!(
            q.body(),
            Some(Body::Asks(
                "Keep the retry, or make the test wait for the port instead?".into()
            ))
        );
        let b = q.buttons();
        assert_eq!(b.len(), 2);
        assert!(b[0].main && !b[1].main);
        assert_eq!(b[0].act, Act::Answer("Wait for the port".into()));
        assert!(!q.in_t3());
        assert_eq!(q.tools(), vec![Tool::Send, Tool::Stop]);

        let a = card(
            json!({"thread": "mcp:1", "state": "waiting", "ask": {"kind": "approval", "request": "ap1", "text": "Run rm -rf build", "choices": ["Approve", "Decline"]}}),
        );
        assert_eq!(
            a.buttons()
                .iter()
                .map(|b| (b.label.as_str(), b.main))
                .collect::<Vec<_>>(),
            vec![("Approve", true), ("Decline", false)]
        );
        assert_eq!(a.buttons()[0].act, Act::Decide("accept"));

        // One loom couldn't answer: Open in T3, to approve it there.
        let in_t3 = card(
            json!({"thread": "mcp:1", "state": "waiting", "ask": {"kind": "approval", "request": "ap1", "text": "Run rm -rf build", "in_t3": true}}),
        );
        assert!(in_t3.in_t3());
        assert_eq!(
            in_t3.buttons(),
            vec![Button {
                label: "Open in T3".into(),
                act: Act::Open,
                main: true
            }]
        );
        // An ask on a card that isn't waiting is left out.
        assert_eq!(
            card(json!({"state": "done", "ask": {"kind": "question"}})).ask,
            None
        );
    }

    #[test]
    fn says_how_long() {
        assert_eq!(took(45_000), "45 s");
        assert_eq!(took(60_000), "1 m");
        assert_eq!(took(3_600_000), "1 h");
        assert_eq!(took(3_900_000), "1 h 5 m");
        assert_eq!(took(-5), "0 s");
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(180_000), "3 min ago");
        assert_eq!(ago(7_200_000), "2 h ago");
        assert_eq!(ago(3 * 86_400_000), "3 d ago");
    }
    #[test]
    fn a_messages_threads_come_in_the_order_its_words_link_them() {
        let m = json!({"blocks": [{"kind": "text", "text": "First [b](t3-thread://v2/env/mcp:b), then [a](t3-thread://v2/env/mcp:a)."}],
            "threads": {"mcp:a": {"thread": "mcp:a", "state": "done"}, "mcp:b": {"thread": "mcp:b", "state": "working"}, "mcp:c": {"thread": "mcp:c", "state": "working", "started": 5}, "mcp:d": {"thread": "mcp:d", "missing": true}}});
        let ids: Vec<String> = threads_of(&m).iter().map(|c| c.thread.clone()).collect();
        assert_eq!(ids, vec!["mcp:b", "mcp:a", "mcp:c"]);
        assert!(threads_of(&json!({})).is_empty());
        assert_eq!(link_thread("t3-thread://v2/env/mcp:a"), Some("mcp:a"));
        assert_eq!(link_thread("https://x/y"), None);
    }

    fn form_answer() -> Value {
        json!({"servers": ["t3-pc", "t3-vps"], "server": "t3-pc", "projects": [{"id": "p1", "title": "loom"}, {"id": "p2", "title": "calcite"}],
            "providers": [{"id": "claudeAgent", "name": "Claude", "models": [{"id": "claude-opus-5-5", "name": "Opus 5.5", "efforts": ["low", "medium", "high", "xhigh", "max"], "effort": "high"}, {"id": "claude-haiku-5-5", "name": "Haiku 5.5", "efforts": [], "effort": ""}]},
                          {"id": "codex", "name": "Codex", "models": [{"id": "gpt-6-luna", "name": "GPT-6 Luna", "efforts": ["low", "medium", "high", "xhigh"], "effort": "medium"}]}]})
    }

    #[test]
    fn the_start_form_picks_and_keeps() {
        let mut f = StartForm::default();
        f.take(&form_answer());
        assert!(f.shows_server());
        assert_eq!(
            (
                f.project.as_str(),
                f.provider.as_str(),
                f.model.as_str(),
                f.effort.as_str()
            ),
            ("p1", "claudeAgent", "claude-opus-5-5", "high")
        );
        assert_eq!(f.efforts().len(), 5);
        assert_eq!(
            f.efforts()[3],
            ("xhigh".to_string(), "Extra high".to_string())
        );
        assert_eq!(f.model_label(), "Opus 5.5 · Claude");
        assert_eq!(f.project_label(), "loom");
        // An effort the next model has stays; one it hasn't becomes its own.
        f.effort = "xhigh".into();
        f.pick_model("codex", "gpt-6-luna");
        assert_eq!(f.effort, "xhigh");
        f.effort = "max".into();
        f.pick_model("claudeAgent", "claude-opus-5-5");
        assert_eq!(f.effort, "max");
        f.pick_model("codex", "gpt-6-luna");
        assert_eq!(f.effort, "medium");
        f.pick_model("claudeAgent", "claude-haiku-5-5");
        assert!(f.effort.is_empty() && f.efforts().is_empty());
        // What was picked stays when the form's asked again.
        f.project = "p2".into();
        f.take(&form_answer());
        assert_eq!(
            (f.project.as_str(), f.model.as_str()),
            ("p2", "claude-haiku-5-5")
        );
        assert!(!f.can_start("  ") && f.can_start("Tidy the CI logs."));
        f.starting = true;
        assert!(!f.can_start("Tidy the CI logs."));
        let mut one = StartForm::default();
        let mut answer = form_answer();
        answer["servers"] = json!(["t3"]);
        one.take(&answer);
        assert!(!one.shows_server());
    }

    #[test]
    fn the_model_sends_what_the_cards_and_the_form_ask() {
        let mut m = Model::new();
        let q = card(
            json!({"thread": "mcp:1", "state": "waiting", "ask": {"kind": "question", "request": "r1", "text": "Q", "choices": ["A", "B"]}}),
        );
        let sent = m.t3_act(&q, &Act::Answer("A".into()), "A").unwrap();
        assert_eq!(sent["name"], "t3.answer");
        assert_eq!(sent["args"], json!({"thread": "mcp:1", "answer": "A"}));
        // Not again while it's on its way.
        assert!(m.t3_act(&q, &Act::Answer("B".into()), "B").is_none());
        assert_eq!(m.t3.busy.get("mcp:1").map(String::as_str), Some("A"));
        let id = sent["id"].as_str().unwrap().to_string();
        assert!(m.take_t3_answer(&id, &json!({"ok": true, "value": {}})));
        assert!(m.t3.busy.is_empty());
        let d = m.t3_act(&q, &Act::Decide("accept"), "Approve").unwrap();
        assert_eq!(d["args"], json!({"thread": "mcp:1", "decision": "accept"}));
        let id = d["id"].as_str().unwrap().to_string();
        assert!(m.take_t3_answer(&id, &json!({"ok": false, "error": {"code": "unavailable", "message": "T3 didn't take the answer"}})));
        assert_eq!(m.notices, vec!["T3 didn't take the answer".to_string()]);
        assert!(m.t3_act(&q, &Act::Open, "Open").is_none());
        let retry = m.t3_act(&q, &Act::Retry, "Try again").unwrap();
        assert_eq!(retry["name"], "t3.retry");
        assert!(m.take_t3_answer(retry["id"].as_str().unwrap(), &json!({"ok": true})));
        // A message written on the card, and Stop; an empty message sends nothing.
        assert!(m.t3_act(&q, &Act::Send("  ".into()), "Send").is_none());
        let sent = m
            .t3_act(&q, &Act::Send(" Use the port. ".into()), "Send")
            .unwrap();
        assert_eq!(sent["name"], "t3.send");
        assert_eq!(
            sent["args"],
            json!({"thread": "mcp:1", "text": "Use the port."})
        );
        assert!(m.take_t3_answer(sent["id"].as_str().unwrap(), &json!({"ok": true})));
        let stop = m.t3_act(&q, &Act::Stop, "Stop").unwrap();
        assert_eq!(stop["name"], "t3.stop");
        assert!(m.take_t3_answer(stop["id"].as_str().unwrap(), &json!({"ok": true})));

        // The form.
        let ask = m.open_t3_form(None);
        assert_eq!(ask["name"], "t3.form");
        assert!(m.t3.form.as_ref().unwrap().loading);
        let id = ask["id"].as_str().unwrap().to_string();
        assert!(m.take_t3_answer(&id, &json!({"ok": true, "value": form_answer()})));
        let start = m.t3_start("Tidy the CI logs.\n").unwrap();
        assert_eq!(start["name"], "t3.start");
        assert_eq!(start["args"]["task"], "Tidy the CI logs.");
        assert_eq!(start["args"]["effort"], "high");
        assert!(m.t3_start("again").is_none());
        let id = start["id"].as_str().unwrap().to_string();
        assert!(m.take_t3_answer(
            &id,
            &json!({"ok": false, "error": {"code": "bad_request", "message": "Pick a project."}})
        ));
        assert_eq!(m.t3.form.as_ref().unwrap().error, "Pick a project.");
        let again = m.t3_start("Tidy the CI logs.").unwrap();
        let id = again["id"].as_str().unwrap().to_string();
        assert!(m.take_t3_answer(&id, &json!({"ok": true, "value": {"thread": "mcp:new1"}})));
        assert!(m.t3.form.is_none());

        // The + menu has "Start a T3 thread" once the person has a T3 server.
        use super::super::PlusRow;
        assert!(!m
            .plus_groups("", false)
            .concat()
            .contains(&PlusRow::StartT3));
        let ask = m.ask_t3_servers();
        assert_eq!(ask["name"], "t3.servers");
        let id = ask["id"].as_str().unwrap().to_string();
        assert!(m.take_t3_answer(&id, &json!({"ok": true, "value": {"servers": ["t3"]}})));
        assert_eq!(m.t3.servers, vec!["t3".to_string()]);
        assert_eq!(m.plus_groups("", false)[1].last(), Some(&PlusRow::StartT3));
        assert_eq!(m.plus_groups("t3 thr", false), vec![vec![PlusRow::StartT3]]);
    }

    #[test]
    fn the_shown_chat_says_which_messages_have_threads() {
        let mut m = Model::new();
        assert!(m.t3_fetches().is_empty());
        m.synced = true;
        assert!(m.t3_fetches().is_empty(), "nothing to say with no threads");
        let with = |m: &Model, id: &str| {
            m.message_of(&json!({"id": id, "role": "agent", "blocks": [], "threads": {"mcp:1": {"thread": "mcp:1", "state": "working", "url": "https://pc/env/mcp:1"}}}))
        };
        let a = with(&m, "m_1");
        m.messages.push(a);
        let sent = m.t3_fetches();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["name"], "t3.view");
        assert_eq!(sent[0]["args"]["messages"], json!(["m_1"]));
        assert!(m.t3_fetches().is_empty(), "said once");
        // The chat changes: said again, and none once it has no threads.
        m.messages.clear();
        assert_eq!(m.t3_fetches()[0]["args"]["messages"], json!([]));
        // After a reconnect it's said again.
        let b = with(&m, "m_2");
        m.messages.push(b);
        m.synced = false;
        assert!(m.t3_fetches().is_empty());
        m.synced = true;
        assert_eq!(m.t3_fetches()[0]["args"]["messages"], json!(["m_2"]));
    }
}
