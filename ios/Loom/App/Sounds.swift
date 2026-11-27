import AVFoundation
import Foundation
import UserNotifications

/// loom's sounds on the phone (PLAN.md, "Sounds"): the prefs as Settings → Sounds keeps
/// them, which the core decides every sound by (`app.sounds`, shared/sounds.calx), and
/// playing what the core says (`core.sound`) or naming it in a notification.
struct SoundPrefs: Equatable {
    /// loom, system or off.
    var mode = "loom"
    var volume = 0.8
    /// Each cue's switch, by its name in `cues`.
    var on: [String: Bool] = ["reply": true, "waiting": true, "other": true, "routine": false, "failed": true, "sent": false, "dictation": true]
    var inFront = true
    var quiet = false
    var from = "22:00"
    var to = "07:30"

    /// Each cue and its row's words.
    static let cues: [(String, String)] = [
        ("reply", "Reply arrived"),
        ("waiting", "Waiting on you"),
        ("other", "Another agent answered"),
        ("routine", "Routine posted"),
        ("failed", "Routine failed"),
        ("sent", "Message sent"),
        ("dictation", "Dictation on and off"),
    ]

    static let key = "loom.sounds"

    init() {}

    /// The prefs as `app.sounds` and `core.sounds` carry them.
    init(json j: JSON) {
        if let m = j["mode"].string { mode = m }
        if let v = j["volume"].double { volume = v }
        for (cue, _) in Self.cues {
            if let b = j["cues"][cue].bool { on[cue] = b }
        }
        if let b = j["in_front"].bool { inFront = b }
        if let b = j["quiet"]["on"].bool { quiet = b }
        if let f = j["quiet"]["from"].string { from = f }
        if let t = j["quiet"]["to"].string { to = t }
    }

    var json: JSON {
        var cues: [String: JSON] = [:]
        for (cue, b) in on { cues[cue] = JSON(b) }
        return ["mode": JSON(mode), "volume": .number((volume * 100).rounded() / 100), "cues": .object(cues),
                "in_front": JSON(inFront), "quiet": ["on": JSON(quiet), "from": JSON(from), "to": JSON(to)]]
    }

    /// The `app.sounds` frame for them.
    var frame: JSON { ["t": "app.sounds", "sounds": json] }

    /// The prefs this phone keeps.
    static func load() -> SoundPrefs {
        guard let text = UserDefaults.standard.string(forKey: key), let j = try? JSON(parsing: Data(text.utf8)) else { return SoundPrefs() }
        return SoundPrefs(json: j)
    }

    func save() {
        UserDefaults.standard.set(String(decoding: json.encoded, as: UTF8.self), forKey: Self.key)
    }
}

enum Sounds {
    /// The sound playing now, kept until it's done.
    @MainActor private static var playing: AVAudioPlayer?

    /// Plays one of loom's sounds, bundled as `FILE.wav`, at `volume`. It's ambient, so
    /// the ring switch silences it.
    @MainActor
    static func play(_ file: String, volume: Double) {
        guard let url = Bundle.main.url(forResource: file, withExtension: "wav"),
              let player = try? AVAudioPlayer(contentsOf: url) else { return }
        // While the mic records, the session stays as dictation set it.
        if !Mic.on { try? AVAudioSession.sharedInstance().setCategory(.ambient) }
        player.volume = Float(volume)
        player.play()
        playing = player
    }

    /// What a `core.notify` sounds with: loom's file by name, so Focus applies, the
    /// system's sound, or none.
    static func notification(_ named: String) -> UNNotificationSound? {
        switch named {
        case "": return nil
        case "default": return .default
        default: return UNNotificationSound(named: UNNotificationSoundName("\(named).wav"))
        }
    }
}
