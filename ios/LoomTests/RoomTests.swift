import Foundation
import Testing
@testable import Loom

/// New group chat (PLAN.md, "Group chats"), as desktop/src/model/rooms.rs's `room_panel`
/// has it: Make turns on with a name and two members, makes one group chat for a quick
/// double tap, and turns on again if no answer comes.
@MainActor
@Suite struct RoomTests {
    /// A model with three agents and New group chat's form started.
    func form() -> Model {
        let m = caughtUp(agents: ["a_1", "a_2", "a_3"])
        m.startRoomForm()
        return m
    }

    /// room.create's answer for `id`: the group chat made, `c_9`.
    func made(_ id: String, name: String = "Money", members: [String] = ["a_1", "a_2"]) -> JSON {
        ["t": "result", "id": JSON(id), "ok": true, "value": ["conversation": [
            "id": "c_9", "agent": "", "title": JSON(name), "at": 1_795_187_000_000,
            "room": ["name": JSON(name), "members": JSON(members), "color": "", "default": "", "corrections": []],
        ]]]
    }

    @Test func makeWantsANameFirst() {
        let m = form()
        #expect(m.newRoom(name: "").needs == .name)
        #expect(m.newRoom(name: "  \n").needs == .name)
        #expect(!m.newRoom(name: "").canMake)
    }

    @Test func thenTwoMembers() {
        let m = form()
        #expect(m.newRoom(name: "Money").needs == .members)
        m.toggleFormMember("a_1")
        #expect(m.newRoom(name: "Money").needs == .members)
        m.toggleFormMember("a_2")
        #expect(m.newRoom(name: "Money").needs == nil)
        #expect(m.newRoom(name: "Money").canMake)
        // With no name it's the name it wants, the form's first.
        #expect(m.newRoom(name: "").needs == .name)
    }

    @Test func membersGoInAsPickedAndComeOutAgain() {
        let m = form()
        m.toggleFormMember("a_3")
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        #expect(m.newRoom(name: "").members == ["a_3", "a_1", "a_2"])
        m.toggleFormMember("a_1")
        #expect(m.newRoom(name: "").members == ["a_3", "a_2"])
        m.setFormColor("amber")
        #expect(m.newRoom(name: "").color == "amber")
        m.startRoomForm()
        #expect(m.newRoom(name: "") == NewRoom(members: [], color: "", needs: .name, waiting: false))
    }

    @Test func makeSendsTheFormTrimmed() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        m.setFormColor("amber")
        let f = try #require(m.makeFormRoom(name: "  Money \n"))
        #expect(f["name"] == "room.create")
        #expect(f["args"] == ["name": "Money", "members": ["a_1", "a_2"], "color": "amber"])
        #expect(m.makeWaiting)
    }

    @Test func makeIsOffUntilTheFormIsReady() {
        let m = form()
        m.toggleFormMember("a_1")
        #expect(m.makeFormRoom(name: "Money") == nil)
        #expect(m.makingRoom == nil)
    }

    @Test func aDoubleTapMakesOne() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        let now = Date.now
        _ = try #require(m.makeFormRoom(name: "Money", now: now))
        #expect(m.makeFormRoom(name: "Money", now: now.addingTimeInterval(0.2)) == nil)
        #expect(!m.newRoom(name: "Money").canMake)
        #expect(m.newRoom(name: "Money").waiting)
    }

    @Test func makeTurnsOnAgainOnceItHasWaited() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        let now = Date.now
        _ = try #require(m.makeFormRoom(name: "Money", now: now))
        #expect(!m.makeTimedOut(now: now.addingTimeInterval(Model.makeWait - 1)))
        #expect(!m.newRoom(name: "Money").canMake)
        #expect(m.makeTimedOut(now: now.addingTimeInterval(Model.makeWait)))
        #expect(m.newRoom(name: "Money").canMake)
        // Once is enough.
        #expect(!m.makeTimedOut(now: now.addingTimeInterval(Model.makeWait * 2)))
    }

    @Test func anAnswerAfterWaitingStillShowsTheGroupChat() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        let now = Date.now
        let f = try #require(m.makeFormRoom(name: "Money", now: now))
        m.makeTimedOut(now: now.addingTimeInterval(Model.makeWait))
        m.take(made(f["id"].text))
        #expect(m.room == "c_9")
    }

    @Test func anAnswerToOneBeforeTheLastIsIgnored() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        let now = Date.now
        let first = try #require(m.makeFormRoom(name: "Money", now: now))
        m.makeTimedOut(now: now.addingTimeInterval(Model.makeWait))
        let second = try #require(m.makeFormRoom(name: "Money", now: now.addingTimeInterval(Model.makeWait + 1)))
        m.take(made(first["id"].text))
        #expect(m.room == nil)
        #expect(m.makeWaiting)
        m.take(made(second["id"].text))
        #expect(m.room == "c_9")
    }

    @Test func theMadeGroupChatOpensAndTheFormEmpties() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        let f = try #require(m.makeFormRoom(name: "Money"))
        m.take(made(f["id"].text))
        #expect(m.room == "c_9")
        #expect(m.roomOpen?.name == "Money")
        #expect(m.roomOpen?.members == ["a_1", "a_2"])
        // In the list at once, before the core lists it.
        #expect(listedRooms(m) == ["Money"])
        #expect(m.makingRoom == nil)
        #expect(m.roomForm == RoomForm())
        #expect(m.dirty.contains(.sidebar))
    }

    @Test func aRefusalSaysWhyAndMakeTurnsOnAgain() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_9")
        let f = try #require(m.makeFormRoom(name: "Money"))
        m.take(["t": "result", "id": f["id"], "ok": false, "error": ["code": "not_found", "message": "There's no agent a_9."]])
        #expect(m.notice == "There's no agent a_9.")
        #expect(m.room == nil)
        // What was picked stays, to fix.
        #expect(m.newRoom(name: "Money").members == ["a_1", "a_9"])
        #expect(m.newRoom(name: "Money").canMake)
    }

    @Test func oneSentOnAConnectionThatWentTurnsMakeOnAgain() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        _ = try #require(m.makeFormRoom(name: "Money"))
        m.take(["t": "core.conn", "state": "offline", "retry_ms": 1000])
        #expect(m.newRoom(name: "Money").canMake)
    }

    @Test func oneThatDidntGoTurnsMakeOnAgain() throws {
        let m = form()
        m.toggleFormMember("a_1")
        m.toggleFormMember("a_2")
        let f = try #require(m.makeFormRoom(name: "Money"))
        m.take(["t": "error", "code": "offline", "for": "app.other_1", "message": "Not connected."])
        #expect(m.makeWaiting)
        m.take(["t": "error", "code": "offline", "for": f["id"], "message": "Not connected."])
        #expect(m.newRoom(name: "Money").canMake)
        #expect(m.notice == nil)
    }

    @Test func aGroupChatKeepsTwoMembers() throws {
        let m = caughtUp(agents: ["a_1", "a_2", "a_3"])
        let two = Room(id: "c_9", ["name": "Money", "members": ["a_1", "a_2"]])
        #expect(m.toggleMember("a_1", in: two) == nil)
        let added = try #require(m.toggleMember("a_3", in: two))
        #expect(added["name"] == "room.update")
        #expect(added["args"] == ["conversation": "c_9", "members": ["a_1", "a_2", "a_3"]])
        let three = Room(id: "c_9", ["name": "Money", "members": ["a_1", "a_2", "a_3"]])
        #expect(try #require(m.toggleMember("a_2", in: three))["args"]["members"] == ["a_1", "a_3"])
    }

    // On SampleCore, as the app runs with `-sample`.

    @Test func makeMakesTheGroupChatOnTheSample() async throws {
        let session = try await sampleSession()
        let model = session.model
        model.startRoomForm()
        model.toggleFormMember("a_1")
        model.toggleFormMember("a_2")
        model.setFormColor("mint")
        session.makeRoom(name: " Money ")
        #expect(!model.newRoom(name: "Money").canMake)
        try #require(await eventually { model.roomOpen != nil })
        let room = try #require(model.roomOpen)
        #expect(room.name == "Money")
        #expect(room.members == ["a_1", "a_2"])
        #expect(room.color == "mint")
        // In the list after Daily, as they were made.
        try #require(await eventually { model.sideOrder.contains(room.id) })
        #expect(listedRooms(model) == ["Daily", "Money"])
        #expect(model.roomForm == RoomForm())
        // It takes messages: its first member answers.
        try #require(await eventually { model.messagesCame })
        session.send(model.send("Who pays the rent?"))
        try #require(await eventually { model.shown.contains { $0.isPerson && !$0.queued && $0.text == "Who pays the rent?" } })
        try #require(await eventually { model.runs.contains { $0.conversation == room.id && $0.agent == "a_1" } })
    }

    @Test func theSampleRefusesAnAgentThatIsntThere() async throws {
        let session = try await sampleSession()
        let model = session.model
        model.startRoomForm()
        model.toggleFormMember("a_1")
        model.toggleFormMember("a_9")
        session.makeRoom(name: "Ghosts")
        try #require(await eventually { model.notice != nil })
        #expect(model.notice == "There's no agent a_9.")
        #expect(model.room == nil)
        #expect(model.newRoom(name: "Ghosts").canMake)
        #expect(listedRooms(model) == ["Daily"])
    }

    @Test func aDragIsKeptOnTheSample() async throws {
        let session = try await sampleSession()
        let model = session.model
        // Gardener, out of Home, to the top of the list, over Daily.
        let sent = model.drop("a_4", .before("c_6"))
        #expect(sent.map { $0["name"] } == ["agent.group", "sidebar.order"])
        let place = try #require(sent[1]["args"]["items"].array.firstIndex(of: "a_4"))
        for c in sent { session.send(c) }
        try #require(await eventually { model.agent("a_4")?.order == place && model.agent("a_4")?.group == nil })
        #expect(model.listRows.first == .item(.agent("a_4")))
        // Daily into Home, and back out.
        session.send(model.regroup("c_6", "g_1"))
        try #require(await eventually { model.folderOf("c_6") == "g_1" && model.sideOrder.contains("c_6") })
        session.send(model.regroup("c_6", nil))
        try #require(await eventually { model.folderOf("c_6") == nil })
    }
}

/// The group chats in the agent list's one list, by name, in order.
@MainActor
func listedRooms(_ m: Model) -> [String] {
    m.sideSections.filter(\.part.listed).flatMap(\.items).compactMap { item in
        if case .room(let id) = item { return m.roomNamed(id)?.name }
        return nil
    }
}
