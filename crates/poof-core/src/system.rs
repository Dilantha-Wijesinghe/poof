//! Thin wrappers around macOS facilities: APFS local snapshots (`tmutil`),
//! Full Disk Access detection, Finder and user notifications.

use std::path::Path;
use std::process::Command;

use crate::{Error, Result, time};

/// Runs a command and returns its stdout, turning a non-zero exit into an error.
pub(crate) fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| Error::Command { program: program.to_owned(), message: e.to_string() })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() { output.status.to_string() } else { stderr };
        Err(Error::Command { program: program.to_owned(), message })
    }
}

// ---------------------------------------------------------------------------
// APFS local snapshots
// ---------------------------------------------------------------------------

/// A local Time Machine snapshot. Snapshots pin the blocks of deleted files,
/// which is the most common reason "deleting stuff frees nothing".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub name: String,
    /// Creation time parsed from the name, when it follows Apple's format.
    pub created: Option<i64>,
}

/// Lists local Time Machine snapshots on the boot volume.
pub fn snapshots() -> Result<Vec<Snapshot>> {
    Ok(parse_snapshots(&run("tmutil", &["listlocalsnapshots", "/"])?))
}

/// Asks Time Machine to purge as many local snapshots as it can. Works
/// without administrator rights; snapshots still in use may survive.
pub fn thin_snapshots() -> Result<()> {
    run("tmutil", &["thinlocalsnapshots", "/", "999999999999999", "4"]).map(drop)
}

fn parse_snapshots(output: &str) -> Vec<Snapshot> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("com.apple."))
        .map(|name| Snapshot { name: name.to_owned(), created: snapshot_date(name) })
        .collect()
}

/// `com.apple.TimeMachine.2026-10-15-120000.local` → Unix seconds (local time).
fn snapshot_date(name: &str) -> Option<i64> {
    let stamp = name.strip_prefix("com.apple.TimeMachine.")?.strip_suffix(".local")?;
    let civil = jiff::civil::DateTime::strptime("%Y-%m-%d-%H%M%S", stamp).ok()?;
    let zoned = civil.to_zoned(jiff::tz::TimeZone::system()).ok()?;
    Some(zoned.timestamp().as_second())
}

// ---------------------------------------------------------------------------
// Privacy & Finder
// ---------------------------------------------------------------------------

/// Whether this process has Full Disk Access. Without it, Mail, Messages,
/// Safari, other apps' containers and the Trash cannot be measured.
///
/// The TCC database is readable only with Full Disk Access, which makes it a
/// reliable probe.
pub fn has_full_disk_access() -> bool {
    let probes = [
        Path::new("/Library/Application Support/com.apple.TCC/TCC.db").to_path_buf(),
        crate::paths::home().join("Library/Safari"),
    ];
    probes.iter().any(
        |p| {
            if p.is_dir() { std::fs::read_dir(p).is_ok() } else { std::fs::File::open(p).is_ok() }
        },
    )
}

/// Opens System Settings at Privacy & Security → Full Disk Access.
pub fn open_full_disk_access_settings() -> Result<()> {
    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
}

/// Reveals and selects an item in Finder.
pub fn reveal_in_finder(path: &Path) -> Result<()> {
    run("open", &["-R", &path.to_string_lossy()]).map(drop)
}

/// Opens a URL or file with its default handler.
pub fn open_url(url: &str) -> Result<()> {
    run("open", &[url]).map(drop)
}

/// Shows the native "choose folder" panel. `None` if the user cancelled.
pub fn choose_folder(prompt: &str) -> Option<std::path::PathBuf> {
    let script = format!("POSIX path of (choose folder with prompt \"{}\")", applescript_escape(prompt));
    let path = run("osascript", &["-e", &script]).ok()?;
    let path = path.trim();
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    (!path.is_empty()).then(|| std::path::PathBuf::from(path))
}

/// Posts a user notification. Uses `AppleScript` so it also works from the
/// background `LaunchAgent` without any bundle-level entitlements.
pub fn notify(title: &str, body: &str) -> Result<()> {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(body),
        applescript_escape(title)
    );
    run("osascript", &["-e", &script]).map(drop)
}

fn applescript_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// How long ago a snapshot was taken, for display.
pub fn snapshot_age(snapshot: &Snapshot) -> String {
    snapshot.created.map_or_else(|| "unknown date".to_owned(), time::relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tmutil_output() {
        let out = "Snapshots for disk /:\ncom.apple.TimeMachine.2026-10-15-120000.local\ncom.apple.TimeMachine.bogus\n";
        let snaps = parse_snapshots(out);
        assert_eq!(snaps.len(), 2);
        assert!(snaps[0].created.is_some());
        assert_eq!(snaps[1].created, None);
    }

    #[test]
    fn escapes_applescript_strings() {
        assert_eq!(applescript_escape(r#"say "hi" \ bye"#), r#"say \"hi\" \\ bye"#);
    }

    #[test]
    fn failing_commands_surface_errors() {
        assert!(run("false", &[]).is_err());
        assert_eq!(run("echo", &["ok"]).unwrap().trim(), "ok");
    }
}
