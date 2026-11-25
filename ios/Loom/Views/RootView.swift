import SwiftUI

/// Signing in, or the agents and their chats, as the core's connection says.
struct RootView: View {
    @Environment(Session.self) private var session

    var body: some View {
        let model = session.model
        Group {
            switch model.conn {
            case .signedOut:
                SignInView()
            case .starting where model.agents.isEmpty:
                Starting()
            default:
                AgentsView()
            }
        }
        .tint(P.ink)
        .animation(.easeOut(duration: 0.25), value: model.conn == .signedOut)
        .overlay(alignment: .top) {
            if let notice = model.notice {
                Text(notice)
                    .font(Fonts.small)
                    .foregroundStyle(P.buttonInk)
                    .padding(.horizontal, Tokens.Space.l)
                    .padding(.vertical, Tokens.Space.s + 2)
                    .background(P.button, in: Capsule())
                    .padding(.horizontal, Tokens.PhoneSpace.gutter)
                    .padding(.top, Tokens.Space.s)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .onTapGesture { model.notice = nil }
                    .task(id: notice) {
                        try? await Task.sleep(for: .seconds(4))
                        model.notice = nil
                    }
            }
        }
        .animation(.easeOut(duration: 0.2), value: model.notice)
        .alert(askTitle(model), isPresented: asking(model), presenting: model.openAsks.first) { ask in
            Button("Not me", role: .cancel) { session.send(model.answerAsk(ask.id, allow: false)) }
            Button("Add") { session.send(model.answerAsk(ask.id, allow: true)) }
        } message: { ask in
            Text("\(ask.check.map(String.init).joined(separator: " "))\nOnly if the new device shows the same")
        }
    }

    private func askTitle(_ model: Model) -> String {
        model.openAsks.first.map { "Add \u{201c}\($0.name)\u{201d}?" } ?? ""
    }

    /// Whether a device is asking to be added. Its buttons answer it, which takes it away.
    private func asking(_ model: Model) -> Binding<Bool> {
        Binding(get: { !model.openAsks.isEmpty }, set: { _ in })
    }
}

/// The moment between launch and the core saying where it is.
private struct Starting: View {
    var body: some View {
        VStack(spacing: Tokens.Space.l) {
            ThreadView(mood: .thinking, color: P.thread("periwinkle"), large: true)
                .frame(width: 96, height: 16)
            Text("loom").font(Fonts.heading).foregroundStyle(P.ink)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(P.bg)
    }
}

/// A line under the title while the connection isn't live.
struct ConnLine: View {
    var conn: Conn

    var body: some View {
        if let words {
            Text(words)
                .font(Fonts.caption)
                .foregroundStyle(conn == .updateNeeded ? P.bad : P.ink3)
                .transition(.opacity)
        }
    }

    private var words: String? {
        switch conn {
        case .live, .signedOut: return nil
        case .starting, .connecting: return "Connecting…"
        case .offline(let ms): return ms > 2000 ? "Offline. Trying again in \(ms / 1000) s" : "Offline. Trying again…"
        case .updateNeeded: return "This app and the instance need the same version. Update one of them."
        }
    }
}

#Preview("Light") {
    RootView().environment(Session(sample: true))
}

#Preview("Dark") {
    RootView().environment(Session(sample: true)).preferredColorScheme(.dark)
}
