use aec3::voip::VoipAec3;
use tracing::{info, warn};

use crate::audio::vad;

const SAMPLE_RATE: u32 = 16000;

/// Detect the render-ahead delay in milliseconds by comparing the first speech
/// onset of the reference (system/far-end) vs the mic (near-end).
///
/// AEC3 needs to know how many ms ahead the reference signal is relative to
/// the capture signal. In a typical call the AI starts speaking before the user
/// responds, so `sys_onset < mic_onset`, giving a positive delay.
///
/// Returns 0 if either channel has no speech or if the computed delay is
/// negative (mic starts first — unusual, but AEC can handle delay=0).
pub fn detect_render_delay_ms(mic: &[f32], reference: &[f32]) -> u32 {
    let mic_onset = first_speech_onset(mic);
    let sys_onset = first_speech_onset(reference);

    let (mic_onset, sys_onset) = match (mic_onset, sys_onset) {
        (Some(m), Some(s)) => (m, s),
        _ => {
            info!("AEC delay detect: one channel has no speech — using delay=0");
            return 0;
        }
    };

    let offset_samples = mic_onset as i64 - sys_onset as i64;
    let offset_ms = offset_samples * 1000 / SAMPLE_RATE as i64;

    // Negative means mic started before system (rare). Use 0 — AEC still works,
    // it will adapt. Very large values (>5s) are implausible; clamp to 5000ms.
    let delay_ms = offset_ms.clamp(0, 5000) as u32;

    info!(
        mic_onset_ms = mic_onset * 1000 / SAMPLE_RATE as usize,
        sys_onset_ms = sys_onset * 1000 / SAMPLE_RATE as usize,
        offset_ms,
        delay_ms,
        "AEC: detected render-ahead delay"
    );

    delay_ms
}

/// Cancel echo from `mic` using `reference` (system audio) as the far-end signal.
/// `initial_delay_ms` tells AEC3 how far ahead the reference is — use
/// `detect_render_delay_ms()` to compute this before calling.
///
/// Processes in 10ms frames using WebRTC AEC3 (pure Rust). Both buffers are
/// used at their full length — no trimming. Returns the cleaned mic signal.
pub fn cancel_echo(mic: &[f32], reference: &[f32], initial_delay_ms: u32) -> Vec<f32> {
    let mut pipeline = match VoipAec3::builder(SAMPLE_RATE as usize, 1, 1)
        .initial_delay_ms(initial_delay_ms as i32)
        .build()
    {
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

        // Reference (render) frame — silence if reference is shorter (tail of mic after system ended)
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
        initial_delay_ms,
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
