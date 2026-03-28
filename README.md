# Hyv

A macOS menu bar app that records both sides of a conversation — system audio (remote speakers) and your microphone — then produces a speaker-labeled transcript on your Desktop using on-device Whisper. No cloud, no API keys.

## How it works

1. Click **Start Recording** — captures mic + system audio simultaneously as separate streams
2. Click **Stop Recording** — triggers processing
3. Energy-based VAD (with 200ms hangover) finds speech in each channel
4. Whisper (medium model, Metal GPU) transcribes each channel
5. Duplicate segments caused by mic bleed-through are removed
6. Segments merged by timestamp → `.txt` file on your Desktop

Mic channel → labeled **"Speaker 1"**. System audio channel → labeled **"Speaker 2"**. Consecutive same-speaker segments within 2 seconds are merged into paragraphs.

---

## Requirements

| Dependency | Version | Install |
|---|---|---|
| macOS | 14.0+ (Apple Silicon) | — |
| Xcode Command Line Tools | latest | `xcode-select --install` |
| Rust | 1.85+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 18+ | `brew install node` |
| CMake | 3.20+ | `brew install cmake` |

No Python, no API keys, no HuggingFace token required.

---

## Setup

```bash
git clone https://github.com/ritujoychowdhury/hyv.git
cd hyv
npm install
```

### Whisper model (first-time only)

The app downloads the Whisper medium model (~1.5 GB) automatically on first recording. You can also pre-download to avoid waiting:

```bash
mkdir -p ~/Library/Application\ Support/Hyv/models

# Medium model — recommended (~1.5 GB, ~2–3 min to process 30s of audio)
curl -L -o ~/Library/Application\ Support/Hyv/models/ggml-medium.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin

# Small model — faster (~490 MB, ~1 min to process 30s of audio)
# curl -L -o ~/Library/Application\ Support/Hyv/models/ggml-small.bin \
#   https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

---

## Run

```bash
# Development (hot reload)
npm run tauri dev

# Production build
npm run tauri build
# App: src-tauri/target/release/bundle/macos/Hyv.app
```

---

## macOS Permissions

On first launch the system will prompt for two permissions — both are required:

| Permission | Why |
|---|---|
| **Microphone** | Captures your voice via CPAL |
| **Screen Recording** | Required by Core Audio Process Tap to capture system audio |

Grant both in **System Settings → Privacy & Security**. If you accidentally deny:

```bash
tccutil reset Microphone com.hyv.app
tccutil reset ScreenCapture com.hyv.app
```

Then relaunch the app.

---

## Usage

1. Click the menu bar icon to open the panel
2. Click **Start Recording**
   - First run: model downloads (~1.5 GB). Status shows progress. Click Start again when it returns to "Ready to record."
3. Record your conversation
4. Click **Stop Recording**
5. Processing runs on-device (progress shown). Transcript appears on Desktop when done.
6. Recent transcripts listed in the panel — click to open, ✕ to delete.

### Output format

```
=== Hyv Transcript ===
Date: March 28, 2026 at 9:00 AM
Duration: 5:12
Speakers: 2
========================

[00:03] Speaker 2: Hey, can you hear me okay?
[00:07] Speaker 1: Yeah, loud and clear. Great, let's get started.
...

=== End of Transcript ===
```

Transcripts saved to `~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt`.

---

## Deployment

### For personal use

```bash
npm run tauri build
cp -r src-tauri/target/release/bundle/macos/Hyv.app /Applications/
open /Applications/Hyv.app
```

### For sharing with others (unsigned)

Build produces a `.dmg` and `.app` in `src-tauri/target/release/bundle/macos/`. Recipients will need to right-click → Open on first launch to bypass Gatekeeper (unsigned build).

### Key settings

| Setting | Value | Location |
|---|---|---|
| Bundle ID | `com.hyv.app` | `src-tauri/tauri.conf.json` |
| Min macOS | 14.0 | `src-tauri/tauri.conf.json` |
| Model storage | `~/Library/Application Support/Hyv/models/` | `src-tauri/src/transcription/model_manager.rs` |
| Output path | `~/Desktop/Hyv_Transcript_*.txt` | `src-tauri/src/output/transcript_writer.rs` |
| Audio sample rate | 16 kHz mono | `src-tauri/src/audio/capture.rs` |
| VAD energy threshold | 0.002 RMS | `src-tauri/src/commands.rs` |
| VAD hangover | 200ms | `src-tauri/src/audio/vad.rs` |
| Dedup time window | 3.0s | `src-tauri/src/commands.rs` |
| Dedup similarity threshold | 0.65 (word overlap) | `src-tauri/src/commands.rs` |
| Debug artifact storage | `~/Library/Application Support/Hyv/debug/` | `src-tauri/src/debug.rs` |
| Whisper no_speech_thold | 0.6 | `src-tauri/src/transcription/engine.rs` |
| Whisper entropy_thold | 2.4 | `src-tauri/src/transcription/engine.rs` |
| Whisper logprob_thold | -1.0 | `src-tauri/src/transcription/engine.rs` |
| Whisper language | English (hardcoded) | `src-tauri/src/transcription/engine.rs` |

---

## Debugging

After each recording, debug artifacts are saved automatically:

| Artifact | Location | Purpose |
|---|---|---|
| **Log file** | `~/Library/Logs/Hyv/hyv.log` | Persistent structured logs (daily rolling, info level by default) |
| **Raw mic audio** | `~/Library/Application Support/Hyv/debug/mic_*.wav` | Listen to what the mic captured — confirm bleed-through |
| **Raw system audio** | `~/Library/Application Support/Hyv/debug/system_*.wav` | Listen to the system audio tap |
| **Pre-dedup segments** | `~/Library/Application Support/Hyv/debug/segments_raw_*.json` | Full Whisper output before duplicate removal, with timestamps and speakers |

Debug files older than 7 days are pruned automatically on startup.

```bash
# Enable verbose Rust logs (also written to log file)
RUST_LOG=debug npm run tauri dev

# Tail the log file live
tail -f ~/Library/Logs/Hyv/hyv.log

# Frontend DevTools
# Press Cmd+Option+I inside the app window (dev mode only)
```

---

## Architecture

```
During recording:
  Core Audio Process Tap (cidre) ──→ ring buffer ──→ drain thread (50ms) ──→ system_buffer Vec<f32>
  CPAL mic callback              ──→ try_lock    ──→ mic_buffer Vec<f32>

After stop:
  mic_buffer    → normalize (-16 LUFS) → AEC (echo cancel, delay-aware) → VAD → chunk → Whisper → "Speaker 1"
  system_buffer → normalize (-16 LUFS) → (AEC reference) → VAD → chunk → Whisper → "Speaker 2"
  Align timestamps → dedup bleed (word overlap >65%, 3s window) → merge (≤2s gap) → ~/Desktop/Hyv_Transcript_*.txt
```

**Stack:** Tauri 2, React 19 / TypeScript / Vite, Rust, whisper-rs 0.14 (Metal), cidre (Core Audio), CPAL, tokio

See [CLAUDE.md](CLAUDE.md) for full internal architecture reference.

---

## Troubleshooting

| Problem | Fix |
|---|---|
| No menu bar icon | App may be running but tray didn't register — restart |
| Microphone not working | System Settings → Privacy & Security → Microphone → enable Hyv. Or: `tccutil reset Microphone com.hyv.app` |
| System audio not capturing | System Settings → Privacy & Security → Screen Recording → enable Hyv. Or: `tccutil reset ScreenCapture com.hyv.app` |
| Transcript empty / `duration=0.0s` | Recording stopped before mic thread started — try recording for at least 3–5 seconds |
| Model download stuck | Check connection. Verify: `ls -lh ~/Library/Application\ Support/Hyv/models/` — should be ~1.5 GB |
| Build fails: `cmake not found` | `brew install cmake` |
| Build fails: Metal errors | `xcode-select --install` — Metal requires Xcode CLI tools |
| Old path errors after moving repo | `rm -rf src-tauri/target && npm run tauri dev` |

---

## Key Dependencies

| Crate | Purpose |
|---|---|
| `tauri` 2 | Desktop framework, tray icon, IPC |
| `whisper-rs` 0.14 (metal) | On-device Whisper transcription via Metal GPU |
| `cpal` 0.15 | Microphone capture |
| `cidre` (git) | Apple Core Audio bindings — system audio Process Tap |
| `ringbuf` 0.4 | Lock-free ring buffer for audio callbacks |
| `tokio` 1 | Async runtime |
| `reqwest` 0.12 | Whisper model download |
| `ebur128` 0.1 | EBU R128 loudness measurement and normalization |
| `aec3` 0.1 | Pure-Rust WebRTC AEC3 echo cancellation |
| `hound` 3.5 | WAV file writing (debug audio saves) |
| `tracing-appender` 0.2 | Rolling log file output |
| `chrono` 0.4 | Date/time formatting |

---

## Known Limitations

- English only (Whisper language hardcoded)
- Processing is ~2–3× real-time on M-series (medium model) — a 10-minute recording takes ~20–30 minutes to process
- System audio capture requires Screen Recording permission (macOS 14 limitation)
- Multiple recordings within the same minute produce the same filename
- All remote speakers labeled "Speaker 2" — no per-speaker diarization yet

## License

MIT
