import AVFoundation
import Foundation

/// The mic, for loom's dictation (PLAN.md, "Voice"): what it hears goes to the core as
/// 16-bit little-endian PCM at the mic's own rate and channels, in base64, about every
/// 100 ms (`voice.audio`), straight from the mic's thread, and the core does the rest.
final class Mic: @unchecked Sendable {
    /// Whether a mic is recording, so loom's own sounds leave the audio session as it is.
    @MainActor static var on = false

    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var held = Data()
    private var rate = 0
    private var channels = 1
    private let queue = DispatchQueue(label: "loom.mic")
    private var timer: DispatchSourceTimer?
    private let send: @Sendable (JSON) -> Void

    init(send: @escaping @Sendable (JSON) -> Void) {
        self.send = send
    }

    /// Starts recording. Gives whether it did: there may be no mic to hear.
    @MainActor
    func start() -> Bool {
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
            try session.setActive(true)
        } catch {
            return false
        }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else { return false }
        rate = Int(format.sampleRate.rounded())
        channels = Int(format.channelCount)
        input.installTap(onBus: 0, bufferSize: 2048, format: format) { [weak self] buffer, _ in
            self?.heard(buffer)
        }
        engine.prepare()
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            return false
        }
        Mic.on = true
        let t = DispatchSource.makeTimerSource(queue: queue)
        t.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100))
        t.setEventHandler { [weak self] in self?.flush() }
        t.resume()
        timer = t
        return true
    }

    /// Stops recording, once what it heard last has gone to the core.
    @MainActor
    func stop() {
        timer?.cancel()
        timer = nil
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        queue.sync { flush() }
        Mic.on = false
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private func heard(_ b: AVAudioPCMBuffer) {
        let pcm = Mic.pcm(b)
        guard !pcm.isEmpty else { return }
        lock.lock()
        held.append(pcm)
        lock.unlock()
    }

    private func flush() {
        lock.lock()
        let pcm = held
        held = Data()
        lock.unlock()
        guard !pcm.isEmpty else { return }
        send(["t": "voice.audio", "pcm": JSON(pcm.base64EncodedString()), "rate": JSON(rate), "channels": JSON(channels)])
    }

    /// A buffer as 16-bit little-endian PCM, its channels interleaved.
    static func pcm(_ b: AVAudioPCMBuffer) -> Data {
        let frames = Int(b.frameLength)
        let ch = Int(b.format.channelCount)
        guard frames > 0, ch > 0 else { return Data() }
        let interleaved = b.format.isInterleaved
        var out = [Int16](repeating: 0, count: frames * ch)
        if let f = b.floatChannelData {
            for i in 0..<frames {
                for c in 0..<ch {
                    let v = interleaved ? f[0][i * ch + c] : f[c][i]
                    out[i * ch + c] = Int16(max(-1, min(1, v)) * 32767).littleEndian
                }
            }
        } else if let s = b.int16ChannelData {
            for i in 0..<frames {
                for c in 0..<ch {
                    out[i * ch + c] = (interleaved ? s[0][i * ch + c] : s[c][i]).littleEndian
                }
            }
        } else {
            return Data()
        }
        return out.withUnsafeBufferPointer { Data(buffer: $0) }
    }
}

/// Speech the core wrote (`core.speak`): a reply read aloud or a voice's preview, each in
/// place of what was playing.
enum Speaker {
    @MainActor private static var playing: AVAudioPlayer?

    @MainActor
    static func play(_ path: String) {
        stop()
        guard let player = try? AVAudioPlayer(contentsOf: URL(fileURLWithPath: path)) else { return }
        if !Mic.on {
            try? AVAudioSession.sharedInstance().setCategory(.playback, mode: .spokenAudio)
            try? AVAudioSession.sharedInstance().setActive(true)
        }
        player.play()
        playing = player
    }

    @MainActor
    static func stop() {
        playing?.stop()
        playing = nil
    }
}
