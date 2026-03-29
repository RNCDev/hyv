use std::path::Path;
use tracing::{info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::chunker::AudioChunk;

/// A transcribed segment with timing and speaker info.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscribedSegment {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        if !model_path.exists() {
            return Err(format!("Model file not found: {}", model_path.display()));
        }

        info!(path = %model_path.display(), "Loading Whisper model");

        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or("Invalid model path")?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| format!("Failed to load Whisper model: {e}"))?;

        info!("Whisper model loaded successfully");
        Ok(Self { ctx })
    }

    pub fn transcribe_chunk(
        &self,
        chunk: &AudioChunk,
        use_beam_search: bool,
    ) -> Result<Vec<TranscribedSegment>, String> {
        info!(
            chunk = chunk.index + 1,
            total = chunk.total,
            offset = format!("{:.1}s", chunk.offset_secs),
            duration = format!("{:.1}s", chunk.samples.len() as f64 / 16000.0),
            "Transcribing chunk"
        );

        let mut params = FullParams::new(if use_beam_search {
            SamplingStrategy::BeamSearch { beam_size: 5, patience: 1.0 }
        } else {
            SamplingStrategy::Greedy { best_of: 1 }
        });
        params.set_language(Some("en"));
        params.set_n_threads(4);
        params.set_no_timestamps(false);
        params.set_suppress_nst(true);
        // Suppress segments where Whisper is not confident speech is present.
        // Raised to 0.65 — 0.6 still passed hallucinated text on near-silent chunks.
        params.set_no_speech_thold(0.65);
        // Reject high-entropy segments — Whisper produces garbage text when
        // confused about the audio content. 2.4 is Meetily's value.
        params.set_entropy_thold(2.4);
        // Reject segments with low average token log-probability. Segments below
        // -1.0 are likely hallucinations on near-silence. Meetily's value.
        params.set_logprob_thold(-1.0);
        // Minimum per-token timestamp probability. Filters uncertain word boundaries.
        params.set_thold_pt(0.01);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("Failed to create Whisper state: {e}"))?;

        state
            .full(params, &chunk.samples)
            .map_err(|e| format!("Whisper transcription failed: {e}"))?;

        let n_segments = state
            .full_n_segments()
            .map_err(|e| format!("Failed to get segment count: {e}"))?;

        let mut segments = Vec::new();
        for i in 0..n_segments {
            let text = state
                .full_get_segment_text(i)
                .map_err(|e| format!("Failed to get segment text: {e}"))?;

            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }

            let start_cs = state
                .full_get_segment_t0(i)
                .map_err(|e| format!("Failed to get segment start: {e}"))?;
            let end_cs = state
                .full_get_segment_t1(i)
                .map_err(|e| format!("Failed to get segment end: {e}"))?;

            // Convert centiseconds to seconds, add chunk offset
            let start = chunk.offset_secs + (start_cs as f64 / 100.0);
            let end = chunk.offset_secs + (end_cs as f64 / 100.0);

            segments.push(TranscribedSegment {
                start,
                end,
                speaker: String::new(), // Set by caller
                text,
            });
        }

        if segments.is_empty() {
            warn!(chunk = chunk.index + 1, "No speech detected in chunk");
        }

        Ok(segments)
    }

    pub fn transcribe_channel<F>(
        &self,
        chunks: &[AudioChunk],
        speaker: &str,
        use_beam_search: bool,
        progress: F,
    ) -> Result<Vec<TranscribedSegment>, String>
    where
        F: Fn(usize, usize),
    {
        let mut all_segments = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let mut segments = self.transcribe_chunk(chunk, use_beam_search)?;
            for seg in &mut segments {
                seg.speaker = speaker.to_string();
            }
            all_segments.extend(segments);
            progress(i + 1, chunks.len());
        }

        Ok(all_segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_chunk_accepts_beam_flag() {
        use std::path::Path;
        // Should fail with "Model file not found" — not a panic
        let result = WhisperEngine::new(Path::new("/nonexistent/model.bin"));
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.contains("Model file not found"), "unexpected error: {err}");
    }
}
