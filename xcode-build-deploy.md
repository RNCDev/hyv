# Hyv — Build & Deploy Guide

## Prerequisites

### 1. Python Dependencies
```bash
cd /Users/ritujoychowdhury/Documents/Github/hyv
pip install -r scripts/requirements.txt
```

This installs pyannote.audio, torch, soundfile, numpy, and requests. First run of pyannote will also download ~1GB of model weights from HuggingFace.

### 2. Environment Variables
Ensure your `.env` file at the repo root has both keys:
```
COHERE_TRIAL_API_KEY=your_key_here
HF_TOKEN=your_token_here
```

### 3. Point Xcode Tools to Xcode.app
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

### Grant Screen Recording Permission
On first launch, macOS will prompt you to allow Screen Recording.
- Go to **System Settings → Privacy & Security → Screen Recording**
- Enable **Hyv**
- You may need to restart the app after granting permission

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
2. Click it — should show "No meeting detected"
3. Click **Start Recording** (works even without a meeting app)
4. Play some audio on your Mac (YouTube, music, etc.)
5. Wait a few seconds, then click **Stop Recording**
6. The app transitions to "Processing..." — this runs the Python diarization + transcription
7. When done, check your Desktop for `Hyv_Transcript_*.txt`

### Test with a real meeting
1. Open Zoom, Teams, FaceTime, or any supported meeting app
2. The menu bar icon should change and show "Meeting detected: Zoom"
3. Start a meeting and click **Start Recording**
4. Have a conversation
5. Click **Stop Recording** when done
6. Wait for processing (roughly 1:1 ratio — 10 min meeting ≈ 10 min processing)
7. Speaker-labeled transcript appears on Desktop

---

## Troubleshooting

### "No Cohere API key" or "No HuggingFace token"
The app can't find your `.env` file. Make sure:
- The `.env` file is at the repo root: `/Users/ritujoychowdhury/Documents/Github/hyv/.env`
- Run the app from the repo directory: `cd /Users/ritujoychowdhury/Documents/Github/hyv && build/Debug/Hyv.app/Contents/MacOS/Hyv`

### "Grant Screen Recording permission"
- System Settings → Privacy & Security → Screen Recording → enable Hyv
- Restart the app after granting

### "Processing failed: Python not found"
```bash
# Check Python is available
which python3
python3 --version

# Ensure pyannote is installed
python3 -c "import pyannote.audio; print('OK')"
```

### "Processing failed" with pyannote errors
First run downloads model weights (~1GB). If it fails:
```bash
# Test the script directly
python3 scripts/diarize_and_transcribe.py \
  --audio /path/to/test.wav \
  --hf-token YOUR_HF_TOKEN \
  --cohere-key YOUR_COHERE_KEY
```

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
