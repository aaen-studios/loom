import SwiftUI
import UIKit

/// Emoji as the app draws them (PLAN.md, "Emoji"), as desktop/src/emoji.rs does: built-in
/// ones with Twemoji's pictures, which scripts/ios.calx puts in an asset catalog that keeps
/// their vectors, and the person's own from their pictures. What an emoji is called, the
/// shortcodes and the picker come from the core (`shared/emoji.calx`); this only finds
/// and draws them.
enum Emoji {
    /// How big an emoji is drawn among words, against the text's size.
    static let inText: CGFloat = 1.25
    /// How big a message of only one to three emoji draws them.
    static let jumboSide: CGFloat = 48
    /// How a message's words name one of the person's own, once `linkTokens` made its
    /// token a link: `loom-emoji:e_3`.
    static let link = "loom-emoji:"

    /// Each way an emoji is written, and its picture's name, from design/twemoji's
    /// sequences.txt.
    private static let table: [String: String] = {
        guard let url = Bundle.main.url(forResource: "sequences", withExtension: "txt"),
              let text = try? String(contentsOf: url, encoding: .utf8) else { return [:] }
        var out: [String: String] = [:]
        for line in text.split(separator: "\n") where !line.hasPrefix("# ") {
            let cut = line.split(separator: "\t", maxSplits: 1)
            if cut.count == 2 { out[String(cut[0])] = String(cut[1]) }
        }
        return out
    }()

    /// The picture of the emoji `c` is, if it's one. A Swift character is a whole
    /// emoji, its skin tone, joiners and keycap included.
    static func name(_ c: Character) -> String? {
        guard c.unicodeScalars.contains(where: { !$0.isASCII }) else { return nil }
        let s = String(c)
        return table[s] ?? table[s.replacingOccurrences(of: "\u{FE0F}", with: "")]
    }

    /// Whether `s` may have an emoji or a token in it, to skip the rest for plain words.
    static func mayHave(_ s: String) -> Bool {
        s.unicodeScalars.contains { !$0.isASCII } || s.contains("<:") || s.contains("<a:")
    }

    // MARK: The person's own

    /// One of the person's own in a message's text, `<:party_cat:e_3>` or
    /// `<a:party_cat:e_3>` for one that moves (protocol.md, "Emoji").
    struct Token: Equatable {
        var range: Range<String.Index>
        var name: String
        var id: String
        var animated: Bool
    }

    /// The tokens in `text`, in order, as `shared/emoji.calx` reads them.
    static func tokens(_ text: String) -> [Token] {
        guard text.contains("<:") || text.contains("<a:") else { return [] }
        var out: [Token] = []
        var i = text.startIndex
        while let open = text[i...].firstIndex(of: "<") {
            if let t = token(text, at: open) {
                out.append(t)
                i = t.range.upperBound
            } else {
                i = text.index(after: open)
            }
        }
        return out
    }

    private static func token(_ text: String, at open: String.Index) -> Token? {
        let u = text.utf8
        var j = u.index(after: open)
        let animated = j < u.endIndex && u[j] == UInt8(ascii: "a")
        if animated { j = u.index(after: j) }
        guard j < u.endIndex, u[j] == UInt8(ascii: ":") else { return nil }
        j = u.index(after: j)
        let nameFrom = j
        while j < u.endIndex, u[j] != UInt8(ascii: ":") {
            let b = u[j]
            guard (b >= 97 && b <= 122) || (b >= 48 && b <= 57) || b == 95 else { return nil }
            j = u.index(after: j)
        }
        let nameLength = u.distance(from: nameFrom, to: j)
        guard j < u.endIndex, (2...32).contains(nameLength) else { return nil }
        let nameTo = j
        j = u.index(after: j)
        let idFrom = j
        while j < u.endIndex, u[j] != UInt8(ascii: ">") {
            let b = u[j]
            guard (b >= 97 && b <= 122) || (b >= 65 && b <= 90) || (b >= 48 && b <= 57) || b == 95 else { return nil }
            j = u.index(after: j)
        }
        let idLength = u.distance(from: idFrom, to: j)
        guard j < u.endIndex, idLength > 0, idLength <= 40 else { return nil }
        return Token(range: open..<u.index(after: j), name: String(text[nameFrom..<nameTo]), id: String(text[idFrom..<j]), animated: animated)
    }

    /// `text` with each of the person's own emoji as `:name:`, as copying gives it.
    static func asNames(_ text: String) -> String {
        var out = ""
        var at = text.startIndex
        for t in tokens(text) {
            out += text[at..<t.range.lowerBound]
            out += ":\(t.name):"
            at = t.range.upperBound
        }
        out += text[at...]
        return out
    }

    /// A message's words with each of the person's own outside code as a link to it,
    /// `[:party_cat:](loom-emoji:e_3)`, which `MarkdownView.text` draws as its picture.
    static func linkTokens(_ text: String) -> String {
        let all = tokens(text)
        guard !all.isEmpty else { return text }
        let code = codeRanges(text)
        var out = ""
        var at = text.startIndex
        for t in all where !code.contains(where: { $0.overlaps(t.range) }) {
            out += text[at..<t.range.lowerBound]
            out += "[:\(t.name):](\(link)\(t.id))"
            at = t.range.upperBound
        }
        out += text[at...]
        return out
    }

    /// Where `text` has code: between backticks, a run of them closed by as many.
    static func codeRanges(_ text: String) -> [Range<String.Index>] {
        var out: [Range<String.Index>] = []
        var i = text.startIndex
        while let open = text[i...].firstIndex(of: "`") {
            let run = text[open...].prefix { $0 == "`" }
            let fence = String(run)
            guard let close = text[run.endIndex...].range(of: fence) else { break }
            out.append(open..<close.upperBound)
            i = close.upperBound
        }
        return out
    }

    /// Whether a message's words are only one to three emoji, built-in or the person's
    /// own, with nothing but spaces between, which the chat shows big.
    static func jumbo(_ text: String) -> Bool {
        guard mayHave(text) else { return false }
        var rest = text
        var count = 0
        for t in tokens(text).reversed() {
            rest.replaceSubrange(t.range, with: " ")
            count += 1
        }
        for c in rest where !c.isWhitespace {
            guard name(c) != nil else { return false }
            count += 1
        }
        return (1...3).contains(count)
    }

    // MARK: Pictures

    /// Twemoji's picture `name` drawn `side` points square, sharp at the screen's scale,
    /// made once.
    static func picture(_ name: String, side: CGFloat) -> UIImage? {
        let key = "\(name)@\(side)" as NSString
        if let kept = drawn.object(forKey: key) { return kept }
        guard let vector = UIImage(named: name) else { return nil }
        let size = CGSize(width: side, height: side)
        let made = UIGraphicsImageRenderer(size: size).image { _ in vector.draw(in: CGRect(origin: .zero, size: size)) }
        drawn.setObject(made, forKey: key)
        return made
    }

    /// One of the person's own, from its picture's file, `side` points square.
    static func picture(path: String, side: CGFloat) -> UIImage? {
        let key = "\(path)@\(side)" as NSString
        if let kept = drawn.object(forKey: key) { return kept }
        guard let image = UIImage(contentsOfFile: path) else { return nil }
        let size = CGSize(width: side, height: side)
        // Its shape kept, in the middle of the square, as the instance made it.
        let k = min(side / max(image.size.width, 1), side / max(image.size.height, 1))
        let w = image.size.width * k, h = image.size.height * k
        let made = UIGraphicsImageRenderer(size: size).image { _ in
            image.draw(in: CGRect(x: (side - w) / 2, y: (side - h) / 2, width: w, height: h))
        }
        drawn.setObject(made, forKey: key)
        return made
    }

    /// The picture for what's sent, a built-in emoji or a token, `side` points square:
    /// one of the person's own once `paths` has its file.
    static func picture(of emoji: String, side: CGFloat, paths: [String: String]) -> UIImage? {
        if let t = tokens(emoji).first {
            return paths[t.id].flatMap { picture(path: $0, side: side) }
        }
        guard let c = emoji.first, let name = name(c) else { return nil }
        return picture(name, side: side)
    }

    nonisolated(unsafe) private static let drawn: NSCache<NSString, UIImage> = {
        let c = NSCache<NSString, UIImage>()
        c.countLimit = 1500
        return c
    }()

    /// An emoji's picture in a line of text, `side` points square, sitting on the line as
    /// the emoji font's would, read out as `label`.
    static func inline(_ image: UIImage, side: CGFloat, label: String) -> Text {
        guard let cg = image.cgImage else { return Text(verbatim: label) }
        return Text(Image(decorative: cg, scale: image.scale)).baselineOffset(-side * 0.2).accessibilityLabel(label)
    }

    /// The side of an emoji among words of `size` points, as the person's text size
    /// scales it.
    static func side(_ size: CGFloat) -> CGFloat {
        (UIFontMetrics(forTextStyle: .body).scaledValue(for: size * inText)).rounded()
    }

    /// `piece` with each built-in emoji in it drawn with Twemoji, `side` points square.
    static func text(_ piece: AttributedString, side: CGFloat) -> Text {
        let chars = piece.characters
        guard String(chars).unicodeScalars.contains(where: { !$0.isASCII }) else { return Text(piece) }
        var out = Text("")
        var from = chars.startIndex
        var i = chars.startIndex
        while i < chars.endIndex {
            let next = chars.index(after: i)
            if let name = name(chars[i]), let image = picture(name, side: side) {
                if from < i { out = Text("\(out)\(Text(AttributedString(piece[from..<i])))") }
                out = Text("\(out)\(inline(image, side: side, label: String(chars[i])))")
                from = next
            }
            i = next
        }
        if from < chars.endIndex { out = Text("\(out)\(Text(AttributedString(piece[from..<chars.endIndex])))") }
        return out
    }
}
