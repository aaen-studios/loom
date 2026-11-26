import SwiftUI

/// A watch's line in the chat (PLAN.md, "Watching outside events"), as the desktop draws
/// it: a quiet row on the left, the board's tile or T3's strand, the card (its id, its
/// title and a dot in its status's colour) or the thread's title, and what happened and
/// who in quiet ink. Lines one after another sit tight (`stackedLines`).
struct WatchLineRow: View {
    @Environment(Session.self) private var session
    var line: WatchLine

    var body: some View {
        HStack(spacing: Tokens.Space.s) {
            LineMark(mark: line.mark)
            switch line.mark {
            case .board:
                // A tap opens the card as it is now.
                Button {
                    session.send(session.model.openCard(board: line.board, id: line.card))
                } label: {
                    LineCardPill(id: line.card, title: line.title, status: line.status)
                }
                .buttonStyle(.plain)
                .disabled(line.board.isEmpty)
                .layoutPriority(-1)
            case .strand:
                Text(line.threadTitle)
                    .font(Fonts.small).foregroundStyle(P.ink2)
                    .lineLimit(1).truncationMode(.tail)
                    .layoutPriority(-1)
            case .none:
                EmptyView()
            }
            Text(line.words)
                .font(Fonts.caption).foregroundStyle(P.ink3)
                .lineLimit(1).truncationMode(.tail)
                .fixedSize(horizontal: line.mark != .none, vertical: false)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(line.spoken)
        .accessibilityAction {
            guard line.mark == .board, !line.board.isEmpty else { return }
            session.send(session.model.openCard(board: line.board, id: line.card))
        }
    }
}

/// A line's mark, 14 points across: Acta's board tile, or T3's strand.
private struct LineMark: View {
    var mark: WatchLine.Mark

    var body: some View {
        switch mark {
        case .board:
            RoundedRectangle(cornerRadius: 3, style: .continuous)
                .fill(Color.loom(Tokens.ColorLight.board, Tokens.ColorDark.board))
                .frame(width: 14, height: 14)
                .overlay(
                    Image(systemName: "square.3.layers.3d")
                        .font(.system(size: 8, weight: .semibold))
                        .foregroundStyle(Color(red: 0.96, green: 0.95, blue: 0.99))
                )
        case .strand(let lie):
            LineStrand(lie: lie).frame(width: 14, height: 10)
        case .none:
            Color.clear.frame(width: 14, height: 10)
        }
    }
}

/// T3's strand: straight in the ok colour, or slack, its far end hanging lowest, in the
/// bad colour once the run failed and quiet once it was stopped.
private struct LineStrand: View {
    var lie: WatchLine.Lie

    var body: some View {
        let color: Color = switch lie {
        case .straight: P.ok
        case .sags: P.bad
        case .slack: P.ink3
        }
        Canvas { cx, size in
            var path = Path()
            let mid = size.height / 2
            path.move(to: CGPoint(x: 0, y: mid))
            if lie == .straight {
                path.addLine(to: CGPoint(x: size.width, y: mid))
            } else {
                // Shorter and hanging, as the desktop's failed thread does.
                let end = size.width * 0.8
                path.addQuadCurve(to: CGPoint(x: end, y: mid + size.height * 0.4), control: CGPoint(x: end * 0.6, y: mid))
            }
            cx.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: Tokens.ThreadStroke.small, lineCap: .round))
        }
    }
}

/// A card as a line names it, with no tile of its own (the line's mark is it): its id in
/// mono, its title cut to fit, and a dot in the colour of the status the line is about,
/// when it says one, on the bubble's fill with a line around it.
private struct LineCardPill: View {
    var id: String
    var title: String
    var status: String?

    var body: some View {
        HStack(spacing: 5) {
            Text(id).font(Fonts.mono(Tokens.Size.caption)).foregroundStyle(P.ink2)
                .fixedSize()
            if !title.isEmpty {
                Text(title).font(Fonts.small).foregroundStyle(P.ink)
                    .lineLimit(1).truncationMode(.tail)
            }
            if let status { Circle().fill(dot(status)).frame(width: 7, height: 7) }
        }
        .padding(.horizontal, 7)
        .frame(height: 22)
        .background(P.bubble, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous).strokeBorder(P.line))
    }

    /// The dot's colour, as a card's status is drawn: in progress sky, review gold, done
    /// ok, and the rest quiet.
    private func dot(_ status: String) -> Color {
        switch status {
        case "in_progress": P.thread("sky")
        case "review": P.thread("gold")
        case "done": P.ok
        default: P.ink3
        }
    }
}
