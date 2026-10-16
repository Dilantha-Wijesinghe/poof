//! Apps on this Mac: which are open, and which app a folder of data belongs to.
//!
//! Removing an app's caches while it runs can confuse it, so cleanup leaves
//! the data of open apps alone. Ownership is judged from folder names, which
//! apps base on their bundle identifier or their name.

use std::path::{Path, PathBuf};

use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};

use crate::paths;

/// Where apps keep per-app data, in folders named after themselves.
const DATA_ROOTS: &[&str] = &[
    "~/Library/Caches",
    "~/Library/Application Support",
    "~/Library/Containers",
    "~/Library/HTTPStorages",
    "~/Library/WebKit",
    "~/Library/Developer",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// The name shown in the Dock, e.g. "Google Chrome".
    pub name: String,
    pub id: String,
    /// The `.app` bundle.
    pub path: PathBuf,
}

impl App {
    /// Whether `folder` is named after this app's bundle identifier
    /// (`com.apple.Safari`, `com.apple.Safari.SafeBrowsing`).
    fn has_id(&self, folder: &str) -> bool {
        let (folder, id) = (folder.to_ascii_lowercase(), self.id.to_ascii_lowercase());
        folder.strip_prefix(&id).is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    }

    /// Whether `name` (normalized) is what this app is called.
    fn is_called(&self, name: &str) -> bool {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
        !name.is_empty() && (name == normalize(&self.name) || name == normalize(&stem))
    }
}

/// Apps with a Dock icon that are open now, except Poof itself. Background
/// helpers and daemons are left out: the user can't quit them.
pub fn running() -> Vec<App> {
    let me = std::process::id().cast_signed();
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .iter()
        .filter(|a| a.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter(|a| a.processIdentifier() != me && !a.isTerminated())
        .filter_map(|a| {
            Some(App {
                name: a.localizedName()?.to_string(),
                id: a.bundleIdentifier()?.to_string(),
                path: a.bundleURL()?.to_file_path()?,
            })
        })
        .collect()
}

/// The app in `apps` that `path` belongs to, judged by the first two folders
/// below a per-app data root: `~/Library/Caches/com.tinyspeck.slackmacgap`,
/// `~/Library/Application Support/Slack/Cache` and
/// `~/Library/Caches/Google/Chrome` all match by bundle id or name.
pub fn owner<'a>(path: &Path, apps: &'a [App]) -> Option<&'a App> {
    let rest = DATA_ROOTS.iter().find_map(|root| path.strip_prefix(paths::expand(root)).ok())?;
    let mut folders = rest.iter().map(|f| f.to_string_lossy());
    let first = folders.next()?;
    let second = folders.next().unwrap_or_default();
    let names = [normalize(&first), normalize(&second), normalize(&format!("{first}{second}"))];
    apps.iter().find(|app| app.has_id(&first) || names.iter().any(|n| app.is_called(n)))
}

/// `Draw-Things`, `draw things` → `drawthings`.
pub(crate) fn normalize(name: &str) -> String {
    name.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, id: &str, bundle: &str) -> App {
        App { name: name.into(), id: id.into(), path: PathBuf::from(format!("/Applications/{bundle}.app")) }
    }

    fn owner_of(path: &str, apps: &[App]) -> Option<String> {
        owner(&paths::expand(path), apps).map(|a| a.name.clone())
    }

    #[test]
    fn matches_data_folders_by_bundle_id_or_name() {
        let apps = [
            app("Slack", "com.tinyspeck.slackmacgap", "Slack"),
            app("Google Chrome", "com.google.Chrome", "Google Chrome"),
            app("Code", "com.microsoft.VSCode", "Visual Studio Code"),
            app("Mail", "com.apple.mail", "Mail"),
        ];
        let slack = Some("Slack".to_owned());
        assert_eq!(owner_of("~/Library/Caches/com.tinyspeck.slackmacgap", &apps), slack);
        assert_eq!(owner_of("~/Library/Caches/com.tinyspeck.slackmacgap.ShipIt", &apps), slack);
        assert_eq!(owner_of("~/Library/Application Support/Slack/Cache", &apps), slack);
        assert_eq!(owner_of("~/Library/Caches/Google/Chrome", &apps).as_deref(), Some("Google Chrome"));
        assert_eq!(
            owner_of(
                "~/Library/Application Support/Google/Chrome/Default/Service Worker/CacheStorage",
                &apps
            )
            .as_deref(),
            Some("Google Chrome")
        );
        assert_eq!(owner_of("~/Library/Application Support/Code/CachedData", &apps).as_deref(), Some("Code"));
        assert_eq!(
            owner_of("~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads/x", &apps).as_deref(),
            Some("Mail")
        );
    }

    #[test]
    fn ignores_other_apps_and_places() {
        let apps = [
            app("Code", "com.microsoft.VSCode", "Visual Studio Code"),
            app("Mail", "com.apple.mail", "Mail"),
        ];
        assert_eq!(owner_of("~/Library/Caches/com.apple.mailcore", &apps), None, "id prefix only on a dot");
        assert_eq!(owner_of("~/Library/Caches/Google/Drive", &apps), None);
        assert_eq!(owner_of("~/Developer/Code/target", &apps), None, "only per-app data roots count");
        assert_eq!(owner_of("~/Library/Caches", &apps), None);
    }
}
