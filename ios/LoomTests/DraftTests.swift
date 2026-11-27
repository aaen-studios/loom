import Foundation
import Testing
@testable import Loom

/// A draft card (PLAN.md, "Drafts"), as desktop/src/model/draft_card.rs reads it: the
/// whole email, how it stands, and its answers.
@MainActor
@Suite struct DraftTests {
    /// An agent's reply with a draft card in `state`.
    func reply(_ state: String, extra: [String: JSON] = [:]) -> JSON {
        var c: [String: JSON] = ["id": "draft_1", "type": "draft", "title": "Draft email", "kind": "email", "service": "gmail",
                                 "thread": "Partnership", "answers": "Can we meet?",
                                 "rows": [["key": "from", "label": "From", "value": "sam@example.com", "edit": false],
                                          ["key": "to", "label": "To", "value": "hello@example.com", "edit": true],
                                          ["key": "subject", "label": "Subject", "value": "Re: Partnership", "edit": true]],
                                 "body": "Hi Mira,\nThursday works.", "state": JSON(state)]
        for (k, v) in extra { c[k] = v }
        return ["id": "msg_9", "conversation": "c_1", "role": "agent", "agent": "a_1", "at": 1_795_187_000_000,
                "blocks": [["kind": "card", "card": .object(c)]]]
    }

    func card(_ m: Model) throws -> Card {
        let msg = try #require(m.shown.first { $0.id == "msg_9" })
        return try #require(msg.blocks.compactMap(\.card).first)
    }

    @Test func readsTheWholeDraft() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("draft")])
        let c = try card(m)
        let d = try #require(c.draft)
        #expect(c.message == "msg_9")
        #expect(d.rows.map(\.label) == ["From", "To", "Subject"])
        #expect(d.serviceName == "Gmail" && d.mark == "gmail")
        #expect(d.shown(now: 0) == .open)
        #expect(d.rowsShown(editing: true).map(\.key) == ["from", "to", "cc", "subject"])
    }

    @Test func saysHowItWent() {
        let going = Draft(reply("sending", extra: ["at": 1000, "until": 6000])["blocks"][0]["card"])
        #expect(going.shown(now: 1500) == .going(what: "Re: Partnership", to: "hello@example.com", at: 1000, left: 5))
        #expect(going.shown(now: 6000) == .sending(what: "Re: Partnership", to: "hello@example.com"))
        let failed = Draft(reply("failed", extra: ["why": "Bad address"])["blocks"][0]["card"])
        #expect(failed.shown(now: 0) == .failed(what: "Re: Partnership", to: "hello@example.com", why: "Bad address"))
        #expect(Draft(reply("rejected")["blocks"][0]["card"]).shown(now: 0) == .rejected(what: "Re: Partnership"))
    }

    @Test func answersGoAsDraftAnswerAndARefusalShows() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("draft")])
        let cmd = m.answerDraft(message: "msg_9", card: "draft_1", answer: "send", edits: ["body": "Hi"])
        #expect(cmd["name"] == "draft.answer")
        #expect(cmd["args"] == ["message": "msg_9", "card": "draft_1", "answer": "send", "edits": ["body": "Hi"]])
        #expect(try card(m).sending)
        m.take(["t": "result", "id": cmd["id"], "ok": false, "error": ["code": "not_now", "message": "Answered already."]])
        #expect(try card(m).error == "Answered already.")
        #expect(try !card(m).sending)
        let reject = m.answerDraft(message: "msg_9", card: "draft_1", answer: "reject", note: " Too formal. ")
        #expect(reject["args"]["note"] == "Too formal.")
    }

    @Test func laterHasThreeTimes() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(secondsFromGMT: 3600)!
        let now = cal.date(from: DateComponents(year: 2026, month: 11, day: 27, hour: 22, minute: 30))!
        let times = laterTimes(from: now, calendar: cal)
        #expect(times.map(\.words) == ["In 1 hour", "In 3 hours", "Tomorrow at 08:00"])
        let eight = cal.date(from: DateComponents(year: 2026, month: 11, day: 28, hour: 8))!
        #expect(times[2].at == Int(eight.timeIntervalSince1970 * 1000))
    }
}
