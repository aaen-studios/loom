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

    /// The core's config (core/README.md, "Config"): its cache in the app's own folder.
    private static func config() -> JSON {
        var config: [String: JSON] = ["data": JSON(dataFolder().path)]
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
            default:
                model.take(f)
            }
        }
        flush()
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

    /// Hands the core this phone's APNs token. The instance's side, the `push.register`
    /// command, comes with LOO-47; until then the token is kept here and nothing is sent.
    func registered(pushToken token: Data) {
        pushToken = token.map { String(format: "%02x", $0) }.joined()
        guard Session.pushRegisterReady, let pushToken else { return }
        send(["t": "cmd", "id": "app.push", "name": "push.register",
              "args": ["token": JSON(pushToken), "environment": JSON(Session.apnsEnvironment)]])
    }

    /// Turned on once the instance has `push.register` (LOO-47).
    static let pushRegisterReady = false

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
