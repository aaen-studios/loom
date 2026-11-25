import SwiftUI

// The cards in a chat, all in one frame, as the desktop's `CardFrame`: the services it
// reaches as badges, its title with the agent's thread beside it, where it goes in small
// mono, what loom knows it does in a pill, then its rows and its buttons. An agent's
// `show_card`, loom's own cards, and the questions an agent waits on all use it.

/// The frame every card in the chat shares. While it waits for the person, the agent's
/// thread beside the title tugs and the edge takes the agent's colour.
struct CardFrame<Content: View>: View {
    var title: String
    var sub = ""
    var lead: AnyView?
    var side: AnyView?
    /// The agent's colour, for the thread beside the title, or none for no thread.
    var tint: Color?
    var waits = false
    var content: Content

    init(_ title: String, sub: String = "", lead: AnyView? = nil, side: AnyView? = nil, tint: Color? = nil, waits: Bool = false, @ViewBuilder content: () -> Content) {
        self.title = title
        self.sub = sub
        self.lead = lead
        self.side = side
        self.tint = tint
        self.waits = waits
        self.content = content()
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            if !title.isEmpty || lead != nil || side != nil {
                // On a phone the title takes the head's width: with a line under it, what
                // goes beside it goes on that line.
                HStack(alignment: sub.isEmpty ? .center : .top, spacing: Tokens.Space.m) {
                    if let lead { lead }
                    VStack(alignment: .leading, spacing: 3) {
                        HStack(alignment: .top, spacing: Tokens.Space.s) {
                            Text(title)
                                .font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                                .foregroundStyle(P.ink)
                                .fixedSize(horizontal: false, vertical: true)
                            if let tint { CardMark(tint: tint, waits: waits).padding(.top, 4) }
                        }
                        if !sub.isEmpty {
                            HStack(spacing: Tokens.Space.s) {
                                Text(sub)
                                    .font(Fonts.mono(Tokens.PhoneSize.caption - 0.5, relativeTo: .caption))
                                    .foregroundStyle(P.ink3)
                                    .lineLimit(1)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                if let side { side }
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    if sub.isEmpty, let side { side }
                }
            }
            content
        }
        .padding(Tokens.Space.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(P.cardFill, in: shape)
        .overlay(shape.strokeBorder(waits ? (tint ?? P.cardEdge).opacity(0.4) : P.cardEdge, lineWidth: 1))
        .shadow(color: .black.opacity(0.07), radius: 14, y: 8)
    }
}

extension CardFrame where Content == EmptyView {
    init(_ title: String, sub: String = "", lead: AnyView? = nil, side: AnyView? = nil, tint: Color? = nil, waits: Bool = false) {
        self.init(title, sub: sub, lead: lead, side: side, tint: tint, waits: waits) { EmptyView() }
    }
}

/// The agent's thread beside a card's title: tugging while the card waits for the
/// person, settled once it's answered.
struct CardMark: View {
    var tint: Color
    var waits: Bool

    var body: some View {
        ThreadView(mood: waits ? .waiting : .idle, color: tint)
            .frame(width: 26, height: 14)
    }
}

/// A service's badge (`shared/badge.calx`): its initial on its colour. Something with a
/// name and no badge of its own gets its initial on a quiet square.
struct BadgeView: View {
    @Environment(Session.self) private var session
    @Environment(\.colorScheme) private var scheme
    var badge: Badge?
    var name = ""
    var size: CGFloat = Tokens.Control.iconLarge
    /// An MCP server's own icon, read and decoded off the main thread.
    @State private var picture: UIImage?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size >= Tokens.Control.iconLarge ? Tokens.Radius.card : Tokens.Radius.row, style: .continuous)
        // Its own icon, once it's fetched, in the initial's place.
        let path = badge.map { $0.icon(dark: scheme == .dark) }.flatMap { $0.isEmpty ? nil : session.model.files["\($0).thumb"] }
        if let image = path.flatMap({ picture ?? Pictures.kept($0) }) {
            Image(uiImage: image)
                .resizable()
                .scaledToFit()
                .padding((size * 0.14).rounded())
                .frame(width: size, height: size)
                .background(shape.fill(P.cardInset).overlay(shape.strokeBorder(P.line, lineWidth: 1)))
                .accessibilityHidden(true)
        } else {
            initialView(shape)
                .task(id: path) {
                    guard let path, Pictures.kept(path) == nil else { return }
                    picture = await Pictures.load(path)
                }
        }
    }

    private func initialView(_ shape: RoundedRectangle) -> some View {
        let initial = badge?.initial ?? name.first(where: { $0.isLetter || $0.isNumber }).map { String($0).uppercased() } ?? "?"
        return Text(initial.isEmpty ? "?" : initial)
            .font(Fonts.ui((size * 0.44).rounded(), .semibold))
            .foregroundStyle(badge == nil ? P.ink2 : P.buttonInk)
            .frame(width: size, height: size)
            .background {
                if let badge {
                    shape.fill(P.thread(badge.color))
                } else {
                    shape.fill(P.cardInset).overlay(shape.strokeBorder(P.line, lineWidth: 1))
                }
            }
            .accessibilityHidden(true)
    }
}

/// A card's badges at its start: one, or two overlapped for a call that reaches two
/// services, each ringed in the card's colour where they cross.
struct Badges: View {
    var badges: [Badge]

    var body: some View {
        if badges.count >= 2 {
            // Far enough apart that the first's initial shows.
            let small = (Tokens.Control.iconLarge * 0.7).rounded()
            let box = Tokens.Control.iconLarge + 6
            let ring = RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous)
            ZStack(alignment: .topLeading) {
                ForEach(0..<2, id: \.self) { i in
                    BadgeView(badge: badges[i], size: small)
                        .overlay(ring.stroke(P.cardFill, lineWidth: 2))
                        .offset(x: i == 0 ? 0 : box - small, y: i == 0 ? 0 : box - small)
                }
            }
            .frame(width: box, height: box, alignment: .topLeading)
        } else if let one = badges.first {
            BadgeView(badge: one)
        }
    }
}

/// A card's icon, for a call that reaches no service: its kind, in the agent's colour.
struct CardIcon: View {
    var icon: String
    var tint: Color

    var body: some View {
        RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
            .fill(P.cardInset)
            .frame(width: Tokens.Control.iconLarge, height: Tokens.Control.iconLarge)
            .overlay(Image(systemName: icon).font(.system(size: 14, weight: .medium)).foregroundStyle(tint))
            .accessibilityHidden(true)
    }
}

/// What a call does, as loom knows it, in a pill: green when it only reads, amber when
/// it can change or run something, red when it deletes.
struct EffectPill: View {
    var kind: String
    var words: String

    var body: some View {
        let ink = kind == "reads" ? P.ok : kind == "deletes" ? P.bad : P.warn
        Text(words)
            .font(Fonts.ui(Tokens.PhoneSize.caption - 1, .medium, relativeTo: .caption))
            .foregroundStyle(ink)
            .lineLimit(1)
            .padding(.horizontal, 9)
            .padding(.vertical, 3)
            .overlay(Capsule().strokeBorder(ink.opacity(0.7), lineWidth: 1))
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

/// A short value in a card, in mono on a chip: a step's argument, a tool's name.
struct CardChip: View {
    var label = ""
    var value: String
    var tint: Color?

    var body: some View {
        HStack(spacing: 4) {
            if !label.isEmpty { Text(label).foregroundStyle(P.ink3) }
            Text(value).foregroundStyle(tint == nil ? P.ink2 : P.ink)
        }
        .font(Fonts.mono(Tokens.PhoneSize.caption - 1, relativeTo: .caption))
        .lineLimit(1)
        .padding(.horizontal, 7)
        .padding(.vertical, 2)
        .background {
            let shape = RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous)
            if let tint {
                shape.fill(tint.opacity(0.08)).overlay(shape.strokeBorder(tint.opacity(0.35), lineWidth: 1))
            } else {
                shape.fill(P.bubble)
            }
        }
    }
}

/// The tools a call runs, one row each in a box of their own: the service's badge, what
/// the step does, and a few of its arguments as chips.
struct CardSteps: View {
    var steps: [Shown.Step]
    var more: Int

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(steps.enumerated()), id: \.offset) { i, step in
                if i > 0 { Rectangle().fill(P.line).frame(height: 1) }
                HStack(alignment: .top, spacing: Tokens.Space.m - 2) {
                    BadgeView(badge: step.badge, name: step.title, size: 22)
                    VStack(alignment: .leading, spacing: 6) {
                        Text(step.title).font(Fonts.small).foregroundStyle(P.ink)
                        if !step.chips.isEmpty || step.more > 0 {
                            Flow(spacing: 5) {
                                ForEach(Array(step.chips.enumerated()), id: \.offset) { _, c in
                                    CardChip(label: c.label, value: c.value)
                                }
                                if step.more > 0 { CardChip(value: "+\(step.more)") }
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(.horizontal, Tokens.Space.m)
                .padding(.vertical, Tokens.Space.m - 2)
                .accessibilityElement(children: .combine)
            }
            if more > 0 {
                Rectangle().fill(P.line).frame(height: 1)
                Text(more == 1 ? "1 more step" : "\(more) more steps")
                    .font(Fonts.caption)
                    .foregroundStyle(P.ink3)
                    .padding(.horizontal, Tokens.Space.m)
                    .padding(.vertical, Tokens.Space.s)
            }
        }
        .background(P.cardInset)
        .clipShape(shape)
        .overlay(shape.strokeBorder(P.line, lineWidth: 1))
    }
}

/// Why the agent makes the call, in its own words: a quote line in its colour.
struct CardReason: View {
    var text: String
    var tint: Color

    var body: some View {
        Text(text)
            .font(Fonts.small)
            .foregroundStyle(P.ink2)
            .lineSpacing(2)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, Tokens.Space.m)
            .overlay(alignment: .leading) { Rectangle().fill(tint.opacity(0.45)).frame(width: 2) }
    }
}

/// All of what a card shows only some of, in a box that scrolls past a few lines.
struct CardRaw: View {
    var raw: String

    var body: some View {
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

/// Details' toggle, quiet at the far end of a card's buttons.
struct DetailsToggle: View {
    @Binding var open: Bool

    var body: some View {
        Button {
            open.toggle()
        } label: {
            HStack(spacing: 4) {
                Text("Details")
                Image(systemName: "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .rotationEffect(.degrees(open ? 90 : 0))
            }
            .font(Fonts.caption)
            .foregroundStyle(P.ink3)
            .frame(minHeight: Tokens.Control.button)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A card once it's answered, or once the chat has gone on: one quiet line, with the
/// service's badge, how it went, and what it was.
struct CardDone<Trailing: View>: View {
    var badge: Badge?
    var name = ""
    var icon: String
    var how: String
    var what = ""
    var trailing: Trailing

    init(badge: Badge? = nil, name: String = "", icon: String, how: String, what: String = "", @ViewBuilder trailing: () -> Trailing = { EmptyView() }) {
        self.badge = badge
        self.name = name
        self.icon = icon
        self.how = how
        self.what = what
        self.trailing = trailing()
    }

    var body: some View {
        HStack(spacing: Tokens.Space.s + 2) {
            Group {
                if badge != nil || !name.isEmpty {
                    BadgeView(badge: badge, name: name, size: 20)
                } else {
                    Image(systemName: icon)
                        .font(.system(size: 10, weight: .semibold))
                        .frame(width: 20, height: 20)
                }
            }
            (Text(how).foregroundStyle(P.ink2).fontWeight(.medium) + Text(what.isEmpty ? "" : " · \(what)").foregroundStyle(P.ink3))
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
            trailing
        }
        .font(Fonts.small)
        .foregroundStyle(P.ink3)
        .padding(.horizontal, 4)
        .accessibilityElement(children: .combine)
    }
}

/// A card's button: the main one filled, the rest outlined, smaller than a page's. Half
/// of a split button rounds only its own end.
struct CardButtonStyle: ButtonStyle {
    enum Half { case whole, leading, trailing }
    var primary = false
    var half = Half.whole

    func makeBody(configuration: Configuration) -> some View {
        let r = Tokens.Radius.button
        let shape = UnevenRoundedRectangle(
            topLeadingRadius: half == .trailing ? 0 : r, bottomLeadingRadius: half == .trailing ? 0 : r,
            bottomTrailingRadius: half == .leading ? 0 : r, topTrailingRadius: half == .leading ? 0 : r, style: .continuous)
        configuration.label
            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
            .lineLimit(1)
            .padding(.horizontal, half == .trailing ? Tokens.Space.s + 1 : Tokens.Space.m + 2)
            .frame(minHeight: Tokens.Control.button)
            .foregroundStyle(primary ? P.buttonInk : P.ink)
            .background {
                shape
                    .fill(primary ? P.button : Color.clear)
                    .strokeBorder(primary ? Color.clear : P.lineStrong, lineWidth: 1)
            }
            .opacity(configuration.isPressed ? 0.7 : 1)
            .contentShape(Rectangle())
    }
}

/// Views in a row that goes on to the next line where it runs out: a card's buttons, and
/// chips.
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

/// A card an agent put in its reply (PLAN.md, "Cards in the chat"), or loom's own: choices
/// as tiles that answer, loom's sign-in and the MCP server an agent proposed, and the rest
/// in the card frame.
struct CardView: View {
    @Environment(Session.self) private var session
    var card: Card
    /// The agent's colour, its one hint on a card.
    var tint: Color
    /// The agent's name, for what a card says about it.
    var agent = ""

    var body: some View {
        switch card.type {
        case "choices": choices
        case "sign_in": signIn
        case "mcp_add": McpAddCard(card: card, tint: tint, agent: agent)
        case "secret_ask": SecretAskCard(card: card, tint: tint)
        case "form": FormCard(card: card, tint: tint)
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

    /// The question with the agent's thread beside it, tugging while it waits, then the
    /// answers as tiles, two to a row, which answer while the reply is the chat's newest
    /// and finished (`takesAnswers`). Once the chat has gone on, the one the person sent
    /// stays marked and the rest go quiet.
    private var choices: some View {
        let rows = stride(from: 0, to: card.options.count, by: 2).map { Array(card.options.indices)[$0..<min($0 + 2, card.options.count)] }
        return CardFrame(card.title, tint: tint, waits: card.takesAnswers) {
            Grid(horizontalSpacing: Tokens.Space.s, verticalSpacing: Tokens.Space.s) {
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        ForEach(Array(row), id: \.self) { i in
                            tile(i).gridCellColumns(row.count == 1 ? 2 : 1)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func tile(_ i: Int) -> some View {
        let option = card.options[i]
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        let words = Text(option)
            .font(Fonts.ui(Tokens.PhoneSize.small, card.passed && card.chosen != i ? .regular : .medium))
            .multilineTextAlignment(.leading)
            .lineLimit(3)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        if card.takesAnswers {
            Button {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                session.send(session.model.choose(option))
            } label: {
                words
                    .foregroundStyle(P.ink)
                    .padding(.horizontal, Tokens.Space.m)
                    .padding(.vertical, Tokens.Space.m - 1)
                    .frame(minHeight: 44)
                    .background(shape.fill(P.cardFill).overlay(shape.strokeBorder(P.lineStrong, lineWidth: 1)))
                    .contentShape(shape)
            }
            .buttonStyle(.plain)
        } else if card.chosen == i {
            HStack(alignment: .top, spacing: Tokens.Space.s) {
                words.foregroundStyle(P.ink)
                Image(systemName: "checkmark").font(.system(size: 11, weight: .semibold)).foregroundStyle(tint).padding(.top, 2)
            }
            .padding(.horizontal, Tokens.Space.m)
            .padding(.vertical, Tokens.Space.m - 1)
            .frame(minHeight: 44)
            .background(shape.fill(tint.opacity(0.08)).overlay(shape.strokeBorder(tint.opacity(0.6), lineWidth: 1)))
            .accessibilityAddTraits(.isSelected)
        } else {
            words
                .foregroundStyle(P.ink3)
                .padding(.horizontal, Tokens.Space.m)
                .padding(.vertical, Tokens.Space.m - 1)
                .frame(minHeight: 44)
                .background(shape.strokeBorder(P.line, lineWidth: 1))
        }
    }

    /// Loom's own card for an MCP server that needs the person to sign in, as the desktop
    /// draws it: the server's badge, "Sign in to acta", and Sign in, which opens its
    /// sign-in page over the app. Once they have, one quiet line says so, as whom when the
    /// sign-in said; and once the chat has gone on without it, one quiet line keeps a small
    /// Sign in. A sign-in that didn't finish says why under it.
    @ViewBuilder
    private var signIn: some View {
        if let who = card.signedAs {
            CardDone(badge: card.badge, name: card.server, icon: "checkmark", how: "Signed in", what: who.isEmpty ? card.server : "\(card.server) as \(who)")
        } else if card.passed {
            VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                CardDone(badge: card.badge, name: card.server, icon: "key", how: card.title) {
                    Button(card.signing ? "Signing in…" : "Sign in") { startSignIn() }
                        .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                        .foregroundStyle(P.ink2)
                        .buttonStyle(.plain)
                        .disabled(card.signing)
                }
                signInError
            }
        } else {
            CardFrame(card.title, lead: AnyView(BadgeView(badge: card.badge, name: card.server)), side: AnyView(
                Button { startSignIn() } label: {
                    HStack(spacing: 5) {
                        Text(card.signing ? "Signing in…" : "Sign in")
                        Image(systemName: "arrow.up.right").font(.system(size: 10, weight: .semibold))
                    }
                }
                .buttonStyle(CardButtonStyle(primary: true))
                .disabled(card.signing)
            ), tint: tint, waits: true) {
                signInError
            }
        }
    }

    @ViewBuilder
    private var signInError: some View {
        if let why = card.signInError {
            Text(why).font(Fonts.caption).foregroundStyle(P.bad)
        }
    }

    private func startSignIn() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.signIn(card.server)
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

/// loom's mark, the knot (design/logo/knot.svg): one thread with a loop that crosses
/// itself, the falling stroke passing over the rising one.
struct Knot: View {
    var body: some View {
        Canvas { ctx, size in
            let k = min(size.width, size.height) / 32
            ctx.scaleBy(x: k, y: k)
            let style = StrokeStyle(lineWidth: 2.6, lineCap: .round, lineJoin: .round)
            var rising = Path()
            rising.move(to: CGPoint(x: 3, y: 24.5))
            rising.addLine(to: CGPoint(x: 12, y: 24.5))
            rising.addCurve(to: CGPoint(x: 22, y: 14), control1: CGPoint(x: 19, y: 24.5), control2: CGPoint(x: 22, y: 18.5))
            rising.addCurve(to: CGPoint(x: 16, y: 7), control1: CGPoint(x: 22, y: 9.5), control2: CGPoint(x: 19, y: 7))
            rising.addCurve(to: CGPoint(x: 10, y: 14), control1: CGPoint(x: 13, y: 7), control2: CGPoint(x: 10, y: 9.5))
            var gap = Path()
            gap.move(to: CGPoint(x: 13.67, y: 22.24))
            gap.addCurve(to: CGPoint(x: 18.14, y: 24.34), control1: CGPoint(x: 14.8, y: 23.3), control2: CGPoint(x: 16.3, y: 24.1))
            var falling = Path()
            falling.move(to: CGPoint(x: 10, y: 14))
            falling.addCurve(to: CGPoint(x: 20, y: 24.5), control1: CGPoint(x: 10, y: 19), control2: CGPoint(x: 13.5, y: 24.5))
            falling.addLine(to: CGPoint(x: 29, y: 24.5))
            ctx.drawLayer { layer in
                layer.stroke(rising, with: .color(P.ink), style: style)
                layer.blendMode = .destinationOut
                layer.stroke(gap, with: .color(.black), style: StrokeStyle(lineWidth: 5.6, lineCap: .round))
            }
            ctx.stroke(falling, with: .color(P.ink), style: style)
        }
        .accessibilityHidden(true)
    }
}

/// The agent's thread from loom's knot to a server's badge: slack and drifting while the
/// server waits to be added, pulled straight, with a twang, once it's connected.
struct SpanView: View {
    var taut: Bool
    var tint: Color
    @State private var sag: Double = 1

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: taut)) { context in
            Span(sag: sag, phase: context.date.timeIntervalSinceReferenceDate)
                .stroke(tint, style: StrokeStyle(lineWidth: Tokens.ThreadStroke.small, lineCap: .round))
        }
        .frame(height: 24)
        .onAppear { sag = taut ? 0 : 1 }
        .onChange(of: taut) { _, now in
            withAnimation(.spring(response: 0.55, dampingFraction: 0.3)) { sag = now ? 0 : 1 }
        }
        .accessibilityHidden(true)
    }

    private struct Span: Shape {
        var sag: Double
        var phase: Double
        var animatableData: Double {
            get { sag }
            set { sag = newValue }
        }

        func path(in r: CGRect) -> Path {
            var p = Path()
            let steps = max(Int(r.width / 3), 8)
            for i in 0...steps {
                let u = Double(i) / Double(steps)
                let hang = sin(.pi * u) * 0.6
                let drift = sin(2 * .pi * (u * 1.5 - phase * 0.2)) * 0.25 * sin(.pi * u)
                let pt = CGPoint(x: r.minX + u * r.width, y: r.midY + (hang + drift) * sag * r.height / 2)
                if i == 0 { p.move(to: pt) } else { p.addLine(to: pt) }
            }
            return p
        }
    }
}

/// Loom's own card for an MCP server an agent proposed: loom's knot and the server's
/// badge joined by the agent's thread, slack while it waits and pulled straight once it's
/// connected; what would be added, with a masked field for each key the person doesn't
/// have yet; and Add or Cancel, then Sign in and Later for one that needs it. Connected,
/// it lists its tools; settled otherwise, it's one quiet line.
struct McpAddCard: View {
    @Environment(Session.self) private var session
    var card: Card
    var tint: Color
    var agent: String
    /// What the person typed for each secret it needs, by the secret's name.
    @State private var typed: [String: String] = [:]

    var body: some View {
        let signed = card.signedAs != nil
        if let done = done(signed: signed) {
            CardDone(badge: card.badge, name: card.server, icon: done.icon, how: done.how, what: card.server)
        } else if card.state == "ready" {
            CardFrame("") {
                link
                Label("\(card.server) is connected", systemImage: "checkmark")
                    .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                    .foregroundStyle(P.ok)
                if !card.tools.isEmpty {
                    Flow(spacing: 6) {
                        ForEach(Array(card.tools.prefix(6).enumerated()), id: \.offset) { _, t in
                            CardChip(value: t, tint: tint)
                        }
                        if card.tools.count > 6 { CardChip(value: "+ \(card.tools.count - 6)") }
                    }
                }
            }
        } else {
            let waits = ["asking", "adding", "sign_in"].contains(card.state)
            CardFrame(card.title, sub: card.url.isEmpty ? ([card.command] + card.args).joined(separator: " ") : card.url, tint: tint, waits: waits) {
                link
                if !card.settings.isEmpty { fields }
                if typing {
                    Text("The key goes to your secrets. \(agent.isEmpty ? "The agent" : agent) never sees it.")
                        .font(Fonts.caption)
                        .foregroundStyle(P.ink3)
                }
                if let line = line {
                    Text(line.text).font(Fonts.small).foregroundStyle(line.bad ? P.bad : P.ink3)
                }
                actions
            }
        }
    }

    /// loom's knot, the thread, and the server's badge.
    private var link: some View {
        HStack(spacing: Tokens.Space.xs) {
            Knot()
                .frame(width: 20, height: 20)
                .frame(width: Tokens.Control.iconLarge, height: Tokens.Control.iconLarge)
                .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
            SpanView(taut: card.state == "ready", tint: tint)
            BadgeView(badge: card.badge, name: card.server)
        }
    }

    /// The one quiet line of a settled card.
    private func done(signed: Bool) -> (icon: String, how: String)? {
        switch card.state {
        case "added": return ("checkmark", "Added")
        case "later" where signed: return ("checkmark", "Added and signed in")
        case "cancelled": return ("xmark", "Not added")
        default: return nil
        }
    }

    /// Whether a key is being typed in here.
    private var typing: Bool {
        card.state == "asking" && card.settings.contains { !$0.secret.isEmpty && $0.wanted }
    }

    /// Each setting: its value, or a masked field for a key the person doesn't have yet.
    private var fields: some View {
        Grid(alignment: .leading, horizontalSpacing: Tokens.Space.m, verticalSpacing: Tokens.Space.s) {
            ForEach(Array(card.settings.enumerated()), id: \.offset) { i, s in
                GridRow {
                    Text(s.name).foregroundStyle(P.ink3).lineLimit(1)
                    Group {
                        if s.secret.isEmpty {
                            Text(s.value).font(Fonts.code).foregroundStyle(P.ink2).lineLimit(1)
                        } else if card.state == "asking" && s.wanted && first(i) {
                            HStack(spacing: Tokens.Space.s) {
                                Image(systemName: "lock").font(.system(size: 11, weight: .medium)).foregroundStyle(P.ink3)
                                SecureField(s.secret, text: typing(s.secret))
                                    .font(Fonts.code)
                                    .textInputAutocapitalization(.never)
                                    .autocorrectionDisabled()
                            }
                            .padding(.horizontal, Tokens.Space.m)
                            .frame(minHeight: Tokens.Control.buttonSmall)
                            .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
                            .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
                        } else {
                            Text(s.have ? "Your secret \(s.secret)" : "Secret \(s.secret)").foregroundStyle(P.ink3).lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .font(Fonts.small)
    }

    /// A secret two settings name is typed in once, at the first.
    private func first(_ i: Int) -> Bool {
        !card.settings[..<i].contains { $0.secret == card.settings[i].secret }
    }

    private func typing(_ secret: String) -> Binding<String> {
        Binding(get: { typed[secret, default: ""] }, set: { typed[secret] = $0 })
    }

    /// How it stands, in a line, and whether it's trouble.
    private var line: (text: String, bad: Bool)? {
        // A sign-in here that didn't finish says why.
        if card.signsIn, let why = card.signInError { return (why, true) }
        switch card.state {
        case "adding": return ("Starting \(card.server)…", false)
        case "sign_in": return card.said.isEmpty ? nil : (card.said, true)
        case "later": return ("Added. Sign in to use \(card.server).", false)
        case "failed": return ("It didn't start: \(card.said)", true)
        default: return nil
        }
    }

    @ViewBuilder
    private var actions: some View {
        switch card.state {
        case "asking":
            Flow {
                Button("Add") { answer("add") }.buttonStyle(CardButtonStyle(primary: true))
                Button("Cancel") { answer("cancel") }.buttonStyle(CardButtonStyle())
            }
        case "sign_in":
            Flow {
                signInButton
                Button("Later") { answer("later") }.buttonStyle(CardButtonStyle())
            }
        case "later":
            signInButton
        default:
            EmptyView()
        }
    }

    /// Sign in, which opens the server's sign-in page over the app, as on a sign-in card.
    private var signInButton: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            session.signIn(card.server)
        } label: {
            HStack(spacing: 5) {
                Text(card.signing ? "Signing in…" : "Sign in")
                Image(systemName: "arrow.up.right").font(.system(size: 10, weight: .semibold))
            }
        }
        .buttonStyle(CardButtonStyle(primary: true))
        .disabled(card.signing)
    }

    private func answer(_ answer: String) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        let secrets = answer == "add" ? typed.filter { name, _ in card.settings.contains { $0.secret == name && $0.wanted } } : [:]
        session.send(session.model.answerMcpAdd(card.call, answer, secrets: secrets))
    }
}

/// Loom's own card for something an agent's page wants that only the person knows: the
/// site's badge, or a key for a file, what's asked, with the site under it, and a field
/// for each thing asked, its label over its box so the box has the card's width, masked
/// as the card says, with Save as and the name it's kept as where it can be kept; then
/// Fill and Not now. Once it's answered, one quiet line says how.
struct SecretAskCard: View {
    @Environment(Session.self) private var session
    var card: Card
    var tint: Color
    /// What the person typed in each field, the name each is kept as, and whether Save as
    /// is on for each, by the field's place.
    @State private var typed: [Int: String] = [:]
    @State private var names: [Int: String] = [:]
    @State private var off: Set<Int> = []
    /// The masked fields the person chose to see.
    @State private var shown: Set<Int> = []

    var body: some View {
        switch card.state {
        case "filled":
            CardDone(badge: card.badge, icon: "checkmark", how: "Filled in", what: card.saved.isEmpty ? card.site : "\(card.site), kept as \(card.saved.joined(separator: ", "))")
        case "declined":
            CardDone(badge: card.badge, icon: "xmark", how: "Not now", what: card.title)
        case "closed":
            CardDone(badge: card.badge, icon: "xmark", how: "Not answered", what: card.title)
        default:
            CardFrame(card.title, sub: card.site, lead: AnyView(lead), tint: tint, waits: true) {
                VStack(alignment: .leading, spacing: Tokens.Space.s) {
                    ForEach(Array(card.fields.enumerated()), id: \.offset) { k, f in
                        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
                            Text(f.label).foregroundStyle(P.ink3).lineLimit(1)
                            // A masked box has an eye, to see what's typed.
                            HStack(spacing: Tokens.Space.xs) {
                                Group {
                                    if f.masked && !shown.contains(k) {
                                        SecureField(f.label, text: bound(k, in: $typed))
                                    } else {
                                        TextField(f.label, text: bound(k, in: $typed))
                                            .keyboardType(f.kind == "code" ? .numberPad : f.kind == "username" ? .emailAddress : .default)
                                    }
                                }
                                .textContentType(f.kind == "code" ? .oneTimeCode : f.kind == "username" ? .username : .password)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                if f.masked {
                                    Button {
                                        if shown.contains(k) { shown.remove(k) } else { shown.insert(k) }
                                    } label: {
                                        Image(systemName: shown.contains(k) ? "eye.slash" : "eye").foregroundStyle(P.ink3)
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityLabel(shown.contains(k) ? "Hide" : "Show")
                                }
                            }
                            .boxed()
                            if !f.save.isEmpty {
                                HStack(spacing: Tokens.Space.s) {
                                    Toggle(isOn: Binding(get: { !off.contains(k) }, set: { on in if on { off.remove(k) } else { off.insert(k) } })) {
                                        Text("Save as")
                                    }
                                    .toggleStyle(Tick())
                                    .foregroundStyle(P.ink2)
                                    .fixedSize()
                                    if !off.contains(k) {
                                        TextField("Name", text: Binding(get: { names[k] ?? f.save }, set: { names[k] = $0 }))
                                            .textInputAutocapitalization(.never)
                                            .autocorrectionDisabled()
                                            .boxed()
                                    }
                                }
                            }
                        }
                    }
                }
                .font(Fonts.small)
                if !card.error.isEmpty {
                    Text(card.error).font(Fonts.small).foregroundStyle(P.bad)
                }
                Flow {
                    Button(card.sending ? "Filling in…" : "Fill") { fill() }.buttonStyle(CardButtonStyle(primary: true))
                    Button("Not now") { notNow() }.buttonStyle(CardButtonStyle())
                }
                .padding(.top, Tokens.Space.xs)
            }
        }
    }

    /// The site's badge, or a key for a file on this computer.
    @ViewBuilder private var lead: some View {
        if let badge = card.badge {
            BadgeView(badge: badge)
        } else {
            CardIcon(icon: "key", tint: tint)
        }
    }

    private func bound(_ k: Int, in map: Binding<[Int: String]>) -> Binding<String> {
        Binding(get: { map.wrappedValue[k] ?? "" }, set: { map.wrappedValue[k] = $0 })
    }

    private func fill() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        let values = card.fields.indices.map { typed[$0] ?? "" }
        let kept = card.fields.indices.map { names[$0] ?? card.fields[$0].save }
        let keep = card.fields.indices.map { !card.fields[$0].save.isEmpty && !off.contains($0) }
        if let f = session.model.answerSecret(agent: card.agent, call: card.call, values: values, names: kept, keep: keep) {
            session.send(f)
        }
    }

    private func notNow() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.send(session.model.declineSecret(agent: card.agent, call: card.call))
    }
}

/// Several questions an agent asks at once (show_card's `form`), as the desktop's
/// `form_card`: each field under its label, text and numbers in a box, a choice as its
/// options side by side, a toggle as a switch beside its label, then Send, which sends
/// them as one answer. Sent, it's one quiet line, as it is once the chat has gone on
/// without it, and while its reply is still being written, when it says it opens once
/// that's done.
struct FormCard: View {
    @Environment(Session.self) private var session
    var card: Card
    var tint: Color
    /// What the person typed in each text and number field, by the field's place.
    @State private var typed: [Int: String] = [:]

    var body: some View {
        if !card.answers.isNull {
            CardDone(icon: "checkmark", how: "Sent", what: card.title)
        } else if card.answersLater {
            CardDone(icon: "clock", how: "Opens once the reply is done", what: card.title)
        } else if !card.takesAnswers {
            CardDone(icon: "xmark", how: "Not sent", what: card.title)
        } else {
            CardFrame(card.title, lead: AnyView(CardIcon(icon: "list.clipboard", tint: tint)), tint: tint, waits: true) {
                VStack(alignment: .leading, spacing: Tokens.Space.m) {
                    ForEach(Array(card.formFields.enumerated()), id: \.offset) { k, f in
                        field(k, f)
                    }
                }
                .font(Fonts.small)
                if !card.error.isEmpty {
                    Text(card.error).font(Fonts.small).foregroundStyle(P.bad)
                }
                Flow {
                    Button(card.sending ? "Sending…" : "Send") { send() }.buttonStyle(CardButtonStyle(primary: true))
                }
                .padding(.top, Tokens.Space.xs)
            }
        }
    }

    private func label(_ f: FormField) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: Tokens.Space.s) {
            Text(f.label).fontWeight(.medium).foregroundStyle(P.ink).lineLimit(2)
            if !f.required && f.kind != "toggle" {
                Text("Optional").font(Fonts.caption).foregroundStyle(P.ink3)
            }
        }
    }

    @ViewBuilder
    private func field(_ k: Int, _ f: FormField) -> some View {
        let pick = k < card.picks.count ? card.picks[k] : .null
        switch f.kind {
        case "toggle":
            Toggle(isOn: Binding(get: { pick.bool ?? false }, set: { on in self.pick(k, .bool(on)) })) {
                label(f)
            }
            .tint(P.button)
        case "choice":
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                label(f)
                Flow {
                    ForEach(f.options, id: \.self) { o in
                        let chosen = pick.string == o
                        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
                        Button { self.pick(k, .string(o)) } label: {
                            Text(o)
                                .font(Fonts.ui(Tokens.PhoneSize.small, chosen ? .medium : .regular))
                                .foregroundStyle(chosen ? P.ink : P.ink2)
                                .padding(.horizontal, Tokens.Space.m)
                                .frame(minHeight: Tokens.Control.buttonSmall)
                                .background(shape.fill(chosen ? tint.opacity(0.14) : Color.clear).overlay(shape.strokeBorder(chosen ? tint : P.lineStrong, lineWidth: 1)))
                                .contentShape(shape)
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(chosen ? .isSelected : [])
                    }
                }
            }
        default:
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                label(f)
                TextField(f.placeholder, text: Binding(get: { typed[k] ?? f.startText }, set: { typed[k] = $0 }))
                    .keyboardType(f.kind == "number" ? .decimalPad : .default)
                    .boxed()
            }
        }
    }

    private func pick(_ k: Int, _ value: JSON) {
        UISelectionFeedbackGenerator().selectionChanged()
        session.model.pickForm(message: card.message, card: card.id, field: k, value: value)
    }

    private func send() {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        let values = card.formFields.indices.map { typed[$0] ?? card.formFields[$0].startText }
        if let f = session.model.sendForm(message: card.message, card: card.id, typed: values) {
            session.send(f)
        }
    }
}

/// A tick before its words, as Save as has.
private struct Tick: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button {
            configuration.isOn.toggle()
        } label: {
            HStack(spacing: 6) {
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(configuration.isOn ? P.button : Color.clear)
                    .strokeBorder(configuration.isOn ? Color.clear : P.lineStrong, lineWidth: 1)
                    .frame(width: 16, height: 16)
                    .overlay {
                        if configuration.isOn {
                            Image(systemName: "checkmark").font(.system(size: 8, weight: .bold)).foregroundStyle(P.buttonInk)
                        }
                    }
                configuration.label.lineLimit(1)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(configuration.isOn ? .isSelected : [])
    }
}

private extension View {
    /// A field on a card, in its box, as the MCP server card's key is.
    func boxed() -> some View {
        self
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: Tokens.Control.buttonSmall)
            .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
    }
}

// MARK: - Questions

/// A question the agent waits on (PLAN.md, "Permissions"), in the card frame, as the core
/// put the call in words (core/asks.calx): the services it reaches as badges, what it
/// does in plain words with the agent's thread tugging beside it, where it goes in small
/// mono, and what loom knows it does in a pill; then the tools it runs as steps, or its
/// arguments as lines, the command or script it runs, and why the agent makes it. Allow
/// is a split button, its menu holding Always allow where that's offered, then Deny, and
/// Details for all of it; on a risky one, Allow is held (`HoldButton`). Answered here,
/// it's one quiet line, until the instance takes the question away.
struct ApprovalCard: View {
    @Environment(Session.self) private var session
    var approval: Approval
    var tint: Color
    @State private var open = false

    var body: some View {
        let shown = approval.shown
        let title = !shown.title.isEmpty ? shown.title : approval.title.isEmpty ? "Can it go ahead?" : approval.title
        if let a = approval.answered {
            let how = !a.allow ? "Denied" : a.always ? "Always allowed" : "Allowed once"
            CardDone(badge: shown.badges.first, icon: a.allow ? "checkmark" : "xmark", how: how, what: title)
        } else {
            CardFrame(
                title, sub: shown.place,
                lead: AnyView(Group {
                    if shown.badges.isEmpty { CardIcon(icon: icon, tint: tint) } else { Badges(badges: shown.badges) }
                }),
                side: shown.effectWords.isEmpty ? nil : AnyView(EffectPill(kind: shown.effect, words: shown.effectWords)),
                tint: tint, waits: true
            ) {
                if !shown.steps.isEmpty {
                    CardSteps(steps: shown.steps, more: shown.stepsMore)
                }
                if !shown.lines.isEmpty {
                    CardLines(lines: shown.lines)
                }
                if !shown.code.isEmpty {
                    CardCode(text: shown.code)
                }
                if !shown.reason.isEmpty {
                    CardReason(text: shown.reason, tint: tint)
                }
                if open && !shown.raw.isEmpty {
                    CardRaw(raw: shown.raw)
                }
                HStack(spacing: Tokens.Space.s) {
                    HStack(spacing: 1) {
                        if approval.risky {
                            HoldButton { answer(true) }
                        } else {
                            Button("Allow") { answer(true) }
                                .buttonStyle(CardButtonStyle(primary: true, half: approval.canAlways ? .leading : .whole))
                        }
                        if approval.canAlways && !approval.risky {
                            Menu {
                                Button { answer(true, always: true) } label: {
                                    Label("Always allow", systemImage: "checkmark.shield")
                                }
                            } label: {
                                Image(systemName: "chevron.down").font(.system(size: 11, weight: .semibold))
                            }
                            .buttonStyle(CardButtonStyle(primary: true, half: .trailing))
                            .accessibilityLabel("More ways to allow")
                        }
                    }
                    Button("Deny") { answer(false) }
                        .buttonStyle(CardButtonStyle())
                    Spacer(minLength: 0)
                    if !shown.raw.isEmpty {
                        DetailsToggle(open: $open)
                    }
                }
                .animation(.easeOut(duration: 0.15), value: open)
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

/// Allow on a risky question (PLAN.md, "Permissions"), as the desktop's `hold_button`:
/// pressed and held, it fills from its leading edge, and once it's full (`holdTime`) the
/// question is allowed. Let go sooner, it says to hold.
struct HoldButton: View {
    var allow: () -> Void
    @State private var since: Date?
    @State private var hint = false
    @State private var done = false

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
        TimelineView(.animation(paused: since == nil)) { t in
            let k = since.map { min(t.date.timeIntervalSince($0) / holdTime, 1) } ?? 0
            Text(since != nil ? "Keep holding" : hint ? "Hold to allow" : "Allow")
                .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                .lineLimit(1)
                .padding(.horizontal, Tokens.Space.m + 2)
                .frame(minHeight: Tokens.Control.button)
                .foregroundStyle(P.buttonInk)
                .background {
                    GeometryReader { g in
                        ZStack(alignment: .leading) {
                            shape.fill(P.button)
                            Rectangle().fill(P.bad.opacity(0.75)).frame(width: g.size.width * k)
                        }
                        .clipShape(shape)
                    }
                }
                .contentShape(shape)
        }
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in press() }
                .onEnded { _ in release() }
        )
        .accessibilityLabel("Allow")
        .accessibilityHint("Hold to allow")
        .accessibilityAddTraits(.isButton)
    }

    private func press() {
        guard since == nil, !done else { return }
        let start = Date.now
        since = start
        hint = false
        UIImpactFeedbackGenerator(style: .soft).impactOccurred()
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(holdTime))
            // Still held since then: it's allowed, without waiting to be let go.
            guard since == start else { return }
            since = nil
            done = true
            allow()
        }
    }

    private func release() {
        guard since != nil else { return }
        since = nil
        hint = true
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(2.4))
            hint = false
        }
    }
}
