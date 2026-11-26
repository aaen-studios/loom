import Foundation
import Testing
@testable import Loom

// What the tests hand the model: frames as the core sends them (core/README.md), and a
// session on SampleCore, which answers as the core does, without it.

/// A model that's connected and caught up, with `agents` and their chats, each agent's
/// chat `c_` and its number (`a_1`'s is `c_1`).
@MainActor
func caughtUp(agents: [String] = ["a_1", "a_2"]) -> Model {
    let m = Model()
    m.take(["t": "core.conn", "state": "live"])
    m.take(["t": "welcome", "user": "u_1", "device": "d_4", "seq": 1])
    m.take(["t": "synced", "seq": 1])
    m.take(data("agents", agents.map { ["id": JSON($0), "name": JSON(name(of: $0)), "color": "mint", "acp_agent": "claude"] }))
    m.take(data("conversations", agents.map { ["id": JSON(chat(of: $0)), "agent": JSON($0), "title": ""] }))
    _ = m.fetches()
    return m
}

/// An agent's name in `caughtUp`, as the sample's are.
func name(of agent: String) -> String {
    ["a_1": "Builder", "a_2": "Scribe", "a_3": "Postmaster", "a_4": "Gardener"][agent] ?? agent
}

/// An agent's chat in `caughtUp`.
func chat(of agent: String) -> String {
    "c_" + agent.dropFirst(2)
}

/// A list the core gives for `core.get`.
func data(_ what: String, _ items: [JSON], conversation: String? = nil) -> JSON {
    var f: [String: JSON] = ["t": "core.data", "id": JSON("app.\(what)"), "what": JSON(what), "items": .array(items), "more": false]
    if let conversation { f["conversation"] = JSON(conversation) }
    return .object(f)
}

/// A stored message, with one text block.
func message(_ id: String, in conversation: String, role: String = "agent", agent: String = "a_1", text: String = "Words.") -> JSON {
    ["id": JSON(id), "conversation": JSON(conversation), "role": JSON(role), "agent": JSON(role == "agent" ? agent : ""), "from": JSON(role == "person" ? "d_4" : ""), "at": 1_795_187_000_000,
     "blocks": [["kind": "text", "text": JSON(text)]]]
}

/// The event that says a message was stored.
func created(_ m: JSON) -> JSON {
    ["t": "event", "kind": "message.created", "seq": 2, "at": 1_795_187_000_000, "conversation": m["conversation"], "message": m]
}

/// Shows `messages` as the open chat's, as the core gives them when the model asks.
@MainActor
func came(_ model: Model, _ messages: [JSON]) {
    model.take(data("messages", messages, conversation: model.conversation))
}

/// How each message the open chat shows came, by id.
@MainActor
func cameOf(_ model: Model) -> [String: Came] {
    Dictionary(model.shown.map { ($0.id, $0.came) }, uniquingKeysWith: { a, _ in a })
}

/// Waits on the main actor, while frames come, until `done` says so, for at most
/// `seconds`. Gives whether it did.
@MainActor
func eventually(within seconds: Double = 15, _ done: () -> Bool) async -> Bool {
    let end = Date.now.addingTimeInterval(seconds)
    while !done() {
        if Date.now > end { return false }
        try? await Task.sleep(for: .milliseconds(20))
    }
    return true
}

/// A session on SampleCore, once it's caught up and has its agents and chats.
@MainActor
func sampleSession() async throws -> Session {
    let session = Session(sample: true)
    try #require(await eventually { session.model.synced && session.model.agents.count == 4 && !session.model.conversations.isEmpty && !session.model.sidebar.isEmpty })
    return session
}

/// Opens `agent`'s chat in `session`, once its messages have come.
@MainActor
func openChat(_ agent: String, in session: Session) async throws {
    session.model.select(agent)
    session.flush()
    try #require(await eventually { session.model.messagesCame && !session.model.messages.isEmpty })
}
