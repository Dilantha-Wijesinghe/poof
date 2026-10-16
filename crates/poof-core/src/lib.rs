//! # poof-core
//!
//! The UI-agnostic heart of Poof. Everything that touches the file system or
//! macOS lives here, behind small, testable functions. The GUI crate only
//! orchestrates these and renders the results.
//!
//! | Module      | Responsibility                                               |
//! |-------------|--------------------------------------------------------------|
//! | [`scan`]    | Parallel directory walk into a compact, navigable [`scan::Tree`] |
//! | [`cleanup`] | Declarative catalog of reclaimable locations and a finder    |
//! | [`remove`]  | Guarded deletion (Trash or permanent) with an operations log |
//! | [`history`] | Scan summaries persisted over time, and diffs between them   |
//! | [`monitor`] | Free-space sampling, alerts and the background LaunchAgent   |
//! | [`volume`]  | Capacity, free and purgeable space of a volume               |
//! | [`system`]  | APFS snapshots, Full Disk Access, Finder and notifications   |
//! | [`suspects`]| Known hidden space hogs ("System Data") measured directly    |
//! | [`treemap`] | Pure squarified treemap layout                               |
//! | [`update`]  | Signed self-updates from GitHub Releases                     |

pub mod cleanup;
pub mod error;
pub mod history;
pub mod monitor;
pub mod paths;
pub mod remove;
pub mod scan;
pub mod settings;
pub mod size;
mod store;
pub mod suspects;
pub mod system;
pub mod time;
pub mod treemap;
pub mod update;
pub mod volume;

pub use error::{Error, Result};

/// The reverse-DNS identifier shared by the app bundle, its `LaunchAgent` and
/// its data directories.
pub const APP_ID: &str = "io.github.dilantha-wijesinghe.poof";

/// Human-facing product name.
pub const APP_NAME: &str = "Poof";
