import Foundation

// What the sample core says of Acta, T3 Code and Spools (protocol.md, "Commands"), for
// the screens that show them without an instance: Scribe's chat has card pills, a card
// made from a message and T3 threads in each state; the person has two Acta boards, a T3
// server and a few servers from Spools.

/// What the sample changes as the person uses it.
struct SampleKept {
    /// Each card's status and its subtasks, as the person moved and ticked them.
    var status: [String: String] = [:]
    var ticked: [String: Bool] = [:]
    /// Builder's chat follows the loom board and a T3 thread, whose lines it has; Scribe's
    /// a card and a board it can't see any more; Everyone the loom board; and a routine
    /// runs when a loom card moves to Done.
    var watches: [JSON] = [
        ["id": "w_1", "integration": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "c_1"],
        ["id": "w_2", "integration": "t3", "server": "t3", "target": ["thread": "th_9", "title": "Fix the build"], "conversation": "c_1", "wake": ["agent": "a_1"]],
        ["id": "w_3", "integration": "acta", "target": ["board": "aaen-studios/loom", "card": "LOO-342"], "conversation": "c_2"],
        ["id": "w_4", "integration": "acta", "target": ["board": "aaen-studios/clx"], "conversation": "c_2",
         "error": "Acta says this key can't see aaen-studios/clx any more.\nIt was looked at 3 times."],
        ["id": "w_5", "integration": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "c_6"],
        ["id": "w_6", "integration": "acta", "target": ["board": "aaen-studios/loom"], "conversation": "", "wake": ["routine": "rt_3"], "kind": "moved", "to": "done"],
    ]
    var nextCard = 372
    var nextWatch = 7
    var servers: [JSON] = Sample.servers
}

extension SampleCore {
    /// Answers a command for Acta, T3 or Spools. Gives whether it was one.
    func integrations(_ f: JSON) -> Bool {
        let a = f["args"]
        switch f["name"].text {
        case "acta.boards":
            answer(f, ["boards": [["prefix": "LOO", "board": "aaen-studios/loom", "name": "loom"], ["prefix": "CLX", "board": "aaen-studios/clx", "name": "clx"]]])
        case "acta.board":
            answer(f, ["board": ["board": a["board"], "name": "loom", "prefix": "LOO", "labels": ["Feature", "Bug", "ios", "desktop"], "people": ["Elliot", "Ola"]]])
        case "acta.tasks":
            let done = a["status"].array.contains("done")
            answer(f, ["tasks": .array(Sample.tasks.filter { (status(of: $0) == "done") == done }.map { withStatus($0) })])
        case "card.preview":
            let ref = a["ref"].text
            let id = cardLinkParts(ref)?.id ?? ref
            later(0.3) { [self] in answer(f, ["card": card(id)]) }
        case "card.get":
            later(0.2) { [self] in answer(f, ["card": detail(a["id"].text)]) }
        case "card.update":
            let id = a["id"].text
            if let s = a["status"].string { kept.status[id] = s }
            if let sub = a["subtask"].string { kept.ticked[sub] = a["done"].bool ?? false }
            later(0.2) { [self] in answer(f, ["card": detail(id)]) }
        case "card.create":
            let id = "LOO-\(kept.nextCard)"
            kept.nextCard += 1
            let made: JSON = ["id": JSON(id), "board": a["board"], "board_name": "loom", "title": a["title"], "status": "todo", "url": JSON("https://acta.aaenz.no/aaen-studios/loom#\(id)"), "at": JSON(Sample.now())]
            later(0.4) { [self] in
                answer(f, ["card": made])
                madeCard(made, message: a["message"].text)
            }
        case "cards.refresh", "t3.view", "t3.retry":
            answer(f, [:])
        case "watch.list":
            answer(f, ["watches": .array(kept.watches)])
        case "watch.put":
            let w: JSON = ["id": JSON("w_\(kept.nextWatch)"), "integration": "acta", "target": a["target"], "conversation": a["conversation"]]
            kept.nextWatch += 1
            kept.watches.append(w)
            answer(f, ["watch": w])
            event("watch.updated", ["watch": w])
        case "watch.remove":
            kept.watches.removeAll { $0["id"] == a["id"] }
            answer(f, [:])
            event("watch.removed", ["watch": a["id"]])
        case "t3.servers":
            answer(f, ["servers": ["t3"]])
        case "t3.form":
            later(0.3) { [self] in
                answer(f, ["servers": ["t3"], "server": "t3", "projects": [["id": "p_1", "title": "loom"], ["id": "p_2", "title": "calx"]],
                    "providers": [["id": "claude", "name": "Claude", "models": [["id": "opus-5.5", "name": "Opus 5.5", "efforts": ["low", "medium", "high", "max"], "effort": "high"],
                                                                                 ["id": "sonnet-5.5", "name": "Sonnet 5.5", "efforts": ["low", "medium", "high"], "effort": "medium"]]]]])
            }
        case "t3.start":
            later(0.5) { [self] in
                answer(f, ["thread": "th_9"])
                let line: JSON = ["id": "msg_29", "conversation": a["conversation"], "role": "loom", "at": JSON(Sample.now()),
                                  "blocks": [["kind": "text", "text": "Started a T3 thread"]],
                                  "threads": ["th_9": thread("th_9", a["task"].text, state: "working", step: "Reading the task")]]
                event("message.created", ["conversation": a["conversation"], "message": line])
            }
        case "t3.answer":
            later(0.6) { [self] in answer(f, [:]) }
        case "mcp.list":
            answer(f, ["servers": .array(kept.servers)])
        case "mcp.test":
            later(0.8) { [self] in answer(f, ["tools": ["list_tasks", "get_task"]]) }
        case "mcp.agents":
            let name = a["name"].text
            setServer(name) { $0["agents"] = a["agents"] }
            answer(f, ["server": ["name": a["name"]], "agents": a["agents"]])
            tell(["t": "connect", "connect": ["entry": JSON(serverEntry(name)), "server": a["name"], "step": "done", "said": "", "tools": []]])
        case "mcp.sign_out", "mcp.delete":
            let name = a["name"].text
            if f["name"].text == "mcp.delete" { kept.servers.removeAll { $0["name"].text == name } }
            answer(f, [:])
        case "catalog.list":
            answer(f, ["entries": .array(Sample.entries.map { withServers($0) }), "categories": ["Work", "Code", "Mail & calendar", "Files", "Home", "Fun"]])
        case "catalog.search":
            let q = a["query"].text.lowercased()
            later(0.5) { [self] in
                answer(f, ["entries": .array(Sample.registry.filter { $0["name"].text.lowercased().contains(q) }), "curated": []])
            }
        case "catalog.connect":
            connect(f)
        default:
            return false
        }
        return true
    }

    private func status(of task: JSON) -> String {
        kept.status[task["id"].text] ?? task["status"].text
    }

    private func withStatus(_ task: JSON) -> JSON {
        guard case .object(var o) = task else { return task }
        o["status"] = JSON(status(of: task))
        return .object(o)
    }

    /// A card as a pill knows it.
    private func card(_ id: String) -> JSON {
        guard let t = Sample.tasks.first(where: { $0["id"].text == id }) else {
            return ["id": JSON(id), "missing": true, "at": JSON(Sample.now())]
        }
        let subs = Sample.subtasks[id] ?? []
        return ["id": JSON(id), "board": "aaen-studios/loom", "board_name": "loom", "title": t["title"], "status": JSON(status(of: t)),
                "url": JSON("https://acta.aaenz.no/aaen-studios/loom#\(id)"), "assignee": t["assignee"], "size": t["size"], "labels": t["labels"],
                "subtasks": JSON(subs.count), "subtasks_done": JSON(subs.filter { kept.ticked[$0.id] ?? $0.done }.count), "at": JSON(Sample.now())]
    }

    /// A card as its preview shows it.
    private func detail(_ id: String) -> JSON {
        guard case .object(var o) = card(id), let t = Sample.tasks.first(where: { $0["id"].text == id }) else { return card(id) }
        o["description"] = t["description"]
        o["priority"] = t["priority"]
        o["checklist"] = .array((Sample.subtasks[id] ?? []).map { ["id": JSON($0.id), "title": JSON($0.title), "done": JSON(kept.ticked[$0.id] ?? $0.done)] })
        o["comments"] = id == "LOO-342" ? Sample.comments : []
        return .object(o)
    }

    /// A card made from a message: the message keeps it, and says so.
    private func madeCard(_ made: JSON, message: String) {
        for (c, list) in messages {
            guard let i = list.firstIndex(where: { $0["id"].text == message }), case .object(var m) = list[i] else { continue }
            var cards: [String: JSON] = [:]
            if case .object(let had) = m["cards"] { cards = had }
            cards[made["id"].text] = made
            m["cards"] = .object(cards)
            let had: [JSON] = (m["made_cards"] ?? JSON.null).array
            let pill: JSON = ["id": made["id"], "board": made["board"]]
            m["made_cards"] = JSON.array(had + [pill])
            messages[c]![i] = .object(m)
            event("message.updated", ["conversation": JSON(c), "message": .object(m)])
        }
    }

    private func thread(_ id: String, _ title: String, state: String, step: String = "") -> JSON {
        ["thread": JSON(id), "server": "t3", "title": JSON(title), "project": "loom", "model": "Opus 5.5", "effort": "high", "state": JSON(state),
         "step": JSON(step), "started": JSON(Sample.now()), "url": JSON("https://t3.example.com/threads/\(id)")]
    }

    private func setServer(_ name: String, _ change: (inout [String: JSON]) -> Void) {
        guard let i = kept.servers.firstIndex(where: { $0["name"].text == name }), case .object(var o) = kept.servers[i] else { return }
        change(&o)
        kept.servers[i] = .object(o)
    }

    private func serverEntry(_ name: String) -> String {
        kept.servers.first { $0["name"].text == name }?["catalog"].text ?? ""
    }

    /// An entry with the person's servers made from it.
    private func withServers(_ e: JSON) -> JSON {
        guard case .object(var o) = e else { return e }
        let mine = kept.servers.filter { $0["catalog"] == e["id"] }.map { $0["name"] }
        o["servers"] = .array(mine)
        o["connected"] = JSON(!mine.isEmpty)
        return .object(o)
    }

    /// Connecting an entry: it's added, and one that signs in with an account waits on the
    /// sign-in, which sample servers can't do; the rest go on to which agents get it.
    private func connect(_ f: JSON) {
        let a = f["args"]
        let id = a["id"].string ?? "registry:\(a["registry"].text)"
        let e = (Sample.entries + Sample.registry).first { $0["id"].text == id }
        let server = a["name"].string ?? e?["server"].text ?? id
        tell(["t": "result", "id": f["id"], "ok": true, "value": ["connect": ["entry": JSON(id), "server": JSON(server), "step": "adding", "said": "", "tools": []]]])
        if kept.servers.first(where: { $0["name"].text == server }) == nil {
            kept.servers.append(["name": JSON(server), "url": e?["url"] ?? "", "catalog": JSON(id), "status": "untested", "agents": [], "badge": e?["badge"] ?? .null])
        }
        let step = e?["sign_in"].text == "oauth" ? "sign_in" : "agents"
        later(0.8) { [self] in
            if step == "agents" { setServer(server) { $0["status"] = "connected" } }
            tell(["t": "connect", "connect": ["entry": JSON(id), "server": JSON(server), "step": JSON(step), "said": "", "tools": ["list_tasks", "get_task"]]])
        }
    }
}

extension Sample {
    /// Scribe's chat: a reply naming cards, a message a card was made from, and T3
    /// threads, started from loom and named in replies, in each state.
    static let integrationMessages: [JSON] = [
        ["id": "msg_12", "conversation": "c_2", "role": "person", "from": "d_4", "at": 1795186800000,
         "blocks": [["kind": "text", "text": "What's open on the loom board this week?"]]],
        ["id": "msg_13", "conversation": "c_2", "role": "agent", "agent": "a_2", "run": "run_12", "at": 1795186830000,
         "blocks": [["kind": "text", "text": "Three cards: LOO-342 waits for review, LOO-206 is in progress, and LOO-210 is still to do."]],
         "cards": ["LOO-342": pill("LOO-342"), "LOO-206": pill("LOO-206"), "LOO-210": pill("LOO-210")]],
        ["id": "msg_14", "conversation": "c_2", "role": "person", "from": "d_4", "at": 1795186900000,
         "blocks": [["kind": "text", "text": "The upload test is flaky on CI again."]],
         "cards": ["LOO-371": pill("LOO-371")], "made_cards": [["id": "LOO-371", "board": "aaen-studios/loom"]]],
        ["id": "msg_15", "conversation": "c_2", "role": "agent", "agent": "a_2", "run": "run_14", "at": 1795186960000,
         "blocks": [["kind": "text", "text": "The build fix is done, and one thread needs you to approve a command in T3."]],
         "threads": ["th_3": ["thread": "th_3", "server": "t3", "title": "Fix the release build", "project": "loom", "model": "Opus 5.5", "effort": "high", "state": "done",
                              "started": 1795183000000, "ended": 1795184080000, "prs": [["number": 331, "url": "https://github.com/aaen-studios/loom/pull/331"], ["number": 332, "url": "https://github.com/aaen-studios/loom/pull/332"]],
                              "url": "https://t3.example.com/threads/th_3"],
                     "th_5": ["thread": "th_5", "server": "t3", "title": "Update the Docker image", "project": "loom", "model": "Opus 5.5", "effort": "medium", "state": "waiting",
                              "started": 1795186000000, "ask": ["kind": "approval", "request": "", "text": "Run docker buildx push?", "in_t3": true],
                              "url": "https://t3.example.com/threads/th_5"]]],
        // The line a thread the person started lands in.
        ["id": "msg_16", "conversation": "c_2", "role": "loom", "at": 1795187000000,
         "blocks": [["kind": "text", "text": "Started a T3 thread"]],
         "threads": ["th_1": ["thread": "th_1", "server": "t3", "title": "Fix the flaky upload test", "project": "loom", "model": "Opus 5.5", "effort": "high", "state": "working",
                              "step": "Running swift test --filter UploadTests", "started": JSON(now() - 1_080_000), "url": "https://t3.example.com/threads/th_1"]]],
        ["id": "msg_17", "conversation": "c_2", "role": "agent", "agent": "a_2", "run": "run_16", "at": 1795187100000,
         "blocks": [["kind": "text", "text": "Two more from this morning: one asks you, and the docs one failed."]],
         "threads": ["th_2": ["thread": "th_2", "server": "t3", "title": "Tidy the upload retries", "project": "loom", "model": "Sonnet 5.5", "effort": "medium", "state": "waiting",
                              "started": 1795185000000, "ask": ["kind": "question", "request": "q_1", "text": "Keep the old retry loop, or drop it now that uploads resume?", "choices": ["Drop it", "Keep it"]],
                              "url": "https://t3.example.com/threads/th_2"],
                     "th_4": ["thread": "th_4", "server": "t3", "title": "Write the 0.3 docs", "project": "loom", "model": "Opus 5.5", "effort": "low", "state": "failed",
                              "started": 1795182000000, "ended": 1795182400000, "reason": "The T3 server stopped before the run finished.",
                              "url": "https://t3.example.com/threads/th_4"]]],
    ]

    private static func pill(_ id: String) -> JSON {
        guard let t = tasks.first(where: { $0["id"].text == id }) else { return ["id": JSON(id), "missing": true] }
        let subs = subtasks[id] ?? []
        return ["id": JSON(id), "board": "aaen-studios/loom", "board_name": "loom", "title": t["title"], "status": t["status"],
                "url": JSON("https://acta.aaenz.no/aaen-studios/loom#\(id)"), "assignee": t["assignee"], "size": t["size"], "labels": t["labels"],
                "subtasks": JSON(subs.count), "subtasks_done": JSON(subs.filter(\.done).count), "at": JSON(now())]
    }

    /// The loom board's cards.
    static let tasks: [JSON] = [
        ["id": "LOO-342", "title": "Show Acta's cards beside the chat", "status": "review", "assignee": "Elliot", "size": "l", "priority": "high", "labels": ["Feature", "ios"],
         "description": "A card's pill opens its preview beside the chat, a message's menu makes a card from it, and a board opens from the + menu."],
        ["id": "LOO-206", "title": "Show agents' usage limits in the sidebar", "status": "in_progress", "assignee": "Ola", "size": "m", "priority": "medium", "labels": ["desktop"],
         "description": "Each agent's ring in the sidebar fills as its budget is used."],
        ["id": "LOO-210", "title": "Keep drafts when the app quits", "status": "todo", "assignee": "Elliot", "size": "s", "priority": "", "labels": ["Bug"], "description": ""],
        ["id": "LOO-371", "title": "The upload test is flaky on CI again", "status": "todo", "assignee": "", "size": "", "priority": "", "labels": ["Bug"], "description": "The upload test is flaky on CI again."],
        ["id": "LOO-212", "title": "Search a chat's messages", "status": "todo", "assignee": "Ola", "size": "m", "priority": "", "labels": ["Feature"], "description": ""],
        ["id": "LOO-215", "title": "Read a PDF an agent names", "status": "in_progress", "assignee": "Elliot", "size": "s", "priority": "", "labels": ["ios"], "description": ""],
        ["id": "LOO-198", "title": "Pin a message from its menu", "status": "done", "assignee": "Elliot", "size": "s", "priority": "", "labels": ["Feature"], "description": ""],
        ["id": "LOO-201", "title": "Quote what a reply answers", "status": "done", "assignee": "Ola", "size": "m", "priority": "", "labels": ["Feature"], "description": ""],
    ]

    struct SampleSubtask {
        var id: String
        var title: String
        var done: Bool
    }

    static let subtasks: [String: [SampleSubtask]] = [
        "LOO-342": [SampleSubtask(id: "s_1", title: "Pills in messages", done: true), SampleSubtask(id: "s_2", title: "The side preview", done: true),
                    SampleSubtask(id: "s_3", title: "Make a card from a message", done: true), SampleSubtask(id: "s_4", title: "The iPhone's sheets", done: false)],
    ]

    static let comments: JSON = [
        ["by": "Ola", "at": "2026-11-24T16:20:00Z", "body": "The pill reads well in a long reply."],
        ["by": "Elliot", "at": "2026-11-25T09:05:00Z", "body": "Make a card goes in the message's menu on the phone."],
    ]

    /// The person's MCP servers, as `mcp.list` gives them.
    static let servers: [JSON] = [
        ["name": "acta", "url": "https://acta.aaenz.no/api/mcp", "catalog": "acta", "sign_in": "signed_in", "signed_in_as": "Elliot", "status": "connected",
         "tools": ["list_tasks", "get_task", "search_tasks", "create_tasks", "update_task", "add_comment"], "agents": ["a_1", "a_2"], "badge": ["name": "acta", "initial": "A", "color": "lavender"]],
        ["name": "vercel", "url": "https://mcp.vercel.com", "headers": [:], "sign_in": "needed", "status": "sign_in", "agents": ["a_1"], "catalog": "vercel",
         "badge": ["name": "vercel", "initial": "V", "color": "sky"]],
        ["name": "github", "url": "https://api.githubcopilot.com/mcp/", "catalog": "github", "status": "failing", "why": "Its token was revoked", "agents": ["a_1"],
         "badge": ["name": "github", "initial": "G", "color": "grey"]],
    ]

    /// Spools, as server/catalog.calx has it: loom's own first, then the rest, those with a
    /// mark in server/catalog saying so (`icon_svg`; the app draws its own copy).
    static let entries: [JSON] = [
        ["id": "acta", "name": "Acta", "category": "Work", "description": "Boards, tasks and comments in Acta.", "maker": "Aaen Studios", "server": "acta", "runs": "remote",
         "url": "https://acta.aaenz.no/api/mcp", "fields": [], "sign_in": "oauth", "secrets": [], "badge": ["name": "acta", "initial": "A", "color": "lavender"], "icon_svg": "<svg/>", "tools_checked": true,
         "tools": [["name": "list_tasks", "kind": "read"], ["name": "get_task", "kind": "read"], ["name": "search_tasks", "kind": "read"], ["name": "create_tasks", "kind": "change"],
                   ["name": "update_task", "kind": "change"], ["name": "add_comment", "kind": "change"]]],
        ["id": "t3", "name": "T3 Code", "category": "Code", "description": "Projects and coding threads in T3 Code.", "maker": "T3 Tools", "server": "t3", "runs": "remote",
         "url": "{address}/mcp", "fields": [["name": "address", "label": "Address", "placeholder": "https://my-computer.tail1234.ts.net"]], "sign_in": "oauth", "secrets": [],
         "badge": ["name": "t3", "initial": "T", "color": "sky"], "tools_checked": true,
         "tools": [["name": "list_threads", "kind": "read"], ["name": "get_thread", "kind": "read"], ["name": "start_thread", "kind": "change"], ["name": "answer_thread", "kind": "change"]]],
        entry("github", "GitHub", "Code", "Repositories, issues and pull requests on GitHub.", "GitHub", "grey", mark: true),
        entry("notion", "Notion", "Work", "Pages, databases and comments in Notion.", "Notion", "grey", mark: true),
        entry("linear", "Linear", "Work", "Issues, projects and cycles in Linear.", "Linear", "periwinkle", mark: true),
        entry("vercel", "Vercel", "Code", "Projects, deployments and their logs on Vercel.", "Vercel", "sky", mark: true),
        entry("cloudflare", "Cloudflare", "Code", "Your Cloudflare account through its API, and Cloudflare's docs.", "Cloudflare", "apricot", mark: true),
        entry("playwright", "Playwright", "Code", "Microsoft's Playwright browser, run on the instance.", "Microsoft", "mint", mark: false),
        entry("home-assistant", "Home Assistant", "Home", "Lights, devices and what's on at home, through Home Assistant.", "Open Home Foundation", "sky", mark: true),
    ]

    private static func entry(_ id: String, _ name: String, _ category: String, _ line: String, _ maker: String, _ color: String, mark: Bool) -> JSON {
        var e: [String: JSON] = ["id": JSON(id), "name": JSON(name), "category": JSON(category), "description": JSON(line), "maker": JSON(maker), "server": JSON(id), "runs": "remote",
                                 "url": JSON("https://mcp.\(id).example.com/mcp"), "fields": [], "sign_in": "oauth", "secrets": [], "tools": [["name": "search", "kind": "read"]], "tools_checked": false,
                                 "badge": ["name": JSON(id), "initial": JSON(String(name.prefix(1))), "color": JSON(color)]]
        if mark { e["icon_svg"] = "<svg/>" }
        return .object(e)
    }

    /// What the MCP Registry finds.
    static let registry: [JSON] = [
        ["id": "registry:io.example/weather", "name": "weather", "community": true, "registry": "io.example/weather", "version": "1.2.0", "category": "", "description": "Forecasts by place",
         "maker": "example.io", "server": "weather", "runs": "remote", "url": "https://weather.example.io/mcp", "fields": [], "sign_in": "unknown", "secrets": [], "tools": [],
         "badge": ["name": "weather", "initial": "W", "color": "sky"]],
        ["id": "registry:io.example/weatherkit", "name": "weatherkit", "community": true, "registry": "io.example/weatherkit", "version": "0.4.1", "category": "", "description": "Apple's WeatherKit",
         "maker": "example.io", "server": "weatherkit", "runs": "instance", "command": "npx", "args": ["weatherkit-mcp"], "fields": [], "sign_in": "secret",
         "secrets": [["name": "weatherkit.KEY", "label": "KEY", "have": false]], "tools": [], "badge": ["name": "weatherkit", "initial": "W", "color": "gold"]],
    ]
}

extension SampleCore {
    /// The answer to the command `f`.
    fileprivate func answer(_ f: JSON, _ value: JSON) {
        tell(["t": "result", "id": f["id"], "ok": true, "value": value])
    }
}
