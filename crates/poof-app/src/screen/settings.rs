//! Settings. Stateless: it renders [`Shared::settings`] and emits an updated
//! copy, which the app persists and applies.

use std::fmt;

use iced::widget::{column, container, pick_list, row, space, toggler};
use iced::{Alignment, Element, Fill};
use poof_core::settings::{Appearance, Settings};
use poof_core::{APP_NAME, paths, system as mac};

use super::{Action, Event, Shared};
use crate::icon::Icon;
use crate::style::{self, Kind, Tone, size, space as gap};
use crate::widget::{badge, btn, caption, card, divider, label, page_header, page_scroll, section};

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// A whole-number choice with a unit, for pick lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice(u64, &'static str);

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.1)
    }
}

const DROP_CHOICES: [Choice; 5] =
    [Choice(5, "GB"), Choice(10, "GB"), Choice(20, "GB"), Choice(30, "GB"), Choice(50, "GB")];
const LOW_CHOICES: [Choice; 5] =
    [Choice(10, "GB"), Choice(20, "GB"), Choice(30, "GB"), Choice(50, "GB"), Choice(100, "GB")];
const STALE_CHOICES: [Choice; 4] =
    [Choice(14, "days"), Choice(30, "days"), Choice(60, "days"), Choice(90, "days")];

#[derive(Debug, Clone)]
pub enum Message {
    Appearance(Appearance),
    CleanupToTrash(bool),
    Monitor(bool),
    AlertDrop(Choice),
    LowSpace(Choice),
    StaleDays(Choice),
    GrantAccess,
    OpenLog,
    OpenRepository,
    MonitorFailed(String),
}

pub fn update(message: Message, current: &Settings) -> Action<Message> {
    let mut next = current.clone();
    match message {
        Message::Appearance(a) => next.appearance = a,
        Message::CleanupToTrash(on) => next.cleanup_to_trash = on,
        Message::Monitor(on) => next.monitor_enabled = on,
        Message::AlertDrop(c) => next.alert_drop_gb = c.0,
        Message::LowSpace(c) => next.low_space_gb = c.0,
        Message::StaleDays(c) => next.stale_project_days = u32::try_from(c.0).unwrap_or(30),
        Message::GrantAccess => {
            let _ = mac::open_full_disk_access_settings();
            return Action::none();
        }
        Message::OpenLog => {
            let _ = mac::open_url(&paths::log_dir().to_string_lossy());
            return Action::none();
        }
        Message::OpenRepository => {
            let _ = mac::open_url(REPOSITORY);
            return Action::none();
        }
        Message::MonitorFailed(e) => {
            return Action::event(Event::Toast(format!("Background monitoring: {e}"), Tone::Danger));
        }
    }
    Action::event(Event::SettingsChanged(next))
}

pub fn view(shared: &Shared) -> Element<'_, Message> {
    let s = &shared.settings;
    let choice = |list: &[Choice], value: u64| list.iter().copied().find(|c| c.0 == value);

    let appearance = card(setting(
        "Appearance",
        "Follow macOS, or always use light or dark.",
        pick_list(Appearance::ALL, Some(s.appearance), Message::Appearance)
            .text_size(size::BODY)
            .style(style::pick_list),
    ));

    let cleaning = card(
        column![
            setting(
                "Move cleaned items to the Trash",
                "Safer, but space is only freed once you empty the Trash. When off, Clean Up deletes caches immediately.",
                toggler(s.cleanup_to_trash).on_toggle(Message::CleanupToTrash).style(style::toggler),
            ),
            divider(),
            setting(
                "Inactive project threshold",
                "Build folders (node_modules, target, …) in projects untouched this long are offered for cleanup.",
                pick_list(STALE_CHOICES, choice(&STALE_CHOICES, u64::from(s.stale_project_days)), Message::StaleDays)
                    .text_size(size::BODY)
                    .style(style::pick_list),
            ),
        ]
        .spacing(gap::L),
    );

    let monitoring = card(
        column![
            setting(
                "Background monitoring",
                "Checks free space every 30 minutes, even when Poof is closed, and notifies you about sudden drops.",
                toggler(s.monitor_enabled).on_toggle(Message::Monitor).style(style::toggler),
            ),
            divider(),
            setting(
                "Alert when free space drops by",
                "Within 24 hours.",
                pick_list(DROP_CHOICES, choice(&DROP_CHOICES, s.alert_drop_gb), Message::AlertDrop)
                    .text_size(size::BODY)
                    .style(style::pick_list),
            ),
            divider(),
            setting(
                "Alert when available space falls below",
                "macOS needs headroom for updates, swap and snapshots.",
                pick_list(LOW_CHOICES, choice(&LOW_CHOICES, s.low_space_gb), Message::LowSpace)
                    .text_size(size::BODY)
                    .style(style::pick_list),
            ),
        ]
        .spacing(gap::L),
    );

    let access_status = if shared.full_disk_access {
        badge("Granted", Tone::Success)
    } else {
        btn("Open Privacy Settings", Some(Icon::Lock), Kind::Secondary).on_press(Message::GrantAccess).into()
    };
    let privacy = card(
        column![
            setting(
                "Full Disk Access",
                "Lets Poof measure Mail, Messages, Safari, the Trash and other apps' containers. Poof never reads file contents.",
                access_status,
            ),
            divider(),
            setting(
                "Operations log",
                "Every item Poof deletes or moves to the Trash is recorded, with its size and path.",
                btn("Show Log", Some(Icon::FolderOpen), Kind::Secondary).on_press(Message::OpenLog),
            ),
        ]
        .spacing(gap::L),
    );

    let about = card(setting(
        "About",
        format!(
            "{APP_NAME} {} · open source under the MIT license · no telemetry, ever.",
            env!("CARGO_PKG_VERSION")
        ),
        btn("GitHub", Some(Icon::ExternalLink), Kind::Secondary).on_press(Message::OpenRepository),
    ));

    page_scroll(
        column![
            page_header("Settings", "Make Poof work the way you like.", space()),
            column![section("General"), appearance].spacing(gap::S),
            column![section("Cleaning"), cleaning].spacing(gap::S),
            column![section("Monitoring"), monitoring].spacing(gap::S),
            column![section("Privacy & Safety"), privacy].spacing(gap::S),
            column![section("About"), about].spacing(gap::S),
        ]
        .spacing(gap::XL)
        .max_width(860),
    )
}

/// A labelled setting with its control on the right.
fn setting<'a>(
    title: &'a str,
    description: impl iced::widget::text::IntoFragment<'a>,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        column![label(title), caption(description)].spacing(3).width(Fill),
        container(control).align_y(Alignment::Center),
    ]
    .spacing(gap::XL)
    .align_y(Alignment::Center)
    .into()
}
