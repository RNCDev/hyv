use crate::transcription::engine::TranscribedSegment;
use chrono::Local;
use hound::{SampleFormat, WavSpec, WavWriter};
use std::path::PathBuf;
use tracing::{info, warn};

const SAMPLE_RATE: u32 = 16000;

/// Returns ~/Library/Application Support/Hyv/debug/, creating it if needed.
fn debug_dir() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("Hyv/debug");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("Could not create debug dir: {e}");
        return None;
    }
    Some(dir)
}

fn timestamp() -> String {
    Local::now().format("%Y-%m-%d_%H-%M").to_string()
}

/// Save raw audio buffer as a 16kHz mono f32 WAV file.
pub fn save_audio(samples: &[f32], label: &str) {
    let Some(dir) = debug_dir() else { return };
    let ts = timestamp();
    let path = dir.join(format!("{label}_{ts}.wav"));

    let spec = WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    match WavWriter::create(&path, spec) {
        Ok(mut w) => {
            for &s in samples {
                if let Err(e) = w.write_sample(s) {
                    warn!("WAV write error for {label}: {e}");
                    return;
                }
            }
            if let Err(e) = w.finalize() {
                warn!("WAV finalize error for {label}: {e}");
                return;
            }
            info!(path = %path.display(), samples = samples.len(), "Saved debug audio");
        }
        Err(e) => warn!("Could not create WAV file {}: {e}", path.display()),
    }
}

/// Save pre-dedup segment list as JSON.
pub fn save_segments(segments: &[TranscribedSegment], label: &str) {
    let Some(dir) = debug_dir() else { return };
    let ts = timestamp();
    let path = dir.join(format!("{label}_{ts}.json"));

    match serde_json::to_string_pretty(segments) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!("Could not write segments JSON {}: {e}", path.display());
            } else {
                info!(path = %path.display(), count = segments.len(), "Saved debug segments");
            }
        }
        Err(e) => warn!("Could not serialize segments: {e}"),
    }
}

/// Delete debug files older than `days` days. Called at app start to avoid unbounded growth.
pub fn prune_old_files(days: u64) {
    let Some(dir) = debug_dir() else { return };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(days * 86400))
        .unwrap_or(std::time::UNIX_EPOCH);

    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.modified().map(|m| m < cutoff).unwrap_or(false) {
            let _ = std::fs::remove_file(entry.path());
            info!(path = %entry.path().display(), "Pruned old debug file");
        }
    }
}
