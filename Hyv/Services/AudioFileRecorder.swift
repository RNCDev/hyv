import Foundation
import CoreMedia
import AVFoundation
import os

private let logger = Logger(subsystem: "com.hyv.app", category: "audio-recorder")

/// Records stereo WAV: channel 0 = system audio (remote), channel 1 = microphone (you).
/// Both streams are kept separate for independent transcription — no mixing.
actor AudioFileRecorder {
    private let sampleRate: Int = 16000
    private let bitsPerSample: Int = 16
    private let channels: Int = 2  // stereo: system (L) + mic (R)

    private var fileHandle: FileHandle?
    private(set) var recordingURL: URL?
    private var totalDataBytes: UInt32 = 0

    // Accumulate float samples from each source; interleave and flush when both have data
    private var systemBuffer: [Float] = []
    private var micBuffer: [Float] = []

    private var recordingsDirectory: URL {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Hyv/recordings")
    }

    /// Start recording to a new stereo WAV file. Returns the file URL.
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

        logger.info("Recording started (stereo): \(url.lastPathComponent)")
        return url
    }

    /// Append system audio samples (channel 0 — remote participants)
    func appendSystemSamples(sampleBuffer: CMSampleBuffer) {
        guard fileHandle != nil else { return }
        let floats = extractMono(from: sampleBuffer)
        guard !floats.isEmpty else { return }
        systemBuffer.append(contentsOf: floats)
        flushInterleaved()
    }

    /// Append microphone samples (channel 1 — you)
    func appendMicSamples(sampleBuffer: CMSampleBuffer) {
        guard fileHandle != nil else { return }
        let floats = extractMono(from: sampleBuffer)
        guard !floats.isEmpty else { return }
        micBuffer.append(contentsOf: floats)
        flushInterleaved()
    }

    /// Stop recording, flush remaining samples, patch WAV header, close file
    func stopRecording() throws {
        // Flush remaining samples (pad the shorter buffer with silence)
        let remaining = max(systemBuffer.count, micBuffer.count)
        if remaining > 0 {
            writeInterleaved(system: systemBuffer, mic: micBuffer, count: remaining)
            systemBuffer = []
            micBuffer = []
        }

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

        let fileSizeMB = Double(totalDataBytes) / (1024 * 1024)
        let durationSec = Double(totalDataBytes) / Double(sampleRate * channels * bitsPerSample / 8)
        logger.info("Recording stopped: \(String(format: "%.1f", fileSizeMB)) MB, \(String(format: "%.0f", durationSec))s stereo audio")
    }

    // MARK: - Private

    /// Write interleaved stereo samples when both buffers have data
    private func flushInterleaved() {
        let count = min(systemBuffer.count, micBuffer.count)
        guard count > 0 else { return }
        writeInterleaved(system: systemBuffer, mic: micBuffer, count: count)
        systemBuffer.removeFirst(count)
        micBuffer.removeFirst(count)
    }

    /// Interleave system (ch0) and mic (ch1) samples and write as int16 stereo PCM
    private func writeInterleaved(system sys: [Float], mic: [Float], count: Int) {
        guard let fileHandle = fileHandle else { return }
        // Each frame = 2 samples (L + R) * 2 bytes each = 4 bytes per frame
        var out = Data(capacity: count * 4)
        for i in 0..<count {
            // Channel 0: system audio (remote)
            let s = i < sys.count ? sys[i] : 0.0
            let sVal = Int16(max(-1.0, min(1.0, s)) * Float(Int16.max))
            withUnsafeBytes(of: sVal.littleEndian) { out.append(contentsOf: $0) }

            // Channel 1: microphone (you)
            let m = i < mic.count ? mic[i] : 0.0
            let mVal = Int16(max(-1.0, min(1.0, m)) * Float(Int16.max))
            withUnsafeBytes(of: mVal.littleEndian) { out.append(contentsOf: $0) }
        }
        fileHandle.write(out)
        totalDataBytes += UInt32(out.count)
    }

    /// Extract mono float32 samples from a CMSampleBuffer (handles multi-channel → mono downmix)
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

        // RIFF header
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
