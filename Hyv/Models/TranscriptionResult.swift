import Foundation

struct TranscriptionResult: Codable {
    struct Segment: Codable, Identifiable {
        var id: String { "\(start)-\(end)-\(speaker)" }
        let start: Double
        let end: Double
        let speaker: String
        let text: String
    }
    let segments: [Segment]
    let speakers: [String]
    let empty: Bool?
}
