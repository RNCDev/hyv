# Hyv

A macOS menu bar app that records both sides of a meeting (system audio + microphone) and produces speaker-labeled transcripts on your Desktop using on-device Whisper.

## How it works

1. **Records dual audio** — captures system audio (remote participants) via Core Audio Process Tap and your microphone via CPAL
2. **Runs VAD** — energy-based voice activity detection on both channels to find speech segments
3. **Transcribes locally** — runs Whisper (medium model, on-device via Metal) on each channel
4. **Labels speakers** — mic channel → "Me", system channel → "Remote"
5. **Writes transcript** — merges segments by timestamp into a `.txt` file on your Desktop

Processing happens after recording stops. First run downloads the Whisper medium model (~1.5GB) automatically.

## Requirements

- macOS 14.0+ (Apple Silicon)
- Rust (via `rustup`)
- Node.js 18+
- Tauri CLI (`cargo install tauri-cli`)

## Setup

```bash
git clone https://github.com/your-username/hyv.git
cd hyv
npm install
```

## Build & Run

```bash
# Development
npm run tauri dev

# Production build
npm run tauri build
```

On first launch, grant **Screen & System Audio Recording** and **Microphone** permissions in System Settings > Privacy & Security.

On first recording, the Whisper medium model (~1.5GB) downloads automatically. The status bar will show download progress. Click Start Recording again once it returns to "Ready to record."

## Usage

1. App runs as a menu bar icon — click it to open the panel
2. Click **Start Recording** when your meeting begins
3. Click **Stop Recording** when done
4. Processing runs locally — transcript appears on your Desktop when complete

### Output format

```
=== Hyv Transcript ===
Date: March 27, 2026 at 3:27 PM
Duration: 30:00
Speakers: 2
========================

[00:03] Remote: Hello, can you hear me?
[00:08] Me: Yeah, loud and clear.
...

=== End of Transcript ===
```

## Architecture

```
During recording:
  Core Audio Process Tap (system audio) ──→ ring buffer → shared Vec<f32>
  CPAL (microphone)                     ──→ CPAL callback → shared Vec<f32>

After recording:
  mic buffer    → VAD → Whisper (Metal) → "Me" segments
  system buffer → VAD → Whisper (Metal) → "Remote" segments
  → merge by timestamp → transcript .txt on Desktop
```

**Stack:** Tauri 2 + React/TypeScript frontend, Rust backend, whisper-rs (Metal), cidre (Core Audio), CPAL

## Debugging

```bash
RUST_LOG=debug npm run tauri dev
```

See [CLAUDE.md](CLAUDE.md) for full project structure and architecture details.

## License

MIT
