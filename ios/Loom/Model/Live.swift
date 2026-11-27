import Foundation

// Live mode on the phone (PLAN.md, "Live mode"): talking with an agent, in place of its
// chat. The core hears the turns, sends them, has the reply spoken and says how it stands
// (`core.live`, core/live.calx); the phone records the mic, plays each piece it's told to,
// and draws the thread, the words and the buttons from what it said.

/// A piece of the reply playing, as `core.live`'s `play` gives it: its file, its words and
/// when each starts (ms into it), and its level each `step` ms, from 0 to 1.
struct LivePiece: Equatable, Sendable {
    var file: String
    var words: [String]
    var at: [Int]
    var levels: [Double]
    var step: Int
    var ms: Int
    /// When it started playing: as it came, since the app plays it at once.
    var began: Date

    init(_ p: JSON, began: Date) {
        file = p["file"].text
        words = p["words"].array.map(\.text)
        at = p["at"].array.compactMap(\.int)
        levels = p["levels"].array.compactMap(\.double)
        step = max(p["step"].int ?? 20, 1)
        ms = p["ms"].int ?? (levels.count * (p["step"].int ?? 20))
        self.began = began
    }

    /// How long it has played at `t`, in ms.
    func elapsed(_ t: Date) -> Int {
        max(Int(t.timeIntervalSince(began) * 1000), 0)
    }

    /// Whether it's still playing at `t`.
    func playing(_ t: Date) -> Bool {
        elapsed(t) < ms
    }

    /// How many of its words have started at `t`.
    func spoken(_ t: Date) -> Int {
        let e = elapsed(t)
        var n = 0
        for (i, _) in words.enumerated() where i < at.count && at[i] <= e { n = i + 1 }
        return n
    }

    /// Its level at `t`.
    func level(_ t: Date) -> Double {
        guard !levels.isEmpty else { return 0 }
        return levels[min(elapsed(t) / step, levels.count - 1)]
    }
}

/// Live mode, as the core last said it.
struct LiveState: Equatable {
    var on = false
    /// `listening`, `thinking` or `speaking`.
    var state = ""
    var agent = ""
    var name = ""
    var conversation = ""
    /// When it began, unix ms, as the core gives it.
    var started = 0
    /// When this phone first heard of it, which the time it has run counts from.
    var since = Date.distantPast
    var muted = false
    /// Whether the words show, from Show what's said, which the Text button changes here.
    var captions = true
    /// What shows under the thread while nothing plays.
    var words = ""
    /// The mic's level, from 0 to 1, while it listens.
    var level = 0.0
    var playing: LivePiece?

    /// The state in words, as the header says it.
    var stateWords: String {
        switch state {
        case "speaking": "Speaking"
        case "thinking": "Thinking"
        default: "Listening"
        }
    }

    /// How far the thread reaches at `t`, from 0 to 1: the level of what's playing while
    /// the agent speaks, half the mic's while it listens, and nearly still otherwise.
    func target(_ t: Date) -> Double {
        let still = 0.03
        switch state {
        case "speaking":
            guard let p = playing, p.playing(t) else { return still }
            return max(p.level(t), still)
        case "listening":
            return muted ? still : max(level * 0.5, still)
        default:
            return still
        }
    }

    /// The words under the thread at `t`: while a piece plays, its words, those said and
    /// those still to come; otherwise the words the core gave, all said.
    func caption(_ t: Date) -> (said: String, coming: String) {
        if let p = playing, p.playing(t) {
            let n = p.spoken(t)
            let said = p.words.prefix(n).joined(separator: " ")
            let coming = p.words.dropFirst(n).joined(separator: " ")
            return (said, said.isEmpty || coming.isEmpty ? coming : " " + coming)
        }
        return (words, "")
    }
}

/// The time live mode has run, as m:ss.
func liveClock(_ seconds: Int) -> String {
    let s = max(seconds, 0)
    return String(format: "%d:%02d", s / 60, s % 60)
}

/// A pause as the Send when I stop talking row says it.
func pauseWords(_ p: Double) -> String {
    String(format: "After %g s", p)
}

extension Model {
    /// Live in the composer: live mode with the chat open's agent.
    func liveStart() -> JSON? {
        guard let agent = selected, let conversation else { return nil }
        return ["t": "live.start", "agent": JSON(agent), "conversation": JSON(conversation)]
    }

    /// Mute, or unmute.
    func liveMute() -> JSON {
        ["t": "live.mute", "on": JSON(!liveMode.muted)]
    }

    /// End: the chat comes back at once, with the talk in it as messages.
    func liveEnd() -> JSON {
        liveMode = LiveState()
        return ["t": "live.end"]
    }

    /// The Text button: the words under the thread show or hide.
    func liveText() {
        liveMode.captions.toggle()
    }

    /// A change on Settings › Voice's Live tab, `voice.put` with `live`, shown at once.
    func voiceLive(_ change: [String: JSON]) -> JSON {
        if let b = change["barge_in"]?.bool { voice.settings.bargeIn = b }
        if let p = change["pause"]?.double { voice.settings.pause = p }
        if let c = change["captions"]?.bool { voice.settings.captions = c }
        return voicePut("live", change)
    }

    /// `core.live`, at `now`. Gives whether it was one.
    func liveFrame(_ f: JSON, now: Date = .now) -> Bool {
        guard f["t"].text == "core.live" else { return false }
        let state = f["state"].text
        switch state {
        case "ended":
            liveMode = LiveState()
        case "no_engine":
            // Settings › Voice opens on the tab of the engine that's missing.
            liveMode = LiveState()
            voice.openKind = f["kind"].string ?? "dictation"
            voice.open = true
            if let why = f["why"].string { notice = why }
        default:
            if !liveMode.on || liveMode.started != (f["started"].int ?? liveMode.started) {
                // The first word of it: the time counts from here, and the words show
                // as Show what's said has them.
                liveMode = LiveState()
                liveMode.on = true
                liveMode.since = now
                liveMode.captions = f["captions"].bool ?? true
            }
            liveMode.state = state
            liveMode.agent = f["agent"].text
            liveMode.name = f["name"].text
            liveMode.conversation = f["conversation"].text
            liveMode.started = f["started"].int ?? liveMode.started
            liveMode.muted = f["muted"].bool ?? false
            if let words = f["words"].string { liveMode.words = words }
            if let level = f["level"].double { liveMode.level = level } else if state != "listening" { liveMode.level = 0 }
            if f["stop"].bool == true || state != "speaking" { liveMode.playing = nil }
            if case .object = f["play"] { liveMode.playing = LivePiece(f["play"], began: now) }
            if let why = f["why"].string, !why.isEmpty { notice = why }
        }
        return true
    }
}
