# Hyv — Internal Architecture Reference

A macOS menu bar app (Tauri 2 + Rust + React/TypeScript) that records system audio + microphone as separate streams, then produces speaker-labeled transcripts on the Desktop using on-device transcription (Metal GPU).

**No cloud. No API keys. No Python.**

---

## Tech Stack

| Layer | Technology |
|---|---|
| Frontend | React 19, TypeScript, Vite 6 |
| Backend | Rust, Tauri 2 |
| Transcription | Cohere ONNX (default), whisper-rs 0.14 (alternative), Metal GPU |
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
        │   ├── engine.rs     # WhisperEngine + TranscriptionEngine trait
        │   ├── cohere.rs     # CohereEngine: ONNX encoder-decoder, greedy decode
        │   ├── model_manager.rs  # Model registry, download, existence check
        │   ├── chunker.rs    # VAD segments → audio chunks for transcription
        │   ├── onnx_runtime.rs   # ORT init + session factory (used by cohere.rs)
        │   ├── tokenizer.rs  # BPE tokenizer for Cohere output
        │   └── mel.rs        # Mel-spectrogram computation
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

## Build & Run

```bash
npm install
npm run tauri dev        # dev: hot-reload frontend, auto-recompile Rust
npm run tauri build      # production: src-tauri/target/release/bundle/macos/Hyv.app
RUST_LOG=debug npm run tauri dev  # verbose Rust logs
```

Permissions required: **Microphone** + **Screen Recording** (Core Audio Process Tap).
