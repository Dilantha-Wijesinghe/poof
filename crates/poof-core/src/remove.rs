//! Guarded deletion.
//!
//! Every removal in Poof goes through [`remove`], which
//! 1. refuses protected locations ([`guard`]),
//! 2. moves the item to the Trash or deletes it permanently, and
//! 3. appends a line to `~/Library/Logs/Poof/operations.log`.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use objc2_foundation::{NSFileManager, NSString, NSURL};

use crate::{Error, Result, paths, size::ByteSize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Move to the Trash (recoverable; space is freed when the Trash is emptied).
    Trash,
    /// Delete immediately.
    Delete,
}

/// Something to remove, with its known size so freed space can be reported
/// without re-measuring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// Bytes deleted or moved to the Trash.
    pub bytes: u64,
    pub removed: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

/// Removes `items`, continuing past failures. Blocking.
pub fn remove(items: &[Item], method: Method) -> Outcome {
    let mut outcome = Outcome::default();
    for item in items {
        match guard(&item.path).and_then(|()| remove_one(&item.path, method)) {
            Ok(()) => {
                log(method, item);
                outcome.bytes += item.size;
                outcome.removed.push(item.path.clone());
            }
            Err(e) => outcome.failed.push((item.path.clone(), e.to_string())),
        }
    }
    outcome
}

fn remove_one(path: &Path, method: Method) -> Result<()> {
    match method {
        Method::Trash => move_to_trash(path),
        Method::Delete => {
            let meta = fs::symlink_metadata(path).map_err(Error::io("inspect", path))?;
            if meta.is_dir() {
                fs::remove_dir_all(path).map_err(Error::io("delete", path))
            } else {
                fs::remove_file(path).map_err(Error::io("delete", path))
            }
        }
    }
}

/// Uses `NSFileManager` (not Finder scripting) so there is no Automation
/// permission prompt and no Finder sound.
fn move_to_trash(path: &Path) -> Result<()> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager().trashItemAtURL_resultingItemURL_error(&url, None).map_err(|e| {
        Error::Other(format!("could not move {} to the Trash: {}", path.display(), e.localizedDescription()))
    })
}

/// Exact paths that must never be removed.
const PROTECTED_EXACT: &[&str] = &[
    "/",
    "/Applications",
    "/Library",
    "/System",
    "/Users",
    "/Volumes",
    "/bin",
    "/cores",
    "/dev",
    "/etc",
    "/opt",
    "/private",
    "/private/etc",
    "/private/tmp",
    "/private/var",
    "/sbin",
    "/tmp",
    "/usr",
    "/usr/local",
    "/var",
];

/// Paths whose entire subtree is off-limits.
const PROTECTED_TREES: &[&str] = &[
    "/System",
    "/bin",
    "/sbin",
    "/usr/bin",
    "/usr/lib",
    "/usr/libexec",
    "/usr/sbin",
    "/usr/share",
    "/private/etc",
    "/private/var/db",
    "/Library/Apple",
];

/// Home sub-folders that may be emptied but never removed themselves.
const PROTECTED_HOME: &[&str] = &[
    "Applications",
    "Desktop",
    "Documents",
    "Downloads",
    "Library",
    "Movies",
    "Music",
    "Pictures",
    "Public",
    ".Trash",
];

/// Returns an error if `path` is a location Poof must never delete.
pub fn guard(path: &Path) -> Result<()> {
    let refuse = || Err(Error::Protected(path.to_path_buf()));
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return refuse();
    }
    let canonical = paths::without_data_prefix(path);
    if PROTECTED_EXACT.iter().any(|p| canonical == Path::new(p))
        || PROTECTED_TREES.iter().any(|p| canonical.starts_with(p))
    {
        return refuse();
    }

    let home = paths::home();
    if let Ok(rest) = canonical.strip_prefix(&home) {
        let parts: Vec<_> = rest.components().collect();
        let first = parts.first().map(|c| c.as_os_str().to_string_lossy());
        let protected_home = match parts.len() {
            0 => true, // the home folder itself
            1 => first.is_some_and(|f| PROTECTED_HOME.contains(&f.as_ref())),
            // ~/Library's direct children (Caches, Preferences, …) are structure, not content.
            2 => first.as_deref() == Some("Library"),
            _ => false,
        };
        let keychains = rest.starts_with("Library/Keychains");
        if protected_home || keychains {
            return refuse();
        }
    }
    Ok(())
}

fn log(method: Method, item: &Item) {
    if cfg!(test) {
        return; // keep the real log free of test noise
    }
    let dir = paths::log_dir();
    let line = format!(
        "{}\t{}\t{}\t{}\n",
        jiff::Timestamp::now(),
        match method {
            Method::Trash => "trash",
            Method::Delete => "delete",
        },
        ByteSize(item.size),
        item.path.display()
    );
    // Logging is best-effort: never fail a removal because the log is unwritable.
    let _ = fs::create_dir_all(&dir).and_then(|()| {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("operations.log"))?
            .write_all(line.as_bytes())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_blocks_system_and_structural_locations() {
        let home = paths::home();
        for p in [
            PathBuf::from("/"),
            PathBuf::from("/System/Library/Kernels"),
            PathBuf::from("/System/Volumes/Data/Users"),
            PathBuf::from("/usr/bin/true"),
            PathBuf::from("/Applications"),
            home.clone(),
            home.join("Documents"),
            home.join("Library/Caches"),
            home.join("Library/Keychains/login.keychain-db"),
            PathBuf::from("relative/path"),
            home.join("Library/Caches/../Preferences/x"),
        ] {
            assert!(guard(&p).is_err(), "{} should be protected", p.display());
        }
    }

    #[test]
    fn guard_allows_content() {
        let home = paths::home();
        for p in [
            home.join("Library/Caches/com.example.app"),
            home.join("Downloads/installer.dmg"),
            home.join(".npm"),
            PathBuf::from("/Applications/Example.app"),
            PathBuf::from("/System/Volumes/Data/Users/someone/Movies/big.mov"),
        ] {
            assert!(guard(&p).is_ok(), "{} should be allowed", p.display());
        }
    }

    /// Touches the real Trash, so it only runs on demand:
    /// `cargo test -p poof-core -- --ignored moves_items_to_the_trash`
    #[test]
    #[ignore = "moves a temporary file into the user's real Trash"]
    fn moves_items_to_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("poof-trash-selftest.txt");
        fs::write(&file, b"poof").unwrap();
        let outcome = remove(&[Item { path: file.clone(), size: 4 }], Method::Trash);
        assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
        assert!(!file.exists());
    }

    #[test]
    fn deletes_files_and_directories_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.bin");
        let folder = dir.path().join("folder");
        fs::write(&file, b"hello").unwrap();
        fs::create_dir_all(folder.join("nested")).unwrap();
        fs::write(folder.join("nested/b.bin"), b"x").unwrap();

        let items = [
            Item { path: file.clone(), size: 5 },
            Item { path: folder.clone(), size: 1 },
            Item { path: dir.path().join("missing"), size: 9 },
        ];
        let outcome = remove(&items, Method::Delete);
        assert_eq!(outcome.bytes, 6);
        assert_eq!(outcome.removed.len(), 2);
        assert_eq!(outcome.failed.len(), 1);
        assert!(!file.exists() && !folder.exists());
    }
}
