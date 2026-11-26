import Testing
@testable import Loom

/// A watch's lines in a chat (PLAN.md, "Watching outside events"), as
/// desktop/src/model/watch.rs reads them: which mark, the words, which show, and which
/// stack tight.
@MainActor
@Suite struct WatchTests {
    /// A watch's line, as the instance stores it.
    func line(_ id: String, at: Int = 1_795_187_000_000, _ update: JSON) -> JSON {
        ["id": JSON(id), "conversation": "c_1", "role": "loom", "at": JSON(at), "blocks": [update]]
    }

    let moved: JSON = ["kind": "update", "integration": "acta", "watch": "w_1", "board": "aaen-studios/loom", "board_name": "loom",
                       "card": "LOO-206", "title": "Show agents' usage limits in the sidebar", "event": "moved", "to": "done",
                       "said": "moved to Done", "by": "Elliot", "url": "https://acta.aaenz.no/aaen-studios/loom#LOO-206",
                       "text": "LOO-206 moved to Done · Elliot"]

    @Test func anActaLineIsItsBoardItsCardAndWhatHappened() {
        let l = WatchLine(moved)
        #expect(l.mark == .board)
        #expect(l.words == "moved to Done · Elliot")
        #expect(l.spoken == "LOO-206 moved to Done · Elliot")
        #expect(l.status == "done")
        let comment = WatchLine(["integration": "acta", "card": "LOO-351", "event": "commented", "said": "commented", "by": "Ola"])
        #expect(comment.words == "commented · Ola")
    }

    @Test func aLinesDotIsTheStatusItIsAbout() {
        // A move's is where it went, whatever the card is now.
        let toReview = WatchLine(["integration": "acta", "card": "LOO-206", "event": "moved", "to": "review", "status": "done"])
        #expect(toReview.status == "review")
        // A comment's or a new card's is the card's status then, where the line says it.
        let commented = WatchLine(["integration": "acta", "card": "LOO-206", "event": "commented", "status": "in_progress"])
        #expect(commented.status == "in_progress")
        let made = WatchLine(["integration": "acta", "card": "LOO-207", "event": "created", "status": "todo"])
        #expect(made.status == "todo")
        // Where it doesn't, none: the phone doesn't look the card up, so its pill has no dot.
        let saidNothing = WatchLine(["integration": "acta", "card": "LOO-351", "event": "commented"])
        #expect(saidNothing.status == nil)
        #expect(WatchLine(["integration": "acta", "card": "LOO-1", "event": "moved", "to": " "]).status == nil)
    }

    @Test func aThreadLineIsItsStrandAsTheRunEnded() {
        func t3(_ state: String, _ said: String) -> WatchLine {
            WatchLine(["integration": "t3", "thread": "th_9", "title": "Fix the build", "event": "finished", "state": JSON(state), "said": JSON(said)])
        }
        #expect(t3("completed", "Thread finished").mark == .strand(.straight))
        #expect(t3("failed", "Thread failed").mark == .strand(.sags))
        #expect(t3("interrupted", "Thread stopped").mark == .strand(.slack))
        #expect(t3("completed", "Thread finished").words == "Thread finished")
        #expect(t3("completed", "Thread finished").spoken == "Thread finished · Fix the build")
        #expect(WatchLine(["integration": "t3", "thread": "th_3"]).threadTitle == "th_3")
    }

    @Test func aLineFromSomethingElseIsItsWords() {
        let l = WatchLine(["integration": "github", "said": "merged", "text": "PR 12 merged"])
        #expect(l.mark == .none)
        #expect(l.words == "PR 12 merged")
    }

    @Test func theChatShowsAWatchsLinesAndNotLoomsOthers() {
        let m = caughtUp()
        m.select("a_1")
        let fresh: JSON = ["id": "m_2", "conversation": "c_1", "role": "loom", "at": 1_795_187_000_000, "blocks": [["kind": "mark", "mark": "fresh"]]]
        came(m, [message("m_1", in: "c_1"), fresh, line("m_3", moved)])
        #expect(m.shown.map(\.id) == ["m_1", "m_3"])
        #expect(m.shown.last?.watchLine?.card == "LOO-206")
        #expect(m.shown.first?.watchLine == nil)
    }

    @Test func linesOneAfterAnotherStackTight() {
        let m = caughtUp()
        m.select("a_1")
        let later = 1_795_187_000_000 + 60 * 60 * 1000
        came(m, [line("m_1", moved), line("m_2", moved), message("m_3", in: "c_1"), line("m_4", moved), line("m_5", at: later, moved)])
        // The second of two, not the first, nor one after a reply or an hour on.
        #expect(stackedLines(m.shown) == ["m_2"])
    }
}
