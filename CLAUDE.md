# Hyv — macOS Meeting Transcription App

## What is this?
A macOS menu bar app (Tauri 2 + Rust + React/TypeScript) that records system audio + microphone separately, then produces speaker-labeled transcripts on the Desktop using on-device Whisper.

## Tech Stack
- **Frontend:** React 18 + TypeScript + Vite
- **Backend:** Rust (Tauri 2), whisper-rs 0.14 (Metal), cidre (Core Audio Process Tap), CPAL (microphone)
- **Target:** macOS 14.0+, Apple Silicon
- **Audio capture:** Core Audio Process Tap (system audio) + CPAL (microphone), both resampled to 16kHz mono
- **Transcription:** whisper-rs (ggml-medium.bin, on-device via Metal)
- **VAD:** Energy-based, custom implementation in `src-tauri/src/audio/vad.rs`
- **Build:** `npm run tauri dev` / `npm run tauri build`

## Architecture

```
During recording:
  Core Audio Process Tap  ──→  ring buffer  ──→  drain thread (50ms)  ──→  system_buffer Vec<f32>
  CPAL mic callback       ──→  try_lock  ──→  mic_buffer Vec<f32>

After stop_recording:
  mic_buffer    → VAD → chunk → Whisper (Metal) → "Me" segments
  system_buffer → VAD → chunk → Whisper (Metal) → "Remote" segments
  Merge by timestamp → write .txt to ~/Desktop
```

1. User clicks Start → `start_recording` Tauri command
2. System audio: Core Audio Process Tap via cidre, drains every 50ms into `system_buffer`
3. Mic: CPAL stream callback writes into `mic_buffer` (try_lock, non-blocking)
4. User clicks Stop → `stop_recording` command, sets `recording_active = false`
5. Whisper medium model loaded, VAD + chunking + transcription runs in `spawn_blocking`
6. Transcript written to `~/Desktop/Hyv_Transcript_<timestamp>.txt`

## Project Structure

```
hyv/
├── index.html
├── package.json                  # npm scripts: dev, build, tauri
├── vite.config.ts
├── tsconfig.json
├── src/                          # React frontend
│   ├── main.tsx                  # React entry, StrictMode
│   ├── App.tsx                   # Root component, uses useAppState
│   ├── components/
│   │   ├── RecordingControls.tsx # Start/Stop button, progress bar
│   │   ├── StatusIndicator.tsx   # Status dot + message
│   │   └── TranscriptList.tsx    # Recent transcripts list
│   ├── hooks/
│   │   └── useAppState.ts        # Status state, Tauri event listener + 1s polling fallback
│   └── lib/
│       └── commands.ts           # Tauri invoke wrappers + AppStatus type
└── src-tauri/                    # Rust backend
    ├── tauri.conf.json           # Window, tray, bundle settings
    ├── Cargo.toml
    └── src/
        ├── main.rs               # Binary entry
        ├── lib.rs                # Tauri builder, tray toggle, Accessory activation policy
        ├── commands.rs           # start_recording, stop_recording, get_status, get_recent_transcripts
        ├── state.rs              # AppState: status Mutex, audio buffers, recording_active AtomicBool
        ├── audio/
        │   ├── capture.rs        # MicCapture (CPAL) + SystemCapture (Core Audio tap via cidre)
        │   └── vad.rs            # Energy-based VAD → Vec<SpeechSegment>
        ├── transcription/
        │   ├── engine.rs         # WhisperEngine: load ggml model, transcribe_channel
        │   ├── model_manager.rs  # Download ggml-medium.bin to ~/Library/Application Support/Hyv/models/
        │   └── chunker.rs        # SpeechSegment list → audio chunks for Whisper
        └── output/
            └── transcript_writer.rs  # Write speaker-labeled .txt to Desktop
```

## State Machine

```
Idle ──(click Start, model missing)──→ ModelDownloading ──→ Idle  (click Start again)
Idle ──(click Start, model ready)───→ Recording ──→ Processing ──→ Idle
                                                               └──→ Error
```

`AppStatus` is in `state.rs`, serialized with `#[serde(tag = "type", content = "data")]`.

Frontend receives updates via:
1. Tauri `status-changed` events from Rust (emitted after mutex is released)
2. 1-second polling fallback in `useAppState.ts` (guards against missed events)

## Build & Run

```bash
npm install
npm run tauri dev       # dev mode
npm run tauri build     # production
```

First run downloads ggml-medium.bin (~1.5GB) to `~/Library/Application Support/Hyv/models/`. Status shows download progress. Click Start again after returning to "Ready to record."

## Logging

```bash
RUST_LOG=debug npm run tauri dev
```

Frontend devtools: Cmd+Option+I in dev mode.

## Design Principles
- **Local-first** — Whisper runs on-device via Metal, no API keys required
- **Accuracy over speed** — batch post-processing after recording stops
- **Channel separation** — mic and system audio captured independently, never mixed
- **Manual control** — user clicks Start/Stop explicitly
