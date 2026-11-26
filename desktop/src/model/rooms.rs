//! Group chats (PLAN.md, "Group chats"): conversations several of the person's agents
//! take part in, where the instance picks one to answer each message. The model shows
//! them as the instance has them and sends what the person changes; the picking is the
//! instance's.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{s, List, Model};

/// How long a room.create from the form waits for its answer before Make turns on
/// again.
pub const MAKE_WAIT: Duration = Duration::from_secs(15);

/// A room.create sent from here, until its answer comes or can't.
#[derive(Clone, Debug, PartialEq)]
pub struct Making {
    /// Its command id.
    id: String,
    /// When it went, while Make waits on it; None once it has waited MAKE_WAIT. Make is
    /// on again then, and an answer that comes later still shows the group chat.
    since: Option<Instant>,
}

/// A group chat, as its conversation's `room` has it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Room {
    /// Its conversation.
    pub id: String,
    pub name: String,
    /// Its agents, by id, in the order the person picked them.
    pub members: Vec<String>,
    /// A thread colour, or "" for none.
    pub color: String,
    /// The member a message goes to when the router can't pick, or "" for the one
    /// that answered last.
    pub default: String,
    /// Everyone's: every one of the person's agents, made by the instance.
    pub everyone: bool,
    /// What the person re-sent to another member, newest last.
    pub corrections: Vec<Correction>,
}

/// A message the person re-sent to another member, which the router follows for
/// messages like it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Correction {
    pub id: String,
    /// The start of the person's message.
    pub text: String,
    /// The member it should have gone to, and the one the router picked.
    pub to: String,
    pub from: String,
    pub at: i64,
}

/// How a reply in a group chat came to its agent: the line over it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Routed {
    pub agent: String,
    /// router, mention, handoff, person or default.
    pub by: String,
    /// The line's words after the agent's name: the router's why, or what loom says.
    pub said: String,
    /// For a handoff, the agent that handed it on.
    pub from: String,
}

/// What a group chat used (`room.summary`): its members' turns there and the router's
/// picks for it, in tokens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomUsage {
    /// Its conversation.
    pub room: String,
    pub tokens_today: i64,
    pub tokens_month: i64,
    pub router_today: i64,
    pub router_month: i64,
}

/// The panel's form for a group chat being made: what the person picked so far. Its
/// name is the panel's name field.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomForm {
    /// Making a new one, rather than changing the open one.
    pub making: bool,
    pub members: Vec<String>,
    pub color: String,
}

/// What the panel for group chats shows: the form while one is made, never the open
/// group chat's values, or the open one's settings after.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomPanel {
    /// The group chat it changes, or None while one is made.
    pub room: Option<Room>,
    /// The members row: there while one is made and for every group chat but
    /// Everyone, whose members are all of the person's agents.
    pub members_row: bool,
    pub members: Vec<String>,
    pub color: String,
    /// What the one being made still needs before Make turns on, or None once it can
    /// be made.
    pub needs: Option<MakeNeeds>,
    /// The count after the Members label while one is made, or None for the open
    /// one's settings.
    pub members_count: Option<MembersCount>,
    /// A room.create from the form is waiting on the instance: Make does nothing, and
    /// looks as it does while off.
    pub waiting: bool,
}

/// The count after Members while a group chat is made: how many are picked of the two
/// it needs ("1 of 2"), then how many once it has them ("2 picked").
#[derive(Clone, Debug, PartialEq)]
pub struct MembersCount {
    pub text: String,
    /// Two picked at least: drawn in the ok colour, and in ink-3 until then.
    pub met: bool,
}

/// The fewest members a group chat has: the instance says so too.
const MIN_MEMBERS: usize = 2;

impl MembersCount {
    fn of(picked: usize) -> MembersCount {
        let met = picked >= MIN_MEMBERS;
        MembersCount {
            text: if met {
                format!("{picked} picked")
            } else {
                format!("{picked} of {MIN_MEMBERS}")
            },
            met,
        }
    }
}

/// What a group chat being made still needs before Make turns on, the first in the
/// form's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MakeNeeds {
    /// A name.
    Name,
    /// Two members at least.
    Members,
}

impl RoomPanel {
    /// Making a new group chat, rather than changing the open one.
    pub fn making(&self) -> bool {
        self.room.is_none()
    }

    /// Make is on: a name and two members at least, and no room.create waiting.
    pub fn can_make(&self) -> bool {
        self.making() && self.needs.is_none() && !self.waiting
    }
}

impl Room {
    pub fn of(id: &str, r: &Value) -> Room {
        Room {
            id: id.to_string(),
            name: s(r, "name"),
            members: r
                .get("members")
                .and_then(Value::as_array)
                .map(|ms| {
                    ms.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            color: s(r, "color"),
            default: s(r, "default"),
            everyone: r.get("everyone").and_then(Value::as_bool).unwrap_or(false),
            corrections: r
                .get("corrections")
                .and_then(Value::as_array)
                .map(|cs| {
                    cs.iter()
                        .map(|c| Correction {
                            id: s(c, "id"),
                            text: s(c, "text"),
                            to: s(c, "to"),
                            from: s(c, "from"),
                            at: c.get("at").and_then(Value::as_i64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

impl Routed {
    /// A message's or a run's `routed`, with the agent that answered.
    pub fn of(v: &Value) -> Option<Routed> {
        let r = v.get("routed").filter(|r| r.is_object())?;
        Some(Routed {
            agent: s(v, "agent"),
            by: s(r, "by"),
            said: s(r, "said"),
            from: s(r, "from"),
        })
    }
}

impl Model {
    /// The group chats, as the conversations list has them.
    pub(super) fn take_rooms(&mut self, items: &[Value]) {
        self.rooms = items
            .iter()
            .filter_map(|c| {
                let r = c.get("room").filter(|r| r.is_object())?;
                Some(Room::of(&s(c, "id"), r))
            })
            .collect();
        if let Some(open) = &self.open_room {
            if !self.rooms.iter().any(|r| &r.id == open) && self.conversations_known {
                self.open_room = None;
                self.dirty.insert(List::Messages);
            }
        }
    }

    pub fn room(&self, id: &str) -> Option<&Room> {
        self.rooms.iter().find(|r| r.id == id)
    }

    /// The group chat the chat shows, if it shows one.
    pub fn room_open(&self) -> Option<&Room> {
        self.room(self.open_room.as_deref()?)
    }

    /// Shows a group chat. Its default member, or else its first, is the agent the
    /// window's agent parts are about meanwhile: its browser, its files for the @
    /// picker.
    pub fn show_room(&mut self, id: &str) {
        let Some(room) = self.room(id).cloned() else {
            return;
        };
        if self.open_room.as_deref() == Some(id) {
            return;
        }
        let lead = Some(room.default.clone())
            .filter(|d| self.agent(d).is_some())
            .or_else(|| {
                room.members
                    .iter()
                    .find(|m| self.agent(m).is_some())
                    .cloned()
            });
        // The lead is selected only so the screen's agent parts are about it. Its own chat
        // isn't read, so it keeps its dot and where its new replies start (LOO-371).
        let kept = lead.as_ref().map(|a| {
            (
                a.clone(),
                self.unread.contains(a),
                self.first_unread.get(a).cloned(),
            )
        });
        self.select_keeping(lead);
        if let Some((a, unread, first)) = kept {
            if unread {
                self.unread.insert(a.clone());
            }
            if let Some(first) = first {
                self.first_unread.insert(a, first);
            }
        }
        self.unread.remove(id);
        let at = self.conversation_last.get(id).copied().unwrap_or(0);
        self.mark_read(id, at);
        self.open_room = Some(id.to_string());
        self.open_task = None;
        self.open_branch = None;
        self.messages.clear();
        self.messages_for = None;
        self.first_page();
        self.replying = None;
        self.new_line = None;
        self.dirty.insert(List::Messages);
        self.queue_stale = true;
        self.pins.clear();
        self.pins_stale = true;
    }

    /// The group chat's other members a reply could go to instead of `agent`.
    pub fn resend_to(&self, agent: &str) -> Vec<String> {
        self.room_open()
            .map(|r| {
                r.members
                    .iter()
                    .filter(|m| *m != agent && self.agent(m).is_some())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Starts the form that makes a group chat: no one in it, no colour, or the agent
    /// whose chat is open already picked. The open group chat's values stay its own.
    pub fn start_room_form(&mut self) {
        self.room_error = None;
        let from = match self.open_room {
            Some(_) => None,
            None => self.selected.clone().filter(|a| self.agent(a).is_some()),
        };
        self.room_form = RoomForm {
            making: true,
            members: from.into_iter().collect(),
            color: String::new(),
        };
    }

    /// The group chat the panel changes: the open one, unless one is being made.
    pub fn room_changing(&self) -> Option<&Room> {
        if self.room_form.making {
            return None;
        }
        self.room_open()
    }

    /// What the panel for group chats shows, with `name` what its name field holds.
    pub fn room_panel(&self, name: &str) -> RoomPanel {
        if let Some(r) = self.room_changing() {
            return RoomPanel {
                room: Some(r.clone()),
                members_row: !r.everyone,
                members: r.members.clone(),
                color: r.color.clone(),
                needs: None,
                members_count: None,
                waiting: false,
            };
        }
        let f = &self.room_form;
        let needs = if name.trim().is_empty() {
            Some(MakeNeeds::Name)
        } else if f.members.len() < MIN_MEMBERS {
            Some(MakeNeeds::Members)
        } else {
            None
        };
        RoomPanel {
            room: None,
            members_row: true,
            members: f.members.clone(),
            color: f.color.clone(),
            needs,
            members_count: Some(MembersCount::of(f.members.len())),
            waiting: self.make_waiting(),
        }
    }

    /// A room.create from here is waiting on its answer, so Make does nothing.
    pub fn make_waiting(&self) -> bool {
        self.making_room.as_ref().is_some_and(|m| m.since.is_some())
    }

    /// Make turns on again once a room.create has waited MAKE_WAIT with no answer, so
    /// the person can try again. Gives whether it turned on.
    pub fn make_timed_out(&mut self, now: Instant) -> bool {
        let Some(m) = self.making_room.as_mut() else {
            return false;
        };
        let waited = m
            .since
            .is_some_and(|at| now.saturating_duration_since(at) >= MAKE_WAIT);
        if waited {
            m.since = None;
        }
        waited
    }

    /// Puts `agent` in the group chat the panel shows, or takes it out: in the form
    /// while one is made, or a room.update for the open one, which keeps two at least.
    pub fn toggle_room_member(&mut self, agent: &str) -> Option<Value> {
        let Some(r) = self.room_changing() else {
            let ms = &mut self.room_form.members;
            match ms.iter().position(|m| m == agent) {
                Some(i) => {
                    ms.remove(i);
                }
                None => ms.push(agent.to_string()),
            }
            return None;
        };
        let (room, mut members) = (r.id.clone(), r.members.clone());
        match members.iter().position(|m| m == agent) {
            Some(_) if members.len() <= MIN_MEMBERS => return None,
            Some(i) => {
                members.remove(i);
            }
            None => members.push(agent.to_string()),
        }
        Some(self.update_room(&room, json!({"members": members})))
    }

    /// Sets the colour of the group chat the panel shows: the form's while one is
    /// made, or a room.update for the open one.
    pub fn set_room_color(&mut self, color: &str) -> Option<Value> {
        let Some(room) = self.room_changing().map(|r| r.id.clone()) else {
            self.room_form.color = color.to_string();
            return None;
        };
        Some(self.update_room(&room, json!({"color": color})))
    }

    /// Makes the group chat in the form, named `name`, once Make is on: never twice for
    /// one click and the next while the first waits. The form stays until it's made, so
    /// a refusal shows under it.
    pub fn make_form_room(&mut self, name: &str) -> Option<Value> {
        if !self.room_panel(name).can_make() {
            return None;
        }
        let (members, color) = (self.room_form.members.clone(), self.room_form.color.clone());
        Some(self.make_room(name, &members, &color))
    }

    /// Makes a group chat. The chat shows it once it's made.
    pub fn make_room(&mut self, name: &str, members: &[String], color: &str) -> Value {
        self.room_error = None;
        let id = self.id("app.room_create");
        self.making_room = Some(Making {
            id: id.clone(),
            since: Some(Instant::now()),
        });
        json!({"t": "cmd", "id": id, "name": "room.create", "args": {"name": name.trim(), "members": members, "color": color}})
    }

    /// The room.create sent last, by its id.
    fn making(&self, id: &str) -> bool {
        self.making_room.as_ref().is_some_and(|m| m.id == id)
    }

    /// room.create's answer: the group chat made, or why it wasn't, under the form. An
    /// answer to one sent before the last is the last's no more.
    pub(super) fn room_create_answered(&mut self, id: &str, f: &Value) {
        if let Some(v) = f.get("value") {
            self.room_made(id, v);
        }
        if let Some(e) = f.pointer("/error/message").and_then(Value::as_str) {
            if self.making(id) {
                self.making_room = None;
                self.room_error = Some(e.to_string());
            }
        }
    }

    /// A room.create that will never be answered: `id`'s, which didn't go for want of
    /// a connection, or with None whatever was sent on a connection that went. Make is
    /// on again.
    pub(super) fn room_create_lost(&mut self, id: Option<&str>) {
        if id.is_none_or(|id| self.making(id)) {
            self.making_room = None;
        }
    }

    /// A group chat made from here: the chat shows it, and the panel its settings, with
    /// its usage asked for so the Usage row is there without opening them again.
    pub(super) fn room_made(&mut self, id: &str, value: &Value) {
        if !self.making(id) {
            return;
        }
        self.making_room = None;
        self.room_form = RoomForm::default();
        let Some(c) = value.get("conversation") else {
            return;
        };
        let conversation = s(c, "id");
        if let Some(r) = c.get("room").filter(|r| r.is_object()) {
            if self.room(&conversation).is_none() {
                self.rooms.push(Room::of(&conversation, r));
            }
        }
        self.show_room(&conversation);
        let f = self.ask_room_usage(&conversation);
        self.to_send.push(f);
        self.dirty.insert(List::Conversations);
        self.dirty.insert(List::Sidebar);
    }

    /// Changes a group chat: `args` as room.update takes them.
    pub fn update_room(&mut self, room: &str, mut args: Value) -> Value {
        args["conversation"] = json!(room);
        let id = self.id("app.room_update");
        json!({"t": "cmd", "id": id, "name": "room.update", "args": args})
    }

    /// Sends the person's message a reply answered to another member, which the
    /// router keeps as a correction.
    pub fn resend(&mut self, message: &str, agent: &str) -> Value {
        let id = self.id("app.room_resend");
        json!({"t": "cmd", "id": id, "name": "room.resend", "args": {"message": message, "agent": agent}})
    }

    /// Deletes a group chat the person made.
    pub fn delete_room(&mut self, room: &str) -> Value {
        let id = self.id("app.room_delete");
        json!({"t": "cmd", "id": id, "name": "conversation.delete", "args": {"conversation": room}})
    }

    /// Forgets one of a group chat's corrections.
    pub fn forget_correction(&mut self, room: &str, correction: &str) -> Value {
        if let Some(r) = self.rooms.iter_mut().find(|r| r.id == room) {
            r.corrections.retain(|c| c.id != correction);
        }
        let id = self.id("app.room_forget");
        json!({"t": "cmd", "id": id, "name": "room.forget", "args": {"conversation": room, "correction": correction}})
    }

    /// Asks what the group chat `room` used, for its settings.
    pub fn ask_room_usage(&mut self, room: &str) -> Value {
        if self.room_usage.as_ref().is_some_and(|u| u.room != room) {
            self.room_usage = None;
        }
        let id = self.id(&format!("app.room_summary:{room}"));
        json!({"t": "cmd", "id": id, "name": "room.summary", "args": {"conversation": room}})
    }

    /// What a group chat used, as room.summary answered.
    pub(super) fn room_usage_came(&mut self, f: &Value) {
        let id = s(f, "id");
        let Some(v) = f.get("value") else {
            return;
        };
        // The id is app.room_summary:ROOM_N, N the model's own count.
        let room = id
            .strip_prefix("app.room_summary:")
            .and_then(|rest| rest.rsplit_once('_').map(|(r, _)| r))
            .unwrap_or_default()
            .to_string();
        let n = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        self.room_usage = Some(RoomUsage {
            room,
            tokens_today: n("tokens_today"),
            tokens_month: n("tokens_month"),
            router_today: n("router_today"),
            router_month: n("router_month"),
        });
    }

    /// The open group chat's usage, once it has come.
    pub fn open_room_usage(&self) -> Option<&RoomUsage> {
        let open = self.open_room.as_deref()?;
        self.room_usage.as_ref().filter(|u| u.room == open)
    }

    /// Changes what an agent is for, the line the router reads.
    pub fn save_for(&mut self, agent: &str, line: &str) -> Value {
        let id = self.id("app.agent_for");
        json!({"t": "cmd", "id": id, "name": "agent.update", "args": {"agent": agent, "for": line.trim()}})
    }
}

#[cfg(test)]
mod tests {
    use super::super::Came;
    use super::*;

    fn with_room(m: &mut Model) {
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Ledger"}, {"id": "a_2", "name": "Hermes"}
        ]}));
        let items = vec![
            json!({"id": "c_1", "agent": "a_1", "title": "Chat"}),
            json!({"id": "c_9", "agent": "", "title": "Money",
                   "room": {"name": "Money", "members": ["a_1", "a_2"], "default": "a_2", "color": "amber",
                            "corrections": [{"id": "k_1", "text": "the vps", "to": "a_1", "from": "a_2", "at": 3}]}}),
        ];
        m.take(&json!({"t": "core.data", "what": "conversations", "items": items}));
    }

    #[test]
    fn a_group_chat_shows_its_own_conversation_with_its_default_member_selected() {
        let mut m = Model::new();
        with_room(&mut m);
        assert_eq!(m.rooms.len(), 1);
        m.show_room("c_9");
        assert_eq!(m.conversation().as_deref(), Some("c_9"));
        assert_eq!(m.selected.as_deref(), Some("a_2"));
        let sent = m.send("hi").unwrap();
        assert_eq!(sent["conversation"], "c_9");
        // An agent picked in the sidebar is its own chat again, even the same one.
        m.select(Some("a_2".into()));
        assert_eq!(m.open_room, None);
        assert_ne!(m.conversation().as_deref(), Some("c_9"));
    }

    // LOO-350: as in an agent's chat, what comes while it's open shows whole, and what
    // was there when it opened is cut.
    #[test]
    fn a_group_chat_s_live_messages_come_before_once_it_opens_again() {
        let mut m = Model::new();
        with_room(&mut m);
        m.synced = true;
        m.show_room("c_9");
        let page = |ids: &[&str]| {
            let items: Vec<Value> = ids
                .iter()
                .map(|id| json!({"id": id, "role": "agent", "at": 1, "blocks": [{"kind": "text", "text": "Words"}]}))
                .collect();
            json!({"t": "core.data", "id": "app.messages:c_9", "what": "messages", "items": items})
        };
        m.take(&page(&["msg_1"]));
        m.take(&json!({"t": "event", "kind": "message.created", "conversation": "c_9", "message": {"id": "msg_2", "role": "agent", "agent": "a_1", "at": 2, "blocks": [{"kind": "text", "text": "Words"}]}}));
        let came = |m: &Model| -> Vec<Came> { m.shown().into_iter().map(|x| x.came).collect() };
        assert_eq!(came(&m), vec![Came::Before, Came::Live]);
        m.flip_fold("more-msg_1/0");
        m.select(Some("a_1".into()));
        m.show_room("c_9");
        assert!(m.fold_flips().is_empty());
        m.take(&page(&["msg_1", "msg_2"]));
        assert_eq!(came(&m), vec![Came::Before, Came::Before]);
    }

    // LOO-371: opening a group chat reads the group chat, not its lead agent's own chat.
    #[test]
    fn opening_a_group_chat_keeps_its_lead_agent_s_unread_dot() {
        let mut m = Model::new();
        with_room(&mut m);
        m.take(&json!({"t": "core.data", "what": "conversations", "items": [
            {"id": "c_1", "agent": "a_1", "title": "Chat"},
            {"id": "c_2", "agent": "a_2", "title": "Chat"},
            {"id": "c_9", "agent": "", "title": "Money", "room": {"name": "Money", "members": ["a_1", "a_2"], "default": "a_2"}}
        ]}));
        m.synced = true;
        m.take(&json!({"t": "event", "seq": 4, "kind": "message.created", "conversation": "c_2",
            "message": {"id": "msg_7", "conversation": "c_2", "role": "agent", "agent": "a_2", "at": 5, "blocks": []}}));
        assert!(m.unread.contains("a_2"));
        // Hermes leads the group chat, and is selected while it's open.
        m.show_room("c_9");
        assert_eq!(m.selected.as_deref(), Some("a_2"));
        assert!(m.unread.contains("a_2"), "its own chat isn't read");
        assert_eq!(m.new_line, None);
        // Closed for another agent's chat, Hermes still has its dot.
        m.select(Some("a_1".into()));
        assert!(m.unread.contains("a_2"));
        // Its own chat read: the dot goes, and the new line is over its reply.
        m.select(Some("a_2".into()));
        assert!(!m.unread.contains("a_2"));
        assert_eq!(m.new_line, Some(("msg_7".to_string(), 1)));
    }

    #[test]
    fn one_pick_of_another_agent_leaves_the_group_chat() {
        let mut m = Model::new();
        with_room(&mut m);
        m.show_room("c_9");
        assert_eq!(m.selected.as_deref(), Some("a_2"));
        // Ledger isn't the member selected while the group chat is open: the one
        // pick shows its own chat (LOO-351).
        m.select(Some("a_1".into()));
        assert_eq!(m.open_room, None);
        assert_eq!(m.selected.as_deref(), Some("a_1"));
        assert_eq!(m.conversation().as_deref(), Some("c_1"));
        // And back into the group chat, then out to its selected member.
        m.show_room("c_9");
        assert_eq!(m.conversation().as_deref(), Some("c_9"));
        m.select(Some("a_2".into()));
        assert_eq!(m.open_room, None);
        // From one group chat into another, the first is left.
        m.take(&json!({"t": "core.data", "what": "conversations", "items": [
            {"id": "c_1", "agent": "a_1", "title": "Chat"},
            {"id": "c_9", "agent": "", "title": "Money", "room": {"name": "Money", "members": ["a_1", "a_2"], "default": "a_2"}},
            {"id": "c_10", "agent": "", "title": "Code", "room": {"name": "Code", "members": ["a_1", "a_2"], "default": "a_1"}}
        ]}));
        m.show_room("c_9");
        m.show_room("c_10");
        assert_eq!(m.conversation().as_deref(), Some("c_10"));
        assert_eq!(m.selected.as_deref(), Some("a_1"));
    }

    #[test]
    fn a_group_chat_s_context_is_the_member_s_that_answered_last() {
        let mut m = Model::new();
        with_room(&mut m);
        let items = vec![
            json!({"id": "c_1", "agent": "a_1", "title": "Chat"}),
            json!({"id": "c_9", "agent": "", "title": "Money",
                   "room": {"name": "Money", "members": ["a_1", "a_2"], "default": "a_2", "color": "amber"},
                   "context": {"used": 40_000, "size": 200_000, "compacts": true, "agent": "a_1"}}),
        ];
        m.take(&json!({"t": "core.data", "what": "conversations", "items": items}));
        m.show_room("c_9");
        let fill = m.context().expect("a member's");
        assert_eq!(m.context_title(&fill), "Ledger's context");
        let f = m.compact_now().unwrap();
        assert_eq!(f["args"], json!({"conversation": "c_9", "agent": "a_1"}));
        let f = m.fresh_session().unwrap();
        assert_eq!(f["name"], "conversation.fresh");
        assert_eq!(f["args"]["agent"], "a_1");
    }

    #[test]
    fn a_reply_goes_to_another_member_and_a_correction_is_forgotten() {
        let mut m = Model::new();
        with_room(&mut m);
        m.show_room("c_9");
        assert_eq!(m.resend_to("a_2"), vec!["a_1".to_string()]);
        let f = m.resend("msg_4", "a_1");
        assert_eq!(f["name"], "room.resend");
        assert_eq!(f["args"]["agent"], "a_1");
        let f = m.forget_correction("c_9", "k_1");
        assert_eq!(f["args"]["correction"], "k_1");
        assert!(m.room("c_9").unwrap().corrections.is_empty());
    }

    #[test]
    fn a_group_chat_made_here_opens_once_it_is_made() {
        let mut m = Model::new();
        with_room(&mut m);
        let f = m.make_room(" Code ", &["a_1".into(), "a_2".into()], "mint");
        assert_eq!(f["args"]["name"], "Code");
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "ok": true, "value": {"conversation": {
            "id": "c_12", "agent": "", "title": "Code",
            "room": {"name": "Code", "members": ["a_1", "a_2"], "color": "mint", "default": "", "corrections": []}
        }}}));
        assert_eq!(m.open_room.as_deref(), Some("c_12"));
        assert_eq!(m.room_open().map(|r| r.color.as_str()), Some("mint"));
    }

    #[test]
    fn a_group_chat_s_usage_has_the_router_s_beside_its_members() {
        let mut m = Model::new();
        with_room(&mut m);
        m.show_room("c_9");
        let f = m.ask_room_usage("c_9");
        assert_eq!(f["name"], "room.summary");
        assert_eq!(f["args"]["conversation"], "c_9");
        assert_eq!(m.open_room_usage(), None);
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "ok": true, "value": {
            "tokens_today": 850, "tokens_month": 12400, "router_today": 60, "router_month": 900
        }}));
        let u = m.open_room_usage().unwrap();
        assert_eq!(
            (
                u.tokens_today,
                u.tokens_month,
                u.router_today,
                u.router_month
            ),
            (850, 12400, 60, 900)
        );
        // Another group chat's isn't this one's.
        m.room_usage.as_mut().unwrap().room = "c_1".into();
        assert_eq!(m.open_room_usage(), None);
    }

    /// Three agents, Everyone, and the Money group chat with two of them.
    fn with_everyone(m: &mut Model) {
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Ledger"}, {"id": "a_2", "name": "Hermes"}, {"id": "a_3", "name": "Casper"}
        ]}));
        let items = vec![
            json!({"id": "c_1", "agent": "a_1", "title": "Chat"}),
            json!({"id": "c_0", "agent": "", "title": "Everyone",
                   "room": {"name": "Everyone", "members": ["a_1", "a_2", "a_3"], "default": "", "color": "", "everyone": true}}),
            json!({"id": "c_9", "agent": "", "title": "Money",
                   "room": {"name": "Money", "members": ["a_1", "a_2"], "default": "a_2", "color": "amber"}}),
        ];
        m.take(&json!({"t": "core.data", "what": "conversations", "items": items}));
    }

    /// The form as New group chat starts it: Members there, no one picked, no colour,
    /// and Make off until it has a name.
    fn assert_fresh_form(m: &Model) {
        let p = m.room_panel("");
        assert!(p.making());
        assert_eq!(p.room, None);
        assert!(p.members_row);
        assert!(p.members.is_empty());
        assert_eq!(p.color, "");
        assert_eq!(p.needs, Some(MakeNeeds::Name));
        assert_eq!(
            p.members_count,
            Some(MembersCount {
                text: "0 of 2".into(),
                met: false
            })
        );
        assert!(!p.can_make());
        assert_eq!(m.room_changing(), None);
    }

    #[test]
    fn a_new_group_chat_opened_from_everyone_shows_members_and_makes_it() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_0");
        // Everyone's own settings have no members row.
        let p = m.room_panel("Everyone");
        assert!(!p.making());
        assert!(!p.members_row);
        m.start_room_form();
        assert_fresh_form(&m);
        assert_eq!(m.room_panel("Code").needs, Some(MakeNeeds::Members));
        assert_eq!(m.toggle_room_member("a_1"), None);
        assert_eq!(m.toggle_room_member("a_3"), None);
        assert_eq!(m.set_room_color("mint"), None);
        // A name of spaces isn't one.
        assert_eq!(m.room_panel("  ").needs, Some(MakeNeeds::Name));
        assert_eq!(m.make_form_room("  "), None);
        let p = m.room_panel(" Code ");
        assert!(p.can_make());
        assert_eq!(p.members, vec!["a_1".to_string(), "a_3".to_string()]);
        assert_eq!(p.color, "mint");
        let f = m.make_form_room(" Code ").unwrap();
        assert_eq!(f["name"], "room.create");
        assert_eq!(f["args"]["name"], "Code");
        assert_eq!(f["args"]["members"], json!(["a_1", "a_3"]));
        assert_eq!(f["args"]["color"], "mint");
        // Until it's made the form stays, not Everyone's settings, so Save there can't
        // rename Everyone and a refusal shows under the form.
        assert!(m.room_panel("Code").making());
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "ok": true, "value": {"conversation": {
            "id": "c_12", "agent": "", "title": "Code",
            "room": {"name": "Code", "members": ["a_1", "a_3"], "color": "mint", "default": "", "corrections": []}
        }}}));
        assert_eq!(m.open_room.as_deref(), Some("c_12"));
        let p = m.room_panel("Code");
        assert_eq!(p.room.map(|r| r.id), Some("c_12".to_string()));
        assert!(p.members_row);
    }

    // LOO-355: the new group chat's settings had no Usage row until they were opened
    // again.
    #[test]
    fn a_group_chat_made_from_the_panel_asks_for_its_usage() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_0");
        // Everyone's settings were open, with their usage.
        let f = m.ask_room_usage("c_0");
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {
            "tokens_today": 850, "tokens_month": 12400, "router_today": 60, "router_month": 900
        }}));
        assert!(m.open_room_usage().is_some());
        m.start_room_form();
        m.toggle_room_member("a_1");
        m.toggle_room_member("a_3");
        let f = m.make_form_room("Code").unwrap();
        assert!(m.to_send.is_empty());
        m.take(&json!({"t": "result", "id": f["id"], "ok": true, "value": {"conversation": {
            "id": "c_12", "agent": "", "title": "Code",
            "room": {"name": "Code", "members": ["a_1", "a_3"], "color": "", "default": "", "corrections": []}
        }}}));
        assert_eq!(
            m.room_panel("Code").room.map(|r| r.id),
            Some("c_12".to_string())
        );
        let asked: Vec<&Value> = m
            .to_send
            .iter()
            .filter(|f| f["name"] == "room.summary")
            .collect();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0]["args"]["conversation"], "c_12");
        // Not Everyone's meanwhile, and the new one's zeros once they come.
        assert_eq!(m.open_room_usage(), None);
        let id = asked[0]["id"].clone();
        m.take(&json!({"t": "result", "id": id, "ok": true, "value": {
            "tokens_today": 0, "tokens_month": 0, "router_today": 0, "router_month": 0
        }}));
        let u = m.open_room_usage().unwrap();
        assert_eq!(u.room, "c_12");
        assert_eq!((u.tokens_today, u.router_month), (0, 0));
    }

    #[test]
    fn a_refused_group_chat_keeps_its_form() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_0");
        m.start_room_form();
        m.toggle_room_member("a_1");
        m.toggle_room_member("a_2");
        let f = m.make_form_room("Code").unwrap();
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&json!({"t": "result", "id": id, "ok": false, "error": {"message": "That name is taken"}}));
        assert_eq!(m.room_error.as_deref(), Some("That name is taken"));
        let p = m.room_panel("Code");
        assert!(p.making());
        assert!(p.can_make());
        // Opening it again starts it afresh.
        m.start_room_form();
        assert_eq!(m.room_error, None);
        assert_fresh_form(&m);
    }

    /// The form ready to make Code, with Ledger and Hermes, from Everyone.
    fn ready_to_make(m: &mut Model) {
        with_everyone(m);
        m.show_room("c_0");
        m.start_room_form();
        m.toggle_room_member("a_1");
        m.toggle_room_member("a_2");
    }

    fn made(id: &str) -> Value {
        json!({"t": "result", "id": id, "ok": true, "value": {"conversation": {
            "id": "c_12", "agent": "", "title": "Code",
            "room": {"name": "Code", "members": ["a_1", "a_2"], "color": "", "default": "", "corrections": []}
        }}})
    }

    // LOO-354: a quick second click on Make, or Enter after it, made a second group
    // chat with the same name.
    #[test]
    fn make_clicked_twice_makes_one_group_chat() {
        let mut m = Model::new();
        ready_to_make(&mut m);
        let f = m.make_form_room("Code").unwrap();
        let p = m.room_panel("Code");
        assert!(p.waiting);
        assert_eq!(p.needs, None);
        assert!(!p.can_make());
        assert_eq!(m.make_form_room("Code"), None);
        // Not yet waited long enough.
        assert!(!m.make_timed_out(Instant::now()));
        assert_eq!(m.make_form_room("Code"), None);
        let id = f["id"].as_str().unwrap().to_string();
        m.take(&made(&id));
        assert_eq!(m.open_room.as_deref(), Some("c_12"));
        assert!(!m.make_waiting());
        // A refusal turns Make on again, with why under the form.
        m.start_room_form();
        m.toggle_room_member("a_1");
        m.toggle_room_member("a_3");
        let f = m.make_form_room("Code").unwrap();
        assert_eq!(m.make_form_room("Code"), None);
        m.take(&json!({"t": "result", "id": f["id"], "ok": false, "error": {"message": "That name is taken"}}));
        assert_eq!(m.room_error.as_deref(), Some("That name is taken"));
        assert!(m.room_panel("Code").can_make());
    }

    #[test]
    fn make_turns_on_again_when_no_answer_comes() {
        let mut m = Model::new();
        ready_to_make(&mut m);
        let first = m.make_form_room("Code").unwrap();
        assert!(m.make_timed_out(Instant::now() + MAKE_WAIT));
        assert!(!m.make_timed_out(Instant::now() + MAKE_WAIT));
        let p = m.room_panel("Code");
        assert!(!p.waiting);
        assert!(p.can_make());
        // Tried again, the first's late answer is the second's no more.
        let second = m.make_form_room("Code").unwrap();
        assert_ne!(first["id"], second["id"]);
        assert_eq!(m.make_form_room("Code"), None);
        m.take(&json!({"t": "result", "id": first["id"], "ok": false, "error": {"message": "That name is taken"}}));
        assert_eq!(m.room_error, None);
        assert!(m.make_waiting());
        m.take(&made(first["id"].as_str().unwrap()));
        assert_eq!(m.open_room.as_deref(), Some("c_0"));
        m.take(&made(second["id"].as_str().unwrap()));
        assert_eq!(m.open_room.as_deref(), Some("c_12"));
    }

    #[test]
    fn a_late_answer_after_the_wait_still_shows_the_group_chat() {
        let mut m = Model::new();
        ready_to_make(&mut m);
        let f = m.make_form_room("Code").unwrap();
        assert!(m.make_timed_out(Instant::now() + MAKE_WAIT));
        m.take(&made(f["id"].as_str().unwrap()));
        assert_eq!(m.open_room.as_deref(), Some("c_12"));
        assert!(!m.room_panel("Code").making());
    }

    #[test]
    fn make_turns_on_again_when_the_connection_goes() {
        let mut m = Model::new();
        ready_to_make(&mut m);
        let f = m.make_form_room("Code").unwrap();
        // Reconnecting: what was sent on the old connection is never answered.
        m.take(&json!({"t": "core.conn", "state": "offline", "retry_ms": 500}));
        assert!(!m.make_waiting());
        assert!(m.room_panel("Code").can_make());
        m.take(&json!({"t": "core.conn", "state": "live"}));
        // Sent while offline, it didn't go.
        let f2 = m.make_form_room("Code").unwrap();
        assert_ne!(f["id"], f2["id"]);
        assert!(m.make_waiting());
        // Another command's didn't-go isn't this one's.
        m.take(&json!({"t": "error", "code": "offline", "for": "app.room_update_9"}));
        assert!(m.make_waiting());
        m.take(&json!({"t": "error", "code": "offline", "for": f2["id"]}));
        assert!(!m.make_waiting());
        assert!(m.room_panel("Code").can_make());
        assert!(m.notices.is_empty());
    }

    #[test]
    fn a_new_group_chat_opened_from_another_group_chat_starts_empty() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_9");
        // Money's settings are Money's.
        let p = m.room_panel("Money");
        assert!(p.members_row);
        assert_eq!(p.members, vec!["a_1".to_string(), "a_2".to_string()]);
        assert_eq!(p.color, "amber");
        m.start_room_form();
        // Not its members, its colour or its default member.
        assert_fresh_form(&m);
        m.toggle_room_member("a_2");
        m.toggle_room_member("a_3");
        // Picked again, one goes.
        m.toggle_room_member("a_2");
        assert_eq!(m.room_panel("Code").needs, Some(MakeNeeds::Members));
        m.toggle_room_member("a_1");
        assert!(m.room_panel("Code").can_make());
        // The open group chat isn't changed meanwhile.
        assert_eq!(
            m.room("c_9").map(|r| r.members.clone()),
            Some(vec!["a_1".to_string(), "a_2".to_string()])
        );
    }

    #[test]
    fn a_new_group_chat_opened_from_an_agent_s_chat_has_that_agent_picked() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.select(Some("a_2".into()));
        m.start_room_form();
        let p = m.room_panel("");
        assert!(p.making());
        assert!(p.members_row);
        assert_eq!(p.members, vec!["a_2".to_string()]);
        assert_eq!(p.color, "");
        assert_eq!(m.room_panel("Code").needs, Some(MakeNeeds::Members));
        m.toggle_room_member("a_3");
        assert!(m.room_panel("Code").can_make());
        let f = m.make_form_room("Code").unwrap();
        assert_eq!(f["args"]["members"], json!(["a_2", "a_3"]));
    }

    #[test]
    fn members_counts_the_picked_of_two_until_it_has_them() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_0");
        m.start_room_form();
        let count = |m: &Model| {
            let c = m.room_panel("Code").members_count.unwrap();
            (c.text, c.met)
        };
        assert_eq!(count(&m), ("0 of 2".to_string(), false));
        m.toggle_room_member("a_1");
        assert_eq!(count(&m), ("1 of 2".to_string(), false));
        m.toggle_room_member("a_2");
        assert_eq!(count(&m), ("2 picked".to_string(), true));
        m.toggle_room_member("a_3");
        assert_eq!(count(&m), ("3 picked".to_string(), true));
        // Taken out again, it counts down, and the name doesn't change it.
        m.toggle_room_member("a_3");
        m.toggle_room_member("a_2");
        assert_eq!(count(&m), ("1 of 2".to_string(), false));
        assert_eq!(
            m.room_panel("").members_count.map(|c| c.text),
            Some("1 of 2".to_string())
        );
    }

    #[test]
    fn an_open_group_chat_s_settings_have_no_members_count() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_9");
        let p = m.room_panel("Money");
        assert!(p.members_row);
        assert_eq!(p.members_count, None);
        m.show_room("c_0");
        assert_eq!(m.room_panel("Everyone").members_count, None);
    }

    #[test]
    fn an_open_group_chat_s_settings_change_it_and_keep_two_members() {
        let mut m = Model::new();
        with_everyone(&mut m);
        m.show_room("c_9");
        // Two at least: taking one out of two sends nothing.
        assert_eq!(m.toggle_room_member("a_1"), None);
        let f = m.toggle_room_member("a_3").unwrap();
        assert_eq!(f["name"], "room.update");
        assert_eq!(f["args"]["conversation"], "c_9");
        assert_eq!(f["args"]["members"], json!(["a_1", "a_2", "a_3"]));
        let f = m.set_room_color("rose").unwrap();
        assert_eq!(f["args"]["color"], "rose");
        assert!(m.room_form.members.is_empty());
        assert_eq!(m.room_form.color, "");
        // There's nothing to make from its settings.
        assert_eq!(m.make_form_room("Money"), None);
    }

    #[test]
    fn a_reply_says_how_it_came_to_its_agent() {
        let r = Routed::of(&json!({"agent": "a_2", "routed": {"by": "handoff", "said": "handed off by Ledger", "from": "a_1"}}))
            .unwrap();
        assert_eq!(
            (r.agent.as_str(), r.by.as_str(), r.from.as_str()),
            ("a_2", "handoff", "a_1")
        );
        assert_eq!(Routed::of(&json!({"agent": "a_2"})), None);
    }
}
