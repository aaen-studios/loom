import Testing
@testable import Loom

/// A long message is cut only when it was there and read when its chat opened (PLAN.md,
/// "Long messages and tables"), as desktop/src/model.rs's `folds`, and the model says
/// how each message came (`Model.shown`).
@MainActor
@Suite struct FoldTests {
    @Test func onlyWhatWasThereAndReadIsCut() {
        #expect(folds(.before, flipped: false))
        #expect(!folds(.live, flipped: false))
        #expect(!folds(.unread, flipped: false))
    }

    @Test func aFlipTurnsItOver() {
        #expect(!folds(.before, flipped: true))
        #expect(folds(.live, flipped: true))
        #expect(folds(.unread, flipped: true))
    }

    @Test func whatWasThereCameBefore() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1"), message("m_2", in: "c_1", role: "person")])
        #expect(cameOf(m) == ["m_1": .before, "m_2": .before])
    }

    @Test func whatComesWhileItsOpenIsLive() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        let reply = message("m_2", in: "c_1")
        let mine = message("m_3", in: "c_1", role: "person")
        m.take(created(reply))
        m.take(created(mine))
        came(m, [message("m_1", in: "c_1"), reply, mine])
        #expect(cameOf(m) == ["m_1": .before, "m_2": .live, "m_3": .live])
    }

    @Test func whatsBeingWrittenOrWaitingToGoIsLive() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        m.take(["t": "core.queued", "cid": "q_1", "conversation": "c_1", "text": "And the notes?", "at": 1_795_187_000_000])
        m.take(data("runs", [["run": "r_1", "conversation": "c_1", "agent": "a_1", "message": "m_9"]]))
        #expect(cameOf(m) == ["m_1": .before, "q_1": .live, "m_9": .live])
    }

    @Test func whatCatchingUpBringsCameBefore() {
        // What the core tells as it catches up, before `synced`, was there before.
        let m = Model()
        m.take(["t": "core.conn", "state": "live"])
        m.take(data("agents", [["id": "a_1", "name": "Builder", "color": "mint", "acp_agent": "claude"]]))
        m.take(data("conversations", [["id": "c_1", "agent": "a_1", "title": ""]]))
        m.select("a_1")
        let reply = message("m_2", in: "c_1")
        m.take(created(reply))
        m.take(["t": "synced", "seq": 3])
        came(m, [reply])
        #expect(cameOf(m) == ["m_2": .before])
    }

    @Test func repliesThatCameWhileShutAreUnread() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        let first = message("m_20", in: "c_2", agent: "a_2")
        let second = message("m_21", in: "c_2", agent: "a_2")
        m.take(created(first))
        m.take(created(second))
        m.select("a_2")
        came(m, [message("m_19", in: "c_2", agent: "a_2"), first, second])
        #expect(cameOf(m) == ["m_19": .before, "m_20": .unread, "m_21": .unread])
    }

    @Test func thePersonsOwnWhileShutIsntUnread() {
        // Sent from another device: only replies are unread.
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        let mine = message("m_20", in: "c_2", role: "person")
        m.take(created(mine))
        m.select("a_2")
        came(m, [mine])
        #expect(cameOf(m) == ["m_20": .before])
    }

    @Test func openedAgainWhatCameIsCutAsTheRest() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        let reply = message("m_2", in: "c_1")
        m.take(created(reply))
        m.select("a_2")
        came(m, [message("m_19", in: "c_2", agent: "a_2")])
        m.select("a_1")
        came(m, [message("m_1", in: "c_1"), reply])
        #expect(cameOf(m) == ["m_1": .before, "m_2": .before])
    }

    @Test func leftForTheListWhatComesIsUnreadWhenItOpens() {
        // Back at the agents list its agent stays selected, so the chat opens again at
        // once, afresh: what came while it was open is cut, and what came since is unread.
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        let live = message("m_2", in: "c_1")
        m.take(created(live))
        m.leaveChat()
        let away = message("m_3", in: "c_1")
        m.take(created(away))
        m.select("a_1")
        came(m, [message("m_1", in: "c_1"), live, away])
        #expect(cameOf(m) == ["m_1": .before, "m_2": .before, "m_3": .unread])
    }

    @Test func aFlipHoldsWhileTheChatIsOpen() {
        let m = caughtUp()
        m.select("a_1")
        came(m, [message("m_1", in: "c_1")])
        m.flipFold("m_1/0")
        #expect(m.foldFlips == ["m_1/0"])
        m.flipFold("m_1/0")
        #expect(m.foldFlips.isEmpty)
        m.flipFold("m_1/0")
        m.select("a_2")
        #expect(m.foldFlips.isEmpty)
    }

    @Test func aGroupChatOpenedAgainFromTheListIsAfresh() {
        let m = caughtUp()
        m.take(data("conversations", [
            ["id": "c_1", "agent": "a_1", "title": ""], ["id": "c_2", "agent": "a_2", "title": ""],
            ["id": "c_9", "agent": "", "title": "Money", "room": ["name": "Money", "members": ["a_1", "a_2"], "color": "", "default": ""]],
        ]))
        m.open(room: "c_9")
        #expect(m.conversation == "c_9")
        came(m, [message("m_1", in: "c_9")])
        let reply = message("m_2", in: "c_9", agent: "a_2")
        m.take(created(reply))
        m.flipFold("m_1/0")
        came(m, [message("m_1", in: "c_9"), reply])
        #expect(cameOf(m) == ["m_1": .before, "m_2": .live])
        // Opened again without leaving: nothing changes.
        m.open(room: "c_9")
        #expect(m.foldFlips == ["m_1/0"])
        m.leaveChat()
        m.open(room: "c_9")
        #expect(m.foldFlips.isEmpty)
        #expect(cameOf(m) == ["m_1": .before, "m_2": .before])
    }

    // On SampleCore, as the app runs with `-sample`.

    @Test func theSamplesLongReplyIsCutWhenItsChatOpens() async throws {
        let session = try await sampleSession()
        try await openChat("a_1", in: session)
        #expect(session.model.shown.first { $0.id == "msg_2" }?.came == .before)
    }

    @Test func whatThePersonSendsAndItsReplyShowWhole() async throws {
        let session = try await sampleSession()
        let model = session.model
        try await openChat("a_1", in: session)
        session.send(model.send("Did the build on main pass?"))
        try #require(await eventually { model.shown.contains { $0.isPerson && !$0.queued && $0.text == "Did the build on main pass?" } })
        try #require(await eventually { model.shown.contains { $0.live } })
        for m in model.shown where m.live || m.text == "Did the build on main pass?" {
            #expect(m.came == .live)
        }
        #expect(model.shown.first { $0.id == "msg_2" }?.came == .before)
    }

    @Test func aSampleReplyThatCameWhileShutIsUnread() async throws {
        let session = try await sampleSession()
        let model = session.model
        try await openChat("a_1", in: session)
        session.send(model.send("Is the build on main green?"))
        // The sample's agent asks before it runs a command: answered from another chat,
        // so its reply comes while its own is shut.
        try #require(await eventually { !model.shownApprovals.isEmpty })
        let call = try #require(model.shownApprovals.first?.call)
        try await openChat("a_4", in: session)
        session.send(model.approve(call, allow: true))
        try #require(await eventually { !model.runs.contains { $0.agent == "a_1" } })
        try await openChat("a_1", in: session)
        try #require(await eventually { model.shown.last?.role == "agent" && model.shown.last?.live == false })
        let reply = try #require(model.shown.last)
        #expect(reply.came == .unread)
        // What the person sent while it was open, before it was shut, is read.
        #expect(model.shown.first { $0.text == "Is the build on main green?" }?.came == .before)
    }
}
