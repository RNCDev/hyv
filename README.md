# Hyv

A macOS menu bar app that records both sides of a conversation — system audio (remote speakers) and your microphone — then produces a speaker-labeled transcript on your Desktop using on-device Whisper. No cloud, no API keys.

## How it works

1. Click **Start Recording** — captures mic + system audio simultaneously as separate streams
2. Click **Stop Recording** — triggers processing
3. Energy-based VAD finds speech in each channel
4. Whisper (medium model, Metal GPU) transcribes each channel independently
5. Mic bleed-through is detected and removed via word-overlap deduplication
6. Segments merged by timestamp → `.txt` file on your Desktop

Mic channel → **Speaker 1**. System audio → **Speaker 2**.

---

## Requirements

| Dependency | Version | Install |
|---|---|---|
| macOS | 14.0+ (Apple Silicon) | — |
| Xcode Command Line Tools | latest | `xcode-select --install` |
| Rust | 1.85+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 18+ | `brew install node` |
| CMake | 3.20+ | `brew install cmake` |

---

## Setup

```bash
git clone https://github.com/RNCDev/hyv.git
cd hyv
npm install
```

### Whisper model (first-time only)

The app downloads the Whisper medium model (~1.5 GB) automatically on first recording. To pre-download:

```bash
mkdir -p ~/Library/Application\ Support/Hyv/models
curl -L -o ~/Library/Application\ Support/Hyv/models/ggml-medium.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin
```

---

## Run

```bash
npm run tauri dev        # development (hot reload)
npm run tauri build      # production build → src-tauri/target/release/bundle/macos/Hyv.app
```

---

## macOS Permissions

On first launch the system prompts for two permissions — both required:

| Permission | Why |
|---|---|
| **Microphone** | Captures your voice |
| **Screen Recording** | Required by Core Audio Process Tap to capture system audio |

If accidentally denied: `tccutil reset Microphone com.hyv.app` / `tccutil reset ScreenCapture com.hyv.app`, then relaunch.

---

## Usage

1. Click the menu bar icon to open the panel
2. Click **Start Recording** — first run downloads the model (~1.5 GB), click Start again when ready
3. Record your conversation
4. Click **Stop Recording** — processing runs on-device, transcript appears on Desktop
5. Recent transcripts listed in the panel — click to open, ✕ to delete

### Output

```
=== Hyv Transcript ===
Date: March 28, 2026 at 9:00 AM
Duration: 5:12
Speakers: 2
========================

[00:03] Speaker 2: Hey, can you hear me okay?
[00:07] Speaker 1: Yeah, loud and clear.
...

=== End of Transcript ===
```

Saved to `~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt`.

---

## Debugging

After each recording, debug artifacts are saved automatically to `~/Library/Application Support/Hyv/debug/`:

| Artifact | Purpose |
|---|---|
| `mic_*.wav` / `system_*.wav` | Raw captured audio |
| `mic_normalized_*.wav` / `system_normalized_*.wav` | After loudness normalization |
| `mic_aec_*.wav` | After echo cancellation |
| `segments_raw_*.json` | Whisper output before dedup, with timestamps |

Files older than 7 days are pruned automatically.

```bash
RUST_LOG=debug npm run tauri dev    # verbose Rust logs
tail -f ~/Library/Logs/Hyv/hyv.log  # live log tail
```

---

## Troubleshooting

| Problem | Fix |
|---|---|
| No menu bar icon | Restart the app |
| Mic not working | System Settings → Privacy → Microphone → enable Hyv |
| System audio not capturing | System Settings → Privacy → Screen Recording → enable Hyv |
| Transcript empty | Recording too short — try at least 3–5 seconds |
| Model download stuck | `ls -lh ~/Library/Application\ Support/Hyv/models/` — should be ~1.5 GB |
| Build fails: `cmake not found` | `brew install cmake` |
| Build fails: Metal errors | `xcode-select --install` |
| Old path errors | `rm -rf src-tauri/target && npm run tauri dev` |

---

## Known Limitations

- English only (Whisper language hardcoded)
- Processing ~2–3× real-time on M-series — a 10-minute recording takes ~20–30 minutes
- Multiple recordings within the same minute share the same filename
- All remote speakers labeled "Speaker 2" — no per-speaker diarization

---

See [CLAUDE.md](CLAUDE.md) for internal architecture reference.

## License

MIT
