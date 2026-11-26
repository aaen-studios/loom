import Foundation
import Testing
@testable import Loom

/// The agents list (PLAN.md, "The sidebar"), each agent's chat, what its thread shows,
/// and the composer's @ picker, as desktop/src/model.rs has them.
@MainActor
@Suite struct AgentListTests {
    func agent(_ id: String, pinned: Int? = nil, order: Int? = nil) -> Agent {
        Agent(id: id, name: id, color: "", program: "", pinned: pinned, order: order)
    }

    @Test func pinnedFirstThenWhereTheyWereDragged() {
        let all = [agent("a"), agent("b", pinned: 5), agent("c", order: 1), agent("d", pinned: 3, order: 0), agent("e")]
        #expect(listed(all).map(\.id) == ["d", "b", "c", "a", "e"])
    }

    /// Four agents and Everyone, as the core groups them: none pinned, Scribe waiting, then
    /// Everyone and Builder in the list, Gardener in Home, and Postmaster hidden.
    func grouped() -> Model {
        let m = caughtUp(agents: ["a_1", "a_2", "a_3", "a_4"])
        m.take(data("conversations", ["a_1", "a_2", "a_3", "a_4"].map { ["id": JSON(chat(of: $0)), "agent": JSON($0), "title": ""] } + [
            ["id": "c_6", "agent": "", "title": "Everyone", "room": ["name": "Everyone", "everyone": true, "members": ["a_1", "a_2", "a_3", "a_4"]]],
        ]))
        m.take(data("sidebar", [
            ["kind": "pinned", "agents": []], ["kind": "waiting", "agents": ["a_2"]],
            ["kind": "list", "items": [["kind": "room", "id": "c_6", "name": "Everyone", "everyone": true], ["kind": "agent", "id": "a_1"]]],
            ["kind": "group", "id": "g_1", "name": "Home", "collapsed": false, "items": [["kind": "agent", "id": "a_4"]]],
            ["kind": "hidden", "agents": ["a_3"]],
            ["kind": "order", "items": ["c_6", "a_2", "a_1", "a_4", "a_3"]],
        ]))
        return m
    }

    @Test func theCoreGroupsTheList() throws {
        let folder = try #require(SideSection(["kind": "group", "id": "g_1", "name": "Home", "collapsed": true,
                                               "items": [["kind": "agent", "id": "a_4"], ["kind": "room", "id": "c_7"]]]))
        #expect(folder.part == .folder(id: "g_1", name: "Home", collapsed: true))
        #expect(folder.items == [.agent("a_4"), .room("c_7")])
        #expect(folder.agents == ["a_4"])
        // The old parts, and the whole order, aren't parts of the list.
        #expect(SideSection(["kind": "rooms", "rooms": [], "agents": []]) == nil)
        #expect(SideSection(["kind": "order", "items": ["a_4"]]) == nil)
        let m = grouped()
        #expect(m.sideOrder == ["c_6", "a_2", "a_1", "a_4", "a_3"])
        // Agents and group chats in one list, with the folder in it.
        #expect(m.listRows == [.item(.room("c_6")), .item(.agent("a_1")), .folder(id: "g_1", name: "Home", collapsed: false), .item(.agent("a_4"))])
        #expect(m.folders.map(\.name) == ["Home"])
        // Hidden ones aren't shown.
        #expect(m.shownAgents.map(\.id) == ["a_2", "a_1", "a_4"])
        // Moved to a folder, it's there at once.
        let f = try #require(m.regroup("a_1", "g_1"))
        #expect(f["name"] == "agent.group")
        #expect(f["args"] == ["agent": "a_1", "group": "g_1"])
        #expect(m.shownAgents.map(\.id) == ["a_2", "a_4", "a_1"])
        // A group chat goes in one too, and comes out again.
        let r = try #require(m.regroup("c_6", "g_1"))
        #expect(r["name"] == "room.update")
        #expect(r["args"] == ["conversation": "c_6", "group": "g_1"])
        #expect(m.folderOf("c_6") == "g_1")
        #expect(m.regroup("c_6", nil)?["args"] == ["conversation": "c_6", "group": ""])
        #expect(m.folderOf("c_6") == nil)
        #expect(m.newFolder("  ", agent: "a_1") == nil)
        #expect(m.renameFolder("g_1", " ") == nil)
        #expect(m.renameFolder("g_1", "Garden")?["args"] == ["group": "g_1", "name": "Garden"])
        #expect(m.folders.map(\.name) == ["Garden"])
    }

    @Test func aRowMovedInTheListTellsTheInstanceTheNewOrder() throws {
        let m = grouped()
        // Builder, dragged over Everyone.
        let up = try #require(m.rowDrop(from: 1, to: 0))
        #expect(up.0 == "a_1")
        #expect(up.1 == .before("c_6"))
        let f = m.drop(up.0, up.1)
        #expect(f.map { $0["name"] } == ["sidebar.order"])
        #expect(f[0]["args"] == ["items": ["a_1", "c_6", "a_2", "a_4", "a_3"]])
        #expect(m.sideOrder == ["a_1", "c_6", "a_2", "a_4", "a_3"])
        #expect(m.listRows.prefix(2) == [.item(.agent("a_1")), .item(.room("c_6"))])
        // The same place again: nothing to tell.
        #expect(m.drop("a_1", .before("c_6")).isEmpty)
    }

    @Test func aGroupChatDraggedIntoAFolderGoesInIt() throws {
        let m = grouped()
        // Everyone, dragged under Gardener, the folder's last.
        let down = try #require(m.rowDrop(from: 0, to: 4))
        #expect(down.0 == "c_6")
        #expect(down.1 == .after("a_4"))
        let f = m.drop(down.0, down.1)
        #expect(f.map { $0["name"] } == ["room.update", "sidebar.order"])
        #expect(f[0]["args"] == ["conversation": "c_6", "group": "g_1"])
        #expect(f[1]["args"] == ["items": ["a_2", "a_1", "a_4", "c_6", "a_3"]])
        #expect(m.folderOf("c_6") == "g_1")
        // And onto the folder's name: last in it.
        let g = m.drop("a_1", .folder("g_1"))
        #expect(g[0]["args"] == ["agent": "a_1", "group": "g_1"])
        #expect(m.listRows == [.folder(id: "g_1", name: "Home", collapsed: false), .item(.agent("a_4")), .item(.room("c_6")), .item(.agent("a_1"))])
        #expect(m.agent("a_1")?.group == "g_1")
    }

    @Test func whoWaitsAndTheHiddenDontMove() {
        let m = grouped()
        #expect(m.drop("a_2", .before("c_6")).isEmpty)
        #expect(m.drop("a_3", .after("a_1")).isEmpty)
        #expect(m.drop("a_1", .before("a_2")).isEmpty)
    }

    @Test func tilesMoveAmongTheTiles() {
        let m = caughtUp(agents: ["a_1", "a_2", "a_3"])
        m.take(data("sidebar", [
            ["kind": "pinned", "agents": ["a_1", "a_2"]], ["kind": "waiting", "agents": []],
            ["kind": "list", "items": [["kind": "agent", "id": "a_3"]]], ["kind": "hidden", "agents": []],
            ["kind": "order", "items": ["a_1", "a_2", "a_3"]],
        ]))
        // Dragged onto the one after it, it takes that one's place.
        #expect(m.onto("a_1", "a_2") == .after("a_2"))
        #expect(m.onto("a_2", "a_1") == .before("a_1"))
        let f = m.drop("a_1", m.onto("a_1", "a_2"))
        #expect(f[0]["args"] == ["items": ["a_2", "a_1", "a_3"]])
        #expect(m.sideSections.first?.agents == ["a_2", "a_1"])
        // A tile isn't dropped in the list, nor a row among the tiles.
        #expect(m.drop("a_1", .before("a_3")).isEmpty)
        #expect(m.drop("a_3", .before("a_1")).isEmpty)
    }

    @Test func oneMadeSinceTheCoreGroupedThemIsLastInTheList() {
        let m = grouped()
        m.take(data("agents", ["a_1", "a_2", "a_3", "a_4", "a_5"].map { ["id": JSON($0), "name": JSON(name(of: $0)), "color": "mint", "acp_agent": "claude"] }))
        // Out of the folders, after the list's own.
        #expect(m.listRows == [.item(.room("c_6")), .item(.agent("a_1")), .item(.agent("a_5")), .folder(id: "g_1", name: "Home", collapsed: false), .item(.agent("a_4"))])
    }

    @Test func anAgentsChatIsNeverAnotherAgentsThreadWithIt() {
        let m = caughtUp()
        m.take(data("conversations", [
            ["id": "c_5", "agent": "a_1", "title": "From Scribe", "asked_by": "a_2"],
            ["id": "c_7", "agent": "a_1", "title": "Branch", "branch_of": "c_1"],
            ["id": "c_1", "agent": "a_1", "title": ""],
            ["id": "c_2", "agent": "a_2", "title": ""],
        ]))
        #expect(m.conversation(of: "a_1") == "c_1")
        #expect(m.asks("a_1").map(\.label) == ["From Scribe"])
        m.select("a_1")
        #expect(m.conversation == "c_1")
        m.open(thread: "c_5")
        #expect(m.conversation == "c_5")
    }

    @Test func anAgentsChatIsTheMarkedOneWhateverItMakes() {
        let m = caughtUp()
        // Newest first: a conversation the agent made for itself, an older chat from
        // before the person started a new one, and the chat, marked main.
        m.take(data("conversations", [
            ["id": "c_22", "agent": "a_1", "title": "Plenpartner warm-up", "made_by": "a_1"],
            ["id": "c_4", "agent": "a_1", "title": ""],
            ["id": "c_1", "agent": "a_1", "title": "", "main": true],
        ]))
        #expect(m.conversation(of: "a_1") == "c_1")
        let threads = m.asks("a_1")
        #expect(threads.map(\.label) == ["Plenpartner warm-up"])
        #expect(threads.first?.made == true)
        m.select("a_1")
        #expect(m.send("hello")?["conversation"] == "c_1")
        m.open(thread: "c_22")
        #expect(m.conversation == "c_22")
        #expect(m.send("for the thread")?["conversation"] == "c_22")
        m.open(thread: nil)
        #expect(m.conversation == "c_1")
    }

    @Test func whatAnAgentsThreadShows() {
        let m = caughtUp()
        #expect(m.mood("a_1") == .idle)
        m.take(data("runs", [["run": "r_1", "conversation": "c_1", "agent": "a_1", "message": "m_9"]]))
        #expect(m.mood("a_1") == .thinking)
        #expect(m.preview(m.agent("a_1")!) == "Thinking")
        m.take(data("tools", [["call": "t_1", "run": "r_1", "title": "Check the last run"]]))
        #expect(m.mood("a_1") == .tool)
        #expect(m.preview(m.agent("a_1")!) == "Check the last run")
        m.take(data("approvals", [["call": "t_1", "run": "r_1", "tool": "execute", "title": "Run a command"]]))
        #expect(m.mood("a_1") == .waiting)
        #expect(m.preview(m.agent("a_1")!) == "Wants to run a command")
        // A routine's quiet run isn't shown as working.
        m.take(data("approvals", []))
        m.take(data("runs", [["run": "r_2", "conversation": "c_2", "agent": "a_2", "message": "m_8", "routine": "rt_1"]]))
        #expect(m.mood("a_2") == .idle)
        #expect(m.preview(m.agent("a_2")!) == "Claude")
    }

    @Test func aProgramShowsByTheNamePeopleKnow() {
        let m = caughtUp()
        #expect(m.programTitle("claude") == "Claude")
        m.take(["t": "result", "id": "app.acp", "ok": true, "value": ["programs": [["name": "claude", "title": "Claude Agent"]]]])
        #expect(m.programTitle("claude") == "Claude Agent")
        #expect(m.preview(m.agent("a_1")!) == "Claude Agent")
    }

    @Test func thePickerHasTheOtherAgentsThatMatch() {
        let m = caughtUp(agents: ["a_1", "a_2", "a_3"])
        m.take(data("agents", [
            ["id": "a_1", "name": "Builder", "color": "mint", "acp_agent": "claude"],
            ["id": "a_2", "name": "Mason", "color": "mint", "acp_agent": "claude"],
            ["id": "a_3", "name": "Scribe", "color": "mint", "acp_agent": "claude", "instructions": "\n# Keeps the notes\nPlans and meetings."],
        ]))
        m.select("a_1")
        func picked(_ q: String) -> [String] {
            m.picks(q).map { p in
                if case .agent(let a, let about) = p { return about.isEmpty ? a.name : "\(a.name): \(about)" }
                return ""
            }
        }
        #expect(picked("") == ["Mason", "Scribe: Keeps the notes"])
        // Those whose names start with it first.
        #expect(picked("s") == ["Scribe: Keeps the notes", "Mason"])
        #expect(picked("ribe") == ["Scribe: Keeps the notes"])
        // Never the agent whose chat it is.
        #expect(picked("build").isEmpty)
        let chosen = m.pick(m.picks("ribe")[0], at: 4, in: "Ask @rib")
        #expect(chosen?.text == "Ask @Scribe ")
        #expect(chosen?.mention == Mention(words: "@Scribe", link: "loom:agent/a_3"))
    }
}
