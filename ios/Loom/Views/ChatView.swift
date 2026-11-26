import PhotosUI
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

/// An agent's chat: its messages, the reply being written, the questions it's waiting
/// on, and the composer. Or another agent's thread with it, to read.
struct ChatView: View {
    @Environment(Session.self) private var session
    let agentId: String
    /// Another agent's thread with it, shown in place of its chat, with no composer.
    var thread: Ask? = nil
    /// A group chat, shown in place of an agent's chat: each reply by the agent that gave it.
    var roomId: String? = nil
    /// In a thread: back to the agent's own chat, ready to write.
    var continueChat: (() -> Void)? = nil
    /// Drawn once, hidden, before any chat is opened, so the first one opened doesn't
    /// pay for building the screen the first time (AgentsView's `Warmup`). It selects
    /// nothing and fetches nothing.
    var warming = false
    @State private var atBottom = true
    @State private var openThoughts: Set<String> = []
    /// A message's text open to select from.
    @State private var reading: Reading?
    /// Another agent's thread with it, opened from the menu.
    @State private var threadOpen: Ask?
    /// The message a quote went to, lit for a moment.
    @State private var lit: String?
    /// The chat's pins, open in a sheet from the header's pin.
    @State private var pinsOpen = false
    /// The context's card, open in a sheet from the header's ring.
    @State private var contextOpen = false
    @AppStorage(Prefs.showContext) private var showContext = false
    /// The group chat's settings, open in a sheet.
    @State private var roomSettings = false
    /// What the agent follows, open in a sheet from the menu.
    @State private var followingOpen = false

    var body: some View {
        let model = session.model
        let room = roomId.flatMap { model.roomNamed($0) }
        // In a group chat, the member the router would fall back to stands for the chat.
        let agent = room == nil ? model.agent(agentId) : model.selected.flatMap { model.agent($0) }
        let here = roomId != nil ? model.room == roomId : model.selected == agentId && model.thread == thread?.id && model.room == nil
        let messages = here ? model.shown : []
        let approvals = here ? model.shownApprovals : []
        let stacked = stackedLines(messages)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Tokens.Space.l + 2) {
                    if model.moreMessages {
                        Text("Earlier messages are on your computer.")
                            .font(Fonts.caption).foregroundStyle(P.ink3)
                            .frame(maxWidth: .infinity)
                    }
                    ForEach(messages) { m in
                        VStack(alignment: m.isPerson ? .trailing : .leading, spacing: Tokens.Space.s) {
                            if let line = m.watchLine {
                                WatchLineRow(line: line)
                                    .padding(.top, stacked.contains(m.id) ? 6 - (Tokens.Space.l + 2) : 0)
                            } else if m.deleted {
                                DeletedMark(message: m, agent: m.routed.flatMap { model.agent($0.agent) } ?? agent)
                            } else if isStartLine(m) {
                                // The line a thread the person started lands in: its card.
                                ThreadCards(message: m)
                            } else if let card = m.routineOff {
                                // A routine that turned off on its own: its card, to turn it on.
                                RoutineOffRow(card: card)
                            } else if m.isPerson {
                                PersonMessage(message: m, agent: agent, tint: room.map { $0.color.isEmpty ? "grey" : $0.color })
                            } else {
                                Reply(message: m, agent: m.routed.flatMap { model.agent($0.agent) } ?? agent, openThoughts: $openThoughts)
                            }
                            // The cards made from it, under it on its side.
                            if !m.deleted && !m.made.isEmpty {
                                MadePills(message: m)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: m.isPerson ? .trailing : .leading)
                        // Lit for a moment, once a quote went to it.
                        .background(lit == m.id ? P.thread(agent?.color ?? "").opacity(0.12) : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
                        .modifier(SwipeToReply(enabled: thread == nil && !m.live && !m.queued && !m.deleted && m.watchLine == nil) { model.reply(to: m.id) })
                        .contextMenu {
                            // One menu a message rather than selectable text in each
                            // paragraph, which made each row many times dearer to build.
                            let text = m.text
                            if thread == nil && !m.live && !m.queued && !m.deleted && m.watchLine == nil {
                                Button {
                                    model.reply(to: m.id)
                                } label: {
                                    Label("Reply", systemImage: "arrowshape.turn.up.left")
                                }
                            }
                            if thread == nil && !m.live && !m.queued && !m.deleted && m.watchLine == nil {
                                Button {
                                    session.send(model.pin(m.id, !m.pinned))
                                } label: {
                                    Label(m.pinned ? "Unpin" : "Pin", systemImage: m.pinned ? "pin.slash" : "pin")
                                }
                            }
                            if !m.live && !text.isEmpty {
                                Button {
                                    UIPasteboard.general.string = Emoji.asNames(text)
                                } label: {
                                    Label("Copy", systemImage: "doc.on.doc")
                                }
                                Button {
                                    reading = Reading(text: text, message: thread == nil && !m.queued ? m.id : nil, agent: !m.isPerson)
                                } label: {
                                    Label("Select text", systemImage: "selection.pin.in.out")
                                }
                            }
                            // A card on one of the person's Acta boards, made from its words
                            // (PLAN.md, "Acta beside the chat").
                            if model.hasActa && thread == nil && !m.live && !m.queued && !m.deleted && !isStartLine(m) && m.routineOff == nil && m.watchLine == nil {
                                Button {
                                    session.send(model.startCard(message: m.id, text: text))
                                } label: {
                                    Label("Make a card…", systemImage: "square.3.layers.3d")
                                }
                            }
                            // At once, leaving a mark where it was (PLAN.md, "Deleting a message").
                            if thread == nil && !m.live && !m.queued && !m.deleted && m.watchLine == nil {
                                Divider()
                                Button(role: .destructive) {
                                    session.send(model.deleteMessage(m.id))
                                } label: {
                                    Label("Delete", systemImage: "trash")
                                }
                            }
                        }
                    }
                    ForEach(approvals) { a in
                        ApprovalCard(approval: a, tint: P.thread(agent?.color ?? ""))
                    }
                    Color.clear.frame(height: 1).id("end")
                }
                .environment(\.openURL, OpenURLAction { url in MainActor.assumeIsolated { follow(url) } })
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .padding(.top, Tokens.Space.l)
                .padding(.bottom, Tokens.Space.s)
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            // A tap in the chat puts the emoji picker away, as it goes on to what it hit.
            .simultaneousGesture(
                TapGesture().onEnded {
                    withAnimation(EmojiPicker.swap) { model.emoji.shutPicker() }
                },
                including: model.emoji.up ? .all : .subviews
            )
            .onScrollGeometryChange(for: Bool.self) { g in
                g.contentOffset.y + g.containerSize.height >= g.contentSize.height - 80
            } action: { _, near in
                atBottom = near
            }
            // A quote tapped: to its original, once the chat has it, reaching back for it.
            .onChange(of: model.jumpTo) { _, to in
                if let to { jump(to, proxy) }
            }
            .onChange(of: messages.count) { _, _ in
                if let to = model.jumpTo, !model.reaching { jump(to, proxy) }
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
                if messages.isEmpty && approvals.isEmpty && here && model.messagesCame {
                    Text("Nothing here yet")
                        .font(Fonts.body).foregroundStyle(P.ink3)
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                // The composer writes to the agent's chat, or a thread an agent made, so
                // another agent's thread is only read here, with the way back to that chat
                // in the composer's place.
                if thread == nil || thread?.made == true {
                    Composer(agentName: room?.name ?? agent?.name ?? "the agent") {
                        withAnimation { proxy.scrollTo("end", anchor: .bottom) }
                    }
                } else {
                    ContinueButton { continueChat?() }
                }
            }
        }
        .background(P.bg)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 1) {
                    HStack(spacing: 6) {
                        // A group chat by its name and its members' woven threads.
                        if let room {
                            RoomMark(colors: model.roomColors(room)).frame(width: 22, height: 10)
                            Text(room.name).font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
                        } else if let thread {
                            if let by = thread.by {
                                ThreadView(mood: .idle, color: P.thread(by.color))
                                    .frame(width: 18, height: 10)
                            }
                            Text(thread.label).font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
                        } else {
                            if let agent {
                                ThreadView(mood: model.mood(agent.id), color: P.thread(agent.color), snapped: model.finished[agent.id])
                                    .frame(width: 18, height: 10)
                            }
                            Text(agent?.name ?? "").font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
                        }
                    }
                    ConnLine(conn: model.conn)
                }
            }
            // How full the agent's session context is, in a group chat the member's that
            // answered last, with its card in a sheet.
            if thread == nil, here, showContext, let fill = model.context {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        contextOpen = true
                    } label: {
                        ContextRing(fill: fill)
                    }
                    .accessibilityLabel(fill.tip)
                }
            }
            if thread == nil {
                ToolbarItem(placement: .topBarTrailing) {
                    // The chat's pins, with how many there are.
                    let pinned = max(model.pinsCount, model.pins.count)
                    Button {
                        pinsOpen = true
                    } label: {
                        HStack(spacing: 3) {
                            Image(systemName: "pin")
                            if pinned > 0 {
                                Text("\(pinned)").font(Fonts.ui(Tokens.PhoneSize.small, .medium)).monospacedDigit()
                            }
                        }
                    }
                    .accessibilityLabel(pinned > 0 ? "Pinned, \(pinned)" : "Pinned")
                }
            }
            if roomId != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        roomSettings = true
                    } label: {
                        Image(systemName: "person.2")
                    }
                    .accessibilityLabel("Group chat settings")
                }
            } else if thread == nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button {
                            session.send(model.startFresh())
                        } label: {
                            Label("Start a new chat", systemImage: "square.and.pencil")
                        }
                        // What the agent follows, when it follows something.
                        if !model.following(.agent(agentId)).isEmpty {
                            Button {
                                followingOpen = true
                            } label: {
                                Label("Following", systemImage: "bell")
                            }
                        }
                        // Other agents' threads with it, and the ones agents made beside its chat,
                        // each on a screen of its own.
                        let asks = model.asks(agentId)
                        if !asks.isEmpty {
                            Section {
                                ForEach(asks) { k in
                                    Button {
                                        threadOpen = k
                                    } label: {
                                        Label(k.label, systemImage: "bubble.left.and.bubble.right")
                                    }
                                }
                            }
                        }
                    } label: {
                        Image(systemName: "ellipsis")
                    }
                    .accessibilityLabel("More")
                }
            }
        }
        .navigationDestination(item: $threadOpen) { k in
            ChatView(agentId: agentId, thread: k) {
                session.model.writeNow = true
                threadOpen = nil
            }
        }
        // Back from a thread, by Continue in your own chat or the back button, the chat is
        // the agent's own again. Its onAppear doesn't come again when Continue pops the
        // thread (LOO-341).
        .onChange(of: threadOpen?.id) { _, now in
            guard now == nil, !warming, roomId == nil, thread == nil else { return }
            model.open(thread: nil)
            session.flush()
        }
        .sheet(isPresented: $roomSettings) {
            if let roomId { RoomSettingsSheet(roomId: roomId) }
        }
        .sheet(isPresented: warming ? .constant(false) : $followingOpen) {
            FollowingSheet(agentId: agentId)
        }
        // Acta beside the chat and T3's start sheet, from the chat shown: a card's preview
        // here while no board is pushed over it, which shows its own.
        .sheet(isPresented: shown(here, model.acta.sheet != nil && model.acta.screen == nil) { model.acta.sheet = nil }) {
            CardSheetView()
        }
        .sheet(isPresented: shown(here, model.acta.form != nil) { model.acta.form = nil }) {
            MakeCardSheet()
                .presentationDetents([.large])
        }
        .sheet(isPresented: shown(here, model.t3.form != nil) { model.t3.form = nil }) {
            T3StartSheet()
                .presentationDetents([.large])
        }
        .navigationDestination(isPresented: shown(here && thread == nil, model.acta.screen != nil) {
            model.acta.screen = nil
            model.acta.sheet = nil
        }) {
            BoardScreenView()
        }
        .toolbarBackground(P.bg, for: .navigationBar)
        .quickLookPreview(warming ? .constant(nil) : Bindable(model).toOpen)
        .fullScreenCover(item: warming ? .constant(nil) : Bindable(model).viewing) { v in
            PictureViewer(stack: v.stack)
        }
        .sheet(isPresented: warming ? .constant(false) : $pinsOpen) {
            PinsSheet(agent: agent)
        }
        .sheet(isPresented: warming || !showContext ? .constant(false) : $contextOpen) {
            ContextSheet()
        }
        .sheet(item: $reading) { r in
            NavigationStack {
                SelectableText(text: r.text, message: r.message, agent: r.agent, paths: session.model.emoji.paths, selectAll: r.selectAll) { quote, ask in
                    reading = nil
                    if let id = r.message { session.model.reply(to: id, quote: quote, ask: ask) }
                }
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
            if let roomId {
                model.open(room: roomId)
                session.flush()
                openFromLaunch()
                return
            }
            model.select(agentId)
            // Back from a thread, the chat is the agent's own again.
            model.open(thread: thread?.id)
            session.flush()
            openFromLaunch()
        }
    }

    /// `-thread <conversation>` at launch, with `-open`, opens that agent's thread with
    /// another, for screenshots of it (ios/README.md).
    private func openFromLaunch() {
        let args = ProcessInfo.processInfo.arguments
        // `-continue` in that thread does what its Continue in your own chat does.
        if thread != nil, args.contains("-continue") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { continueChat?() }
        }
        // `-reply <message>` starts a reply to that message, once the chat's messages have
        // come, for screenshots of the composer's bar.
        if thread == nil, let i = args.firstIndex(of: "-reply"), i + 1 < args.count {
            let id = args[i + 1]
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { session.model.reply(to: id) }
        }
        // `-pins` opens the chat's pins, for screenshots of the sheet.
        if thread == nil, args.contains("-pins") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { pinsOpen = true }
        }
        // `-context` opens the context's card, for screenshots of the sheet.
        if thread == nil, args.contains("-context") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { contextOpen = true }
        }
        // `-select <message>` opens Select text on that message with all its words
        // selected, for screenshots of the selection over its emoji.
        if thread == nil, let i = args.firstIndex(of: "-select"), i + 1 < args.count {
            let id = args[i + 1]
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) {
                guard let m = session.model.messages.first(where: { $0.id == id }) else { return }
                reading = Reading(text: m.text, message: m.id, agent: !m.isPerson, selectAll: true)
            }
        }
        // `-card <id>` opens that card's preview on the loom board, `-make <message>` Make a
        // card on that message, `-board` the board last used, `-t3start` Start a T3
        // thread, and `-jump <message>` goes to that message, once the chat's messages and
        // the person's boards have come, for screenshots of each.
        func value(_ flag: String) -> String? {
            guard let i = args.firstIndex(of: flag), i + 1 < args.count else { return nil }
            return args[i + 1]
        }
        if thread == nil {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
                let model = session.model
                if let id = value("-jump") { model.jumpTo = id }
                if let id = value("-card") { session.send(model.openCard(board: "aaen-studios/loom", id: id)) }
                if let id = value("-make"), let m = model.messages.first(where: { $0.id == id }) { session.send(model.startCard(message: m.id, text: m.text)) }
                if args.contains("-board") { for f in model.openBoard() { session.send(f) } }
                if args.contains("-t3start") { session.send(model.openT3Form()) }
                // `-following` opens what the chat follows: the agent's, or the group
                // chat's settings.
                if args.contains("-following") {
                    if roomId != nil { roomSettings = true } else { followingOpen = true }
                }
            }
        }
        guard thread == nil, let i = args.firstIndex(of: "-thread"), i + 1 < args.count else { return }
        threadOpen = session.model.asks(agentId).first { $0.id == args[i + 1] }
    }

    /// Whether what the model has open shows from this chat: only from the one shown, and
    /// never from the hidden one drawn before any opens. Shut by the person, `shut` puts it
    /// away in the model.
    private func shown(_ here: Bool, _ open: Bool, shut: @escaping () -> Void) -> Binding<Bool> {
        Binding(get: { !warming && here && open }, set: { if !$0 { shut() } })
    }

    /// A link tapped in a message: a card's pill opens its preview, a T3 thread opens in T3,
    /// a file it mentions or one sent with it opens here, and a page in the browser.
    private func follow(_ url: URL) -> OpenURLAction.Result {
        let model = session.model
        let link = url.absoluteString
        if link.hasPrefix(cardLink) {
            if let c = model.pillCard(link), !c.board.isEmpty {
                session.send(model.openCard(board: c.board, id: c.id))
            }
            return .handled
        }
        if let thread = linkThread(link) {
            let page = model.messages.lazy.flatMap(\.threads).first { $0.thread == thread }?.url
            return page.flatMap(URL.init(string:)).map { .systemAction($0) } ?? .handled
        }
        switch LinkTo(link) {
        case .file:
            session.send(model.openFileMention(url.absoluteString))
            return .handled
        case .attached(let sha):
            session.send(model.openAttached(sha))
            return .handled
        case .agent(let id):
            // An agent it mentions opens its chat (LOO-301).
            if model.agent(id) != nil { model.openAgent = id }
            return .handled
        case .skill:
            return .handled
        case .page, .other:
            return .systemAction
        }
    }

    /// What, when it changes, scrolls the chat along.
    private struct Follow: Equatable {
        var messages: [Message]
        var approvals: Int
    }

    /// Goes to the message `to` a quote names and lights it for a moment, or reaches back
    /// for older messages while the chat doesn't have it.
    private func jump(_ to: String, _ proxy: ScrollViewProxy) {
        let model = session.model
        if model.messages.contains(where: { $0.id == to }) {
            model.jumpTo = nil
            withAnimation(.easeOut(duration: 0.25)) { proxy.scrollTo(to, anchor: .center) }
            lit = to
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) {
                if lit == to { withAnimation { lit = nil } }
            }
        } else if !model.loadFurtherBack() {
            model.jumpTo = nil
        } else {
            session.flush()
        }
    }

    private struct Reading: Identifiable {
        let id = UUID()
        var text: String
        /// The message it's from, to reply to a part of; nil for one that can't be.
        var message: String? = nil
        var agent = false
        /// Whether it opens with all its words selected (`-select`).
        var selectAll = false
    }
}

/// A message's text, to select any part of it, and with a part selected, to quote it or,
/// in an agent's reply, ask about it (LOO-200). Its emoji are drawn as the chat draws
/// them, each one character to the selection, which highlights it as the words beside it
/// and copies and quotes it as the emoji itself or `:name:` (LOO-347).
private struct SelectableText: UIViewRepresentable {
    var text: String
    var message: String?
    var agent: Bool
    /// The person's own emoji's pictures, by id.
    var paths: [String: String]
    /// Whether it opens with all its words selected (`-select` at launch).
    var selectAll = false
    var quote: (String, Bool) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: SelectableText
        /// What it shows, piece by piece, to copy and quote from.
        var shown: [Emoji.Piece] = []
        /// What it was drawn from, so it's drawn again only when that changes: the words,
        /// the pictures of the person's own, and the text size.
        var drawn: (text: String, paths: [String: String], size: DynamicTypeSize)?
        init(_ parent: SelectableText) { self.parent = parent }

        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            guard parent.message != nil, range.length > 0 else { return nil }
            let words = Emoji.copied(shown, range)
            var mine = [UIAction(title: "Quote", image: UIImage(systemName: "text.quote")) { [parent] _ in parent.quote(words, false) }]
            if parent.agent {
                mine.append(UIAction(title: "Ask about this", image: UIImage(systemName: "questionmark.bubble")) { [parent] _ in parent.quote(words, true) })
            }
            return UIMenu(children: mine + suggestedActions)
        }
    }

    /// The text view, which copies what's selected as `copied` gives it.
    final class Words: UITextView {
        var copied: ((NSRange) -> String)?

        override func copy(_ sender: Any?) {
            guard let copied, selectedRange.length > 0 else { return super.copy(sender) }
            UIPasteboard.general.string = copied(selectedRange)
        }
    }

    func makeUIView(context: Context) -> Words {
        let v = Words()
        v.delegate = context.coordinator
        v.isEditable = false
        v.isSelectable = true
        v.backgroundColor = .clear
        v.adjustsFontForContentSizeCategory = true
        v.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 16, right: 12)
        let coordinator = context.coordinator
        v.copied = { [weak coordinator] range in coordinator.map { Emoji.copied($0.shown, range) } ?? "" }
        return v
    }

    func updateUIView(_ v: Words, context: Context) {
        let c = context.coordinator
        c.parent = self
        let size = context.environment.dynamicTypeSize
        guard c.drawn?.text != text || c.drawn?.paths != paths || c.drawn?.size != size else { return }
        let first = c.drawn == nil
        c.drawn = (text, paths, size)
        let font = UIFont.preferredFont(forTextStyle: .body)
        let words: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: UIColor(P.ink)]
        let side = (font.pointSize * Emoji.inText).rounded()
        let made = NSMutableAttributedString()
        c.shown = []
        for p in Emoji.pieces(text, paths: paths) {
            var image: UIImage?
            if case .emoji(_, let picture, let own) = p {
                image = own ? paths[picture].flatMap { Emoji.picture(path: $0, side: side) } : Emoji.picture(picture, side: side)
            }
            switch (p, image) {
            case (.emoji, .some(let image)):
                let a = NSTextAttachment(image: image)
                // On the line as the chat's emoji sit (`Emoji.inline`).
                a.bounds = CGRect(x: 0, y: -side * 0.2, width: side, height: side)
                let one = NSMutableAttributedString(attachment: a)
                one.addAttributes(words, range: NSRange(location: 0, length: one.length))
                made.append(one)
                c.shown.append(p)
            case (.emoji(let copies, _, _), .none):
                // A picture it doesn't have is the emoji as text, as it copies.
                made.append(NSAttributedString(string: copies, attributes: words))
                c.shown.append(.words(copies))
            case (.words(let w), _):
                made.append(NSAttributedString(string: w, attributes: words))
                c.shown.append(p)
            }
        }
        // Drawn again, as a picture came or the text size changed, it keeps what the person
        // selected while it's still the same length.
        let kept = v.selectedRange
        let same = v.textStorage.length == made.length
        v.attributedText = made
        if !first && same && kept.length > 0 { v.selectedRange = kept }
        if first && selectAll {
            DispatchQueue.main.async {
                v.becomeFirstResponder()
                v.selectedRange = NSRange(location: 0, length: v.textStorage.length)
            }
        }
    }
}

/// A swipe right on a message starts a reply to it, as in Messages: the message follows
/// the finger a little, with the reply arrow showing behind it, and goes back once let go.
private struct SwipeToReply: ViewModifier {
    var enabled: Bool
    var reply: () -> Void
    @State private var dx: CGFloat = 0
    private let enough: CGFloat = 56

    func body(content: Content) -> some View {
        content
            .offset(x: dx)
            .background(alignment: .leading) {
                if dx > 0 {
                    Image(systemName: "arrowshape.turn.up.left.fill")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(dx >= enough ? P.ink : P.ink3)
                        .opacity(Double(min(dx / enough, 1)))
                        .padding(.leading, 4)
                }
            }
            .simultaneousGesture(
                DragGesture(minimumDistance: 16)
                    .onChanged { g in
                        // Only a swipe across, to the right; a scroll stays a scroll.
                        guard enabled, g.translation.width > 0, abs(g.translation.width) > abs(g.translation.height) * 1.5 else { return }
                        let was = dx
                        dx = min(g.translation.width * 0.6, enough + 16)
                        if was < enough && dx >= enough { UIImpactFeedbackGenerator(style: .light).impactOccurred() }
                    }
                    .onEnded { _ in
                        if dx >= enough { reply() }
                        withAnimation(.spring(duration: 0.25)) { dx = 0 }
                    },
                including: enabled ? .all : .subviews
            )
    }
}

/// Who said what a quote quotes, as the chat names them, and their colour: the agent,
/// another agent that sent it, or the person, "You", in this chat's agent's colour.
@MainActor
func quotedBy(_ r: ReplyTo, model: Model) -> (String, Color) {
    let id = r.role == "agent" ? r.agent : (r.from.hasPrefix("a_") ? r.from : "")
    if !id.isEmpty {
        guard let a = model.agent(id) else { return ("An agent", P.ink2) }
        return (a.name, P.thread(a.color))
    }
    return ("You", P.thread(model.selected.flatMap { model.agent($0) }?.color ?? ""))
}

/// The quote over a message that replies to another, on one line: the agent's thread, who
/// said it in their colour ("You" quiet), a picture's thumbnail or a file's icon, and what,
/// as plain words cut with "…" (picked words in curly quotes). A tap goes to the original
/// (PLAN.md, "Replying to a message").
struct QuoteView: View {
    @Environment(Session.self) private var session
    var reply: ReplyTo
    /// The picture's thumbnail, read and decoded off the main thread.
    @State private var thumb: UIImage?

    var body: some View {
        let model = session.model
        let (who, color) = quotedBy(reply, model: model)
        let id = reply.role == "agent" ? reply.agent : (reply.from.hasPrefix("a_") ? reply.from : "")
        let agent = id.isEmpty ? nil : model.agent(id)
        let first = reply.files.first
        let path = first.flatMap { $0.isImage ? model.files["\($0.file).thumb"] : nil }
        let shown = self.shown
        Button {
            model.jumpTo = reply.message
        } label: {
            HStack(spacing: 5) {
                if let agent {
                    Image(uiImage: MentionGlyph.thread(agent.outline))
                        .renderingMode(.template)
                        .foregroundStyle(color)
                }
                Text(who)
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .medium, relativeTo: .caption))
                    .foregroundStyle(id.isEmpty ? P.ink3 : color)
                    .fixedSize()
                if reply.deleted {
                    Image(systemName: "trash")
                        .font(.system(size: 11))
                        .foregroundStyle(P.ink3)
                } else if let first {
                    if let image = path.flatMap({ thumb ?? Pictures.kept($0) }) {
                        Image(uiImage: image)
                            .resizable()
                            .scaledToFill()
                            .frame(width: 16, height: 16)
                            .clipShape(RoundedRectangle(cornerRadius: 3, style: .continuous))
                    } else {
                        Image(systemName: first.isImage ? "photo" : "doc.text")
                            .font(.system(size: 11))
                            .foregroundStyle(P.ink3)
                    }
                }
                if !shown.isEmpty {
                    Text(shown).font(Fonts.caption).foregroundStyle(reply.deleted ? P.ink3 : P.ink2).lineLimit(1).truncationMode(.tail)
                }
            }
            .frame(height: 20)
            .padding(.horizontal, 6)
            .contentShape(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Replying to \(who): \(shown)")
        .task(id: path) {
            guard let path, Pictures.kept(path) == nil else { return }
            thumb = await Pictures.load(path)
        }
    }

    /// Its words, picked words in curly quotes; with none, "Picture" or its files' names.
    /// Once the original is deleted, that it was.
    private var shown: String {
        if reply.deleted { return "The original was deleted" }
        let words = reply.oneLine
        if !words.isEmpty { return reply.part ? "“\(words)”" : words }
        let pictures = reply.files.filter(\.isImage).count
        if pictures > 0 && pictures == reply.files.count {
            return pictures == 1 ? "Picture" : "\(pictures) pictures"
        }
        return reply.files.map(\.name).joined(separator: ", ")
    }
}

/// The spine from a quote over the person's message down onto its bubble: a 2-point line
/// in line-strong from the line's middle, turning down with a 7-point corner, 14 wide and
/// 13 tall, its foot 20 points in from the bubble's edge.
struct QuoteSpine: Shape {
    func path(in r: CGRect) -> Path {
        var p = Path()
        let x = r.maxX - 1
        let top = r.minY + 10
        p.move(to: CGPoint(x: r.minX, y: top))
        p.addArc(tangent1End: CGPoint(x: x, y: top), tangent2End: CGPoint(x: x, y: top + 13), radius: 7)
        p.addLine(to: CGPoint(x: x, y: top + 13))
        return p
    }
}

// MARK: - Messages

/// What the person wrote: a bubble on the right in the agent's colour, a breath of it or
/// more with Vibrant bubbles, with its files above it. What another agent sent sits on
/// the same side, its bubble a breath of the sender's colour, with the sender's thread
/// and name in its colour over it.
/// What's left of a deleted message (PLAN.md, "Deleting a message"): a quiet line where it
/// was, so replies to it still make sense, on the side of who said it.
struct DeletedMark: View {
    var message: Message
    var agent: Agent?

    var body: some View {
        let said = message.isPerson
            ? message.from.map { "\($0)'s message was deleted" } ?? "You deleted a message"
            : "\(agent?.name ?? "The agent")'s message was deleted"
        HStack(spacing: 4) {
            Image(systemName: "trash").font(.system(size: 11))
            Text(said).font(Fonts.caption)
        }
        .foregroundStyle(P.ink3)
        .frame(maxWidth: .infinity, alignment: message.isPerson ? .trailing : .leading)
        .accessibilityElement(children: .combine)
    }
}

extension View {
    /// A message's words in the chat, cut after about ten lines while `folds` says so
    /// (PLAN.md, "Long messages and tables"); in a pin, the pin's own cut.
    @ViewBuilder
    func foldLong(_ came: Came, key: String, pin: PinCut?) -> some View {
        if pin != nil {
            pinCut(pin, key: key)
        } else {
            modifier(ChatFold(came: came, key: key))
        }
    }
}

/// Long words in the chat, cut at about ten lines as the desktop cuts them, with a short
/// fade into what's behind them and a quiet Show more under them, then Show less. A tap
/// on the cut words opens them too. What the person flips holds while the chat is open
/// (`Model.flipFold`).
private struct ChatFold: ViewModifier {
    @Environment(Session.self) private var session
    var came: Came
    var key: String
    /// A line of a message's words.
    @ScaledMetric(relativeTo: .body) private var line: CGFloat = Tokens.PhoneSize.body * Tokens.Line.body
    @State private var tall: CGFloat = 0

    func body(content: Content) -> some View {
        let model = session.model
        let most = line * 10
        // Cut only what's more than a line and a half past it: nothing opens to one line.
        let long = tall > most + line * 1.5
        let cut = long && folds(came, flipped: model.foldFlips.contains(key))
        // The fade, over its last two lines.
        let fade = cut ? 1 - 2 * line / most : 1
        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
            content
                .fixedSize(horizontal: false, vertical: true)
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { tall = $0 }
                .frame(maxHeight: cut ? most : nil, alignment: .top)
                .clipped()
                .mask {
                    LinearGradient(stops: [.init(color: .black, location: 0), .init(color: .black, location: fade), .init(color: .black.opacity(cut ? 0 : 1), location: 1)], startPoint: .top, endPoint: .bottom)
                }
                .contentShape(Rectangle())
                .simultaneousGesture(TapGesture().onEnded { model.flipFold(key) }, including: cut ? .all : .subviews)
            if long {
                Button(cut ? "Show more" : "Show less") { model.flipFold(key) }
                    .font(Fonts.caption)
                    .foregroundStyle(P.ink3)
                    .buttonStyle(.plain)
            }
        }
    }
}

struct PersonMessage: View {
    @Environment(Session.self) private var session
    @AppStorage(Prefs.vibrant) private var vibrant = false
    var message: Message
    var agent: Agent?
    /// Drawn as a pin in the Pinned sheet rather than in the chat.
    var pin: PinCut? = nil
    /// In a group chat, its colour, or grey with none: no one member's.
    var tint: String? = nil

    var body: some View {
        let model = session.model
        let files = fileParts(message)
        // A fan's pictures only listed by name are left out of the words: the fan
        // shows them, and the viewer their names.
        let stacked = files.flatMap { part -> [FileRef] in
            guard case .pictures(let run) = part else { return [] }
            return run.compactMap { message.blocks[$0].file }
        }
        let text = unmarkStacked(message.blocks.filter { $0.kind == "text" }.map(\.text).joined(separator: "\n"), stacked)
        // Each file reads as a link where it was attached, which opens it, its name cut
        // in the middle when it's long.
        let words = cutFileNames(linkFiles(text, message.blocks.compactMap(\.file)))
        let sender = message.fromAgent.flatMap { model.agent($0) }
        // In a pin it starts at the left, under who said it, which the pin says over it.
        VStack(alignment: pin == nil ? .trailing : .leading, spacing: Tokens.Space.xs + 2) {
            if let from = message.from, pin == nil {
                HStack(spacing: Tokens.Space.xs) {
                    if let sender {
                        Image(uiImage: MentionGlyph.thread(sender.outline))
                            .renderingMode(.template)
                            .accessibilityHidden(true)
                    }
                    Text(from)
                }
                .font(Fonts.ui(Tokens.PhoneSize.caption, .medium, relativeTo: .caption))
                .foregroundStyle(sender.map { P.mention($0.color) } ?? P.ink3)
            }
            if let reply = message.reply {
                HStack(spacing: 0) {
                    QuoteView(reply: reply)
                    QuoteSpine()
                        .stroke(P.lineStrong, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                        .frame(width: 14, height: 20)
                }
                .padding(.trailing, 20)
                // The spine meets what's under it.
                .padding(.bottom, -(Tokens.Space.xs + 1))
            }
            ForEach(Array(files.enumerated()), id: \.offset) { _, part in
                switch part {
                case .pictures(let run):
                    PictureFan(files: run.compactMap { message.blocks[$0].file })
                case .block(let i):
                    if let f = message.blocks[i].file { FileView(file: f) }
                default:
                    EmptyView()
                }
            }
            if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                MarkdownView(titledLinks(withCardPills(words, message.cards), message.links), keepBreaks: true, font: Fonts.body, links: model.inline(message))
                    .fixedSize(horizontal: false, vertical: true)
                    .foldLong(message.came, key: message.id, pin: pin)
                    .padding(.horizontal, Tokens.Space.l)
                    .padding(.vertical, Tokens.Space.s + 3)
                    .background(bubble(sender), in: RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
                    .opacity(message.queued ? 0.6 : 1)
            }
            if message.queued {
                Text("Waiting to send").font(Fonts.caption).foregroundStyle(P.ink3)
            }
            // The T3 threads it names, each a card.
            if pin == nil {
                ThreadCards(message: message)
            }
        }
        .frame(maxWidth: .infinity, alignment: pin == nil ? .trailing : .leading)
        .padding(.leading, pin == nil ? 48 : 0)
    }

    /// The person's own bubble in the agent's colour; another agent's, a breath of the
    /// sender's, or grey for one that's gone.
    private func bubble(_ sender: Agent?) -> Color {
        if message.from == nil { return P.personBubble(tint ?? agent?.color ?? "", vibrant: vibrant) }
        return sender.map { P.agentBubble($0.color) } ?? P.bubble
    }
}

/// In another agent's thread, where the composer would be: back to the person's own chat
/// with the agent, with the cursor in its composer.
private struct ContinueButton: View {
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text("Continue in your own chat")
                .font(Fonts.ui(Tokens.PhoneSize.body, .medium))
                .foregroundStyle(P.ink2)
                // As tall as the composer, which it stands in for.
                .frame(maxWidth: .infinity, minHeight: Tokens.Control.iconComposer + 2 * (Tokens.Space.xs + 2) + 2)
                .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.composer, style: .continuous).strokeBorder(P.line, lineWidth: 1))
                .contentShape(RoundedRectangle(cornerRadius: Tokens.Radius.composer, style: .continuous))
        }
        .buttonStyle(.plain)
        .padding(.horizontal, Tokens.Space.m)
        .padding(.top, Tokens.Space.xs)
        .padding(.bottom, Tokens.Space.s)
        .background(P.bg)
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
    /// Drawn as a pin in the Pinned sheet: what it said, with its pictures, files and
    /// cards, under the pin's own line of who and when, without its steps.
    var pin: PinCut? = nil

    var body: some View {
        let model = session.model
        VStack(alignment: .leading, spacing: Tokens.Space.s + 2) {
            if pin == nil {
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
                    // In a group chat, how it came to its agent, which a tap sends elsewhere.
                    if let routed = message.routed {
                        RouteLine(message: message, routed: routed, agent: agent)
                    } else {
                        Text(agent?.name ?? "Agent").font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink)
                    }
                    Text(message.live ? "now" : When.time(message.at)).font(Fonts.caption).foregroundStyle(P.ink3)
                }
            }
            let laid = parts(message, tools: model.tools)
            ForEach(Array(laid.enumerated()), id: \.offset) { _, part in
                self.part(part, onTheWay: false)
            }
            // The pictures it names, as the chat's pictures are, opening in the viewer
            // (PLAN.md, "Mentions").
            let pictured = message.named.filter { $0.thumb != nil }
            if pictured.count == 1, let f = pictured[0].picture {
                FileView(file: f, named: pictured)
            } else if pictured.count > 1 {
                PictureFan(files: pictured.compactMap(\.picture), named: pictured)
            }
            if pin == nil && !message.sources.isEmpty && sourcesAt(laid) == nil {
                SourcesTab(message: message)
            }
            // The T3 threads it started or names, each a card.
            if pin == nil {
                ThreadCards(message: message)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var links: Inline { session.model.inline(message) }

    /// The block whose bubble the reply's sources hang from: its last words.
    private var sourcesAt: Int? { message.sources.isEmpty || pin != nil ? nil : sourcesAt(parts(message, tools: session.model.tools)) }

    private func sourcesAt(_ laid: [Part]) -> Int? {
        for part in laid.reversed() {
            if case .words(let run) = part { return run.last }
        }
        return nil
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
                    MarkdownView(withCardPills(session.model.replyWords(message, b.text), message.cards), font: Fonts.small, ink: P.ink2, live: live, links: links)
                } else if replies == Prefs.bubbles {
                    // Grey, never the agent's colour, which is the person's: its corner by
                    // the thread small, as a speech bubble's tail.
                    let bubble = MarkdownView(titledLinks(withCardPills(session.model.replyWords(message, b.text), message.cards), message.links), live: live, links: links)
                        .foldLong(message.came, key: "\(message.id)/\(i)", pin: pin)
                        .padding(.horizontal, Tokens.Space.l)
                        .padding(.vertical, Tokens.Space.s + 3)
                        .background(P.bubble, in: UnevenRoundedRectangle(topLeadingRadius: Tokens.Radius.bubbleJoined, bottomLeadingRadius: Tokens.Radius.bubble, bottomTrailingRadius: Tokens.Radius.bubble, topTrailingRadius: Tokens.Radius.bubble, style: .continuous))
                    if i == sourcesAt {
                        // Its sources on a tab hanging from its bottom-left corner, tucked
                        // under it.
                        ZStack(alignment: .bottomLeading) {
                            SourcesTab(message: message).padding(.leading, 14)
                            bubble.padding(.bottom, SourcesTab.hang)
                        }
                    } else {
                        bubble
                    }
                } else {
                    MarkdownView(titledLinks(withCardPills(session.model.replyWords(message, b.text), message.cards), message.links), live: live, links: links)
                        .foldLong(message.came, key: "\(message.id)/\(i)", pin: pin)
                    if i == sourcesAt { SourcesTab(message: message) }
                }
            }
        case .tools where pin != nil, .fold where pin != nil:
            EmptyView()
        case .tools(let run):
            VStack(alignment: .leading, spacing: Tokens.Space.xs + 2) {
                ForEach(run, id: \.self) { i in
                    ToolLine(tool: session.model.tool(message.blocks[i].call), live: message.live)
                }
            }
        case .block(let i):
            block(message.blocks[i], key: "\(message.id)/\(i)")
        case .pictures(let run):
            PictureFan(files: run.compactMap { message.blocks[$0].file })
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
        case "thought" where pin != nil, "tool" where pin != nil, "plan" where pin != nil:
            EmptyView()
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
            if let c = b.card { CardView(card: c, tint: P.thread(agent?.color ?? ""), agent: agent?.name ?? "") }
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
/// fetches it and shows it. A picture a reply names (`named`) is drawn by its thumbnail,
/// and opens once the instance has kept it.
struct FileView: View {
    @Environment(Session.self) private var session
    var file: FileRef
    var named: [NamedFile] = []
    @Environment(\.inPin) private var inPin
    /// Its thumbnail, read and decoded off the main thread.
    @State private var thumb: UIImage?

    /// The most room a picture takes, and the place it keeps while its size isn't known;
    /// in a pin, less.
    static let most = CGSize(width: 360, height: 400)
    static let unknown = CGSize(width: 240, height: 160)
    static let mostInPin = CGSize(width: 200, height: 200)
    static let unknownInPin = CGSize(width: 160, height: 110)

    var body: some View {
        let path = file.isImage ? session.model.files["\(file.file).thumb"] : nil
        Button {
            if named.isEmpty {
                session.send(session.model.open(file))
            } else {
                for frame in session.model.viewNamed(named) { session.send(frame) }
            }
        } label: {
            if file.isImage {
                let image = path.flatMap { thumb ?? Pictures.kept($0) }
                // Its own place in the message, sized by what the instance said, else
                // its file's header, before it's decoded: scaled down to fit the column
                // and 360 × 400, its shape kept, and never up, as on the desktop. Until
                // its size is known, 240 × 160.
                let pixels = file.shownSize ?? path.flatMap { Pictures.size($0) } ?? image.map { CGSize(width: $0.size.width * $0.scale, height: $0.size.height * $0.scale) }
                let size = pixels.map { Pictures.fit($0, in: inPin ? FileView.mostInPin : FileView.most) } ?? (inPin ? FileView.unknownInPin : FileView.unknown)
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
                        Text(middleCut(file.name)).font(Fonts.small).foregroundStyle(P.ink).lineLimit(1)
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

/// Two or more pictures in a message as one fan of a fixed size, however many there are,
/// as the desktop draws it at rest: the first in front, up to four more tilted behind it
/// either side (`fanCards`), and what `fanCount` says in its corner. It keeps the room its
/// tilted cards reach into. Tapping it opens the viewer at the first: the pictures a
/// reply names (`named`, drawn by their thumbnails) once the instance has kept them.
struct PictureFan: View {
    @Environment(Session.self) private var session
    var files: [FileRef]
    var named: [NamedFile] = []
    @Environment(\.inPin) private var inPin

    /// The front card's size, and in a pin.
    static let size = CGSize(width: 240, height: 180)
    static let sizeInPin = CGSize(width: 136, height: 102)
    /// The room kept past the cards for their shadows.
    static let shadowRoom: CGFloat = 8

    var body: some View {
        let card = inPin ? Self.sizeInPin : Self.size
        let (w, h) = (card.width, card.height)
        let reach = fanReach(files.count, aspect: h / w)
        let (left, right) = ((reach.left * w).rounded(.up) + Self.shadowRoom, (reach.right * w).rounded(.up) + Self.shadowRoom)
        let up = (reach.up * w).rounded(.up) + Self.shadowRoom
        Button {
            if named.isEmpty {
                session.model.view(files)
            } else {
                for frame in session.model.viewNamed(named) { session.send(frame) }
            }
        } label: {
            ZStack {
                ForEach(fanCards(files.count), id: \.picture) { c in
                    FanPicture(file: files[c.picture], size: card, behind: true, count: nil)
                        .scaleEffect(c.scale)
                        .rotationEffect(.degrees(c.tilt))
                        .offset(x: c.x * w)
                        .fanShadow()
                }
                FanPicture(file: files[0], size: card, behind: false, count: fanCount(files.count))
                    .fanShadow()
            }
            .frame(width: w, height: h)
            .padding(.leading, left)
            .padding(.trailing, right)
            .padding(.vertical, up)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(files.count) pictures")
    }
}

private extension View {
    /// A fan card's soft shadow: black at 14%, 3 points down, a blur of 10 as the desktop
    /// measures it, which is a radius of 5 here.
    func fanShadow() -> some View {
        shadow(color: .black.opacity(0.14), radius: 5, x: 0, y: 3)
    }
}

/// One picture of a fan, filling the front card's place, with the bubble's corners and a
/// fine edge: the ones behind a little quieter, and the front one with the count.
private struct FanPicture: View {
    @Environment(Session.self) private var session
    var file: FileRef
    var size: CGSize
    var behind: Bool
    var count: String?
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
        .frame(width: size.width, height: size.height)
        .saturation(behind ? 0.88 : 1)
        .colorMultiply(Color(white: behind ? 0.97 : 1))
        .clipShape(shape)
        .overlay(shape.strokeBorder(P.line, lineWidth: 1))
        .overlay(alignment: .bottomTrailing) {
            if let count {
                Text(count)
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                    .monospacedDigit()
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .frame(minWidth: 20, minHeight: 20)
                    .background(.black.opacity(0.6), in: Capsule())
                    // A fine light edge keeps it apart from a dark picture, as the
                    // dark fill does from a light one.
                    .overlay(Capsule().strokeBorder(.white.opacity(0.18), lineWidth: 1))
                    .padding(Tokens.Space.s)
            }
        }
        .task(id: path) {
            guard let path, Pictures.kept(path) == nil else { return }
            thumb = await Pictures.load(path)
        }
    }
}

/// A stack's pictures big, over everything, a page each, with the picture's name and
/// where it is among them. Each is laid out at its final size from the first frame, its
/// thumbnail scaled up there until it's whole. Pinched, it zooms; zoomed, a swipe pans
/// it rather than going to the next, and going to another puts it back at its fit.
struct PictureViewer: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var stack: [FileRef]
    @State private var page = 0
    @State private var shown: Int? = 0
    @State private var zoomed = false

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            ScrollView(.horizontal) {
                LazyHStack(spacing: 0) {
                    ForEach(Array(stack.enumerated()), id: \.offset) { i, f in
                        WholePicture(file: f, current: i == page, zoomed: $zoomed)
                            .containerRelativeFrame([.horizontal, .vertical])
                            .id(i)
                    }
                }
                .scrollTargetLayout()
            }
            .scrollTargetBehavior(.paging)
            .scrollPosition(id: $shown)
            .scrollIndicators(.hidden)
            .scrollDisabled(zoomed)
            .ignoresSafeArea()
            .onChange(of: shown) {
                page = shown ?? 0
                zoomed = false
            }
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
            // With a count, the name ends just left of the middle and the count starts
            // just right of it, so the count stays put from one picture to the next.
            HStack(spacing: Tokens.Space.m) {
                Text(middleCut(stack[min(page, stack.count - 1)].name, 64))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .foregroundStyle(.white.opacity(0.75))
                    .frame(maxWidth: stack.count > 1 ? .infinity : nil, alignment: .trailing)
                if stack.count > 1 {
                    Text("\(page + 1) / \(stack.count)")
                        .monospacedDigit()
                        .foregroundStyle(.white.opacity(0.5))
                        .frame(maxWidth: .infinity, alignment: .leading)
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

/// One page of the viewer: the picture fitted to the screen, in the shape the instance
/// said it is (or its thumbnail's), from the first frame (LOO-303). Its thumbnail shows
/// scaled up to that place until the picture's here, which then fades in over it; zoomed
/// in, it's decoded whole, so it stays sharp.
private struct WholePicture: View {
    @Environment(Session.self) private var session
    var file: FileRef
    /// Whether it's the page on screen: one that isn't goes back to its fit.
    var current: Bool
    @Binding var zoomed: Bool
    @State private var thumb: UIImage?
    @State private var screen: UIImage?
    @State private var whole: UIImage?
    /// The shape it was first laid out in, kept so it never changes once it's shown.
    @State private var kept: CGSize?

    /// Points across it's decoded for at its fit, at the screen's scale.
    static let side: CGFloat = 600

    var body: some View {
        let thumbPath = session.model.files["\(file.file).thumb"]
        let wholePath = session.model.files[file.file]
        let known = kept ?? file.shownSize ?? wholePath.flatMap { Pictures.size($0) } ?? thumbPath.flatMap { Pictures.size($0) }
        let shown = whole ?? screen ?? wholePath.flatMap { Pictures.kept($0, side: Self.side) } ?? thumb ?? thumbPath.flatMap { Pictures.kept($0) }
        Group {
            if let known {
                ZoomingPicture(image: shown, shape: known, natural: file.shownSize ?? wholePath.flatMap { Pictures.size($0) } ?? known, current: current, zoomed: $zoomed)
            } else {
                ProgressView().tint(.white)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityLabel("Picture, \(file.name)")
        .onChange(of: known, initial: true) {
            if kept == nil { kept = known }
        }
        .task(id: thumbPath) {
            guard let thumbPath, Pictures.kept(thumbPath) == nil else { return }
            thumb = await Pictures.load(thumbPath)
        }
        .task(id: wholePath) {
            guard let wholePath else { return }
            screen = await Pictures.load(wholePath, side: Self.side)
        }
        .task(id: current && zoomed && wholePath != nil) {
            guard current, zoomed, let wholePath, whole == nil else { return }
            whole = await Pictures.load(wholePath)
        }
    }
}

/// A picture that pinches to zoom, toward the fingers, from its fit up to 8 times it, or
/// four times its own size where that's more, pans with a finger once zoomed, with the
/// scroll view's give at its edges, and goes between its fit and zoomed in with a double
/// tap. Its place is `shape` fitted to the page, never the image's own, so a sharper image
/// coming in takes the same place, faded in over the one before.
private struct ZoomingPicture: UIViewRepresentable {
    var image: UIImage?
    var shape: CGSize
    var natural: CGSize
    var current: Bool
    @Binding var zoomed: Bool

    func makeCoordinator() -> PictureZoom {
        PictureZoom(zoomed: $zoomed)
    }

    func makeUIView(context: Context) -> PictureScroll {
        let v = PictureScroll()
        v.delegate = context.coordinator
        v.picture.image = image
        let twice = UITapGestureRecognizer(target: context.coordinator, action: #selector(PictureZoom.doubleTap(_:)))
        twice.numberOfTapsRequired = 2
        v.addGestureRecognizer(twice)
        return v
    }

    func updateUIView(_ v: PictureScroll, context: Context) {
        context.coordinator.zoomed = $zoomed
        v.shape = shape
        v.natural = natural
        if !current && v.zoomScale > 1 {
            v.setZoomScale(1, animated: false)
        }
        if v.picture.image !== image {
            if v.picture.image == nil || image == nil {
                v.picture.image = image
            } else {
                UIView.transition(with: v.picture, duration: 0.18, options: [.transitionCrossDissolve, .allowUserInteraction]) {
                    v.picture.image = image
                }
            }
        }
    }
}

/// The scroll view a viewer's picture zooms in: the picture in the middle, at `shape`
/// fitted inside the page less room above and below for the controls and the caption.
final class PictureScroll: UIScrollView {
    let picture = UIImageView()
    var shape = CGSize(width: 1, height: 1) {
        didSet { if shape != oldValue { fittedFor = .zero; setNeedsLayout() } }
    }
    var natural = CGSize.zero
    /// The page size it was last fitted for.
    private var fittedFor = CGSize.zero

    /// The room kept above and below it at its fit.
    static let edge: CGFloat = 56

    override init(frame: CGRect) {
        super.init(frame: frame)
        picture.contentMode = .scaleToFill
        addSubview(picture)
        showsHorizontalScrollIndicator = false
        showsVerticalScrollIndicator = false
        contentInsetAdjustmentBehavior = .never
        bouncesZoom = true
        decelerationRate = .fast
        minimumZoomScale = 1
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    /// How many times its fit its own size is, at the screen's scale.
    var actual: CGFloat {
        let scale = traitCollection.displayScale
        return natural.width / max(picture.bounds.width * scale, 1)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if bounds.size != fittedFor, bounds.width > 0, bounds.height > 0 {
            fittedFor = bounds.size
            zoomScale = 1
            let room = CGSize(width: bounds.width, height: max(bounds.height - 2 * Self.edge, 1))
            let k = min(room.width / max(shape.width, 1), room.height / max(shape.height, 1))
            let size = CGSize(width: (shape.width * k).rounded(), height: (shape.height * k).rounded())
            picture.frame = CGRect(origin: .zero, size: size)
            contentSize = size
            maximumZoomScale = max(8, 4 * actual)
        }
        centre()
    }

    /// Keeps it in the middle while it's smaller than the page either way.
    func centre() {
        let x = max((bounds.width - contentSize.width) / 2, 0)
        let y = max((bounds.height - contentSize.height) / 2, 0)
        contentInset = UIEdgeInsets(top: y, left: x, bottom: y, right: x)
    }
}

/// What a viewer's picture does as it's zoomed: tells the viewer whether it's zoomed in,
/// so its swipes pan it, and zooms with a double tap.
@MainActor
final class PictureZoom: NSObject, UIScrollViewDelegate {
    var zoomed: Binding<Bool>

    init(zoomed: Binding<Bool>) {
        self.zoomed = zoomed
    }

    func viewForZooming(in scrollView: UIScrollView) -> UIView? {
        (scrollView as? PictureScroll)?.picture
    }

    func scrollViewDidZoom(_ scrollView: UIScrollView) {
        (scrollView as? PictureScroll)?.centre()
        let now = scrollView.zoomScale > 1.01
        if zoomed.wrappedValue != now {
            let zoomed = zoomed
            Task { @MainActor in zoomed.wrappedValue = now }
        }
    }

    /// Back to its fit when zoomed, else in to its own size, or 2.5 times its fit for
    /// one that's smaller, about where it was tapped.
    @objc func doubleTap(_ g: UITapGestureRecognizer) {
        guard let v = g.view as? PictureScroll else { return }
        if v.zoomScale > 1.01 {
            v.setZoomScale(1, animated: true)
            return
        }
        let to = min(v.maximumZoomScale, max(2.5, v.actual))
        let at = g.location(in: v.picture)
        let size = CGSize(width: v.bounds.width / to, height: v.bounds.height / to)
        v.zoom(to: CGRect(x: at.x - size.width / 2, y: at.y - size.height / 2, width: size.width, height: size.height), animated: true)
    }
}

// MARK: - Links and sources

/// A site's icon, once it's fetched, with loom's small radius; a globe until then.
struct SiteIcon: View {
    @Environment(Session.self) private var session
    var icon: String?
    var size: CGFloat = 16
    @State private var picture: UIImage?

    var body: some View {
        let path = icon.flatMap { session.model.files["\($0).thumb"] }
        if let image = path.flatMap({ picture ?? Pictures.kept($0) }) {
            Image(uiImage: image)
                .resizable()
                .scaledToFit()
                .frame(width: size, height: size)
                .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
                .accessibilityHidden(true)
        } else {
            Image(systemName: "globe")
                .font(.system(size: size * 0.75))
                .foregroundStyle(P.ink3)
                .frame(width: size, height: size)
                .accessibilityHidden(true)
                .task(id: path) {
                    guard let path, Pictures.kept(path) == nil else { return }
                    picture = await Pictures.load(path)
                }
        }
    }
}

/// The tab with a reply's sources (PLAN.md, "Links and sources"): the first three sites'
/// icons and how many there are, or the site of one alone. A tap opens their list in a
/// sheet, a line each, which opens the page.
struct SourcesTab: View {
    var message: Message
    @State private var open = false

    /// How far it hangs below its bubble, and how much of it is tucked under the bubble.
    static let hang: CGFloat = 21
    static let tuck: CGFloat = 7

    private static let fill = Color.loom(0xf7f7f8, 0x222226)
    private static let edge = Color.loom(0xe4e4e8, 0x2e2e33)
    private static let pressed = Color.loom(0xeaeaed, 0x2c2c31)

    var body: some View {
        let shown = message.sources.map { message.shown($0) }
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
        Button { open = true } label: {
            HStack(spacing: 6) {
                HStack(spacing: 3) {
                    // Each site's icon once, the first three.
                    let icons = shown.compactMap(\.icon).reduce(into: [String]()) { out, i in if !out.contains(i) { out.append(i) } }.prefix(3)
                    if icons.isEmpty {
                        SiteIcon(icon: nil)
                    } else {
                        ForEach(Array(icons.enumerated()), id: \.offset) { _, icon in SiteIcon(icon: icon) }
                    }
                }
                if shown.count == 1, let one = shown.first {
                    Text(one.site).font(Fonts.ui(Tokens.PhoneSize.caption, relativeTo: .caption))
                } else {
                    Text("\(shown.count) sources").font(Fonts.caption)
                }
                Image(systemName: "chevron.down").font(.system(size: 10, weight: .semibold)).foregroundStyle(P.ink3)
            }
            .foregroundStyle(P.ink2)
            .padding(.top, SourcesTab.tuck)
            .padding(.leading, 5)
            .padding(.trailing, Tokens.Space.s)
            .frame(height: SourcesTab.hang + SourcesTab.tuck)
            .background(open ? SourcesTab.pressed : SourcesTab.fill, in: shape)
            .overlay(shape.strokeBorder(SourcesTab.edge, lineWidth: 1))
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(shown.count == 1 ? "Source, \(shown[0].site)" : "\(shown.count) sources")
        .sheet(isPresented: $open) {
            SourcesSheet(message: message)
                .presentationDetents([.medium, .large])
                .presentationDragIndicator(.visible)
        }
    }
}

/// A reply's sources, a line each: its site's icon, its title, and its site.
private struct SourcesSheet: View {
    @Environment(\.openURL) private var openURL
    @Environment(\.dismiss) private var dismiss
    var message: Message

    var body: some View {
        NavigationStack {
            List {
                ForEach(Array(message.sources.enumerated()), id: \.offset) { _, s in
                    let shown = message.shown(s)
                    Button {
                        if let url = URL(string: s.url) { openURL(url) }
                        dismiss()
                    } label: {
                        HStack(spacing: Tokens.Space.m) {
                            SiteIcon(icon: shown.icon, size: 20)
                            Text(shown.title).font(Fonts.body).foregroundStyle(P.ink).lineLimit(1)
                            Spacer(minLength: Tokens.Space.s)
                            Text(shown.site).font(Fonts.ui(Tokens.PhoneSize.caption, relativeTo: .caption)).foregroundStyle(P.ink3).lineLimit(1).truncationMode(.tail)
                        }
                    }
                    .listRowBackground(P.raised)
                }
            }
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .navigationTitle("Sources")
            .navigationBarTitleDisplayMode(.inline)
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
