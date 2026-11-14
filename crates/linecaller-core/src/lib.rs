//! LineCaller core: everything except the UI and the Tauri glue.
//!
//! * [`connection`] – TCP connection manager for the Remote Light Protocol.
//! * [`store`] – latest line status with timing metadata.
//! * [`settings`] – local preferences and their validation.
//! * [`recents`] – last 20 dialled numbers.
//! * [`simulator`] – a THipPro stand-in that speaks RLP over TCP.

pub mod connection;
pub mod persist;
pub mod recents;
pub mod settings;
pub mod simulator;
pub mod store;

pub use connection::{
    ClientConfig, ClientHandle, ConnectionPhase, ConnectionStatus, EventSink, SendError,
};
pub use recents::{RecentCall, Recents};
pub use settings::{Density, Settings, SettingsError, Theme};
pub use simulator::Simulator;
pub use store::{LineStore, LineView};

impl Settings {
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
        }
    }
}
