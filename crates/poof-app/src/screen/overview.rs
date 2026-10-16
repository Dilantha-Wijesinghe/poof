//! Overview: how full the disk is, how that's changing, and where space
//! usually hides — answerable in seconds, before any full scan.

use std::path::PathBuf;

use iced::widget::{Space, canvas, column, container, row, space, text, tooltip};
use iced::{Alignment, Element, Fill, FillPortion, Length};
use poof_core::size::{ByteSize, gb, signed};
use poof_core::suspects::{self, Measured};
use poof_core::system::{self as mac, Snapshot};
use poof_core::time;

use super::{Action, Event, Screen, Shared};
use crate::icon::Icon;
use crate::style::{self, Kind, Tone, size, space as gap};
use crate::task;
use crate::widget::{
    self, Paint, badge, banner, body, btn, caption, card, chart, display, divider, icon_button, label,
    legend, muted, page_header, page_scroll, section,
};

#[derive(Debug, Default)]
pub struct State {
    suspects: Option<Vec<Measured>>,
    snapshots: Option<Result<Vec<Snapshot>, String>>,
    thinning: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Refresh,
    SuspectsMeasured(Vec<Measured>),
    SnapshotsListed(Result<Vec<Snapshot>, String>),
    ThinSnapshots,
    Thinned(Result<(), String>),
    Explore(PathBuf),
    Reveal(PathBuf),
    Go(Screen),
    GrantAccess,
}

impl State {
    pub fn new() -> (Self, iced::Task<Message>) {
        (Self::default(), load())
    }

    pub fn update(&mut self, message: Message) -> Action<Message> {
        match message {
            Message::Refresh => {
                self.suspects = None;
                self.snapshots = None;
                load().into()
            }
            Message::SuspectsMeasured(measured) => {
                self.suspects = Some(measured);
                Action::none()
            }
            Message::SnapshotsListed(result) => {
                self.snapshots = Some(result);
                Action::none()
            }
            Message::ThinSnapshots => {
                self.thinning = true;
                task::blocking(|| mac::thin_snapshots().map_err(|e| e.to_string()), Message::Thinned).into()
            }
            Message::Thinned(result) => {
                self.thinning = false;
                let toast = match &result {
                    Ok(()) => {
                        Event::Toast("Asked Time Machine to clear local snapshots.".into(), Tone::Success)
                    }
                    Err(e) => Event::Toast(format!("Could not thin snapshots: {e}"), Tone::Danger),
                };
                Action::task(list_snapshots()).with_event(toast)
            }
            Message::Explore(path) => Action::event(Event::Explore(path)),
            Message::Reveal(path) => {
                let _ = mac::reveal_in_finder(&path);
                Action::none()
            }
            Message::Go(screen) => Action::event(Event::Navigate(screen)),
            Message::GrantAccess => {
                let _ = mac::open_full_disk_access_settings();
                Action::none()
            }
        }
    }

    pub fn view<'a>(&'a self, shared: &'a Shared, reclaimable: Option<u64>) -> Element<'a, Message> {
        let header = page_header(
            "Overview",
            "How much space you have and what's using it.",
            btn("Refresh", Some(Icon::Refresh), Kind::Secondary).on_press(Message::Refresh),
        );

        let access = (!shared.full_disk_access).then(|| {
            banner(
                Icon::Lock,
                Tone::Warning,
                "Give Poof Full Disk Access for complete results",
                "Without it, Mail, Messages, Safari, the Trash and other apps' data can't be measured.",
                Some(btn("Open Settings", None, Kind::Secondary).on_press(Message::GrantAccess).into()),
            )
        });

        let cards = row![reclaim_card(reclaimable), trend_card(shared), self.snapshot_card(),]
            .spacing(gap::L)
            .height(Length::Fixed(204.0));

        let content = column![header]
            .push(access)
            .push(hero(shared))
            .push(cards)
            .push(section("Common space hogs"))
            .push(self.suspects_card(shared))
            .spacing(gap::XL);
        page_scroll(content)
    }

    fn snapshot_card(&self) -> Element<'_, Message> {
        let (value, note, action): (String, String, Option<Message>) = match &self.snapshots {
            None => ("…".into(), "Checking Time Machine…".into(), None),
            Some(Err(e)) => ("Unknown".into(), format!("Couldn't list snapshots: {e}"), None),
            Some(Ok(list)) if list.is_empty() => {
                ("None".into(), "Deleting files frees space right away.".into(), None)
            }
            Some(Ok(list)) => {
                let oldest = list.iter().filter_map(|s| s.created).min();
                let note = format!(
                    "Oldest was taken {}. Until a snapshot expires, files you delete still take up space.",
                    oldest.map_or_else(|| "unknown".to_owned(), time::relative)
                );
                (list.len().to_string(), note, (!self.thinning).then_some(Message::ThinSnapshots))
            }
        };
        let label = if self.thinning { "Thinning…" } else { "Thin snapshots" };
        stat_card(
            Icon::Camera,
            "Local snapshots",
            text(value).size(28).font(crate::font::BOLD).into(),
            note,
            btn(label, None, Kind::Secondary).on_press_maybe(action),
        )
    }

    fn suspects_card(&self, shared: &Shared) -> Element<'_, Message> {
        let Some(measured) = &self.suspects else {
            return card(row![Icon::Search.view(16), muted("Checking common space hogs…")].spacing(gap::S))
                .into();
        };
        let used = shared.volume.map_or(1, |v| v.used()).max(1);
        let rows = measured.iter().enumerate().fold(column![].spacing(gap::M), |col, (i, m)| {
            let col = if i > 0 { col.push(divider()) } else { col };
            col.push(suspect_row(m, used))
        });
        card(column![
            muted("These are measured directly, so no scan is needed. Most are best cleaned up from inside the app that created them."),
            space().height(gap::S),
            rows
        ])
        .into()
    }
}

fn reclaim_card<'a>(reclaimable: Option<u64>) -> Element<'a, Message> {
    let value: Element<'_, Message> = match reclaimable {
        Some(bytes) => text(ByteSize(bytes).to_string()).size(28).font(crate::font::BOLD).into(),
        None => muted("Analyzing…").into(),
    };
    stat_card(
        Icon::Sparkles,
        "Ready to clean",
        value,
        "Caches, logs and build files. Apps recreate these when needed.",
        btn("Review", None, Kind::Primary).on_press(Message::Go(Screen::Cleanup)),
    )
}

fn load() -> iced::Task<Message> {
    iced::Task::batch([task::blocking(suspects::measure, Message::SuspectsMeasured), list_snapshots()])
}

fn list_snapshots() -> iced::Task<Message> {
    task::blocking(|| mac::snapshots().map_err(|e| e.to_string()), Message::SnapshotsListed)
}

/// The big capacity read-out with a used / purgeable / free bar.
fn hero<'a>(shared: &Shared) -> Element<'a, Message> {
    let Some(v) = shared.volume else {
        return card(muted("Reading disk information…")).into();
    };
    let fraction = |bytes: u64| bytes as f32 / v.total.max(1) as f32;
    let used = v.used_fraction();
    let (status, tone) = match used {
        u if u > 0.9 => ("Almost full", Tone::Danger),
        u if u > 0.8 => ("Getting full", Tone::Warning),
        _ => ("Healthy", Tone::Success),
    };

    let numbers = row![
        column![
            caption("AVAILABLE ON MACINTOSH HD"),
            row![display(ByteSize(v.available).to_string()), muted(format!("of {}", ByteSize(v.total)))]
                .spacing(gap::M)
                .align_y(Alignment::End),
        ]
        .spacing(gap::XS),
        space::horizontal(),
        badge(status, tone),
    ]
    .align_y(Alignment::Start);

    let purgeable = Paint::Token(|t| style::alpha(t.violet, 0.45));
    let bar = widget::segmented_bar(&[(used, Paint::Brand), (fraction(v.purgeable()), purgeable)], 14.0);
    let legend_row = row![
        legend(Paint::Brand, "Used", ByteSize(v.used()).to_string()),
        legend(purgeable, "Purgeable", ByteSize(v.purgeable()).to_string()),
        legend(Paint::Token(|t| t.track), "Free", ByteSize(v.free).to_string()),
    ]
    .spacing(gap::XL);

    let explainer = (v.purgeable() > gb(1)).then(|| {
        caption(format!(
            "Purgeable space ({}) counts as available. macOS clears it when it needs room, usually by removing local snapshots, iCloud downloads and caches.",
            ByteSize(v.purgeable())
        ))
    });

    card(column![numbers, bar, legend_row].push(explainer).spacing(gap::L)).padding(gap::XL).into()
}

/// Free-space trend over the past week, with a sparkline.
fn trend_card(shared: &Shared) -> Element<'_, Message> {
    let samples = &shared.samples;
    let week: Vec<_> = samples.last().map_or_else(Vec::new, |last| {
        samples.iter().copied().filter(|s| last.at - s.at <= 7 * time::DAY).collect()
    });
    let (value, note, tone) = match (week.first(), week.last()) {
        (Some(first), Some(last)) if week.len() > 1 => {
            let delta = last.available as i64 - first.available as i64;
            let tone = if delta < -(gb(1) as i64) { Tone::Danger } else { Tone::Success };
            let since = time::date_time(first.at);
            (signed(delta), format!("Change since {since}."), tone)
        }
        _ => (
            "No data yet".to_owned(),
            "Poof notes your free space each time it opens. Check back in a few days to see a trend.".into(),
            Tone::Neutral,
        ),
    };
    let spark = shared.samples.len() > 1;
    let chart: Element<'_, ()> = if spark {
        canvas(chart::Chart { samples: &shared.samples, compact: true }).width(Fill).height(44).into()
    } else {
        Space::new().height(44).into()
    };
    card(
        column![
            row![Icon::History.view(16), label("Free space trend")]
                .spacing(gap::S)
                .align_y(Alignment::Center),
            text(value).size(28).font(crate::font::BOLD).style(style::tone_text(tone)),
            chart.map(|()| Message::Go(Screen::Changes)),
            caption(note),
        ]
        .spacing(gap::S),
    )
    .height(Fill)
    .width(FillPortion(1))
    .into()
}

fn stat_card<'a>(
    icon: Icon,
    title: &'a str,
    value: Element<'a, Message>,
    note: impl text::IntoFragment<'a>,
    action: iced::widget::Button<'a, Message>,
) -> Element<'a, Message> {
    card(
        column![
            row![icon.view(16), label(title)].spacing(gap::S).align_y(Alignment::Center),
            value,
            caption(note),
            space::vertical(),
            action,
        ]
        .spacing(gap::S),
    )
    .height(Fill)
    .width(FillPortion(1))
    .into()
}

fn suspect_row(m: &Measured, used: u64) -> Element<'_, Message> {
    let s = m.suspect;
    let location = m.locations.first().cloned().unwrap_or_default();
    let explore_target = if location.is_dir() {
        location.clone()
    } else {
        location.parent().map(PathBuf::from).unwrap_or_default()
    };
    let advice = tooltip(
        Icon::Info.view(15),
        container(text(s.advice).size(size::CAPTION)).padding([6, 10]).max_width(300).style(style::tooltip),
        tooltip::Position::Left,
    );
    row![
        column![body(s.name).font(crate::font::MEDIUM), caption(s.description)]
            .spacing(2)
            .width(FillPortion(6)),
        column![
            text(ByteSize(m.size).to_string()).size(size::LABEL).font(crate::font::SEMIBOLD),
            widget::meter(m.size as f32 / used as f32, 4.0),
        ]
        .spacing(6)
        .align_x(Alignment::End)
        .width(FillPortion(2)),
        advice,
        icon_button(Icon::FolderOpen, "Explore", Some(Message::Explore(explore_target))),
        icon_button(Icon::ExternalLink, "Show in Finder", Some(Message::Reveal(location))),
    ]
    .spacing(gap::M)
    .align_y(Alignment::Center)
    .into()
}
