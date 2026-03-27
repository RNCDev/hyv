# Hyv — macOS Conference Transcription App

## What is this?
A macOS menu bar app (like Granola) that auto-detects multi-party meetings, records system audio, and produces speaker-labeled transcription documents on the Desktop.

## Tech Stack
- **Language:** Swift 6.2 / SwiftUI + Python 3.10+
- **Target:** macOS 14.0+, Apple Silicon
- **Audio capture:** ScreenCaptureKit (system audio, 16kHz mono)
- **Speaker diarization:** pyannote.audio 3.1 (via Python subprocess)
- **Transcription:** Cohere Transcribe API (per-segment, after diarization)
- **Build system:** XcodeGen (`project.yml` → `xcodegen generate` → `Hyv.xcodeproj`)
- **No App Sandbox** — ScreenCaptureKit requires screen recording permission

## Architecture
```
During meeting:  AudioCaptureService → AudioFileRecorder (full WAV to disk)
After meeting:   Python script (pyannote diarization → per-segment Cohere API) → TranscriptFileWriter
```

1. Record full meeting audio to `~/Library/Application Support/Hyv/recordings/`
2. On stop, spawn `scripts/diarize_and_transcribe.py` for speaker diarization + transcription
3. Write speaker-labeled segments incrementally to Desktop `.txt` file during processing

## Project Structure
```
hyv/
├── .env                              # COHERE_TRIAL_API_KEY, HF_TOKEN (gitignored)
├── project.yml                       # XcodeGen config
├── scripts/
│   ├── diarize_and_transcribe.py     # pyannote + Cohere API pipeline
│   ├── requirements.txt              # Python dependencies
│   └── setup.sh                      # One-command setup script
└── Hyv/
    ├── HyvApp.swift                  # @main, MenuBarExtra entry point
    ├── Hyv.entitlements              # No sandbox for MVP
    ├── Config/AppConfig.swift        # Loads API keys, detects Python path
    ├── Models/
    │   ├── AppState.swift            # Central orchestrator, owns all services
    │   ├── MeetingApp.swift          # Enum of meeting app bundle IDs
    │   ├── TranscriptSegment.swift   # Legacy timestamped segment
    │   └── TranscriptionResult.swift # Codable result from Python script
    ├── Services/
    │   ├── MeetingDetectorService.swift    # Polls NSWorkspace every 3s
    │   ├── AudioCaptureService.swift       # SCStream wrapper → AudioFileRecorder
    │   ├── AudioFileRecorder.swift         # Writes PCM to WAV file on disk
    │   ├── DiarizationService.swift        # Swift ↔ Python subprocess bridge
    │   ├── CohereTranscriptionService.swift # REST client (retained for future use)
    │   └── TranscriptFileWriter.swift      # Speaker-labeled FileHandle append
    ├── Views/MenuBarView.swift       # Status, controls, transcript preview
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

# Type-check (no Xcode needed)
swiftc -typecheck -target arm64-apple-macos14.0 -sdk $(xcrun --show-sdk-path) $(find Hyv -name "*.swift")

# Full build from CLI
xcodebuild -project Hyv.xcodeproj -scheme Hyv -configuration Debug build SYMROOT=build
open build/Debug/Hyv.app
```

## Environment
- `COHERE_TRIAL_API_KEY` — Cohere API key (in `.env`)
- `HF_TOKEN` — HuggingFace token for pyannote gated model (in `.env`)
- Screen Recording permission required at first launch
- Python 3.10+ with pyannote.audio, torch, soundfile, numpy, requests

## Logging
All services use `os.Logger` with subsystem `com.hyv.app` and per-service categories:

| Category | Service |
|---|---|
| `config` | AppConfig — key loading, Python path detection |
| `meeting-detection` | MeetingDetectorService — app detected/lost events |
| `audio-capture` | AudioCaptureService — capture start/stop, errors |
| `audio-recorder` | AudioFileRecorder — file creation, size/duration on stop |
| `diarization` | DiarizationService — subprocess launch, timing, exit code |
| `transcription` | CohereTranscriptionService — HTTP status, latency, retries |
| `transcript-writer` | TranscriptFileWriter — file open/close |
| `app-state` | AppState — state transitions, pipeline stages, errors |

Stream logs live:
```bash
log stream --predicate 'subsystem == "com.hyv.app"' --level debug
```

Or filter in Console.app by subsystem `com.hyv.app`.

## Design Principles
- **Accuracy over speed** — batch post-processing, not real-time streaming
- User will wait 30 min for a 30 min meeting for accurate speaker-labeled results
- Incremental file writes during *processing* (not during recording)
- Record → Diarize → Transcribe → Write (full pipeline after meeting ends)
- Adjacent same-speaker segments are merged for cleaner output
- Fallback to unlabeled transcription if diarization fails
- WAV recordings are cleaned up after successful transcription
