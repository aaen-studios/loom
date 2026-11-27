import Foundation

/// What the sample keeps of Settings › Voice: the settings and the services with a key, as
/// the instance would (shared/voice.calx), and the dictation the mic is on for.
struct SampleVoice {
    var settings: JSON = [
        "dictation": ["engine": "deepgram", "language": ""],
        "speech": ["engine": "openai", "voice": "marin", "voice_name": "Marin", "speed": 1, "read_aloud": "never"],
    ]
    var keys: Set<String> = ["deepgram", "openai"]
    var listening = 0

    static let engines: [(kind: String, id: String, name: String, line: String, streams: Bool)] = [
        ("dictation", "elevenlabs", "ElevenLabs Scribe", "Words as you say them", true),
        ("dictation", "deepgram", "Deepgram Nova-3", "Words as you say them", true),
        ("dictation", "openai", "OpenAI", "gpt-4o-transcribe, words as you say them", true),
        ("dictation", "groq", "Groq", "Whisper large v3, writes once you stop", false),
        ("dictation", "parakeet", "Parakeet v2", "English, 660 MB", true),
        ("dictation", "whisper", "Whisper large v3 turbo", "99 languages, 1 GB", true),
        ("speech", "elevenlabs", "ElevenLabs", "Flash v2.5, your voices and the library's", false),
        ("speech", "fish", "Fish Audio", "S2.1 Pro, the voice library", false),
        ("speech", "openai", "OpenAI", "gpt-4o-mini-tts, 13 voices", false),
        ("speech", "kokoro", "Kokoro", "English and Chinese, 350 MB", false),
    ]

    /// The models on the device, which the core says an iPhone can't run, and their makers'
    /// marks (Kokoro's authors have none).
    static let onDevice: [String: String] = ["parakeet": "nvidia", "whisper": "openai", "kokoro": ""]

    static let languages: [(String, String)] = [
        ("", "Detect"), ("en", "English"), ("no", "Norwegian"), ("sv", "Swedish"), ("da", "Danish"),
        ("fi", "Finnish"), ("de", "German"), ("nl", "Dutch"), ("fr", "French"), ("es", "Spanish"),
        ("it", "Italian"), ("pt", "Portuguese"), ("pl", "Polish"), ("uk", "Ukrainian"), ("tr", "Turkish"),
        ("hi", "Hindi"), ("ja", "Japanese"), ("ko", "Korean"), ("zh", "Chinese"), ("ar", "Arabic"),
    ]

    static let openaiVoices = ["marin", "cedar", "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer", "verse"]

    /// The engine's tile, as `voice.get` gives it.
    func tile(_ kind: String, _ id: String) -> JSON {
        guard let e = Self.engines.first(where: { $0.kind == kind && $0.id == id }) else { return .null }
        if let made = Self.onDevice[e.id] {
            var t: [String: JSON] = ["id": JSON(e.id), "name": JSON(e.name), "line": JSON(e.line), "icon": "laptop", "streams": JSON(e.streams),
                                     "key": false, "have": false, "in_use": JSON(settings[kind]["engine"].text == e.id),
                                     "model": ["state": "unavailable", "why": "Runs on your computer, not on iPhone."]]
            if !made.isEmpty {
                t["icon_svg"] = "<svg/>"
                t["mark"] = JSON(made)
            }
            return .object(t)
        }
        // Its maker's mark, as the app has its copy (`voice-deepgram`, from shared/voice).
        let mark = e.id == "fish" ? "fish-audio" : e.id
        return ["id": JSON(e.id), "name": JSON(e.name), "line": JSON(e.line), "icon": "cloud", "icon_svg": "<svg/>", "mark": JSON(mark), "streams": JSON(e.streams),
                "key": true, "have": JSON(keys.contains(e.id)), "in_use": JSON(settings[kind]["engine"].text == e.id)]
    }

    /// What `voice.get` gives.
    var listed: JSON {
        func tiles(_ kind: String) -> JSON { .array(Self.engines.filter { $0.kind == kind }.map { tile(kind, $0.id) }) }
        var have: [String: JSON] = [:]
        for s in ["elevenlabs", "deepgram", "openai", "groq", "fish"] { have[s] = JSON(keys.contains(s)) }
        return ["settings": settings, "engines": ["dictation": tiles("dictation"), "speech": tiles("speech")],
                "languages": .array(Self.languages.map { ["code": JSON($0.0), "name": JSON($0.1)] }),
                "speeds": [.number(0.8), .number(0.9), .number(1), .number(1.1), .number(1.2)],
                "read_aloud": ["never", "live", "always"], "keys": .object(have)]
    }

    /// The settings with `args`' changes.
    mutating func put(_ args: JSON) {
        guard case .object(var all) = settings else { return }
        for kind in ["dictation", "speech"] {
            guard case .object(let change) = args[kind], case .object(var now) = all[kind] ?? [:] else { continue }
            if kind == "speech", let engine = change["engine"], engine != now["engine"], change["voice"] == nil {
                now["voice"] = ""
                now["voice_name"] = ""
            }
            for (k, v) in change { now[k] = v }
            all[kind] = .object(now)
        }
        settings = .object(all)
    }
}

extension SampleCore {
    /// Settings › Voice's commands and a voice_setup card's Add.
    func voiceCmd(_ f: JSON) {
        let args = f["args"]
        func answer() {
            tell(["t": "result", "id": f["id"], "ok": true, "value": ["voice": voice.listed]])
        }
        func changed() {
            event("voice.updated", ["voice": voice.listed, "by": "d_4"])
        }
        switch f["name"].text {
        case "voice.get":
            answer()
        case "voice.put":
            voice.put(args)
            answer()
            changed()
        case "voice.key":
            let service = args["service"].text
            if args["key"].text.isEmpty { voice.keys.remove(service) } else { voice.keys.insert(service) }
            answer()
            changed()
        case "voice.card":
            let id = args["message"].text
            guard let i = messages["c_2"]?.firstIndex(where: { $0["id"].text == id }), case .object(var m) = messages["c_2"]![i],
                  let c = m["blocks"]?.array.first?["card"], case .object(var card) = c else {
                tell(["t": "result", "id": f["id"], "ok": false, "error": ["code": "not_found", "message": "That card isn't there anymore."]])
                return
            }
            let kind = c["kind"].text
            let service = c["service"].text
            voice.keys.insert(service)
            voice.put(.object([kind: c["settings"]]))
            card["state"] = "set"
            card["engine"] = voice.tile(kind, service)
            m["blocks"] = [["kind": "card", "card": .object(card)]]
            messages["c_2"]![i] = .object(m)
            answer()
            changed()
            event("message.updated", ["conversation": "c_2", "message": .object(m)])
        default:
            tell(["t": "result", "id": f["id"], "ok": false, "error": ["code": "not_found", "message": "No such command."]])
        }
    }

    /// The mic's dictation, a voice's preview and an engine's voices, as the core says them.
    func voiceFrame(_ f: JSON) {
        switch f["t"].text {
        case "voice.listen":
            let engine = voice.settings["dictation"]["engine"].text
            guard !engine.isEmpty, voice.keys.contains(engine) else {
                tell(["t": "core.dictation", "state": "no_engine"])
                return
            }
            voice.listening += 1
            let n = voice.listening
            tell(["t": "core.dictation", "state": "listening", "engine": JSON(engine), "streams": true, "text": ""])
            for (k, words) in ["Water the", "Water the tomatoes", "Water the tomatoes at six."].enumerated() {
                later(0.7 * Double(k + 1)) { [self] in
                    guard voice.listening == n else { return }
                    tell(["t": "core.dictation", "state": "listening", "engine": JSON(engine), "streams": true, "text": JSON(words)])
                }
            }
        case "voice.stop":
            guard voice.listening > 0 else { return }
            voice.listening += 1
            tell(["t": "core.dictation", "state": "finishing", "text": "Water the tomatoes at six."])
            later(0.3) { [self] in
                tell(["t": "core.dictation", "state": "done", "text": "Water the tomatoes at six."])
            }
        case "voice.voices":
            let engine = f["engine"].text
            if engine == "openai" {
                tell(["t": "core.voices", "engine": "openai", "voices": .array(SampleVoice.openaiVoices.map { ["id": JSON($0), "name": JSON($0.prefix(1).uppercased() + String($0.dropFirst()))] })])
            } else if voice.keys.contains(engine) {
                tell(["t": "core.voices", "engine": JSON(engine), "voices": [["id": "v_1", "name": "Narrator"], ["id": "v_2", "name": "Rachel"]]])
            } else {
                tell(["t": "core.voices", "engine": JSON(engine), "error": "Add a key for it first."])
            }
        case "voice.speak":
            tell(["t": "core.speak", "id": f["id"], "error": "The sample has no speech to play."])
        default:
            break
        }
    }
}

extension Sample {
    /// Scribe's chat: an agent's voice_setup, waiting on the person's Fish Audio key.
    static let voiceMessages: [JSON] = [
        ["id": "msg_18", "conversation": "c_2", "role": "person", "from": "d_4", "at": 1795187200000,
         "blocks": [["kind": "text", "text": "Read your replies to me aloud, with a calm voice."]]],
        ["id": "msg_19", "conversation": "c_2", "role": "agent", "agent": "a_2", "run": "run_18", "at": 1795187230000,
         "blocks": [["kind": "card", "card": [
             "id": "voice_tc_18", "type": "voice_setup", "title": "Set up speech", "call": "tc_18", "kind": "speech", "agent": "a_2",
             "engine": ["id": "fish", "name": "Fish Audio", "line": "S2.1 Pro, the voice library", "icon": "cloud", "icon_svg": "<svg/>", "mark": "fish-audio", "streams": false, "key": true, "have": false, "in_use": false],
             "settings": ["engine": "fish", "voice": "v_1", "voice_name": "Narrator", "speed": .number(0.9), "read_aloud": "always"],
             "service": "fish", "state": "asking", "said": ""]]]],
    ]
}
