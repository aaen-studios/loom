import Foundation

// The person's routines on the phone (PLAN.md, "Routines" and "The routine's page"), as
// desktop/src/model.rs and desktop/src/model/watch.rs have them: each routine as
// `routine.list` gives it, its schedule and its event as the routine's page edits them,
// and what the page sends to save it.

/// A routine's event, as `routine.put` takes it and `routine.list` gives it: what outside
/// loom it runs on.
struct RoutineEvent: Equatable, Sendable {
    /// `acta` or `t3`; empty before one is picked.
    var integration = ""
    /// The person's MCP server it's on; empty for their Acta.
    var server = ""
    var board = ""
    var card = ""
    var thread = ""
    /// For Acta: moved, created, commented, assigned or all. For T3: finished.
    var kind = ""
    /// For a move to one status.
    var to = ""

    init() {}

    /// A routine's event, or none.
    init?(_ v: JSON) {
        guard case .object = v else { return nil }
        let i = v["integration"].text
        integration = i.isEmpty ? "acta" : i
        server = v["server"].text
        board = v["target"]["board"].text
        card = v["target"]["card"].text
        thread = v["target"]["thread"].text
        kind = v["kind"].text
        to = v["to"].text
    }

    /// A new event's, as "On an event" is first picked: a card on `board` (the first of
    /// the person's) moving to Done.
    static func newActa(_ board: String) -> RoutineEvent {
        var e = RoutineEvent()
        e.integration = "acta"
        e.board = board
        e.kind = "moved"
        e.to = "done"
        return e
    }

    /// A new event on T3: the thread given finishing.
    static func newT3(_ server: String) -> RoutineEvent {
        var e = RoutineEvent()
        e.integration = "t3"
        e.server = server
        e.kind = "finished"
        return e
    }

    /// As `routine.put` takes it.
    var json: JSON {
        let target: JSON
        if integration == "t3" {
            target = ["thread": JSON(thread.trimmingCharacters(in: .whitespaces))]
        } else if card.isEmpty {
            target = ["board": JSON(board)]
        } else {
            target = ["board": JSON(board), "card": JSON(card)]
        }
        var v: [String: JSON] = ["integration": JSON(integration), "target": target, "kind": JSON(kind)]
        if !server.isEmpty { v["server"] = JSON(server) }
        if kind == "moved" && !to.isEmpty { v["to"] = JSON(to) }
        return .object(v)
    }

    /// What's missing before it can be saved, as Save says it.
    var missing: String? {
        switch integration {
        case "t3" where server.isEmpty: return "Pick the T3 server it's on."
        case "t3" where thread.trimmingCharacters(in: .whitespaces).isEmpty: return "Give the thread it waits on."
        case "t3": return nil
        default: return board.isEmpty ? "Pick the board it watches." : nil
        }
    }

    /// When it runs, in words, for the routines list: "When a LOO card moves to Done".
    /// `boards` are the person's, each its prefix and `workspace/board`.
    func words(_ boards: [(prefix: String, board: String)]) -> String {
        if integration == "t3" {
            return "When the thread \(thread.trimmingCharacters(in: .whitespaces)) finishes"
        }
        let any = boards.first { $0.board == board }.map { "a \($0.prefix) card" } ?? "a card on \(boardLabel(board))"
        let subject = card.isEmpty ? any : card
        switch kind {
        case "moved" where !to.isEmpty: return "When \(subject) moves to \(statusLabel(to))"
        case "moved": return "When \(subject) moves"
        case "created": return "When \(any) is made"
        case "commented": return "When \(subject) gets a comment"
        case "assigned": return "When \(subject) is assigned"
        default: return "When \(subject) changes"
        }
    }
}

/// Acta's statuses, as a move's `to` names them, and as the routine's page and a routine's
/// words say them.
let routineStatuses: [(key: String, label: String)] = [
    ("todo", "Todo"), ("in_progress", "In progress"), ("review", "Review"), ("done", "Done"), ("canceled", "Canceled"),
]

private func statusLabel(_ key: String) -> String {
    routineStatuses.first { $0.key == key }?.label ?? key.replacingOccurrences(of: "_", with: " ")
}

/// A board as a routine names it: its part after the workspace.
func boardLabel(_ board: String) -> String {
    board.split(separator: "/").last.map(String.init) ?? board
}

/// Whether an MCP server over HTTP is the person's Acta, or a T3 server of theirs: from
/// Spools' entry for it (`catalog`), or else by its address for Acta and its name for T3.
func serverIs(_ integration: String, catalog: String, name: String, url: String) -> Bool {
    if url.isEmpty { return false }
    if !catalog.isEmpty && catalog == integration { return true }
    switch integration {
    case "acta": return url.hasPrefix("https://\(actaHost)/")
    case "t3": return name.lowercased().hasPrefix("t3")
    default: return false
    }
}

/// The days of the week as a schedule names them, Monday first: bit 0 is Monday.
let dayKeys = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
let everyDay: UInt8 = 0b111_1111
let weekdays: UInt8 = 0b001_1111

/// A routine's schedule as the routine's page edits it (protocol.md, `routine.put`). The
/// instance reads it and works out when it runs; this only carries the fields.
struct Schedule: Equatable, Sendable {
    /// "hours" (every few hours or minutes), "days" (at times on chosen days), or "" for
    /// none: a routine that only runs by webhook or on an event.
    var repeatKind = "hours"
    /// The days it runs on, bit 0 Monday to bit 6 Sunday.
    var days = everyDay
    /// Its times of day, "HH:MM", for "days".
    var times = ["08:00"]
    /// Minutes apart, for "hours".
    var every = 120
    /// The window of the day it runs in, "HH:MM", for "hours". Both empty for one every
    /// N minutes from when it was made.
    var from = "08:00"
    var to = "20:00"

    /// A new routine's: every 2 hours from 08:00 to 20:00, every day.
    init() {}

    /// A schedule as the instance gives it, in any of its forms.
    init(_ v: JSON) {
        var days = everyDay
        if case .array(let names) = v["days"] {
            days = names.compactMap { n in dayKeys.firstIndex { n.text.lowercased().hasPrefix($0) } }.reduce(0) { $0 | (1 << $1) }
        }
        if let every = v["every"].int {
            repeatKind = "hours"
            self.days = days
            self.every = every
            from = v["from"].text
            to = v["to"].text
            return
        }
        if case .array(let t) = v["times"] {
            times = t.compactMap(\.string)
            self.days = days
        } else if let t = v["daily"].string {
            times = [t]
        } else if let t = v["weekdays"].string {
            times = [t]
            self.days = weekdays
        } else {
            repeatKind = ""
            return
        }
        repeatKind = "days"
    }

    /// The schedule as `routine.put` takes it, or null for none.
    var json: JSON {
        let names = (0..<7).filter { days & (1 << $0) != 0 }.map { dayKeys[$0] }
        switch repeatKind {
        case "days":
            var v: [String: JSON] = ["times": JSON(times)]
            if days != everyDay { v["days"] = JSON(names) }
            return .object(v)
        case "hours":
            var v: [String: JSON] = ["every": JSON(every)]
            let f = from.trimmingCharacters(in: .whitespaces), t = to.trimmingCharacters(in: .whitespaces)
            if !f.isEmpty { v["from"] = JSON(f) }
            if !t.isEmpty { v["to"] = JSON(t) }
            if days != everyDay { v["days"] = JSON(names) }
            return .object(v)
        default:
            return .null
        }
    }
}

/// What a routine's runs cost over the last week (protocol.md, `routine.list`): tokens a
/// run and a day, how many ran and how many of them were quiet, and whether it spends a
/// lot for nothing, its runs mostly quiet.
struct RoutineCost: Equatable, Sendable {
    var run = 0
    var day = 0
    var runs = 0
    var quiet = 0
    var wasted = false

    init() {}

    init(_ v: JSON) {
        run = v["run"].int ?? 0
        day = v["day"].int ?? 0
        runs = v["runs"].int ?? 0
        quiet = v["quiet"].int ?? 0
        wasted = v["wasted"].bool ?? false
    }

    /// What it costs, for its page's head: "1.4M tokens a run · 5.4M a day · mostly
    /// quiet". Empty before it has run.
    var words: String {
        if runs == 0 { return "" }
        var w = day == 0 ? "No tokens this week" : "\(ContextFill.short(run)) tokens a run · \(ContextFill.short(day)) a day"
        if quiet * 2 > runs { w += " · mostly quiet" }
        return w
    }

    /// Its row's line in the list, for one that spends a lot for nothing: "5.4M tokens a
    /// day, mostly quiet". Empty for any other.
    var warning: String {
        wasted ? "\(ContextFill.short(day)) tokens a day, mostly quiet" : ""
    }
}

/// One of the person's routines, as `routine.list` gives it: a prompt an agent is sent on
/// a schedule, by webhook or on an event.
struct Routine: Equatable, Sendable, Identifiable {
    var id: String
    var name: String
    var agent: String
    var prompt: String
    var schedule: Schedule
    /// When it runs, in words, as the instance says it: "every 30 minutes".
    var when: String
    var notify: Bool
    var on: Bool
    var catchUp: Bool
    /// Its webhook's id, when it has one.
    var hook: String
    /// When it runs next (Unix ms), or 0 while it's off.
    var next: Int
    /// How its last run went, and when.
    var lastStatus: String
    var lastAt: Int
    /// Where it posts: "chat", its agent's chat, or "thread", a thread of its own.
    var post: String
    var event: RoutineEvent?
    /// Its gate's command, and the seconds it may take. Empty for none.
    var gate: String
    var gateSeconds: Int
    /// Its runs' model and effort, values its agent's program offers; empty for its
    /// agent's own.
    var model: String
    var effort: String
    var cost: RoutineCost

    init(_ r: JSON) {
        id = r["id"].text
        name = r["name"].text
        agent = r["agent"].text
        prompt = r["prompt"].text
        schedule = Schedule(r["schedule"])
        when = r["when"].text
        notify = r["notify"].bool ?? false
        on = r["on"].bool ?? true
        catchUp = r["catch_up"].bool ?? true
        hook = r["hook"].text
        next = r["next"].int ?? 0
        lastStatus = r["last"]["status"].text
        lastAt = r["last"]["at"].int ?? 0
        post = r["post"].string ?? "chat"
        event = RoutineEvent(r["event"])
        gate = r["gate"]["command"].text
        gateSeconds = r["gate"]["timeout"].int ?? 0
        model = r["model"].text
        effort = r["effort"].text
        cost = RoutineCost(r["cost"])
    }
}

/// One of the things an agent's program lets the person pick, such as its model, as the
/// agents list gives its `options`.
struct ConfigOption: Equatable, Sendable {
    var id: String
    var name: String
    /// "model", "thought_level", "mode" or another the program names.
    var category: String
    /// What a new session starts on.
    var start: String
    /// Each value: what's sent, and its name.
    var values: [ConfigValue]

    init(_ o: JSON) {
        id = o["id"].text
        name = o["name"].text
        category = o["category"].text
        start = o["value"].text
        values = o["options"].array.map { ConfigValue(value: $0["value"].text, name: $0["name"].text) }
    }
}

struct ConfigValue: Equatable, Sendable, Hashable {
    var value: String
    var name: String
}

/// An object's string values, by key: an agent's `config`, or the defaults' picks for one
/// program.
func stringsOf(_ v: JSON) -> [String: String] {
    guard case .object(let o) = v else { return [:] }
    return o.compactMapValues(\.string)
}

extension Agent {
    /// Its model and its effort, of what its program offers, as a routine's page picks
    /// them.
    var routinePicks: [ConfigOption] {
        ["model", "thought_level"].flatMap { c in options.filter { $0.category == c && !$0.values.isEmpty } }
    }

    /// The value option `id` is on: the agent's own pick, or the person's default for its
    /// program (`defaults`, by option) where the program offers it, or what the program
    /// starts on.
    func chosen(_ id: String, defaults: [String: String]) -> String {
        if let v = config[id] { return v }
        let option = options.first { $0.id == id }
        if let v = defaults[id], option?.values.contains(where: { $0.value == v }) == true { return v }
        return option?.start ?? ""
    }
}

/// The routine's page as the person changes it, to save: its fields as the page shows
/// them (desktop/src/model/drafts.rs's `RoutineDraft`).
struct RoutineDraft: Equatable, Sendable {
    /// Empty for a new routine.
    var id = ""
    var name = ""
    var agent = ""
    var prompt = ""
    /// "hours", "days", "webhook" or "event".
    var repeatKind = "hours"
    var schedule = Schedule()
    var event = RoutineEvent()
    var gate = ""
    var gateSeconds = ""
    var model = ""
    var effort = ""
    var on = true
    var notify = true
    var catchUp = true
    var thread = false
    var webhook = false
    /// Whether it was saved running on an event alone, whose event goes once another
    /// Repeat is picked.
    var eventAlone = false

    /// A new routine's, for `agent`: on, notifying and catching up, every 2 hours from
    /// 08:00 to 20:00.
    init(agent: String) {
        self.agent = agent
    }

    /// Routine `r`'s, as its page opens: one that runs on an event alone with "On an
    /// event" picked, and one by its webhook alone with "Webhook".
    init(_ r: Routine) {
        id = r.id
        name = r.name
        agent = r.agent
        prompt = r.prompt
        schedule = r.schedule
        event = r.event ?? RoutineEvent()
        gate = r.gate
        gateSeconds = r.gate.isEmpty || r.gateSeconds <= 0 ? "" : String(r.gateSeconds)
        model = r.model
        effort = r.effort
        on = r.on
        notify = r.notify
        catchUp = r.catchUp
        thread = r.post == "thread"
        webhook = !r.hook.isEmpty
        eventAlone = r.event != nil && r.schedule.repeatKind.isEmpty
        repeatKind = eventAlone ? "event" : r.schedule.repeatKind.isEmpty ? "webhook" : r.schedule.repeatKind
        // A schedule it doesn't have yet starts as a new routine's.
        if r.schedule.repeatKind.isEmpty { schedule = Schedule() }
    }

    /// Whether it runs by its webhook alone, which "Webhook too" isn't asked for.
    var byHookAlone: Bool { repeatKind == "webhook" }

    /// Picks Repeat `kind`: "On an event" first picked starts on `fresh`.
    mutating func pick(repeat kind: String, fresh: @autoclosure () -> RoutineEvent) {
        repeatKind = kind
        if kind == "event" && event.integration.isEmpty { event = fresh() }
        if kind == "hours" || kind == "days" { schedule.repeatKind = kind }
    }

    /// Picks a board: one card's event is on its board, so another board is any card's.
    mutating func pick(board: String) {
        if event.board != board { event.card = "" }
        event.board = board
    }

    /// Picks what happens on the board; a move is to Done until another status is picked.
    mutating func pick(kind: String) {
        event.kind = kind
        if kind == "moved" && event.to.isEmpty { event.to = "done" }
    }

    /// Adds a time of day, kept in order and once.
    mutating func add(time: String) {
        if !schedule.times.contains(time) {
            schedule.times.append(time)
            schedule.times.sort()
        }
    }

    /// `routine.put` as it saves, with `id` for the command, or what's missing, as the
    /// desktop's Save says it. `agents` are the person's, for a model or effort its agent's
    /// program no longer offers, which goes back to the agent's own.
    func put(id cmd: String, agents: [Agent]) -> RoutineSave {
        if agent.isEmpty { return .missing("Pick the agent it runs.") }
        var s = schedule
        switch repeatKind {
        case "hours", "days": s.repeatKind = repeatKind
        default: s.repeatKind = ""
        }
        let command = gate.trimmingCharacters(in: .whitespacesAndNewlines)
        let seconds = gateSeconds.trimmingCharacters(in: .whitespaces)
        var timeout = 0
        if !command.isEmpty && !seconds.isEmpty {
            guard let n = Int(seconds), n > 0 else { return .missing("Write the seconds its gate may take as a number.") }
            timeout = n
        }
        var args: [String: JSON] = [
            "name": JSON(name.trimmingCharacters(in: .whitespaces)), "agent": JSON(agent), "prompt": JSON(prompt),
            "schedule": s.json, "notify": JSON(notify), "on": JSON(on), "catch_up": JSON(catchUp),
            "webhook": JSON(webhook || byHookAlone), "post": JSON(thread ? "thread" : "chat"),
        ]
        if !id.isEmpty { args["id"] = JSON(id) }
        // On an event: the one picked. Off one it ran on alone: taken away. One it runs
        // on beside its schedule stays as it is.
        if repeatKind == "event" {
            if let why = event.missing { return .missing(why) }
            args["event"] = event.json
        } else if eventAlone {
            args["event"] = .null
        }
        args["gate"] = command.isEmpty ? .null : timeout > 0 ? ["command": JSON(command), "timeout": JSON(timeout)] : ["command": JSON(command)]
        let options = agents.first { $0.id == agent }?.routinePicks ?? []
        func offered(_ category: String, _ value: String) -> String {
            guard let o = options.first(where: { $0.category == category }) else { return value }
            return o.values.contains { $0.value == value } ? value : ""
        }
        args["model"] = JSON(offered("model", model))
        args["effort"] = JSON(offered("thought_level", effort))
        return .frame(["t": "cmd", "id": JSON(cmd), "name": "routine.put", "args": .object(args)])
    }
}

/// What Save on a routine's page comes to: the frame to send, or what's missing.
enum RoutineSave: Equatable {
    case frame(JSON)
    case missing(String)
}

/// A next run, for a routine's row: "next at 08:00" today, "next tomorrow 08:00", "next
/// Fri 08:00" within the week, and "next 3 Dec 08:00" after.
func nextWords(_ ms: Int, now: Date = .now, calendar: Calendar = .current) -> String {
    let date = Date(timeIntervalSince1970: Double(ms) / 1000)
    let f = DateFormatter()
    f.locale = Locale(identifier: "en_US_POSIX")
    f.calendar = calendar
    f.timeZone = calendar.timeZone
    f.dateFormat = "HH:mm"
    let time = f.string(from: date)
    let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: now), to: calendar.startOfDay(for: date)).day ?? 0
    switch days {
    case 0: return "next at \(time)"
    case 1: return "next tomorrow \(time)"
    case 2...6:
        f.dateFormat = "EEE HH:mm"
        return "next \(f.string(from: date))"
    default:
        f.dateFormat = "d MMM HH:mm"
        return "next \(f.string(from: date))"
    }
}

/// How a routine's row in the list stands: its line, and its dot.
enum RoutineMark: Equatable {
    case on, wasted, failed, off
}

extension Model {
    /// The person's MCP servers an event can be on, by name: their Acta, or their T3
    /// servers, as the instance last said them (`t3.servers`), or by their names until it
    /// has.
    func eventServers(_ integration: String) -> [String] {
        if integration == "t3" && !t3.servers.isEmpty { return t3.servers }
        return mcpServers.filter { serverIs(integration, catalog: $0.catalog, name: $0.name, url: $0.url) }.map(\.name)
    }

    /// Whether a routine can run on an event: the person has an Acta board, or a T3 server.
    var watchable: Bool {
        !acta.boards.isEmpty || !eventServers("t3").isEmpty
    }

    /// A new event for "On an event": on Acta, a card on the person's first board moving
    /// to Done; on T3, the first T3 server's thread finishing. Acta when T3 is asked for
    /// and there's none, and T3 when Acta is and the person has no boards.
    func newEvent(_ integration: String) -> RoutineEvent {
        let t3 = eventServers("t3")
        if (integration == "t3" || acta.boards.isEmpty), let first = t3.first {
            return .newT3(first)
        }
        return .newActa(acta.boards.first?.board ?? "")
    }

    /// When a routine runs, in words, for its row and its page: its event's for one that
    /// runs on an event alone, or else as the instance says it.
    func routineWhen(_ r: Routine) -> String {
        if let e = r.event, r.schedule.repeatKind.isEmpty { return e.words(acta.boards) }
        return r.when
    }

    /// A routine's line in the list, and how it stands: one that spends a lot for nothing
    /// says so; one whose last run failed says when; one that's off says so; one with no
    /// schedule says what runs it; and one on a schedule, when it runs next.
    func routineLine(_ r: Routine, now: Date = .now) -> (String, RoutineMark) {
        if !r.on { return ("Off", .off) }
        if r.cost.wasted { return (r.cost.warning, .wasted) }
        switch r.lastStatus {
        case "", "done", "quiet", "running", "waiting", "skipped": break
        default: return ("Failed \(When.time(r.lastAt))", .failed)
        }
        if r.schedule.repeatKind.isEmpty { return (routineWhen(r), .on) }
        return (r.next > 0 ? nextWords(r.next, now: now) : r.when, .on)
    }

    /// Asks for what Settings → Routines needs: the person's routines, their MCP servers
    /// and T3's for "On an event", and the person's defaults for each routine's model and
    /// effort.
    func loadRoutines() {
        routinesStale = true
        mcpStale = true
        pending.append(askT3Servers())
        pending.append(["t": "cmd", "id": JSON(id("app.defaults")), "name": "defaults.get", "args": [:]])
    }

    /// Saves the routine's page, new or changed: the frame to send, or what's missing,
    /// which shows as a notice.
    func saveRoutine(_ d: RoutineDraft) -> JSON? {
        switch d.put(id: id("app.routine_put"), agents: agents) {
        case .frame(let f): return f
        case .missing(let why):
            notice = why
            return nil
        }
    }

    func deleteRoutine(_ routine: String) -> JSON {
        routineList.removeAll { $0.id == routine }
        routines.removeAll { $0.id == routine }
        return ["t": "cmd", "id": JSON(id("app.routine_delete")), "name": "routine.delete", "args": ["id": JSON(routine)]]
    }

    /// The person's routines as `routine.list` gave them, for the + sheet and Settings.
    func takeRoutines(_ list: [JSON]) {
        routineList = list.map(Routine.init)
        routines = list.map {
            PlusRoutine(id: $0["id"].text, name: $0["name"].text, agent: $0["agent"].text, status: $0["last"]["status"].text, at: $0["last"]["at"].int ?? 0)
        }
    }

    /// The answers to `routine.put`, `routine.delete` and `defaults.get`. Gives whether it
    /// was one.
    func routineResult(_ f: JSON) -> Bool {
        let id = f["id"].text
        let error = f["error"].isNull ? nil : f["error"]["message"].string ?? f["error"]["code"].text
        if id.hasPrefix("app.defaults") {
            if error == nil { defaultPicks = f["value"]["defaults"]["config"] }
            return true
        }
        if id.hasPrefix("app.routine_put") || id.hasPrefix("app.routine_delete") {
            if let error { notice = error }
            routinesStale = true
            return true
        }
        return false
    }

    /// The person's default picks for `program`, by option.
    func defaults(for program: String) -> [String: String] {
        stringsOf(defaultPicks[program])
    }
}
