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

    // MARK: Following

    /// Answers `cmd` with `f`, as the core does.
    func answer(_ m: Model, _ cmd: JSON?, _ f: [String: JSON]) {
        var frame = f
        frame["t"] = "result"
        frame["id"] = cmd?["id"] ?? .null
        frame["ok"] = JSON(f["error"] == nil)
        m.take(.object(frame))
    }

    /// The person's watches, as `watch.list` gives them in the desktop's tests.
    let watches: [String: JSON] = ["value": ["watches": [
        ["id": "w_1", "integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "c_1"],
        ["id": "w_2", "integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom", "card": "LOO-206"], "conversation": "", "wake": ["agent": "a_1"],
         "error": "Acta didn't answer.\nTried 3 times."],
        ["id": "w_3", "integration": "t3", "server": "t3", "target": ["thread": "th_9", "title": "Fix the build"], "conversation": "c_9"],
        ["id": "w_4", "integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "", "wake": ["routine": "rt_1"]],
        ["id": "w_5", "integration": "acta", "server": "acta", "target": ["board": "aaen-studios/clx"], "conversation": "c_2"],
    ]]]

    /// A model caught up, with Builder's and Scribe's chats and the group chat Money, and
    /// the person's watches.
    func withWatches() throws -> Model {
        let m = caughtUp()
        m.take(data("conversations", [
            ["id": "c_1", "agent": "a_1", "title": ""],
            ["id": "c_2", "agent": "a_2", "title": ""],
            ["id": "c_9", "agent": "", "title": "Money", "room": ["name": "Money", "members": ["a_1", "a_2"]]],
        ]))
        m.take(["t": "synced", "seq": 2])
        let list = try #require(m.cardFetches(now: nowMs()).first { $0["name"] == "watch.list" })
        answer(m, list, watches)
        return m
    }

    @Test func followingListsWhatAnAgentOrAGroupChatFollows() throws {
        let m = try withWatches()
        let mine = m.following(.agent("a_1"))
        // Posting in its chat, or waking it; never the routine's, nor the group chat's.
        #expect(mine.map(\.watch) == ["w_1", "w_2"])
        #expect(mine[0].pill == .board("aaen-studios/loom"))
        #expect(mine[1].pill == .card(id: "LOO-206", board: "aaen-studios/loom"))
        #expect(mine[1].pill.link == "https://acta.aaenz.no/aaen-studios/loom#LOO-206")
        // Why it couldn't look, in one line.
        #expect(mine[1].error == "Acta didn't answer.")
        #expect(mine[0].error == nil)
        #expect(m.following(.room("c_9")).map(\.pill) == [.thread("Fix the build")])
        #expect(m.following(.agent("a_2")).map(\.watch) == ["w_5"])
        // An agent with none follows nothing.
        #expect(m.following(.agent("a_9")).isEmpty)
        // A thread with no title is its id.
        #expect(Watch(["id": "w_8", "integration": "t3", "target": ["thread": "th_3", "title": " "]])?.pill == .thread("th_3"))
    }

    @Test func watchesFollowTheirEventsAndGoAtOnceWhenTakenAway() throws {
        let m = try withWatches()
        m.take(["t": "event", "kind": "watch.updated", "seq": 3,
                "watch": ["id": "w_1", "integration": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "c_1", "error": "Signed out."]])
        #expect(m.following(.agent("a_1")).first?.error == "Signed out.")
        m.take(["t": "event", "kind": "watch.updated", "seq": 4,
                "watch": ["id": "w_6", "integration": "acta", "target": ["board": "aaen-studios/loom", "card": "LOO-9"], "conversation": "c_1"]])
        #expect(m.following(.agent("a_1")).count == 3)
        m.take(["t": "event", "kind": "watch.removed", "seq": 5, "watch": "w_6"])
        #expect(m.following(.agent("a_1")).count == 2)
        // The × takes it away at once.
        let f = try #require(m.stopFollowing("w_2"))
        #expect(f["name"] == "watch.remove")
        #expect(f["args"] == ["id": "w_2"])
        #expect(m.following(.agent("a_1")).map(\.watch) == ["w_1"])
        // Refused: said, and back as it was.
        answer(m, f, ["error": ["code": "bad_request", "message": "A routine's."]])
        #expect(m.notice == "A routine's.")
        #expect(m.following(.agent("a_1")).map(\.watch) == ["w_1", "w_2"])
        #expect(m.stopFollowing("w_9") == nil)
    }

    @Test func watchesAreAskedForOnceCaughtUpAndCardsFollowedOnceEach() throws {
        let m = Model()
        #expect(m.cardFetches(now: nowMs()).isEmpty)
        m.synced = true
        let first = try #require(m.cardFetches(now: nowMs()).first { $0["name"] == "watch.list" })
        #expect(m.cardFetches(now: nowMs()).isEmpty)
        // Each card followed is looked up once, for its pill.
        answer(m, first, watches)
        let looked = m.cardFetches(now: nowMs()).filter { $0["name"] == "card.preview" }
        #expect(looked.map { $0["args"]["ref"].text } == ["https://acta.aaenz.no/aaen-studios/loom#LOO-206"])
        #expect(m.cardFetches(now: nowMs()).isEmpty)
        let pill = FollowPill.card(id: "LOO-206", board: "aaen-studios/loom")
        #expect(m.followedCard(pill) == nil)
        answer(m, looked.first, ["value": ["card": ["id": "LOO-206", "board": "aaen-studios/loom", "title": "Show agents' usage limits in the sidebar", "status": "review"]]])
        #expect(m.followedCard(pill)?.title == "Show agents' usage limits in the sidebar")
        // Catching up again, they're asked for again.
        m.take(["t": "synced", "seq": 2])
        #expect(m.cardFetches(now: nowMs()).contains { $0["name"] == "watch.list" })
    }

    @Test func aLinesCardIsOnItsBoard() {
        #expect(WatchLine(moved).board == "aaen-studios/loom")
    }
}
