import Foundation

// Acta's cards in the chat (PLAN.md, "Cards in messages" and "Acta beside the chat"), as
// desktop/src/model.rs and desktop/src/model/acta.rs have them: a card a message names,
// as the instance looked it up, and the pill its words show it as; a card pasted or typed
// in the composer; a card's preview sheet, the form that makes a card from a message, and
// a board's screen, with Follow. The model holds what each shows and what the person's
// changes send; the instance calls Acta as them.

/// Where Acta is: its card links are on it.
let actaHost = "acta.aaenz.no"
/// A card looked up longer ago than this is looked up again when its message shows.
let cardFreshMs = 300_000
/// The person's boards are asked for again once they're this old.
let boardsFreshMs = 600_000
/// How a card's link in a message's Markdown starts, then the card's id, and how a card's
/// piece in the composer starts, then what it names: its id or its link.
let cardLink = "loom-card:"
/// The most characters of a card's title its pill shows, cut with "…": the desktop's
/// `layout.pill-title`, at the phone's size.
let pillTitleMost = 20

/// A card on one of the person's Acta boards that a message names, as the instance looked
/// it up (protocol.md, "Messages and streaming"), or one that isn't there or that the
/// person can't see (`missing`).
struct ActaCard: Equatable, Sendable {
    var id: String
    /// Its board, `workspace/board`, and the board's name.
    var board = ""
    var boardName = ""
    var title = ""
    /// todo, in_progress, review, done or canceled.
    var status = ""
    var url = ""
    var assignee = ""
    var size = ""
    var labels: [String] = []
    /// The start of its description, as plain words.
    var text = ""
    var subtasks = 0
    var subtasksDone = 0
    var missing = false
    /// When it was looked up, in Unix ms.
    var at = 0

    init(id: String) { self.id = id }

    init(_ v: JSON) {
        id = v["id"].text
        board = v["board"].text
        boardName = v["board_name"].text
        title = v["title"].text
        status = v["status"].text
        url = v["url"].text
        assignee = v["assignee"].text
        size = v["size"].text
        labels = v["labels"].array.compactMap(\.string)
        text = v["text"].text
        subtasks = v["subtasks"].int ?? 0
        subtasksDone = v["subtasks_done"].int ?? 0
        missing = v["missing"].bool ?? false
        at = v["at"].int ?? 0
    }

    /// The colour its status is drawn in: its dot, and its chip.
    var ink: CardInk { CardInk(status) }

    /// A canceled card's title is struck through.
    var struck: Bool { status == "canceled" }

    /// Its status in words, as its chip says it.
    var statusWords: String { statusName(status) }

    /// Its board's name, as the preview's head has it before the id: as Acta names it, or
    /// the board's part after its workspace.
    var boardLabel: String {
        boardName.isEmpty ? String(board.split(separator: "/").last ?? "") : boardName
    }

    /// The preview's chips after its status: its assignee, its size and its labels.
    var chips: [String] {
        var out: [String] = []
        if !assignee.isEmpty { out.append(assignee) }
        if !size.isEmpty { out.append(size.uppercased()) }
        out += labels.filter { !$0.isEmpty }
        return out
    }

    /// "Subtasks 3 of 5", for a card with subtasks.
    var subtasksLine: String? { subtasks > 0 ? "Subtasks \(subtasksDone) of \(subtasks)" : nil }

    /// How much of its subtasks' bar is filled, from 0 to 1.
    var subtasksPart: Double { subtasks == 0 ? 0 : min(max(Double(subtasksDone) / Double(subtasks), 0), 1) }

    /// Where it opens: its page in Acta.
    var link: String { url.isEmpty ? "https://\(actaHost)/\(board)#\(id)" : url }

    /// Looked up long enough ago to be looked up again when it shows.
    func stale(_ now: Int) -> Bool { now - at >= cardFreshMs }

    /// Its title as its pill shows it: cut at `pillTitleMost` characters, with "…".
    var pillTitle: String {
        let t = title.trimmingCharacters(in: .whitespaces)
        guard t.count > pillTitleMost else { return t }
        return String(t.prefix(pillTitleMost - 1)).trimmingCharacters(in: .whitespaces) + "…"
    }
}

/// The colour a card's status is drawn in.
enum CardInk: Equatable, Sendable {
    /// To do, or canceled: the quiet ink.
    case quiet
    /// In progress.
    case sky
    /// In review.
    case gold
    /// Done.
    case ok

    init(_ status: String) {
        switch status {
        case "in_progress": self = .sky
        case "review": self = .gold
        case "done": self = .ok
        default: self = .quiet
        }
    }
}

/// The statuses a card moves to from its chip's menu, in order, and a board's columns.
let cardStatuses = ["todo", "in_progress", "review", "done", "canceled"]
let boardColumns = ["todo", "in_progress", "review", "done"]
/// A card's sizes, smallest first, and its priorities, as the form's segments show them.
let cardSizes = ["xs", "s", "m", "l", "xl"]
let cardPriorities = ["low", "medium", "high", "urgent"]

/// A status in words: "In progress".
func statusName(_ status: String) -> String {
    switch status {
    case "todo", "": return "Todo"
    case "in_progress": return "In progress"
    case "review": return "Review"
    case "done": return "Done"
    case "canceled": return "Canceled"
    default:
        let words = status.replacingOccurrences(of: "_", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }
}

/// A priority in words: "High".
func priorityName(_ priority: String) -> String {
    switch priority {
    case "low": "Low"
    case "medium": "Medium"
    case "high": "High"
    case "urgent": "Urgent"
    default: "None"
    }
}

// MARK: Card ids and links

private func isUpper(_ b: UInt8) -> Bool { b >= 65 && b <= 90 }
private func isDigit(_ b: UInt8) -> Bool { b >= 48 && b <= 57 }

/// How long the card id at the start of `s` is, `PREFIX-N` (an upper-case letter, up to
/// nine more letters or digits, a hyphen and a number with no leading zero), as the
/// instance reads one (server/acta.calx). It's all ASCII, so its length in characters.
func cardIdLength(_ s: Substring) -> Int? {
    let b = Array(s.utf8.prefix(20))
    guard let first = b.first, isUpper(first) else { return nil }
    var j = 1
    while j < b.count && j < 10 && (isUpper(b[j]) || isDigit(b[j])) { j += 1 }
    guard j < b.count, b[j] == UInt8(ascii: "-") else { return nil }
    j += 1
    let start = j
    guard j < b.count, isDigit(b[j]), b[j] != UInt8(ascii: "0") else { return nil }
    while j < b.count && j - start < 7 && isDigit(b[j]) { j += 1 }
    return j
}

/// Whether `s` is a whole card id, such as `LOO-206`.
func isCardId(_ s: some StringProtocol) -> Bool {
    cardIdLength(Substring(s)) == s.count
}

/// The card a link points at, its board and its id: `https://acta.aaenz.no/<workspace>/<board>#ID`,
/// or with `?task=ID` in its query.
func cardLinkParts(_ url: String) -> (board: String, id: String)? {
    let start = "https://\(actaHost)/"
    guard url.hasPrefix(start) else { return nil }
    let rest = url.dropFirst(start.count)
    let pathEnd = rest.firstIndex { $0 == "?" || $0 == "#" } ?? rest.endIndex
    var path = rest[..<pathEnd]
    while path.hasSuffix("/") { path = path.dropLast() }
    let parts = path.split(separator: "/", omittingEmptySubsequences: false)
    guard parts.count == 2, !parts[0].isEmpty, !parts[1].isEmpty else { return nil }
    var id: Substring?
    if let hash = rest.firstIndex(of: "#") {
        let after = rest[rest.index(after: hash)...]
        if isCardId(after) { id = after }
    }
    if id == nil, pathEnd < rest.endIndex, rest[pathEnd] == "?" {
        let query = rest[rest.index(after: pathEnd)...].split(separator: "#", omittingEmptySubsequences: false).first ?? ""
        id = query.split(separator: "&").lazy.compactMap { p -> Substring? in
            p.hasPrefix("task=") ? p.dropFirst(5) : nil
        }.first { isCardId($0) }
    }
    guard let id else { return nil }
    return ("\(parts[0])/\(parts[1])", String(id))
}

/// Whether a card id can start after `c`, the character before it: not inside a word, a
/// path or an address.
func cardStartsAfter(_ c: Character?) -> Bool {
    guard let c else { return true }
    return !(c.isLetter || c.isNumber || "_-/.@".contains(c))
}

/// Whether a card id that ends before `rest` is a whole one: not a piece of `LOO-3-b`,
/// `LOO-3.5` or `LOO-3_x`.
func cardEndsBefore(_ rest: Substring) -> Bool {
    guard let x = rest.first else { return true }
    if x.isLetter || x.isNumber || x == "_" { return false }
    if x == "-" || x == "." {
        guard let y = rest.dropFirst().first else { return true }
        return !(y.isLetter || y.isNumber || y == "_")
    }
    return true
}

/// A message's card as its words show it: a link the chat draws as the card's pill.
func cardMark(_ id: String) -> String { "[\(id)](\(cardLink)\(id))" }

/// The card `url` links to, when it's a card's link and the instance looked it up.
private func cardIn(_ url: Substring, _ cards: [String: ActaCard]) -> String? {
    guard let (board, id) = cardLinkParts(String(url)), let c = cards[id], c.board.isEmpty || c.board == board else { return nil }
    return id
}

/// A message's words with each Acta card it names, by its id or its link, as a link the
/// chat draws as the card's pill (`cardMark`), as desktop/src/model.rs's
/// `with_link_icons` does, for the cards the instance looked up. Code, a Markdown link to
/// anything else and an address sent as plain text (`<https://…>`) stay as they are.
func withCardPills(_ text: String, _ cards: [String: ActaCard]) -> String {
    guard !cards.isEmpty else { return text }
    var out: [String] = []
    var fenced = false
    for line in text.components(separatedBy: "\n") {
        let t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("```") || t.hasPrefix("~~~") {
            fenced.toggle()
            out.append(line)
        } else if fenced {
            out.append(line)
        } else {
            out.append(lineCards(line, cards))
        }
    }
    return out.joined(separator: "\n")
}

private func lineCards(_ line: String, _ cards: [String: ActaCard]) -> String {
    var out = ""
    var rest = Substring(line)
    var before: Character?
    func take(_ n: Substring.Index) {
        let piece = rest[..<n]
        out += piece
        before = piece.last ?? before
        rest = rest[n...]
    }
    while let c = rest.first {
        // Inline code stays as written.
        if c == "`" {
            let after = rest.dropFirst()
            take(after.firstIndex(of: "`").map { rest.index(after: $0) } ?? rest.endIndex)
            continue
        }
        // A Markdown link, [words](url): a card's is its pill, the rest stay.
        if c == "[", let close = rest.range(of: "]("), !rest[..<close.lowerBound].contains("\n"),
           let end = rest[close.upperBound...].firstIndex(of: ")") {
            let url = rest[close.upperBound..<end].trimmingCharacters(in: .whitespaces).split(separator: " ").first ?? ""
            if let id = cardIn(url, cards) {
                out += cardMark(id)
                before = ")"
                rest = rest[rest.index(after: end)...]
            } else {
                take(rest.index(after: end))
            }
            continue
        }
        // An address the person sent as plain text stays as it is.
        if rest.hasPrefix("<http://") || rest.hasPrefix("<https://"), let end = rest.firstIndex(of: ">") {
            take(rest.index(after: end))
            continue
        }
        // A bare address: a card's is its pill.
        if rest.hasPrefix("https://") || rest.hasPrefix("http://") {
            let end = rest.firstIndex { $0.isWhitespace || "<>\"`".contains($0) } ?? rest.endIndex
            var url = rest[..<end]
            while let last = url.last, ".,;:!?)'".contains(last) { url = url.dropLast() }
            if url.isEmpty { url = rest[..<end] }
            if let id = cardIn(url, cards) {
                out += cardMark(id)
                before = ")"
                rest = rest[url.endIndex...]
            } else {
                take(url.endIndex)
            }
            continue
        }
        // A card's id at the start of a word, for a card the instance looked up.
        if let n = cardIdLength(rest), cardStartsAfter(before) {
            let end = rest.index(rest.startIndex, offsetBy: n)
            let id = String(rest[..<end])
            if cards[id] != nil, cardEndsBefore(rest[end...]) {
                out += cardMark(id)
                before = ")"
                rest = rest[end...]
                continue
            }
        }
        take(rest.index(after: rest.startIndex))
    }
    return out
}

/// The card link a whole paste is, when it's one and nothing else.
func pastedCard(_ text: String) -> String? {
    guard let url = pastedLink(text), cardLinkParts(url) != nil else { return nil }
    return url
}

/// The card id the person just finished typing in the composer: what's before the caret
/// ends with a space or a mark that ends a word, after an id whose prefix is one of
/// `prefixes`. Where it starts, in characters, and the id. A piece in the composer is
/// U+FFFC there, which no id holds.
func typedCard(_ before: String, prefixes: [String]) -> (start: Int, id: String)? {
    guard let last = before.last, last.isWhitespace || ",.;:!?)".contains(last) else { return nil }
    let word = before.dropLast()
    var start = word.endIndex
    while start > word.startIndex {
        let p = word.index(before: start)
        let c = word[p]
        guard c.isASCII && (c.isUppercase || c.isNumber || c == "-") else { break }
        start = p
    }
    let id = word[start...]
    guard isCardId(id), cardStartsAfter(start > word.startIndex ? word[word.index(before: start)] : nil) else { return nil }
    let prefix = id.split(separator: "-").first.map(String.init) ?? ""
    guard prefixes.contains(prefix) else { return nil }
    return (word.distance(from: word.startIndex, to: start), String(id))
}

/// A card pasted in the composer, or typed for one of the person's boards, as it's looked
/// up (`card.preview`).
enum CardPreview: Equatable, Sendable {
    case loading
    case ready(ActaCard)
    /// It couldn't be looked up: it shows as its id.
    case failed
}

// MARK: A card's preview, the form, a board

/// One of a card's subtasks.
struct Subtask: Equatable, Sendable, Identifiable {
    var id: String
    var title: String
    var done: Bool
}

/// One of a card's comments.
struct CardComment: Equatable, Sendable {
    var by: String
    /// When, as Acta gives it (RFC 3339).
    var at: String
    /// Markdown.
    var body: String
}

/// A card as its preview shows it (`card.get`): as a pill has it, with its whole
/// description, its priority, its subtasks and its last three comments.
struct CardDetail: Equatable, Sendable {
    var card: ActaCard
    var description = ""
    var priority = ""
    var checklist: [Subtask] = []
    var comments: [CardComment] = []

    init(_ v: JSON) {
        card = ActaCard(v)
        description = v["description"].text
        priority = v["priority"].text
        checklist = v["checklist"].array.map { Subtask(id: $0["id"].text, title: $0["title"].text, done: $0["done"].bool ?? false) }
        comments = v["comments"].array.map { CardComment(by: $0["by"].text, at: $0["at"].text, body: $0["body"].text) }
    }

    /// While it's looked up: what a pill knew of it.
    init(known card: ActaCard) {
        self.card = card
        description = card.text
    }

    /// The chips after its status: its assignee, its size, its labels and its priority.
    var chips: [String] {
        var out = card.chips
        if !priority.isEmpty && priority != "none" { out.append(priorityName(priority)) }
        return out
    }
}

/// When a comment was written, as its line shows it: "25 Nov".
func commentDay(_ at: String) -> String {
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
    let chars = Array(at)
    guard chars.count >= 10, let m = Int(String(chars[5..<7])), (1...12).contains(m), let d = Int(String(chars[8..<10])) else { return "" }
    return "\(d) \(months[m - 1])"
}

/// One of a board's cards, as its screen shows it (`acta.tasks`).
struct BoardTask: Equatable, Sendable, Identifiable {
    var id: String
    var title = ""
    var status = ""
    var assignee = ""
    var labels: [String] = []
    var size = ""
    var priority = ""

    init(id: String, title: String = "", status: String = "", assignee: String = "", labels: [String] = []) {
        self.id = id
        self.title = title
        self.status = status
        self.assignee = assignee
        self.labels = labels
    }

    init(_ v: JSON) {
        id = v["id"].text
        title = v["title"].text
        status = v["status"].text
        assignee = v["assignee"].text
        labels = v["labels"].array.compactMap(\.string)
        size = v["size"].text
        priority = v["priority"].text
    }

    /// Its assignee's initial, for its row: "E".
    var initial: String { assignee.first.map { String($0).uppercased() } ?? "" }
}

/// A board, as `acta.board` gives it: its labels and its people.
struct BoardInfo: Equatable, Sendable {
    var board: String
    var name: String
    var prefix: String
    var labels: [String]
    var people: [String]
}

/// A message's first line as a card's title: without a heading's #, a list's mark or a
/// quote's >, without emphasis and code marks, cut at a word to fit 120 characters.
func titleFrom(_ text: String) -> String {
    let most = 120
    let line = text.components(separatedBy: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? ""
    var t = Substring(line)
    while true {
        let before = t
        t = t.drop { $0 == "#" || $0 == ">" }
        t = t.drop { $0 == " " }
        for mark in ["- ", "* ", "+ ", "[ ] ", "[x] "] where t.hasPrefix(mark) { t = t.dropFirst(mark.count) }
        if t == before { break }
    }
    let plain = t.replacingOccurrences(of: "**", with: "").replacingOccurrences(of: "__", with: "").replacingOccurrences(of: "`", with: "").trimmingCharacters(in: .whitespaces)
    guard plain.count > most else { return plain }
    let cut = String(plain.prefix(most))
    var kept = cut
    if let space = cut.lastIndex(of: " "), cut.distance(from: cut.startIndex, to: space) > cut.count / 2 {
        kept = String(cut[..<space])
    }
    return kept.trimmingCharacters(in: .whitespaces) + "…"
}

/// A board's cards in one of its columns, in board order. A canceled card is in none.
func column(_ tasks: [BoardTask], _ status: String) -> [BoardTask] {
    tasks.filter { $0.status == status }
}

/// A board's cards that a search, a label and an assignee pick: those whose id or title
/// has `search` in it, in any case, with `label` and `assignee` when they're picked.
func filtered(_ tasks: [BoardTask], search: String, label: String?, assignee: String?) -> [BoardTask] {
    let q = search.trimmingCharacters(in: .whitespaces).lowercased()
    return tasks.filter { t in
        (q.isEmpty || t.id.lowercased().contains(q) || t.title.lowercased().contains(q))
            && (label.map { t.labels.contains($0) } ?? true)
            && (assignee.map { t.assignee == $0 } ?? true)
    }
}

/// A card's preview sheet.
struct CardSheet: Equatable, Sendable, Identifiable {
    var board: String
    var id: String
    /// As `card.get` last gave it, or what a pill knew while it's looked up.
    var detail: CardDetail?
    /// Being looked up, or changed.
    var loading = true
    var error: String?
    /// What it was before a change on its way, to go back to if Acta refuses it.
    fileprivate var before: CardDetail?
}

/// The form that makes a card from a message.
struct MakeForm: Equatable, Sendable {
    /// The message's id, which the card is kept with.
    var message: String
    var board: String
    /// What the title and description were filled with.
    var title: String
    var description: String
    /// The board's labels picked.
    var labels: [String] = []
    var size: String?
    var priority: String?
    /// card.create is on its way: Make does nothing until it's answered.
    var making = false
    var error: String?
}

/// A board's screen: its cards, by status.
struct BoardScreen: Equatable, Sendable {
    var board: String
    /// The column shown.
    var status = "todo"
    /// Its open cards in board order, then its latest done.
    var tasks: [BoardTask] = []
    /// What's still to come: the open cards and the done.
    fileprivate var waiting = 2
    var error: String?
    /// Cards moved, until Acta answers: each with the status it had.
    fileprivate var moving: [(id: String, was: String)] = []

    var loading: Bool { waiting > 0 }

    /// The cards of the column shown.
    var shown: [BoardTask] { column(tasks, status) }

    static func == (a: BoardScreen, b: BoardScreen) -> Bool {
        a.board == b.board && a.status == b.status && a.tasks == b.tasks && a.waiting == b.waiting && a.error == b.error && a.moving.map(\.id) == b.moving.map(\.id)
    }
}

/// A watch of the person's on an Acta board, or one card on it (PLAN.md, "Watching
/// outside events"), as far as Follow needs it: what it follows and the chat it posts in.
struct ActaWatch: Equatable, Sendable {
    /// "" while `watch.put` is under way.
    var id: String
    var board: String
    /// "" for the whole board.
    var card: String
    var conversation: String

    /// A watch as the instance gives it, when it's on an Acta board and not a routine's.
    init?(_ w: JSON) {
        let integration = w["integration"].text
        guard integration.isEmpty || integration == "acta", w["wake"]["routine"].isNull else { return nil }
        let board = w["target"]["board"].text
        guard !board.isEmpty, !w["id"].text.isEmpty else { return nil }
        id = w["id"].text
        self.board = board
        card = w["target"]["card"].text
        conversation = w["conversation"].text
    }

    init(id: String, board: String, card: String, conversation: String) {
        self.id = id
        self.board = board
        self.card = card
        self.conversation = conversation
    }

    func same(_ board: String, _ card: String, _ conversation: String) -> Bool {
        self.board == board && self.card == card && self.conversation == conversation
    }
}

/// What an answer from the instance was asked for.
private enum ActaAsk {
    case card(board: String, id: String)
    case move(board: String, id: String)
    case create
    case board
    case tasks(board: String, done: Bool)
    case watches
    case follow(ActaWatch)
    case unfollow(ActaWatch)
}

/// What the model keeps of Acta.
struct ActaState {
    /// The prefixes of the person's boards, each with its board, from `acta.boards`, and
    /// each board's name.
    var boards: [(prefix: String, board: String)] = []
    var names: [String: String] = [:]
    fileprivate var boardsAt: Int?
    fileprivate var boardsAsking = false
    /// The cards pasted or typed in the composer, by what names them, as they're looked up.
    var previews: [String: CardPreview] = [:]
    /// The messages of the shown chat whose cards were looked at since it showed.
    fileprivate var looked: Set<String> = []
    /// The card whose preview sheet is open.
    var sheet: CardSheet?
    /// The form that makes a card, while its sheet is open.
    var form: MakeForm?
    /// A board's screen, while it's pushed.
    var screen: BoardScreen?
    /// Each board's labels and people, by its ref, as `acta.board` last gave them.
    var info: [String: BoardInfo] = [:]
    /// The boards cards were made on here, the last first.
    var used: [String] = []
    /// The messages whose pills a change here redrew, for the instance to keep them so.
    fileprivate var again: [String] = []
    /// The person's watches on Acta, once `watch.list` gave them, for Follow.
    var watches: [ActaWatch]?
    fileprivate var watchesAsked = false
    fileprivate var asks: [String: ActaAsk] = [:]
}

extension Model {
    private func actaCmd(_ name: String, _ args: JSON, _ ask: ActaAsk) -> JSON {
        let id = id("app.acta")
        acta.asks[id] = ask
        return ["t": "cmd", "id": JSON(id), "name": JSON(name), "args": args]
    }

    /// Whether the person has Acta connected: the instance gave their boards.
    var hasActa: Bool { !acta.boards.isEmpty }

    /// The prefixes of the person's boards, which a typed card id becomes a piece for.
    var boardPrefixes: [String] { acta.boards.map(\.prefix) }

    /// The person's boards for a picker, each its ref and name: those cards were made on
    /// here first, the last first, then the rest in Acta's order.
    var boardChoices: [(board: String, name: String)] {
        var out = acta.used.filter { b in acta.boards.contains { $0.board == b } }
        for b in acta.boards where !out.contains(b.board) { out.append(b.board) }
        return out.map { ($0, boardName($0)) }
    }

    /// A board's name: as Acta names it, or its part after the workspace.
    func boardName(_ board: String) -> String {
        if let n = acta.names[board], !n.isEmpty { return n }
        return String(board.split(separator: "/").last ?? "")
    }

    /// Looks up a card pasted or typed in the composer (`card.preview`), unless it has
    /// been: `ref` is its id or its link.
    func previewCard(_ ref: String) -> JSON? {
        switch acta.previews[ref] {
        case .loading, .ready: return nil
        default: break
        }
        acta.previews[ref] = .loading
        return ["t": "cmd", "id": JSON(id("app.card") + "#" + ref), "name": "card.preview", "args": ["ref": JSON(ref)]]
    }

    /// What the cards need asked of the instance (PLAN.md, "Cards in messages"): the
    /// person's boards, every 10 minutes, for the composer's typed ids; each message of
    /// the shown chat, once it shows, whose cards were looked up over 5 minutes ago, and
    /// those whose pills a change here redrew; and the person's watches, once, while a
    /// card's sheet or a board is open, for Follow.
    func cardFetches(now: Int) -> [JSON] {
        var out: [JSON] = []
        if (acta.sheet != nil || acta.screen != nil) && !acta.watchesAsked {
            acta.watchesAsked = true
            out.append(actaCmd("watch.list", [:], .watches))
        }
        if synced, !acta.boardsAsking, acta.boardsAt.map({ now - $0 >= boardsFreshMs }) ?? true {
            acta.boardsAsking = true
            out.append(["t": "cmd", "id": JSON(id("app.acta_boards")), "name": "acta.boards", "args": [:]])
        }
        var stale: [String] = []
        for m in messages where !m.cards.isEmpty && acta.looked.insert(m.id).inserted {
            if m.cards.values.contains(where: { $0.stale(now) }) { stale.append(m.id) }
        }
        for id in acta.again where !stale.contains(id) { stale.append(id) }
        acta.again = []
        for k in stride(from: 0, to: stale.count, by: 100) {
            out.append(["t": "cmd", "id": JSON(id("app.cards_refresh")), "name": "cards.refresh", "args": ["messages": JSON(Array(stale[k..<min(k + 100, stale.count)]))]])
        }
        return out
    }

    /// A chat just opened: its cards are looked at again as it shows.
    func cardsChatOpened() {
        acta.looked = []
    }

    /// A card a message has, as its pill knows it.
    private func pillOf(_ board: String, _ id: String) -> ActaCard? {
        messages.lazy.compactMap { $0.cards[id] }.first { ($0.board == board || board.isEmpty) && !$0.missing }
    }

    /// The card a pill's link names, `loom-card:ID`, as the shown chat's messages have it.
    func pillCard(_ link: String) -> ActaCard? {
        guard link.hasPrefix(cardLink) else { return nil }
        return pillOf("", String(link.dropFirst(cardLink.count)))
    }

    // MARK: A card's preview

    /// Opens a card's preview sheet, from its pill or a board's row, and looks it up.
    func openCard(board: String, id: String) -> JSON {
        let known = pillOf(board, id).map { CardDetail(known: $0) }
        acta.sheet = CardSheet(board: board, id: id, detail: known)
        return actaCmd("card.get", ["board": JSON(board), "id": JSON(id)], .card(board: board, id: id))
    }

    /// Moves the card in the sheet to `status`: shown at once, and back as it was if Acta
    /// refuses.
    func moveCard(_ status: String) -> JSON? {
        guard var s = acta.sheet, var d = s.detail, d.card.status != status, cardStatuses.contains(status) else { return nil }
        if s.before == nil { s.before = d }
        d.card.status = status
        s.detail = d
        s.loading = true
        acta.sheet = s
        return actaCmd("card.update", ["board": JSON(s.board), "id": JSON(s.id), "status": JSON(status)], .move(board: s.board, id: s.id))
    }

    /// Ticks one of the sheet's subtasks, or takes its tick off.
    func toggleSubtask(_ subtask: String) -> JSON? {
        guard var s = acta.sheet, var d = s.detail, let k = d.checklist.firstIndex(where: { $0.id == subtask }) else { return nil }
        if s.before == nil { s.before = d }
        d.checklist[k].done.toggle()
        let done = d.checklist[k].done
        d.card.subtasksDone = d.checklist.filter(\.done).count
        s.detail = d
        s.loading = true
        acta.sheet = s
        return actaCmd("card.update", ["board": JSON(s.board), "id": JSON(s.id), "subtask": JSON(subtask), "done": JSON(done)], .move(board: s.board, id: s.id))
    }

    /// The pills of the messages shown that are this card, as it is now.
    private func patchPills(_ card: ActaCard) {
        for i in messages.indices {
            guard let had = messages[i].cards[card.id], had.board == card.board, had != card else { continue }
            messages[i].cards[card.id] = card
            if !acta.again.contains(messages[i].id) { acta.again.append(messages[i].id) }
        }
    }

    // MARK: Making a card

    /// Opens the form that makes a card from a message, filled from its words: the title
    /// its first line, the description all of it, the board the last used.
    func startCard(message: String, text: String) -> JSON? {
        let board = boardChoices.first?.board ?? ""
        acta.form = MakeForm(message: message, board: board, title: titleFrom(text), description: text.trimmingCharacters(in: .whitespacesAndNewlines))
        return askBoard(board)
    }

    /// Asks for a board's labels and people, unless they're known.
    private func askBoard(_ board: String) -> JSON? {
        guard !board.isEmpty, acta.info[board] == nil else { return nil }
        return actaCmd("acta.board", ["board": JSON(board)], .board)
    }

    /// The form's board: the labels picked go, as they were the old board's.
    func formBoard(_ board: String) -> JSON? {
        guard var f = acta.form else { return nil }
        if f.board != board {
            f.board = board
            f.labels = []
        }
        acta.form = f
        return askBoard(board)
    }

    /// Picks one of the board's labels on the form, or unpicks it.
    func toggleLabel(_ label: String) {
        guard var f = acta.form else { return }
        if let k = f.labels.firstIndex(of: label) { f.labels.remove(at: k) } else { f.labels.append(label) }
        acta.form = f
    }

    /// The form's size: `size`, or none when it's picked again.
    func formSize(_ size: String) {
        let had = acta.form?.size
        acta.form?.size = had == size ? nil : size
    }

    /// The form's priority: `priority`, or none when it's picked again.
    func formPriority(_ priority: String) {
        let had = acta.form?.priority
        acta.form?.priority = had == priority ? nil : priority
    }

    /// The labels the form offers: the board's.
    var formLabels: [String] {
        acta.form.flatMap { acta.info[$0.board]?.labels } ?? []
    }

    /// Whether Make makes the card, with `title` as typed: a board and a title, and
    /// nothing on its way.
    func canMake(_ title: String) -> Bool {
        guard let f = acta.form else { return false }
        let t = title.trimmingCharacters(in: .whitespacesAndNewlines)
        return !f.making && !f.board.isEmpty && !t.isEmpty && t.utf8.count <= 500
    }

    /// Makes the card, with the title and description as typed: kept with its message,
    /// which shows its pill once the instance says.
    func makeCard(title: String, description: String) -> JSON? {
        guard canMake(title), var f = acta.form else { return nil }
        f.making = true
        f.error = nil
        acta.form = f
        var args: [String: JSON] = ["board": JSON(f.board), "title": JSON(title.trimmingCharacters(in: .whitespacesAndNewlines)), "description": JSON(description),
                                    "labels": JSON(f.labels), "message": JSON(f.message)]
        if let size = f.size { args["size"] = JSON(size) }
        if let priority = f.priority { args["priority"] = JSON(priority) }
        acta.used.removeAll { $0 == f.board }
        acta.used.insert(f.board, at: 0)
        return actaCmd("card.create", .object(args), .create)
    }

    // MARK: A board

    /// Opens a board's screen, `board` or else the one last used, and asks for its open
    /// cards, its latest done, and its labels and people.
    func openBoard(_ board: String? = nil) -> [JSON] {
        guard let board = board ?? boardChoices.first?.board else { return [] }
        let status = acta.screen?.status ?? "todo"
        acta.screen = BoardScreen(board: board, status: status)
        var out = [
            actaCmd("acta.tasks", ["board": JSON(board), "limit": 200], .tasks(board: board, done: false)),
            actaCmd("acta.tasks", ["board": JSON(board), "status": ["done"], "limit": 30], .tasks(board: board, done: true)),
        ]
        if let b = askBoard(board) { out.append(b) }
        return out
    }

    /// The column the board's screen shows.
    func boardStatus(_ status: String) {
        acta.screen?.status = status
    }

    /// A card moved to another column from its row's menu: there at once, and back if Acta
    /// refuses.
    func moveTask(_ id: String, to status: String) -> JSON? {
        guard boardColumns.contains(status), var b = acta.screen, let k = b.tasks.firstIndex(where: { $0.id == id }), b.tasks[k].status != status else { return nil }
        b.moving.append((id, b.tasks[k].status))
        b.tasks[k].status = status
        b.error = nil
        acta.screen = b
        return actaCmd("card.update", ["board": JSON(b.board), "id": JSON(id), "status": JSON(status)], .move(board: b.board, id: id))
    }

    // MARK: Following (PLAN.md, "Watching outside events")

    /// Whether the person follows the board, or its card, in the chat shown: a watch on it
    /// posts there. Nil until their watches are known, or with no chat shown.
    func following(board: String, card: String? = nil) -> Bool? {
        guard let watches = acta.watches, let chat = conversation else { return nil }
        return watches.contains { $0.same(board, card ?? "", chat) }
    }

    /// Follow, or stop following, the board or its card in the chat shown: `watch.put`
    /// with this chat for its lines, or `watch.remove`. It shows at once, and goes back as
    /// it was when the instance refuses.
    func toggleFollow(board: String, card: String? = nil) -> JSON? {
        guard let following = following(board: board, card: card), let chat = conversation, var watches = acta.watches else { return nil }
        let card = card ?? ""
        if following {
            guard let at = watches.firstIndex(where: { $0.same(board, card, chat) }), !watches[at].id.isEmpty else { return nil }
            let w = watches.remove(at: at)
            acta.watches = watches
            return actaCmd("watch.remove", ["id": JSON(w.id)], .unfollow(w))
        }
        let w = ActaWatch(id: "", board: board, card: card, conversation: chat)
        watches.append(w)
        acta.watches = watches
        var target: [String: JSON] = ["board": JSON(board)]
        if !card.isEmpty { target["card"] = JSON(card) }
        return actaCmd("watch.put", ["target": .object(target), "conversation": JSON(chat)], .follow(w))
    }

    /// `watch.updated` and `watch.removed`, from this device or another.
    func takeWatchEvent(_ kind: String, _ f: JSON) {
        guard var watches = acta.watches else { return }
        if kind == "watch.removed" {
            let id = f["watch"].text
            watches.removeAll { $0.id == id }
        } else if let w = ActaWatch(f["watch"]) {
            // Made here: it takes the place of the one shown while it was made.
            watches.removeAll { $0.id == w.id || ($0.id.isEmpty && $0.same(w.board, w.card, w.conversation)) }
            watches.append(w)
        }
        acta.watches = watches
    }

    /// Shows the instance's words for a refused Follow on the sheet or the board open.
    private func actaError(_ error: String) {
        if acta.sheet != nil { acta.sheet?.error = error } else if acta.screen != nil { acta.screen?.error = error } else { notice = error }
    }

    // MARK: Answers

    /// The answers to what the cards, the sheets and the board asked. Gives whether `f`
    /// was one.
    func actaResult(_ f: JSON) -> Bool {
        let id = f["id"].text
        let error = f["error"].isNull ? nil : (f["error"]["message"].string ?? f["error"]["code"].text)
        let value = f["value"]
        if id.hasPrefix("app.acta_boards") {
            acta.boardsAsking = false
            acta.boardsAt = nowMs()
            if error == nil {
                let list = value["boards"].array
                acta.boards = list.map { (prefix: $0["prefix"].text, board: $0["board"].text) }.filter { !$0.prefix.isEmpty }
                acta.names = Dictionary(list.map { ($0["board"].text, $0["name"].text) }, uniquingKeysWith: { a, _ in a })
            }
            return true
        }
        if id.hasPrefix("app.card_"), let hash = id.firstIndex(of: "#") {
            let ref = String(id[id.index(after: hash)...])
            acta.previews[ref] = error == nil && !value["card"].isNull ? .ready(ActaCard(value["card"])) : .failed
            return true
        }
        if id.hasPrefix("app.cards_refresh") { return true }
        guard let ask = acta.asks.removeValue(forKey: id) else { return false }
        switch ask {
        case .board:
            let b = value["board"]
            if error == nil, !b.isNull {
                acta.info[b["board"].text] = BoardInfo(board: b["board"].text, name: b["name"].text, prefix: b["prefix"].text, labels: b["labels"].array.compactMap(\.string), people: b["people"].array.compactMap(\.string))
            }
        case .tasks(let board, let done):
            guard var b = acta.screen, b.board == board else { break }
            b.waiting = max(b.waiting - 1, 0)
            if let error { b.error = error }
            let tasks = value["tasks"].array.map(BoardTask.init)
            // The open first, then the done, whichever came first.
            b.tasks.removeAll { ($0.status == "done") == done }
            b.tasks = done ? b.tasks + tasks : tasks + b.tasks
            acta.screen = b
        case .card(let board, let id), .move(let board, let id):
            let detail = error == nil && !value["card"].isNull ? CardDetail(value["card"]) : nil
            if let detail { patchPills(detail.card) }
            if var s = acta.sheet, s.board == board, s.id == id {
                s.loading = false
                if let detail {
                    s.detail = detail
                    s.before = nil
                    s.error = nil
                } else {
                    if let before = s.before { s.detail = before }
                    s.before = nil
                    s.error = error
                }
                acta.sheet = s
            }
            if var b = acta.screen, b.board == board {
                let was = b.moving.firstIndex { $0.id == id }.map { b.moving.remove(at: $0).was }
                if let k = b.tasks.firstIndex(where: { $0.id == id }) {
                    if let detail {
                        b.tasks[k].status = detail.card.status
                        b.tasks[k].title = detail.card.title
                    } else if let was {
                        // Acta refused the move: back where it was.
                        b.tasks[k].status = was
                        b.error = error
                    }
                }
                acta.screen = b
            }
        case .create:
            guard var form = acta.form else { break }
            form.making = false
            if error == nil, !value["card"].isNull {
                // Made: the form goes, and the message shows its pill.
                acta.form = nil
            } else {
                form.error = error ?? "Acta didn't answer."
                acta.form = form
            }
        case .watches:
            if error == nil {
                acta.watches = value["watches"].array.compactMap(ActaWatch.init)
            } else {
                // Asked again with the next sheet or board.
                acta.watchesAsked = false
            }
        case .follow(let asked):
            if var watches = acta.watches, let k = watches.firstIndex(where: { $0.id.isEmpty && $0 == asked }) {
                // Its watch.updated, which comes first, may have put it in already.
                if error == nil, let w = ActaWatch(value["watch"]) { watches[k] = w } else { watches.remove(at: k) }
                acta.watches = watches
            }
            if let error { actaError(error) }
        case .unfollow(let w):
            if let error {
                acta.watches?.append(w)
                actaError(error)
            }
        }
        return true
    }
}
