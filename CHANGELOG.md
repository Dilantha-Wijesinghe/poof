# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- Light and dark themes that follow the system.
- A universal (Apple silicon + Intel) `.app`, `.dmg` and `.zip` via
  `scripts/bundle.sh` and GitHub Actions.

[Unreleased]: https://github.com/Dilantha-Wijesinghe/poof/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Dilantha-Wijesinghe/poof/releases/tag/v0.1.0
