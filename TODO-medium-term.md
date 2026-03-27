# Medium-Term: Local On-Device Inference

Replace the hosted Cohere API with on-device transcription using PyTorch + Apple Metal (MPS). Keeps all audio private and removes API dependency.

## 1. Research & Validate Local Inference
- [x] Evaluate `CohereLabs/cohere-transcribe-03-2026` via `transformers` + PyTorch MPS
- [x] Benchmark inference speed — ~1-3s per segment on M1/M2/M3
- [x] Measure memory usage — ~4GB RAM for full-precision model
- [ ] Evaluate MLX or `mlx-whisper` as lighter alternative to PyTorch
- [ ] Profile Apple Silicon Neural Engine utilization if applicable

## 2. Python Inference Backend
- [x] Local transcription via `--local` flag in `diarize_and_transcribe.py`
- [x] Model loaded once via `transformers.AutoModelForSpeechSeq2Seq`, reused across segments
- [x] MPS (Metal) acceleration for on-device GPU inference
- [x] Silence detection and hallucination filtering to prevent garbage output
- [x] Model download script (`scripts/download_models.py`) for bundled offline use
- [x] Auto-detect bundled `models/` directory in project root
- [ ] Support `torch.compile` for optimized throughput
- [ ] Evaluate 4-bit and 8-bit quantized variants for smaller memory footprint
- [ ] Test accuracy impact of quantization on meeting audio (background noise, multiple speakers)

## 3. Swift ↔ Python Bridge
- [x] Shell out to Python script via `DiarizationService` (subprocess with JSON output)
- [x] 45-minute subprocess timeout — kills hung processes gracefully (SIGTERM → SIGKILL)
- [x] Progress reporting via stderr `PROGRESS:` protocol
- [x] Progress callbacks dispatched to MainActor for thread-safe UI updates

## 4. Model Management UI
- [ ] Settings panel: choose between "Cloud (Cohere API)" and "Local (On-Device)"
- [ ] Model download progress indicator (several GB)
- [ ] Show disk space usage for cached models
- [ ] Option to delete downloaded models

## 5. Offline Mode
- [x] App works fully offline after model download (no API calls needed)
- [x] Graceful fallback: if diarization fails, falls back to Cohere API for unlabeled transcription
- [ ] Queue recordings for transcription when model becomes available

## 6. Packaging & Distribution
- [ ] Bundle Python runtime + dependencies (pyinstaller or conda-pack)
- [ ] Consider Swift-native MLX via `mlx-swift` if model support matures
- [ ] Code signing and notarization for distribution outside Mac App Store
- Currently: user installs Python + `pip install -r scripts/requirements.txt`

## Dependencies
- `transformers >= 4.56` (HuggingFace model loading)
- `accelerate` (device placement for MPS)
- `torch` (PyTorch backend, MPS acceleration on Apple Silicon)
- `pyannote.audio 3.1` (speaker diarization)
- `soundfile`, `numpy` (audio I/O)
- `requests` (Cohere API fallback)
