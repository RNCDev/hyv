# Hyv — macOS Conference Transcription App

## What is this?
A macOS menu bar app that records system audio + microphone as separate channels, then produces speaker-labeled transcription documents on the Desktop.

## Tech Stack
- **Language:** Swift 6.2 / SwiftUI + Python 3.10+
- **Target:** macOS 14.0+, Apple Silicon
- **Audio capture:** ScreenCaptureKit (system audio) + AVCaptureSession (microphone), 16kHz stereo
- **Speaker diarization:** pyannote.audio 3.1 on system channel only (remote speakers)
- **Transcription:** Cohere Transcribe API (per-segment, per-channel)
- **Build system:** XcodeGen (`project.yml` → `xcodegen generate` → `Hyv.xcodeproj`)
- **No App Sandbox** — ScreenCaptureKit requires screen recording permission

## Architecture
```
During recording:
  ScreenCaptureKit (system audio) ──→ AudioFileRecorder ──→ stereo WAV
  AVCaptureSession (microphone)   ──→   (ch0=remote, ch1=mic)

After recording:
  Python script splits stereo WAV:
    ch1 (mic)    → energy VAD → transcribe segments → label "Me"
    ch0 (system) → pyannote diarization → transcribe segments → label "Remote" / "Remote (SPEAKER_XX)"
  Merge all segments by timestamp → TranscriptFileWriter → Desktop .txt
```

1. Record stereo WAV to `~/Library/Application Support/Hyv/recordings/`
2. Channel 0 = system audio (remote participants), Channel 1 = microphone (you)
3. On stop, spawn `scripts/diarize_and_transcribe.py` which:
   - Transcribes mic channel directly (energy-based VAD, no diarization needed)
   - Runs pyannote on system channel to separate multiple remote speakers
   - Merges all segments by timestamp
4. Write speaker-labeled transcript to Desktop `.txt` file

## Project Structure
```
hyv/
├── .env                              # COHERE_TRIAL_API_KEY, HF_TOKEN (gitignored)
├── project.yml                       # XcodeGen config
├── scripts/
│   ├── diarize_and_transcribe.py     # Stereo split + diarize + transcribe pipeline
│   ├── requirements.txt              # Python dependencies
│   └── setup.sh                      # One-command setup script
└── Hyv/
    ├── HyvApp.swift                  # @main, MenuBarExtra entry point
    ├── Hyv.entitlements              # No sandbox for MVP
    ├── Config/AppConfig.swift        # Loads API keys, detects Python path
    ├── Models/
    │   ├── AppState.swift            # Central orchestrator, owns all services
    │   ├── MeetingApp.swift          # Enum of meeting app bundle IDs (unused, retained)
    │   ├── TranscriptSegment.swift   # Legacy timestamped segment
    │   └── TranscriptionResult.swift # Codable result from Python script
    ├── Services/
    │   ├── AudioCaptureService.swift       # SCStream + AVCaptureSession → AudioFileRecorder
    │   ├── AudioFileRecorder.swift         # Writes stereo interleaved PCM to WAV
    │   ├── DiarizationService.swift        # Swift ↔ Python subprocess bridge
    │   ├── CohereTranscriptionService.swift # REST client (retained for future use)
    │   ├── MeetingDetectorService.swift    # Unused, retained for future use
    │   └── TranscriptFileWriter.swift      # Speaker-labeled FileHandle append
    ├── Views/MenuBarView.swift       # Status, controls, transcript list
    └── Utilities/
        ├── WAVEncoder.swift          # 44-byte RIFF header + PCM
        └── ProcessUtils.swift        # NSWorkspace running app helpers
```

## Build & Run
```bash
# Quick setup (installs deps, generates project)
./scripts/setup.sh

# Or manually:
pip install -r scripts/requirements.txt
brew install xcodegen && xcodegen generate

# Full build from CLI
xcodebuild -project Hyv.xcodeproj -scheme Hyv -configuration Debug build SYMROOT=build
open build/Debug/Hyv.app
```

## Environment
- `COHERE_TRIAL_API_KEY` — Cohere API key (in `.env`)
- `HF_TOKEN` — HuggingFace token for pyannote gated model (in `.env`)
- Screen Recording permission required at first launch
- Microphone permission required at first launch
- Python 3.10+ with pyannote.audio, torch, soundfile, numpy, requests

## Logging
All services use `os.Logger` with subsystem `com.hyv.app` and per-service categories:

| Category | Service |
|---|---|
| `config` | AppConfig — key loading, Python path detection |
| `audio-capture` | AudioCaptureService — system audio + mic capture start/stop |
| `audio-recorder` | AudioFileRecorder — stereo WAV creation, size/duration on stop |
| `diarization` | DiarizationService — subprocess launch, timing, exit code |
| `transcription` | CohereTranscriptionService — HTTP status, latency, retries |
| `transcript-writer` | TranscriptFileWriter — file open/close |
| `app-state` | AppState — state transitions, pipeline stages, errors |

Stream logs live:
```bash
/usr/bin/log show --predicate 'subsystem == "com.hyv.app"' --last 1h --info
```

Or filter in Console.app by subsystem `com.hyv.app`.

## Design Principles
- **Accuracy over speed** — batch post-processing, not real-time streaming
- **Channel separation** — mic and system audio recorded as separate stereo channels, never mixed
- Mic channel transcribed directly (energy VAD), system channel diarized with pyannote
- Pyannote only processes the system channel (remote speakers) — cleaner signal, faster
- User clicks Start/Stop manually — no automatic meeting detection
- Adjacent same-speaker segments are merged for cleaner output
- WAV recordings are cleaned up after successful transcription
