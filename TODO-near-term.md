# Near-Term: Accuracy-First Architecture — COMPLETED

Rewrote the transcription pipeline from real-time chunking to batch post-processing with speaker diarization.

## 1. Record Full Audio to Disk ✅
- [x] Replaced `AudioChunkBuffer` with `AudioFileRecorder` that writes raw PCM to a WAV file
- [x] Writes WAV header with placeholder sizes, patches on stop
- [x] Stores recordings in `~/Library/Application Support/Hyv/recordings/`

## 2. Speaker Diarization (pyannote.audio) ✅
- [x] Python script at `scripts/diarize_and_transcribe.py` with pyannote.audio 3.1
- [x] `DiarizationService` shells out to Python via Foundation `Process`
- [x] Outputs JSON with speaker segments, reports progress on stderr
- [x] Accepts HuggingFace token from `.env` (HF_TOKEN)
- [x] Supports `--min-speakers` / `--max-speakers` parameters

## 3. Segment-Based Transcription ✅
- [x] Python script diarizes, then transcribes each speaker segment via Cohere API
- [x] Merges results into ordered transcript: `[MM:SS] SPEAKER_00: text`
- [x] Writes each segment to `.txt` file incrementally during processing

## 4. Post-Processing Pipeline ✅
- [x] AppState orchestrates: record → stop → diarize → transcribe → write
- [x] Progress updates shown in MenuBarView via `.processing(String)` status
- [x] Errors handled gracefully with `.error` state

## 5. Updated AppState & UI ✅
- [x] `startRecording()` only starts audio file recording
- [x] `stopRecording()` triggers post-processing pipeline
- [x] Added `.processing(String)` status with progress spinner in UI
- [x] "Open Transcript" button after processing completes

## 6. Output Format ✅
- [x] Speaker-labeled format: `[MM:SS] SPEAKER_00: text`
- [x] Header includes date, meeting app, duration, speaker count

## Remaining Polish (optional)
- [x] Test with real meeting audio end-to-end
- [x] Merge adjacent same-speaker segments for cleaner output
- [x] Add fallback to unlabeled transcription if diarization fails
- [x] Clean up temp WAV files after successful transcription
- [x] Add setup script to install Python dependencies automatically
