//! Screens follow the Elm architecture in miniature: each owns its `State`
//! and `Message`, exposes `update` and `view`, and talks to the rest of the
//! app only through the [`Event`]s it returns inside an [`Action`].

pub mod changes;
pub mod cleanup;
pub mod explore;
pub mod overview;
pub mod settings;

use std::path::PathBuf;

use iced::Task;
use poof_core::monitor::Sample;
use poof_core::settings::Settings;
use poof_core::volume::VolumeInfo;

use crate::icon::Icon;
use crate::style::Tone;

/// The sidebar destinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Overview,
    Explore,
    Cleanup,
    Changes,
    Settings,
}

impl Screen {
    pub const ALL: [Screen; 5] =
        [Screen::Overview, Screen::Explore, Screen::Cleanup, Screen::Changes, Screen::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Overview => "Overview",
            Screen::Explore => "Explore",
            Screen::Cleanup => "Clean Up",
            Screen::Changes => "Changes",
            Screen::Settings => "Settings",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Screen::Overview => Icon::Gauge,
            Screen::Explore => Icon::Grid,
            Screen::Cleanup => Icon::Sparkles,
            Screen::Changes => Icon::History,
            Screen::Settings => Icon::Settings,
        }
    }
}

/// App-wide data every screen may read.
#[derive(Debug, Default)]
pub struct Shared {
    pub volume: Option<VolumeInfo>,
    pub samples: Vec<Sample>,
    pub settings: Settings,
    pub full_disk_access: bool,
}

/// Something a screen wants the app (or another screen) to react to.
#[derive(Debug, Clone)]
pub enum Event {
    /// Files were deleted or trashed; disk figures are stale.
    Removed {
        bytes: u64,
        trashed: bool,
        failed: usize,
    },
    /// Show this path in Explore (scanning it if needed).
    Explore(PathBuf),
    Navigate(Screen),
    /// A scan finished and its summary was saved to history.
    Scanned,
    Toast(String, Tone),
    /// The user changed a setting.
    SettingsChanged(Settings),
}

/// The result of a screen's `update`: work to run and/or an event to emit.
#[derive(Debug)]
pub struct Action<M> {
    pub task: Task<M>,
    pub event: Option<Event>,
}

impl<M> Action<M> {
    pub fn none() -> Self {
        Self { task: Task::none(), event: None }
    }

    pub fn task(task: Task<M>) -> Self {
        Self { task, event: None }
    }

    pub fn event(event: Event) -> Self {
        Self { task: Task::none(), event: Some(event) }
    }

    pub fn with_event(mut self, event: Event) -> Self {
        self.event = Some(event);
        self
    }
}

impl<M> From<Task<M>> for Action<M> {
    fn from(task: Task<M>) -> Self {
        Self::task(task)
    }
}
