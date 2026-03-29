//! ONNX-based transcription engine wrapping Parakeet and Cohere.
//!
//! Implements `TranscriptionEngine`. The active decoder is determined at
//! construction time from `ModelKind`.

use std::path::Path;
use std::sync::Mutex;

use crate::transcription::{
    chunker::AudioChunk,
    parakeet,
    engine::{TranscribedSegment, TranscriptionEngine},
    model_manager::ModelKind,
    onnx_runtime,
    tokenizer::Tokenizer,
};

pub struct OnnxEngine {
    session: Mutex<ort::session::Session>,
    kind: ModelKind,
    tokenizer: Option<Tokenizer>,
}

impl OnnxEngine {
    /// Load an ONNX model session.
    /// `kind` must be `ParakeetOnnx` or `CohereOnnx`.
    /// `tokenizer_path` is the path to the tokenizer.json for this model (if downloaded).
    pub fn new(
        model_path: &Path,
        kind: ModelKind,
        tokenizer_path: Option<&Path>,
    ) -> Result<Self, String> {
        let session = onnx_runtime::session(model_path)?;
        let tokenizer = tokenizer_path
            .map(Tokenizer::load)
            .transpose()?;
        Ok(Self {
            session: Mutex::new(session),
            kind,
            tokenizer,
        })
    }
}

impl TranscriptionEngine for OnnxEngine {
    fn transcribe_channel(
        &self,
        chunks: &[AudioChunk],
        speaker: &str,
        _use_beam_search: bool,            // ONNX models ignore this
        _base_prompt: &str,                // no prompt injection for CTC
        _context_segments: &[TranscribedSegment],
        progress: &dyn Fn(usize, usize),
    ) -> Result<Vec<TranscribedSegment>, String> {
        let total = chunks.len();
        let mut all_segments = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let mut session = self.session.lock()
                .map_err(|e| format!("OnnxEngine session lock poisoned: {e}"))?;

            let segs = match self.kind {
                ModelKind::ParakeetOnnx => {
                    parakeet::transcribe(
                        &mut session,
                        &chunk.samples,
                        speaker,
                        chunk.offset_secs,
                        self.tokenizer.as_ref(),
                    )?
                }
                ModelKind::CohereOnnx => {
                    return Err("OnnxEngine constructed with CohereOnnx kind — use CohereEngine (encoder+decoder)".into());
                }
                ModelKind::Whisper => {
                    return Err("OnnxEngine constructed with Whisper kind — use WhisperEngine".into());
                }
            };

            all_segments.extend(segs);
            progress(i + 1, total);
        }
        Ok(all_segments)
    }
}
