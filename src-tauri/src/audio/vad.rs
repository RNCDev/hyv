/// Speech segment identified by VAD.
#[derive(Debug, Clone)]
pub struct SpeechSegment {
    pub start_sample: usize,
    pub end_sample: usize,
}

impl SpeechSegment {
    pub fn start_secs(&self, sample_rate: u32) -> f64 {
        self.start_sample as f64 / sample_rate as f64
    }

    pub fn end_secs(&self, sample_rate: u32) -> f64 {
        self.end_sample as f64 / sample_rate as f64
    }

    pub fn duration_secs(&self, sample_rate: u32) -> f64 {
        self.end_secs(sample_rate) - self.start_secs(sample_rate)
    }
}

/// Energy-based Voice Activity Detection.
/// Ported from Python find_speech_segments.
pub fn find_speech_segments(
    audio: &[f32],
    sample_rate: u32,
    min_duration: f64,
    energy_threshold: f32,
    merge_gap: f64,
) -> Vec<SpeechSegment> {
    let frame_len = (sample_rate as f64 * 0.03) as usize; // 30ms frames
    let hop = frame_len;
    let n_frames = audio.len() / hop;

    let mut segments: Vec<SpeechSegment> = Vec::new();
    let mut in_speech = false;
    let mut start_sample: usize = 0;

    for i in 0..n_frames {
        let frame_start = i * hop;
        let frame_end = (frame_start + frame_len).min(audio.len());
        let frame = &audio[frame_start..frame_end];

        // RMS energy
        let energy: f32 = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();

        if energy > energy_threshold && !in_speech {
            in_speech = true;
            start_sample = frame_start;
        } else if energy <= energy_threshold && in_speech {
            in_speech = false;
            let duration = (frame_start - start_sample) as f64 / sample_rate as f64;
            if duration >= min_duration {
                segments.push(SpeechSegment {
                    start_sample,
                    end_sample: frame_start,
                });
            }
        }
    }

    // Handle trailing speech
    if in_speech {
        let duration = (audio.len() - start_sample) as f64 / sample_rate as f64;
        if duration >= min_duration {
            segments.push(SpeechSegment {
                start_sample,
                end_sample: audio.len(),
            });
        }
    }

    // Merge segments separated by less than merge_gap
    let merge_gap_samples = (merge_gap * sample_rate as f64) as usize;
    let mut merged: Vec<SpeechSegment> = Vec::new();
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if seg.start_sample.saturating_sub(last.end_sample) < merge_gap_samples {
                last.end_sample = seg.end_sample;
                continue;
            }
        }
        merged.push(seg);
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_returns_no_segments() {
        let audio = vec![0.0f32; 16000]; // 1 second of silence
        let segments = find_speech_segments(&audio, 16000, 0.3, 0.002, 1.0);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_short_burst_filtered() {
        // 0.1s of noise — below min_duration
        let mut audio = vec![0.0f32; 16000];
        for i in 0..1600 {
            audio[i] = 0.5;
        }
        let segments = find_speech_segments(&audio, 16000, 0.3, 0.002, 1.0);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_speech_detected() {
        // 1 second of "speech" in the middle
        let mut audio = vec![0.0f32; 48000]; // 3 seconds
        for i in 16000..32000 {
            audio[i] = 0.1;
        }
        let segments = find_speech_segments(&audio, 16000, 0.3, 0.002, 1.0);
        assert_eq!(segments.len(), 1);
    }
}
