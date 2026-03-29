# Future Improvements

## NVIDIA Parakeet and Cohere Transcribe via ONNX

Replace or supplement Whisper with NVIDIA's Parakeet-TDT model running locally via ONNX Runtime. May 

**Why:** Parakeet-TDT-1.1B scores lower WER than Whisper large-v3 on English conversational speech benchmarks, with faster inference. It uses a CTC/TDT architecture that is fundamentally better at handling quiet or sub-vocalized speech — the primary remaining accuracy floor in Hyv.

Cohere is also a newer model

**Implementation path:**
- Add `ort` crate (ONNX Runtime Rust bindings) with `load-dynamic` feature
- Ship `libonnxruntime.dylib` (1.22.x, ARM64) as a Tauri resource bundle — copy to `~/Library/Application Support/Hyv/` on first launch, set `ORT_DYLIB_PATH` at runtime
- Download ONNX-exported Parakeet model from Hugging Face (`istupakov/parakeet-tdt-0.6b-v3-onnx`) via the existing `ModelManager` download infrastructure
- Implement audio feature extraction (log-mel filterbank, 80 bins, 10ms hop) in Rust — Parakeet expects the same 16kHz mono input as Whisper
- Add `parakeet` as a third option in the `ModelInfo` / model selector UI
- Use per-channel strategy: Parakeet for mic (better on noisy conversational), Whisper large-v3-turbo for system (already at 2.1% WER)
- Follow this link for Cohere: https://huggingface.co/models?other=base_model:quantized:CohereLabs/cohere-transcribe-03-2026

**Key blocker:** `ort` rc.10 requires ONNX Runtime 1.22.x dylib at runtime via `dlopen`. The dylib must be bundled in the Tauri app bundle and its path set before the first `ort` call. Tauri's resource system (`tauri::AppHandle::path().resource_dir()`) can locate it.

**Key crates:** `ort = "=2.0.0-rc.10"` (pinned — newer versions use ndarray 0.17 which conflicts with silero-vad-rust), `ndarray = "0.16"`
