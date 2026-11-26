import SwiftUI

/// The card a routine leaves in the chat when it turns off on its own (RoutineOff.swift),
/// as the desktop's `routine_off_view.rs` draws it: one compact card, its name and "is
/// off", why under it, and Turn on at its end. Once it's on again, or deleted, one quiet
/// line, as an answered card's.
struct RoutineOffRow: View {
    @Environment(Session.self) private var session
    var card: Card

    var body: some View {
        switch session.model.routineOffShown(card) {
        case .on:
            CardDone(icon: "checkmark", how: "On again", what: card.title)
        case .deleted:
            CardDone(icon: "xmark", how: "Deleted", what: card.title)
        case let .off(why, sending, error):
            let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                HStack(spacing: Tokens.Space.m) {
                    VStack(alignment: .leading, spacing: 2) {
                        (Text(card.title).fontWeight(.semibold).foregroundStyle(P.ink) + Text(" is off").foregroundStyle(P.ink3))
                            .font(Fonts.ui(Tokens.PhoneSize.body))
                            .lineLimit(1)
                        Text(why)
                            .font(Fonts.small)
                            .foregroundStyle(P.ink3)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    Button(sending ? "Turning on…" : "Turn on") {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        session.send(session.model.turnRoutineOn(card.routine))
                    }
                    .buttonStyle(CardButtonStyle(primary: true))
                    .disabled(sending)
                }
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
            .accessibilityElement(children: .combine)
        }
    }
}
