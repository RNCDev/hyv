import SwiftUI
import Combine
import os

private let logger = Logger(subsystem: "com.hyv.app", category: "app-state")

@MainActor
final class AppState: ObservableObject {
    enum Status: Equatable {
        case idle
        case meetingDetected
        case recording
        case processing(String)
        case error(String)

        static func == (lhs: Status, rhs: Status) -> Bool {
            switch (lhs, rhs) {
            case (.idle, .idle), (.meetingDetected, .meetingDetected), (.recording, .recording):
                return true
            case let (.processing(a), .processing(b)):
                return a == b
            case let (.error(a), .error(b)):
                return a == b
            default:
                return false
            }
        }
    }

    @Published var status: Status = .idle
    @Published var detectedApp: String? = nil
    @Published var transcriptLines: [String] = []
    @Published var recordingStartTime: Date? = nil
    @Published var currentTranscriptPath: String? = nil

    // Services
    let meetingDetector = MeetingDetectorService()
    private var recorder = AudioFileRecorder()
    private var audioCaptureService: AudioCaptureService
    private let diarizationService: DiarizationService
    private let fileWriter = TranscriptFileWriter()

    // Pipeline
    private var processingTask: Task<Void, Never>?
    private var detectorCancellable: AnyCancellable?
    private var meetingGoneTimer: Task<Void, Never>?
    private var currentRecordingURL: URL?

    var menuBarIcon: String {
        switch status {
        case .idle: return "waveform.slash"
        case .meetingDetected: return "waveform.circle"
        case .recording: return "waveform"
        case .processing: return "gear"
        case .error: return "exclamationmark.triangle"
        }
    }

    var statusText: String {
        switch status {
        case .idle: return "No meeting detected"
        case .meetingDetected: return "Meeting detected: \(detectedApp ?? "Unknown")"
        case .recording: return "Recording..."
        case .processing(let msg): return msg
        case .error(let msg): return "Error: \(msg)"
        }
    }

    init() {
        let config = AppConfig.shared
        self.recorder = AudioFileRecorder()
        self.audioCaptureService = AudioCaptureService(recorder: recorder)
        self.diarizationService = DiarizationService(
            pythonPath: config.pythonPath,
            scriptPath: config.diarizeScriptPath,
            hfToken: config.huggingFaceToken,
            cohereKey: config.cohereApiKey
        )
        setupMeetingDetection()
        setupTerminationHandler()
    }

    // MARK: - Meeting Detection

    private func setupMeetingDetection() {
        meetingDetector.start()

        detectorCancellable = meetingDetector.$detectedApp
            .receive(on: DispatchQueue.main)
            .sink { [weak self] app in
                guard let self = self else { return }
                if let app = app {
                    self.detectedApp = app.displayName
                    if self.status == .idle {
                        logger.info("Status: idle → meetingDetected (\(app.displayName))")
                        self.status = .meetingDetected
                    }
                    self.meetingGoneTimer?.cancel()
                    self.meetingGoneTimer = nil
                } else {
                    if self.status == .meetingDetected {
                        logger.info("Status: meetingDetected → idle (app closed)")
                        self.status = .idle
                        self.detectedApp = nil
                    } else if self.status == .recording {
                        logger.info("Meeting app closed during recording — auto-stop in 10s")
                        self.meetingGoneTimer?.cancel()
                        self.meetingGoneTimer = Task { @MainActor [weak self] in
                            try? await Task.sleep(nanoseconds: 10_000_000_000)
                            guard !Task.isCancelled else { return }
                            self?.stopRecording()
                        }
                    }
                }
            }
    }

    // MARK: - Recording Control

    func startRecording() {
        guard status == .meetingDetected || status == .idle else {
            logger.warning("startRecording called in unexpected state: \(String(describing: self.status))")
            return
        }

        // Check tokens
        guard AppConfig.shared.hasValidApiKey else {
            logger.error("Cannot start recording: missing Cohere API key")
            status = .error("No Cohere API key. Set COHERE_TRIAL_API_KEY in .env")
            return
        }
        guard AppConfig.shared.hasValidHFToken else {
            logger.error("Cannot start recording: missing HuggingFace token")
            status = .error("No HuggingFace token. Set HF_TOKEN in .env")
            return
        }

        // Check permission
        guard AudioCaptureService.hasPermission() else {
            logger.error("Cannot start recording: missing Screen Recording permission")
            AudioCaptureService.requestPermission()
            status = .error("Grant Screen Recording permission in System Settings")
            return
        }

        logger.info("Status: → recording (app: \(self.detectedApp ?? "unknown"))")
        status = .recording
        recordingStartTime = Date()
        transcriptLines = []
        currentTranscriptPath = nil

        // Use any running meeting app for transcript labeling (including background apps)
        if detectedApp == nil {
            detectedApp = meetingDetector.runningMeetingApp?.displayName
        }

        // Create fresh recorder
        recorder = AudioFileRecorder()
        audioCaptureService = AudioCaptureService(recorder: recorder)

        Task {
            do {
                let audioURL = try await recorder.startRecording()
                self.currentRecordingURL = audioURL
                try await audioCaptureService.startCapture()
            } catch {
                logger.error("Recording setup failed: \(error.localizedDescription)")
                self.status = .error("Recording failed: \(error.localizedDescription)")
            }
        }
    }

    func stopRecording() {
        guard status == .recording else {
            logger.warning("stopRecording called in unexpected state: \(String(describing: self.status))")
            return
        }

        logger.info("Stopping recording")
        Task {
            // Stop capture
            await audioCaptureService.stopCapture()
            do {
                try await recorder.stopRecording()
            } catch {
                logger.error("Error stopping recorder: \(error.localizedDescription)")
            }

            guard let audioURL = self.currentRecordingURL else {
                logger.error("stopRecording: no currentRecordingURL set")
                self.status = .error("No recording file found")
                return
            }

            // Transition to processing
            self.status = .processing("Starting post-processing...")
            self.meetingGoneTimer?.cancel()
            self.meetingGoneTimer = nil

            // Start post-processing
            self.processRecording(audioURL: audioURL)
        }
    }

    // MARK: - Post-Processing

    private func processRecording(audioURL: URL) {
        logger.info("Status: → processing (\(audioURL.lastPathComponent))")
        processingTask = Task {
            do {
                let result = try await diarizationService.process(
                    audioPath: audioURL,
                    progress: { [weak self] message in
                        Task { @MainActor in
                            self?.status = .processing(message)
                        }
                    }
                )

                // Calculate duration from last segment
                let duration = result.segments.last.map { $0.end } ?? 0

                // Open transcript file with metadata
                try self.fileWriter.open(
                    meetingApp: self.detectedApp,
                    duration: duration,
                    speakerCount: result.speakers.count
                )
                self.currentTranscriptPath = self.fileWriter.filePath?.path

                // Write segments incrementally
                for segment in result.segments {
                    self.fileWriter.appendSegment(
                        segment.text,
                        speaker: segment.speaker,
                        timestamp: segment.start
                    )
                    let timeStr = self.formatElapsed(segment.start)
                    self.transcriptLines.append("[\(timeStr)] \(segment.speaker): \(segment.text)")
                }

                self.fileWriter.close()

                // Clean up WAV file after successful transcription
                do {
                    try FileManager.default.removeItem(at: audioURL)
                    logger.info("WAV file deleted: \(audioURL.lastPathComponent)")
                } catch {
                    logger.error("Failed to delete WAV file: \(error.localizedDescription)")
                }

                let finalStatus: String = self.meetingDetector.isMeetingActive ? "meetingDetected" : "idle"
                logger.info("Processing complete → \(finalStatus)")
                self.status = self.meetingDetector.isMeetingActive ? .meetingDetected : .idle
                self.recordingStartTime = nil

            } catch {
                // Fallback: transcribe without speaker labels if diarization fails
                logger.error("Diarization failed: \(error.localizedDescription) — falling back to unlabeled transcription")
                self.status = .processing("Diarization failed, transcribing without speaker labels...")
                do {
                    let wavData = try Data(contentsOf: audioURL)
                    let transcriber = CohereTranscriptionService(apiKey: AppConfig.shared.cohereApiKey)
                    let text = try await transcriber.transcribe(wavData: wavData)

                    try self.fileWriter.open(meetingApp: self.detectedApp)
                    self.currentTranscriptPath = self.fileWriter.filePath?.path
                    self.fileWriter.appendSegment(text, timestamp: 0)
                    self.transcriptLines.append("[00:00] \(text)")
                    self.fileWriter.close()

                    // Clean up WAV file after successful fallback
                    do {
                        try FileManager.default.removeItem(at: audioURL)
                        logger.info("WAV file deleted after fallback: \(audioURL.lastPathComponent)")
                    } catch {
                        logger.error("Failed to delete WAV file after fallback: \(error.localizedDescription)")
                    }

                    let finalStatus: String = self.meetingDetector.isMeetingActive ? "meetingDetected" : "idle"
                    logger.info("Fallback transcription complete → \(finalStatus)")
                    self.status = self.meetingDetector.isMeetingActive ? .meetingDetected : .idle
                } catch {
                    logger.error("Fallback transcription also failed: \(error.localizedDescription)")
                    self.fileWriter.close()
                    self.status = .error("Processing failed: \(error.localizedDescription)")
                }
                self.recordingStartTime = nil
            }
        }
    }

    func openTranscript() {
        guard let path = currentTranscriptPath else { return }
        NSWorkspace.shared.open(URL(fileURLWithPath: path))
    }

    // MARK: - Helpers

    private func formatElapsed(_ interval: TimeInterval) -> String {
        let hours = Int(interval) / 3600
        let minutes = (Int(interval) % 3600) / 60
        let seconds = Int(interval) % 60
        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        }
        return String(format: "%02d:%02d", minutes, seconds)
    }

    // MARK: - Cleanup

    private func setupTerminationHandler() {
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.processingTask?.cancel()
                self?.fileWriter.close()
            }
        }
    }
}
