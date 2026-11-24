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
    /// agent `-open` opened once it's synced, and `-allow` answers that chat's questions
    /// yes. Each does what its screen's button does.
    private func fromLaunch() {
        let args = ProcessInfo.processInfo.arguments
        func value(_ flag: String) -> String? {
            guard let i = args.firstIndex(of: flag), i + 1 < args.count else { return nil }
            return args[i + 1]
        }
        if model.conn == .signedOut, let qr = value("-link"), launchDone.insert("-link").inserted {
            send(model.link(qr: qr, device: Session.device))
        }
        guard model.synced, let agent = value("-open"), model.selected == agent else { return }
        if let text = value("-say"), launchDone.insert("-say").inserted {
            send(model.send(text))
        }
        if args.contains("-allow") {
            for a in model.shownApprovals where launchDone.insert(a.call).inserted {
                send(model.approve(a.call, allow: true))
            }
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
    /// is going.
    func opened(_ url: URL) {
        signInPage.took(url)
    }

    // MARK: The system

    /// Someone started or stopped looking at the app. Coming back from the background,
    /// the socket has likely gone, so the core connects again at once.
    func active(_ active: Bool, returning: Bool) {
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

    func signOut() {
        send(["t": "logout"])
    }
}
