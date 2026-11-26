import SwiftUI

/// How full the agent's session context is, as a ring in the chat's header: the line all
/// round, filled clockwise from the top, warm from 70% and red from 90% (PLAN.md, "One
/// endless chat"). A tap opens its card as a sheet.
struct ContextRing: View {
    let fill: ContextFill

    var body: some View {
        ZStack {
            Circle().stroke(P.lineStrong, lineWidth: ContextRing.line)
            Circle()
                .trim(from: 0, to: fill.fraction)
                .stroke(ContextRing.ink(fill), style: StrokeStyle(lineWidth: ContextRing.line, lineCap: .butt))
                .rotationEffect(.degrees(-90))
        }
        .padding(ContextRing.line / 2)
        .frame(width: Tokens.Control.contextRing, height: Tokens.Control.contextRing)
        .frame(width: Tokens.Control.contextButton, height: Tokens.Control.contextButton)
        .contentShape(Rectangle())
        .animation(.easeOut(duration: 0.6), value: fill.fraction)
    }

    /// As the desktop draws it: an eighth of its size, and never under 1.5.
    static let line = max(Tokens.Control.contextRing / 8, 1.5)

    static func ink(_ fill: ContextFill) -> Color {
        switch fill.level {
        case .quiet: P.ink3
        case .warn: P.warn
        case .bad: P.bad
        }
    }
}

/// The ring's card, as a sheet: how many tokens the session holds of its window, as a bar,
/// when it was last compacted and where a fresh session starts, with Compact now (where
/// its program compacts when asked) and Start a fresh session.
struct ContextSheet: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let model = session.model
        VStack(alignment: .leading, spacing: Tokens.Space.m) {
            if let fill = model.context {
                HStack(alignment: .firstTextBaseline) {
                    Text(model.contextTitle(fill))
                        .font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                        .foregroundStyle(P.ink)
                        .lineLimit(1)
                    Spacer(minLength: Tokens.Space.s)
                    Text(fill.amount)
                        .font(Fonts.mono(Tokens.PhoneSize.small, relativeTo: .subheadline))
                        .foregroundStyle(P.ink2)
                }
                GeometryReader { g in
                    ZStack(alignment: .leading) {
                        Capsule().fill(P.lineStrong)
                        Capsule().fill(ContextRing.ink(fill)).frame(width: g.size.width * fill.fraction)
                    }
                }
                .frame(height: 6)
                TimelineView(.periodic(from: .now, by: 60)) { t in
                    Text(fill.line(now: Int(t.date.timeIntervalSince1970 * 1000)))
                        .font(Fonts.caption)
                        .foregroundStyle(P.ink3)
                }
                HStack(spacing: Tokens.Space.s) {
                    if fill.compacts {
                        Button {
                            if let f = model.compactNow() { session.send(f) }
                            dismiss()
                        } label: {
                            Text("Compact now").lineLimit(1).minimumScaleFactor(0.85).frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.loomQuiet)
                    }
                    Button {
                        if let f = model.freshSession() { session.send(f) }
                        dismiss()
                    } label: {
                        Text("Start a fresh session").lineLimit(1).minimumScaleFactor(0.85).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.loomQuiet)
                }
                .padding(.top, Tokens.Space.xs)
            }
        }
        .padding(.horizontal, Tokens.PhoneSpace.gutter)
        .padding(.top, Tokens.Space.xl)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(P.bg)
        .presentationDetents([.height(220)])
        .presentationDragIndicator(.visible)
        // Its chat's context went, as when another chat opened: so does its card.
        .onChange(of: model.context == nil) { _, gone in
            if gone { dismiss() }
        }
    }
}
