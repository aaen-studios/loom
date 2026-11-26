import Foundation

/// A watch's line in a chat (PLAN.md, "Watching outside events"): a `loom` message's
/// `update` block, what happened outside loom, as desktop/src/model/watch.rs reads it.
struct WatchLine: Equatable, Sendable {
    /// `acta` or `t3`.
    var integration: String
    /// From Acta: the card, its title, and what happened to it.
    var card: String
    /// The card's title, or the thread's.
    var title: String
    var event: String
    /// The status a card moved to.
    var to: String
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
        card = b["card"].text
        title = b["title"].text
        event = b["event"].text
        to = b["to"].text
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

    /// The status its card's dot shows: the one it moved to, or none.
    var status: String { event == "moved" ? to : "" }
}

extension Message {
    /// Its watch line, when it's one: a `loom` message with an `update` block.
    var watchLine: WatchLine? {
        role == "loom" ? blocks.first?.update : nil
    }
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
