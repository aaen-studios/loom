import SwiftUI

// Group chats on the phone (PLAN.md, "Group chats"), as the desktop draws them
// (desktop/src/room_view.rs): their rows in the agent list, each with its members'
// threads woven together, the line over each reply that says how it came to its agent,
// and the sheets that make one or change it.

/// A group chat's mark: its members' threads (the chat's own colour first, when it has
/// one), four at most, woven across one another.
struct RoomMark: View {
    var colors: [Color]

    var body: some View {
        Canvas { ctx, size in
            let n = max(colors.count, 1)
            let mid = size.height / 2
            let half = size.height / 2 - Tokens.ThreadStroke.small
            for (k, color) in colors.enumerated() {
                var path = Path()
                let steps = max(Int(size.width / 1.5), 12)
                let phase = Double(k) * .pi * 2 / Double(n)
                for i in 0...steps {
                    let u = Double(i) / Double(steps)
                    let p = CGPoint(x: u * size.width, y: mid - sin(u * .pi * 2 + phase) * half)
                    if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
                }
                ctx.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: Tokens.ThreadStroke.small, lineCap: .round))
            }
        }
        .accessibilityHidden(true)
    }
}

extension Model {
    /// The colours a group chat's mark weaves.
    func roomColors(_ r: Room) -> [Color] {
        var out: [Color] = r.color.isEmpty ? [] : [P.thread(r.color)]
        out += r.members.compactMap { agent($0) }.map { P.thread($0.color) }
        return Array(out.prefix(4))
    }
}

struct RoomRow: View {
    @Environment(Session.self) private var session
    var room: Room

    var body: some View {
        HStack(spacing: Tokens.Space.m + 2) {
            RoomMark(colors: session.model.roomColors(room))
                .frame(width: 26, height: 14)
                .frame(width: 26, height: 22)
            Text(room.name)
                .font(Fonts.ui(Tokens.PhoneSize.body, .medium))
                .foregroundStyle(P.ink)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.vertical, Tokens.Space.m + 2)
        .accessibilityElement(children: .combine)
    }
}

/// The line over a reply in a group chat: "→ Ledger · money question", in the agent's
/// colour. A tap offers the chat's other members, to send the person's message to
/// instead, which the router keeps as a correction.
struct RouteLine: View {
    @Environment(Session.self) private var session
    var message: Message
    var routed: Routed
    var agent: Agent?

    var body: some View {
        let model = session.model
        let others = model.resendTo(routed.agent)
        let line = HStack(spacing: 4) {
            Text("→").foregroundStyle(P.ink3)
            Text(agent?.name ?? "Agent")
                .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                .foregroundStyle(P.thread(agent?.color ?? ""))
            if !routed.said.isEmpty {
                Text("· \(routed.said)").foregroundStyle(P.ink3).lineLimit(1)
            }
        }
        .font(Fonts.caption)
        if message.live || others.isEmpty {
            line
        } else {
            Menu {
                Section("Send to") {
                    ForEach(others) { a in
                        Button(a.name) { session.send(model.resend(message.id, to: a.id)) }
                    }
                }
            } label: {
                line
            }
            .accessibilityLabel("Answered by \(agent?.name ?? "an agent"). Send to another agent")
        }
    }
}

/// Makes a group chat: its name, two or more members, and a colour if the person wants
/// one. The chat shows it once it's made. What it shows and whether Make is on are the
/// model's (`Model.newRoom`), with its form started when the sheet opens.
struct NewRoomSheet: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""

    var body: some View {
        let model = session.model
        let form = model.newRoom(name: name)
        NavigationStack {
            Form {
                Section("Name") {
                    TextField("Money", text: $name)
                }
                Section("Members") {
                    MemberToggles(members: form.members) { model.toggleFormMember($0) }
                }
                Section("Colour") {
                    ColorChoices(color: form.color) { model.setFormColor($0) }
                }
            }
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .navigationTitle("New group chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Make") {
                        session.makeRoom(name: name)
                        dismiss()
                    }
                    .disabled(!form.canMake)
                }
            }
        }
    }
}

/// A group chat's settings: its name, members (not Everyone's, which are all the
/// person's agents), who a message goes to when the router can't pick, its colour, the
/// corrections the router follows, each one swiped away to forget it, and what it follows.
struct RoomSettingsSheet: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var roomId: String
    @State private var name = ""

    var body: some View {
        let model = session.model
        NavigationStack {
            Form {
                if let r = model.roomNamed(roomId) {
                    Section("Name") {
                        TextField(r.name, text: $name)
                            .onSubmit { rename(r) }
                    }
                    if !r.everyone {
                        Section("Members") {
                            MemberToggles(members: r.members) { session.send(model.toggleMember($0, in: r)) }
                        }
                    }
                    Section("When it can't pick") {
                        Picker("Answers", selection: Binding(get: { r.defaultAgent }, set: { session.send(model.updateRoom(r.id, ["default": JSON($0)])) })) {
                            Text("Last to answer").tag("")
                            ForEach(r.members.compactMap { model.agent($0) }) { a in
                                Text(a.name).tag(a.id)
                            }
                        }
                    }
                    Section("Colour") {
                        ColorChoices(color: r.color) { session.send(model.updateRoom(r.id, ["color": JSON($0)])) }
                    }
                    Section("Corrections") {
                        if r.corrections.isEmpty {
                            Text("None yet").font(Fonts.small).foregroundStyle(P.ink3)
                        }
                        ForEach(r.corrections.reversed()) { c in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(c.text).font(Fonts.small).foregroundStyle(P.ink).lineLimit(2)
                                Text("→ \(model.agent(c.to)?.name ?? "An agent")").font(Fonts.caption).foregroundStyle(P.ink3)
                            }
                            .swipeActions {
                                Button("Forget", role: .destructive) { session.send(model.forget(c.id, in: r.id)) }
                            }
                        }
                    }
                    // What posts its lines here, only when there's some.
                    if !model.following(.room(r.id)).isEmpty {
                        Section("Following") {
                            FollowingRows(who: .room(r.id))
                        }
                    }
                    if let u = model.roomUsage, u.room == r.id {
                        Section("Usage") {
                            LabeledContent("Agents", value: usageLine(u.tokensToday, u.tokensMonth))
                            LabeledContent("Router", value: usageLine(u.routerToday, u.routerMonth))
                        }
                        .font(Fonts.small)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .navigationTitle(model.roomNamed(roomId)?.name ?? "Group chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        if let r = model.roomNamed(roomId) { rename(r) }
                        dismiss()
                    }
                }
            }
            .onAppear {
                name = model.roomNamed(roomId)?.name ?? ""
                session.send(model.askRoomUsage(roomId))
            }
        }
    }

    private func rename(_ r: Room) {
        let n = name.trimmingCharacters(in: .whitespaces)
        if !n.isEmpty && n != r.name { session.send(session.model.updateRoom(r.id, ["name": JSON(n)])) }
    }
}

/// Each of the person's agents, on or off.
private struct MemberToggles: View {
    @Environment(Session.self) private var session
    var members: [String]
    var toggle: (String) -> Void

    var body: some View {
        ForEach(session.model.agents) { a in
            Button {
                toggle(a.id)
            } label: {
                HStack(spacing: Tokens.Space.m) {
                    StillThread().stroke(P.thread(a.color), style: StrokeStyle(lineWidth: Tokens.ThreadStroke.small, lineCap: .round))
                        .frame(width: 18, height: 10)
                    Text(a.name).font(Fonts.body).foregroundStyle(P.ink)
                    Spacer()
                    if members.contains(a.id) {
                        Image(systemName: "checkmark").foregroundStyle(P.ink)
                    }
                }
            }
            .accessibilityAddTraits(members.contains(a.id) ? [.isSelected] : [])
        }
    }
}

/// loom's thread colours, and none.
private struct ColorChoices: View {
    var color: String
    var pick: (String) -> Void
    static let names = ["periwinkle", "lavender", "plum", "pink", "coral", "apricot", "gold", "moss", "mint", "teal", "sky", "grey"]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Tokens.Space.s) {
                Button {
                    pick("")
                } label: {
                    Circle().strokeBorder(P.line, lineWidth: 1).frame(width: 24, height: 24)
                        .overlay(Circle().strokeBorder(color.isEmpty ? P.ink2 : .clear, lineWidth: 2).padding(-4))
                }
                .accessibilityLabel("None")
                ForEach(Self.names, id: \.self) { name in
                    Button {
                        pick(name)
                    } label: {
                        Circle().fill(P.thread(name)).frame(width: 24, height: 24)
                            .overlay(Circle().strokeBorder(color == name ? P.ink2 : .clear, lineWidth: 2).padding(-4))
                    }
                    .accessibilityLabel(name.capitalized)
                }
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 4)
        }
    }
}
