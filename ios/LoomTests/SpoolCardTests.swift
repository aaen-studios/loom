import Testing
@testable import Loom

/// The card an agent's spool_connect puts in its reply (PLAN.md, "An agent connects a
/// spool"), as desktop/src/model/spool_card.rs reads it: what its foot says, Sign in, and
/// T3's, which is finished on a computer.
@MainActor
@Suite struct SpoolCardTests {
    /// A reply with a spool card for `entry`, as the instance stores it.
    func reply(_ state: String, entry: String = "home-assistant", said: String = "") -> JSON {
        let c: JSON = ["id": "spool_tc_9", "type": "spool_connect", "title": "Connect Home Assistant", "call": "tc_9",
                       "entry": JSON(entry), "name": "Home Assistant", "server": "ha", "address": "https://ha.example.com/mcp",
                       "agents": [["id": "a_1", "name": "Builder"]], "secrets": [["name": "HA_TOKEN", "label": "Token", "placeholder": ""]],
                       "state": JSON(state), "said": JSON(said), "tools": ["lights", "scenes"]]
        return ["id": "msg_9", "conversation": "c_1", "role": "agent", "at": 1_795_187_000_000,
                "blocks": [["kind": "card", "card": c]]]
    }

    func shown(_ m: Model) throws -> Card {
        let msg = try #require(m.shown.first { $0.id == "msg_9" })
        return try #require(msg.blocks.compactMap(\.card).first)
    }

    @Test func readsTheCardAndSaysHowItStands() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("asking")])
        let c = try shown(m)
        #expect(c.name == "Home Assistant" && c.agentNames == ["Builder"] && c.keys.map(\.name) == ["HA_TOKEN"] && c.message == "msg_9")
        #expect(spoolFoot(c) == .forAgents("For Builder"))
        #expect(spoolShortAddress(c.address) == "ha.example.com")
        came(m, [reply("connected")])
        #expect(spoolFoot(try shown(m)) == .connected(2))
        #expect(spoolConnectedLine(2) == "Connected · 2 tools")
        came(m, [reply("failed", said: "The address didn't answer.")])
        #expect(spoolFoot(try shown(m)) == .failed("The address didn't answer."))
    }

    @Test func t3IsFinishedOnAComputer() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("asking", entry: "t3")])
        #expect(spoolFoot(try shown(m)) == .computer)
    }

    @Test func signInSendsTheKeysAndARefusalShowsOnTheCard() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("asking")])
        let cmd = m.spoolSignIn(message: "msg_9", card: "spool_tc_9", server: "ha", secrets: ["HA_TOKEN": "t0ken"])
        #expect(cmd["name"] == "spool.connect")
        #expect(cmd["args"] == ["message": "msg_9", "card": "spool_tc_9", "secrets": ["HA_TOKEN": "t0ken"]])
        #expect(try shown(m).sending)
        m.take(["t": "result", "id": cmd["id"], "ok": false, "error": ["code": "bad_request", "message": "Fill in Token first."]])
        let c = try shown(m)
        #expect(!c.sending && c.error == "Fill in Token first.")
        #expect(spoolFoot(c) == .failed("Fill in Token first."))
    }
}
