import Foundation
import UIKit

/// A stand-in for the core that answers with sample frames, for SwiftUI previews and for
/// working on the screens without an instance (`-sample` at launch, or a build without
/// LoomCore.xcframework). It says what the real core says, in the same shapes
/// (core/README.md), and holds nothing the app relies on.
final class SampleCore: CoreLink, @unchecked Sendable {
    private let queue = DispatchQueue(label: "loom.sample-core")
    private let inbox: Inbox
    private var seq = 100
    private var next = 0
    private var signedIn: Bool
    private var linkWaiting = false
    private var agents: [JSON]
    private var conversations: [JSON]
    private var messages: [String: [JSON]]
    private var runs: [JSON] = []
    private var tools: [JSON] = []
    private var approvals: [JSON] = []
    /// The agent list's folders.
    private var groups: [JSON] = Sample.groups
    /// The person's own emoji, the ones deleted, and Recently used.
    private var own: [JSON] = Sample.own
    private var gone: [JSON] = []
    private var recent: [String] = ["🎉", "👍", "😭"]
    private var stopped = false

    init(inbox: Inbox, signedIn: Bool = true) {
        self.inbox = inbox
        self.signedIn = signedIn
        let s = Sample.self
        agents = s.agents
        conversations = s.conversations
        messages = s.messages
        runs = s.runs
        tools = s.tools
        approvals = s.approvals
        queue.async { [self] in
            if self.signedIn {
                connect()
            } else {
                tell(["t": "core.conn", "state": "signed_out", "why": "not logged in"])
            }
        }
    }

    private func tell(_ f: JSON) {
        if !stopped { inbox.push(f) }
    }

    private func later(_ seconds: Double, _ work: @escaping @Sendable () -> Void) {
        queue.asyncAfter(deadline: .now() + seconds, execute: work)
    }

    private func connect() {
        tell(["t": "core.conn", "state": "connecting"])
        tell(["t": "core.conn", "state": "live"])
        tell(["t": "welcome", "version": 1, "user": "u_1", "device": "d_4", "seq": JSON(seq), "reset": false, "has_password": false])
        tell(["t": "synced", "seq": JSON(seq)])
    }

    private func event(_ kind: String, _ fields: [String: JSON]) {
        seq += 1
        var f = fields
        f["t"] = "event"
        f["kind"] = JSON(kind)
        f["seq"] = JSON(seq)
        f["at"] = JSON(Sample.now())
        tell(.object(f))
        apply(kind, .object(f))
    }

    private func apply(_ kind: String, _ e: JSON) {
        switch kind {
        case "message.created":
            let c = e["conversation"].text
            messages[c, default: []].removeAll { $0["id"] == e["message"]["id"] }
            messages[c, default: []].append(e["message"])
        case "run.started": runs.append(e)
        case "run.finished":
            runs.removeAll { $0["run"] == e["run"] }
            approvals.removeAll { $0["run"] == e["run"] }
        case "tool.started": tools.append(e)
        case "tool.finished":
            if let i = tools.firstIndex(where: { $0["call"] == e["call"] }), case .object(var t) = tools[i] {
                t["status"] = e["status"]
                tools[i] = .object(t)
            }
        case "approval.needed": approvals.append(e)
        case "approval.answered": approvals.removeAll { $0["call"] == e["call"] }
        default: break
        }
    }

    func send(_ frame: JSON) {
        queue.async { [self] in take(frame) }
    }

    func stop() {
        queue.sync { stopped = true }
    }

    private func take(_ f: JSON) {
        switch f["t"].text {
        case "link.cancel":
            linkWaiting = false
        case "login", "link":
            // A code waits for a yes on the device that showed it, as an instance's does,
            // which the sample gives after a few seconds.
            if f["t"].text == "link" {
                linkWaiting = true
                later(0.4) { [self] in
                    tell(["t": "core.login", "id": f["id"], "waiting": true, "check": "4821", "expires": JSON(Sample.now() + 300_000)])
                }
            }
            later(f["t"].text == "link" ? 4 : 0.6) { [self] in
                if f["t"].text == "link" && !linkWaiting { return }
                if f["t"].text == "login" && f["password"].text.isEmpty {
                    tell(["t": "core.login", "id": f["id"], "error": ["code": "bad_password", "message": "That username and password don't match."]])
                    return
                }
                signedIn = true
                tell(["t": "core.login", "id": f["id"], "ok": true, "user": "u_1", "device": "d_4", "kept": true])
                connect()
            }
        case "logout":
            signedIn = false
            tell(["t": "core.conn", "state": "signed_out", "why": "logged out"])
        case "core.get":
            tell(data(f))
        case "send":
            reply(to: f)
        case "approve":
            let call = f["call"].text
            guard let a = approvals.first(where: { $0["call"].text == call }) else {
                tell(["t": "error", "code": "already_answered", "message": "Someone already answered that."])
                return
            }
            let allow = f["allow"].bool ?? false
            event("approval.answered", ["call": JSON(call), "allow": JSON(allow), "always": false, "by": "d_4"])
            finish(run: a["run"].text, call: call, allowed: allow)
        case "cancel":
            let run = f["run"].text
            if let r = runs.first(where: { $0["run"].text == run }) {
                event("message.created", ["conversation": r["conversation"], "message": ["id": r["message"], "conversation": r["conversation"], "role": "agent", "agent": r["agent"], "run": JSON(run), "at": JSON(Sample.now()), "blocks": [["kind": "text", "text": "Stopped."]]]])
                event("run.finished", ["run": JSON(run), "status": "cancelled"])
            }
        case "file.upload":
            let path = f["path"].text
            let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? Int) ?? 0
            later(0.4) { [self] in
                let name = (path as NSString).lastPathComponent
                let mime = name.hasSuffix(".jpg") ? "image/jpeg" : "application/octet-stream"
                tell(["t": "core.file", "id": f["id"], "file": ["file": JSON("sample-\(name)"), "name": JSON(name), "mime": JSON(mime), "size": JSON(size)]])
            }
        case "cmd":
            // The person's MCP servers, as the sample's sign-in card needs them, and a
            // sign-in, which sample servers can't do. The files the composer's @ picker
            // finds, and a file a message mentions, kept as the person's.
            switch f["name"].text {
            case "link.create":
                // A code to add a device, and a Mac asking to be added with it soon after.
                let code = "K7QF-M2XD"
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["code": JSON(code), "expires": JSON(Sample.now() + 600_000), "qr": JSON("loom://add?a=https%3A%2F%2Floom.example.com&c=\(code)&i=i_1&k=sample"), "text": JSON("\(code)@loom.example.com")]])
                later(3) { [self] in
                    event("link.asked", ["ask": ["id": "la_1", "name": "Elliot's MacBook", "kind": "macos", "check": "4821", "expires": JSON(Sample.now() + 300_000)]])
                }
            case "link.answer":
                tell(["t": "result", "id": f["id"], "ok": true, "value": [:]])
                event("link.answered", ["ask": f["args"]["ask"], "allow": f["args"]["allow"], "by": "d_4"])
            case "device.list":
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["devices": [
                    ["id": "d_1", "name": "Desk PC", "kind": "windows", "at": 1, "seen": JSON(Sample.now() - 3_600_000), "online": false],
                    ["id": "d_2", "name": "Elliot's MacBook", "kind": "macos", "at": 2, "seen": JSON(Sample.now()), "online": true],
                    ["id": "d_4", "name": "iPhone", "kind": "ios", "at": 3, "seen": JSON(Sample.now()), "online": true],
                ]]])
            case "device.revoke", "password.set":
                tell(["t": "result", "id": f["id"], "ok": true, "value": [:]])
            case "mcp.list":
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["servers": [["name": "vercel", "url": "https://mcp.vercel.com", "headers": [:], "sign_in": "needed"]]]])
            case "mcp.sign_in", "mcp.signed_in":
                later(0.4) { [self] in
                    tell(["t": "result", "id": f["id"], "ok": false, "error": ["code": "sign_in_failed", "message": "Sample servers can't be signed in to."]])
                }
            case "agent.group", "agent.hide", "group.create", "group.update", "group.delete":
                folders(f)
            case "agent.files":
                let query = f["args"]["query"].text.lowercased()
                let found = Sample.folder.filter { query.isEmpty || $0["path"].text.lowercased().contains(query) }
                later(0.1) { [self] in
                    tell(["t": "result", "id": f["id"], "ok": true, "value": ["files": .array(found)]])
                }
            // What the composer's + menu lists.
            case "skill.list":
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["skills": .array(Sample.skills)]])
            case "routine.list":
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["routines": .array(Sample.routines)]])
            case "files.recent":
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["files": .array(Sample.recent)]])
            case "link.preview":
                // A link pasted in the composer, looked up as the instance does, with its
                // preview picture.
                let url = f["args"]["url"].text
                later(0.3) { [self] in
                    tell(["t": "result", "id": f["id"], "ok": true, "value": ["link": ["title": "Watering schedule for balcony tomatoes", "site": JSON(siteOf(url)), "image": "sample-dusk"]]])
                }
            case "routine.run":
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["run": ["id": "rr_1"]]])
            case "emoji.add", "emoji.rename", "emoji.delete", "emoji.restore":
                ownEmoji(f)
            case "agent.file":
                let path = f["args"]["path"].text
                let name = (path as NSString).lastPathComponent
                let picture = isPicture(name)
                tell(["t": "result", "id": f["id"], "ok": true, "value": ["file": ["file": JSON(picture ? "sample-leaf" : "sample-\(name)"), "name": JSON(name), "mime": JSON(picture ? "image/png" : "text/markdown"), "size": 2_400]]])
            default:
                break
            }
        case "emoji.typed", "emoji.find", "emoji.picker":
            tell(emoji(f))
        case "emoji.used":
            recent.removeAll { $0 == f["emoji"].text }
            recent.insert(f["emoji"].text, at: 0)
        case "emoji.file":
            if let path = Sample.picture("sample-leaf") {
                tell(["t": "core.file", "id": f["id"], "path": JSON(path)])
            }
        case "file.get":
            if let path = Sample.picture(f["file"].text) {
                tell(["t": "core.file", "id": f["id"], "path": JSON(path)])
            } else {
                tell(["t": "core.file", "id": f["id"], "error": ["code": "not_found", "message": "Sample files aren't kept."]])
            }
        default:
            break
        }
    }

    /// What the core says to the composer's emoji (core/README.md), from the sample's few:
    /// the suggestion row or a replacement for what's typed, the picker's search, and its
    /// groups.
    private func emoji(_ f: JSON) -> JSON {
        let tone = f["tone"].int ?? 0
        func item(_ e: Sample.EmojiEntry) -> JSON {
            let shown = e.tones && tone > 0 ? e.emoji + String(UnicodeScalar(0x1F3FA + tone)!) : e.emoji
            return ["emoji": JSON(shown), "names": JSON(e.names), "tones": JSON(e.tones)]
        }
        func custom(_ c: JSON) -> JSON {
            let token = "<\(c["animated"].bool == true ? "a" : ""):\(c["name"].text):\(c["id"].text)>"
            return ["emoji": JSON(token), "custom": c["id"], "names": JSON([c["name"].text]), "animated": c["animated"]]
        }
        switch f["t"].text {
        case "emoji.picker":
            var out: [JSON] = [
                ["id": "recent", "name": "Recently used", "emoji": .array(recent.compactMap { r in Sample.emoji.first { $0.emoji == r }.map(item) })],
                ["id": "custom", "name": "Your emoji", "emoji": .array(own.map(custom))],
            ]
            for (id, name) in Sample.emojiGroups {
                out.append(["id": JSON(id), "name": JSON(name), "emoji": .array(Sample.emoji.filter { $0.group == id }.map(item))])
            }
            return ["t": "core.emoji", "id": f["id"], "groups": .array(out)]
        case "emoji.find":
            let q = f["query"].text.lowercased()
            let items = own.filter { $0["name"].text.contains(q) }.map(custom) + Sample.emoji.filter { $0.names.contains { $0.contains(q) } }.map(item)
            return ["t": "core.emoji", "id": f["id"], "query": JSON(q), "items": .array(items)]
        default:
            let text = f["text"].text
            if f["emoticons"].bool == true && text.hasSuffix(":) ") {
                return ["t": "core.emoji", "id": f["id"], "replace": ["suffix": ":) ", "with": "🙂 "]]
            }
            guard let colon = text.lastIndex(of: ":") else { return ["t": "core.emoji", "id": f["id"]] }
            let typed = String(text[text.index(after: colon)...])
            // `:sob:`, closed with its colon.
            if typed.isEmpty, let open = text[..<colon].lastIndex(of: ":") {
                let name = String(text[text.index(after: open)..<colon])
                if let c = own.first(where: { $0["name"].text == name }) {
                    return ["t": "core.emoji", "id": f["id"], "replace": ["suffix": JSON(":\(name):"), "with": custom(c)["emoji"]]]
                }
                if let e = Sample.emoji.first(where: { $0.names.contains(name) }) {
                    return ["t": "core.emoji", "id": f["id"], "replace": ["suffix": JSON(":\(name):"), "with": item(e)["emoji"]]]
                }
            }
            guard typed.count >= 2, typed.allSatisfy({ $0.isLetter || $0.isNumber || $0 == "_" }) else { return ["t": "core.emoji", "id": f["id"]] }
            let mine = own.filter { $0["name"].text.hasPrefix(typed) }.map { c -> JSON in
                guard case .object(var o) = custom(c) else { return c }
                o["name"] = c["name"]
                return .object(o)
            }
            let builtIn = Sample.emoji.compactMap { e -> JSON? in
                guard let n = e.names.first(where: { $0.hasPrefix(typed) }), case .object(var o) = item(e) else { return nil }
                o["name"] = JSON(n)
                return .object(o)
            }
            return ["t": "core.emoji", "id": f["id"], "suffix": JSON(":\(typed)"), "query": JSON(typed), "items": .array(Array((mine + builtIn).prefix(8)))]
        }
    }

    /// The emoji commands, as the instance answers them.
    private func ownEmoji(_ f: JSON) {
        let args = f["args"]
        func set(_ j: JSON, _ key: String, _ value: JSON) -> JSON {
            guard case .object(var o) = j else { return j }
            o[key] = value
            return .object(o)
        }
        var value: JSON = [:]
        switch f["name"].text {
        case "emoji.add":
            next += 1
            let e: JSON = ["id": JSON("e_s\(next)"), "name": args["name"], "file": args["file"], "animated": false, "at": JSON(Sample.now())]
            own.append(e)
            value = ["emoji": e]
        case "emoji.rename":
            guard let i = own.firstIndex(where: { $0["id"] == args["emoji"] }) else { return }
            own[i] = set(own[i], "name", args["name"])
            value = ["emoji": own[i]]
        case "emoji.delete":
            gone += own.filter { $0["id"] == args["emoji"] }
            own.removeAll { $0["id"] == args["emoji"] }
        default:
            guard let e = gone.first(where: { $0["id"] == args["emoji"] }) else { return }
            gone.removeAll { $0["id"] == args["emoji"] }
            own.append(e)
            value = ["emoji": e]
        }
        tell(["t": "result", "id": f["id"], "ok": true, "value": value])
        event("emoji.changed", ["emoji": .array(own), "by": "d_4"])
    }

    /// The agent list's sections as the core gives them (core/sidebar.calx), for the
    /// sample's agents, which are in the order they're listed.
    private func sidebar() -> [JSON] {
        let waiting = Set(approvals.compactMap { a in runs.first { $0["run"] == a["run"] }?["agent"].string })
        var asking: [String] = [], pinned: [String] = [], loose: [String] = [], hidden: [String] = []
        var inFolder: [String: [String]] = [:]
        let known = Set(groups.map { $0["id"].text })
        for a in agents {
            let id = a["id"].text
            if !a["pinned"].isNull && a["hidden"].isNull { pinned.append(id) }
            else if waiting.contains(id) { asking.append(id) }
            else if !a["hidden"].isNull { hidden.append(id) }
            else if let g = a["group"].string, known.contains(g) { inFolder[g, default: []].append(id) }
            else { loose.append(id) }
        }
        let part = { (kind: String, ids: [String]) -> JSON in ["kind": JSON(kind), "agents": JSON(ids)] }
        var out = [part("waiting", asking), part("pinned", pinned), part("agents", loose)]
        for g in groups {
            out.append(["kind": "group", "id": g["id"], "name": g["name"], "collapsed": JSON(g["collapsed"].bool ?? false), "agents": JSON(inFolder[g["id"].text] ?? [])])
        }
        out.append(part("hidden", hidden))
        return out
    }

    /// The folder commands, as the instance answers them.
    private func folders(_ f: JSON) {
        let args = f["args"]
        func set(_ j: JSON, _ key: String, _ value: JSON) -> JSON {
            guard case .object(var o) = j else { return j }
            o[key] = value
            return .object(o)
        }
        var value: JSON = [:]
        switch f["name"].text {
        case "agent.group", "agent.hide":
            guard let i = agents.firstIndex(where: { $0["id"] == args["agent"] }) else { return }
            var a = set(set(agents[i], "pinned", .null), "hidden", .null)
            if f["name"].text == "agent.group" {
                a = set(a, "group", args["group"].text.isEmpty ? .null : args["group"])
            } else if args["hidden"].bool ?? true {
                a = set(a, "hidden", JSON(Sample.now()))
            }
            agents[i] = a
            event("agent.updated", ["agent": a])
            value = ["agent": a]
        case "group.create":
            next += 1
            let g: JSON = ["id": JSON("g_s\(next)"), "name": args["name"], "collapsed": false]
            groups.append(g)
            event("group.updated", ["group": g])
            value = ["group": g]
        case "group.update":
            guard let i = groups.firstIndex(where: { $0["id"] == args["group"] }) else { return }
            if !args["name"].isNull { groups[i] = set(groups[i], "name", args["name"]) }
            if !args["collapsed"].isNull { groups[i] = set(groups[i], "collapsed", args["collapsed"]) }
            event("group.updated", ["group": groups[i]])
            value = ["group": groups[i]]
        default:
            let id = args["group"]
            groups.removeAll { $0["id"] == id }
            for i in agents.indices where agents[i]["group"] == id {
                agents[i] = set(agents[i], "group", .null)
                event("agent.updated", ["agent": agents[i]])
            }
            event("group.deleted", ["group": id])
        }
        tell(["t": "result", "id": f["id"], "ok": true, "value": value])
    }

    private func data(_ f: JSON) -> JSON {
        let what = f["what"].text
        var items: [JSON] = []
        switch what {
        case "agents": items = agents
        case "conversations": items = conversations
        case "runs": items = runs
        case "tools": items = tools
        case "approvals": items = approvals
        case "devices": items = Sample.devices
        case "sidebar": items = sidebar()
        case "messages": items = messages[f["conversation"].text] ?? []
        case "emoji": items = own
        default: break
        }
        return ["t": "core.data", "id": f["id"], "what": JSON(what), "items": .array(items), "more": false]
    }

    /// The whole trip of a reply: stored, a run, deltas, a tool, and the finished message.
    private func reply(to f: JSON) {
        next += 1
        let cid = f["cid"].string ?? "m_s\(next)"
        let conversation = f["conversation"].string ?? conversations.first { $0["agent"] == f["agent"] }?["id"].text ?? "c_1"
        let agent = conversations.first { $0["id"].text == conversation }?["agent"].text ?? "a_1"
        tell(["t": "core.queued", "cid": JSON(cid), "conversation": JSON(conversation), "text": f["text"], "at": JSON(Sample.now())])
        let mine = "msg_p\(next)", theirs = "msg_r\(next)", run = "run_s\(next)", call = "tc_s\(next)"
        later(0.3) { [self] in
            tell(["t": "ack", "cid": JSON(cid), "message": JSON(mine), "run": JSON(run)])
            var blocks: [JSON] = []
            if !f["text"].text.isEmpty { blocks.append(["kind": "text", "text": f["text"]]) }
            for file in f["files"].array {
                blocks.append(["kind": "file", "file": file, "name": "photo.jpg", "mime": "image/jpeg", "size": 1_200_000])
            }
            var message: [String: JSON] = ["id": JSON(mine), "conversation": JSON(conversation), "role": "person", "from": "d_4", "at": JSON(Sample.now()), "blocks": .array(blocks)]
            // A reply keeps what it replies to, as the instance would: here, always the agent's.
            if !f["reply"].isNull {
                let quote = f["reply"]["quote"].text
                message["reply"] = ["message": f["reply"]["message"], "role": "agent", "agent": JSON(agent), "text": JSON(quote.isEmpty ? "Here they are, from the 12 PRs merged since v0.2." : quote), "part": JSON(!quote.isEmpty)]
            }
            event("message.created", ["conversation": JSON(conversation), "message": .object(message)])
            event("run.started", ["run": JSON(run), "conversation": JSON(conversation), "agent": JSON(agent), "revision": 1, "message": JSON(theirs), "answering": JSON(mine)])
        }
        var pos = 0
        var t = 1.2
        let opening = Sample.streamed
        for piece in opening {
            let at = pos
            later(t) { [self] in
                tell(["t": "delta", "run": JSON(run), "message": JSON(theirs), "block": 0, "kind": "text", "pos": JSON(at), "text": JSON(piece)])
            }
            pos += piece.utf8.count
            t += 0.12
        }
        later(t + 0.3) { [self] in
            event("tool.started", ["run": JSON(run), "call": JSON(call), "block": 1, "tool": "execute", "title": "Check the last run on main", "input": ["command": "gh run list --branch main --limit 1"]])
            event("approval.needed", ["run": JSON(run), "call": JSON(call), "tool": "execute", "title": "Run a command", "detail": "gh run list --branch main --limit 1", "can_always": true])
        }
    }

    private func finish(run: String, call: String, allowed: Bool) {
        guard let r = runs.first(where: { $0["run"].text == run }) else { return }
        event("tool.finished", ["call": JSON(call), "status": JSON(allowed ? "done" : "denied"), "output": [["kind": "text", "text": "completed  success  CI  main  push  2m3s  14:02"]]])
        let text = allowed ? Sample.finished : "All right, I won't run it. Ask again when you want me to check."
        var pos = 0
        var t = 0.4
        for piece in text.split(separator: " ", omittingEmptySubsequences: false).map({ $0 + " " }) {
            let at = pos
            later(t) { [self] in
                tell(["t": "delta", "run": JSON(run), "message": r["message"], "block": 2, "kind": "text", "pos": JSON(at), "text": JSON(String(piece))])
            }
            pos += piece.utf8.count
            t += 0.05
        }
        later(t + 0.3) { [self] in
            let opening = Sample.streamed.joined()
            event("message.created", ["conversation": r["conversation"], "message": ["id": r["message"], "conversation": r["conversation"], "role": "agent", "agent": r["agent"], "run": JSON(run), "at": JSON(Sample.now()), "blocks": [["kind": "text", "text": JSON(opening)], ["kind": "tool", "call": JSON(call)], ["kind": "text", "text": JSON(text)]]]])
            event("run.finished", ["run": JSON(run), "status": "done", "usage": ["input": 5210, "output": 118]])
        }
    }
}

/// The sample data: three agents, their chats, and one waiting on a question.
enum Sample {
    static func now() -> Int { Int(Date.now.timeIntervalSince1970 * 1000) }

    /// A few of Unicode's emoji, by group, with their names, as shared/emoji.calx has
    /// them.
    struct EmojiEntry: Sendable {
        var emoji: String
        var names: [String]
        var group: String
        var tones = false
    }

    static let emojiGroups: [(String, String)] = [
        ("smileys", "Smileys & Emotion"), ("people", "People & Body"), ("nature", "Animals & Nature"),
        ("food", "Food & Drink"), ("travel", "Travel & Places"), ("activities", "Activities"),
        ("objects", "Objects"), ("symbols", "Symbols"), ("flags", "Flags"),
    ]

    static let emoji: [EmojiEntry] = [
        EmojiEntry(emoji: "😀", names: ["grinning", "grinning_face"], group: "smileys"),
        EmojiEntry(emoji: "😄", names: ["smile", "grinning_face_with_smiling_eyes"], group: "smileys"),
        EmojiEntry(emoji: "😂", names: ["joy", "face_with_tears_of_joy"], group: "smileys"),
        EmojiEntry(emoji: "🙂", names: ["slight_smile", "slightly_smiling_face"], group: "smileys"),
        EmojiEntry(emoji: "😉", names: ["wink", "winking_face"], group: "smileys"),
        EmojiEntry(emoji: "😍", names: ["heart_eyes", "smiling_face_with_heart_eyes"], group: "smileys"),
        EmojiEntry(emoji: "🤔", names: ["thinking", "thinking_face"], group: "smileys"),
        EmojiEntry(emoji: "😭", names: ["sob", "loudly_crying_face"], group: "smileys"),
        EmojiEntry(emoji: "😅", names: ["sweat_smile", "grinning_face_with_sweat"], group: "smileys"),
        EmojiEntry(emoji: "💀", names: ["skull"], group: "smileys"),
        EmojiEntry(emoji: "👍", names: ["thumbsup", "+1", "thumbs_up"], group: "people", tones: true),
        EmojiEntry(emoji: "👋", names: ["wave", "waving_hand"], group: "people", tones: true),
        EmojiEntry(emoji: "🙏", names: ["pray", "folded_hands"], group: "people", tones: true),
        EmojiEntry(emoji: "👏", names: ["clap", "clapping_hands"], group: "people", tones: true),
        EmojiEntry(emoji: "💪", names: ["muscle", "flexed_biceps"], group: "people", tones: true),
        EmojiEntry(emoji: "🐶", names: ["dog", "dog_face"], group: "nature"),
        EmojiEntry(emoji: "🌱", names: ["seedling"], group: "nature"),
        EmojiEntry(emoji: "🍅", names: ["tomato"], group: "food"),
        EmojiEntry(emoji: "☕", names: ["coffee", "hot_beverage"], group: "food"),
        EmojiEntry(emoji: "🚀", names: ["rocket"], group: "travel"),
        EmojiEntry(emoji: "🎉", names: ["tada", "party_popper"], group: "activities"),
        EmojiEntry(emoji: "⚽", names: ["soccer", "soccer_ball"], group: "activities"),
        EmojiEntry(emoji: "💡", names: ["bulb", "light_bulb"], group: "objects"),
        EmojiEntry(emoji: "🔥", names: ["fire"], group: "travel"),
        EmojiEntry(emoji: "❤️", names: ["heart", "red_heart"], group: "symbols"),
        EmojiEntry(emoji: "✅", names: ["white_check_mark", "check_mark_button"], group: "symbols"),
        EmojiEntry(emoji: "🇳🇴", names: ["flag_no", "flag_norway"], group: "flags"),
    ]

    /// The person's own: one, its picture a sample's.
    static let own: [JSON] = [
        ["id": "e_1", "name": "knot", "file": "sample-leaf", "animated": false, "at": 1795100000000],
    ]

    static let streamed = ["Let me ", "check ", "the last ", "run on ", "main."]
    static let finished = """
    Yes. The last build on **main** passed at 14:02, in 2 minutes.

    ```sh
    gh run list --branch main --limit 1
    ```

    Nothing failed, so there's nothing to fix.
    """

    static let agents: [JSON] = [
        ["id": "a_1", "name": "Builder", "color": "periwinkle", "shape": "yarn", "acp_agent": "claude", "permission": "ask",
         "instructions": "Builds loom, runs its tests and writes the release notes."],
        ["id": "a_2", "name": "Scribe", "color": "mint", "shape": "heart", "acp_agent": "claude", "permission": "auto",
         "instructions": "Keeps the notes: plans, meetings and the week's log."],
        ["id": "a_3", "name": "Postmaster", "color": "apricot", "acp_agent": "codex", "permission": "ask",
         "instructions": "Reads the mail in every inbox and says what needs me."],
        ["id": "a_4", "name": "Gardener", "color": "pink", "shape": "star", "acp_agent": "gemini", "permission": "allow", "group": "g_1",
         "instructions": "Looks after the balcony garden."],
    ]

    /// The agent list's folders, as `synced` gives them.
    static let groups: [JSON] = [
        ["id": "g_1", "name": "Home", "collapsed": false],
    ]

    /// The files the composer's @ picker finds, as `agent.files` gives them: in the
    /// agent's folder, and the shared one.
    static let folder: [JSON] = [
        ["path": "notes/watering.md", "name": "watering.md", "where": "notes"],
        ["path": "notes/harvest 2026.md", "name": "harvest 2026.md", "where": "notes"],
        ["path": "/Users/sample/Shared/balcony plan.png", "name": "balcony plan.png", "where": "shared"],
    ]

    /// The person's skills, routines and the files sent lately, for the + menu.
    static let skills: [JSON] = [
        ["name": "release-notes", "description": "When a version ships and its notes need writing", "at": 1795187131000],
        ["name": "review", "description": "Before a pull request merges", "at": 1795187131000],
        ["name": "watering", "description": "When the balcony plants need a schedule", "at": 1795187131000],
    ]

    static let routines: [JSON] = [
        ["id": "rt_1", "name": "Morning news", "agent": "a_3", "prompt": "", "schedule": ["daily": "08:00"], "on": true,
         "last": ["status": "done", "at": 1795165200000]],
        ["id": "rt_2", "name": "Water check", "agent": "a_4", "prompt": "", "schedule": ["daily": "18:00"], "on": true,
         "last": ["status": "failed", "at": 1795100000000]],
    ]

    static let recent: [JSON] = [
        ["file": "sample-leaf", "name": "leaf.png", "mime": "image/png", "size": 48_213, "at": 1795180000000, "conversation": "c_4"],
        ["file": "sample-notes", "name": "release notes.md", "mime": "text/markdown", "size": 2_400, "at": 1795170000000, "conversation": "c_1"],
    ]

    static let conversations: [JSON] = [
        // Builder's thread with Gardener (`message_agent`), newer than the person's chat
        // with Gardener and never in its place.
        ["id": "c_5", "agent": "a_4", "title": "From Builder", "asked_by": "a_1", "at": 1795187300000],
        ["id": "c_1", "agent": "a_1", "title": "", "at": 1795187131000],
        ["id": "c_2", "agent": "a_2", "title": "", "at": 1795187131000],
        ["id": "c_3", "agent": "a_3", "title": "", "at": 1795187131000],
        ["id": "c_4", "agent": "a_4", "title": "", "at": 1795187131000],
    ]

    static let devices: [JSON] = [
        ["id": "d_1", "name": "Desk PC", "kind": "windows", "at": 1795187131000],
        ["id": "d_4", "name": "iPhone", "kind": "ios", "at": 1795187131000],
    ]

    static let messages: [String: [JSON]] = [
        "c_1": [
            ["id": "msg_1", "conversation": "c_1", "role": "person", "from": "d_1", "at": 1795180000000,
             "blocks": [["kind": "text", "text": "Can you make the release notes for 0.3 from the merged PRs?"]]],
            ["id": "msg_2", "conversation": "c_1", "role": "agent", "agent": "a_1", "run": "run_1", "at": 1795180042000,
             "blocks": [
                ["kind": "thought", "text": "The person wants notes for 0.3. I'll list the PRs merged since the v0.2 tag."],
                ["kind": "tool", "call": "tc_1"],
                ["kind": "card", "card": ["id": "card_1", "type": "checklist", "title": "Release 0.3", "items": [["text": "List the merged PRs", "done": true], ["text": "Write the notes", "done": true], ["text": "Tag v0.3", "done": false]]]],
                ["kind": "text", "text": "Here they are, from the **12 PRs** merged since `v0.2`:\n\n## loom 0.3\n\n- Routines run on webhooks, signed as GitHub signs.\n- Agents get your MCP servers, with secrets sealed.\n- Screenshots and the clipboard as device tools.\n\nI kept them short. Want the full list too?"],
             ]],
            ["id": "msg_3", "conversation": "c_1", "role": "person", "from": "d_4", "at": 1795187100000,
             "blocks": [["kind": "text", "text": "That's good. Did the build on main pass?"]],
             "reply": ["message": "msg_2", "role": "agent", "agent": "a_1", "text": "I kept them short. Want the full list too?", "part": true]],
            // Emoji, drawn with Twemoji (PLAN.md, "Emoji"): two alone show big.
            ["id": "msg_4", "conversation": "c_1", "role": "agent", "agent": "a_1", "run": "run_2", "at": 1795187110000,
             "blocks": [["kind": "text", "text": "It did 👍 All green on main. <:knot:e_1> In code it stays as written: `:sob:`."]]],
            ["id": "msg_5", "conversation": "c_1", "role": "person", "from": "d_4", "at": 1795187120000,
             "blocks": [["kind": "text", "text": "🎉🎉"]]],
        ],
        "c_2": [
            ["id": "msg_10", "conversation": "c_2", "role": "person", "from": "d_1", "at": 1795100000000,
             "blocks": [["kind": "text", "text": "Write a function that turns bytes into a size people read."]]],
            ["id": "msg_11", "conversation": "c_2", "role": "agent", "agent": "a_2", "run": "run_10", "at": 1795100030000,
             "blocks": [["kind": "text", "text": "In Swift:\n\n```swift\nfunc size(_ bytes: Int) -> String {\n    switch bytes {\n    case ..<1000: return \"\\(bytes) B\"\n    case ..<1_000_000: return \"\\(bytes / 1000) KB\"\n    default: return String(format: \"%.1f MB\", Double(bytes) / 1e6)\n    }\n}\n```\n\nIt counts in thousands, as Finder does."]],
             "sources": [["url": "https://support.apple.com/en-us/102119", "title": "How iOS and macOS report storage capacity", "site": "support.apple.com"]]],
        ],
        "c_3": [
            // A routine's prompt kept from before routines ran quietly: not shown.
            ["id": "msg_20", "conversation": "c_3", "role": "person", "from": "rt_1", "at": 1795170000000,
             "blocks": [["kind": "text", "text": "Read the new mail in every inbox and tell me what needs me."]]],
            // What the routine's agent posted, which shows as any reply does.
            ["id": "msg_21", "conversation": "c_3", "role": "agent", "agent": "a_3", "routine": "rt_1", "at": 1795170090000,
             "blocks": [["kind": "text", "text": "Two things need you:\n\n1. The VPS invoice is due Friday.\n2. Ola asked whether Saturday still works."],
                        ["kind": "card", "card": ["id": "card_1", "type": "facts", "title": "Inbox, today", "items": [["label": "Unread", "value": "14"], ["label": "Need you", "value": "2"], ["label": "Newsletters", "value": "9, all archived"]]]],
                        ["kind": "card", "card": ["id": "sign_in_vercel", "type": "sign_in", "title": "Sign in to vercel", "server": "vercel", "badge": ["name": "vercel", "initial": "V", "color": "sky"]]],
                        ["kind": "card", "card": ["id": "mcp_add_tc_29", "type": "mcp_add", "title": "Add the MCP server tracker", "call": "tc_29", "server": "tracker",
                                                  "badge": ["name": "tracker", "initial": "T", "color": "mint"], "url": "https://mcp.tracker.example/mcp",
                                                  "settings": [["name": "Authorization", "in": "header", "secret": "TRACKER_TOKEN", "have": false], ["name": "X-Team", "in": "header", "value": "design"]],
                                                  "state": "asking"]],
                        // A question goes at the end of its reply, after all of it.
                        ["kind": "card", "card": ["id": "card_2", "type": "choices", "title": "Shall I tell Ola yes?", "options": ["Yes", "Not yet"]]]]],
        ],
        "c_4": [
            ["id": "msg_41", "conversation": "c_4", "role": "person", "from": "d_4", "at": 1795187000000,
             "blocks": [["kind": "file", "file": "sample-tall", "name": "balcony.png", "mime": "image/png", "size": 412_000],
                        ["kind": "text", "text": "Is [balcony.png] ready to pick?"]]],
            ["id": "msg_42", "conversation": "c_4", "role": "agent", "agent": "a_4", "run": "run_41", "at": 1795187060000,
             "blocks": [
                ["kind": "tool", "call": "tc_40"],
                ["kind": "text", "text": "The photo is a little dark, so I'll check the colour near the stem."],
                ["kind": "tool", "call": "tc_41"],
                ["kind": "text", "text": "Not yet. It's still orange near the stem; give it two or three more days."],
             ]],
            ["id": "msg_43", "conversation": "c_4", "role": "person", "from": "d_4", "at": 1795187120000,
             "blocks": [["kind": "file", "file": "sample-dusk", "name": "balcony 2.png", "mime": "image/png", "size": 398_000],
                        ["kind": "file", "file": "sample-leaf", "name": "leaves.png", "mime": "image/png", "size": 351_000],
                        ["kind": "file", "file": "sample-tall", "name": "balcony.png", "mime": "image/png", "size": 412_000],
                        ["kind": "text", "text": "[balcony 2.png] [leaves.png] [balcony.png] And these, from this evening?"]]],
            // Another agent's message (`message_agent`), on the person's side in its colour.
            ["id": "msg_40", "conversation": "c_4", "role": "person", "from": "a_1", "at": 1795187150000,
             "blocks": [["kind": "text", "text": "Water the tomatoes on the balcony when the forecast says no rain."]]],
            // Mentions of an agent and of a file in the agent's folder.
            ["id": "msg_44", "conversation": "c_4", "role": "person", "from": "d_4", "at": 1795187180000,
             "blocks": [["kind": "text", "text": "Once they're watered, note it in [watering.md](loom:file/a_4/notes/watering.md) and tell [@Scribe](loom:agent/a_2)."]]],
            // A reply naming a picture in the agent's folder, shown by its thumbnail.
            ["id": "msg_46", "conversation": "c_4", "role": "agent", "agent": "a_4", "run": "run_46", "at": 1795187210000,
             "mentions": [["path": "photos/seedlings.png", "name": "seedlings.png", "size": 361_000, "mime": "image/png", "thumb": "sample-sprout"]],
             "blocks": [["kind": "text", "text": "Last week's seedlings are in `photos/seedlings.png`."]]],
            // A reply naming an agent and a file, shown as the person's mentions are, and a
            // fan of more than five pictures, whose corner says how many there are. It's the
            // chat's last, so its form still takes answers.
            ["id": "msg_45", "conversation": "c_4", "role": "agent", "agent": "a_4", "run": "run_45", "at": 1795187240000,
             "mentions": [["path": "notes/watering.md", "name": "watering.md"]],
             "blocks": [["kind": "text", "text": "Noted in `notes/watering.md`, and @Scribe knows. Here's the balcony this week, a picture a day."],
                        ["kind": "file", "file": "sample-noon", "name": "monday.png", "mime": "image/png", "size": 380_000],
                        ["kind": "file", "file": "sample-rain", "name": "tuesday.png", "mime": "image/png", "size": 372_000],
                        ["kind": "file", "file": "sample-dusk", "name": "wednesday.png", "mime": "image/png", "size": 398_000],
                        ["kind": "file", "file": "sample-night", "name": "thursday.png", "mime": "image/png", "size": 344_000],
                        ["kind": "file", "file": "sample-sprout", "name": "friday.png", "mime": "image/png", "size": 361_000],
                        ["kind": "file", "file": "sample-leaf", "name": "saturday.png", "mime": "image/png", "size": 351_000],
                        ["kind": "file", "file": "sample-tall", "name": "sunday.png", "mime": "image/png", "size": 412_000],
                        // A form, at the end of the reply: several questions at once.
                        ["kind": "card", "card": ["id": "card_1", "type": "form", "title": "Next watering", "fields": [
                            ["label": "When", "kind": "choice", "required": true, "options": ["Morning", "Evening"], "value": "Evening", "placeholder": ""],
                            ["label": "Litres", "kind": "number", "required": true, "value": 2, "placeholder": "2"],
                            ["label": "Remind me", "kind": "toggle", "required": false, "value": true],
                            ["label": "Note", "kind": "text", "required": false, "value": "", "placeholder": "For the plants on the left"]]]]]],
        ],
        "c_5": [
            ["id": "msg_50", "conversation": "c_5", "role": "person", "from": "a_1", "at": 1795187300000,
             "blocks": [["kind": "text", "text": "Which pots need a bigger saucer before the rain?"]]],
            ["id": "msg_51", "conversation": "c_5", "role": "agent", "agent": "a_4", "run": "run_51", "at": 1795187330000,
             "blocks": [["kind": "text", "text": "The two tomato pots by the railing. The basil is fine as it is."]]],
        ],
    ]

    static let runs: [JSON] = [
        ["run": "run_30", "conversation": "c_2", "agent": "a_2", "message": "msg_31", "kind": "run.started"],
        // A routine's quiet run: not drawn in the chat, and not shown as working.
        ["run": "run_21", "conversation": "c_3", "agent": "a_3", "message": "msg_22", "routine": "rt_1", "kind": "run.started"],
    ]

    /// The file of a sample picture, drawn the first time it's asked for.
    static func picture(_ sha: String) -> String? {
        // The sky, the ground and the tomato of each: a tall one by day, a wide one at
        // dusk, leaves, and the rest of a week's.
        let looks: [String: (size: CGSize, sky: UIColor, ground: UIColor, fruit: UIColor)] = [
            "sample-tall": (CGSize(width: 300, height: 800), UIColor(red: 0.55, green: 0.75, blue: 0.95, alpha: 1),
                            UIColor(red: 0.3, green: 0.6, blue: 0.3, alpha: 1), UIColor(red: 0.95, green: 0.45, blue: 0.2, alpha: 1)),
            "sample-dusk": (CGSize(width: 800, height: 600), UIColor(red: 0.85, green: 0.55, blue: 0.45, alpha: 1),
                            UIColor(red: 0.25, green: 0.35, blue: 0.3, alpha: 1), UIColor(red: 0.85, green: 0.25, blue: 0.15, alpha: 1)),
            "sample-leaf": (CGSize(width: 600, height: 600), UIColor(red: 0.45, green: 0.65, blue: 0.4, alpha: 1),
                            UIColor(red: 0.2, green: 0.45, blue: 0.25, alpha: 1), UIColor(red: 0.9, green: 0.6, blue: 0.2, alpha: 1)),
            "sample-noon": (CGSize(width: 800, height: 600), UIColor(red: 0.4, green: 0.7, blue: 1, alpha: 1),
                            UIColor(red: 0.35, green: 0.65, blue: 0.3, alpha: 1), UIColor(red: 0.95, green: 0.35, blue: 0.2, alpha: 1)),
            "sample-rain": (CGSize(width: 600, height: 800), UIColor(red: 0.6, green: 0.65, blue: 0.7, alpha: 1),
                            UIColor(red: 0.3, green: 0.4, blue: 0.35, alpha: 1), UIColor(red: 0.8, green: 0.4, blue: 0.25, alpha: 1)),
            "sample-night": (CGSize(width: 800, height: 600), UIColor(red: 0.15, green: 0.18, blue: 0.35, alpha: 1),
                             UIColor(red: 0.1, green: 0.2, blue: 0.15, alpha: 1), UIColor(red: 0.6, green: 0.2, blue: 0.15, alpha: 1)),
            "sample-sprout": (CGSize(width: 600, height: 600), UIColor(red: 0.95, green: 0.85, blue: 0.6, alpha: 1),
                              UIColor(red: 0.5, green: 0.35, blue: 0.2, alpha: 1), UIColor(red: 0.5, green: 0.75, blue: 0.3, alpha: 1)),
        ]
        guard let look = looks[sha] else { return nil }
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("loom-\(sha).png").path
        if FileManager.default.fileExists(atPath: path) { return path }
        let (w, h) = (look.size.width, look.size.height)
        let image = UIGraphicsImageRenderer(size: look.size, format: {
            let f = UIGraphicsImageRendererFormat()
            f.scale = 1
            return f
        }()).image { c in
            look.sky.setFill()
            c.fill(CGRect(x: 0, y: 0, width: w, height: h))
            look.ground.setFill()
            c.fill(CGRect(x: 0, y: h * 0.65, width: w, height: h * 0.35))
            look.fruit.setFill()
            UIBezierPath(ovalIn: CGRect(x: w / 2 - 60, y: h * 0.45, width: 120, height: 120)).fill()
        }
        guard let data = image.pngData(), (try? data.write(to: URL(fileURLWithPath: path))) != nil else { return nil }
        return path
    }

    static let tools: [JSON] = [
        ["call": "tc_1", "run": "run_1", "tool": "execute", "title": "List the PRs merged since v0.2", "status": "done"],
        ["call": "tc_40", "run": "run_41", "tool": "read", "title": "Look at balcony.png", "status": "done"],
        ["call": "tc_41", "run": "run_41", "tool": "execute", "title": "Measure the colour near the stem", "status": "done"],
        ["call": "tc_30", "run": "run_30", "tool": "edit", "title": "Edit notes/2026-11.md"],
    ]

    /// Questions as `core.get` gives them, each with what its card shows (core/asks.calx).
    static let approvals: [JSON] = [
        ["call": "tc_30", "run": "run_30", "tool": "edit", "title": "Edit notes/2026-11.md", "detail": "+ ## 21 November\n+ Shipped the iPhone app's first build.", "can_always": true,
         "shown": ["what": "", "lines": [], "more": 0, "code": "+ ## 21 November\n+ Shipped the iPhone app's first build.", "raw": "",
                   "title": "Edit notes/2026-11.md", "place": "", "badges": [], "effect": ["kind": "changes", "words": "Changes files"], "steps": [], "steps_more": 0, "reason": ""]],
        ["call": "tc_31", "run": "run_30", "tool": "other", "title": "Use GMAIL_SEND_EMAIL on the MCP server composio", "can_always": true,
         "detail": ["server": "composio", "tool": "GMAIL_SEND_EMAIL", "arguments": ["recipient_email": "ola@example.com", "subject": "Saturday", "body": "Saturday still works. I'll bring the cake.", "is_html": false]],
         "shown": ["what": "mcp", "more": 0, "code": "",
                   "lines": [["label": "Recipient email", "value": "ola@example.com"], ["label": "Subject", "value": "Saturday"], ["label": "Body", "value": "Saturday still works. I'll bring the cake."], ["label": "Is HTML", "value": "No"]],
                   "raw": "", "title": "Send email in Gmail", "place": "composio · GMAIL_SEND_EMAIL",
                   "badges": [["name": "gmail", "initial": "G", "color": "teal"]], "effect": ["kind": "changes", "words": "Can change"], "steps": [], "steps_more": 0, "reason": ""]],
        ["call": "tc_32", "run": "run_30", "tool": "other", "title": "Use COMPOSIO_MULTI_EXECUTE_TOOL on the MCP server composio", "can_always": true,
         "detail": ["server": "composio", "tool": "COMPOSIO_MULTI_EXECUTE_TOOL", "arguments": [
            "tools": [["tool_slug": "OUTLOOK_OUTLOOK_SEARCH_MESSAGES", "arguments": ["query": "invoice", "top": 10]],
                      ["tool_slug": "GMAIL_FETCH_EMAILS", "arguments": ["query": "from:billing", "max_results": 5]]],
            "thought": "Find last month's invoices in both inboxes so I can total them."]],
         "shown": ["what": "mcp", "lines": [], "more": 0, "code": "",
                   "raw": "{\n  \"tools\": [\n    {\n      \"tool_slug\": \"OUTLOOK_OUTLOOK_SEARCH_MESSAGES\",\n      \"arguments\": {\n        \"query\": \"invoice\",\n        \"top\": 10\n      }\n    },\n    {\n      \"tool_slug\": \"GMAIL_FETCH_EMAILS\",\n      \"arguments\": {\n        \"query\": \"from:billing\",\n        \"max_results\": 5\n      }\n    }\n  ],\n  \"thought\": \"Find last month's invoices in both inboxes so I can total them.\"\n}",
                   "title": "Search messages in Outlook and fetch emails in Gmail", "place": "composio · COMPOSIO_MULTI_EXECUTE_TOOL",
                   "badges": [["name": "outlook", "initial": "O", "color": "sky"], ["name": "gmail", "initial": "G", "color": "teal"]],
                   "effect": ["kind": "changes", "words": "Can change"],
                   "steps": [["title": "Search messages in Outlook", "badge": ["name": "outlook", "initial": "O", "color": "sky"], "chips": [["label": "query", "value": "invoice"], ["label": "top", "value": "10"]], "more": 0],
                             ["title": "Fetch emails in Gmail", "badge": ["name": "gmail", "initial": "G", "color": "teal"], "chips": [["label": "query", "value": "from:billing"], ["label": "max results", "value": "5"]], "more": 0]],
                   "steps_more": 0, "reason": "Find last month's invoices in both inboxes so I can total them."]],
        // A risky one, which Allow is held for.
        ["call": "tc_33", "run": "run_30", "tool": "execute", "title": "rm -rf build", "detail": ["command": "rm -rf build"], "can_always": false, "risky": true,
         "shown": ["what": "", "lines": [], "more": 0, "code": "rm -rf build", "raw": "",
                   "title": "Delete the build folder", "place": "", "badges": [], "effect": ["kind": "deletes", "words": "Risky"], "steps": [], "steps_more": 0, "reason": ""]],
    ]
}
