import Foundation

// A draft card (PLAN.md, "Drafts"): an email or a message an agent proposes, which the
// person sends, edits or rejects, and which loom sends as them, as
// desktop/src/model/draft_card.rs works it out. The whole of what goes out is on the
// card; once sent it's one line, with Undo while it can still be taken back.

/// The id a draft's answer goes as, before its answer and `message/card`.
let draftAnswer = "app.draft:"

/// A row of a draft: its field, its label, what it says, and whether Edit changes it.
struct DraftRow: Equatable, Sendable {
    var key: String
    var label: String
    var value: String
    var edit: Bool
}

/// A draft as the instance keeps it on its card.
struct Draft: Equatable, Sendable {
    var kind: String
    var service: String
    var thread: String
    var answers: String
    var rows: [DraftRow]
    var body: String
    var state: String
    var at: Int
    var until: Int
    var why: String
    var edited: Bool

    init(_ c: JSON) {
        kind = c["kind"].text
        service = c["service"].text
        thread = c["thread"].text
        answers = c["answers"].text
        rows = c["rows"].array.map { DraftRow(key: $0["key"].text, label: $0["label"].text, value: $0["value"].text, edit: $0["edit"].bool ?? false) }
        body = c["body"].text
        state = c["state"].text
        at = c["at"].int ?? 0
        until = c["until"].int ?? 0
        why = c["why"].text
        edited = c["edited"].bool ?? false
    }

    /// The mark the app has for its service (`mark-gmail`, from server/catalog), or none.
    var mark: String? { ["gmail", "discord", "github"].contains(service) ? service : nil }

    /// The service's name as the From row says it after the account: "(Gmail)".
    var serviceName: String {
        ["gmail": "Gmail", "outlook": "Outlook", "slack": "Slack", "discord": "Discord", "github": "GitHub", "twilio": "Twilio"][service] ?? ""
    }

    /// Its line once it's gone or going: the subject, or the start of the body, and
    /// where it goes.
    var line: (what: String, to: String) {
        let row = { (k: String) in rows.first { $0.key == k }?.value ?? "" }
        var what = row("subject")
        if what.isEmpty {
            let flat = body.split(whereSeparator: \.isWhitespace).joined(separator: " ")
            what = flat.count > 60 ? String(flat.prefix(60)) + "…" : flat
        }
        return (what, row("to"))
    }

    /// The rows the card shows: and while it's edited, an empty Cc to fill in for an email.
    func rowsShown(editing: Bool) -> [DraftRow] {
        var out = rows
        if editing && kind == "email" && !out.contains(where: { $0.key == "cc" }) {
            let at = (out.firstIndex { $0.key == "to" }).map { $0 + 1 } ?? out.count
            out.insert(DraftRow(key: "cc", label: "Cc", value: "", edit: true), at: at)
        }
        return out
    }

    /// What it says at `now` (milliseconds since 1970).
    func shown(now: Int) -> DraftShown {
        let (what, to) = line
        switch state {
        case "sending" where until > now: return .going(what: what, to: to, at: at, left: (until - now + 999) / 1000)
        case "sending": return .sending(what: what, to: to)
        case "sent": return .sent(what: what, to: to, at: at)
        case "later": return .later(what: what, to: to, at: at)
        case "failed": return .failed(what: what, to: to, why: why)
        case "rejected": return .rejected(what: what)
        default: return .open
        }
    }
}

/// What a draft card says now.
enum DraftShown: Equatable {
    case open
    case going(what: String, to: String, at: Int, left: Int)
    case sending(what: String, to: String)
    case sent(what: String, to: String, at: Int)
    case later(what: String, to: String, at: Int)
    case failed(what: String, to: String, why: String)
    case rejected(what: String)
}

/// Send later's times from `now`: in an hour, in three hours, and tomorrow at 08:00,
/// each its words and when, in milliseconds.
func laterTimes(from now: Date, calendar: Calendar = .current) -> [(words: String, at: Int)] {
    let ms = { (d: Date) in Int(d.timeIntervalSince1970 * 1000) }
    var out = [("In 1 hour", ms(now) + 3_600_000), ("In 3 hours", ms(now) + 3 * 3_600_000)]
    if let tomorrow = calendar.date(byAdding: .day, value: 1, to: now),
       let eight = calendar.date(bySettingHour: 8, minute: 0, second: 0, of: tomorrow) {
        out.append(("Tomorrow at 08:00", ms(eight)))
    }
    return out.map { (words: $0.0, at: $0.1) }
}

extension Model {
    /// The person's answer on a draft: `send` (with `edits`, by field, when it was
    /// edited), `later` (at `at`), `reject` (with `note`), `undo` or `retry`.
    func answerDraft(message: String, card: String, answer: String, edits: [String: String]? = nil, note: String = "", at: Int = 0) -> JSON {
        let key = "\(message)/\(card)"
        draftErrors[key] = nil
        draftSending.insert(key)
        var args: [String: JSON] = ["message": JSON(message), "card": JSON(card), "answer": JSON(answer)]
        if let edits { args["edits"] = .object(edits.mapValues { JSON($0) }) }
        let n = note.trimmingCharacters(in: .whitespacesAndNewlines)
        if answer == "reject" && !n.isEmpty { args["note"] = JSON(n) }
        if answer == "later" { args["at"] = JSON(at) }
        return ["t": "cmd", "id": JSON("\(draftAnswer)\(answer):\(key)"), "name": "draft.answer", "args": .object(args)]
    }

    /// A draft's answer came back: a refusal shows on the card, which stays as it was;
    /// once it's taken, the card's own message.updated says how it stands.
    func takeDraftAnswer(_ id: String, error: String?) {
        let rest = id.dropFirst(draftAnswer.count)
        let key = rest.split(separator: ":", maxSplits: 1).last.map(String.init) ?? String(rest)
        draftSending.remove(key)
        draftErrors[key] = error
    }
}
