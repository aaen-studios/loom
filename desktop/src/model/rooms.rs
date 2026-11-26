//! Group chats (PLAN.md, "Group chats"): conversations several of the person's agents
//! take part in, where the instance picks one to answer each message. The model shows
//! them as the instance has them and sends what the person changes; the picking is the
//! instance's.

use serde_json::{json, Value};

use super::{s, List, Model};

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
        self.select(lead);
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

    /// Makes a group chat. The chat shows it once it's made.
    pub fn make_room(&mut self, name: &str, members: &[String], color: &str) -> Value {
        self.room_error = None;
        let id = self.id("app.room_create");
        self.making_room = Some(id.clone());
        json!({"t": "cmd", "id": id, "name": "room.create", "args": {"name": name.trim(), "members": members, "color": color}})
    }

    /// A group chat made from here: the chat shows it.
    pub(super) fn room_made(&mut self, id: &str, value: &Value) {
        if self.making_room.as_deref() != Some(id) {
            return;
        }
        self.making_room = None;
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

    /// The group chats in the sidebar's order, Everyone first, as the core puts them.
    pub fn sidebar_rooms(&self) -> Vec<&Room> {
        let mut out: Vec<&Room> = self
            .room_order
            .iter()
            .filter_map(|id| self.room(id))
            .collect();
        // Made since the sidebar came.
        for r in &self.rooms {
            if !self.room_order.contains(&r.id) {
                out.push(r);
            }
        }
        out
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
