//! Data left behind by apps that are no longer installed.
//!
//! Only locations named after a **bundle identifier** are considered
//! (`~/Library/Containers/com.vendor.app`, `….savedState`, `….plist`), so
//! matching is exact rather than guesswork on folder names. An item counts as
//! a leftover only if *no* installed app shares its vendor prefix *or* its
//! product name — this deliberately misses some leftovers to avoid ever
//! flagging live app data.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::apps::normalize;
use crate::{paths, system, time};

/// Where per-app data lives, and the suffix to strip to get the bundle id.
const LOCATIONS: &[(&str, &str)] = &[
    ("~/Library/Containers", ""),
    ("~/Library/Application Support", ""),
    ("~/Library/HTTPStorages", ""),
    ("~/Library/WebKit", ""),
    ("~/Library/Saved Application State", ".savedState"),
    ("~/Library/Preferences", ".plist"),
];

/// Ignore anything touched this recently: the app may be running from a
/// disk image or be mid-reinstall.
const GRACE_DAYS: i64 = 7;

/// Below this many known apps, app discovery probably failed; report nothing.
const MIN_INSTALLED: usize = 20;

pub(super) fn leftovers() -> Vec<PathBuf> {
    let installed = installed_apps();
    if installed.len() < MIN_INSTALLED {
        return Vec::new();
    }
    let known = Known::new(&installed);
    let cutoff = time::now() - GRACE_DAYS * time::DAY;

    LOCATIONS
        .iter()
        .flat_map(|(dir, suffix)| {
            fs::read_dir(paths::expand(dir)).into_iter().flatten().flatten().filter_map(move |entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let id = name.strip_suffix(suffix)?.to_owned();
                Some((entry.path(), id))
            })
        })
        .filter(|(_, id)| known.is_orphan(id))
        .filter(|(path, _)| modified(path) < cutoff)
        .map(|(path, _)| path)
        .collect()
}

/// What is installed, reduced to the two keys used for matching.
struct Known {
    vendors: HashSet<String>,
    products: HashSet<String>,
}

impl Known {
    /// `apps` are `(bundle path, bundle id)` pairs.
    fn new(apps: &[(String, String)]) -> Self {
        let vendors = apps.iter().filter_map(|(_, id)| vendor(id)).collect();
        let products = apps
            .iter()
            .flat_map(|(path, id)| {
                let stem = Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned());
                [id.rsplit('.').next().map(str::to_owned), stem]
            })
            .flatten()
            .map(|p| normalize(&p))
            .collect();
        Self { vendors, products }
    }

    /// Whether `id` looks like a third-party bundle identifier that no
    /// installed app could own.
    fn is_orphan(&self, id: &str) -> bool {
        let product = id.rsplit('.').next().map(normalize).unwrap_or_default();
        looks_like_bundle_id(id)
            && !id.starts_with("com.apple.")
            && !id.starts_with("group.")
            && vendor(id).is_some_and(|v| !self.vendors.contains(&v))
            && !self.products.contains(&product)
    }
}

/// `com.Vendor.App.helper` → `com.vendor`.
fn vendor(id: &str) -> Option<String> {
    let mut parts = id.split('.');
    let (tld, name) = (parts.next()?, parts.next()?);
    (!tld.is_empty() && !name.is_empty()).then(|| format!("{tld}.{name}").to_ascii_lowercase())
}

/// Reverse-DNS with at least three components and a TLD-like first part.
fn looks_like_bundle_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 3
        && (2..=6).contains(&parts[0].len())
        && parts[0].chars().all(|c| c.is_ascii_lowercase())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
}

fn modified(path: &Path) -> i64 {
    fs::symlink_metadata(path).and_then(|m| m.modified()).map_or(i64::MAX, time::to_unix)
}

/// `(path, bundle id)` of every app on the system, via Spotlight.
fn installed_apps() -> Vec<(String, String)> {
    let query = "kMDItemContentType == 'com.apple.application-bundle'";
    system::run("mdfind", &["-attr", "kMDItemCFBundleIdentifier", query])
        .map(|out| parse_mdfind(&out))
        .unwrap_or_default()
}

fn parse_mdfind(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|line| line.rsplit_once("kMDItemCFBundleIdentifier = "))
        .map(|(path, id)| (path.trim().to_owned(), id.trim().trim_matches('"').to_owned()))
        .filter(|(_, id)| !id.is_empty() && id != "(null)")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(apps: &[(&str, &str)]) -> Known {
        let apps: Vec<_> = apps.iter().map(|(p, id)| ((*p).to_owned(), (*id).to_owned())).collect();
        Known::new(&apps)
    }

    #[test]
    fn only_data_no_installed_app_could_own_is_orphaned() {
        let installed = known(&[
            ("/Applications/Visual Studio Code.app", "com.microsoft.VSCode"),
            ("/Applications/Slack.app", "com.tinyspeck.slackmacgap"),
            ("/Applications/OpenUsage.app", "com.robinebers.openusage"),
        ]);
        assert!(installed.is_orphan("com.google.Chrome"));
        assert!(!installed.is_orphan("com.microsoft.teams2"), "same vendor as an installed app");
        assert!(!installed.is_orphan("com.tinyspeck.slackmacgap.helper"));
        assert!(!installed.is_orphan("com.sunstory.openusage"), "same product, different vendor");
        assert!(!installed.is_orphan("com.apple.Safari"), "system data is never flagged");
        assert!(!installed.is_orphan("group.com.google.drive"));
        assert!(!installed.is_orphan("Google"), "plain folder names are ambiguous");
        assert!(!installed.is_orphan("CrashReporter"));
    }

    #[test]
    fn recognises_bundle_ids() {
        assert!(looks_like_bundle_id("com.docker.docker"));
        assert!(looks_like_bundle_id("io.github.some-app.Helper"));
        assert!(!looks_like_bundle_id("Application Support"));
        assert!(!looks_like_bundle_id("com.example"));
        assert!(!looks_like_bundle_id("UBF8T346G9.Office.thing"));
    }

    #[test]
    fn parses_spotlight_output() {
        let out = "/Applications/Safari.app   kMDItemCFBundleIdentifier = com.apple.Safari\n\
                   /Applications/Broken.app   kMDItemCFBundleIdentifier = (null)\n";
        assert_eq!(
            parse_mdfind(out),
            [("/Applications/Safari.app".to_owned(), "com.apple.Safari".to_owned())]
        );
    }
}
