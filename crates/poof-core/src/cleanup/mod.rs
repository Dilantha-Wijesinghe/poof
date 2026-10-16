//! Finding space that can be reclaimed safely.
//!
//! A [`Rule`] declares *where* reclaimable data lives and *how safe* it is to
//! remove. [`find`] evaluates every rule in parallel and returns concrete,
//! measured [`Candidate`]s. Removal itself is delegated to
//! [`crate::remove`], so cleanup and manual deletion share one guarded path.

mod catalog;
mod leftovers;
mod projects;

pub use catalog::CATALOG;

use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::{paths, scan, time};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    System,
    Apps,
    Developer,
    Files,
}

impl Category {
    pub const ALL: [Category; 4] = [Category::System, Category::Apps, Category::Developer, Category::Files];

    pub fn label(self) -> &'static str {
        match self {
            Category::System => "System junk",
            Category::Apps => "App data",
            Category::Developer => "Developer",
            Category::Files => "Your files",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    /// Regenerated automatically; nothing of value is lost.
    Safe,
    /// Probably unneeded, but the user should look before removing.
    Review,
}

/// Where a rule's reclaimable items live.
#[derive(Debug, Clone, Copy)]
pub enum Source {
    /// Every entry inside these directories is an item (the directory stays).
    Contents(&'static [&'static str]),
    /// Each path (after wildcard expansion) is one item.
    Paths(&'static [&'static str]),
    /// Files directly inside `dirs` whose extension is in `extensions`.
    Files { dirs: &'static [&'static str], extensions: &'static [&'static str] },
    /// Build artifacts in projects that haven't been touched recently.
    StaleProjects,
    /// Data of apps that are no longer installed.
    Leftovers,
}

#[derive(Debug)]
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: Category,
    pub safety: Safety,
    pub source: Source,
    /// Always delete permanently — moving the Trash's contents to the Trash
    /// would be meaningless.
    pub permanent: bool,
}

/// One removable item found by a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub size: u64,
    pub modified: i64,
}

/// The evaluation of a single rule.
#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: &'static Rule,
    /// Items, largest first.
    pub items: Vec<Candidate>,
    /// A source directory exists but could not be read (usually Full Disk Access).
    pub unreadable: bool,
}

impl Finding {
    pub fn total(&self) -> u64 {
        self.items.iter().map(|c| c.size).sum()
    }
}

/// Context that influences which items qualify.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Project build folders count as stale after this many days.
    pub stale_project_days: u32,
}

/// Evaluates `rules` and returns one [`Finding`] per rule that has something
/// to offer, largest first. Blocking; runs rules and measurements in parallel.
pub fn find(rules: &'static [Rule], options: Options) -> Vec<Finding> {
    // Every location some rule targets directly. A broad `Contents` rule skips
    // children that contain (or are) one of these, so a more specific rule
    // reports them instead and nothing is counted twice.
    let claimed: Vec<PathBuf> = rules.iter().flat_map(|r| roots(r.source)).collect();
    let never = [paths::data_dir(), paths::log_dir()];

    let mut findings: Vec<Finding> = rules
        .par_iter()
        .map(|rule| evaluate(rule, options, &claimed, &never))
        .filter(|f| !f.items.is_empty() || f.unreadable)
        .collect();
    findings.sort_by_key(|f| std::cmp::Reverse(f.total()));
    findings
}

fn evaluate(rule: &'static Rule, options: Options, claimed: &[PathBuf], never: &[PathBuf]) -> Finding {
    let mut unreadable = false;
    let paths: Vec<PathBuf> = match rule.source {
        Source::Contents(dirs) => dirs
            .iter()
            .map(|d| paths::expand(d))
            .flat_map(|dir| match fs::read_dir(&dir) {
                Ok(entries) => entries.flatten().map(|e| e.path()).collect(),
                Err(e) => {
                    unreadable |= e.kind() == std::io::ErrorKind::PermissionDenied;
                    Vec::new()
                }
            })
            .filter(|child| !claimed.iter().any(|c| c.starts_with(child)))
            .collect(),
        Source::Paths(_) => roots(rule.source),
        Source::Files { dirs, extensions } => dirs
            .iter()
            .flat_map(|d| fs::read_dir(paths::expand(d)).into_iter().flatten().flatten())
            .map(|e| e.path())
            .filter(|p| has_extension(p, extensions) && !p.is_symlink())
            .collect(),
        Source::StaleProjects => projects::stale_artifacts(options.stale_project_days),
        Source::Leftovers => leftovers::leftovers(),
    };

    let mut items: Vec<Candidate> = paths
        .into_par_iter()
        .filter(|p| !never.iter().any(|n| n.starts_with(p) || p.starts_with(n)))
        .filter_map(|path| {
            let meta = fs::symlink_metadata(&path).ok()?;
            let size = scan::measure(&path);
            let modified = meta.modified().map(time::to_unix).unwrap_or_default();
            (size > 0).then_some(Candidate { path, size, modified })
        })
        .collect();
    items.sort_by_key(|c| std::cmp::Reverse(c.size));
    Finding { rule, items, unreadable }
}

/// The concrete locations a source targets directly.
fn roots(source: Source) -> Vec<PathBuf> {
    match source {
        Source::Contents(dirs) | Source::Files { dirs, .. } => {
            dirs.iter().map(|d| paths::expand(d)).collect()
        }
        Source::Paths(patterns) => patterns.iter().flat_map(|p| paths::glob(&paths::expand(p))).collect(),
        Source::StaleProjects | Source::Leftovers => Vec::new(),
    }
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; bytes]).unwrap();
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    #[test]
    fn specific_rules_claim_paths_from_broad_ones() {
        let dir = tempfile::tempdir().unwrap();
        let caches = dir.path().join("Caches");
        write(&caches.join("app/blob"), 5_000);
        write(&caches.join("Homebrew/bottle.tar.gz"), 9_000);

        let broad = leak(caches.display().to_string());
        let specific = leak(caches.join("Homebrew").display().to_string());
        let rules: &'static [Rule] = Box::leak(Box::new([
            Rule {
                id: "broad",
                name: "",
                description: "",
                category: Category::System,
                safety: Safety::Safe,
                source: Source::Contents(Box::leak(Box::new([broad]))),
                permanent: false,
            },
            Rule {
                id: "specific",
                name: "",
                description: "",
                category: Category::Developer,
                safety: Safety::Safe,
                source: Source::Paths(Box::leak(Box::new([specific]))),
                permanent: false,
            },
        ]));

        let findings = find(rules, Options { stale_project_days: 30 });
        let by_id = |id| findings.iter().find(|f| f.rule.id == id).unwrap();
        assert_eq!(by_id("broad").items.len(), 1, "Homebrew is reported by the specific rule");
        assert!(by_id("broad").items[0].path.ends_with("app"));
        assert!(by_id("specific").items[0].path.ends_with("Homebrew"));
    }

    #[test]
    fn extension_filter_is_case_insensitive() {
        assert!(has_extension(Path::new("/a/Setup.DMG"), &["dmg"]));
        assert!(!has_extension(Path::new("/a/notes.txt"), &["dmg"]));
    }

    #[test]
    fn catalog_ids_are_unique_and_paths_are_home_relative() {
        let mut ids: Vec<_> = CATALOG.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for rule in CATALOG {
            for root in roots(rule.source) {
                assert!(root.starts_with(paths::home()), "{} escapes home", root.display());
            }
        }
    }
}
