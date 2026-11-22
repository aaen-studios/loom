import Foundation
import Observation

// What the screens show, as the core tells it, the way desktop/src/model.rs does for the
// desktop app. The model keeps no state of its own beyond that: lists come from the
// core's cache (`core.get`), and are fetched again when an event says they changed. Only
// the text of a reply still being written lives here alone, from its deltas, until the
// finished message replaces it.

/// The lists the app asks the core for.
enum CoreList: String, CaseIterable, Sendable {
    case agents, conversations, runs, tools, approvals, outbox, messages, devices
}

struct Agent: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var color: String
    var program: String
    /// The shape its thread takes in the agent list: one of the eight that come with
    /// loom (Tokens.ThreadShape), "drawn" for its drawing, or "" for the plain thread.
    var shape = ""
    /// The one the person drew, as the instance smoothed it, in a box from -1 to 1.
    var drawing: [CGPoint] = []

    /// The points its thread follows in the list, or nil for the plain thread.
    var outline: [CGPoint]? {
        if shape == "drawn" { return drawing.count > 1 ? drawing : nil }
        return Tokens.ThreadShape.all[shape]
    }
}

/// A drawing as the instance keeps it: `[[x, y], ...]`.
func points(_ j: JSON) -> [CGPoint] {
    j.array.compactMap { p in
        let xy = p.array
        guard xy.count == 2, case .number(let x) = xy[0], case .number(let y) = xy[1] else { return nil }
        return CGPoint(x: x, y: y)
    }
}

struct Conversation: Equatable, Sendable {
    var id: String
    var agent: String
    /// Whether it's the agent's chat, the one the core's `send` with `agent` goes to: not
    /// a task's conversation, the side questions', a branch or a routine's own thread.
    var isChat = true

    init(id: String, agent: String, isChat: Bool = true) {
        self.id = id
        self.agent = agent
        self.isChat = isChat
    }

    init(_ c: JSON) {
        id = c["id"].text
        agent = c["agent"].text
        isChat = c["task"].isNull && !(c["side"].bool ?? false) && c["branch_of"].isNull && c["routine"].isNull
    }
}

/// A file in a message, or one sent: its SHA-256, name, type and size.
struct FileRef: Equatable, Sendable {
    var file: String
    var name: String
    var mime: String
    var size: Int

    init(_ v: JSON) {
        file = v["file"].text
        name = v["name"].text
        mime = v["mime"].text
        size = v["size"].int ?? 0
    }

    init(file: String, name: String, mime: String, size: Int) {
        self.file = file
        self.name = name
        self.mime = mime
        self.size = size
    }

    /// Whether the chat shows it as a picture.
    var isImage: Bool {
        ["image/png", "image/jpeg", "image/gif", "image/webp"].contains(mime)
    }
}

struct PlanEntry: Equatable, Sendable {
    var content: String
    var status: String
}

struct Block: Equatable, Sendable {
    var kind: String
    var text: String
    var call: String?
    var file: FileRef?
    var plan: [PlanEntry] = []

    static func text(_ kind: String, _ text: String) -> Block {
        Block(kind: kind, text: text, call: nil, file: nil)
    }

    static func of(_ b: JSON) -> Block {
        let kind = b["kind"].text
        return Block(
            kind: kind,
            text: b["text"].text,
            call: b["call"].string,
            file: kind == "file" ? FileRef(b) : nil,
            plan: b["entries"].array.map { PlanEntry(content: $0["content"].text, status: $0["status"].text) }
        )
    }
}

struct Message: Identifiable, Equatable, Sendable {
    var id: String
    var role: String
    var at: Int
    var blocks: [Block]
    /// Still being written, from deltas.
    var live = false
    /// Written here and waiting in the outbox.
    var queued = false
    /// For a message another agent or a routine sent, who.
    var from: String?

    var isPerson: Bool { role == "person" }
}

struct Tool: Equatable, Sendable {
    var call: String
    var run: String
    var title: String
    var status: String?
}

struct Approval: Identifiable, Equatable, Sendable {
    var call: String
    var run: String
    var title: String
    var detail: String
    var canAlways: Bool
    var id: String { call }
}

struct Run: Equatable, Sendable {
    var id: String
    var conversation: String
    var agent: String
    var message: String
}

struct Device: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var kind: String
}

/// A file picked to go with the next message.
struct Attachment: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    /// Where it is on this phone, to show it while it goes up.
    var path: String
    var file: FileRef?
    var error: String?
}

/// What an agent's thread shows.
enum Mood: Sendable {
    case idle, thinking, writing, tool, waiting, failed
}

/// Where the connection to the instance is.
enum Conn: Equatable, Sendable {
    case starting, connecting, live, offline(retryMs: Int), signedOut, updateNeeded
}

private struct Queued: Sendable {
    var cid: String
    var agent: String?
    var conversation: String?
    var text: String
    var files: Int
    /// The id it gets once it goes, for one the instance keeps in the conversation's
    /// queue while the agent answers (`ack` with `queued`).
    var message: String?

    init(_ q: JSON) {
        cid = q["cid"].text
        agent = q["agent"].string
        conversation = q["conversation"].string
        text = q["text"].text
        files = q["files"].array.count
    }
}

@MainActor
@Observable
final class Model {
    var conn: Conn = .starting
    var synced = false
    var loginError: String?
    var loggingIn = false
    var agents: [Agent] = []
    var conversations: [Conversation] = []
    var runs: [Run] = []
    var tools: [Tool] = []
    var approvals: [Approval] = []
    var devices: [Device] = []
    /// The selected agent's messages, oldest first, with the queued ones last.
    var messages: [Message] = []
    var moreMessages = false
    var selected: String?
    /// This device's id and the account's, from `welcome`.
    var me: (user: String, device: String)?
    /// Something to tell the person, once.
    var notice: String?
    /// Files for the next message.
    var attachments: [Attachment] = []
    /// Files the core has fetched, by SHA-256, or SHA-256 and ".thumb": where they are.
    var files: [String: String] = [:]
    /// A file fetched to be opened, for the screen to show.
    var toOpen: URL?
    /// When an agent's reply last grew, and when its last run finished or failed.
    var lastDelta: [String: Date] = [:]
    var finished: [String: Date] = [:]
    var failed: [String: Date] = [:]

    /// Replies being written, by message id: their blocks so far.
    private var live: [String: [Block]] = [:]
    private var queued: [Queued] = []
    private var dirty: Set<CoreList> = []
    private var getting: [String: (key: String, open: Bool)] = [:]
    private var nextId = 0

    init() {
        dirty = [.agents, .conversations, .runs, .tools, .approvals, .outbox, .devices]
    }

    private func id(_ prefix: String) -> String {
        nextId += 1
        return "\(prefix)_\(nextId)"
    }

    // MARK: What the screens read

    func agent(_ id: String) -> Agent? { agents.first { $0.id == id } }

    /// The conversation an agent's chat shows: its newest.
    func conversation(of agent: String?) -> String? {
        guard let agent else { return nil }
        return conversations.first { $0.agent == agent && $0.isChat }?.id
    }

    var conversation: String? { conversation(of: selected) }

    /// The selected chat's messages, with the replies still being written at the end.
    var shown: [Message] {
        var out = messages
        let conversation = self.conversation
        for r in runs where r.conversation == conversation {
            if out.contains(where: { $0.id == r.message }) { continue }
            var blocks = live[r.message] ?? []
            for t in tools where t.run == r.id {
                if !blocks.contains(where: { $0.call == t.call }) {
                    var b = Block.text("tool", "")
                    b.call = t.call
                    blocks.append(b)
                }
            }
            out.append(Message(id: r.message, role: "agent", at: .max, blocks: blocks, live: true))
        }
        return out
    }

    /// The questions waiting in the selected chat.
    var shownApprovals: [Approval] {
        let conversation = self.conversation
        return approvals.filter { a in
            runs.contains { $0.id == a.run && $0.conversation == conversation }
        }
    }

    func tool(_ call: String?) -> Tool? {
        guard let call else { return nil }
        return tools.first { $0.call == call }
    }

    /// What an agent's thread shows at `now`.
    func mood(_ agent: String, at now: Date = .now) -> Mood {
        let mine = runs.filter { $0.agent == agent }
        if mine.contains(where: { r in approvals.contains { $0.run == r.id } }) { return .waiting }
        if mine.contains(where: { r in tools.contains { $0.run == r.id && $0.status == nil } }) { return .tool }
        if !mine.isEmpty {
            if let at = lastDelta[agent], now.timeIntervalSince(at) < 0.7 { return .writing }
            return .thinking
        }
        if let at = failed[agent], now.timeIntervalSince(at) < 600 { return .failed }
        return .idle
    }

    /// What the agents list says under an agent's name.
    func preview(_ agent: Agent) -> String {
        guard let r = runs.first(where: { $0.agent == agent.id }) else {
            return mood(agent.id) == .failed ? "Couldn't finish" : agent.program
        }
        if let a = approvals.first(where: { $0.run == r.id }) {
            return "Wants to \(a.title.prefix(1).lowercased())\(a.title.dropFirst())"
        }
        if let t = tools.last(where: { $0.run == r.id && $0.status == nil }) {
            return t.title
        }
        return "Thinking"
    }

    /// Whether the selected chat's agent is answering.
    var busy: Bool {
        let conversation = self.conversation
        return runs.contains { $0.conversation == conversation }
    }

    var uploading: Bool { attachments.contains { $0.file == nil && $0.error == nil } }

    /// This device, as the instance knows it.
    var thisDevice: Device? {
        guard let me else { return nil }
        return devices.first { $0.id == me.device }
    }

    // MARK: Taking frames from the core

    func select(_ agent: String?) {
        guard selected != agent else { return }
        selected = agent
        messages = []
        dirty.insert(.messages)
    }

    /// Takes a frame from the core.
    func take(_ f: JSON) {
        switch f["t"].text {
        case "core.conn":
            let state = f["state"].text
            switch state {
            case "connecting": conn = .connecting
            case "live": conn = .live
            case "offline": conn = .offline(retryMs: f["retry_ms"].int ?? 0)
            case "signed_out": conn = .signedOut
            case "update_needed": conn = .updateNeeded
            default: conn = .starting
            }
            if state != "live" { synced = false }
            if state == "signed_out" {
                loggingIn = false
                me = nil
            }
        case "core.login":
            loggingIn = false
            loginError = f["error"].isNull ? nil : f["error"]["message"].string ?? f["error"]["code"].text
        case "welcome":
            me = (f["user"].text, f["device"].text)
        case "synced":
            synced = true
            dirty.formUnion(CoreList.allCases)
        case "core.data":
            data(f)
        case "core.queued":
            queued.append(Queued(f))
            placeQueued()
        case "core.file":
            fileCame(f)
        case "ack":
            // One that waits in the instance's queue stays shown until its message is stored.
            let cid = f["cid"].text
            if f["queued"].bool == true, let i = queued.firstIndex(where: { $0.cid == cid }) {
                queued[i].message = f["message"].string
            } else {
                queued.removeAll { $0.cid == cid }
            }
            dirty.insert(.messages)
        case "delta":
            delta(f)
        case "event":
            event(f)
        case "result":
            if !f["error"].isNull { notice = f["error"]["message"].string }
        case "error":
            let code = f["code"].text
            if code != "offline" { notice = f["message"].string ?? code }
        default:
            break
        }
    }

    private func event(_ f: JSON) {
        let kind = f["kind"].text
        let family = kind.split(separator: ".").first.map(String.init) ?? kind
        switch family {
        case "agent":
            dirty.insert(.agents)
        case "conversation":
            dirty.formUnion([.conversations, .messages])
        case "message":
            let id = f["message"]["id"].text
            live[id] = nil
            if kind == "message.created" { queued.removeAll { $0.message == id } }
            if f["conversation"].string == conversation { dirty.insert(.messages) }
            dirty.insert(.conversations)
        case "run":
            dirty.insert(.runs)
            if kind == "run.finished" {
                if let r = runs.first(where: { $0.id == f["run"].text }) {
                    if f["status"].text == "failed" {
                        failed[r.agent] = .now
                    } else {
                        finished[r.agent] = .now
                    }
                }
                dirty.formUnion([.tools, .approvals])
            }
        case "tool":
            dirty.insert(.tools)
        case "approval":
            dirty.insert(.approvals)
        case "device":
            dirty.insert(.devices)
        default:
            break
        }
    }

    private func delta(_ f: JSON) {
        let message = f["message"].text
        let block = f["block"].int ?? 0
        var blocks = live[message] ?? []
        while blocks.count <= block {
            blocks.append(.text(f["kind"].text, ""))
        }
        // The core sends only what adds to the text, so the piece goes on the end. `pos`
        // counts UTF-8 bytes.
        let pos = f["pos"].int ?? 0
        let have = blocks[block].text.utf8.count
        if pos <= have {
            if pos < have {
                blocks[block].text = String(decoding: Array(blocks[block].text.utf8.prefix(pos)), as: UTF8.self)
            }
            blocks[block].text += f["text"].text
        }
        live[message] = blocks
        if let r = runs.first(where: { $0.message == message }) {
            lastDelta[r.agent] = .now
        }
    }

    private func data(_ f: JSON) {
        guard let what = CoreList(rawValue: f["what"].text) else { return }
        let items = f["items"].array
        switch what {
        case .agents:
            agents = items.map {
                Agent(id: $0["id"].text, name: $0["name"].text, color: $0["color"].text, program: $0["acp_agent"].text, shape: $0["shape"].text, drawing: points($0["drawing"]))
            }
            if let s = selected, agent(s) == nil { select(nil) }
        case .conversations:
            let before = conversation
            conversations = items.map(Conversation.init)
            if conversation != before { dirty.insert(.messages) }
        case .runs:
            runs = items.map {
                Run(id: $0["run"].text, conversation: $0["conversation"].text, agent: $0["agent"].text, message: $0["message"].text)
            }
        case .tools:
            tools = items.map {
                Tool(call: $0["call"].text, run: $0["run"].text, title: $0["title"].text, status: $0["status"].string)
            }
        case .approvals:
            approvals = items.map { a in
                let detail: String
                switch a["detail"] {
                case .string(let s): detail = s
                case .null: detail = ""
                case let other: detail = other.pretty
                }
                return Approval(call: a["call"].text, run: a["run"].text, title: a["title"].text, detail: detail, canAlways: a["can_always"].bool ?? false)
            }
        case .outbox:
            // The outbox has only what the instance hasn't acked: keep those it queued.
            let waiting = queued.filter { $0.message != nil }
            queued = items.map(Queued.init)
            queued += waiting.filter { w in !queued.contains { $0.cid == w.cid } }
            placeQueued()
        case .devices:
            devices = items.map { Device(id: $0["id"].text, name: $0["name"].text, kind: $0["kind"].text) }
        case .messages:
            moreMessages = f["more"].bool ?? false
            messages = items.map { m in
                Message(id: m["id"].text, role: m["role"].text, at: m["at"].int ?? 0, blocks: m["blocks"].array.map(Block.of), from: from(m["from"].text))
            }
            placeQueued()
        }
    }

    /// Who sent a person message other than the person: another agent, or a routine.
    private func from(_ id: String) -> String? {
        if id.hasPrefix("a_") { return agent(id)?.name ?? "Another agent" }
        if id.hasPrefix("rt_") { return "A routine" }
        return nil
    }

    /// The queued messages for the shown chat go after its stored ones.
    private func placeQueued() {
        messages.removeAll { $0.queued }
        let conversation = self.conversation
        for q in queued {
            if let m = q.message, messages.contains(where: { $0.id == m }) { continue }
            let here = q.conversation.map { $0 == conversation } ?? (q.agent != nil && q.agent == selected)
            guard here else { continue }
            var text = q.text
            if q.files > 0 {
                let files = q.files == 1 ? "1 file" : "\(q.files) files"
                text = text.isEmpty ? files : "\(text)\n\n\(files)"
            }
            messages.append(Message(id: q.cid, role: "person", at: .max, blocks: [.text("text", text)], queued: true))
        }
    }

    private func fileCame(_ f: JSON) {
        let id = f["id"].text
        let error = f["error"].isNull ? nil : f["error"]["message"].text
        if let i = attachments.firstIndex(where: { $0.id == id }) {
            if error == nil, !f["file"].isNull {
                attachments[i].file = FileRef(f["file"])
            } else {
                attachments[i].error = error ?? "It didn't go up."
            }
            return
        }
        guard let (key, open) = getting.removeValue(forKey: id) else { return }
        if error == nil, let path = f["path"].string {
            files[key] = path
            if open { toOpen = URL(fileURLWithPath: path) }
        } else if open {
            notice = error ?? "The file couldn't be fetched."
        }
    }

    // MARK: What the app sends

    /// The frames to send the core for the lists that changed, and the thumbnails the
    /// shown chat needs.
    func fetches() -> [JSON] {
        var out: [JSON] = []
        let lists = dirty
        dirty = []
        for l in CoreList.allCases where lists.contains(l) {
            if l == .messages {
                guard let conversation else {
                    messages = []
                    placeQueued()
                    continue
                }
                out.append(["t": "core.get", "id": "app.messages", "what": "messages", "conversation": JSON(conversation), "limit": 200])
            } else {
                out.append(["t": "core.get", "id": JSON("app.\(l.rawValue)"), "what": JSON(l.rawValue)])
            }
        }
        let wanted = shown.flatMap(\.blocks).compactMap(\.file).filter(\.isImage).map(\.file)
        for sha in Set(wanted) {
            let key = "\(sha).thumb"
            if files[key] != nil || getting.values.contains(where: { $0.key == key }) { continue }
            let id = id("app.get")
            getting[id] = (key, false)
            out.append(["t": "file.get", "id": JSON(id), "file": JSON(sha), "thumb": true])
        }
        return out
    }

    /// The next message, with the attachments that are up. Nil while one is still going
    /// up, or with nothing to send.
    func send(_ text: String) -> JSON? {
        guard let agent = selected, !uploading else { return nil }
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let files = attachments.compactMap { $0.file?.file }
        if text.isEmpty && files.isEmpty { return nil }
        attachments = []
        var frame: [String: JSON] = ["t": "send", "agent": JSON(agent), "text": JSON(text)]
        if !files.isEmpty { frame["files"] = JSON(files) }
        return .object(frame)
    }

    /// Starts sending a file for the next message.
    func attach(path: String, name: String) -> JSON {
        let id = id("app.up")
        attachments.append(Attachment(id: id, name: name, path: path))
        return ["t": "file.upload", "id": JSON(id), "path": JSON(path)]
    }

    func unattach(_ id: String) {
        attachments.removeAll { $0.id == id }
    }

    /// Opens a file: at once if it's here, or once it's fetched.
    func open(_ f: FileRef) -> JSON? {
        if let path = files[f.file] {
            toOpen = URL(fileURLWithPath: path)
            return nil
        }
        let id = id("app.get")
        getting[id] = (f.file, true)
        return ["t": "file.get", "id": JSON(id), "file": JSON(f.file), "name": JSON(f.name)]
    }

    func login(server: String, username: String, password: String, device: JSON) -> JSON {
        loggingIn = true
        loginError = nil
        var frame: [String: JSON] = ["t": "login", "id": JSON(id("app.login")), "username": JSON(username.trimmingCharacters(in: .whitespaces)),
                                     "password": JSON(password), "device": device]
        // Left out, the core uses the instance it last knew.
        let server = server.trimmingCharacters(in: .whitespaces)
        if !server.isEmpty { frame["server"] = JSON(server) }
        return .object(frame)
    }

    /// Joins the instance in a QR code the desktop app shows under Add a phone.
    func link(qr: String, device: JSON) -> JSON {
        loggingIn = true
        loginError = nil
        return ["t": "link", "id": JSON(id("app.link")), "qr": JSON(qr), "device": device]
    }

    func cancel() -> JSON? {
        guard let conversation, let run = runs.first(where: { $0.conversation == conversation }) else { return nil }
        return ["t": "cancel", "run": JSON(run.id)]
    }

    func approve(_ call: String, allow: Bool, always: Bool = false) -> JSON {
        ["t": "approve", "call": JSON(call), "allow": JSON(allow), "always": JSON(always)]
    }

    func startFresh() -> JSON? {
        guard let agent = selected else { return nil }
        return ["t": "cmd", "id": JSON(id("app.fresh")), "name": "conversation.create", "args": ["agent": JSON(agent)]]
    }
}
