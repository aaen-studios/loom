import Testing
@testable import Loom

/// What the person has read (protocol.md, "Unread"): the phone marks what's open read, as
/// the desktop does, so the desktop's dots clear.
@MainActor
@Suite struct ReadTests {
    /// The `conversation.read`s a fetch sends, each conversation and how far.
    func reads(_ m: Model) -> [String: Int] {
        var out: [String: Int] = [:]
        for f in m.fetches() where f["name"] == "conversation.read" {
            out[f["args"]["conversation"].text] = f["args"]["at"].int ?? 0
        }
        return out
    }

    /// Builder's chat and a task of its, another agent's thread with it, Scribe's chat, and
    /// the group chat Money, each with its newest message and what's read of it.
    func listed(_ m: Model, builder: Int = 50) {
        m.take(data("conversations", [
            ["id": "c_1", "agent": "a_1", "title": "", "main": true, "last": JSON(builder), "replied": JSON(builder), "read": 10],
            ["id": "c_7", "agent": "a_1", "title": "Fix the build", "task": "t_1", "last": 45, "read": 45],
            ["id": "c_8", "agent": "a_1", "title": "Side", "side": true, "last": 44],
            ["id": "c_19", "agent": "a_1", "title": "From Scribe", "asked_by": "a_2", "last": 60, "replied": 60],
            ["id": "c_2", "agent": "a_2", "title": "", "main": true, "last": 40, "read": 20],
            ["id": "c_9", "agent": "", "title": "Money", "room": ["name": "Money", "members": ["a_1", "a_2"]], "last": 70, "read": 30],
        ]))
    }

    @Test func anAgentsChatIsReadAsItOpensAndAsRepliesComeWhileItsOpen() {
        let m = caughtUp()
        listed(m)
        // Nothing while no chat is open.
        #expect(reads(m).isEmpty)
        m.select("a_1")
        // Its chat, up to its newest; not the threads beside it, its side questions, or
        // what's read already.
        #expect(reads(m) == ["c_1": 50])
        #expect(reads(m).isEmpty)
        // A reply comes while it's open.
        listed(m, builder: 80)
        #expect(reads(m) == ["c_1": 80])
        // Left for the agents list: what comes now stays unread.
        m.leaveChat()
        listed(m, builder: 90)
        #expect(reads(m).isEmpty)
        // Opened again: read.
        m.select("a_1")
        #expect(reads(m) == ["c_1": 90])
    }

    @Test func aThreadOrAGroupChatIsReadAsItShows() {
        let m = caughtUp()
        listed(m)
        m.select("a_1")
        _ = m.fetches()
        m.open(thread: "c_19")
        #expect(reads(m) == ["c_19": 60])
        m.open(room: "c_9")
        #expect(reads(m) == ["c_9": 70])
    }

    @Test func whatsReadIsQuietEvenWhereItsRefused() throws {
        let m = caughtUp()
        listed(m)
        m.select("a_2")
        let f = try #require(m.fetches().first { $0["name"] == "conversation.read" })
        // An instance from before it kept reads doesn't know the command: nothing is said.
        m.take(["t": "result", "id": f["id"], "ok": false, "error": ["code": "unknown_command", "message": "No such command."]])
        #expect(m.notice == nil)
    }
}
