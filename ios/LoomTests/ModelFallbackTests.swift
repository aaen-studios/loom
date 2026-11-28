import Testing
@testable import Loom

/// Loom's card under a reply whose chat went to another model (PLAN.md, "A model
/// fallback"), as desktop/src/model/model_fallback.rs reads it: what it says, Switch back,
/// New chat, and what it says once done.
@MainActor
@Suite struct ModelFallbackTests {
    /// The card, as the instance stores it.
    func card(_ state: String, said: String = "") -> JSON {
        let c: JSON = ["id": "msg_9", "type": "model_fallback", "title": "This chat is on Sonnet 5.5 now.", "agent": "a_1",
                       "model": "sonnet", "model_name": "Sonnet 5.5", "from": "default", "from_name": "Opus 5.5",
                       "state": JSON(state), "said": JSON(said)]
        return ["id": "msg_9", "conversation": "c_1", "role": "loom", "at": 1_795_187_000_000,
                "blocks": [["kind": "card", "card": c]]]
    }

    func shown(_ m: Model) throws -> FallbackShown {
        let msg = try #require(m.shown.first { $0.id == "msg_9" })
        return m.fallbackShown(try #require(msg.modelFallback))
    }

    func answer(_ m: Model, _ cmd: JSON, error: String? = nil) {
        var f: [String: JSON] = ["t": "result", "id": cmd["id"], "ok": JSON(error == nil)]
        if let error { f["error"] = ["code": "not_found", "message": JSON(error)] }
        m.take(.object(f))
    }

    @Test func switchBackAndNewChatSendTheirCommands() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1"), card("fallback")])
        #expect(m.shown.map(\.id) == ["m_1", "msg_9"])
        #expect(try shown(m) == .on(title: "This chat is on Sonnet 5.5 now.", from: "Opus 5.5", going: nil, error: ""))
        let back = m.switchModelBack("c_1")
        #expect(back["name"] == "conversation.model_back")
        #expect(back["args"] == ["conversation": "c_1"])
        #expect(try shown(m) == .on(title: "This chat is on Sonnet 5.5 now.", from: "Opus 5.5", going: .back, error: ""))
        answer(m, back, error: "There's no conversation c_1.")
        #expect(try shown(m) == .on(title: "This chat is on Sonnet 5.5 now.", from: "Opus 5.5", going: nil, error: "There's no conversation c_1."))
        let fresh = m.newChatFrom("c_1", agent: "a_1")
        #expect(fresh["name"] == "conversation.create")
        #expect(fresh["args"] == ["agent": "a_1", "from": "c_1"])
        #expect(try shown(m) == .on(title: "This chat is on Sonnet 5.5 now.", from: "Opus 5.5", going: .newChat, error: ""))
    }

    @Test func onceDoneItSaysWhatHappened() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [card("back", said: "Back on Opus 5.5.")])
        #expect(try shown(m) == .done("Back on Opus 5.5."))
        came(m, [card("moved", said: "Moved to a new chat.")])
        #expect(try shown(m) == .done("Moved to a new chat."))
    }
}
