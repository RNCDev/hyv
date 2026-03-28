use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum AppStatus {
    ModelDownloading { progress: f64, message: String },
    Idle,
    Recording,
    Processing { progress: f64, message: String },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub status: AppStatus,
}

pub struct AppState {
    pub status: Mutex<AppStatus>,
    pub system_buffer: Arc<Mutex<Vec<f32>>>,
    pub mic_buffer: Arc<Mutex<Vec<f32>>>,
    pub recording_active: Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(AppStatus::Idle),
            system_buffer: Arc::new(Mutex::new(Vec::new())),
            mic_buffer: Arc::new(Mutex::new(Vec::new())),
            recording_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}
