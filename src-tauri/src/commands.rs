use crate::audio::aec;
use crate::audio::capture::{MicCapture, SystemCapture};
use crate::audio::normalize;
use crate::audio::vad;
use crate::debug;
use crate::output::transcript_writer;
use crate::state::{AppState, AppStatus, ProgressPayload};
use crate::transcription::chunker;
use crate::transcription::engine::{TranscribedSegment, WhisperEngine};
use crate::transcription::model_manager::{ModelInfo, ModelManager};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, info};

const SAMPLE_RATE: u32 = 16000;
const VAD_ENERGY_THRESHOLD: f32 = 0.002;
const VAD_MIN_DURATION: f64 = 0.3;
const VAD_MERGE_GAP: f64 = 1.0;
const PROGRESS_MIC_START: f64 = 10.0;
const PROGRESS_MIC_RANGE: f64 = 40.0;
const PROGRESS_SYS_START: f64 = 55.0;
const PROGRESS_SYS_RANGE: f64 = 40.0;

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    Ok(state.status.lock().await.clone())
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut status = state.status.lock().await;
    if *status != AppStatus::Idle {
        return Err("Cannot start recording: not idle".to_string());
    }

    // Check model is available
    let model_mgr = ModelManager::new()?;
    let model = ModelInfo::medium();
    if !model_mgr.is_downloaded(&model) {
        *status = AppStatus::ModelDownloading {
            progress: 0.0,
            message: "Downloading Whisper model...".to_string(),
        };
        emit_status(&app, &status);
        drop(status);

        // Download in background
        let app_clone = app.clone();

        tokio::spawn(async move {
            let progress_app = app_clone.clone();
            let result = model_mgr
                .download(&model, move |downloaded, total| {
                    let pct = (downloaded as f64 / total as f64) * 100.0;
                    let msg = format!(
                        "Downloading model: {:.0}% ({} / {} MB)",
                        pct,
                        downloaded / 1_000_000,
                        total / 1_000_000
                    );
                    let status = AppStatus::ModelDownloading {
                        progress: pct,
                        message: msg,
                    };
                    let _ = progress_app.emit("status-changed", ProgressPayload { status });
                })
                .await;

            let state: State<'_, AppState> = app_clone.state();
            let mut s = state.status.lock().await;
            match result {
                Ok(_) => {
                    *s = AppStatus::Idle;
                    emit_status(&app_clone, &s);
                    info!("Model downloaded, ready to record");
                }
                Err(e) => {
                    *s = AppStatus::Error {
                        message: format!("Model download failed: {e}"),
                    };
                    emit_status(&app_clone, &s);
                    error!("Model download failed: {e}");
                }
            }
        });

        return Ok(());
    }

    // Clear buffers
    state.system_buffer.lock().await.clear();
    state.mic_buffer.lock().await.clear();
    state.recording_active.store(true, Ordering::SeqCst);

    let mic_buffer = state.mic_buffer.clone();
    let system_buffer = state.system_buffer.clone();
    let active = state.recording_active.clone();

    // Start mic capture in a dedicated thread (CPAL streams are not Send).
    // Use a oneshot channel to wait until the mic is actually running before
    // returning, so stop_recording can't clone empty buffers due to a race.
    let active_clone = active.clone();
    let (mic_ready_tx, mic_ready_rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    std::thread::spawn(move || {
        let mut mic = MicCapture::new();
        match mic.start(mic_buffer, active_clone.clone()) {
            Ok(()) => {
                let _ = mic_ready_tx.send(Ok(()));
            }
            Err(e) => {
                error!("Mic capture failed: {e}");
                let _ = mic_ready_tx.send(Err(e));
                return;
            }
        }
        while active_clone.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        mic.stop();
    });

    // Wait for mic to confirm it started (or failed) before proceeding.
    // This prevents stop_recording from cloning empty buffers if the OS
    // delays scheduling the thread.
    if let Ok(Err(e)) = mic_ready_rx.await {
        error!("Mic capture did not start: {e}");
    }

    // System audio via Core Audio Process Tap
    {
        let sys = SystemCapture::new();
        if let Err(e) = sys.start(system_buffer, active.clone()) {
            // Non-fatal: app works with mic only if system audio fails
            error!("System audio capture failed (will record mic only): {e}");
        }
    }

    *status = AppStatus::Recording;
    let recording_status = status.clone();
    drop(status); // release lock before emitting
    emit_status(&app, &recording_status);
    info!("Recording started");
    Ok(())
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let status = state.status.lock().await;
        if *status != AppStatus::Recording {
            return Err("Not recording".to_string());
        }
    }

    state.recording_active.store(false, Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mic_audio = std::mem::take(&mut *state.mic_buffer.lock().await);
    let system_audio = std::mem::take(&mut *state.system_buffer.lock().await);

    let mic_duration = mic_audio.len() as f64 / 16000.0;
    let sys_duration = system_audio.len() as f64 / 16000.0;
    let duration = mic_duration.max(sys_duration);

    info!(
        mic_samples = mic_audio.len(),
        system_samples = system_audio.len(),
        duration = format!("{:.1}s", duration),
        "Recording stopped, starting processing"
    );

    {
        let mut status = state.status.lock().await;
        *status = AppStatus::Processing {
            progress: 0.0,
            message: "Analyzing audio...".to_string(),
        };
        emit_status(&app, &status);
    }

    let app_clone = app.clone();

    tokio::task::spawn_blocking(move || {
        let result = process_recording(&mic_audio, &system_audio, duration, &app_clone);

        // Use a new tokio runtime handle to update state
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let state: State<'_, AppState> = app_clone.state();
            let mut status = state.status.lock().await;
            match result {
                Ok(path) => {
                    info!(path = %path.display(), "Processing complete");
                    *status = AppStatus::Idle;
                }
                Err(e) => {
                    error!("Processing failed: {e}");
                    *status = AppStatus::Error { message: e };
                }
            }
            emit_status(&app_clone, &status);
        });
    });

    Ok(())
}

fn process_recording(
    mic_audio: &[f32],
    system_audio: &[f32],
    duration: f64,
    app: &AppHandle,
) -> Result<std::path::PathBuf, String> {
    // Save raw audio for debugging before any processing
    debug::save_audio(mic_audio, "mic");
    debug::save_audio(system_audio, "system");

    // Normalize both channels to -16 LUFS before VAD and Whisper.
    // Ensures consistent energy levels for the VAD threshold and improves
    // Whisper accuracy on quiet speech.
    const TARGET_LUFS: f64 = -16.0;
    let mic_audio = normalize::normalize_loudness(mic_audio, SAMPLE_RATE, TARGET_LUFS);
    let system_audio = normalize::normalize_loudness(system_audio, SAMPLE_RATE, TARGET_LUFS);
    debug::save_audio(&mic_audio, "mic_normalized");
    debug::save_audio(&system_audio, "system_normalized");

    // Echo cancellation: detect how far ahead the reference (system audio) is
    // relative to the mic, then pass that as initial_delay_ms to AEC3.
    // Both buffers stay at full length — no trimming of content.
    let mic_audio = if !system_audio.is_empty() {
        let delay_ms = aec::detect_render_delay_ms(&mic_audio, &system_audio);
        let cleaned = aec::cancel_echo(&mic_audio, &system_audio, delay_ms);
        debug::save_audio(&cleaned, "mic_aec");
        cleaned
    } else {
        mic_audio
    };

    let model_mgr = ModelManager::new()?;
    let model_info = ModelInfo::medium();
    let model_path = model_mgr.model_path(&model_info);
    let engine = WhisperEngine::new(&model_path)?;

    let mut all_segments = Vec::new();

    if !mic_audio.is_empty() {
        update_progress(app, 0.0, "Analyzing microphone audio...");
        let mic_segments = process_channel(
            &mic_audio,
            "Speaker 1",
            PROGRESS_MIC_START,
            PROGRESS_MIC_RANGE,
            &engine,
            app,
        )?;
        all_segments.extend(mic_segments);
    }

    if !system_audio.is_empty() {
        update_progress(app, 50.0, "Analyzing system audio...");
        let sys_segments = process_channel(
            &system_audio,
            "Speaker 2",
            PROGRESS_SYS_START,
            PROGRESS_SYS_RANGE,
            &engine,
            app,
        )?;
        all_segments.extend(sys_segments);
    }

    // Align channel timestamps: the mic and system buffers may have different
    // amounts of leading silence (e.g. mic starts capturing before the system
    // audio tap begins draining). Detect the offset by comparing the first
    // speech onset in each channel and shift Speaker 1 timestamps to match.
    align_channels(&mut all_segments);

    // Save pre-dedup segments so we can inspect what Whisper produced
    debug::save_segments(&all_segments, "segments_raw");

    let mut all_segments = deduplicate_bleed(all_segments);

    update_progress(app, 95.0, "Writing transcript...");
    let path = transcript_writer::write_transcript(&mut all_segments, duration)?;
    update_progress(app, 100.0, "Done");
    Ok(path)
}

fn process_channel(
    audio: &[f32],
    speaker: &str,
    progress_start: f64,
    progress_range: f64,
    engine: &WhisperEngine,
    app: &AppHandle,
) -> Result<Vec<TranscribedSegment>, String> {
    let speech = vad::find_speech_segments(
        audio,
        SAMPLE_RATE,
        VAD_MIN_DURATION,
        VAD_ENERGY_THRESHOLD,
        VAD_MERGE_GAP,
    );
    info!(segments = speech.len(), speaker, "VAD complete");

    let chunks = chunker::chunk_speech(audio, &speech, SAMPLE_RATE);
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    update_progress(
        app,
        progress_start,
        &format!("Transcribing {speaker} ({} chunks)...", chunks.len()),
    );

    engine.transcribe_channel(&chunks, speaker, |done, total| {
        let pct = progress_start + (done as f64 / total as f64) * progress_range;
        update_progress(app, pct, &format!("Transcribing {speaker}: {done}/{total}"));
    })
}

/// Align Speaker 1 (mic) timestamps to Speaker 2 (system) by detecting the
/// offset between each channel's first speech onset. The mic buffer often has
/// more leading audio because CPAL starts before the system audio tap drains.
fn align_channels(segments: &mut [TranscribedSegment]) {
    let s1_first = segments
        .iter()
        .filter(|s| s.speaker == "Speaker 1")
        .map(|s| s.start)
        .reduce(f64::min);
    let s2_first = segments
        .iter()
        .filter(|s| s.speaker == "Speaker 2")
        .map(|s| s.start)
        .reduce(f64::min);

    let (Some(s1), Some(s2)) = (s1_first, s2_first) else {
        return; // Only one channel present, nothing to align
    };

    let offset = s1 - s2;
    // Only correct if the offset is significant (>3s). Smaller offsets are
    // likely conversational timing (e.g. AI speaks first, user responds later)
    // rather than a genuine buffer-start misalignment. Over-correcting pushes
    // bleed segments to wrong timestamps and confuses dedup.
    if offset.abs() <= 3.0 {
        return;
    }

    info!(
        offset = format!("{:.2}s", offset),
        "Aligning Speaker 1 timestamps (shifting by -{:.2}s)", offset
    );

    for seg in segments.iter_mut() {
        if seg.speaker == "Speaker 1" {
            seg.start -= offset;
            seg.end -= offset;
        }
    }
}

fn deduplicate_bleed(segments: Vec<TranscribedSegment>) -> Vec<TranscribedSegment> {
    const TIME_WINDOW: f64 = 5.0;
    const SIMILARITY_THRESHOLD: f64 = 0.65;
    // Short "Speaker 1" segments (≤ this many words) are never dropped — they're
    // likely genuine brief responses ("Sure", "Cool", "Thanks") that would
    // false-positive against any nearby Remote segment.
    const MIN_WORDS_TO_DEDUP: usize = 3;

    let has_remote = segments.iter().any(|s| s.speaker == "Speaker 2");
    if !has_remote {
        return segments;
    }

    fn words(text: &str) -> Vec<String> {
        text.chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .to_lowercase()
            .split_whitespace()
            .map(String::from)
            .collect()
    }

    // Pre-compute remote data: (start, end, word set)
    let remote_entries: Vec<(f64, f64, HashSet<String>)> = segments
        .iter()
        .filter(|s| s.speaker == "Speaker 2")
        .map(|s| {
            let ws: HashSet<String> = words(&s.text).into_iter().collect();
            (s.start, s.end, ws)
        })
        .collect();

    let mut dropped = 0usize;
    let mut result = Vec::with_capacity(segments.len());
    for seg in segments {
        if seg.speaker != "Speaker 1" {
            result.push(seg);
            continue;
        }

        let me_words = words(&seg.text);
        let me_set: HashSet<String> = me_words.iter().cloned().collect();

        // Short responses are always kept
        if me_set.len() <= MIN_WORDS_TO_DEDUP {
            result.push(seg);
            continue;
        }

        // Gather the union of all Remote words from segments that overlap in
        // time (within TIME_WINDOW of the Me segment's start OR end). This
        // handles the case where a long Me segment spans multiple short Remote
        // segments — we check against the combined Remote content.
        let mut remote_union: HashSet<String> = HashSet::new();
        for (r_start, r_end, r_words) in &remote_entries {
            let time_overlap = (r_start - seg.start).abs() <= TIME_WINDOW
                || (r_end - seg.end).abs() <= TIME_WINDOW
                || (*r_start >= seg.start && *r_start <= seg.end)
                || (seg.start >= *r_start && seg.start <= *r_end);
            if time_overlap {
                remote_union.extend(r_words.iter().cloned());
            }
        }

        if remote_union.is_empty() {
            result.push(seg);
            continue;
        }

        // What fraction of the Me words appear in nearby Remote segments?
        let matched = me_set.intersection(&remote_union).count();
        let ratio = matched as f64 / me_set.len() as f64;

        if ratio > SIMILARITY_THRESHOLD {
            dropped += 1;
            info!(
                text = seg.text,
                start = format!("{:.1}s", seg.start),
                ratio = format!("{:.2}", ratio),
                "Dedup: dropping Me segment (bleed)"
            );
        } else {
            result.push(seg);
        }
    }

    info!(dropped, "Deduplication: removed bleed-through Me segments");
    result
}

fn update_progress(app: &AppHandle, progress: f64, message: &str) {
    let _ = app.emit(
        "status-changed",
        ProgressPayload {
            status: AppStatus::Processing {
                progress,
                message: message.to_string(),
            },
        },
    );
}

fn emit_status(app: &AppHandle, status: &AppStatus) {
    let _ = app.emit(
        "status-changed",
        ProgressPayload {
            status: status.clone(),
        },
    );
}

#[tauri::command]
pub async fn get_recent_transcripts() -> Result<Vec<String>, String> {
    let desktop = dirs::desktop_dir().ok_or("Cannot find Desktop")?;
    let mut transcripts: Vec<(String, std::time::SystemTime)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&desktop) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("Hyv_Transcript_") && name.ends_with(".txt") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        transcripts.push((entry.path().to_string_lossy().to_string(), modified));
                    }
                }
            }
        }
    }

    transcripts.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(transcripts.into_iter().take(5).map(|(p, _)| p).collect())
}

#[tauri::command]
pub async fn open_transcript(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open file: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_transcript(path: String) -> Result<(), String> {
    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete file: {e}"))
}
