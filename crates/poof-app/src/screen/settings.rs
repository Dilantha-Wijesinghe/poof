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
use crate::updater;
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
    ReportProblem,
    MonitorFailed(String),
    AutoUpdate(bool),
    CheckForUpdates,
    ReviewUpdate,
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
        Message::AutoUpdate(on) => next.check_for_updates = on,
        Message::CheckForUpdates => return Action::event(Event::CheckForUpdates),
        Message::ReviewUpdate => return Action::event(Event::ReviewUpdate),
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
        Message::ReportProblem => {
            let _ = mac::open_url(&issue_url(env!("CARGO_PKG_VERSION"), &mac::describe_mac()));
            return Action::none();
        }
        Message::MonitorFailed(e) => {
            return Action::event(Event::Toast(
                format!("Couldn't change background monitoring: {e}"),
                Tone::Danger,
            ));
        }
    }
    Action::event(Event::SettingsChanged(next))
}

pub fn view<'a>(shared: &'a Shared, updater: &'a updater::State) -> Element<'a, Message> {
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
                "Safer, but you only get the space back after emptying the Trash. When this is off, Clean Up deletes items right away.",
                toggler(s.cleanup_to_trash).on_toggle(Message::CleanupToTrash).style(style::toggler),
            ),
            divider(),
            setting(
                "Inactive project threshold",
                "Projects you haven't changed in this long count as inactive, and their build folders (node_modules, target and so on) show up in Clean Up.",
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
                "macOS needs free space for updates and virtual memory.",
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
                "Lets Poof measure Mail, Messages, Safari, the Trash and other apps' data. Poof only reads file sizes, never what's inside.",
                access_status,
            ),
            divider(),
            setting(
                "Operations log",
                "A record of every item Poof has deleted or moved to the Trash, with its size and location.",
                btn("Show Log", Some(Icon::FolderOpen), Kind::Secondary).on_press(Message::OpenLog),
            ),
        ]
        .spacing(gap::L),
    );

    page_scroll(
        column![
            page_header("Settings", "Appearance, cleaning and monitoring options.", space()),
            column![section("General"), appearance].spacing(gap::S),
            column![section("Cleaning"), cleaning].spacing(gap::S),
            column![section("Monitoring"), monitoring].spacing(gap::S),
            column![section("Updates"), updates_card(s, updater)].spacing(gap::S),
            column![section("Privacy & Safety"), privacy].spacing(gap::S),
            column![section("About"), about_card()].spacing(gap::S),
        ]
        .spacing(gap::XL)
        .max_width(860),
    )
}

/// Automatic update checks plus the current version and its status.
fn updates_card<'a>(s: &Settings, updater: &'a updater::State) -> Element<'a, Message> {
    let update_action = if updater.has_update() {
        btn("Update…", Some(Icon::Download), Kind::Primary).on_press(Message::ReviewUpdate)
    } else {
        btn("Check Now", Some(Icon::Refresh), Kind::Secondary)
            .on_press_maybe((!updater.is_busy()).then_some(Message::CheckForUpdates))
    };
    card(
        column![
            setting(
                "Check for updates automatically",
                "Once a day, Poof asks GitHub whether a newer version is out. Nothing about you or your Mac is sent.",
                toggler(s.check_for_updates).on_toggle(Message::AutoUpdate).style(style::toggler),
            ),
            divider(),
            setting("Version", updater.summary(), update_action),
        ]
        .spacing(gap::L),
    )
    .into()
}

/// The license, the repository, and a way to report a problem.
fn about_card<'a>() -> Element<'a, Message> {
    card(
        column![
            setting(
                "About",
                format!("{APP_NAME} is open source under the MIT license and doesn't collect any data."),
                btn("GitHub", Some(Icon::ExternalLink), Kind::Secondary).on_press(Message::OpenRepository),
            ),
            divider(),
            setting(
                "Report a problem",
                "Opens a bug report on GitHub with your Poof and macOS versions filled in. Nothing is sent until you submit it.",
                btn("Report…", Some(Icon::ExternalLink), Kind::Secondary).on_press(Message::ReportProblem),
            ),
        ]
        .spacing(gap::L),
    )
    .into()
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

/// A new GitHub bug report with the versions already filled in.
fn issue_url(poof: &str, mac: &str) -> String {
    format!(
        "{REPOSITORY}/issues/new?template=bug_report.yml&version={}&macos={}",
        encode_query(poof),
        encode_query(mac)
    )
}

/// Percent-encodes a URL query value.
fn encode_query(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_url_prefills_the_bug_report() {
        assert_eq!(
            issue_url("0.1.2", "macOS 26.4 (Mac16,12)"),
            format!(
                "{REPOSITORY}/issues/new?template=bug_report.yml&version=0.1.2&macos=macOS%2026.4%20%28Mac16%2C12%29"
            )
        );
    }
}
