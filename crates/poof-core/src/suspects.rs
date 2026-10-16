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
        description: "Swap files grow when apps need more memory than your Mac has. The sleep image is the same size as your RAM.",
        advice: "Restarting clears swap. Quitting apps that use a lot of memory keeps it small.",
        paths: &["/private/var/vm"],
    },
    Suspect {
        id: "simulators",
        name: "Simulators & runtimes",
        description: "iOS, watchOS and visionOS simulator devices and runtime images installed by Xcode.",
        advice: "Remove runtimes you don't use in Xcode → Settings → Components, then run xcrun simctl delete unavailable in Terminal.",
        paths: &["~/Library/Developer/CoreSimulator/Devices", "/Library/Developer/CoreSimulator/Images"],
    },
    Suspect {
        id: "docker",
        name: "Docker disk image",
        description: "One virtual disk that holds all your Docker images, containers and volumes. It grows as you use Docker and doesn't shrink by itself.",
        advice: "Run docker system prune -a in Terminal, then lower the disk limit in Docker Desktop → Settings → Resources.",
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
        advice: "Delete old VMs from inside Parallels, UTM or VMware rather than in Finder.",
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
        description: "iCloud Drive files downloaded to this Mac, including synced Desktop and Documents folders.",
        advice: "In Finder, right-click a large item and choose Remove Download. It stays in iCloud.",
        paths: &["~/Library/Mobile Documents"],
    },
    Suspect {
        id: "messages",
        name: "Messages attachments",
        description: "Photos, videos and files you've sent or received in Messages.",
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
        description: "Backups of your iPhone or iPad made through Finder. Each one can take tens of gigabytes.",
        advice: "Delete old backups from Clean Up, or in Finder → your device → Manage Backups.",
        paths: &["~/Library/Application Support/MobileSync/Backup"],
    },
    Suspect {
        id: "macos-installers",
        name: "macOS installers & updates",
        description: "Downloaded macOS installers and staged software updates.",
        advice: "You can delete installer apps once you've upgraded. Downloaded updates are removed after they install.",
        paths: &["/Applications/Install macOS*.app", "/macOS Install Data", "/Library/Updates"],
    },
    Suspect {
        id: "containers",
        name: "App containers",
        description: "Sandboxed apps, including most App Store apps, keep their data, caches and downloads here.",
        advice: "Use the folder button to see which app is using the space.",
        paths: &["~/Library/Containers", "~/Library/Group Containers"],
    },
    Suspect {
        id: "app-support",
        name: "Application Support",
        description: "Data from apps outside the App Store, like games, code editors, Chrome-based apps and AI tools.",
        advice: "Use the folder button to find the app using the most space.",
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
            assert_ne!(m.locations, Vec::<PathBuf>::new());
        }
    }
}
