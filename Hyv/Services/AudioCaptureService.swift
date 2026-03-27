import Foundation
import ScreenCaptureKit
import CoreMedia
import AVFoundation
import os

private let logger = Logger(subsystem: "com.hyv.app", category: "audio-capture")

final class AudioCaptureService: NSObject, @unchecked Sendable {
    let recorder: AudioFileRecorder
    private var stream: SCStream?
    private var captureSession: AVCaptureSession?
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
        guard !isCapturing else {
            logger.debug("startCapture called but already capturing")
            return
        }

        logger.info("Starting audio capture via ScreenCaptureKit + microphone")

        // --- System audio via ScreenCaptureKit ---
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)

        guard let display = content.displays.first else {
            logger.error("No display found for audio capture")
            throw AudioCaptureError.noDisplay
        }

        let filter = SCContentFilter(display: display, excludingApplications: [], exceptingWindows: [])

        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true
        config.channelCount = 1
        config.sampleRate = 16000
        config.width = 2
        config.height = 2
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)

        let stream = SCStream(filter: filter, configuration: config, delegate: nil)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: .global(qos: .userInitiated))
        try await stream.startCapture()
        self.stream = stream

        // --- Microphone via AVCaptureSession ---
        startMicCapture()

        self.isCapturing = true
        logger.info("Audio capture started (system audio + microphone, 16kHz mono)")
    }

    func stopCapture() async {
        guard isCapturing, let stream = stream else { return }

        do {
            try await stream.stopCapture()
            logger.info("System audio capture stopped")
        } catch {
            logger.error("Error stopping system audio capture: \(error.localizedDescription)")
        }

        captureSession?.stopRunning()
        captureSession = nil
        logger.info("Microphone capture stopped")

        self.stream = nil
        self.isCapturing = false
    }

    // MARK: - Microphone

    private func startMicCapture() {
        let session = AVCaptureSession()
        session.beginConfiguration()

        guard let mic = AVCaptureDevice.default(for: .audio) else {
            logger.error("No microphone found")
            return
        }

        do {
            let input = try AVCaptureDeviceInput(device: mic)
            guard session.canAddInput(input) else {
                logger.error("Cannot add microphone input to capture session")
                return
            }
            session.addInput(input)
        } catch {
            logger.error("Failed to create microphone input: \(error.localizedDescription)")
            return
        }

        let output = AVCaptureAudioDataOutput()
        output.setSampleBufferDelegate(self, queue: .global(qos: .userInitiated))
        guard session.canAddOutput(output) else {
            logger.error("Cannot add audio output to capture session")
            return
        }
        session.addOutput(output)
        session.commitConfiguration()
        session.startRunning()
        self.captureSession = session
        logger.info("Microphone capture started: \(mic.localizedName)")
    }
}

// MARK: - SCStreamOutput (system audio)

extension AudioCaptureService: SCStreamOutput {
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }
        Task {
            await recorder.appendSystemSamples(sampleBuffer: sampleBuffer)
        }
    }
}

// MARK: - AVCaptureAudioDataOutputSampleBufferDelegate (microphone)

extension AudioCaptureService: AVCaptureAudioDataOutputSampleBufferDelegate {
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        Task {
            await recorder.appendMicSamples(sampleBuffer: sampleBuffer)
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
