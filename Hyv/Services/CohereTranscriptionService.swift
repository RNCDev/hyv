import Foundation

protocol TranscriptionService {
    func transcribe(wavData: Data) async throws -> String
}

final class CohereTranscriptionService: TranscriptionService {
    private let apiKey: String
    private let endpoint = URL(string: "https://api.cohere.com/v2/audio/transcriptions")!
    private let session: URLSession
    private let maxRetries = 3

    init(apiKey: String) {
        self.apiKey = apiKey
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        self.session = URLSession(configuration: config)
    }

    func transcribe(wavData: Data) async throws -> String {
        var lastError: Error?

        for attempt in 0..<maxRetries {
            do {
                return try await sendRequest(wavData: wavData)
            } catch let error as TranscriptionError where error.isRetryable {
                lastError = error
                let delay = pow(2.0, Double(attempt)) // 1s, 2s, 4s
                try await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            } catch {
                throw error
            }
        }

        throw lastError ?? TranscriptionError.unknown
    }

    private func sendRequest(wavData: Data) async throws -> String {
        let boundary = UUID().uuidString

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")

        var body = Data()

        // Add "model" field
        body.appendMultipart(boundary: boundary, name: "model", value: "cohere-transcribe-03-2026")

        // Add "language" field
        body.appendMultipart(boundary: boundary, name: "language", value: "en")

        // Add "file" field
        body.appendMultipartFile(boundary: boundary, name: "file", filename: "chunk.wav", contentType: "audio/wav", data: wavData)

        // Final boundary
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)

        request.httpBody = body

        let (data, response) = try await session.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw TranscriptionError.invalidResponse
        }

        switch httpResponse.statusCode {
        case 200:
            let result = try JSONDecoder().decode(TranscriptionResponse.self, from: data)
            return result.text
        case 429:
            throw TranscriptionError.rateLimited
        case 401, 403:
            throw TranscriptionError.unauthorized
        case 500...599:
            throw TranscriptionError.serverError(httpResponse.statusCode)
        default:
            let message = String(data: data, encoding: .utf8) ?? "Unknown error"
            throw TranscriptionError.apiError(httpResponse.statusCode, message)
        }
    }
}

// MARK: - Models

private struct TranscriptionResponse: Decodable {
    let text: String
}

// MARK: - Errors

enum TranscriptionError: LocalizedError {
    case rateLimited
    case unauthorized
    case serverError(Int)
    case apiError(Int, String)
    case invalidResponse
    case unknown

    var isRetryable: Bool {
        switch self {
        case .rateLimited, .serverError: return true
        default: return false
        }
    }

    var errorDescription: String? {
        switch self {
        case .rateLimited: return "API rate limit exceeded. Retrying..."
        case .unauthorized: return "Invalid API key"
        case .serverError(let code): return "Server error (\(code))"
        case .apiError(let code, let msg): return "API error (\(code)): \(msg)"
        case .invalidResponse: return "Invalid response from server"
        case .unknown: return "Unknown transcription error"
        }
    }
}

// MARK: - Data Multipart Helpers

private extension Data {
    mutating func appendMultipart(boundary: String, name: String, value: String) {
        append("--\(boundary)\r\n".data(using: .utf8)!)
        append("Content-Disposition: form-data; name=\"\(name)\"\r\n\r\n".data(using: .utf8)!)
        append("\(value)\r\n".data(using: .utf8)!)
    }

    mutating func appendMultipartFile(boundary: String, name: String, filename: String, contentType: String, data fileData: Data) {
        append("--\(boundary)\r\n".data(using: .utf8)!)
        append("Content-Disposition: form-data; name=\"\(name)\"; filename=\"\(filename)\"\r\n".data(using: .utf8)!)
        append("Content-Type: \(contentType)\r\n\r\n".data(using: .utf8)!)
        append(fileData)
        append("\r\n".data(using: .utf8)!)
    }
}
