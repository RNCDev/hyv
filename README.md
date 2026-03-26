# Hyv

A macOS menu bar app that auto-detects multi-party meetings, records system audio, and produces speaker-labeled transcription documents on your Desktop.

## How it works

1. **Detects meetings** — polls for running meeting apps (Zoom, Teams, FaceTime, WhatsApp, Webex, Slack)
2. **Records system audio** — captures via ScreenCaptureKit to a WAV file
3. **Diarizes speakers** — runs pyannote.audio to identify who spoke when
4. **Transcribes segments** — sends each speaker segment to the Cohere Transcribe API
5. **Writes transcript** — outputs a speaker-labeled `.txt` file to your Desktop

Processing happens after the meeting ends. Accuracy over speed — a 30-minute meeting takes roughly 30 minutes to process.

## Requirements

- macOS 14.0+ (Apple Silicon)
- Xcode 16+
- Python 3.10+
- [XcodeGen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`)
- [FFmpeg](https://ffmpeg.org/) (`brew install ffmpeg`)
- [Cohere API key](https://dashboard.cohere.com/api-keys)
- [HuggingFace token](https://huggingface.co/settings/tokens) with access to pyannote gated models

## Setup

```bash
git clone https://github.com/your-username/hyv.git
cd hyv

# Run setup script
./scripts/setup.sh

# Add your API keys to .env
COHERE_TRIAL_API_KEY=your-key
HF_TOKEN=your-token
```

### HuggingFace model access

Accept the license for each gated model (use the same account as your `HF_TOKEN`):
- [pyannote/segmentation-3.0](https://hf.co/pyannote/segmentation-3.0)
- [pyannote/speaker-diarization-3.1](https://hf.co/pyannote/speaker-diarization-3.1)

## Build & Run

### From the command line
```bash
xcodebuild -project Hyv.xcodeproj -scheme Hyv -configuration Debug build SYMROOT=build
open build/Debug/Hyv.app
```

### From Xcode
```bash
open Hyv.xcodeproj
# Press Cmd+R to build and run
```

On first launch, grant **Screen & System Audio Recording** permission in System Settings > Privacy & Security.

## Usage

1. The app runs as a menu bar icon (waveform)
2. Start a meeting in any supported app — Hyv detects it automatically
3. Click the icon and press **Start Recording**
4. When the meeting ends, click **Stop Recording**
5. Wait for processing — a transcript file appears on your Desktop

### Output format
```
=== Hyv Transcript ===
Date: March 26, 2026 at 6:04 PM
Meeting: WhatsApp
Duration: 30:00
Speakers: 2
========================

[00:03] SPEAKER_01: This is like all the rage, man.
[00:08] SPEAKER_00: Anyways, go ahead.
...

=== End of Transcript ===
```

## Supported meeting apps

| App | Bundle ID |
|-----|-----------|
| Zoom | `us.zoom.xos` |
| Microsoft Teams | `com.microsoft.teams2` |
| Teams (Classic) | `com.microsoft.teams` |
| FaceTime | `com.apple.FaceTime` |
| WhatsApp | `net.whatsapp.WhatsApp` |
| Webex | `com.webex.meetingmanager` |
| Slack | `com.tinyspeck.slackmacgap` |

## Architecture

```
During meeting:  AudioCaptureService → AudioFileRecorder (WAV to disk)
After meeting:   Python (pyannote diarization → Cohere API) → TranscriptFileWriter
```

See [CLAUDE.md](CLAUDE.md) for detailed project structure and [xcode-build-deploy.md](xcode-build-deploy.md) for the full build & deploy guide.

## License

MIT
