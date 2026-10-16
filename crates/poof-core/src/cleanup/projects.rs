//! Finds rebuildable build artifacts (`node_modules`, `target`, …) inside
//! projects that haven't been touched for a while.
//!
//! An artifact only counts when its project marker sits next to it, e.g.
//! `target/` next to `Cargo.toml`. This avoids flagging folders that merely
//! happen to share a name.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{paths, time};

/// `(artifact folder, marker that must exist beside it)`.
const ARTIFACTS: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".build", &["Package.swift"]),
    ("Pods", &["Podfile"]),
    (".gradle", &["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"]),
    (".next", &["package.json"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
];

/// Folders commonly used to keep source code.
const PROJECT_ROOTS: &[&str] = &[
    "~/Developer",
    "~/Projects",
    "~/Code",
    "~/src",
    "~/dev",
    "~/Work",
    "~/workspace",
    "~/repos",
    "~/git",
    "~/GitHub",
    "~/Sites",
    "~/Documents",
    "~/Desktop",
];

const MAX_DEPTH: usize = 6;

pub(super) fn stale_artifacts(stale_days: u32) -> Vec<PathBuf> {
    let cutoff = time::now() - i64::from(stale_days) * time::DAY;
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in PROJECT_ROOTS.iter().map(|r| paths::expand(r)) {
        // The file system is case-insensitive: ~/Code and ~/code are one folder.
        if let Ok(canonical) = root.canonicalize()
            && seen.insert(canonical.clone())
        {
            search(&canonical, 0, cutoff, &mut found);
        }
    }
    found
}

fn search(dir: &Path, depth: usize, cutoff: i64, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let children: Vec<_> = entries.flatten().collect();
    let has = |name: &str| children.iter().any(|e| e.file_name() == name);

    for entry in &children {
        let Ok(kind) = entry.file_type() else { continue };
        if !kind.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some((_, markers)) = ARTIFACTS.iter().find(|(artifact, _)| *artifact == name) {
            if markers.iter().any(|m| has(m)) && last_activity(dir, &name) < cutoff {
                found.push(entry.path());
            }
            continue; // never descend into an artifact
        }
        if !name.starts_with('.') && name != "Library" {
            search(&entry.path(), depth + 1, cutoff, found);
        }
    }
}

/// The most recent modification among the project's own top-level entries,
/// ignoring the artifact folder itself (which installs touch constantly).
fn last_activity(project: &Path, artifact: &str) -> i64 {
    fs::read_dir(project)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name() != artifact)
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .map(time::to_unix)
        .max()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_marked_artifacts_only() {
        let dir = tempfile::tempdir().unwrap();
        let rust = dir.path().join("rust-app");
        fs::create_dir_all(rust.join("target/debug")).unwrap();
        fs::write(rust.join("Cargo.toml"), "").unwrap();
        let fake = dir.path().join("photos");
        fs::create_dir_all(fake.join("target")).unwrap();

        // A cutoff in the future makes every project "stale".
        let mut found = Vec::new();
        search(dir.path(), 0, i64::MAX, &mut found);
        assert_eq!(found, vec![rust.join("target")]);

        // A cutoff in the past makes every project "active".
        let mut found = Vec::new();
        search(dir.path(), 0, 0, &mut found);
        assert_eq!(found, Vec::<PathBuf>::new());
    }
}
