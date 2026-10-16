//! User preferences, persisted as JSON in the app's data directory.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::store::{read_json, write_json};
use crate::{Result, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];
}

impl std::fmt::Display for Appearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Appearance::System => "Match System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    /// When cleaning up caches and junk, move items to the Trash instead of
    /// deleting them. Safer, but frees nothing until the Trash is emptied.
    pub cleanup_to_trash: bool,
    /// Run the background free-space monitor (a `LaunchAgent`).
    pub monitor_enabled: bool,
    /// Notify when free space drops by at least this many GB within 24 hours.
    pub alert_drop_gb: u64,
    /// Notify when free space falls below this many GB.
    pub low_space_gb: u64,
    /// Project build folders untouched for this many days are offered for cleanup.
    pub stale_project_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            cleanup_to_trash: false,
            monitor_enabled: false,
            alert_drop_gb: 10,
            low_space_gb: 20,
            stale_project_days: 30,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        paths::data_dir().join("settings.json")
    }

    /// Loads settings, falling back to defaults when missing or unreadable.
    pub fn load() -> Self {
        read_json(&Self::path()).ok().flatten().unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        write_json(&Self::path(), self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_missing_fields_fall_back_to_defaults() {
        let parsed: Settings = serde_json::from_str(r#"{"low_space_gb": 5, "future": true}"#).unwrap();
        assert_eq!(parsed.low_space_gb, 5);
        assert_eq!(parsed.alert_drop_gb, Settings::default().alert_drop_gb);
    }
}
