import Testing
@testable import Loom

/// The ring in the chat's header and its card (PLAN.md, "One endless chat"), as
/// desktop/src/model.rs has them.
@MainActor
@Suite struct ContextTests {
    @Test func theRingWarmsAt70AndGoesRedAt90() {
        #expect(ContextFill(used: 82_000, size: 200_000).level == .quiet)
        #expect(ContextFill(used: 139_999, size: 200_000).level == .quiet)
        #expect(ContextFill(used: 140_000, size: 200_000).level == .warn)
        #expect(ContextFill(used: 180_000, size: 200_000).level == .bad)
    }

    @Test func itFillsNoFurtherThanFull() {
        let over = ContextFill(used: 250_000, size: 200_000)
        #expect(over.percent == 100)
        #expect(over.fraction == 1)
        #expect(ContextFill(used: -5, size: 200_000).percent == 0)
        #expect(ContextFill(used: 82_000, size: 200_000).percent == 41)
    }

    @Test func aWindowOfNothingIsntSaid() {
        #expect(ContextFill(["used": 10, "size": 0]) == nil)
        #expect(ContextFill(.null) == nil)
        let fill = ContextFill(["used": 82_000, "size": 200_000, "compacts": true, "agent": ""])
        #expect(fill == ContextFill(used: 82_000, size: 200_000, compacts: true))
    }

    @Test func tokensAreSaidShort() {
        #expect(ContextFill.short(900) == "900")
        #expect(ContextFill.short(16_400) == "16k")
        #expect(ContextFill.short(182_000) == "182k")
        #expect(ContextFill.short(1_000_000) == "1M")
        #expect(ContextFill.short(1_500_000) == "1.5M")
        #expect(ContextFill(used: 82_000, size: 200_000).amount == "82k of 200k")
        #expect(ContextFill(used: 82_000, size: 200_000).tip == "Context 41%, 82k of 200k")
    }

    @Test func whenItWasCompacted() {
        let minute = 60_000
        #expect(ContextFill.ago(30_000) == "just now")
        #expect(ContextFill.ago(5 * minute) == "5 min ago")
        #expect(ContextFill.ago(2 * 60 * minute) == "2 h ago")
        #expect(ContextFill.ago(30 * 60 * minute) == "yesterday")
        #expect(ContextFill.ago(3 * 24 * 60 * minute) == "3 days ago")
        #expect(ContextFill(used: 1, size: 2).line(now: 0) == "A fresh session starts at 80%")
        let now = 1_795_187_131_000
        #expect(ContextFill(used: 1, size: 2, compacted: now - 2 * 60 * minute).line(now: now) == "Compacted 2 h ago · a fresh session starts at 80%")
    }

    @Test func aGroupChatsCardSaysWhoseItIs() throws {
        let m = caughtUp()
        m.take(data("conversations", [
            ["id": "c_1", "agent": "a_1", "title": "", "context": ["used": 82_000, "size": 200_000, "compacts": true]],
            ["id": "c_9", "agent": "", "title": "Money", "room": ["name": "Money", "members": ["a_1", "a_2"]], "context": ["used": 10_000, "size": 200_000, "agent": "a_2"]],
        ]))
        m.select("a_1")
        let own = try #require(m.context)
        #expect(m.contextTitle(own) == "Context")
        #expect(m.compactNow()?["args"] == ["conversation": "c_1"])
        m.open(room: "c_9")
        let theirs = try #require(m.context)
        #expect(m.contextTitle(theirs) == "Scribe's context")
        #expect(m.freshSession()?["args"] == ["conversation": "c_9", "agent": "a_2"])
    }
}
