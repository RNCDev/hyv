use ebur128::{EbuR128, Mode};
use tracing::{info, warn};

/// Normalize audio to a target integrated loudness (LUFS) using EBU R128.
/// Returns a new Vec<f32> with consistent loudness and hard-limited peaks.
/// If the audio is silence or too short to measure, returns the original unchanged.
pub fn normalize_loudness(audio: &[f32], sample_rate: u32, target_lufs: f64) -> Vec<f32> {
    if audio.is_empty() {
        return audio.to_vec();
    }

    let mut meter = match EbuR128::new(1, sample_rate, Mode::I) {
        Ok(m) => m,
        Err(e) => {
            warn!("EBU R128 init failed: {e} — skipping normalization");
            return audio.to_vec();
        }
    };

    if let Err(e) = meter.add_frames_f32(audio) {
        warn!("EBU R128 measurement failed: {e} — skipping normalization");
        return audio.to_vec();
    }

    let measured = match meter.loudness_global() {
        Ok(l) => l,
        Err(e) => {
            warn!("EBU R128 loudness read failed: {e} — skipping normalization");
            return audio.to_vec();
        }
    };

    // Silence or unmeasureable — don't apply gain
    if !measured.is_finite() {
        info!("Normalization skipped: audio is silence");
        return audio.to_vec();
    }

    let gain_db = target_lufs - measured;
    let gain_linear = 10f64.powf(gain_db / 20.0) as f32;

    // Hard limiter: clamp to ±1.0 after gain
    let normalized: Vec<f32> = audio.iter().map(|&s| (s * gain_linear).clamp(-1.0, 1.0)).collect();

    // Log peak after normalization
    let peak = normalized.iter().cloned().fold(0.0f32, f32::max);

    info!(
        measured_lufs = format!("{:.1}", measured),
        gain_db = format!("{:+.1}", gain_db),
        peak_after = format!("{:.3}", peak),
        samples = audio.len(),
        "Normalized audio loudness"
    );

    // Re-feed normalized samples for a second measurement to confirm (debug only)
    #[cfg(debug_assertions)]
    {
        let mut check = EbuR128::new(1, sample_rate, Mode::I).unwrap();
        let _ = check.add_frames_f32(&normalized);
        if let Ok(l) = check.loudness_global() {
            info!(confirmed_lufs = format!("{:.1}", l), "Post-normalization LUFS");
        }
    }

    normalized
}
