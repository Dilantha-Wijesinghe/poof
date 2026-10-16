# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- An Empty Trash button on the message that follows moving items to the Trash,
  from Explore or Clean Up.

### Changed

- Clean Up leaves out the data of apps that are open, such as a running
  browser's cache, and says so before cleaning. Quit the app and clean again
  to include it.

### Fixed

- The update dialog now shows release notes that wrap over several lines in
  full, instead of cutting them off.
- The sidebar's Update button now centres its label.

## [0.1.1] - 2026-10-16

### Added

- After updating itself, Poof confirms the new version when it reopens.

### Fixed

- Builds now lint cleanly on Rust 1.99, and the toolchain is pinned so local
  builds and CI always agree.

## [0.1.0] - 2026-10-16

### Added

- **Overview**:
  - disk capacity with used, purgeable and free space explained in plain words
  - a free-space trend
  - Time Machine local snapshots, with one-click thinning
  - "usual suspects" behind System Data (Docker, simulators, swap, iCloud,
    Messages, backups, installers, app containers), measured without a full
    scan
- **Explore**:
  - a parallel scanner for your home folder, the whole data volume or any
    folder
  - an interactive two-level treemap with a sorted list and breadcrumbs
  - growth badges compared with the previous scan
  - move items to the Trash, or reveal them in Finder
- **Changes**:
  - a free-space timeline
  - a diff of what grew or shrank between any two saved scans, with growth
    attributed to the deepest folder that explains it
- **Clean Up**, with a reviewed catalog of 21 rules:
  - caches, logs and the Trash
  - Xcode data
  - JavaScript, Rust, Python and other package caches
  - build folders in inactive projects
  - leftovers of removed apps
  - installers, iOS backups and Mail downloads

  Only Safe items are preselected.
- **Background monitoring**: an optional LaunchAgent that notifies you about
  sudden free-space drops or low space.
- **Safety**:
  - a protected-path guard
  - Trash-only removal in Explore
  - an operations log at `~/Library/Logs/Poof/operations.log`
- **Updates**: Poof checks GitHub once a day and installs new versions in one
  click. Updates are verified with a minisign signature and swapped in place
  with rollback. Releases are signed with a stable certificate so Full Disk
  Access survives updates.
- Light and dark themes that follow the system, with a Light/Dark switch in
  the sidebar.
- A universal (Apple silicon + Intel) `.app`, `.dmg` and `.zip` via
  `scripts/bundle.sh` and GitHub Actions.

[Unreleased]: https://github.com/Dilantha-Wijesinghe/poof/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/Dilantha-Wijesinghe/poof/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Dilantha-Wijesinghe/poof/releases/tag/v0.1.0
