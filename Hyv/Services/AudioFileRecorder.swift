import Foundation
import CoreMedia
import AVFoundation
import os

private let logger = Logger(subsystem: "com.hyv.app", category: "audio-recorder")

actor AudioFileRecorder {
    private let sampleRate: Int = 16000
    private let bitsPerSample: Int = 16
    private let channels: Int = 1

    private var fileHandle: FileHandle?
    private(set) var recordingURL: URL?
    private var totalDataBytes: UInt32 = 0

    // Mixing: accumulate float samples from each source, mix and flush when both have data
    private var systemBuffer: [Float] = []
    private var micBuffer: [Float] = []

    private var recordingsDirectory: URL {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Hyv/recordings")
    }

    /// Start recording to a new WAV file. Returns the file URL.
    func startRecording() throws -> URL {
        try FileManager.default.createDirectory(at: recordingsDirectory, withIntermediateDirectories: true)

        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withFullDate, .withFullTime]
        let filename = "recording_\(formatter.string(from: Date())).wav"
        let url = recordingsDirectory.appendingPathComponent(filename)

        FileManager.default.createFile(atPath: url.path, contents: nil)
        fileHandle = try FileHandle(forWritingTo: url)
        recordingURL = url
        totalDataBytes = 0
        systemBuffer = []
        micBuffer = []

        writeWAVHeader()

        logger.info("Recording started: \(url.lastPathComponent)")
        return url
    }

    /// Append system audio (from ScreenCaptureKit) — float32, any channel count, 16kHz
    func appendSystemSamples(sampleBuffer: CMSampleBuffer) {
        guard fileHandle != nil else { return }
        let floats = extractMono(from: sampleBuffer)
        guard !floats.isEmpty else { return }
        systemBuffer.append(contentsOf: floats)
        flushMixed()
    }

    /// Append microphone audio (from AVCaptureSession) — float32 mono, 16kHz
    func appendMicSamples(sampleBuffer: CMSampleBuffer) {
        guard fileHandle != nil else { return }
        let floats = extractMono(from: sampleBuffer)
        guard !floats.isEmpty else { return }
        micBuffer.append(contentsOf: floats)
        flushMixed()
    }

    /// Stop recording, patch WAV header with correct sizes, close file
    func stopRecording() throws {
        // Flush any remaining samples from either buffer (write whatever we have)
        let count = max(systemBuffer.count, micBuffer.count)
        if count > 0 {
            writeMixed(systemBuffer, micBuffer, count: count)
            systemBuffer = []
            micBuffer = []
        }

        guard let fileHandle = fileHandle else { return }

        let riffSize = UInt32(36 + totalDataBytes)
        fileHandle.seek(toFileOffset: 4)
        withUnsafeBytes(of: riffSize.littleEndian) { fileHandle.write(Data($0)) }

        fileHandle.seek(toFileOffset: 40)
        withUnsafeBytes(of: totalDataBytes.littleEndian) { fileHandle.write(Data($0)) }

        fileHandle.synchronizeFile()
        fileHandle.closeFile()
        self.fileHandle = nil

        let fileSizeMB = Double(totalDataBytes) / (1024 * 1024)
        let durationSec = Double(totalDataBytes) / Double(sampleRate * channels * bitsPerSample / 8)
        logger.info("Recording stopped: \(String(format: "%.1f", fileSizeMB)) MB, \(String(format: "%.0f", durationSec))s audio")
    }

    // MARK: - Private

    /// Mix and write samples when both buffers have data; flush the overlapping portion
    private func flushMixed() {
        let count = min(systemBuffer.count, micBuffer.count)
        guard count > 0 else { return }
        writeMixed(systemBuffer, micBuffer, count: count)
        systemBuffer.removeFirst(count)
        micBuffer.removeFirst(count)
    }

    /// Sum samples from both buffers (up to `count`), clamp, and write as int16 PCM
    private func writeMixed(_ sys: [Float], _ mic: [Float], count: Int) {
        guard let fileHandle = fileHandle else { return }
        var out = Data(capacity: count * 2)
        for i in 0..<count {
            let s = i < sys.count ? sys[i] : 0.0
            let m = i < mic.count ? mic[i] : 0.0
            let mixed = max(-1.0, min(1.0, s + m))
            let value = Int16(mixed * Float(Int16.max))
            withUnsafeBytes(of: value.littleEndian) { out.append(contentsOf: $0) }
        }
        fileHandle.write(out)
        totalDataBytes += UInt32(out.count)
    }

    /// Extract mono float32 samples from a CMSampleBuffer at any channel count
    private func extractMono(from sampleBuffer: CMSampleBuffer) -> [Float] {
        guard let dataBuffer = CMSampleBufferGetDataBuffer(sampleBuffer) else { return [] }

        var length = 0
        var dataPointer: UnsafeMutablePointer<Int8>?
        let status = CMBlockBufferGetDataPointer(dataBuffer, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &length, dataPointerOut: &dataPointer)
        guard status == kCMBlockBufferNoErr, let pointer = dataPointer else { return [] }

        guard let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc) else {
            return []
        }

        let channelCount = Int(asbd.pointee.mChannelsPerFrame)

        if asbd.pointee.mFormatFlags & kAudioFormatFlagIsFloat != 0 {
            let floatCount = length / MemoryLayout<Float32>.size
            let floatPointer = UnsafeRawPointer(pointer).bindMemory(to: Float32.self, capacity: floatCount)
            let sampleCount = floatCount / max(1, channelCount)
            var result = [Float](repeating: 0, count: sampleCount)
            for i in 0..<sampleCount {
                var sum: Float = 0
                for ch in 0..<channelCount {
                    sum += floatPointer[i * channelCount + ch]
                }
                result[i] = sum / Float(max(1, channelCount))
            }
            return result
        } else {
            // Integer PCM — treat as int16
            let sampleCount = length / (MemoryLayout<Int16>.size * max(1, channelCount))
            let int16Pointer = UnsafeRawPointer(pointer).bindMemory(to: Int16.self, capacity: sampleCount * channelCount)
            var result = [Float](repeating: 0, count: sampleCount)
            for i in 0..<sampleCount {
                var sum: Float = 0
                for ch in 0..<channelCount {
                    sum += Float(int16Pointer[i * channelCount + ch]) / Float(Int16.max)
                }
                result[i] = sum / Float(max(1, channelCount))
            }
            return result
        }
    }

    private func writeWAVHeader() {
        guard let fileHandle = fileHandle else { return }

        var header = Data()
        let byteRate = UInt32(sampleRate * channels * bitsPerSample / 8)
        let blockAlign = UInt16(channels * bitsPerSample / 8)

        header.append(contentsOf: "RIFF".utf8)
        header.append(contentsOf: withUnsafeBytes(of: UInt32(0).littleEndian) { Array($0) })
        header.append(contentsOf: "WAVE".utf8)

        header.append(contentsOf: "fmt ".utf8)
        header.append(contentsOf: withUnsafeBytes(of: UInt32(16).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt16(1).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt16(channels).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt32(sampleRate).littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: byteRate.littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: blockAlign.littleEndian) { Array($0) })
        header.append(contentsOf: withUnsafeBytes(of: UInt16(bitsPerSample).littleEndian) { Array($0) })

        header.append(contentsOf: "data".utf8)
        header.append(contentsOf: withUnsafeBytes(of: UInt32(0).littleEndian) { Array($0) })

        fileHandle.write(header)
    }
}
