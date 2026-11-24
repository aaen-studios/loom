import SwiftUI

/// The agents, each one ongoing chat, as the desktop's sidebar shows them: its thread,
/// its name, and what it's doing.
struct AgentsView: View {
    @Environment(Session.self) private var session
    @State private var path: [String] = []
    @State private var settings = false

    var body: some View {
        let model = session.model
        NavigationStack(path: $path) {
            List {
                ForEach(model.agents) { agent in
                    NavigationLink(value: agent.id) {
                        AgentRow(agent: agent)
                    }
                    .listRowBackground(P.bg)
                    .listRowSeparatorTint(P.line)
                    .listRowInsets(EdgeInsets(top: 0, leading: Tokens.PhoneSpace.gutter, bottom: 0, trailing: Tokens.Space.l))
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
                ChatView(agentId: id)
            }
            .toolbarBackground(P.bg, for: .navigationBar)
        }
        .sheet(isPresented: $settings) {
            SettingsView()
        }
        .background {
            Warmup(agent: model.synced && path.isEmpty ? model.agents.first?.id : nil)
        }
        .onAppear(perform: openFromLaunch)
    }

    /// `-open <agent>` and `-settings` at launch open that screen, for screenshots of each
    /// (ios/README.md).
    private func openFromLaunch() {
        let args = ProcessInfo.processInfo.arguments
        if let i = args.firstIndex(of: "-open"), i + 1 < args.count { path = [args[i + 1]] }
        if args.contains("-settings") { settings = true }
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
