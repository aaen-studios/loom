import Foundation
import Testing
@testable import Loom

/// The sound prefs as the phone keeps them and hands them to the core (PLAN.md, "Sounds"),
/// and the sounds it carries.
@Suite struct SoundTests {
    @Test func theDefaultsAreTheCards() {
        let p = SoundPrefs()
        #expect(p.mode == "loom" && p.volume == 0.8 && p.inFront && !p.quiet)
        #expect(p.on == ["reply": true, "waiting": true, "other": true, "routine": false, "failed": true, "sent": false, "dictation": true])
        #expect(p.from == "22:00" && p.to == "07:30")
    }

    @Test func prefsComeBackAsTheyWereKept() {
        var p = SoundPrefs()
        p.mode = "system"
        p.volume = 0.35
        p.on["sent"] = true
        p.quiet = true
        p.from = "23:15"
        let again = SoundPrefs(json: p.json)
        #expect(again == p)
        #expect(p.frame["t"].text == "app.sounds" && p.frame["sounds"]["cues"]["sent"].bool == true)
        // What the core says back whole reads the same.
        let said = SoundPrefs(json: ["mode": "off", "quiet": ["on": true, "from": "21:00"]])
        #expect(said.mode == "off" && said.quiet && said.from == "21:00" && said.to == "07:30")
    }

    @Test func aNotificationNamesItsSound() {
        #expect(Sounds.notification("") == nil)
        #expect(Sounds.notification("default") == .default)
        #expect(Sounds.notification("loom-reply") != nil)
    }

    @Test func theAppCarriesEverySound() {
        for file in ["loom-reply", "loom-waiting", "loom-routine", "loom-failed", "loom-sent", "loom-dictation-on", "loom-dictation-off"] {
            #expect(Bundle.main.url(forResource: file, withExtension: "wav") != nil, "\(file).wav")
        }
    }
}
