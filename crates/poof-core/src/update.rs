//! Self-updating from GitHub Releases.
//!
//! 1. [`check`] asks the GitHub API for the latest release and compares versions.
//! 2. [`install`] downloads the release zip and its minisign signature, verifies
//!    the signature against [`PUBLIC_KEY`] (compiled into the app), unpacks it,
//!    checks the bundle identifier, version and code signature, then swaps the
//!    running app bundle for the new one.
//! 3. [`relaunch`] starts the new version once this process has exited. The
//!    previous version is kept until the new one has started, and is put back
//!    if it doesn't ([`finish_update`]).
//!
//! Downloads go through `curl`, which ships with macOS, so there is no HTTP
//! stack to maintain. Files fetched this way are not quarantined, so
//! Gatekeeper does not interrupt the update.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::{APP_ID, Error, Result, system};

/// The minisign public key that release archives must be signed with.
pub const PUBLIC_KEY: &str = include_str!("../update-key.pub");

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// A `major.minor.patch` version. Pre-releases are never offered as updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// Parses `1.2.3` or `v1.2.3`. Returns `None` for pre-releases (`1.2.3-beta`).
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().trim_start_matches('v').split('.');
        let mut next = || parts.next()?.parse().ok();
        let version = Version(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }

    /// The version of this build.
    pub fn current() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("crate version is valid semver")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A published release that is newer than this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// Release notes (Markdown).
    pub notes: String,
    /// The release page, for manual downloads.
    pub page: String,
    archive: String,
    signature: String,
}

impl Release {
    /// Up to `limit` bullet points from the release notes, as plain text.
    /// Bullets wrapped over several lines are joined back together.
    pub fn highlights(&self, limit: usize) -> Vec<String> {
        let mut items: Vec<String> = Vec::new();
        let mut open = false; // the last line belonged to a bullet
        for line in self.notes.lines() {
            let trimmed = line.trim();
            if let Some(item) = trimmed.strip_prefix("- ") {
                items.push(item.to_owned());
                open = true;
            } else if open && !trimmed.is_empty() && line.starts_with(char::is_whitespace) {
                if let Some(last) = items.last_mut() {
                    last.push(' ');
                    last.push_str(trimmed);
                }
            } else {
                open = false;
            }
        }
        items.into_iter().take(limit).map(|item| item.replace("**", "").replace('`', "")).collect()
    }
}

// ------------------------------------------------------------------- checking

/// Returns the latest release if it is newer than `current`. Blocking.
pub fn check(current: Version) -> Result<Option<Release>> {
    let api =
        REPOSITORY.replacen("https://github.com/", "https://api.github.com/repos/", 1) + "/releases/latest";
    let user_agent = format!("User-Agent: Poof/{current}");
    let json = system::run(
        "curl",
        &["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", "-H", &user_agent, &api],
    )
    .map_err(|_| Error::Other("couldn't reach GitHub".into()))?;
    let release = parse_release(&json)?;
    Ok(release.filter(|r| r.version > current))
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// Turns the GitHub API response into a [`Release`]. Releases without a
/// signed zip are ignored rather than treated as errors.
fn parse_release(json: &str) -> Result<Option<Release>> {
    let api: ApiRelease =
        serde_json::from_str(json).map_err(|e| Error::Other(format!("unexpected response: {e}")))?;
    let Some(version) = Version::parse(&api.tag_name) else { return Ok(None) };
    if api.draft || api.prerelease {
        return Ok(None);
    }
    let url = |name: &str| api.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
    let zip_name = format!("Poof-{version}.zip");
    let (Some(archive), Some(signature)) = (url(&zip_name), url(&format!("{zip_name}.minisig"))) else {
        return Ok(None);
    };
    Ok(Some(Release { version, notes: api.body.unwrap_or_default(), page: api.html_url, archive, signature }))
}

// ----------------------------------------------------------------- installing

/// The `.app` bundle this process runs from, if any (not when run via `cargo`).
pub fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let bundle = exe.parent()?.parent()?.parent()?; // <name>.app/Contents/MacOS/<exe>
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// Downloads, verifies and installs `release` over `bundle`. Blocking.
pub fn install(release: &Release, bundle: &Path) -> Result<()> {
    install_with_key(release, bundle, PUBLIC_KEY)
}

fn install_with_key(release: &Release, bundle: &Path, public_key: &str) -> Result<()> {
    let work = unique_work_dir(release.version);
    fs::create_dir_all(&work).map_err(Error::io("create", &work))?;
    let result = (|| {
        let archive = work.join("update.zip");
        let signature = work.join("update.zip.minisig");
        download(&release.archive, &archive)?;
        download(&release.signature, &signature)?;
        verify(&archive, &signature, public_key)?;

        let unpacked = work.join("unpacked");
        system::run("ditto", &["-x", "-k", &archive.to_string_lossy(), &unpacked.to_string_lossy()])?;
        let new_app = find_app(&unpacked)?;
        check_bundle(&new_app, release.version)?;
        replace(bundle, &new_app)
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

/// A fresh scratch folder, so concurrent installs never share files.
fn unique_work_dir(version: Version) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("{APP_ID}.update-{version}-{}-{n}", std::process::id()))
}

fn download(url: &str, destination: &Path) -> Result<()> {
    system::run("curl", &["-fsSL", "--max-time", "600", "-o", &destination.to_string_lossy(), url]).map(drop)
}

/// Checks the archive's minisign signature against `public_key`.
fn verify(archive: &Path, signature: &Path, public_key: &str) -> Result<()> {
    let invalid = |what: &str| Error::Other(format!("the update's signature is invalid ({what})"));
    let key = minisign_verify::PublicKey::decode(public_key).map_err(|_| invalid("bad public key"))?;
    let signature = minisign_verify::Signature::from_file(signature).map_err(|_| invalid("unreadable"))?;
    let data = fs::read(archive).map_err(Error::io("read", archive))?;
    key.verify(&data, &signature, false).map_err(|_| invalid("does not match"))
}

fn find_app(dir: &Path) -> Result<PathBuf> {
    fs::read_dir(dir)
        .map_err(Error::io("read", dir))?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or_else(|| Error::Other("the update doesn't contain an app".into()))
}

/// Makes sure the unpacked app really is the expected version of Poof.
fn check_bundle(app: &Path, expected: Version) -> Result<()> {
    let identifier = bundle_info(app, "CFBundleIdentifier")?;
    if identifier.trim() != APP_ID || bundle_version(app) != Some(expected) {
        return Err(Error::Other(format!("the update is not Poof {expected}")));
    }
    system::run("codesign", &["--verify", "--deep", "--strict", &app.to_string_lossy()]).map(drop)
}

/// A value from an app's `Info.plist`.
fn bundle_info(app: &Path, key: &str) -> Result<String> {
    let plist = app.join("Contents/Info.plist");
    system::run("plutil", &["-extract", key, "raw", &plist.to_string_lossy()])
}

fn bundle_version(app: &Path) -> Option<Version> {
    Version::parse(&bundle_info(app, "CFBundleShortVersionString").ok()?)
}

/// A hidden folder beside `bundle`: `/Applications/.Poof.app.<suffix>`.
fn beside(bundle: &Path, suffix: &str) -> PathBuf {
    let name = bundle.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    bundle.with_file_name(format!(".{name}.{suffix}"))
}

/// Where the replaced version is kept until the new one has started.
fn backup_of(bundle: &Path) -> PathBuf {
    beside(bundle, "previous")
}

/// Swaps `current` for `new`, restoring `current` if anything goes wrong.
/// The old bundle is kept as [`backup_of`] `current`.
fn replace(current: &Path, new: &Path) -> Result<()> {
    let parent = current.parent().ok_or_else(|| Error::Other("the app has no parent folder".into()))?;
    let staged = beside(current, "update");
    let backup = backup_of(current);
    let _ = fs::remove_dir_all(&staged);
    let _ = fs::remove_dir_all(&backup);

    let denied = |e: Error| {
        if e.is_permission_denied() || matches!(e, Error::Command { .. }) {
            Error::Other(format!(
                "Poof can't write to {}. Download the update from GitHub instead.",
                parent.display()
            ))
        } else {
            e
        }
    };
    // `ditto` copies onto the same volume first, so the final swap is two renames.
    system::run("ditto", &[&new.to_string_lossy(), &staged.to_string_lossy()]).map_err(denied)?;
    fs::rename(current, &backup).map_err(Error::io("move", current)).map_err(denied)?;
    if let Err(e) = fs::rename(&staged, current) {
        let _ = fs::rename(&backup, current);
        let _ = fs::remove_dir_all(&staged);
        return Err(Error::io("install", current)(e));
    }
    Ok(())
}

/// Records the version that was running before an update, so the new version
/// can confirm the update when it starts.
pub fn remember_update(from: Version) {
    remember_in(&note_path(), from);
}

/// If this launch is the first after an update, returns the version updated
/// from. Consumes the note, so it is reported once. Consuming it also tells
/// [`finish_update`] that the new version started.
pub fn take_update_note() -> Option<Version> {
    take_from(&note_path()).filter(|&from| from < Version::current())
}

/// If an update was rolled back because it didn't start, returns the version
/// that failed. Reported once.
pub fn take_failed_update() -> Option<Version> {
    take_from(&failed_note_path())
}

fn note_path() -> PathBuf {
    crate::paths::data_dir().join("updated-from")
}

fn failed_note_path() -> PathBuf {
    crate::paths::data_dir().join("update-failed")
}

fn remember_in(path: &Path, from: Version) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, from.to_string());
}

fn take_from(path: &Path) -> Option<Version> {
    let text = fs::read_to_string(path).ok()?;
    let _ = fs::remove_file(path);
    Version::parse(&text)
}

// ------------------------------------------------------------- first launch

/// Run with this flag (plus a process id and a bundle path), Poof finishes an
/// update instead of opening its window. See [`finish_update`].
pub const FINISH_FLAG: &str = "--finish-update";

/// How long a new version gets to start before the update is rolled back.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// Hands over to the previous version's binary, kept beside the new `bundle`,
/// which runs [`finish_update`] once this process (`pid`) has exited.
pub fn relaunch(bundle: &Path, pid: u32) -> Result<()> {
    let exe = std::env::current_exe().map_err(Error::io("locate", "the running app"))?;
    let name = exe.file_name().ok_or_else(|| Error::Other("the running app has no name".into()))?;
    let watcher = backup_of(bundle).join("Contents/MacOS").join(name);
    Command::new(&watcher)
        .arg(FINISH_FLAG)
        .arg(pid.to_string())
        .arg(bundle)
        .spawn()
        .map(drop)
        .map_err(|e| Error::Command { program: watcher.display().to_string(), message: e.to_string() })
}

/// Waits for process `pid` to exit, then opens the newly installed `bundle`.
/// If it hasn't started within a minute, puts the previous version back and
/// opens that instead, which then reports the failed update. Blocking.
pub fn finish_update(pid: u32, bundle: &Path) -> Result<()> {
    while system::run("kill", &["-0", &pid.to_string()]).is_ok() {
        std::thread::sleep(Duration::from_millis(200));
    }
    let open = |app: &Path| system::run("open", &[&app.to_string_lossy()]).map(drop);
    watch_first_launch(bundle, &note_path(), &failed_note_path(), STARTUP_TIMEOUT, open)
}

/// Opens `bundle` and waits for it to consume `note` (see
/// [`take_update_note`]). On success the backup is removed; otherwise the
/// backup replaces `bundle` and the failed version is written to `failed_note`.
fn watch_first_launch(
    bundle: &Path,
    note: &Path,
    failed_note: &Path,
    timeout: Duration,
    open: impl Fn(&Path) -> Result<()>,
) -> Result<()> {
    let backup = backup_of(bundle);
    if open(bundle).is_ok() && wait_until(timeout, || !note.exists()) {
        return fs::remove_dir_all(&backup).map_err(Error::io("remove", &backup));
    }
    let failed = bundle_version(bundle);
    // Stop the new version in case it hangs rather than crashed.
    let _ = system::run("pkill", &["-f", &bundle.join("Contents/MacOS/").to_string_lossy()]);
    let broken = beside(bundle, "failed");
    let _ = fs::remove_dir_all(&broken);
    fs::rename(bundle, &broken).map_err(Error::io("move", bundle))?;
    fs::rename(&backup, bundle).map_err(Error::io("restore", bundle))?;
    let _ = fs::remove_dir_all(&broken);
    let _ = fs::remove_file(note);
    if let Some(version) = failed {
        remember_in(failed_note, version);
    }
    open(bundle)
}

/// Polls `done` until it holds or `timeout` passes. Returns whether it held.
fn wait_until(timeout: Duration, done: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while !done() {
        if start.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(Version::parse("v1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::parse("0.10.0"), Some(Version(0, 10, 0)));
        assert_eq!(Version::parse("1.2.3-beta.1"), None);
        assert_eq!(Version::parse("1.2"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        assert_eq!(Version(1, 0, 2).to_string(), "1.0.2");
        let _ = Version::current();
    }

    const RELEASE_JSON: &str = r####"{
        "tag_name": "v0.2.0", "draft": false, "prerelease": false,
        "html_url": "https://github.com/o/poof/releases/tag/v0.2.0",
        "body": "### Added\n- **Clean Up:** a new rule\n- Faster `scans`\n",
        "assets": [
            {"name": "Poof-0.2.0.dmg", "browser_download_url": "https://x/Poof-0.2.0.dmg"},
            {"name": "Poof-0.2.0.zip", "browser_download_url": "https://x/Poof-0.2.0.zip"},
            {"name": "Poof-0.2.0.zip.minisig", "browser_download_url": "https://x/Poof-0.2.0.zip.minisig"}
        ]
    }"####;

    #[test]
    fn parses_signed_releases() {
        let release = parse_release(RELEASE_JSON).unwrap().unwrap();
        assert_eq!(release.version, Version(0, 2, 0));
        assert_eq!(release.archive, "https://x/Poof-0.2.0.zip");
        assert_eq!(release.signature, "https://x/Poof-0.2.0.zip.minisig");
        assert_eq!(release.highlights(5), ["Clean Up: a new rule", "Faster scans"]);
    }

    #[test]
    fn wrapped_bullets_are_joined() {
        let notes = "### Fixed\n\n- Builds now lint cleanly on Rust 1.99, and the toolchain is pinned so local\n  builds and CI always agree.\n- Second item\n\nNot a bullet\n  nor this\n";
        let release = Release { notes: notes.into(), ..parse_release(RELEASE_JSON).unwrap().unwrap() };
        assert_eq!(
            release.highlights(5),
            [
                "Builds now lint cleanly on Rust 1.99, and the toolchain is pinned so local builds and CI always agree.",
                "Second item"
            ]
        );
    }

    #[test]
    fn ignores_unsigned_and_pre_releases() {
        let unsigned = RELEASE_JSON.replace("Poof-0.2.0.zip.minisig", "other");
        assert_eq!(parse_release(&unsigned).unwrap(), None);
        let pre = RELEASE_JSON.replace(r#""prerelease": false"#, r#""prerelease": true"#);
        assert_eq!(parse_release(&pre).unwrap(), None);
        assert!(parse_release("not json").is_err());
    }

    /// A throwaway signing key, as `(public key file, secret key)`.
    fn test_key() -> (String, minisign::SecretKey) {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        (pair.pk.to_box().unwrap().into_string(), pair.sk)
    }

    fn sign(path: &Path, secret: &minisign::SecretKey) -> PathBuf {
        let signature = minisign::sign(None, secret, fs::File::open(path).unwrap(), None, None).unwrap();
        let out = path.with_extension("zip.minisig");
        fs::write(&out, signature.into_string()).unwrap();
        out
    }

    #[test]
    fn signatures_must_match_the_key_and_the_data() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        fs::write(&archive, b"payload").unwrap();
        let (public, secret) = test_key();
        let signature = sign(&archive, &secret);
        verify(&archive, &signature, &public).unwrap();

        let (other_public, _) = test_key();
        assert!(verify(&archive, &signature, &other_public).is_err(), "wrong key");
        fs::write(&archive, b"tampered").unwrap();
        assert!(verify(&archive, &signature, &public).is_err(), "modified data");
    }

    /// Builds a minimal, ad-hoc signed `Poof.app` reporting `version`.
    fn fake_app(parent: &Path, version: &str) -> PathBuf {
        let app = parent.join("Poof.app");
        let macos = app.join("Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        fs::copy("/usr/bin/true", macos.join("poof")).unwrap();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{APP_ID}</string>
<key>CFBundleExecutable</key><string>poof</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
</dict></plist>"#
        );
        fs::write(app.join("Contents/Info.plist"), plist).unwrap();
        system::run("codesign", &["--force", "--sign", "-", &app.to_string_lossy()]).unwrap();
        app
    }

    fn release_for(build: &Path, version: Version, secret: &minisign::SecretKey) -> Release {
        let app = fake_app(build, &version.to_string());
        let archive = build.join(format!("Poof-{version}.zip"));
        system::run(
            "ditto",
            &["-c", "-k", "--keepParent", &app.to_string_lossy(), &archive.to_string_lossy()],
        )
        .unwrap();
        let signature = sign(&archive, secret);
        Release {
            version,
            notes: String::new(),
            page: String::new(),
            archive: format!("file://{}", archive.display()),
            signature: format!("file://{}", signature.display()),
        }
    }

    fn installed_version(app: &Path) -> String {
        bundle_version(app).unwrap().to_string()
    }

    #[test]
    fn installs_a_signed_update_over_the_running_bundle() {
        let root = tempfile::tempdir().unwrap();
        let (applications, build) = (root.path().join("Applications"), root.path().join("build"));
        fs::create_dir_all(&applications).unwrap();
        fs::create_dir_all(&build).unwrap();
        let installed = fake_app(&applications, "0.1.0");
        let (public, secret) = test_key();
        let release = release_for(&build, Version(0, 2, 0), &secret);

        install_with_key(&release, &installed, &public).unwrap();
        assert_eq!(installed_version(&installed), "0.2.0");
        assert_eq!(installed_version(&backup_of(&installed)), "0.1.0", "kept until the new version starts");
        let mut leftovers: Vec<_> =
            fs::read_dir(&applications).unwrap().flatten().map(|e| e.file_name()).collect();
        leftovers.sort();
        assert_eq!(leftovers, [".Poof.app.previous", "Poof.app"], "no staging copy remains");
    }

    /// `Poof.app` at `new` installed over a backed-up `old`, as after [`replace`].
    fn updated_app(root: &Path, old: &str, new: &str) -> PathBuf {
        let app = fake_app(&root.join("new"), new);
        let installed = root.join("Applications/Poof.app");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::rename(fake_app(&root.join("old"), old), backup_of(&installed)).unwrap();
        fs::rename(app, &installed).unwrap();
        installed
    }

    #[test]
    fn a_new_version_that_starts_keeps_the_update() {
        let root = tempfile::tempdir().unwrap();
        let installed = updated_app(root.path(), "0.1.0", "0.2.0");
        let (note, failed) = (root.path().join("updated-from"), root.path().join("update-failed"));
        remember_in(&note, Version(0, 1, 0));

        // The new version consumes the note when it starts.
        let start = |_: &Path| fs::remove_file(&note).map_err(Error::io("start", &note));
        watch_first_launch(&installed, &note, &failed, Duration::from_secs(5), start).unwrap();
        assert_eq!(installed_version(&installed), "0.2.0");
        assert!(!backup_of(&installed).exists());
        assert!(!failed.exists());
    }

    #[test]
    fn a_new_version_that_never_starts_is_rolled_back() {
        let root = tempfile::tempdir().unwrap();
        let installed = updated_app(root.path(), "0.1.0", "0.2.0");
        let (note, failed) = (root.path().join("updated-from"), root.path().join("update-failed"));
        remember_in(&note, Version(0, 1, 0));

        let opened = std::cell::Cell::new(0);
        let crash = |_: &Path| {
            opened.set(opened.get() + 1);
            Ok(())
        };
        watch_first_launch(&installed, &note, &failed, Duration::from_millis(300), crash).unwrap();
        assert_eq!(installed_version(&installed), "0.1.0");
        assert_eq!(opened.get(), 2, "the previous version is opened again");
        assert!(!backup_of(&installed).exists() && !beside(&installed, "failed").exists());
        assert!(!note.exists());
        assert_eq!(take_from(&failed), Some(Version(0, 2, 0)));
    }

    #[test]
    fn rejects_updates_signed_with_another_key_and_keeps_the_old_app() {
        let root = tempfile::tempdir().unwrap();
        let (applications, build) = (root.path().join("Applications"), root.path().join("build"));
        fs::create_dir_all(&applications).unwrap();
        fs::create_dir_all(&build).unwrap();
        let installed = fake_app(&applications, "0.1.0");
        let (_, attacker) = test_key();
        let (public, _) = test_key();
        let release = release_for(&build, Version(0, 2, 0), &attacker);

        assert!(install_with_key(&release, &installed, &public).is_err());
        assert_eq!(installed_version(&installed).trim(), "0.1.0");
    }

    #[test]
    fn rejects_a_bundle_whose_version_does_not_match_the_release() {
        let root = tempfile::tempdir().unwrap();
        let (applications, build) = (root.path().join("Applications"), root.path().join("build"));
        fs::create_dir_all(&applications).unwrap();
        fs::create_dir_all(&build).unwrap();
        let installed = fake_app(&applications, "0.1.0");
        let (public, secret) = test_key();
        let mut release = release_for(&build, Version(0, 2, 0), &secret);
        release.version = Version(0, 3, 0);

        assert!(install_with_key(&release, &installed, &public).is_err());
        assert_eq!(installed_version(&installed).trim(), "0.1.0");
    }

    #[test]
    fn update_notes_are_reported_once() {
        let dir = tempfile::tempdir().unwrap();
        let note = dir.path().join("nested/updated-from");
        remember_in(&note, Version(0, 1, 0));
        assert_eq!(take_from(&note), Some(Version(0, 1, 0)));
        assert_eq!(take_from(&note), None, "consumed");
    }

    #[test]
    fn the_embedded_public_key_is_valid() {
        assert!(minisign_verify::PublicKey::decode(PUBLIC_KEY).is_ok());
    }
}
