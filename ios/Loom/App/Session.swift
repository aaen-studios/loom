import AVFoundation
import Foundation
import Network
import Observation
import UIKit
import UserNotifications

/// The app's side of the core: starts it, hands it frames, and takes what it says onto
/// the main actor for the model. It also does what only the phone can, when the core
/// asks or the system says so: notifications, the clipboard, the network changing, the
/// app going to the background (PLAN.md, "The apps hold no logic").
@MainActor
@Observable
final class Session {
    let model = Model()
    /// Whether this session runs on sample frames rather than the core.
    let sample: Bool
    @ObservationIgnored private var core: CoreLink?
    @ObservationIgnored private var network: NWPathMonitor?
    @ObservationIgnored private var networkSeen = false

    /// Starts the core, or the sample core: for previews, a build without the core, or
    /// `-sample` at launch (`-signedOut` with it starts at the sign-in screen).
    init(sample: Bool? = nil, signedIn: Bool = true) {
        let args = ProcessInfo.processInfo.arguments
        #if LOOM_CORE
        let sample = sample ?? args.contains("-sample")
        #else
        let sample = true
        #endif
        self.sample = sample
        let inbox = Inbox { [weak self] frames in self?.receive(frames) }
        if sample {
            core = SampleCore(inbox: inbox, signedIn: signedIn && !args.contains("-signedOut"))
        } else {
            #if LOOM_CORE
            core = CalxCore(config: Session.config(), inbox: inbox)
            #endif
        }
        // The sound prefs, which the core decides every sound by.
        core?.send(SoundPrefs.load().frame)
        watchNetwork()
        flush()
    }

    /// The core's config (core/README.md, "Config"): its cache in the app's own folder, and
    /// the phone's time zone, which the core gives the instance in `hello`.
    private static func config() -> JSON {
        var config: [String: JSON] = ["data": JSON(dataFolder().path), "tz": JSON(TimeZone.current.identifier)]
        if let server = ProcessInfo.processInfo.environment["LOOM_SERVER"] {
            config["server"] = JSON(server)
        }
        return .object(config)
    }

    static func dataFolder() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let url = base.appendingPathComponent("Loom", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// This phone, as a login names it.
    static var device: JSON {
        ["name": JSON(UIDevice.current.name), "kind": "ios", "tz": JSON(TimeZone.current.identifier)]
    }

    func send(_ frame: JSON?) {
        guard let frame else { return }
        core?.send(frame)
        flush()
    }

    /// Makes the group chat in New group chat's form, named `name`, once the model says
    /// Make is on, and asks the model again once it has waited `Model.makeWait`, which
    /// turns Make on again if no answer came.
    func makeRoom(name: String) {
        guard let f = model.makeFormRoom(name: name) else { return }
        send(f)
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(Model.makeWait))
            self?.model.makeTimedOut(now: .now)
        }
    }

    /// Asks the core for whatever the model found out of date.
    func flush() {
        for f in model.fetches() {
            core?.send(f)
        }
    }

    private func receive(_ frames: [JSON]) {
        for f in frames {
            switch f["t"].text {
            case "core.device_ask":
                deviceAsk(f)
            case "core.notify":
                Notifications.show(f)
                model.take(f)
            case "core.sound":
                Sounds.play(f["file"].text, volume: f["volume"].double ?? 1)
            case "core.speak":
                if f["stop"].bool == true {
                    Speaker.stop()
                } else if let file = f["file"].string {
                    Speaker.play(file)
                }
                model.take(f)
            case "core.dictation":
                model.take(f)
                // Failed, or no engine to hear it: the mic stops, as the core already has.
                if ["failed", "no_engine"].contains(f["state"].text) { endMic() }
            case "core.live":
                model.take(f)
                if f["stop"].bool == true { Speaker.stop() }
                if let file = f["play"]["file"].string { Speaker.play(file) }
                liveMic()
            case "welcome":
                model.take(f)
                registerPush()
            default:
                model.take(f)
            }
        }
        if let s = model.toSignIn {
            model.toSignIn = nil
            openSignIn(s.url, server: s.server)
        }
        #if DEBUG
        fromLaunch()
        #endif
        flush()
    }

    #if DEBUG
    @ObservationIgnored private var launchDone: Set<String> = []

    /// What a debug build's launch arguments ask for, so tests/ios/live.calx can take the
    /// app through signing in, a reply and a question with no one to tap: `-link <qr>`
    /// joins with the code once the core is signed out, `-say <text>` sends it to the
    /// agent `-open` opened once it's synced, `-allow` answers that chat's questions
    /// yes, and `-signIn` taps Sign in on its sign-in cards. Each does what its screen's
    /// button does. `-callback <url>` is what the browser brings back to the `loom:`
    /// redirect, handed over as the system hands it, once a sign-in page is open.
    private func fromLaunch() {
        let args = ProcessInfo.processInfo.arguments
        func value(_ flag: String) -> String? {
            guard let i = args.firstIndex(of: flag), i + 1 < args.count else { return nil }
            return args[i + 1]
        }
        if model.conn == .signedOut, let qr = value("-link"), launchDone.insert("-link").inserted {
            send(model.link(qr: qr, device: Session.device))
        }
        // `-live` without `-open` opens the first agent's chat.
        if model.synced, value("-open") == nil, value("-live") != nil, let first = model.agents.first, launchDone.insert("-live open").inserted {
            model.openAgent = first.id
        }
        let open = value("-open") ?? (value("-live") != nil ? model.agents.first?.id : nil)
        guard model.synced, let agent = open, model.selected == agent else { return }
        if let text = value("-say"), launchDone.insert("-say").inserted {
            send(model.send(text))
        }
        // `-live speaking` or `-live listening` taps Live in that chat, and the sample core
        // holds live mode in that state, for screenshots of it.
        if value("-live") != nil, launchDone.insert("-live").inserted {
            startLive()
        }
        if args.contains("-allow") {
            for a in model.shownApprovals where launchDone.insert(a.call).inserted {
                send(model.approve(a.call, allow: true))
            }
        }
        // `-view` opens the chat's last pictures in the viewer, for screenshots of it.
        if args.contains("-view"), !launchDone.contains("-view"),
           let stack = model.shown.reversed().lazy.map({ $0.blocks.compactMap(\.file).filter(\.isImage) }).first(where: { !$0.isEmpty }) {
            launchDone.insert("-view")
            model.view(stack)
        }
        if args.contains("-signIn") {
            for c in model.shown.flatMap(\.blocks).compactMap(\.card) where c.type == "sign_in" && c.signedAs == nil && launchDone.insert("-signIn \(c.server)").inserted {
                signIn(c.server)
            }
        }
        if signInPage.server != nil, let url = value("-callback").flatMap(URL.init(string:)), launchDone.insert("-callback").inserted {
            opened(url)
        }
    }
    #endif

    // MARK: Signing in to MCP servers

    @ObservationIgnored private let signInPage = SignInPage()

    /// Signs in to the person's MCP server `server` from its card (PLAN.md, "Signing in
    /// to MCP servers"): the instance gives its sign-in page, which opens over the app,
    /// and what the browser brings back to the `loom:` redirect goes to the instance,
    /// which swaps it for tokens.
    func signIn(_ server: String) {
        send(model.signIn(server, redirect: SignInPage.redirect))
    }

    private func openSignIn(_ url: URL, server: String) {
        signInPage.open(url, server: server) { [weak self] server, ended in
            guard let self else { return }
            switch ended {
            case .back(let query): self.send(self.model.signedIn(server, query: query))
            case .failed(let why): self.model.signInEnded(server, error: why)
            }
        }
    }

    /// A `loom:` address the system opened the app with: the end of a sign-in, when one
    /// is going, or a code that adds this phone (`loom://add?…`, as a QR code the camera
    /// read), at the step where the person says yes on the device that showed it.
    func opened(_ url: URL) {
        let text = url.absoluteString
        if text.hasPrefix("loom://add?") || text.hasPrefix("loom://link?") {
            if model.conn == .signedOut {
                send(model.link(qr: text, device: Session.device))
            } else {
                model.notice = "This phone is signed in already. Sign out in Settings to add it to another account."
            }
            return
        }
        signInPage.took(url)
    }

    // MARK: The system

    /// Someone started or stopped looking at the app. Coming back from the background,
    /// the socket has likely gone, so the core connects again at once.
    func active(_ active: Bool, returning: Bool) {
        // The mic doesn't hear in the background, so dictation and live mode end.
        if !active {
            stopDictation()
            endLive()
        }
        send(["t": "app.active", "active": JSON(active)])
        if active && returning {
            send(["t": "app.wake"])
        }
    }

    private func watchNetwork() {
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                // The first update is how things are, not a change.
                if self.networkSeen {
                    self.send(["t": "app.network"])
                }
                self.networkSeen = true
            }
        }
        monitor.start(queue: DispatchQueue(label: "loom.network"))
        network = monitor
    }

    /// Does what the core asks of this phone for an agent, once the core has checked the
    /// call is allowed. An iPhone app can't see the rest of the screen, so a screenshot
    /// is refused.
    private func deviceAsk(_ ask: JSON) {
        let call = ask["call"]
        func answer(_ ok: Bool, _ output: String) {
            send(["t": "device.answer", "call": call, "ok": JSON(ok), "output": JSON(output)])
        }
        switch ask["tool"].text {
        case "read_clipboard":
            answer(true, UIPasteboard.general.string ?? "The clipboard has no text.")
        case "write_clipboard":
            UIPasteboard.general.string = ask["args"]["text"].text
            answer(true, "Copied.")
        case "screenshot":
            answer(false, "An iPhone app can only see its own screen, so it can't take a screenshot.")
        default:
            answer(false, "This app has no such tool.")
        }
    }

    // MARK: Push

    /// The APNs device token, once the system gives it.
    var pushToken: String?

    /// Keeps this phone's APNs token, and hands it to the instance if it's connected.
    func registered(pushToken token: Data) {
        pushToken = token.map { String(format: "%02x", $0) }.joined()
        if model.me != nil { registerPush() }
    }

    /// `push.register` with the APNs token, on each `welcome`, since the core sends a
    /// command only while it's connected. The core seals it for the push relay when the
    /// instance pushes through one.
    private func registerPush() {
        guard !sample, let pushToken else { return }
        send(["t": "cmd", "id": "app.push", "name": "push.register",
              "args": ["token": JSON(pushToken), "env": JSON(Session.apnsEnvironment)]])
    }

    static var apnsEnvironment: String {
        #if DEBUG
        "development"
        #else
        "production"
        #endif
    }

    // MARK: Sounds

    @ObservationIgnored private var frontSent = ""

    /// Tells the core which conversation is in front of the person, silent while they look
    /// at it (`app.front`).
    func front(_ conversation: String) {
        guard conversation != frontSent else { return }
        frontSent = conversation
        send(["t": "app.front", "conversation": JSON(conversation)])
    }

    /// A chat closed: none is in front, unless another chat is already.
    func left(_ conversation: String) {
        if conversation == frontSent { front("") }
    }

    func signOut() {
        send(["t": "logout"])
    }

    // MARK: Voice

    @ObservationIgnored private var mic: Mic?
    /// Whether the composer's mic is wanted on: pressed and not let go yet, while the
    /// system asks whether loom may hear it.
    @ObservationIgnored private var micWanted = false
    /// Whether the mic records for a dictation now.
    private(set) var listening = false

    /// The composer's mic: loom's dictation starts (`voice.listen`) and the mic's PCM goes
    /// to the core. The first time, the system asks whether loom may hear the mic.
    func startDictation() {
        guard mic == nil, liveMicOn == nil else { return }
        micWanted = true
        mayHear { [weak self] in
            guard let self, self.micWanted else { return }
            self.beginDictation()
        }
    }

    /// Whether loom may hear the mic: `then` once it may, the first time once the system
    /// has asked, or why it can't.
    private func mayHear(_ then: @escaping @MainActor () -> Void) {
        switch AVAudioApplication.shared.recordPermission {
        case .granted:
            then()
        case .undetermined:
            AVAudioApplication.requestRecordPermission { [weak self] ok in
                Task { @MainActor in
                    guard let self else { return }
                    if ok {
                        then()
                    } else {
                        self.micWanted = false
                        self.model.notice = "loom can't hear the mic. Turn it on in Settings › Apps › loom."
                    }
                }
            }
        default:
            micWanted = false
            model.notice = "loom can't hear the mic. Turn it on in Settings › Apps › loom."
        }
    }

    /// The mic let go, or tapped again: the last words come, and the dictation is done.
    func stopDictation() {
        micWanted = false
        guard mic != nil else { return }
        endMic()
        send(["t": "voice.stop"])
    }

    /// The message sent while dictating (LOO-448): the mic stops, and the dictation ends
    /// with no more words, here and in the core.
    func dictationSent() {
        micWanted = false
        endMic()
        send(model.dictationSent())
    }

    private func beginDictation() {
        micWanted = false
        Speaker.stop()
        model.dictationStarted()
        send(["t": "voice.listen"])
        let core = self.core
        let m = Mic { frame in core?.send(frame) }
        guard m.start() else {
            send(["t": "voice.stop"])
            model.notice = "The mic didn't start."
            return
        }
        mic = m
        listening = true
    }

    private func endMic() {
        mic?.stop()
        mic = nil
        listening = false
    }

    // MARK: Live mode

    /// The mic while live mode hears it.
    @ObservationIgnored private var liveMicOn: Mic?

    /// Live in the composer: live mode starts with the chat open's agent (`live.start`),
    /// and once the core says it's on, the mic records for it.
    func startLive() {
        stopDictation()
        mayHear { [weak self] in
            guard let self else { return }
            Speaker.stop()
            self.send(self.model.liveStart())
        }
    }

    /// Mute in live mode: the mic stops recording until it's let go.
    func muteLive() {
        send(model.liveMute())
    }

    /// End in live mode: the chat comes back, speech stops and the mic lets go.
    func endLive() {
        guard model.liveMode.on else { return }
        send(model.liveEnd())
        Speaker.stop()
        liveMic()
    }

    /// The mic records while live mode is on and not muted, as the core last said.
    private func liveMic() {
        let live = model.liveMode
        if live.on && !live.muted {
            // Live asked whether loom may hear the mic before it started. The sample core
            // hears nothing, so it isn't recorded for, and a simulator with no mic to hear
            // draws live mode all the same.
            guard liveMicOn == nil, !sample, AVAudioApplication.shared.recordPermission == .granted else { return }
            let core = self.core
            let m = Mic { frame in core?.send(frame) }
            guard m.start() else {
                model.notice = "The mic didn't start."
                send(model.liveEnd())
                Speaker.stop()
                return
            }
            liveMicOn = m
        } else {
            liveMicOn?.stop(keep: live.on)
            liveMicOn = nil
            if !live.on && mic == nil { Mic.release() }
        }
    }
}
