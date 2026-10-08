import AVFoundation
import CoreGraphics
import QuartzCore
import UIKit

/// One running title: the emulator's loop on a thread of its own, its frames
/// published for the screen, and its sound pulled by the audio engine.
final class Emulator: ObservableObject {
    @Published private(set) var frame: CGImage?
    @Published private(set) var message: String?

    private let lock = NSLock()
    private var running = false
    private var thread: Thread?
    private let audio = AudioOutput()

    /// The largest frame the emulator draws is a handset panel; this is room
    /// for anything up to 1024x1024.
    private static let frameCapacity = 1024 * 1024 * 4
    /// How often the loop runs a tick when the title does not say otherwise.
    private static let interval: Double = 1.0 / 60.0

    private var isRunning: Bool {
        lock.lock()
        defer { lock.unlock() }
        return running
    }

    func start(game: GameFile) {
        guard !isRunning else { return }

        let data: Data
        do {
            data = try Data(contentsOf: game.url)
        } catch {
            message = "게임 파일을 읽을 수 없습니다: \(error.localizedDescription)"
            return
        }

        // The speed this title was last played at; the runner keeps it from
        // the first tick.
        wie_set_speed(GameSpeed.get(game))

        let runtimeDirectory = Library.dataDirectory.path
        let failure = data.withUnsafeBytes { buffer -> String? in
            let bytes = buffer.bindMemory(to: UInt8.self)
            return takeString(wie_start(bytes.baseAddress, bytes.count, runtimeDirectory, UIDevice.current.model))
        }
        if let failure {
            message = failure
            return
        }

        lock.lock()
        running = true
        lock.unlock()

        audio.start()
        let thread = Thread { [weak self] in self?.loop() }
        thread.name = "WIE emulator"
        thread.qualityOfService = .userInteractive
        self.thread = thread
        thread.start()
    }

    func stop() {
        lock.lock()
        let wasRunning = running
        running = false
        lock.unlock()

        if wasRunning {
            audio.stop()
            wie_stop()
        }
    }

    func key(_ index: Int32, pressed: Bool) {
        wie_key(index, pressed)
    }

    private func loop() {
        var pixels = [UInt8](repeating: 0, count: Self.frameCapacity)

        while isRunning {
            let started = CACurrentMediaTime()

            if let failure = takeString(wie_tick(16)) {
                finish(with: failure)
                return
            }

            var width: UInt32 = 0
            var height: UInt32 = 0
            let painted = pixels.withUnsafeMutableBufferPointer { buffer in
                wie_take_frame(buffer.baseAddress, buffer.count, &width, &height)
            }
            if painted, let image = Self.image(from: pixels, width: Int(width), height: Int(height)) {
                DispatchQueue.main.async { [weak self] in self?.frame = image }
            }

            var vibration: UInt32 = 0
            while wie_take_vibration(&vibration) {
                DispatchQueue.main.async {
                    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                }
            }

            if !wie_running() {
                finish(with: takeString(wie_last_error()) ?? "게임이 종료되었습니다.")
                return
            }

            // Sleep out the rest of the frame, or less when the title's next
            // timer comes sooner.
            let hint = wie_sleep_hint_ms()
            let target = hint >= 0 ? min(Double(hint) / 1000, Self.interval) : Self.interval
            let elapsed = CACurrentMediaTime() - started
            if elapsed < target {
                Thread.sleep(forTimeInterval: target - elapsed)
            }
        }
    }

    private func finish(with text: String) {
        lock.lock()
        running = false
        lock.unlock()
        audio.stop()
        DispatchQueue.main.async { [weak self] in self?.message = text }
    }

    private static func image(from pixels: [UInt8], width: Int, height: Int) -> CGImage? {
        let length = width * height * 4
        guard width > 0, height > 0, length <= pixels.count else { return nil }
        let bytes = Data(pixels[0..<length])
        guard let provider = CGDataProvider(data: bytes as CFData) else { return nil }
        return CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        )
    }
}

/// Plays the emulator's mixer: the audio engine asks for samples on its own
/// clock, and each request renders that many from the title's synthesiser.
final class AudioOutput {
    private let engine = AVAudioEngine()
    private var node: AVAudioSourceNode?

    func start() {
        guard node == nil else { return }

        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .default, options: [.mixWithOthers])
        try? session.setActive(true)

        guard let format = AVAudioFormat(standardFormatWithSampleRate: 44100, channels: 2) else { return }
        var scratch = [Int16](repeating: 0, count: 8192)

        let node = AVAudioSourceNode(format: format) { _, _, frameCount, bufferList -> OSStatus in
            let frames = Int(frameCount)
            if scratch.count < frames * 2 {
                scratch = [Int16](repeating: 0, count: frames * 2)
            }
            let rendered = scratch.withUnsafeMutableBufferPointer { buffer in
                wie_render_audio(buffer.baseAddress, frames)
            }

            let buffers = UnsafeMutableAudioBufferListPointer(bufferList)
            guard buffers.count >= 2,
                  let left = buffers[0].mData?.assumingMemoryBound(to: Float.self),
                  let right = buffers[1].mData?.assumingMemoryBound(to: Float.self)
            else { return noErr }

            for i in 0..<frames {
                if i < rendered {
                    left[i] = Float(scratch[2 * i]) / 32768
                    right[i] = Float(scratch[2 * i + 1]) / 32768
                } else {
                    left[i] = 0
                    right[i] = 0
                }
            }
            return noErr
        }

        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
        self.node = node
        try? engine.start()
    }

    func stop() {
        engine.stop()
        if let node {
            engine.detach(node)
        }
        node = nil
    }
}
