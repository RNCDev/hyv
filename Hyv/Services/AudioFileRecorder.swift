import Foundation
import CoreMedia
import AVFoundation

actor AudioFileRecorder {
    private let sampleRate: Int = 16000
    private let bitsPerSample: Int = 16
    private let channels: Int = 1

    private var fileHandle: FileHandle?
    private(set) var recordingURL: URL?
    private var totalDataBytes: UInt32 = 0

    private var recordingsDirectory: URL {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Hyv/recordings")
    }

    /// Start recording to a new WAV file. Returns the file URL.
    func startRecording() throws -> URL {
        // Create directory if needed
        try FileManager.default.createDirectory(at: recordingsDirectory, withIntermediateDirectories: true)

        // Generate filename
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withFullDate, .withFullTime]
        let filename = "recording_\(formatter.string(from: Date())).wav"
        let url = recordingsDirectory.appendingPathComponent(filename)

        // Create file and open handle
        FileManager.default.createFile(atPath: url.path, contents: nil)
        fileHandle = try FileHandle(forWritingTo: url)
        recordingURL = url
        totalDataBytes = 0

        // Write WAV header with placeholder sizes
        writeWAVHeader()

        return url
    }

    /// Append audio samples from a CMSampleBuffer (handles float32→int16 and stereo→mono)
    func appendSamples(sampleBuffer: CMSampleBuffer) {
        guard let fileHandle = fileHandle else { return }
        guard let dataBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else { return }

        var length = 0
        var dataPointer: UnsafeMutablePointer<Int8>?
        let status = CMBlockBufferGetDataPointer(dataBuffer, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &dataPointer)

        guard status == kCMBlockBufferNoErr, let pointer = dataPointer else { return }

        let int16Data: Data

        // Check format — ScreenCaptureKit typically delivers float32
        if let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer),
           let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc) {

            if asbd.pointee.mFormatFlags & kAudioFormatFlagIsFloat != 0 {
                // Float32 audio → convert to Int16
                let floatCount = length / MemoryLayout<Float32>.size
                let floatPointer = UnsafeRawPointer(pointer).bindMemory(to: Float32.self, capacity: floatCount)

                let channelCount = Int(asbd.pointee.mChannelsPerFrame)
                let sampleCount = floatCount / channelCount

                var converted = Data(capacity: sampleCount * 2)
                for i in 0..<sampleCount {
                    var sample: Float32 = 0
                    for ch in 0..<channelCount {
                        sample += floatPointer[i * channelCount + ch]
                    }
                    sample /= Float32(channelCount)

                    let clamped = max(-1.0, min(1.0, sample))
                    let value = Int16(clamped * Float32(Int16.max))
                    withUnsafeBytes(of: value.littleEndian) { converted.append(contentsOf: $0) }
                }
                int16Data = converted
            } else {
                // Already integer PCM
                int16Data = Data(bytes: pointer, count: length)
            }
        } else {
            int16Data = Data(bytes: pointer, count: length)
        }

        // Write to file
        fileHandle.write(int16Data)
        totalDataBytes += UInt32(int16Data.count)
    }

    /// Stop recording, patch WAV header with correct sizes, close file
    func stopRecording() throws {
        guard let fileHandle = fileHandle else { return }

        // Patch RIFF chunk size at byte offset 4
        let riffSize = UInt32(36 + totalDataBytes)
        fileHandle.seek(toFileOffset: 4)
        withUnsafeBytes(of: riffSize.littleEndian) { fileHandle.write(Data($0)) }

        // Patch data subchunk size at byte offset 40
        fileHandle.seek(toFileOffset: 40)
        withUnsafeBytes(of: totalDataBytes.littleEndian) { fileHandle.write(Data($0)) }

        fileHandle.synchronizeFile()
        fileHandle.closeFile()
        self.fileHandle = nil
    }

    // MARK: - Private

    private func writeWAVHeader() {
        guard let fileHandle = fileHandle else { return }

        var header = Data()
        let byteRate = UInt32(sampleRate * channels * bitsPerSample / 8)
        let blockAlign = UInt16(channels * bitsPerSample / 8)

        // RIFF header (placeholder sizes)
        header.append(contentsOf: "RIFF".utf8)
        header.append(contentsOf: withUnsafeBytes(of: UInt32(0).littleEndian) { Array($0) }) // placeholder
        header.append(contentsOf: "WAVE".utf8)

        // fmt subchunk
        header.append(contentsOf: "fmt ".utf8)
        header.append(contentsOf: withUnsafeBytes(of: UInt32(16).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt16(1).littleEndian) { Array($0) }) // PCM
        header.append(contentsOf: withUnsafeBytes(of: UInt16(channels).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt32(sampleRate).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: byteRate.littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: blockAlign.littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt16(bitsPerSample).littleEndian) { Array($0) })

        // data subchunk
        header.append(contentsOf: "data".utf8)
        header.append(contentsOf: withUnsafeBytes(of: UInt32(0).littleEndian) { Array($0) }) // placeholder

        fileHandle.write(header)
    }
}
