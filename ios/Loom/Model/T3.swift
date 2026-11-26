import Foundation

// T3 Code's threads in the chat (PLAN.md, "T3 threads"), as desktop/src/model/t3.rs has
// them: a thread a message names, or one an agent's T3 tool call started, as the instance
// keeps it on the message (`threads`), and what its card shows: how the strand beside its
// title lies, the line under the title, the time or what it waits on at the title's end,
// its latest step or why it failed, its pull requests, and the buttons that answer it.
// And the sheet that starts a thread. The instance looks threads up, follows them and
// answers them; this reads what it kept and says what to send.

/// How a thread's link in a message's words starts: `t3-thread://v2/ENV/ID`.
private let threadLinkStart = "t3-thread://v2/"

/// Where a thread is.
enum ThreadState: Equatable, Sendable {
    case working
    /// A question or an approval waits on the person.
    case waiting
    case done
    case failed
}

/// How a card's strand lies, and the colour it, its time and its edge are drawn in: it
/// waves in sky while working, knots in warn while it waits, lies straight in ok once
/// done and sags in bad when it failed. Nothing is added to it.
enum Strand: Equatable, Sendable {
    case waves, knots, straight, sags
}

/// A pull request the thread opened.
struct ThreadPR: Equatable, Sendable {
    var number: Int
    var url: String
}

/// What a thread waits on.
struct ThreadAsk: Equatable, Sendable {
    /// `question` or `approval`.
    var kind: String
    var request: String
    var text: String
    var choices: [String]
    /// An approval loom couldn't answer: the person answers it in T3.
    var inT3: Bool
}

/// What a button on a card does.
enum ThreadAct: Equatable, Sendable {
    /// Answers its question with this choice.
    case answer(String)
    /// Answers its approval: `accept` or `decline`.
    case decide(String)
    /// Opens its page in T3's app.
    case open
    /// Asks it to try again.
    case retry
}

/// A button on a card, the main one filled.
struct ThreadButton: Equatable, Sendable {
    var label: String
    var act: ThreadAct
    var main: Bool
}

/// The line in a quiet box under a card's head: its latest step, or why it failed.
enum ThreadQuiet: Equatable, Sendable {
    case step(String)
    case reason(String)
}

/// A thread as a message keeps it (protocol.md, "T3 threads").
struct ThreadCard: Equatable, Sendable, Identifiable {
    var thread: String
    var server = ""
    var title = ""
    var project = ""
    var model = ""
    var effort = ""
    var state = ThreadState.working
    var step = ""
    /// When its latest run started and ended, in Unix ms, or 0.
    var started = 0
    var ended = 0
    var prs: [ThreadPR] = []
    var ask: ThreadAsk?
    var reason = ""
    /// Its page in T3's app.
    var url = ""
    /// None of the person's T3 servers has it.
    var missing = false

    var id: String { thread }

    init(_ v: JSON) {
        switch v["state"].text {
        case "waiting": state = .waiting
        case "done": state = .done
        case "failed": state = .failed
        default: state = .working
        }
        thread = v["thread"].text
        server = v["server"].text
        title = v["title"].text
        project = v["project"].text
        model = v["model"].text
        effort = v["effort"].text
        step = v["step"].text
        started = v["started"].int ?? 0
        ended = v["ended"].int ?? 0
        prs = v["prs"].array.map { ThreadPR(number: $0["number"].int ?? 0, url: $0["url"].text) }.filter { $0.number > 0 }
        let a = v["ask"]
        if state == .waiting, case .object = a {
            ask = ThreadAsk(kind: a["kind"].text, request: a["request"].text, text: a["text"].text, choices: a["choices"].array.compactMap(\.string), inT3: a["in_t3"].bool ?? false)
        }
        reason = v["reason"].text
        url = v["url"].text
        missing = v["missing"].bool ?? false
    }

    /// How its strand lies.
    var strand: Strand {
        switch state {
        case .working: .waves
        case .waiting: .knots
        case .done: .straight
        case .failed: .sags
        }
    }

    /// The mono line under its title: `T3 · loom · Opus 5.5 · high`.
    var sub: String {
        (["T3"] + [project, model, effort].filter { !$0.isEmpty }).joined(separator: " · ")
    }

    /// What sits at the title's end, in its colour: how long it has run or ran, "Asks
    /// you", or "Failed".
    func side(_ now: Int) -> String {
        switch state {
        case .waiting: return "Asks you"
        case .failed: return "Failed"
        case .working where started > 0: return took(now - started)
        case .done where started > 0 && ended >= started: return took(ended - started)
        default: return ""
        }
    }

    /// The line in a quiet box: the step it's on while it works, or why it failed.
    var quiet: ThreadQuiet? {
        switch state {
        case .working where !step.isEmpty: .step(step)
        case .failed: .reason(reason.isEmpty ? "Failed" : reason)
        default: nil
        }
    }

    /// What it asks, beside a warn line, while it waits.
    var asks: String? {
        guard let t = ask?.text, !t.isEmpty else { return nil }
        return t
    }

    /// Its pull requests as chips, once it's done: "PR #295" and where it opens.
    var chips: [(label: String, url: String)] {
        state == .done ? prs.map { ("PR #\($0.number)", $0.url) } : []
    }

    /// Its buttons, the main one first: a question's choices, an approval's Approve and
    /// Decline, Open where the person approves in T3 itself, Try again once it failed.
    var buttons: [ThreadButton] {
        switch (state, ask) {
        case (.waiting, .some(let a)) where a.kind == "question":
            return a.choices.enumerated().map { ThreadButton(label: $1, act: .answer($1), main: $0 == 0) }
        case (.waiting, .some(let a)) where a.inT3 || a.request.isEmpty:
            return [ThreadButton(label: "Open", act: .open, main: true)]
        case (.waiting, .some):
            return [ThreadButton(label: "Approve", act: .decide("accept"), main: true), ThreadButton(label: "Decline", act: .decide("decline"), main: false)]
        case (.failed, _):
            return [ThreadButton(label: "Try again", act: .retry, main: false)]
        default:
            return []
        }
    }

    /// An approval the person answers in T3: "Approve in T3" beside Open.
    var inT3: Bool {
        guard let a = ask else { return false }
        return a.kind == "approval" && (a.inT3 || a.request.isEmpty)
    }

    /// Its edge: tinted lightly in sky while it works, warn while it waits.
    var edge: (strand: Strand, opacity: Double)? {
        switch state {
        case .working: (.waves, 0.35)
        case .waiting: (.knots, 0.6)
        default: nil
        }
    }

    /// Whether its strand moves, and its time counts up.
    var moving: Bool { state == .working || state == .waiting }
}

/// How long a run took, as a card says it: "45 s", "18 m", "1 h 5 m".
func took(_ ms: Int) -> String {
    let secs = max(ms / 1000, 0)
    if secs < 60 { return "\(secs) s" }
    let mins = secs / 60
    if mins < 60 { return "\(mins) m" }
    let (h, m) = (mins / 60, mins % 60)
    return m == 0 ? "\(h) h" : "\(h) h \(m) m"
}

/// A message's threads, in the order its words link them, then the rest by when they
/// started.
func threadsOf(_ m: JSON) -> [ThreadCard] {
    guard case .object(let o) = m["threads"] else { return [] }
    let words = m["blocks"].array.filter { $0["kind"].text == "text" }.map { $0["text"].text }.joined(separator: "\n")
    func place(_ c: ThreadCard) -> Int {
        guard let at = words.range(of: "/\(c.thread)"), words[..<at.lowerBound].range(of: threadLinkStart) != nil else { return .max }
        return words.distance(from: words.startIndex, to: at.lowerBound)
    }
    return o.values.map(ThreadCard.init).filter { !$0.missing }.sorted { (place($0), $0.started) < (place($1), $1.started) }
}

/// The id of the thread a link in a message's words points at, `t3-thread://v2/ENV/ID`.
func linkThread(_ url: String) -> String? {
    guard url.hasPrefix(threadLinkStart) else { return nil }
    let rest = url.dropFirst(threadLinkStart.count)
    guard let slash = rest.firstIndex(of: "/") else { return nil }
    let id = rest[rest.index(after: slash)...]
    return id.isEmpty ? nil : String(id)
}

/// A model, as the start sheet offers it.
struct ModelChoice: Equatable, Sendable {
    var id: String
    var name: String
    var efforts: [String]
    /// The effort it starts on.
    var effort: String
}

/// A provider T3 runs, with its models.
struct T3Provider: Equatable, Sendable {
    var id: String
    var name: String
    var models: [ModelChoice]
}

/// An effort as the sheet's segments say it.
func effortLabel(_ e: String) -> String {
    switch e {
    case "low": "Low"
    case "medium": "Medium"
    case "high": "High"
    case "xhigh": "Extra high"
    case "max": "Max"
    default: e
    }
}

/// The start sheet (PLAN.md, "T3 threads"): Server when the person has several, Project,
/// Model, Effort and Task, and Start.
struct StartForm: Equatable, Sendable {
    var servers: [String] = []
    var server = ""
    /// Each project's id and title.
    var projects: [(id: String, title: String)] = []
    var project = ""
    var providers: [T3Provider] = []
    var provider = ""
    var model = ""
    var effort = ""
    /// Waiting for t3.form's answer.
    var loading = false
    /// Waiting for t3.start's answer.
    var starting = false
    /// Why it couldn't load or start, or empty.
    var error = ""

    static func == (a: StartForm, b: StartForm) -> Bool {
        a.servers == b.servers && a.server == b.server && a.projects.map(\.id) == b.projects.map(\.id) && a.project == b.project
            && a.providers == b.providers && a.provider == b.provider && a.model == b.model && a.effort == b.effort
            && a.loading == b.loading && a.starting == b.starting && a.error == b.error
    }

    /// What t3.form answered: the servers, the projects of the one picked and the models.
    /// What the person picked stays where it still can.
    mutating func take(_ v: JSON) {
        servers = v["servers"].array.compactMap(\.string)
        server = v["server"].text
        projects = v["projects"].array.map { (id: $0["id"].text, title: $0["title"].text) }.filter { !$0.id.isEmpty }
        providers = v["providers"].array.map { p in
            T3Provider(id: p["id"].text, name: p["name"].text, models: p["models"].array.map {
                ModelChoice(id: $0["id"].text, name: $0["name"].text, efforts: $0["efforts"].array.compactMap(\.string), effort: $0["effort"].text)
            })
        }
        if !projects.contains(where: { $0.id == project }) { project = projects.first?.id ?? "" }
        if chosen == nil, let p = providers.first(where: { !$0.models.isEmpty }), let m = p.models.first {
            pick(provider: p.id, model: m.id)
        }
        loading = false
    }

    /// The model picked, and its provider.
    var chosen: (provider: T3Provider, model: ModelChoice)? {
        guard let p = providers.first(where: { $0.id == provider }), let m = p.models.first(where: { $0.id == model }) else { return nil }
        return (p, m)
    }

    /// Picks a model: its effort stays when the model has it, or else it's the model's own.
    mutating func pick(provider: String, model: String) {
        self.provider = provider
        self.model = model
        let efforts = chosen?.model.efforts ?? []
        let own = chosen?.model.effort ?? ""
        if !efforts.contains(effort) {
            effort = efforts.contains(own) ? own : efforts.first ?? ""
        }
    }

    /// The efforts the model picked has, each its id and its label: none for one with none,
    /// and then Effort isn't shown.
    var efforts: [(id: String, label: String)] {
        (chosen?.model.efforts ?? []).map { ($0, effortLabel($0)) }
    }

    /// Every model, as the Model menu lists them: its provider, its id and how it's shown,
    /// `Opus 5.5` with one provider, `GPT-6 Luna · Codex` with several.
    var modelList: [(provider: String, model: String, label: String)] {
        let several = providers.count > 1
        return providers.flatMap { p in p.models.map { (p.id, $0.id, several ? "\($0.name) · \(p.name)" : $0.name) } }
    }

    /// How the picked model is shown on its field.
    var modelLabel: String {
        modelList.first { $0.provider == provider && $0.model == model }?.label ?? ""
    }

    /// How the picked project is shown on its field.
    var projectLabel: String {
        projects.first { $0.id == project }?.title ?? ""
    }

    /// Server shows only when the person has several T3 servers.
    var showsServer: Bool { servers.count > 1 }

    /// Whether Start can go: a project, a model and a task, and nothing on its way.
    func canStart(_ task: String) -> Bool {
        !loading && !starting && !project.isEmpty && chosen != nil && !task.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

/// What the T3 cards keep in the model.
struct T3State {
    /// The messages last said to be shown (`t3.view`), once said since the app caught up.
    fileprivate var viewSent: [String]?
    /// Answers on their way, by thread: which button was pressed.
    var busy: [String: String] = [:]
    /// The start form, while its sheet is open.
    var form: StartForm?
    /// The person's T3 servers, as the + sheet last asked: with none, it has no "Start a
    /// T3 thread".
    var servers: [String] = []
    /// t3.answer and t3.retry asked, by the command's id: the thread.
    fileprivate var asked: [String: String] = [:]
}

extension Model {
    /// The frames the T3 cards need (PLAN.md, "T3 threads"): which messages of the shown
    /// chat have threads, so the instance follows those still going, said again when they
    /// change and after the app catches up.
    func t3Fetches() -> [JSON] {
        guard synced else {
            t3.viewSent = nil
            return []
        }
        // The newest 50 messages with threads, newest first.
        let shown = Array(messages.reversed().filter { !$0.threads.isEmpty }.prefix(50).map(\.id))
        if t3.viewSent == shown || (t3.viewSent == nil && shown.isEmpty) { return [] }
        t3.viewSent = shown
        return [["t": "cmd", "id": JSON(id("app.t3_view")), "name": "t3.view", "args": ["messages": JSON(shown)]]]
    }

    /// Asks which T3 servers the person has, for the + sheet.
    func askT3Servers() -> JSON {
        ["t": "cmd", "id": JSON(id("app.t3_servers")), "name": "t3.servers", "args": [:]]
    }

    /// What a card's button sends: the answer to a question or an approval, or Try again.
    /// Nil for Open, which the app opens itself, and while an answer to the thread is on
    /// its way.
    func t3Act(_ card: ThreadCard, _ act: ThreadAct, label: String) -> JSON? {
        guard t3.busy[card.thread] == nil else { return nil }
        let call: (name: String, args: JSON)
        switch act {
        case .answer(let choice):
            call = ("t3.answer", ["thread": JSON(card.thread), "answer": JSON(choice)])
        case .decide(let d):
            call = ("t3.answer", ["thread": JSON(card.thread), "decision": JSON(d)])
        case .retry:
            call = ("t3.retry", ["thread": JSON(card.thread)])
        case .open:
            return nil
        }
        let id = id("app.t3_act")
        t3.busy[card.thread] = label
        t3.asked[id] = card.thread
        return ["t": "cmd", "id": JSON(id), "name": JSON(call.name), "args": call.args]
    }

    /// Opens the start form, and asks for what it offers: the projects of `server`, or of
    /// the first of the person's T3 servers.
    func openT3Form(server: String? = nil) -> JSON {
        var form = t3.form ?? StartForm()
        form.loading = true
        form.error = ""
        t3.form = form
        let args: JSON = server.map { ["server": JSON($0)] } ?? [:]
        return ["t": "cmd", "id": JSON(id("app.t3_form")), "name": "t3.form", "args": args]
    }

    /// Starts the thread the form describes, with `task`, in the chat shown.
    func t3Start(_ task: String) -> JSON? {
        guard var form = t3.form, form.canStart(task) else { return nil }
        form.starting = true
        form.error = ""
        t3.form = form
        let args: JSON = ["server": JSON(form.server), "project": JSON(form.project), "provider": JSON(form.provider), "model": JSON(form.model),
                          "effort": JSON(form.effort), "task": JSON(task.trimmingCharacters(in: .whitespacesAndNewlines)), "conversation": JSON(conversation ?? "")]
        return ["t": "cmd", "id": JSON(id("app.t3_start")), "name": "t3.start", "args": args]
    }

    /// The answers to what the T3 cards and the form asked. Gives whether `f` was one.
    func t3Result(_ f: JSON) -> Bool {
        let id = f["id"].text
        let refused = f["error"].isNull ? nil : (f["error"]["message"].string ?? f["error"]["code"].text)
        if let thread = t3.asked.removeValue(forKey: id) {
            t3.busy[thread] = nil
            if let refused { notice = refused }
            return true
        }
        if id.hasPrefix("app.t3_form") {
            if var form = t3.form {
                if refused == nil, !f["value"].isNull {
                    form.take(f["value"])
                } else {
                    form.loading = false
                    form.error = refused ?? "T3 didn't answer."
                }
                t3.form = form
            }
            return true
        }
        if id.hasPrefix("app.t3_start") {
            if let refused {
                t3.form?.starting = false
                t3.form?.error = refused
            } else {
                // Its card lands in the chat: the form's done.
                t3.form = nil
            }
            return true
        }
        if id.hasPrefix("app.t3_servers") {
            if refused == nil { t3.servers = f["value"]["servers"].array.compactMap(\.string) }
            return true
        }
        return id.hasPrefix("app.t3_view")
    }
}

/// Whether a message is the line from loom a thread the person started lands in, once its
/// card has come: the card shows in the line's place.
func isStartLine(_ m: Message) -> Bool {
    m.role == "loom" && !m.threads.isEmpty
}
