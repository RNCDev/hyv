# How Hyv Works

End-to-end flow from pressing Record to the transcript file on your Desktop.

---

## Overview

```
[Start Recording]
      │
      ├── MicCapture (CPAL thread)       → mic_buffer:    Vec<f32>  16kHz mono
      └── SystemCapture (Core Audio tap) → system_buffer: Vec<f32>  16kHz mono

[Stop Recording]
      │
      ├── Normalize (-16 LUFS, EBU R128)
      ├── AEC echo cancellation (mic only, delay-aware)
      ├── VAD → speech segments
      ├── Chunk → 30s max audio chunks
      │
      ├── Transcribe mic    → Speaker 1 segments  (Greedy / Cohere greedy)
      ├── Transcribe system → Speaker 2 segments  (BeamSearch / Cohere greedy)
      │
      ├── align_channels()      — fix buffer timing offset if > 8s
      ├── deduplicate_bleed()   — drop mic echo of system TTS
      └── merge_segments()      — merge same-speaker segments within 2s gap
                │
                └── ~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt
```

---

## 1. Audio Capture

Two streams are captured independently and stored as 16kHz mono `f32` samples.

### Microphone (`MicCapture` — CPAL)

- Default input device. Supports F32 and I16 sample formats.
- Multi-channel input averaged to mono; linearly resampled to 16kHz if needed.
- CPAL callback appends to `mic_buffer` via `try_lock()` — samples are dropped on contention (rare).
- Runs in a dedicated `std::thread`. A oneshot channel confirms the stream is live before `start_recording` returns, preventing empty buffers on fast stop.

### System Audio (`SystemCapture` — Core Audio Process Tap)

- macOS-only. Captures all system output via `ca::TapDesc::with_mono_global_tap_excluding_processes()`.
- Audio callback pushes to a lock-free ring buffer (131,072 samples ≈ 8s at 16kHz).
- A drain thread wakes every 50ms, pops from the ring, resamples to 16kHz, and appends to `system_buffer`.
- Non-fatal: if the tap fails to initialize, recording continues mic-only.

---

## 2. Processing Pipeline

Triggered on `stop_recording`. Runs in a Tokio blocking task so it doesn't block the async executor.

### 2a. Normalization

Both buffers are independently normalized to **-16 LUFS** (EBU R128 integrated loudness).

- Measures integrated loudness across the full buffer.
- Applies linear gain to reach target; hard-limits output to ±1.0.
- Skips normalization if the buffer is silence (loudness not finite).

### 2b. Echo Cancellation (mic only)

The mic recording contains acoustic bleed of system audio (speaker audio leaking into the microphone). AEC removes this before transcription.

**Delay detection:** VAD onset timestamps are compared between mic and system to estimate how far the echo lags. Plausible delays are 0–1000ms; outside that range AEC is skipped.

**Cancellation:** WebRTC AEC3. Processed in 160-sample (10ms) frames at 16kHz. The system buffer is the reference signal. Falls back to raw mic if AEC3 init fails.

### 2c. Voice Activity Detection

Energy-based VAD on 30ms frames (480 samples).

| Parameter | Value |
|---|---|
| Energy threshold | 0.002 RMS (on a 1.75× boosted sidechain) |
| Minimum segment duration | 0.3s |
| Hangover | ~200ms (7 frames of silence before segment ends) |
| Merge gap | 1.0s (adjacent segments closer than this are joined) |

Returns a list of `SpeechSegment { start_sample, end_sample }` over the audio buffer.

### 2d. Chunking

VAD segments are split into chunks ≤ 30 seconds for the transcription model.

Chunks are discarded if:
- Duration < 0.4s (prevents punctuation hallucinations)
- RMS energy < 0.002 (prevents hallucinations on near-silence)

Each chunk carries an `offset_secs` timestamp so output segments can be placed on the recording timeline.

---

## 3. Transcription

Both channels are transcribed independently using the selected model. The active engine is determined by `ModelKind` on the selected `ModelInfo`.

### Cohere Transcribe (default — ONNX encoder-decoder)

Two ONNX Runtime sessions: encoder and decoder.

**Encode:** Audio chunk → 128-bin mel spectrogram → encoder session → `hidden_states [1, T, 1024]`

**Greedy decode:** Autoregressive token generation with KV-cache.
- Prompt: `[BOS, LANG_EN, PNC, NOTIMESTAMP]`
- At each step: run decoder with last token + encoder hidden states → argmax over 16,384-token vocab
- Encoder KV-cache is frozen after step 0; decoder KV-cache grows with each step
- Stops at EOS, NOSPEECH, or repetition (same token 3× in a row, or bigram/trigram cycle)
- Max 448 new tokens per chunk

Special tokens (IDs < 14) are filtered before the text is returned.

### Whisper (alternative — ggml via whisper-rs)

- **Mic channel:** Greedy decoding (`best_of=1`) — faster, adequate for conversational speech.
- **System channel:** Beam search (`beam_size=5, patience=1.0`) — higher accuracy for TTS output.
- Language: `"en"` (hardcoded). New `WhisperState` per chunk (stateless).
- Hallucination guards: `no_speech_thold=0.65`, `entropy_thold=2.4`, `logprob_thold=-1.0`.
- Rolling context: the most recent segment from the *other* channel is injected as the initial prompt, provided it ended more than 10 seconds before the current chunk starts (prevents token-doubling artifacts).

---

## 4. Post-Processing

### Channel Alignment

CPAL and the Core Audio tap start at slightly different times. If the first-speech offset between Speaker 1 and Speaker 2 exceeds 8 seconds, Speaker 1 timestamps are shifted by `-offset` to align the two channels. Offsets ≤ 8s are treated as conversational timing (e.g. an AI greeting) and left alone.

### Bleed Deduplication

Speaker 1 (mic) segments that are acoustic echoes of Speaker 2 (system TTS) are dropped.

For each Speaker 1 segment, the algorithm collects all Speaker 2 segments overlapping a window of `[start - 5.0s, end + 1.0s]`. If ≥ 55% of the Speaker 1 words appear in that System text, the segment is classified as bleed and removed.

Guard: segments of ≤ 2 words are never dropped (preserves back-channel responses like "OK", "Got it").

### Merge and Write

Consecutive segments from the same speaker separated by ≤ 2 seconds are merged into one. The result is written to:

```
~/Desktop/Hyv_Transcript_YYYY-MM-DD_HH-MM.txt
```

Format:

```
=== Hyv Transcript ===
Date: March 29, 2026 at 3:45 PM
Duration: 04:12
Speakers: 2

[00:00] Speaker 2: Hi, how can I help you today?
[00:04] Speaker 1: I wanted to ask about pricing.
...
```

---

## 5. Thread Architecture

| Thread | Role |
|---|---|
| Tauri main async | Command handlers, status events |
| `std::thread` (mic) | CPAL stream; held alive until `recording_active` → false |
| `std::thread` (system) | 50ms drain loop from ring buffer |
| Tokio blocking task | Full processing pipeline (VAD → transcription → write) |

Inter-thread state: `Arc<Mutex<Vec<f32>>>` for audio buffers, `Arc<AtomicBool>` for the recording flag.

---

## 6. Debug Artifacts

Written to `~/Library/Application Support/Hyv/debug/`. Pruned after 7 days.

| File | Contents |
|---|---|
| `mic_*.wav` / `system_*.wav` | Raw captured buffers |
| `mic_normalized_*.wav` / `system_normalized_*.wav` | After normalization |
| `mic_aec_*.wav` | After echo cancellation |
| `segments_raw_*.json` | All transcribed segments before dedup |

Logs: `~/Library/Logs/Hyv/hyv.log.YYYY-MM-DD`

### Offline Replay Harness

Run the post-normalization pipeline against saved WAVs without re-recording:

```bash
cargo run --bin replay_pipeline -- \
  --mic ~/Library/Application\ Support/Hyv/debug/mic_normalized_*.wav \
  --system ~/Library/Application\ Support/Hyv/debug/system_normalized_*.wav
```
