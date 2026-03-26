import Foundation
import ScreenCaptureKit
import CoreMedia
import AVFoundation

final class AudioCaptureService: NSObject, @unchecked Sendable {
    let recorder: AudioFileRecorder
    private var stream: SCStream?
    private var isCapturing = false

    init(recorder: AudioFileRecorder) {
        self.recorder = recorder
        super.init()
    }

    /// Check if screen capture permission is granted
    static func hasPermission() -> Bool {
        CGPreflightScreenCaptureAccess()
    }

    /// Request screen capture permission
    static func requestPermission() {
        CGRequestScreenCaptureAccess()
    }

    func startCapture() async throws {
        guard !isCapturing else { return }

        // Get shareable content
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)

        guard let display = content.displays.first else {
            throw AudioCaptureError.noDisplay
        }

        // Create a filter that captures the entire display audio
        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])

        // Configure for audio-only capture
        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true
        config.channelCount = 1
        config.sampleRate = 16000

        // Minimize video overhead (SCStream requires some video config)
        config.width = 2
        config.height = 2
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1) // 1 fps minimum

        let stream = SCStream(filter: filter, configuration: config, delegate: nil)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: .global(qos: .userInitiated))

        try await stream.startCapture()
        self.stream = stream
        self.isCapturing = true
    }

    func stopCapture() async {
        guard isCapturing, let stream = stream else { return }

        do {
            try await stream.stopCapture()
        } catch {
            print("Warning: Error stopping capture: \(error)")
        }

        self.stream = nil
        self.isCapturing = false
    }
}

// MARK: - SCStreamOutput
extension AudioCaptureService: SCStreamOutput {
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }

        Task {
            await recorder.appendSamples(sampleBuffer: sampleBuffer)
        }
    }
}

// MARK: - Errors
enum AudioCaptureError: LocalizedError {
    case noDisplay
    case permissionDenied

    var errorDescription: String? {
        switch self {
        case .noDisplay: return "No display found for audio capture"
        case .permissionDenied: return "Screen capture permission is required"
        }
    }
}
