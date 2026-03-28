# Hyv v0.2.1 — Internal Architecture Reference

## What is this?
A macOS menu bar app (Tauri 2 + Rust + React/TypeScript) that records system audio + microphone as separate streams, then produces speaker-labeled transcripts on the Desktop using on-device Whisper (Metal GPU).

**No cloud. No API keys. No Python.**

---

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | React 19, TypeScript, Vite 6 |
| Backend | Rust, Tauri 2 |
| Transcription | whisper-rs 0.14 (ggml-medium.bin, Metal GPU) |
| System audio | cidre (Core Audio Process Tap, macOS only) |
| Microphone | CPAL 0.15 |
| Async runtime | tokio 1 (full) |
| Logging | tracing + tracing-subscriber |
| HTTP (model download) | reqwest 0.12 |
| Timestamps / paths | chrono 0.4, dirs 6 |
| Target | macOS 14.0+, Apple Silicon |

---

## Project Structure

```
hyv/
├── index.html
├── package.json              # version, npm scripts
├── vite.config.ts            # devPort=1420, hmrPort=1421
├── tsconfig.json
├── src/                      # React frontend
│   ├── main.tsx              # ReactDOM.createRoot, StrictMode
│   ├── App.tsx               # Root layout: StatusIndicator + RecordingControls + TranscriptList
│   ├── components/
│   │   ├── RecordingControls.tsx  # Start/Stop button + progress bar
│   │   ├── StatusIndicator.tsx    # Icon + status message
│   │   └── TranscriptList.tsx     # Recent transcripts, open + delete, polls every 5s
│   ├── hooks/
│   │   └── useAppState.ts    # Status state, Tauri event listener + 1s polling fallback
│   └── lib/
│       └── commands.ts       # Tauri invoke wrappers + AppStatus union type
└── src-tauri/
    ├── tauri.conf.json       # Window (360×500), tray icon, bundle, CSP null
    ├── Cargo.toml            # All Rust deps
    ├── build.rs              # tauri-build
    └── src/
        ├── main.rs           # Binary entry → lib::run()
        ├── lib.rs            # Tauri builder: tray toggle, Accessory policy, command registration
        ├── state.rs          # AppState, AppStatus enum, ProgressPayload
        ├── commands.rs       # All Tauri commands
        ├── audio/
        │   ├── capture.rs    # MicCapture (CPAL) + SystemCapture (Core Audio tap)
        │   └── vad.rs        # Energy-based VAD
        ├── transcription/
        │   ├── engine.rs     # WhisperEngine: load model, transcribe_channel
        │   ├── model_manager.rs  # Download ggml model, check existence
        │   └── chunker.rs    # VAD segments → 30s audio chunks for Whisper
        └── output/
            └── transcript_writer.rs  # Write .txt to Desktop
```

---

## State Machine

```
Idle
  → (click Start, model missing) → ModelDownloading → Idle   (must click Start again)
  → (click Start, model ready)  → Recording
                                       → (click Stop) → Processing → Idle
                                                                   → Error
```

`AppStatus` in `state.rs`:
```rust
#[serde(tag = "type", content = "data")]
pub enum AppStatus {
    Idle,
    Recording,
    ModelDownloading { progress: f64, message: String },
    Processing { progress: f64, message: String },
    Error { message: String },
}
```

Frontend receives status via:
1. `status-changed` Tauri events (emitted after mutex released in each command)
2. 1-second polling fallback in `useAppState.ts` (guards against missed events)

---

## Recording Pipeline

### start_recording (commands.rs)

1. Acquire `status` mutex — reject if not `Idle`
2. Check `ModelManager::is_downloaded()` → if missing, emit `ModelDownloading`, spawn async download, return early
3. Clear `mic_buffer` and `system_buffer`
4. Set `recording_active = true`
5. Spawn std::thread for mic (CPAL streams are not Send):
   - `MicCapture::start(mic_buffer, active)` — builds CPAL input stream at device rate, resamples to 16kHz mono, `try_lock` pushes samples to shared buffer
   - Send `Ok(())` on oneshot channel when stream is running
6. **Await oneshot** — blocks until mic is confirmed running (prevents empty buffer race on fast stop)
7. Start system audio inline: `SystemCapture::start(system_buffer, active)` — non-fatal if fails
   - Core Audio Process Tap via cidre captures ALL system output
   - Drain thread pops from ring buffer every 50ms into `system_buffer`
8. Drop status mutex, emit `Recording` status, return `Ok(())`

### stop_recording (commands.rs)

1. Verify status is `Recording`
2. `recording_active.store(false)` → mic and drain threads exit their loops
3. `tokio::time::sleep(200ms)` — let threads finish
4. Clone both buffers; calculate duration
5. Emit `Processing` status
6. `spawn_blocking(process_recording)` — heavy work off the async runtime

### process_recording (commands.rs)

All runs in a blocking thread. Sample rate: 16000 Hz.

```
mic_buffer → vad::find_speech_segments(threshold=0.002 RMS, min_dur=0.3s, merge_gap=1.0s)
           → chunker::chunk_speech (max 30s chunks)
           → WhisperEngine::transcribe_channel → Vec<TranscriptSegment> speaker="Me"

system_buffer → same VAD + chunking
              → WhisperEngine::transcribe_channel → Vec<TranscriptSegment> speaker="Remote"

all_segments.sort_by(start_time)
→ transcript_writer::write_transcript → ~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt
```

Progress emits: 0% → 10% (mic VAD) → 10–50% (mic Whisper) → 50% → 55–95% (system Whisper) → 95% → 100%

---

## Audio Capture Details

### Microphone (MicCapture — CPAL)

- Default input device, supports F32 and I16 sample formats
- Converts to mono (average channels), linear-resamples to 16kHz
- CPAL callback uses `buffer.try_lock()` — drops samples on lock contention (rare)
- Runs in dedicated `std::thread` (CPAL streams not `Send`)

### System Audio (SystemCapture — cidre Core Audio Process Tap)

- `ca::TapDesc::with_mono_global_tap_excluding_processes()` — captures all system output
- Wrapped in aggregate device named `"hyv-audio-tap"`, private=true
- Audio C callback → lock-free ring buffer (131072 samples ≈ 8s at 16kHz)
- Drain thread: every 50ms, pop from ring, resample if needed, `try_lock` push to `system_buffer`
- Non-fatal: if tap fails, app records mic-only (no user warning currently)

---

## VAD Algorithm (audio/vad.rs)

- Split audio into 30ms frames (480 samples @ 16kHz)
- RMS energy per frame: `sqrt(Σ(s²) / n)`
- State machine: silence → speech onset (energy > threshold) → speech → silence (energy < threshold)
- Discard segments < `min_duration` (300ms)
- Merge segments separated by < `merge_gap` (1.0s)
- Called with: `energy_threshold=0.002`, `min_duration=0.3`, `merge_gap=1.0`

---

## Whisper Configuration (transcription/engine.rs)

- Model: `ggml-medium.bin`, loaded from `~/Library/Application Support/Hyv/models/`
- GPU: Metal (Apple Silicon), `use_gpu=true`
- Threads: 4
- Language: `"en"` (hardcoded)
- Strategy: Greedy (best_of=1)
- `no_speech_thold`: 0.6 (suppresses low-confidence segments — Meetily)
- `entropy_thold`: 2.4 (rejects high-entropy/hallucinated segments — Meetily)
- `logprob_thold`: -1.0 (rejects low average token log-probability — Meetily)
- `thold_pt`: 0.01 (minimum per-token timestamp probability)
- New `WhisperState` per chunk (stateless)
- Timestamps in centiseconds, converted to seconds + chunk offset

---

## Model Manager (transcription/model_manager.rs)

- Storage: `~/Library/Application Support/Hyv/models/ggml-medium.bin`
- Download: streams from HuggingFace via reqwest, writes to `.tmp`, renames on success
- No SHA256 validation (field exists but set to None)
- Progress callback emits `status-changed` events to frontend
- After download → status returns to `Idle` → user must click Start again

---

## Frontend State (hooks/useAppState.ts)

```typescript
// On mount:
getStatus().then(setStatus)               // initial sync
listen("status-changed", e => setStatus(e.payload.status))  // event-driven
setInterval(() => getStatus().then(setStatus), 1000)         // 1s fallback poll

// Recording timer: increments every 1s when status.type === "Recording"
```

---

## Transcript Output (output/transcript_writer.rs)

```
~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt

=== Hyv Transcript ===
Date: March 28, 2026 at 9:00 AM
Duration: 5:12
Speakers: 2
========================

[00:03] Speaker 1: ...
[00:07] Speaker 2: ...

=== End of Transcript ===
```

**Note:** Filename has minute precision — recordings within the same minute overwrite.

---

## Build & Run

```bash
npm install
npm run tauri dev        # dev: hot-reload frontend, auto-recompile Rust
npm run tauri build      # production: src-tauri/target/release/bundle/macos/Hyv.app
RUST_LOG=debug npm run tauri dev  # verbose Rust logs
```

### macOS Permissions Required

- **Microphone** — CPAL mic capture
- **Screen Recording** — Core Audio Process Tap (macOS 14 requirement)

Both prompted automatically on first use. Reset if accidentally denied:
```bash
tccutil reset Microphone com.hyv.app
tccutil reset ScreenCapture com.hyv.app
```

### System Dependencies (build-time)

```bash
xcode-select --install   # Xcode CLI tools (Metal, clang)
brew install cmake        # Required by whisper-rs build
brew install node         # Node 18+
curl ... | sh             # Rust via rustup
```

---

## Known Limitations / Future Work

- **English only** — `params.set_language(Some("en"))` in `engine.rs`
- **Single speaker per channel** — no diarization on system audio; all system speakers labeled "Speaker 2"
- **Processing time** — medium model: ~2–3× real-time on M3. 10-min recording → ~20–30 min to process
- **Ring buffer** — 131,072 samples (~8s) — very long system audio bursts could overflow before drain
- **No SHA256 check** on downloaded model
- **Filename collision** — minute-precision timestamp; two recordings in same minute overwrite
- **System audio failure is silent** — user not notified if tap fails (mic-only fallback)
- **`rubato` imported but unused** — linear resampling used instead (lower quality)

See [docs/future-improvements.md](docs/future-improvements.md) for planned enhancements sourced from reference repositories.

---

## Design Principles

- **Local-first** — Whisper on-device via Metal, zero cloud dependency
- **Accuracy over speed** — full post-processing batch after stop, not real-time streaming
- **Channel separation** — mic and system audio captured and transcribed independently
- **Manual control** — user starts/stops explicitly, no auto-detection

---

## Reference Repositories

Similar apps to study for proven patterns and configuration values. Check these when investigating improvements to audio capture, VAD, transcription, or diarization.

### Meetily — [Zackriya-Solutions/meetily](https://github.com/Zackriya-Solutions/meetily/tree/main/backend)

Meeting transcription with whisper.cpp server + FastAPI backend.

- **Whisper tuning:** `no_speech_thold=0.6`, `entropy_thold=2.4`, `logprob_thold=-1.0`, `word_thold=0.01`
- **Diarization:** Stereo energy-based — 1.1x amplitude ratio threshold between channels to determine dominant speaker
- **Streaming:** 200ms overlap between consecutive processed segments for continuity
- **Key files:** `backend/whisper-custom/server/server.cpp` (Whisper params), `backend/app/transcript_processor.py` (text chunking)

### Hyprnote — [bahodirr/hyprnote](https://github.com/bahodirr/hyprnote)

Tauri + Rust meeting transcriber (closest architecture to Hyv).

- **Dual VAD:** Silero VAD (`silero-rs`) + Ten-VAD ONNX model bundled via `include_bytes!()`
- **Audio normalization:** EBU R128 targeting -23 LUFS with true peak limiting (10ms lookahead)
- **Model:** Quantized `ggml-small-q8_0.bin` for speed (vs our unquantized medium)
- **macOS capture:** Same cidre + aggregate device approach as Hyv, with lock-free ring buffers
- **Streaming transcription:** Real-time per-segment output via `TranscriptionTask<S, T>` struct
- **Speaker embeddings:** MFCC-based via ONNX, cosine distance for speaker discrimination
- **Key crates:** `silero-rs`, `ten-vad-rs`, `knf_rs` (MFCC features), `dasp` (resampling)

### Minute — [roblibob/minute](https://github.com/roblibob/minute)

macOS Apple Silicon meeting app with FluidAudio (Parakeet ASR).

- **VAD:** RMS silence threshold 0.03, 0.75s transient tolerance, auto-stop after 120s silence
- **Diarization:** FluidAudio with 0.55 clustering threshold, 0.25s silence gap, 0.4s min speech duration, 1.0s chunk overlap
- **Vocabulary boosting:** Three strength levels (gentle/balanced/aggressive) for domain terms
- **Output:** Obsidian-compatible Markdown with YAML frontmatter, deterministic JSON schema
- **Audio normalization:** Two-pass FFmpeg loudnorm (`I=-16:TP=-1.5:LRA=11`)

### Project Raven — [Laxcorp-Research/project-raven](https://github.com/Laxcorp-Research/project-raven)

Electron meeting recorder with Deepgram Nova-3.

- **Echo cancellation:** GStreamer WebRTC AEC3 (same as Chrome) with adaptive bypass on drift >200ms or high overflow
- **Segment merging:** 2-second window for consecutive same-speaker segments (we adopted this)
- **Session management:** 5000-entry cap with 20% eviction, auto-save every 60s, crash recovery
- **macOS capture:** ScreenCaptureKit for system audio + CoreAudio for mic
- **Transcription:** Deepgram Nova-3 with 300ms endpointing, 1.5s utterance end threshold
- **Storage:** SQLite WAL mode with encrypted API keys
