import Foundation

// loom's voice on the phone (PLAN.md, "Voice"): Settings › Voice as `voice.get` gives it,
// the voice_setup card an agent's voice_setup leaves in its reply, and the words of a
// dictation as the core says them (`core.dictation`). The engines, the keys and the
// streaming are the core's and the instance's (shared/voice.calx, core/voice.calx); the
// phone draws them, records the mic and plays what the core wrote.

/// The ids Settings › Voice's commands go as, and a voice_setup card's Add, before the
/// card's id.
let voiceAnswer = "app.voice"
let voiceCardAnswer = "app.voice_card:"

/// An engine as its tile shows it, as `voice.get` gives one.
struct VoiceTile: Equatable, Sendable, Identifiable {
    var id: String
    var name: String
    var line: String
    /// `cloud` for a service, `laptop` for a model on the device, drawn where it has no
    /// mark.
    var icon: String
    /// Its maker's mark, by the name the app has its copy by, as scripts/ios.calx bundles
    /// shared/voice's (`voice-deepgram`), or none.
    var mark: String?
    var streams: Bool
    /// Whether it needs a key, and whether the person has it.
    var key: Bool
    var have: Bool
    var inUse: Bool

    init(_ j: JSON) {
        id = j["id"].text
        name = j["name"].text
        line = j["line"].text
        icon = j["icon"].string ?? "cloud"
        mark = j["mark"].text.isEmpty ? nil : j["mark"].text
        streams = j["streams"].bool ?? false
        key = j["key"].bool ?? true
        have = j["have"].bool ?? false
        inUse = j["in_use"].bool ?? false
    }

    /// The button beside it: Use, Add key or Change key, or none for one in use that
    /// needs no key.
    var button: VoiceTileButton? {
        if key && !have { return .addKey }
        if inUse { return key ? .changeKey : nil }
        return .use
    }
}

enum VoiceTileButton: Equatable {
    case use, addKey, changeKey

    var words: String {
        switch self {
        case .use: "Use"
        case .addKey: "Add key"
        case .changeKey: "Change key"
        }
    }
}

/// The person's voice settings, as `voice.get` gives them.
struct VoiceSettings: Equatable, Sendable {
    var dictationEngine = ""
    /// `""` detects it.
    var language = ""
    var speechEngine = ""
    /// The voice's id, `""` for the engine's first, and its name.
    var voice = ""
    var voiceName = ""
    var speed = 1.0
    /// never, live or always.
    var readAloud = "never"

    init() {}

    init(_ j: JSON) {
        self.init(dictation: j["dictation"], speech: j["speech"])
    }

    /// One kind's settings alone, as a voice_setup card has them.
    init(kind: String, _ j: JSON) {
        self.init(dictation: kind == "dictation" ? j : .null, speech: kind == "speech" ? j : .null)
    }

    private init(dictation d: JSON, speech s: JSON) {
        dictationEngine = d["engine"].text
        language = d["language"].text
        speechEngine = s["engine"].text
        voice = s["voice"].text
        voiceName = s["voice_name"].text
        speed = s["speed"].double ?? 1
        readAloud = s["read_aloud"].string ?? "never"
    }
}

/// A language dictation hears, by its code, `""` being Detect.
struct VoiceLanguage: Equatable, Sendable, Identifiable {
    var code: String
    var name: String
    var id: String { code }
}

/// One of a speech engine's voices, for the Voice row's picker.
struct VoiceOption: Equatable, Sendable, Identifiable {
    var id: String
    var name: String
}

/// A dictation, as the core tells it: which one it is (each start is the next), how it
/// stands (`listening`, `finishing`, `done`, `failed`, `no_engine`) and all its words so far.
struct Dictation: Equatable, Sendable {
    var n = 0
    var state = ""
    var text = ""

    /// Whether it's over, and the mic is to stop.
    var over: Bool { ["done", "failed", "no_engine"].contains(state) }
}

/// Settings › Voice and dictation, as the instance and the core last said.
struct VoiceState {
    /// Whether `voice.get` has answered.
    var came = false
    var settings = VoiceSettings()
    var dictation: [VoiceTile] = []
    var speech: [VoiceTile] = []
    var languages: [VoiceLanguage] = [VoiceLanguage(code: "", name: "Detect")]
    var speeds: [Double] = [0.8, 0.9, 1.0, 1.1, 1.2]
    var readAloud: [String] = ["never", "live", "always"]
    /// Which services have a key, never a key.
    var keys: [String: Bool] = [:]
    /// Each speech engine's voices once the core has them, or why there are none.
    var voices: [String: [VoiceOption]] = [:]
    var voicesError: [String: String] = [:]
    /// Keys on their way, by service, and the kind to use the engine for once each is kept.
    var keySending: Set<String> = []
    var useOnceKept: [String: String] = [:]
    /// The dictation now, or the last.
    var heard = Dictation()
    /// Settings › Voice, asked for by the mic with no engine set up.
    var open = false

    /// Takes `voice.get`'s `voice`, or `voice.updated`'s.
    mutating func take(_ v: JSON) {
        guard !v.isNull else { return }
        came = true
        settings = VoiceSettings(v["settings"])
        dictation = v["engines"]["dictation"].array.map(VoiceTile.init)
        speech = v["engines"]["speech"].array.map(VoiceTile.init)
        let langs = v["languages"].array.map { VoiceLanguage(code: $0["code"].text, name: $0["name"].text) }
        if !langs.isEmpty { languages = langs }
        let sp = v["speeds"].array.compactMap(\.double)
        if !sp.isEmpty { speeds = sp }
        let ra = v["read_aloud"].array.compactMap(\.string)
        if !ra.isEmpty { readAloud = ra }
        if case .object(let o) = v["keys"] { keys = o.compactMapValues(\.bool) }
    }

    func tiles(_ kind: String) -> [VoiceTile] {
        kind == "speech" ? speech : dictation
    }

    /// A language's name, by its code.
    func languageName(_ code: String) -> String {
        languages.first { $0.code == code }?.name ?? (code.isEmpty ? "Detect" : code)
    }
}

/// A speed as the Speed row says it: `0.8×`, `1×`.
func voiceSpeed(_ s: Double) -> String {
    String(format: "%g×", (s * 10).rounded() / 10)
}

/// A read-aloud choice in words.
func readAloudWords(_ r: String) -> String {
    switch r {
    case "live": "Only in live"
    case "always": "Always"
    default: "Never"
    }
}

/// What a voice_setup card carries beside its frame: its kind, its engine's tile, the
/// settings as they are once it's set, and whose key it needs.
struct VoiceCard: Equatable, Sendable {
    var kind: String
    var tile: VoiceTile
    var settings: VoiceSettings
    var service: String

    init(_ c: JSON) {
        kind = c["kind"].string ?? "dictation"
        tile = VoiceTile(c["engine"])
        settings = VoiceSettings(kind: kind, c["settings"])
        service = c["service"].text
    }

    /// Its rows, read only, as Settings › Voice has them, with the language's name from
    /// `languages`.
    func rows(_ voice: VoiceState) -> [(String, String)] {
        if kind == "speech" {
            return [("Voice", settings.voiceName.isEmpty ? settings.voice : settings.voiceName),
                    ("Speed", voiceSpeed(settings.speed)),
                    ("Read replies aloud", readAloudWords(settings.readAloud))]
        }
        return [("Language", voice.languageName(settings.language))]
    }
}

/// Where a dictation's words go in the composer's text, `text` being NSString's UTF-16:
/// in place of the `count` characters it put there before, from `at`, with a space
/// before them where they'd run into a word. Gives what goes in and the text after.
func dictationSplice(_ text: String, at: Int, count: Int, words: String) -> (put: String, text: String) {
    let s = text as NSString
    let from = min(max(at, 0), s.length)
    let length = min(max(count, 0), s.length - from)
    var put = words
    if from > 0, let first = words.unicodeScalars.first, !CharacterSet.whitespacesAndNewlines.contains(first), !CharacterSet.punctuationCharacters.contains(first),
       let before = s.substring(with: NSRange(location: from - 1, length: 1)).unicodeScalars.first, !CharacterSet.whitespacesAndNewlines.contains(before) {
        put = " " + words
    }
    return (put, s.replacingCharacters(in: NSRange(location: from, length: length), with: put))
}

extension Model {
    /// Settings › Voice, as the instance has it.
    func loadVoice() -> JSON {
        ["t": "cmd", "id": JSON("\(voiceAnswer)_get"), "name": "voice.get", "args": [:]]
    }

    /// A change to the settings, `voice.put` with `dictation` or `speech`.
    func voicePut(_ kind: String, _ change: [String: JSON]) -> JSON {
        ["t": "cmd", "id": JSON("\(voiceAnswer)_put"), "name": "voice.put", "args": [kind: .object(change)]]
    }

    /// Use on a tile.
    func voiceUse(_ kind: String, _ engine: String) -> JSON {
        voicePut(kind, ["engine": JSON(engine)])
    }

    /// Save on a tile's key: the service's key, and once it's kept, the engine in use for
    /// `kind` when it wasn't.
    func voiceKey(_ service: String, _ key: String, use kind: String?) -> JSON {
        voice.keySending.insert(service)
        voice.useOnceKept[service] = kind
        return ["t": "cmd", "id": JSON("\(voiceAnswer)_key:\(service)"), "name": "voice.key",
                "args": ["service": JSON(service), "key": JSON(key)]]
    }

    /// Add on voice_setup card `card` in reply `message`, with the key typed on it.
    func voiceCardAdd(message: String, card: String, key: String) -> JSON {
        voiceCardErrors[card] = nil
        voiceCardSending.insert(card)
        return ["t": "cmd", "id": JSON("\(voiceCardAnswer)\(card)"), "name": "voice.card",
                "args": ["message": JSON(message), "card": JSON(card), "key": JSON(key)]]
    }

    /// The Voice row's voices for speech engine `engine`, from the core.
    func voiceVoices(_ engine: String) -> JSON {
        ["t": "voice.voices", "engine": JSON(engine)]
    }

    /// A voice's preview, with the play button beside the Voice row.
    func voicePreview(engine: String, voice id: String, name: String) -> JSON {
        ["t": "voice.speak", "text": JSON("Hi, I'm \(name)."), "id": "preview", "engine": JSON(engine), "voice": JSON(id)]
    }

    /// The core's frames about voice: a dictation's words, an engine's voices, and why a
    /// preview couldn't be spoken. Gives whether it was one.
    func voiceFrame(_ f: JSON) -> Bool {
        switch f["t"].text {
        case "core.dictation":
            voice.heard.state = f["state"].text
            if let text = f["text"].string { voice.heard.text = text }
            switch voice.heard.state {
            case "no_engine": voice.open = true
            case "failed":
                let why = f["why"].text
                notice = why.isEmpty ? "Dictation stopped." : why
            default: break
            }
        case "core.voices":
            let engine = f["engine"].text
            if let error = f["error"].string ?? f["error"]["message"].string {
                voice.voicesError[engine] = error
                voice.voices[engine] = nil
            } else {
                voice.voicesError[engine] = nil
                voice.voices[engine] = f["voices"].array.map { VoiceOption(id: $0["id"].text, name: $0["name"].text) }
            }
        case "core.speak":
            if f["id"].text == "preview", let error = f["error"].string ?? f["error"]["message"].string {
                notice = error
            }
        default:
            return false
        }
        return true
    }

    /// A dictation starting: its words go in from here.
    func dictationStarted() {
        voice.heard = Dictation(n: voice.heard.n + 1, state: "listening", text: "")
    }

    /// The answers to Settings › Voice's commands and a voice_setup card's Add. Gives
    /// whether it was one.
    func voiceResult(_ f: JSON) -> Bool {
        let id = f["id"].text
        let error = f["error"].isNull ? nil : (f["error"]["message"].string ?? f["error"]["code"].text)
        if id.hasPrefix(voiceCardAnswer) {
            let card = String(id.dropFirst(voiceCardAnswer.count))
            voiceCardSending.remove(card)
            if let error { voiceCardErrors[card] = error } else { voice.take(f["value"]["voice"]) }
            return true
        }
        guard id.hasPrefix(voiceAnswer) else { return false }
        if id.hasPrefix("\(voiceAnswer)_key:") {
            let service = String(id.dropFirst("\(voiceAnswer)_key:".count))
            voice.keySending.remove(service)
            let kind = voice.useOnceKept.removeValue(forKey: service)
            if let error {
                notice = error
            } else {
                voice.take(f["value"]["voice"])
                if let kind { pending.append(voiceUse(kind, service)) }
            }
            return true
        }
        if let error {
            notice = error
        } else {
            voice.take(f["value"]["voice"])
        }
        return true
    }
}
