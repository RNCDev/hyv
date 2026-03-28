# Future Improvements

Proven patterns from reference repositories ([meetily](https://github.com/Zackriya-Solutions/meetily/tree/main/backend), [hyprnote](https://github.com/bahodirr/hyprnote), [minute](https://github.com/roblibob/minute), [project-raven](https://github.com/Laxcorp-Research/project-raven)) that are worth pursuing in future iterations.

---

## Quick Wins (Low Complexity)

### Quantized Whisper Models
**Source:** Hyprnote

Use `ggml-small-q8_0.bin` instead of unquantized `ggml-medium.bin`. ~2-3x faster with minor accuracy trade-off. Would cut processing time from ~20-30 min for a 10-min recording down to ~5-10 min.

**Implementation:** Swap model file in `ModelInfo` (`src-tauri/src/transcription/model_manager.rs`), update download URL and file size.

### Auto-Save / Crash Recovery
**Source:** Project Raven

Auto-save partial transcripts every 60 seconds during processing. Recover sessions after crashes instead of losing all progress.

**Implementation:** Save intermediate `Vec<TranscribedSegment>` to `~/Library/Application Support/Hyv/recovery/` during `process_recording`. Check for recovery files on startup.

### Vocabulary Boosting
**Source:** Minute

Custom word lists with configurable strength (gentle/balanced/aggressive) to improve domain-specific transcription accuracy. Useful for technical meetings with jargon.

**Implementation:** Whisper supports an initial prompt parameter (`params.set_initial_prompt()`) that biases output toward specific vocabulary. Could load a user-editable word list from Application Support.

---

## Medium Complexity

### EBU R128 Loudness Normalization
**Source:** Hyprnote

Normalize audio volume levels before feeding to Whisper. Improves accuracy on quiet speech and reduces sensitivity to mic gain settings. Target -23 LUFS with true peak limiting (10ms lookahead window).

**Implementation:** Add a normalization pass in `process_recording` before VAD. Calculate integrated loudness, apply gain. Can use existing `hound` crate for audio manipulation. Hyprnote's approach: recalculate gain every 512 samples with a circular buffer limiter.

**Reference values:** Minute uses FFmpeg two-pass loudnorm with `I=-16:TP=-1.5:LRA=11`.

### Neural VAD (Silero / ONNX)
**Source:** Hyprnote

Replace energy-based RMS VAD (`src-tauri/src/audio/vad.rs`) with ML-based Silero VAD. Far more accurate at distinguishing speech from noise, especially at low volumes or with background noise. Hyprnote bundles the ONNX model directly into the binary via `include_bytes!()`.

**Implementation:** Add `silero-rs` or `ort` (ONNX Runtime) crate. Replace `find_speech_segments` with neural inference. Hyprnote uses `ten-vad-rs` as a secondary VAD for redundancy.

**Key crates:** `silero-rs`, `ten-vad-rs`, `ort`

### Adaptive AEC Bypass
**Source:** Project Raven

Once echo cancellation is implemented, monitor its health metrics (drift >200ms, overflow rates >=10 per check, pipeline stalls) and automatically bypass when AEC is degrading quality. Re-enable after a 5-second holdoff period.

**Implementation:** Only relevant after WebRTC AEC3 is integrated (see below).

---

## High Complexity

### WebRTC AEC3 Echo Cancellation
**Source:** Project Raven

Eliminate speaker bleed at capture time instead of relying on post-hoc deduplication. Uses the same acoustic echo cancellation technology as Chrome. Would remove the need for `deduplicate_bleed()` entirely, producing cleaner source audio for Whisper.

**Implementation:** Requires GStreamer or WebRTC native library integration. Project Raven runs AEC via separate WebSocket connections for each audio stream. The adaptive bypass logic (see above) handles edge cases where AEC degrades quality.

### Speaker Embedding / Clustering Diarization
**Source:** Hyprnote, Minute

True multi-speaker (3+) identification on a single audio channel using MFCC-based speaker embeddings with cosine similarity clustering. Would replace the current two-channel assumption (Speaker 1 = mic, Speaker 2 = system) with actual voice-based identification.

**Implementation:**
- Segmentation: ONNX model analyzing 10-second windows (Hyprnote uses 270-sample frames with 721-sample initial offset)
- Embedding: `knf_rs::compute_fbank` for MFCC feature extraction
- Clustering: k-means on embeddings, Minute uses 0.55 cosine distance threshold
- **Key crates:** `ort`, `knf_rs`, `dasp`

### Streaming Transcription
**Source:** Hyprnote

Real-time per-segment output during recording instead of batch processing after stop. Would enable live captions in the UI.

**Implementation:** Requires rearchitecting from batch to streaming pipeline. Hyprnote uses a `TranscriptionTask<S, T>` struct that yields segments as they're transcribed from 512-byte (~64ms) audio chunks. VAD runs continuously, feeding speech segments to Whisper in real-time.

**Trade-off:** Conflicts with our "accuracy over speed" design principle. Streaming transcription is less accurate than batch processing of full recordings. Could offer as an optional mode.

---

## Priority Recommendation

Based on impact vs. effort:

1. **Quantized models** — immediate processing speed improvement, trivial to implement
2. **Neural VAD** — biggest accuracy improvement for speech detection, medium effort
3. **EBU R128 normalization** — improves Whisper accuracy across varying audio conditions
4. **Speaker embeddings** — enables true multi-speaker support, high effort but high value
5. **WebRTC AEC3** — eliminates bleed at source, high effort but would simplify the pipeline
