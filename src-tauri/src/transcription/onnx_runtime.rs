//! ONNX Runtime initialisation and session factory.
//!
//! Call `init(dylib_path)` once inside the Tauri `.setup()` closure before
//! any OnnxEngine is constructed. Subsequent calls are no-ops.
//! Call `session(model_path)` to create an inference session.

use std::path::Path;
use ort::session::{Session, builder::GraphOptimizationLevel};

/// Set ORT_DYLIB_PATH so `ort` rc.10 can dlopen the bundled runtime library.
/// Must be called before any `session()` call.
pub fn init(dylib_path: &Path) -> Result<(), String> {
    if !dylib_path.exists() {
        return Err(format!(
            "libonnxruntime.dylib not found at {:?}. ONNX models will not work.",
            dylib_path
        ));
    }
    // ort rc.10 reads ORT_DYLIB_PATH at the first session creation.
    // SAFETY: called once during setup before any threads read the environment.
    unsafe { std::env::set_var("ORT_DYLIB_PATH", dylib_path) };
    tracing::info!("ONNX Runtime dylib configured: {:?}", dylib_path);
    Ok(())
}

/// Create an ONNX inference session from a model file.
/// Optimisation level 3 + 4 intra-op threads.
pub fn session(model_path: &Path) -> Result<Session, String> {
    // ort rc.10 API: Session::builder() (no Environment, no SessionBuilder::new)
    Session::builder()
        .map_err(|e| format!("ORT builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("ORT opt level: {e}"))?
        .with_intra_threads(4)
        .map_err(|e| format!("ORT threads: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| format!("ORT session load '{}': {e}", model_path.display()))
}
