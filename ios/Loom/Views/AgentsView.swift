import SwiftUI

/// The agents, each one ongoing chat, as the desktop's sidebar shows them: its thread,
/// its name, and what it's doing.
struct AgentsView: View {
    @Environment(Session.self) private var session
    @State private var path: [String] = []
    @State private var settings = false
    /// Whether the hidden agents show, and the folder being named.
    @State private var hiddenOpen = false
    @State private var naming: Naming?
    /// The sheet that makes a group chat.
    @State private var newRoom = false

    var body: some View {
        let model = session.model
        NavigationStack(path: $path) {
            List {
                // The group chats first, Everyone at the top, as the core orders them.
                if !model.rooms.isEmpty || model.agents.count >= 2 {
                    Section {
                        ForEach(model.rooms) { r in
                            NavigationLink(value: "room:" + r.id) {
                                RoomRow(room: r)
                            }
                            .listRowBackground(P.bg)
                            .listRowSeparatorTint(P.line)
                            .listRowInsets(EdgeInsets(top: 0, leading: Tokens.PhoneSpace.gutter, bottom: 0, trailing: Tokens.Space.l))
                            .contextMenu {
                                if !r.everyone {
                                    Button("Delete group chat", systemImage: "trash", role: .destructive) {
                                        session.send(model.deleteRoom(r.id))
                                    }
                                }
                            }
                        }
                    } header: {
                        HStack {
                            GroupHead(title: "Group chats")
                            Button {
                                model.startRoomForm()
                                newRoom = true
                            } label: {
                                Image(systemName: "plus").font(.system(size: 12, weight: .semibold)).foregroundStyle(P.ink3)
                            }
                            .accessibilityLabel("New group chat")
                        }
                    }
                }
                if model.sidebar.isEmpty {
                    rows(model.agents)
                }
                ForEach(Array(model.sidebar.enumerated()), id: \.offset) { _, section in
                    let members = section.agents.compactMap { model.agent($0) }
                    switch section.part {
                    case .waiting:
                        if !members.isEmpty {
                            Section { rows(members) } header: { GroupHead(title: "Waiting on you") }
                        }
                    case .pinned:
                        rows(members)
                    case .agents:
                        // One just made that the list doesn't have yet goes here too.
                        let listed = Set(model.sidebar.flatMap(\.agents))
                        rows(members + model.agents.filter { !listed.contains($0.id) })
                    case .folder(let id, let name, let collapsed):
                        Section {
                            if !collapsed { rows(members) }
                        } header: {
                            Button {
                                session.send(model.collapseFolder(id, !collapsed))
                            } label: {
                                GroupHead(title: name, open: !collapsed)
                            }
                            .contextMenu {
                                Button("Rename", systemImage: "pencil") {
                                    naming = Naming(folder: id, agent: nil, name: name)
                                }
                                Button("Delete folder", systemImage: "trash", role: .destructive) {
                                    session.send(model.deleteFolder(id))
                                }
                            }
                        }
                    case .hidden:
                        if !members.isEmpty {
                            Section {
                                if hiddenOpen { rows(members) }
                            } header: {
                                Button {
                                    hiddenOpen.toggle()
                                } label: {
                                    GroupHead(title: "Hidden", open: hiddenOpen)
                                }
                            }
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
        // A tap on an agent a message mentions opens its chat, in place of this one.
        .onChange(of: model.openAgent) { _, id in
            guard let id else { return }
            model.openAgent = nil
            path = [id]
        }
    }

    /// Each agent's row, which opens its chat, with a long press for its folder and
    /// whether it's hidden.
    private func rows(_ agents: [Agent]) -> some View {
        let model = session.model
        return ForEach(agents) { agent in
            NavigationLink(value: agent.id) {
                AgentRow(agent: agent)
            }
            .listRowBackground(P.bg)
            .listRowSeparatorTint(P.line)
            .listRowInsets(EdgeInsets(top: 0, leading: Tokens.PhoneSpace.gutter, bottom: 0, trailing: Tokens.Space.l))
            .contextMenu {
                Menu("Move to folder", systemImage: "folder") {
                    ForEach(model.folders, id: \.id) { folder in
                        Button {
                            session.send(model.groupAgent(agent.id, folder.id))
                        } label: {
                            if agent.group == folder.id {
                                Label(folder.name, systemImage: "checkmark")
                            } else {
                                Text(folder.name)
                            }
                        }
                        .disabled(agent.group == folder.id)
                    }
                    if agent.group != nil {
                        Button("Remove from folder", systemImage: "folder.badge.minus") {
                            session.send(model.groupAgent(agent.id, nil))
                        }
                    }
                    Button("New folder", systemImage: "folder.badge.plus") {
                        naming = Naming(folder: nil, agent: agent.id, name: "")
                    }
                }
                Button(agent.hidden ? "Show" : "Hide", systemImage: agent.hidden ? "eye" : "eye.slash") {
                    session.send(model.hideAgent(agent.id, !agent.hidden))
                }
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

/// A folder being named: a new one, for an agent, or one being renamed.
private struct Naming {
    var folder: String?
    var agent: String?
    var name: String
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
            VStack(alignment: .leading, spacing: 3) {
                Text(agent.name)
                    .font(Fonts.ui(Tokens.PhoneSize.body, .medium))
                    .foregroundStyle(P.ink)
                Text(model.preview(agent))
                    .font(Fonts.small)
                    .foregroundStyle(mood == .failed ? P.bad : P.ink3)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            if mood == .waiting {
                Text("Asks")
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                    .foregroundStyle(P.buttonInk)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(P.button, in: Capsule())
            }
        }
        .padding(.vertical, Tokens.Space.m + 2)
        .accessibilityElement(children: .combine)
    }
}

#Preview("Agents") {
    AgentsView().environment(Session(sample: true))
}

#Preview("Agents, dark") {
    AgentsView().environment(Session(sample: true)).preferredColorScheme(.dark)
}
