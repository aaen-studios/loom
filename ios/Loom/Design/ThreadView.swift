import SwiftUI

/// An agent's thread: the only colour in the app, a line whose movement says what the
/// agent is doing (design/tokens.toml, [thread.motion.*]). The same drawing as the
/// desktop's desktop/src/thread.rs, its phase taken from the time.
struct ThreadView: View {
    var mood: Mood
    var color: Color
    /// When the agent last finished, for the one-time snap back to idle.
    var snapped: Date?
    var large = false

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 60, paused: !moves)) { context in
            Canvas { ctx, size in
                draw(&ctx, size, t: context.date.timeIntervalSinceReferenceDate, now: context.date)
            }
        }
        .accessibilityHidden(true)
    }

    private var moves: Bool {
        (mood != .idle && mood != .failed) || (snapped.map { Date.now.timeIntervalSince($0) < 1.2 } ?? false)
    }

    private func draw(_ ctx: inout GraphicsContext, _ size: CGSize, t: Double, now: Date) {
        let stroke = large ? Tokens.ThreadStroke.large : Tokens.ThreadStroke.small
        let width = size.width
        let mid = size.height / 2
        let half = size.height / 2
        let sinceSnap = snapped.map { now.timeIntervalSince($0) } ?? .infinity
        let steps = max(Int(width / 1.5), 8)
        let tau = Double.pi * 2

        func y(_ u: Double) -> Double {
            let x = u * width
            switch mood {
            case .idle:
                typealias M = Tokens.ThreadMotionIdle
                let base = sin(tau * (u * 1.3 + t * M.speed)) * M.amplitude * 2
                var snap = 0.0
                if sinceSnap < 1.2 {
                    typealias S = Tokens.ThreadMotionSnap
                    let a = S.amplitude * 2 * exp(-S.damping * sinceSnap)
                    snap = a * sin(tau * (S.frequency / 10) * sinceSnap) * sin(.pi * u)
                }
                return base + snap
            case .thinking:
                // A slow loose wave, with a loop travelling along it.
                typealias M = Tokens.ThreadMotionThinking
                let head = (t * M.loopSpeed).truncatingRemainder(dividingBy: 1)
                let d = min(abs(u - head), 1 - abs(u - head))
                let bump = pow(1 - min(d / M.loopWidth, 1), 2)
                return sin(tau * (u * 2 - t * 0.35)) * M.amplitude * 4 + bump * 0.55
            case .writing:
                typealias M = Tokens.ThreadMotionWriting
                return sin(tau * (x / M.wavelength - t * M.speed)) * M.amplitude * 2.4
            case .tool:
                // Short stitches sliding along.
                typealias M = Tokens.ThreadMotionTool
                var phase = (x / 8 - t * M.slideSpeed * 2).truncatingRemainder(dividingBy: 1)
                if phase < 0 { phase += 1 }
                return (phase < 0.5 ? 1 : -1) * M.amplitude * 1.4
            case .waiting:
                // Straight, with a knot in the middle that bobs, and a tug now and then.
                typealias M = Tokens.ThreadMotionWaiting
                let tug = t.truncatingRemainder(dividingBy: M.tugEvery) < 0.25
                let bob = sin(tau * t * M.bobSpeed / 4) * 0.25
                let d = abs(u - 0.5)
                let knot = d < M.knotWidth * 2 ? 1 - d / (M.knotWidth * 2) : 0
                return knot * (0.8 + bob) * (tug ? 1.3 : 1)
            case .failed:
                return 0
            }
        }

        var path = Path()
        let end = mood == .failed ? 0.62 : 1.0
        for i in 0...steps {
            let u = end * Double(i) / Double(steps)
            let p = CGPoint(x: u * width, y: mid - y(u) * (half - stroke))
            if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
        }
        if mood == .failed {
            // Frayed at the end.
            let fx = end * width
            for (dx, dy) in [(0.22, -0.8), (0.26, -0.2), (0.24, 0.35), (0.18, 0.9)] {
                path.move(to: CGPoint(x: fx, y: mid))
                path.addLine(to: CGPoint(x: fx + dx * width, y: mid + dy * (half - stroke)))
            }
        }
        if mood == .waiting {
            let r = max(half * 0.55, 2.5)
            path.addEllipse(in: CGRect(x: width / 2 - r, y: mid - r, width: r * 2, height: r * 2))
        }
        ctx.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: stroke, lineCap: .round, lineJoin: .round))
    }
}

#Preview("Threads") {
    VStack(alignment: .leading, spacing: 16) {
        ForEach([Mood.idle, .thinking, .writing, .tool, .waiting, .failed], id: \.self) { m in
            HStack {
                ThreadView(mood: m, color: P.thread("periwinkle")).frame(width: 40, height: 14)
                Text("\(String(describing: m))").font(Fonts.small)
            }
        }
    }
    .padding()
}
