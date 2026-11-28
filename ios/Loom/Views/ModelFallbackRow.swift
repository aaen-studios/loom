import SwiftUI

/// Loom's card under a reply whose chat went to another model (ModelFallback.swift), as
/// the desktop's `model_fallback_view.rs` draws it: its title, and Switch back and New
/// chat under it. Once one is used, or the person picks a model, one quiet line saying
/// what happened.
struct ModelFallbackRow: View {
    @Environment(Session.self) private var session
    var card: Card

    var body: some View {
        switch session.model.fallbackShown(card) {
        case let .done(said):
            CardDone(icon: "arrow.left.arrow.right", how: said)
        case let .on(title, from, going, error):
            let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
            let conversation = session.model.conversation ?? ""
            VStack(alignment: .leading, spacing: Tokens.Space.m) {
                HStack(spacing: Tokens.Space.s) {
                    Image(systemName: "arrow.left.arrow.right")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(P.warn)
                    Text(title)
                        .font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                        .foregroundStyle(P.ink)
                        .fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: Tokens.Space.s) {
                    Button(going == .back ? "Switching back…" : "Switch back to \(from)") {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        session.send(session.model.switchModelBack(conversation))
                    }
                    .buttonStyle(CardButtonStyle(primary: true))
                    Button(going == .newChat ? "Starting a new chat…" : "New chat on \(from)") {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        session.send(session.model.newChatFrom(conversation, agent: card.agent))
                    }
                    .buttonStyle(CardButtonStyle(primary: false))
                }
                .disabled(going != nil)
                if !error.isEmpty {
                    Text(error).font(Fonts.small).foregroundStyle(P.bad)
                }
            }
            .padding(.horizontal, Tokens.Space.l)
            .padding(.vertical, Tokens.Space.m)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(P.cardFill, in: shape)
            .overlay(shape.strokeBorder(P.cardEdge, lineWidth: 1))
            .shadow(color: .black.opacity(0.07), radius: 14, y: 8)
            .accessibilityElement(children: .contain)
        }
    }
}
