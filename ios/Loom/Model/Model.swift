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
    /// When it was pinned to the top of the list, and its place where the person
    /// dragged it, if either.
    var pinned: Int?
    var order: Int?

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
    /// That the chat has gone on past its reply: choices no longer answer then, and a
    /// sign-in only waits quietly.
    var passed = false
    /// For a card that signs in, as `mcp.list` says: whom the person is signed in to its
    /// server as once they are (empty when the sign-in didn't say), whether its sign-in
    /// page is open now, and why the last one here didn't finish.
    var signedAs: String?
    var signing = false
    var signInError: String?

    /// Whether it signs in to its server: a sign-in card, or a proposed server that
    /// waits on one.
    var signsIn: Bool {
        type == "sign_in" || (type == "mcp_add" && ["sign_in", "later"].contains(state))
    }

    init?(_ c: JSON) {
        type = c["type"].text
        guard ["choices", "checklist", "table", "progress", "facts", "sign_in", "mcp_add", "secret_ask"].contains(type) else { return nil }
        agent = c["agent"].text
        site = c["site"].text
        fields = c["fields"].array.map {
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
    /// For a message another agent sent, who.
    var from: String?
    /// How a reply's turn ended: `done`, or `cancelled`, `failed` or `limit` for one cut
    /// short.
    var status = ""
    /// The Sources list the instance took off the end of a reply's words, shown on a tab
    /// under its bubble.
    var sources: [Source] = []
    /// The pages it links to that the instance has looked up, by address.
    var links: [String: PageLink] = [:]

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

    static func all(_ j: JSON) -> [String: PageLink] {
        guard case .object(let o) = j else { return [:] }
        return o.mapValues { v in
            PageLink(title: v["title"].text, site: v["site"].text, icon: v["icon"].string.flatMap { $0.isEmpty ? nil : $0 })
        }
    }
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
    /// Two or more pictures next to each other, drawn as one stack the viewer opens.
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
/// stack.
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
/// stack, wherever they were among the others.
func fileParts(_ m: Message) -> [Part] {
    let files = m.blocks.indices.filter { m.blocks[$0].file != nil }
    let pictures = files.filter { m.blocks[$0].file?.isImage == true }
    let others = files.filter { m.blocks[$0].file?.isImage != true }
    return layOut(m, pictures) + others.map { .block($0) }
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
func routinePrompt(_ m: JSON) -> Bool {
    m["role"].text == "person" && m["from"].text.hasPrefix("rt_")
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
    /// Answers given here, by call, until the instance takes them.
    private var answering: [String: Approval.Answer] = [:]
    var devices: [Device] = []
    /// Each agent program's name as people know it ("Claude Agent" for `claude-acp`), by
    /// its name, as `acp.list` gives it.
    var programTitles: [String: String] = [:]
    private var programsWanted = false
    /// The selected agent's messages, oldest first, with the queued ones last.
    var messages: [Message] = []
    /// Whether the selected chat's messages have come since it was selected.
    var messagesCame = false
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
    /// A stack of pictures shown big, to page through.
    var viewing: Viewing?
    /// The person's secrets, for the Secrets page, read again when they change.
    var secrets: [SecretItem] = []
    private var secretsStale = false
    /// Secret cards whose answer went and hasn't come back, and what the instance said to
    /// one it refused, by call.
    private var secretSending: Set<String> = []
    private var secretErrors: [String: String] = [:]
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
                c.passed = next != nil
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
        messages = []
        messagesCame = false
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
        default:
            if let error { notice = error }
        }
    }

    private func data(_ f: JSON) {
        guard let what = CoreList(rawValue: f["what"].text) else { return }
        let items = f["items"].array
        switch what {
        case .agents:
            agents = listed(items.map {
                Agent(id: $0["id"].text, name: $0["name"].text, color: $0["color"].text, program: $0["acp_agent"].text, shape: $0["shape"].text, drawing: points($0["drawing"]), pinned: $0["pinned"].int, order: $0["order"].int)
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
                Approval(call: a["call"].text, run: a["run"].text, kind: a["tool"].text, title: a["title"].text, shown: Shown(a), canAlways: a["can_always"].bool ?? false)
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
            messages = items.filter { !routinePrompt($0) }.map { m in
                Message(id: m["id"].text, role: m["role"].text, at: m["at"].int ?? 0, blocks: m["blocks"].array.map(Block.of), from: from(m["from"].text), status: m["status"].text, sources: m["sources"].array.map(Source.of).filter { !$0.url.isEmpty }, links: PageLink.all(m["links"]))
            }
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
                out.append(["t": "core.get", "id": "app.messages", "what": "messages", "conversation": JSON(conversation), "limit": 200])
            } else if l == .tools {
                // Only the calls the shown chat's blocks name, and the runs going on: a
                // heavy user has tens of thousands of calls, and this comes at each one.
                let calls = messages.flatMap(\.blocks).compactMap(\.call)
                out.append(["t": "core.get", "id": "app.tools", "what": "tools", "calls": JSON(calls), "runs": JSON(runs.map(\.id))])
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
        // The icons of the sites its messages link to.
        wanted += shown.flatMap { $0.links.values.compactMap(\.icon) }
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

    /// Joins the instance in a QR code the desktop app shows under Add a phone.
    func link(qr: String, device: JSON) -> JSON {
        loggingIn = true
        loginError = nil
        return ["t": "link", "id": JSON(id("app.link")), "qr": JSON(qr), "device": device]
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

    func startFresh() -> JSON? {
        guard let agent = selected else { return nil }
        return ["t": "cmd", "id": JSON(id("app.fresh")), "name": "conversation.create", "args": ["agent": JSON(agent)]]
    }
}
