mod audio;
mod commands;
mod output;
mod platform;
mod state;
mod transcription;

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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::ActivationPolicy;
                app.set_activation_policy(ActivationPolicy::Accessory);
            }

            tracing::info!("Hyv v0.2.11 started");
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
