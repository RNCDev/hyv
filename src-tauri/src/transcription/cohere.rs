//! Cohere ONNX inference stub.
//!
//! The Cohere ONNX graph topology is unconfirmed as of March 2026.
//! This stub:
//!   1. Loads the session (passed in by caller)
//!   2. Logs ALL input/output names via tracing::info! for Netron verification
//!   3. Attempts inference with guessed input name "input_features"
//!   4. Returns Ok(vec![]) on any failure (graceful fallback)
//!
//! Once the real topology is confirmed via Netron, replace the guessed
//! input name and add proper output decoding.
//!
//! NOTE: Verify input/output names against the downloaded model with Netron
//! (https://netron.app). Names are logged on first call.

use ndarray::Order;
use ort::session::Session;
use ort::value::Tensor;

use crate::transcription::{
    engine::TranscribedSegment,
    mel::log_mel_spectrogram,
};

/// Run Cohere ONNX inference on 16kHz mono samples.
///
/// Returns a placeholder segment on success, or `Ok(vec![])` on any failure
/// (inference errors are logged as warnings and not propagated).
pub fn transcribe(
    session: &mut Session,
    samples: &[f32],
    speaker: &str,
    offset_secs: f64,
) -> Result<Vec<TranscribedSegment>, String> {
    if samples.is_empty() {
        return Ok(vec![]);
    }

    // Log ALL input/output names for Netron verification
    tracing::info!(
        "Cohere session inputs: {:?}",
        session.inputs.iter().map(|i| &i.name).collect::<Vec<_>>()
    );
    tracing::info!(
        "Cohere session outputs: {:?}",
        session.outputs.iter().map(|o| &o.name).collect::<Vec<_>>()
    );

    // Mel spectrogram [N_MELS, n_frames] → [1, N_MELS, n_frames]
    let mel = log_mel_spectrogram(samples);
    let (n_mels, n_frames) = mel.dim();
    let mel_3d = match mel.into_shape_with_order(((1, n_mels, n_frames), Order::RowMajor)) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Cohere mel reshape failed: {e}");
            return Ok(vec![]);
        }
    };

    let mel_tensor = match Tensor::from_array(mel_3d) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Cohere mel tensor construction failed: {e}");
            return Ok(vec![]);
        }
    };

    // Attempt inference with guessed input name "input_features"
    // Replace once topology is confirmed via Netron.
    let outputs = session.run(ort::inputs![
        "input_features" => mel_tensor,
    ]);

    match outputs {
        Err(e) => {
            tracing::warn!(
                "Cohere inference failed (input name may be wrong — check Netron logs): {e}"
            );
            Ok(vec![])
        }
        Ok(_outputs) => {
            // Topology unconfirmed: return a placeholder segment so callers know
            // inference succeeded and can proceed with output decoding in a future task.
            tracing::info!("Cohere inference succeeded — output decoding not yet implemented");
            let duration = n_frames as f64 * 0.01; // placeholder frame duration
            Ok(vec![TranscribedSegment {
                start: offset_secs,
                end: offset_secs + duration,
                speaker: speaker.to_string(),
                text: "[Cohere stub: inference succeeded]".to_string(),
            }])
        }
    }
}
