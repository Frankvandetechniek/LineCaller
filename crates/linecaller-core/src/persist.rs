//! Small JSON files in the app's config/data directory.
//!
//! Nothing LineCaller stores is a secret (no passwords: the Remote Light
//! Protocol has no authentication), so plain JSON is appropriate. Files are
//! written atomically (temp file + rename) so a crash never leaves a
//! half-written settings file, and on Unix they are readable by the owner only.

use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::path::Path;

pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!("{} is unreadable ({e}); using defaults", path.display());
            T::default()
        }),
        Err(_) => T::default(),
    }
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(value)?)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Settings;

    #[test]
    fn round_trip_and_corrupt_file_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("settings.json");
        let s = Settings {
            host: "10.1.2.3".into(),
            line_count: 12,
            ..Default::default()
        };
        save_json(&p, &s).unwrap();
        assert_eq!(load_json::<Settings>(&p), s);
        std::fs::write(&p, b"{not json").unwrap();
        assert_eq!(load_json::<Settings>(&p), Settings::default());
        assert_eq!(
            load_json::<Settings>(&dir.path().join("missing.json")),
            Settings::default()
        );
    }
}
