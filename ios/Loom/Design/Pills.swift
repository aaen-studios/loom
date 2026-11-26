import SwiftUI
import UIKit

// An Acta card's pill (PLAN.md, "Cards in messages"), as the desktop's `card_pill` draws
// it: the board's tile, the id in mono, the title cut to fit and a dot in the status's
// colour, on the bubble's fill with a line around it, 22 points high. In a message's words
// it's text with a frame drawn behind it (`PillFrames`), so it wraps with them and its
// link opens the card; in the composer and under a message it's its own picture or view.

extension CardInk {
    /// Its colour: quiet for to do, sky for in progress, gold for review, ok for done.
    var color: Color {
        switch self {
        case .quiet: P.ink3
        case .sky: P.thread("sky")
        case .gold: P.thread("gold")
        case .ok: P.ok
        }
    }
}

enum Pill {
    /// The board's tile, until Acta gives each board its own icon: Acta's purple with its
    /// layers in white, `side` points square, kept once it's made.
    static func tile(_ side: CGFloat = 14) -> UIImage {
        let key = side as NSNumber
        if let kept = tiles.object(forKey: key) { return kept }
        let made = UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { _ in
            let box = CGRect(x: 0, y: 0, width: side, height: side)
            UIColor(loom: Tokens.ColorLight.board).setFill()
            UIBezierPath(roundedRect: box, cornerRadius: side * 0.24).fill()
            let glyph = UIImage(systemName: "square.3.layers.3d", withConfiguration: UIImage.SymbolConfiguration(pointSize: side * 0.62, weight: .semibold))?
                .withTintColor(.white, renderingMode: .alwaysOriginal)
            if let glyph {
                glyph.draw(in: CGRect(x: (side - glyph.size.width) / 2, y: (side - glyph.size.height) / 2, width: glyph.size.width, height: glyph.size.height))
            }
        }
        tiles.setObject(made, forKey: key)
        return made
    }

    nonisolated(unsafe) private static let tiles = NSCache<NSNumber, UIImage>()

    /// A card's pill among a message's words: its tile, its id and its title as a link to
    /// it, and its status's dot, each part marked for `PillFrames` to draw its frame behind.
    /// Joined by no-break spaces, so a line never ends inside it. A card that isn't there,
    /// or that the person can't see, is its id alone, quiet.
    static func text(_ card: ActaCard?, id: String, url: URL) -> Text {
        guard let card, !card.missing else {
            return Text(verbatim: id).font(Fonts.mono(Tokens.PhoneSize.small - 1)).foregroundStyle(P.ink3)
        }
        let mark = PillMark(id: id)
        let space = "\u{00A0}"
        var words = AttributedString(id)
        words.font = Fonts.mono(Tokens.PhoneSize.small - 1)
        words.foregroundColor = P.ink2
        words.link = url
        let title = card.pillTitle
        if !title.isEmpty {
            var t = AttributedString(space + title.replacingOccurrences(of: " ", with: space))
            t.font = Fonts.ui(Tokens.PhoneSize.small)
            t.foregroundColor = card.struck ? P.ink3 : P.ink
            if card.struck { t.strikethroughStyle = .single }
            t.link = url
            words += t
        }
        let lead = Text(verbatim: space) + Text(Image(uiImage: tile())).baselineOffset(-2) + Text(verbatim: space)
        let dot = Text(verbatim: space) + Text(Image(systemName: "circle.fill")).font(.system(size: 7)).foregroundStyle(card.ink.color).baselineOffset(1) + Text(verbatim: space)
        return (lead + Text(words) + dot).customAttribute(mark)
    }
}

/// Marks the parts of one card's pill in a message's words, for `PillFrames`.
struct PillMark: TextAttribute {
    var id: String
}

/// Draws each card's pill frame behind its parts in a message's words: the bubble's fill
/// with a line around it and 6-point corners, 22 points high, one for each line it's on.
struct PillFrames: TextRenderer {
    var displayPadding: EdgeInsets { EdgeInsets(top: 4, leading: 1, bottom: 4, trailing: 1) }

    func draw(layout: Text.Layout, in ctx: inout GraphicsContext) {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous)
        for line in layout {
            var open: (id: String, rect: CGRect)?
            func shut() {
                guard let o = open else { return }
                let h = max(Tokens.Control.pill, o.rect.height + 2)
                let r = CGRect(x: o.rect.minX, y: o.rect.midY - h / 2, width: o.rect.width, height: h)
                let path = shape.path(in: r)
                ctx.fill(path, with: .color(P.bubble))
                ctx.stroke(path, with: .color(P.line), lineWidth: 1)
                open = nil
            }
            for run in line {
                guard let mark = run[PillMark.self] else {
                    shut()
                    continue
                }
                let r = run.typographicBounds.rect
                if let o = open, o.id == mark.id {
                    open = (o.id, o.rect.union(r))
                } else {
                    shut()
                    open = (mark.id, r)
                }
            }
            shut()
        }
        for line in layout { ctx.draw(line) }
    }
}

extension View {
    /// Draws the frames of the card pills in its words, where it has any.
    @ViewBuilder
    func pillFrames(_ on: Bool) -> some View {
        if on { textRenderer(PillFrames()) } else { self }
    }
}

/// A card's pill as a view of its own: under the message a card was made from, and in a
/// board's rows. A tap opens its sheet.
struct CardPillView: View {
    var card: ActaCard
    var open: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous)
        Button(action: open) {
            HStack(spacing: 5) {
                Image(uiImage: Pill.tile())
                Text(card.id).font(Fonts.mono(Tokens.PhoneSize.small - 1)).foregroundStyle(P.ink2)
                if !card.missing && !card.title.isEmpty {
                    Text(card.title)
                        .font(Fonts.ui(Tokens.PhoneSize.small))
                        .foregroundStyle(card.struck ? P.ink3 : P.ink)
                        .strikethrough(card.struck)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: Tokens.Layout.pillTitle, alignment: .leading)
                        .fixedSize(horizontal: true, vertical: false)
                    Circle().fill(card.ink.color).frame(width: 7, height: 7)
                }
            }
            .padding(.horizontal, 6)
            .frame(height: Tokens.Control.pill)
            .background(P.bubble, in: shape)
            .overlay(shape.strokeBorder(P.line, lineWidth: 1))
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(card.id), \(card.title), \(card.statusWords)")
    }
}

/// The composer's picture of a card's piece: its tile and id, and once it's looked up its
/// title and dot, framed as its pill is, no wider than `most`.
@MainActor
func cardPieceImage(ref: String, preview: CardPreview?, font: UIFont, most: CGFloat) -> (image: UIImage, bounds: CGRect) {
    let id = cardLinkParts(ref)?.id ?? ref
    var card: ActaCard?
    if case .ready(let c) = preview { card = c }
    let height = max(ceil(font.lineHeight), Tokens.Control.pill)
    let mono = UIFont(name: Tokens.Font.mono, size: font.pointSize - 2) ?? font
    let small = UIFont(name: Tokens.Font.ui, size: font.pointSize - 2) ?? font
    let idText = NSAttributedString(string: id, attributes: [.font: mono, .foregroundColor: UIColor(P.ink2)])
    let title = card.map { $0.missing ? "" : $0.title } ?? ""
    let style = NSMutableParagraphStyle()
    style.lineBreakMode = .byTruncatingTail
    let titleText = NSAttributedString(string: title, attributes: [.font: small, .foregroundColor: UIColor(P.ink), .paragraphStyle: style])
    let side: CGFloat = 14, pad: CGFloat = 6, gap: CGFloat = 5, dot: CGFloat = 7
    let fixed = pad + side + gap + ceil(idText.size().width) + pad + (title.isEmpty ? 0 : gap + gap + dot)
    let titleWidth = title.isEmpty ? 0 : min(ceil(titleText.size().width), max(0, most - fixed), Tokens.Layout.pillTitle)
    let width = ceil(fixed + titleWidth)
    let image = UIGraphicsImageRenderer(size: CGSize(width: width + 2, height: height)).image { _ in
        let frame = CGRect(x: 1, y: (height - Tokens.Control.pill) / 2 + 0.5, width: width, height: Tokens.Control.pill - 1)
        let path = UIBezierPath(roundedRect: frame, cornerRadius: Tokens.Radius.chip)
        UIColor(P.bubble).setFill()
        path.fill()
        UIColor(P.line).setStroke()
        path.lineWidth = 1
        path.stroke()
        var x = 1 + pad
        Pill.tile(side).draw(in: CGRect(x: x, y: (height - side) / 2, width: side, height: side))
        x += side + gap
        let ids = idText.size()
        idText.draw(at: CGPoint(x: x, y: (height - ids.height) / 2))
        x += ceil(ids.width)
        if !title.isEmpty, let card {
            x += gap
            let ts = titleText.size()
            titleText.draw(with: CGRect(x: x, y: (height - ts.height) / 2, width: titleWidth, height: ts.height), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine], context: nil)
            x += titleWidth + gap
            UIColor(card.ink.color).setFill()
            UIBezierPath(ovalIn: CGRect(x: x, y: (height - dot) / 2, width: dot, height: dot)).fill()
        }
    }
    return (image, CGRect(x: 0, y: font.descender - (height - ceil(font.lineHeight)) / 2, width: width + 2, height: height))
}
