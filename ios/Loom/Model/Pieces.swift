import Foundation

// What the composer holds besides words (PLAN.md, "Mentions" and "Links in the
// composer"), as desktop/src/model.rs has it: each piece one unit in the words, with an
// id and its text, sent as the chat will show it.

/// How a piece's id starts, then what it is.
enum PieceKind {
    static let attach = "loom-attach:"
    static let link = "loom-link:"
}

/// A piece in the composer: an agent or a file mentioned (its `loom:` link), a file
/// attached (`loom-attach:` and its attachment's id) or a link pasted as a preview
/// (`loom-link:` and its address), with its text: the mention's words, the file's whole
/// name or the address.
struct Piece: Equatable, Sendable {
    var id: String
    var text: String
}

/// An attached file as a piece.
func attachPiece(_ a: Attachment) -> Piece {
    Piece(id: PieceKind.attach + a.id, text: a.name)
}

/// A link pasted as a preview as a piece.
func linkPiece(_ url: String) -> Piece {
    Piece(id: PieceKind.link + url, text: url)
}

/// What the person wrote, with each piece as it's sent, as desktop/src/model.rs's
/// `with_mention_links`: a mention as its Markdown link, a file as its name in brackets,
/// which the chat makes a link to it, and a link pasted as a preview as its bare
/// address. An address written or pasted as plain text goes as `<https://…>`.
func composerText(_ parts: [Either]) -> String {
    var out = ""
    for part in parts {
        switch part {
        case .words(let w):
            out += plainLinks(w)
        case .piece(let p):
            if p.id.hasPrefix(PieceKind.attach) {
                out += "[\(p.text)]"
            } else if p.id.hasPrefix(PieceKind.link) {
                out += String(p.id.dropFirst(PieceKind.link.count))
            } else {
                let words = p.text.replacingOccurrences(of: "[", with: "").replacingOccurrences(of: "]", with: "")
                out += "[\(words)](\(p.id))"
            }
        }
    }
    return out
}

/// A stretch of the composer: words, or a piece.
enum Either: Equatable, Sendable {
    case words(String)
    case piece(Piece)
}

/// The length of the code, Markdown link or `<…>` at the start of `s`, which a bare
/// address isn't looked for in.
private func skipped(_ s: Substring) -> Substring.Index? {
    if s.first == "`" {
        let after = s.dropFirst()
        guard let end = after.firstIndex(of: "`") else { return s.endIndex }
        return s.index(after: end)
    }
    if s.first == "[", let close = s.range(of: "]("), !s[..<close.lowerBound].contains("\n"),
       let end = s[close.upperBound...].firstIndex(of: ")") {
        return s.index(after: end)
    }
    if s.hasPrefix("<http://") || s.hasPrefix("<https://") {
        return s.firstIndex(of: ">").map { s.index(after: $0) }
    }
    return nil
}

/// The bare address at the start of `s`, without what usually ends a sentence after it.
private func bareLink(_ s: Substring) -> Substring? {
    guard s.hasPrefix("https://") || s.hasPrefix("http://") else { return nil }
    let end = s.firstIndex { $0.isWhitespace || "<>\"`".contains($0) } ?? s.endIndex
    var url = s[..<end]
    while let last = url.last, ".,;:!?)'".contains(last) { url = url.dropLast() }
    return url.count > "https://".count ? url : s[..<end]
}

/// Calls `f` for each piece of `text` outside code blocks: a bare address, or what isn't
/// one.
private func eachLine(_ text: String, _ f: (Substring, Bool) -> Void) {
    var fenced = false
    for (n, line) in text.components(separatedBy: "\n").enumerated() {
        if n > 0 { f("\n", false) }
        let t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("```") || t.hasPrefix("~~~") {
            fenced.toggle()
            f(Substring(line), false)
            continue
        }
        if fenced {
            f(Substring(line), false)
            continue
        }
        var rest = Substring(line)
        while let c = rest.first {
            if let end = skipped(rest) {
                f(rest[..<end], false)
                rest = rest[end...]
                continue
            }
            if let url = bareLink(rest) {
                f(url, true)
                rest = rest[url.endIndex...]
                continue
            }
            f(Substring(String(c)), false)
            rest = rest.dropFirst()
        }
    }
}

/// What the person wrote with each bare address as plain text, `<https://…>`, so it
/// shows as written: only a link pasted as a preview goes bare, as desktop/src/model.rs's
/// `plain_links`.
func plainLinks(_ text: String) -> String {
    var out = ""
    eachLine(text) { piece, isLink in
        out += isLink ? "<\(piece)>" : String(piece)
    }
    return out
}

/// The address a whole paste is, when it's one http or https address and nothing else.
func pastedLink(_ text: String) -> String? {
    let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let url = bareLink(Substring(t)), url.count == t.count, !t.contains(where: \.isWhitespace) else { return nil }
    return String(url)
}

/// The site an address is on, as it's shown: its host without `www.`.
func siteOf(_ url: String) -> String {
    let host = URL(string: url)?.host()?.lowercased() ?? ""
    return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
}

/// A file's name cut in the middle to at most `most` characters, keeping its start and
/// its end with its type, as desktop/src/model.rs's `middle_cut`.
func middleCut(_ name: String, _ most: Int = 28) -> String {
    let chars = Array(name)
    guard chars.count > most, most >= 5 else { return name }
    var ext = 0
    if let dot = chars.lastIndex(of: ".") {
        let n = chars.count - dot
        if n > 1 && n <= 8 { ext = n }
    }
    let tail = min(ext + 5, most / 2)
    let head = most - 1 - tail
    let start = String(chars[..<head]).trimmingCharacters(in: .whitespaces)
    let end = String(chars[(chars.count - tail)...]).trimmingCharacters(in: .whitespaces)
    return "\(start)…\(end)"
}

/// A message's words with the name of each file it links to (`loom-file:` or
/// `loom:file/`) cut in the middle (`middleCut`), so a long one doesn't fill the line.
func cutFileNames(_ text: String) -> String {
    guard text.contains("](loom") else { return text }
    var out = ""
    var rest = Substring(text)
    while let open = rest.firstIndex(of: "[") {
        out += rest[..<open]
        let after = rest[open...]
        guard let close = after.range(of: "]("), !after[..<close.lowerBound].contains("\n"),
              let end = after[close.upperBound...].firstIndex(of: ")") else {
            out += "["
            rest = after.dropFirst()
            continue
        }
        let words = after[after.index(after: open)..<close.lowerBound]
        let url = after[close.upperBound..<end]
        if url.hasPrefix("loom-file:") || url.hasPrefix("loom:file/") {
            out += "[\(middleCut(String(words)))](\(url))"
        } else {
            out += after[...end]
        }
        rest = after[after.index(after: end)...]
    }
    out += rest
    return out
}
