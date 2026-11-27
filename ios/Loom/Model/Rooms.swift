import Foundation

// Group chats (PLAN.md, "Group chats"), as desktop/src/model/rooms.rs has them:
// conversations several of the person's agents take part in, where the instance picks
// one to answer each message. The model shows them and sends what the person changes;
// the picking is the instance's.

/// A group chat, as its conversation's `room` has it.
struct Room: Identifiable, Equatable, Sendable {
    /// Its conversation.
    var id: String
    var name: String
    /// Its agents, by id, in the order the person picked them.
    var members: [String]
    /// A thread colour, or "" for none.
    var color: String
    /// The member a message goes to when the router can't pick, or "" for the one that
    /// answered last.
    var defaultAgent: String
    /// What the person re-sent to another member, newest last.
    var corrections: [Correction]

    init(id: String, _ r: JSON) {
        self.id = id
        name = r["name"].text
        members = r["members"].array.map(\.text)
        color = r["color"].text
        defaultAgent = r["default"].text
        corrections = r["corrections"].array.map {
            Correction(id: $0["id"].text, text: $0["text"].text, to: $0["to"].text, from: $0["from"].text, at: $0["at"].int ?? 0)
        }
    }
}

/// A message the person re-sent to another member, which the router follows for
/// messages like it.
struct Correction: Identifiable, Equatable, Sendable {
    var id: String
    /// The start of the person's message.
    var text: String
    /// The member it should have gone to, and the one the router picked.
    var to: String
    var from: String
    var at: Int
}

extension Room {
    /// The fewest members a group chat has: the instance says so too.
    static let minMembers = 2
}

/// New group chat's form: what the person picked so far. Its name is the sheet's name
/// field.
struct RoomForm: Equatable, Sendable {
    var members: [String] = []
    var color = ""
}

/// What New group chat still needs before Make turns on, the first in the form's order.
enum MakeNeeds: Equatable, Sendable {
    /// A name.
    case name
    /// Two members at least.
    case members
}

/// What New group chat shows, as desktop/src/model/rooms.rs's `room_panel` has it while
/// one is made: the form's members and colour, and whether Make is on.
struct NewRoom: Equatable, Sendable {
    var members: [String]
    var color: String
    /// What it still needs before Make turns on, or nil once it can be made.
    var needs: MakeNeeds?
    /// A room.create from the form is waiting on the instance: Make does nothing, and
    /// looks as it does while off.
    var waiting: Bool

    /// Make is on: a name and two members at least, and no room.create waiting.
    var canMake: Bool { needs == nil && !waiting }
}

/// A room.create sent from here, until its answer comes or can't.
struct MakingRoom: Equatable, Sendable {
    /// Its command id.
    var id: String
    /// When it went, while Make waits on it; nil once it has waited `Model.makeWait`.
    /// Make is on again then, and an answer that comes later still shows the group chat.
    var since: Date?
}

/// What a group chat used (`room.summary`): its members' turns there and the router's
/// picks for it, in tokens.
struct RoomUsage: Equatable, Sendable {
    /// Its conversation.
    var room: String
    var tokensToday: Int
    var tokensMonth: Int
    var routerToday: Int
    var routerMonth: Int
}

/// Tokens used, as the desktop says them: "850 today · 12.4k this month".
func usageLine(_ today: Int, _ month: Int) -> String {
    func short(_ n: Int) -> String {
        if n < 1000 { return "\(n)" }
        if n < 1_000_000 { return String(format: "%.1fk", Double(n) / 1000) }
        return String(format: "%.1fM", Double(n) / 1_000_000)
    }
    return "\(short(today)) today · \(short(month)) this month"
}

/// How a reply in a group chat came to its agent: the line over it.
struct Routed: Equatable, Sendable {
    var agent: String
    /// router, mention, handoff, person or default.
    var by: String
    /// The line's words after the agent's name: the router's why, or what loom says.
    var said: String
    /// For a handoff, the agent that handed it on.
    var from: String

    /// A message's or a run's `routed`, with the agent that answered.
    static func of(_ v: JSON) -> Routed? {
        let r = v["routed"]
        guard case .object = r else { return nil }
        return Routed(agent: v["agent"].text, by: r["by"].text, said: r["said"].text, from: r["from"].text)
    }
}

extension Model {
    func roomNamed(_ id: String) -> Room? { conversations.first { $0.id == id }?.room }

    /// The group chat the chat shows, if it shows one.
    var roomOpen: Room? { room.flatMap { self.roomNamed($0) } }

    /// Shows a group chat. Its default member, or else its first, is the agent the
    /// screen's agent parts are about meanwhile.
    func open(room id: String) {
        guard let r = roomNamed(id) else { return }
        guard room != id else {
            // Opened again from the agents list: afresh (`leaveChat`).
            reopenRoom()
            return
        }
        let lead = [r.defaultAgent].first { agent($0) != nil } ?? r.members.first { agent($0) != nil }
        select(lead)
        room = id
        messages = []
        messagesCame = false
        replying = nil
        chatOpened(unreadFrom: nil)
        dirty.insert(.messages)
    }

    /// The group chat's other members a reply could go to instead of `agent`.
    func resendTo(_ agent: String) -> [Agent] {
        (roomOpen?.members ?? []).filter { $0 != agent }.compactMap { self.agent($0) }
    }

    /// How long a room.create from New group chat waits for its answer before Make turns
    /// on again, in seconds.
    static let makeWait: TimeInterval = 15

    /// Starts New group chat's form: no one in it and no colour.
    func startRoomForm() {
        roomForm = RoomForm()
    }

    /// What New group chat shows, with `name` what its name field holds.
    func newRoom(name: String) -> NewRoom {
        let needs: MakeNeeds?
        if name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            needs = .name
        } else if roomForm.members.count < Room.minMembers {
            needs = .members
        } else {
            needs = nil
        }
        return NewRoom(members: roomForm.members, color: roomForm.color, needs: needs, waiting: makeWaiting)
    }

    /// A room.create from here is waiting on its answer, so Make does nothing.
    var makeWaiting: Bool { makingRoom?.since != nil }

    /// Make turns on again once a room.create has waited `makeWait` with no answer, so
    /// the person can try again. Gives whether it turned on.
    @discardableResult
    func makeTimedOut(now: Date) -> Bool {
        guard let since = makingRoom?.since, now.timeIntervalSince(since) >= Model.makeWait else { return false }
        makingRoom?.since = nil
        return true
    }

    /// Puts `agent` in New group chat's form, or takes it out.
    func toggleFormMember(_ agent: String) {
        if let i = roomForm.members.firstIndex(of: agent) {
            roomForm.members.remove(at: i)
        } else {
            roomForm.members.append(agent)
        }
    }

    func setFormColor(_ color: String) {
        roomForm.color = color
    }

    /// Makes the group chat in New group chat's form, named `name`, once Make is on:
    /// never twice for one tap and the next while the first waits. The chat shows it
    /// once it's made.
    func makeFormRoom(name: String, now: Date = .now) -> JSON? {
        guard newRoom(name: name).canMake else { return nil }
        let cid = id("app.room_create")
        makingRoom = MakingRoom(id: cid, since: now)
        return ["t": "cmd", "id": JSON(cid), "name": "room.create", "args": ["name": JSON(name.trimmingCharacters(in: .whitespacesAndNewlines)), "members": JSON(roomForm.members), "color": JSON(roomForm.color)]]
    }

    /// room.create's answer: the group chat made, or why it wasn't. An answer to one sent
    /// before the last is the last's no more.
    func roomCreateAnswered(_ f: JSON) {
        guard f["id"].text == makingRoom?.id else { return }
        guard f["error"].isNull else {
            makingRoom = nil
            notice = f["error"]["message"].string ?? f["error"]["code"].text
            return
        }
        roomMade(f)
    }

    /// A room.create that will never be answered: `id`'s, which didn't go for want of a
    /// connection, or with nil whatever was sent on a connection that went. Make is on
    /// again.
    func roomCreateLost(_ id: String?) {
        if id == nil || id == makingRoom?.id { makingRoom = nil }
    }

    /// A group chat made from here: the chat shows it.
    private func roomMade(_ f: JSON) {
        makingRoom = nil
        roomForm = RoomForm()
        let id = f["value"]["conversation"]["id"].text
        guard !id.isEmpty else { return }
        if roomNamed(id) == nil {
            conversations.insert(Conversation(f["value"]["conversation"]), at: 0)
        }
        open(room: id)
        dirty.insert(.conversations)
        dirty.insert(.sidebar)
    }

    /// Changes a group chat: `args` as room.update takes them.
    func updateRoom(_ room: String, _ args: [String: JSON]) -> JSON {
        var a = args
        a["conversation"] = JSON(room)
        return ["t": "cmd", "id": JSON(id("app.room_update")), "name": "room.update", "args": .object(a)]
    }

    /// Puts `agent` in group chat `room`, or takes it out: the room.update, or nil when
    /// it would leave fewer than `Room.minMembers`.
    func toggleMember(_ agent: String, in room: Room) -> JSON? {
        var members = room.members
        if let i = members.firstIndex(of: agent) {
            guard members.count > Room.minMembers else { return nil }
            members.remove(at: i)
        } else {
            members.append(agent)
        }
        return updateRoom(room.id, ["members": JSON(members)])
    }

    /// Sends the person's message a reply answered to another member, which the router
    /// keeps as a correction.
    func resend(_ message: String, to agent: String) -> JSON {
        ["t": "cmd", "id": JSON(id("app.room_resend")), "name": "room.resend", "args": ["message": JSON(message), "agent": JSON(agent)]]
    }

    /// Asks what the group chat `room` used, for its settings.
    func askRoomUsage(_ room: String) -> JSON {
        if roomUsage?.room != room { roomUsage = nil }
        return ["t": "cmd", "id": JSON(id("app.room_summary:\(room)")), "name": "room.summary", "args": ["conversation": JSON(room)]]
    }

    /// What a group chat used, as room.summary answered. Its id is
    /// app.room_summary:ROOM_N, N the model's own count.
    func roomUsageCame(_ f: JSON) {
        let rest = f["id"].text.dropFirst("app.room_summary:".count)
        guard let cut = rest.lastIndex(of: "_") else { return }
        let v = f["value"]
        roomUsage = RoomUsage(room: String(rest[..<cut]), tokensToday: v["tokens_today"].int ?? 0, tokensMonth: v["tokens_month"].int ?? 0, routerToday: v["router_today"].int ?? 0, routerMonth: v["router_month"].int ?? 0)
    }

    /// Forgets one of a group chat's corrections.
    func forget(_ correction: String, in room: String) -> JSON {
        ["t": "cmd", "id": JSON(id("app.room_forget")), "name": "room.forget", "args": ["conversation": JSON(room), "correction": JSON(correction)]]
    }

    /// Deletes a group chat the person made.
    func deleteRoom(_ room: String) -> JSON {
        ["t": "cmd", "id": JSON(id("app.room_delete")), "name": "conversation.delete", "args": ["conversation": JSON(room)]]
    }
}
