//! The usual suspects behind "my disk filled up and I don't know why".
//!
//! macOS hides several large, fast-growing stores under "System Data". These
//! are measured directly (in seconds, without a full scan) and paired with
//! advice, because many of them should be managed from the owning app rather
//! than deleted from the file system.

use std::path::PathBuf;

use rayon::prelude::*;

use crate::{paths, scan};

#[derive(Debug)]
pub struct Suspect {
    pub id: &'static str,
    pub name: &'static str,
    /// What it is, in plain words.
    pub description: &'static str,
    /// How to reclaim the space safely.
    pub advice: &'static str,
    /// Locations, with `~/` and `*` wildcards (see [`paths::glob`]).
    pub paths: &'static [&'static str],
}

pub static SUSPECTS: &[Suspect] = &[
    Suspect {
        id: "swap",
        name: "Virtual memory & sleep image",
        description: "Swap files grow when apps need more memory than you have; the sleep image matches your RAM.",
        advice: "Swap shrinks after a restart. Quitting memory-hungry apps keeps it small.",
        paths: &["/private/var/vm"],
    },
    Suspect {
        id: "simulators",
        name: "Simulators & runtimes",
        description: "iOS, watchOS and visionOS simulator devices and runtime images installed by Xcode.",
        advice: "Remove unused runtimes in Xcode → Settings → Components, and run `xcrun simctl delete unavailable`.",
        paths: &["~/Library/Developer/CoreSimulator/Devices", "/Library/Developer/CoreSimulator/Images"],
    },
    Suspect {
        id: "docker",
        name: "Docker disk image",
        description: "A single virtual disk holding every Docker image, container and volume. It grows but never shrinks on its own.",
        advice: "Run `docker system prune -a`, then lower the disk limit in Docker Desktop → Settings → Resources.",
        paths: &[
            "~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
            "~/.orbstack/data",
            "~/.colima",
        ],
    },
    Suspect {
        id: "virtual-machines",
        name: "Virtual machines",
        description: "Disk images for Parallels, UTM, VMware Fusion and Linux VMs.",
        advice: "Delete VMs you no longer use from within the VM app so its settings stay consistent.",
        paths: &[
            "~/Parallels",
            "~/Library/Containers/com.utmapp.UTM/Data/Documents",
            "~/Virtual Machines.localized",
        ],
    },
    Suspect {
        id: "photos",
        name: "Photos library",
        description: "Originals, thumbnails and edits for every photo and video in Photos.",
        advice: "Turn on Photos → Settings → iCloud → Optimize Mac Storage, and empty Recently Deleted.",
        paths: &["~/Pictures/*.photoslibrary"],
    },
    Suspect {
        id: "icloud",
        name: "iCloud Drive (downloaded)",
        description: "Local copies of files in iCloud Drive, including Desktop & Documents sync.",
        advice: "In Finder, right-click large items and choose Remove Download; they stay in iCloud.",
        paths: &["~/Library/Mobile Documents"],
    },
    Suspect {
        id: "messages",
        name: "Messages attachments",
        description: "Every photo, video and file ever sent or received in Messages.",
        advice: "System Settings → General → Storage → Messages lets you review and delete large attachments.",
        paths: &["~/Library/Messages"],
    },
    Suspect {
        id: "mail",
        name: "Mail",
        description: "Downloaded mailboxes and attachments for every Mail account.",
        advice: "Remove unused accounts, or stop downloading attachments in Mail → Settings → Accounts.",
        paths: &["~/Library/Mail"],
    },
    Suspect {
        id: "device-backups",
        name: "iPhone & iPad backups",
        description: "Full local backups created by Finder; each can be tens of gigabytes.",
        advice: "Delete old backups from Clean Up, or in Finder → your device → Manage Backups.",
        paths: &["~/Library/Application Support/MobileSync/Backup"],
    },
    Suspect {
        id: "macos-installers",
        name: "macOS installers & updates",
        description: "Downloaded macOS installers and staged software updates.",
        advice: "Installer apps can be deleted once you've upgraded. Staged updates clear after installing.",
        paths: &["/Applications/Install macOS*.app", "/macOS Install Data", "/Library/Updates"],
    },
    Suspect {
        id: "containers",
        name: "App containers",
        description: "Where sandboxed apps keep their data, caches and downloads.",
        advice: "Open Explore on ~/Library/Containers to see which app is responsible.",
        paths: &["~/Library/Containers", "~/Library/Group Containers"],
    },
    Suspect {
        id: "app-support",
        name: "Application Support",
        description: "Data stored by non-sandboxed apps: games, IDEs, Chromium-based apps, AI models.",
        advice: "Open Explore on ~/Library/Application Support to find the largest app.",
        paths: &["~/Library/Application Support"],
    },
];

/// A suspect together with how much space it currently uses.
#[derive(Debug, Clone)]
pub struct Measured {
    pub suspect: &'static Suspect,
    pub size: u64,
    /// The existing locations that were measured.
    pub locations: Vec<PathBuf>,
}

/// Measures every suspect in parallel, returning those that use space,
/// largest first. Blocking.
pub fn measure() -> Vec<Measured> {
    let mut measured: Vec<Measured> = SUSPECTS
        .par_iter()
        .map(|suspect| {
            let locations: Vec<PathBuf> =
                suspect.paths.iter().flat_map(|p| paths::glob(&paths::expand(p))).collect();
            let size = locations.par_iter().map(|l| scan::measure(l)).sum();
            Measured { suspect, size, locations }
        })
        .filter(|m| m.size > 0)
        .collect();
    measured.sort_by_key(|m| std::cmp::Reverse(m.size));
    measured
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = SUSPECTS.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SUSPECTS.len());
    }

    #[test]
    fn measures_without_panicking() {
        // Results depend on the machine; the call itself must be robust.
        for m in measure() {
            assert!(!m.locations.is_empty());
        }
    }
}
