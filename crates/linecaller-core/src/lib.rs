//! LineCaller core: everything except the UI and the Tauri glue.
//!
//! * [`connection`] – TCP connection manager for the Remote Light Protocol.
//! * [`store`] – latest line status with timing metadata.
//! * [`settings`] – local preferences and their validation.
//! * [`recents`] – last 20 dialled numbers.
//! * [`simulator`] – a THipPro stand-in that speaks RLP over TCP.

pub mod cmdproto;
pub mod config_file;
pub mod connection;
pub mod level_link;
pub mod levels;
pub mod persist;
pub mod recents;
pub mod settings;
pub mod simulator;
pub mod store;

pub use connection::{
    ClientConfig, ClientHandle, ConnectionPhase, ConnectionStatus, EventSink, SendError,
};
pub use level_link::{LevelConfig, LevelLink};
pub use levels::LineLevels;
pub use recents::{RecentCall, Recents};
pub use settings::{Density, Settings, SettingsError, Theme};
pub use simulator::Simulator;
pub use store::{LineStore, LineView};

impl Settings {
    /// Level-meter link to the Command Protocol port, if turned on and a
    /// real THipPro is configured.
    pub fn level_config(&self) -> Option<LevelConfig> {
        (self.level_meters && !self.simulator && !self.host.is_empty()).then(|| LevelConfig {
            host: self.host.clone(),
            port: self.level_port,
            connection_type: self.level_connection_type,
            password: self.level_password.clone(),
        })
    }

    /// Connection parameters for a real THipPro (ignores simulator mode).
    pub fn client_config(&self) -> ClientConfig {
        ClientConfig {
            host: self.host.clone(),
            port: self.port,
            location: rlp::Location(self.location),
            line_count: self.line_count,
            liveness_interval: self
                .liveness_check
                .then(|| std::time::Duration::from_secs(self.liveness_interval_secs as u64)),
            conference: self.conference_mode,
        }
    }
}
