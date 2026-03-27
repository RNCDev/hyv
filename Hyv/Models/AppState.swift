import SwiftUI
import Combine

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
    private var meetingDebounceTask: Task<Void, Never>?
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
                        self.meetingDebounceTask?.cancel()
                        self.meetingDebounceTask = Task { @MainActor [weak self] in
                            try? await Task.sleep(nanoseconds: AppConstants.meetingDebounceDelay)
                            guard !Task.isCancelled else { return }
                            if self?.status == .idle {
                                self?.status = .meetingDetected
                            }
                        }
                    } else {
                        self.meetingDebounceTask?.cancel()
                        self.meetingDebounceTask = nil
                    }
                    self.meetingGoneTimer?.cancel()
                    self.meetingGoneTimer = nil
                } else {
                    self.meetingDebounceTask?.cancel()
                    self.meetingDebounceTask = nil
                    if self.status == .meetingDetected {
                        self.status = .idle
                        self.detectedApp = nil
                    } else if self.status == .recording {
                        self.meetingGoneTimer?.cancel()
                        self.meetingGoneTimer = Task { @MainActor [weak self] in
                            try? await Task.sleep(nanoseconds: AppConstants.meetingGoneTimeout)
                            guard !Task.isCancelled else { return }
                            self?.stopRecording()
                        }
                    }
                }
            }
    }

    // MARK: - Recording Control

    func startRecording() {
        guard status == .meetingDetected || status == .idle else { return }

        // Check tokens
        guard AppConfig.shared.hasValidApiKey else {
            status = .error("No Cohere API key. Set COHERE_TRIAL_API_KEY in .env")
            return
        }
        guard AppConfig.shared.hasValidHFToken else {
            status = .error("No HuggingFace token. Set HF_TOKEN in .env")
            return
        }

        // Check permission
        guard AudioCaptureService.hasPermission() else {
            AudioCaptureService.requestPermission()
            status = .error("Grant Screen Recording permission in System Settings")
            return
        }

        status = .recording
        recordingStartTime = Date()
        transcriptLines = []
        currentTranscriptPath = nil

        // Only label with auto-detected meeting apps, not background-running ones
        if detectedApp == nil {
            if let app = meetingDetector.runningMeetingApp, !app.runsInBackground {
                detectedApp = app.displayName
            }
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
                self.status = .error("Recording failed: \(error.localizedDescription)")
            }
        }
    }

    func stopRecording() {
        guard status == .recording else { return }

        Task {
            // Stop capture
            await audioCaptureService.stopCapture()
            try? await recorder.stopRecording()

            guard let audioURL = self.currentRecordingURL else {
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

    private func audioHasSpeech(at url: URL) -> Bool {
        guard let fileHandle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? fileHandle.close() }

        // Check file is larger than WAV header
        guard let endOffset = try? fileHandle.seekToEnd(), endOffset > 44 else { return false }

        // Seek past the 44-byte WAV header
        try? fileHandle.seek(toOffset: 44)

        let chunkSize = 65536 // 64KB
        var sumSquares: Double = 0
        var sampleCount: Int = 0

        while let chunk = try? fileHandle.read(upToCount: chunkSize), !chunk.isEmpty {
            chunk.withUnsafeBytes { buffer in
                let int16s = buffer.bindMemory(to: Int16.self)
                for i in 0..<int16s.count {
                    let normalized = Double(int16s[i]) / 32768.0
                    sumSquares += normalized * normalized
                }
                sampleCount += int16s.count
            }
        }

        guard sampleCount > 0 else { return false }
        let rms = (sumSquares / Double(sampleCount)).squareRoot()
        return rms > AppConstants.speechRMSThreshold
    }

    private func processRecording(audioURL: URL) {
        // Quick check: skip Python pipeline if audio is silent
        guard audioHasSpeech(at: audioURL) else {
            self.status = .error("Recording contained no speech audio")
            try? FileManager.default.removeItem(at: audioURL)
            self.recordingStartTime = nil
            return
        }

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

                // Handle empty result (no speech detected)
                if result.segments.isEmpty {
                    self.status = .error("No speech detected in recording")
                    try? FileManager.default.removeItem(at: audioURL)
                    self.recordingStartTime = nil
                    return
                }

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
                    let timeStr = TimeFormatting.formatElapsed(segment.start)
                    self.transcriptLines.append("[\(timeStr)] \(segment.speaker): \(segment.text)")
                }

                self.fileWriter.close()

                // Clean up WAV file after successful transcription
                try? FileManager.default.removeItem(at: audioURL)

                self.status = self.meetingDetector.isMeetingActive ? .meetingDetected : .idle
                self.recordingStartTime = nil

            } catch {
                // Fallback: transcribe without speaker labels if diarization fails
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
                    try? FileManager.default.removeItem(at: audioURL)

                    self.status = self.meetingDetector.isMeetingActive ? .meetingDetected : .idle
                } catch {
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
