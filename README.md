# Hyv

A macOS menu bar app that records both sides of a meeting (system audio + microphone) as separate channels, then produces speaker-labeled transcription documents on your Desktop.

## How it works

1. **Records dual audio** — captures system audio (remote participants) via ScreenCaptureKit and your microphone via AVCaptureSession into a stereo WAV
2. **Splits channels** — separates your voice (mic, channel 1) from remote audio (system, channel 0)
3. **Transcribes your voice** — energy-based VAD on mic channel, segments sent to Cohere API → labeled "Me"
4. **Diarizes remote speakers** — runs pyannote.audio on system channel to identify who spoke when
5. **Transcribes remote segments** — each diarized segment sent to Cohere API → labeled "Remote" or "Remote (SPEAKER_XX)"
6. **Writes transcript** — merges all segments by timestamp into a `.txt` file on your Desktop

Processing happens after the recording stops. Accuracy over speed — a 30-minute meeting takes roughly 30 minutes to process.

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

On first launch, grant **Screen & System Audio Recording** and **Microphone** permissions in System Settings > Privacy & Security.

## Usage

1. The app runs as a menu bar icon (waveform)
2. Click the icon and press **Start Recording** when your meeting begins
3. When the meeting ends, click **Stop Recording**
4. Wait for processing — a transcript file appears on your Desktop

### Output format
```
=== Hyv Transcript ===
Date: March 27, 2026 at 3:27 PM
Duration: 30:00
Speakers: 3
========================

[00:03] Remote (SPEAKER_00): This is like all the rage, man.
[00:08] Me: Yeah, I agree.
[00:15] Remote (SPEAKER_01): Anyways, go ahead.
...

=== End of Transcript ===
```

## Architecture

```
During recording:
  ScreenCaptureKit (system audio) ──→ stereo WAV (ch0=remote, ch1=mic)
  AVCaptureSession (microphone)   ──→

After recording:
  ch1 (mic)    → VAD → Cohere API → "Me"
  ch0 (system) → pyannote → Cohere API → "Remote" / "Remote (SPEAKER_XX)"
  → merge by timestamp → transcript file
```

## Debugging

All services log to `os.Logger` (subsystem `com.hyv.app`). Stream logs live:

```bash
/usr/bin/log show --predicate 'subsystem == "com.hyv.app"' --last 1h --info
```

Or open Console.app and filter by subsystem `com.hyv.app`.

See [CLAUDE.md](CLAUDE.md) for detailed project structure and [xcode-build-deploy.md](xcode-build-deploy.md) for the full build & deploy guide.

## License

MIT
