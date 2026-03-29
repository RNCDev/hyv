//! BPE tokenizer for ONNX transcription models.
//!
//! Loads a Hugging Face `tokenizer.json` and decodes token ID sequences to strings.
//! Handles SentencePiece `▁` (U+2581) word-boundary markers.

use std::{collections::HashMap, path::Path};

pub struct Tokenizer {
    vocab: HashMap<u32, String>,
}

impl Tokenizer {
    /// Load vocab from a Hugging Face `tokenizer.json`.
    /// Extracts `model.vocab` (map of string → id) and inverts it to id → string.
    pub fn load(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("tokenizer read '{}': {e}", path.display()))?;
        let json: serde_json::Value = serde_json::from_str(&data)
            .map_err(|e| format!("tokenizer parse: {e}"))?;

        let vocab_map = json
            .get("model")
            .and_then(|m| m.get("vocab"))
            .and_then(|v| v.as_object())
            .ok_or_else(|| "tokenizer.json missing model.vocab".to_string())?;

        let vocab: HashMap<u32, String> = vocab_map
            .iter()
            .filter_map(|(k, v)| v.as_u64().map(|id| (id as u32, k.clone())))
            .collect();

        tracing::info!("Tokenizer loaded: {} tokens from {}", vocab.len(), path.display());
        Ok(Self { vocab })
    }

    /// Decode a sequence of token IDs to a UTF-8 string.
    /// SentencePiece `▁` (U+2581) is replaced with a space.
    pub fn decode(&self, ids: &[u32]) -> String {
        let raw: String = ids
            .iter()
            .filter_map(|id| self.vocab.get(id))
            .cloned()
            .collect::<Vec<_>>()
            .join("");
        raw.replace('\u{2581}', " ").trim().to_string()
    }
}
