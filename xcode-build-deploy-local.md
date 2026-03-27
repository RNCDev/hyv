# Hyv — Local Inference Build Guide (`feat-local-refactor`)

This guide covers running Hyv with **local on-device transcription** using the Cohere Transcribe model via PyTorch + Apple Metal (MPS). No API calls needed for transcription — audio never leaves your machine.

## What's Different from `main`

| | `main` (API mode) | `feat-local-refactor` |
|---|---|---|
| Transcription | Cohere REST API | Local PyTorch + MPS |
| Requires internet | Yes (per segment) | Only for first model download |
| API key needed | `COHERE_TRIAL_API_KEY` | No (still needs `HF_TOKEN`) |
| Model size | N/A | ~4-8GB download, ~4GB RAM |
| Speed | ~5s/segment (network bound) | ~1-3s/segment (GPU bound) |
| Privacy | Audio sent to Cohere | Audio stays on device |

---

## Prerequisites

### 1. Switch to the feature branch
```bash
cd /Users/ritujoychowdhury/Documents/Github/hyv
git checkout feat-local-refactor
```

### 2. Install Python dependencies
```bash
source venv/bin/activate
pip install -r scripts/requirements.txt
```

This adds `transformers>=4.56` and `accelerate` on top of the existing deps.

### 3. HuggingFace Token
You still need `HF_TOKEN` in `.env` for:
- Pyannote diarization models (gated)
- Downloading the Cohere Transcribe model weights

Accept model licenses if you haven't already:
- [pyannote/segmentation-3.0](https://hf.co/pyannote/segmentation-3.0)
- [pyannote/speaker-diarization-3.1](https://hf.co/pyannote/speaker-diarization-3.1)

### 4. FFmpeg
```bash
brew install ffmpeg
```

### 5. Xcode tools (same as main)
```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

---

## First Run — Model Download

The first time you run transcription, models are downloaded from HuggingFace. This is a one-time cost.

### Option A: Download to bundled `models/` directory (recommended for offline use)
```bash
source venv/bin/activate
python3 scripts/download_models.py --hf-token $(grep HF_TOKEN .env | cut -d= -f2)
```

This saves all models (~4GB) to `models/` in the repo root. The app auto-detects this directory and loads from it — no internet needed after this step.

### Option B: Let HuggingFace cache handle it
```bash
source venv/bin/activate
python3 -c "
from transformers import AutoProcessor, AutoModelForSpeechSeq2Seq
AutoProcessor.from_pretrained('CohereLabs/cohere-transcribe-03-2026', trust_remote_code=True)
AutoModelForSpeechSeq2Seq.from_pretrained('CohereLabs/cohere-transcribe-03-2026', trust_remote_code=True)
print('Model downloaded successfully')
"
```

This caches to `~/.cache/huggingface/`. Works fine but isn't bundleable.

---

## Build & Run

### From command line
```bash
xcodegen generate
xcodebuild -project Hyv.xcodeproj -scheme Hyv -configuration Debug build SYMROOT=build
build/Debug/Hyv.app/Contents/MacOS/Hyv
```

### From Xcode
```bash
xcodegen generate
open Hyv.xcodeproj
# Press Cmd+R
```

---

## Testing

### Test the Python script directly (fastest way to validate)
```bash
source venv/bin/activate

# Local mode (no API key needed)
python3 scripts/diarize_and_transcribe.py \
  --audio ~/Library/Application\ Support/Hyv/recordings/recording_2026-03-26T22:03:16Z.wav \
  --hf-token $(grep HF_TOKEN .env | cut -d= -f2) \
  --local

# API mode (fallback, same as main branch)
python3 scripts/diarize_and_transcribe.py \
  --audio ~/Library/Application\ Support/Hyv/recordings/recording_2026-03-26T22:03:16Z.wav \
  --hf-token $(grep HF_TOKEN .env | cut -d= -f2) \
  --cohere-key $(grep COHERE_TRIAL_API_KEY .env | cut -d= -f2)
```

### Test with the app
1. Build and launch Hyv
2. Start a call (or just play audio)
3. Click **Start Recording**, wait, then **Stop Recording**
4. Watch the menu bar for progress — you should see "Loading local transcription model..." on first run
5. Check Desktop for `Hyv_Transcript_*.txt`

---

## Troubleshooting

### "No module named 'transformers'"
```bash
source venv/bin/activate
pip install -r scripts/requirements.txt
```

### Model download fails or hangs
- Check your internet connection
- Ensure `HF_TOKEN` is valid: `huggingface-cli whoami`
- Try downloading manually: `huggingface-cli download CohereLabs/cohere-transcribe-03-2026`

### Out of memory
The model needs ~4GB RAM. If you're running other heavy apps:
- Close memory-intensive apps
- The model uses MPS (Metal GPU) by default — check Activity Monitor for GPU memory pressure

### Slow inference
- First segment is slowest (model warmup + MPS compilation)
- Subsequent segments should be faster
- Expected: ~1-3 seconds per segment on M1/M2/M3

### Want to fall back to API mode
Set `COHERE_TRIAL_API_KEY` in `.env` and remove `"--local"` from the arguments array in `Hyv/Services/DiarizationService.swift`. Rebuild. Note: the Python subprocess has a 45-minute timeout — if inference stalls, the app will kill the process and report an error.

---

## Architecture Notes

```
Recording:    AudioCaptureService (thread-safe, NSLock) → AudioFileRecorder (actor, WAV to disk)
Processing:   Python script (45-minute timeout):
              1. pyannote diarization (speaker detection)
              2. Load CohereLabs/cohere-transcribe-03-2026 locally
              3. Transcribe each segment on-device via MPS
              4. Output JSON → TranscriptFileWriter
Detection:    NSWorkspace notifications + 30s safety poll (with 5s debounce)
```

The local model is loaded once and reused for all segments. Transcription is sequential (GPU inference isn't safely concurrent), but each segment is fast on Apple Silicon.

Meeting detection uses `NSWorkspace.didActivateApplicationNotification` and `NSWorkspace.didTerminateApplicationNotification` instead of frequent polling. A 5-second debounce prevents rapid state transitions when switching windows. Audio speech detection streams the WAV file in 64KB chunks to avoid loading large recordings into memory.
