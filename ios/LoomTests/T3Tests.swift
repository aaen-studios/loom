import Foundation
import Testing
@testable import Loom

/// T3 Code's threads on the phone (PLAN.md, "T3 threads"), as desktop/src/model/t3.rs's
/// tests have them: what a thread's card shows, what its buttons send, the start sheet,
/// and which messages the instance follows.
@MainActor
@Suite struct T3Tests {
    func card(_ v: JSON) -> ThreadCard { ThreadCard(v) }

    func answer(_ m: Model, _ cmd: JSON?, _ f: [String: JSON]) {
        var frame = f
        frame["t"] = "result"
        frame["id"] = cmd?["id"] ?? .null
        frame["ok"] = JSON(f["error"] == nil)
        m.take(.object(frame))
    }

    @Test func aCardReadsItsState() {
        let working = card(["thread": "mcp:1", "server": "t3", "title": "Fix the flaky sign-in test", "project": "loom", "model": "Opus 5.5", "effort": "high", "state": "working",
                            "step": "Running tests/protocol/signin.calx", "started": 1_000, "url": "https://pc/env/mcp:1", "at": 1])
        #expect(working.strand == .waves)
        #expect(working.sub == "T3 · loom · Opus 5.5 · high")
        #expect(working.side(241_000) == "4 m")
        #expect(working.quiet == .step("Running tests/protocol/signin.calx"))
        #expect(working.edge?.strand == .waves && working.edge?.opacity == 0.35)
        #expect(working.buttons.isEmpty && working.chips.isEmpty && working.moving)

        let done = card(["thread": "mcp:1", "title": "T", "project": "loom", "model": "Opus 5.5", "state": "done", "step": "git push", "started": 1_000, "ended": 1_081_000,
                         "prs": [["number": 295, "url": "https://github.com/a/b/pull/295"]]])
        #expect(done.strand == .straight)
        #expect(done.side(5) == "18 m")
        #expect(done.quiet == nil)
        #expect(done.chips.map(\.label) == ["PR #295"])
        #expect(done.chips.map(\.url) == ["https://github.com/a/b/pull/295"])
        #expect(done.edge == nil)
        #expect(done.sub == "T3 · loom · Opus 5.5")

        let failed = card(["thread": "mcp:1", "state": "failed", "reason": "Usage limit reached · resets 15:30", "started": 1])
        #expect(failed.strand == .sags)
        #expect(failed.side(9) == "Failed")
        #expect(failed.quiet == .reason("Usage limit reached · resets 15:30"))
        #expect(failed.buttons == [ThreadButton(label: "Try again", act: .retry, main: false)])
        #expect(!failed.moving)
    }

    @Test func aWaitingCardOffersItsAnswers() {
        let q = card(["thread": "mcp:1", "state": "waiting", "ask": ["kind": "question", "request": "r1", "text": "Keep the retry, or make the test wait for the port instead?",
                                                                    "choices": ["Wait for the port", "Keep the retry"]]])
        #expect(q.strand == .knots)
        #expect(q.side(0) == "Asks you")
        #expect(q.asks == "Keep the retry, or make the test wait for the port instead?")
        #expect(q.edge?.strand == .knots && q.edge?.opacity == 0.6)
        let b = q.buttons
        #expect(b.count == 2)
        #expect(b[0].main && !b[1].main)
        #expect(b[0].act == .answer("Wait for the port"))
        #expect(!q.inT3)

        let a = card(["thread": "mcp:1", "state": "waiting", "ask": ["kind": "approval", "request": "ap1", "text": "Run rm -rf build", "choices": ["Approve", "Decline"]]])
        #expect(a.buttons.map(\.label) == ["Approve", "Decline"])
        #expect(a.buttons.map(\.main) == [true, false])
        #expect(a.buttons[0].act == .decide("accept"))

        // One loom couldn't answer: Open, to approve in T3.
        let inT3 = card(["thread": "mcp:1", "state": "waiting", "ask": ["kind": "approval", "request": "ap1", "text": "Run rm -rf build", "in_t3": true]])
        #expect(inT3.inT3)
        #expect(inT3.buttons == [ThreadButton(label: "Open", act: .open, main: true)])
        // An ask on a card that isn't waiting is left out.
        #expect(card(["state": "done", "ask": ["kind": "question"]]).ask == nil)
    }

    @Test func saysHowLong() {
        #expect(took(45_000) == "45 s")
        #expect(took(60_000) == "1 m")
        #expect(took(3_600_000) == "1 h")
        #expect(took(3_900_000) == "1 h 5 m")
        #expect(took(-5) == "0 s")
    }

    @Test func aMessagesThreadsComeInTheOrderItsWordsLinkThem() {
        let m: JSON = ["blocks": [["kind": "text", "text": "First [b](t3-thread://v2/env/mcp:b), then [a](t3-thread://v2/env/mcp:a)."]],
                       "threads": ["mcp:a": ["thread": "mcp:a", "state": "done"], "mcp:b": ["thread": "mcp:b", "state": "working"],
                                   "mcp:c": ["thread": "mcp:c", "state": "working", "started": 5], "mcp:d": ["thread": "mcp:d", "missing": true]]]
        #expect(threadsOf(m).map(\.thread) == ["mcp:b", "mcp:a", "mcp:c"])
        #expect(threadsOf([:]).isEmpty)
        #expect(linkThread("t3-thread://v2/env/mcp:a") == "mcp:a")
        #expect(linkThread("https://x/y") == nil)
    }

    var formAnswer: JSON {
        ["servers": ["t3-pc", "t3-vps"], "server": "t3-pc", "projects": [["id": "p1", "title": "loom"], ["id": "p2", "title": "calcite"]],
         "providers": [["id": "claudeAgent", "name": "Claude", "models": [["id": "claude-opus-5-5", "name": "Opus 5.5", "efforts": ["low", "medium", "high", "xhigh", "max"], "effort": "high"],
                                                                         ["id": "claude-haiku-5-5", "name": "Haiku 5.5", "efforts": [], "effort": ""]]],
                       ["id": "codex", "name": "Codex", "models": [["id": "gpt-6-luna", "name": "GPT-6 Luna", "efforts": ["low", "medium", "high", "xhigh"], "effort": "medium"]]]]]
    }

    @Test func theStartFormPicksAndKeeps() {
        var f = StartForm()
        f.take(formAnswer)
        #expect(f.showsServer)
        #expect([f.project, f.provider, f.model, f.effort] == ["p1", "claudeAgent", "claude-opus-5-5", "high"])
        #expect(f.efforts.count == 5)
        #expect(f.efforts[3].id == "xhigh" && f.efforts[3].label == "Extra high")
        #expect(f.modelLabel == "Opus 5.5 · Claude")
        #expect(f.projectLabel == "loom")
        // An effort the next model has stays; one it hasn't becomes its own.
        f.effort = "xhigh"
        f.pick(provider: "codex", model: "gpt-6-luna")
        #expect(f.effort == "xhigh")
        f.effort = "max"
        f.pick(provider: "claudeAgent", model: "claude-opus-5-5")
        #expect(f.effort == "max")
        f.pick(provider: "codex", model: "gpt-6-luna")
        #expect(f.effort == "medium")
        f.pick(provider: "claudeAgent", model: "claude-haiku-5-5")
        #expect(f.effort.isEmpty && f.efforts.isEmpty)
        // What was picked stays when the form's asked again.
        f.project = "p2"
        f.take(formAnswer)
        #expect(f.project == "p2" && f.model == "claude-haiku-5-5")
        #expect(!f.canStart("  ") && f.canStart("Tidy the CI logs."))
        f.starting = true
        #expect(!f.canStart("Tidy the CI logs."))
        var one = StartForm()
        var answer = formAnswer
        if case .object(var o) = answer {
            o["servers"] = ["t3"]
            answer = .object(o)
        }
        one.take(answer)
        #expect(!one.showsServer)
    }

    @Test func theModelSendsWhatTheCardsAndTheFormAsk() throws {
        let m = caughtUp()
        m.select("a_1")
        let q = card(["thread": "mcp:1", "state": "waiting", "ask": ["kind": "question", "request": "r1", "text": "Q", "choices": ["A", "B"]]])
        let sent = try #require(m.t3Act(q, .answer("A"), label: "A"))
        #expect(sent["name"] == "t3.answer")
        #expect(sent["args"] == ["thread": "mcp:1", "answer": "A"])
        // Not again while it's on its way.
        #expect(m.t3Act(q, .answer("B"), label: "B") == nil)
        #expect(m.t3.busy["mcp:1"] == "A")
        answer(m, sent, ["value": [:]])
        #expect(m.t3.busy.isEmpty)
        let d = try #require(m.t3Act(q, .decide("accept"), label: "Approve"))
        #expect(d["args"] == ["thread": "mcp:1", "decision": "accept"])
        answer(m, d, ["error": ["code": "unavailable", "message": "T3 didn't take the answer"]])
        #expect(m.notice == "T3 didn't take the answer")
        #expect(m.t3Act(q, .open, label: "Open") == nil)
        #expect(m.t3Act(q, .retry, label: "Try again")?["name"] == "t3.retry")

        // The form.
        let ask = m.openT3Form()
        #expect(ask["name"] == "t3.form")
        #expect(m.t3.form?.loading == true)
        answer(m, ask, ["value": formAnswer])
        let start = try #require(m.t3Start("Tidy the CI logs.\n"))
        #expect(start["name"] == "t3.start")
        #expect(start["args"]["task"] == "Tidy the CI logs.")
        #expect(start["args"]["effort"] == "high")
        #expect(start["args"]["conversation"] == "c_1")
        #expect(m.t3Start("again") == nil)
        answer(m, start, ["error": ["code": "bad_request", "message": "Pick a project."]])
        #expect(m.t3.form?.error == "Pick a project.")
        let again = try #require(m.t3Start("Tidy the CI logs."))
        answer(m, again, ["value": ["thread": "mcp:new1"]])
        #expect(m.t3.form == nil)

        // The + menu has "Start a T3 thread" once the person has a T3 server.
        #expect(!m.plusGroups("").joined().contains(.startT3))
        let servers = m.askT3Servers()
        #expect(servers["name"] == "t3.servers")
        answer(m, servers, ["value": ["servers": ["t3"]]])
        #expect(m.t3.servers == ["t3"])
        #expect(m.plusGroups("").last?.last == .startT3)
        #expect(m.plusGroups("t3 thr") == [[.startT3]])
    }

    @Test func theShownChatSaysWhichMessagesHaveThreads() {
        let m = caughtUp()
        m.select("a_1")
        #expect(m.t3Fetches().isEmpty, "nothing to say with no threads")
        func with(_ id: String) -> JSON {
            ["id": JSON(id), "conversation": "c_1", "role": "agent", "agent": "a_1", "at": 1, "blocks": [],
             "threads": ["mcp:1": ["thread": "mcp:1", "state": "working", "url": "https://pc/env/mcp:1"]]]
        }
        came(m, [with("m_1")])
        let sent = m.t3Fetches()
        #expect(sent.count == 1)
        #expect(sent.first?["name"] == "t3.view")
        #expect(sent.first?["args"]["messages"] == ["m_1"])
        #expect(m.t3Fetches().isEmpty, "said once")
        // The chat changes: said again, and none once it has no threads.
        came(m, [])
        #expect(m.t3Fetches().first?["args"]["messages"] == [])
        // After a reconnect it's said again.
        came(m, [with("m_2")])
        m.synced = false
        #expect(m.t3Fetches().isEmpty)
        m.synced = true
        #expect(m.t3Fetches().first?["args"]["messages"] == ["m_2"])
    }

    @Test func aThreadsStartLineShowsAsItsCard() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [["id": "m_1", "conversation": "c_1", "role": "loom", "at": 1, "blocks": [["kind": "text", "text": "Started a T3 thread"]],
                  "threads": ["th_1": ["thread": "th_1", "title": "Fix it", "state": "working"]]],
                 ["id": "m_2", "conversation": "c_1", "role": "loom", "at": 2, "blocks": [["kind": "text", "text": "Compacted"]]]])
        // Loom's own line that started a thread shows, as its card; the rest are the desktop's.
        #expect(m.messages.map(\.id) == ["m_1"])
        #expect(m.messages.first.map(isStartLine) == true)
        #expect(m.messages.first?.threads.first?.title == "Fix it")
    }
}
