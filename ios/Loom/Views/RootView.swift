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
