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
    pub sha256: Option<String>,
}

impl ModelInfo {
    pub fn medium() -> Self {
        Self {
            name: "medium".to_string(),
            filename: "ggml-medium.bin".to_string(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
                .to_string(),
            size_bytes: 1_533_774_781,
            sha256: None,
        }
    }

    pub fn small() -> Self {
        Self {
            name: "small".to_string(),
            filename: "ggml-small.bin".to_string(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
                .to_string(),
            size_bytes: 487_601_003,
            sha256: None,
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

        let client = reqwest::Client::new();
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

        // Rename temp to final
        tokio::fs::rename(&temp_path, &path)
            .await
            .map_err(|e| format!("Failed to finalize model file: {e}"))?;

        info!(
            model = %model.name,
            size_mb = downloaded / 1_000_000,
            "Model download complete"
        );

        Ok(path)
    }
}
