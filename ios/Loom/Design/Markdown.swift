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

    var body: some View {
        let parts = live && !keepBreaks ? MarkdownView.growing(text) : MarkdownView.parse(text, keepBreaks: keepBreaks)
        VStack(alignment: .leading, spacing: 10) {
            // Each part is its own view, compared before it's drawn again: as a reply is
            // written, only its last part changes.
            ForEach(parts.indices, id: \.self) { i in
                PartView(part: parts[i], font: font, ink: ink).equatable()
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

    var body: some View {
        switch part {
        case .paragraph(let s):
            Text(MarkdownView.inline(s)).font(font).foregroundStyle(ink).lineSpacing(4)
        case .heading(let level, let s):
            Text(MarkdownView.inline(s))
                .font(Fonts.ui(level == 1 ? 21 : level == 2 ? 19 : 17, .semibold, relativeTo: .headline))
                .foregroundStyle(P.ink)
                .padding(.top, 4)
        case .item(let indent, let mark, let s):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(mark).font(font).foregroundStyle(P.ink3).frame(minWidth: 14, alignment: .trailing)
                Text(MarkdownView.inline(s)).font(font).foregroundStyle(ink).lineSpacing(4)
            }
            .padding(.leading, CGFloat(indent) * 16)
        case .quote(let s):
            Text(MarkdownView.inline(s))
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
    init(_ text: String, keepBreaks: Bool = false, font: Font = Fonts.reply, ink: Color = P.ink, live: Bool = false) {
        self.text = text
        self.keepBreaks = keepBreaks
        self.font = font
        self.ink = ink
        self.live = live
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
