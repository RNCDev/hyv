//! Wav2Vec2 CTC ONNX inference.
//!
//! Model: Xenova/wav2vec2-base-960h (int8, ~91 MB)
//! Architecture: CTC encoder, raw waveform input, character-level output.
//!
//! Input:  "input_values"  shape [1, T]  f32  (16kHz mono, normalised to mean=0 std=1)
//! Output: "logits"        shape [1, T', 32]  f32  (32-token char vocab)
//!
//! Vocab: <pad>=0 (blank), <s>=1, </s>=2, <unk>=3, |=4 (word boundary), A-Z + '

use ndarray::Array2;
use ort::session::Session;
use ort::value::{DynTensorValueType, Tensor};

use crate::transcription::{engine::TranscribedSegment, tokenizer::Tokenizer};

/// Run wav2vec2 CTC inference on 16kHz mono samples.
pub fn transcribe(
    session: &mut Session,
    samples: &[f32],
    speaker: &str,
    offset_secs: f64,
    tokenizer: Option<&Tokenizer>,
) -> Result<Vec<TranscribedSegment>, String> {
    if samples.is_empty() {
        return Ok(vec![]);
    }

    tracing::debug!(
        "wav2vec2 session inputs: {:?}",
        session.inputs.iter().map(|i| &i.name).collect::<Vec<_>>()
    );
    tracing::debug!(
        "wav2vec2 session outputs: {:?}",
        session.outputs.iter().map(|o| &o.name).collect::<Vec<_>>()
    );

    // Normalise to zero-mean unit-variance (wav2vec2 feature extractor default)
    let mean = samples.iter().copied().sum::<f32>() / samples.len() as f32;
    let variance = samples.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / samples.len() as f32;
    let std = variance.sqrt().max(1e-7);
    let normalised: Vec<f32> = samples.iter().map(|&x| (x - mean) / std).collect();

    // Input: [1, T]
    let input = Array2::from_shape_vec((1, normalised.len()), normalised)
        .map_err(|e| format!("wav2vec2 input shape: {e}"))?;

    let input_tensor =
        Tensor::from_array(input).map_err(|e| format!("wav2vec2 input tensor: {e}"))?;

    let outputs = session
        .run(ort::inputs!["input_values" => input_tensor])
        .map_err(|e| format!("wav2vec2 inference: {e}"))?;

    // Output: "logits" [1, T', 32]
    let dyn_tensor = outputs[0]
        .downcast_ref::<DynTensorValueType>()
        .map_err(|e| format!("wav2vec2 output downcast: {e}"))?;
    let (shape, logits) = dyn_tensor
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("wav2vec2 output extract: {e}"))?;

    let (t_frames, vocab_size) = (shape[1] as usize, shape[2] as usize);
    tracing::debug!("wav2vec2 output shape: [1, {t_frames}, {vocab_size}]");

    // Greedy CTC: argmax per frame, collapse repeats, remove blank (token 0)
    let token_ids = greedy_ctc_token_ids(logits, t_frames, vocab_size);

    if token_ids.is_empty() {
        return Ok(vec![]);
    }

    let text = match tokenizer {
        Some(tok) => {
            // wav2vec2 vocab: chars are uppercase, '|' = word boundary → space
            let raw = tok.decode_wav2vec2(&token_ids);
            raw
        }
        None => format!("[wav2vec2: {} tokens]", token_ids.len()),
    };

    if text.trim().is_empty() {
        return Ok(vec![]);
    }

    // Each output frame ≈ 20ms (wav2vec2 conv downsampling ~320x at 16kHz)
    let duration = t_frames as f64 * 0.02;

    Ok(vec![TranscribedSegment {
        start: offset_secs,
        end: offset_secs + duration,
        speaker: speaker.to_string(),
        text,
    }])
}

/// Greedy CTC decode: argmax per frame, collapse repeats, remove blank (token 0).
pub fn greedy_ctc_token_ids(logits: &[f32], t_frames: usize, vocab_size: usize) -> Vec<u32> {
    let mut tokens: Vec<u32> = Vec::with_capacity(t_frames);
    let mut prev = u32::MAX;

    for t in 0..t_frames {
        let frame = &logits[t * vocab_size..(t + 1) * vocab_size];
        let argmax = frame
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i as u32)
            .unwrap_or(0);

        if argmax != 0 && argmax != prev {
            tokens.push(argmax);
        }
        prev = argmax;
    }

    tokens
}
