import Foundation
import Observation

// What the screens show, as the core tells it, the way desktop/src/model.rs does for the
// desktop app. The model keeps no state of its own beyond that: lists come from the
// core's cache (`core.get`), and are fetched again when an event says they changed. Only
// the text of a reply still being written lives here alone, from its deltas, until the
// finished message replaces it.

/// The lists the app asks the core for.
enum CoreList: String, CaseIterable, Sendable {
    case agents, conversations, runs, tools, approvals, outbox, messages, devices, sidebar
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
    /// When it was pinned to the top of the list, and its place where the person
    /// dragged it, if either.
    var pinned: Int?
    var order: Int?
    /// The sidebar folder it's in, if any, and whether it's hidden there.
    var group: String?
    var hidden = false
    /// What the person told it, which the composer's @ picker shows the first line of.
    var instructions = ""

    /// The points its thread follows in the list, or nil for the plain thread.
    var outline: [CGPoint]? {
        if shape == "drawn" { return drawing.count > 1 ? drawing : nil }
        return Tokens.ThreadShape.all[shape]
    }
}

/// Agents as the desktop lists them (model.rs, sort_agents): pinned ones first; within
/// each, where the person dragged them, then the ones not placed yet, as they were made.
func listed(_ agents: [Agent]) -> [Agent] {
    agents.enumerated().sorted { a, b in
        let ka = (a.element.pinned == nil ? 1 : 0, a.element.order ?? Int.max, a.element.pinned ?? 0, a.offset)
        let kb = (b.element.pinned == nil ? 1 : 0, b.element.order ?? Int.max, b.element.pinned ?? 0, b.offset)
        return ka < kb
    }.map(\.element)
}

/// A part of the agent list, as the core groups the agents (core/sidebar.calx), the way
/// the desktop's sidebar shows them: who's waiting on the person, the pinned ones, the
/// rest not in a folder, each folder, and the hidden ones.
struct SideSection: Equatable, Sendable {
    enum Part: Equatable, Sendable {
        case waiting, pinned, agents, hidden
        case folder(id: String, name: String, collapsed: Bool)
    }
    var part: Part
    /// Its agents' ids, in order.
    var agents: [String]

    init?(_ j: JSON) {
        switch j["kind"].text {
        case "waiting": part = .waiting
        case "pinned": part = .pinned
        case "agents": part = .agents
        case "hidden": part = .hidden
        case "group": part = .folder(id: j["id"].text, name: j["name"].text, collapsed: j["collapsed"].bool ?? false)
        default: return nil
        }
        agents = j["agents"].array.map(\.text)
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
    /// a task's conversation, the side questions', a branch, a routine's own thread or
    /// another agent's thread with it.
    var isChat = true
    /// For another agent's thread with it (`message_agent`), that agent's id, or
    /// `plugin:ID` for a plugin's panel.
    var askedBy: String?
    var title = ""

    init(id: String, agent: String, isChat: Bool = true, askedBy: String? = nil, title: String = "") {
        self.id = id
        self.agent = agent
        self.isChat = isChat && askedBy == nil
        self.askedBy = askedBy
        self.title = title
    }

    init(_ c: JSON) {
        id = c["id"].text
        agent = c["agent"].text
        askedBy = c["asked_by"].string
        title = c["title"].text
        isChat = c["task"].isNull && !(c["side"].bool ?? false) && c["branch_of"].isNull && c["routine"].isNull && askedBy == nil
    }
}

/// Another agent's thread with an agent, as its chat lists it.
struct Ask: Hashable, Identifiable {
    func hash(into h: inout Hasher) { h.combine(id) }

    var id: String
    /// The agent that asks, when it's one of the person's agents still.
    var by: Agent?
    /// "From Casper", by the asking agent's name now, or the thread's own title.
    var label: String
}

/// Pictures shown big, to page through.
struct Viewing: Identifiable {
    let id = UUID()
    var stack: [FileRef]
}

/// A file in a message, or one sent: its SHA-256, name, type and size.
struct FileRef: Equatable, Sendable {
    var file: String
    var name: String
    var mime: String
    var size: Int
    /// A picture's width and height in pixels as it shows, as the instance read them from
    /// its header (protocol.md, "Files"); 0 where they aren't known.
    var width: Int
    var height: Int

    init(_ v: JSON) {
        file = v["file"].text
        name = v["name"].text
        mime = v["mime"].text
        size = v["size"].int ?? 0
        width = v["width"].int ?? 0
        height = v["height"].int ?? 0
    }

    init(file: String, name: String, mime: String, size: Int, width: Int = 0, height: Int = 0) {
        self.file = file
        self.name = name
        self.mime = mime
        self.size = size
        self.width = width
        self.height = height
    }

    /// A picture's size, where the instance said it.
    var shownSize: CGSize? {
        width > 0 && height > 0 ? CGSize(width: width, height: height) : nil
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

/// One line of a card: a checklist's item, done or not, or a fact's label and value.
struct CardItem: Equatable, Sendable {
    var text: String
    var value = ""
    var done = false
}

/// A card in a reply (PLAN.md, "Cards in the chat"): an agent's `show_card` (`choices`,
/// `checklist`, `table`, `progress`, `facts`) or loom's own `sign_in` and `mcp_add`, with
/// the fields its type has.
struct Card: Equatable, Sendable {
    var id: String
    var type: String
    var title: String
    var options: [String] = []
    var items: [CardItem] = []
    var columns: [String] = []
    var rows: [[String]] = []
    var label = ""
    var value = 0.0
    var server = ""
    /// For `mcp_add`: the call `mcp.add_answer` names, the server's address, or the
    /// command and arguments that start it, its settings, how it stands, and a line in
    /// plain words.
    var call = ""
    var url = ""
    var command = ""
    var args: [String] = []
    var settings: [CardSetting] = []
    var state = ""
    var said = ""
    /// Once an added server is ready, the tools it offers.
    var tools: [String] = []
    /// Loom's own cards' server's badge (`shared/badge.calx`).
    var badge: Badge?
    /// For `secret_ask`: the agent that asked, the site, the fields, the names what was
    /// given is kept as, whether the answer is on its way, and what the instance said to
    /// one it refused.
    var agent = ""
    var site = ""
    var fields: [CardField] = []
    var saved: [String] = []
    var sending = false
    var error = ""
    /// For choices, the answer the person sent once the chat went on.
    var chosen: Int?
    /// For a form: its fields, the reply it's in (for Send), what's picked on each
    /// choice and toggle so far, and the answers, by label, once they're sent. `sending`
    /// and `error` say while they're on their way, and what the instance said to them.
    var formFields: [FormField] = []
    var message = ""
    var picks: [JSON] = []
    var answers: JSON = .null
    /// That its reply is still being written: its questions don't answer yet, so the
    /// person reads it all first.
    var live = false
    /// That the chat has gone on past its reply: choices no longer answer then, and a
    /// sign-in only waits quietly.
    var passed = false
    /// For a card that signs in, as `mcp.list` says: whom the person is signed in to its
    /// server as once they are (empty when the sign-in didn't say), whether its sign-in
    /// page is open now, and why the last one here didn't finish.
    var signedAs: String?
    var signing = false
    var signInError: String?

    /// Whether its questions (choices and forms) answer now: the chat hasn't gone on,
    /// and its reply is finished.
    var takesAnswers: Bool { !passed && !live }

    /// Whether they will once its reply is finished: the chat hasn't gone on, and it's
    /// still being written. A form placed `here` waits then, rather than reading as
    /// passed by.
    var answersLater: Bool { !passed && live }

    /// Whether it signs in to its server: a sign-in card, or a proposed server that
    /// waits on one.
    var signsIn: Bool {
        type == "sign_in" || (type == "mcp_add" && ["sign_in", "later"].contains(state))
    }

    init?(_ c: JSON) {
        type = c["type"].text
        guard ["choices", "form", "checklist", "table", "progress", "facts", "sign_in", "mcp_add", "secret_ask"].contains(type) else { return nil }
        if type == "form" {
            formFields = c["fields"].array.map {
                FormField(label: $0["label"].text, kind: $0["kind"].text, required: $0["required"].bool ?? false, options: $0["options"].array.map(\.text), placeholder: $0["placeholder"].text, value: $0["value"])
            }
            picks = formFields.map(\.value)
            answers = c["answers"]
        }
        agent = c["agent"].text
        site = c["site"].text
        fields = type == "form" ? [] : c["fields"].array.map {
            CardField(label: $0["label"].text, kind: $0["kind"].text, masked: $0["masked"].bool ?? false, save: $0["save"].text)
        }
        saved = c["saved"].array.map(\.text)
        call = c["call"].text
        url = c["url"].text
        command = c["command"].text
        args = c["args"].array.map(\.text)
        settings = c["settings"].array.map {
            CardSetting(name: $0["name"].text, value: $0["value"].text, secret: $0["secret"].text, have: $0["have"].bool ?? false)
        }
        state = c["state"].text
        said = c["said"].text
        tools = c["tools"].array.map(\.text)
        badge = Badge(c["badge"])
        id = c["id"].text
        title = c["title"].text
        options = c["options"].array.map(\.text)
        items = c["items"].array.map { i in
            type == "facts" ? CardItem(text: i["label"].text, value: i["value"].text) : CardItem(text: i["text"].text, done: i["done"].bool ?? false)
        }
        columns = c["columns"].array.map(\.text)
        rows = c["rows"].array.map { $0.array.map(\.text) }
        label = c["label"].text
        value = min(max(c["value"].double ?? 0, 0), 1)
        server = c["server"].text
    }
}

struct Block: Equatable, Sendable {
    var kind: String
    var text: String
    var call: String?
    var file: FileRef?
    var plan: [PlanEntry] = []
    /// The subagent call a tool call, words or a thought sit inside (protocol.md,
    /// "Messages and streaming"): the phone leaves them out of the reply.
    var parent: String?
    var card: Card?

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
            plan: b["entries"].array.map { PlanEntry(content: $0["content"].text, status: $0["status"].text) },
            parent: b["parent"].string,
            card: kind == "card" ? Card(b["card"]) : nil
        )
    }
}

/// A header or environment variable of an MCP server an agent proposed: its text, or the
/// name of one of the person's secrets, and whether they have that secret yet.
struct CardSetting: Equatable, Sendable {
    var name: String
    var value: String
    var secret: String
    var have: Bool

    /// A secret the person types on the card before Add.
    var wanted: Bool { !secret.isEmpty && !have }
}

/// A field the person fills in on a card: what it's for, whether it's hidden as it's
/// typed, and the name it's offered to be kept as among their secrets ("" for none).
struct CardField: Equatable, Sendable {
    var label: String
    var kind: String
    var masked: Bool
    var save: String
}

/// A field of an agent's form: what it asks, its kind (`text`, `choice`, `toggle` or
/// `number`), whether it must be answered, a choice's options, what a text or a number
/// says while it's empty, and what it starts as.
struct FormField: Equatable, Sendable {
    var label: String
    var kind: String
    var required: Bool
    var options: [String]
    var placeholder: String
    var value: JSON

    /// What it starts as, as text in its box: a number as it's written, nothing for null.
    var startText: String {
        switch value {
        case .string(let s): return s
        case .number(let n): return Int(exactly: n).map(String.init) ?? String(n)
        default: return ""
        }
    }
}

/// A form's answers, by label, as desktop/src/model.rs's `form_answers`: from what's
/// typed in its text and number fields (`typed`, one for each field) and what's picked on
/// its choices and toggles (`picks`); or the label of the first required field with
/// nothing in it. An optional field left empty is left out.
func formAnswers(_ fields: [FormField], typed: [String], picks: [JSON]) -> Result<JSON, FormMissing> {
    var out: [String: JSON] = [:]
    for (k, f) in fields.enumerated() {
        let pick = k < picks.count ? picks[k] : .null
        var answer = JSON.null
        switch f.kind {
        case "toggle": answer = .bool(pick.bool ?? false)
        case "choice": if let o = pick.string, !o.isEmpty { answer = .string(o) }
        default:
            let t = k < typed.count ? typed[k].trimmingCharacters(in: .whitespacesAndNewlines) : ""
            if !t.isEmpty { answer = .string(t) }
        }
        if answer.isNull {
            if f.required { return .failure(FormMissing(label: f.label)) }
            continue
        }
        out[f.label] = answer
    }
    return .success(.object(out))
}

/// The required field a form's Send waits on.
struct FormMissing: Error, Equatable {
    var label: String
}

/// How long Allow on a risky question is held before it allows (PLAN.md, "Permissions"),
/// as the desktop's `HOLD`.
let holdTime: TimeInterval = 1.2

/// One of the person's secrets, by name, and the sites it may be filled in on in their
/// agents' browsers ("" for a file on the instance's computer).
struct SecretItem: Equatable, Sendable, Identifiable {
    var name: String
    var sites: [String]
    var id: String { name }
}

/// Which of a reply's blocks sit inside a subagent call of the same reply, as
/// desktop/src/model.rs's `inside_a_call`: their parent is the call of one of its tool
/// blocks.
func insideACall(_ m: Message) -> [Bool] {
    let calls = Set(m.blocks.filter { $0.kind == "tool" }.compactMap(\.call))
    return m.blocks.map { b in b.parent.map { calls.contains($0) } ?? false }
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
    /// For a message another agent sent (`message_agent`), its name, and its id.
    var from: String?
    var fromAgent: String?
    /// How a reply's turn ended: `done`, or `cancelled`, `failed` or `limit` for one cut
    /// short.
    var status = ""
    /// The Sources list the instance took off the end of a reply's words, shown on a tab
    /// under its bubble.
    var sources: [Source] = []
    /// The pages it links to that the instance has looked up, by address.
    var links: [String: PageLink] = [:]
    /// For a reply, the files it names in code that are in its agent's folder or the shared
    /// one (`mentions`): each path as it wrote it, and the file's name.
    var named: [NamedFile] = []
    /// For a person's message that replies to another, what it replies to.
    var reply: ReplyTo?

    var isPerson: Bool { role == "person" }

    /// A source as the tab and its list show it: the title the reply gave, or else the
    /// page's own, or else its site; and its site's icon, if the instance found one.
    func shown(_ s: Source) -> (title: String, site: String, icon: String?) {
        let link = links[s.url]
        let title = [s.title, link?.title ?? "", s.site, s.url]
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? s.url
        return (title, s.site, link?.icon)
    }

    /// A turn that didn't end by itself, which keeps everything it had in view, since
    /// there's no answer to fold its steps under.
    var cutShort: Bool { !live && ["cancelled", "failed", "limit"].contains(status) }

    /// What it says, its text blocks one after another, to copy or select.
    var text: String {
        blocks.filter { $0.kind == "text" && $0.parent == nil && !$0.text.isEmpty }.map(\.text).joined(separator: "\n\n")
    }
}

/// What a person's message replies to (PLAN.md, "Replying to a message"), as the instance
/// keeps it with the message, and as the composer's bar shows it while it's written:
/// desktop/src/model.rs's `ReplyTo`.
struct ReplyTo: Equatable, Sendable {
    var message: String
    /// `agent` or `person`.
    var role: String
    /// The agent that said it, for an agent's; the device or agent that sent it, for a person's.
    var agent = ""
    var from = ""
    /// The part selected, or the message's words.
    var text: String
    var part = false
    var ask = false
    var files: [FileRef] = []

    static func of(_ m: JSON) -> ReplyTo? {
        let r = m["reply"]
        guard !r["message"].text.isEmpty else { return nil }
        return ReplyTo(message: r["message"].text, role: r["role"].text, agent: r["agent"].text, from: r["from"].text, text: r["text"].text, part: r["part"].bool ?? false, ask: r["ask"].bool ?? false, files: r["files"].array.map(FileRef.init))
    }

    /// A reply to `m` in `agent`'s chat: to the part `quote` when one was selected, asking
    /// about it with `ask`. A reply's words are its answer, its last words; a person's, all.
    static func to(_ m: Message, agent: String, quote: String?, ask: Bool) -> ReplyTo {
        let quote = quote?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
        let words = m.blocks.filter { $0.kind == "text" && $0.parent == nil }.map { $0.text.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        let text = quote ?? (m.isPerson ? words.joined(separator: "\n") : words.last ?? "")
        return ReplyTo(message: m.id, role: m.role, agent: m.isPerson ? "" : agent, from: m.fromAgent ?? "", text: text, part: quote != nil, ask: ask && quote != nil, files: quote != nil ? [] : m.blocks.compactMap(\.file))
    }

    /// Its words without their Markdown, each line's heading or quote marks taken off, as
    /// the desktop's `spoken` has them.
    var words: String {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        let plain = String(((try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)).characters)
        return plain.split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.drop { $0 == "#" || $0 == ">" || $0 == " " } }
            .joined(separator: "\n")
    }

    /// Its first line with something in it.
    var line: String {
        words.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
    }

    /// Its words on one plain line, as the quote over a message shows them, as the
    /// desktop's `one_line`: list marks and rules taken off too, its lines run together
    /// with spaces, and cut at 240 characters with "…".
    var oneLine: String {
        var out: [Substring] = []
        for raw in words.split(separator: "\n") {
            var line = Substring(raw.trimmingCharacters(in: .whitespaces))
            // A rule, or a table's divider, says nothing.
            if line.allSatisfy({ "-=_|: ".contains($0) }) { continue }
            for mark in ["- ", "+ ", "* ", "• "] where line.hasPrefix(mark) {
                line = line.dropFirst(mark.count).drop { $0 == " " }
                break
            }
            for mark in ["[ ] ", "[x] ", "[X] "] where line.hasPrefix(mark) {
                line = line.dropFirst(mark.count).drop { $0 == " " }
                break
            }
            out += line.split { $0.isWhitespace || $0 == "|" }
        }
        let joined = out.joined(separator: " ")
        return joined.count > 240 ? String(joined.prefix(240)) + "…" : joined
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}

/// One of a reply's sources (PLAN.md, "Links and sources").
struct Source: Equatable, Sendable {
    var url: String
    /// As the reply wrote it, maybe empty.
    var title: String
    var site: String

    static func of(_ j: JSON) -> Source {
        Source(url: j["url"].text, title: j["title"].text, site: j["site"].text)
    }
}

/// A page a message links to, as the instance looked it up.
struct PageLink: Equatable, Sendable {
    /// The page's own, maybe empty.
    var title: String
    var site: String
    /// Its site's icon, a file to fetch.
    var icon: String?
    /// Its preview picture (its `og:image`), a file to fetch: only for a page pasted in
    /// the composer.
    var image: String? = nil

    static func all(_ j: JSON) -> [String: PageLink] {
        guard case .object(let o) = j else { return [:] }
        return o.mapValues(PageLink.of)
    }

    /// A page as the instance looked it up: `{title, site, icon?, image?}`.
    static func of(_ v: JSON) -> PageLink {
        let file = { (key: String) in v[key].string.flatMap { $0.isEmpty ? nil : $0 } }
        return PageLink(title: v["title"].text, site: v["site"].text, icon: file("icon"), image: file("image"))
    }
}

/// A link pasted in the composer, as it's looked up (`Model.previewLink`).
enum Preview: Equatable, Sendable {
    case loading
    case ready(PageLink)
    /// It couldn't be looked up: it shows as its site.
    case failed
}

/// A message's words with each bare address that was looked up as its page's title,
/// linking to it, as desktop/src/model.rs's `with_link_icons` does. Code stays as it is.
func titledLinks(_ text: String, _ links: [String: PageLink]) -> String {
    guard !links.isEmpty, text.contains("http") else { return text }
    var out: [String] = []
    var fenced = false
    for line in text.components(separatedBy: "\n") {
        let t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("```") || t.hasPrefix("~~~") {
            fenced.toggle()
            out.append(line)
            continue
        }
        out.append(fenced ? line : titledLine(line, links))
    }
    return out.joined(separator: "\n")
}

/// A page's title as a bare address shows it: at most 60 characters, cut at a word, as
/// desktop/src/model.rs's `short_title`.
func shortTitle(_ title: String) -> String {
    guard title.count > 60 else { return title }
    let cut = String(title.prefix(60))
    let words = cut.range(of: " ", options: .backwards).map { String(cut[..<$0.lowerBound]) } ?? cut
    var end = Substring(words)
    while let last = end.last, " ·-|,:.".contains(last) { end = end.dropLast() }
    return String(end) + "…"
}

private func titledLine(_ line: String, _ links: [String: PageLink]) -> String {
    var out = ""
    var rest = Substring(line)
    while let c = rest.first {
        // An address the person sent as plain text: itself, linking to it, with no title.
        if rest.hasPrefix("<http://") || rest.hasPrefix("<https://"), let end = rest.firstIndex(of: ">") {
            let url = rest[rest.index(after: rest.startIndex)..<end]
            out += "[\(url)](\(url))"
            rest = rest[rest.index(after: end)...]
            continue
        }
        // Inline code and Markdown links stay as written.
        if c == "`" {
            let after = rest.dropFirst()
            guard let end = after.firstIndex(of: "`") else { out += rest; break }
            out += rest[...end]
            rest = after[after.index(after: end)...]
            continue
        }
        if c == "[", let close = rest.range(of: "]("), let end = rest[close.upperBound...].firstIndex(of: ")"), !rest[..<close.lowerBound].contains("\n") {
            out += rest[...end]
            rest = rest[rest.index(after: end)...]
            continue
        }
        if rest.hasPrefix("https://") || rest.hasPrefix("http://") {
            let end = rest.firstIndex { $0.isWhitespace || "<>\"`".contains($0) } ?? rest.endIndex
            var url = rest[..<end]
            while let last = url.last, ".,;:!?)'".contains(last) { url = url.dropLast() }
            if url.isEmpty { url = rest[..<end] }
            if let title = links[String(url)]?.title.trimmingCharacters(in: .whitespaces), !title.isEmpty {
                let plain = shortTitle(title).replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: "")
                out += "[\(plain)](\(url))"
            } else {
                out += url
            }
            rest = rest[url.endIndex...]
            continue
        }
        out.append(c)
        rest = rest.dropFirst()
    }
    return out
}

// MARK: Mentions (PLAN.md, "Mentions")

/// Where a link in a message's words goes, as desktop/src/model.rs's `line_links` tells
/// them apart.
enum LinkTo: Equatable, Sendable {
    /// A page on the web.
    case page
    /// An agent it mentions, by id.
    case agent(String)
    /// A file it mentions, in the agent's folder or the shared one: the agent, and the
    /// path as a reply would name it.
    case file(agent: String, path: String)
    /// A file sent with the message, by its SHA-256.
    case attached(String)
    /// A skill picked from the + menu, by its name.
    case skill(String)
    case other

    init(_ link: String) {
        if link.hasPrefix("http://") || link.hasPrefix("https://") {
            self = .page
        } else if link.hasPrefix("loom:agent/") {
            self = .agent(String(link.dropFirst("loom:agent/".count)))
        } else if link.hasPrefix("loom:file/") {
            let rest = link.dropFirst("loom:file/".count)
            guard let cut = rest.firstIndex(of: "/") else {
                self = .other
                return
            }
            let path = String(rest[rest.index(after: cut)...])
            self = .file(agent: String(rest[..<cut]), path: path.removingPercentEncoding ?? path)
        } else if link.hasPrefix("loom-file:") {
            self = .attached(String(link.dropFirst("loom-file:".count)))
        } else if link.hasPrefix("loom:skill/") {
            self = .skill(String(link.dropFirst("loom:skill/".count)))
        } else {
            self = .other
        }
    }
}

/// Whether a file of this name is a picture, by its type.
func isPicture(_ name: String) -> Bool {
    let lower = name.lowercased()
    return [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".heic", ".svg"].contains { lower.hasSuffix($0) }
}

/// A person's words with each of the message's files marked where it was attached
/// (`[name]`) made a link to it, `loom-file:` and its SHA-256, which the chat opens, as
/// desktop/src/model.rs's `link_files`. A name that can't be a link's words is left as
/// it is.
func linkFiles(_ text: String, _ files: [FileRef]) -> String {
    var out = text
    for f in files where !f.name.isEmpty && !f.name.contains(where: { "[]`\n".contains($0) }) {
        let mark = "[\(f.name)]"
        // Not one that's a link already.
        var from = out.startIndex
        while let r = out.range(of: mark, range: from..<out.endIndex) {
            if out[r.upperBound...].hasPrefix("(") {
                from = r.upperBound
                continue
            }
            out.replaceSubrange(r, with: "\(mark)(loom-file:\(f.file))")
            break
        }
    }
    return out
}

/// A file a reply names in code that's in its agent's folder or the shared one: its path as
/// the reply wrote it, its name, and for a picture, its thumbnail, kept as the person's
/// file once the instance has made it, which the reply shows (PLAN.md, "Mentions").
struct NamedFile: Equatable, Sendable {
    var path: String
    var name: String
    var size: Int = 0
    var thumb: String? = nil

    /// Its thumbnail as a picture in the chat draws it, a file like any other.
    var picture: FileRef? {
        thumb.map { FileRef(file: $0, name: name, mime: "image/jpeg", size: size) }
    }
}

/// A mention the composer put in: the words it put in the field, and the `loom:` link it
/// goes as.
struct Mention: Equatable, Sendable {
    var words: String
    var link: String
}

/// An agent as a mention: `@Casper`, going as `loom:agent/<id>`.
func agentMention(id: String, name: String) -> Mention {
    Mention(words: "@\(name)", link: "loom:agent/\(id)")
}

/// A file in `agent`'s folder or the shared one as a mention, its path as a reply would
/// name it, escaped where a Markdown link needs it, as desktop/src/model.rs's
/// `file_mention`.
func fileMention(agent: String, path: String, name: String) -> Mention {
    var escaped = ""
    for c in path {
        if " ()%[]<>\\".contains(c) {
            for byte in String(c).utf8 { escaped += String(format: "%%%02X", byte) }
        } else {
            escaped.append(c)
        }
    }
    return Mention(words: name, link: "loom:file/\(agent)/\(escaped)")
}

/// What the person typed with each mention the composer put in that's still there as
/// it was, as the Markdown link it's sent as, as desktop/src/model.rs's
/// `with_mention_links`. Each mention takes the first of its words not taken already.
func withMentionLinks(_ text: String, _ mentions: [Mention]) -> String {
    var taken: [(range: Range<String.Index>, mention: Mention)] = []
    for m in mentions where !m.words.isEmpty {
        var from = text.startIndex
        while let r = text.range(of: m.words, range: from..<text.endIndex) {
            if taken.contains(where: { $0.range.overlaps(r) }) {
                from = r.upperBound
                continue
            }
            taken.append((r, m))
            break
        }
    }
    var out = ""
    var at = text.startIndex
    for t in taken.sorted(by: { $0.range.lowerBound < $1.range.lowerBound }) {
        out += text[at..<t.range.lowerBound]
        let words = t.mention.words.replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: "")
        out += "[\(words)](\(t.mention.link))"
        at = t.range.upperBound
    }
    out += text[at...]
    return out
}

/// A reply's words with what it mentions as the person's messages have it, as
/// desktop/src/model.rs's `with_reply_mentions`: an agent reads a mention as `@Casper` or
/// `` `notes/hours.md` `` and writes it back that way, so `@` and the name of one of the
/// person's `agents` becomes that agent's link, and a path in code that the reply names
/// (`files`, in `agent`'s folder or the shared one) becomes the file's. The longest name
/// wins. Links and code blocks stay as they are.
func withReplyMentions(_ text: String, agents: [(id: String, name: String)], files: [NamedFile], agent: String) -> String {
    if !text.contains("@") && (files.isEmpty || !text.contains("`")) { return text }
    let names = agents.filter { !$0.name.isEmpty }.sorted { $0.name.count > $1.name.count }
    var lines: [String] = []
    var fenced = false
    for line in text.components(separatedBy: "\n") {
        let trimmed = line.drop(while: { $0 == " " || $0 == "\t" })
        let fence = trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~")
        if fence { fenced.toggle() }
        if fenced || fence {
            lines.append(line)
            continue
        }
        var out = ""
        var rest = Substring(line)
        var before: Character? = nil
        while let c = rest.first {
            // A path in code: the file's link when the reply named a file there.
            if c == "`" {
                let after = rest.dropFirst()
                guard let end = after.firstIndex(of: "`") else {
                    out += rest
                    break
                }
                let inside = String(after[after.startIndex..<end])
                if let f = files.first(where: { $0.path == inside }) {
                    let m = fileMention(agent: agent, path: f.path, name: f.name)
                    out += "[\(m.words.replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: ""))](\(m.link))"
                } else {
                    out += rest[rest.startIndex...end]
                }
                before = "`"
                rest = rest[rest.index(after: end)...]
                continue
            }
            // A link stays as it is.
            if c == "[", let close = rest.range(of: "]("), let end = rest[close.upperBound...].firstIndex(of: ")"),
               !rest[rest.startIndex..<close.lowerBound].contains("\n") {
                out += rest[rest.startIndex...end]
                before = ")"
                rest = rest[rest.index(after: end)...]
                continue
            }
            // @ and an agent's name, not in the middle of a word or an address.
            if c == "@", before.map({ !$0.isLetter && !$0.isNumber && $0 != "." && $0 != "/" }) ?? true {
                let after = rest.dropFirst()
                if let found = names.first(where: { n in
                    guard after.hasPrefix(n.name) else { return false }
                    let next = after.dropFirst(n.name.count).first
                    return next.map { !$0.isLetter && !$0.isNumber && $0 != "_" } ?? true
                }) {
                    let m = agentMention(id: found.id, name: found.name)
                    out += "[\(m.words.replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: ""))](\(m.link))"
                    before = found.name.last
                    rest = after.dropFirst(found.name.count)
                    continue
                }
            }
            out.append(c)
            before = c
            rest = rest.dropFirst()
        }
        lines.append(out)
    }
    return lines.joined(separator: "\n")
}

/// The @ being typed at the end of the composer's text, for the picker: how many
/// characters come before it, and what comes after it. Nil unless it starts the text or
/// follows a space, with no space since, as desktop/src/model.rs's `mention_query`.
func mentionQuery(_ text: String) -> (at: Int, query: String)? {
    guard let at = text.lastIndex(of: "@") else { return nil }
    let query = text[text.index(after: at)...]
    if query.contains(where: \.isWhitespace) || query.count > 40 { return nil }
    if let before = text[..<at].last, !(before.isWhitespace || before == "(") { return nil }
    return (text.distance(from: text.startIndex, to: at), String(query))
}

/// A file the composer's @ picker found (`agent.files`): its path as a reply would name
/// it, its name, and its folder, as the picker shows it.
struct FoundFile: Equatable, Sendable {
    var path: String
    var name: String
    var place: String
}

/// One of the @ picker's choices: one of the person's other agents, with the first line
/// of its instructions, or a file.
enum Pick: Equatable, Sendable {
    case agent(Agent, about: String)
    case file(FoundFile)
}

/// What a message's words need to show their links and mentions, as
/// desktop/src/model.rs's `Inline`.
struct Inline: Equatable, Sendable {
    /// Where the small picture before a link is, by its address: its site's icon, or a
    /// picture sent with the message, once it's fetched.
    var icons: [String: String] = [:]
    /// The agents it mentions that are still there, by id.
    var agents: [String: Agent] = [:]
}

struct Tool: Equatable, Sendable {
    var call: String
    var run: String
    var title: String
    var status: String?
    /// Its block in the reply, as tool.started says: where a reply still being written
    /// draws it among its words.
    var block: Int?
    /// The subagent call it sits inside, as tool.started says.
    var parent: String? = nil
}

/// A reply still being written: its run's tool calls among the blocks the deltas built,
/// as desktop/src/model.rs's `place_tools` does. A delta for a block past the end fills
/// the gap with empty blocks, so each call goes in its gap, at the block tool.started
/// named, and words written after it come after it. A call with no block, from an older
/// instance, or whose place has something else, goes on the end.
func placeTools(_ blocks: inout [Block], _ tools: [Tool]) {
    var have = Set(blocks.compactMap(\.call))
    for t in tools where have.insert(t.call).inserted {
        var b = Block.text("tool", "")
        b.call = t.call
        b.parent = t.parent
        guard let i = t.block, i >= 0 else {
            blocks.append(b)
            continue
        }
        while blocks.count <= i { blocks.append(.text("", "")) }
        let gap = blocks[i]
        if gap.call == nil && gap.file == nil && gap.card == nil && gap.text.isEmpty {
            blocks[i] = b
        } else {
            blocks.append(b)
        }
    }
}

/// One piece of a reply as it's drawn, its blocks named by their place in the message,
/// as desktop/src/model.rs's `Part`.
enum Part: Equatable, Sendable {
    /// Words, with nothing else between them.
    case words([Int])
    /// A run of tool calls.
    case tools([Int])
    /// Anything else on its own: a thought, a plan, a notice, a file or a card.
    case block(Int)
    /// Two or more pictures next to each other, drawn as one fan the viewer opens.
    case pictures([Int])
    /// The turn's steps, folded under one quiet line that opens to show them. While the
    /// turn runs (`working`), the line is the step it's on, and it's there even with no
    /// steps yet. Once it's done, it says how many.
    case fold(words: String, working: Bool, parts: [Part])
}

/// How a reply reads: one answer at the end (PLAN.md, "One answer at the end"), as
/// desktop/src/model.rs's `parts` lays it out with every step shown. Its last words are
/// the answer; what it said on the way, its thoughts and its tool calls are steps,
/// folded under one line, which while it works is the latest step. Files, plans, notices
/// and words that ask the person something stay in view. A turn cut short keeps all it
/// had in view, and so does a person's message.
func parts(_ m: Message, tools: [Tool]) -> [Part] {
    let n = m.blocks.count
    // A subagent's calls and words aren't the reply's: the phone leaves them out.
    let nested = insideACall(m)
    func said(_ i: Int) -> Bool {
        m.blocks[i].kind == "text" && !nested[i] && !m.blocks[i].text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
    func drawn(_ i: Int) -> Bool {
        let b = m.blocks[i]
        if nested[i] { return false }
        switch b.kind {
        case "text": return said(i)
        case "thought", "plan": return !b.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !b.plan.isEmpty
        case "tool", "notice": return true
        case "file": return b.file != nil
        case "card": return b.card != nil
        default: return false
        }
    }
    let all = (0..<n).filter(drawn)
    if m.role != "agent" || m.cutShort { return layOut(m, all) }
    let answer = m.live ? nil : (0..<n).reversed().first(where: said)
    // While the turn runs, words that ask only stay once more has come after them, so
    // words still coming don't show and then fold away.
    func stays(_ i: Int) -> Bool {
        switch m.blocks[i].kind {
        case "text": return i == answer || (asks(m.blocks[i].text) && (!m.live || i + 1 < n))
        case "thought", "tool": return false
        default: return true
        }
    }
    let kept = all.filter(stays)
    let steps = all.filter { !stays($0) }
    if m.live {
        return layOut(m, kept) + [.fold(words: workingWords(m, tools: tools), working: true, parts: layOut(m, steps))]
    }
    var out: [Part] = []
    if !steps.isEmpty {
        out.append(.fold(words: steps.count == 1 ? "1 step" : "\(steps.count) steps", working: false, parts: layOut(m, steps)))
    }
    return out + layOut(m, kept)
}

/// Words that ask the person something, which stay in view though they came mid-turn.
private func asks(_ text: String) -> Bool {
    text.trimmingCharacters(in: .whitespacesAndNewlines).hasSuffix("?")
}

/// The blocks `shown`, in order, with words next to each other as one group, tool calls
/// next to each other as one run, and two or more pictures next to each other as one
/// fan.
private func layOut(_ m: Message, _ shown: [Int]) -> [Part] {
    func picture(_ i: Int) -> Bool { m.blocks[i].file?.isImage == true }
    var out: [Part] = []
    for i in shown {
        if picture(i) {
            if case .pictures(let run)? = out.last {
                out[out.count - 1] = .pictures(run + [i])
                continue
            }
            if case .block(let j)? = out.last, picture(j) {
                out[out.count - 1] = .pictures([j, i])
                continue
            }
        }
        let kind = m.blocks[i].kind
        switch (kind, out.last) {
        case ("text", .words(let run)?):
            out[out.count - 1] = .words(run + [i])
        case ("tool", .tools(let run)?):
            out[out.count - 1] = .tools(run + [i])
        case ("text", _): out.append(.words([i]))
        case ("tool", _): out.append(.tools([i]))
        default: out.append(.block(i))
        }
    }
    return out
}

/// A person's message's files as they're drawn, above its words, as
/// desktop/src/model.rs's `file_parts`: each on its own, and two or more pictures as one
/// fan, wherever they were among the others.
func fileParts(_ m: Message) -> [Part] {
    let files = m.blocks.indices.filter { m.blocks[$0].file != nil }
    let pictures = files.filter { m.blocks[$0].file?.isImage == true }
    let others = files.filter { m.blocks[$0].file?.isImage != true }
    return layOut(m, pictures) + others.map { .block($0) }
}

/// Where a picture behind the first sits in a fan, at rest, as desktop/src/app.rs's
/// `FanCard` (PLAN.md, "Pictures"): its picture's place among them, how far its middle is
/// to the side of the first's, as a share of the card's width, its tilt in degrees and
/// its size. The phone has no pointer, so its fan never opens further.
struct FanCard: Equatable, Sendable {
    var picture: Int
    var x: CGFloat
    var tilt: CGFloat
    var scale: CGFloat
}

/// The cards behind the first in a fan of `n` pictures, the ones furthest back first, as
/// desktop/src/app.rs's `fan_cards` lays them out at rest:
/// - two: one behind, tilted to the right;
/// - three: one each side, apart already;
/// - four: tight, the fourth furthest back on the left, the side a reader starts from;
/// - five or more: tight, two each side. Only five are drawn.
///
/// The numbers are the sketch's, in points at the chat's 240-wide card.
func fanCards(_ n: Int) -> [FanCard] {
    func card(_ picture: Int, _ x: CGFloat, _ tilt: CGFloat, _ scale: CGFloat) -> FanCard {
        FanCard(picture: picture, x: x / 240, tilt: tilt, scale: scale)
    }
    switch n {
    case ..<2: return []
    case 2: return [card(1, 24, 6, 0.95)]
    case 3: return [card(1, -27, -7, 0.94), card(2, 27, 7, 0.94)]
    case 4: return [card(3, -20, -8, 0.92), card(1, -12, -4, 0.96), card(2, 12, 4, 0.96)]
    default: return [card(3, -20, -8, 0.92), card(4, 20, 8, 0.92), card(1, -12, -4, 0.96), card(2, 12, 4, 0.96)]
    }
}

/// How far a fan of `n` pictures reaches past its front card, with the tilted corners of
/// the ones behind, as shares of the card's width: to the left, to the right, and above
/// and below it. `aspect` is the card's height over its width. The fan keeps that room,
/// so nothing of it is cut off.
func fanReach(_ n: Int, aspect: CGFloat) -> (left: CGFloat, right: CGFloat, up: CGFloat) {
    var out: (left: CGFloat, right: CGFloat, up: CGFloat) = (0, 0, 0)
    for c in fanCards(n) {
        let t = abs(c.tilt) * .pi / 180
        let half = (c.scale * (cos(t) + aspect * sin(t)) - 1) / 2
        out.left = max(out.left, half - c.x)
        out.right = max(out.right, half + c.x)
        out.up = max(out.up, (c.scale * (sin(t) + aspect * cos(t)) - aspect) / 2)
    }
    return out
}

/// What a fan's corner says, as desktop/src/app.rs's `fan_count`: how many pictures there
/// are, all of them, however many it draws.
func fanCount(_ n: Int) -> String {
    "\(n)"
}

/// A person's words without the marks (`[name]`) of the pictures in a stack where they
/// only list them, as desktop/src/model.rs's `unmark_stacked`: two or more standing
/// together with nothing but spaces between, or one alone on its line. A mark among
/// other words is part of what was said, and stays.
func unmarkStacked(_ text: String, _ stacked: [FileRef]) -> String {
    var left = stacked.filter { !$0.name.isEmpty }.map { "[\($0.name)]" }
    if left.isEmpty { return text }
    var lines: [String] = []
    for line in text.split(separator: "\n", omittingEmptySubsequences: false).map(String.init) {
        // The stacked pictures' marks on this line, each picture's first, in order.
        var marks: [Range<String.Index>] = []
        var from = line.startIndex
        while let open = line[from...].firstIndex(of: "[") {
            if let k = left.firstIndex(where: { line[open...].hasPrefix($0) }) {
                let end = line.index(open, offsetBy: left.remove(at: k).count)
                marks.append(open..<end)
                from = end
            } else {
                from = line.index(after: open)
            }
        }
        // Marks with only spaces between them stand together.
        var runs: [(range: Range<String.Index>, count: Int)] = []
        for mark in marks {
            if let last = runs.last, line[last.range.upperBound..<mark.lowerBound].allSatisfy(\.isWhitespace) {
                runs[runs.count - 1] = (last.range.lowerBound..<mark.upperBound, last.count + 1)
            } else {
                runs.append((mark, 1))
            }
        }
        var out = ""
        var at = line.startIndex
        var cut = false
        for run in runs {
            let alone = line[..<run.range.lowerBound].allSatisfy(\.isWhitespace) && line[run.range.upperBound...].allSatisfy(\.isWhitespace)
            if run.count >= 2 || alone {
                out += line[at..<run.range.lowerBound]
                at = line[run.range.upperBound...].firstIndex { !$0.isWhitespace } ?? line.endIndex
                cut = true
            }
        }
        if !cut {
            lines.append(line)
            continue
        }
        out += line[at...]
        // A line that was only marks goes with them.
        let kept = out.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        if !kept.isEmpty { lines.append(kept) }
    }
    return lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
}

/// The step a running turn is on, in a line: the first line of what it last said, the
/// tool it last called, or that it's thinking.
private func workingWords(_ m: Message, tools: [Tool]) -> String {
    // While a subagent works, the line is its call, not what it says or does inside.
    let nested = insideACall(m)
    for (i, b) in m.blocks.enumerated().reversed() where !nested[i] {
        switch b.kind {
        case "text":
            if let line = b.text.split(separator: "\n").lazy.map({ $0.trimmingCharacters(in: .whitespaces) }).first(where: { !$0.isEmpty }) {
                return line.count > 160 ? "\(line.prefix(160))…" : line
            }
        case "tool":
            let title = tools.first { $0.call == b.call }?.title ?? ""
            return "\(title.isEmpty ? "Using a tool" : title)…"
        case "thought":
            return "Thinking…"
        default:
            break
        }
    }
    return "Thinking…"
}

/// What an approval's card shows of the call, as the core shaped it (core/asks.calx):
/// what kind of call it is, its arguments as labelled lines, how many more there were,
/// the command or script it runs, and everything for Details.
struct Shown: Equatable, Sendable {
    struct Line: Equatable, Sendable {
        var label: String
        var value: String
    }

    /// One tool a call runs: what it does, the service it reaches, a few of its arguments
    /// as chips, and how many more it was given.
    struct Step: Equatable, Sendable {
        var title: String
        var badge: Badge?
        var chips: [Line]
        var more: Int
    }

    /// `script`, `mcp`, `plugin`, `device`, `install`, or empty.
    var what = ""
    var lines: [Line] = []
    var more = 0
    var code = ""
    var raw = ""
    /// What it does in plain words, and where it goes, small ("acta · search").
    var title = ""
    var place = ""
    /// The services it reaches, at most two.
    var badges: [Badge] = []
    /// What loom knows it does (`reads`, `changes`, `deletes`, `runs`) and how the card
    /// says it, or both empty.
    var effect = ""
    var effectWords = ""
    var steps: [Step] = []
    var stepsMore = 0
    /// Why the agent makes the call, in its own words.
    var reason = ""

    /// From the core's `shown`, or, for a question kept before the core shaped them, its
    /// `detail` as it came, for Details.
    init(_ a: JSON) {
        let s = a["shown"]
        guard !s.isNull else {
            switch a["detail"] {
            case .string(let d): raw = d
            case .null: raw = ""
            case let other: raw = other.pretty
            }
            title = a["title"].text
            return
        }
        func lines(_ j: JSON) -> [Line] { j.array.map { Line(label: $0["label"].text, value: $0["value"].text) } }
        what = s["what"].text
        self.lines = lines(s["lines"])
        more = s["more"].int ?? 0
        code = s["code"].text
        raw = s["raw"].text
        title = s["title"].text.isEmpty ? a["title"].text : s["title"].text
        place = s["place"].text
        badges = s["badges"].array.compactMap(Badge.init)
        effect = s["effect"]["kind"].text
        effectWords = s["effect"]["words"].text
        steps = s["steps"].array.map { Step(title: $0["title"].text, badge: Badge($0["badge"]), chips: lines($0["chips"]), more: $0["more"].int ?? 0) }
        stepsMore = s["steps_more"].int ?? 0
        reason = s["reason"].text
    }
}

/// A service's badge on a card (`shared/badge.calx`): its initial on one of the thread
/// colours, by name, or an MCP server's own icon, a file's SHA-256, with the one it gave
/// for a dark background when it gave one.
struct Badge: Equatable, Sendable {
    var name: String
    var initial: String
    var color: String
    var icon: String
    var iconDark: String

    init?(_ j: JSON) {
        name = j["name"].text
        guard !name.isEmpty else { return nil }
        initial = j["initial"].text
        color = j["color"].text
        icon = j["icon"].text
        iconDark = j["icon_dark"].text
    }

    /// The icon it's drawn with in the theme, or "" for its initial.
    func icon(dark: Bool) -> String {
        dark && !iconDark.isEmpty ? iconDark : icon
    }

    /// Every icon it has, for fetching.
    var icons: [String] { [icon, iconDark].filter { !$0.isEmpty } }
}

struct Approval: Identifiable, Equatable, Sendable {
    var call: String
    var run: String
    /// The kind of tool it is, as `tool.started` says: `execute`, `delete`, `other`…
    var kind: String
    var title: String
    var shown: Shown
    var canAlways: Bool
    /// Loom counts it risky: Allow is held, not tapped (`holdTime`).
    var risky = false
    /// How the person answered it here, while the instance hasn't said yet.
    var answered: Answer?
    var id: String { call }

    struct Answer: Equatable, Sendable {
        var allow: Bool
        var always: Bool
    }
}

struct Run: Equatable, Sendable {
    var id: String
    var conversation: String
    var agent: String
    var message: String
    /// A routine's quiet run, or a step of its flow: the chat shows only what its agent
    /// posts, never the run itself.
    var quiet = false
}

/// A routine's prompt that an instance kept in the chat before routines ran quietly
/// (PLAN.md, "Routines"): stored, but not shown.
/// Now, in Unix ms, as the instance's times are.
func nowMs() -> Int { Int(Date.now.timeIntervalSince1970 * 1000) }

func routinePrompt(_ m: JSON) -> Bool {
    m["role"].text == "person" && m["from"].text.hasPrefix("rt_")
}

struct Device: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var kind: String
    /// When it was last connected (Unix ms), or 0 for never or not known.
    var seen: Int = 0
    var online = false
}

/// A device asking to be added to the account (protocol.md, "Adding a device"), with the
/// four digits it shows too.
struct LinkAsk: Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var kind: String
    var check: String
    var expires: Int
}

/// One of the person's skills, as the + menu lists it.
struct PlusSkill: Equatable, Sendable {
    var name: String
    var description: String
}

/// One of the person's routines, as the + menu lists it: how its last run went
/// (`done`, `quiet`, `failed`, ..., or empty before the first) and when.
struct PlusRoutine: Equatable, Sendable {
    var id: String
    var name: String
    var agent: String
    var status: String
    var at: Int

    /// How its last run went, in words, and whether it went wrong: "Ran 14:05 ✓".
    var went: (String, Bool) {
        let when = When.time(at)
        switch status {
        case "": return ("Not run yet", false)
        case "running": return ("Running now", false)
        case "waiting": return ("Waiting for its chat", false)
        case "done", "quiet": return ("Ran \(when) ✓", false)
        case "skipped": return ("Skipped \(when)", false)
        default: return ("Failed \(when)", true)
        }
    }
}

/// A file sent or received lately in one of the person's chats (`files.recent`).
struct RecentFile: Equatable, Sendable {
    var file: FileRef
    var at: Int
}

/// What a row of the + menu opens.
enum PlusSub: Sendable {
    case recent, skills, routines
}

/// One row of the composer's + menu.
enum PlusRow: Equatable, Sendable {
    case attach
    case open(PlusSub)
    case file(RecentFile)
    case skill(PlusSkill)
    case routine(PlusRoutine)
}

/// A skill picked from the + menu as a mention: its name, going as `loom:skill/<name>`.
func skillMention(_ name: String) -> Mention {
    Mention(words: name, link: "loom:skill/\(name)")
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
    /// What it replies to, as the core worked it out.
    var reply: ReplyTo?
    /// The id it gets once it goes, for one the instance keeps in the conversation's
    /// queue while the agent answers (`ack` with `queued`).
    var message: String?

    init(_ q: JSON) {
        cid = q["cid"].text
        agent = q["agent"].string
        conversation = q["conversation"].string
        text = q["text"].text
        files = q["files"].array.count
        reply = ReplyTo.of(q)
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
    /// How the agent list groups them, as the core says.
    var sidebar: [SideSection] = []
    /// An agent to put in the folder asked for last, once it's made.
    private var folderFor: String?
    var conversations: [Conversation] = []
    var runs: [Run] = []
    var tools: [Tool] = []
    var approvals: [Approval] = []
    /// Answers given here, by call, until the instance takes them.
    private var answering: [String: Approval.Answer] = [:]
    var devices: [Device] = []
    /// The code that adds a device, while Add a device shows one: the QR code's text, the
    /// code to paste on a computer, and when it expires (Unix ms).
    var addCode: (qr: String, text: String, expires: Int)?
    /// The name of the device that joined with it, once one has.
    var joined: String?
    /// Devices asking to be added, until one of the person's devices answers.
    var asks: [LinkAsk] = []
    /// This phone used a code, and waits for a yes on the device that showed it: the
    /// check both show.
    var adding: String?
    /// Whether the account has a password, as welcome says it, and why the last one
    /// wasn't set.
    var hasPassword: Bool?
    var passwordError: String?
    /// Each agent program's name as people know it ("Claude Agent" for `claude-acp`), by
    /// its name, as `acp.list` gives it.
    var programTitles: [String: String] = [:]
    private var programsWanted = false
    /// The selected agent's messages, oldest first, with the queued ones last.
    var messages: [Message] = []
    /// Whether the selected chat's messages have come since it was selected.
    var messagesCame = false
    var moreMessages = false
    /// How many more of the chat's messages it asks for, to reach one a quote goes to.
    private var messagesMore = 0
    /// Asked for more of them, which haven't come yet.
    private(set) var reaching = false
    /// The message the next message replies to, while it's written.
    var replying: ReplyTo?
    /// The message a quote was tapped to go to, until the chat is there.
    var jumpTo: String?
    var selected: String?
    /// Another agent's thread with the selected agent, while it's shown in its chat's place.
    private(set) var thread: String?
    /// This device's id and the account's, from `welcome`.
    var me: (user: String, device: String)?
    /// Something to tell the person, once.
    var notice: String?
    /// Files for the next message.
    var attachments: [Attachment] = []
    /// Files whose piece in the composer was taken out, kept until the message goes in
    /// case an undo puts the piece back (`syncAttachments`).
    var dropped: [Attachment] = []
    /// The links pasted in the composer, by address, as they're looked up.
    var linkPreviews: [String: Preview] = [:]
    /// An agent whose chat a tap on its mention asked for, for the list to open.
    var openAgent: String?
    /// What the composer's + menu lists: the person's skills, their routines, and the
    /// files sent or received lately in their chats, newest first.
    var skills: [PlusSkill] = []
    var routines: [PlusRoutine] = []
    var recentFiles: [RecentFile] = []
    /// Files the core has fetched, by SHA-256, or SHA-256 and ".thumb": where they are.
    var files: [String: String] = [:]
    /// A file fetched to be opened, for the screen to show.
    var toOpen: URL?
    /// A stack of pictures shown big, to page through.
    var viewing: Viewing?
    /// The pictures a reply names that the viewer waits for the instance to keep as the
    /// person's (`viewNamed`): the ask's id, and each one's file once it's answered, or
    /// nil inside for one that couldn't be kept.
    var mentionStack: (id: String, files: [FileRef??])?
    /// The person's secrets, for the Secrets page, read again when they change.
    var secrets: [SecretItem] = []
    private var secretsStale = false
    /// Secret cards whose answer went and hasn't come back, and what the instance said to
    /// one it refused, by call.
    private var secretSending: Set<String> = []
    private var secretErrors: [String: String] = [:]
    /// What's picked on the choices and toggles of forms not sent yet, by
    /// `message/card/field`; the forms whose answers went and haven't come back, by their
    /// send's cid, with their `message/card`; and what the instance said to ones it
    /// refused, by `message/card`.
    private var formPicks: [String: JSON] = [:]
    private var formSending: [String: String] = [:]
    private var formErrors: [String: String] = [:]
    /// When an agent's reply last grew, and when its last run finished or failed.
    var lastDelta: [String: Date] = [:]
    var finished: [String: Date] = [:]
    var failed: [String: Date] = [:]
    /// The person's MCP servers they're signed in to, from `mcp.list`, as whom (empty
    /// when the sign-in didn't say).
    var mcpSignedIn: [String: String] = [:]
    /// MCP servers whose sign-in page is open here, and why the last sign-in to one
    /// here didn't finish.
    var mcpSigning: Set<String> = []
    var mcpSignInErrors: [String: String] = [:]
    /// A sign-in page the instance gave, for the session to open: the server and its
    /// address.
    var toSignIn: (server: String, url: URL)?
    /// Whether `mcp.list` is to be asked again.
    private var mcpStale = false
    /// The files the @ picker found: for which agent, for what, and the files.
    private var foundFiles: (agent: String, query: String, files: [FoundFile]) = ("", "", [])
    /// The `agent.files` asked for and not answered yet: its id, the agent and the query.
    private var filesAsking: (id: String, agent: String, query: String)?
    /// Frames the model has to send that no screen asked for, such as fetching a file a
    /// message mentions once the instance has it ready: sent with the next fetches.
    private var pending: [JSON] = []

    /// Replies being written, by message id: their blocks so far.
    private var live: [String: [Block]] = [:]
    private var queued: [Queued] = []
    private var dirty: Set<CoreList> = []
    private var getting: [String: (key: String, open: Bool)] = [:]
    private var nextId = 0

    init() {
        dirty = [.agents, .conversations, .runs, .tools, .approvals, .outbox, .devices, .sidebar]
    }

    private func id(_ prefix: String) -> String {
        nextId += 1
        return "\(prefix)_\(nextId)"
    }

    // MARK: What the screens read

    func agent(_ id: String) -> Agent? { agents.first { $0.id == id } }

    /// The conversation an agent's chat shows: its newest, never another agent's thread
    /// with it, however new that is.
    func conversation(of agent: String?) -> String? {
        guard let agent else { return nil }
        return conversations.first { $0.agent == agent && $0.isChat }?.id
    }

    /// The selected agent's chat, or another agent's thread with it while that's open.
    var conversation: String? { thread ?? conversation(of: selected) }

    /// The threads other agents (and plugins' panels) keep with `agent`, newest first.
    func asks(_ agent: String) -> [Ask] {
        conversations.filter { $0.agent == agent && $0.askedBy != nil }.map { c in
            let by = c.askedBy.flatMap { self.agent($0) }
            return Ask(id: c.id, by: by, label: by.map { "From \($0.name)" } ?? c.title)
        }
    }

    /// Shows another agent's thread with the selected agent in its chat's place, or
    /// with nil, its chat again.
    func open(thread: String?) {
        guard self.thread != thread else { return }
        self.thread = thread
        messages = []
        messagesCame = false
        dirty.insert(.messages)
    }

    /// The selected chat's messages, with the replies still being written at the end.
    var shown: [Message] {
        var out = messages
        let conversation = self.conversation
        // A routine's quiet run shows only what its agent posts, which comes as messages.
        for r in runs where !r.quiet && r.conversation == conversation {
            if out.contains(where: { $0.id == r.message }) { continue }
            var blocks = live[r.message] ?? []
            placeTools(&blocks, tools.filter { $0.run == r.id })
            out.append(Message(id: r.message, role: "agent", at: .max, blocks: blocks, live: true))
        }
        // A choices card says which answer the person sent, as the message after it, and
        // a card goes quiet once the chat has gone on past its reply, as
        // desktop/src/model.rs's `shown` does.
        for i in out.indices {
            let next = i + 1 < out.count ? out[i + 1] : nil
            let sent = next.flatMap { $0.isPerson ? $0.blocks.filter { $0.kind == "text" }.map { $0.text.trimmingCharacters(in: .whitespacesAndNewlines) }.joined(separator: "\n") : nil }
            for j in out[i].blocks.indices {
                guard var c = out[i].blocks[j].card else { continue }
                if c.type == "choices" { c.chosen = sent.flatMap { t in c.options.firstIndex { $0.trimmingCharacters(in: .whitespaces) == t } } }
                if c.signsIn {
                    c.signedAs = mcpSignedIn[c.server]
                    c.signing = mcpSigning.contains(c.server)
                    c.signInError = mcpSignInErrors[c.server]
                }
                if c.type == "secret_ask" {
                    c.sending = secretSending.contains(c.call)
                    c.error = secretErrors[c.call] ?? ""
                }
                if c.type == "form" {
                    let key = "\(out[i].id)/\(c.id)"
                    c.message = out[i].id
                    for k in c.picks.indices { if let p = formPicks["\(key)/\(k)"] { c.picks[k] = p } }
                    c.sending = formSending.values.contains(key)
                    c.error = formErrors[key] ?? ""
                }
                c.passed = next != nil
                c.live = out[i].live
                out[i].blocks[j].card = c
            }
        }
        return out
    }

    /// The questions waiting in the selected chat, each with the answer given here while
    /// the instance hasn't taken it yet.
    var shownApprovals: [Approval] {
        let conversation = self.conversation
        return approvals.filter { a in
            runs.contains { $0.id == a.run && $0.conversation == conversation }
        }.map { a in
            var a = a
            a.answered = answering[a.call]
            return a
        }
    }

    func tool(_ call: String?) -> Tool? {
        guard let call else { return nil }
        return tools.first { $0.call == call }
    }

    /// What an agent's thread shows at `now`.
    func mood(_ agent: String, at now: Date = .now) -> Mood {
        let all = runs.filter { $0.agent == agent }
        // A question waits for the person whoever's run asked, a routine's quiet one too.
        if all.contains(where: { r in approvals.contains { $0.run == r.id } }) { return .waiting }
        // Otherwise a routine's quiet run isn't shown as working.
        let mine = all.filter { !$0.quiet }
        if mine.contains(where: { r in tools.contains { $0.run == r.id && $0.status == nil } }) { return .tool }
        if !mine.isEmpty {
            if let at = lastDelta[agent], now.timeIntervalSince(at) < 0.7 { return .writing }
            return .thinking
        }
        if let at = failed[agent], now.timeIntervalSince(at) < 600 { return .failed }
        return .idle
    }

    /// An agent program's name as people know it: the instance's title for it, or its
    /// own with a capital, as `claude` is Claude.
    func programTitle(_ program: String) -> String {
        programTitles[program] ?? program.prefix(1).uppercased() + program.dropFirst()
    }

    /// What the agents list says under an agent's name.
    func preview(_ agent: Agent) -> String {
        // A question from any of its runs, a routine's quiet one too.
        if let a = approvals.first(where: { a in runs.contains { $0.agent == agent.id && $0.id == a.run } }) {
            return "Wants to \(a.title.prefix(1).lowercased())\(a.title.dropFirst())"
        }
        guard let r = runs.first(where: { $0.agent == agent.id && !$0.quiet }) else {
            return mood(agent.id) == .failed ? "Couldn't finish" : programTitle(agent.program)
        }
        if let t = tools.last(where: { $0.run == r.id && $0.status == nil }) {
            return t.title
        }
        return "Thinking"
    }

    /// Whether the selected chat's agent is answering, so the button stops it. A
    /// routine's quiet run isn't shown, so it isn't stopped from here.
    var busy: Bool {
        let conversation = self.conversation
        return runs.contains { !$0.quiet && $0.conversation == conversation }
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
        thread = nil
        messages = []
        messagesCame = false
        messagesMore = 0
        replying = nil
        jumpTo = nil
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
            if f["waiting"].bool == true {
                // The code was used, and waits for a yes on the device that showed it.
                adding = f["check"].text
                loginError = nil
            } else {
                adding = nil
                loginError = f["error"].isNull ? nil : f["error"]["message"].string ?? f["error"]["code"].text
            }
        case "welcome":
            me = (f["user"].text, f["device"].text)
            hasPassword = f["has_password"].bool
        case "synced":
            synced = true
            dirty.formUnion(CoreList.allCases)
            programsWanted = true
            // The core tells what catching up brought here, not an event at a time
            // (core/README.md): the text of replies that ended meanwhile goes, and an
            // agent whose last run failed shows it.
            let going = Set(f["runs"].array.map { $0["message"].text })
            live = live.filter { going.contains($0.key) }
            for e in f["held"]["ended"].array {
                failed[e["agent"].text] = e["status"].text == "failed" ? .now : nil
            }
        case "core.data":
            data(f)
        case "core.filled":
            // Older messages of a conversation came: the shown one is read again.
            if f["conversation"].text == conversation { dirty.insert(.messages) }
        case "core.queued":
            // A form's answers wait on their card, not as a message in the queue.
            if formSending[f["cid"].text] != nil { break }
            queued.append(Queued(f))
            placeQueued()
        case "core.file":
            fileCame(f)
        case "ack":
            // One that waits in the instance's queue stays shown until its message is stored.
            let cid = f["cid"].text
            formSending[cid] = nil
            if f["queued"].bool == true, let i = queued.firstIndex(where: { $0.cid == cid }) {
                queued[i].message = f["message"].string
            } else {
                queued.removeAll { $0.cid == cid }
            }
            dirty.insert(.messages)
        case "delta":
            delta(f)
        case "card":
            // A card in a reply still being written, new or changed: it takes its
            // block's place whole.
            let message = f["message"].text
            let block = f["block"].int ?? 0
            guard let card = Card(f["card"]), block >= 0 else { break }
            // Whether the person signed in since, for a card that signs in.
            if card.signsIn { mcpStale = true }
            // A secret card that's answered waits on nothing, and the secrets are read
            // again once one is kept.
            if card.type == "secret_ask" && card.state != "asking" {
                secretSending.remove(card.call)
                secretErrors[card.call] = nil
                if !card.saved.isEmpty { secretsStale = true }
            }
            var blocks = live.removeValue(forKey: message) ?? []
            while blocks.count <= block { blocks.append(.text("", "")) }
            blocks[block] = Block(kind: "card", text: "", call: nil, file: nil, card: card)
            live[message] = blocks
        case "event":
            event(f)
        case "core.moved":
            // The instance moved, and the core followed it there (protocol.md, "moved").
            notice = "Your instance moved to \(f["address"].text). The app is connected there now."
        case "result":
            result(f)
        case "error":
            // A form's answers the instance refused: its card says why.
            if let key = formSending.removeValue(forKey: f["ref"].text) {
                formErrors[key] = f["message"].text
                dirty.insert(.messages)
                break
            }
            // A move the core won't follow (`moved_unsigned`, `moved_elsewhere`) says why
            // here too, and the app stays where it is.
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
        case "group":
            dirty.insert(.sidebar)
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
                    } else if !r.quiet {
                        // A routine's quiet run never showed working, so it doesn't snap.
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
            if kind == "device.added", addCode != nil {
                addCode = nil
                joined = f["device"]["name"].text
            }
        case "link":
            if kind == "link.asked" {
                let a = f["ask"]
                let ask = LinkAsk(id: a["id"].text, name: a["name"].text, kind: a["kind"].text, check: a["check"].text, expires: a["expires"].int ?? 0)
                if ask.expires > nowMs(), !asks.contains(where: { $0.id == ask.id }) { asks.append(ask) }
            } else if kind == "link.answered" {
                let id = f["ask"].text
                asks.removeAll { $0.id == id }
            }
        case "account":
            if let has = f["has_password"].bool { hasPassword = has }
        default:
            break
        }
    }

    private func delta(_ f: JSON) {
        let message = f["message"].text
        let block = f["block"].int ?? 0
        // Taken out while it changes, so appending to its text doesn't copy it first.
        var blocks = live.removeValue(forKey: message) ?? []
        while blocks.count <= block {
            blocks.append(.text(f["kind"].text, ""))
        }
        // The core sends only what adds to the text, so the piece goes on the end. `pos`
        // counts UTF-8 bytes.
        let pos = f["pos"].int ?? 0
        let have = blocks[block].text.utf8.count
        // A subagent's words, which aren't the reply's.
        if let parent = f["parent"].string { blocks[block].parent = parent }
        if pos <= have {
            if pos < have {
                blocks[block].text = String(decoding: Array(blocks[block].text.utf8.prefix(pos)), as: UTF8.self)
            }
            blocks[block].text += f["text"].text
        }
        live[message] = blocks
        // Written a few times a second at most, not at each delta: every view that shows
        // the agent's thread reads it, and each write draws them all again.
        if let r = runs.first(where: { $0.message == message }), !r.quiet {
            let now = Date.now
            if lastDelta[r.agent].map({ now.timeIntervalSince($0) > 0.25 }) ?? true {
                lastDelta[r.agent] = now
            }
        }
    }

    private func result(_ f: JSON) {
        if mcpResult(f) { return }
        if f["id"].text == "app.acp" {
            // Quietly: without the titles, programs show by their own names.
            var titles: [String: String] = [:]
            for p in f["value"]["programs"].array {
                if let title = p["title"].string { titles[p["name"].text] = title }
            }
            programTitles = titles
            return
        }
        let error = f["error"].isNull ? nil : f["error"]["message"].string
        switch f["id"].text {
        case let id where id.hasPrefix("app.preview:"):
            // A link pasted in the composer, looked up: its preview, or its site when
            // the lookup failed.
            guard let hash = id.firstIndex(of: "#") else { break }
            let url = String(id[id.index(after: hash)...])
            linkPreviews[url] = error == nil && !f["value"]["link"].isNull ? .ready(PageLink.of(f["value"]["link"])) : .failed
        case let id where id.hasPrefix("app.folder_new"):
            // A folder made for an agent: the agent goes in.
            let agent = folderFor
            folderFor = nil
            if let error {
                notice = error
            } else if let agent, let group = f["value"]["group"]["id"].string {
                pending.append(groupAgent(agent, group))
            }
        case let id where id.hasPrefix("app.agent_files"):
            // The files the @ picker asked for, unless it has asked for others since.
            guard let asking = filesAsking, asking.id == id else { break }
            filesAsking = nil
            let files = f["value"]["files"].array.map { FoundFile(path: $0["path"].text, name: $0["name"].text, place: $0["where"].text) }
            foundFiles = (asking.agent, asking.query, files)
        case let id where id.hasPrefix("app.plus_skills"):
            skills = f["value"]["skills"].array.map { PlusSkill(name: $0["name"].text, description: $0["description"].text) }
        case let id where id.hasPrefix("app.plus_routines"):
            routines = f["value"]["routines"].array.map {
                PlusRoutine(id: $0["id"].text, name: $0["name"].text, agent: $0["agent"].text, status: $0["last"]["status"].text, at: $0["last"]["at"].int ?? 0)
            }
        case let id where id.hasPrefix("app.recent_files"):
            recentFiles = f["value"]["files"].array.map { RecentFile(file: FileRef($0), at: $0["at"].int ?? 0) }
        case let id where id.hasPrefix("app.agent_file.pictures"):
            // One of the pictures a reply names, for the viewer: it opens on them once
            // they're all kept, without those that couldn't be.
            guard var asked = mentionStack, let hash = id.lastIndex(of: "#"), String(id[..<hash]) == asked.id,
                  let k = Int(id[id.index(after: hash)...]), k < asked.files.count else { break }
            asked.files[k] = .some(f["value"]["file"].isNull ? nil : FileRef(f["value"]["file"]))
            mentionStack = asked
            if asked.files.allSatisfy({ $0 != nil }) {
                mentionStack = nil
                view(asked.files.compactMap { $0 ?? nil })
            }
        case let id where id.hasPrefix("app.agent_file"):
            // A file a message mentions, kept as the person's: a picture opens big, as the
            // chat's do, and anything else once it's here.
            if let error {
                notice = error
            } else if !f["value"]["file"].isNull {
                let file = FileRef(f["value"]["file"])
                if file.isImage {
                    view([file])
                } else if let frame = open(file) {
                    pending.append(frame)
                }
            }
        case let id where id.hasPrefix("app.secret_fill:"):
            // The card's own frame says how it went; a refusal shows on the card, which
            // still asks.
            if let error {
                let call = String(id.dropFirst("app.secret_fill:".count))
                secretSending.remove(call)
                secretErrors[call] = error
            }
        case let id where id.hasPrefix("app.secrets"):
            if let error {
                notice = error
            } else {
                secrets = f["value"]["secrets"].array.map { SecretItem(name: $0["name"].text, sites: $0["sites"].array.map(\.text)) }
            }
        case let id where id.hasPrefix("app.secret_"):
            if let error { notice = error }
            secretsStale = true
        case let id where id.hasPrefix("app.add_code"):
            if let error {
                notice = error
            } else {
                let v = f["value"]
                addCode = (v["qr"].text, v["text"].text, v["expires"].int ?? 0)
                joined = nil
            }
        case let id where id.hasPrefix("app.device_list"):
            if let error {
                notice = error
            } else {
                devices = f["value"]["devices"].array.map {
                    Device(id: $0["id"].text, name: $0["name"].text, kind: $0["kind"].text, seen: $0["seen"].int ?? 0, online: $0["online"].bool ?? false)
                }
            }
        case let id where id.hasPrefix("app.password_set"):
            if let error {
                passwordError = error
            } else {
                passwordError = nil
                hasPassword = true
            }
        default:
            if let error { notice = error }
        }
    }

    private func data(_ f: JSON) {
        guard let what = CoreList(rawValue: f["what"].text) else { return }
        let items = f["items"].array
        switch what {
        case .sidebar:
            sidebar = items.compactMap(SideSection.init)
        case .agents:
            agents = listed(items.map {
                Agent(id: $0["id"].text, name: $0["name"].text, color: $0["color"].text, program: $0["acp_agent"].text, shape: $0["shape"].text, drawing: points($0["drawing"]), pinned: $0["pinned"].int, order: $0["order"].int, group: $0["group"].string, hidden: !$0["hidden"].isNull, instructions: $0["instructions"].text)
            })
            if let s = selected, agent(s) == nil { select(nil) }
        case .conversations:
            let before = conversation
            conversations = items.map(Conversation.init)
            if conversation != before { dirty.insert(.messages) }
        case .runs:
            runs = items.map {
                Run(id: $0["run"].text, conversation: $0["conversation"].text, agent: $0["agent"].text, message: $0["message"].text, quiet: !$0["routine"].text.isEmpty)
            }
        case .tools:
            tools = items.map {
                Tool(call: $0["call"].text, run: $0["run"].text, title: $0["title"].text, status: $0["status"].string, block: $0["block"].int, parent: $0["parent"].string)
            }
        case .approvals:
            approvals = items.map { a in
                Approval(call: a["call"].text, run: a["run"].text, kind: a["tool"].text, title: a["title"].text, shown: Shown(a), canAlways: a["can_always"].bool ?? false, risky: a["risky"].bool ?? false)
            }
            // An answer given here is for a question that's still open.
            let open = Set(approvals.map(\.call))
            answering = answering.filter { open.contains($0.key) }
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
            messagesCame = true
            // Loom's own lines across the chat (a compaction, a fresh session) are the desktop's.
            messages = items.filter { !routinePrompt($0) && $0["role"].text != "loom" }.map { m in
                Message(id: m["id"].text, role: m["role"].text, at: m["at"].int ?? 0, blocks: m["blocks"].array.map(Block.of), from: from(m["from"].text), fromAgent: m["from"].text.hasPrefix("a_") ? m["from"].text : nil, status: m["status"].text, sources: m["sources"].array.map(Source.of).filter { !$0.url.isEmpty }, links: PageLink.all(m["links"]), named: m["mentions"].array.map { NamedFile(path: $0["path"].text, name: $0["name"].text, size: $0["size"].int ?? 0, thumb: $0["thumb"].string) }.filter { !$0.path.isEmpty }, reply: ReplyTo.of(m))
            }
            reaching = false
            placeQueued()
            // The tool calls these messages name.
            dirty.insert(.tools)
            // A sign-in card for a server not known to be signed in: whether it is since,
            // as on another device.
            if messages.flatMap(\.blocks).contains(where: { b in b.card.map { $0.signsIn && mcpSignedIn[$0.server] == nil } ?? false }) {
                mcpStale = true
            }
        }
    }

    /// Takes the answer to one of the MCP commands the model sent, if it is one: the
    /// servers' sign-ins, a sign-in page to open, or a sign-in finished. Its error goes on
    /// the sign-in card, not in a notice.
    private func mcpResult(_ f: JSON) -> Bool {
        let id = f["id"].text
        let error = f["error"].isNull ? nil : f["error"]["message"].string ?? f["error"]["code"].text
        if id.hasPrefix("app.mcp_list") {
            if error == nil {
                var signed: [String: String] = [:]
                for s in f["value"]["servers"].array where s["sign_in"].text == "signed_in" {
                    signed[s["name"].text] = s["signed_in_as"].text
                }
                mcpSignedIn = signed
            }
            return true
        }
        func after(_ prefix: String) -> String? {
            id.hasPrefix(prefix) ? String(id.dropFirst(prefix.count)) : nil
        }
        if let server = after("app.mcp_sign_in:") {
            if let error {
                signInEnded(server, error: error)
            } else if let url = URL(string: f["value"]["url"].text), ["https", "http"].contains(url.scheme?.lowercased()) {
                toSignIn = (server, url)
            } else {
                signInEnded(server, error: "The instance gave no page to sign in on.")
            }
            return true
        }
        if let server = after("app.mcp_signed_in:") {
            mcpSigning.remove(server)
            if let error {
                mcpSignInErrors[server] = error
            } else {
                mcpSignInErrors[server] = nil
                let name = f["value"]["server"].string ?? server
                mcpSignedIn[name] = f["value"]["signed_in_as"].text
            }
            mcpStale = true
            return true
        }
        return false
    }

    /// Who sent a person message other than the person: another agent. (A routine's
    /// prompt isn't shown.)
    private func from(_ id: String) -> String? {
        id.hasPrefix("a_") ? agent(id)?.name ?? "Another agent" : nil
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
            messages.append(Message(id: q.cid, role: "person", at: .max, blocks: [.text("text", text)], queued: true, reply: q.reply))
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
        var out: [JSON] = pending
        pending = []
        var lists = dirty
        dirty = []
        // The sidebar groups the agents by what they are, and who's waiting by the
        // questions open in their runs.
        if !lists.isDisjoint(with: [.agents, .runs, .approvals]) { lists.insert(.sidebar) }
        if mcpStale {
            mcpStale = false
            out.append(["t": "cmd", "id": JSON(id("app.mcp_list")), "name": "mcp.list"])
        }
        if programsWanted {
            programsWanted = false
            out.append(["t": "cmd", "id": "app.acp", "name": "acp.list"])
        }
        for l in CoreList.allCases where lists.contains(l) {
            if l == .messages {
                guard let conversation else {
                    // No chat yet: there's nothing to come.
                    messages = []
                    messagesCame = selected != nil
                    placeQueued()
                    continue
                }
                out.append(["t": "core.get", "id": "app.messages", "what": "messages", "conversation": JSON(conversation), "limit": JSON(200 + messagesMore)])
            } else if l == .tools {
                // Only the calls the shown chat's blocks name, and the runs going on: a
                // heavy user has tens of thousands of calls, and this comes at each one.
                let calls = messages.flatMap(\.blocks).compactMap(\.call)
                out.append(["t": "core.get", "id": "app.tools", "what": "tools", "calls": JSON(calls), "runs": JSON(runs.map(\.id))])
            } else if l == .devices && synced {
                // From the instance, with when each was last connected.
                out.append(["t": "cmd", "id": JSON(id("app.device_list")), "name": "device.list"])
            } else {
                out.append(["t": "core.get", "id": JSON("app.\(l.rawValue)"), "what": JSON(l.rawValue)])
            }
        }
        if secretsStale {
            secretsStale = false
            out.append(["t": "cmd", "id": JSON(id("app.secrets")), "name": "secret.list"])
        }
        var wanted = shown.flatMap(\.blocks).compactMap(\.file).filter(\.isImage).map(\.file)
        // The icons of the badges on its cards and its questions.
        let cardBadges = shown.flatMap(\.blocks).compactMap { $0.card?.badge }
        let askBadges = approvals.flatMap { $0.shown.badges + $0.shown.steps.compactMap(\.badge) }
        wanted += (cardBadges + askBadges).flatMap(\.icons)
        // The icons of the sites its messages link to. A page's picture is shown only
        // under a pointer, which the iPhone hasn't, so it isn't fetched.
        wanted += shown.flatMap { $0.links.values.compactMap(\.icon) }
        // And the links pasted in the composer.
        for case .ready(let l) in linkPreviews.values { if let icon = l.icon { wanted.append(icon) } }
        // The thumbnails of the pictures its replies name.
        wanted += shown.flatMap { $0.named.compactMap(\.thumb) }
        // And those of pictures sent before and picked to go again, which the composer
        // has no file of its own to draw.
        wanted += attachments.filter { $0.path.isEmpty && isPicture($0.name) }.compactMap { $0.file?.file }
        for sha in Set(wanted) {
            let key = "\(sha).thumb"
            if files[key] != nil || getting.values.contains(where: { $0.key == key }) { continue }
            let id = id("app.get")
            getting[id] = (key, false)
            out.append(["t": "file.get", "id": JSON(id), "file": JSON(sha), "thumb": true])
        }
        return out
    }

    /// The next message, with the attachments that are up and each mention the composer
    /// put in that's still there as its link. Nil while one is still going up, or with
    /// nothing to send.
    func send(_ text: String, mentions: [Mention] = []) -> JSON? {
        guard let agent = selected, !uploading else { return nil }
        // An agent's name typed after @ without the picker is a mention too.
        let text = withReplyMentions(withMentionLinks(text, mentions), agents: agents.map { (id: $0.id, name: $0.name) }, files: [], agent: agent).trimmingCharacters(in: .whitespacesAndNewlines)
        let files = attachments.compactMap { $0.file?.file }
        if text.isEmpty && files.isEmpty { return nil }
        attachments = []
        dropped = []
        var frame: [String: JSON] = ["t": "send", "agent": JSON(agent), "text": JSON(text)]
        if !files.isEmpty { frame["files"] = JSON(files) }
        if let r = replyingHere {
            var reply: [String: JSON] = ["message": JSON(r.message)]
            if r.part { reply["quote"] = JSON(r.text) }
            if r.ask { reply["ask"] = true }
            frame["reply"] = .object(reply)
        }
        replying = nil
        return .object(frame)
    }

    // MARK: Replying to a message (PLAN.md, "Replying to a message")

    /// Starts a reply to the chat's message `message`: to the part `quote` when one was
    /// selected, asking about it with `ask`.
    func reply(to message: String, quote: String? = nil, ask: Bool = false) {
        guard let m = messages.first(where: { $0.id == message && !$0.live && !$0.queued }) else { return }
        replying = ReplyTo.to(m, agent: selected ?? "", quote: quote, ask: ask)
    }

    /// What the message being written replies to, while it's in the chat on screen.
    var replyingHere: ReplyTo? {
        guard let r = replying, messages.contains(where: { $0.id == r.message }) else { return nil }
        return r
    }

    /// Asks for more of the chat's older messages, to reach one a quote goes to. False when
    /// the chat has no more.
    func loadFurtherBack() -> Bool {
        guard moreMessages, messagesMore < 1800 else { return false }
        messagesMore += 200
        reaching = true
        dirty.insert(.messages)
        return true
    }

    /// Starts sending a file for the next message.
    func attach(path: String, name: String) -> JSON {
        let id = id("app.up")
        attachments.append(Attachment(id: id, name: name, path: path))
        return ["t": "file.upload", "id": JSON(id), "path": JSON(path)]
    }

    /// Keeps the files for the next message in step with their pieces in the composer,
    /// `shown` (the attachments' ids, in the order they're written), as
    /// desktop/src/model.rs's `sync_attachments`: a file whose piece was taken out is
    /// dropped, and comes back when an undo puts it back.
    func syncAttachments(_ shown: [String]) {
        let all = attachments + dropped
        let kept = shown.compactMap { id in all.first { $0.id == id } }
        let gone = all.filter { a in !shown.contains(a.id) }
        if kept.map(\.id) != attachments.map(\.id) { attachments = kept }
        if gone.map(\.id) != dropped.map(\.id) { dropped = gone }
    }

    /// Looks up a link pasted in the composer (`link.preview`), unless it has been.
    func previewLink(_ url: String) -> JSON? {
        switch linkPreviews[url] {
        case .loading, .ready: return nil
        default: break
        }
        linkPreviews[url] = .loading
        return ["t": "cmd", "id": JSON(id("app.preview:") + "#" + url), "name": "link.preview", "args": ["url": JSON(url)]]
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

    // MARK: Mentions

    /// A reply's words with what it mentions as the person's messages show it
    /// (`withReplyMentions`), so the person sees the same whoever wrote them.
    func replyWords(_ m: Message, _ text: String) -> String {
        withReplyMentions(text, agents: agents.map { (id: $0.id, name: $0.name) }, files: m.named, agent: selected ?? "")
    }

    /// What a message's words need to show their links and mentions: the icons of the
    /// sites it links to and the thumbnails of the pictures sent with it, once they're
    /// fetched, and the agents it mentions.
    func inline(_ m: Message) -> Inline {
        var out = Inline()
        for (url, link) in m.links {
            if let icon = link.icon, let path = files["\(icon).thumb"] { out.icons[url] = path }
        }
        for f in m.blocks.compactMap(\.file) where f.isImage {
            if let path = files["\(f.file).thumb"] { out.icons["loom-file:\(f.file)"] = path }
        }
        // A picture a reply names, by its tiny thumbnail where it's named.
        for n in m.named {
            if let thumb = n.thumb, let path = files["\(thumb).thumb"] { out.icons[fileMention(agent: selected ?? "", path: n.path, name: n.name).link] = path }
        }
        // A reply names agents as @Name, which become links only as it's shown.
        let words = m.role == "agent" ? m.blocks.map { replyWords(m, $0.text) } : m.blocks.map(\.text)
        if words.contains(where: { $0.contains("loom:agent/") }) {
            for a in agents where words.contains(where: { $0.contains("loom:agent/\(a.id))") }) {
                out.agents[a.id] = a
            }
        }
        return out
    }

    /// Asks for the files in the shown agent's folder and the shared one that match
    /// `query`, for the composer's @ picker, unless it has them or is asking already.
    func findFiles(_ query: String) -> JSON? {
        guard let agent = selected else { return nil }
        if foundFiles.agent == agent && foundFiles.query == query { return nil }
        if let a = filesAsking, a.agent == agent, a.query == query { return nil }
        let id = id("app.agent_files")
        filesAsking = (id, agent, query)
        return ["t": "cmd", "id": JSON(id), "name": "agent.files", "args": ["agent": JSON(agent), "query": JSON(query)]]
    }

    /// The @ picker's choices for `query`: the person's other agents whose names have it,
    /// those starting with it first, at most five, then at most eight of the files found
    /// for it.
    func picks(_ query: String) -> [Pick] {
        let q = query.lowercased()
        // An empty query matches every agent, which `contains` wouldn't say.
        let matching = agents.filter { $0.id != selected && (q.isEmpty || $0.name.lowercased().contains(q)) }
        let starting = matching.filter { $0.name.lowercased().hasPrefix(q) }
        let rest = matching.filter { !$0.name.lowercased().hasPrefix(q) }
        var out: [Pick] = (starting + rest).prefix(5).map { a in
            let line = a.instructions.split(separator: "\n").lazy.map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
            let about = String(line.drop { $0 == "#" || $0 == " " }.prefix(48))
            return .agent(a, about: about)
        }
        if foundFiles.agent == selected && foundFiles.query == query {
            out += foundFiles.files.prefix(8).map { .file($0) }
        }
        return out
    }

    /// The composer's text with what the @ picker chose in place of the @ at `at` and
    /// what follows it, and a space after, with the mention it put in.
    func pick(_ pick: Pick, at: Int, in text: String) -> (text: String, mention: Mention)? {
        guard let agent = selected, at <= text.count else { return nil }
        let mention: Mention
        switch pick {
        case .agent(let a, _): mention = agentMention(id: a.id, name: a.name)
        case .file(let f): mention = fileMention(agent: agent, path: f.path, name: f.name)
        }
        return (String(text.prefix(at)) + mention.words + " ", mention)
    }

    // MARK: The composer's + menu (PLAN.md, "The composer's + menu")

    /// Asks for what the + menu lists: the person's skills, their routines, and the files
    /// sent lately in their chats.
    func loadPlus() -> [JSON] {
        [
            ["t": "cmd", "id": JSON(id("app.plus_skills")), "name": "skill.list", "args": [:]],
            ["t": "cmd", "id": JSON(id("app.plus_routines")), "name": "routine.list", "args": [:]],
            ["t": "cmd", "id": JSON(id("app.recent_files")), "name": "files.recent", "args": ["limit": 50]],
        ]
    }

    /// The + menu's groups, in order: what adds to the message and what it does. With
    /// something typed in its search, every group's rows that match it instead, the lists
    /// beside the menu among them, names that start with it first, as
    /// desktop/src/model.rs's `plus_groups`.
    func plusGroups(_ query: String) -> [[PlusRow]] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty {
            return [[.attach, .open(.recent)], [.open(.skills), .open(.routines)]]
        }
        func found<T>(_ rows: [T], _ name: (T) -> (String, String), _ row: (T) -> PlusRow) -> [PlusRow] {
            let hits = rows.compactMap { r -> (Bool, T)? in
                let (n, also) = name(r)
                if n.lowercased().contains(q) { return (n.lowercased().hasPrefix(q), r) }
                return also.lowercased().contains(q) ? (false, r) : nil
            }
            return (hits.filter(\.0) + hits.filter { !$0.0 }).map { row($0.1) }
        }
        var add: [PlusRow] = "attach files".contains(q) ? [.attach] : []
        add += found(recentFiles, { ($0.file.name, "") }, PlusRow.file)
        let act = found(skills, { ($0.name, $0.description) }, PlusRow.skill)
            + found(plusRoutines, { ($0.name, "") }, PlusRow.routine)
        return [add, act].filter { !$0.isEmpty }
    }

    /// The list a row of the + menu opens.
    func plusSub(_ which: PlusSub) -> [PlusRow] {
        switch which {
        case .recent: recentFiles.map(PlusRow.file)
        case .skills: skills.map(PlusRow.skill)
        case .routines: plusRoutines.map(PlusRow.routine)
        }
    }

    /// The person's routines as the + menu lists them, this agent's first.
    var plusRoutines: [PlusRoutine] {
        routines.filter { $0.agent == selected } + routines.filter { $0.agent != selected }
    }

    /// A file from the + menu's recent files, for the next message: it's on the instance
    /// already, so nothing goes up. Whether it was added, which it isn't when it waits
    /// already.
    /// Where an attached picture is drawn from: its file on this phone, or for one sent
    /// before and picked to go again, its thumbnail once that has come.
    func picturePath(_ a: Attachment) -> String {
        guard a.path.isEmpty else { return a.path }
        return a.file.flatMap { files["\($0.file).thumb"] } ?? ""
    }

    func attachSent(_ f: FileRef) -> Bool {
        if attachments.contains(where: { $0.file?.file == f.file }) { return false }
        attachments.append(Attachment(id: id("app.sent"), name: f.name, path: "", file: f))
        return true
    }

    /// Runs a routine now, from the + menu.
    func runRoutine(_ id: String) -> JSON {
        ["t": "cmd", "id": JSON(self.id("app.routine_run")), "name": "routine.run", "args": ["id": JSON(id)]]
    }

    /// Opens a file a message mentions, from its `loom:file/AGENT/PATH` link: the
    /// instance keeps it as the person's first, then it's fetched and shown.
    func openFileMention(_ link: String) -> JSON? {
        guard case .file(let agent, let path) = LinkTo(link) else { return nil }
        return ["t": "cmd", "id": JSON(id("app.agent_file")), "name": "agent.file", "args": ["agent": JSON(agent), "path": JSON(path)]]
    }

    /// Opens a file sent with a message where its name links to it: a picture big, to
    /// page through, and anything else once it's fetched.
    func openAttached(_ sha: String) -> JSON? {
        guard let f = shown.flatMap(\.blocks).compactMap(\.file).first(where: { $0.file == sha }) else { return nil }
        if f.isImage {
            view([f])
            return nil
        }
        return open(f)
    }

    /// Opens the viewer on the pictures a reply names, at the first: the instance keeps
    /// each as the person's first, and the viewer opens once they're all kept.
    func viewNamed(_ named: [NamedFile]) -> [JSON] {
        guard let agent = selected, !named.isEmpty else { return [] }
        let group = id("app.agent_file.pictures")
        mentionStack = (group, Array(repeating: nil, count: named.count))
        return named.enumerated().map { k, n in
            ["t": "cmd", "id": JSON("\(group)#\(k)"), "name": "agent.file", "args": ["agent": JSON(agent), "path": JSON(n.path)]]
        }
    }

    /// Shows a stack of pictures big, at its first.
    func view(_ stack: [FileRef]) {
        guard !stack.isEmpty else { return }
        viewing = Viewing(stack: stack)
    }

    /// Fetches a picture whole, for the viewer, unless it's here or on its way.
    func whole(_ f: FileRef) -> JSON? {
        if files[f.file] != nil || getting.values.contains(where: { $0.key == f.file }) { return nil }
        let id = id("app.get")
        getting[id] = (f.file, false)
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

    /// Joins the instance in a code another device shows under Add a device: its QR
    /// code's text, the link, or the code pasted. It waits for a yes on that device.
    func link(qr: String, device: JSON) -> JSON {
        loggingIn = true
        loginError = nil
        adding = nil
        return ["t": "link", "id": JSON(id("app.link")), "qr": JSON(qr.trimmingCharacters(in: .whitespacesAndNewlines)), "device": device]
    }

    /// Stops waiting for a yes on the other device.
    func cancelLink() -> JSON {
        adding = nil
        loggingIn = false
        return ["t": "link.cancel"]
    }

    /// A new code for Add a device.
    func makeAddCode() -> JSON {
        addCode = nil
        joined = nil
        return ["t": "cmd", "id": JSON(id("app.add_code")), "name": "link.create"]
    }

    /// The asks still waiting, the ones that ran out let go.
    var openAsks: [LinkAsk] {
        let now = nowMs()
        return asks.filter { $0.expires > now }
    }

    /// The person's yes or no, here, to a device asking to be added.
    func answerAsk(_ ask: String, allow: Bool) -> JSON {
        asks.removeAll { $0.id == ask }
        return ["t": "cmd", "id": JSON(id("app.link_answer")), "name": "link.answer", "args": ["ask": JSON(ask), "allow": JSON(allow)]]
    }

    /// Asks the instance for the person's devices again, with when each was last on.
    func loadDevices() { dirty.insert(.devices) }

    /// Signs one of the person's devices out, for good.
    func removeDevice(_ device: String) -> JSON {
        devices.removeAll { $0.id == device }
        return ["t": "cmd", "id": JSON(id("app.device_revoke")), "name": "device.revoke", "args": ["device": JSON(device)]]
    }

    /// The first password of an account a link code made, which has none.
    func setPassword(_ password: String) -> JSON {
        passwordError = nil
        return ["t": "cmd", "id": JSON(id("app.password_set")), "name": "password.set", "args": ["password": JSON(password)]]
    }

    func cancel() -> JSON? {
        guard let conversation, let run = runs.first(where: { !$0.quiet && $0.conversation == conversation }) else { return nil }
        return ["t": "cancel", "run": JSON(run.id)]
    }

    /// The person's answer to a question: its card says how at once, until the instance
    /// takes the question away.
    func approve(_ call: String, allow: Bool, always: Bool = false) -> JSON {
        answering[call] = Approval.Answer(allow: allow, always: always)
        return ["t": "approve", "call": JSON(call), "allow": JSON(allow), "always": JSON(always)]
    }

    /// An answer picked on an agent's choices card: it goes as the person's message.
    func choose(_ answer: String) -> JSON? {
        guard let agent = selected else { return nil }
        return ["t": "send", "agent": JSON(agent), "text": JSON(answer)]
    }

    /// Picks `value` on field `field` of the form `card` in reply `message`: an option of
    /// a choice, or a toggle on or off.
    func pickForm(message: String, card: String, field: Int, value: JSON) {
        formPicks["\(message)/\(card)/\(field)"] = value
        formErrors["\(message)/\(card)"] = nil
    }

    /// Send on the form `card` in reply `message`, with what's typed in its text and
    /// number fields: its answers go as the person's next message. Nil, saying which on
    /// the card, while a required field has nothing in it.
    func sendForm(message: String, card: String, typed: [String]) -> JSON? {
        let key = "\(message)/\(card)"
        guard let agent = selected,
              let found = messages.first(where: { $0.id == message })?.blocks.compactMap(\.card).first(where: { $0.id == card && $0.type == "form" })
        else { return nil }
        let picks = found.formFields.indices.map { formPicks["\(key)/\($0)"] ?? found.formFields[$0].value }
        switch formAnswers(found.formFields, typed: typed, picks: picks) {
        case .failure(let missing):
            formErrors[key] = "\(missing.label) needs an answer."
            return nil
        case .success(let answers):
            let cid = id("m_form")
            formErrors[key] = nil
            formSending[cid] = key
            return ["t": "send", "cid": JSON(cid), "agent": JSON(agent), "text": "",
                    "form": ["message": JSON(message), "card": JSON(card), "answers": answers]]
        }
    }

    /// The person's answer to the MCP server an agent proposed on card `call`: `add`, with
    /// each secret it needs that they don't have yet, `cancel`, or `later`, for one waiting
    /// on a sign-in.
    func answerMcpAdd(_ call: String, _ answer: String, secrets: [String: String] = [:]) -> JSON {
        ["t": "cmd", "id": JSON(id("app.mcp_add")), "name": "mcp.add_answer",
         "args": ["call": JSON(call), "answer": JSON(answer), "secrets": .object(secrets.mapValues { JSON($0) })]]
    }

    /// What the person filled in on the secret card for `call`, which `agent` asked for:
    /// each field's value, and the name to keep it as where Save as is on (`keep`). Nil,
    /// saying so on the card, while a field is empty, since Fill with nothing would be a no.
    func answerSecret(agent: String, call: String, values: [String], names: [String], keep: [Bool]) -> JSON? {
        if values.isEmpty || values.contains(where: \.isEmpty) {
            secretErrors[call] = "Fill in each field, or say Not now."
            return nil
        }
        let fields: [JSON] = values.indices.map { (k) -> JSON in
            let name = k < names.count ? names[k].trimmingCharacters(in: .whitespaces) : ""
            let kept = k < keep.count && keep[k] && !name.isEmpty
            return kept ? ["value": JSON(values[k]), "save": JSON(name)] : ["value": JSON(values[k])]
        }
        secretErrors[call] = nil
        secretSending.insert(call)
        return ["t": "cmd", "id": JSON("app.secret_fill:\(call)"), "name": "browser.secret",
                "args": ["agent": JSON(agent), "fields": .array(fields)]]
    }

    /// Not now, on the secret card for `call`.
    func declineSecret(agent: String, call: String) -> JSON {
        secretErrors[call] = nil
        secretSending.insert(call)
        return ["t": "cmd", "id": JSON("app.secret_fill:\(call)"), "name": "browser.secret",
                "args": ["agent": JSON(agent), "fields": .array([])]]
    }

    /// Has the Secrets page's list read again.
    func loadSecrets() { secretsStale = true }

    func putSecret(_ name: String, _ value: String) -> JSON {
        ["t": "cmd", "id": JSON(id("app.secret_put")), "name": "secret.put",
         "args": ["name": JSON(name.trimmingCharacters(in: .whitespaces)), "value": JSON(value)]]
    }

    func deleteSecret(_ name: String) -> JSON {
        secrets.removeAll { $0.name == name }
        return ["t": "cmd", "id": JSON(id("app.secret_delete")), "name": "secret.delete", "args": ["name": JSON(name)]]
    }

    /// Takes the site `site` off where the secret `name` may be filled in.
    func removeSecretSite(_ name: String, _ site: String) -> JSON? {
        guard let i = secrets.firstIndex(where: { $0.name == name }) else { return nil }
        secrets[i].sites.removeAll { $0 == site }
        return ["t": "cmd", "id": JSON(id("app.secret_sites")), "name": "secret.sites",
                "args": ["name": JSON(name), "sites": JSON(secrets[i].sites)]]
    }

    /// Asks the instance where to sign in to the person's MCP server `server`, with the
    /// browser coming back to `redirect` (PLAN.md, "Signing in to MCP servers"). Its card
    /// says Signing in… until the sign-in ends; the page comes as `toSignIn`.
    func signIn(_ server: String, redirect: String) -> JSON? {
        guard mcpSigning.insert(server).inserted else { return nil }
        mcpSignInErrors[server] = nil
        return ["t": "cmd", "id": JSON("app.mcp_sign_in:\(server)"), "name": "mcp.sign_in", "args": ["name": JSON(server), "redirect": JSON(redirect)]]
    }

    /// What the browser brought back to the redirect, as it came, for the instance to
    /// swap for tokens.
    func signedIn(_ server: String, query: String) -> JSON {
        ["t": "cmd", "id": JSON("app.mcp_signed_in:\(server)"), "name": "mcp.signed_in", "args": ["query": JSON(query)]]
    }

    /// A sign-in that ended here without coming back: the card is live again, saying why.
    func signInEnded(_ server: String, error: String) {
        mcpSigning.remove(server)
        mcpSignInErrors[server] = error
    }

    // MARK: The agent list's folders (PLAN.md, "The sidebar")

    /// The folders, each id and name, in order.
    var folders: [(id: String, name: String)] {
        sidebar.compactMap {
            if case .folder(let id, let name, _) = $0.part { return (id, name) }
            return nil
        }
    }

    /// The agents in the list's order, without the hidden ones.
    var shownAgents: [Agent] {
        let ids = sidebar.filter { $0.part != .hidden }.flatMap(\.agents)
        return ids.isEmpty ? agents : ids.compactMap { self.agent($0) }
    }

    /// Puts an agent in a folder, or with nil, out of them. It shows there at once.
    func groupAgent(_ agent: String, _ group: String?) -> JSON {
        for i in sidebar.indices { sidebar[i].agents.removeAll { $0 == agent } }
        if let i = sidebar.firstIndex(where: {
            switch $0.part {
            case .folder(let id, _, _): return id == group
            case .agents: return group == nil
            default: return false
            }
        }) {
            sidebar[i].agents.append(agent)
        }
        return ["t": "cmd", "id": JSON(id("app.group_agent")), "name": "agent.group", "args": ["agent": JSON(agent), "group": JSON(group ?? "")]]
    }

    /// Hides an agent, or shows it again.
    func hideAgent(_ agent: String, _ hidden: Bool) -> JSON {
        ["t": "cmd", "id": JSON(id("app.hide_agent")), "name": "agent.hide", "args": ["agent": JSON(agent), "hidden": JSON(hidden)]]
    }

    /// Makes a folder, with `agent` in it once it's made. Nil for a blank name.
    func newFolder(_ name: String, agent: String?) -> JSON? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        folderFor = agent
        return ["t": "cmd", "id": JSON(id("app.folder_new")), "name": "group.create", "args": ["name": JSON(name)]]
    }

    /// Renames a folder. Nil for a blank name, which keeps the one it has.
    func renameFolder(_ folder: String, _ name: String) -> JSON? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        for i in sidebar.indices {
            if case .folder(let id, _, let collapsed) = sidebar[i].part, id == folder {
                sidebar[i].part = .folder(id: id, name: name, collapsed: collapsed)
            }
        }
        return ["t": "cmd", "id": JSON(id("app.folder")), "name": "group.update", "args": ["group": JSON(folder), "name": JSON(name)]]
    }

    /// Opens or shuts a folder, at once here, and on the person's other devices.
    func collapseFolder(_ folder: String, _ collapsed: Bool) -> JSON {
        for i in sidebar.indices {
            if case .folder(let id, let name, _) = sidebar[i].part, id == folder {
                sidebar[i].part = .folder(id: id, name: name, collapsed: collapsed)
            }
        }
        return ["t": "cmd", "id": JSON(id("app.folder")), "name": "group.update", "args": ["group": JSON(folder), "collapsed": JSON(collapsed)]]
    }

    /// Deletes a folder. Its agents stay, out of any folder.
    func deleteFolder(_ folder: String) -> JSON {
        ["t": "cmd", "id": JSON(id("app.folder")), "name": "group.delete", "args": ["group": JSON(folder)]]
    }

    func startFresh() -> JSON? {
        guard let agent = selected else { return nil }
        return ["t": "cmd", "id": JSON(id("app.fresh")), "name": "conversation.create", "args": ["agent": JSON(agent)]]
    }
}
