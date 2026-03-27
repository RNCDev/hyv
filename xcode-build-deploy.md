# Hyv — Build & Deploy Guide

## Prerequisites

### 1. Quick Setup (recommended)
```bash
cd /Users/ritujoychowdhury/Documents/Github/hyv
./scripts/setup.sh
```
This checks Python, installs dependencies, creates `.env` if needed, and generates the Xcode project.

### 1b. Manual Python Virtual Environment
```bash
cd /Users/ritujoychowdhury/Documents/Github/hyv
python3 -m venv venv
source venv/bin/activate
pip install -r scripts/requirements.txt
```

This installs pyannote.audio, torch, soundfile, numpy, and requests. First run of pyannote will also download ~1GB of model weights from HuggingFace.

### 2. FFmpeg
Required by pyannote/torchcodec for audio decoding:
```bash
brew install ffmpeg
```

### 3. HuggingFace Model Access
The pyannote diarization models are gated. You must accept the license for each:
1. Visit https://hf.co/pyannote/segmentation-3.0 — click **Agree**
2. Visit https://hf.co/pyannote/speaker-diarization-3.1 — click **Agree**
3. Visit https://hf.co/pyannote/speaker-diarization-community-1 — click **Agree**

Use the same HuggingFace account that owns your `HF_TOKEN`.

### 4. Environment Variables
Ensure your `.env` file at the repo root has both keys:
```
COHERE_TRIAL_API_KEY=your_key_here
HF_TOKEN=your_token_here
```

### 5. Point Xcode Tools to Xcode.app
Your system currently uses Command Line Tools. Switch to full Xcode:
```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

You only need to do this once.

---

## Option A: Build & Run from Command Line (no Xcode GUI)

### Generate the Xcode project
```bash
cd /Users/ritujoychowdhury/Documents/Github/hyv
xcodegen generate
```

### Build the app
```bash
xcodebuild -project Hyv.xcodeproj -scheme Hyv -configuration Debug build \
  SYMROOT=build
```

The built `.app` will be at: `build/Debug/Hyv.app`

### Run the app
```bash
# Run directly
open build/Debug/Hyv.app

# Or run from terminal to see console output (useful for debugging)
build/Debug/Hyv.app/Contents/MacOS/Hyv
```

### Grant Permissions
On first launch, macOS will prompt for two permissions:

**Screen & System Audio Recording** (for capturing remote participants):
- Go to **System Settings → Privacy & Security → Screen & System Audio Recording**
- Add **Hyv.app** to the **top section** ("Screen & System Audio Recording"), NOT the "System Audio Recording Only" section
- Make sure the toggle is **ON** (green)
- **Quit and relaunch** the app after granting permission — macOS caches permissions

**Microphone** (for capturing your voice):
- Go to **System Settings → Privacy & Security → Microphone**
- Toggle **ON** for Hyv.app

**Important:** After each rebuild, macOS may see the new binary as a different app. If you get permission errors after rebuilding:
1. Remove old Hyv entries from the Screen Recording list (select, click **−**)
2. Re-add `build/Debug/Hyv.app` with the **+** button
3. Quit and relaunch

---

## Option B: Build & Run from Xcode GUI

### Open the project
```bash
cd /Users/ritujoychowdhury/Documents/Github/hyv
xcodegen generate
open Hyv.xcodeproj
```

### In Xcode
1. The project opens. You should see "Hyv" in the file navigator on the left
2. At the top center, make sure the scheme says **Hyv** and the destination says **My Mac**
3. Press **⌘R** (Cmd+R) to build and run
4. The app appears as a waveform icon in your **menu bar** (top right of screen, near WiFi/battery)
5. Click the icon to see the popover UI

### If you see build errors
- Make sure the scheme is set to "Hyv" (top center dropdown)
- Make sure destination is "My Mac" (not iPhone simulator)
- Try **⌘⇧K** (Cmd+Shift+K) to clean build folder, then **⌘R** again

---

## Testing the App

### Quick test (no real meeting needed)
1. Launch the app — you'll see a waveform icon in the menu bar
2. Click it — should show "Ready"
3. Click **Start Recording**
4. Play some audio on your Mac (YouTube, music, etc.) and speak into your mic
5. Wait a few seconds, then click **Stop Recording**
6. The app transitions to "Processing..." — this runs the Python pipeline
7. When done, check your Desktop for `Hyv_Transcript_*.txt`
8. Your speech should be labeled "Me", played audio labeled "Remote"

### Test with a real meeting
1. Start a call in any meeting app (Zoom, Teams, WhatsApp, etc.)
2. Click the menu bar icon and press **Start Recording**
3. Have a conversation
4. Click **Stop Recording** when done
5. Wait for processing (roughly 1:1 ratio — 10 min meeting ≈ 10 min processing)
6. Transcript appears on Desktop with "Me" for your voice and "Remote" / "Remote (SPEAKER_XX)" for others

### Test the Python script directly
```bash
source venv/bin/activate
python3 scripts/diarize_and_transcribe.py \
  --audio /path/to/recording.wav \
  --hf-token YOUR_HF_TOKEN \
  --cohere-key YOUR_COHERE_KEY
```

---

## Troubleshooting

### "No Cohere API key" or "No HuggingFace token"
The app looks for `.env` by walking up from the app bundle to the repo root. If it can't find it:
- Ensure `.env` is at the repo root: `/Users/ritujoychowdhury/Documents/Github/hyv/.env`
- Or copy it next to the bundle: `cp .env build/Debug/.env`
- Or place it at `~/.hyv/.env` for a persistent location

### "Grant Screen Recording permission"
- System Settings → Privacy & Security → Screen & System Audio Recording
- Add Hyv.app to the **top** "Screen & System Audio Recording" section
- Toggle **ON**, then **quit and relaunch** the app
- After rebuilds, you may need to remove and re-add Hyv.app

### Reading logs

All services emit structured logs via `os.Logger`. Stream them live while the app is running:

```bash
log stream --predicate 'subsystem == "com.hyv.app"' --level debug
```

Useful filters:
```bash
# Only errors and warnings
log stream --predicate 'subsystem == "com.hyv.app"' --level error

# Specific service (e.g. diarization)
log stream --predicate 'subsystem == "com.hyv.app" AND category == "diarization"'
```

Or open **Console.app**, search for `com.hyv.app` in the top-right filter box.

Log categories: `config`, `meeting-detection`, `audio-capture`, `audio-recorder`, `diarization`, `transcription`, `transcript-writer`, `app-state`.

### "Processing failed: Script not found"
The app walks up from its bundle path to find `scripts/diarize_and_transcribe.py`. If the app is nested too deep or outside the repo, it may fail. Run from the repo directory or ensure the build output is within the repo tree.

### "Processing failed: Python not found"
The app looks for Python in this order:
1. `<repo_root>/venv/bin/python3` (project venv — preferred)
2. `/opt/homebrew/bin/python3`
3. `/usr/local/bin/python3`
4. `/usr/bin/python3`

Make sure the venv exists: `python3 -m venv venv && source venv/bin/activate && pip install -r scripts/requirements.txt`

### "Processing failed" with pyannote errors
- Ensure you accepted all HuggingFace model licenses (see Prerequisites §3)
- Ensure ffmpeg is installed: `brew install ffmpeg`
- First run downloads model weights (~1GB) — needs internet access

### Build says "xcodebuild requires Xcode"
```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
```

---

## Rebuilding After Code Changes

```bash
# Regenerate project (if project.yml changed)
xcodegen generate

# Rebuild
xcodebuild -project Hyv.xcodeproj -scheme Hyv -configuration Debug build SYMROOT=build

# Run
build/Debug/Hyv.app/Contents/MacOS/Hyv
```

Or in Xcode: just press **⌘R** again.
