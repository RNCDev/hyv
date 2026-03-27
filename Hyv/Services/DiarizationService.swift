import Foundation
import os

private let logger = Logger(subsystem: "com.hyv.app", category: "diarization")

/// Manages a long-running Python process that keeps the pyannote model in memory.
/// The process loads the model once on start(), then accepts JSON requests via stdin
/// and returns JSON responses via stdout. Progress updates come on stderr.
final class DiarizationService: @unchecked Sendable {
    private let pythonPath: String
    private let scriptPath: String
    private let hfToken: String
    private let cohereKey: String

    private var process: Process?
    private var stdinPipe: Pipe?
    private var stdoutPipe: Pipe?
    private var stderrPipe: Pipe?
    private var isReady = false

    /// Callback for progress updates from the Python process (set per-request)
    private var progressHandler: (@Sendable (String) -> Void)?

    init(pythonPath: String, scriptPath: String, hfToken: String, cohereKey: String) {
        self.pythonPath = pythonPath
        self.scriptPath = scriptPath
        self.hfToken = hfToken
        self.cohereKey = cohereKey
    }

    /// Start the Python server process and wait for the model to load.
    /// Call this once at app launch. The model stays in memory for all subsequent requests.
    func start(progress: @escaping @Sendable (String) -> Void) async throws {
        guard process == nil else {
            logger.debug("Python server already running")
            return
        }

        guard FileManager.default.isExecutableFile(atPath: pythonPath) else {
            logger.error("Python not found at: \(self.pythonPath)")
            throw DiarizationError.pythonNotFound(pythonPath)
        }
        guard FileManager.default.fileExists(atPath: scriptPath) else {
            logger.error("Script not found at: \(self.scriptPath)")
            throw DiarizationError.scriptNotFound(scriptPath)
        }

        logger.info("Starting Python server (model will load once)...")
        progress("Loading diarization model...")

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: pythonPath)
        proc.arguments = [scriptPath]

        // Pass HF_TOKEN via environment
        var env = ProcessInfo.processInfo.environment
        env["HF_TOKEN"] = hfToken
        proc.environment = env

        let stdin = Pipe()
        let stdout = Pipe()
        let stderr = Pipe()
        proc.standardInput = stdin
        proc.standardOutput = stdout
        proc.standardError = stderr

        self.stdinPipe = stdin
        self.stdoutPipe = stdout
        self.stderrPipe = stderr

        // Set up stderr handler for progress updates
        stderr.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }

            for line in text.components(separatedBy: .newlines) where !line.isEmpty {
                if line == "READY" {
                    self?.isReady = true
                    logger.info("Python server ready (model loaded)")
                } else if line.hasPrefix("PROGRESS:") {
                    let parts = line.dropFirst("PROGRESS:".count)
                    if let colonIndex = parts.firstIndex(of: ":") {
                        let message = String(parts[parts.index(after: colonIndex)...])
                        if !message.isEmpty {
                            self?.progressHandler?(message)
                        }
                    }
                }
            }
        }

        try proc.run()
        self.process = proc
        logger.info("Python server launched (PID: \(proc.processIdentifier))")

        // Wait for READY signal (model loaded)
        let startTime = CFAbsoluteTimeGetCurrent()
        while !isReady {
            try await Task.sleep(nanoseconds: 100_000_000) // 100ms
            let elapsed = CFAbsoluteTimeGetCurrent() - startTime
            if elapsed > 120 {
                logger.error("Python server timed out waiting for model load after 120s")
                stop()
                throw DiarizationError.processFailed(-1, "Model load timed out after 120s")
            }
            if proc.isRunning == false {
                let exitCode = proc.terminationStatus
                logger.error("Python server exited during model load (code \(exitCode))")
                self.process = nil
                throw DiarizationError.processFailed(exitCode, "Server exited during model load")
            }
        }

        let elapsed = CFAbsoluteTimeGetCurrent() - startTime
        logger.info("Model loaded in \(String(format: "%.1f", elapsed))s")
    }

    /// Send a processing request to the running Python server.
    func process(
        audioPath: URL,
        minSpeakers: Int = 1,
        maxSpeakers: Int = 10,
        progress: @escaping @Sendable (String) -> Void
    ) async throws -> TranscriptionResult {
        guard let proc = process, proc.isRunning else {
            logger.error("Python server not running — restarting")
            try await start(progress: progress)
            return try await self.process(audioPath: audioPath, minSpeakers: minSpeakers, maxSpeakers: maxSpeakers, progress: progress)
        }

        guard let stdinPipe = stdinPipe, let stdoutPipe = stdoutPipe else {
            throw DiarizationError.processFailed(-1, "Server pipes not available")
        }

        // Set progress handler for this request
        self.progressHandler = progress

        let startTime = CFAbsoluteTimeGetCurrent()
        logger.info("Sending request: \(audioPath.lastPathComponent), speakers: \(minSpeakers)-\(maxSpeakers)")

        // Build JSON request
        let request: [String: Any] = [
            "audio": audioPath.path,
            "cohere_key": cohereKey,
            "language": "en",
            "min_speakers": minSpeakers,
            "max_speakers": maxSpeakers
        ]

        let requestData = try JSONSerialization.data(withJSONObject: request)
        var requestLine = requestData
        requestLine.append(contentsOf: "\n".utf8)
        stdinPipe.fileHandleForWriting.write(requestLine)

        // Read response (one JSON line from stdout)
        let responseData: Data = try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                // Read until we get a complete line
                var accumulated = Data()
                let handle = stdoutPipe.fileHandleForReading

                while true {
                    let chunk = handle.availableData
                    if chunk.isEmpty {
                        // EOF — process probably died
                        continuation.resume(throwing: DiarizationError.processFailed(-1, "Server closed stdout"))
                        return
                    }
                    accumulated.append(chunk)
                    if let str = String(data: accumulated, encoding: .utf8), str.contains("\n") {
                        // Got a complete line — take the first line
                        if let lineEnd = str.firstIndex(of: "\n") {
                            let firstLine = String(str[str.startIndex..<lineEnd])
                            if let lineData = firstLine.data(using: .utf8) {
                                continuation.resume(returning: lineData)
                                return
                            }
                        }
                    }
                }
            }
        }

        let elapsed = CFAbsoluteTimeGetCurrent() - startTime
        self.progressHandler = nil

        // Parse response
        if let errorResponse = try? JSONDecoder().decode(ErrorResponse.self, from: responseData),
           let errorMsg = errorResponse.error {
            logger.error("Server returned error after \(String(format: "%.1f", elapsed))s: \(errorMsg)")
            throw DiarizationError.processFailed(1, errorMsg)
        }

        let result = try JSONDecoder().decode(TranscriptionResult.self, from: responseData)
        logger.info("Processing complete in \(String(format: "%.1f", elapsed))s: \(result.segments.count) segments, \(result.speakers.count) speakers")
        return result
    }

    /// Stop the Python server process.
    func stop() {
        guard let proc = process else { return }
        stdinPipe?.fileHandleForWriting.closeFile()
        stderrPipe?.fileHandleForReading.readabilityHandler = nil
        if proc.isRunning {
            proc.terminate()
            logger.info("Python server stopped")
        }
        self.process = nil
        self.stdinPipe = nil
        self.stdoutPipe = nil
        self.stderrPipe = nil
        self.isReady = false
        self.progressHandler = nil
    }

    deinit {
        stop()
    }
}

// MARK: - Error Response
private struct ErrorResponse: Decodable {
    let error: String?
}

// MARK: - Errors
enum DiarizationError: LocalizedError {
    case pythonNotFound(String)
    case scriptNotFound(String)
    case processFailed(Int32, String)
    case invalidOutput(String)

    var errorDescription: String? {
        switch self {
        case .pythonNotFound(let path):
            return "Python not found at \(path). Install Python 3.10+."
        case .scriptNotFound(let path):
            return "Script not found at \(path)"
        case .processFailed(let code, let msg):
            return "Processing failed (exit \(code)): \(msg)"
        case .invalidOutput(let detail):
            return "Invalid script output: \(detail)"
        }
    }
}
