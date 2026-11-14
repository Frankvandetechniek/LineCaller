//! Application state shared by the Tauri commands: settings, recents, the
//! connection handle and (in simulator mode) the simulator.

use linecaller_core::persist::{load_json, save_json};
use linecaller_core::{
    ClientConfig, ClientHandle, LevelLink, LineView, Recents, Settings, Simulator,
};
use rlp::{LineIndex, LineState, Location};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub client: ClientHandle,
    pub settings: Mutex<Settings>,
    pub recents: Mutex<Recents>,
    pub simulator: tokio::sync::Mutex<Option<Simulator>>,
    /// Optional read-only Command Protocol link for the level meters.
    pub level_link: LevelLink,
    /// False after the user pressed Disconnect, until they press Connect.
    pub want_connected: Mutex<bool>,
    pub settings_path: PathBuf,
    pub recents_path: PathBuf,
}

/// Error returned to the frontend. `field` points at a settings field when
/// validation of that field failed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<&'static str>,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            field: None,
        }
    }
}

impl From<linecaller_core::SendError> for AppError {
    fn from(e: linecaller_core::SendError) -> Self {
        let code = match e {
            linecaller_core::SendError::NotConnected => "notConnected",
            linecaller_core::SendError::Invalid(_) => "invalid",
            _ => "io",
        };
        AppError::new(code, e.to_string())
    }
}

impl From<linecaller_core::SettingsError> for AppError {
    fn from(e: linecaller_core::SettingsError) -> Self {
        AppError {
            code: "invalidSettings",
            message: e.to_string(),
            field: Some(e.field()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl AppState {
    pub fn load(
        client: ClientHandle,
        level_link: LevelLink,
        config_dir: PathBuf,
        data_dir: PathBuf,
    ) -> Self {
        let settings_path = config_dir.join("settings.json");
        let recents_path = data_dir.join("recents.json");
        let settings: Settings = load_json(&settings_path);
        let settings = settings.validated().unwrap_or_default();
        let recents: Recents = load_json(&recents_path);
        let auto = settings.auto_connect;
        Self {
            client,
            settings: Mutex::new(settings),
            recents: Mutex::new(recents),
            simulator: tokio::sync::Mutex::new(None),
            level_link,
            want_connected: Mutex::new(auto),
            settings_path,
            recents_path,
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn location(&self) -> Location {
        Location(self.settings.lock().unwrap().location)
    }

    /// Start/stop the simulator as needed and point the connection at the
    /// right endpoint.
    pub async fn apply_connection(&self) -> AppResult<()> {
        let s = self.settings();
        let want = *self.want_connected.lock().unwrap();
        let mut sim = self.simulator.lock().await;
        let cfg: Option<ClientConfig> = if s.simulator {
            if sim.is_none() {
                let started = Simulator::start("127.0.0.1:0".parse().unwrap(), s.location)
                    .await
                    .map_err(|e| {
                        AppError::new("simulator", format!("Could not start the simulator: {e}"))
                    })?;
                *sim = Some(started);
            }
            let sim = sim.as_ref().unwrap();
            sim.set_visible_lines(s.line_count);
            want.then(|| ClientConfig {
                host: "127.0.0.1".into(),
                port: sim.addr().port(),
                ..s.client_config()
            })
        } else {
            *sim = None;
            (want && !s.host.is_empty()).then(|| s.client_config())
        };
        drop(sim);
        self.level_link
            .configure(if want { s.level_config() } else { None });
        self.client.configure(cfg).await;
        Ok(())
    }

    pub fn save_settings(&self, new: Settings) -> AppResult<Settings> {
        let new = new.validated()?;
        save_json(&self.settings_path, &new)
            .map_err(|e| AppError::new("storage", format!("Could not save settings: {e}")))?;
        *self.settings.lock().unwrap() = new.clone();
        Ok(new)
    }

    pub fn save_recents(&self) {
        let r = self.recents.lock().unwrap().clone();
        if let Err(e) = save_json(&self.recents_path, &r) {
            tracing::warn!("could not save recents: {e}");
        }
    }

    /// Check that a line index is one of the displayed lines.
    pub fn check_line(&self, index: u8) -> AppResult<LineIndex> {
        let count = self.settings.lock().unwrap().line_count;
        if index >= count {
            return Err(AppError::new(
                "invalid",
                format!(
                    "Line {} is not one of the {count} lines shown",
                    index as u16 + 1
                ),
            ));
        }
        LineIndex::new(index).map_err(|e| AppError::new("invalid", e.to_string()))
    }

    /// Without conference mode, move our other lines on the target ON AIR bus
    /// to HOLD first, so the THipPro never drops them (spec §4.2.3, §4.2.7).
    pub async fn hold_others_on_bus(&self, target: LineIndex, audio: rlp::Audio) -> AppResult<()> {
        let s = self.settings();
        let lines = self.client.lines();
        for line in linecaller_core::store::lines_to_hold_before(
            &lines,
            target.0,
            audio,
            s.location,
            s.conference_mode,
        ) {
            self.client
                .send(rlp::Command::SetAudio {
                    line,
                    audio: rlp::Audio::Hold,
                    location: self.location(),
                })
                .await?;
        }
        Ok(())
    }

    /// Latest known status of a line, required to be fresh (not stale).
    pub fn fresh_line(&self, index: LineIndex) -> AppResult<LineView> {
        match self.client.line(index.0) {
            Some(v) if !v.stale => Ok(v),
            Some(_) => Err(AppError::new(
                "notConnected",
                "The line status is out of date because the THipPro is not connected.",
            )),
            None => Err(AppError::new(
                "unknownState",
                format!(
                    "Line {} has not reported its status yet.",
                    index.display_number()
                ),
            )),
        }
    }

    /// Refuse to act on a line another location occupies (spec §4.1: only
    /// that location may modify it; the THipPro would ignore us anyway).
    pub fn check_owner(&self, v: &LineView) -> AppResult<()> {
        let me = self.location();
        if v.status.is_occupied_by_other(me) {
            return Err(AppError::new(
                "inUseElsewhere",
                format!(
                    "Line {} is in use by Remote Light location {}.",
                    v.status.line.display_number(),
                    v.status.location
                ),
            ));
        }
        Ok(())
    }

    pub fn require_state(v: &LineView, allowed: &[LineState], action: &str) -> AppResult<()> {
        if allowed.contains(&v.status.state) {
            Ok(())
        } else {
            Err(AppError::new(
                "wrongState",
                format!(
                    "Cannot {action} on line {} while it is {:?}.",
                    v.status.line.display_number(),
                    v.status.state
                ),
            ))
        }
    }
}
