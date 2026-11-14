//! Tauri shell for LineCaller. Protocol and networking live in the `rlp` and
//! `linecaller-core` crates; this crate only wires them to the window.

mod app;
mod commands;

use app::AppState;
use linecaller_core::{ClientHandle, ConnectionStatus, EventSink, LineView};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// Event names (kept in sync with src/lib/api.ts).
pub const EV_CONNECTION: &str = "lc://connection";
pub const EV_LINE: &str = "lc://line";
pub const EV_WARNING: &str = "lc://warning";

struct TauriSink(AppHandle);

impl EventSink for TauriSink {
    fn connection(&self, status: &ConnectionStatus) {
        let _ = self.0.emit_to("main", EV_CONNECTION, status);
    }
    fn line(&self, view: &LineView) {
        let _ = self.0.emit_to("main", EV_LINE, view);
    }
    fn warning(&self, message: &str) {
        let _ = self.0.emit_to("main", EV_WARNING, message);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let client = tauri::async_runtime::block_on(async {
                ClientHandle::spawn(Arc::new(TauriSink(handle)))
            });
            let state = AppState::load(client, config_dir, data_dir);
            app.manage(state);
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = handle.state::<AppState>().apply_connection().await {
                    tracing::error!("{}", e.message);
                    let _ = handle.emit_to("main", EV_WARNING, e.message);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap,
            commands::save_settings,
            commands::connect,
            commands::disconnect,
            commands::dial,
            commands::accept_call,
            commands::set_audio,
            commands::drop_line,
            commands::set_audio_lock,
            commands::set_incoming_block,
            commands::refresh_lines,
            commands::clear_recents,
            commands::simulate_incoming_call,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LineCaller");
}
