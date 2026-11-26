import Testing
@testable import Loom

/// The card a routine leaves in the chat when it turns off on its own (PLAN.md,
/// "Routines"), as desktop/src/model/routine_off.rs reads it: what it says, Turn on, and
/// its states.
@MainActor
@Suite struct RoutineOffTests {
    /// A routine's card, as the instance stores it, in a message of `role`.
    func card(_ state: String, agent: String = "", why: String = "the shop was down", role: String = "loom") -> JSON {
        var c: [String: JSON] = ["id": "msg_9", "type": "routine_off", "title": "Prices", "routine": "rt_1", "why": JSON(why), "state": JSON(state)]
        if agent.isEmpty { c["failures"] = 5 } else { c["agent"] = JSON(agent) }
        return ["id": "msg_9", "conversation": "c_1", "role": JSON(role), "at": 1_795_187_000_000,
                "blocks": [["kind": "card", "card": .object(c)]]]
    }

    func shown(_ m: Model) throws -> RoutineOffShown {
        let msg = try #require(m.shown.first { $0.id == "msg_9" })
        return m.routineOffShown(try #require(msg.routineOff))
    }

    /// Answers `cmd` as the core does.
    func answer(_ m: Model, _ cmd: JSON, error: String? = nil) {
        var f: [String: JSON] = ["t": "result", "id": cmd["id"], "ok": JSON(error == nil)]
        if let error { f["error"] = ["code": "not_found", "message": JSON(error)] }
        m.take(.object(f))
    }

    @Test func offAfterFailuresSaysWhyAndTurnsOn() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1"), card("off")])
        #expect(m.shown.map(\.id) == ["m_1", "msg_9"])
        #expect(try shown(m) == .off(why: "Failed 5 times in a row: the shop was down", sending: false, error: ""))
        let cmd = m.turnRoutineOn("rt_1")
        #expect(cmd["name"] == "routine.put")
        #expect(cmd["args"] == ["id": "rt_1", "on": true])
        #expect(try shown(m) == .off(why: "Failed 5 times in a row: the shop was down", sending: true, error: ""))
        // The card as it is now, then the answer.
        came(m, [message("m_1", in: "c_1"), card("on")])
        answer(m, cmd)
        #expect(try shown(m) == .on)
    }

    @Test func pausedByAnAgentNamesIt() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [card("off", agent: "a_1", why: "the sale is over")])
        #expect(try shown(m) == .off(why: "Builder paused it: the sale is over", sending: false, error: ""))
    }

    @Test func aRefusedTurnOnSaysWhyOnTheCard() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [card("off")])
        let cmd = m.turnRoutineOn("rt_1")
        answer(m, cmd, error: "There's no agent a_1.")
        #expect(try shown(m) == .off(why: "Failed 5 times in a row: the shop was down", sending: false, error: "There's no agent a_1."))
        _ = m.turnRoutineOn("rt_1")
        #expect(try shown(m) == .off(why: "Failed 5 times in a row: the shop was down", sending: true, error: ""))
    }

    @Test func onElsewhereOrDeletedGoesQuiet() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [card("on")])
        #expect(try shown(m) == .on)
        came(m, [card("deleted")])
        #expect(try shown(m) == .deleted)
    }

    @Test func onlyALoomMessageIsOne() {
        #expect(!isRoutineOffLine(card("off", role: "agent")))
        #expect(isRoutineOffLine(card("off")))
    }
}
