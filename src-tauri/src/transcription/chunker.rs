use crate::audio::vad::SpeechSegment;

const WHISPER_CHUNK_SECS: f64 = 30.0;

/// A chunk of audio to be transcribed.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub offset_secs: f64,
    pub index: usize,
    pub total: usize,
}

/// Split speech segments into Whisper-sized chunks (30s max).
/// Skips silence between segments.
pub fn chunk_speech(
    audio: &[f32],
    segments: &[SpeechSegment],
    sample_rate: u32,
) -> Vec<AudioChunk> {
    let chunk_samples = (WHISPER_CHUNK_SECS * sample_rate as f64) as usize;

    let mut chunks = Vec::new();

    for segment in segments {
        let seg_audio = &audio[segment.start_sample..segment.end_sample.min(audio.len())];
        let seg_offset = segment.start_secs(sample_rate);

        // Split this segment into 30s chunks if it's longer
        let mut pos = 0;
        while pos < seg_audio.len() {
            let end = (pos + chunk_samples).min(seg_audio.len());
            let chunk_offset = seg_offset + (pos as f64 / sample_rate as f64);

            chunks.push(AudioChunk {
                samples: seg_audio[pos..end].to_vec(),
                offset_secs: chunk_offset,
                index: 0, // Set after collecting all chunks
                total: 0,
            });

            pos = end;
        }
    }

    // Set index and total
    let total = chunks.len();
    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.index = i;
        chunk.total = total;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_segments() {
        let audio = vec![0.0f32; 16000];
        let chunks = chunk_speech(&audio, &[], 16000);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_short_segment_single_chunk() {
        let audio = vec![0.1f32; 160000]; // 10 seconds
        let segments = vec![SpeechSegment {
            start_sample: 0,
            end_sample: 160000,
        }];
        let chunks = chunk_speech(&audio, &segments, 16000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].samples.len(), 160000);
    }

    #[test]
    fn test_long_segment_splits() {
        let audio = vec![0.1f32; 960000]; // 60 seconds
        let segments = vec![SpeechSegment {
            start_sample: 0,
            end_sample: 960000,
        }];
        let chunks = chunk_speech(&audio, &segments, 16000);
        assert_eq!(chunks.len(), 2); // 30s + 30s
    }
}
