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
    private var agents: [JSON]
    private var conversations: [JSON]
    private var messages: [String: [JSON]]
    private var runs: [JSON] = []
    private var tools: [JSON] = []
    private var approvals: [JSON] = []
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
        tell(["t": "welcome", "version": 1, "user": "u_1", "device": "d_4", "seq": JSON(seq), "reset": false])
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
        case "login", "link":
            later(0.6) { [self] in
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
        case "messages": items = messages[f["conversation"].text] ?? []
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
            event("message.created", ["conversation": JSON(conversation), "message": ["id": JSON(mine), "conversation": JSON(conversation), "role": "person", "from": "d_4", "at": JSON(Sample.now()), "blocks": .array(blocks)]])
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

    static let streamed = ["Let me ", "check ", "the last ", "run on ", "main."]
    static let finished = """
    Yes. The last build on **main** passed at 14:02, in 2 minutes.

    ```sh
    gh run list --branch main --limit 1
    ```

    Nothing failed, so there's nothing to fix.
    """

    static let agents: [JSON] = [
        ["id": "a_1", "name": "Builder", "color": "periwinkle", "shape": "yarn", "acp_agent": "claude", "permission": "ask"],
        ["id": "a_2", "name": "Scribe", "color": "mint", "shape": "heart", "acp_agent": "claude", "permission": "auto"],
        ["id": "a_3", "name": "Postmaster", "color": "apricot", "acp_agent": "codex", "permission": "ask"],
        ["id": "a_4", "name": "Gardener", "color": "pink", "shape": "star", "acp_agent": "gemini", "permission": "allow"],
    ]

    static let conversations: [JSON] = [
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
                ["kind": "text", "text": "Here they are, from the **12 PRs** merged since `v0.2`:\n\n## loom 0.3\n\n- Routines run on webhooks, signed as GitHub signs.\n- Agents get your MCP servers, with secrets sealed.\n- Screenshots and the clipboard as device tools.\n\nI kept them short. Want the full list too?"],
             ]],
            ["id": "msg_3", "conversation": "c_1", "role": "person", "from": "d_4", "at": 1795187100000,
             "blocks": [["kind": "text", "text": "That's good. Did the build on main pass?"]]],
        ],
        "c_2": [
            ["id": "msg_10", "conversation": "c_2", "role": "person", "from": "d_1", "at": 1795100000000,
             "blocks": [["kind": "text", "text": "Write a function that turns bytes into a size people read."]]],
            ["id": "msg_11", "conversation": "c_2", "role": "agent", "agent": "a_2", "run": "run_10", "at": 1795100030000,
             "blocks": [["kind": "text", "text": "In Swift:\n\n```swift\nfunc size(_ bytes: Int) -> String {\n    switch bytes {\n    case ..<1000: return \"\\(bytes) B\"\n    case ..<1_000_000: return \"\\(bytes / 1000) KB\"\n    default: return String(format: \"%.1f MB\", Double(bytes) / 1e6)\n    }\n}\n```\n\nIt counts in thousands, as Finder does."]]],
        ],
        "c_3": [
            // A routine's prompt kept from before routines ran quietly: not shown.
            ["id": "msg_20", "conversation": "c_3", "role": "person", "from": "rt_1", "at": 1795170000000,
             "blocks": [["kind": "text", "text": "Read the new mail in every inbox and tell me what needs me."]]],
            // What the routine's agent posted, which shows as any reply does.
            ["id": "msg_21", "conversation": "c_3", "role": "agent", "agent": "a_3", "routine": "rt_1", "at": 1795170090000,
             "blocks": [["kind": "text", "text": "Two things need you:\n\n1. The VPS invoice is due Friday.\n2. Ola asked whether Saturday still works."]]],
        ],
        "c_4": [
            ["id": "msg_40", "conversation": "c_4", "role": "person", "from": "a_1", "at": 1795160000000,
             "blocks": [["kind": "text", "text": "Water the tomatoes on the balcony when the forecast says no rain."]]],
            ["id": "msg_41", "conversation": "c_4", "role": "person", "from": "d_4", "at": 1795187000000,
             "blocks": [["kind": "file", "file": "sample-tall", "name": "balcony.png", "mime": "image/png", "size": 412_000],
                        ["kind": "text", "text": "Is this one ready to pick?"]]],
            ["id": "msg_42", "conversation": "c_4", "role": "agent", "agent": "a_4", "run": "run_41", "at": 1795187060000,
             "blocks": [
                ["kind": "tool", "call": "tc_40"],
                ["kind": "text", "text": "The photo is a little dark, so I'll check the colour near the stem."],
                ["kind": "tool", "call": "tc_41"],
                ["kind": "text", "text": "Not yet. It's still orange near the stem; give it two or three more days."],
             ]],
        ],
    ]

    static let runs: [JSON] = [
        ["run": "run_30", "conversation": "c_2", "agent": "a_2", "message": "msg_31", "kind": "run.started"],
        // A routine's quiet run: not drawn in the chat, and not shown as working.
        ["run": "run_21", "conversation": "c_3", "agent": "a_3", "message": "msg_22", "routine": "rt_1", "kind": "run.started"],
    ]

    /// The file of a sample picture, drawn the first time it's asked for.
    static func picture(_ sha: String) -> String? {
        guard sha == "sample-tall" else { return nil }
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("loom-sample-tall.png").path
        if FileManager.default.fileExists(atPath: path) { return path }
        let image = UIGraphicsImageRenderer(size: CGSize(width: 300, height: 800), format: {
            let f = UIGraphicsImageRendererFormat()
            f.scale = 1
            return f
        }()).image { c in
            UIColor(red: 0.55, green: 0.75, blue: 0.95, alpha: 1).setFill()
            c.fill(CGRect(x: 0, y: 0, width: 300, height: 800))
            UIColor(red: 0.3, green: 0.6, blue: 0.3, alpha: 1).setFill()
            c.fill(CGRect(x: 0, y: 520, width: 300, height: 280))
            UIColor(red: 0.95, green: 0.45, blue: 0.2, alpha: 1).setFill()
            UIBezierPath(ovalIn: CGRect(x: 90, y: 360, width: 120, height: 120)).fill()
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

    static let approvals: [JSON] = [
        ["call": "tc_30", "run": "run_30", "tool": "edit", "title": "Edit notes/2026-11.md", "detail": "+ ## 21 November\n+ Shipped the iPhone app's first build.", "can_always": true],
    ]
}
