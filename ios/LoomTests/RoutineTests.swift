import Foundation
import Testing
@testable import Loom

/// The routines on the phone (PLAN.md, "The routine's page"), as desktop/src/model.rs and
/// desktop/src/model/watch.rs have them: a routine's event and its words, its schedule,
/// its cost, the list's rows, and what the page's Save sends.
@MainActor
@Suite struct RoutineTests {
    let boards: [(prefix: String, board: String)] = [("LOO", "aaen-studios/loom"), ("CLX", "aaen-studios/clx")]

    /// Answers `cmd` with `f`, as the core does.
    func answer(_ m: Model, _ cmd: JSON?, _ f: [String: JSON]) {
        var frame = f
        frame["t"] = "result"
        frame["id"] = cmd?["id"] ?? .null
        frame["ok"] = JSON(f["error"] == nil)
        m.take(.object(frame))
    }

    @Test func aRoutinesEventSaysWhenItRuns() {
        var e = RoutineEvent.newActa("aaen-studios/loom")
        #expect(e.words(boards) == "When a LOO card moves to Done")
        e.to = "in_progress"
        #expect(e.words(boards) == "When a LOO card moves to In progress")
        e.kind = "created"
        #expect(e.words(boards) == "When a LOO card is made")
        e.kind = "commented"
        #expect(e.words(boards) == "When a LOO card gets a comment")
        e.card = "LOO-206"
        #expect(e.words(boards) == "When LOO-206 gets a comment")
        e.kind = "all"
        #expect(e.words(boards) == "When LOO-206 changes")
        // A board whose prefix isn't known: by its name.
        #expect(RoutineEvent.newActa("aaen-studios/calcite").words(boards) == "When a card on calcite moves to Done")
        var t3 = RoutineEvent.newT3("t3")
        t3.thread = " th_9 "
        #expect(t3.words(boards) == "When the thread th_9 finishes")
    }

    @Test func aRoutinesEventGoesAsRoutinePutTakesItAndComesBackTheSame() throws {
        let e = RoutineEvent.newActa("aaen-studios/loom")
        #expect(e.json == ["integration": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"])
        // As the instance keeps it, with its server.
        let kept: JSON = ["integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"]
        let back = try #require(RoutineEvent(kept))
        #expect(back.board == "aaen-studios/loom" && back.server == "acta")
        #expect(back.json == kept)
        // A new card has no status; a comment on one card names it.
        var made = e
        made.kind = "created"
        #expect(made.json["to"].isNull)
        var card = e
        card.card = "LOO-206"
        card.kind = "commented"
        #expect(card.json["target"] == ["board": "aaen-studios/loom", "card": "LOO-206"])
        var t3 = RoutineEvent.newT3("t3-pc")
        #expect(t3.missing == "Give the thread it waits on.")
        t3.thread = "th_9"
        #expect(t3.missing == nil)
        #expect(t3.json == ["integration": "t3", "server": "t3-pc", "target": ["thread": "th_9"], "kind": "finished"])
        #expect(RoutineEvent.newActa("").missing == "Pick the board it watches.")
        #expect(RoutineEvent(.null) == nil)
        // One kept without its integration is Acta's.
        #expect(RoutineEvent(["target": ["board": "aaen-studios/loom"], "kind": "all"])?.integration == "acta")
    }

    @Test func anEventIsOnThePersonsActaOrTheirT3Servers() {
        #expect(serverIs("acta", catalog: "", name: "acta", url: "https://acta.aaenz.no/api/mcp"))
        #expect(!serverIs("acta", catalog: "", name: "acta", url: ""))
        #expect(serverIs("t3", catalog: "", name: "t3", url: "https://pc.tail1234.ts.net/mcp"))
        #expect(serverIs("t3", catalog: "", name: "T3-vps", url: "https://vps.tail1234.ts.net/mcp"))
        #expect(!serverIs("t3", catalog: "", name: "github", url: "https://api.githubcopilot.com/mcp/"))
        #expect(!serverIs("t3", catalog: "", name: "t3", url: ""))
        // From Spools' entry, under a name of the person's own.
        #expect(serverIs("t3", catalog: "t3", name: "studio", url: "https://pc.tail1234.ts.net/mcp"))
        #expect(serverIs("acta", catalog: "acta", name: "work", url: "https://acta.example.com/api/mcp"))
        #expect(!serverIs("t3", catalog: "acta", name: "work", url: "https://acta.example.com/api/mcp"))
    }

    @Test func t3sServersAreTheInstancesOnceItSaidThem() {
        let m = Model()
        var t3 = McpServer(name: "t3")
        t3.url = "https://pc.tail1234.ts.net/mcp"
        m.mcpServers = [t3]
        // By its name, until the instance says which.
        #expect(m.eventServers("t3") == ["t3"])
        m.t3.servers = ["t3-code", "studio"]
        #expect(m.eventServers("t3") == ["t3-code", "studio"])
        #expect(m.eventServers("acta").isEmpty)
        // On an event once there's a server, and a new one on it with no boards.
        #expect(m.watchable)
        #expect(m.newEvent("acta") == .newT3("t3-code"))
        m.acta.boards = boards
        #expect(m.newEvent("acta") == .newActa("aaen-studios/loom"))
        #expect(m.newEvent("t3") == .newT3("t3-code"))
    }

    @Test func aRoutineOnAnEventAloneSaysItsEvent() {
        let m = Model()
        m.acta.boards = boards
        let r = Routine(["id": "rt_1", "name": "Shipped", "agent": "a_1", "prompt": "Note it.", "schedule": .null,
                         "when": "when a card on aaen-studios/loom moves to Done",
                         "event": ["integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"]])
        #expect(m.routineWhen(r) == "When a LOO card moves to Done")
        // With a schedule too: as the instance says it.
        let both = Routine(["id": "rt_2", "schedule": ["every": 60], "when": "every hour, and when a card on aaen-studios/loom moves to Done",
                            "event": ["integration": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"]])
        #expect(m.routineWhen(both) == both.when)
    }

    @Test func aScheduleComesInAnyFormAndGoesAsRoutinePutTakesIt() {
        let hours = Schedule(["every": 120, "from": "08:00", "to": "20:00", "days": ["mon", "tue", "wed", "thu", "fri"]])
        #expect(hours.repeatKind == "hours" && hours.every == 120 && hours.days == weekdays)
        #expect(hours.json == ["every": 120, "from": "08:00", "to": "20:00", "days": ["mon", "tue", "wed", "thu", "fri"]])
        let days = Schedule(["times": ["08:00", "17:00"]])
        #expect(days.repeatKind == "days" && days.days == everyDay)
        #expect(days.json == ["times": ["08:00", "17:00"]])
        #expect(Schedule(["daily": "07:30"]).json == ["times": ["07:30"]])
        #expect(Schedule(["weekdays": "09:00"]).days == weekdays)
        #expect(Schedule(["times": ["08:00"], "days": ["Saturday", "sun"]]).days == 0b110_0000)
        // None: a routine run by webhook or on an event alone.
        #expect(Schedule(.null).repeatKind.isEmpty)
        #expect(Schedule(.null).json.isNull)
        // A new routine's: every 2 hours from 08:00 to 20:00, every day.
        #expect(Schedule().json == ["every": 120, "from": "08:00", "to": "20:00"])
    }

    @Test func everyFewHoursIsAllDayUntilItHasHours() {
        // No window: All day, with no clocks, and none sent.
        var s = Schedule(["every": 30])
        #expect(s.allDay)
        #expect(s.json == ["every": 30])
        // Some hours: a new routine's, then the person's.
        s.allDay = false
        #expect(s.from == "08:00" && s.to == "20:00")
        s.to = "22:00"
        s.allDay = false
        #expect(s.json == ["every": 30, "from": "08:00", "to": "22:00"])
        // All day again takes the window off.
        s.allDay = true
        #expect(s.json == ["every": 30])
        #expect(!Schedule().allDay)
    }

    @Test func aRoutinesCostSaysWhatItSpends() {
        #expect(RoutineCost(["run": 1_400_000, "day": 5_400_000, "runs": 27, "quiet": 19]).words == "1.4M tokens a run · 5.4M a day · mostly quiet")
        #expect(RoutineCost(["run": 180_000, "day": 180_000, "runs": 7, "quiet": 1]).words == "180k tokens a run · 180k a day")
        #expect(RoutineCost(["runs": 3, "quiet": 3]).words == "No tokens this week · mostly quiet")
        #expect(RoutineCost(.null).words.isEmpty)
        #expect(RoutineCost(["day": 5_400_000, "wasted": true]).warning == "5.4M tokens a day, mostly quiet")
        #expect(RoutineCost(["day": 5_400_000]).warning.isEmpty)
    }

    @Test func aRoutinesRowSaysWhenItRunsOrHowItStands() throws {
        let m = Model()
        m.acta.boards = boards
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = try #require(TimeZone(identifier: "Europe/Oslo"))
        let now = try #require(cal.date(from: DateComponents(year: 2026, month: 11, day: 26, hour: 10)))
        func at(_ day: Int, _ hour: Int) -> Int {
            Int((cal.date(from: DateComponents(year: 2026, month: 11, day: day, hour: hour)) ?? now).timeIntervalSince1970 * 1000)
        }
        #expect(nextWords(at(26, 17), now: now, calendar: cal) == "next at 17:00")
        #expect(nextWords(at(27, 8), now: now, calendar: cal) == "next tomorrow 08:00")
        #expect(nextWords(at(29, 8), now: now, calendar: cal) == "next Sun 08:00")
        #expect(nextWords(at(30, 8) + 7 * 86_400_000, now: now, calendar: cal) == "next 7 Dec 08:00")
        let event: JSON = ["integration": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"]
        #expect(m.routineLine(Routine(["id": "rt_1", "schedule": .null, "event": event, "on": true])).0 == "When a LOO card moves to Done")
        #expect(m.routineLine(Routine(["id": "rt_1", "schedule": .null, "event": event, "on": false])) == ("Off", .off))
        let wasted = Routine(["id": "rt_2", "schedule": ["every": 30], "on": true, "next": JSON(at(26, 11)), "cost": ["day": 5_400_000, "wasted": true]])
        #expect(m.routineLine(wasted) == ("5.4M tokens a day, mostly quiet", .wasted))
        let failed = Routine(["id": "rt_3", "schedule": ["times": ["18:00"]], "on": true, "last": ["status": "failed", "at": 1]])
        #expect(m.routineLine(failed).1 == .failed)
        let timed = Routine(["id": "rt_4", "schedule": ["times": ["17:00"]], "on": true, "next": JSON(at(26, 17)), "last": ["status": "done", "at": 1]])
        #expect(m.routineLine(timed).1 == .on)
    }

    /// Builder, whose program offers a model and an effort, and who picked Sonnet.
    let builder: JSON = ["id": "a_1", "name": "Builder", "acp_agent": "claude",
                         "options": [
                            ["id": "model", "category": "model", "value": "default",
                             "options": [["value": "default", "name": "Default"], ["value": "opus", "name": "Opus"], ["value": "sonnet", "name": "Sonnet"]]],
                            ["id": "thought_level", "category": "thought_level", "value": "high",
                             "options": [["value": "low", "name": "Low"], ["value": "medium", "name": "Medium"], ["value": "high", "name": "High"]]],
                            ["id": "mode", "category": "mode", "value": "ask", "options": [["value": "ask", "name": "Ask"]]],
                         ],
                         "config": ["model": "sonnet"]]

    @Test func aRoutinesModelAndEffortAreWhatItsAgentsProgramOffers() throws {
        let m = caughtUp(agents: [])
        m.take(data("agents", [builder]))
        let a = try #require(m.agent("a_1"))
        #expect(a.routinePicks.map(\.category) == ["model", "thought_level"])
        // Its own pick, then the person's default for its program, then what it starts on.
        #expect(a.chosen("model", defaults: [:]) == "sonnet")
        #expect(a.chosen("thought_level", defaults: [:]) == "high")
        #expect(a.chosen("thought_level", defaults: ["thought_level": "medium"]) == "medium")
        #expect(a.chosen("thought_level", defaults: ["thought_level": "max"]) == "high")
        // The defaults come from defaults.get, and change with defaults.updated.
        m.loadRoutines()
        let ask = try #require(m.fetches().first { $0["name"] == "defaults.get" })
        answer(m, ask, ["value": ["defaults": ["config": ["claude": ["thought_level": "low"]]]]])
        #expect(m.defaults(for: "claude") == ["thought_level": "low"])
        m.take(["t": "event", "kind": "defaults.updated", "seq": 3, "defaults": ["config": ["claude": ["model": "opus"]]]])
        #expect(m.defaults(for: "claude") == ["model": "opus"])
        #expect(m.defaults(for: "codex").isEmpty)
    }

    @Test func routinesAreAskedForAndAskedAgainWhenOneChanges() throws {
        let m = caughtUp()
        m.loadRoutines()
        let out = m.fetches()
        let list = try #require(out.first { $0["name"] == "routine.list" })
        #expect(out.contains { $0["name"] == "mcp.list" })
        #expect(out.contains { $0["name"] == "t3.servers" })
        answer(m, list, ["value": ["routines": [
            ["id": "rt_1", "name": "Shipped", "agent": "a_1", "schedule": .null, "on": true,
             "event": ["integration": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"]],
        ]]])
        #expect(m.routineList.map(\.name) == ["Shipped"])
        // The + sheet's list is the same.
        #expect(m.routines.map(\.id) == ["rt_1"])
        #expect(!m.fetches().contains { $0["name"] == "routine.list" })
        m.take(["t": "event", "kind": "routine.updated", "seq": 3, "routine": ["id": "rt_1"]])
        #expect(m.fetches().contains { $0["name"] == "routine.list" })
        // Delete takes it off at once.
        let gone = m.deleteRoutine("rt_1")
        #expect(gone["name"] == "routine.delete" && gone["args"] == ["id": "rt_1"])
        #expect(m.routineList.isEmpty && m.routines.isEmpty)
        // A refusal is said.
        answer(m, gone, ["error": ["code": "not_found", "message": "No routine rt_1."]])
        #expect(m.notice == "No routine rt_1.")
    }

    /// A routine as `routine.list` gives it, on an event alone, with a gate and a model.
    let shipped: JSON = ["id": "rt_3", "name": "Shipped", "agent": "a_1", "prompt": "Note it.", "schedule": .null, "on": true,
                         "notify": true, "catch_up": false, "post": "thread",
                         "event": ["integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"],
                         "gate": ["command": "./new-cards.sh", "timeout": 60], "model": "opus"]

    @Test func aRoutinesPageOpensAsItWasSaved() {
        let d = RoutineDraft(Routine(shipped))
        #expect(d.repeatKind == "event" && d.eventAlone)
        #expect(d.event.board == "aaen-studios/loom" && d.event.to == "done")
        #expect(d.gate == "./new-cards.sh" && d.gateSeconds == "60")
        #expect(d.model == "opus" && d.effort.isEmpty)
        #expect(d.thread && d.notify && !d.catchUp && !d.webhook)
        // Its schedule, when it has none, starts as a new routine's.
        #expect(d.schedule == Schedule())
        // By its webhook alone: Webhook.
        let hook = RoutineDraft(Routine(["id": "rt_9", "schedule": .null, "hook": "h_1", "on": true]))
        #expect(hook.repeatKind == "webhook" && hook.webhook && hook.byHookAlone)
        let timed = RoutineDraft(Routine(["id": "rt_8", "schedule": ["times": ["08:00"]], "on": true]))
        #expect(timed.repeatKind == "days" && !timed.eventAlone)
        // A new one's: on, notifying and catching up, every 2 hours.
        let new = RoutineDraft(agent: "a_1")
        #expect(new.id.isEmpty && new.on && new.notify && new.catchUp && new.repeatKind == "hours")
    }

    @Test func savingSendsRoutinePutAsTheDesktopDoes() throws {
        let m = caughtUp(agents: [])
        m.take(data("agents", [builder]))
        var d = RoutineDraft(Routine(shipped))
        let f = try #require(m.saveRoutine(d))
        let args = f["args"]
        #expect(f["name"] == "routine.put")
        #expect(args["id"] == "rt_3" && args["agent"] == "a_1" && args["prompt"] == "Note it.")
        #expect(args["schedule"].isNull)
        #expect(args["event"] == ["integration": "acta", "server": "acta", "target": ["board": "aaen-studios/loom"], "kind": "moved", "to": "done"])
        #expect(args["gate"] == ["command": "./new-cards.sh", "timeout": 60])
        #expect(args["model"] == "opus" && args["effort"] == "")
        #expect(args["post"] == "thread" && args["webhook"] == false && args["catch_up"] == false)
        // Its flow isn't the phone's: it's left as it is.
        #expect(args["flow"].isNull)
        // Off the event it ran on alone, onto a schedule: the event goes.
        d.pick(repeat: "days", fresh: .newActa("x"))
        d.add(time: "17:00")
        d.add(time: "08:00")
        d.add(time: "17:00")
        let timed = try #require(m.saveRoutine(d))
        #expect(timed["args"]["schedule"] == ["times": ["08:00", "17:00"]])
        guard case .object(let sent) = timed["args"] else { Issue.record("No args"); return }
        #expect(sent["event"] == .some(.null))
        // By webhook alone: no schedule, and its webhook.
        d.pick(repeat: "webhook", fresh: .newActa("x"))
        let hook = try #require(m.saveRoutine(d))
        #expect(hook["args"]["schedule"].isNull && hook["args"]["webhook"] == true)
        // A gate with no command goes; seconds that aren't a number are said.
        d.gate = " "
        #expect(try #require(m.saveRoutine(d))["args"]["gate"] == .null)
        d.gate = "./check.sh"
        d.gateSeconds = "soon"
        #expect(m.saveRoutine(d) == nil)
        #expect(m.notice == "Write the seconds its gate may take as a number.")
        d.gateSeconds = ""
        #expect(try #require(m.saveRoutine(d))["args"]["gate"] == ["command": "./check.sh"])
        // A model its agent's program doesn't offer goes back to the agent's own.
        d.model = "gpt-5"
        #expect(try #require(m.saveRoutine(d))["args"]["model"] == "")
    }

    @Test func onAnEventSavesThePickedEventOrSaysWhatsMissing() throws {
        let m = caughtUp(agents: [])
        m.take(data("agents", [builder]))
        var d = RoutineDraft(agent: "a_1")
        d.name = " Shipped "
        d.pick(repeat: "event", fresh: .newActa("aaen-studios/loom"))
        d.pick(kind: "created")
        let made = try #require(m.saveRoutine(d))
        #expect(made["args"]["id"].isNull && made["args"]["name"] == "Shipped")
        #expect(made["args"]["schedule"].isNull)
        #expect(made["args"]["event"] == ["integration": "acta", "target": ["board": "aaen-studios/loom"], "kind": "created"])
        // Another board is any card's.
        d.event.card = "LOO-206"
        d.pick(board: "aaen-studios/clx")
        #expect(d.event.card.isEmpty)
        // A move is to Done until another status is picked.
        d.event.to = ""
        d.pick(kind: "moved")
        #expect(d.event.to == "done")
        // On T3 with no thread: said, and nothing sent.
        d.event = .newT3("t3")
        #expect(m.saveRoutine(d) == nil)
        #expect(m.notice == "Give the thread it waits on.")
        // With no agent: said.
        var none = RoutineDraft(agent: "")
        none.name = "x"
        #expect(m.saveRoutine(none) == nil)
        #expect(m.notice == "Pick the agent it runs.")
    }

    // MARK: Its webhook, its next runs and its history

    @Test func aWebhooksAddressIsOnTheInstancesOrItsPathAlone() {
        #expect(hookAddress(server: "https://loom.example.com", hook: "h_3") == "https://loom.example.com/hooks/h_3")
        #expect(hookAddress(server: " loom.example.com/ ", hook: "h_3") == "https://loom.example.com/hooks/h_3")
        #expect(hookAddress(server: "http://192.168.1.4:8787", hook: "h_3") == "http://192.168.1.4:8787/hooks/h_3")
        // Signed in with a code: the phone doesn't know the address.
        #expect(hookAddress(server: "", hook: "h_3") == "/hooks/h_3")
        let r = Routine(["id": "rt_1", "schedule": .null, "hook": "h_3", "secret": "whsec_1"])
        #expect(r.hook == "h_3" && r.secret == "whsec_1")
    }

    @Test func aRunSaysHowItEndedInAWordALineAndHowLongItTook() {
        let posted = RoutineRun(["run": "rr_1", "at": 1000, "started": 2000, "ended": 14_000, "status": "done", "posted": ["Shipped LOO-383.\nMore."], "said": "Done."])
        #expect(posted.mark == .posted && posted.pill == "Posted")
        #expect(posted.line == "Shipped LOO-383.")
        #expect(posted.took == "12 s")
        // Its gate found nothing: quiet, and nothing new.
        let quiet = RoutineRun(["run": "rr_2", "at": 1000, "ended": 4000, "status": "quiet", "woke": false])
        #expect(quiet.mark == .quiet && quiet.pill == "Quiet" && quiet.line == "Nothing new")
        // One that failed says why.
        let failed = RoutineRun(["run": "rr_3", "at": 1000, "started": 1000, "ended": 126_000, "status": "failed", "error": "Acta didn't answer.", "said": "Trying."])
        #expect(failed.mark == .failed && failed.line == "Acta didn't answer." && failed.took == "2 min 5 s")
        // Done with nothing posted, its answer handed on.
        let handed = RoutineRun(["run": "rr_4", "status": "done", "said": "\n  Three cards moved.\n"])
        #expect(handed.pill == "Done" && handed.line == "Three cards moved." && handed.took.isEmpty)
        #expect(RoutineRun(["status": "running"]).mark == .going)
        #expect(RoutineRun(["status": "skipped", "at": 1, "ended": 2]).took.isEmpty)
        #expect(RoutineRun(["status": "cancelled"]).pill == "Stopped")
        #expect(RoutineRun(["status": "done", "at": 1, "ended": 3_780_001]).took == "1 h 3 min")
    }

    @Test func aRoutinesRunsAreAskedForWhileItsPageIsOpen() throws {
        let m = caughtUp()
        let ask = try #require(m.loadRoutineRuns("rt_1"))
        #expect(ask["name"] == "routine.runs" && ask["args"] == ["id": "rt_1", "limit": 100])
        answer(m, ask, ["value": ["runs": [["run": "rr_2", "status": "quiet"], ["run": "rr_1", "status": "done"]]]])
        #expect(m.routineRuns.map(\.run) == ["rr_2", "rr_1"])
        // A routine that changed asks again, while its page is open.
        m.take(["t": "event", "kind": "routine.updated", "seq": 3, "routine": ["id": "rt_1"]])
        #expect(m.fetches().contains { $0["name"] == "routine.runs" })
        // Closed: forgotten, and not asked again.
        #expect(m.loadRoutineRuns(nil) == nil)
        #expect(m.routineRuns.isEmpty)
        m.take(["t": "event", "kind": "routine.updated", "seq": 4, "routine": ["id": "rt_1"]])
        #expect(!m.fetches().contains { $0["name"] == "routine.runs" })
        // Another routine's page doesn't show the last one's runs.
        _ = m.loadRoutineRuns("rt_1")
        answer(m, ask, ["value": ["runs": [["run": "rr_2"]]]])
        _ = m.loadRoutineRuns("rt_2")
        #expect(m.routineRuns.isEmpty)
    }

    @Test func nextIsTheRoutinesOwnWhileItsScheduleIsAsSavedAndElseTheInstances() throws {
        let m = caughtUp()
        let saved = Routine(["id": "rt_1", "schedule": ["every": 30], "on": true, "upcoming": [100, 200, 300]])
        var d = RoutineDraft(saved)
        // As saved: its own, which count from its last run.
        #expect(m.nextRuns(saved, d) == [100, 200, 300])
        // Changed: the instance is asked, once, and its answer shown.
        d.schedule.every = 60
        let ask = try #require(m.askScheduleNext(d.scheduleJSON))
        #expect(ask["name"] == "routine.next" && ask["args"]["schedule"] == ["every": 60])
        #expect(m.askScheduleNext(d.scheduleJSON) == nil)
        #expect(m.nextRuns(saved, d).isEmpty)
        answer(m, ask, ["value": ["next": [400, 500, 600]]])
        #expect(m.nextRuns(saved, d) == [400, 500, 600])
        // An answer to an older ask is left.
        d.schedule.every = 90
        _ = m.askScheduleNext(d.scheduleJSON)
        answer(m, ask, ["value": ["next": [1, 2, 3]]])
        #expect(m.nextRuns(saved, d).isEmpty)
        // No schedule: nothing asked.
        d.pick(repeat: "webhook", fresh: .newActa("x"))
        #expect(d.scheduleJSON.isNull)
        #expect(m.askScheduleNext(d.scheduleJSON) == nil)
    }

    @Test func nextSaysEachDayOnce() throws {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = try #require(TimeZone(identifier: "Europe/Oslo"))
        let now = try #require(cal.date(from: DateComponents(year: 2026, month: 11, day: 26, hour: 10)))
        func at(_ day: Int, _ hour: Int) -> Int {
            Int((cal.date(from: DateComponents(year: 2026, month: 11, day: day, hour: hour)) ?? now).timeIntervalSince1970 * 1000)
        }
        #expect(nextRunsWords([at(26, 16), at(26, 18), at(26, 20)], now: now, calendar: cal) == "Today 16:00, 18:00, 20:00")
        #expect(nextRunsWords([at(26, 22), at(27, 8), at(29, 8)], now: now, calendar: cal) == "Today 22:00, Tomorrow 08:00, Sun 08:00")
        #expect(nextRunsWords([at(30, 8) + 7 * 86_400_000], now: now, calendar: cal) == "7 Dec 08:00")
        #expect(nextRunsWords([], now: now, calendar: cal).isEmpty)
    }
}
