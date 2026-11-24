import PhotosUI
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

/// An agent's chat: its messages, the reply being written, the questions it's waiting
/// on, and the composer.
struct ChatView: View {
    @Environment(Session.self) private var session
    let agentId: String
    /// Drawn once, hidden, before any chat is opened, so the first one opened doesn't
    /// pay for building the screen the first time (AgentsView's `Warmup`). It selects
    /// nothing and fetches nothing.
    var warming = false
    @State private var atBottom = true
    @State private var openThoughts: Set<String> = []
    /// A message's text open to select from.
    @State private var reading: Reading?

    var body: some View {
        let model = session.model
        let agent = model.agent(agentId)
        let messages = model.selected == agentId ? model.shown : []
        let approvals = model.selected == agentId ? model.shownApprovals : []
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Tokens.Space.l + 2) {
                    if model.moreMessages {
                        Text("Earlier messages are on your computer.")
                            .font(Fonts.caption).foregroundStyle(P.ink3)
                            .frame(maxWidth: .infinity)
                    }
                    ForEach(messages) { m in
                        Group {
                            if m.isPerson {
                                PersonMessage(message: m, agent: agent)
                            } else {
                                Reply(message: m, agent: agent, openThoughts: $openThoughts)
                            }
                        }
                        .contextMenu {
                            // One menu a message rather than selectable text in each
                            // paragraph, which made each row many times dearer to build.
                            let text = m.text
                            if !m.live && !text.isEmpty {
                                Button {
                                    UIPasteboard.general.string = text
                                } label: {
                                    Label("Copy", systemImage: "doc.on.doc")
                                }
                                Button {
                                    reading = Reading(text: text)
                                } label: {
                                    Label("Select text", systemImage: "selection.pin.in.out")
                                }
                            }
                        }
                    }
                    ForEach(approvals) { a in
                        ApprovalCard(approval: a, tint: P.thread(agent?.color ?? ""))
                    }
                    Color.clear.frame(height: 1).id("end")
                }
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .padding(.top, Tokens.Space.l)
                .padding(.bottom, Tokens.Space.s)
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            .onScrollGeometryChange(for: Bool.self) { g in
                g.contentOffset.y + g.containerSize.height >= g.contentSize.height - 80
            } action: { _, near in
                atBottom = near
            }
            .onChange(of: Follow(messages: messages, approvals: approvals.count)) { before, _ in
                // Follow a reply as it's written, unless the person scrolled up to read.
                // A chat's first page opens at its end by itself (defaultScrollAnchor), so
                // it isn't scrolled to as well: that laid out its rows a second time while
                // the chat was sliding in.
                if atBottom && !before.messages.isEmpty {
                    withAnimation(.easeOut(duration: 0.15)) { proxy.scrollTo("end", anchor: .bottom) }
                }
            }
            .overlay {
                // Only once its messages have come, not while it opens.
                if messages.isEmpty && approvals.isEmpty && model.selected == agentId && model.messagesCame {
                    Text("Nothing here yet. Say hello.")
                        .font(Fonts.body).foregroundStyle(P.ink3)
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                Composer(agentName: agent?.name ?? "the agent") {
                    withAnimation { proxy.scrollTo("end", anchor: .bottom) }
                }
            }
        }
        .background(P.bg)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 1) {
                    HStack(spacing: 6) {
                        if let agent {
                            ThreadView(mood: model.mood(agent.id), color: P.thread(agent.color), snapped: model.finished[agent.id])
                                .frame(width: 18, height: 10)
                        }
                        Text(agent?.name ?? "").font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
                    }
                    ConnLine(conn: model.conn)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button {
                        session.send(model.startFresh())
                    } label: {
                        Label("Start a new chat", systemImage: "square.and.pencil")
                    }
                } label: {
                    Image(systemName: "ellipsis")
                }
                .accessibilityLabel("More")
            }
        }
        .toolbarBackground(P.bg, for: .navigationBar)
        .quickLookPreview(warming ? .constant(nil) : Bindable(model).toOpen)
        .fullScreenCover(item: warming ? .constant(nil) : Bindable(model).viewing) { v in
            PictureViewer(stack: v.stack)
        }
        .sheet(item: $reading) { r in
            NavigationStack {
                SelectableText(text: r.text)
                    .navigationTitle("Select text")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { reading = nil }
                        }
                    }
            }
            .presentationDetents([.medium, .large])
        }
        .onAppear {
            guard !warming else { return }
            model.select(agentId)
            session.flush()
        }
    }

    /// What, when it changes, scrolls the chat along.
    private struct Follow: Equatable {
        var messages: [Message]
        var approvals: Int
    }

    private struct Reading: Identifiable {
        let id = UUID()
        var text: String
    }
}

/// A message's text, to select any part of it.
private struct SelectableText: UIViewRepresentable {
    var text: String

    func makeUIView(context: Context) -> UITextView {
        let v = UITextView()
        v.isEditable = false
        v.isSelectable = true
        v.backgroundColor = .clear
        v.font = UIFont.preferredFont(forTextStyle: .body)
        v.adjustsFontForContentSizeCategory = true
        v.textColor = UIColor(P.ink)
        v.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 16, right: 12)
        return v
    }

    func updateUIView(_ v: UITextView, context: Context) {
        if v.text != text { v.text = text }
    }
}

// MARK: - Messages

/// What the person wrote: a bubble on the right in the agent's colour, a breath of it or
/// more with Vibrant bubbles, with its files above it. What another agent sent is plain
/// grey.
struct PersonMessage: View {
    @Environment(Session.self) private var session
    @AppStorage(Prefs.vibrant) private var vibrant = true
    var message: Message
    var agent: Agent?

    var body: some View {
        let files = fileParts(message)
        // A stack's pictures only listed by name are left out of the words: the stack
        // shows them, and the viewer their names.
        let stacked = files.flatMap { part -> [FileRef] in
            guard case .pictures(let run) = part else { return [] }
            return run.compactMap { message.blocks[$0].file }
        }
        let text = unmarkStacked(message.blocks.filter { $0.kind == "text" }.map(\.text).joined(separator: "\n"), stacked)
        VStack(alignment: .trailing, spacing: Tokens.Space.xs + 2) {
            if let from = message.from {
                Text("From \(from)").font(Fonts.caption).foregroundStyle(P.ink3)
            }
            ForEach(Array(files.enumerated()), id: \.offset) { _, part in
                switch part {
                case .pictures(let run):
                    PictureStack(files: run.compactMap { message.blocks[$0].file })
                case .block(let i):
                    if let f = message.blocks[i].file { FileView(file: f) }
                default:
                    EmptyView()
                }
            }
            if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                MarkdownView(text, keepBreaks: true, font: Fonts.body)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, Tokens.Space.l)
                    .padding(.vertical, Tokens.Space.s + 3)
                    .background(message.from == nil ? P.personBubble(agent?.color ?? "", vibrant: vibrant) : P.bubble, in: RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
                    .opacity(message.queued ? 0.6 : 1)
            }
            if message.queued {
                Text("Waiting to send").font(Fonts.caption).foregroundStyle(P.ink3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .padding(.leading, 48)
    }
}

/// An agent's reply: its name and thread, then its blocks, with thoughts folded and tool
/// calls quiet, and its answer in a grey bubble, or plain if the person picked that.
struct Reply: View {
    @Environment(Session.self) private var session
    @AppStorage(Prefs.replies) private var replies = Prefs.bubbles
    var message: Message
    var agent: Agent?
    @Binding var openThoughts: Set<String>

    var body: some View {
        let model = session.model
        VStack(alignment: .leading, spacing: Tokens.Space.s + 2) {
            HStack(spacing: Tokens.Space.s) {
                Group {
                    if message.live {
                        ThreadView(mood: model.mood(agent?.id ?? ""), color: P.thread(agent?.color ?? ""))
                    } else {
                        // A finished reply's thread is still: a line, with no timeline or
                        // canvas of its own.
                        StillThread().stroke(P.thread(agent?.color ?? ""), style: StrokeStyle(lineWidth: Tokens.ThreadStroke.small, lineCap: .round, lineJoin: .round))
                    }
                }
                .frame(width: 16, height: 10)
                Text(agent?.name ?? "Agent").font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink)
                Text(message.live ? "now" : When.time(message.at)).font(Fonts.caption).foregroundStyle(P.ink3)
            }
            let laid = parts(message, tools: model.tools)
            ForEach(Array(laid.enumerated()), id: \.offset) { _, part in
                self.part(part, onTheWay: false)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func part(_ part: Part, onTheWay: Bool) -> some View {
        switch part {
        case .words(let run):
            ForEach(run, id: \.self) { i in
                let b = message.blocks[i]
                // Only the last block grows while a reply is written.
                let live = message.live && i == message.blocks.count - 1
                if onTheWay {
                    // What it said on its way to the answer: smaller and quieter, never a
                    // bubble, so it doesn't read as a message.
                    MarkdownView(b.text, font: Fonts.small, ink: P.ink2, live: live)
                } else if replies == Prefs.bubbles {
                    // Grey, never the agent's colour, which is the person's: its corner by
                    // the thread small, as a speech bubble's tail.
                    MarkdownView(b.text, live: live)
                        .padding(.horizontal, Tokens.Space.l)
                        .padding(.vertical, Tokens.Space.s + 3)
                        .background(P.bubble, in: UnevenRoundedRectangle(topLeadingRadius: Tokens.Radius.bubbleJoined, bottomLeadingRadius: Tokens.Radius.bubble, bottomTrailingRadius: Tokens.Radius.bubble, topTrailingRadius: Tokens.Radius.bubble, style: .continuous))
                } else {
                    MarkdownView(b.text, live: live)
                }
            }
        case .tools(let run):
            VStack(alignment: .leading, spacing: Tokens.Space.xs + 2) {
                ForEach(run, id: \.self) { i in
                    ToolLine(tool: session.model.tool(message.blocks[i].call), live: message.live)
                }
            }
        case .block(let i):
            block(message.blocks[i], key: "\(message.id)/\(i)")
        case .pictures(let run):
            PictureStack(files: run.compactMap { message.blocks[$0].file })
        case .fold(let words, let working, let steps):
            Steps(words: words, working: working, open: openThoughts.contains(foldKey), canOpen: !steps.isEmpty) {
                if openThoughts.contains(foldKey) { openThoughts.remove(foldKey) } else { openThoughts.insert(foldKey) }
            } steps: {
                ForEach(Array(steps.enumerated()), id: \.offset) { _, step in
                    AnyView(self.part(step, onTheWay: true))
                }
            }
        }
    }

    private var foldKey: String { "steps/\(message.id)" }

    @ViewBuilder
    private func block(_ b: Block, key: String) -> some View {
        switch b.kind {
        case "thought" where !b.text.isEmpty:
            Thought(text: b.text, open: openThoughts.contains(key)) {
                if openThoughts.contains(key) { openThoughts.remove(key) } else { openThoughts.insert(key) }
            }
        case "tool":
            ToolLine(tool: session.model.tool(b.call), live: message.live)
        case "plan":
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(b.plan.enumerated()), id: \.offset) { _, e in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Image(systemName: e.status == "completed" ? "checkmark.circle" : e.status == "in_progress" ? "circle.dotted" : "circle")
                            .font(.system(size: 11))
                            .foregroundStyle(P.ink3)
                        Text(e.content).font(Fonts.small).foregroundStyle(e.status == "completed" ? P.ink3 : P.ink2)
                    }
                }
            }
        case "file":
            if let f = b.file { FileView(file: f) }
        case "card":
            if let c = b.card { CardView(card: c, tint: P.thread(agent?.color ?? "")) }
        case "notice" where !b.text.isEmpty:
            // One line from loom, not the agent's own words: small and quiet.
            Text(b.text).font(Fonts.caption).foregroundStyle(P.ink3)
        default:
            EmptyView()
        }
    }
}

/// A turn's steps under one quiet line: the step it's on while it works, how many once
/// it's done. Tapped, it opens to show them in the order they came.
struct Steps<Content: View>: View {
    var words: String
    var working: Bool
    var open: Bool
    var canOpen: Bool
    var toggle: () -> Void
    @ViewBuilder var steps: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
            Button(action: toggle) {
                HStack(spacing: 4) {
                    if canOpen {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 9, weight: .semibold))
                            .rotationEffect(.degrees(open && canOpen ? 90 : 0))
                    }
                    Text(working && !words.hasSuffix("…") ? words + "…" : words)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                .font(Fonts.small)
                .foregroundStyle(P.ink3)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!canOpen)
            if open && canOpen {
                VStack(alignment: .leading, spacing: Tokens.Space.s) {
                    steps
                }
                .padding(.leading, Tokens.Space.m)
                .overlay(alignment: .leading) { Rectangle().fill(P.lineStrong).frame(width: 1) }
            }
        }
        .animation(.easeOut(duration: 0.15), value: open)
    }
}

/// The agent's reasoning, folded until it's tapped.
struct Thought: View {
    var text: String
    var open: Bool
    var toggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xs + 2) {
            Button(action: toggle) {
                HStack(spacing: 4) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .semibold))
                        .rotationEffect(.degrees(open ? 90 : 0))
                    Text("Thought")
                }
                .font(Fonts.caption)
                .foregroundStyle(P.ink3)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open {
                Text(text)
                    .font(Fonts.small)
                    .foregroundStyle(P.ink3)
                    .lineSpacing(3)
                    .padding(.leading, Tokens.Space.m)
                    .overlay(alignment: .leading) { Rectangle().fill(P.lineStrong).frame(width: 1) }
            }
        }
        .animation(.easeOut(duration: 0.15), value: open)
    }
}

/// A tool call, one quiet line: what it is, and how it went.
struct ToolLine: View {
    var tool: Tool?
    /// A reply still being written, where a call nothing finished yet is running. In a
    /// kept reply its turn is over, so it stopped.
    var live = false

    var body: some View {
        HStack(spacing: Tokens.Space.s) {
            Text("›_").font(Fonts.code).foregroundStyle(P.ink3)
            Text(tool?.title.isEmpty == false ? tool!.title : "Used a tool")
                .font(Fonts.code)
                .foregroundStyle(P.ink2)
                .lineLimit(1)
                .truncationMode(.middle)
            status
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var status: some View {
        switch tool?.status {
        case "done": Image(systemName: "checkmark").font(.system(size: 10, weight: .semibold)).foregroundStyle(P.ok)
        case "failed": Image(systemName: "xmark").font(.system(size: 10, weight: .semibold)).foregroundStyle(P.bad)
        case "denied": Text("denied").font(Fonts.caption).foregroundStyle(P.bad)
        case "cancelled": Text("stopped").font(Fonts.caption).foregroundStyle(P.ink3)
        case nil where tool != nil && live: ProgressView().controlSize(.mini)
        case nil where tool != nil: Text("stopped").font(Fonts.caption).foregroundStyle(P.ink3)
        default: EmptyView()
        }
    }
}

/// A file in a message: a picture for an image, its name and size otherwise. Tapping it
/// fetches it and shows it.
struct FileView: View {
    @Environment(Session.self) private var session
    var file: FileRef
    /// Its thumbnail, read and decoded off the main thread.
    @State private var thumb: UIImage?

    /// The most room a picture takes, and the place it keeps while its size isn't known.
    static let most = CGSize(width: 360, height: 400)
    static let unknown = CGSize(width: 240, height: 160)

    var body: some View {
        let path = file.isImage ? session.model.files["\(file.file).thumb"] : nil
        Button {
            session.send(session.model.open(file))
        } label: {
            if file.isImage {
                let image = path.flatMap { thumb ?? Pictures.kept($0) }
                // Its own place in the message, sized from its file's header before it's
                // decoded: scaled down to fit the column and 360 × 400, its shape kept,
                // and never up, as on the desktop. Until its size is known, 240 × 160.
                let pixels = path.flatMap { Pictures.size($0) } ?? image.map { CGSize(width: $0.size.width * $0.scale, height: $0.size.height * $0.scale) }
                let size = pixels.map { Pictures.fit($0, in: FileView.most) } ?? FileView.unknown
                Group {
                    if let image {
                        Image(uiImage: image)
                            .resizable()
                    } else {
                        P.bubble.overlay { Image(systemName: "photo").foregroundStyle(P.ink3) }
                    }
                }
                .aspectRatio(size.width / size.height, contentMode: .fit)
                .frame(maxWidth: size.width)
                .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
            } else {
                HStack(spacing: Tokens.Space.m) {
                    Image(systemName: "doc.text").foregroundStyle(P.ink3)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(file.name).font(Fonts.small).foregroundStyle(P.ink).lineLimit(1)
                        Text(When.size(file.size)).font(Fonts.caption).foregroundStyle(P.ink3)
                    }
                }
                .padding(.horizontal, Tokens.Space.m)
                .padding(.vertical, Tokens.Space.s + 2)
                .frame(maxWidth: 260, alignment: .leading)
                .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous).strokeBorder(P.line, lineWidth: 1))
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(file.isImage ? "Picture, \(file.name)" : "\(file.name), \(When.size(file.size))")
        .task(id: path) {
            guard let path, Pictures.kept(path) == nil else { return }
            thumb = await Pictures.load(path)
        }
    }
}

/// Two or more pictures in a message as one stack of a fixed size, however many there
/// are, as the desktop draws it: the first on top, up to two more peeking out behind it,
/// and how many there are in its corner. Tapping it shows them big, to page through.
struct PictureStack: View {
    @Environment(Session.self) private var session
    var files: [FileRef]

    static let size = CGSize(width: 240, height: 180)
    /// How far each picture behind peeks out.
    static let step: CGFloat = 6

    var body: some View {
        let behind = min(max(files.count - 1, 0), 2)
        let room = Self.step * CGFloat(behind)
        Button {
            session.model.view(files)
        } label: {
            ZStack(alignment: .bottomLeading) {
                ForEach((0...behind).reversed(), id: \.self) { layer in
                    StackCard(file: files[layer], behind: layer > 0, count: layer == 0 ? files.count : nil)
                        .offset(x: Self.step * CGFloat(layer), y: -Self.step * CGFloat(layer))
                }
            }
            .frame(width: Self.size.width + room, height: Self.size.height + room, alignment: .bottomLeading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(files.count) pictures")
    }
}

/// One picture of a stack, filling its place: the ones behind a step back, toward the
/// page, and the top one with the count.
private struct StackCard: View {
    @Environment(Session.self) private var session
    var file: FileRef
    var behind: Bool
    var count: Int?
    @State private var thumb: UIImage?

    var body: some View {
        let path = session.model.files["\(file.file).thumb"]
        let image = path.flatMap { thumb ?? Pictures.kept($0) }
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
        Group {
            if let image {
                Image(uiImage: image).resizable().scaledToFill()
            } else {
                P.bubble
            }
        }
        .frame(width: PictureStack.size.width, height: PictureStack.size.height)
        .overlay { if behind { P.bg.opacity(0.35) } }
        .clipShape(shape)
        .overlay(shape.strokeBorder(P.line, lineWidth: 1))
        .overlay(alignment: .bottomTrailing) {
            if let count {
                Text("\(count)")
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                    .monospacedDigit()
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .frame(minWidth: 20, minHeight: 20)
                    .background(.black.opacity(0.6), in: Capsule())
                    .padding(Tokens.Space.s)
            }
        }
        .task(id: path) {
            guard let path, Pictures.kept(path) == nil else { return }
            thumb = await Pictures.load(path)
        }
    }
}

/// A stack's pictures big, over everything, a page each: whole once fetched, the
/// thumbnail until then, with the picture's name and where it is among them.
struct PictureViewer: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var stack: [FileRef]
    @State private var page = 0

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            TabView(selection: $page) {
                ForEach(Array(stack.enumerated()), id: \.offset) { i, f in
                    WholePicture(file: f).tag(i)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .ignoresSafeArea()
        }
        .overlay(alignment: .topTrailing) {
            Button {
                dismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 36)
                    .background(.white.opacity(0.12), in: Circle())
            }
            .accessibilityLabel("Close")
            .padding(.horizontal, Tokens.PhoneSpace.gutter)
        }
        .overlay(alignment: .bottom) {
            HStack(spacing: Tokens.Space.m) {
                Text(stack[min(page, stack.count - 1)].name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .foregroundStyle(.white.opacity(0.75))
                if stack.count > 1 {
                    Text("\(page + 1) / \(stack.count)")
                        .monospacedDigit()
                        .foregroundStyle(.white.opacity(0.5))
                }
            }
            .font(Fonts.small)
            .padding(.horizontal, Tokens.PhoneSpace.gutter)
            .padding(.bottom, Tokens.Space.m)
        }
        .task(id: page) {
            // This picture whole, and the ones either side.
            for i in [page, page + 1, page - 1] where stack.indices.contains(i) {
                session.send(session.model.whole(stack[i]))
            }
        }
    }
}

/// One page of the viewer: the picture fitted to the screen.
private struct WholePicture: View {
    @Environment(Session.self) private var session
    var file: FileRef
    @State private var image: UIImage?

    /// Points across it's decoded for, at the screen's scale.
    static let side: CGFloat = 600

    var body: some View {
        let path = session.model.files[file.file] ?? session.model.files["\(file.file).thumb"]
        Group {
            if let shown = image ?? path.flatMap({ Pictures.kept($0, side: Self.side) }) {
                Image(uiImage: shown).resizable().scaledToFit()
            } else {
                ProgressView().tint(.white)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.vertical, 56)
        .accessibilityLabel("Picture, \(file.name)")
        .task(id: path) {
            guard let path else { return }
            image = await Pictures.load(path, side: Self.side)
        }
    }
}

// MARK: - Previews

#Preview("Chat") {
    let session = Session(sample: true)
    return NavigationStack { ChatView(agentId: "a_1") }.environment(session)
}

#Preview("Chat, dark") {
    let session = Session(sample: true)
    return NavigationStack { ChatView(agentId: "a_2") }.environment(session).preferredColorScheme(.dark)
}
