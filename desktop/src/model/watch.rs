//! Watches in the app (PLAN.md, "Watching outside events"; protocol.md, "Watches"): a
//! watch's quiet lines in a chat, the watches an agent or a group chat follows, to see
//! and take away, and a routine that runs on an event. The instance does the watching;
//! this reads what it says and puts what the person picks into its commands.

use std::collections::HashSet;

use serde_json::{json, Value};

use super::{s, ActaCard, Message, Model, ACTA};

/// A watch's line in a chat: a `loom` message's `update` block, what happened outside
/// loom.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WatchLine {
    /// `acta` or `t3`.
    pub integration: String,
    pub watch: String,
    /// From Acta: the board, `workspace/board`, and its name, the card and its title.
    pub board: String,
    pub board_name: String,
    pub card: String,
    /// The card's title, or the thread's.
    pub title: String,
    /// moved, created, commented, assigned, other; finished for a thread.
    pub event: String,
    /// The status a card moved to.
    pub to: String,
    /// The card's status when it happened, where the line says it.
    pub status: String,
    /// What happened, in words: "moved to Done", "Thread finished".
    pub said: String,
    pub by: String,
    /// The card's page in Acta.
    pub url: String,
    /// From T3: the thread, and how its run ended.
    pub thread: String,
    pub state: String,
    /// The whole line, for what's drawn of none of the rest.
    pub text: String,
}

/// What a line's mark is, at its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineMark {
    /// The board's tile.
    Board,
    /// T3's strand, as the thread's run ended.
    Strand(Lie),
    /// From something the app doesn't know: no mark.
    None,
}

/// How T3's strand lies at a line's start: straight once the run finished, sagging in
/// the bad colour once it failed, and slack and quiet once it was stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lie {
    Straight,
    Sags,
    Slack,
}

impl WatchLine {
    pub fn of(b: &Value) -> WatchLine {
        WatchLine {
            integration: s(b, "integration"),
            watch: s(b, "watch"),
            board: s(b, "board"),
            board_name: s(b, "board_name"),
            card: s(b, "card"),
            title: s(b, "title"),
            event: s(b, "event"),
            to: s(b, "to"),
            status: s(b, "status"),
            said: s(b, "said"),
            by: s(b, "by"),
            url: s(b, "url"),
            thread: s(b, "thread"),
            state: s(b, "state"),
            text: s(b, "text"),
        }
    }

    pub fn mark(&self) -> LineMark {
        match self.integration.as_str() {
            "acta" if !self.card.is_empty() => LineMark::Board,
            "t3" => LineMark::Strand(match self.state.as_str() {
                "completed" => Lie::Straight,
                "failed" => Lie::Sags,
                _ => Lie::Slack,
            }),
            _ => LineMark::None,
        }
    }

    /// The card's pill, for a line from Acta.
    pub fn is_card(&self) -> bool {
        self.mark() == LineMark::Board
    }

    /// The thread's title, for a line from T3: its id when it has none.
    pub fn thread_title(&self) -> String {
        if self.title.trim().is_empty() {
            self.thread.clone()
        } else {
            self.title.trim().to_string()
        }
    }

    /// What happened and who, after the pill or the title, in quiet ink: "moved to Done ·
    /// Elliot", "Thread finished". A line the app draws none of the rest of is its words.
    pub fn words(&self) -> String {
        if self.mark() == LineMark::None || self.said.is_empty() {
            return self.text.clone();
        }
        if self.by.is_empty() || self.integration != "acta" {
            self.said.clone()
        } else {
            format!("{} · {}", self.said, self.by)
        }
    }

    /// What a screen reader hears of it.
    pub fn spoken(&self) -> String {
        match self.mark() {
            LineMark::Board => format!("{} {}", self.card, self.words()),
            LineMark::Strand(_) => format!("{} · {}", self.words(), self.thread_title()),
            LineMark::None => self.text.clone(),
        }
    }

    /// What its card's pill is looked up by: its page, or its id.
    pub fn card_ref(&self) -> String {
        if self.url.is_empty() {
            self.card.clone()
        } else {
            self.url.clone()
        }
    }

    /// The status the line is about, which its pill's dot shows: where a move went, or
    /// for a comment or a new card the card's status then, where the line says it. None
    /// when it doesn't, for the card's status now.
    pub fn status_then(&self) -> Option<&str> {
        let then = if self.event == "moved" {
            self.to.trim()
        } else {
            self.status.trim()
        };
        (!then.is_empty()).then_some(then)
    }

    /// Its pill's dot: the status the line is about, or else the card's now (`now`, the
    /// card as its pill shows it). A message's pill shows the card's status now.
    pub fn dot(&self, now: &ActaCard) -> super::CardInk {
        match self.status_then() {
            Some(status) => ActaCard {
                status: status.to_string(),
                ..ActaCard::default()
            }
            .ink(),
            None => now.ink(),
        }
    }

    /// Its card as its pill shows it: as it was looked up (live), or else as the line
    /// says it, with the status it moved to.
    pub fn card_shown(&self, looked: Option<&ActaCard>) -> ActaCard {
        if let Some(c) = looked.filter(|c| !c.missing) {
            return c.clone();
        }
        ActaCard {
            id: self.card.clone(),
            board: self.board.clone(),
            board_name: self.board_name.clone(),
            title: self.title.clone(),
            status: self.status_then().unwrap_or_default().to_string(),
            url: self.url.clone(),
            ..ActaCard::default()
        }
    }
}

/// A message's watch line, when it's one: a `loom` message with an `update` block.
pub fn line_of(m: &Message) -> Option<&WatchLine> {
    if m.role != "loom" {
        return None;
    }
    m.blocks.first().and_then(|b| b.update.as_deref())
}

/// One of the person's watches, as `watch.list` gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Watch {
    pub id: String,
    pub integration: String,
    pub server: String,
    pub board: String,
    pub card: String,
    pub thread: String,
    pub thread_title: String,
    /// The chat its lines go in, or "".
    pub conversation: String,
    /// What it wakes: an agent, a routine (the routine's own), or neither.
    pub wake_agent: String,
    pub wake_routine: String,
    /// Why it couldn't look, from its third failure in a row.
    pub error: String,
}

impl Watch {
    pub fn of(v: &Value) -> Watch {
        let target = v.get("target").unwrap_or(&Value::Null);
        let wake = v.get("wake").unwrap_or(&Value::Null);
        Watch {
            id: s(v, "id"),
            integration: s(v, "integration"),
            server: s(v, "server"),
            board: s(target, "board"),
            card: s(target, "card"),
            thread: s(target, "thread"),
            thread_title: s(target, "title"),
            conversation: s(v, "conversation"),
            wake_agent: s(wake, "agent"),
            wake_routine: s(wake, "routine"),
            error: s(v, "error"),
        }
    }

    /// What it's on, as its pill in Following.
    pub fn pill(&self) -> FollowPill {
        if self.integration == "t3" {
            let title = if self.thread_title.trim().is_empty() {
                self.thread.clone()
            } else {
                self.thread_title.trim().to_string()
            };
            return FollowPill::Thread(title);
        }
        if self.card.is_empty() {
            return FollowPill::Board(board_label(&self.board));
        }
        FollowPill::Card {
            id: self.card.clone(),
            link: format!("https://{ACTA}/{}#{}", self.board, self.card),
        }
    }
}

/// A board's name in a pill or a picker: its part after the workspace.
pub fn board_label(board: &str) -> String {
    board.rsplit('/').next().unwrap_or(board).to_string()
}

/// What a watch in Following is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FollowPill {
    /// A board: its name.
    Board(String),
    /// A card: its id, and its page, which its pill is looked up by.
    Card { id: String, link: String },
    /// A T3 thread: its title.
    Thread(String),
}

/// One row of Following: the watch, its pill, and why it couldn't look, in a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Followed {
    pub watch: String,
    pub pill: FollowPill,
    pub error: Option<String>,
}

impl Followed {
    fn of(w: &Watch) -> Followed {
        let error = w
            .error
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(String::from);
        Followed {
            watch: w.id.clone(),
            pill: w.pill(),
            error,
        }
    }
}

/// What Following is for: an agent's settings, or a group chat's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follower<'a> {
    Agent(&'a str),
    Room(&'a str),
}

/// The person's watches, as the instance last gave them, and what's been asked.
#[derive(Clone, Debug, Default)]
pub struct Watches {
    pub list: Vec<Watch>,
    /// `watch.list` asked for and not answered, or answered; asked again when stale.
    asked: bool,
    stale: bool,
    /// The cards looked up for lines and Following, each once.
    looked: HashSet<String>,
}

/// A routine's event, as `routine.put` takes it and `routine.list` gives it: what
/// outside loom it runs on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoutineEvent {
    /// `acta` or `t3`; empty before one is picked.
    pub integration: String,
    /// The person's MCP server it's on; empty for their Acta.
    pub server: String,
    pub board: String,
    pub card: String,
    pub thread: String,
    /// For Acta: moved, created, commented, assigned or all. For T3: finished.
    pub kind: String,
    /// For a move to one status.
    pub to: String,
}

/// Acta's statuses, as a move's `to` names them, and as the routine form's picker and
/// a routine's words say them.
pub const STATUSES: [(&str, &str); 5] = [
    ("todo", "Todo"),
    ("in_progress", "In progress"),
    ("review", "Review"),
    ("done", "Done"),
    ("canceled", "Canceled"),
];

fn status_label(key: &str) -> String {
    STATUSES
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| key.replace('_', " "), |(_, l)| l.to_string())
}

impl RoutineEvent {
    /// A routine's event, or none.
    pub fn of(v: &Value) -> Option<RoutineEvent> {
        if !v.is_object() {
            return None;
        }
        let target = v.get("target").unwrap_or(&Value::Null);
        let integration = match s(v, "integration") {
            i if i.is_empty() => "acta".to_string(),
            i => i,
        };
        Some(RoutineEvent {
            server: s(v, "server"),
            board: s(target, "board"),
            card: s(target, "card"),
            thread: s(target, "thread"),
            kind: s(v, "kind"),
            to: s(v, "to"),
            integration,
        })
    }

    /// A new event's, as "On an event" is first picked: a card on `board` (the first of
    /// the person's) moving to Done.
    pub fn new_acta(board: &str) -> RoutineEvent {
        RoutineEvent {
            integration: "acta".into(),
            board: board.into(),
            kind: "moved".into(),
            to: "done".into(),
            ..RoutineEvent::default()
        }
    }

    /// A new event on T3: the thread given finishing.
    pub fn new_t3(server: &str) -> RoutineEvent {
        RoutineEvent {
            integration: "t3".into(),
            server: server.into(),
            kind: "finished".into(),
            ..RoutineEvent::default()
        }
    }

    /// As `routine.put` takes it.
    pub fn to_json(&self) -> Value {
        let target = if self.integration == "t3" {
            json!({"thread": self.thread.trim()})
        } else if self.card.is_empty() {
            json!({"board": self.board})
        } else {
            json!({"board": self.board, "card": self.card})
        };
        let mut v = json!({"integration": self.integration, "target": target, "kind": self.kind});
        if !self.server.is_empty() {
            v["server"] = json!(self.server);
        }
        if self.kind == "moved" && !self.to.is_empty() {
            v["to"] = json!(self.to);
        }
        v
    }

    /// What's missing before it can be saved, as Save says it.
    pub fn missing(&self) -> Option<&'static str> {
        match self.integration.as_str() {
            "t3" if self.server.is_empty() => Some("Pick the T3 server it's on."),
            "t3" if self.thread.trim().is_empty() => Some("Give the thread it waits on."),
            "t3" => None,
            _ if self.board.is_empty() => Some("Pick the board it watches."),
            _ => None,
        }
    }

    /// When it runs, in words, for the routines list: "When a LOO card moves to Done".
    /// `boards` are the person's, each its prefix and `workspace/board`.
    pub fn words(&self, boards: &[(String, String)]) -> String {
        if self.integration == "t3" {
            return format!("When the thread {} finishes", self.thread.trim());
        }
        let prefix = boards
            .iter()
            .find(|(_, b)| *b == self.board)
            .map(|(p, _)| p.clone());
        let any = match &prefix {
            Some(p) => format!("a {p} card"),
            None => format!("a card on {}", board_label(&self.board)),
        };
        let subject = if self.card.is_empty() {
            any.clone()
        } else {
            self.card.clone()
        };
        match self.kind.as_str() {
            "moved" if !self.to.is_empty() => {
                format!("When {subject} moves to {}", status_label(&self.to))
            }
            "moved" => format!("When {subject} moves"),
            "created" => format!("When {any} is made"),
            "commented" => format!("When {subject} gets a comment"),
            "assigned" => format!("When {subject} is assigned"),
            _ => format!("When {subject} changes"),
        }
    }
}

/// Whether an MCP server over HTTP is the person's Acta, or a T3 server of theirs: from
/// Spools' entry for it (`catalog`), or else by its address for Acta and its name for T3.
pub fn server_is(integration: &str, catalog: &str, name: &str, url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    if !catalog.is_empty() && catalog == integration {
        return true;
    }
    match integration {
        "acta" => url.starts_with(&format!("https://{ACTA}/")),
        "t3" => name.to_lowercase().starts_with("t3"),
        _ => false,
    }
}

impl Model {
    /// What watches need asked of the instance: the person's watches, once caught up and
    /// again when they changed while the app wasn't told; and the card of each line in
    /// the open chat and each card followed, once each, for their pills.
    pub fn watch_fetches(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        if self.synced && (!self.watch.asked || self.watch.stale) {
            self.watch.asked = true;
            self.watch.stale = false;
            let id = self.id("app.watch_list");
            out.push(json!({"t": "cmd", "id": id, "name": "watch.list"}));
        }
        let mut refs: Vec<String> = self
            .messages
            .iter()
            .filter_map(line_of)
            .filter(|l| l.is_card())
            .map(WatchLine::card_ref)
            .collect();
        for w in &self.watch.list {
            if let FollowPill::Card { link, .. } = w.pill() {
                refs.push(link);
            }
        }
        for r in refs {
            if self.watch.looked.insert(r.clone()) {
                out.extend(self.preview_card(&r));
            }
        }
        out
    }

    /// Each line's card as it was looked up, on the line's message as a message's own
    /// cards are, so its pill is live and its row is drawn again once the card comes.
    pub(super) fn line_cards(&self, shown: &mut [Message]) {
        for m in shown.iter_mut() {
            let Some(line) = line_of(m).filter(|l| l.is_card()) else {
                continue;
            };
            if let Some(super::CardPreview::Ready(c)) = self.card_previews.get(&line.card_ref()) {
                let cards = std::collections::HashMap::from([(line.card.clone(), (**c).clone())]);
                m.cards = std::sync::Arc::new(cards);
            }
        }
    }

    /// Catching up brought a change to the person's watches: they're asked for again.
    pub(super) fn watches_changed(&mut self) {
        self.watch.stale = true;
    }

    /// A watch made, changed or taken away (`watch.updated`, `watch.removed`).
    pub(super) fn watch_event(&mut self, kind: &str, f: &Value) {
        match kind {
            "watch.updated" => {
                let w = Watch::of(f.get("watch").unwrap_or(&Value::Null));
                match self.watch.list.iter_mut().find(|x| x.id == w.id) {
                    Some(old) => *old = w,
                    None => self.watch.list.push(w),
                }
            }
            "watch.removed" => {
                let id = s(f, "watch");
                self.watch.list.retain(|w| w.id != id);
            }
            _ => {}
        }
    }

    /// The answers to `watch.list` and `watch.remove`. Gives whether `id` was one.
    pub(super) fn take_watch_answer(&mut self, id: &str, f: &Value) -> bool {
        if id.starts_with("app.watch_list") {
            if let Some(list) = f.pointer("/value/watches").and_then(Value::as_array) {
                self.watch.list = list.iter().map(Watch::of).collect();
            }
            return true;
        }
        if id.starts_with("app.watch_remove") {
            // Refused: it's said, and the list comes again as it is.
            if let Some(e) = f.get("error") {
                self.notices.push(s(e, "message"));
                self.watch.stale = true;
            }
            return true;
        }
        false
    }

    /// Takes watch `id` away: from Following at once, and from the instance.
    pub fn remove_watch(&mut self, id: &str) -> Value {
        self.watch.list.retain(|w| w.id != id);
        let cmd = self.id("app.watch_remove");
        json!({"t": "cmd", "id": cmd, "name": "watch.remove", "args": {"id": id}})
    }

    /// What `who` follows, for Following in its settings: an agent's watches are those
    /// that wake it or post in one of its chats, and a group chat's those that post in
    /// it. A routine's event is the routine's, on its own page.
    pub fn following(&self, who: Follower) -> Vec<Followed> {
        self.watch
            .list
            .iter()
            .filter(|w| w.wake_routine.is_empty())
            .filter(|w| match who {
                Follower::Room(id) => w.conversation == id,
                Follower::Agent(agent) => {
                    w.wake_agent == agent
                        || (self.room(&w.conversation).is_none()
                            && self
                                .conversations
                                .iter()
                                .any(|c| c.id == w.conversation && c.agent == agent))
                }
            })
            .map(Followed::of)
            .collect()
    }

    /// The person's MCP servers an event can be on, by name: their Acta, or their T3
    /// servers, as the instance last said them (`t3.servers`), or by their names until it
    /// has.
    pub fn event_servers(&self, integration: &str) -> Vec<String> {
        if integration == "t3" && !self.t3.servers.is_empty() {
            return self.t3.servers.clone();
        }
        self.mcp_servers
            .iter()
            .filter(|m| server_is(integration, &m.catalog, &m.name, &m.url))
            .map(|m| m.name.clone())
            .collect()
    }

    /// When a routine runs, in words, for its row and its page: its event's for one
    /// that runs on an event alone, or else as the instance says it.
    pub fn routine_when(&self, r: &super::Routine) -> String {
        match &r.event {
            Some(e) if r.schedule.repeat.is_empty() => e.words(&self.acta_boards),
            _ => r.when.clone(),
        }
    }

    /// What a routine with no schedule says where a next run would go: what runs it, an
    /// event or its webhook ("by webhook"), since it has no next run. None for one on a
    /// schedule.
    pub fn routine_unscheduled(&self, r: &super::Routine) -> Option<String> {
        r.schedule.repeat.is_empty().then(|| self.routine_when(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(v: Value) -> WatchLine {
        WatchLine::of(&v)
    }

    fn moved() -> WatchLine {
        block(json!({
            "kind": "update", "integration": "acta", "watch": "w_1",
            "board": "aaen-studios/loom", "board_name": "loom", "card": "LOO-206",
            "title": "Show agents' usage limits in the sidebar", "event": "moved",
            "to": "done", "said": "moved to Done", "by": "Elliot",
            "url": "https://acta.aaenz.no/aaen-studios/loom#LOO-206", "happened": 1,
            "text": "LOO-206 moved to Done · Elliot"
        }))
    }

    #[test]
    fn an_acta_line_is_its_board_its_pill_and_what_happened() {
        let l = moved();
        assert_eq!(l.mark(), LineMark::Board);
        assert!(l.is_card());
        assert_eq!(l.words(), "moved to Done · Elliot");
        assert_eq!(l.spoken(), "LOO-206 moved to Done · Elliot");
        assert_eq!(
            l.card_ref(),
            "https://acta.aaenz.no/aaen-studios/loom#LOO-206"
        );
        // Not looked up yet: the card as the line says it, done.
        let c = l.card_shown(None);
        assert_eq!((c.id.as_str(), c.status.as_str()), ("LOO-206", "done"));
        assert_eq!(c.title, "Show agents' usage limits in the sidebar");
        // Looked up: as it is now.
        let now = ActaCard {
            status: "review".into(),
            ..c.clone()
        };
        assert_eq!(l.card_shown(Some(&now)).status, "review");
        // One that couldn't be: as the line says it.
        let gone = ActaCard {
            missing: true,
            ..ActaCard::default()
        };
        assert_eq!(l.card_shown(Some(&gone)).title, c.title);
    }

    #[test]
    fn a_comment_says_who_and_has_no_status_of_its_own() {
        let l = block(json!({
            "integration": "acta", "card": "LOO-351", "title": "A", "event": "commented",
            "said": "commented", "by": "Ola", "text": "LOO-351 commented · Ola"
        }));
        assert_eq!(l.words(), "commented · Ola");
        assert_eq!(l.card_shown(None).status, "");
        // From Acta with no one named: what happened alone.
        let quiet = block(json!({"integration": "acta", "card": "LOO-1", "said": "new card"}));
        assert_eq!(quiet.words(), "new card");
    }

    #[test]
    fn a_thread_line_is_its_strand_as_the_run_ended() {
        let t3 = |state: &str, said: &str| {
            block(json!({
                "integration": "t3", "watch": "w_2", "server": "t3", "thread": "th_9",
                "title": "Fix the build", "run": "r_1", "event": "finished",
                "state": state, "said": said, "text": format!("{said} · Fix the build")
            }))
        };
        let done = t3("completed", "Thread finished");
        assert_eq!(done.mark(), LineMark::Strand(Lie::Straight));
        assert!(!done.is_card());
        assert_eq!(done.thread_title(), "Fix the build");
        assert_eq!(done.words(), "Thread finished");
        assert_eq!(done.spoken(), "Thread finished · Fix the build");
        assert_eq!(
            t3("failed", "Thread failed").mark(),
            LineMark::Strand(Lie::Sags)
        );
        assert_eq!(
            t3("interrupted", "Thread stopped").mark(),
            LineMark::Strand(Lie::Slack)
        );
        // A thread with no title is its id.
        let bare = block(json!({"integration": "t3", "thread": "th_3", "state": "completed"}));
        assert_eq!(bare.thread_title(), "th_3");
    }

    #[test]
    fn a_line_from_something_else_is_its_words() {
        let l = block(json!({"integration": "github", "said": "merged", "text": "PR 12 merged"}));
        assert_eq!(l.mark(), LineMark::None);
        assert_eq!(l.words(), "PR 12 merged");
        assert_eq!(l.spoken(), "PR 12 merged");
    }

    #[test]
    fn a_message_is_a_line_when_it_is_from_loom_with_an_update() {
        let mut m = Model::default();
        let line = m.message_of(&json!({
            "id": "m_1", "role": "loom", "at": 1,
            "blocks": [{"kind": "update", "integration": "acta", "card": "LOO-2", "said": "new card", "text": "LOO-2 new card"}]
        }));
        assert_eq!(line_of(&line).map(|l| l.card.as_str()), Some("LOO-2"));
        assert_eq!(line.blocks[0].text, "LOO-2 new card");
        let mark = m.message_of(&json!({
            "id": "m_2", "role": "loom", "at": 2, "blocks": [{"kind": "mark", "mark": "fresh"}]
        }));
        assert!(line_of(&mark).is_none());
        let reply = m.message_of(&json!({
            "id": "m_3", "role": "agent", "at": 3, "blocks": [{"kind": "text", "text": "Hi"}]
        }));
        assert!(line_of(&reply).is_none());
        // Lines one after another stack tight; the reply after them doesn't join them.
        m.messages = vec![line.clone(), line.clone(), reply];
        let rows: Vec<Option<&Message>> = m.messages.iter().map(Some).collect();
        let flow = super::super::flow(&rows, |_| 0);
        assert!(!flow[0].joined.above && flow[0].joined.below);
        assert!(flow[1].joined.above && !flow[1].joined.below);
        assert!(!flow[2].joined.above);
        assert!(flow[2].head);
    }

    fn watches() -> Value {
        json!({"value": {"watches": [
            {"id": "w_1", "integration": "acta", "server": "acta", "target": {"board": "aaen-studios/loom"}, "conversation": "c_1", "wake": null},
            {"id": "w_2", "integration": "acta", "server": "acta", "target": {"board": "aaen-studios/loom", "card": "LOO-206"}, "conversation": "", "wake": {"agent": "a_1"}, "error": "Acta didn't answer.\nTried 3 times."},
            {"id": "w_3", "integration": "t3", "server": "t3", "target": {"thread": "th_9", "title": "Fix the build"}, "conversation": "c_room", "wake": null},
            {"id": "w_4", "integration": "acta", "server": "acta", "target": {"board": "aaen-studios/loom"}, "conversation": "", "wake": {"routine": "rt_1"}},
            {"id": "w_5", "integration": "acta", "server": "acta", "target": {"board": "aaen-studios/clx"}, "conversation": "c_2", "wake": null}
        ]}})
    }

    fn chat(id: &str, agent: &str) -> super::super::Conversation {
        super::super::Conversation {
            id: id.into(),
            agent: agent.into(),
            task: None,
            side: false,
            branch_of: None,
            branch_at: None,
            routine: None,
            program: None,
            asked_by: None,
            main: false,
            made_by: None,
            title: String::new(),
        }
    }

    fn with_watches() -> Model {
        let mut m = Model {
            conversations: vec![chat("c_1", "a_1"), chat("c_2", "a_2")],
            rooms: vec![super::super::Room {
                id: "c_room".into(),
                ..Default::default()
            }],
            ..Model::default()
        };
        assert!(m.take_watch_answer("app.watch_list_1", &watches()));
        m
    }

    #[test]
    fn following_lists_what_an_agent_or_a_group_chat_follows() {
        let m = with_watches();
        let mine = m.following(Follower::Agent("a_1"));
        let ids: Vec<&str> = mine.iter().map(|f| f.watch.as_str()).collect();
        // Posting in its chat, or waking it; never the routine's.
        assert_eq!(ids, ["w_1", "w_2"]);
        assert_eq!(mine[0].pill, FollowPill::Board("loom".into()));
        assert_eq!(
            mine[1].pill,
            FollowPill::Card {
                id: "LOO-206".into(),
                link: "https://acta.aaenz.no/aaen-studios/loom#LOO-206".into()
            }
        );
        // Why it couldn't look, in one line.
        assert_eq!(mine[1].error.as_deref(), Some("Acta didn't answer."));
        assert_eq!(mine[0].error, None);
        let room = m.following(Follower::Room("c_room"));
        assert_eq!(room.len(), 1);
        assert_eq!(room[0].pill, FollowPill::Thread("Fix the build".into()));
        // An agent with none follows nothing.
        assert!(m.following(Follower::Agent("a_9")).is_empty());
    }

    #[test]
    fn watches_follow_their_events_and_go_at_once_when_taken_away() {
        let mut m = with_watches();
        m.take(&json!({"t": "event", "kind": "watch.updated", "watch": {"id": "w_1", "integration": "acta", "target": {"board": "aaen-studios/loom"}, "conversation": "c_1", "error": "Signed out."}}));
        assert_eq!(
            m.following(Follower::Agent("a_1"))[0].error.as_deref(),
            Some("Signed out.")
        );
        m.take(&json!({"t": "event", "kind": "watch.updated", "watch": {"id": "w_6", "integration": "acta", "target": {"board": "aaen-studios/loom", "card": "LOO-9"}, "conversation": "c_1"}}));
        assert_eq!(m.following(Follower::Agent("a_1")).len(), 3);
        m.take(&json!({"t": "event", "kind": "watch.removed", "watch": "w_6"}));
        assert_eq!(m.following(Follower::Agent("a_1")).len(), 2);
        let f = m.remove_watch("w_2");
        assert_eq!(f["name"], "watch.remove");
        assert_eq!(f["args"]["id"], "w_2");
        assert_eq!(m.following(Follower::Agent("a_1")).len(), 1);
        // Refused: said, and asked for again.
        let id = f["id"].as_str().unwrap_or_default().to_string();
        m.synced = true;
        m.watch.asked = true;
        assert!(m.take_watch_answer(
            &id,
            &json!({"error": {"code": "bad_request", "message": "A routine's."}})
        ));
        assert_eq!(m.notices.last().map(String::as_str), Some("A routine's."));
        let asked = m.watch_fetches();
        assert!(asked.iter().any(|f| f["name"] == "watch.list"));
    }

    #[test]
    fn watches_are_asked_for_once_caught_up_and_cards_once_each() {
        let mut m = Model::default();
        assert!(m.watch_fetches().is_empty());
        m.synced = true;
        let first = m.watch_fetches();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0]["name"], "watch.list");
        assert!(m.watch_fetches().is_empty());
        // The chat's lines and the cards followed: each card looked up once.
        let line = m.message_of(&json!({"id": "m_1", "role": "loom", "at": 1, "blocks": [
            {"kind": "update", "integration": "acta", "card": "LOO-2", "board": "aaen-studios/loom", "url": "https://acta.aaenz.no/aaen-studios/loom#LOO-2", "said": "new card"}
        ]}));
        m.messages = vec![line.clone(), line];
        assert!(m.take_watch_answer("app.watch_list_2", &watches()));
        let asked = m.watch_fetches();
        let refs: Vec<&str> = asked
            .iter()
            .filter(|f| f["name"] == "card.preview")
            .filter_map(|f| f["args"]["ref"].as_str())
            .collect();
        assert_eq!(
            refs,
            [
                "https://acta.aaenz.no/aaen-studios/loom#LOO-2",
                "https://acta.aaenz.no/aaen-studios/loom#LOO-206"
            ]
        );
        assert!(m.watch_fetches().is_empty());
        // Catching up held a change to them: asked for again.
        m.watches_changed();
        assert_eq!(m.watch_fetches()[0]["name"], "watch.list");
    }

    #[test]
    fn a_lines_card_shows_as_it_was_looked_up() {
        let mut m = Model::default();
        let url = "https://acta.aaenz.no/aaen-studios/loom#LOO-2";
        let line = m.message_of(&json!({"id": "m_1", "role": "loom", "at": 1, "blocks": [
            {"kind": "update", "integration": "acta", "card": "LOO-2", "board": "aaen-studios/loom", "url": url, "event": "commented", "said": "commented"}
        ]}));
        let mut shown = vec![line.clone()];
        m.line_cards(&mut shown);
        assert!(shown[0].cards.is_empty());
        m.card_previews.insert(
            url.into(),
            super::super::CardPreview::Ready(Box::new(ActaCard {
                id: "LOO-2".into(),
                title: "Two".into(),
                status: "review".into(),
                ..ActaCard::default()
            })),
        );
        let mut shown = vec![line];
        m.line_cards(&mut shown);
        let l = line_of(&shown[0]).cloned().unwrap_or_default();
        assert_eq!(l.card_shown(shown[0].cards.get("LOO-2")).status, "review");
    }

    #[test]
    fn a_lines_dot_is_the_status_it_is_about() {
        use super::super::CardInk;
        let done_now = ActaCard {
            id: "LOO-206".into(),
            status: "done".into(),
            ..ActaCard::default()
        };
        // A move's is where it went, though the card has gone on since.
        let to_review = block(
            json!({"integration": "acta", "card": "LOO-206", "event": "moved", "to": "review"}),
        );
        assert_eq!(to_review.status_then(), Some("review"));
        assert_eq!(to_review.dot(&done_now), CardInk::Gold);
        assert_eq!(moved().dot(&ActaCard::default()), CardInk::Ok);
        // A comment's or a new card's is its status then, where the line says it.
        let commented = block(
            json!({"integration": "acta", "card": "LOO-206", "event": "commented", "status": "in_progress"}),
        );
        assert_eq!(commented.dot(&done_now), CardInk::Sky);
        let made = block(
            json!({"integration": "acta", "card": "LOO-207", "event": "created", "status": "todo"}),
        );
        assert_eq!(made.dot(&done_now), CardInk::Quiet);
        // And its status now where it doesn't.
        let said_nothing =
            block(json!({"integration": "acta", "card": "LOO-206", "event": "commented"}));
        assert_eq!(said_nothing.status_then(), None);
        assert_eq!(said_nothing.dot(&done_now), CardInk::Ok);
        // The card the pill opens on is the card now: its preview says Done.
        assert_eq!(to_review.card_shown(Some(&done_now)).status, "done");
        // Until it's looked up, the pill is the card as the line says it.
        assert_eq!(to_review.card_shown(None).status, "review");
        assert_eq!(commented.card_shown(None).status, "in_progress");
        assert_eq!(said_nothing.card_shown(None).status, "");
    }

    fn boards() -> Vec<(String, String)> {
        vec![
            ("LOO".into(), "aaen-studios/loom".into()),
            ("CLX".into(), "aaen-studios/clx".into()),
        ]
    }

    #[test]
    fn a_routines_event_says_when_it_runs() {
        let mut e = RoutineEvent::new_acta("aaen-studios/loom");
        assert_eq!(e.words(&boards()), "When a LOO card moves to Done");
        e.to = "in_progress".into();
        assert_eq!(e.words(&boards()), "When a LOO card moves to In progress");
        e.kind = "created".into();
        assert_eq!(e.words(&boards()), "When a LOO card is made");
        e.kind = "commented".into();
        assert_eq!(e.words(&boards()), "When a LOO card gets a comment");
        e.card = "LOO-206".into();
        assert_eq!(e.words(&boards()), "When LOO-206 gets a comment");
        e.kind = "all".into();
        assert_eq!(e.words(&boards()), "When LOO-206 changes");
        // A board whose prefix isn't known: by its name.
        let other = RoutineEvent::new_acta("aaen-studios/calcite");
        assert_eq!(
            other.words(&boards()),
            "When a card on calcite moves to Done"
        );
        let mut t3 = RoutineEvent::new_t3("t3");
        t3.thread = " th_9 ".into();
        assert_eq!(t3.words(&boards()), "When the thread th_9 finishes");
    }

    #[test]
    fn a_routines_event_goes_as_routine_put_takes_it_and_comes_back_the_same() {
        let e = RoutineEvent::new_acta("aaen-studios/loom");
        let v = e.to_json();
        assert_eq!(
            v,
            json!({"integration": "acta", "target": {"board": "aaen-studios/loom"}, "kind": "moved", "to": "done"})
        );
        // As the instance keeps it, with its server.
        let kept = json!({"integration": "acta", "server": "acta", "target": {"board": "aaen-studios/loom"}, "kind": "moved", "to": "done"});
        let back = RoutineEvent::of(&kept).unwrap_or_default();
        assert_eq!(
            (back.board.as_str(), back.server.as_str()),
            ("aaen-studios/loom", "acta")
        );
        assert_eq!(back.to_json(), kept);
        // A new card has no status; a comment on one card names it.
        let made = RoutineEvent {
            kind: "created".into(),
            ..e.clone()
        };
        assert_eq!(made.to_json().get("to"), None);
        let card = RoutineEvent {
            card: "LOO-206".into(),
            kind: "commented".into(),
            ..e.clone()
        };
        assert_eq!(
            card.to_json()["target"],
            json!({"board": "aaen-studios/loom", "card": "LOO-206"})
        );
        let mut t3 = RoutineEvent::new_t3("t3-pc");
        assert_eq!(t3.missing(), Some("Give the thread it waits on."));
        t3.thread = "th_9".into();
        assert_eq!(t3.missing(), None);
        assert_eq!(
            t3.to_json(),
            json!({"integration": "t3", "server": "t3-pc", "target": {"thread": "th_9"}, "kind": "finished"})
        );
        assert_eq!(
            RoutineEvent::new_acta("").missing(),
            Some("Pick the board it watches.")
        );
        assert_eq!(RoutineEvent::of(&Value::Null), None);
    }

    #[test]
    fn an_event_is_on_the_persons_acta_or_their_t3_servers() {
        assert!(server_is(
            "acta",
            "",
            "acta",
            "https://acta.aaenz.no/api/mcp"
        ));
        assert!(!server_is("acta", "", "acta", ""));
        assert!(server_is("t3", "", "t3", "https://pc.tail1234.ts.net/mcp"));
        assert!(server_is(
            "t3",
            "",
            "T3-vps",
            "https://vps.tail1234.ts.net/mcp"
        ));
        assert!(!server_is(
            "t3",
            "",
            "github",
            "https://api.githubcopilot.com/mcp/"
        ));
        assert!(!server_is("t3", "", "t3", ""));
        // From Spools' entry, under a name of the person's own.
        assert!(server_is(
            "t3",
            "t3",
            "studio",
            "https://pc.tail1234.ts.net/mcp"
        ));
        assert!(server_is(
            "acta",
            "acta",
            "work",
            "https://acta.example.com/api/mcp"
        ));
        assert!(!server_is(
            "t3",
            "acta",
            "work",
            "https://acta.example.com/api/mcp"
        ));
    }

    #[test]
    fn t3s_servers_are_the_instances_once_it_said_them() {
        let mut m = Model {
            mcp_servers: vec![crate::model::McpServer::of(
                &json!({"name": "t3", "url": "https://pc.tail1234.ts.net/mcp"}),
            )],
            ..Model::default()
        };
        // By its name, until the instance says which.
        assert_eq!(m.event_servers("t3"), vec!["t3".to_string()]);
        // Spools' T3 entry, under a name of its own.
        m.t3.servers = vec!["t3-code".into(), "studio".into()];
        assert_eq!(
            m.event_servers("t3"),
            vec!["t3-code".to_string(), "studio".to_string()]
        );
        assert!(m.event_servers("acta").is_empty());
    }

    #[test]
    fn a_routine_on_an_event_alone_says_its_event() {
        let m = Model {
            acta_boards: boards(),
            ..Model::default()
        };
        let r = super::super::Routine::of(&json!({
            "id": "rt_1", "name": "Shipped", "agent": "a_1", "prompt": "Note it.",
            "schedule": null, "when": "when a card on aaen-studios/loom moves to Done",
            "event": {"integration": "acta", "server": "acta", "target": {"board": "aaen-studios/loom"}, "kind": "moved", "to": "done"}
        }));
        assert_eq!(m.routine_when(&r), "When a LOO card moves to Done");
        // With a schedule too: as the instance says it.
        let both = super::super::Routine::of(&json!({
            "id": "rt_2", "schedule": {"every": 60}, "when": "every hour, and when a card on aaen-studios/loom moves to Done",
            "event": {"integration": "acta", "target": {"board": "aaen-studios/loom"}, "kind": "moved", "to": "done"}
        }));
        assert_eq!(m.routine_when(&both), both.when);
    }

    #[test]
    fn a_routine_with_no_schedule_says_what_runs_it() {
        let m = Model {
            acta_boards: boards(),
            ..Model::default()
        };
        // Run by its webhook alone, it has no next run, and isn't paused.
        let hook = super::super::Routine::of(&json!({
            "id": "rt_1", "name": "Version check", "agent": "a_1", "prompt": "x",
            "schedule": null, "hook": "h_1", "when": "by webhook", "on": true, "next": 0
        }));
        assert_eq!(m.routine_unscheduled(&hook).as_deref(), Some("by webhook"));
        let event = super::super::Routine::of(&json!({
            "id": "rt_2", "schedule": null, "when": "when a card on aaen-studios/loom moves to Done",
            "event": {"integration": "acta", "target": {"board": "aaen-studios/loom"}, "kind": "moved", "to": "done"}
        }));
        assert_eq!(
            m.routine_unscheduled(&event).as_deref(),
            Some("When a LOO card moves to Done")
        );
        // On a schedule, its next run says when.
        let timed = super::super::Routine::of(&json!({
            "id": "rt_3", "schedule": {"every": 60}, "when": "every hour", "on": true
        }));
        assert_eq!(m.routine_unscheduled(&timed), None);
    }
}
