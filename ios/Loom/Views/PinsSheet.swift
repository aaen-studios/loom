import SwiftUI

extension EnvironmentValues {
    /// Drawn in the Pinned sheet, where pictures and files are smaller than in the chat.
    @Entry var inPin = false
}

/// How a pin's long words are cut: at about four lines, under a fade, with Show more
/// under them only when they run past that. `open` holds the ones shown whole.
struct PinCut {
    var open: Set<String>
    var toggle: (String) -> Void
}

extension View {
    /// Cuts long words in a pin (`PinCut`); in the chat, leaves them as they are.
    @ViewBuilder
    func pinCut(_ cut: PinCut?, key: String) -> some View {
        if let cut {
            modifier(PinCutting(open: cut.open.contains(key)) { cut.toggle(key) })
        } else {
            self
        }
    }
}

private struct PinCutting: ViewModifier {
    var open: Bool
    var toggle: () -> Void
    /// About four lines, as the desktop's 6.2em.
    @ScaledMetric(relativeTo: .body) private var most: CGFloat = 6.2 * Tokens.PhoneSize.body
    @State private var tall: CGFloat = 0

    func body(content: Content) -> some View {
        let cut = tall > most + 2
        VStack(alignment: .leading, spacing: Tokens.Space.xs) {
            content
                .fixedSize(horizontal: false, vertical: true)
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { tall = $0 }
                .frame(maxHeight: cut && !open ? most : nil, alignment: .top)
                .clipped()
                .mask {
                    LinearGradient(stops: [.init(color: .black, location: 0), .init(color: .black, location: cut && !open ? 0.62 : 1), .init(color: .black.opacity(cut && !open ? 0 : 1), location: 1)], startPoint: .top, endPoint: .bottom)
                }
            if cut {
                Button(open ? "Show less" : "Show more", action: toggle)
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .medium, relativeTo: .caption))
                    .foregroundStyle(P.ink2)
                    .buttonStyle(.plain)
            }
        }
    }
}

/// The chat's pins, newest pinned first, a page at a time, from the header's pin: each
/// drawn as the chat draws its message, under who said it and when, along a line in
/// their colour. A long press gives Jump to message, Copy and Unpin, which Undo takes
/// back for a few seconds (PLAN.md, "Pinned messages").
struct PinsSheet: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var agent: Agent?
    @State private var open: Set<String> = []
    @State private var note: Note?

    /// What the chip at the foot says, each time its own, so a second Unpin starts the
    /// time over.
    private struct Note: Equatable {
        var unpinned: Bool
        var id = UUID()
    }

    var body: some View {
        let model = session.model
        NavigationStack {
            Group {
                // Not while the pins it has are still on their way.
                if model.pins.isEmpty && model.pinsCount == 0 {
                    VStack(spacing: Tokens.Space.xs) {
                        Text("Nothing pinned yet").font(Fonts.ui(Tokens.PhoneSize.body, .medium)).foregroundStyle(P.ink)
                        Text("Long-press a message and pick Pin").font(Fonts.small).foregroundStyle(P.ink3)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: Tokens.Space.l) {
                            ForEach(model.pins) { p in
                                PinRow(pin: p, chatAgent: agent, cut: PinCut(open: open) { key in
                                    if open.contains(key) { open.remove(key) } else { open.insert(key) }
                                })
                                .contextMenu { menu(p) }
                                .onAppear {
                                    if p.id == model.pins.last?.id { session.send(model.loadMorePins()) }
                                }
                            }
                        }
                        .padding(.horizontal, Tokens.PhoneSpace.gutter)
                        .padding(.top, Tokens.Space.s)
                        .padding(.bottom, Tokens.Space.xl * 3)
                    }
                }
            }
            .background(P.bg)
            .overlay(alignment: .bottom) {
                if let note { chip(note) }
            }
            .animation(.easeOut(duration: 0.2), value: note)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    HStack(spacing: 6) {
                        Text("Pinned").font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
                        if model.pinsCount > 0 {
                            Text("\(model.pinsCount)").font(Fonts.ui(Tokens.PhoneSize.body, .regular)).monospacedDigit().foregroundStyle(P.ink3)
                        }
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        dismiss()
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .accessibilityLabel("Close")
                }
            }
            .toolbarBackground(P.bg, for: .navigationBar)
        }
        .environment(\.inPin, true)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .task(id: note) {
            guard let shown = note else { return }
            try? await Task.sleep(for: .seconds(shown.unpinned ? 6 : 1.6))
            guard !Task.isCancelled, note == shown else { return }
            if shown.unpinned { session.model.forgetUnpinned() }
            note = nil
        }
    }

    @ViewBuilder
    private func menu(_ p: Pin) -> some View {
        let model = session.model
        Button {
            dismiss()
            model.jumpTo = p.id
        } label: {
            Label("Jump to message", systemImage: "arrow.turn.down.right")
        }
        if !p.message.text.isEmpty {
            Button {
                UIPasteboard.general.string = Emoji.asNames(p.message.text)
                note = Note(unpinned: false)
            } label: {
                Label("Copy", systemImage: "doc.on.doc")
            }
        }
        Button(role: .destructive) {
            session.send(model.unpin(p.id))
            note = Note(unpinned: true)
        } label: {
            Label("Unpin", systemImage: "pin.slash")
        }
    }

    private func chip(_ note: Note) -> some View {
        HStack(spacing: Tokens.Space.m) {
            Text(note.unpinned ? "Unpinned" : "Copied").font(Fonts.small).foregroundStyle(P.ink)
            if note.unpinned {
                Button("Undo") {
                    session.send(session.model.undoUnpin())
                    self.note = nil
                }
                .font(Fonts.ui(Tokens.PhoneSize.small, .semibold))
                .foregroundStyle(P.ink)
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, Tokens.Space.l)
        .padding(.vertical, Tokens.Space.s + 2)
        .background(P.bubble, in: Capsule())
        .overlay(Capsule().strokeBorder(P.line, lineWidth: 1))
        .shadow(color: .black.opacity(0.12), radius: 8, y: 3)
        .padding(.bottom, Tokens.Space.l)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}

/// One pin: who said it, in their colour, and when, over the message as the chat draws it,
/// along a line of that colour down its left.
private struct PinRow: View {
    @Environment(Session.self) private var session
    var pin: Pin
    var chatAgent: Agent?
    var cut: PinCut

    /// Whose reply it is: in a group chat, the member it went to.
    private var agent: Agent? {
        pin.message.routed.flatMap { session.model.agent($0.agent) } ?? chatAgent
    }

    var body: some View {
        let m = pin.message
        let (who, color, mark) = author(m)
        HStack(alignment: .top, spacing: Tokens.Space.m) {
            RoundedRectangle(cornerRadius: 1).fill(color.opacity(0.75)).frame(width: 2)
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                HStack(spacing: Tokens.Space.s) {
                    if mark {
                        StillThread()
                            .stroke(color, style: StrokeStyle(lineWidth: Tokens.ThreadStroke.small, lineCap: .round, lineJoin: .round))
                            .frame(width: 22, height: 12)
                    }
                    Text(who).font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink)
                    Text(pinWhen(m.at)).font(Fonts.caption).foregroundStyle(P.ink3)
                }
                if m.isPerson {
                    PersonMessage(message: m, agent: agent, pin: cut)
                } else {
                    Reply(message: m, agent: agent, openThoughts: .constant([]), pin: cut)
                }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
        .padding(.vertical, Tokens.Space.xs)
        .contentShape(Rectangle())
    }

    /// Who said it, their colour, and whether their thread goes by their name: the agent,
    /// another agent that sent it, or the person, "You", with no thread.
    private func author(_ m: Message) -> (String, Color, Bool) {
        if !m.isPerson { return (agent?.name ?? "Agent", P.thread(agent?.color ?? ""), true) }
        if let id = m.fromAgent {
            guard let a = session.model.agent(id) else { return (m.from ?? "An agent", P.ink3, true) }
            return (a.name, P.thread(a.color), true)
        }
        if let from = m.from { return (from, P.ink3, false) }
        return ("You", P.ink3, false)
    }
}

/// When a pin's message was said: "Today 12:57", "Yesterday 18:04", "16 Nov".
func pinWhen(_ ms: Int) -> String {
    guard ms > 0, ms != .max else { return "" }
    let date = Date(timeIntervalSince1970: Double(ms) / 1000)
    let time = date.formatted(date: .omitted, time: .shortened)
    if Calendar.current.isDateInToday(date) { return "Today " + time }
    if Calendar.current.isDateInYesterday(date) { return "Yesterday " + time }
    return date.formatted(.dateTime.day().month(.abbreviated))
}
