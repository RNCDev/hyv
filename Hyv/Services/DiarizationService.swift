import Foundation

final class DiarizationService: @unchecked Sendable {
    private let pythonPath: String
    private let scriptPath: String
    private let hfToken: String
    private let cohereKey: String

    init(pythonPath: String, scriptPath: String, hfToken: String, cohereKey: String) {
        self.pythonPath = pythonPath
        self.scriptPath = scriptPath
        self.hfToken = hfToken
        self.cohereKey = cohereKey
    }

    /// Process an audio file: diarize speakers, transcribe each segment
    /// - Parameters:
    ///   - audioPath: Path to the WAV file
    ///   - minSpeakers: Minimum expected speakers
    ///   - maxSpeakers: Maximum expected speakers
    ///   - progress: Callback for progress updates (called on arbitrary thread)
    /// - Returns: TranscriptionResult with all speaker-labeled segments
    func process(
        audioPath: URL,
        minSpeakers: Int = 2,
        maxSpeakers: Int = 10,
        progress: @escaping @Sendable (String) -> Void
    ) async throws -> TranscriptionResult {
        // Validate python exists
        guard FileManager.default.isExecutableFile(atPath: pythonPath) else {
            throw DiarizationError.pythonNotFound(pythonPath)
        }

        // Validate script exists
        guard FileManager.default.fileExists(atPath: scriptPath) else {
            throw DiarizationError.scriptNotFound(scriptPath)
        }

        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async { [pythonPath, scriptPath, hfToken, cohereKey] in
                let process = Process()
                process.executableURL = URL(fileURLWithPath: pythonPath)
                var arguments = [
                    scriptPath,
                    "--audio", audioPath.path,
                    "--hf-token", hfToken,
                    "--local",
                    "--min-speakers", "\(minSpeakers)",
                    "--max-speakers", "\(maxSpeakers)"
                ]
                if !cohereKey.isEmpty {
                    arguments += ["--cohere-key", cohereKey]
                }
                if let modelsDir = AppConfig.shared.modelsDirectory {
                    arguments += ["--models-dir", modelsDir]
                }
                process.arguments = arguments

                let stdoutPipe = Pipe()
                let stderrPipe = Pipe()
                process.standardOutput = stdoutPipe
                process.standardError = stderrPipe

                // Read stderr for progress updates
                stderrPipe.fileHandleForReading.readabilityHandler = { handle in
                    let data = handle.availableData
                    guard !data.isEmpty, let line = String(data: data, encoding: .utf8) else { return }

                    // Parse progress lines
                    for rawLine in line.components(separatedBy: .newlines) where !rawLine.isEmpty {
                        if rawLine.hasPrefix("PROGRESS:") {
                            // Format: PROGRESS:<current>/<total>:<message>
                            let parts = rawLine.dropFirst("PROGRESS:".count)
                            if let colonIndex = parts.firstIndex(of: ":"),
                               let message = parts[parts.index(after: colonIndex)...].isEmpty ? nil : String(parts[parts.index(after: colonIndex)...]) {
                                progress(message.isEmpty ? String(parts) : message)
                            } else {
                                progress(String(parts))
                            }
                        }
                    }
                }

                do {
                    try process.run()
                } catch {
                    continuation.resume(throwing: DiarizationError.processFailed(-1, "Failed to launch: \(error.localizedDescription)"))
                    return
                }

                process.waitUntilExit()

                // Clean up handler
                stderrPipe.fileHandleForReading.readabilityHandler = nil

                let stdoutData = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
                let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()
                let stderrText = String(data: stderrData, encoding: .utf8) ?? ""

                guard process.terminationStatus == 0 else {
                    continuation.resume(throwing: DiarizationError.processFailed(
                        process.terminationStatus,
                        stderrText.isEmpty ? "Process exited with code \(process.terminationStatus)" : stderrText
                    ))
                    return
                }

                // Parse JSON output
                do {
                    // Check for error response
                    if let errorResponse = try? JSONDecoder().decode(ErrorResponse.self, from: stdoutData),
                       let errorMsg = errorResponse.error {
                        continuation.resume(throwing: DiarizationError.processFailed(1, errorMsg))
                        return
                    }

                    let result = try JSONDecoder().decode(TranscriptionResult.self, from: stdoutData)
                    continuation.resume(returning: result)
                } catch {
                    let rawOutput = String(data: stdoutData, encoding: .utf8) ?? "<non-utf8>"
                    continuation.resume(throwing: DiarizationError.invalidOutput("JSON decode failed: \(error.localizedDescription)\nRaw: \(rawOutput.prefix(500))"))
                }
            }
        }
    }
}

// MARK: - Error Response (for Python script errors)
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
