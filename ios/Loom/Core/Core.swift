import Foundation

#if LOOM_CORE
import LoomCore
#endif

/// The core, as the app sees it: frames go in with `send`, and come out through the
/// closure it was started with, on the core's own thread. Only JSON crosses
/// (core/README.md, "Between the app and the core").
protocol CoreLink: AnyObject, Sendable {
    func send(_ frame: JSON)
    func stop()
}

/// What the core says, handed from its thread to the main actor in batches, so a burst of
/// deltas is one redraw rather than hundreds.
final class Inbox: @unchecked Sendable {
    private let lock = NSLock()
    private var frames: [JSON] = []
    private var scheduled = false
    private let deliver: @MainActor ([JSON]) -> Void

    init(deliver: @escaping @MainActor ([JSON]) -> Void) {
        self.deliver = deliver
    }

    func push(_ frame: JSON) {
        lock.lock()
        frames.append(frame)
        let schedule = !scheduled
        scheduled = true
        lock.unlock()
        guard schedule else { return }
        DispatchQueue.main.async { [self] in
            lock.lock()
            let batch = frames
            frames = []
            scheduled = false
            lock.unlock()
            MainActor.assumeIsolated { deliver(batch) }
        }
    }
}

#if LOOM_CORE
/// The Calx core, linked as a static library (LoomCore.xcframework), through its three C
/// functions.
final class CalxCore: CoreLink, @unchecked Sendable {
    private let handle: UnsafeMutablePointer<UInt8>
    private let inbox: Unmanaged<Inbox>
    private let lock = NSLock()
    private var stopped = false

    /// The Calx runtime starts once a process (PLAN.md, "The core's C boundary").
    private static let runtime: Void = calx_init()

    init(config: JSON, inbox: Inbox) {
        _ = CalxCore.runtime
        let ctx = Unmanaged.passRetained(inbox)
        self.inbox = ctx
        let bytes = config.encoded
        let raw = ctx.toOpaque().assumingMemoryBound(to: UInt8.self)
        handle = bytes.withUnsafeBytes { buf in
            loom_core_start(buf.bindMemory(to: UInt8.self).baseAddress, buf.count, { json, len, ctx in
                guard let json, let ctx else { return }
                let inbox = Unmanaged<Inbox>.fromOpaque(UnsafeRawPointer(ctx)).takeUnretainedValue()
                let data = Data(bytes: json, count: len)
                if let frame = try? JSON(parsing: data) {
                    inbox.push(frame)
                }
            }, raw)
        }
    }

    func send(_ frame: JSON) {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return }
        let bytes = frame.encoded
        bytes.withUnsafeBytes { buf in
            loom_core_send(handle, buf.bindMemory(to: UInt8.self).baseAddress, buf.count)
        }
    }

    /// Stops the core. It waits for the core's threads, so nothing comes after it.
    func stop() {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return }
        stopped = true
        loom_core_stop(handle)
        inbox.release()
    }

    deinit { stop() }
}
#endif
