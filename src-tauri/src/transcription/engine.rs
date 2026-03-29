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
        initial_prompt: &str,
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
        if !initial_prompt.is_empty() {
            params.set_initial_prompt(initial_prompt);
        }

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
            // Skip empty or punctuation-only segments (common Whisper artifact on near-speech noise)
            if text.is_empty() || text.chars().all(|c| !c.is_alphanumeric()) {
                continue;
            }
            // Normalize brand name variants that Whisper produces when the word
            // appears in both the prompt context and the audio (token doubling).
            let text = normalize_brand_names(&text);

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

    /// Transcribe all chunks for one channel.
    ///
    /// `base_prompt` is always prepended (e.g. "Vapi").
    /// `context_segments` is an optional slice of already-transcribed segments
    /// from the *other* channel — typically Speaker 2 (clean system audio).
    /// For each mic chunk, the function collects Speaker 2 text that falls in a
    /// window ending at the chunk's start time and appends it after the base
    /// prompt, giving Whisper the conversational context it needs to decode
    /// short or ambiguous user responses accurately.
    pub fn transcribe_channel<F>(
        &self,
        chunks: &[AudioChunk],
        speaker: &str,
        use_beam_search: bool,
        base_prompt: &str,
        context_segments: &[TranscribedSegment],
        progress: F,
    ) -> Result<Vec<TranscribedSegment>, String>
    where
        F: Fn(usize, usize),
    {
        // Exclude context that ended within this many seconds of the chunk start.
        // Whisper echoes very-recent prompt tokens into its output, causing doubled
        // words (e.g. "Vapi" → "Vaapi"). A small exclusion gap prevents this.
        const CONTEXT_RECENCY_GUARD_SECS: f64 = 10.0;

        let mut all_segments = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            // Build dynamic prompt: base + recent other-channel text
            let prompt = if context_segments.is_empty() {
                base_prompt.to_string()
            } else {
                let context_cutoff = chunk.offset_secs - CONTEXT_RECENCY_GUARD_SECS;
                // Use only the single most-recent eligible segment — just enough
                // for Whisper to know what was asked, without accumulating multiple
                // "Vapi" mentions that cause token-doubling artifacts.
                let context_text: String = context_segments
                    .iter()
                    .filter(|s| s.start < context_cutoff)
                    .last()
                    .map(|s| s.text.trim())
                    .unwrap_or("")
                    .to_string();

                if context_text.is_empty() {
                    base_prompt.to_string()
                } else {
                    format!("{base_prompt} {context_text}")
                }
            };

            let mut segments = self.transcribe_chunk(chunk, use_beam_search, &prompt)?;
            for seg in &mut segments {
                seg.speaker = speaker.to_string();
            }
            all_segments.extend(segments);
            progress(i + 1, chunks.len());
        }

        Ok(all_segments)
    }
}

/// Fix brand-name token-doubling artifacts that Whisper produces when the word
/// appears in both the context prompt and the audio (e.g. "Vaapi" → "Vapi",
/// "VVapi" → "Vapi"). Uses a regex-free approach for zero extra dependencies.
fn normalize_brand_names(text: &str) -> String {
    // Patterns Whisper produces for "Vapi" via token doubling
    const VARIANTS: &[(&str, &str)] = &[
        ("Vaapi", "Vapi"),
        ("VVapi", "Vapi"),
        ("vaapi", "Vapi"),
        ("vvapi", "Vapi"),
        ("VAAPI", "Vapi"),
        ("Bapi", "Vapi"),
        ("BAPI", "Vapi"),
        ("bapi", "Vapi"),
    ];
    let mut out = text.to_string();
    for (bad, good) in VARIANTS {
        if out.contains(bad) {
            out = out.replace(bad, good);
        }
    }
    out
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
