import Foundation

struct WAVEncoder {
    /// Encode raw PCM data as a WAV file
    /// - Parameters:
    ///   - pcmData: Raw PCM audio data (16-bit signed integer, little-endian)
    ///   - sampleRate: Sample rate in Hz (e.g., 16000)
    ///   - channels: Number of audio channels (1 = mono)
    ///   - bitsPerSample: Bits per sample (16)
    /// - Returns: Complete WAV file data with RIFF header
    static func encode(pcmData: Data, sampleRate: Int = 16000, channels: Int = 1, bitsPerSample: Int = 16) -> Data {
        var data = Data()
        let byteRate = sampleRate * channels * bitsPerSample / 8
        let blockAlign = channels * bitsPerSample / 8
        let dataSize = UInt32(pcmData.count)
        let fileSize = UInt32(36 + pcmData.count)

        // RIFF header
        data.append(contentsOf: "RIFF".utf8)
        data.append(contentsOf: withUnsafeBytes(of: fileSize.littleEndian) { Array($0) })
        data.append(contentsOf: "WAVE".utf8)

        // fmt subchunk
        data.append(contentsOf: "fmt ".utf8)
        data.append(contentsOf: withUnsafeBytes(of: UInt32(16).littleEndian) { Array($0) }) // subchunk size
        data.append(contentsOf: withUnsafeBytes(of: UInt16(1).littleEndian) { Array($0) })  // PCM format
        data.append(contentsOf: withUnsafeBytes(of: UInt16(channels).littleEndian) { Array($0) })
        data.append(contentsOf: withUnsafeBytes(of: UInt32(sampleRate).littleEndian) { Array($0) })
        data.append(contentsOf: withUnsafeBytes(of: UInt32(byteRate).littleEndian) { Array($0) })
        data.append(contentsOf: withUnsafeBytes(of: UInt16(blockAlign).littleEndian) { Array($0) })
        data.append(contentsOf: withUnsafeBytes(of: UInt16(bitsPerSample).littleEndian) { Array($0) })

        // data subchunk
        data.append(contentsOf: "data".utf8)
        data.append(contentsOf: withUnsafeBytes(of: dataSize.littleEndian) { Array($0) })
        data.append(pcmData)

        return data
    }
}
