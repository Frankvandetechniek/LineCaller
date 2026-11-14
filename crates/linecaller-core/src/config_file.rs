//! Import / export of LineCaller's configuration as a readable JSON file, so
//! a studio can set up one computer and copy the configuration to the next.
//!
//! ```json
//! {
//!   "format": "linecaller-config",
//!   "formatVersion": 1,
//!   "exportedAt": "2026-11-14T06:45:00Z",
//!   "appVersion": "0.1.0",
//!   "settings": { "host": "10.0.0.5", "port": 10100, ... }
//! }
//! ```
//!
//! Only LineCaller's own preferences are in the file (no recents, nothing
//! from the THipPro). On import, missing settings take their defaults,
//! unknown keys are ignored, and the result is validated like any save.

use crate::settings::{Settings, SettingsError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FORMAT: &str = "linecaller-config";
pub const FORMAT_VERSION: u32 = 1;
pub const DEFAULT_FILE_NAME: &str = "linecaller-config.json";
/// Refuse absurdly large files before parsing them.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub format: String,
    pub format_version: u32,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default)]
    pub app_version: String,
    pub settings: Settings,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("The file is larger than {} KB and is not a LineCaller configuration.", MAX_FILE_BYTES / 1024)]
    TooLarge,
    #[error("The file is not valid JSON: {0}")]
    NotJson(String),
    #[error(
        "This is not a LineCaller configuration file (missing \"format\": \"linecaller-config\")."
    )]
    WrongFormat,
    #[error("This configuration was made by a newer LineCaller (format version {0}). Update LineCaller first.")]
    NewerVersion(u32),
    #[error("The configuration contains an invalid setting: {0}")]
    Invalid(SettingsError),
}

/// Pretty-printed JSON (2-space indent) with a trailing newline.
pub fn export_json(
    settings: &Settings,
    app_version: &str,
    now_ms: u64,
) -> Result<String, SettingsError> {
    let settings = settings.clone().validated()?;
    let file = ConfigFile {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        exported_at: rfc3339_utc(now_ms),
        app_version: app_version.into(),
        settings,
    };
    let mut s = serde_json::to_string_pretty(&file).expect("settings serialise");
    s.push('\n');
    Ok(s)
}

pub fn import_json(text: &str) -> Result<Settings, ImportError> {
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| ImportError::NotJson(e.to_string()))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(ImportError::WrongFormat);
    }
    let file: ConfigFile =
        serde_json::from_value(value).map_err(|e| ImportError::NotJson(e.to_string()))?;
    if file.format_version > FORMAT_VERSION {
        return Err(ImportError::NewerVersion(file.format_version));
    }
    file.settings.validated().map_err(ImportError::Invalid)
}

/// Format Unix milliseconds as `YYYY-MM-DDTHH:MM:SSZ` (no extra dependency).
fn rfc3339_utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Theme;

    fn sample() -> Settings {
        let mut s = Settings {
            host: "10.0.0.5".into(),
            simulator: false,
            line_count: 12,
            theme: Theme::Light,
            ..Default::default()
        };
        s.line_labels[0] = "Studio hotline".into();
        s
    }

    #[test]
    fn export_is_readable_and_round_trips() {
        let json = export_json(&sample(), "0.2.0", 1_794_635_100_000).unwrap();
        assert!(
            json.starts_with("{\n  \"format\": \"linecaller-config\",\n  \"formatVersion\": 1,")
        );
        assert!(json.contains("\"exportedAt\": \"2026-11-14T"));
        assert!(json.contains("\"host\": \"10.0.0.5\""));
        assert_eq!(import_json(&json).unwrap(), sample().validated().unwrap());
    }

    #[test]
    fn date_formatting() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(951_782_400_000), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn partial_files_take_defaults_and_ignore_unknown_keys() {
        let s = import_json(r#"{"format":"linecaller-config","formatVersion":1,"settings":{"host":"thippro.local","simulator":false,"futureOption":true}}"#).unwrap();
        assert_eq!(s.host, "thippro.local");
        assert_eq!(s.port, crate::settings::DEFAULT_RLP_PORT);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(matches!(
            import_json("not json"),
            Err(ImportError::NotJson(_))
        ));
        assert!(matches!(
            import_json(r#"{"host":"x"}"#),
            Err(ImportError::WrongFormat)
        ));
        assert!(matches!(
            import_json(r#"{"format":"linecaller-config","formatVersion":9,"settings":{}}"#),
            Err(ImportError::NewerVersion(9))
        ));
        assert!(matches!(
            import_json(
                r#"{"format":"linecaller-config","formatVersion":1,"settings":{"location":7}}"#
            ),
            Err(ImportError::Invalid(SettingsError::Location))
        ));
        assert!(matches!(
            import_json(
                r#"{"format":"linecaller-config","formatVersion":1,"settings":{"port":"abc"}}"#
            ),
            Err(ImportError::NotJson(_))
        ));
    }
}
