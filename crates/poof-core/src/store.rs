//! Tiny JSON persistence helpers shared by settings, history and the monitor.

use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};

use crate::{Error, Result};

/// Reads a JSON file. A missing file is `Ok(None)`, not an error.
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| Error::Json { path: path.to_owned(), source }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io("read", path)(e)),
    }
}

/// Writes JSON atomically (temp file + rename) so a crash never leaves a
/// half-written file behind.
pub(crate) fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(Error::io("create", parent))?;
    }
    let bytes = serde_json::to_vec(value).map_err(|e| Error::Other(e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes).map_err(Error::io("write", &tmp))?;
    std::fs::rename(&tmp, path).map_err(Error::io("rename", path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn json_round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        assert!(read_json::<Settings>(&path).unwrap().is_none());

        let settings = Settings { alert_drop_gb: 42, ..Settings::default() };
        write_json(&path, &settings).unwrap();
        assert_eq!(read_json::<Settings>(&path).unwrap(), Some(settings));
    }
}
