use aec3::voip::VoipAec3;
use tracing::{info, warn};

use crate::audio::vad;

const SAMPLE_RATE: u32 = 16000;

/// Align mic and system audio buffers at the sample level by detecting each
/// buffer's first speech onset and trimming leading silence from whichever
/// channel starts later.
///
/// Only corrects offsets > 500ms — smaller differences are normal jitter and
/// aren't worth adjusting. Returns originals unchanged if neither channel
/// contains speech or the offset is below the threshold.
pub fn align_buffers(mic: &[f32], system: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mic_onset = first_speech_onset(mic);
    let sys_onset = first_speech_onset(system);

    let (mic_onset, sys_onset) = match (mic_onset, sys_onset) {
        (Some(m), Some(s)) => (m, s),
        _ => {
            info!("AEC align: one channel has no speech — skipping sample alignment");
            return (mic.to_vec(), system.to_vec());
        }
    };

    let offset = mic_onset as i64 - sys_onset as i64;
    let threshold = (SAMPLE_RATE / 2) as i64; // 500ms

    if offset.abs() < threshold {
        info!(
            offset_ms = offset * 1000 / SAMPLE_RATE as i64,
            "AEC align: offset within threshold, skipping"
        );
        return (mic.to_vec(), system.to_vec());
    }

    let (mic_out, sys_out) = if offset > 0 {
        // Mic has more leading audio — trim its front
        let trim = offset as usize;
        let trimmed_mic = if trim < mic.len() { mic[trim..].to_vec() } else { vec![] };
        (trimmed_mic, system.to_vec())
    } else {
        // System has more leading audio — trim its front
        let trim = (-offset) as usize;
        let trimmed_sys = if trim < system.len() { system[trim..].to_vec() } else { vec![] };
        (mic.to_vec(), trimmed_sys)
    };

    // Truncate both to same length
    let len = mic_out.len().min(sys_out.len());
    let mic_out = mic_out[..len].to_vec();
    let sys_out = sys_out[..len].to_vec();

    info!(
        offset_ms = offset * 1000 / SAMPLE_RATE as i64,
        aligned_samples = len,
        "AEC: aligned buffers at sample level"
    );

    (mic_out, sys_out)
}

/// Cancel echo from `mic` using `reference` (system audio) as the far-end signal.
/// Processes in 10ms frames using WebRTC AEC3 (pure Rust).
/// Returns the cleaned mic signal (echo suppressed, user voice preserved).
pub fn cancel_echo(mic: &[f32], reference: &[f32]) -> Vec<f32> {
    let mut pipeline = match VoipAec3::builder(SAMPLE_RATE as usize, 1, 1).build() {
        Ok(p) => p,
        Err(e) => {
            warn!("AEC3 init failed: {e:?} — skipping echo cancellation");
            return mic.to_vec();
        }
    };

    let frame_size = pipeline.capture_frame_samples(); // 160 @ 16kHz
    let total_frames = mic.len() / frame_size;

    let mut output = Vec::with_capacity(mic.len());
    let mut out_frame = vec![0.0f32; frame_size];

    for i in 0..total_frames {
        let cap_start = i * frame_size;
        let cap_end = cap_start + frame_size;
        let capture = &mic[cap_start..cap_end];

        // Reference (render) frame — use same frame index, or silence if reference is shorter
        let render: Option<&[f32]> = if cap_end <= reference.len() {
            Some(&reference[cap_start..cap_end])
        } else {
            None
        };

        match pipeline.process(capture, render, false, &mut out_frame) {
            Ok(_) => output.extend_from_slice(&out_frame),
            Err(e) => {
                warn!("AEC3 frame {i} failed: {e:?} — using raw frame");
                output.extend_from_slice(capture);
            }
        }
    }

    // Append any tail samples that didn't fill a complete frame, unprocessed
    let processed_samples = total_frames * frame_size;
    if processed_samples < mic.len() {
        output.extend_from_slice(&mic[processed_samples..]);
    }

    info!(
        frames_processed = total_frames,
        input_samples = mic.len(),
        output_samples = output.len(),
        "AEC: echo cancellation complete"
    );

    output
}

/// Returns the sample index of the first speech onset using VAD.
fn first_speech_onset(audio: &[f32]) -> Option<usize> {
    let segments = vad::find_speech_segments(audio, SAMPLE_RATE, 0.3, 0.002, 1.0);
    segments.first().map(|s| s.start_sample)
}
