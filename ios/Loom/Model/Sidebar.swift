import Foundation

// The agent list as the desktop's sidebar shows it (PLAN.md, "The sidebar"), as the core
// groups it (core/sidebar.calx): the pinned agents as tiles, who's waiting on the
// person, one list of agents and group chats in the person's order with the folders in
// it, and the hidden agents. What a long press and a drag does is here too, as
// desktop/src/model/sidebar.rs does it: where the moved one lands, and what the instance
// is told, so every device shows the same order.

/// An agent or a group chat in the list, by its id (a group chat's is its conversation's).
enum SideItem: Hashable, Sendable {
    case agent(String)
    case room(String)

    var id: String {
        switch self {
        case .agent(let id), .room(let id): return id
        }
    }

    /// One of a section's `items`, or one of its `agents`, which are ids alone.
    init?(_ j: JSON) {
        if case .string(let id) = j {
            self = .agent(id)
            return
        }
        let id = j["id"].text
        guard !id.isEmpty else { return nil }
        self = j["kind"].text == "room" ? .room(id) : .agent(id)
    }
}

/// A part of the agent list.
struct SideSection: Equatable, Sendable {
    enum Part: Equatable, Sendable {
        case pinned, waiting, list, hidden
        case folder(id: String, name: String, collapsed: Bool)

        var folder: String? {
            if case .folder(let id, _, _) = self { return id }
            return nil
        }

        /// Whether it's the list or a folder, whose rows move among each other's.
        var listed: Bool {
            switch self {
            case .list, .folder: return true
            default: return false
            }
        }
    }
    var part: Part
    /// What's in it, in order.
    var items: [SideItem]

    init(part: Part, items: [SideItem] = []) {
        self.part = part
        self.items = items
    }

    init?(_ j: JSON) {
        switch j["kind"].text {
        case "pinned": part = .pinned
        case "waiting": part = .waiting
        case "list": part = .list
        case "hidden": part = .hidden
        case "group": part = .folder(id: j["id"].text, name: j["name"].text, collapsed: j["collapsed"].bool ?? false)
        default: return nil
        }
        let listed = j["items"].isNull ? j["agents"] : j["items"]
        items = listed.array.compactMap(SideItem.init)
    }

    /// Its agents' ids, in order.
    var agents: [String] {
        items.compactMap { if case .agent(let id) = $0 { return id } else { return nil } }
    }

    func has(_ id: String) -> Bool { items.contains { $0.id == id } }
}

/// Where a moved agent or group chat lands.
enum SideDrop: Equatable, Sendable {
    case before(String)
    case after(String)
    /// In a folder, last.
    case folder(String)
}

/// A row of the list's part: an agent or a group chat, or a folder's name.
enum SideRow: Hashable, Identifiable, Sendable {
    case item(SideItem)
    case folder(id: String, name: String, collapsed: Bool)

    var id: String {
        switch self {
        case .item(let it): return it.id
        case .folder(let id, _, _): return "folder:" + id
        }
    }
}

extension Model {
    /// The sidebar as the core grouped it, with the person's whole order.
    func takeSidebar(_ items: [JSON]) {
        sidebar = items.compactMap(SideSection.init)
        sideOrder = items.first { $0["kind"].text == "order" }?["items"].array.map(\.text) ?? []
    }

    /// The list to draw: as the core grouped it, without what's gone since, and with an
    /// agent or a group chat made since at the end. Before the core has grouped them, the
    /// pinned agents, then the group chats and the other agents as they're listed, then
    /// the hidden ones.
    var sideSections: [SideSection] {
        let rooms = conversations.compactMap(\.room)
        if sidebar.isEmpty {
            var pinned = SideSection(part: .pinned), list = SideSection(part: .list), hidden = SideSection(part: .hidden)
            list.items = (rooms.filter(\.everyone) + rooms.filter { !$0.everyone }).map { .room($0.id) }
            for a in agents {
                if a.hidden { hidden.items.append(.agent(a.id)) }
                else if a.pinned != nil { pinned.items.append(.agent(a.id)) }
                else { list.items.append(.agent(a.id)) }
            }
            return [pinned, SideSection(part: .waiting), list, hidden]
        }
        var out = sidebar
        for i in out.indices {
            out[i].items.removeAll { it in
                switch it {
                case .agent(let id): return agent(id) == nil
                case .room(let id): return !rooms.contains { $0.id == id }
                }
            }
        }
        let listed = Set(out.flatMap { $0.items.map(\.id) })
        let made = rooms.filter { !listed.contains($0.id) }.map { SideItem.room($0.id) }
            + agents.filter { !listed.contains($0.id) }.map { SideItem.agent($0.id) }
        if let i = out.firstIndex(where: { $0.part == .list }) { out[i].items += made }
        return out
    }

    /// The list's part as rows: its agents and group chats, then each folder's name with
    /// what's in it while it's open.
    var listRows: [SideRow] {
        var rows: [SideRow] = []
        for s in sideSections {
            switch s.part {
            case .list: rows += s.items.map { .item($0) }
            case .folder(let id, let name, let collapsed):
                rows.append(.folder(id: id, name: name, collapsed: collapsed))
                if !collapsed { rows += s.items.map { .item($0) } }
            default: break
            }
        }
        return rows
    }

    /// Every agent and group chat in the person's order, as `sidebar.order` takes them:
    /// the core's, with what was made since at the end.
    private func wholeOrder(_ sections: [SideSection]) -> [String] {
        let here = Set(sections.flatMap { $0.items.map(\.id) })
        var order = sideOrder.filter { here.contains($0) }
        for s in sections {
            for it in s.items where !order.contains(it.id) { order.append(it.id) }
        }
        return order
    }

    /// Where a row moved in the list lands, as SwiftUI's onMove gives it: the row at
    /// `from` to just before the one at `to`. After an agent or a group chat, it goes
    /// after it, in its folder or out of them; just under a folder's name, first in that
    /// folder, or in it when it's shut or empty.
    func rowDrop(from: Int, to: Int) -> (String, SideDrop)? {
        var rows = listRows
        guard rows.indices.contains(from), case .item(let it) = rows[from] else { return nil }
        rows.remove(at: from)
        let at = to > from ? to - 1 : to
        var next: SideItem?
        if rows.indices.contains(at), case .item(let n) = rows[at] { next = n }
        guard at > 0 else { return next.map { (it.id, .before($0.id)) } }
        switch rows[at - 1] {
        case .item(let prev): return (it.id, .after(prev.id))
        case .folder(let id, _, let collapsed):
            if !collapsed, let next, sideSections.first(where: { $0.part.folder == id })?.has(next.id) == true {
                return (it.id, .before(next.id))
            }
            return (it.id, .folder(id))
        }
    }

    /// The drop that puts the tile `id` in the place of the tile `onto`: after it when it
    /// came from before it, else before it.
    func onto(_ id: String, _ onto: String) -> SideDrop {
        let pinned = sideSections.first { $0.part == .pinned }?.items ?? []
        if let a = pinned.firstIndex(where: { $0.id == id }), let b = pinned.firstIndex(where: { $0.id == onto }), a < b {
            return .after(onto)
        }
        return .before(onto)
    }

    /// Moves the agent or group chat `id` to `to`. Tiles move among the tiles; the list's
    /// rows move in the list and in and out of folders. Who's waiting and the hidden ones
    /// don't move. It shows there at once, and what the instance is told comes back: the
    /// folder it's in now, when that changed, and the new order.
    func drop(_ id: String, _ to: SideDrop) -> [JSON] {
        let sections = sideSections
        func find(_ x: String) -> Int? { sections.firstIndex { $0.has(x) } }
        guard let from = find(id) else { return [] }
        var into: Int
        var place: Int
        switch to {
        case .before(let t), .after(let t):
            guard t != id, let k = find(t), let at = sections[k].items.firstIndex(where: { $0.id == t }) else { return [] }
            into = k
            if case .after = to { place = at + 1 } else { place = at }
        case .folder(let f):
            guard let k = sections.firstIndex(where: { $0.part.folder == f }) else { return [] }
            into = k
            place = sections[k].items.count
        }
        let (fromPart, intoPart) = (sections[from].part, sections[into].part)
        guard (fromPart == .pinned && intoPart == .pinned) || (fromPart.listed && intoPart.listed) else { return [] }
        let refolder = fromPart.folder != intoPart.folder
        let folder = intoPart.folder ?? ""
        let oldOrder = wholeOrder(sections)
        var next = sections
        guard let was = next[from].items.firstIndex(where: { $0.id == id }) else { return [] }
        let item = next[from].items.remove(at: was)
        if from == into && was < place { place -= 1 }
        next[into].items.insert(item, at: place)
        // Its place in the whole order: after the one it now follows, or else before the
        // one it now leads; alone in a folder, last.
        var order = oldOrder.filter { $0 != id }
        let list = next[into].items
        let after = place > 0 ? order.firstIndex(of: list[place - 1].id).map { $0 + 1 } : nil
        let before = place + 1 < list.count ? order.firstIndex(of: list[place + 1].id) : nil
        order.insert(id, at: after ?? before ?? order.count)
        var out: [JSON] = []
        if refolder {
            switch item {
            case .agent(let a):
                if let i = agents.firstIndex(where: { $0.id == a }) { agents[i].group = folder.isEmpty ? nil : folder }
                out.append(["t": "cmd", "id": JSON(self.id("app.group_agent")), "name": "agent.group", "args": ["agent": JSON(a), "group": JSON(folder)]])
            case .room(let r):
                out.append(["t": "cmd", "id": JSON(self.id("app.room_update")), "name": "room.update", "args": ["conversation": JSON(r), "group": JSON(folder)]])
            }
        }
        if order != oldOrder {
            out.append(["t": "cmd", "id": JSON(self.id("app.order")), "name": "sidebar.order", "args": ["items": JSON(order)]])
            sideOrder = order
        }
        if !out.isEmpty { sidebar = next }
        return out
    }

    /// The folders, each id and name, in order.
    var folders: [(id: String, name: String)] {
        sidebar.compactMap {
            if case .folder(let id, let name, _) = $0.part { return (id, name) }
            return nil
        }
    }

    /// The agents in the list's order, without the hidden ones.
    var shownAgents: [Agent] {
        sideSections.filter { $0.part != .hidden }.flatMap(\.agents).compactMap { self.agent($0) }
    }

    /// Puts an agent or a group chat in a folder, or with nil, out of them, from its long
    /// press. It shows there at once, last, and keeps its place in the person's order.
    func regroup(_ item: String, _ group: String?) -> JSON? {
        var sections = sideSections
        guard let it = sections.lazy.flatMap(\.items).first(where: { $0.id == item }) else { return nil }
        // A folder just made isn't in the list yet: it shows there when the core lists it.
        if let i = sections.firstIndex(where: { group == nil ? $0.part == .list : $0.part.folder == group }) {
            for k in sections.indices { sections[k].items.removeAll { $0.id == item } }
            sections[i].items.append(it)
            sidebar = sections
        }
        switch it {
        case .agent(let a):
            if let i = agents.firstIndex(where: { $0.id == a }) { agents[i].group = group }
            return ["t": "cmd", "id": JSON(id("app.group_agent")), "name": "agent.group", "args": ["agent": JSON(a), "group": JSON(group ?? "")]]
        case .room(let r):
            return ["t": "cmd", "id": JSON(id("app.room_update")), "name": "room.update", "args": ["conversation": JSON(r), "group": JSON(group ?? "")]]
        }
    }

    /// The folder an agent or a group chat is in, by its id.
    func folderOf(_ item: String) -> String? {
        sideSections.first { $0.has(item) }?.part.folder
    }
}
