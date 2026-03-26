import SwiftUI

@MainActor
final class AppState: ObservableObject {
    enum Status: Equatable {
        case idle
        case meetingDetected
        case recording
        case error(String)

        static func == (lhs: Status, rhs: Status) -> Bool {
            switch (lhs, rhs) {
            case (.idle, .idle), (.meetingDetected, .meetingDetected), (.recording, .recording):
                return true
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

    var menuBarIcon: String {
        switch status {
        case .idle: return "waveform.slash"
        case .meetingDetected: return "waveform.circle"
        case .recording: return "waveform"
        case .error: return "exclamationmark.triangle"
        }
    }

    var statusText: String {
        switch status {
        case .idle: return "No meeting detected"
        case .meetingDetected: return "Meeting detected: \(detectedApp ?? "Unknown")"
        case .recording: return "Recording..."
        case .error(let msg): return "Error: \(msg)"
        }
    }
}
