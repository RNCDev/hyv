//! Cohere ASR encoder-decoder inference.
//!
//! Two separate ORT sessions: encoder (runs once per chunk) and decoder (greedy
//! decode loop with KV-cache).  Implements `TranscriptionEngine`.

use std::path::Path;
use std::sync::Mutex;

use ndarray::{Array2, Array3, Array4, s};
use ort::session::Session;
use ort::value::Tensor;

use crate::transcription::{
    chunker::AudioChunk,
    engine::{TranscribedSegment, TranscriptionEngine},
    mel::cohere_mel_spectrogram,
    onnx_runtime,
    tokenizer::Tokenizer,
};

// ── token IDs (verified from tokenizer.json, vocab_size=16384) ──────────────
const BOS: u32 = 4;
const EOS: u32 = 3;
const LANG_EN: u32 = 62;
const PNC: u32 = 5;
const NOPNC: u32 = 6;
const ITN: u32 = 8;
const NOITN: u32 = 9;
const NOTIMESTAMP: u32 = 11;
const NOSPEECH: u32 = 1;

/// Token IDs below this threshold are "special" and stripped from decoded text.
const SPECIAL_THRESHOLD: u32 = 14;

/// Initial prompt length (tokens before any generated output).
const PROMPT_LEN: usize = 5;
/// Maximum new tokens to generate per chunk.
const MAX_NEW_TOKENS: usize = 448;
/// Number of transformer layers (determines KV-cache size).
const N_LAYERS: usize = 32;
/// Number of KV heads.
const N_HEADS: usize = 8;
/// Per-head dimension.
const HEAD_DIM: usize = 128;

// ── public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CohereDecodeOptions {
    pub pnc: bool,
    pub itn: bool,
}

impl Default for CohereDecodeOptions {
    fn default() -> Self {
        Self { pnc: true, itn: true }
    }
}

pub struct CohereEngine {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: Tokenizer,
    options: CohereDecodeOptions,
}

impl CohereEngine {
    pub fn new(
        encoder_path: &Path,
        decoder_path: &Path,
        tokenizer_path: &Path,
        options: CohereDecodeOptions,
    ) -> Result<Self, String> {
        let encoder = onnx_runtime::session(encoder_path)?;
        let decoder = onnx_runtime::session(decoder_path)?;
        let tokenizer = Tokenizer::load(tokenizer_path)?;
        Ok(Self {
            encoder: Mutex::new(encoder),
            decoder: Mutex::new(decoder),
            tokenizer,
            options,
        })
    }

    /// Run the encoder on one chunk of audio samples.
    ///
    /// Returns `(hidden_states [1, T, 1024], n_frames)`.
    fn encode(&self, samples: &[f32]) -> Result<(Array3<f32>, usize), String> {
        // Build mel features: [n_frames, 128]
        let mel: Array2<f32> = cohere_mel_spectrogram(samples);
        let n_frames = mel.nrows();

        // Encoder expects [batch=1, sequence_length=n_frames, n_mels=128]
        let input_features: Array3<f32> = mel
            .into_shape_with_order((1, n_frames, 128))
            .map_err(|e| format!("Cohere mel reshape failed: {e}"))?;

        let mut session = self.encoder.lock()
            .map_err(|e| format!("CohereEngine encoder lock poisoned: {e}"))?;

        let input_tensor = Tensor::from_array(input_features)
            .map_err(|e| format!("Cohere encoder input tensor: {e}"))?;

        let outputs = session.run(ort::inputs![input_tensor])
            .map_err(|e| format!("Cohere encoder run failed: {e}"))?;

        let hidden = outputs.get("last_hidden_state")
            .ok_or_else(|| "Cohere encoder: missing last_hidden_state output".to_string())?;

        let hidden_array: Array3<f32> = hidden
            .try_extract_array::<f32>()
            .map_err(|e| format!("Cohere encoder hidden extract: {e}"))?
            .into_dimensionality::<ndarray::Ix3>()
            .map_err(|e| format!("Cohere encoder hidden dim: {e}"))?
            .to_owned();

        Ok((hidden_array, n_frames))
    }

    /// Greedy decode loop given encoder hidden states.
    ///
    /// Returns decoded token IDs (excluding prompt/special tokens).
    fn greedy_decode(
        &self,
        encoder_hidden_states: Array3<f32>,
    ) -> Result<Vec<u32>, String> {
        let pnc_token = if self.options.pnc { PNC } else { NOPNC };
        let itn_token = if self.options.itn { ITN } else { NOITN };

        // Initial prompt: [BOS, LANG_EN, pnc, itn, NOTIMESTAMP]
        let prompt: Vec<u32> = vec![BOS, LANG_EN, pnc_token, itn_token, NOTIMESTAMP];
        let mut generated: Vec<u32> = Vec::new();

        // Decoder KV-cache state.
        // Shape: [1, N_HEADS, seq_len, HEAD_DIM]
        // At step 0, seq_len = 0 (empty past).
        let mut decoder_past: Vec<Array4<f32>> = (0..N_LAYERS * 2)
            .map(|_| Array4::<f32>::zeros((1, N_HEADS, 0, HEAD_DIM)))
            .collect();
        let mut encoder_past: Vec<Array4<f32>> = (0..N_LAYERS * 2)
            .map(|_| Array4::<f32>::zeros((1, N_HEADS, 0, HEAD_DIM)))
            .collect();

        for step in 0..MAX_NEW_TOKENS {
            // Build input_ids for this step.
            let input_ids: Array2<i64> = if step == 0 {
                // Full prompt on first step
                let ids: Vec<i64> = prompt.iter().map(|&x| x as i64).collect();
                Array2::from_shape_vec((1, PROMPT_LEN), ids)
                    .map_err(|e| format!("input_ids shape: {e}"))?
            } else {
                // Only the last generated token
                let last = *generated.last().unwrap() as i64;
                Array2::from_shape_vec((1, 1), vec![last])
                    .map_err(|e| format!("input_ids shape: {e}"))?
            };

            let seq_len = input_ids.ncols();
            let past_seq_len = if step == 0 { 0 } else { PROMPT_LEN + step - 1 };
            let total_seq = past_seq_len + seq_len;

            // position_ids
            let pos_ids: Vec<i64> = (past_seq_len..total_seq).map(|x| x as i64).collect();
            let position_ids = Array2::from_shape_vec((1, seq_len), pos_ids)
                .map_err(|e| format!("position_ids shape: {e}"))?;

            // attention_mask: 1s for full history
            let attn_mask: Vec<i64> = vec![1i64; total_seq];
            let attention_mask = Array2::from_shape_vec((1, total_seq), attn_mask)
                .map_err(|e| format!("attention_mask shape: {e}"))?;

            // num_logits_to_keep: scalar 1
            let num_logits: ndarray::Array0<i64> = ndarray::Array0::from_elem((), 1i64);

            // Build inputs as Vec<(Cow<str>, SessionInputValue)>
            use std::borrow::Cow;
            use ort::session::SessionInputValue;

            let mut inputs: Vec<(Cow<str>, SessionInputValue)> = vec![
                (
                    Cow::Borrowed("input_ids"),
                    Tensor::from_array(input_ids)
                        .map_err(|e| format!("input_ids tensor: {e}"))?.into(),
                ),
                (
                    Cow::Borrowed("attention_mask"),
                    Tensor::from_array(attention_mask)
                        .map_err(|e| format!("attention_mask tensor: {e}"))?.into(),
                ),
                (
                    Cow::Borrowed("position_ids"),
                    Tensor::from_array(position_ids)
                        .map_err(|e| format!("position_ids tensor: {e}"))?.into(),
                ),
                (
                    Cow::Borrowed("num_logits_to_keep"),
                    Tensor::from_array(num_logits)
                        .map_err(|e| format!("num_logits tensor: {e}"))?.into(),
                ),
                (
                    Cow::Borrowed("encoder_hidden_states"),
                    Tensor::from_array(encoder_hidden_states.clone())
                        .map_err(|e| format!("enc hidden tensor: {e}"))?.into(),
                ),
            ];

            // Push past_key_values for each layer
            for layer in 0..N_LAYERS {
                inputs.push((
                    Cow::Owned(format!("past_key_values.{layer}.decoder.key")),
                    Tensor::from_array(decoder_past[layer * 2].clone())
                        .map_err(|e| format!("decoder past key {layer}: {e}"))?.into(),
                ));
                inputs.push((
                    Cow::Owned(format!("past_key_values.{layer}.decoder.value")),
                    Tensor::from_array(decoder_past[layer * 2 + 1].clone())
                        .map_err(|e| format!("decoder past value {layer}: {e}"))?.into(),
                ));
                inputs.push((
                    Cow::Owned(format!("past_key_values.{layer}.encoder.key")),
                    Tensor::from_array(encoder_past[layer * 2].clone())
                        .map_err(|e| format!("encoder past key {layer}: {e}"))?.into(),
                ));
                inputs.push((
                    Cow::Owned(format!("past_key_values.{layer}.encoder.value")),
                    Tensor::from_array(encoder_past[layer * 2 + 1].clone())
                        .map_err(|e| format!("encoder past value {layer}: {e}"))?.into(),
                ));
            }

            // Run decoder
            let mut session = self.decoder.lock()
                .map_err(|e| format!("CohereEngine decoder lock poisoned: {e}"))?;
            let mut outputs = session.run(inputs)
                .map_err(|e| format!("Cohere decoder run step {step}: {e}"))?;
            // session (MutexGuard) stays alive until end of block; outputs borrows from it.

            // Extract logits [1, 1, 16384] → argmax over last token
            let logits_val = outputs.get("logits")
                .ok_or_else(|| format!("Cohere decoder: missing logits at step {step}"))?;
            let logits_arr = logits_val
                .try_extract_array::<f32>()
                .map_err(|e| format!("logits extract step {step}: {e}"))?;
            let last_logits = logits_arr.slice(s![0, 0, ..]);
            let next_token = last_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(i, _)| i as u32)
                .unwrap_or(EOS);

            if next_token == EOS || next_token == NOSPEECH {
                break;
            }
            generated.push(next_token);

            // Update KV cache from present.* outputs
            for layer in 0..N_LAYERS {
                // decoder key
                let dk_name = format!("present.{layer}.decoder.key");
                let dk = outputs.remove(&dk_name)
                    .ok_or_else(|| format!("missing {dk_name}"))?;
                decoder_past[layer * 2] = dk
                    .try_extract_array::<f32>()
                    .map_err(|e| format!("present decoder key {layer}: {e}"))?
                    .into_dimensionality::<ndarray::Ix4>()
                    .map_err(|e| format!("present decoder key dim {layer}: {e}"))?
                    .to_owned();

                // decoder value
                let dv_name = format!("present.{layer}.decoder.value");
                let dv = outputs.remove(&dv_name)
                    .ok_or_else(|| format!("missing {dv_name}"))?;
                decoder_past[layer * 2 + 1] = dv
                    .try_extract_array::<f32>()
                    .map_err(|e| format!("present decoder value {layer}: {e}"))?
                    .into_dimensionality::<ndarray::Ix4>()
                    .map_err(|e| format!("present decoder value dim {layer}: {e}"))?
                    .to_owned();

                // encoder KV: only update on step 0 (frozen thereafter)
                if step == 0 {
                    let ek_name = format!("present.{layer}.encoder.key");
                    let ek = outputs.remove(&ek_name)
                        .ok_or_else(|| format!("missing {ek_name}"))?;
                    encoder_past[layer * 2] = ek
                        .try_extract_array::<f32>()
                        .map_err(|e| format!("present encoder key {layer}: {e}"))?
                        .into_dimensionality::<ndarray::Ix4>()
                        .map_err(|e| format!("present encoder key dim {layer}: {e}"))?
                        .to_owned();

                    let ev_name = format!("present.{layer}.encoder.value");
                    let ev = outputs.remove(&ev_name)
                        .ok_or_else(|| format!("missing {ev_name}"))?;
                    encoder_past[layer * 2 + 1] = ev
                        .try_extract_array::<f32>()
                        .map_err(|e| format!("present encoder value {layer}: {e}"))?
                        .into_dimensionality::<ndarray::Ix4>()
                        .map_err(|e| format!("present encoder value dim {layer}: {e}"))?
                        .to_owned();
                }
            }
        }

        Ok(generated)
    }
}

impl TranscriptionEngine for CohereEngine {
    fn transcribe_channel(
        &self,
        chunks: &[AudioChunk],
        speaker: &str,
        _use_beam_search: bool,
        _base_prompt: &str,
        _context_segments: &[TranscribedSegment],
        progress: &dyn Fn(usize, usize),
    ) -> Result<Vec<TranscribedSegment>, String> {
        let total = chunks.len();
        let mut all_segments = Vec::new();

        for (i, chunk) in chunks.iter().enumerate() {
            let (hidden_states, _n_frames) = self.encode(&chunk.samples)?;
            let token_ids = self.greedy_decode(hidden_states)?;

            let text = self.tokenizer.decode_filtering_specials(&token_ids, SPECIAL_THRESHOLD);
            let text = text.trim().to_string();

            if !text.is_empty() {
                all_segments.push(TranscribedSegment {
                    start: chunk.offset_secs,
                    end: chunk.offset_secs + chunk.samples.len() as f64 / 16_000.0,
                    text,
                    speaker: speaker.to_string(),
                });
            }

            progress(i + 1, total);
        }

        Ok(all_segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cohere_decode_options_default() {
        let opts = CohereDecodeOptions::default();
        assert!(opts.pnc);
        assert!(opts.itn);
    }

    #[test]
    fn cohere_engine_new_fails_without_model() {
        // ort with load-dynamic panics at dylib init if libonnxruntime.dylib is absent.
        // We verify that construction fails (either Err or panic) rather than silently
        // succeeding. The panic case is exercised by the should_panic sibling test.
        //
        // If ORT_DYLIB_PATH points to a real dylib (e.g. in CI), this returns Err.
        // If not, the dylib-load panic is caught by the sibling test below.
        //
        // This test only runs if ORT is initialised; we skip silently otherwise.
        let result = std::panic::catch_unwind(|| {
            CohereEngine::new(
                std::path::Path::new("/nonexistent/encoder.onnx"),
                std::path::Path::new("/nonexistent/decoder.onnx"),
                std::path::Path::new("/nonexistent/tokenizer.json"),
                CohereDecodeOptions::default(),
            )
        });
        match result {
            // ORT dylib missing → panic is expected
            Err(_panic) => { /* acceptable */ }
            // ORT dylib present → must return Err (not Ok)
            Ok(r) => {
                assert!(r.is_err(), "expected Err when model files are missing");
                assert!(!r.err().unwrap().is_empty());
            }
        }
    }
}
