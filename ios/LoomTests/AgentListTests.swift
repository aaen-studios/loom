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

    @Test func theCoreGroupsTheList() throws {
        let folder = try #require(SideSection(["kind": "group", "id": "g_1", "name": "Home", "collapsed": true, "agents": ["a_4"]]))
        #expect(folder.part == .folder(id: "g_1", name: "Home", collapsed: true))
        #expect(folder.agents == ["a_4"])
        #expect(SideSection(["kind": "rooms", "rooms": [], "agents": []]) == nil)
        let m = caughtUp(agents: ["a_1", "a_2", "a_3", "a_4"])
        m.take(data("sidebar", [
            ["kind": "rooms", "rooms": [["id": "c_6", "name": "Everyone", "everyone": true]], "agents": []],
            ["kind": "waiting", "agents": ["a_2"]], ["kind": "pinned", "agents": []], ["kind": "agents", "agents": ["a_1"]],
            ["kind": "group", "id": "g_1", "name": "Home", "collapsed": false, "agents": ["a_4"]], ["kind": "hidden", "agents": ["a_3"]],
        ]))
        #expect(m.roomOrder == ["c_6"])
        #expect(m.folders.map(\.name) == ["Home"])
        // Hidden ones aren't shown.
        #expect(m.shownAgents.map(\.id) == ["a_2", "a_1", "a_4"])
        // Moved to a folder, it's there at once.
        let f = m.groupAgent("a_1", "g_1")
        #expect(f["args"] == ["agent": "a_1", "group": "g_1"])
        #expect(m.shownAgents.map(\.id) == ["a_2", "a_4", "a_1"])
        #expect(m.newFolder("  ", agent: "a_1") == nil)
        #expect(m.renameFolder("g_1", " ") == nil)
        #expect(m.renameFolder("g_1", "Garden")?["args"] == ["group": "g_1", "name": "Garden"])
        #expect(m.folders.map(\.name) == ["Garden"])
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
