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
| Target | macOS 14.0+, Apple Silicon |

---

## Versioning

**Single source of truth: `src-tauri/Cargo.toml`**

When bumping the version, update **both**:
1. `src-tauri/Cargo.toml` — read by `env!("CARGO_PKG_VERSION")` at compile time
2. `package.json` — injected as `__APP_VERSION__` Vite define, displayed in UI

`src-tauri/tauri.conf.json` has **no** `version` field — Tauri falls back to `Cargo.toml`.

---

## Project Structure

```
hyv/
├── package.json              # frontend version + npm scripts
├── vite.config.ts            # defines __APP_VERSION__, devPort=1420, hmrPort=1421
├── src/                      # React frontend
│   ├── App.tsx               # Root layout
│   ├── components/
│   │   ├── RecordingControls.tsx  # Start/Stop button + progress bar
│   │   ├── StatusIndicator.tsx    # Icon + status message
│   │   └── TranscriptList.tsx     # Recent transcripts, polls every 5s
│   ├── hooks/useAppState.ts  # Status state, Tauri events + 1s polling fallback
│   └── lib/commands.ts       # Tauri invoke wrappers + AppStatus union type
└── src-tauri/
    ├── Cargo.toml            # Rust deps + authoritative version
    └── src/
        ├── lib.rs            # Tauri builder: tray toggle, Accessory policy, logging
        ├── state.rs          # AppState, AppStatus enum, ProgressPayload
        ├── commands.rs       # All Tauri commands + processing pipeline
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
        ├── output/
        │   └── transcript_writer.rs  # merge_segments() + write .txt to Desktop
        └── bin/
            └── replay_pipeline.rs    # Dev-only: offline WER scoring against ground truth
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

Frontend receives status via `status-changed` Tauri events + 1s polling fallback.

---

## Processing Pipeline

```
After stop_recording:

mic_buffer
  → normalize (-16 LUFS, EBU R128)
  → AEC echo cancellation (delay-aware via VAD onset detection)
  → VAD (energy 0.002, min 0.3s, merge 1.0s, 200ms hangover)
  → chunk (max 30s) → Whisper Greedy → Speaker 1 segments

system_buffer
  → normalize (-16 LUFS, EBU R128)
  → VAD → chunk → Whisper BeamSearch(5) → Speaker 2 segments

all_segments
  → align_channels() — shift Speaker 1 timestamps if buffer offset >8s
  → deduplicate_bleed() — drop Speaker 1 bleed matching Speaker 2
  → merge_segments() — merge same-speaker segments within 2s gap
  → ~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt
```

Progress emits: 0% → 10% (mic VAD) → 10–50% (mic Whisper) → 50% → 55–95% (system Whisper) → 95% → 100%

---

## Audio Capture

### Microphone (MicCapture — CPAL)
- Default input device, F32 and I16 formats → mono, linear-resampled to 16kHz
- CPAL callback uses `buffer.try_lock()` — drops samples on lock contention (rare)
- Runs in dedicated `std::thread` (CPAL streams not `Send`)
- Oneshot channel signals when stream is running — `start_recording` awaits before returning

### System Audio (SystemCapture — cidre Core Audio Process Tap)
- `ca::TapDesc::with_mono_global_tap_excluding_processes()` — captures all system output
- Audio C callback → lock-free ring buffer (131,072 samples ≈ 8s at 16kHz)
- Drain thread: every 50ms, pop from ring, resample, `try_lock` push to `system_buffer`
- Non-fatal: if tap fails, records mic-only

---

## Audio Processing

### EBU R128 Normalization
- Target: -16 LUFS; hard limiter clamps to ±1.0; skips silence
- Debug WAVs: `mic_normalized_*.wav`, `system_normalized_*.wav`

### WebRTC AEC3
- `detect_render_delay_ms()`: VAD onset comparison → passed as `initial_delay_ms`
- `cancel_echo()`: 10ms frames (160 samples @ 16kHz); system audio is reference
- Falls back to raw mic if AEC3 init fails

### VAD
- 30ms frames, RMS energy threshold 0.002, 200ms hangover (7 frames)

---

## Whisper Configuration

- Model: `ggml-medium.bin` from `~/Library/Application Support/Hyv/models/`
- Mic channel: Greedy (best_of=1). System channel: BeamSearch (beam_size=5, patience=1.0)
- `no_speech_thold=0.6`, `entropy_thold=2.4`, `logprob_thold=-1.0`, `thold_pt=0.01`
- Language: `"en"` (hardcoded); new `WhisperState` per chunk (stateless)

---

## Post-Processing

### align_channels()
- If first-segment offset between Speaker 1 and Speaker 2 > 8s: shifts Speaker 1 timestamps by `-offset`

### deduplicate_bleed()
- Drops Speaker 1 segments that are mic echo of Speaker 2
- Interval: `LOOK_BACK=5.0s` before seg start, `LOOK_FORWARD=1.0s` after seg end
- Similarity: >55% word overlap (matched words / Speaker 1 word count)
- Guard: segments ≤2 words (total) are never dropped

### merge_segments()
- Merges consecutive same-speaker segments within 2s gap

---

## Debug Artifacts

Saved to `~/Library/Application Support/Hyv/debug/`. Pruned after 7 days.

| File | Contents |
|---|---|
| `mic_*.wav` / `system_*.wav` | Raw buffers |
| `mic_normalized_*.wav` / `system_normalized_*.wav` | After normalization |
| `mic_aec_*.wav` | After AEC |
| `segments_raw_*.json` | All Whisper segments before dedup |

Logs: `~/Library/Logs/Hyv/hyv.log.YYYY-MM-DD`

### Replay Harness (dev only)

Runs the post-normalization pipeline offline against saved WAVs:

```bash
cargo run --bin replay_pipeline -- \
  --mic ~/Library/Application\ Support/Hyv/debug/mic_normalized_*.wav \
  --system ~/Library/Application\ Support/Hyv/debug/system_normalized_*.wav \
  --ground-truth docs/test-fixtures/vapi-demo-ground-truth.txt
```

---

## Build & Run

```bash
npm install
npm run tauri dev        # dev: hot-reload frontend, auto-recompile Rust
npm run tauri build      # production: src-tauri/target/release/bundle/macos/Hyv.app
RUST_LOG=debug npm run tauri dev  # verbose Rust logs
```

Permissions required: **Microphone** + **Screen Recording** (Core Audio Process Tap).

---

## Known Limitations

- English only
- Processing ~2–3× real-time (medium model, M-series)
- Ring buffer ~8s — long system audio bursts could overflow
- No SHA256 check on downloaded model
- Filename collision at minute precision
- System audio failure is silent (mic-only fallback, no UI warning)
- All remote speakers labeled "Speaker 2" — no diarization

---

## Design Principles

- **Local-first** — Whisper on-device via Metal, zero cloud dependency
- **Accuracy over speed** — full post-processing batch after stop, not real-time streaming
- **Channel separation** — mic and system audio captured and transcribed independently
- **Manual control** — user starts/stops explicitly, no auto-detection
