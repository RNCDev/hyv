use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

const MODELS_DIR: &str = "models";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
}

impl ModelInfo {
    pub fn all() -> Vec<Self> {
        vec![
            Self::large_v3_turbo(),
            Self::medium(),
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
        let path = self.model_path(model);
        path.exists() && path.metadata().is_ok_and(|m| m.len() > 0)
    }

    /// Download a model with progress callback.
    /// callback receives (bytes_downloaded, total_bytes).
    pub async fn download<F>(
        &self,
        model: &ModelInfo,
        progress: F,
    ) -> Result<PathBuf, String>
    where
        F: Fn(u64, u64) + Send + 'static,
    {
        let path = self.model_path(model);

        if self.is_downloaded(model) {
            info!(model = %model.name, "Model already downloaded");
            return Ok(path);
        }

        info!(
            model = %model.name,
            url = %model.url,
            "Downloading model"
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
        let response = client
            .get(&model.url)
            .send()
            .await
            .map_err(|e| format!("Download request failed: {e}"))?;

        let total = response.content_length().unwrap_or(model.size_bytes);
        let mut downloaded: u64 = 0;

        let temp_path = path.with_extension("tmp");
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
        if let Err(e) = tokio::fs::rename(&temp_path, &path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(format!("Failed to finalize model file: {e}"));
        }

        info!(
            model = %model.name,
            size_mb = downloaded / 1_000_000,
            "Model download complete"
        );

        Ok(path)
    }
}
