import Foundation

struct TranscriptSegment: Identifiable {
    let id = UUID()
    let text: String
    let timestamp: Date
    let relativeTime: TimeInterval // seconds from recording start
}
