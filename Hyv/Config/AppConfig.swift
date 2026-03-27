import Foundation

enum AppConstants {
    static let meetingDetectionPollInterval: TimeInterval = 30
    static let meetingGoneTimeout: UInt64 = 10_000_000_000 // 10 seconds in nanoseconds
    static let meetingDebounceDelay: UInt64 = 5_000_000_000 // 5 seconds in nanoseconds
    static let speechRMSThreshold: Double = 0.003
    static let processTimeoutMinutes: Int = 45
}

struct AppConfig {
    let cohereApiKey: String
    let huggingFaceToken: String
    let pythonPath: String
    let scriptDirectory: String

    static let shared: AppConfig = {
        // Try environment variables first
        let envCohere = ProcessInfo.processInfo.environment["COHERE_TRIAL_API_KEY"] ?? ""
        let envHF = ProcessInfo.processInfo.environment["HF_TOKEN"] ?? ""

        if !envCohere.isEmpty && !envHF.isEmpty {
            return AppConfig(
                cohereApiKey: envCohere,
                huggingFaceToken: envHF,
                pythonPath: detectPythonPath(),
                scriptDirectory: detectScriptDirectory()
            )
        }

        // Try .env file
        let searchPaths = [
            Bundle.main.bundlePath + "/../.env",
            FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".hyv/.env").path,
            FileManager.default.currentDirectoryPath + "/.env"
        ]

        for path in searchPaths {
            let env = loadEnvFile(path: path)
            let cohere = !envCohere.isEmpty ? envCohere : (env["COHERE_TRIAL_API_KEY"] ?? "")
            let hf = !envHF.isEmpty ? envHF : (env["HF_TOKEN"] ?? "")

            if !cohere.isEmpty || !hf.isEmpty {
                return AppConfig(
                    cohereApiKey: cohere,
                    huggingFaceToken: hf,
                    pythonPath: detectPythonPath(),
                    scriptDirectory: detectScriptDirectory()
                )
            }
        }

        return AppConfig(
            cohereApiKey: "",
            huggingFaceToken: "",
            pythonPath: detectPythonPath(),
            scriptDirectory: detectScriptDirectory()
        )
    }()

    private static func loadEnvFile(path: String) -> [String: String] {
        guard let contents = try? String(contentsOfFile: path, encoding: .utf8) else { return [:] }

        var env: [String: String] = [:]
        for line in contents.components(separatedBy: .newlines) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, !trimmed.hasPrefix("#") else { continue }
            if let equalsIndex = trimmed.firstIndex(of: "=") {
                let key = String(trimmed[trimmed.startIndex..<equalsIndex])
                let value = String(trimmed[trimmed.index(after: equalsIndex)...])
                    .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
                env[key] = value
            }
        }
        return env
    }

    private static func detectPythonPath() -> String {
        // Prefer project venv first
        let projectRoot = detectProjectRoot()
        let venvPython = projectRoot + "/venv/bin/python3"

        let candidates = [
            venvPython,
            "/opt/homebrew/bin/python3",
            "/usr/local/bin/python3",
            "/usr/bin/python3"
        ]
        for path in candidates {
            if FileManager.default.isExecutableFile(atPath: path) {
                return path
            }
        }
        return "python3"
    }

    private static func detectProjectRoot() -> String {
        // Walk up from the bundle to find the repo root (contains scripts/)
        var dir = Bundle.main.bundlePath
        for _ in 0..<6 {
            dir = (dir as NSString).deletingLastPathComponent
            if FileManager.default.fileExists(atPath: dir + "/scripts/diarize_and_transcribe.py") {
                return dir
            }
        }
        return FileManager.default.currentDirectoryPath
    }

    private static func detectScriptDirectory() -> String {
        return detectProjectRoot() + "/scripts"
    }

    var hasValidApiKey: Bool {
        !cohereApiKey.isEmpty
    }

    var hasValidHFToken: Bool {
        !huggingFaceToken.isEmpty
    }

    var diarizeScriptPath: String {
        scriptDirectory + "/diarize_and_transcribe.py"
    }

    /// Path to bundled models directory, if it exists
    var modelsDirectory: String? {
        let path = AppConfig.detectProjectRoot() + "/models"
        return FileManager.default.fileExists(atPath: path + "/cohere-transcribe") ? path : nil
    }
}
