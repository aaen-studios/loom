import SwiftUI

// Acta beside the chat, on the phone (PLAN.md, "Acta beside the chat"): a card's preview
// in a sheet, the form that makes a card from a message in a sheet, and a board pushed as
// a screen of its own, with Follow in the sheet's ⋯ and on the board. The model (Acta.swift)
// says what each shows and what the person's changes send.

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

// MARK: A card's preview

/// One of a card's properties as a chip under its title: its status, priority, assignee
/// or size.
private struct PropChip<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 5) { content }
            .font(Fonts.ui(Tokens.PhoneSize.caption, .medium, relativeTo: .caption))
            .foregroundStyle(P.ink2)
            .lineLimit(1)
            .padding(.horizontal, 8)
            .frame(minHeight: Tokens.Control.pill)
            .background(P.bg, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).strokeBorder(P.line, lineWidth: 1))
    }
}

/// A priority as three bars, rising, `filled` of them in ink.
private struct PriorityBars: View {
    var filled: Int

    var body: some View {
        HStack(alignment: .bottom, spacing: 1.5) {
            ForEach(0..<3, id: \.self) { k in
                RoundedRectangle(cornerRadius: 0.5)
                    .fill(k < filled ? P.ink2 : P.lineStrong)
                    .frame(width: 2.5, height: [4, 7, 10][k])
            }
        }
        .accessibilityHidden(true)
    }
}

/// A person as a round avatar with their initial.
private struct Avatar: View {
    var initial: String
    var side: CGFloat

    var body: some View {
        Text(initial)
            .font(.system(size: max(side * 0.55, 8), weight: .semibold))
            .foregroundStyle(P.ink)
            .frame(width: side, height: side)
            .background(P.active, in: Circle())
            .accessibilityHidden(true)
    }
}

/// A card's preview, in a sheet from its pill or a board's row: Acta's mark, its board
/// and id, Open in Acta, ⋯ and close; its title; its status, priority, assignee and size
/// as chips, the status's a menu that moves it; then Details, Comments and Activity.
struct CardSheetView: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var draft = ""

    var body: some View {
        let model = session.model
        let s = model.acta.sheet
        VStack(spacing: 0) {
            if let s { head(s) }
            ScrollView {
                if let s { page(s) }
            }
        }
        .background(P.bg)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let s, s.tab == .comments, s.detail?.canComment == true { commentBox(s) }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onChange(of: s?.commented ?? 0) { _, _ in draft = "" }
        .onChange(of: s?.id ?? "") { _, _ in draft = "" }
    }

    /// Acta's mark, the board and the id, which a tap copies, then Open in Acta, ⋯ and close.
    private func head(_ s: CardSheet) -> some View {
        HStack(spacing: Tokens.Space.s) {
            Pill.mark(15).accessibilityHidden(true)
            HStack(spacing: 4) {
                Text("\(boardLabel(s)) ·").font(Fonts.small).lineLimit(1)
                Button(s.id) { UIPasteboard.general.string = s.id }
                    .font(Fonts.mono(Tokens.PhoneSize.small - 1))
                    .buttonStyle(.plain)
                    .fixedSize()
                    .accessibilityLabel("Copy \(s.id)")
            }
            .foregroundStyle(P.ink2)
            Spacer(minLength: Tokens.Space.s)
            Button("Open in Acta") {
                if let url = URL(string: link(s)) { openURL(url) }
            }
            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
            .foregroundStyle(P.ink)
            .buttonStyle(.plain)
            .fixedSize()
            more(s).frame(width: 32, height: 32)
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark").font(.system(size: 13, weight: .semibold)).foregroundStyle(P.ink3).frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Close")
        }
        .padding(.leading, Tokens.PhoneSpace.gutter)
        .padding(.trailing, Tokens.PhoneSpace.gutter - 6)
        .padding(.top, Tokens.Space.l)
        .padding(.bottom, Tokens.Space.s)
    }

    private func boardLabel(_ s: CardSheet) -> String {
        if let n = s.detail?.card.boardName, !n.isEmpty { return n }
        return session.model.boardName(s.board)
    }

    private func link(_ s: CardSheet) -> String {
        s.detail?.card.link ?? "https://\(actaHost)/\(s.board)#\(s.id)"
    }

    /// ⋯: Follow, Copy link, and Copy branch where Acta gives one.
    private func more(_ s: CardSheet) -> some View {
        let model = session.model
        return Menu {
            if let following = model.following(board: s.board, card: s.id) {
                Button {
                    session.send(model.toggleFollow(board: s.board, card: s.id))
                } label: {
                    Label(following ? "Following" : "Follow", systemImage: following ? "checkmark" : "bell")
                }
            }
            Button {
                UIPasteboard.general.string = link(s)
            } label: {
                Label("Copy link", systemImage: "doc.on.doc")
            }
            if let branch = s.detail?.branch, !branch.isEmpty {
                Button {
                    UIPasteboard.general.string = branch
                } label: {
                    Label("Copy branch", systemImage: "arrow.triangle.branch")
                }
            }
        } label: {
            Image(systemName: "ellipsis").font(.system(size: 15, weight: .semibold)).foregroundStyle(P.ink3)
        }
        .accessibilityLabel("More")
    }

    @ViewBuilder
    private func page(_ s: CardSheet) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if let d = s.detail {
                VStack(alignment: .leading, spacing: Tokens.Space.s) {
                    Text(d.card.title)
                        .font(Fonts.ui(Tokens.Size.heading, .semibold, relativeTo: .title2))
                        .foregroundStyle(d.card.struck ? P.ink3 : P.ink)
                        .strikethrough(d.card.struck)
                        .fixedSize(horizontal: false, vertical: true)
                    chips(d)
                    if let e = s.error { Text(e).font(Fonts.small).foregroundStyle(P.bad) }
                }
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .padding(.bottom, Tokens.Space.m)
                tabs(s, d)
                Group {
                    switch s.tab {
                    case .details: details(d)
                    case .comments: comments(d)
                    case .activity: activity(d)
                    }
                }
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .padding(.top, Tokens.Space.m)
            } else if let e = s.error {
                Text(e).font(Fonts.small).foregroundStyle(P.bad).padding(.horizontal, Tokens.PhoneSpace.gutter)
            } else {
                ProgressView().frame(maxWidth: .infinity)
            }
        }
        .padding(.top, Tokens.Space.s)
        .padding(.bottom, Tokens.Space.xl)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func chips(_ d: CardDetail) -> some View {
        let model = session.model
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
                PropChip {
                    Circle().fill(d.card.ink.color).frame(width: 7, height: 7)
                    Text(d.card.statusWords)
                    Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold)).foregroundStyle(P.ink3)
                }
            }
            .accessibilityLabel("Status: \(d.card.statusWords)")
            let bars = priorityBars(d.priority)
            if bars > 0 {
                PropChip {
                    PriorityBars(filled: bars)
                    Text(priorityName(d.priority))
                }
            }
            if !d.card.assignee.isEmpty {
                PropChip {
                    Avatar(initial: initialOf(d.card.assignee), side: 14)
                    Text(d.card.assignee)
                }
            }
            if !d.card.size.isEmpty {
                PropChip { Text(d.card.size.uppercased()) }
            }
        }
    }

    /// Details, Comments · N and Activity, the one shown underlined.
    private func tabs(_ s: CardSheet, _ d: CardDetail) -> some View {
        HStack(spacing: Tokens.Space.l) {
            ForEach(CardTab.allCases, id: \.self) { tab in
                let on = s.tab == tab
                let label = tab == .details ? "Details" : tab == .comments ? d.commentsTab : "Activity"
                Button {
                    session.model.acta.sheet?.tab = tab
                } label: {
                    Text(label)
                        .font(Fonts.small)
                        .foregroundStyle(on ? P.ink : P.ink3)
                        .padding(.vertical, 8)
                        .overlay(alignment: .bottom) {
                            Rectangle().fill(on ? P.ink : Color.clear).frame(height: 2)
                        }
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Tokens.PhoneSpace.gutter)
        .overlay(alignment: .bottom) { Rectangle().fill(P.line).frame(height: 1) }
    }

    /// Its description, its labels, its subtasks and its pull request.
    private func details(_ d: CardDetail) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            let words = d.description.trimmingCharacters(in: .whitespacesAndNewlines)
            if !words.isEmpty {
                MarkdownView(words, font: Fonts.body).textSelection(.enabled)
            }
            let labels = d.card.labels.filter { !$0.isEmpty }
            if !labels.isEmpty {
                Flow(spacing: 6) {
                    ForEach(labels, id: \.self) { l in
                        Text(l)
                            .font(Fonts.caption)
                            .foregroundStyle(P.ink2)
                            .lineLimit(1)
                            .padding(.horizontal, 7)
                            .frame(minHeight: 20)
                            .overlay(RoundedRectangle(cornerRadius: 5, style: .continuous).strokeBorder(P.line, lineWidth: 1))
                    }
                }
            }
            if !d.checklist.isEmpty { subtasks(d) }
            if let pr = d.pullRequest { pullRequest(pr) }
        }
    }

    private func subtasks(_ d: CardDetail) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(d.subtasksLine)
                .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                .foregroundStyle(P.ink3)
                .padding(.bottom, Tokens.Space.xs)
            ForEach(d.checklist) { t in
                Button {
                    session.send(session.model.toggleSubtask(t.id))
                } label: {
                    HStack(alignment: .firstTextBaseline, spacing: Tokens.Space.s + 2) {
                        let box = RoundedRectangle(cornerRadius: 4, style: .continuous)
                        box.fill(t.done ? P.ink : Color.clear)
                            .overlay(box.strokeBorder(t.done ? P.ink : P.lineStrong, lineWidth: 1))
                            .overlay {
                                if t.done { Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)).foregroundStyle(P.bg) }
                            }
                            .frame(width: 18, height: 18)
                            .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 5 }
                        Text(t.title)
                            .font(Fonts.small)
                            .foregroundStyle(P.ink)
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

    private func pullRequest(_ pr: PullRequest) -> some View {
        let (words, tone) = pr.checksWords
        return Button {
            if let url = URL(string: pr.url), !pr.url.isEmpty { openURL(url) }
        } label: {
            HStack(spacing: Tokens.Space.s) {
                Text("#\(pr.number)").font(Fonts.mono(Tokens.PhoneSize.small - 1)).foregroundStyle(P.ink2)
                Text(pr.title).font(Fonts.small).foregroundStyle(P.ink).lineLimit(1)
                Spacer(minLength: Tokens.Space.s)
                Text(words)
                    .font(Fonts.caption)
                    .foregroundStyle(tone == .ok ? P.ok : tone == .bad ? P.bad : P.ink3)
            }
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Pull request #\(pr.number), \(pr.title)")
    }

    /// Its comments, oldest first: who's initial, who and the day, then what they said.
    private func comments(_ d: CardDetail) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            ForEach(Array(d.comments.enumerated()), id: \.offset) { _, c in
                let day = commentDay(c.at)
                HStack(alignment: .top, spacing: Tokens.Space.s) {
                    Avatar(initial: c.initial, side: 20)
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 4) {
                            Text(c.by).font(Fonts.ui(Tokens.PhoneSize.caption, .semibold)).foregroundStyle(P.ink)
                            if !day.isEmpty { Text("· \(day)").font(Fonts.caption).foregroundStyle(P.ink3) }
                        }
                        MarkdownView(c.body, font: Fonts.small).textSelection(.enabled)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    /// Its activity, newest first: who, what in Acta's words, and the day.
    private func activity(_ d: CardDetail) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(d.activity.reversed().enumerated()), id: \.offset) { _, a in
                let day = commentDay(a.at)
                (Text(a.by).foregroundStyle(P.ink) + Text(" \(a.text)").foregroundStyle(P.ink2) + Text(day.isEmpty ? "" : " · \(day)").foregroundStyle(P.ink3))
                    .font(Fonts.small)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    /// The box to comment in, under the comments, with Comment.
    private func commentBox(_ s: CardSheet) -> some View {
        HStack(alignment: .bottom, spacing: Tokens.Space.s) {
            TextField("Comment…", text: $draft, axis: .vertical)
                .font(Fonts.small)
                .lineLimit(1...6)
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(P.line, lineWidth: 1))
            Button(s.commenting ? "Commenting…" : "Comment") {
                session.send(session.model.commentCard(draft))
            }
            .buttonStyle(.loom)
            .disabled(s.commenting || draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
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
