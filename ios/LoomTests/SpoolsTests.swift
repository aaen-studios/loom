import Foundation
import Testing
@testable import Loom

/// Spools on the phone (PLAN.md, "The integrations catalog"), as desktop/src/model/spools.rs's
/// tests have it: the catalog, Find's cards, pills and tiles, the registry's search,
/// connecting step by step, and Installed.
@MainActor
@Suite struct SpoolsTests {
    var catalog: JSON {
        ["entries": [
            ["id": "acta", "name": "Acta", "category": "Work", "description": "Boards, tasks and comments in Acta.", "maker": "Aaen Studios",
             "badge": ["name": "acta", "initial": "A", "color": "plum", "icon": "ab12"], "icon_svg": "<svg/>", "mark": "acta", "server": "acta", "runs": "remote",
             "url": "https://acta.aaenz.no/api/mcp", "fields": [], "sign_in": "oauth", "secrets": [],
             "tools": [["name": "list_tasks", "kind": "read"], ["name": "create_tasks", "kind": "change"]], "tools_from": "x", "tools_checked": true, "servers": ["acta"], "connected": true],
            ["id": "t3", "name": "T3 Code", "category": "Code", "description": "Projects and coding threads in T3 Code.", "maker": "T3 Tools", "server": "t3", "runs": "remote",
             "url": "{address}/mcp", "fields": [["name": "address", "label": "Address", "placeholder": "https://host.tailnet.ts.net"]], "sign_in": "oauth", "secrets": [],
             "note": "Use an address your browser reaches too.", "tools": [], "tools_checked": false, "servers": [], "connected": false],
            ["id": "github", "name": "GitHub", "category": "Code", "description": "Repositories, issues and pull requests on GitHub.", "maker": "GitHub", "server": "github",
             "runs": "remote", "url": "https://api.githubcopilot.com/mcp/", "fields": [], "sign_in": "secret",
             "secrets": [["name": "GITHUB_TOKEN", "label": "Personal access token", "placeholder": "github_pat_…", "have": false]], "tools": [], "tools_checked": true, "servers": [], "connected": false],
            ["id": "playwright", "name": "Playwright", "category": "Code", "description": "Microsoft's Playwright browser, run on the instance.", "maker": "Microsoft",
             "server": "playwright", "runs": "instance", "command": "npx", "args": ["-y", "@playwright/mcp@latest"], "fields": [], "sign_in": "none", "secrets": [], "tools": [],
             "tools_checked": true, "servers": [], "connected": false],
            ["id": "home-assistant", "name": "Home Assistant", "category": "Home", "description": "Lights, devices and what's on at home.", "maker": "Open Home Foundation",
             "server": "home-assistant", "runs": "remote", "url": "{address}/api/mcp", "fields": [["name": "address", "label": "Address"]], "sign_in": "secret",
             "secrets": [["name": "HOME_ASSISTANT_TOKEN", "label": "Long-lived access token", "have": true]], "tools": [], "tools_checked": false, "servers": [], "connected": false],
        ], "categories": ["Work", "Code", "Home"]]
    }

    func withCatalog() -> Model {
        let m = caughtUp()
        m.take(["t": "result", "id": "app.spools_list_1", "ok": true, "value": catalog])
        return m
    }

    func server(_ name: String, _ catalog: String, _ status: McpStatus) -> McpServer {
        McpServer(name: name, catalog: catalog, status: status)
    }

    func ids(_ entries: [Entry]) -> [String] { entries.map(\.id) }

    @Test func theCatalogComesAsEntries() throws {
        let m = withCatalog()
        let acta = try #require(m.entry("acta"))
        #expect(acta.name == "Acta")
        #expect(acta.featured && acta.connected && acta.remote)
        #expect(acta.badge?.initial == "A")
        #expect(acta.tools.map(\.name) == ["list_tasks", "create_tasks"])
        #expect(acta.tools.map(\.changes) == [false, true])
        #expect(acta.iconSvg == "<svg/>")
        let t3 = try #require(m.entry("t3"))
        #expect(t3.featured && !t3.connected && t3.several)
        #expect(t3.fields[0].placeholder == "https://host.tailnet.ts.net")
        #expect(t3.iconSvg.isEmpty)
        let playwright = try #require(m.entry("playwright"))
        #expect(playwright.command == "npx -y @playwright/mcp@latest")
        #expect(!playwright.remote)
    }

    @Test func anEntryWithAMarkDrawsTheAppsCopyOfIt() throws {
        let m = withCatalog()
        // The app's copy is by the name the instance gives (`mark-acta`); one without has
        // its initial.
        #expect(m.entry("acta")?.mark == "acta")
        #expect(m.entry("t3")?.mark == nil)
        // A registry server the instance knows by its name has that one's.
        let known = Entry(["id": "registry:com.foreverworks/notion", "name": "notion", "community": true, "icon_svg": "<svg/>", "mark": "notion"])
        #expect(known.mark == "notion")
        // One whose only mark is its own SVG has its initial.
        #expect(Entry(["id": "registry:io.x/lights", "name": "lights", "community": true, "icon_svg": "<svg/>"]).mark == nil)
    }

    @Test func aTileIsFilledOnceThePersonHasAServerFromIt() throws {
        let m = withCatalog()
        #expect(try #require(m.entry("acta")).tileFilled)
        #expect(try !#require(m.entry("t3")).tileFilled)
    }

    @Test func anInstalledRowHasItsServersMarkAndIsFilledOnceConnected() {
        var composio = McpServer(["name": "composio", "url": "https://rube.app/mcp", "catalog": "composio", "status": "connected", "mark": "composio"])
        var row = InstalledRow(server: composio, entry: nil, title: "Composio", line: "Connected", dot: .ok)
        #expect(row.mark == "composio" && row.filled)
        composio.mark = ""
        row = InstalledRow(server: composio, entry: Entry(["id": "acta", "mark": "acta"]), title: "Acta", line: "Not tried yet", dot: .quiet)
        #expect(row.mark == "acta" && !row.filled)
    }

    @Test func anEntrySaysWhereItRunsAndHowItSignsIn() {
        let m = withCatalog()
        let words = { (id: String) in (m.entry(id)?.runsWords ?? "", m.entry(id)?.signInWords ?? "") }
        #expect(words("acta") == ("acta.aaenz.no", "With your Acta account"))
        #expect(words("t3").0 == "At the address you give")
        #expect(words("github").1 == "With a personal access token")
        #expect(words("playwright") == ("On your server, with npx", "No sign-in"))
        #expect(words("home-assistant").1 == "With a long-lived access token")
    }

    @Test func findShowsTheTwoFirstAsCardsAndFiltersByPillAndWords() {
        let m = withCatalog()
        let entries = m.spools.entries
        let shown = { (pill: String?, q: String) in
            let (cards, tiles) = found(entries, pill: pill, query: q, also: [])
            return [self.ids(cards), self.ids(tiles)]
        }
        #expect(shown(nil, "") == [["acta", "t3"], ["github", "playwright", "home-assistant"]])
        #expect(shown("Code", "") == [["t3"], ["github", "playwright"]])
        #expect(shown("Fun", "") == [[], []])
        #expect(shown(nil, "git") == [[], ["github"]])
        #expect(shown(nil, "MICROSOFT") == [[], ["playwright"]])
        #expect(shown(nil, "boards") == [["acta"], []])
        #expect(shown("Home", "acta") == [[], []])
        #expect(pillsOf(["Work", "Money"]) == ["Work", "Code", "Mail & calendar", "Files", "Home", "Fun", "Money"])
    }

    var searched: JSON {
        ["entries": [
            ["id": "registry:com.example/notes", "name": "Notes", "category": "", "description": "Notes in Notion.", "maker": "example.com",
             "badge": ["name": "notes", "initial": "N", "color": "sky"], "server": "notes", "runs": "remote", "url": "https://{region}.notes.example.com/mcp",
             "fields": [["name": "region", "label": "region", "default": "eu", "description": "Where it runs"]], "sign_in": "secret",
             "secrets": [["name": "notes.NOTES_KEY", "label": "NOTES_KEY", "description": "Your key", "have": false],
                         ["name": "notes.NOTES_TEAM", "label": "NOTES_TEAM", "have": false, "optional": true]],
             "tools": [], "tools_checked": false, "servers": [], "connected": false, "community": true, "registry": "com.example/notes", "version": "1.2.0"],
            ["id": "registry:com.example/boxed", "name": "Boxed", "category": "", "description": "Notion in a box.", "maker": "example.com", "server": "boxed", "runs": "remote",
             "fields": [], "sign_in": "none", "secrets": [], "tools": [], "tools_checked": false, "servers": [], "connected": false, "community": true,
             "registry": "com.example/boxed", "version": "0.1.0", "unsupported": "It only runs in Docker."],
        ], "curated": ["github"]]
    }

    func search(_ m: Model, _ query: String, _ value: JSON) throws {
        let ask = try #require(m.spoolSearch(query))
        #expect(ask["name"] == "catalog.search")
        #expect(ask["args"]["query"] == JSON(query))
        m.take(["t": "result", "id": ask["id"], "ok": true, "value": value])
    }

    @Test func theRegistryIsSearchedFromThreeLettersAndOnlyItsLastAnswerIsTaken() throws {
        let m = withCatalog()
        #expect(m.spoolSearch("no") == nil)
        let first = try #require(m.spoolSearch("not"))
        #expect(m.spoolSearch(" not ") == nil)
        let shown = m.spoolsShown("not")
        #expect(shown.line == "Searching…")
        #expect(!shown.nothing)
        let last = try #require(m.spoolSearch("notion"))
        m.take(["t": "result", "id": first["id"], "ok": true, "value": ["entries": [], "curated": []]])
        #expect(!m.spools.search.answered)
        m.take(["t": "result", "id": last["id"], "ok": true, "value": searched])
        #expect(m.spools.search.answered)
        #expect(m.spoolsShown("notion").line == nil)
        #expect(m.spoolSearch("no") == nil)
        #expect(m.spools.search == RegistrySearch())
    }

    @Test func theRegistrysServersComeAfterTheCatalogsMarkedCommunity() throws {
        let m = withCatalog()
        try search(m, "notion", searched)
        let shown = m.spoolsShown("notion")
        #expect(ids(shown.tiles) == ["github", "registry:com.example/notes", "registry:com.example/boxed"])
        #expect(shown.cards.isEmpty && !shown.nothing)
        let notes = try #require(m.entry("registry:com.example/notes"))
        #expect(notes.community && !notes.several)
        #expect(notes.signInWords == "With a key")
        #expect(notes.fields[0].placeholder == "eu")
        #expect(notes.secrets[0].placeholder == "Your key")
        #expect(m.entry("github")?.community == false)
        m.spools.pill = "Code"
        #expect(ids(m.spoolsShown("notion").tiles) == ["github"])
        m.spools.pill = nil
        #expect(m.spoolsShown("notio").tiles.isEmpty)
    }

    @Test func aRegistryThatsDownSaysWhyInPlaceOfNothingFound() throws {
        let m = withCatalog()
        try search(m, "zzzz", ["entries": [], "curated": [], "error": "The MCP Registry couldn't be reached."])
        let shown = m.spoolsShown("zzzz")
        #expect(shown.line == "The MCP Registry couldn't be reached.")
        #expect(!shown.nothing)
        try search(m, "qqqq", ["entries": [], "curated": []])
        let none = m.spoolsShown("qqqq")
        #expect(none.nothing && none.line == nil)
    }

    @Test func aRegistryServerConnectsByItsNameAndVersion() throws {
        let m = withCatalog()
        _ = m.openSpools()
        try search(m, "notion", searched)
        m.spoolOpen("registry:com.example/notes")
        #expect(m.spoolConnect() == nil)
        #expect(m.spools.flow?.stage == .form)
        #expect(m.spoolSubmit(fields: ["region": ""], secrets: [:]) == nil)
        #expect(m.spools.flow?.said == "Fill in NOTES_KEY")
        let f = try #require(m.spoolSubmit(fields: ["region": ""], secrets: ["notes.NOTES_KEY": " k1 "]))
        #expect(f["name"] == "catalog.connect")
        #expect(f["args"] == ["registry": "com.example/notes", "version": "1.2.0", "fields": [:], "secrets": ["notes.NOTES_KEY": "k1"]])
        m.take(["t": "connect", "connect": ["entry": "registry:com.example/notes", "server": "notes", "step": "agents", "tools": []]])
        #expect(m.spools.flow?.stage == .going(.agents))
    }

    @Test func oneLoomCantConnectHasNoConnect() throws {
        let m = withCatalog()
        try search(m, "notion", searched)
        m.spoolOpen("registry:com.example/boxed")
        #expect(m.spoolConnect() == nil)
        #expect(m.spools.flow?.stage == .about)
        #expect(m.entry("registry:com.example/boxed")?.connectsHere == false)
    }

    @Test func t3IsConnectedFromAComputer() throws {
        // T3's sign-in takes no app's way back, so the phone doesn't connect it.
        let m = withCatalog()
        #expect(m.entry("t3")?.connectsHere == false)
        #expect(m.entry("acta")?.connectsHere == true)
        m.spoolOpen("t3")
        #expect(m.spoolConnect() == nil)
        #expect(m.spoolConnect(another: true) == nil)
        #expect(m.spools.flow?.stage == .about)
    }

    @Test func connectingOneThatAsksNothingGoesAtOnce() throws {
        let m = withCatalog()
        m.spoolOpen("playwright")
        let f = try #require(m.spoolConnect())
        #expect(f["name"] == "catalog.connect")
        #expect(f["args"] == ["id": "playwright"])
        #expect(m.spools.flow?.stage == .going(.adding))
    }

    @Test func connectingOneThatAsksFillsItsFormFirst() throws {
        let m = withCatalog()
        m.spoolOpen("github")
        #expect(m.spoolConnect() == nil)
        #expect(m.spools.flow?.stage == .form)
        #expect(m.spoolSubmit(fields: [:], secrets: ["GITHUB_TOKEN": " "]) == nil)
        #expect(m.spools.flow?.said == "Fill in Personal access token")
        let f = try #require(m.spoolSubmit(fields: [:], secrets: ["GITHUB_TOKEN": "github_pat_1"]))
        #expect(f["args"] == ["id": "github", "fields": [:], "secrets": ["GITHUB_TOKEN": "github_pat_1"]])
        #expect(m.spools.flow?.stage == .going(.adding) && m.spools.flow?.said == "")
        let home = try #require(m.entry("home-assistant"))
        #expect(missingInForm(home, fields: [:], secrets: [:]) == "Fill in Address")
        #expect(missingInForm(home, fields: ["address": "https://home.example.com"], secrets: [:]) == nil)
    }

    @Test func aConnectMovesThroughItsStepsAsTheInstanceSays() throws {
        let m = withCatalog()
        _ = m.openSpools()
        m.spoolOpen("acta")
        // Acta has a server already: Connect connects another of its own name.
        let f = try #require(m.spoolConnect())
        #expect(f["args"] == ["id": "acta"])
        m.take(["t": "result", "id": f["id"], "ok": true, "value": ["connect": ["entry": "acta", "server": "acta", "step": "adding", "said": "", "tools": []]]])
        let frame = { (step: String) -> JSON in ["t": "connect", "connect": ["entry": "acta", "server": "acta", "step": JSON(step), "said": "", "tools": ["list_tasks"]]] }
        m.take(frame("sign_in"))
        #expect(m.spools.flow?.stage == .going(.signIn))
        m.take(frame("adding"))
        m.mcpServers = [server("acta", "acta", .untested)]
        #expect(m.installedRows.first?.line == "Connecting…")
        m.take(frame("agents"))
        #expect(m.spools.flow?.stage == .going(.agents))
        #expect(m.spools.flow?.agents == [])
        m.spoolPickAgent("a_1")
        m.spoolPickAgent("a_2")
        m.spoolPickAgent("a_1")
        let give = try #require(m.spoolGive())
        #expect(give["name"] == "mcp.agents")
        #expect(give["args"] == ["name": "acta", "agents": ["a_2"]])
        m.take(["t": "result", "id": give["id"], "ok": true, "value": ["server": "acta", "agents": ["a_2"]]])
        #expect(m.spools.flow?.stage == .going(.done))
        m.take(frame("done"))
        #expect(m.spools.connects.isEmpty)
        // Once connected, the catalog is asked for again, for its servers.
        #expect(m.fetches().contains { $0["name"] == "catalog.list" })
    }

    @Test func aConnectThatFailsSaysWhyAndGoesBackToItsForm() throws {
        let m = withCatalog()
        m.spoolOpen("github")
        _ = m.spoolConnect()
        let f = try #require(m.spoolSubmit(fields: [:], secrets: ["GITHUB_TOKEN": "bad"]))
        m.take(["t": "connect", "connect": ["entry": "github", "server": "github", "step": "failed", "said": "It answered 401."]])
        #expect(m.spools.flow?.stage == .going(.failed))
        #expect(m.spools.flow?.said == "It answered 401.")
        m.spoolBack()
        #expect(m.spools.flow?.stage == .form)
        _ = m.spoolConnect()
        let f2 = try #require(m.spoolSubmit(fields: [:], secrets: ["GITHUB_TOKEN": "x"]))
        #expect(f["id"] != f2["id"])
        m.take(["t": "result", "id": f2["id"], "ok": false, "error": ["code": "exists", "message": "There's an MCP server named github already."]])
        #expect(m.spools.flow?.stage == .going(.failed))
        #expect(m.spools.flow?.said == "There's an MCP server named github already.")
    }

    @Test func aDetailOpenedOnAConnectUnderWayShowsItsStep() throws {
        let m = withCatalog()
        var list = catalog
        if case .object(var o) = list, case .array(var entries) = o["entries"]!, case .object(var github) = entries[2] {
            github["connecting"] = ["entry": "github", "server": "github", "step": "sign_in", "said": "", "tools": []]
            entries[2] = .object(github)
            o["entries"] = .array(entries)
            list = .object(o)
        }
        m.take(["t": "result", "id": "app.spools_list_2", "ok": true, "value": list])
        m.spoolOpen("github")
        #expect(m.spools.flow?.stage == .going(.signIn))
        #expect(m.spools.flow?.server == "github")
    }

    @Test func anotherIsNamedForItsHost() throws {
        let m = withCatalog()
        let t3 = try #require(m.entry("t3"))
        #expect(anotherName(t3, address: "https://vps.tail1234.ts.net", taken: ["t3"]) == "t3-vps")
        #expect(anotherName(t3, address: "https://VPS.tail1234.ts.net", taken: ["t3", "t3-vps"]) == "t3-vps-2")
        #expect(anotherName(t3, address: "http://192.168.1.4:3773", taken: []) == "t3-192")
        #expect(anotherName(t3, address: "https://my_pc.ts.net", taken: []) == "t3-my-pc")
        #expect(anotherName(t3, address: "", taken: []) == "t3-2")
    }

    @Test func installedListsTheCatalogsFirstInItsOrderThenTheRest() {
        let m = withCatalog()
        var vps = server("t3-vps", "t3", .signIn)
        vps.url = "https://vps.ts.net/mcp"
        var github = server("github", "github", .failing)
        github.why = "It answered 401."
        var pc = server("t3", "t3", .connected)
        pc.url = "https://pc.ts.net/mcp"
        pc.signIn = .signedIn("Elliot")
        var acta = server("acta", "acta", .connected)
        acta.signIn = .signedIn("Elliot")
        m.mcpServers = [server("notes", "", .untested), vps, github, pc, acta, server("calendar", "", .connected)]
        let rows = m.installedRows
        #expect(rows.map(\.server.name) == ["acta", "t3", "t3-vps", "github", "calendar", "notes"])
        #expect(rows.map(\.title) == ["Acta", "T3 Code", "T3 Code", "GitHub", "calendar", "notes"])
        #expect(rows.map(\.line) == ["Connected as Elliot", "pc.ts.net", "vps.ts.net", "It answered 401.", "Connected", "Not tried yet"])
        #expect(rows.map(\.dot) == [.ok, .ok, .warn, .bad, .ok, .quiet])
        #expect(rows[0].entry?.id == "acta")
        #expect(rows[4].entry == nil)
    }

    @Test func aCardSaysTheWorstOfHowItsServersStand() throws {
        let m = withCatalog()
        let t3 = try #require(m.entry("t3"))
        #expect(cardStatus(t3, []) == nil)
        var mine = t3
        mine.servers = ["t3", "t3-vps"]
        var servers = [server("t3", "t3", .connected), server("t3-vps", "t3", .connected), server("other", "", .failing)]
        // Two T3s say how many.
        #expect(cardStatus(mine, servers)?.words == "2 connected")
        var one = mine
        one.servers = ["t3"]
        #expect(cardStatus(one, servers)?.words == "Connected")
        #expect(cardStatus(mine, servers)?.dot == .ok)
        servers[1].status = .untested
        #expect(cardStatus(mine, servers)?.words == "Not tried yet")
        #expect(cardStatus(mine, servers)?.dot == .quiet)
        servers[1].status = .signIn
        #expect(cardStatus(mine, servers)?.words == "Needs you to sign in")
        #expect(cardStatus(mine, servers)?.dot == .warn)
        servers[0].status = .failing
        servers[0].why = "It didn't answer."
        #expect(cardStatus(mine, servers)?.words == "It didn't answer.")
        #expect(cardStatus(mine, servers)?.dot == .bad)
    }

    @Test func aDetailShowsAFewOfEachKindOfToolUntilShowAll() {
        func tools(_ reads: Int, _ changes: Int) -> [(name: String, changes: Bool)] {
            (0..<reads).map { ("r\($0)", false) } + (0..<changes).map { ("c\($0)", true) }
        }
        #expect(toolsShown(tools(2, 3), all: false) == [0, 1, 2, 3, 4])
        #expect(toolsShown(tools(18, 14), all: false) == [0, 1, 2, 3, 18, 19, 20, 21])
        #expect(toolsShown(tools(10, 2), all: false) == [0, 1, 2, 3, 4, 5, 10, 11])
        #expect(toolsShown(tools(1, 12), all: false) == [0, 1, 2, 3, 4, 5, 6, 7])
        #expect(toolsShown(tools(18, 14), all: true).count == 32)
    }

    @Test func whatCanBeDoneWithAServerFollowsHowItStands() {
        var acta = server("acta", "acta", .connected)
        acta.tools = ["a", "b"]
        #expect(statusLine(acta) == "Connected · 2 tools")
        acta.tools = ["a"]
        #expect(statusLine(acta) == "Connected · 1 tool")
        var needs = server("t3", "t3", .signIn)
        needs.signIn = .needed
        #expect(serverActs(needs) == [.signIn, .reconnect, .remove])
        var signed = server("acta", "acta", .connected)
        signed.signIn = .signedIn("")
        #expect(serverActs(signed) == [.reconnect, .disconnect, .remove])
        #expect(serverActs(server("github", "github", .connected)) == [.reconnect, .remove])
        #expect(serverActs(server("acta", "acta", .untested)) == [.tryIt, .remove])
    }

    @Test func tryingAServerTestsItAndItsDetailSaysSo() {
        let m = withCatalog()
        let acta = server("acta", "acta", .untested)
        m.mcpServers = [acta]
        #expect(m.serverStanding(acta).words == "Not tried yet")
        let f = m.spoolTry("acta")
        #expect(f["name"] == "mcp.test")
        #expect(f["args"]["name"] == "acta")
        #expect(m.serverStanding(acta).words == "Trying…")
        m.take(["t": "result", "id": f["id"], "ok": true, "value": ["tools": ["a"]]])
        #expect(m.serverStanding(acta).words == "Not tried yet")
    }

    @Test func aServersDetailNamesItOnlyWhereItsTitleDoesnt() {
        let m = withCatalog()
        let acta = m.entry("acta"), t3 = m.entry("t3")
        #expect(installedUnder(server("acta", "acta", .connected), acta) == "")
        var first = server("t3", "t3", .connected)
        first.url = "https://pc.ts.net/mcp"
        #expect(installedUnder(first, t3) == "pc.ts.net")
        var another = server("t3-vps-2", "t3", .connected)
        another.url = "https://vps.ts.net/mcp"
        #expect(installedUnder(another, t3) == "t3-vps-2 · vps.ts.net")
        var hand = server("calendar", "", .connected)
        hand.url = "https://mcp.example.com/sse"
        #expect(installedUnder(hand, nil) == "mcp.example.com")
        #expect(installedUnder(server("notes", "", .failing), nil) == "")
    }

    @Test func anAgentsChipTogglesAtOnce() throws {
        let m = withCatalog()
        var acta = server("acta", "acta", .connected)
        acta.agents = ["a_1"]
        m.mcpServers = [acta]
        let f = try #require(m.spoolToggleAgent(server: "acta", agent: "a_2"))
        #expect(f["args"] == ["name": "acta", "agents": ["a_1", "a_2"]])
        #expect(m.mcpServers[0].agents == ["a_1", "a_2"])
        #expect(m.spoolToggleAgent(server: "acta", agent: "a_1")?["args"]["agents"] == ["a_2"])
        #expect(m.spoolToggleAgent(server: "gone", agent: "a_1") == nil)
    }

    @Test func reconnectConnectsOneFromTheCatalogAgainAndKeepsItsAgents() throws {
        let m = withCatalog()
        var vps = server("t3-vps", "t3", .failing)
        vps.agents = ["a_1"]
        m.mcpServers = [vps, server("notes", "", .failing)]
        let f = try #require(m.spoolReconnect("t3-vps"))
        #expect(f["name"] == "catalog.connect")
        #expect(f["args"] == ["id": "t3", "name": "t3-vps"])
        #expect(m.installedRows.first?.line == "Connecting…")
        let agents: JSON = ["t": "connect", "connect": ["entry": "t3", "server": "t3-vps", "step": "agents", "said": "", "tools": []]]
        m.take(agents)
        let sent = m.fetches().filter { $0["name"] == "mcp.agents" }
        #expect(sent.count == 1)
        #expect(sent.first?["args"] == ["name": "t3-vps", "agents": ["a_1"]])
        // Only once.
        m.take(agents)
        #expect(m.fetches().allSatisfy { $0["name"] != "mcp.agents" })
        #expect(m.spoolReconnect("notes")?["name"] == "mcp.test")
    }

    @Test func removeDeletesTheServer() {
        let m = withCatalog()
        m.mcpServers = [server("acta", "acta", .connected)]
        let f = m.spoolRemove("acta")
        #expect(f["name"] == "mcp.delete")
        #expect(m.mcpServers.isEmpty)
    }

    @Test func anInstanceWithoutSpoolsSaysWhy() {
        let m = caughtUp()
        m.take(["t": "result", "id": "app.spools_list_1", "ok": false, "error": ["code": "unknown_command", "message": "No such command."]])
        #expect(m.spools.error == "No such command.")
        #expect(!m.spools.came)
    }

    @Test func theServersComeWithHowTheyStand() {
        let m = caughtUp()
        _ = m.openSpools()
        let ask = m.fetches().first { $0["name"] == "mcp.list" }
        #expect(ask != nil)
        m.take(["t": "result", "id": ask?["id"] ?? .null, "ok": true, "value": ["servers": [
            ["name": "acta", "url": "https://acta.aaenz.no/api/mcp", "status": "connected", "sign_in": "signed_in", "signed_in_as": "Elliot", "tools": ["a", "b"], "agents": ["a_1"], "catalog": "acta",
             "badge": ["name": "acta", "initial": "A", "color": "plum"]],
            ["name": "github", "url": "https://x", "status": "failing", "why": "It answered 401."],
        ]]])
        #expect(m.mcpServers.map(\.name) == ["acta", "github"])
        #expect(m.mcpServers[0].status == .connected && m.mcpServers[0].signIn == .signedIn("Elliot"))
        #expect(m.mcpServers[0].badge?.initial == "A")
        #expect(m.mcpServers[1].why == "It answered 401.")
    }
}
