//! Offline pipeline replay tool.
//!
//! Reads mic_normalized + system_normalized WAVs from a Hyv debug session,
//! runs the post-normalization pipeline (AEC -> VAD -> Whisper -> align -> dedup -> merge),
//! prints the transcript, and optionally computes per-speaker WER against a ground truth.
//!
//! Usage:
//!   cargo run --bin replay_pipeline -- \
//!     --mic ~/Library/Application\ Support/Hyv/debug/mic_normalized_2026-03-28_16-55.wav \
//!     --system ~/Library/Application\ Support/Hyv/debug/system_normalized_2026-03-28_16-55.wav \
//!     [--ground-truth docs/test-fixtures/vapi-demo-ground-truth.txt]

use clap::Parser;
use hound::WavReader;
use hyv_lib::{
    audio::aec,
    commands::{align_channels_pub, deduplicate_bleed_pub, run_channel_pipeline},
    transcription::engine::WhisperEngine,
    transcription::model_manager::{ModelInfo, ModelManager},
    output::transcript_writer,
};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(about = "Replay Hyv pipeline on debug WAV files")]
struct Args {
    /// Path to mic_normalized_*.wav
    #[arg(long)]
    mic: PathBuf,

    /// Path to system_normalized_*.wav
    #[arg(long)]
    system: PathBuf,

    /// Optional ground truth file for WER scoring
    #[arg(long)]
    ground_truth: Option<PathBuf>,
}

fn main() -> Result<(), String> {
    tracing_subscriber::fmt().with_writer(std::io::stderr).init();

    let args = Args::parse();

    let mic_audio = read_wav(&args.mic)?;
    let system_audio = read_wav(&args.system)?;
    let duration = mic_audio.len() as f64 / 16000.0;

    info!(
        mic_samples = mic_audio.len(),
        system_samples = system_audio.len(),
        duration_secs = format!("{:.1}", duration),
        "Loaded WAV files"
    );

    // AEC (same logic as process_recording)
    let mic_audio = if !system_audio.is_empty() {
        match aec::detect_render_delay_ms(&mic_audio, &system_audio) {
            Some(delay_ms) => {
                info!(delay_ms, "AEC: cancelling echo");
                aec::cancel_echo(&mic_audio, &system_audio, delay_ms)
            }
            None => {
                info!("AEC: skipped");
                mic_audio
            }
        }
    } else {
        mic_audio
    };

    let model_mgr = ModelManager::new().map_err(|e| e.to_string())?;
    let model_info = ModelInfo::medium();
    let model_path = model_mgr.model_path(&model_info);
    let engine = WhisperEngine::new(&model_path)?;

    let progress = |_frac: f64, msg: &str| eprintln!("  {msg}");

    eprintln!("--- Transcribing mic (Speaker 1, Greedy) ---");
    let mut mic_segments =
        run_channel_pipeline(&mic_audio, "Speaker 1", &engine, false, &progress)?;

    eprintln!("--- Transcribing system (Speaker 2, Beam Search) ---");
    let mut sys_segments =
        run_channel_pipeline(&system_audio, "Speaker 2", &engine, true, &progress)?;

    let mut all_segments = Vec::new();
    all_segments.append(&mut mic_segments);
    all_segments.append(&mut sys_segments);

    align_channels_pub(&mut all_segments);
    let mut all_segments = deduplicate_bleed_pub(all_segments);

    let transcript = transcript_writer::format_transcript(&mut all_segments, duration)?;
    println!("{transcript}");

    if let Some(gt_path) = &args.ground_truth {
        let gt_text = std::fs::read_to_string(gt_path)
            .map_err(|e| format!("Failed to read ground truth: {e}"))?;
        score_wer(&all_segments, &gt_text);
    }

    Ok(())
}

fn read_wav(path: &PathBuf) -> Result<Vec<f32>, String> {
    let mut reader = WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV {}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != 16000 {
        return Err(format!(
            "WAV {} has sample rate {}; expected 16000",
            path.display(),
            spec.sample_rate
        ));
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("WAV read error: {e}"))?,
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("WAV read error: {e}"))?
            .into_iter()
            .map(|s| s as f32 / i16::MAX as f32)
            .collect(),
    };
    Ok(samples)
}

fn score_wer(
    segments: &[hyv_lib::transcription::engine::TranscribedSegment],
    ground_truth: &str,
) {
    let mut gt_words: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for line in ground_truth.lines() {
        if let Some((speaker, text)) = line.split_once(": ") {
            gt_words
                .entry(speaker.to_string())
                .or_default()
                .extend(normalize_words(text));
        }
    }

    let mut hyp_words: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for seg in segments {
        hyp_words
            .entry(seg.speaker.clone())
            .or_default()
            .extend(normalize_words(&seg.text));
    }

    let speakers = ["Speaker 1", "Speaker 2"];
    let mut total_gt = 0usize;
    let mut total_err = 0usize;

    for speaker in &speakers {
        let gt = gt_words.get(*speaker).map(|v| v.as_slice()).unwrap_or(&[]);
        let hyp = hyp_words.get(*speaker).map(|v| v.as_slice()).unwrap_or(&[]);
        let edits = levenshtein(gt, hyp);
        let wer = if gt.is_empty() { 0.0 } else { edits as f64 / gt.len() as f64 };
        eprintln!("WER {speaker}: {:.1}% ({edits} edits / {} ref words)", wer * 100.0, gt.len());
        total_gt += gt.len();
        total_err += edits;
    }

    let overall_wer = if total_gt == 0 { 0.0 } else { total_err as f64 / total_gt as f64 };
    eprintln!("WER overall: {:.1}% ({total_err} edits / {total_gt} ref words)", overall_wer * 100.0);
}

fn normalize_words(text: &str) -> Vec<String> {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect()
}

fn levenshtein(a: &[String], b: &[String]) -> usize {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j - 1].min(dp[i - 1][j]).min(dp[i][j - 1])
            };
        }
    }
    dp[m][n]
}
