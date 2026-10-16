//! Well-known locations: the user's home, Poof's own data directories, and
//! helpers to expand `~` and present paths nicely.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{APP_ID, APP_NAME};

/// The mount point of the writable APFS Data volume on modern macOS. Scanning
/// it (rather than `/`) covers every user file exactly once, because `/Users`,
/// `/Applications` etc. are firmlinks into this volume.
pub const DATA_VOLUME: &str = "/System/Volumes/Data";

/// The current user's home directory.
pub fn home() -> PathBuf {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// Expands a leading `~/` to the user's home directory.
pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

/// Renders a path for display, abbreviating the home directory to `~` and
/// stripping the Data volume prefix so paths look like they do in Finder.
pub fn display(path: &Path) -> String {
    let path = without_data_prefix(path);
    let home = home();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Maps `/System/Volumes/Data/Users/x` to the canonical `/Users/x`.
pub fn without_data_prefix(path: &Path) -> PathBuf {
    match path.strip_prefix(DATA_VOLUME) {
        Ok(rest) => Path::new("/").join(rest),
        Err(_) => path.to_path_buf(),
    }
}

/// `~/Library/Application Support/<app id>` — settings, history, samples.
pub fn data_dir() -> PathBuf {
    home().join("Library/Application Support").join(APP_ID)
}

/// `~/Library/Logs/Poof` — the operations log lives here.
pub fn log_dir() -> PathBuf {
    home().join("Library/Logs").join(APP_NAME)
}

/// `~/Library/LaunchAgents/<app id>.monitor.plist`.
pub fn launch_agent_plist() -> PathBuf {
    home().join("Library/LaunchAgents").join(format!("{APP_ID}.monitor.plist"))
}

/// Expands `*` wildcards (one per path segment, matching within that segment)
/// against the file system, returning only paths that exist.
pub fn glob(pattern: &Path) -> Vec<PathBuf> {
    let mut matches = vec![PathBuf::from("/")];
    for segment in pattern.components().skip(1) {
        let segment = segment.as_os_str().to_string_lossy();
        matches = match segment.split_once('*') {
            None => matches.into_iter().map(|m| m.join(&*segment)).collect(),
            Some((prefix, suffix)) => matches
                .iter()
                .flat_map(|m| fs::read_dir(m).into_iter().flatten().flatten())
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.len() >= prefix.len() + suffix.len()
                        && name.starts_with(prefix)
                        && name.ends_with(suffix)
                })
                .map(|e| e.path())
                .collect(),
        };
    }
    matches.retain(|m| m.symlink_metadata().is_ok());
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde() {
        assert_eq!(expand("~/Library"), home().join("Library"));
        assert_eq!(expand("~"), home());
        assert_eq!(expand("/tmp"), PathBuf::from("/tmp"));
    }

    #[test]
    fn strips_data_volume_prefix() {
        assert_eq!(without_data_prefix(Path::new("/System/Volumes/Data/Users/a")), Path::new("/Users/a"));
        assert_eq!(without_data_prefix(Path::new(DATA_VOLUME)), Path::new("/"));
        assert_eq!(without_data_prefix(Path::new("/usr/bin")), Path::new("/usr/bin"));
    }

    #[test]
    fn displays_home_relative() {
        assert_eq!(display(&home().join("Movies")), "~/Movies");
        assert_eq!(display(&home()), "~");
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; bytes]).unwrap();
    }

    #[test]
    fn glob_expands_single_segment_wildcards() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("Slack/Cache/data"), 10);
        write(&dir.path().join("Code/Cache/data"), 10);
        write(&dir.path().join("Other/Nope/data"), 10);
        let mut found = glob(&dir.path().join("*/Cache"));
        found.sort();
        assert_eq!(found, vec![dir.path().join("Code/Cache"), dir.path().join("Slack/Cache")]);
        assert!(glob(&dir.path().join("missing/*")).is_empty());
    }
}
