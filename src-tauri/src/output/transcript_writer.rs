use crate::transcription::engine::TranscribedSegment;
use chrono::Local;
use std::path::PathBuf;
use tracing::info;

/// Build the transcript as a String (no file I/O).
fn build_transcript_string(segments: &mut [TranscribedSegment], duration_secs: f64) -> String {
    segments.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
    let merged = merge_segments(segments);

    let speaker_count = merged
        .iter()
        .map(|s| s.speaker.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();

    let duration_str = format_duration(duration_secs);
    let date_str = Local::now().format("%B %d, %Y at %-I:%M %p").to_string();

    let mut out = String::new();
    use std::fmt::Write as _;
    writeln!(out, "=== Hyv Transcript ===").ok();
    writeln!(out, "Date: {date_str}").ok();
    writeln!(out, "Duration: {duration_str}").ok();
    writeln!(out, "Speakers: {speaker_count}").ok();
    writeln!(out, "========================").ok();
    writeln!(out).ok();

    for seg in &merged {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let ts = format_timestamp(seg.start);
        writeln!(out, "[{ts}] {}: {text}", seg.speaker).ok();
    }

    info!(
        raw = segments.len(),
        merged = merged.len(),
        "Segments merged for output"
    );

    writeln!(out).ok();
    writeln!(out, "=== End of Transcript ===").ok();
    out
}

/// Format transcript as a String without writing to disk.
pub fn format_transcript(
    segments: &mut [TranscribedSegment],
    duration_secs: f64,
) -> Result<String, String> {
    Ok(build_transcript_string(segments, duration_secs))
}

/// Write a transcript to the Desktop as a plain text file.
pub fn write_transcript(
    segments: &mut [TranscribedSegment],
    duration_secs: f64,
) -> Result<PathBuf, String> {
    let desktop = dirs::desktop_dir().ok_or("Cannot find Desktop directory")?;
    let now = Local::now();
    let filename = format!("Hyv_Transcript_{}.txt", now.format("%Y-%m-%d_%H-%M"));
    let path = desktop.join(&filename);

    let content = build_transcript_string(segments, duration_secs);
    std::fs::write(&path, &content)
        .map_err(|e| format!("Failed to create transcript: {e}"))?;

    info!(path = %path.display(), "Transcript written");
    Ok(path)
}

/// Merge consecutive segments from the same speaker when the gap is ≤ 2 seconds.
fn merge_segments(segments: &[TranscribedSegment]) -> Vec<TranscribedSegment> {
    const MERGE_GAP: f64 = 2.0;

    let mut merged: Vec<TranscribedSegment> = Vec::new();
    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let should_merge = merged
            .last()
            .is_some_and(|prev| prev.speaker == seg.speaker && seg.start - prev.end <= MERGE_GAP);
        if should_merge {
            let prev = merged.last_mut().unwrap();
            prev.end = seg.end;
            prev.text.push(' ');
            prev.text.push_str(text);
        } else {
            merged.push(seg.clone());
        }
    }
    merged
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
