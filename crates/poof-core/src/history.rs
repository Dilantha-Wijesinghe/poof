//! Scan history: "what grew since last time?"
//!
//! After every scan a compact [`Summary`] (sizes of everything at least
//! [`THRESHOLD`] bytes, a few levels deep) is saved. Comparing two summaries
//! with [`changes`] pinpoints where space went, attributing growth to the
//! deepest folder that explains it so totals are never double-counted.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::scan::{NodeKind, Tree};
use crate::store::{read_json, write_json};
use crate::{Result, paths};

/// Smallest item recorded in a summary. Bounds the error of a diff: an item
/// that was just below the threshold last time can be over-reported by at
/// most this much.
pub const THRESHOLD: u64 = 32 * 1_000_000;

/// How many levels below the scan root are recorded.
pub const MAX_DEPTH: usize = 8;

/// Summaries kept per scan root.
const KEEP: usize = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub root: PathBuf,
    pub taken_at: i64,
    pub total: u64,
    /// Path relative to `root` (`""` is the root itself) → size in bytes.
    pub entries: BTreeMap<String, u64>,
}

impl Summary {
    pub fn from_tree(tree: &Tree) -> Self {
        let mut entries = BTreeMap::new();
        tree.visit_dirs(|id, depth| {
            let node = tree.get(id);
            if node.size < THRESHOLD || depth > MAX_DEPTH {
                return false;
            }
            entries.insert(rel_key(tree, id), node.size);
            // Large single files matter too: a VM image that keeps growing.
            for (child, file) in tree.children(id) {
                if file.kind == NodeKind::File && file.size >= THRESHOLD {
                    entries.insert(rel_key(tree, child), file.size);
                }
            }
            true
        });
        Self {
            root: tree.root_path().to_path_buf(),
            taken_at: tree.scanned_at,
            total: tree.get(tree.root()).size,
            entries,
        }
    }

    /// Size recorded for a relative path, if it was above the threshold.
    pub fn size_of(&self, relative: &str) -> Option<u64> {
        self.entries.get(relative).copied()
    }
}

fn rel_key(tree: &Tree, id: crate::scan::NodeId) -> String {
    tree.relative_path(id).to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

fn root_dir(root: &Path) -> PathBuf {
    let key: String =
        root.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    paths::data_dir().join("history").join(key)
}

/// Saves a summary and prunes old ones for the same root.
pub fn save(summary: &Summary) -> Result<()> {
    save_in(&root_dir(&summary.root), summary)
}

/// All saved summary timestamps for `root`, newest first.
pub fn list(root: &Path) -> Vec<i64> {
    list_in(&root_dir(root))
}

pub fn load(root: &Path, taken_at: i64) -> Result<Option<Summary>> {
    read_json(&root_dir(root).join(format!("{taken_at}.json")))
}

/// The newest summary for `root` taken strictly before `before`.
pub fn previous(root: &Path, before: i64) -> Option<Summary> {
    let at = list(root).into_iter().find(|&t| t < before)?;
    load(root, at).ok().flatten()
}

/// The most recent summary for `root`.
pub fn latest(root: &Path) -> Option<Summary> {
    previous(root, i64::MAX)
}

fn save_in(dir: &Path, summary: &Summary) -> Result<()> {
    write_json(&dir.join(format!("{}.json", summary.taken_at)), summary)?;
    for old in list_in(dir).into_iter().skip(KEEP) {
        let _ = fs::remove_file(dir.join(format!("{old}.json")));
    }
    Ok(())
}

fn list_in(dir: &Path) -> Vec<i64> {
    let mut stamps: Vec<i64> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json")?.parse().ok())
        .collect();
    stamps.sort_unstable_by(|a, b| b.cmp(a));
    stamps
}

// ---------------------------------------------------------------------------
// Diffing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Relative to the scan root; `""` is the root.
    pub path: String,
    pub before: u64,
    pub after: u64,
    /// The part of the change attributed to this item itself, after
    /// subtracting what its listed descendants already explain.
    pub attributed: i64,
    /// Part of the change is spread across many small items inside this folder
    /// rather than one listed descendant.
    pub spread: bool,
}

/// Where space grew (positive `attributed`) or shrank (negative) between two
/// summaries, ignoring changes smaller than `min_delta`. Biggest first.
pub fn changes(old: &Summary, new: &Summary, min_delta: u64) -> Vec<Change> {
    let mut deltas: Vec<(&str, u64, u64)> = old
        .entries
        .keys()
        .chain(new.entries.keys())
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|p| (p, old.size_of(p).unwrap_or(0), new.size_of(p).unwrap_or(0)))
        .collect();
    // Deepest first, so children are attributed before their parents.
    deltas.sort_by_key(|(p, _, _)| std::cmp::Reverse(depth(p)));

    let mut out = attribute(&deltas, min_delta, 1);
    out.extend(attribute(&deltas, min_delta, -1));
    out.sort_by_key(|c| std::cmp::Reverse(c.attributed.unsigned_abs()));
    out
}

/// One pass of attribution for growth (`sign = 1`) or shrinkage (`sign = -1`).
fn attribute(deltas: &[(&str, u64, u64)], min_delta: u64, sign: i64) -> Vec<Change> {
    let min = i64::try_from(min_delta).unwrap_or(i64::MAX);
    // Already-explained changes that no processed ancestor has absorbed yet.
    let mut frontier: Vec<(&str, i64)> = Vec::new();
    let mut out = Vec::new();
    for &(path, before, after) in deltas {
        let delta = (after as i64 - before as i64) * sign;
        if delta < min {
            continue;
        }
        let mut explained = 0;
        frontier.retain(|&(p, d)| {
            let inside = is_descendant(p, path);
            if inside {
                explained += d;
            }
            !inside
        });
        frontier.push((path, delta));
        let remainder = delta - explained;
        if remainder >= min {
            out.push(Change {
                path: path.to_owned(),
                before,
                after,
                attributed: remainder * sign,
                spread: explained > 0,
            });
        }
    }
    out
}

fn depth(path: &str) -> usize {
    if path.is_empty() { 0 } else { path.matches('/').count() + 1 }
}

fn is_descendant(child: &str, ancestor: &str) -> bool {
    if ancestor.is_empty() {
        return !child.is_empty();
    }
    child.len() > ancestor.len() && child.starts_with(ancestor) && child.as_bytes()[ancestor.len()] == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::size::gb;

    fn summary(at: i64, entries: &[(&str, u64)]) -> Summary {
        Summary {
            root: PathBuf::from("/Users/me"),
            taken_at: at,
            total: entries.iter().find(|(p, _)| p.is_empty()).map_or(0, |e| e.1),
            entries: entries.iter().map(|&(p, s)| (p.to_owned(), s)).collect(),
        }
    }

    #[test]
    fn growth_is_attributed_to_the_deepest_explanation() {
        let old =
            summary(1, &[("", gb(100)), ("Library", gb(60)), ("Library/Caches", gb(5)), ("Movies", gb(20))]);
        let new = summary(
            2,
            &[
                ("", gb(130)),
                ("Library", gb(85)),
                ("Library/Caches", gb(25)),
                ("Movies", gb(20)),
                ("Downloads", gb(2)),
            ],
        );
        let changes = changes(&old, &new, gb(1));

        // Caches explains 20 of Library's 25; Library still has 5 unexplained.
        assert_eq!(changes[0].path, "Library/Caches");
        assert_eq!(changes[0].attributed, gb(20) as i64);
        let library = changes.iter().find(|c| c.path == "Library").unwrap();
        assert_eq!(library.attributed, gb(5) as i64);
        assert!(library.spread);
        // Downloads is new; root growth is fully explained by children (20+5+2=27 < 30 → 3 left).
        assert!(changes.iter().any(|c| c.path == "Downloads"));
        let root = changes.iter().find(|c| c.path.is_empty()).unwrap();
        assert_eq!(root.attributed, gb(3) as i64);
        // Attributions add up to the real total growth.
        let sum: i64 = changes.iter().map(|c| c.attributed).sum();
        assert_eq!(sum, gb(30) as i64);
    }

    #[test]
    fn shrinkage_is_reported_negatively() {
        let old = summary(1, &[("", gb(50)), ("Movies", gb(30))]);
        let new = summary(2, &[("", gb(20)), ("Movies", gb(0))]);
        let changes = changes(&old, &new, gb(1));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "Movies");
        assert_eq!(changes[0].attributed, -(gb(30) as i64));
    }

    #[test]
    fn small_changes_are_ignored() {
        let old = summary(1, &[("", gb(10))]);
        let new = summary(2, &[("", gb(10) + 5)]);
        assert_eq!(changes(&old, &new, gb(1)), Vec::<Change>::new());
    }

    #[test]
    fn descendant_matching_respects_segment_boundaries() {
        assert!(is_descendant("Library/Caches", "Library"));
        assert!(!is_descendant("LibraryX", "Library"));
        assert!(is_descendant("Library", ""));
        assert!(!is_descendant("", ""));
        assert_eq!(depth(""), 0);
        assert_eq!(depth("a/b"), 2);
    }

    #[test]
    fn persistence_keeps_the_newest_summaries() {
        let dir = tempfile::tempdir().unwrap();
        for at in 0..(KEEP as i64 + 5) {
            save_in(dir.path(), &summary(at, &[("", 1)])).unwrap();
        }
        let stamps = list_in(dir.path());
        assert_eq!(stamps.len(), KEEP);
        assert_eq!(stamps[0], KEEP as i64 + 4);
    }

    #[test]
    fn summaries_capture_large_items_from_a_tree() {
        let tree = crate::scan::tree_for_tests(&[("big/huge.vm", 2 * THRESHOLD), ("tiny.txt", 10)]);
        let s = Summary::from_tree(&tree);
        assert_eq!(s.size_of(""), Some(2 * THRESHOLD + 10));
        assert_eq!(s.size_of("big"), Some(2 * THRESHOLD));
        assert_eq!(s.size_of("big/huge.vm"), Some(2 * THRESHOLD));
        assert_eq!(s.size_of("tiny.txt"), None);
    }
}
