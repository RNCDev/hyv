# Medium-Term: Local MLX Inference

Replace the hosted Cohere API with on-device transcription using Apple Silicon's MLX framework. Keeps all audio private and removes API dependency.

## 1. Research & Validate MLX Path
- [ ] Check if `CohereLabs/cohere-transcribe-03-2026` has an MLX-compatible conversion (or if one can be created via `mlx-lm`)
- [ ] Evaluate `mlx-whisper` or `mlx-audio` as alternative ASR options if Cohere model isn't MLX-ready
- [ ] Benchmark inference speed on M1/M2/M3 — target: process 30 min audio in < 30 min
- [ ] Measure memory usage — model is 2B params, need ~4-8GB depending on quantization

## 2. Python Inference Backend
- [ ] Create `LocalMLXTranscriptionService` conforming to `TranscriptionService` protocol
- [ ] Python script that loads model via `transformers` + MLX backend (or `mlx-lm`)
- [ ] Accept audio file path + language code, return transcribed text
- [ ] Support `torch.compile` and `pipeline_detokenization` for optimized throughput
- [ ] Handle model download and caching in `~/Library/Application Support/Hyv/models/`

## 3. Swift ↔ Python Bridge
- [ ] Option A: Shell out to bundled Python script (simplest, current pyannote approach)
- [ ] Option B: Use `PythonKit` for in-process Python calls (tighter integration)
- [ ] Option C: Run Python as a local HTTP server, call from Swift via URLSession (process isolation)
- [ ] Evaluate tradeoffs: startup time, memory, error handling, packaging complexity

## 4. Model Management UI
- [ ] Settings panel: choose between "Cloud (Cohere API)" and "Local (On-Device)"
- [ ] Model download progress indicator (several GB)
- [ ] Show disk space usage for cached models
- [ ] Option to delete downloaded models

## 5. Quantization & Optimization
- [ ] Evaluate 4-bit and 8-bit quantized variants for smaller footprint
- [ ] Test accuracy impact of quantization on meeting audio (background noise, multiple speakers)
- [ ] Profile Apple Silicon Neural Engine utilization if applicable

## 6. Offline Mode
- [ ] When local model is downloaded, app works fully offline
- [ ] Graceful fallback: if local model not available, offer Cohere API
- [ ] Queue recordings for transcription when model becomes available

## 7. Packaging & Distribution
- [ ] Bundle Python runtime + dependencies (pyinstaller or conda-pack)
- [ ] Or require user to install Python + pip dependencies (simpler, less user-friendly)
- [ ] Consider Swift-native MLX via `mlx-swift` if model support matures
- [ ] Code signing and notarization for distribution outside Mac App Store

## Dependencies
- `mlx` / `mlx-lm` (Apple's ML framework for Apple Silicon)
- `transformers >= 4.56` (HuggingFace model loading)
- `torch` (PyTorch backend, MPS acceleration on Apple Silicon)
- `soundfile`, `librosa` (audio I/O)
- `sentencepiece`, `protobuf` (tokenizer)
