import SwiftUI

// T3 Code's threads in the chat (PLAN.md, "T3 threads"), as desktop/src/t3_view.rs draws
// them: each thread a message names or started, as a card (LOO-435, option C) with a band
// across its top saying where it is, and the sheet that starts one. The model (T3.swift)
// says what each shows and what to send; this draws it, in black and white as Spools is.

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

extension ThreadState {
    /// Its icon in the band, so where it is never rests on colour alone.
    var symbol: String {
        switch self {
        case .working: "rays"
        case .waiting: "hand.raised.fill"
        case .done: "checkmark"
        case .failed: "xmark"
        }
    }
}

/// One thread's card: its band; T3's mark and the title; where it runs, its project,
/// branch, model and effort; what it's doing in plain words with the step as it came,
/// what it asks, its summary or why it failed; its last steps while the band's count is
/// pressed; its pull requests with how their checks stand; the buttons that answer it;
/// and Open in T3, Send, Stop and ⋯ (Copy link). A tap opens the thread in T3.
struct ThreadCardView: View {
    @Environment(Session.self) private var session
    @Environment(\.openURL) private var openURL
    var card: ThreadCard
    @State private var showSteps = false
    @State private var sending = false
    @State private var text = ""
    @FocusState private var typing: Bool

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
        let waits = card.state == .waiting
        VStack(alignment: .leading, spacing: 0) {
            TimelineView(.periodic(from: .now, by: card.moving ? 1 : 60)) { _ in
                band(card.band(nowMs()))
            }
            VStack(alignment: .leading, spacing: Tokens.Space.m) {
                head
                let meta = card.meta
                if !meta.isEmpty {
                    Text(meta.joined(separator: "  ·  "))
                        .font(Fonts.caption)
                        .foregroundStyle(P.ink3)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                if let body = card.body { bodyView(body) }
                if showSteps && !card.steps.isEmpty { stepList }
                ForEach(Array(card.prs.enumerated()), id: \.offset) { _, pr in prRow(pr) }
                answers
                foot
                if sending { sendBox }
            }
            .padding(Tokens.Space.l)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(P.cardFill, in: shape)
        .clipShape(shape)
        .overlay(shape.strokeBorder(waits ? P.ink : P.cardEdge, lineWidth: 1))
        .shadow(color: .black.opacity(0.07), radius: 14, y: 8)
        .contentShape(shape)
        .onTapGesture { open() }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(card.title), \(card.band(nowMs()).state)")
    }

    /// The band across its top: where it is in words beside an icon of it, filled with ink
    /// while it waits on the person; its run and steps, which show its last steps when
    /// tapped; and its time.
    private func band(_ b: ThreadBand) -> some View {
        let ink = b.ink ? P.bg : P.ink
        let quiet = b.ink ? P.bg.opacity(0.75) : P.ink2
        return HStack(spacing: Tokens.Space.s + 2) {
            HStack(spacing: Tokens.Space.xs + 2) {
                Image(systemName: card.state.symbol).font(.system(size: 10, weight: .semibold))
                Text(b.state).font(Fonts.ui(Tokens.PhoneSize.small, .semibold))
            }
            .foregroundStyle(ink)
            if !b.run.isEmpty {
                Text(b.run)
                    .font(Fonts.small)
                    .foregroundStyle(quiet)
                    .lineLimit(1)
                    .onTapGesture { if !card.steps.isEmpty { showSteps.toggle() } }
                    .accessibilityAddTraits(card.steps.isEmpty ? [] : .isButton)
                    .accessibilityHint(card.steps.isEmpty ? "" : (showSteps ? "Hide steps" : "Show steps"))
            }
            Spacer(minLength: 0)
            Text(b.time).font(Fonts.small).foregroundStyle(quiet).monospacedDigit().lineLimit(1)
        }
        .padding(.horizontal, Tokens.Space.l)
        .frame(height: Tokens.Control.buttonSmall)
        // A flat-bottomed strip whose top follows the card's corners exactly: a bare
        // `.background(colour)` here came out rounded at its bottom corners too.
        .background {
            UnevenRoundedRectangle(topLeadingRadius: Tokens.Radius.bubble, bottomLeadingRadius: 0, bottomTrailingRadius: 0,
                                   topTrailingRadius: Tokens.Radius.bubble, style: .continuous)
                .fill(b.ink ? P.ink : Color.clear)
        }
        .overlay(alignment: .bottom) { Rectangle().fill(b.ink ? P.ink : P.line).frame(height: 1) }
    }

    /// T3's mark on an ink tile, and its title.
    private var head: some View {
        HStack(spacing: Tokens.Space.s + 2) {
            RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous)
                .fill(P.button)
                .frame(width: 22, height: 22)
                .overlay {
                    if let mark = UIImage(named: "mark-t3") {
                        Image(uiImage: mark).renderingMode(.template).resizable().scaledToFit()
                            .frame(width: 13, height: 13).foregroundStyle(P.buttonInk)
                    } else {
                        Text("T3").font(Fonts.ui(9, .bold)).foregroundStyle(P.buttonInk)
                    }
                }
                .accessibilityHidden(true)
            Text(card.title)
                .font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                .foregroundStyle(P.ink)
                .lineLimit(2)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    @ViewBuilder private func bodyView(_ body: ThreadBody) -> some View {
        switch body {
        case .doing(let words, let raw):
            VStack(alignment: .leading, spacing: 2) {
                Text(words).font(Fonts.body).foregroundStyle(P.ink).fixedSize(horizontal: false, vertical: true)
                if !raw.isEmpty {
                    Text(raw).font(Fonts.mono(Tokens.PhoneSize.caption, relativeTo: .caption)).foregroundStyle(P.ink3).lineLimit(1).truncationMode(.tail)
                }
            }
        case .asks(let q):
            Text(q)
                .font(Fonts.body)
                .foregroundStyle(P.ink)
                .lineSpacing(3)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, Tokens.Space.m)
                .overlay(alignment: .leading) { Rectangle().fill(P.ink).frame(width: 2) }
        case .summary(let words):
            Text(words).font(Fonts.body).foregroundStyle(P.ink2).lineSpacing(3).lineLimit(3)
        case .failed(let why):
            Text(why).font(Fonts.body).foregroundStyle(P.ink).fixedSize(horizontal: false, vertical: true)
        }
    }

    /// Its last few steps, the newest last.
    private var stepList: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(card.steps.enumerated()), id: \.offset) { _, s in
                HStack(spacing: Tokens.Space.s) {
                    Text("·").foregroundStyle(P.ink3)
                    Text(s).foregroundStyle(P.ink2).lineLimit(1)
                }
                .font(Fonts.small)
            }
        }
    }

    /// A pull request: its number, its title, and how its checks stand.
    private func prRow(_ pr: ThreadPR) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous)
        return Button {
            if let url = URL(string: pr.url) { openURL(url) }
        } label: {
            HStack(spacing: Tokens.Space.s) {
                Text("#\(pr.number)").foregroundStyle(P.ink3)
                Text(pr.title.isEmpty ? "Pull request #\(pr.number)" : pr.title).foregroundStyle(P.ink).lineLimit(1)
                Spacer(minLength: 0)
                Text(pr.standing).font(Fonts.caption).foregroundStyle(P.ink2).lineLimit(1)
            }
            .font(Fonts.small)
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: Tokens.Control.buttonSmall + 4)
            .background(P.cardInset, in: shape)
            .overlay(shape.strokeBorder(P.line, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("#\(pr.number) \(pr.title)")
    }

    /// The buttons that answer it, and "Approve in T3" beside Open in T3.
    @ViewBuilder private var answers: some View {
        let buttons = card.buttons
        if !buttons.isEmpty || card.inT3 {
            let busy = session.model.t3.busy[card.thread]
            Flow {
                ForEach(Array(buttons.enumerated()), id: \.offset) { _, b in
                    Button {
                        press(b.act, label: b.label)
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
                if card.inT3 {
                    Text("Approve in T3")
                        .font(Fonts.small)
                        .foregroundStyle(P.ink2)
                        .frame(minHeight: Tokens.Control.button)
                }
            }
        }
    }

    /// Its foot: Open in T3, Send, Stop, and ⋯ at its end.
    private var foot: some View {
        HStack(spacing: Tokens.Space.s) {
            ForEach(card.tools, id: \.self) { t in
                switch t {
                case .open:
                    tool("arrow.up.right.square", "Open in T3", on: false) { open() }
                case .send:
                    tool("pencil", "Send a message", on: sending) {
                        sending.toggle()
                        typing = sending
                    }
                case .stop:
                    tool("stop", "Stop", on: false) { press(.stop, label: "Stop") }
                }
            }
            Spacer(minLength: 0)
            Menu {
                Button {
                    UIPasteboard.general.string = card.url
                } label: {
                    Label("Copy link", systemImage: "doc.on.doc")
                }
                .disabled(card.url.isEmpty)
            } label: {
                toolFace("ellipsis", on: false)
            }
            .accessibilityLabel("More")
        }
    }

    /// The Send box under its foot while it's open: what to send the thread, and Send.
    private var sendBox: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous)
        return HStack(spacing: Tokens.Space.s) {
            TextField("Message the thread", text: $text)
                .font(Fonts.body)
                .focused($typing)
                .submitLabel(.send)
                .onSubmit(sendNow)
                .padding(.horizontal, Tokens.Space.m)
                .frame(minHeight: Tokens.Control.button)
                .background(P.cardInset, in: shape)
                .overlay(shape.strokeBorder(P.line, lineWidth: 1))
            Button("Send", action: sendNow)
                .buttonStyle(CardButtonStyle(primary: true))
                .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
    }

    private func tool(_ symbol: String, _ name: String, on: Bool, _ action: @escaping () -> Void) -> some View {
        Button(action: action) { toolFace(symbol, on: on) }
            .buttonStyle(.plain)
            .accessibilityLabel(name)
    }

    private func toolFace(_ symbol: String, on: Bool) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
        return Image(systemName: symbol)
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(P.ink)
            .frame(width: Tokens.Control.buttonSmall + 4, height: Tokens.Control.buttonSmall + 4)
            .overlay(shape.strokeBorder(on ? P.ink : P.lineStrong, lineWidth: 1))
            .contentShape(shape)
    }

    private func sendNow() {
        guard let sent = session.model.t3Act(card, .send(text), label: "Send") else { return }
        session.send(sent)
        text = ""
        sending = false
    }

    private func open() {
        if let url = URL(string: card.url), !card.url.isEmpty { openURL(url) }
    }

    /// Open opens the thread in T3; the rest answer it, ask it to try again, or stop it.
    private func press(_ act: ThreadAct, label: String) {
        if act == .open {
            open()
            return
        }
        session.send(session.model.t3Act(card, act, label: label))
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
