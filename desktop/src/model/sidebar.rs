//! The sidebar (PLAN.md, "The sidebar"), as the core groups it (`core/sidebar.calx`):
//! the pinned agents as tiles, who's waiting on the person, then one list of agents and
//! group chats in the person's order with the folders in it, and the hidden agents. What
//! a drag does is here too: where the dragged one lands, and what the instance is told,
//! so every device shows the same order.

use std::collections::HashSet;
use std::rc::Rc;

use serde_json::{json, Value};

use super::{s, Model};

/// An agent or a group chat in the sidebar, by its id (a group chat's is its
/// conversation's).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SideItem {
    Agent(String),
    Room(String),
}

impl SideItem {
    pub fn id(&self) -> &str {
        match self {
            SideItem::Agent(id) | SideItem::Room(id) => id,
        }
    }

    /// One of a section's `items`, or one of its `agents`, which are ids alone.
    fn of(v: &Value) -> Option<SideItem> {
        if let Some(id) = v.as_str() {
            return Some(SideItem::Agent(id.to_string()));
        }
        let id = s(v, "id");
        if id.is_empty() {
            return None;
        }
        Some(match v.get("kind").and_then(Value::as_str) {
            Some("room") => SideItem::Room(id),
            _ => SideItem::Agent(id),
        })
    }
}

/// A part of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum SidePart {
    /// The pinned agents, as tiles at the top.
    Pinned,
    /// The agents with a question open, under the tiles.
    Waiting,
    /// The agents and group chats out of any folder, together.
    List,
    Folder {
        id: String,
        name: String,
        collapsed: bool,
    },
    Hidden,
}

impl SidePart {
    /// The folder it is, or None.
    fn folder(&self) -> Option<&str> {
        match self {
            SidePart::Folder { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Whether it's the list or a folder, whose rows drag among each other's.
    fn listed(&self) -> bool {
        matches!(self, SidePart::List | SidePart::Folder { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub part: SidePart,
    /// What's in it, in order.
    pub items: Vec<SideItem>,
}

impl Section {
    fn of(v: &Value) -> Option<Section> {
        let part = match v.get("kind").and_then(Value::as_str)? {
            "pinned" => SidePart::Pinned,
            "waiting" => SidePart::Waiting,
            "list" => SidePart::List,
            "hidden" => SidePart::Hidden,
            "group" => SidePart::Folder {
                id: s(v, "id"),
                name: s(v, "name"),
                collapsed: v.get("collapsed").and_then(Value::as_bool).unwrap_or(false),
            },
            _ => return None,
        };
        let items = v
            .get("items")
            .or_else(|| v.get("agents"))
            .and_then(Value::as_array)
            .map(|is| is.iter().filter_map(SideItem::of).collect())
            .unwrap_or_default();
        Some(Section { part, items })
    }

    fn empty(part: SidePart) -> Section {
        Section {
            part,
            items: Vec::new(),
        }
    }

    /// Its agents' ids, in order.
    pub fn agents(&self) -> impl Iterator<Item = &str> + '_ {
        self.items.iter().filter_map(|i| match i {
            SideItem::Agent(id) => Some(id.as_str()),
            SideItem::Room(_) => None,
        })
    }

    fn has(&self, id: &str) -> bool {
        self.items.iter().any(|i| i.id() == id)
    }
}

/// Where a dragged agent or group chat is dropped in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drop {
    /// Just over or under another one, where the sidebar draws its line.
    Before(String),
    After(String),
    /// On a folder's name: in that folder, last.
    Folder(String),
}

impl Model {
    /// The sidebar as the core grouped it, with the person's whole order.
    pub(super) fn take_sidebar(&mut self, items: &[Value]) {
        self.sidebar = Rc::new(items.iter().filter_map(Section::of).collect());
        self.side_order = items
            .iter()
            .find(|sec| sec.get("kind").and_then(Value::as_str) == Some("order"))
            .and_then(|sec| sec.get("items"))
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
    }

    /// The sidebar to draw: as the core grouped it, without what's gone since, and with
    /// an agent or a group chat made since at the end of the list. Before the core has
    /// grouped them, the pinned agents, then the group chats and the other agents as
    /// they're listed, then the hidden ones.
    pub fn side_sections(&self) -> Vec<Section> {
        if self.sidebar.is_empty() {
            let mut pinned = Section::empty(SidePart::Pinned);
            let mut list = Section::empty(SidePart::List);
            let mut hidden = Section::empty(SidePart::Hidden);
            list.items
                .extend(self.rooms.iter().map(|r| SideItem::Room(r.id.clone())));
            for a in self.agents.iter() {
                let it = SideItem::Agent(a.id.clone());
                if a.hidden {
                    hidden.items.push(it);
                } else if a.pinned.is_some() {
                    pinned.items.push(it);
                } else {
                    list.items.push(it);
                }
            }
            return vec![pinned, Section::empty(SidePart::Waiting), list, hidden];
        }
        let mut out: Vec<Section> = self.sidebar.as_ref().clone();
        for sec in out.iter_mut() {
            sec.items.retain(|i| match i {
                SideItem::Agent(id) => self.agent(id).is_some(),
                SideItem::Room(id) => self.room(id).is_some(),
            });
        }
        let listed: HashSet<String> = out
            .iter()
            .flat_map(|sec| sec.items.iter().map(|i| i.id().to_string()))
            .collect();
        let made: Vec<SideItem> = self
            .rooms
            .iter()
            .filter(|r| !listed.contains(&r.id))
            .map(|r| SideItem::Room(r.id.clone()))
            .chain(
                self.agents
                    .iter()
                    .filter(|a| !listed.contains(&a.id))
                    .map(|a| SideItem::Agent(a.id.clone())),
            )
            .collect();
        if let Some(list) = out.iter_mut().find(|sec| sec.part == SidePart::List) {
            list.items.extend(made);
        }
        out
    }

    /// The part of the sidebar an agent or a group chat is in.
    #[cfg(test)]
    pub fn part_of(&self, id: &str) -> Option<SidePart> {
        self.side_sections()
            .into_iter()
            .find(|sec| sec.has(id))
            .map(|sec| sec.part)
    }

    /// The sidebar's folders, each id and name, in order.
    pub fn folders(&self) -> Vec<(String, String)> {
        self.sidebar
            .iter()
            .filter_map(|sec| match &sec.part {
                SidePart::Folder { id, name, .. } => Some((id.clone(), name.clone())),
                _ => None,
            })
            .collect()
    }

    /// What the rail shows, a line between each part: the pinned agents, who's waiting,
    /// the list, and each folder's, folded or not, without the hidden ones and empty parts.
    pub fn rail_parts(&self) -> Vec<Section> {
        self.side_sections()
            .into_iter()
            .filter(|sec| sec.part != SidePart::Hidden && !sec.items.is_empty())
            .collect()
    }

    /// Whether a reply came in the group chat `room` while it wasn't open.
    pub fn room_unread(&self, room: &str) -> bool {
        self.unread.contains(room)
    }

    /// Every agent and group chat in the person's order, as `sidebar.order` takes them:
    /// the core's, with what was made since at the end.
    fn whole_order(&self, sections: &[Section]) -> Vec<String> {
        let here: HashSet<&str> = sections
            .iter()
            .flat_map(|sec| sec.items.iter().map(SideItem::id))
            .collect();
        let mut order: Vec<String> = self
            .side_order
            .iter()
            .filter(|id| here.contains(id.as_str()))
            .cloned()
            .collect();
        for sec in sections {
            for it in &sec.items {
                if !order.iter().any(|id| id == it.id()) {
                    order.push(it.id().to_string());
                }
            }
        }
        order
    }

    /// The drop that puts the tile `id` in the place of the tile `onto`: after it when
    /// it came from before it, else before it.
    pub fn onto(&self, id: &str, onto: &str) -> Drop {
        let pinned = self
            .side_sections()
            .into_iter()
            .find(|sec| sec.part == SidePart::Pinned)
            .map(|sec| sec.items)
            .unwrap_or_default();
        let at = |x: &str| pinned.iter().position(|i| i.id() == x);
        match (at(id), at(onto)) {
            (Some(a), Some(b)) if a < b => Drop::After(onto.to_string()),
            _ => Drop::Before(onto.to_string()),
        }
    }

    /// Moves the agent or group chat `id`, dragged in the sidebar, to `to`. Tiles move
    /// among the tiles; the list's rows move in the list and in and out of folders, a
    /// row dropped beside one in a folder going in it, and beside one out of them coming
    /// out. Who's waiting and the hidden ones don't move. It shows there at once, and
    /// what the instance is told comes back: the folder it's in now, when that changed,
    /// and the new order.
    pub fn drop_item(&mut self, id: &str, to: &Drop) -> Vec<Value> {
        let sections = self.side_sections();
        let find = |x: &str| sections.iter().position(|sec| sec.has(x));
        let Some(from) = find(id) else {
            return Vec::new();
        };
        let (into, mut place) = match to {
            Drop::Before(t) | Drop::After(t) => {
                if t == id {
                    return Vec::new();
                }
                let Some(k) = find(t) else {
                    return Vec::new();
                };
                let at = sections[k].items.iter().position(|i| i.id() == t);
                let at = at.unwrap_or_default();
                (
                    k,
                    if matches!(to, Drop::After(_)) {
                        at + 1
                    } else {
                        at
                    },
                )
            }
            Drop::Folder(f) => {
                let Some(k) = sections
                    .iter()
                    .position(|sec| sec.part.folder() == Some(f.as_str()))
                else {
                    return Vec::new();
                };
                (k, sections[k].items.len())
            }
        };
        let (from_part, into_part) = (&sections[from].part, &sections[into].part);
        let moves = (*from_part == SidePart::Pinned && *into_part == SidePart::Pinned)
            || (from_part.listed() && into_part.listed());
        if !moves {
            return Vec::new();
        }
        let refolder = from_part.folder() != into_part.folder();
        let folder = into_part.folder().unwrap_or_default().to_string();
        let old_order = self.whole_order(&sections);
        let mut next = sections.clone();
        let Some(was) = next[from].items.iter().position(|i| i.id() == id) else {
            return Vec::new();
        };
        let item = next[from].items.remove(was);
        if from == into && was < place {
            place -= 1;
        }
        next[into].items.insert(place, item.clone());
        // Its place in the whole order: after the one it now follows, or else before the
        // one it now leads; alone in a folder, last.
        let mut order = old_order.clone();
        order.retain(|x| x != id);
        let after = place
            .checked_sub(1)
            .and_then(|k| next[into].items.get(k))
            .and_then(|i| order.iter().position(|x| x == i.id()))
            .map(|k| k + 1);
        let before = next[into]
            .items
            .get(place + 1)
            .and_then(|i| order.iter().position(|x| x == i.id()));
        let at = after.or(before).unwrap_or(order.len());
        order.insert(at, id.to_string());
        let mut out = Vec::new();
        if refolder {
            out.push(match &item {
                SideItem::Agent(agent) => {
                    if let Some(a) = Rc::make_mut(&mut self.agents)
                        .iter_mut()
                        .find(|a| &a.id == agent)
                    {
                        a.group = (!folder.is_empty()).then(|| folder.clone());
                    }
                    let cmd = self.id("app.group_agent");
                    json!({"t": "cmd", "id": cmd, "name": "agent.group", "args": {"agent": agent, "group": folder}})
                }
                SideItem::Room(room) => {
                    let cmd = self.id("app.room_update");
                    json!({"t": "cmd", "id": cmd, "name": "room.update", "args": {"conversation": room, "group": folder}})
                }
            });
        }
        if order != old_order {
            let cmd = self.id("app.order");
            out.push(
                json!({"t": "cmd", "id": cmd, "name": "sidebar.order", "args": {"items": order}}),
            );
            self.side_order = order;
        }
        if !out.is_empty() {
            self.sidebar = Rc::new(next);
        }
        out
    }

    /// Puts an agent in a sidebar folder, or with None, out of them, from its menu. It
    /// shows there at once, and the instance tells every device.
    pub fn group_agent(&mut self, agent: &str, group: Option<&str>) -> Value {
        let mut sections = self.side_sections();
        if !sections.is_empty() {
            for sec in sections.iter_mut() {
                sec.items.retain(|i| i.id() != agent);
            }
            let into = sections.iter_mut().find(|sec| match (&sec.part, group) {
                (SidePart::Folder { id, .. }, Some(g)) => id == g,
                (SidePart::List, None) => true,
                _ => false,
            });
            if let Some(sec) = into {
                sec.items.push(SideItem::Agent(agent.to_string()));
            }
            self.sidebar = Rc::new(sections);
        }
        let id = self.id("app.group_agent");
        json!({"t": "cmd", "id": id, "name": "agent.group", "args": {"agent": agent, "group": group.unwrap_or_default()}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A person with three agents, Hermes waiting on them and Casper pinned, two group
    /// chats, and a folder with an agent and a group chat in it.
    fn with_sidebar(m: &mut Model) {
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Casper", "pinned": 5},
            {"id": "a_2", "name": "Hermes"},
            {"id": "a_3", "name": "Juniper"},
            {"id": "a_4", "name": "Shop"},
            {"id": "a_5", "name": "Nook", "group": "g_1"},
            {"id": "a_6", "name": "Old", "hidden": 3}
        ]}));
        let room = |id: &str, name: &str| json!({"id": id, "agent": "", "title": name, "room": {"name": name, "members": ["a_2", "a_3"]}});
        m.take(
            &json!({"t": "core.data", "what": "conversations", "items": [
                room("c_1", "Money"), room("c_2", "House")
            ]}),
        );
        m.take(&json!({"t": "core.data", "what": "sidebar", "items": [
            {"kind": "pinned", "agents": ["a_1"]},
            {"kind": "waiting", "agents": ["a_2"]},
            {"kind": "list", "items": [{"kind": "room", "id": "c_1", "name": "Money"}, {"kind": "agent", "id": "a_3"}, {"kind": "agent", "id": "a_4"}]},
            {"kind": "group", "id": "g_1", "name": "Home", "collapsed": false, "items": [{"kind": "agent", "id": "a_5"}, {"kind": "room", "id": "c_2"}]},
            {"kind": "hidden", "agents": ["a_6"]},
            {"kind": "order", "items": ["a_1", "c_1", "a_2", "a_3", "a_4", "a_5", "c_2", "a_6"]}
        ]}));
    }

    fn ids(m: &Model, part: SidePart) -> Vec<String> {
        m.side_sections()
            .into_iter()
            .find(|sec| sec.part == part)
            .map(|sec| sec.items.iter().map(|i| i.id().to_string()).collect())
            .unwrap_or_default()
    }

    fn home() -> SidePart {
        SidePart::Folder {
            id: "g_1".into(),
            name: "Home".into(),
            collapsed: false,
        }
    }

    fn order_sent(f: &[Value]) -> Vec<String> {
        let sent = f.iter().find(|f| f["name"] == "sidebar.order").unwrap();
        serde_json::from_value(sent["args"]["items"].clone()).unwrap()
    }

    #[test]
    fn the_sidebar_is_tiles_then_who_waits_then_one_list_of_agents_and_group_chats() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        let parts: Vec<SidePart> = m.side_sections().into_iter().map(|s| s.part).collect();
        assert_eq!(
            parts,
            [
                SidePart::Pinned,
                SidePart::Waiting,
                SidePart::List,
                home(),
                SidePart::Hidden
            ]
        );
        assert_eq!(ids(&m, SidePart::List), ["c_1", "a_3", "a_4"]);
        assert_eq!(m.side_sections()[2].items[0], SideItem::Room("c_1".into()));
        assert_eq!(ids(&m, home()), ["a_5", "c_2"]);
        assert_eq!(m.part_of("c_2"), Some(home()));
        assert_eq!(m.folders(), [("g_1".to_string(), "Home".to_string())]);
        // The rail: each part but the hidden ones.
        let rail: Vec<usize> = m.rail_parts().iter().map(|s| s.items.len()).collect();
        assert_eq!(rail, [1, 1, 3, 2]);
    }

    #[test]
    fn what_was_made_since_goes_last_and_what_is_gone_goes() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Casper", "pinned": 5},
            {"id": "a_2", "name": "Hermes"},
            {"id": "a_3", "name": "Juniper"},
            {"id": "a_5", "name": "Nook", "group": "g_1"},
            {"id": "a_6", "name": "Old", "hidden": 3},
            {"id": "a_7", "name": "Mira"}
        ]}));
        assert_eq!(ids(&m, SidePart::List), ["c_1", "a_3", "a_7"]);
    }

    #[test]
    fn before_the_core_groups_them_the_pinned_come_first_then_group_chats_then_agents() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "One"}, {"id": "a_2", "name": "Two", "pinned": 1}, {"id": "a_3", "name": "Three", "hidden": 1}
        ]}));
        m.take(&json!({"t": "core.data", "what": "conversations", "items": [
            {"id": "c_2", "agent": "", "title": "Money", "room": {"name": "Money", "members": ["a_1", "a_2"]}},
            {"id": "c_1", "agent": "", "title": "Code", "room": {"name": "Code", "members": ["a_1", "a_2"]}}
        ]}));
        assert_eq!(ids(&m, SidePart::Pinned), ["a_2"]);
        assert_eq!(ids(&m, SidePart::List), ["c_2", "c_1", "a_1"]);
        assert_eq!(ids(&m, SidePart::Hidden), ["a_3"]);
    }

    #[test]
    fn a_row_dropped_under_another_takes_its_place_in_the_whole_order() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        // Shop over Money, at the top of the list.
        let f = m.drop_item("a_4", &Drop::Before("c_1".into()));
        assert_eq!(f.len(), 1);
        assert_eq!(
            order_sent(&f),
            ["a_1", "a_4", "c_1", "a_2", "a_3", "a_5", "c_2", "a_6"]
        );
        assert_eq!(ids(&m, SidePart::List), ["a_4", "c_1", "a_3"]);
        // Money under Juniper. Hermes, waiting, keeps its place to go back to.
        let f = m.drop_item("c_1", &Drop::After("a_3".into()));
        assert_eq!(
            order_sent(&f),
            ["a_1", "a_4", "a_2", "a_3", "c_1", "a_5", "c_2", "a_6"]
        );
        assert_eq!(ids(&m, SidePart::List), ["a_4", "a_3", "c_1"]);
        // Dropped where it is, nothing is sent.
        assert!(m.drop_item("c_1", &Drop::After("a_3".into())).is_empty());
        assert!(m.drop_item("c_1", &Drop::Before("c_1".into())).is_empty());
    }

    #[test]
    fn a_row_dropped_beside_one_in_a_folder_goes_in_and_out_again() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        // Money between Nook and House, in Home.
        let f = m.drop_item("c_1", &Drop::After("a_5".into()));
        assert_eq!(f[0]["name"], "room.update");
        assert_eq!(f[0]["args"], json!({"conversation": "c_1", "group": "g_1"}));
        assert_eq!(
            order_sent(&f),
            ["a_1", "a_2", "a_3", "a_4", "a_5", "c_1", "c_2", "a_6"]
        );
        assert_eq!(ids(&m, home()), ["a_5", "c_1", "c_2"]);
        // Nook out of it, over Juniper.
        let f = m.drop_item("a_5", &Drop::Before("a_3".into()));
        assert_eq!(f[0]["name"], "agent.group");
        assert_eq!(f[0]["args"], json!({"agent": "a_5", "group": ""}));
        assert_eq!(ids(&m, SidePart::List), ["a_5", "a_3", "a_4"]);
        assert_eq!(m.agent("a_5").unwrap().group, None);
    }

    #[test]
    fn a_row_dropped_on_a_folder_goes_in_it_last() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        let f = m.drop_item("a_3", &Drop::Folder("g_1".into()));
        assert_eq!(f[0]["args"], json!({"agent": "a_3", "group": "g_1"}));
        assert_eq!(ids(&m, home()), ["a_5", "c_2", "a_3"]);
        assert_eq!(
            order_sent(&f),
            ["a_1", "c_1", "a_2", "a_4", "a_5", "c_2", "a_3", "a_6"]
        );
        assert_eq!(m.agent("a_3").unwrap().group.as_deref(), Some("g_1"));
        // Already last in it: nothing changes.
        assert!(m.drop_item("a_3", &Drop::Folder("g_1".into())).is_empty());
    }

    #[test]
    fn in_an_empty_folder_it_goes_last_in_the_order() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        let mut sections = m.sidebar.as_ref().clone();
        sections.insert(
            4,
            Section {
                part: SidePart::Folder {
                    id: "g_2".into(),
                    name: "Later".into(),
                    collapsed: true,
                },
                items: Vec::new(),
            },
        );
        m.sidebar = Rc::new(sections);
        let f = m.drop_item("c_1", &Drop::Folder("g_2".into()));
        assert_eq!(f[0]["args"]["group"], "g_2");
        assert_eq!(order_sent(&f).last().map(String::as_str), Some("c_1"));
    }

    #[test]
    fn tiles_move_among_tiles_and_who_waits_and_the_hidden_dont_move() {
        let mut m = Model::new();
        m.take(&json!({"t": "core.data", "what": "agents", "items": [
            {"id": "a_1", "name": "Casper", "pinned": 5}, {"id": "a_2", "name": "Ada", "pinned": 6}, {"id": "a_3", "name": "Juniper"}
        ]}));
        m.take(&json!({"t": "core.data", "what": "sidebar", "items": [
            {"kind": "pinned", "agents": ["a_1", "a_2"]},
            {"kind": "waiting", "agents": []},
            {"kind": "list", "items": [{"kind": "agent", "id": "a_3"}]},
            {"kind": "hidden", "agents": []},
            {"kind": "order", "items": ["a_1", "a_2", "a_3"]}
        ]}));
        // Dragged onto the one after it, it takes that one's place.
        assert_eq!(m.onto("a_1", "a_2"), Drop::After("a_2".into()));
        assert_eq!(m.onto("a_2", "a_1"), Drop::Before("a_1".into()));
        let to = m.onto("a_1", "a_2");
        let f = m.drop_item("a_1", &to);
        assert_eq!(order_sent(&f), ["a_2", "a_1", "a_3"]);
        assert_eq!(ids(&m, SidePart::Pinned), ["a_2", "a_1"]);
        // A tile isn't dropped in the list, nor a row among the tiles.
        assert!(m.drop_item("a_1", &Drop::Before("a_3".into())).is_empty());
        assert!(m.drop_item("a_3", &Drop::Before("a_1".into())).is_empty());
        let mut m = Model::new();
        with_sidebar(&mut m);
        assert!(m.drop_item("a_2", &Drop::Before("a_3".into())).is_empty());
        assert!(m.drop_item("a_6", &Drop::Before("a_3".into())).is_empty());
        assert!(m.drop_item("a_3", &Drop::Before("a_2".into())).is_empty());
    }

    #[test]
    fn an_agent_put_in_a_folder_from_its_menu_shows_there_at_once() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        let f = m.group_agent("a_3", Some("g_1"));
        assert_eq!(f["name"], "agent.group");
        assert_eq!(f["args"], json!({"agent": "a_3", "group": "g_1"}));
        assert_eq!(ids(&m, home()), ["a_5", "c_2", "a_3"]);
        m.group_agent("a_3", None);
        assert_eq!(ids(&m, SidePart::List), ["c_1", "a_4", "a_3"]);
    }

    #[test]
    fn a_reply_in_a_group_chat_that_isnt_open_marks_it_until_it_opens() {
        let mut m = Model::new();
        with_sidebar(&mut m);
        m.synced = true;
        m.take(&json!({"t": "event", "seq": 4, "kind": "message.created", "conversation": "c_2",
            "message": {"id": "msg_1", "conversation": "c_2", "role": "agent", "agent": "a_3", "at": 5, "blocks": []}}));
        assert!(m.room_unread("c_2"));
        assert!(
            !m.unread.contains("a_3"),
            "the group chat is marked, not its agent"
        );
        m.show_room("c_2");
        assert!(!m.room_unread("c_2"));
    }
}
