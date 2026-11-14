//! Tauri commands: the whole surface the UI can call. Each one validates its
//! input and the current line state before anything goes on the wire.
//!
//! Line indices are zero-based (as in the protocol). The UI adds one for display.

use crate::app::{AppError, AppResult, AppState};
use linecaller_core::{ConnectionStatus, LineView, RecentCall, Settings};
use rlp::{Audio, Command, LineState};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    settings: Settings,
    connection: ConnectionStatus,
    lines: Vec<Option<LineView>>,
    recents: Vec<RecentCall>,
    connect_requested: bool,
    version: &'static str,
}

fn bootstrap(state: &AppState) -> Bootstrap {
    Bootstrap {
        settings: state.settings(),
        connection: state.client.status(),
        lines: state.client.lines(),
        recents: state.recents.lock().unwrap().entries.clone(),
        connect_requested: *state.want_connected.lock().unwrap(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

fn sendable(audio: Audio) -> AppResult<Audio> {
    match audio {
        Audio::Unknown => Err(AppError::new(
            "invalid",
            "Choose PRETALK, HOLD, ON-AIR A or ON-AIR B.",
        )),
        a => Ok(a),
    }
}

#[tauri::command]
pub fn get_bootstrap(state: State<'_, AppState>) -> Bootstrap {
    bootstrap(&state)
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> AppResult<Bootstrap> {
    let before = state.settings();
    let saved = state.save_settings(settings)?;
    if before.endpoint_key() != saved.endpoint_key() && saved.auto_connect {
        *state.want_connected.lock().unwrap() = true;
    }
    state.apply_connection().await?;
    Ok(bootstrap(&state))
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>) -> AppResult<()> {
    let s = state.settings();
    if !s.simulator && s.host.is_empty() {
        return Err(AppError {
            code: "invalidSettings",
            message: "Enter the THipPro host in Settings first.".into(),
            field: Some("host"),
        });
    }
    *state.want_connected.lock().unwrap() = true;
    state.apply_connection().await?;
    state.client.retry_now().await;
    Ok(())
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> AppResult<()> {
    *state.want_connected.lock().unwrap() = false;
    state.apply_connection().await
}

/// Outgoing call on an idle line. Refused if the line is not Disconnected,
/// so a busy line can never get a second outgoing call.
#[tauri::command]
pub async fn dial(
    state: State<'_, AppState>,
    line_index: u8,
    audio: Audio,
    number: String,
) -> AppResult<Vec<RecentCall>> {
    let line = state.check_line(line_index)?;
    let audio = sendable(audio)?;
    let number = number.trim().to_string();
    if number.is_empty() {
        return Err(AppError::new("invalid", "Enter a number to call."));
    }
    rlp::message::validate_phone_number(&number)
        .map_err(|e| AppError::new("invalid", e.to_string()))?;
    let v = state.fresh_line(line)?;
    state.check_owner(&v)?;
    AppState::require_state(&v, &[LineState::Disconnected], "start a call")?;
    state
        .client
        .send(Command::Dial {
            line,
            audio,
            location: state.location(),
            number: number.clone(),
        })
        .await?;
    let entries = {
        let mut r = state.recents.lock().unwrap();
        r.record(RecentCall {
            number,
            line_number: line.display_number(),
            at_ms: linecaller_core::store::now_ms(),
        });
        r.entries.clone()
    };
    state.save_recents();
    Ok(entries)
}

/// Accept a ringing call onto the chosen audio destination. Uses the Dial
/// command, whose number field the THipPro ignores for a pending call
/// (spec §4.2.1); an empty number is sent.
#[tauri::command]
pub async fn accept_call(
    state: State<'_, AppState>,
    line_index: u8,
    audio: Audio,
) -> AppResult<()> {
    let line = state.check_line(line_index)?;
    let audio = sendable(audio)?;
    let v = state.fresh_line(line)?;
    state.check_owner(&v)?;
    AppState::require_state(&v, &[LineState::Ringing], "answer")?;
    Ok(state
        .client
        .send(Command::Dial {
            line,
            audio,
            location: state.location(),
            number: String::new(),
        })
        .await?)
}

/// Move a call in progress to another audio destination (spec §4.2.3).
#[tauri::command]
pub async fn set_audio(state: State<'_, AppState>, line_index: u8, audio: Audio) -> AppResult<()> {
    let line = state.check_line(line_index)?;
    let audio = sendable(audio)?;
    let v = state.fresh_line(line)?;
    state.check_owner(&v)?;
    AppState::require_state(
        &v,
        &[
            LineState::Connected,
            LineState::Calling,
            LineState::Reserved,
        ],
        "change audio",
    )?;
    Ok(state
        .client
        .send(Command::SetAudio {
            line,
            audio,
            location: state.location(),
        })
        .await?)
}

#[tauri::command]
pub async fn drop_line(state: State<'_, AppState>, line_index: u8) -> AppResult<()> {
    let line = state.check_line(line_index)?;
    let v = state.fresh_line(line)?;
    state.check_owner(&v)?;
    if v.status.state == LineState::Disconnected {
        return Err(AppError::new(
            "wrongState",
            "There is no call on this line to drop.",
        ));
    }
    Ok(state
        .client
        .send(Command::Drop {
            line,
            location: state.location(),
        })
        .await?)
}

/// The secondary "Audio lock" toggle: Set Lock Audio (spec §4.2.4).
#[tauri::command]
pub async fn set_audio_lock(
    state: State<'_, AppState>,
    line_index: u8,
    locked: bool,
) -> AppResult<()> {
    let line = state.check_line(line_index)?;
    let v = state.fresh_line(line)?;
    state.check_owner(&v)?;
    Ok(state
        .client
        .send(Command::SetLockAudio {
            line,
            locked,
            location: state.location(),
        })
        .await?)
}

/// The LOCK button: Set Lock Line (spec §4.2.5), rejects incoming calls with busy.
#[tauri::command]
pub async fn set_incoming_block(
    state: State<'_, AppState>,
    line_index: u8,
    locked: bool,
) -> AppResult<()> {
    let line = state.check_line(line_index)?;
    let v = state.fresh_line(line)?;
    state.check_owner(&v)?;
    Ok(state
        .client
        .send(Command::SetLockLine {
            line,
            locked,
            location: state.location(),
        })
        .await?)
}

/// Ask the THipPro for the status of every displayed line (spec §4.2.6).
#[tauri::command]
pub async fn refresh_lines(state: State<'_, AppState>) -> AppResult<()> {
    let count = state.settings().line_count;
    for i in 0..count {
        state
            .client
            .send(Command::StatusRequest {
                line: rlp::LineIndex(i),
            })
            .await?;
    }
    Ok(())
}

#[tauri::command]
pub fn clear_recents(state: State<'_, AppState>) -> Vec<RecentCall> {
    state.recents.lock().unwrap().entries.clear();
    state.save_recents();
    Vec::new()
}

/// Simulator mode only: make a line ring with a fake caller.
#[tauri::command]
pub async fn simulate_incoming_call(state: State<'_, AppState>, line_index: u8) -> AppResult<()> {
    let line = state.check_line(line_index)?;
    let sim = state.simulator.lock().await;
    let Some(sim) = sim.as_ref() else {
        return Err(AppError::new(
            "invalid",
            "Simulated calls are only available in simulator mode.",
        ));
    };
    if sim.incoming_call(line) {
        Ok(())
    } else {
        Err(AppError::new(
            "wrongState",
            format!(
                "Line {} is busy or blocks incoming calls.",
                line.display_number()
            ),
        ))
    }
}

/// Export the given settings (as shown in the Settings window) to a
/// pretty-printed JSON file chosen in a native Save dialog. The dialog runs in
/// Rust, so the UI never handles file paths or needs file-system access.
/// Returns the file path, or `None` if the user cancelled.
#[tauri::command]
pub async fn export_config(app: tauri::AppHandle, settings: Settings) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let json = linecaller_core::config_file::export_json(
        &settings,
        env!("CARGO_PKG_VERSION"),
        linecaller_core::store::now_ms(),
    )?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Export LineCaller configuration")
        .add_filter("LineCaller configuration (JSON)", &["json"])
        .set_file_name(linecaller_core::config_file::DEFAULT_FILE_NAME)
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    let Some(path) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let mut path = path
        .into_path()
        .map_err(|e| AppError::new("storage", e.to_string()))?;
    if path.extension().is_none() {
        path.set_extension("json");
    }
    std::fs::write(&path, json).map_err(|e| {
        AppError::new(
            "storage",
            format!("Could not write {}: {e}", path.display()),
        )
    })?;
    Ok(Some(path.display().to_string()))
}

/// Read a configuration file chosen in a native Open dialog and return the
/// validated settings. Nothing is saved: the UI loads them into the Settings
/// form so the user can review (for example change the location) and Save.
#[tauri::command]
pub async fn import_config(app: tauri::AppHandle) -> AppResult<Option<Settings>> {
    use linecaller_core::config_file::{import_json, MAX_FILE_BYTES};
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Import LineCaller configuration")
        .add_filter("LineCaller configuration (JSON)", &["json"])
        .pick_file(move |p| {
            let _ = tx.send(p);
        });
    let Some(path) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|e| AppError::new("storage", e.to_string()))?;
    let meta = std::fs::metadata(&path).map_err(|e| {
        AppError::new(
            "importFailed",
            format!("Could not read {}: {e}", path.display()),
        )
    })?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(AppError::new(
            "importFailed",
            linecaller_core::config_file::ImportError::TooLarge.to_string(),
        ));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| {
        AppError::new(
            "importFailed",
            format!("Could not read {}: {e}", path.display()),
        )
    })?;
    import_json(&text)
        .map(Some)
        .map_err(|e| AppError::new("importFailed", e.to_string()))
}
