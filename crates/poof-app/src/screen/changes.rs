//! Changes: free space over time, and exactly what grew between two scans.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::{canvas, column, container, pick_list, row, space, text};
use iced::{Alignment, Element, Fill, Task};
use poof_core::history::{self, Change, Summary};
use poof_core::size::{ByteSize, signed};
use poof_core::{paths, system as mac, time};

use super::{Action, Event, Screen, Shared};
use crate::icon::Icon;
use crate::style::{self, Kind, Tone, size, space as gap};
use crate::widget::{
    badge, banner, body, btn, caption, card, chart, divider, empty_state, icon_button, label, muted,
    page_header, page_scroll, section, single_line,
};
use crate::{font, task};

/// Changes smaller than this are noise.
const MIN_DELTA: u64 = 100 * 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    Home,
    Disk,
}

impl Scope {
    fn root(self) -> PathBuf {
        match self {
            Scope::Home => paths::home(),
            Scope::Disk => PathBuf::from(paths::DATA_VOLUME),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Scope::Home => "Home",
            Scope::Disk => "Macintosh HD",
        }
    }
}

/// A saved scan offered as a comparison baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanChoice(i64);

impl fmt::Display for ScanChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", time::date_time(self.0), time::relative(self.0))
    }
}

#[derive(Debug, Default)]
pub struct State {
    scope: Scope,
    loaded: Option<Loaded>,
}

#[derive(Debug, Clone)]
pub struct Loaded {
    /// Saved scans for the scope, newest first.
    stamps: Vec<i64>,
    latest: Option<Arc<Summary>>,
    baseline: Option<Arc<Summary>>,
    changes: Vec<Change>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Loaded),
    SetScope(Scope),
    SetBaseline(ScanChoice),
    ScanNow,
    Explore(PathBuf),
    Reveal(PathBuf),
    OpenSettings,
}

impl State {
    pub fn new() -> (Self, Task<Message>) {
        let mut state = Self::default();
        let task = state.reload();
        (state, task)
    }

    /// Reloads history for the current scope, comparing the latest scan with
    /// the one before it.
    pub fn reload(&mut self) -> Task<Message> {
        load(self.scope, None)
    }

    pub fn update(&mut self, message: Message) -> Action<Message> {
        match message {
            Message::Loaded(loaded) => {
                self.loaded = Some(loaded);
                Action::none()
            }
            Message::SetScope(scope) => {
                self.scope = scope;
                self.loaded = None;
                load(scope, None).into()
            }
            Message::SetBaseline(choice) => load(self.scope, Some(choice.0)).into(),
            Message::ScanNow | Message::Explore(_) => {
                let path = match message {
                    Message::Explore(path) => path,
                    _ => self.scope.root(),
                };
                Action::event(Event::Explore(path))
            }
            Message::Reveal(path) => {
                let _ = mac::reveal_in_finder(&path);
                Action::none()
            }
            Message::OpenSettings => Action::event(Event::Navigate(Screen::Settings)),
        }
    }

    pub fn view<'a>(&'a self, shared: &'a Shared) -> Element<'a, Message> {
        let scope_button = |scope: Scope| {
            let kind = if self.scope == scope { Kind::Primary } else { Kind::Secondary };
            btn(scope.label(), None, kind).on_press(Message::SetScope(scope))
        };
        let header = page_header(
            "Changes",
            "How free space moved over time, and exactly what grew.",
            row![scope_button(Scope::Home), scope_button(Scope::Disk)].spacing(gap::XS),
        );

        let monitor = (!shared.settings.monitor_enabled).then(|| {
            banner(
                Icon::Bell,
                Tone::Accent,
                "Never be surprised by a full disk again",
                "Turn on background monitoring to be notified when free space drops suddenly.",
                Some(btn("Settings", None, Kind::Secondary).on_press(Message::OpenSettings).into()),
            )
        });

        page_scroll(
            column![header].push(monitor).push(timeline(shared)).push(self.changes_view()).spacing(gap::XL),
        )
    }

    fn changes_view(&self) -> Element<'_, Message> {
        let Some(loaded) = &self.loaded else {
            return card(muted("Loading scan history…")).into();
        };
        let scope = self.scope;
        let (Some(latest), Some(baseline)) = (&loaded.latest, &loaded.baseline) else {
            let (icon, title, message, action) = match &loaded.latest {
                None => (
                    Icon::History,
                    "No scans yet",
                    format!(
                        "Scan {} once now and again later. Poof will show exactly which folders grew in between.",
                        scope.label()
                    ),
                    "Scan Now",
                ),
                Some(only) => (
                    Icon::Clock,
                    "One scan so far",
                    format!(
                        "Scan again later, or whenever space drops unexpectedly, to see what changed since {}.",
                        time::date_time(only.taken_at)
                    ),
                    "Scan Again",
                ),
            };
            return card(empty_state(
                icon,
                title,
                message,
                Some(btn(action, Some(Icon::Scan), Kind::Primary).on_press(Message::ScanNow).into()),
            ))
            .into();
        };

        let choices: Vec<ScanChoice> = loaded.stamps.iter().skip(1).map(|&t| ScanChoice(t)).collect();
        let picker = pick_list(choices, Some(ScanChoice(baseline.taken_at)), Message::SetBaseline)
            .text_size(size::BODY)
            .style(style::pick_list);
        let total = latest.total as i64 - baseline.total as i64;
        let summary = row![
            column![
                label(format!(
                    "{} since {}",
                    if total >= 0 { "Grew" } else { "Shrank" },
                    time::date_time(baseline.taken_at)
                )),
                caption(format!(
                    "{}: {} → {} · latest scan {}",
                    scope.label(),
                    ByteSize(baseline.total),
                    ByteSize(latest.total),
                    time::relative(latest.taken_at)
                )),
            ]
            .spacing(2)
            .width(Fill),
            text(signed(total)).size(size::TITLE).font(font::BOLD).style(style::tone_text(if total > 0 {
                Tone::Warning
            } else {
                Tone::Success
            })),
        ]
        .align_y(Alignment::Center);

        let list: Element<'_, Message> = if loaded.changes.is_empty() {
            muted("No folder changed by more than 100 MB.").into()
        } else {
            let root = &latest.root;
            loaded
                .changes
                .iter()
                .enumerate()
                .fold(column![].spacing(gap::M), |col, (i, change)| {
                    let col = if i > 0 { col.push(divider()) } else { col };
                    col.push(change_row(root, scope, change))
                })
                .into()
        };

        column![
            row![section("What changed"), space::horizontal(), caption("Compared with"), picker]
                .spacing(gap::S)
                .align_y(Alignment::Center),
            card(column![summary, divider(), list].spacing(gap::L)),
        ]
        .spacing(gap::S)
        .into()
    }
}

fn change_row<'a>(root: &std::path::Path, scope: Scope, change: &'a Change) -> Element<'a, Message> {
    let grew = change.attributed > 0;
    let path = root.join(&change.path);
    let name = if change.path.is_empty() {
        format!("Spread across {}", scope.label())
    } else {
        paths::display(&path)
    };
    let mut detail = format!("{} → {}", ByteSize(change.before), ByteSize(change.after));
    if change.spread {
        detail.push_str(" · includes many smaller items");
    }
    let (icon, tone) = if grew { (Icon::ArrowUp, Tone::Warning) } else { (Icon::ArrowDown, Tone::Success) };
    row![
        container(icon.view(15).style(style::icon_tone(tone))).padding(7).style(style::badge(tone)),
        column![single_line(body(name).font(font::MEDIUM)), caption(detail)].spacing(2).width(Fill),
        badge(signed(change.attributed), tone),
        icon_button(Icon::FolderOpen, "Explore", Some(Message::Explore(path.clone()))),
        icon_button(
            Icon::ExternalLink,
            "Show in Finder",
            path.exists().then_some(Message::Reveal(path.clone()))
        ),
    ]
    .spacing(gap::M)
    .align_y(Alignment::Center)
    .into()
}

/// The free-space line chart.
fn timeline(shared: &Shared) -> Element<'_, Message> {
    let samples = &shared.samples;
    let header =
        row![Icon::History.view(16), label("Available space")].spacing(gap::S).align_y(Alignment::Center);
    let body: Element<'_, Message> = match (samples.first(), samples.last()) {
        (Some(first), Some(last)) if samples.len() > 1 => {
            let delta = last.available as i64 - first.available as i64;
            let tone = if delta < 0 { Tone::Warning } else { Tone::Success };
            let chart: Element<'_, ()> = canvas(chart::Chart { samples, compact: false }).width(Fill).height(190).into();
            column![
                row![
                    header,
                    space::horizontal(),
                    caption(format!("since {}", time::date(first.at))),
                    badge(signed(delta), tone),
                ]
                .spacing(gap::S)
                .align_y(Alignment::Center),
                chart.map(|()| Message::OpenSettings),
            ]
            .spacing(gap::M)
            .into()
        }
        _ => column![
            header,
            muted("Poof records available space every time it runs, and every 30 minutes with background monitoring on. The chart fills in over time."),
        ]
        .spacing(gap::S)
        .into(),
    };
    card(body).into()
}

fn load(scope: Scope, baseline: Option<i64>) -> Task<Message> {
    task::blocking(
        move || {
            let root = scope.root();
            let stamps = history::list(&root);
            let latest = stamps.first().and_then(|&t| history::load(&root, t).ok().flatten());
            let baseline_at = baseline.or_else(|| stamps.get(1).copied());
            let baseline = baseline_at.and_then(|t| history::load(&root, t).ok().flatten());
            let changes = match (&latest, &baseline) {
                (Some(new), Some(old)) => history::changes(old, new, MIN_DELTA),
                _ => Vec::new(),
            };
            Loaded { stamps, latest: latest.map(Arc::new), baseline: baseline.map(Arc::new), changes }
        },
        Message::Loaded,
    )
}
