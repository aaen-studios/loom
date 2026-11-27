import SwiftUI

// Acta beside the chat, on the phone (PLAN.md, "Acta beside the chat"): a card's preview
// in a sheet, the form that makes a card from a message in a sheet, and a board pushed as
// a screen of its own, with Follow on the sheet and the board. The model (Acta.swift)
// says what each shows and what the person's changes send.

/// A small chip, as a card's preview has them: its status in its colour, its assignee,
/// its size, a label or its priority.
struct SmallChip: View {
    var words: String
    var ink: Color = P.ink2
    var edge: Color = P.lineStrong
    var chevron = false

    var body: some View {
        HStack(spacing: 4) {
            Text(words)
            if chevron { Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold)) }
        }
        .font(Fonts.ui(Tokens.PhoneSize.caption, .medium, relativeTo: .caption))
        .foregroundStyle(ink)
        .lineLimit(1)
        .padding(.horizontal, 8)
        .frame(minHeight: Tokens.Control.pill)
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous).strokeBorder(edge, lineWidth: 1))
    }
}

/// A choice that's on or off, as the form's labels are: filled while it's on.
struct ChoiceChip: View {
    var words: String
    var on: Bool
    var pick: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous)
        Button(action: pick) {
            Text(words)
                .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                .foregroundStyle(on ? P.buttonInk : P.ink)
                .lineLimit(1)
                .padding(.horizontal, Tokens.Space.m)
                .frame(minHeight: Tokens.Control.chip)
                .background(shape.fill(on ? P.button : Color.clear))
                .overlay(shape.strokeBorder(on ? Color.clear : P.lineStrong, lineWidth: 1))
                .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// Segments of which one or none is picked: picking the one that's on picks none, as the
/// form's Size and Priority do.
struct Segments: View {
    var options: [(id: String, label: String)]
    var on: String?
    var pick: (String) -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
        HStack(spacing: 2) {
            ForEach(options, id: \.id) { o in
                let picked = on == o.id
                Button {
                    pick(o.id)
                } label: {
                    Text(o.label)
                        .font(Fonts.ui(Tokens.PhoneSize.small, picked ? .semibold : .regular))
                        .foregroundStyle(picked ? P.ink : P.ink2)
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                        .frame(maxWidth: .infinity, minHeight: Tokens.Control.buttonSmall)
                        .background {
                            if picked {
                                RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous)
                                    .fill(P.cardFill)
                                    .shadow(color: .black.opacity(0.08), radius: 2, y: 1)
                            }
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(picked ? .isSelected : [])
            }
        }
        .padding(2)
        .background(P.cardInset, in: shape)
        .overlay(shape.strokeBorder(P.line, lineWidth: 1))
    }
}

/// Follow with a bell, or Following with a tick, for a board or one card on it in the chat
/// shown: a watch whose lines come there. Nothing until the person's watches are known.
struct FollowButton: View {
    @Environment(Session.self) private var session
    var board: String
    var card: String?

    var body: some View {
        if let following = session.model.following(board: board, card: card) {
            Button {
                session.send(session.model.toggleFollow(board: board, card: card))
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: following ? "checkmark" : "bell").font(.system(size: 12, weight: .semibold))
                    Text(following ? "Following" : "Follow")
                }
            }
            .buttonStyle(.loomQuiet)
        }
    }
}

// MARK: A card's preview

/// A card's preview, in a sheet from its pill or a board's row: its board and id, its
/// title, its chips with its status's menu that moves it, its description, its subtasks
/// with ticks, its last comments, and Open in Acta and Follow.
struct CardSheetView: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    var body: some View {
        let model = session.model
        let s = model.acta.sheet
        NavigationStack {
            ScrollView {
                if let s { page(s) }
            }
            .background(P.bg)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    if let s {
                        HStack(spacing: Tokens.Space.s) {
                            Pill.mark(16).accessibilityHidden(true)
                            Text("\(boardLabel(s)) · \(s.id)")
                                .font(Fonts.mono(Tokens.PhoneSize.small - 1))
                                .foregroundStyle(P.ink2)
                                .lineLimit(1)
                        }
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if let s { foot(s) }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    private func boardLabel(_ s: CardSheet) -> String {
        if let n = s.detail?.card.boardName, !n.isEmpty { return n }
        return session.model.boardName(s.board)
    }

    @ViewBuilder
    private func page(_ s: CardSheet) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.l) {
            if let d = s.detail {
                Text(d.card.title)
                    .font(Fonts.ui(Tokens.Size.heading, .semibold, relativeTo: .title2))
                    .foregroundStyle(d.card.struck ? P.ink3 : P.ink)
                    .strikethrough(d.card.struck)
                    .fixedSize(horizontal: false, vertical: true)
                chips(d)
                if let e = s.error { Text(e).font(Fonts.small).foregroundStyle(P.bad) }
                let words = d.description.trimmingCharacters(in: .whitespacesAndNewlines)
                if !words.isEmpty {
                    Text(words)
                        .font(Fonts.body)
                        .foregroundStyle(P.ink)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !d.checklist.isEmpty { subtasks(d) }
                if !d.comments.isEmpty { comments(d) }
            } else if let e = s.error {
                Text(e).font(Fonts.small).foregroundStyle(P.bad)
            } else {
                ProgressView().frame(maxWidth: .infinity)
            }
        }
        .padding(.horizontal, Tokens.PhoneSpace.gutter)
        .padding(.top, Tokens.Space.s)
        .padding(.bottom, Tokens.Space.xl)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func chips(_ d: CardDetail) -> some View {
        let model = session.model
        let ink = d.card.ink.color
        return Flow(spacing: 6) {
            Menu {
                ForEach(cardStatuses, id: \.self) { status in
                    Button {
                        session.send(model.moveCard(status))
                    } label: {
                        if d.card.status == status {
                            Label(statusName(status), systemImage: "checkmark")
                        } else {
                            Text(statusName(status))
                        }
                    }
                }
            } label: {
                SmallChip(words: d.card.statusWords, ink: ink, edge: ink.opacity(0.6), chevron: true)
            }
            .accessibilityLabel("Status: \(d.card.statusWords)")
            ForEach(Array(d.chips.enumerated()), id: \.offset) { _, chip in
                SmallChip(words: chip)
            }
        }
    }

    private func subtasks(_ d: CardDetail) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(d.card.subtasksLine ?? "Subtasks")
                .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                .foregroundStyle(P.ink2)
                .padding(.bottom, Tokens.Space.xs)
            ForEach(d.checklist) { t in
                Button {
                    session.send(session.model.toggleSubtask(t.id))
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: Tokens.Space.s + 2) {
                        let box = RoundedRectangle(cornerRadius: 4, style: .continuous)
                        box.fill(t.done ? P.ok : Color.clear)
                            .overlay(box.strokeBorder(t.done ? P.ok : P.lineStrong, lineWidth: 1))
                            .overlay {
                                if t.done { Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)).foregroundStyle(P.bg) }
                            }
                            .frame(width: 18, height: 18)
                            .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 5 }
                        Text(t.title)
                            .font(Fonts.small)
                            .foregroundStyle(t.done ? P.ink3 : P.ink)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.vertical, 6)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(t.title)
                .accessibilityAddTraits(t.done ? .isSelected : [])
            }
        }
    }

    private func comments(_ d: CardDetail) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            Text("Comments")
                .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                .foregroundStyle(P.ink2)
            ForEach(Array(d.comments.enumerated()), id: \.offset) { _, c in
                let day = commentDay(c.at)
                VStack(alignment: .leading, spacing: 2) {
                    Text(day.isEmpty ? c.by : "\(c.by) · \(day)").font(Fonts.caption).foregroundStyle(P.ink3)
                    Text(c.body)
                        .font(Fonts.small)
                        .foregroundStyle(P.ink)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func foot(_ s: CardSheet) -> some View {
        let link = s.detail?.card.link ?? "https://\(actaHost)/\(s.board)#\(s.id)"
        return HStack(spacing: Tokens.Space.s) {
            Button("Open in Acta") {
                if let url = URL(string: link) { openURL(url) }
            }
            .buttonStyle(.loomQuiet)
            FollowButton(board: s.board, card: s.id)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Tokens.PhoneSpace.gutter)
        .padding(.vertical, Tokens.Space.m)
        .background(P.bg)
        .overlay(alignment: .top) { Rectangle().fill(P.line).frame(height: 1) }
    }
}

// MARK: Making a card

/// The form that makes a card from a message, in a sheet from its menu: Board, Title,
/// Description, Labels, Size and Priority, and Make.
struct MakeCardSheet: View {
    @Environment(Session.self) private var session
    @State private var title = ""
    @State private var text = ""

    var body: some View {
        let model = session.model
        NavigationStack {
            Form {
                if let f = model.acta.form {
                    Section {
                        Picker("Board", selection: Binding(get: { f.board }, set: { session.send(model.formBoard($0)) })) {
                            ForEach(model.boardChoices, id: \.board) { b in
                                Text(b.name).tag(b.board)
                            }
                        }
                    }
                    Section("Title") {
                        TextField("Title", text: $title, axis: .vertical)
                            .lineLimit(1...4)
                    }
                    Section("Description") {
                        TextField("Description", text: $text, axis: .vertical)
                            .lineLimit(3...12)
                    }
                    if !model.formLabels.isEmpty {
                        Section("Labels") {
                            Flow {
                                ForEach(model.formLabels, id: \.self) { l in
                                    ChoiceChip(words: l, on: f.labels.contains(l)) { model.toggleLabel(l) }
                                }
                            }
                            .padding(.vertical, 4)
                        }
                    }
                    Section("Size") {
                        Segments(options: cardSizes.map { ($0, $0.uppercased()) }, on: f.size) { model.formSize($0) }
                    }
                    Section("Priority") {
                        Segments(options: cardPriorities.map { ($0, priorityName($0)) }, on: f.priority) { model.formPriority($0) }
                    }
                    if let e = f.error {
                        Section {
                            Text(e).font(Fonts.small).foregroundStyle(P.bad)
                        }
                    }
                }
            }
            .font(Fonts.body)
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .navigationTitle("Make a card")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { model.acta.form = nil }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if model.acta.form?.making == true {
                        ProgressView()
                    } else {
                        Button("Make") { session.send(model.makeCard(title: title, description: text)) }
                            .disabled(!model.canMake(title))
                    }
                }
            }
            .onAppear {
                title = model.acta.form?.title ?? ""
                text = model.acta.form?.description ?? ""
            }
        }
    }
}

// MARK: A board

/// A board, pushed from the + sheet's Boards: its picker and Follow at the top, Todo, In
/// progress, Review and Done to pick from, and the cards of the one picked. A row opens
/// the card's preview, and its menu moves it.
struct BoardScreenView: View {
    @Environment(Session.self) private var session

    var body: some View {
        let model = session.model
        let b = model.acta.screen
        List {
            if let b {
                if let e = b.error {
                    Text(e).font(Fonts.small).foregroundStyle(P.bad)
                        .listRowBackground(Color.clear)
                }
                ForEach(b.shown) { t in
                    BoardRow(task: t) {
                        session.send(model.openCard(board: b.board, id: t.id))
                    }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 0, leading: Tokens.PhoneSpace.gutter, bottom: 0, trailing: Tokens.PhoneSpace.gutter))
                    .contextMenu {
                        ForEach(boardColumns.filter { $0 != t.status }, id: \.self) { status in
                            Button("Move to \(statusName(status))") { session.send(model.moveTask(t.id, to: status)) }
                        }
                    }
                }
                if b.shown.isEmpty && !b.loading {
                    Text("No cards").font(Fonts.small).foregroundStyle(P.ink3)
                        .listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(P.bg)
        .overlay {
            if let b, b.loading, b.shown.isEmpty { ProgressView() }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if let b {
                Picker("Status", selection: Binding(get: { b.status }, set: { model.boardStatus($0) })) {
                    ForEach(boardColumns, id: \.self) { s in
                        Text(statusName(s)).tag(s)
                    }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .padding(.vertical, Tokens.Space.s)
                .background(P.bg)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(P.bg, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                if let b { picker(b) }
            }
            if let b, let following = model.following(board: b.board) {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        session.send(model.toggleFollow(board: b.board))
                    } label: {
                        Image(systemName: following ? "bell.fill" : "bell")
                    }
                    .accessibilityLabel(following ? "Following" : "Follow")
                }
            }
        }
        .sheet(isPresented: Binding(get: { model.acta.sheet != nil }, set: { if !$0 { model.acta.sheet = nil } })) {
            CardSheetView()
        }
    }

    /// The board's tile, its name and a chevron, with the person's boards under it, the
    /// last used first.
    private func picker(_ b: BoardScreen) -> some View {
        let model = session.model
        return Menu {
            ForEach(model.boardChoices, id: \.board) { c in
                Button {
                    for f in model.openBoard(c.board) { session.send(f) }
                } label: {
                    if c.board == b.board {
                        Label(c.name, systemImage: "checkmark")
                    } else {
                        Text(c.name)
                    }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Pill.mark(16).accessibilityHidden(true)
                Text(model.boardName(b.board)).font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
                Image(systemName: "chevron.down").font(.system(size: 10, weight: .semibold)).foregroundStyle(P.ink3)
            }
        }
        .accessibilityLabel("Board: \(model.boardName(b.board))")
    }
}

/// A card's row in a board: its id in mono, its size and its assignee's initial on the
/// right, and its title under them.
private struct BoardRow: View {
    var task: BoardTask
    var open: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(task.id).font(Fonts.mono(Tokens.PhoneSize.caption, relativeTo: .caption)).foregroundStyle(P.ink3)
                    Spacer(minLength: 0)
                    if !task.size.isEmpty {
                        Text(task.size.uppercased()).font(Fonts.caption).foregroundStyle(P.ink3)
                    }
                    if !task.initial.isEmpty {
                        Text(task.initial)
                            .font(Fonts.ui(10.5, .medium, relativeTo: .caption2))
                            .foregroundStyle(P.ink2)
                            .frame(width: 20, height: 20)
                            .background(P.raised2, in: Circle())
                    }
                }
                Text(task.title)
                    .font(Fonts.small)
                    .foregroundStyle(task.status == "canceled" ? P.ink3 : P.ink)
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, Tokens.Space.s + 2)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(task.id) \(task.title)")
    }
}

/// The pills of the cards made from a message, under it on its side. A tap opens the
/// card's preview.
struct MadePills: View {
    @Environment(Session.self) private var session
    var message: Message

    var body: some View {
        let made = message.made.compactMap { message.cards[$0] }
        if !made.isEmpty {
            Flow(spacing: 6) {
                ForEach(made, id: \.id) { c in
                    CardPillView(card: c) {
                        session.send(session.model.openCard(board: c.board, id: c.id))
                    }
                }
            }
        }
    }
}
