import SwiftUI

// Spools on the phone (PLAN.md, "The integrations catalog"): a screen in Settings with
// Find, loom's own as wide cards, the category pills and a grid of tiles, and Installed,
// the person's servers, each pushing its detail. An entry opens its detail in a sheet,
// where it's connected step by step. The model (Spools.swift) says what each shows and
// what to send; desktop/src/spools.rs draws the same on the desktop.

extension StandingDot {
    var color: Color {
        switch self {
        case .ok: P.ok
        case .warn: P.warn
        case .bad: P.bad
        case .quiet: P.ink3
        }
    }
}

/// Spools, from Settings: Find and Installed.
struct SpoolsView: View {
    @Environment(Session.self) private var session
    @State private var installedTab = false
    @State private var query = ""
    @FocusState private var searching: Bool

    var body: some View {
        let model = session.model
        Group {
            if installedTab {
                installed
            } else {
                find
            }
        }
        .background(P.side)
        .safeAreaInset(edge: .top, spacing: 0) {
            Picker("Spools", selection: $installedTab) {
                Text("Find").tag(false)
                Text("Installed").tag(true)
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, Tokens.PhoneSpace.gutter)
            .padding(.vertical, Tokens.Space.s)
            .background(P.side)
        }
        .navigationTitle("Spools")
        .navigationBarTitleDisplayMode(.inline)
        .task(id: query) {
            // Once typing pauses.
            try? await Task.sleep(for: .milliseconds(350))
            guard !Task.isCancelled else { return }
            session.send(model.spoolSearch(query))
        }
        .onAppear {
            for f in model.openSpools() { session.send(f) }
            session.flush()
            // `-installed` at launch opens Installed, and `-spool <id>` that entry's
            // detail, for screenshots (ios/README.md).
            let args = ProcessInfo.processInfo.arguments
            if args.contains("-installed") { installedTab = true }
            if let i = args.firstIndex(of: "-spool"), i + 1 < args.count {
                let id = args[i + 1]
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { model.spoolOpen(id) }
            }
        }
        .sheet(item: Binding(get: { model.spools.flow }, set: { if $0 == nil { model.spoolClose() } })) { _ in
            SpoolDetailSheet()
        }
    }

    // MARK: Find

    private var find: some View {
        let model = session.model
        let shown = model.spoolsShown(query)
        return ScrollView {
            VStack(alignment: .leading, spacing: Tokens.Space.l) {
                search
                if !model.spools.came {
                    if let e = model.spools.error {
                        Text(e).font(Fonts.small).foregroundStyle(P.ink3)
                    } else {
                        ProgressView().frame(maxWidth: .infinity)
                    }
                } else {
                    ForEach(shown.cards) { e in
                        SpoolCard(entry: e)
                    }
                    pills
                    if shown.nothing {
                        Text("Nothing found").font(Fonts.small).foregroundStyle(P.ink3)
                    }
                    if !shown.tiles.isEmpty {
                        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: Tokens.Space.s), count: 3), spacing: Tokens.Space.s) {
                            ForEach(shown.tiles) { e in
                                SpoolTile(entry: e)
                            }
                        }
                    }
                    if let line = shown.line {
                        Text(line).font(Fonts.small).foregroundStyle(P.ink3)
                    }
                }
            }
            .padding(.horizontal, Tokens.PhoneSpace.gutter)
            .padding(.top, Tokens.Space.xs)
            .padding(.bottom, Tokens.Space.xxl)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    private var search: some View {
        HStack(spacing: Tokens.Space.s) {
            Image(systemName: "magnifyingglass").foregroundStyle(P.ink3)
            TextField("Search", text: $query)
                .font(Fonts.ui(Tokens.PhoneSize.small))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .focused($searching)
                .submitLabel(.search)
            if !query.isEmpty {
                Button {
                    query = ""
                } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(P.ink3)
                }
                .accessibilityLabel("Clear")
            }
        }
        .padding(.horizontal, Tokens.Space.m)
        .frame(height: 40)
        .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous).strokeBorder(P.line, lineWidth: 1))
    }

    /// All, then each category, the one picked filled.
    private var pills: some View {
        let model = session.model
        let picked = model.spools.pill
        return ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Tokens.Space.xs + 2) {
                pill("All", on: picked == nil) { model.spools.pill = nil }
                ForEach(pillsOf(model.spools.categories), id: \.self) { c in
                    pill(c, on: picked == c) { model.spools.pill = c }
                }
            }
        }
        .scrollClipDisabled()
    }

    private func pill(_ label: String, on: Bool, pick: @escaping () -> Void) -> some View {
        Button(action: pick) {
            Text(label)
                .font(Fonts.small)
                .foregroundStyle(on ? P.buttonInk : P.ink2)
                .padding(.horizontal, Tokens.Space.m)
                .frame(height: Tokens.Control.chip)
                .background(Capsule().fill(on ? P.button : Color.clear))
                .overlay(Capsule().strokeBorder(on ? Color.clear : P.line, lineWidth: 1))
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    // MARK: Installed

    @ViewBuilder
    private var installed: some View {
        let model = session.model
        let rows = model.installedRows
        if rows.isEmpty {
            VStack(spacing: Tokens.Space.m) {
                Text("Nothing connected yet").font(Fonts.body).foregroundStyle(P.ink3)
                Button("Find one") { installedTab = false }.buttonStyle(.loom)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            List {
                ForEach(rows) { r in
                    NavigationLink {
                        InstalledDetail(server: r.server.name)
                    } label: {
                        HStack(spacing: Tokens.Space.m) {
                            BadgeView(badge: r.server.badge ?? r.entry?.badge, name: r.title, size: Tokens.Control.lead)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(r.title).font(Fonts.body).foregroundStyle(P.ink).lineLimit(1)
                                if !r.line.isEmpty {
                                    Text(r.line).font(Fonts.caption).foregroundStyle(r.dot == .bad ? P.bad : P.ink3).lineLimit(1)
                                }
                            }
                            Spacer(minLength: 0)
                            Circle().fill(r.dot.color).frame(width: 7, height: 7)
                        }
                    }
                    .accessibilityLabel("\(r.title), \(r.line)")
                }
            }
            .scrollContentBackground(.hidden)
        }
    }
}

/// The colour one of loom's own takes: its badge's, softly.
private func spoolTint(_ e: Entry) -> Color {
    guard let c = e.badge?.color, !c.isEmpty else { return P.ink3 }
    return P.thread(c)
}

/// One of loom's own as a wide card in a soft tint of its colour: loom's knot tied to its
/// badge, slack until it's connected and taut once it is; its name and line; and Connect,
/// or how it stands with Manage, and Connect another for one there can be several of.
private struct SpoolCard: View {
    @Environment(Session.self) private var session
    var entry: Entry

    var body: some View {
        let model = session.model
        let status = model.entryStatus(entry)
        let tint = spoolTint(entry)
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            HStack(spacing: Tokens.Space.xs) {
                RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
                    .fill(P.bg)
                    .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
                    .overlay(Knot().frame(width: 20, height: 20))
                    .frame(width: Tokens.Control.iconLarge, height: Tokens.Control.iconLarge)
                SpanView(taut: status?.dot == .ok, tint: tint)
                    .frame(width: 56)
                BadgeView(badge: entry.badge, name: entry.name)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.name).font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink).lineLimit(1)
                Text(entry.description).font(Fonts.small).foregroundStyle(P.ink2).lineLimit(2)
            }
            HStack(spacing: Tokens.Space.s) {
                if let status {
                    HStack(spacing: Tokens.Space.xs + 2) {
                        if status.dot == .ok {
                            Image(systemName: "checkmark").font(.system(size: 11, weight: .bold))
                        } else {
                            Circle().fill(status.dot.color).frame(width: 6, height: 6)
                        }
                        Text(status.words).lineLimit(1)
                    }
                    .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                    .foregroundStyle(status.dot == .quiet ? P.ink2 : status.dot.color)
                }
                Spacer(minLength: 0)
                if entry.connected {
                    if entry.several && entry.connectsHere {
                        Button("Connect another") {
                            model.spoolOpen(entry.id)
                            session.send(model.spoolConnect(another: true))
                        }
                        .buttonStyle(CardButtonStyle())
                    }
                    Button("Manage") { model.spoolOpen(entry.id) }
                        .buttonStyle(CardButtonStyle())
                } else {
                    Button("Connect") {
                        model.spoolOpen(entry.id)
                        session.send(model.spoolConnect())
                    }
                    .buttonStyle(CardButtonStyle(primary: true))
                }
            }
            .frame(minHeight: Tokens.Control.buttonSmall)
        }
        .padding(Tokens.Space.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(shape.fill(P.cardFill).overlay(shape.fill(tint.opacity(0.07))))
        .overlay(shape.strokeBorder(tint.opacity(0.35), lineWidth: 1))
        .contentShape(shape)
        .onTapGesture { model.spoolOpen(entry.id) }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(entry.name)
    }
}

/// An entry as a tile in the grid: its badge, a tick once it's connected (or a dot in the
/// colour of how else it stands), and its name.
private struct SpoolTile: View {
    @Environment(Session.self) private var session
    var entry: Entry

    var body: some View {
        let model = session.model
        let status = model.entryStatus(entry)
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        Button {
            model.spoolOpen(entry.id)
        } label: {
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top, spacing: Tokens.Space.xs) {
                    BadgeView(badge: entry.badge, name: entry.name, size: 28)
                    Spacer(minLength: 0)
                    if let status {
                        if status.dot == .ok {
                            Image(systemName: "checkmark").font(.system(size: 10, weight: .bold)).foregroundStyle(P.ok)
                        } else {
                            Circle().fill(status.dot.color).frame(width: 6, height: 6).padding(.top, Tokens.Space.xs)
                        }
                    }
                }
                Spacer(minLength: Tokens.Space.xs)
                Text(entry.name)
                    .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                    .foregroundStyle(P.ink)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(Tokens.Space.m)
            .frame(height: Tokens.Control.tile)
            .background(P.cardFill, in: shape)
            .overlay(shape.strokeBorder(P.line, lineWidth: 1))
            .overlay(alignment: .topTrailing) {
                if entry.community && status == nil {
                    CommunityMark().padding(Tokens.Space.s)
                }
            }
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(entry.name)
    }
}

/// The mark of a server from the MCP Registry, not one of loom's catalog.
private struct CommunityMark: View {
    var body: some View {
        Text("Community")
            .font(Fonts.ui(Tokens.PhoneSize.caption - 2, .medium, relativeTo: .caption2))
            .foregroundStyle(P.ink2)
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .overlay(Capsule().strokeBorder(P.line, lineWidth: 1))
    }
}

/// An entry's detail, in a sheet: its badge, name and maker; where it runs and how it
/// signs in; its tools, each marked reads or changes; and connecting it, step by step, or
/// how its servers stand once it has some.
private struct SpoolDetailSheet: View {
    @Environment(Session.self) private var session
    @State private var fields: [String: String] = [:]
    @State private var secrets: [String: String] = [:]

    var body: some View {
        let model = session.model
        NavigationStack {
            ScrollView {
                if let flow = model.spools.flow, let e = model.entry(flow.entry) {
                    page(e, flow)
                }
            }
            .background(P.bg)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { model.spoolClose() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    private func page(_ e: Entry, _ flow: SpoolFlow) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.l) {
            HStack(spacing: Tokens.Space.m) {
                BadgeView(badge: e.badge, name: e.name)
                VStack(alignment: .leading, spacing: 2) {
                    Text(e.name).font(Fonts.ui(Tokens.PhoneSize.body + 2, .semibold)).foregroundStyle(P.ink).lineLimit(1)
                    HStack(spacing: Tokens.Space.s) {
                        Text("by \(e.maker)").font(Fonts.caption).foregroundStyle(P.ink3).lineLimit(1)
                        if e.community { CommunityMark() }
                    }
                }
            }
            if !e.description.isEmpty {
                Text(e.description).font(Fonts.small).foregroundStyle(P.ink2).fixedSize(horizontal: false, vertical: true)
            }
            // One loom can't connect runs nowhere it can say; its why, below, says so.
            if e.unsupported.isEmpty {
                CardLines(lines: [Shown.Line(label: "Runs", value: e.runsWords), Shown.Line(label: "Signs in", value: e.signInWords)])
            }
            if !e.tools.isEmpty { tools(e, flow) }
            VStack(alignment: .leading, spacing: Tokens.Space.m) {
                steps(e, flow)
            }
            .padding(.top, Tokens.Space.l)
            .frame(maxWidth: .infinity, alignment: .leading)
            .overlay(alignment: .top) { Rectangle().fill(P.line).frame(height: 1) }
        }
        .padding(.horizontal, Tokens.PhoneSpace.gutter)
        .padding(.top, Tokens.Space.s)
        .padding(.bottom, Tokens.Space.xl)
    }

    private func tools(_ e: Entry, _ flow: SpoolFlow) -> some View {
        let shown = toolsShown(e.tools, all: flow.allTools)
        return VStack(alignment: .leading, spacing: Tokens.Space.s) {
            HStack(alignment: .firstTextBaseline, spacing: Tokens.Space.s) {
                Text("Tools").font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink2)
                if !e.toolsChecked { Text("not checked").font(Fonts.caption).foregroundStyle(P.ink3) }
            }
            ForEach(shown, id: \.self) { i in
                let t = e.tools[i]
                HStack(spacing: Tokens.Space.s) {
                    Text(t.name).font(Fonts.mono(Tokens.PhoneSize.caption, relativeTo: .caption)).foregroundStyle(P.ink).lineLimit(1)
                    EffectPill(kind: t.changes ? "changes" : "reads", words: t.changes ? "changes" : "reads")
                }
            }
            if shown.count < e.tools.count {
                Button("Show all \(e.tools.count)") { session.model.spools.flow?.allTools = true }
                    .font(Fonts.small)
                    .foregroundStyle(P.ink2)
            }
        }
    }

    @ViewBuilder
    private func steps(_ e: Entry, _ flow: SpoolFlow) -> some View {
        let model = session.model
        switch flow.stage {
        case .about where e.connected:
            let servers = e.servers.compactMap { model.mcpServer($0) }
            ForEach(servers) { m in
                if servers.count > 1 {
                    Text(hostOf(m.url)).font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink)
                }
                ServerBlock(server: m)
            }
            if e.several && e.connectsHere {
                Button("Connect another") { session.send(model.spoolConnect(another: true)) }
                    .buttonStyle(CardButtonStyle())
            }
        case .about where !e.unsupported.isEmpty:
            quiet(e.unsupported)
        case .about where !e.connectsHere:
            // Its sign-in takes no app's way back, so it's connected from loom on a
            // computer.
            quiet("Connect it from loom on a computer")
        case .about:
            Button("Connect") { session.send(model.spoolConnect()) }
                .buttonStyle(.loom)
        case .form:
            ForEach(e.fields, id: \.name) { f in
                field(f.label) {
                    TextField(f.placeholder.isEmpty ? f.label : f.placeholder, text: binding($fields, f.name, f.defaultValue))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                }
            }
            ForEach(e.secrets, id: \.name) { k in
                field(k.label) {
                    if k.have {
                        Text("Saved").font(Fonts.small).foregroundStyle(P.ink3)
                    } else {
                        SecureField(k.placeholder.isEmpty ? k.label : k.placeholder, text: binding($secrets, k.name, ""))
                    }
                }
            }
            if !e.note.isEmpty { Text(e.note).font(Fonts.caption).foregroundStyle(P.ink3) }
            if !flow.said.isEmpty { said(flow.said) }
            HStack(spacing: Tokens.Space.s) {
                Button("Connect") { session.send(model.spoolSubmit(fields: fields, secrets: secrets)) }
                    .buttonStyle(CardButtonStyle(primary: true))
                Button("Cancel") { model.spoolBack() }
                    .buttonStyle(CardButtonStyle())
            }
        case .going(.adding):
            HStack(spacing: Tokens.Space.s) {
                ProgressView().controlSize(.small)
                quiet("Connecting…")
            }
        case .going(.signIn):
            if !flow.said.isEmpty { said(flow.said) }
            Button {
                session.signIn(flow.server)
            } label: {
                HStack(spacing: Tokens.Space.xs) {
                    Text("Sign in")
                    Image(systemName: "arrow.up.right").font(.system(size: 10, weight: .semibold))
                }
            }
            .buttonStyle(CardButtonStyle(primary: true))
        case .going(.agents):
            field("Agents") {
                Flow {
                    ForEach(model.agents) { a in
                        AgentChip(agent: a, on: flow.agents.contains(a.id)) { model.spoolPickAgent(a.id) }
                    }
                }
            }
            Button("Done") { session.send(model.spoolGive()) }
                .buttonStyle(CardButtonStyle(primary: true))
        case .going(.failed):
            said(flow.said.isEmpty ? "It didn't connect" : flow.said)
            Button("Try again") { model.spoolBack() }
                .buttonStyle(CardButtonStyle())
        case .going(.done):
            if let m = model.mcpServer(flow.server) {
                ServerBlock(server: m)
            } else {
                Label("Connected", systemImage: "checkmark")
                    .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                    .foregroundStyle(P.ok)
            }
        }
    }

    private func binding(_ list: Binding<[String: String]>, _ name: String, _ start: String) -> Binding<String> {
        Binding(get: { list.wrappedValue[name] ?? start }, set: { list.wrappedValue[name] = $0 })
    }

    private func field<C: View>(_ label: String, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: Tokens.Space.xs + 2) {
            Text(label).font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink2)
            content()
                .font(Fonts.small)
        }
    }

    private func said(_ text: String) -> some View {
        Text(text).font(Fonts.small).foregroundStyle(P.bad).fixedSize(horizontal: false, vertical: true)
    }

    private func quiet(_ text: String) -> some View {
        Text(text).font(Fonts.small).foregroundStyle(P.ink3)
    }
}

/// One of the person's servers, pushed from Installed: its badge and name, with its own
/// name and its host under it where they say more, then how it stands, its agents, and
/// what can be done with it.
private struct InstalledDetail: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var server: String

    var body: some View {
        let model = session.model
        let row = model.installedRows.first { $0.server.name == server }
        ScrollView {
            if let row {
                VStack(alignment: .leading, spacing: Tokens.Space.l) {
                    HStack(spacing: Tokens.Space.m) {
                        BadgeView(badge: row.server.badge ?? row.entry?.badge, name: row.title)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.title).font(Fonts.ui(Tokens.PhoneSize.body + 2, .semibold)).foregroundStyle(P.ink).lineLimit(1)
                            let under = installedUnder(row.server, row.entry)
                            if !under.isEmpty {
                                Text(under).font(Fonts.mono(Tokens.PhoneSize.caption, relativeTo: .caption)).foregroundStyle(P.ink3).lineLimit(1)
                            }
                        }
                    }
                    ServerBlock(server: row.server) { dismiss() }
                }
                .padding(.horizontal, Tokens.PhoneSpace.gutter)
                .padding(.vertical, Tokens.Space.l)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .background(P.side)
        .navigationTitle(row?.title ?? "")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// How one of the person's servers stands, with its dot; its agents, each a chip that
/// gives it to them or takes it away; and Sign in, Try it or Reconnect, Disconnect and
/// Remove, as it allows.
private struct ServerBlock: View {
    @Environment(Session.self) private var session
    var server: McpServer
    /// After Remove.
    var removed: () -> Void = {}

    var body: some View {
        let model = session.model
        let (words, dot) = model.serverStanding(server)
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            HStack(alignment: .firstTextBaseline, spacing: Tokens.Space.s) {
                Circle().fill(dot.color).frame(width: 6, height: 6).alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 4 }
                Text(words)
                    .font(Fonts.small)
                    .foregroundStyle(dot == .bad ? P.bad : P.ink)
                    .lineLimit(2)
            }
            if !model.agents.isEmpty {
                VStack(alignment: .leading, spacing: Tokens.Space.xs + 2) {
                    Text("Agents").font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink2)
                    Flow {
                        ForEach(model.agents) { a in
                            AgentChip(agent: a, on: server.agents.contains(a.id)) {
                                session.send(model.spoolToggleAgent(server: server.name, agent: a.id))
                            }
                        }
                    }
                }
            }
            Flow {
                ForEach(serverActs(server), id: \.label) { act in
                    Button {
                        press(act)
                    } label: {
                        if act == .remove {
                            Text(act.label).foregroundStyle(P.bad)
                        } else {
                            Text(act.label)
                        }
                    }
                    .buttonStyle(CardButtonStyle(primary: act == .signIn))
                }
            }
        }
    }

    private func press(_ act: ServerAct) {
        let model = session.model
        switch act {
        case .signIn: session.signIn(server.name)
        case .tryIt: session.send(model.spoolTry(server.name))
        case .reconnect: session.send(model.spoolReconnect(server.name))
        case .disconnect: session.send(model.spoolDisconnect(server.name))
        case .remove:
            session.send(model.spoolRemove(server.name))
            removed()
        }
    }
}

/// One of the person's agents, as a chip that's on or off: its thread's colour and its
/// name, filled while it's on.
private struct AgentChip: View {
    var agent: Agent
    var on: Bool
    var pick: () -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.chip, style: .continuous)
        Button(action: pick) {
            HStack(spacing: 6) {
                Circle().fill(P.thread(agent.color)).frame(width: 8, height: 8)
                Text(agent.name).lineLimit(1)
            }
            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
            .foregroundStyle(on ? P.ink : P.ink2)
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: Tokens.Control.chip)
            .background(shape.fill(on ? P.thread(agent.color).opacity(0.14) : Color.clear))
            .overlay(shape.strokeBorder(on ? P.thread(agent.color).opacity(0.6) : P.line, lineWidth: 1))
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(agent.name)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}
