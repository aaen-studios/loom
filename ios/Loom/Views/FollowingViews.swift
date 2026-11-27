import SwiftUI

/// Following (PLAN.md, "Watching outside events"), as the desktop's `following_list`
/// draws it: each watch as its pill, with a red dot and why in one line when it couldn't
/// look, and an × that takes it away. A swipe takes it away too.
struct FollowingRows: View {
    @Environment(Session.self) private var session
    var who: Follower

    var body: some View {
        let model = session.model
        ForEach(model.following(who)) { f in
            HStack(spacing: Tokens.Space.s) {
                FollowPillView(pill: f.pill)
                    .layoutPriority(1)
                // Why takes what the pill leaves.
                if let why = f.error {
                    Circle().fill(P.bad).frame(width: 7, height: 7)
                    Text(why)
                        .font(Fonts.caption).foregroundStyle(P.ink3)
                        .lineLimit(1).truncationMode(.tail)
                }
                Spacer(minLength: 0)
                Button {
                    session.send(model.stopFollowing(f.watch))
                } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(P.ink3)
                        .frame(width: Tokens.Control.icon, height: Tokens.Control.icon)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Stop following")
            }
            .swipeActions {
                Button("Stop following", role: .destructive) {
                    session.send(model.stopFollowing(f.watch))
                }
            }
        }
    }
}

/// What a watch in Following is on, as its pill: a card's, as in a message, a board's
/// tile and name, or T3's strand, quiet since a watch doesn't know how the thread is
/// going, and the thread's title.
struct FollowPillView: View {
    @Environment(Session.self) private var session
    var pill: FollowPill

    var body: some View {
        let model = session.model
        switch pill {
        case .card(let id, _):
            if let card = model.followedCard(pill) {
                CardPillView(card: card, shrinks: true)
            } else {
                PillFrame {
                    Pill.mark()
                    Text(id).font(Fonts.mono(Tokens.PhoneSize.small - 1)).foregroundStyle(P.ink2)
                }
            }
        case .board(let board):
            PillFrame {
                Pill.mark()
                words(model.boardName(board))
            }
        case .thread(let title):
            PillFrame {
                Capsule().fill(P.ink3).frame(width: 14, height: Tokens.ThreadStroke.small)
                words(title)
            }
        }
    }

    /// A board's name or a thread's title, cut at what the row leaves it.
    private func words(_ s: String) -> some View {
        Text(s)
            .font(Fonts.ui(Tokens.PhoneSize.small)).foregroundStyle(P.ink)
            .lineLimit(1).truncationMode(.tail)
    }
}

/// An agent's Following, from its chat's menu: the phone has no agent settings, so it's a
/// sheet of its own, which goes once the last is taken away.
struct FollowingSheet: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    var agentId: String

    var body: some View {
        let model = session.model
        NavigationStack {
            List {
                FollowingRows(who: .agent(agentId))
            }
            .scrollContentBackground(.hidden)
            .background(P.bg)
            .navigationTitle("Following")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .onChange(of: model.following(.agent(agentId)).isEmpty) { _, empty in
            if empty { dismiss() }
        }
    }
}
