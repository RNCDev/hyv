use crate::transcription::engine::TranscribedSegment;
use chrono::Local;
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

/// Write a transcript to the Desktop as a plain text file.
pub fn write_transcript(
    segments: &mut [TranscribedSegment],
    duration_secs: f64,
) -> Result<PathBuf, String> {
    let desktop = dirs::desktop_dir().ok_or("Cannot find Desktop directory")?;
    let now = Local::now();
    let filename = format!("Hyv_Transcript_{}.txt", now.format("%Y-%m-%d_%H-%M"));
    let path = desktop.join(&filename);

    // Sort by start time
    segments.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());

    // Count unique speakers
    let speaker_count = segments
        .iter()
        .map(|s| s.speaker.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();

    let mut file =
        std::fs::File::create(&path).map_err(|e| format!("Failed to create transcript: {e}"))?;

    // Header
    let duration_str = format_duration(duration_secs);
    let date_str = now.format("%B %d, %Y at %-I:%M %p").to_string();

    writeln!(file, "=== Hyv Transcript ===").ok();
    writeln!(file, "Date: {date_str}").ok();
    writeln!(file, "Duration: {duration_str}").ok();
    writeln!(file, "Speakers: {speaker_count}").ok();
    writeln!(file, "========================").ok();
    writeln!(file).ok();

    // Segments
    for seg in segments.iter() {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let ts = format_timestamp(seg.start);
        writeln!(file, "[{ts}] {}: {text}", seg.speaker).ok();
    }

    writeln!(file).ok();
    writeln!(file, "=== End of Transcript ===").ok();

    info!(path = %path.display(), segments = segments.len(), "Transcript written");
    Ok(path)
}

fn format_timestamp(secs: f64) -> String {
    let total = secs as u64;
    let hours = total / 3600;
    let mins = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours:02}:{mins:02}:{secs:02}")
    } else {
        format!("{mins:02}:{secs:02}")
    }
}

fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins}:{secs:02}")
}
