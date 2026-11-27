import Foundation
import Testing
@testable import Loom

/// Live mode (PLAN.md, "Live mode"), as the phone reads what the core says of it
/// (`core.live`), and the Live tab of Settings › Voice.
@MainActor
@Suite struct LiveTests {
    /// A `core.live` as core/live.calx tells it, for Builder's chat.
    func live(_ state: String, _ extra: [String: JSON] = [:]) -> JSON {
        var f: [String: JSON] = ["t": "core.live", "state": JSON(state), "agent": "a_1", "name": "Builder", "conversation": "c_1",
                                 "started": 1_795_187_131_000, "muted": false, "captions": true, "words": ""]
        for (k, v) in extra { f[k] = v }
        return .object(f)
    }

    /// A piece of speech: three words, at 0, 400 and 900 ms, and a level each 20 ms.
    let piece: JSON = ["file": "/tmp/voice/speech-1.wav", "words": ["Water", "the", "tomatoes."], "at": [0, 400, 900],
                       "levels": [.number(0.1), .number(0.6), .number(0.9), .number(0.3)], "step": 20, "ms": 1400]

    @Test func liveStartsWithTheChatOpen() {
        let m = caughtUp()
        #expect(m.liveStart() == nil)
        m.select("a_1")
        #expect(m.liveStart() == ["t": "live.start", "agent": "a_1", "conversation": "c_1"])
    }

    @Test func takesCoreLive() {
        let m = caughtUp()
        let start = Date(timeIntervalSince1970: 1_000)
        _ = m.liveFrame(live("listening", ["captions": false]), now: start)
        #expect(m.liveMode.on && m.liveMode.state == "listening" && m.liveMode.stateWords == "Listening")
        #expect(m.liveMode.name == "Builder" && m.liveMode.agent == "a_1" && m.liveMode.conversation == "c_1" && m.liveMode.started == 1_795_187_131_000)
        #expect(!m.liveMode.muted && !m.liveMode.captions && m.liveMode.since == start)
        // The mic's level, and the person's words; the time still counts from the first.
        m.take(live("listening", ["level": .number(0.62), "words": "Water the", "captions": true]))
        #expect(m.liveMode.level == 0.62 && m.liveMode.words == "Water the" && m.liveMode.since == start)
        // Show what's said is taken once: the Text button changes it after.
        #expect(!m.liveMode.captions)
        m.liveText()
        #expect(m.liveMode.captions)
        m.take(live("thinking", ["muted": true]))
        #expect(m.liveMode.stateWords == "Thinking" && m.liveMode.muted && m.liveMode.level == 0)
        // Mute says the other way.
        #expect(m.liveMute() == ["t": "live.mute", "on": false])
        #expect(liveClock(0) == "0:00" && liveClock(65) == "1:05" && liveClock(600) == "10:00")
    }

    @Test func wordsAreSaidAsThePiecePlays() throws {
        let m = caughtUp()
        let t0 = Date(timeIntervalSince1970: 2_000)
        _ = m.liveFrame(live("speaking", ["play": piece, "words": "Water the tomatoes?"]), now: t0)
        let p = try #require(m.liveMode.playing)
        #expect(p.file == "/tmp/voice/speech-1.wav" && p.words == ["Water", "the", "tomatoes."] && p.ms == 1400)
        #expect(p.spoken(t0) == 1 && p.spoken(t0 + 0.5) == 2 && p.spoken(t0 + 1) == 3)
        #expect(m.liveMode.caption(t0 + 0.5) == ("Water the", " tomatoes."))
        #expect(m.liveMode.caption(t0 + 1) == ("Water the tomatoes.", ""))
        // Once it has played, the core's words show again, all said.
        #expect(m.liveMode.caption(t0 + 2) == ("Water the tomatoes?", ""))
        // A frame with no piece, as a mute's, leaves it playing; stop ends it.
        m.take(live("speaking", ["muted": true]))
        #expect(m.liveMode.playing != nil)
        m.take(live("listening", ["stop": true]))
        #expect(m.liveMode.playing == nil)
    }

    @Test func theThreadFollowsTheLevel() {
        let m = caughtUp()
        let t0 = Date(timeIntervalSince1970: 3_000)
        _ = m.liveFrame(live("speaking", ["play": piece]), now: t0)
        // Each 20 ms of the piece, and its last past its levels, then nearly still once it ends.
        #expect(m.liveMode.target(t0) == 0.1 && m.liveMode.target(t0 + 0.03) == 0.6 && m.liveMode.target(t0 + 0.05) == 0.9)
        #expect(m.liveMode.target(t0 + 1) == 0.3 && m.liveMode.target(t0 + 1.5) == 0.03)
        // Listening: half the mic's level; muted or thinking, nearly still.
        _ = m.liveFrame(live("listening", ["level": .number(0.8)]), now: t0)
        #expect(m.liveMode.playing == nil && m.liveMode.target(t0) == 0.4)
        _ = m.liveFrame(live("listening", ["muted": true, "level": .number(0.8)]), now: t0)
        #expect(m.liveMode.target(t0) == 0.03)
        _ = m.liveFrame(live("thinking"), now: t0)
        #expect(m.liveMode.target(t0) == 0.03)
    }

    @Test func endingBringsTheChatBack() {
        let m = caughtUp()
        m.take(live("speaking", ["play": piece]))
        #expect(m.liveEnd() == ["t": "live.end"] && !m.liveMode.on && m.liveMode.playing == nil)
        // The core's ended after, and a new start counts from its own first frame.
        m.take(live("listening"))
        m.take(["t": "core.live", "state": "ended", "conversation": "c_1", "stop": true])
        #expect(!m.liveMode.on && m.liveMode == LiveState())
        let later = Date(timeIntervalSince1970: 9_000)
        _ = m.liveFrame(live("listening", ["started": 1_795_187_999_000]), now: later)
        #expect(m.liveMode.on && m.liveMode.since == later)
    }

    @Test func withNoEngineVoiceOpensOnItsTab() {
        let m = caughtUp()
        m.take(["t": "core.live", "state": "no_engine", "kind": "speech", "why": "Set up speech in Settings › Voice to hear an agent's replies."])
        #expect(!m.liveMode.on && m.voice.open && m.voice.openKind == "speech")
        #expect(m.notice == "Set up speech in Settings › Voice to hear an agent's replies.")
        // Dictation's own no_engine opens Dictation's.
        m.voice.open = false
        m.take(["t": "core.dictation", "state": "no_engine"])
        #expect(m.voice.open && m.voice.openKind == "dictation")
    }

    @Test func theLiveTabSendsLive() {
        let m = caughtUp()
        guard case .object(var v) = VoiceTests.listed, case .object(var s) = v["settings"]! else { return }
        s["live"] = ["barge_in": false, "pause": .number(1.2), "captions": true]
        v["settings"] = .object(s)
        v["pauses"] = [.number(0.5), .number(0.8), .number(1.2)]
        m.take(["t": "result", "id": "app.voice_get", "ok": true, "value": ["voice": .object(v)]])
        #expect(!m.voice.settings.bargeIn && m.voice.settings.pause == 1.2 && m.voice.settings.captions)
        #expect(m.voice.pauses == [0.5, 0.8, 1.2] && m.voice.tiles("live").isEmpty)
        #expect(pauseWords(0.5) == "After 0.5 s" && pauseWords(1.2) == "After 1.2 s")
        // Each row's change goes as `live`, and shows at once.
        let put = m.voiceLive(["barge_in": true])
        #expect(put["name"] == "voice.put" && put["args"] == ["live": ["barge_in": true]] && m.voice.settings.bargeIn)
        #expect(m.voiceLive(["pause": .number(0.5)])["args"] == ["live": ["pause": .number(0.5)]] && m.voice.settings.pause == 0.5)
        #expect(m.voiceLive(["captions": false])["args"] == ["live": ["captions": false]] && !m.voice.settings.captions)
    }

    @Test func theSampleTalks() async throws {
        let session = try await sampleSession()
        try await openChat("a_1", in: session)
        session.send(session.model.liveStart())
        #expect(await eventually(within: 5) { session.model.liveMode.on && session.model.liveMode.state == "listening" && session.model.liveMode.name == "Builder" })
        #expect(await eventually(within: 6) { session.model.liveMode.state == "speaking" && session.model.liveMode.playing != nil })
        session.send(session.model.liveMute())
        #expect(await eventually(within: 2) { session.model.liveMode.muted })
        session.endLive()
        #expect(!session.model.liveMode.on)
        // The turn is in the chat, as messages.
        #expect(await eventually(within: 2) { session.model.messages.contains { $0.text == Sample.liveAsked } })
    }
}
