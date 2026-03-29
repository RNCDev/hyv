# Hyv — Internal Architecture Reference

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
| Audio processing | ebur128 (normalization), aec3 (echo cancellation) |
| Async runtime | tokio 1 (full) |
| Logging | tracing + tracing-appender (rolling daily log) |
| HTTP | reqwest 0.12 (model download) |
| Timestamps / paths | chrono 0.4, dirs 6 |
| Target | macOS 14.0+, Apple Silicon |

---

## Versioning

**Single source of truth: `src-tauri/Cargo.toml`**

When bumping the version, update **both**:
1. `src-tauri/Cargo.toml` — Rust/Tauri build version (read by `env!("CARGO_PKG_VERSION")` at compile time)
2. `package.json` — frontend version (injected as `__APP_VERSION__` Vite define at build time, displayed in UI)

`src-tauri/tauri.conf.json` intentionally has **no** `version` field — Tauri falls back to `Cargo.toml` automatically.

Do **not** hardcode version strings anywhere else.

---

## Project Structure

```
hyv/
├── package.json              # frontend version + npm scripts
├── vite.config.ts            # defines __APP_VERSION__, devPort=1420, hmrPort=1421
├── src/                      # React frontend
│   ├── App.tsx               # Root layout — displays v{__APP_VERSION__}
│   ├── components/
│   │   ├── RecordingControls.tsx  # Start/Stop button + progress bar
│   │   ├── StatusIndicator.tsx    # Icon + status message
│   │   └── TranscriptList.tsx     # Recent transcripts, open + delete, polls every 5s
│   ├── hooks/
│   │   └── useAppState.ts    # Status state, Tauri event listener + 1s polling fallback
│   └── lib/
│       └── commands.ts       # Tauri invoke wrappers + AppStatus union type
└── src-tauri/
    ├── tauri.conf.json       # Window (360×500), tray icon, bundle, CSP null — no version field
    ├── Cargo.toml            # Rust deps + authoritative version
    └── src/
        ├── main.rs           # Binary entry → lib::run()
        ├── lib.rs            # Tauri builder: tray toggle, Accessory policy, tracing-appender setup
        ├── state.rs          # AppState, AppStatus enum, ProgressPayload
        ├── commands.rs       # All Tauri commands + process_recording pipeline
        ├── debug.rs          # save_audio(), save_segments(), prune_old_files()
        ├── audio/
        │   ├── capture.rs    # MicCapture (CPAL) + SystemCapture (Core Audio tap)
        │   ├── vad.rs        # Energy-based VAD with 200ms hangover
        │   ├── normalize.rs  # EBU R128 loudness normalization
        │   └── aec.rs        # WebRTC AEC3 echo cancellation + delay detection
        ├── transcription/
        │   ├── engine.rs     # WhisperEngine: load model, transcribe_channel
        │   ├── model_manager.rs  # Download ggml model, check existence
        │   └── chunker.rs    # VAD segments → 30s audio chunks for Whisper
        └── output/
            └── transcript_writer.rs  # merge_segments() + write .txt to Desktop
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

Frontend receives status via:
1. `status-changed` Tauri events (emitted after mutex released in each command)
2. 1-second polling fallback in `useAppState.ts` (guards against missed events)

---

## Processing Pipeline

```
After stop_recording:

mic_buffer
  → normalize (-16 LUFS, EBU R128)
  → AEC echo cancellation (aec3, delay-aware via VAD onset detection)
  → VAD (energy 0.002, min 0.3s, merge 1.0s, 200ms hangover)
  → chunk (max 30s)
  → Whisper (Metal GPU) → Speaker 1 segments

system_buffer
  → normalize (-16 LUFS, EBU R128)
  → (AEC reference signal — unchanged)
  → VAD → chunk → Whisper → Speaker 2 segments

all_segments
  → align_channels() — shift Speaker 1 timestamps if buffer offset >3s
  → deduplicate_bleed() — drop Speaker 1 segments that match Speaker 2 (>65% word overlap, 5s window, min 3 words)
  → transcript_writer::merge_segments() — merge same-speaker segments within 2s
  → ~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt
```

Progress emits: 0% → 10% (mic VAD) → 10–50% (mic Whisper) → 50% → 55–95% (system Whisper) → 95% → 100%

---

## Audio Capture

### Microphone (MicCapture — CPAL)
- Default input device, F32 and I16 sample formats
- Converts to mono (average channels), linear-resamples to 16kHz
- CPAL callback uses `buffer.try_lock()` — drops samples on lock contention (rare)
- Runs in dedicated `std::thread` (CPAL streams not `Send`)
- Oneshot channel signals when stream is running — `start_recording` awaits this before returning (prevents empty buffer race on fast stop)

### System Audio (SystemCapture — cidre Core Audio Process Tap)
- `ca::TapDesc::with_mono_global_tap_excluding_processes()` — captures all system output
- Wrapped in aggregate device named `"hyv-audio-tap"`, private=true
- Audio C callback → lock-free ring buffer (131,072 samples ≈ 8s at 16kHz)
- Drain thread: every 50ms, pop from ring, resample if needed, `try_lock` push to `system_buffer`
- Non-fatal: if tap fails, records mic-only (no user warning)

---

## Audio Processing

### EBU R128 Normalization (audio/normalize.rs)
- Target: -16 LUFS (both channels equalized before VAD and Whisper)
- Hard limiter clamps output to ±1.0
- Skips if audio is silence or unmeasurable
- Debug WAVs saved: `mic_normalized_*.wav`, `system_normalized_*.wav`

### WebRTC AEC3 Echo Cancellation (audio/aec.rs)
- `detect_render_delay_ms()`: compares first speech onset (VAD) between mic and system buffers; positive offset → system audio is ahead → passed as `initial_delay_ms` to AEC3
- `cancel_echo()`: processes mic in 10ms frames (160 samples @ 16kHz) via `VoipAec3`; system audio is reference; both buffers stay at full length (no trimming)
- Debug WAV saved: `mic_aec_*.wav`
- Falls back to raw mic if AEC3 init fails

### VAD (audio/vad.rs)
- 30ms frames, RMS energy threshold 0.002
- 200ms hangover (7 frames) — prevents trailing word cutoffs
- Called with: `energy_threshold=0.002`, `min_duration=0.3s`, `merge_gap=1.0s`

---

## Whisper Configuration (transcription/engine.rs)

- Model: `ggml-medium.bin` from `~/Library/Application Support/Hyv/models/`
- GPU: Metal (Apple Silicon)
- Threads: 4, Language: `"en"` (hardcoded), Strategy: Greedy (best_of=1)
- `no_speech_thold`: 0.6 — suppresses low-confidence segments (Meetily)
- `entropy_thold`: 2.4 — rejects high-entropy/hallucinated output (Meetily)
- `logprob_thold`: -1.0 — rejects low average token log-probability (Meetily)
- `thold_pt`: 0.01 — minimum per-token timestamp probability
- New `WhisperState` per chunk (stateless); timestamps in centiseconds → seconds + chunk offset

---

## Post-Processing (commands.rs)

### align_channels()
- Finds first Whisper segment timestamp for Speaker 1 and Speaker 2
- If offset > 3s: shifts all Speaker 1 timestamps by `-offset`
- Threshold is 3s (not 1s) — smaller offsets are conversational timing, not buffer misalignment

### deduplicate_bleed()
- Drops Speaker 1 segments that are echo of Speaker 2 (mic picked up system audio)
- Time window: 5s (bleed segments can drift after timestamp alignment)
- Similarity: >55% word overlap (directional — matched words / Speaker 1 word count)
- Guard: segments ≤3 words are never dropped (brief genuine responses)

### merge_segments() (transcript_writer.rs)
- Merges consecutive same-speaker segments within 2s gap
- Reduces fragmented output from short Whisper chunks

---

## Model Manager (transcription/model_manager.rs)

- Storage: `~/Library/Application Support/Hyv/models/ggml-medium.bin`
- Download: streams from HuggingFace via reqwest, writes to `.tmp`, renames on success
- No SHA256 validation
- After download → status returns to `Idle` → user clicks Start again

---

## Debug Artifacts (debug.rs)

Saved to `~/Library/Application Support/Hyv/debug/` after every recording. Pruned after 7 days.

| File | Contents |
|---|---|
| `mic_*.wav` | Raw mic buffer |
| `system_*.wav` | Raw system audio buffer |
| `mic_normalized_*.wav` | After EBU R128 normalization |
| `system_normalized_*.wav` | After EBU R128 normalization |
| `mic_aec_*.wav` | After AEC echo cancellation |
| `segments_raw_*.json` | All Whisper segments before dedup, with speaker + timestamps |

Logs: `~/Library/Logs/Hyv/hyv.log.YYYY-MM-DD` (rolling daily, tracing-appender)

---

## Transcript Output

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

Filename has minute precision — recordings within the same minute overwrite.

---

## Build & Run

```bash
npm install
npm run tauri dev        # dev: hot-reload frontend, auto-recompile Rust
npm run tauri build      # production: src-tauri/target/release/bundle/macos/Hyv.app
RUST_LOG=debug npm run tauri dev  # verbose Rust logs
```

macOS permissions required (prompted on first use):
- **Microphone** — CPAL mic capture
- **Screen Recording** — Core Audio Process Tap (macOS 14 requirement)

---

## Known Limitations

- English only (`params.set_language(Some("en"))`)
- Single speaker per channel — no per-speaker diarization; all system audio is "Speaker 2"
- Processing ~2–3× real-time (medium model on M-series) — 10-min recording ≈ 20–30 min
- Ring buffer 131,072 samples (~8s) — long system audio bursts could overflow before drain
- No SHA256 check on downloaded model
- Filename collision at minute precision
- System audio failure is silent (mic-only fallback, no UI warning)

See [docs/future-improvements.md](docs/future-improvements.md) for planned enhancements.

---

## Design Principles

- **Local-first** — Whisper on-device via Metal, zero cloud dependency
- **Accuracy over speed** — full post-processing batch after stop, not real-time streaming
- **Channel separation** — mic and system audio captured and transcribed independently
- **Manual control** — user starts/stops explicitly, no auto-detection

---

## Reference Repositories

Check these for proven patterns when investigating improvements to audio capture, VAD, transcription, or diarization.

### Meetily — [Zackriya-Solutions/meetily](https://github.com/Zackriya-Solutions/meetily/tree/main/backend)
whisper.cpp server + FastAPI backend.
- **Whisper tuning (adopted):** `no_speech_thold=0.6`, `entropy_thold=2.4`, `logprob_thold=-1.0`, `word_thold=0.01`
- **Diarization:** Stereo energy-based — 1.1× amplitude ratio threshold between channels

### Hyprnote — [bahodirr/hyprnote](https://github.com/bahodirr/hyprnote)
Tauri + Rust meeting transcriber (closest architecture to Hyv).
- **Dual VAD:** Silero VAD (`silero-rs`) + Ten-VAD ONNX model bundled via `include_bytes!()`
- **Normalization:** EBU R128 targeting -23 LUFS with true peak limiting
- **AEC:** Custom ONNX neural two-stage AEC (FFT-domain + time-domain refinement)
- **Speaker embeddings:** MFCC-based via ONNX, cosine distance — `knf_rs`, `dasp`

### Minute — [roblibob/minute](https://github.com/roblibob/minute)
macOS Apple Silicon app with FluidAudio (Parakeet ASR).
- **Diarization:** 0.55 cosine distance threshold, 0.25s silence gap, 1.0s chunk overlap
- **Vocabulary boosting:** gentle/balanced/aggressive strength levels via initial prompt
- **Normalization:** Two-pass FFmpeg loudnorm `I=-16:TP=-1.5:LRA=11`

### Project Raven — [Laxcorp-Research/project-raven](https://github.com/Laxcorp-Research/project-raven)
Electron recorder with Deepgram Nova-3.
- **AEC:** GStreamer WebRTC AEC3, monitors drift via `GetDriftMs()` (no threshold bypass in code)
- **Segment merging (adopted):** 2s window for consecutive same-speaker segments
- **Session management:** auto-save every 60s, crash recovery, 5000-entry cap
