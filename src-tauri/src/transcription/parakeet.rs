//! Parakeet-TDT ONNX inference.
//!
//! Model: istupakov/parakeet-tdt-0.6b-v2-onnx
//! Architecture: CTC encoder. No decoder prompt (non-autoregressive).
//!
//! Input node:  "processed_signal"  shape [1, N_MELS, n_frames]  f32
//! Input node:  "length"            shape [1]                     i64
//! Output node: "logprobs" or similar  shape [1, T', vocab]        f32
//!
//! NOTE: Verify input/output names against the downloaded model with Netron
//! (https://netron.app). Names are logged on first call.

use ndarray::{Array1, Order};
use ort::session::Session;
use ort::value::{DynTensorValueType, Tensor};

use crate::transcription::{
    engine::TranscribedSegment,
    mel::log_mel_spectrogram,
    tokenizer::Tokenizer,
};

/// Run Parakeet-TDT inference on 16kHz mono samples.
/// `tokenizer` is `None` until Task 8 wires it up — returns placeholder text when None.
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

    // Log input/output names to aid Netron verification
    tracing::debug!(
        "Parakeet session inputs: {:?}",
        session.inputs.iter().map(|i| &i.name).collect::<Vec<_>>()
    );
    tracing::debug!(
        "Parakeet session outputs: {:?}",
        session.outputs.iter().map(|o| &o.name).collect::<Vec<_>>()
    );

    // Mel spectrogram [N_MELS, n_frames] → [1, N_MELS, n_frames]
    let mel = log_mel_spectrogram(samples);
    let (n_mels, n_frames) = mel.dim();
    let mel_3d = mel
        .into_shape_with_order(((1, n_mels, n_frames), Order::RowMajor))
        .map_err(|e| format!("mel reshape: {e}"))?;

    let length = Array1::<i64>::from_vec(vec![n_frames as i64]);

    let mel_tensor = Tensor::from_array(mel_3d)
        .map_err(|e| format!("Parakeet mel tensor: {e}"))?;
    let len_tensor = Tensor::from_array(length)
        .map_err(|e| format!("Parakeet length tensor: {e}"))?;

    let outputs = session
        .run(ort::inputs![
            "processed_signal" => mel_tensor,
            "length"           => len_tensor,
        ])
        .map_err(|e| format!("Parakeet inference: {e}"))?;

    // Extract log_probs [1, T', vocab] via DynTensor downcast
    let dyn_tensor = outputs[0]
        .downcast_ref::<DynTensorValueType>()
        .map_err(|e| format!("Parakeet output downcast: {e}"))?;
    let (shape, log_probs_data) = dyn_tensor
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Parakeet output extract: {e}"))?;

    // shape is [1, T', vocab]
    let (t_frames, vocab_size) = (shape[1] as usize, shape[2] as usize);

    tracing::debug!("Parakeet output shape: [1, {t_frames}, {vocab_size}]");

    // Greedy CTC argmax — returns token IDs
    let token_ids = greedy_ctc_token_ids(log_probs_data, t_frames, vocab_size);

    if token_ids.is_empty() {
        return Ok(vec![]);
    }

    // Decode token IDs if tokenizer is available, else placeholder
    let text = match tokenizer {
        Some(tok) => tok.decode(&token_ids),
        None => format!("[Parakeet stub: {} CTC tokens]", token_ids.len()),
    };

    if text.trim().is_empty() {
        return Ok(vec![]);
    }

    let duration = t_frames as f64 * 0.04; // ~40ms per output frame

    Ok(vec![TranscribedSegment {
        start: offset_secs,
        end: offset_secs + duration,
        speaker: speaker.to_string(),
        text,
    }])
}

/// Greedy CTC decode: argmax per frame, collapse repeats, remove blank (token 0).
/// Returns raw token IDs for the tokenizer to convert to text.
pub fn greedy_ctc_token_ids(log_probs: &[f32], t_frames: usize, vocab_size: usize) -> Vec<u32> {
    let mut tokens: Vec<u32> = Vec::with_capacity(t_frames);
    let mut prev = u32::MAX;

    for t in 0..t_frames {
        let frame = &log_probs[t * vocab_size..(t + 1) * vocab_size];
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
