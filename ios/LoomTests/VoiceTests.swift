import Foundation
import Testing
@testable import Loom

/// Settings › Voice, the voice_setup card and dictation's words in the composer (PLAN.md,
/// "Voice"), as the phone reads what the instance and the core say.
@MainActor
@Suite struct VoiceTests {
    /// `voice.get`'s `voice`, as shared/voice.calx's `listed` gives it.
    static let listed: JSON = [
        "settings": ["dictation": ["engine": "deepgram", "language": "no"],
                     "speech": ["engine": "openai", "voice": "marin", "voice_name": "Marin", "speed": .number(1.1), "read_aloud": "always"]],
        "engines": [
            "dictation": [
                ["id": "elevenlabs", "name": "ElevenLabs", "line": "Scribe v3 Live", "icon": "cloud", "streams": true, "key": true, "have": false, "in_use": false,
                 "models": [["id": "scribe_v3_live", "name": "Scribe v3 Live", "line": "Words as you say them"], ["id": "scribe_v2_realtime", "name": "Scribe v2 Realtime", "line": "The model before"]], "picked": "scribe_v3_live"],
                ["id": "deepgram", "name": "Deepgram", "line": "Nova-3", "icon": "cloud", "icon_svg": "<svg/>", "mark": "deepgram", "streams": true, "key": true, "have": true, "in_use": true,
                 "models": [["id": "nova-3", "name": "Nova-3", "line": "Words as you say them"], ["id": "flux-general-en", "name": "Flux English", "line": "English, hears when you're done"]], "picked": "nova-3"],
                ["id": "groq", "name": "Groq", "line": "Whisper large v3 turbo", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": false],
            ],
            "speech": [
                ["id": "openai", "name": "OpenAI", "line": "gpt-realtime-2.1-mini", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": true,
                 "models": [["id": "gpt-realtime-2.1-mini", "name": "gpt-realtime-2.1-mini", "line": "10 voices"]], "picked": "gpt-realtime-2.1-mini"],
                ["id": "deepgram", "name": "Deepgram", "line": "Flux", "icon": "cloud", "streams": false, "key": true, "have": true, "in_use": false,
                 "models": [["id": "flux", "name": "Flux", "line": "36 English voices"], ["id": "aura-2", "name": "Aura-2", "line": "41 English voices"]], "picked": "flux"],
            ],
        ],
        "languages": [["code": "", "name": "Detect"], ["code": "en", "name": "English"], ["code": "no", "name": "Norwegian"]],
        "speeds": [.number(0.8), .number(0.9), .number(1), .number(1.1), .number(1.2)],
        "read_aloud": ["never", "live", "always"],
        "keys": ["deepgram": true, "groq": true, "openai": true, "fish": false, "elevenlabs": false],
    ]

    @Test func readsVoiceGet() {
        let m = caughtUp()
        let ask = m.loadVoice()
        #expect(ask["name"] == "voice.get" && ask["args"] == [:])
        m.take(["t": "result", "id": ask["id"], "ok": true, "value": ["voice": Self.listed]])
        let v = m.voice
        #expect(v.came)
        #expect(v.settings.dictationEngine == "deepgram" && v.settings.language == "no")
        #expect(v.settings.speechEngine == "openai" && v.settings.voice == "marin" && v.settings.voiceName == "Marin" && v.settings.speed == 1.1 && v.settings.readAloud == "always")
        #expect(v.dictation.map(\.id) == ["elevenlabs", "deepgram", "groq"] && v.speech.map(\.id) == ["openai", "deepgram"])
        #expect(v.languageName("no") == "Norwegian" && v.languageName("") == "Detect")
        #expect(v.speeds == [0.8, 0.9, 1, 1.1, 1.2] && v.keys["fish"] == false && v.keys["groq"] == true)
    }

    @Test func voiceUpdatedChangesThePage() {
        let m = caughtUp()
        m.take(["t": "result", "id": "app.voice_get", "ok": true, "value": ["voice": Self.listed]])
        guard case .object(var v) = Self.listed, case .object(var s) = v["settings"]!, case .object(var d) = s["dictation"]! else { return }
        d["language"] = ""
        s["dictation"] = .object(d)
        v["settings"] = .object(s)
        m.take(["t": "event", "kind": "voice.updated", "seq": 3, "voice": .object(v), "by": "d_1"])
        #expect(m.voice.settings.language == "")
    }

    @Test func eachTileHasItsButton() {
        let m = caughtUp()
        m.take(["t": "result", "id": "app.voice_get", "ok": true, "value": ["voice": Self.listed]])
        #expect(m.voice.dictation.map(\.button) == [.addKey, .changeKey, .use])
        #expect(m.voice.dictation.map(\.inUse) == [false, true, false])
        #expect(m.voice.dictation.map(\.mark) == [nil, "deepgram", nil])
        // A model on the device needs no key: Use, and nothing once it's in use.
        let onDevice = VoiceTile(["id": "whisper", "name": "Whisper", "line": "On this phone", "icon": "laptop", "key": false, "have": true, "in_use": false])
        #expect(onDevice.button == .use)
        #expect(VoiceTile(["id": "whisper", "icon": "laptop", "key": false, "have": true, "in_use": true]).button == nil)
        #expect(VoiceTileButton.addKey.words == "Add key" && VoiceTileButton.changeKey.words == "Change key")
        // One the core says can't run here says why, with no button.
        let phone = VoiceTile(["id": "parakeet", "icon": "laptop", "mark": "nvidia", "key": false, "have": false, "in_use": false,
                               "model": ["state": "unavailable", "why": "Runs on your computer, not on iPhone."]])
        #expect(phone.why == "Runs on your computer, not on iPhone." && phone.button == nil)
        #expect(VoiceTile(["id": "parakeet", "key": false, "have": true, "in_use": false, "model": ["state": "ready"]]).why == nil)
    }

    @Test func theTileInUsePicksItsModel() {
        let m = caughtUp()
        m.take(["t": "result", "id": "app.voice_get", "ok": true, "value": ["voice": Self.listed]])
        // The one in use with more than one model has the chip; one not in use, or with one
        // model, says its model in its line.
        #expect(m.voice.dictation.map(\.picksModel) == [false, true, false])
        #expect(m.voice.speech.map(\.picksModel) == [false, false])
        let deepgram = m.voice.dictation[1]
        #expect(deepgram.models.map(\.id) == ["nova-3", "flux-general-en"] && deepgram.picked == "nova-3" && deepgram.line == "Nova-3")
        #expect(deepgram.models[1] == VoiceModelChoice(id: "flux-general-en", name: "Flux English", line: "English, hears when you're done"))
        #expect(m.voiceModel("dictation", "flux-general-en")["args"] == ["dictation": ["model": "flux-general-en"]])
        // A speech engine on another model drops the voices it had.
        m.take(["t": "core.voices", "engine": "deepgram", "voices": [["id": "flux-thalia-en", "name": "Thalia"]]])
        m.take(["t": "core.voices", "engine": "openai", "voices": [["id": "marin", "name": "Marin"]]])
        guard case .object(var v) = Self.listed, case .object(var e) = v["engines"]!, case .array(var speech) = e["speech"]!, case .object(var dg) = speech[1] else { return }
        dg["picked"] = "aura-2"
        speech[1] = .object(dg)
        e["speech"] = .array(speech)
        v["engines"] = .object(e)
        m.take(["t": "event", "kind": "voice.updated", "seq": 3, "voice": .object(v), "by": "d_1"])
        #expect(m.voice.voices["deepgram"] == nil && m.voice.voices["openai"] != nil)
    }

    @Test func useAndKeysSendWhatTheInstanceTakes() {
        let m = caughtUp()
        #expect(m.voiceUse("dictation", "groq")["args"] == ["dictation": ["engine": "groq"]])
        #expect(m.voicePut("speech", ["speed": .number(0.9)])["args"] == ["speech": ["speed": .number(0.9)]])
        let key = m.voiceKey("elevenlabs", "sk_1", use: "dictation")
        #expect(key["name"] == "voice.key" && key["args"] == ["service": "elevenlabs", "key": "sk_1"])
        #expect(m.voice.keySending.contains("elevenlabs"))
        // Once it's kept, the engine is put in use.
        m.take(["t": "result", "id": key["id"], "ok": true, "value": ["voice": Self.listed]])
        #expect(!m.voice.keySending.contains("elevenlabs"))
        #expect(m.pending.contains { $0["name"] == "voice.put" && $0["args"] == ["dictation": ["engine": "elevenlabs"]] })
        // A refused key says why, and nothing is put in use.
        m.pending = []
        let again = m.voiceKey("groq", "bad", use: "dictation")
        m.take(["t": "result", "id": again["id"], "ok": false, "error": ["code": "bad_request", "message": "That key is empty."]])
        #expect(m.notice == "That key is empty." && m.pending.isEmpty)
    }

    @Test func voicesAndPreviews() {
        let m = caughtUp()
        #expect(m.voiceVoices("fish") == ["t": "voice.voices", "engine": "fish"])
        m.take(["t": "core.voices", "engine": "fish", "voices": [["id": "v_1", "name": "Narrator"]]])
        #expect(m.voice.voices["fish"] == [VoiceOption(id: "v_1", name: "Narrator")])
        m.take(["t": "core.voices", "engine": "elevenlabs", "error": "Add your ElevenLabs key first."])
        #expect(m.voice.voicesError["elevenlabs"] == "Add your ElevenLabs key first." && m.voice.voices["elevenlabs"] == nil)
        #expect(m.voicePreview(engine: "fish", voice: "v_1", name: "Narrator") == ["t": "voice.speak", "text": "Hi, I'm Narrator.", "id": "preview", "engine": "fish", "voice": "v_1"])
        m.take(["t": "core.speak", "id": "preview", "error": "Fish Audio said no."])
        #expect(m.notice == "Fish Audio said no.")
        #expect(voiceSpeed(0.8) == "0.8×" && voiceSpeed(1) == "1×" && readAloudWords("live") == "Only in live")
    }

    @Test func dictationSaysItsWords() {
        let m = caughtUp()
        m.dictationStarted()
        #expect(m.voice.heard == Dictation(n: 1, state: "listening", text: ""))
        m.take(["t": "core.dictation", "state": "listening", "engine": "deepgram", "streams": true, "text": "Water the"])
        #expect(m.voice.heard.text == "Water the" && !m.voice.heard.over)
        m.take(["t": "core.dictation", "state": "done", "text": "Water the tomatoes."])
        #expect(m.voice.heard.over && m.voice.heard.text == "Water the tomatoes.")
        // With no engine, Settings › Voice opens; a failure says why.
        m.dictationStarted()
        m.take(["t": "core.dictation", "state": "no_engine"])
        #expect(m.voice.open && m.voice.heard.n == 2)
        m.take(["t": "core.dictation", "state": "failed", "why": "Deepgram didn't answer.", "text": ""])
        #expect(m.notice == "Deepgram didn't answer.")
    }

    @Test func dictationGoesInAtTheCaret() {
        // Into empty words, then each time in place of what it put before.
        var (put, text) = dictationSplice("", at: 0, count: 0, words: "Water the")
        #expect(put == "Water the" && text == "Water the")
        (put, text) = dictationSplice(text, at: 0, count: (put as NSString).length, words: "Water the tomatoes")
        #expect(text == "Water the tomatoes")
        // After a word, with a space; after a space, without; before what follows.
        (put, text) = dictationSplice("Remind me to. Thanks", at: 12, count: 0, words: "water")
        #expect(put == " water" && text == "Remind me to water. Thanks")
        (put, text) = dictationSplice("Remind me to . Thanks", at: 13, count: 0, words: "water")
        #expect(put == "water" && text == "Remind me to water. Thanks")
        // Punctuation runs on, and a selection is replaced.
        (put, text) = dictationSplice("Done", at: 4, count: 0, words: ", thanks")
        #expect(text == "Done, thanks")
        (put, text) = dictationSplice("Say hi now", at: 4, count: 2, words: "hello")
        #expect(text == "Say hello now")
        // Past the end, it goes on the end.
        (_, text) = dictationSplice("ok", at: 9, count: 3, words: "go")
        #expect(text == "ok go")
    }

    /// A reply with a voice_setup card, as the instance stores it.
    func reply(_ state: String) -> JSON {
        let c: JSON = ["id": "voice_tc_4", "type": "voice_setup", "title": "Set up dictation", "call": "tc_4", "kind": "dictation", "agent": "a_1",
                       "engine": ["id": "deepgram", "name": "Deepgram", "line": "Nova-3", "icon": "cloud", "streams": true, "key": true, "have": JSON(state == "set"), "in_use": JSON(state == "set")],
                       "settings": ["engine": "deepgram", "language": "no"], "service": "deepgram", "state": JSON(state), "said": ""]
        return ["id": "msg_9", "conversation": "c_1", "role": "agent", "at": 1_795_187_000_000, "blocks": [["kind": "card", "card": c]]]
    }

    func shown(_ m: Model) throws -> Card {
        let msg = try #require(m.shown.first { $0.id == "msg_9" })
        return try #require(msg.blocks.compactMap(\.card).first)
    }

    @Test func readsTheCard() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("asking")])
        let c = try shown(m)
        let v = try #require(c.voice)
        #expect(c.type == "voice_setup" && c.title == "Set up dictation" && c.state == "asking" && c.message == "msg_9")
        #expect(v.kind == "dictation" && v.service == "deepgram" && v.tile.name == "Deepgram" && !v.tile.inUse && v.tile.button == .addKey)
        #expect(v.settings.language == "no")
        m.take(["t": "result", "id": "app.voice_get", "ok": true, "value": ["voice": Self.listed]])
        #expect(v.rows(m.voice).map { [$0.0, $0.1] } == [["Language", "Norwegian"]])
        came(m, [reply("set")])
        #expect(try shown(m).voice?.tile.inUse == true)
        // Speech's rows: its voice, speed and when replies are read aloud.
        let speech = VoiceCard(["kind": "speech", "engine": ["id": "fish", "name": "Fish Audio"],
                                "settings": ["engine": "fish", "voice": "v_1", "voice_name": "Narrator", "speed": .number(0.9), "read_aloud": "always"], "service": "fish"])
        #expect(speech.rows(m.voice).map { [$0.0, $0.1] } == [["Voice", "Narrator"], ["Speed", "0.9×"], ["Read replies aloud", "Always"]])
    }

    @Test func addSendsTheKeyAndARefusalShowsOnTheCard() throws {
        let m = caughtUp()
        m.select("a_1")
        came(m, [reply("asking")])
        let cmd = m.voiceCardAdd(message: "msg_9", card: "voice_tc_4", key: "dg_1")
        #expect(cmd["name"] == "voice.card" && cmd["args"] == ["message": "msg_9", "card": "voice_tc_4", "key": "dg_1"])
        #expect(try shown(m).sending)
        m.take(["t": "result", "id": cmd["id"], "ok": false, "error": ["code": "not_now", "message": "That card is set already."]])
        let c = try shown(m)
        #expect(!c.sending && c.error == "That card is set already.")
    }
}
