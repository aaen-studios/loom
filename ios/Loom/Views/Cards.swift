import SwiftUI

// The cards in a chat, all in one frame, as the desktop's `CardFrame`: a small square at
// its start, its title with what goes beside it, its rows, and its buttons. An agent's
// `show_card`, loom's sign-in, and the questions an agent waits on all use it.

/// The frame every card in the chat shares.
struct CardFrame<Content: View>: View {
    var title: String
    var lead: AnyView?
    var side: AnyView?
    var content: Content

    init(_ title: String, lead: AnyView? = nil, side: AnyView? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.lead = lead
        self.side = side
        self.content = content()
    }

    var body: some View {
        HStack(alignment: .top, spacing: Tokens.Space.m) {
            if let lead { lead }
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                HStack(spacing: Tokens.Space.s) {
                    Text(title)
                        .font(Fonts.ui(Tokens.PhoneSize.body, .medium))
                        .foregroundStyle(P.ink)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if let side { side }
                }
                .frame(minHeight: Tokens.Control.iconSmall)
                content
            }
        }
        .padding(.horizontal, Tokens.Space.l)
        .padding(.vertical, Tokens.Space.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(P.cardFill, in: RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous).strokeBorder(P.cardEdge, lineWidth: 1))
    }
}

extension CardFrame where Content == EmptyView {
    init(_ title: String, lead: AnyView? = nil, side: AnyView? = nil) {
        self.init(title, lead: lead, side: side) { EmptyView() }
    }
}

/// The small square at a card's start: an icon, in the agent's colour on a question, or
/// the initial of something with a name and no icon, such as an MCP server.
struct CardLead: View {
    var icon: String?
    var initial: String?
    var tint: Color = P.ink2

    static func of(_ name: String) -> CardLead {
        CardLead(initial: name.first(where: { $0.isLetter || $0.isNumber }).map { String($0).uppercased() } ?? "?")
    }

    var body: some View {
        RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous)
            .fill(P.cardInset)
            .frame(width: Tokens.Control.iconSmall, height: Tokens.Control.iconSmall)
            .overlay {
                if let icon {
                    Image(systemName: icon).font(.system(size: 12, weight: .medium)).foregroundStyle(tint)
                } else {
                    Text(initial ?? "?").font(Fonts.ui(Tokens.PhoneSize.caption, .semibold)).foregroundStyle(P.ink2)
                }
            }
            .accessibilityHidden(true)
    }
}

/// Labelled lines in a card: what each is, quiet, then its value, each kept to one line
/// so a long one never runs past the card.
struct CardLines: View {
    var lines: [Shown.Line]

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: Tokens.Space.m, verticalSpacing: Tokens.Space.xs) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, l in
                GridRow {
                    Text(l.label).foregroundStyle(P.ink3)
                    Text(l.value).foregroundStyle(P.ink).frame(maxWidth: .infinity, alignment: .leading)
                }
                .font(Fonts.small)
                .lineLimit(1)
                .accessibilityElement(children: .combine)
            }
        }
    }
}

/// A command or a script in a card, as it's written.
struct CardCode: View {
    var text: String

    var body: some View {
        Text(text)
            .font(Fonts.code)
            .foregroundStyle(P.ink2)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, Tokens.Space.m)
            .padding(.vertical, Tokens.Space.s)
            .background(P.cardInset, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
    }
}

/// Details, folded until it's tapped: all of what a card shows only some of, in a box
/// that scrolls past a few lines.
struct CardDetails: View {
    var raw: String
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
            Button {
                open.toggle()
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .rotationEffect(.degrees(open ? 90 : 0))
                    Text("Details")
                }
                .font(Fonts.caption)
                .foregroundStyle(P.ink3)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open {
                let text = Text(raw)
                    .font(Fonts.code)
                    .foregroundStyle(P.ink2)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, Tokens.Space.m)
                    .padding(.vertical, Tokens.Space.s)
                ViewThatFits(in: .vertical) {
                    text
                    ScrollView { text }
                }
                .frame(maxHeight: 240)
                .background(P.cardInset, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
            }
        }
        .animation(.easeOut(duration: 0.15), value: open)
    }
}

/// A card once it's answered, or once the chat has gone on: one quiet line saying how.
struct CardDone: View {
    var icon: String
    var words: String

    var body: some View {
        HStack(spacing: Tokens.Space.s) {
            Image(systemName: icon).font(.system(size: 10, weight: .semibold))
            Text(words).lineLimit(1).truncationMode(.tail)
        }
        .font(Fonts.small)
        .foregroundStyle(P.ink3)
        .accessibilityElement(children: .combine)
    }
}

/// A card's button: the main one filled, the rest outlined, smaller than a page's.
struct CardButtonStyle: ButtonStyle {
    var primary = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
            .lineLimit(1)
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: Tokens.Control.button)
            .foregroundStyle(primary ? P.buttonInk : P.ink)
            .background {
                RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
                    .fill(primary ? P.button : Color.clear)
                    .strokeBorder(primary ? Color.clear : P.lineStrong, lineWidth: 1)
            }
            .opacity(configuration.isPressed ? 0.7 : 1)
            .contentShape(Rectangle())
    }
}

/// Views in a row that goes on to the next line where it runs out: a card's buttons, and
/// a choices card's answers.
struct Flow: Layout {
    var spacing = Tokens.Space.s

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        lay(proposal.width ?? .infinity, subviews).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        // Laid at the width it was sized for, not the width that came of it, where the
        // last of a row can come out a hair too wide and go on to a line it has no room for.
        let laid = lay(proposal.width ?? bounds.width, subviews)
        for (v, f) in zip(subviews, laid.frames) {
            v.place(at: CGPoint(x: bounds.minX + f.minX, y: bounds.minY + f.minY), proposal: ProposedViewSize(f.size))
        }
    }

    private func lay(_ width: CGFloat, _ subviews: Subviews) -> (size: CGSize, frames: [CGRect]) {
        var frames: [CGRect] = []
        var x: CGFloat = 0, y: CGFloat = 0, line: CGFloat = 0, widest: CGFloat = 0
        for v in subviews {
            var s = v.sizeThatFits(.unspecified)
            if s.width > width { s = v.sizeThatFits(ProposedViewSize(width: width, height: nil)) }
            if x > 0 && x + s.width > width + 0.5 {
                x = 0
                y += line + spacing
                line = 0
            }
            frames.append(CGRect(x: x, y: y, width: s.width, height: s.height))
            x += s.width + spacing
            line = max(line, s.height)
            widest = max(widest, x - spacing)
        }
        return (CGSize(width: widest, height: y + line), frames)
    }
}

// MARK: - Agents' cards

/// A card an agent put in its reply (PLAN.md, "Cards in the chat"), or loom's own
/// sign-in: choices as buttons that answer, and the rest in the card frame.
struct CardView: View {
    @Environment(Session.self) private var session
    var card: Card
    /// The agent's colour, its one hint on a card.
    var tint: Color

    var body: some View {
        switch card.type {
        case "choices": choices
        case "sign_in": signIn
        case "checklist": checklist
        case "progress": progress
        case "facts":
            CardFrame(card.title) {
                CardLines(lines: card.items.map { Shown.Line(label: $0.text, value: $0.value) })
            }
        case "table": table
        default: EmptyView()
        }
    }

    /// The question, then its answers, which answer while the reply is the chat's newest.
    /// Once the chat has gone on, the one the person sent stays marked and the rest go
    /// quiet.
    private var choices: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
            if !card.title.isEmpty {
                Text(card.title).font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink2)
            }
            Flow {
                ForEach(Array(card.options.enumerated()), id: \.offset) { i, option in
                    if !card.passed {
                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            session.send(session.model.choose(option))
                        } label: {
                            Text(option).chip(edge: tint.opacity(0.55), ink: P.ink)
                        }
                        .buttonStyle(.plain)
                    } else if card.chosen == i {
                        HStack(spacing: 6) {
                            Image(systemName: "checkmark").font(.system(size: 10, weight: .semibold))
                            Text(option)
                        }
                        .chip(edge: P.lineStrong, ink: P.ink2, fill: P.cardInset)
                    } else {
                        Text(option).chip(edge: P.line, ink: P.ink3)
                    }
                }
            }
        }
    }

    /// Loom's own card for an MCP server that needs the person to sign in. The phone
    /// can't sign in to one yet, so it says where to; once the chat has gone on, it's one
    /// quiet line.
    @ViewBuilder
    private var signIn: some View {
        if card.passed {
            CardDone(icon: "key", words: card.title)
        } else {
            CardFrame(card.title, lead: AnyView(CardLead.of(card.server))) {
                Text("Sign in from the desktop app.").font(Fonts.caption).foregroundStyle(P.ink3)
            }
        }
    }

    private var checklist: some View {
        let done = card.items.filter(\.done).count
        return CardFrame(card.title, side: AnyView(Text("\(done) of \(card.items.count)").font(Fonts.caption).foregroundStyle(P.ink3))) {
            VStack(alignment: .leading, spacing: Tokens.Space.xs + 2) {
                ForEach(Array(card.items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .top, spacing: Tokens.Space.s) {
                        Circle()
                            .strokeBorder(item.done ? tint : P.lineStrong, lineWidth: 1)
                            .background(Circle().fill(item.done ? tint : Color.clear))
                            .frame(width: 16, height: 16)
                            .overlay {
                                if item.done {
                                    Image(systemName: "checkmark").font(.system(size: 8, weight: .bold)).foregroundStyle(P.buttonInk)
                                }
                            }
                            .padding(.top, 2)
                        Text(item.text).font(Fonts.small).foregroundStyle(item.done ? P.ink3 : P.ink)
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityValue(item.done ? "Done" : "")
                }
            }
        }
    }

    /// With no title, what's going on is the title.
    private var progress: some View {
        let pct = Int((card.value * 100).rounded())
        let titled = !card.title.isEmpty && !card.label.isEmpty
        return CardFrame(titled ? card.title : card.title + card.label, side: AnyView(Text("\(pct)%").font(Fonts.caption).foregroundStyle(P.ink3))) {
            if titled {
                Text(card.label).font(Fonts.small).foregroundStyle(P.ink2).lineLimit(1)
            }
            Capsule()
                .fill(P.cardInset)
                .frame(height: 6)
                .overlay(alignment: .leading) {
                    GeometryReader { g in
                        Capsule().fill(tint).frame(width: g.size.width * card.value)
                    }
                }
        }
    }

    private var table: some View {
        CardFrame(card.title) {
            Grid(alignment: .leading, horizontalSpacing: Tokens.Space.m, verticalSpacing: Tokens.Space.xs + 2) {
                GridRow {
                    ForEach(Array(card.columns.enumerated()), id: \.offset) { _, c in
                        Text(c).font(Fonts.ui(Tokens.PhoneSize.caption, .medium)).foregroundStyle(P.ink3)
                    }
                }
                Rectangle().fill(P.lineStrong).frame(height: 1).gridCellUnsizedAxes(.horizontal)
                ForEach(Array(card.rows.enumerated()), id: \.offset) { i, row in
                    GridRow {
                        ForEach(card.columns.indices, id: \.self) { k in
                            Text(k < row.count ? row[k] : "").font(Fonts.small).foregroundStyle(P.ink)
                        }
                    }
                    if i < card.rows.count - 1 {
                        Rectangle().fill(P.line).frame(height: 1).gridCellUnsizedAxes(.horizontal)
                    }
                }
            }
            .lineLimit(1)
        }
    }
}

private extension View {
    /// An answer on a choices card.
    func chip(edge: Color, ink: Color, fill: Color = .clear) -> some View {
        self
            .font(Fonts.small)
            .lineLimit(1)
            .foregroundStyle(ink)
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: Tokens.Control.button)
            .background {
                RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
                    .fill(fill)
                    .strokeBorder(edge, lineWidth: 1)
            }
            .contentShape(Rectangle())
    }
}

// MARK: - Questions

/// A question the agent waits on (PLAN.md, "Permissions"), in the card frame: what it
/// would do, as the core put it in words (core/asks.calx), with Details for all of it,
/// then Allow once, Always allow where it's offered, and Deny. Answered here, it's one
/// quiet line saying how, until the instance takes the question away.
struct ApprovalCard: View {
    @Environment(Session.self) private var session
    var approval: Approval
    var tint: Color

    var body: some View {
        if let a = approval.answered {
            let how = !a.allow ? "Denied" : a.always ? "Always allowed" : "Allowed once"
            CardDone(icon: a.allow ? "checkmark" : "xmark", words: approval.title.isEmpty ? how : "\(how) · \(approval.title)")
        } else {
            CardFrame(approval.title.isEmpty ? "Can it go ahead?" : "\(approval.title)?", lead: AnyView(CardLead(icon: icon, tint: tint))) {
                let shown = approval.shown
                if !shown.lines.isEmpty {
                    CardLines(lines: shown.lines)
                }
                if !shown.code.isEmpty {
                    CardCode(text: shown.code)
                }
                if !shown.raw.isEmpty {
                    CardDetails(raw: shown.raw)
                }
                Flow {
                    Button("Allow once") { answer(true) }
                        .buttonStyle(CardButtonStyle(primary: true))
                    if approval.canAlways {
                        Button("Always allow") { answer(true, always: true) }
                            .buttonStyle(CardButtonStyle())
                    }
                    Button("Deny") { answer(false) }
                        .buttonStyle(CardButtonStyle())
                }
                .padding(.top, Tokens.Space.xs)
            }
        }
    }

    /// What kind of call it is, as the core says (Code Mode's script, a tool of an MCP
    /// server or a plugin, one of the person's devices or hosts), or else the kind of tool,
    /// as the desktop's `ask_icon`.
    private var icon: String {
        switch (approval.shown.what, approval.kind) {
        case ("script", _), (_, "execute"): return "terminal"
        case ("mcp", _): return "powerplug"
        case ("plugin", _): return "puzzlepiece.extension"
        case ("device", _): return "laptopcomputer"
        case ("install", _): return "server.rack"
        case (_, "delete"): return "trash"
        case (_, "edit"): return "pencil"
        case (_, "move"): return "box.truck"
        case (_, "fetch"): return "globe"
        case (_, "read"): return "doc.text"
        case (_, "search"): return "magnifyingglass"
        default: return "checkmark.shield"
        }
    }

    private func answer(_ allow: Bool, always: Bool = false) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.send(session.model.approve(approval.call, allow: allow, always: always))
    }
}
