import SwiftUI
import UIKit

/// Markdown as the chat draws it: paragraphs, headings, lists, quotes, rules and code
/// blocks, with Foundation's inline Markdown (bold, italics, code, links) inside each.
/// Tables and anything else it doesn't know show as they were written, in mono.
struct MarkdownView: View {
    var text: String
    /// What a person typed keeps its line breaks.
    var keepBreaks = false
    var font: Font = Fonts.reply
    /// The colour of its words: quieter for what a reply said on its way.
    var ink: Color = P.ink
    /// Still being written, so it grows at the end.
    var live = false
    /// What its links and mentions show: the small pictures before links, and the agents
    /// it mentions.
    var links = Inline()

    var body: some View {
        let parts = live && !keepBreaks ? MarkdownView.growing(text) : MarkdownView.parse(text, keepBreaks: keepBreaks)
        VStack(alignment: .leading, spacing: 10) {
            // Each part is its own view, compared before it's drawn again: as a reply is
            // written, only its last part changes.
            ForEach(parts.indices, id: \.self) { i in
                PartView(part: parts[i], font: font, ink: ink, links: links).equatable()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    enum Part: Equatable {
        case paragraph(String)
        case heading(Int, String)
        case item(indent: Int, mark: String, text: String)
        case quote(String)
        case code(language: String, text: String)
        case raw(String)
        case rule
    }

    /// The parts of a reply that may be growing as it's written. What's before its last
    /// blank line outside a code block is parsed once and kept, and only what follows
    /// it is parsed again: a blank line there ends every part, so the parts come out
    /// the same as parsing the whole.
    static func growing(_ text: String) -> [Part] {
        let bytes = text.utf8
        // A carriage return would put a line's end inside a character.
        if bytes.contains(13) { return parse(text) }
        var parts: [Part] = []
        var from = bytes.startIndex
        if let g = grown, bytes.count >= g.bytes, text.hasPrefix(g.text) {
            parts = g.parts
            from = bytes.index(bytes.startIndex, offsetBy: g.bytes)
        }
        // The last place in what's left that follows a blank line outside a fence.
        var split: String.Index?
        var fence: Substring?
        var lineStart = from
        var blank = true
        var i = from
        while i < bytes.endIndex {
            if bytes[i] == 10 {
                let line = text[lineStart..<i].trimmingCharacters(in: .whitespaces)
                if let f = fence {
                    if line.hasPrefix(f) { fence = nil }
                } else if line.hasPrefix("```") || line.hasPrefix("~~~") {
                    fence = Substring(line.prefix(3))
                } else if line.isEmpty && !blank {
                    split = bytes.index(after: i)
                }
                blank = line.isEmpty
                lineStart = bytes.index(after: i)
            }
            i = bytes.index(after: i)
        }
        guard let split else {
            return parts + parse(String(text[from...]))
        }
        parts += parse(String(text[from..<split]))
        grown = Grown(text: String(text[..<split]), bytes: bytes.distance(from: bytes.startIndex, to: split), parts: parts)
        return parts + parse(String(text[split...]))
    }

    private struct Grown {
        var text: String
        var bytes: Int
        var parts: [Part]
    }

    /// The reply last parsed by `growing`, up to its last blank line.
    nonisolated(unsafe) private static var grown: Grown?

    /// Inline Markdown as the chat draws it. Kept per string, so a part that's drawn
    /// again, or a row that scrolls back, isn't parsed again.
    static func inline(_ s: String) -> AttributedString {
        let key = s as NSString
        if let kept = inlineKept.object(forKey: key) { return kept.value }
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        var out = (try? AttributedString(markdown: s, options: options)) ?? AttributedString(s)
        for run in out.runs {
            if let intent = run.inlinePresentationIntent, intent.contains(.code) {
                out[run.range].font = Fonts.code
                out[run.range].backgroundColor = P.raised2
            }
            if run.link != nil {
                out[run.range].foregroundColor = P.ink
                out[run.range].underlineStyle = .single
            }
        }
        inlineKept.setObject(Kept(out), forKey: key, cost: s.utf8.count)
        return out
    }

    /// Inline Markdown with its links and mentions as the chat shows them (PLAN.md,
    /// "Links and sources" and "Mentions"), as desktop/src/model.rs's `with_link_icons`:
    /// - a link with its site's icon before it, where `links` has one;
    /// - an agent it mentions as its thread and its name in its colour, a little
    ///   weathered, with no @, or `@Name` for one that's gone;
    /// - a file it mentions, or one sent with it, with its icon before its name, or a
    ///   tiny copy of a picture sent with it once it's fetched.
    static func text(_ s: String, links: Inline) -> Text {
        let a = inline(s)
        guard !links.icons.isEmpty || s.contains("](loom") else { return Text(a) }
        var out = Text("")
        var last: URL?
        for run in a.runs {
            let piece = AttributedString(a[run.range])
            guard let url = run.link else {
                last = nil
                out = Text("\(out)\(Text(piece))")
                continue
            }
            // A link's words can be more than one run, as when part of them is bold: what
            // goes before it goes before its first.
            let first = url != last
            last = url
            let address = url.absoluteString
            // An icon and its name are joined by a no-break space, so a line never ends
            // between them.
            switch LinkTo(address) {
            case .agent(let id):
                var name = String(piece.characters)
                if first { name = String(name.drop { $0 == "@" || $0.isWhitespace }) }
                guard let agent = links.agents[id] else {
                    out = Text("\(out)\(Text(verbatim: first ? "@\(name)" : name))")
                    continue
                }
                let ink = P.mention(agent.color)
                if first {
                    out = Text("\(out)\(Text(Image(uiImage: MentionGlyph.thread(agent.outline)).renderingMode(.template)).foregroundStyle(ink).baselineOffset(-1))\u{00A0}")
                }
                out = Text("\(out)\(Text(verbatim: name).foregroundStyle(ink).fontWeight(.medium))")
                continue
            case .file(_, let path) where first:
                out = Text("\(out)\(fileIcon(path))\u{00A0}")
            case .attached where first:
                if let path = links.icons[address], let icon = siteIcon(path) {
                    out = Text("\(out)\(Text(Image(uiImage: icon)).baselineOffset(-2))\u{00A0}")
                } else {
                    out = Text("\(out)\(fileIcon(String(piece.characters)))\u{00A0}")
                }
            case .page where first:
                if let path = links.icons[address], let icon = siteIcon(path) {
                    out = Text("\(out)\(Text(Image(uiImage: icon)).baselineOffset(-2))\u{00A0}")
                }
            default:
                break
            }
            out = Text("\(out)\(Text(piece))")
        }
        return out
    }

    /// The icon before a file's name in a message: a picture's, or a page's, in the
    /// quiet ink.
    private static func fileIcon(_ name: String) -> Text {
        Text(Image(systemName: isPicture(name) ? "photo" : "doc"))
            .font(.system(size: 12))
            .foregroundStyle(P.ink3)
    }

    /// A site's icon at 14 points, with loom's small radius, kept once it's made.
    static func siteIcon(_ path: String) -> UIImage? {
        let key = path as NSString
        if let kept = siteIcons.object(forKey: key) { return kept }
        guard let image = UIImage(contentsOfFile: path) else { return nil }
        let side: CGFloat = 14
        let made = UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { _ in
            let box = CGRect(x: 0, y: 0, width: side, height: side)
            UIBezierPath(roundedRect: box, cornerRadius: 3).addClip()
            image.draw(in: box)
        }
        siteIcons.setObject(made, forKey: key)
        return made
    }

    nonisolated(unsafe) private static let siteIcons = NSCache<NSString, UIImage>()

    private final class Kept {
        let value: AttributedString
        init(_ value: AttributedString) { self.value = value }
    }

    nonisolated(unsafe) private static let inlineKept: NSCache<NSString, Kept> = {
        let c = NSCache<NSString, Kept>()
        c.totalCostLimit = 4 << 20
        return c
    }()
}

/// One part of a Markdown text.
private struct PartView: View, Equatable {
    var part: MarkdownView.Part
    var font: Font
    var ink: Color
    var links: Inline

    var body: some View {
        switch part {
        case .paragraph(let s):
            MarkdownView.text(s, links: links).font(font).foregroundStyle(ink).lineSpacing(4)
        case .heading(let level, let s):
            MarkdownView.text(s, links: links)
                .font(Fonts.ui(level == 1 ? 21 : level == 2 ? 19 : 17, .semibold, relativeTo: .headline))
                .foregroundStyle(P.ink)
                .padding(.top, 4)
        case .item(let indent, let mark, let s):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(mark).font(font).foregroundStyle(P.ink3).frame(minWidth: 14, alignment: .trailing)
                MarkdownView.text(s, links: links).font(font).foregroundStyle(ink).lineSpacing(4)
            }
            .padding(.leading, CGFloat(indent) * 16)
        case .quote(let s):
            MarkdownView.text(s, links: links)
                .font(font).foregroundStyle(P.ink2).lineSpacing(4)
                .padding(.leading, 12)
                .overlay(alignment: .leading) { Rectangle().fill(P.lineStrong).frame(width: 2) }
        case .code(let language, let s):
            CodeBlock(language: language, text: s)
        case .raw(let s):
            Sideways {
                Text(s).font(Fonts.code).foregroundStyle(P.ink2).fixedSize()
            }
        case .rule:
            Rectangle().fill(P.line).frame(height: 1).padding(.vertical, 4)
        }
    }
}

/// Its content as it is when it fits across, and in a sideways scroll only when it's
/// wider: most code blocks and tables need no scroll view of their own.
struct Sideways<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        ViewThatFits(in: .horizontal) {
            content
            ScrollView(.horizontal, showsIndicators: false) { content }
        }
    }
}

extension MarkdownView {

    /// The text's blocks, in order.
    static func parse(_ text: String, keepBreaks: Bool = false) -> [Part] {
        var parts: [Part] = []
        var para: [String] = []
        let lines = text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        var i = 0

        func flush() {
            if !para.isEmpty {
                parts.append(.paragraph(para.joined(separator: keepBreaks ? "\n" : " ")))
                para = []
            }
        }

        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            let indent = line.prefix { $0 == " " }.count / 2

            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flush()
                let fence = String(trimmed.prefix(3))
                let language = trimmed.dropFirst(3).trimmingCharacters(in: .whitespaces)
                var body: [String] = []
                i += 1
                while i < lines.count, !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    body.append(lines[i])
                    i += 1
                }
                parts.append(.code(language: language, text: body.joined(separator: "\n")))
                i += 1
                continue
            }
            if trimmed.isEmpty {
                flush()
            } else if let level = heading(trimmed) {
                flush()
                parts.append(.heading(level, String(trimmed.dropFirst(level + 1))))
            } else if trimmed == "---" || trimmed == "***" || trimmed == "___" {
                flush()
                parts.append(.rule)
            } else if let (mark, rest) = item(trimmed) {
                flush()
                parts.append(.item(indent: indent, mark: mark, text: rest))
            } else if trimmed.hasPrefix(">") {
                flush()
                parts.append(.quote(String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces)))
            } else if trimmed.hasPrefix("|") {
                flush()
                var table: [String] = []
                while i < lines.count, lines[i].trimmingCharacters(in: .whitespaces).hasPrefix("|") {
                    table.append(lines[i].trimmingCharacters(in: .whitespaces))
                    i += 1
                }
                parts.append(.raw(table.joined(separator: "\n")))
                continue
            } else {
                para.append(trimmed)
            }
            i += 1
        }
        flush()
        return parts
    }

    private static func heading(_ s: String) -> Int? {
        let hashes = s.prefix { $0 == "#" }.count
        guard (1...6).contains(hashes), s.dropFirst(hashes).first == " " else { return nil }
        return hashes
    }

    private static func item(_ s: String) -> (String, String)? {
        for bullet in ["- ", "* ", "+ "] where s.hasPrefix(bullet) {
            var rest = String(s.dropFirst(2))
            if rest.hasPrefix("[ ] ") { return ("○", String(rest.dropFirst(4))) }
            if rest.hasPrefix("[x] ") || rest.hasPrefix("[X] ") {
                rest = String(rest.dropFirst(4))
                return ("✓", rest)
            }
            return ("•", rest)
        }
        let digits = s.prefix { $0.isNumber }
        if !digits.isEmpty, digits.count < 4 {
            let after = s.dropFirst(digits.count)
            if after.hasPrefix(". ") || after.hasPrefix(") ") {
                return ("\(digits).", String(after.dropFirst(2)))
            }
        }
        return nil
    }
}

extension MarkdownView {
    init(_ text: String, keepBreaks: Bool = false, font: Font = Fonts.reply, ink: Color = P.ink, live: Bool = false, links: Inline = Inline()) {
        self.text = text
        self.keepBreaks = keepBreaks
        self.font = font
        self.ink = ink
        self.live = live
        self.links = links
    }
}

/// A code block: mono, scrolling sideways, with its language and a copy button.
struct CodeBlock: View {
    var language: String
    var text: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "code" : language)
                    .font(Fonts.caption)
                    .foregroundStyle(P.ink3)
                Spacer()
                Button {
                    UIPasteboard.general.string = text
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(1.5))
                        copied = false
                    }
                } label: {
                    Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .labelStyle(.iconOnly)
                        .font(.system(size: 12))
                        .foregroundStyle(P.ink3)
                        .frame(width: 28, height: 24)
                }
                .accessibilityLabel(copied ? "Copied" : "Copy code")
            }
            .padding(.leading, 12)
            .padding(.trailing, 4)
            .padding(.top, 4)
            Sideways {
                Text(text)
                    .font(Fonts.code)
                    .foregroundStyle(P.ink)
                    .lineSpacing(3)
                    .fixedSize()
                    .padding(.horizontal, 12)
                    .padding(.bottom, 12)
                    .padding(.top, 2)
            }
        }
        .background(P.raised, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.line, lineWidth: 1))
    }
}

#Preview {
    ScrollView {
        MarkdownView(Sample.finished + "\n\n## A list\n\n- one `code`\n- two\n  - nested\n1. first\n2. second\n\n> A quote.\n\n| a | b |\n|---|---|\n| 1 | 2 |")
            .padding()
    }
}

/// The small pictures of mentions (PLAN.md, "Mentions").
enum MentionGlyph {
    /// An agent's thread in its shape, 14 by 14 points, still, as a template image to be
    /// drawn in the agent's colour, as the desktop's `mention_thread`: fitted into the box
    /// by its widest side, so a round shape stands a little taller than the capitals, or
    /// for the plain thread a small wave. Kept per shape.
    static func thread(_ outline: [CGPoint]?) -> UIImage {
        let key = (outline.map { $0.map { "\($0.x),\($0.y)" }.joined(separator: " ") } ?? "") as NSString
        if let kept = kept.object(forKey: key) { return kept }
        let made = UIGraphicsImageRenderer(size: CGSize(width: 14, height: 14)).image { _ in
            let path = UIBezierPath()
            if let pts = outline, pts.count > 1 {
                let sx = pts.map { abs($0.x) }.max() ?? 0
                let sy = pts.map { abs($0.y) }.max() ?? 0
                let scale = min(6 / max(sx, 0.1), 6 / max(sy, 0.1))
                for (i, p) in pts.enumerated() {
                    let q = CGPoint(x: 7 + p.x * scale, y: 7 + p.y * scale)
                    if i == 0 { path.move(to: q) } else { path.addLine(to: q) }
                }
            } else {
                path.move(to: CGPoint(x: 1, y: 8))
                path.addCurve(to: CGPoint(x: 7, y: 7), controlPoint1: CGPoint(x: 3.5, y: 4), controlPoint2: CGPoint(x: 5.5, y: 4))
                path.addCurve(to: CGPoint(x: 13, y: 6), controlPoint1: CGPoint(x: 8.5, y: 10), controlPoint2: CGPoint(x: 10.5, y: 10.5))
            }
            path.lineWidth = 1.6
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            UIColor.black.setStroke()
            path.stroke()
        }.withRenderingMode(.alwaysTemplate)
        kept.setObject(made, forKey: key)
        return made
    }

    nonisolated(unsafe) private static let kept = NSCache<NSString, UIImage>()
}
