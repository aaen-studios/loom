import Foundation

/// An emoji as the core gives it, for the suggestion row and the picker: built-in, as
/// itself in the person's skin tone, or one of the person's own, as its token.
struct EmojiItem: Equatable, Hashable, Sendable {
    /// What it's sent as: the character, or `<:name:id>`.
    var emoji: String
    /// Its names, the first its own.
    var names: [String]
    /// The name that matched what's typed.
    var name: String
    /// One of the person's own: its id.
    var custom: String?
    var animated = false
    /// It has skin tones.
    var tones = false

    init(_ j: JSON) {
        emoji = j["emoji"].text
        names = j["names"].array.compactMap(\.string)
        name = j["name"].string ?? names.first ?? ""
        custom = j["custom"].string
        animated = j["animated"].bool ?? false
        tones = j["tones"].bool ?? false
    }
}

/// One of the picker's groups: Recently used, Your emoji, then Unicode's.
struct EmojiGroup: Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var emoji: [EmojiItem]
}

/// One of the person's own, as the instance keeps it.
struct CustomEmoji: Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var file: String
    var animated: Bool

    /// How a message writes it.
    var token: String { "<\(animated ? "a" : ""):\(name):\(id)>" }
}

/// The suggestion row over the composer while a name is typed after a colon: what a pick
/// takes the place of at the cursor (`:so`), and the emoji.
struct EmojiRow: Equatable, Sendable {
    var suffix: String
    var items: [EmojiItem]
}

/// What the composer's words should become at the cursor: `suffix` there goes, and
/// `with` takes its place.
struct EmojiReplace: Equatable, Sendable {
    var suffix: String
    var with: String
}

/// The app's emoji (PLAN.md, "Emoji"), as desktop/src/emoji.rs's `Emojis`: the person's
/// own, the suggestion row, the picker's groups and search, and where each of the
/// person's own pictures is.
@MainActor
@Observable
final class Emojis {
    /// The skin tone picked, 0 for none and 1 to 5 light to dark.
    var tone = 0
    var own: [CustomEmoji] = []
    /// How many one person may have.
    var most = 200
    var row: EmojiRow?
    /// A replacement the core said for what was typed, for the composer to make.
    var replace: EmojiReplace?
    var groups: [EmojiGroup] = []
    /// The picker's search: what was asked, and what it found.
    var found: (query: String, items: [EmojiItem])?
    /// Where each of the person's own pictures is, once fetched, by its id.
    var paths: [String: String] = [:]
    /// What went wrong adding, renaming or deleting one, to show.
    var error: String?
    /// The one just deleted, for its Undo.
    var deleted: CustomEmoji?
    /// A picture going up to be one of the person's own: the name it gets.
    var uploading: String?
    @ObservationIgnored private var fetching: Set<String> = []
    @ObservationIgnored private var stale = false
    @ObservationIgnored private var asked = 0
    @ObservationIgnored private var pickerTone: Int?
    /// Frames to send once something the core said comes: an emoji to add once its
    /// picture went up.
    @ObservationIgnored private var outbox: [JSON] = []
    /// The picker is up (`openPicker`, `shutPicker`).
    @ObservationIgnored private var picking = false
    /// Recently used as the open picker first showed it, held until it shuts, so an emoji
    /// tapped doesn't move from under the finger. It opens with the new order next time.
    /// Nil until its groups come.
    @ObservationIgnored private var held: [EmojiItem]?

    /// The picker opens: its groups, unless they're here in this skin tone. Tapping in it
    /// keeps it up, and Recently used keeps the order it opened with until it shuts.
    func openPicker() -> JSON? {
        picking = true
        let ask = picker()
        held = ask == nil ? recent : nil
        return ask
    }

    func shutPicker() {
        picking = false
        held = nil
    }

    private var recent: [EmojiItem] { groups.first { $0.id == "recent" }?.emoji ?? [] }

    /// Recently used while the picker is up: in the order it opened with, each emoji as
    /// the groups now have it (in a new skin tone, say), and none tapped since. The first
    /// groups to come after it opens set that order.
    private func holdRecent() {
        guard picking else { return }
        guard let held else {
            self.held = recent
            return
        }
        let all = groups.flatMap(\.emoji)
        let now = held.map { was in all.first { sameEmoji($0, was) } ?? was }
        if let i = groups.firstIndex(where: { $0.id == "recent" }) { groups[i].emoji = now }
        // These aren't the core's groups as they are: the next opening asks again.
        pickerTone = nil
    }

    /// What's typed up to the cursor, for the suggestion row or a replacement.
    func typed(_ before: String, emoticons: Bool) -> JSON {
        asked += 1
        return ["t": "emoji.typed", "id": JSON("app.emoji.typed:\(asked)"), "text": JSON(before), "tone": JSON(tone), "emoticons": JSON(emoticons)]
    }

    /// Nothing typed is a name now: the row goes, and any answer still coming is late.
    func untyped() {
        asked += 1
        row = nil
    }

    func find(_ query: String) -> JSON {
        ["t": "emoji.find", "id": JSON("app.emoji.find:\(query)"), "query": JSON(query), "tone": JSON(tone), "limit": 120]
    }

    /// The picker's groups, unless they're here in this skin tone.
    func picker(again: Bool = false) -> JSON? {
        if !again && pickerTone == tone && !groups.isEmpty { return nil }
        pickerTone = tone
        return ["t": "emoji.picker", "id": "app.emoji.picker", "tone": JSON(tone)]
    }

    func used(_ emoji: String) -> JSON {
        // Recently used changes: the picker reads its groups again when it opens.
        pickerTone = nil
        return ["t": "emoji.used", "emoji": JSON(emoji)]
    }

    /// Fetches one of the person's own pictures, or anyone's in the chat, once.
    func fetch(_ id: String) -> JSON? {
        if paths[id] != nil || fetching.contains(id) { return nil }
        fetching.insert(id)
        return ["t": "emoji.file", "id": JSON("app.emoji.file:\(id)"), "emoji": JSON(id)]
    }

    /// Sends a picture to be one of the person's own, named `name`.
    func upload(path: String, name: String) -> JSON {
        error = nil
        uploading = name
        return ["t": "file.upload", "id": JSON("app.emoji.upload:\(name)"), "path": JSON(path)]
    }

    func rename(_ id: String, to name: String) -> JSON {
        ["t": "cmd", "id": "app.emoji.cmd:rename", "name": "emoji.rename", "args": ["emoji": JSON(id), "name": JSON(name)]]
    }

    func delete(_ id: String) -> JSON {
        deleted = own.first { $0.id == id }
        return ["t": "cmd", "id": "app.emoji.cmd:delete", "name": "emoji.delete", "args": ["emoji": JSON(id)]]
    }

    /// Undoes the delete just made.
    func restore() -> JSON? {
        guard let gone = deleted else { return nil }
        deleted = nil
        return ["t": "cmd", "id": "app.emoji.cmd:restore", "name": "emoji.restore", "args": ["emoji": JSON(gone.id)]]
    }

    /// The frames to send for what changed, and the pictures of the person's own that
    /// `wanted` names and aren't here.
    func fetches(wanted: [String]) -> [JSON] {
        var out = outbox
        outbox = []
        if stale {
            stale = false
            out.append(["t": "core.get", "id": "app.emoji.list", "what": "emoji"])
        }
        for id in Set(wanted + own.map(\.id)) {
            if let f = fetch(id) { out.append(f) }
        }
        return out
    }

    /// Takes what the core said of emoji. Gives whether it was this.
    func take(_ f: JSON) -> Bool {
        let id = f["id"].text
        switch f["t"].text {
        case "synced":
            stale = true
            return false
        case "event" where f["kind"].text == "emoji.changed":
            setOwn(f["emoji"])
            pickerTone = nil
            return true
        case "core.data" where f["what"].text == "emoji":
            setOwn(f["items"])
            return true
        case "core.file" where id.hasPrefix("app.emoji.upload:"):
            let name = String(id.dropFirst("app.emoji.upload:".count))
            if let file = f["file"]["file"].string {
                outbox.append(["t": "cmd", "id": "app.emoji.cmd:add", "name": "emoji.add", "args": ["file": JSON(file), "name": JSON(name)]])
            } else {
                uploading = nil
                error = f["error"]["message"].string ?? "The picture didn't go up."
            }
            return true
        case "core.file" where id.hasPrefix("app.emoji.file:"):
            let emoji = String(id.dropFirst("app.emoji.file:".count))
            fetching.remove(emoji)
            if let path = f["path"].string { paths[emoji] = path }
            return true
        case "result" where id.hasPrefix("app.emoji.cmd:"):
            if id == "app.emoji.cmd:add" { uploading = nil }
            error = f["error"]["message"].string
            return true
        case "core.emoji":
            if id.hasPrefix("app.emoji.typed:") {
                // Only the answer to what was typed last.
                guard id == "app.emoji.typed:\(asked)" else { return true }
                if !f["replace"].isNull {
                    row = nil
                    replace = EmojiReplace(suffix: f["replace"]["suffix"].text, with: f["replace"]["with"].text)
                    return true
                }
                let items = f["items"].array.map(EmojiItem.init)
                row = items.isEmpty ? nil : EmojiRow(suffix: f["suffix"].text, items: items)
            } else if id == "app.emoji.picker" {
                groups = f["groups"].array.map { g in
                    EmojiGroup(id: g["id"].text, name: g["name"].text, emoji: g["emoji"].array.map(EmojiItem.init))
                }
                holdRecent()
            } else if id.hasPrefix("app.emoji.find:") {
                found = (String(id.dropFirst("app.emoji.find:".count)), f["items"].array.map(EmojiItem.init))
            }
            return true
        default:
            return false
        }
    }

    private func setOwn(_ list: JSON) {
        own = list.array.map { CustomEmoji(id: $0["id"].text, name: $0["name"].text, file: $0["file"].text, animated: $0["animated"].bool ?? false) }
        if let gone = deleted, own.contains(where: { $0.id == gone.id }) { deleted = nil }
    }
}

/// Whether two of the picker's emoji are the same one, whatever their skin tone: one of
/// the person's own by its id.
func sameEmoji(_ a: EmojiItem, _ b: EmojiItem) -> Bool {
    if a.custom != nil || b.custom != nil { return a.custom == b.custom }
    let untoned = { (s: String) in s.unicodeScalars.filter { !(0x1F3FB...0x1F3FF).contains($0.value) && $0.value != 0xFE0F } }
    return untoned(a.emoji) == untoned(b.emoji)
}

/// A name for one of the person's own from what the person typed, as the instance takes
/// it: lowercase, a to z, 0 to 9 and _, at most 32.
func emojiName(_ typed: String) -> String {
    let lower = typed.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: ": "))
    var out = ""
    for c in lower.unicodeScalars {
        if ("a"..."z").contains(c) || ("0"..."9").contains(c) || c == "_" {
            out.unicodeScalars.append(c)
        } else if c == " " || c == "-" {
            out += "_"
        }
    }
    return String(out.prefix(32))
}
