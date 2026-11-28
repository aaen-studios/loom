import SwiftUI

/// The agents and group chats, as the desktop's sidebar lists them (PLAN.md, "The
/// sidebar"): the pinned agents as tiles, who's waiting on the person, then one list of
/// agents and group chats in the person's order with the folders in it, which a long
/// press and a drag rearranges, and the hidden agents.
struct AgentsView: View {
    @Environment(Session.self) private var session
    @State private var path: [String] = []
    @State private var settings = false
    /// Whether the hidden agents show, and the folder being named.
    @State private var hiddenOpen = false
    @State private var naming: Naming?
    /// The sheet that makes a group chat.
    @State private var newRoom = false
    /// The agent Make private asks about, saying what it loses: in the system's dialog,
    /// or, where the instance offers a sandbox of its own too, in a sheet with that switch.
    @State private var makingPrivate: Agent?
    @State private var isolatingPrivate: Agent?
    @State private var isolateToo = false

    private static let insets = EdgeInsets(top: 0, leading: Tokens.PhoneSpace.gutter, bottom: 0, trailing: Tokens.Space.l)

    var body: some View {
        let model = session.model
        let sections = model.sideSections
        let pinned = Self.agents(.pinned, sections, model)
        let waiting = Self.agents(.waiting, sections, model)
        let hidden = Self.agents(.hidden, sections, model)
        NavigationStack(path: $path) {
            List {
                if !pinned.isEmpty {
                    tiles(pinned)
                }
                if !waiting.isEmpty {
                    Section {
                        ForEach(waiting) { agentLink($0) }
                    } header: {
                        GroupHead(title: "Waiting on you")
                    }
                }
                Section {
                    ForEach(model.listRows) { listRow($0) }
                        .onMove { from, to in
                            guard let f = from.first, let moved = model.rowDrop(from: f, to: to) else { return }
                            for c in model.drop(moved.0, moved.1) { session.send(c) }
                        }
                    newRow
                }
                if !hidden.isEmpty {
                    Section {
                        if hiddenOpen { ForEach(hidden) { agentLink($0) } }
                    } header: {
                        Button {
                            hiddenOpen.toggle()
                        } label: {
                            GroupHead(title: "Hidden · \(hidden.count)", open: hiddenOpen)
                        }
                    }
                }
            }
            .alert(naming?.folder == nil ? "New folder" : "Rename folder", isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } })) {
                TextField("Name", text: Binding(get: { naming?.name ?? "" }, set: { naming?.name = $0 }))
                Button("Cancel", role: .cancel) { naming = nil }
                Button(naming?.folder == nil ? "Make" : "Rename") {
                    if let n = naming {
                        if let folder = n.folder {
                            session.send(model.renameFolder(folder, n.name))
                        } else {
                            session.send(model.newFolder(n.name, agent: n.agent))
                        }
                    }
                    naming = nil
                }
            }
            .confirmationDialog("Make \(makingPrivate?.name ?? "it") private?", isPresented: Binding(get: { makingPrivate != nil }, set: { if !$0 { makingPrivate = nil } }), titleVisibility: .visible) {
                Button("Make private") {
                    if let a = makingPrivate { session.send(model.makePrivate(a.id, true)) }
                    makingPrivate = nil
                }
                Button("Cancel", role: .cancel) { makingPrivate = nil }
            } message: {
                Text(Model.privateLoses)
            }
            .sheet(item: $isolatingPrivate) { a in
                MakePrivateSheet(name: a.name, isolate: $isolateToo, cancel: { isolatingPrivate = nil }) {
                    session.send(model.makePrivate(a.id, true, isolate: isolateToo && model.offersIsolating(a.id)))
                    isolatingPrivate = nil
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .overlay {
                if model.agents.isEmpty && model.synced {
                    VStack(spacing: Tokens.Space.s) {
                        Text("No agents yet").font(Fonts.ui(Tokens.PhoneSize.body, .medium)).foregroundStyle(P.ink)
                        Text("Make one in loom on your computer")
                            .font(Fonts.small).foregroundStyle(P.ink3).multilineTextAlignment(.center)
                    }
                    .padding(Tokens.Space.xxl)
                }
            }
            .safeAreaInset(edge: .top, spacing: 0) {
                VStack(alignment: .leading, spacing: 2) {
                    ConnLine(conn: model.conn)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .animation(.default, value: model.conn)
            }
            .navigationTitle("Agents")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        settings = true
                    } label: {
                        Image(systemName: "gearshape")
                    }
                    .accessibilityLabel("Settings")
                }
            }
            .navigationDestination(for: String.self) { id in
                if id.hasPrefix("room:") {
                    ChatView(agentId: "", roomId: String(id.dropFirst(5)))
                } else {
                    ChatView(agentId: id)
                }
            }
            .toolbarBackground(P.bg, for: .navigationBar)
        }
        .sheet(isPresented: $settings) {
            SettingsView()
        }
        .sheet(isPresented: $newRoom) {
            NewRoomSheet()
        }
        // A group chat made here opens.
        .onChange(of: model.room) { _, id in
            guard let id, path.last != "room:" + id, model.makingRoom == nil else { return }
            if path.isEmpty { path = ["room:" + id] }
        }
        .background {
            Warmup(agent: model.synced && path.isEmpty ? model.shownAgents.first?.id : nil)
        }
        .onAppear(perform: openFromLaunch)
        // Back at the list, the chat is left, though its agent stays selected.
        .onChange(of: path) { _, now in
            if now.isEmpty { model.leaveChat() }
        }
        // A tap on an agent a message mentions opens its chat, in place of this one, and
        // `-live` at launch the first agent's, which may be asked for before this shows.
        .onChange(of: model.openAgent, initial: true) { _, id in
            guard let id else { return }
            model.openAgent = nil
            path = [id]
        }
    }

    /// The agents of a part of the list, in order.
    private static func agents(_ part: SideSection.Part, _ sections: [SideSection], _ model: Model) -> [Agent] {
        (sections.first { $0.part == part }?.agents ?? []).compactMap { model.agent($0) }
    }

    /// The pinned agents as tiles, four to a row, each row centred. A tap opens its chat,
    /// a long press has its menu, and a long press and a drag moves it among the tiles.
    private func tiles(_ agents: [Agent]) -> some View {
        let model = session.model
        return CentredRows(columns: 4, spacing: Tokens.Space.s) {
            ForEach(agents) { agent in
                Button {
                    path = [agent.id]
                } label: {
                    AgentTile(agent: agent)
                }
                .buttonStyle(.plain)
                .contextMenu { agentMenu(agent) }
                .draggable(agent.id) {
                    AgentTile(agent: agent).frame(width: 80)
                }
                .dropDestination(for: String.self) { ids, _ in
                    guard let id = ids.first else { return false }
                    let sent = model.drop(id, model.onto(id, agent.id))
                    for c in sent { session.send(c) }
                    return !sent.isEmpty
                }
            }
        }
        .padding(.vertical, Tokens.Space.m)
        .listRowBackground(P.bg)
        .listRowSeparatorTint(P.line)
        .listRowInsets(EdgeInsets(top: 0, leading: Tokens.PhoneSpace.gutter, bottom: 0, trailing: Tokens.PhoneSpace.gutter))
    }

    /// A row of the list: an agent, a group chat, or a folder's name.
    @ViewBuilder
    private func listRow(_ row: SideRow) -> some View {
        let model = session.model
        switch row {
        case .item(.agent(let id)):
            if let agent = model.agent(id) { agentLink(agent) }
        case .item(.room(let id)):
            if let room = model.roomNamed(id) { roomLink(room) }
        case .folder(let id, let name, let collapsed):
            folderHead(id, name, collapsed)
        }
    }

    /// An agent's row, which opens its chat, with a long press for its folder and
    /// whether it's hidden.
    private func agentLink(_ agent: Agent) -> some View {
        NavigationLink(value: agent.id) {
            AgentRow(agent: agent)
        }
        .listRowBackground(P.bg)
        .listRowSeparatorTint(P.line)
        .listRowInsets(Self.insets)
        .contextMenu { agentMenu(agent) }
    }

    /// A group chat's row, which opens it, with a long press for its folder, and to delete
    /// it.
    private func roomLink(_ room: Room) -> some View {
        let model = session.model
        return NavigationLink(value: "room:" + room.id) {
            RoomRow(room: room)
        }
        .listRowBackground(P.bg)
        .listRowSeparatorTint(P.line)
        .listRowInsets(Self.insets)
        .contextMenu {
            folderMenu(room.id)
            Button("Delete group chat", systemImage: "trash", role: .destructive) {
                session.send(model.deleteRoom(room.id))
            }
        }
    }

    /// A folder's name in the list, which a tap opens and shuts, and a long press renames
    /// or deletes. It stays where it is; what's moved goes in and out under it.
    private func folderHead(_ id: String, _ name: String, _ collapsed: Bool) -> some View {
        let model = session.model
        return Button {
            session.send(model.collapseFolder(id, !collapsed))
        } label: {
            GroupHead(title: name, open: !collapsed)
                .padding(.top, Tokens.Space.m)
                .padding(.bottom, Tokens.Space.xs)
        }
        .buttonStyle(.plain)
        .listRowBackground(P.bg)
        .listRowSeparator(.hidden)
        .listRowInsets(Self.insets)
        .moveDisabled(true)
        .contextMenu {
            newItems
            Divider()
            Button("Rename", systemImage: "pencil") {
                naming = Naming(folder: id, agent: nil, name: name)
            }
            Button("Delete folder", systemImage: "trash", role: .destructive) {
                session.send(model.deleteFolder(id))
            }
        }
    }

    /// The list's last row: what can be made from here.
    private var newRow: some View {
        Menu {
            newItems
        } label: {
            HStack(spacing: Tokens.Space.m + 2) {
                Image(systemName: "plus")
                    .font(.system(size: 13, weight: .medium))
                    .frame(width: 26, height: 22)
                Text("New").font(Fonts.ui(Tokens.PhoneSize.body))
                Spacer(minLength: 0)
            }
            .foregroundStyle(P.ink3)
            .padding(.vertical, Tokens.Space.m)
            .contentShape(Rectangle())
        }
        .listRowBackground(P.bg)
        .listRowSeparator(.hidden)
        .listRowInsets(Self.insets)
        .moveDisabled(true)
    }

    @ViewBuilder
    private var newItems: some View {
        Button("New group chat", systemImage: "person.2") {
            session.model.startRoomForm()
            newRoom = true
        }
            .disabled(session.model.agents.count < 2)
        Button("New folder", systemImage: "folder.badge.plus") {
            naming = Naming(folder: nil, agent: nil, name: "")
        }
    }

    /// An agent's long-press menu: its folder, whether it's private, and whether it's
    /// hidden. Make private asks first; normal again needs no asking.
    @ViewBuilder
    private func agentMenu(_ agent: Agent) -> some View {
        let model = session.model
        folderMenu(agent.id)
        Button(agent.isPrivate ? "Make normal" : "Make private", systemImage: agent.isPrivate ? "lock.open" : "lock") {
            if agent.isPrivate {
                session.send(model.makePrivate(agent.id, false))
            } else if model.offersIsolating(agent.id) {
                isolateToo = false
                isolatingPrivate = agent
            } else {
                makingPrivate = agent
            }
        }
        Button(agent.hidden ? "Show" : "Hide", systemImage: agent.hidden ? "eye" : "eye.slash") {
            session.send(model.hideAgent(agent.id, !agent.hidden))
        }
    }

    /// Move to folder, for an agent or a group chat: each folder, Remove from folder, and
    /// New folder, named in an alert.
    private func folderMenu(_ item: String) -> some View {
        let model = session.model
        let current = model.folderOf(item) ?? model.agent(item)?.group
        return Menu("Move to folder", systemImage: "folder") {
            ForEach(model.folders, id: \.id) { folder in
                Button {
                    session.send(model.regroup(item, folder.id))
                } label: {
                    if current == folder.id {
                        Label(folder.name, systemImage: "checkmark")
                    } else {
                        Text(folder.name)
                    }
                }
                .disabled(current == folder.id)
            }
            if current != nil {
                Button("Remove from folder", systemImage: "folder.badge.minus") {
                    session.send(model.regroup(item, nil))
                }
            }
            Button("New folder", systemImage: "folder.badge.plus") {
                naming = Naming(folder: nil, agent: item, name: "")
            }
        }
    }

    /// `-open <agent>` and `-settings` at launch open that screen, for screenshots of each
    /// (ios/README.md).
    private func openFromLaunch() {
        let args = ProcessInfo.processInfo.arguments
        if let i = args.firstIndex(of: "-open"), i + 1 < args.count { path = [args[i + 1]] }
        if args.contains("-settings") { settings = true }
    }
}

/// A folder being named: a new one, for an agent or a group chat, or one being renamed.
private struct Naming {
    var folder: String?
    var agent: String?
    var name: String
}

/// Square tiles, `columns` to a row at the size that fills one, each row centred, so a
/// row of fewer has them in the middle.
private struct CentredRows: Layout {
    var columns: Int
    var spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.replacingUnspecifiedDimensions().width
        let rows = (subviews.count + columns - 1) / columns
        let height = rows == 0 ? 0 : CGFloat(rows) * side(width) + CGFloat(rows - 1) * spacing
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let s = side(bounds.width)
        for (i, v) in subviews.enumerated() {
            let row = i / columns
            let inRow = CGFloat(min(columns, subviews.count - row * columns))
            let left = (bounds.width - inRow * s - (inRow - 1) * spacing) / 2
            let x = bounds.minX + left + CGFloat(i % columns) * (s + spacing)
            let y = bounds.minY + CGFloat(row) * (s + spacing)
            v.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: s, height: s))
        }
    }

    private func side(_ width: CGFloat) -> CGFloat {
        max(0, (width - CGFloat(columns - 1) * spacing) / CGFloat(columns))
    }
}

/// A pinned agent's tile: its thread drawn large in its shape, moving as the agent works,
/// and its name under it.
private struct AgentTile: View {
    @Environment(Session.self) private var session
    var agent: Agent

    var body: some View {
        let model = session.model
        let mood = model.mood(agent.id)
        RoundedRectangle(cornerRadius: Tokens.Radius.card)
            .fill(P.raised)
            .aspectRatio(1, contentMode: .fit)
            .overlay {
                VStack(spacing: Tokens.Space.xs) {
                    Group {
                        if let outline = agent.outline {
                            ShapeView(points: outline, mood: mood, color: P.thread(agent.color), snapped: model.finished[agent.id])
                        } else {
                            ThreadView(mood: mood, color: P.thread(agent.color), snapped: model.finished[agent.id])
                                .frame(height: 22)
                        }
                    }
                    .frame(width: 48, height: 32)
                    HStack(spacing: Tokens.Space.xs) {
                        Text(agent.name)
                            .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                            .foregroundStyle(P.ink)
                            .lineLimit(1)
                        if agent.isPrivate { LockMark() }
                    }
                }
                .padding(.horizontal, Tokens.Space.xs)
            }
            // A reply not read: the desktop tile's ink dot in its corner.
            .overlay(alignment: .topTrailing) {
                if model.unread(agent: agent.id) {
                    UnreadDot().padding(Tokens.Space.s)
                }
            }
            .contentShape(RoundedRectangle(cornerRadius: Tokens.Radius.card))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(agent.name)
            .accessibilityValue(model.preview(agent))
    }
}

/// A group's head in the agent list: its name, small and quiet, with a chevron when it
/// opens and shuts.
private struct GroupHead: View {
    var title: String
    var open: Bool?

    var body: some View {
        HStack(spacing: Tokens.Space.xs) {
            Text(title)
                .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                .lineLimit(1)
            if let open {
                Image(systemName: open ? "chevron.down" : "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
            }
            Spacer(minLength: 0)
        }
        .foregroundStyle(P.ink3)
        .textCase(nil)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(open == nil ? [.isHeader] : [.isHeader, .isButton])
    }
}

/// The chat screen drawn once, hidden, a moment after the agents are shown, so the
/// first chat opened isn't the first time SwiftUI and UIKit build one: that made its
/// push about twice as long as any later one's (LOO-228). It selects and fetches
/// nothing, and goes again half a second later.
private struct Warmup: View {
    /// The agent whose chat it draws, once there's one and no chat is open.
    var agent: String?
    @State private var drawing: String?
    @State private var done = false

    var body: some View {
        Group {
            if let drawing {
                NavigationStack { ChatView(agentId: drawing, warming: true) }
                    .opacity(0)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
            }
        }
        .task(id: agent != nil) {
            guard agent != nil, !done else { return }
            try? await Task.sleep(for: .milliseconds(300))
            // A chat opened, or the connection went, in the meantime.
            guard !Task.isCancelled, let agent else { return }
            done = true
            drawing = agent
            try? await Task.sleep(for: .milliseconds(500))
            drawing = nil
        }
    }
}

struct AgentRow: View {
    @Environment(Session.self) private var session
    var agent: Agent

    var body: some View {
        let model = session.model
        let mood = model.mood(agent.id)
        HStack(spacing: Tokens.Space.m + 2) {
            Group {
                if let outline = agent.outline {
                    ShapeView(points: outline, mood: mood, color: P.thread(agent.color), snapped: model.finished[agent.id])
                } else {
                    ThreadView(mood: mood, color: P.thread(agent.color), snapped: model.finished[agent.id])
                        .frame(height: 14)
                }
            }
            .frame(width: 26, height: 22)
            let unread = model.unread(agent: agent.id)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: Tokens.Space.s) {
                    Text(agent.name)
                        .font(Fonts.ui(Tokens.PhoneSize.body, unread ? .semibold : .medium))
                        .foregroundStyle(P.ink)
                    if agent.isPrivate { LockMark() }
                }
                Text(model.preview(agent))
                    .font(Fonts.small)
                    .foregroundStyle(mood == .failed ? P.bad : unread ? P.ink : P.ink3)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            // Waiting on the person: a warm dot, with what it asks under its name. Else a
            // reply not read: an ink dot, or for one in another agent's thread with it, a
            // dot in the asker's colour (PLAN.md, "The sidebar").
            if mood == .waiting {
                Circle().fill(P.warn).frame(width: 6, height: 6)
            } else if unread {
                UnreadDot()
            } else if let asker = model.askUnread(agent.id) {
                UnreadDot(color: model.agent(asker).map { P.thread($0.color) } ?? P.ink)
            }
        }
        .padding(.vertical, Tokens.Space.m + 2)
        .accessibilityElement(children: .combine)
    }
}

/// Make private's question where the instance has sandboxes, for an agent that isn't
/// isolated (PLAN.md, "Private agents"): what it loses, one switch, off at first, to run
/// it in a sandbox of its own too, with what that costs under it, then Cancel and Make
/// private. As tall as what it says.
private struct MakePrivateSheet: View {
    let name: String
    @Binding var isolate: Bool
    let cancel: () -> Void
    let yes: () -> Void
    @State private var tall: CGFloat = 320

    var body: some View {
        VStack(alignment: .leading, spacing: Tokens.Space.l) {
            Text("Make \(name) private?")
                .font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                .foregroundStyle(P.ink)
            Text(Model.privateLoses)
                .font(Fonts.small)
                .foregroundStyle(P.ink3)
                .fixedSize(horizontal: false, vertical: true)
            Toggle(isOn: $isolate) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(Model.privateIsolate).font(Fonts.small).foregroundStyle(P.ink)
                    Text(Model.privateIsolateCosts).font(Fonts.caption).foregroundStyle(P.ink3)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .tint(P.button)
            HStack(spacing: Tokens.Space.s) {
                Button(action: cancel) { Text("Cancel").frame(maxWidth: .infinity) }
                    .buttonStyle(.loomQuiet)
                Button(action: yes) { Text("Make private").frame(maxWidth: .infinity) }
                    .buttonStyle(.loom)
            }
        }
        .padding(.horizontal, Tokens.PhoneSpace.gutter)
        .padding(.top, Tokens.Space.xl)
        .padding(.bottom, Tokens.Space.l)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { tall = $0 }
        .frame(maxHeight: .infinity, alignment: .top)
        .background(P.bg)
        .presentationDetents([.height(tall)])
        .presentationDragIndicator(.visible)
    }
}

/// A private agent's lock, small and quiet after its name (PLAN.md, "Private agents").
struct LockMark: View {
    var body: some View {
        Image(systemName: "lock")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(P.ink3)
            .accessibilityLabel("Private")
    }
}

/// The dot on an agent, a group chat or a thread with a reply the person hasn't read: ink,
/// or the asker's colour for another agent's thread.
struct UnreadDot: View {
    var color: Color = P.ink

    var body: some View {
        Circle().fill(color).frame(width: 6, height: 6)
            .accessibilityLabel("Unread")
    }
}

#Preview("Agents") {
    AgentsView().environment(Session(sample: true))
}

#Preview("Agents, dark") {
    AgentsView().environment(Session(sample: true)).preferredColorScheme(.dark)
}
