//! Free-space monitoring.
//!
//! The app records a [`Sample`] of free space whenever it runs, and the
//! optional `LaunchAgent` runs `poof --check` every 30 minutes to do the same
//! in the background. [`evaluate`] turns the samples into an [`Alert`] when
//! free space drops sharply or runs low, so a disk never fills up silently.

use std::fs;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::size::{ByteSize, gb};
use crate::store::{read_json, write_json};
use crate::{APP_ID, APP_NAME, Error, Result, paths, settings::Settings, system, time, volume};

/// How often the `LaunchAgent` runs.
pub const INTERVAL_SECS: i64 = 30 * time::MINUTE;

/// Never sample more often than this (the app and the agent may both run).
const MIN_GAP: i64 = 10 * time::MINUTE;

/// Samples retained (~2 months at the agent's interval).
const KEEP: usize = 3000;

/// Minimum time between two notifications.
const ALERT_COOLDOWN: i64 = 12 * time::HOUR;

/// The command-line flag that runs a single background check.
pub const CHECK_FLAG: &str = "--check";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    pub at: i64,
    /// Finder's "available" (free + purgeable).
    pub available: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    /// Free space fell by `amount` within the last day.
    Dropped { amount: u64, available: u64 },
    /// Free space is below the configured floor.
    Low { available: u64 },
}

impl Alert {
    pub fn title(&self) -> String {
        match self {
            Alert::Dropped { amount, .. } => format!("Free space dropped by {}", ByteSize(*amount)),
            Alert::Low { .. } => "Your disk is almost full".to_owned(),
        }
    }

    pub fn body(&self) -> String {
        match self {
            Alert::Dropped { available, .. } => {
                format!("{} left. Open {APP_NAME} → Changes to see what grew.", ByteSize(*available))
            }
            Alert::Low { available } => {
                format!("Only {} available. Open {APP_NAME} to free up space.", ByteSize(*available))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------------

fn samples_path() -> PathBuf {
    paths::data_dir().join("samples.jsonl")
}

/// All recorded samples, oldest first.
pub fn samples() -> Vec<Sample> {
    read_samples(&samples_path())
}

/// Measures free space now and appends it, unless a sample was taken very
/// recently (in which case that one is returned).
pub fn record() -> Result<Sample> {
    record_in(&samples_path(), time::now(), || volume::query(Path::new("/")).map(|v| v.available))
}

fn read_samples(path: &Path) -> Vec<Sample> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn record_in(path: &Path, now: i64, measure: impl FnOnce() -> Result<u64>) -> Result<Sample> {
    let mut samples = read_samples(path);
    if let Some(last) = samples.last()
        && (0..MIN_GAP).contains(&(now - last.at))
    {
        return Ok(*last);
    }
    let sample = Sample { at: now, available: measure()? };
    samples.push(sample);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(Error::io("create", parent))?;
    }
    if samples.len() > KEEP + KEEP / 10 {
        // Compact occasionally rather than rewriting on every append.
        let keep = &samples[samples.len() - KEEP..];
        let text: String = keep.iter().map(|s| json_line(s) + "\n").collect();
        fs::write(path, text).map_err(Error::io("write", path))?;
    } else {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| writeln!(f, "{}", json_line(&sample)))
            .map_err(Error::io("write", path))?;
    }
    Ok(sample)
}

fn json_line(sample: &Sample) -> String {
    serde_json::to_string(sample).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Alerts
// ---------------------------------------------------------------------------

/// Decides whether the latest sample warrants an alert. Pure.
pub fn evaluate(samples: &[Sample], settings: &Settings, last_alert: Option<i64>) -> Option<Alert> {
    let latest = samples.last()?;
    if last_alert.is_some_and(|t| latest.at - t < ALERT_COOLDOWN) {
        return None;
    }
    if latest.available < gb(settings.low_space_gb) {
        return Some(Alert::Low { available: latest.available });
    }
    let peak = samples.iter().filter(|s| latest.at - s.at <= time::DAY).map(|s| s.available).max()?;
    let amount = peak.saturating_sub(latest.available);
    (amount >= gb(settings.alert_drop_gb)).then_some(Alert::Dropped { amount, available: latest.available })
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MonitorState {
    last_alert: Option<i64>,
}

fn state_path() -> PathBuf {
    paths::data_dir().join("monitor.json")
}

/// One background check: record a sample, notify if needed. This is what the
/// `LaunchAgent` runs.
pub fn check() -> Result<Option<Alert>> {
    let settings = Settings::load();
    record()?;
    let mut state: MonitorState = read_json(&state_path()).ok().flatten().unwrap_or_default();
    let alert = evaluate(&samples(), &settings, state.last_alert);
    if let Some(alert) = &alert {
        system::notify(&alert.title(), &alert.body())?;
        state.last_alert = Some(time::now());
        write_json(&state_path(), &state)?;
    }
    Ok(alert)
}

// ---------------------------------------------------------------------------
// LaunchAgent
// ---------------------------------------------------------------------------

fn label() -> String {
    format!("{APP_ID}.monitor")
}

fn domain() -> String {
    let uid = fs::metadata(paths::home()).map(|m| m.uid()).unwrap_or(501);
    format!("gui/{uid}")
}

/// Installs (or refreshes) the `LaunchAgent` so `executable --check` runs
/// periodically. Idempotent — safe to call on every launch, which also keeps
/// the agent pointing at the app if it was moved.
pub fn install_agent(executable: &Path) -> Result<()> {
    let plist = paths::launch_agent_plist();
    let contents = agent_plist(&label(), executable);
    if fs::read_to_string(&plist).ok().as_deref() == Some(contents.as_str()) {
        return Ok(());
    }
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent).map_err(Error::io("create", parent))?;
    }
    fs::write(&plist, contents).map_err(Error::io("write", &plist))?;
    let _ = system::run("launchctl", &["bootout", &format!("{}/{}", domain(), label())]);
    system::run("launchctl", &["bootstrap", &domain(), &plist.to_string_lossy()]).map(drop)
}

pub fn uninstall_agent() -> Result<()> {
    let _ = system::run("launchctl", &["bootout", &format!("{}/{}", domain(), label())]);
    let plist = paths::launch_agent_plist();
    match fs::remove_file(&plist) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io("remove", plist)(e)),
        _ => Ok(()),
    }
}

pub fn agent_installed() -> bool {
    paths::launch_agent_plist().exists()
}

fn agent_plist(label: &str, executable: &Path) -> String {
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>{CHECK_FLAG}</string>
    </array>
    <key>StartInterval</key>
    <integer>{INTERVAL_SECS}</integer>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>LowPriorityIO</key>
    <true/>
    <key>Nice</key>
    <integer>10</integer>
</dict>
</plist>
"#,
        label = escape(label),
        exe = escape(&executable.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(at: i64, available_gb: u64) -> Sample {
        Sample { at, available: gb(available_gb) }
    }

    #[test]
    fn alerts_on_a_sharp_drop_within_a_day() {
        let settings = Settings { alert_drop_gb: 10, low_space_gb: 20, ..Settings::default() };
        let samples = [s(0, 150), s(time::HOUR, 148), s(5 * time::HOUR, 120)];
        assert_eq!(
            evaluate(&samples, &settings, None),
            Some(Alert::Dropped { amount: gb(30), available: gb(120) })
        );
    }

    #[test]
    fn ignores_drops_older_than_a_day_and_respects_cooldown() {
        let settings = Settings::default();
        let old_peak = [s(0, 200), s(2 * time::DAY, 100), s(2 * time::DAY + time::HOUR, 99)];
        assert_eq!(evaluate(&old_peak, &settings, None), None);

        let drop = [s(0, 200), s(time::HOUR, 100)];
        assert_eq!(evaluate(&drop, &settings, Some(time::HOUR - 60)), None);
    }

    #[test]
    fn alerts_when_space_is_low() {
        let settings = Settings { low_space_gb: 20, ..Settings::default() };
        assert_eq!(evaluate(&[s(0, 15)], &settings, None), Some(Alert::Low { available: gb(15) }));
        assert!(evaluate(&[], &settings, None).is_none());
    }

    #[test]
    fn records_samples_with_a_minimum_gap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("samples.jsonl");
        record_in(&path, 1_000, || Ok(5)).unwrap();
        let again = record_in(&path, 1_000 + 60, || Ok(6)).unwrap();
        assert_eq!(again.available, 5, "too soon: previous sample reused");
        record_in(&path, 1_000 + MIN_GAP, || Ok(7)).unwrap();
        let all = read_samples(&path);
        assert_eq!(all.iter().map(|s| s.available).collect::<Vec<_>>(), [5, 7]);
    }

    #[test]
    fn plist_escapes_the_executable_path() {
        let plist = agent_plist("x.monitor", Path::new("/Apps/A&B.app/Contents/MacOS/poof"));
        assert!(plist.contains("A&amp;B.app"));
        assert!(plist.contains(CHECK_FLAG));
    }
}
