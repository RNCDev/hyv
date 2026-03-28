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

### ~~EBU R128 Loudness Normalization~~ ✓ Done (v0.2.14)
Implemented via `ebur128` crate, targeting -16 LUFS. Both channels normalized before VAD and Whisper. Hard limiter clamps to ±1.0. Debug WAVs saved pre/post normalization.

### Neural VAD (Silero / ONNX)
**Source:** Hyprnote

Replace energy-based RMS VAD (`src-tauri/src/audio/vad.rs`) with ML-based Silero VAD. Far more accurate at distinguishing speech from noise, especially at low volumes or with background noise. Hyprnote bundles the ONNX model directly into the binary via `include_bytes!()`.

**Implementation:** Add `silero-rs` or `ort` (ONNX Runtime) crate. Replace `find_speech_segments` with neural inference. Hyprnote uses `ten-vad-rs` as a secondary VAD for redundancy.

**Key crates:** `silero-rs`, `ten-vad-rs`, `ort`

### Adaptive AEC Bypass
**Source:** Project Raven

Monitor AEC health metrics (drift, overflow rates, pipeline stalls) and automatically bypass when AEC is degrading quality. Re-enable after a holdoff period.

**Implementation:** Read `VoipAec3::process()` metrics; if echo return loss enhancement (ERLE) drops below threshold or delay estimate diverges, skip the AEC pass and log a warning. Only relevant once AEC quality is confirmed stable.

---

## High Complexity

### ~~WebRTC AEC3 Echo Cancellation~~ ✓ Done (v0.2.16)
Implemented via pure-Rust `aec3` crate. Detects render-ahead delay via VAD onset comparison and passes it to `VoipAec3::initial_delay_ms()`. Both channels processed at full length — no content trimming. Debug WAV saved as `mic_aec_*.wav`. `deduplicate_bleed()` remains as a safety net.

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
3. ~~**EBU R128 normalization**~~ — ✓ done
4. **Speaker embeddings** — enables true multi-speaker support, high effort but high value
5. ~~**WebRTC AEC3**~~ — ✓ done
6. **Vocabulary boosting** — low effort, useful for domain-specific jargon
