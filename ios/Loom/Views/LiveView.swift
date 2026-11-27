import SwiftUI

// Live mode on the phone (PLAN.md, "Live mode"), in place of the chat: the agent's name,
// how it stands and how long it has run; one thread held at both ends across the screen,
// waving in ink as the agent speaks and lightly in grey as the person talks; the words
// being said under it; and Text, Mute and End. The model (Live.swift) reads `core.live`.

/// Live mode, over the whole chat.
struct LiveView: View {
    @Environment(Session.self) private var session

    var body: some View {
        let live = session.model.liveMode
        GeometryReader { g in
            VStack(spacing: 0) {
                header(live)
                    .padding(.top, Tokens.Space.m)
                TimelineView(.animation) { tl in
                    VStack(spacing: Tokens.Space.xl) {
                        LiveThread(live: live, at: tl.date, height: min(g.size.height * 0.18, 70))
                            .frame(maxHeight: .infinity)
                        words(live, at: tl.date)
                            .frame(height: 132, alignment: .top)
                    }
                }
                buttons(live)
                    .padding(.bottom, Tokens.Space.xl)
            }
        }
        .background(P.bg.ignoresSafeArea())
        .ignoresSafeArea(.keyboard)
    }

    /// The agent's name, its state in quiet ink, and the time it has run.
    private func header(_ live: LiveState) -> some View {
        VStack(spacing: 2) {
            Text(live.name).font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink)
            TimelineView(.periodic(from: live.since, by: 1)) { tl in
                Text("\(live.stateWords) · \(liveClock(Int(tl.date.timeIntervalSince(live.since))))")
                    .font(Fonts.ui(Tokens.PhoneSize.caption))
                    .foregroundStyle(P.ink3)
                    .monospacedDigit()
            }
        }
        .frame(maxWidth: .infinity)
        .accessibilityElement(children: .combine)
    }

    /// What's being said, centred: while a piece plays, the words said so far in ink and
    /// the rest faint.
    private func words(_ live: LiveState, at t: Date) -> some View {
        let (said, coming) = live.caption(t)
        return (Text(said).foregroundStyle(P.ink) + Text(coming).foregroundStyle(P.ink3.opacity(0.55)))
            .font(Fonts.ui(15))
            .multilineTextAlignment(.center)
            .lineSpacing(3)
            .lineLimit(5)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Tokens.Space.xxl)
            .opacity(live.captions ? 1 : 0)
            .animation(.easeOut(duration: 0.2), value: live.captions)
            .accessibilityHidden(!live.captions)
    }

    /// Text, Mute and End.
    private func buttons(_ live: LiveState) -> some View {
        HStack(spacing: Tokens.Space.xxl) {
            LiveButton(symbol: "text.alignleft", label: "Text", on: live.captions) {
                session.model.liveText()
            }
            LiveButton(symbol: "mic.slash", label: "Mute", on: live.muted) {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                session.muteLive()
            }
            LiveButton(symbol: "xmark", label: "End", on: false, end: true) {
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                session.endLive()
            }
        }
    }
}

/// One of live mode's round buttons: outlined, in ink while it's on, or End, filled in ink.
private struct LiveButton: View {
    var symbol: String
    var label: String
    var on: Bool
    var end = false
    var act: () -> Void

    var body: some View {
        Button(action: act) {
            Image(systemName: symbol)
                .font(.system(size: 20, weight: .medium))
                .foregroundStyle(end ? P.buttonInk : P.ink)
                .frame(width: 60, height: 60)
                .background(end ? P.button : (on ? P.active : P.raised), in: Circle())
                .overlay {
                    if !end {
                        Circle().strokeBorder(on ? P.ink : P.line, lineWidth: on ? 1.5 : 1)
                    }
                }
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// The thread, held at both ends across the view: it reaches as far as the model says at
/// each moment, rising fast and settling slower, in ink while the agent speaks or thinks
/// and in grey while the person talks.
private struct LiveThread: View {
    var live: LiveState
    var at: Date
    /// How far it reaches at its fullest.
    var height: CGFloat
    @State private var wave = Wave()

    /// What it was last drawn as, kept between frames.
    private final class Wave {
        var reach = 0.0
        var ink = 1.0
        var last: Date?
    }

    var body: some View {
        Canvas { ctx, size in
            let w = wave
            let dt = min(max(w.last.map { at.timeIntervalSince($0) } ?? 0, 0), 0.1)
            w.last = at
            let target = live.target(at)
            let tau = target > w.reach ? 0.08 : 0.25
            w.reach += (target - w.reach) * (1 - exp(-dt / tau))
            let inkTarget = live.state == "listening" ? 0.0 : 1.0
            w.ink += (inkTarget - w.ink) * (1 - exp(-dt / 0.25))

            let inset = Tokens.Space.xl
            let span = size.width - 2 * inset
            let mid = size.height / 2
            let t = at.timeIntervalSinceReferenceDate
            let a = w.reach * Double(height)
            var path = Path()
            let steps = 120
            for i in 0...steps {
                let u = Double(i) / Double(steps)
                let y = a * sin(.pi * u) * (0.6 * sin(2 * .pi * (1.5 * u - 0.9 * t)) + 0.4 * sin(2 * .pi * (2.5 * u + 0.6 * t)))
                let p = CGPoint(x: inset + span * u, y: mid - y)
                if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
            }
            ctx.stroke(path, with: .color(mixed(P.ink3, P.ink, w.ink, in: ctx.environment)), style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round))
        }
        .accessibilityHidden(true)
    }

    /// `a` towards `b` by `k`, from 0 to 1.
    private func mixed(_ a: Color, _ b: Color, _ k: Double, in env: EnvironmentValues) -> Color {
        let x = a.resolve(in: env)
        let y = b.resolve(in: env)
        let f = Float(min(max(k, 0), 1))
        return Color(Color.Resolved(red: x.red + (y.red - x.red) * f, green: x.green + (y.green - x.green) * f,
                                    blue: x.blue + (y.blue - x.blue) * f, opacity: x.opacity + (y.opacity - x.opacity) * f))
    }
}
