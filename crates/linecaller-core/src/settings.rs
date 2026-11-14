//! LineCaller preferences. These are local app preferences only; nothing in
//! here is ever sent to the THipPro.

use rlp::{Audio, MAX_LINES, THIPPRO_MAX_LOCATIONS};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// First Remote Light port in the MAGIC THipPro configuration guide
/// (defaults 10100, 10101, 10102, ... one port per remote device).
pub const DEFAULT_RLP_PORT: u16 = 10100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// THipPro host name or IP address.
    pub host: String,
    /// Remote Light Protocol TCP port configured on the THipPro for this location.
    pub port: u16,
    /// Remote Light Location of this installation (1..=6).
    pub location: u8,
    /// Number of line controls to show (1..=16). Lines 1..=n are shown.
    pub line_count: u8,
    pub theme: Theme,
    pub density: Density,
    /// Optional labels per line (index 0 = Line 1). Empty = no label.
    pub line_labels: Vec<String>,
    /// Destination preselected when answering an incoming call.
    pub default_answer_audio: Audio,
    /// Connect automatically on start-up.
    pub auto_connect: bool,
    /// Use the built-in THipPro simulator instead of a real system.
    pub simulator: bool,
    /// Periodically send a Status Request when the link has been quiet and
    /// reconnect if nothing comes back. See docs/PROTOCOL_NOTES.md.
    pub liveness_check: bool,
    pub liveness_interval_secs: u16,
    /// Preference: outline the whole channel strip in the status colour
    /// (on air, ringing, ...). When false only the status line on top is coloured.
    pub strip_outline: bool,
    /// Preference: conference mode for ON-AIR A/B (Remote Light Set Conference,
    /// spec §4.2.7). On: several lines can share an ON-AIR bus. Off: a line
    /// already on the bus is moved to HOLD before another one takes it.
    pub conference_mode: bool,
    /// Optional strip colour per line (index 0 = Line 1): "" = default, or "#rrggbb".
    pub line_colors: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: DEFAULT_RLP_PORT,
            location: 1,
            line_count: 8,
            theme: Theme::Dark,
            density: Density::Comfortable,
            line_labels: vec![String::new(); MAX_LINES as usize],
            default_answer_audio: Audio::Pretalk,
            auto_connect: true,
            simulator: true,
            liveness_check: true,
            liveness_interval_secs: 20,
            strip_outline: true,
            conference_mode: false,
            line_colors: vec![String::new(); MAX_LINES as usize],
        }
    }
}

pub const MAX_LABEL_LEN: usize = 24;

#[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
pub enum SettingsError {
    #[error("Enter the THipPro host name or IP address.")]
    Host,
    #[error("Port must be between 1 and 65535.")]
    Port,
    #[error("Location must be between 1 and 6 for a MAGIC THipPro.")]
    Location,
    #[error("Number of lines must be between 1 and 16.")]
    LineCount,
    #[error("Line labels can be at most 24 characters.")]
    LineLabels,
    #[error("Answer destination must be PRETALK, HOLD, ON-AIR A or ON-AIR B.")]
    DefaultAnswerAudio,
    #[error("Liveness interval must be between 5 and 300 seconds.")]
    LivenessIntervalSecs,
    #[error("Line colours must be empty (default) or a colour like #3366ff.")]
    LineColors,
}

impl SettingsError {
    /// The settings field (camelCase, as the frontend names it) at fault.
    pub fn field(self) -> &'static str {
        match self {
            SettingsError::Host => "host",
            SettingsError::Port => "port",
            SettingsError::Location => "location",
            SettingsError::LineCount => "lineCount",
            SettingsError::LineLabels => "lineLabels",
            SettingsError::DefaultAnswerAudio => "defaultAnswerAudio",
            SettingsError::LivenessIntervalSecs => "livenessIntervalSecs",
            SettingsError::LineColors => "lineColors",
        }
    }
}

impl Settings {
    /// Normalise and validate. Returns the cleaned settings.
    pub fn validated(mut self) -> Result<Self, SettingsError> {
        self.host = self.host.trim().to_string();
        let simulator = self.simulator;
        if !simulator || !self.host.is_empty() {
            let ok = !self.host.is_empty()
                && self.host.len() <= 253
                && self.host.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_' | '[' | ']')
                });
            if !ok {
                return Err(SettingsError::Host);
            }
        }
        if self.port == 0 {
            return Err(SettingsError::Port);
        }
        if !(1..=THIPPRO_MAX_LOCATIONS).contains(&self.location) {
            return Err(SettingsError::Location);
        }
        if !(1..=MAX_LINES).contains(&self.line_count) {
            return Err(SettingsError::LineCount);
        }
        self.line_labels.resize(MAX_LINES as usize, String::new());
        for l in self.line_labels.iter_mut() {
            *l = l.trim().to_string();
            if l.chars().count() > MAX_LABEL_LEN || l.chars().any(char::is_control) {
                return Err(SettingsError::LineLabels);
            }
        }
        self.line_colors.resize(MAX_LINES as usize, String::new());
        for c in self.line_colors.iter_mut() {
            *c = c.trim().to_ascii_lowercase();
            let ok = c.is_empty()
                || (c.len() == 7
                    && c.starts_with('#')
                    && c[1..].chars().all(|h| h.is_ascii_hexdigit()));
            if !ok {
                return Err(SettingsError::LineColors);
            }
        }
        if self.default_answer_audio == Audio::Unknown {
            return Err(SettingsError::DefaultAnswerAudio);
        }
        if !(5..=300).contains(&self.liveness_interval_secs) {
            return Err(SettingsError::LivenessIntervalSecs);
        }
        Ok(self)
    }

    /// Settings that require a new TCP connection when they change.
    pub fn endpoint_key(&self) -> (bool, String, u16) {
        (self.simulator, self.host.clone(), self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real() -> Settings {
        Settings {
            simulator: false,
            host: "10.0.0.5".into(),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_are_valid() {
        assert!(Settings::default().validated().is_ok());
        assert!(real().validated().is_ok());
    }

    #[test]
    fn host_required_for_real_connection() {
        let s = Settings {
            simulator: false,
            ..Default::default()
        };
        assert!(matches!(s.validated(), Err(SettingsError::Host)));
        let s = Settings {
            host: "thip pro".into(),
            ..real()
        };
        assert!(s.validated().is_err());
        assert!(Settings {
            host: " studio-thippro.local ".into(),
            ..real()
        }
        .validated()
        .is_ok());
    }

    #[test]
    fn location_limited_to_six() {
        for (loc, ok) in [(0, false), (1, true), (6, true), (7, false)] {
            assert_eq!(
                Settings {
                    location: loc,
                    ..real()
                }
                .validated()
                .is_ok(),
                ok,
                "location {loc}"
            );
        }
    }

    #[test]
    fn line_count_limits() {
        for (n, ok) in [(0, false), (1, true), (16, true), (17, false)] {
            assert_eq!(
                Settings {
                    line_count: n,
                    ..real()
                }
                .validated()
                .is_ok(),
                ok,
                "lines {n}"
            );
        }
    }

    #[test]
    fn labels_normalised() {
        let s = Settings {
            line_labels: vec![" Studio 1 ".into()],
            ..real()
        }
        .validated()
        .unwrap();
        assert_eq!(s.line_labels.len(), 16);
        assert_eq!(s.line_labels[0], "Studio 1");
        let s = Settings {
            line_labels: vec!["x".repeat(25)],
            ..real()
        };
        assert!(s.validated().is_err());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"host":"h","lineCount":4}"#).unwrap();
        assert_eq!(s.line_count, 4);
        assert_eq!(s.port, DEFAULT_RLP_PORT);
    }

    #[test]
    fn line_colours_are_normalised_and_checked() {
        let mut s = real();
        s.line_colors = vec![" #AABBCC ".into(), String::new()];
        let v = s.clone().validated().unwrap();
        assert_eq!(v.line_colors.len(), MAX_LINES as usize);
        assert_eq!(v.line_colors[0], "#aabbcc");
        for bad in ["red", "#abc", "#gggggg", "aabbcc1"] {
            s.line_colors = vec![bad.into()];
            assert_eq!(
                s.clone().validated().unwrap_err(),
                SettingsError::LineColors
            );
        }
    }
}
