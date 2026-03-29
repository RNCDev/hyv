pub mod audio;
pub mod commands;
mod debug;
pub mod output;
mod state;
pub mod text_util;
pub mod transcription;

use state::AppState;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

fn focus_window(window: &tauri::WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.eval("window.focus()");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Set up log directory: ~/Library/Logs/Hyv/
    let log_dir = dirs::home_dir()
        .map(|h| h.join("Library/Logs/Hyv"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/Hyv"));
    let _ = std::fs::create_dir_all(&log_dir);

    // Rolling file appender: one file per day, kept for 7 days
    let file_appender = tracing_appender::rolling::daily(&log_dir, "hyv.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false) // no colour codes in log files
        .init();

    // Keep _guard alive for the process lifetime
    std::mem::forget(_guard);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                app.set_activation_policy(ActivationPolicy::Accessory);
            }

            tracing::info!("Hyv v{} started", env!("CARGO_PKG_VERSION"));
            crate::debug::prune_old_files(7);
            Ok(())
        })
        .on_tray_icon_event(|app, event| {
            if matches!(event, tauri::tray::TrayIconEvent::Click { .. }) {
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        focus_window(&window);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::start_recording,
            commands::stop_recording,
            commands::get_recent_transcripts,
            commands::open_transcript,
            commands::delete_transcript,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hyv");
}
