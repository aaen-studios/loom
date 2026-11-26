import SwiftUI

// T3 Code's threads in the chat (PLAN.md, "T3 threads"), as desktop/src/t3_view.rs draws
// them: each thread a message names or started, as a card with its strand beside the
// title, and the sheet that starts one. The model (T3.swift) says what each shows and
// what to send.

extension Strand {
    /// How the strand moves: it waves while working, knots while it waits, lies still
    /// once done and sags when it failed.
    var mood: Mood {
        switch self {
        case .waves: .writing
        case .knots: .waiting
        case .straight: .idle
        case .sags: .failed
        }
    }

    /// Its colour, and its time's and edge's: the sky thread's, warn, ok or bad.
    var color: Color {
        switch self {
        case .waves: P.thread("sky")
        case .knots: P.warn
        case .straight: P.ok
        case .sags: P.bad
        }
    }
}

/// A message's T3 threads, one card under another.
struct ThreadCards: View {
    var message: Message

    var body: some View {
        if !message.threads.isEmpty {
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                ForEach(message.threads) { ThreadCardView(card: $0) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// One thread's card (design C): the strand beside its title in the colour of where it
/// is, the time or what it waits on, and `T3 · project · model · effort` under the title.
/// Then its step, its question, its pull requests or why it failed, and its buttons. A
/// tap opens the thread in T3.
struct ThreadCardView: View {
    @Environment(Session.self) private var session
    @Environment(\.openURL) private var openURL
    var card: ThreadCard

    var body: some View {
        let strand = card.strand
        let color = strand.color
        CardFrame(card.title, sub: card.sub, lead: AnyView(lead(strand)), side: AnyView(side(color)), edge: card.edge.map { color.opacity($0.opacity) }) {
            switch card.quiet {
            case .step(let step):
                quietBox {
                    Text("›_").font(Fonts.mono(Tokens.PhoneSize.small - 1)).foregroundStyle(P.ink3)
                    Text(step).font(Fonts.small).foregroundStyle(P.ink2).lineLimit(1).truncationMode(.tail)
                }
            case .reason(let why):
                quietBox {
                    Text(why).font(Fonts.small).foregroundStyle(P.bad).fixedSize(horizontal: false, vertical: true)
                }
            case nil:
                EmptyView()
            }
            if let q = card.asks {
                Text(q)
                    .font(Fonts.body)
                    .foregroundStyle(P.ink)
                    .lineSpacing(3)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.leading, Tokens.Space.m)
                    .overlay(alignment: .leading) { Rectangle().fill(P.warn).frame(width: 2) }
            }
            let chips = card.chips
            if !chips.isEmpty {
                Flow {
                    ForEach(Array(chips.enumerated()), id: \.offset) { _, chip in
                        Button {
                            if let url = URL(string: chip.url) { openURL(url) }
                        } label: {
                            Text(chip.label)
                                .font(Fonts.mono(Tokens.PhoneSize.caption, relativeTo: .caption))
                                .foregroundStyle(P.ink2)
                                .padding(.horizontal, Tokens.Space.s)
                                .padding(.vertical, 3)
                                .background(P.cardInset, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
                                .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous).strokeBorder(P.line, lineWidth: 1))
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            let buttons = card.buttons
            if !buttons.isEmpty {
                let busy = session.model.t3.busy[card.thread]
                Flow {
                    if card.inT3 {
                        Text("Approve in T3")
                            .font(Fonts.small)
                            .foregroundStyle(P.ink2)
                            .frame(minHeight: Tokens.Control.button)
                    }
                    ForEach(Array(buttons.enumerated()), id: \.offset) { _, b in
                        Button {
                            press(b)
                        } label: {
                            if busy == b.label {
                                ProgressView().controlSize(.small)
                            } else {
                                Text(b.label)
                            }
                        }
                        .buttonStyle(CardButtonStyle(primary: b.main))
                        .disabled(busy != nil)
                    }
                }
            }
        }
        .contentShape(RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
        .onTapGesture {
            if let url = URL(string: card.url), !card.url.isEmpty { openURL(url) }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(card.title)
    }

    /// The strand, level with the title's first line.
    private func lead(_ strand: Strand) -> some View {
        ThreadView(mood: strand.mood, color: strand.color)
            .frame(width: 26, height: 14)
            .frame(height: Tokens.PhoneSize.body * 1.35)
    }

    /// The time it has run or ran, counting up while it moves, or what it waits on.
    private func side(_ color: Color) -> some View {
        TimelineView(.periodic(from: .now, by: card.moving ? 1 : 3600)) { _ in
            let words = card.side(nowMs())
            if !words.isEmpty {
                Text(words)
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .semibold, relativeTo: .caption))
                    .foregroundStyle(color)
                    .monospacedDigit()
                    .lineLimit(1)
            }
        }
    }

    private func quietBox<C: View>(@ViewBuilder _ content: () -> C) -> some View {
        HStack(spacing: Tokens.Space.s) { content() }
            .padding(.horizontal, Tokens.Space.m)
            .padding(.vertical, Tokens.Space.s)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(P.cardInset, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous).strokeBorder(P.line, lineWidth: 1))
    }

    /// Open opens the thread in T3; the rest answer it or ask it to try again.
    private func press(_ b: ThreadButton) {
        if b.act == .open {
            if let url = URL(string: card.url), !card.url.isEmpty { openURL(url) }
            return
        }
        session.send(session.model.t3Act(card, b.act, label: b.label))
    }
}

/// The sheet that starts a T3 thread, from the + sheet: Server when the person has
/// several, Project, Model, Effort and Task, and Start.
struct T3StartSheet: View {
    @Environment(Session.self) private var session
    @State private var task = ""
    @FocusState private var typing: Bool

    var body: some View {
        let model = session.model
        let f = model.t3.form ?? StartForm()
        NavigationStack {
            Form {
                Section {
                    if f.showsServer {
                        Picker("Server", selection: Binding(get: { f.server }, set: { pickServer($0) })) {
                            ForEach(f.servers, id: \.self) { Text($0).tag($0) }
                        }
                    }
                    Picker("Project", selection: Binding(get: { f.project }, set: { model.t3.form?.project = $0 })) {
                        if f.projects.isEmpty { Text("None").tag("") }
                        ForEach(f.projects, id: \.id) { p in
                            Text(p.title).tag(p.id)
                        }
                    }
                    Picker("Model", selection: Binding(get: { "\(f.provider)\n\(f.model)" }, set: { pickModel($0) })) {
                        if f.modelList.isEmpty { Text("None").tag("\n") }
                        ForEach(f.modelList, id: \.label) { m in
                            Text(m.label).tag("\(m.provider)\n\(m.model)")
                        }
                    }
                }
                if !f.efforts.isEmpty {
                    Section("Effort") {
                        Picker("Effort", selection: Binding(get: { f.effort }, set: { model.t3.form?.effort = $0 })) {
                            ForEach(f.efforts, id: \.id) { e in
                                Text(e.label).tag(e.id)
                            }
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                    }
                }
                Section("Task") {
                    TextField("What the thread is to do", text: $task, axis: .vertical)
                        .lineLimit(4...14)
                        .focused($typing)
                }
                if !f.error.isEmpty {
                    Section {
                        Text(f.error).font(Fonts.small).foregroundStyle(P.bad)
                    }
                }
            }
            .font(Fonts.body)
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .disabled(f.loading)
            .overlay {
                if f.loading && f.projects.isEmpty { ProgressView() }
            }
            .navigationTitle("Start a T3 thread")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { model.t3.form = nil }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if f.starting {
                        ProgressView()
                    } else {
                        Button("Start") { session.send(model.t3Start(task)) }
                            .disabled(!f.canStart(task))
                    }
                }
            }
            .onAppear { typing = true }
        }
    }

    private func pickServer(_ server: String) {
        let model = session.model
        guard model.t3.form?.server != server else { return }
        model.t3.form?.server = server
        session.send(model.openT3Form(server: server))
    }

    private func pickModel(_ tag: String) {
        let parts = tag.components(separatedBy: "\n")
        guard parts.count == 2 else { return }
        session.model.t3.form?.pick(provider: parts[0], model: parts[1])
    }
}
