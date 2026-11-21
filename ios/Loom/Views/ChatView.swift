import PhotosUI
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

/// An agent's chat: its messages, the reply being written, the questions it's waiting
/// on, and the composer.
struct ChatView: View {
    @Environment(Session.self) private var session
    let agentId: String
    @State private var atBottom = true
    @State private var openThoughts: Set<String> = []

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
                        if m.isPerson {
                            PersonMessage(message: m)
                        } else {
                            Reply(message: m, agent: agent, openThoughts: $openThoughts)
                        }
                    }
                    ForEach(approvals) { a in
                        ApprovalCard(approval: a)
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
            .onChange(of: Follow(messages: messages, approvals: approvals.count)) {
                // Follow a reply as it's written, unless the person scrolled up to read.
                if atBottom {
                    withAnimation(.easeOut(duration: 0.15)) { proxy.scrollTo("end", anchor: .bottom) }
                }
            }
            .overlay {
                if messages.isEmpty && approvals.isEmpty && model.selected == agentId {
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
        .quickLookPreview(Bindable(model).toOpen)
        .onAppear {
            model.select(agentId)
            session.flush()
        }
    }

    /// What, when it changes, scrolls the chat along.
    private struct Follow: Equatable {
        var messages: [Message]
        var approvals: Int
    }
}

// MARK: - Messages

/// What the person wrote: a bubble on the right, with its files above it.
struct PersonMessage: View {
    @Environment(Session.self) private var session
    var message: Message

    var body: some View {
        let text = message.blocks.filter { $0.kind == "text" }.map(\.text).joined(separator: "\n")
        VStack(alignment: .trailing, spacing: Tokens.Space.xs + 2) {
            if let from = message.from {
                Text("From \(from)").font(Fonts.caption).foregroundStyle(P.ink3)
            }
            ForEach(Array(message.blocks.enumerated()), id: \.offset) { _, b in
                if let f = b.file {
                    FileView(file: f)
                }
            }
            if !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                MarkdownView(text, keepBreaks: true, font: Fonts.body)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, Tokens.Space.l)
                    .padding(.vertical, Tokens.Space.s + 3)
                    .background(P.bubble, in: RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
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
/// calls quiet.
struct Reply: View {
    @Environment(Session.self) private var session
    var message: Message
    var agent: Agent?
    @Binding var openThoughts: Set<String>

    var body: some View {
        let model = session.model
        VStack(alignment: .leading, spacing: Tokens.Space.s + 2) {
            HStack(spacing: Tokens.Space.s) {
                ThreadView(mood: message.live ? model.mood(agent?.id ?? "") : .idle, color: P.thread(agent?.color ?? ""))
                    .frame(width: 16, height: 10)
                Text(agent?.name ?? "Agent").font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink)
                Text(message.live ? "now" : When.time(message.at)).font(Fonts.caption).foregroundStyle(P.ink3)
            }
            ForEach(Array(message.blocks.enumerated()), id: \.offset) { i, b in
                block(b, key: "\(message.id)/\(i)")
            }
            if message.live && message.blocks.allSatisfy({ $0.text.isEmpty && $0.call == nil }) {
                Text("Thinking…").font(Fonts.small).foregroundStyle(P.ink3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func block(_ b: Block, key: String) -> some View {
        switch b.kind {
        case "text" where !b.text.isEmpty:
            MarkdownView(b.text)
        case "thought" where !b.text.isEmpty:
            Thought(text: b.text, open: openThoughts.contains(key)) {
                if openThoughts.contains(key) { openThoughts.remove(key) } else { openThoughts.insert(key) }
            }
        case "tool":
            ToolLine(tool: session.model.tool(b.call))
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
        default:
            EmptyView()
        }
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
                    .textSelection(.enabled)
            }
        }
        .animation(.easeOut(duration: 0.15), value: open)
    }
}

/// A tool call, one quiet line: what it is, and how it went.
struct ToolLine: View {
    var tool: Tool?

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
        case nil where tool != nil: ProgressView().controlSize(.mini)
        default: EmptyView()
        }
    }
}

/// A file in a message: a picture for an image, its name and size otherwise. Tapping it
/// fetches it and shows it.
struct FileView: View {
    @Environment(Session.self) private var session
    var file: FileRef

    var body: some View {
        Button {
            session.send(session.model.open(file))
        } label: {
            if file.isImage {
                if let path = session.model.files["\(file.file).thumb"], let image = UIImage(contentsOfFile: path) {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: 240, maxHeight: 240)
                        .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
                } else {
                    RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
                        .fill(P.bubble)
                        .frame(width: 200, height: 140)
                        .overlay { Image(systemName: "photo").foregroundStyle(P.ink3) }
                }
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
    }
}

// MARK: - Approvals

/// A question the agent waits on, with Allow and Deny.
struct ApprovalCard: View {
    @Environment(Session.self) private var session
    var approval: Approval

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            Text(approval.title.isEmpty ? "Can it go ahead?" : "\(approval.title)?")
                .font(Fonts.ui(Tokens.PhoneSize.body, .medium))
                .foregroundStyle(P.ink)
            let detail = ApprovalCard.command(approval.detail)
            if !detail.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(detail)
                        .font(Fonts.code)
                        .foregroundStyle(P.ink2)
                        .fixedSize()
                        .padding(.horizontal, Tokens.Space.m)
                        .padding(.vertical, Tokens.Space.s + 2)
                }
                .frame(maxHeight: 180)
                .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.line, lineWidth: 1))
            }
            HStack(spacing: Tokens.Space.s) {
                Button("Allow") { answer(true) }
                    .buttonStyle(LoomButtonStyle(primary: true, wide: true))
                Button("Deny") { answer(false) }
                    .buttonStyle(LoomButtonStyle(primary: false, wide: true))
            }
            if approval.canAlways {
                Button("Always allow this") { answer(true, always: true) }
                    .font(Fonts.caption)
                    .foregroundStyle(P.ink3)
                    .frame(maxWidth: .infinity)
            }
        }
        .padding(Tokens.Space.l)
        .background(P.raised, in: RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous).strokeBorder(P.line, lineWidth: 1))
    }

    private func answer(_ allow: Bool, always: Bool = false) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        session.send(session.model.approve(approval.call, allow: allow, always: always))
    }

    /// What an approval would run, from the tool's input: its command if it has one.
    static func command(_ detail: String) -> String {
        guard let data = detail.data(using: .utf8), let v = try? JSON(parsing: data) else { return detail }
        switch v["command"] {
        case .string(let c): return c
        case .array(let parts): return parts.compactMap(\.string).joined(separator: " ")
        default:
            switch v {
            case .null: return ""
            case .string(let s): return s
            default: return v.pretty
            }
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
