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

    // Sidechain gain: amplify only the signal used for VAD decisions, not the
    // audio sent to Whisper. This lifts sub-vocalized speech above the threshold
    // without altering spectral quality. 1.75x ≈ +5dB, recovers "Yeah, please".
    const VAD_SIDECHAIN_GAIN: f32 = 1.75;


    // How many silent frames to tolerate before closing a segment (200ms hangover).
    // Prevents trailing low-energy consonants and word endings from being clipped.
    let hangover_frames = (0.2_f64 / 0.03_f64).ceil() as usize; // ~7 frames

    let mut segments: Vec<SpeechSegment> = Vec::new();
    let mut in_speech = false;
    let mut start_sample: usize = 0;
    let mut silent_frames: usize = 0;

    for i in 0..n_frames {
        let frame_start = i * hop;
        let frame_end = (frame_start + frame_len).min(audio.len());
        let frame = &audio[frame_start..frame_end];

        // RMS energy on sidechain (gain-boosted copy) — original frame untouched
        let energy: f32 = super::util::rms(frame) * VAD_SIDECHAIN_GAIN;

        if energy > energy_threshold {
            if !in_speech {
                in_speech = true;
                start_sample = frame_start;
            }
            silent_frames = 0;
        } else if in_speech {
            silent_frames += 1;
            if silent_frames >= hangover_frames {
                in_speech = false;
                silent_frames = 0;
                // End the segment at the last voiced frame, not where silence started
                let end_sample = frame_start.saturating_sub((hangover_frames - 1) * hop);
                let duration = end_sample.saturating_sub(start_sample) as f64 / sample_rate as f64;
                if duration >= min_duration {
                    segments.push(SpeechSegment {
                        start_sample,
                        end_sample,
                    });
                }
            }
        }
    }

    // Handle trailing speech (still in hangover or active)
    if in_speech {
        let end_sample = if silent_frames > 0 {
            // We were in hangover — end at last voiced frame
            (n_frames * hop).saturating_sub(silent_frames * hop)
        } else {
            audio.len()
        };
        let duration = end_sample.saturating_sub(start_sample) as f64 / sample_rate as f64;
        if duration >= min_duration {
            segments.push(SpeechSegment {
                start_sample,
                end_sample,
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
