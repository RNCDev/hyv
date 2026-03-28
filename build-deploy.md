# Hyv v0.2.0 — Build & Deploy Guide

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.94+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | `brew install node` |
| CMake | 3.20+ | `brew install cmake` |
| Xcode CLI tools | — | `xcode-select --install` |

Verify:
```bash
rustc --version    # 1.94+
node --version     # v20+
cmake --version    # 3.20+
```

## Quick Start

```bash
cd hyv-v2

# Install frontend dependencies
npm install

# Dev mode (hot-reload frontend, debug Rust backend)
npm run tauri dev

# Production build
npm run tauri build --bundles app
```

The built app is at:
```
src-tauri/target/release/bundle/macos/Hyv.app
```

## Whisper Model

The app needs a Whisper model to transcribe. On first launch it will attempt to download automatically, but you can pre-download:

```bash
# Medium model (~1.5GB) — default, good balance of speed/accuracy
mkdir -p ~/Library/Application\ Support/Hyv/models
curl -L -o ~/Library/Application\ Support/Hyv/models/ggml-medium.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin

# Or small model (~500MB) — faster, slightly less accurate
curl -L -o ~/Library/Application\ Support/Hyv/models/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

Verify:
```bash
ls -lh ~/Library/Application\ Support/Hyv/models/
# ggml-medium.bin should be ~1.5GB
```

## macOS Permissions

On first launch, macOS will prompt for:

1. **Microphone** — required for capturing your voice
2. **Screen Recording** — required for system audio capture (Core Audio Process Tap)

If you accidentally deny, re-enable in:
- System Settings → Privacy & Security → Microphone → Hyv
- System Settings → Privacy & Security → Screen Recording → Hyv

## Running the App

### Dev mode
```bash
npm run tauri dev
```
- Frontend hot-reloads on changes to `src/`
- Rust backend recompiles on changes to `src-tauri/src/`
- Logs visible in the terminal (info level by default)
- For debug logs: `RUST_LOG=debug npm run tauri dev`

### From the built .app
```bash
# Open directly
open src-tauri/target/release/bundle/macos/Hyv.app

# Or copy to Applications
cp -r src-tauri/target/release/bundle/macos/Hyv.app /Applications/
open /Applications/Hyv.app
```

The app appears as a menu bar icon (no Dock icon).

## Testing Workflow

1. Launch the app — look for the menu bar icon (top right)
2. Click the icon to open the popover
3. If model isn't downloaded, it will download first (~1.5GB)
4. Click **Start Recording**
5. Play some audio or join a meeting
6. Click **Stop Recording**
7. Wait for processing (progress shown in UI)
8. Transcript appears on your Desktop: `~/Desktop/Hyv_Transcript_*.txt`

## Transcript Output

Transcripts are saved to Desktop as plain text:
```
=== Hyv Transcript ===
Date: March 27, 2026 at 5:00 PM
Duration: 1:30
Speakers: 2
========================

[00:03] Remote: Hello everyone, welcome to the meeting.
[00:08] Me: Hi, thanks for having me.
[00:15] Remote: Let's get started with the agenda.

=== End of Transcript ===
```

- **Me** = your microphone audio
- **Remote** = system audio (meeting participants)

## Architecture Overview

```
┌──────────────────────────────────────┐
│  React UI (menu bar popover)         │
│  Status | Controls | Transcripts     │
└──────────────┬───────────────────────┘
               │ Tauri IPC
┌──────────────▼───────────────────────┐
│  Rust Backend                        │
│                                      │
│  Audio Capture:                      │
│   ├─ CPAL (microphone → mic_buffer)  │
│   └─ Core Audio Process Tap          │
│      (system audio → system_buffer)  │
│                                      │
│  Processing (after recording stops): │
│   ├─ Energy VAD (skip silence)       │
│   ├─ 30s chunking                    │
│   ├─ whisper-rs + Metal GPU          │
│   │  ├─ mic chunks → "Me"           │
│   │  └─ system chunks → "Remote"    │
│   └─ Merge by timestamp → .txt      │
└──────────────────────────────────────┘
```

## Troubleshooting

### No menu bar icon
- Check that `LSUIElement` is set in Info.plist (hides Dock icon)
- The app may be running but the tray icon didn't register — restart

### Microphone not working
- Check System Settings → Privacy & Security → Microphone
- Try: `tccutil reset Microphone com.hyv.app` then relaunch

### System audio not capturing
- Check System Settings → Privacy & Security → Screen Recording
- Core Audio Process Tap requires this permission on macOS 14+
- Try: `tccutil reset ScreenCapture com.hyv.app` then relaunch

### Whisper model not found
- Verify file exists: `ls ~/Library/Application\ Support/Hyv/models/ggml-medium.bin`
- File should be ~1.5GB. If smaller, the download was interrupted — re-download

### Build fails with "cmake not found"
```bash
brew install cmake
```

### Build fails with Metal errors
- Ensure Xcode CLI tools are installed: `xcode-select --install`
- Metal requires macOS 14.0+ and Apple Silicon (M1/M2/M3/M4)

## Project Structure

```
hyv-v2/
├── package.json                 # Frontend deps
├── vite.config.ts               # Vite build config
├── index.html                   # HTML entry
├── src/                         # React frontend
│   ├── App.tsx                  # Root component
│   ├── main.tsx                 # Entry point
│   ├── components/
│   │   ├── StatusIndicator.tsx  # State display
│   │   ├── RecordingControls.tsx # Start/Stop
│   │   └── TranscriptList.tsx   # Recent transcripts
│   ├── hooks/useAppState.ts     # Backend state listener
│   └── lib/commands.ts          # Tauri IPC wrappers
└── src-tauri/                   # Rust backend
    ├── Cargo.toml               # Rust deps
    ├── tauri.conf.json          # Tauri config
    ├── Info.plist               # macOS permissions
    └── src/
        ├── main.rs              # Entry
        ├── lib.rs               # Tauri setup
        ├── state.rs             # AppState machine
        ├── commands.rs          # IPC command handlers
        ├── audio/
        │   ├── capture.rs       # CPAL mic + Core Audio tap
        │   └── vad.rs           # Energy-based VAD
        ├── transcription/
        │   ├── engine.rs        # whisper-rs wrapper
        │   ├── chunker.rs       # 30s audio splitter
        │   └── model_manager.rs # Model download/cache
        └── output/
            └── transcript_writer.rs  # Desktop .txt output
```

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `tauri` 2.x | Desktop framework, tray icon, IPC |
| `whisper-rs` 0.14 (metal) | Local Whisper transcription with GPU |
| `cpal` 0.15 | Microphone capture |
| `cidre` (git) | Apple Core Audio bindings (system audio tap) |
| `ringbuf` 0.4 | Lock-free ring buffer for audio callback |
| `rubato` 0.16 | Sample rate conversion |
