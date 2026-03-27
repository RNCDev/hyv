import Foundation

final class TranscriptFileWriter {
    private var fileHandle: FileHandle?
    private(set) var filePath: URL?

    /// Open a new transcript file on the Desktop
    func open(meetingApp: String? = nil, duration: TimeInterval? = nil, speakerCount: Int? = nil) throws {
        let desktop = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first!
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd_HH-mm"
        let filename = "Hyv_Transcript_\(formatter.string(from: Date())).txt"
        let path = desktop.appendingPathComponent(filename)

        FileManager.default.createFile(atPath: path.path, contents: nil)
        fileHandle = try FileHandle(forWritingTo: path)
        filePath = path

        // Write header
        var header = "=== Hyv Transcript ===\n"
        header += "Date: \(Date().formatted(date: .long, time: .shortened))\n"
        if let app = meetingApp {
            header += "Meeting: \(app)\n"
        }
        if let dur = duration {
            header += "Duration: \(TimeFormatting.formatElapsed(dur))\n"
        }
        if let count = speakerCount {
            header += "Speakers: \(count)\n"
        }
        header += "========================\n\n"

        fileHandle?.write(header.data(using: .utf8)!)
        fileHandle?.synchronizeFile()
    }

    /// Append a speaker-labeled segment with timestamp in seconds from start
    func appendSegment(_ text: String, speaker: String, timestamp: TimeInterval) {
        guard let fileHandle = fileHandle else { return }

        let timeString = TimeFormatting.formatElapsed(timestamp)
        let line = "[\(timeString)] \(speaker): \(text)\n"

        fileHandle.seekToEndOfFile()
        if let data = line.data(using: .utf8) {
            fileHandle.write(data)
            fileHandle.synchronizeFile()
        }
    }

    /// Append an unlabeled segment (fallback when diarization unavailable)
    func appendSegment(_ text: String, timestamp: TimeInterval) {
        guard let fileHandle = fileHandle else { return }

        let timeString = TimeFormatting.formatElapsed(timestamp)
        let line = "[\(timeString)] \(text)\n"

        fileHandle.seekToEndOfFile()
        if let data = line.data(using: .utf8) {
            fileHandle.write(data)
            fileHandle.synchronizeFile()
        }
    }

    /// Close the file and write footer
    func close() {
        guard let fileHandle = fileHandle else { return }

        let footer = "\n=== End of Transcript ===\n"
        fileHandle.seekToEndOfFile()
        fileHandle.write(footer.data(using: .utf8)!)
        fileHandle.synchronizeFile()
        fileHandle.closeFile()

        self.fileHandle = nil
    }
}
