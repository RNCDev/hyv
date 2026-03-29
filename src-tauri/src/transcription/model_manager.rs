use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

const MODELS_DIR: &str = "models";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Whisper,
    ParakeetOnnx,
    CohereOnnx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    pub kind: ModelKind,
    #[serde(default)]
    pub extra_files: Vec<(String, String, u64)>,  // (filename, url, size_bytes)
}

impl ModelInfo {
    pub fn all() -> Vec<Self> {
        vec![
            Self::large_v3_turbo(),
            Self::medium(),
            Self::wav2vec2(),
            Self::cohere_transcribe(),
        ]
    }

    pub fn by_name(name: &str) -> Option<Self> {
        Self::all().into_iter().find(|m| m.name == name)
    }

    pub fn medium() -> Self {
        Self {
            name: "medium".to_string(),
            filename: "ggml-medium.bin".to_string(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
                .to_string(),
            size_bytes: 1_533_774_781,
            kind: ModelKind::Whisper,
            extra_files: vec![],
        }
    }

    /// large-v3-turbo: OpenAI's speed-optimised large-v3 variant.
    /// ~30% faster than large-v3, minimal accuracy loss, 1.6 GB.
    pub fn large_v3_turbo() -> Self {
        Self {
            name: "large-v3-turbo".to_string(),
            filename: "ggml-large-v3-turbo.bin".to_string(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
                .to_string(),
            size_bytes: 1_620_000_000,
            kind: ModelKind::Whisper,
            extra_files: vec![],
        }
    }

    /// distil-large-v3: Knowledge-distilled large-v3, ~6x faster, within 1% WER.
    /// Best accuracy/speed tradeoff for Apple Silicon. 1.5 GB.
    pub fn distil_large_v3() -> Self {
        Self {
            name: "distil-large-v3".to_string(),
            filename: "ggml-distil-large-v3.bin".to_string(),
            url: "https://huggingface.co/distil-whisper/distil-large-v3-ggml/resolve/main/ggml-distil-large-v3.bin"
                .to_string(),
            size_bytes: 1_515_000_000,
            kind: ModelKind::Whisper,
            extra_files: vec![],
        }
    }

    /// large-v3: Full accuracy, 3.1 GB. Slowest but most capable.
    pub fn large_v3() -> Self {
        Self {
            name: "large-v3".to_string(),
            filename: "ggml-large-v3.bin".to_string(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
                .to_string(),
            size_bytes: 3_094_623_691,
            kind: ModelKind::Whisper,
            extra_files: vec![],
        }
    }

    /// wav2vec2-base-960h: Facebook's CTC model, 91 MB int8 ONNX, English only.
    /// Single-file, raw waveform input, character-level CTC output.
    pub fn wav2vec2() -> Self {
        ModelInfo {
            name: "wav2vec2".into(),
            filename: "wav2vec2-base-960h-int8.onnx".into(),
            url: "https://huggingface.co/Xenova/wav2vec2-base-960h/resolve/main/onnx/model_int8.onnx".into(),
            size_bytes: 95_286_006,
            kind: ModelKind::ParakeetOnnx,
            extra_files: vec![],
        }
    }

    pub fn cohere_transcribe() -> Self {
        let base = "https://huggingface.co/onnx-community/cohere-transcribe-03-2026-ONNX/resolve/main/onnx";
        Self {
            name: "cohere-transcribe".into(),
            filename: "cohere-encoder-q4.onnx".into(),
            url: format!("{base}/encoder_model_q4.onnx"),
            size_bytes: 1_400_792,
            kind: ModelKind::CohereOnnx,
            extra_files: vec![
                (
                    "cohere-encoder-q4.onnx_data".into(),
                    format!("{base}/encoder_model_q4.onnx_data"),
                    2_016_067_072,
                ),
                (
                    "cohere-decoder-merged-q4.onnx".into(),
                    format!("{base}/decoder_model_merged_q4.onnx"),
                    193_099,
                ),
                (
                    "cohere-decoder-merged-q4.onnx_data".into(),
                    format!("{base}/decoder_model_merged_q4.onnx_data"),
                    108_855_296,
                ),
            ],
        }
    }
}

pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new() -> Result<Self, String> {
        let data_dir = dirs::data_dir()
            .ok_or("Cannot find Application Support directory")?
            .join("Hyv")
            .join(MODELS_DIR);

        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create models directory: {e}"))?;

        Ok(Self {
            models_dir: data_dir,
        })
    }

    pub fn model_path(&self, model: &ModelInfo) -> PathBuf {
        self.models_dir.join(&model.filename)
    }

    pub fn is_downloaded(&self, model: &ModelInfo) -> bool {
        let check = |path: &std::path::Path| {
            path.exists() && path.metadata().is_ok_and(|m| m.len() > 0)
        };
        if !check(&self.model_path(model)) {
            return false;
        }
        model.extra_files.iter().all(|(filename, _, _)| {
            check(&self.models_dir.join(filename))
        })
    }

    /// Resolve a file path within the models directory by filename.
    /// Used by build_engine() to locate extra model files (e.g. Cohere decoder).
    pub fn extra_file_path(&self, filename: &str) -> PathBuf {
        self.models_dir.join(filename)
    }

    /// Path for the tokenizer JSON co-located with the ONNX model.
    pub fn tokenizer_path(&self, model: &ModelInfo) -> Option<std::path::PathBuf> {
        match model.kind {
            ModelKind::ParakeetOnnx => Some(self.models_dir.join("parakeet-tokenizer.json")),
            ModelKind::CohereOnnx   => Some(self.models_dir.join("cohere-tokenizer.json")),
            ModelKind::Whisper      => None,
        }
    }

    /// Download a model with progress callback.
    /// callback receives (bytes_downloaded, total_bytes).
    pub async fn download<F>(&self, model: &ModelInfo, progress: F) -> Result<PathBuf, String>
    where
        F: Fn(u64, u64) + Send + Clone + 'static,
    {
        if self.is_downloaded(model) {
            info!(model = %model.name, "Model already downloaded");
            return Ok(self.model_path(model));
        }

        info!(model = %model.name, "Downloading model ({} files)", 1 + model.extra_files.len());

        let total_bytes: u64 = model.size_bytes
            + model.extra_files.iter().map(|(_, _, s)| *s).sum::<u64>();

        let mut offset: u64 = 0;

        // Primary file
        {
            let path = self.model_path(model);
            if !(path.exists() && path.metadata().is_ok_and(|m| m.len() > 0)) {
                let size = model.size_bytes;
                let off = offset;
                let prog = progress.clone();
                let total = total_bytes;
                self.download_url(
                    &model.url,
                    &path,
                    size,
                    move |d, _| prog(off + d, total),
                ).await?;
            }
            offset += model.size_bytes;
        }

        // Extra files
        for (filename, url, size) in &model.extra_files {
            let dest = self.models_dir.join(filename);
            if dest.exists() && dest.metadata().is_ok_and(|m| m.len() > 0) {
                offset += size;
                continue;
            }
            let off = offset;
            let sz = *size;
            let prog = progress.clone();
            let total = total_bytes;
            self.download_url(
                url,
                &dest,
                sz,
                move |d, _| prog(off + d, total),
            ).await?;
            offset += size;
        }

        Ok(self.model_path(model))
    }

    /// Download `url` to `dest`, reporting progress via callback.
    /// `hint_bytes` is used as the total size fallback when Content-Length is absent.
    async fn download_url<F>(
        &self,
        url: &str,
        dest: &std::path::Path,
        hint_bytes: u64,
        progress: F,
    ) -> Result<(), String>
    where
        F: Fn(u64, u64) + Send + 'static,
    {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Download request failed: {e}"))?;

        let total = response.content_length().unwrap_or(hint_bytes);
        let mut downloaded: u64 = 0;

        let temp_path = dest.with_extension("tmp");
        let mut file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|e| format!("Failed to create temp file: {e}"))?;

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {e}"))?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
                .await
                .map_err(|e| format!("Write error: {e}"))?;
            downloaded += chunk.len() as u64;
            progress(downloaded, total);
        }

        // Rename temp to final; clean up temp file if rename fails
        if let Err(e) = tokio::fs::rename(&temp_path, dest).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("Failed to finalize file: {e}"));
        }

        info!(size_mb = downloaded / 1_000_000, dest = %dest.display(), "Download complete");
        Ok(())
    }

    /// Download tokenizer JSON alongside the ONNX model if not already present.
    pub async fn ensure_tokenizer<F>(&self, model: &ModelInfo, progress: F) -> Result<(), String>
    where
        F: Fn(u64, u64) + Send + 'static,
    {
        let tokenizer_url = match model.kind {
            ModelKind::ParakeetOnnx => "https://huggingface.co/Xenova/wav2vec2-base-960h/resolve/main/tokenizer.json",
            ModelKind::CohereOnnx   => "https://huggingface.co/onnx-community/cohere-transcribe-03-2026-ONNX/resolve/main/tokenizer.json",
            ModelKind::Whisper      => return Ok(()),
        };
        let dest = match self.tokenizer_path(model) {
            Some(p) => p,
            None => return Ok(()),
        };
        if dest.exists() {
            return Ok(());
        }
        self.download_url(tokenizer_url, &dest, 0, progress).await
    }
}
