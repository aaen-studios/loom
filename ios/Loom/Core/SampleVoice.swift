import Foundation

/// What the sample keeps of Settings › Voice: the settings and the services with a key, as
/// the instance would (shared/voice.calx), the dictation the mic is on for, and live mode.
struct SampleVoice {
    var settings: JSON = [
        "dictation": ["engine": "deepgram", "language": "", "models": ["deepgram": "nova-3"]],
        "speech": ["engine": "openai", "voice": "marin", "voice_name": "Marin", "speed": 1, "read_aloud": "never", "models": [:]],
        "live": ["barge_in": true, "pause": .number(0.8), "captions": true],
    ]
    var keys: Set<String> = ["deepgram", "openai"]
    var listening = 0
    /// Live mode: each start, mute and end is the next, which stops what the last one had
    /// waiting.
    var live = 0
    var liveOn: JSON = .null
    var liveMuted = false

    /// Each engine, with its line on the device's models; a service's is its model's name.
    static let engines: [(kind: String, id: String, name: String, line: String, streams: Bool)] = [
        ("dictation", "elevenlabs", "ElevenLabs", "", true),
        ("dictation", "deepgram", "Deepgram", "", true),
        ("dictation", "openai", "OpenAI", "", true),
        ("dictation", "groq", "Groq", "", false),
        ("dictation", "parakeet-ultra", "Parakeet Ultra", "25 languages, 670 MB", true),
        ("dictation", "parakeet-v3", "Parakeet v3", "25 languages, 670 MB", true),
        ("dictation", "whisper", "Whisper large v3 turbo", "99 languages, 1 GB", true),
        ("dictation", "parakeet", "Parakeet v2", "English, 660 MB", true),
        ("speech", "elevenlabs", "ElevenLabs", "", false),
        ("speech", "fish", "Fish Audio", "", false),
        ("speech", "openai", "OpenAI", "", false),
        ("speech", "deepgram", "Deepgram", "", false),
        ("speech", "kokoro-v1.1", "Kokoro v1.1", "Chinese, 3 English voices, 365 MB", false),
        ("speech", "supertonic", "Supertonic 3", "31 languages, 10 voices, 130 MB", false),
        ("speech", "pocket", "Pocket TTS", "English, 2 voices, 100 MB", false),
        ("speech", "kokoro", "Kokoro v1.0", "English and Chinese, 350 MB", false),
    ]

    /// Each service's models, newest first, the first its default (shared/voice.calx's
    /// MODELS).
    static let models: [String: [(id: String, name: String, line: String)]] = [
        "dictation/elevenlabs": [("scribe_v3_live", "Scribe v3 Live", "Words as you say them"), ("scribe_v2_realtime", "Scribe v2 Realtime", "The model before")],
        "dictation/deepgram": [("nova-3", "Nova-3", "Words as you say them"), ("flux-general-en", "Flux English", "English, hears when you're done"),
                               ("flux-general-multi", "Flux multilingual", "Ten languages, hears when you're done")],
        "dictation/openai": [("gpt-live-transcribe", "gpt-live-transcribe", "Words as you say them"), ("gpt-transcribe", "gpt-transcribe", "Most accurate, writes once you stop"),
                             ("gpt-realtime-whisper", "gpt-realtime-whisper", "Whisper, words as you say them")],
        "dictation/groq": [("whisper-large-v3-turbo", "Whisper large v3 turbo", "Writes once you stop, quicker"), ("whisper-large-v3", "Whisper large v3", "Writes once you stop, more accurate")],
        "speech/elevenlabs": [("eleven_v4_turbo", "v4 Turbo", "Quickest, your voices and the library's"), ("eleven_v4", "v4", "Most expressive"),
                              ("eleven_v3", "v3", "The model before"), ("eleven_multilingual_v2", "Multilingual v2", "Steady, and follows Speed")],
        "speech/fish": [("drama-3", "Drama 3", "Most expressive, the voice library"), ("s2.1-pro", "S2.1 Pro", "Natural and quick"), ("s2-pro", "S2 Pro", "The model before")],
        "speech/openai": [("gpt-realtime-2.1-mini", "gpt-realtime-2.1-mini", "10 voices")],
        "speech/deepgram": [("flux", "Flux", "36 English voices"), ("aura-2", "Aura-2", "41 English voices")],
    ]

    /// The models on the device, which the core says an iPhone can't run, and their makers'
    /// marks (Kokoro's, Supertonic's and Pocket's authors have none).
    static let onDevice: [String: String] = ["parakeet-ultra": "nvidia", "parakeet-v3": "nvidia", "whisper": "openai", "parakeet": "nvidia",
                                             "kokoro-v1.1": "", "supertonic": "", "pocket": "", "kokoro": ""]

    static let languages: [(String, String)] = [
        ("", "Detect"), ("en", "English"), ("no", "Norwegian"), ("sv", "Swedish"), ("da", "Danish"),
        ("fi", "Finnish"), ("de", "German"), ("nl", "Dutch"), ("fr", "French"), ("es", "Spanish"),
        ("it", "Italian"), ("pt", "Portuguese"), ("pl", "Polish"), ("uk", "Ukrainian"), ("tr", "Turkish"),
        ("hi", "Hindi"), ("ja", "Japanese"), ("ko", "Korean"), ("zh", "Chinese"), ("ar", "Arabic"),
    ]

    static let openaiVoices = ["marin", "cedar", "alloy", "ash", "ballad", "coral", "echo", "sage", "shimmer", "verse"]

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
        // Its maker's mark, as the app has its copy (`voice-deepgram`, from shared/voice),
        // and the model it uses: the one kept, else its first.
        let mark = e.id == "fish" ? "fish-audio" : e.id
        let models = Self.models["\(kind)/\(e.id)"] ?? []
        let kept = settings[kind]["models"][e.id].text
        let picked = models.first { $0.id == kept } ?? models[0]
        return ["id": JSON(e.id), "name": JSON(e.name), "line": JSON(picked.name), "icon": "cloud", "icon_svg": "<svg/>", "mark": JSON(mark), "streams": JSON(e.streams),
                "key": true, "have": JSON(keys.contains(e.id)), "in_use": JSON(settings[kind]["engine"].text == e.id),
                "models": .array(models.map { ["id": JSON($0.id), "name": JSON($0.name), "line": JSON($0.line)] }), "picked": JSON(picked.id)]
    }

    /// What `voice.get` gives.
    var listed: JSON {
        func tiles(_ kind: String) -> JSON { .array(Self.engines.filter { $0.kind == kind }.map { tile(kind, $0.id) }) }
        var have: [String: JSON] = [:]
        for s in ["elevenlabs", "deepgram", "openai", "groq", "fish"] { have[s] = JSON(keys.contains(s)) }
        return ["settings": settings, "engines": ["dictation": tiles("dictation"), "speech": tiles("speech")],
                "languages": .array(Self.languages.map { ["code": JSON($0.0), "name": JSON($0.1)] }),
                "speeds": [.number(0.8), .number(0.9), .number(1), .number(1.1), .number(1.2)],
                "read_aloud": ["never", "live", "always"], "pauses": [.number(0.5), .number(0.8), .number(1.2)], "keys": .object(have)]
    }

    /// The settings with `args`' changes: a `model` is the engine in use's.
    mutating func put(_ args: JSON) {
        guard case .object(var all) = settings else { return }
        for kind in ["dictation", "speech", "live"] {
            guard case .object(var change) = args[kind], case .object(var now) = all[kind] ?? [:] else { continue }
            if kind == "speech", let engine = change["engine"], engine != now["engine"], change["voice"] == nil {
                now["voice"] = ""
                now["voice_name"] = ""
            }
            if let model = change.removeValue(forKey: "model") {
                let engine = (change["engine"] ?? now["engine"] ?? "").text
                var models: [String: JSON] = [:]
                if case .object(let o) = now["models"] ?? [:] { models = o }
                models[engine] = model
                now["models"] = .object(models)
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

extension SampleCore {
    /// Live mode, as core/live.calx says it, with a turn the sample makes up: the person
    /// talking for 2 s, the agent thinking for 1 s, then speaking its reply in two pieces
    /// with their words, when each starts and their levels, but no file to play, then
    /// listening again. `-live speaking` or `-live listening` at launch holds it in that
    /// state, for screenshots of it.
    func liveFrame(_ f: JSON, name: String) {
        switch f["t"].text {
        case "live.start":
            voice.live += 1
            let n = voice.live
            voice.liveMuted = false
            voice.liveOn = ["t": "core.live", "agent": f["agent"], "name": JSON(name), "conversation": f["conversation"],
                            "started": JSON(Sample.now()), "captions": voice.settings["live"]["captions"], "state": "listening", "words": ""]
            let args = ProcessInfo.processInfo.arguments
            let hold = args.firstIndex(of: "-live").flatMap { $0 + 1 < args.count ? args[$0 + 1] : nil }
            switch hold {
            case "speaking":
                liveTell("thinking", words: Sample.liveAsked)
                liveSay(n, [Sample.liveHeld], then: nil)
            case "listening":
                liveTell("listening", words: "Which plants need")
                liveHear(n, loud: true)
            default:
                liveTurn(n)
            }
        case "live.mute":
            guard !voice.liveOn.isNull else { return }
            voice.liveMuted = f["on"].bool ?? true
            liveTell(voice.liveOn["state"].text, words: voice.liveOn["words"].text)
        case "live.end":
            guard !voice.liveOn.isNull else { return }
            voice.live += 1
            let conversation = voice.liveOn["conversation"]
            voice.liveOn = .null
            tell(["t": "core.live", "state": "ended", "conversation": conversation, "stop": true])
        default:
            break
        }
    }

    /// Tells the app how live mode stands, with `extra` (level, play).
    private func liveTell(_ state: String, words: String, _ extra: [String: JSON] = [:]) {
        guard case .object(var f) = voice.liveOn else { return }
        f["state"] = JSON(state)
        f["words"] = JSON(words)
        f["muted"] = JSON(voice.liveMuted)
        voice.liveOn = .object(f)
        for (k, v) in extra { f[k] = v }
        tell(.object(f))
    }

    /// The person's turn: their words as they're heard, then the agent thinks, and speaks.
    private func liveTurn(_ n: Int) {
        liveTell("listening", words: "")
        liveHear(n, loud: true)
        for (k, words) in ["Which plants", "Which plants need water", Sample.liveAsked].enumerated() {
            later(0.6 * Double(k + 1)) { [self] in
                guard voice.live == n else { return }
                liveTell("listening", words: words)
            }
        }
        later(2) { [self] in
            guard voice.live == n, case .object = voice.liveOn else { return }
            let c = voice.liveOn["conversation"]
            event("message.created", ["conversation": c, "message": ["id": JSON("msg_live\(n)a"), "conversation": c, "role": "person", "from": "d_4", "at": JSON(Sample.now()),
                                                                     "blocks": [["kind": "text", "text": JSON(Sample.liveAsked)]]]])
            liveTell("thinking", words: Sample.liveAsked)
            later(1) { [self] in
                guard voice.live == n else { return }
                liveSay(n, Sample.liveReply) { [self] in
                    let c = voice.liveOn["conversation"]
                    event("message.created", ["conversation": c, "message": ["id": JSON("msg_live\(n)b"), "conversation": c, "role": "agent", "agent": voice.liveOn["agent"],
                                                                             "run": JSON("run_live\(n)"), "at": JSON(Sample.now()),
                                                                             "blocks": [["kind": "text", "text": JSON(Sample.liveReply.joined(separator: " "))]]]])
                    liveTell("listening", words: Sample.liveAsked)
                    liveHear(n, loud: false)
                }
            }
        }
    }

    /// The mic's level about every 100 ms while it listens and isn't muted: someone
    /// talking, or the room.
    private func liveHear(_ n: Int, loud: Bool, k: Int = 0) {
        later(0.1) { [self] in
            guard voice.live == n, voice.liveOn["state"].text == "listening" else { return }
            if !voice.liveMuted {
                let x = Double(k)
                let level = loud ? 0.3 + 0.45 * abs(sin(x * 0.55)) * (0.7 + 0.3 * sin(x * 1.7)) : 0.04 + 0.03 * abs(sin(x * 0.8))
                liveTell("listening", words: voice.liveOn["words"].text, ["level": .number((level * 100).rounded() / 100)])
            }
            liveHear(n, loud: loud, k: k + 1)
        }
    }

    /// Speaks `pieces` one after another, each when the one before has played, and then
    /// does `then`; or with none, says the first again and again.
    private func liveSay(_ n: Int, _ pieces: [String], then: (@Sendable () -> Void)?) {
        guard let first = pieces.first else {
            then?()
            return
        }
        let p = Sample.livePiece(first)
        liveTell("speaking", words: voice.liveOn["words"].text, ["play": p])
        later(Double(p["ms"].int ?? 0) / 1000) { [self] in
            guard voice.live == n else { return }
            liveSay(n, then == nil ? pieces : Array(pieces.dropFirst()), then: then)
        }
    }
}

extension Sample {
    static let liveAsked = "Which plants need water today?"
    static let liveReply = ["The tomatoes on the balcony need water today, about a litre each.", "The basil is fine until tomorrow."]
    static let liveHeld = "The tomatoes on the balcony need water today, about a litre each, and the basil on the kitchen window is fine until tomorrow morning."

    /// A piece of speech as core/live.calx gives it, for `text`: each word's start, by its
    /// length, and a level each 20 ms that rises and falls with each word. It has no file.
    static func livePiece(_ text: String) -> JSON {
        let words = text.split(separator: " ").map(String.init)
        var at: [Int] = []
        var spans: [(Int, Int)] = []
        var t = 120
        for w in words {
            let ms = 140 + 42 * w.count
            at.append(t)
            spans.append((t, t + ms))
            t += ms + (w.hasSuffix(",") || w.hasSuffix(".") ? 220 : 40)
        }
        let ms = t + 160
        var levels: [JSON] = []
        for i in 0..<(ms / 20) {
            let now = i * 20
            var l = 0.02
            if let span = spans.first(where: { now >= $0.0 && now < $0.1 }) {
                let u = Double(now - span.0) / Double(span.1 - span.0)
                l = sin(.pi * u) * (0.55 + 0.35 * sin(Double(i) * 0.9))
            }
            levels.append(.number((max(l, 0) * 100).rounded() / 100))
        }
        return ["file": "", "words": JSON(words), "at": .array(at.map { JSON($0) }), "levels": .array(levels), "step": 20, "ms": JSON(ms)]
    }

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
