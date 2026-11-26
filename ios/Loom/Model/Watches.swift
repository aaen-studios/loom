import Foundation

/// A watch's line in a chat (PLAN.md, "Watching outside events"): a `loom` message's
/// `update` block, what happened outside loom, as desktop/src/model/watch.rs reads it.
struct WatchLine: Equatable, Sendable {
    /// `acta` or `t3`.
    var integration: String
    /// From Acta: the board, the card, its title, and what happened to it.
    var board: String
    var card: String
    /// The card's title, or the thread's.
    var title: String
    var event: String
    /// The status a card moved to.
    var to: String
    /// The card's status when it happened, where the line says it.
    var cardStatus: String
    /// What happened, in words: "moved to Done", "Thread finished".
    var said: String
    var by: String
    /// The card's page in Acta.
    var url: String
    /// From T3: the thread, and how its run ended.
    var thread: String
    var state: String
    /// The whole line, for what's drawn of none of the rest.
    var text: String

    init(_ b: JSON) {
        integration = b["integration"].text
        board = b["board"].text
        card = b["card"].text
        title = b["title"].text
        event = b["event"].text
        to = b["to"].text
        cardStatus = b["status"].text
        said = b["said"].text
        by = b["by"].text
        url = b["url"].text
        thread = b["thread"].text
        state = b["state"].text
        text = b["text"].text
    }

    /// What's at its start: the board's tile, T3's strand as the run ended, or nothing
    /// for a line from something the app doesn't know.
    enum Mark: Equatable {
        case board
        case strand(Lie)
        case none
    }

    /// How T3's strand lies: straight once the run finished, sagging once it failed, and
    /// slack and quiet once it was stopped.
    enum Lie: Equatable {
        case straight, sags, slack
    }

    var mark: Mark {
        switch integration {
        case "acta" where !card.isEmpty: return .board
        case "t3":
            switch state {
            case "completed": return .strand(.straight)
            case "failed": return .strand(.sags)
            default: return .strand(.slack)
            }
        default: return .none
        }
    }

    /// The thread's title, for a line from T3: its id when it has none.
    var threadTitle: String {
        let t = title.trimmingCharacters(in: .whitespaces)
        return t.isEmpty ? thread : t
    }

    /// What happened and who, after the card or the title, in quiet ink: "moved to Done ·
    /// Elliot", "Thread finished". A line the app draws none of the rest of is its words.
    var words: String {
        if mark == .none || said.isEmpty { return text }
        if by.isEmpty || integration != "acta" { return said }
        return "\(said) · \(by)"
    }

    /// What VoiceOver reads of it.
    var spoken: String {
        switch mark {
        case .board: return "\(card) \(words)"
        case .strand: return "\(words) · \(threadTitle)"
        case .none: return text
        }
    }

    /// The status the line is about, which its card's dot shows: where a move went, or for
    /// a comment or a new card the card's status then, where the line says it. Nil when it
    /// doesn't: the phone doesn't look the card up, so the pill has no dot then.
    var status: String? {
        let then = (event == "moved" ? to : cardStatus).trimmingCharacters(in: .whitespaces)
        return then.isEmpty ? nil : then
    }
}

extension Message {
    /// Its watch line, when it's one: a `loom` message with an `update` block.
    var watchLine: WatchLine? {
        role == "loom" ? blocks.first?.update : nil
    }
}

/// One of the person's watches, as `watch.list` gives it (desktop/src/model/watch.rs's
/// `Watch`): what it's on, the chat its lines go in, and what it wakes.
struct Watch: Equatable, Sendable {
    /// "" while Follow's `watch.put` is under way.
    var id: String
    /// `acta` or `t3`.
    var integration: String
    var board: String
    /// "" for the whole board.
    var card: String
    var thread = ""
    var threadTitle = ""
    /// The chat its lines go in, or "".
    var conversation: String
    /// What it wakes: an agent, a routine (the routine's own), or neither.
    var wakeAgent = ""
    var wakeRoutine = ""
    /// Why it couldn't look, from its third failure in a row.
    var error = ""

    init?(_ v: JSON) {
        guard !v["id"].text.isEmpty else { return nil }
        id = v["id"].text
        let i = v["integration"].text
        integration = i.isEmpty ? "acta" : i
        board = v["target"]["board"].text
        card = v["target"]["card"].text
        thread = v["target"]["thread"].text
        threadTitle = v["target"]["title"].text
        conversation = v["conversation"].text
        wakeAgent = v["wake"]["agent"].text
        wakeRoutine = v["wake"]["routine"].text
        error = v["error"].text
    }

    /// Follow's, on Acta's `board` or its `card`, while `watch.put` makes it.
    init(id: String, board: String, card: String, conversation: String) {
        self.id = id
        integration = "acta"
        self.board = board
        self.card = card
        self.conversation = conversation
    }

    /// Whether it's the one Follow makes for `board`, or its `card`, in `conversation`: on
    /// Acta, and no routine's.
    func same(_ board: String, _ card: String, _ conversation: String) -> Bool {
        integration == "acta" && wakeRoutine.isEmpty && self.board == board && self.card == card && self.conversation == conversation
    }

    /// What it's on, as its pill in Following.
    var pill: FollowPill {
        if integration == "t3" {
            let t = threadTitle.trimmingCharacters(in: .whitespaces)
            return .thread(t.isEmpty ? thread : t)
        }
        return card.isEmpty ? .board(board) : .card(id: card, board: board)
    }
}

/// What a watch in Following is on.
enum FollowPill: Equatable, Sendable {
    /// A board, by its ref.
    case board(String)
    /// A card, by its id, on its board.
    case card(id: String, board: String)
    /// A T3 thread: its title, or its id when it has none.
    case thread(String)

    /// A card's page, which its pill is looked up by.
    var link: String? {
        guard case .card(let id, let board) = self else { return nil }
        return "https://\(actaHost)/\(board)#\(id)"
    }
}

/// One row of Following: the watch, its pill, and why it couldn't look, in a line.
struct Followed: Equatable, Identifiable, Sendable {
    var watch: String
    var pill: FollowPill
    var error: String?
    var id: String { watch }

    init(_ w: Watch) {
        watch = w.id
        pill = w.pill
        error = w.error.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty }
    }
}

/// Whose Following it is: an agent's, or a group chat's.
enum Follower: Equatable, Sendable {
    case agent(String)
    case room(String)
}

/// Whether a message the core gives is a watch's line, which the chat shows, unlike loom's
/// other lines across the chat (a compaction, a fresh session), which are the desktop's.
func isWatchLine(_ m: JSON) -> Bool {
    m["role"].text == "loom" && m["blocks"].array.first?["kind"].text == "update"
}

/// The lines that sit tight under the line before them, as the desktop stacks them: each
/// line just after another, with no time line between (`Message.at` within half an hour).
func stackedLines(_ messages: [Message]) -> Set<String> {
    var out = Set<String>()
    for (i, m) in messages.enumerated() where i > 0 && m.watchLine != nil {
        let before = messages[i - 1]
        if before.watchLine != nil && m.at - before.at < 30 * 60 * 1000 {
            out.insert(m.id)
        }
    }
    return out
}
