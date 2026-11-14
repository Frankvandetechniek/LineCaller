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
pub const EV_LEVELS: &str = "lc://levels";

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
    fn levels(&self, levels: &linecaller_core::LineLevels) {
        let _ = self.0.emit_to("main", EV_LEVELS, levels);
    }
}

/// Bundle identifier used up to 0.1.0. macOS warned about it because it ends
/// in `.app`; the app folders are named after the identifier, so files from
/// that folder are copied over once.
const LEGACY_IDENTIFIER: &str = "studio.linecaller.app";

/// Copies `file` from the legacy app folder next to `dir` into `dir`, unless
/// `dir` already has it. The old file is left in place. Failures are logged and
/// otherwise ignored: the app then starts with defaults, as on a first run.
fn migrate_legacy_files(dir: &std::path::Path, file: &str) {
    let target = dir.join(file);
    if target.exists() {
        return;
    }
    let Some(legacy) = dir.parent().map(|p| p.join(LEGACY_IDENTIFIER).join(file)) else {
        return;
    };
    if !legacy.is_file() {
        return;
    }
    let result = std::fs::create_dir_all(dir).and_then(|_| std::fs::copy(&legacy, &target));
    match result {
        Ok(_) => tracing::info!("copied {} from {}", file, legacy.display()),
        Err(e) => tracing::warn!("could not copy {} from {}: {e}", file, legacy.display()),
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            migrate_legacy_files(&config_dir, "settings.json");
            migrate_legacy_files(&data_dir, "recents.json");
            let (client, level_link) = tauri::async_runtime::block_on(async {
                (
                    ClientHandle::spawn(Arc::new(TauriSink(handle.clone()))),
                    linecaller_core::LevelLink::spawn(Arc::new(TauriSink(handle))),
                )
            });
            let state = AppState::load(client, level_link, config_dir, data_dir);
            app.manage(state);
            // R/T level meters, 20 times a second. Only the simulator has levels
            // for now: the Remote Light Protocol carries none (see levels.rs).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut iv = tokio::time::interval(std::time::Duration::from_millis(50));
                loop {
                    iv.tick().await;
                    let state = handle.state::<AppState>();
                    if !state.client.status().is_connected() {
                        continue;
                    }
                    let levels = match state.simulator.try_lock() {
                        Ok(sim) => sim.as_ref().map(|s| s.levels()),
                        Err(_) => None,
                    };
                    if let Some(l) = levels {
                        let _ = handle.emit_to("main", EV_LEVELS, l);
                    }
                }
            });
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
            commands::set_incoming_block,
            commands::refresh_lines,
            commands::clear_recents,
            commands::simulate_incoming_call,
            commands::export_config,
            commands::import_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LineCaller");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_legacy_file_once_and_never_overwrites() {
        let root = std::env::temp_dir().join(format!("lc-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join(LEGACY_IDENTIFIER);
        let new = root.join("studio.linecaller.desktop");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("settings.json"), "{\"port\":10101}").unwrap();

        migrate_legacy_files(&new, "settings.json");
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"port\":10101}"
        );

        std::fs::write(new.join("settings.json"), "{\"port\":10102}").unwrap();
        migrate_legacy_files(&new, "settings.json");
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "{\"port\":10102}"
        );

        migrate_legacy_files(&new, "recents.json"); // no legacy file: nothing happens
        assert!(!new.join("recents.json").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
