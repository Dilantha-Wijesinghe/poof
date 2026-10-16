//! The root of the Elm-style application: owns every screen, the shared data,
//! the sidebar, toasts and dialogs, and routes messages and events between them.

use std::path::Path;
use std::time::{Duration, Instant};

use iced::widget::{button, column, container, row, space, stack, svg, text};
use iced::{Alignment, Element, Fill, Subscription, Task, Theme, system, theme, time, window};
use poof_core::settings::{Appearance, Settings};
use poof_core::size::ByteSize;
use poof_core::{monitor, system as mac, volume};

use crate::icon::Icon;
use crate::screen::{Action, Event, Screen, Shared, changes, cleanup, explore, overview, settings};
use crate::style::{self, Tone, size, space as gap};
use crate::widget::{self, caption};
use crate::{font, task, updater};

#[derive(Debug)]
pub struct App {
    screen: Screen,
    shared: Shared,
    system_dark: bool,
    overview: overview::State,
    explore: explore::State,
    cleanup: cleanup::State,
    changes: changes::State,
    updater: updater::State,
    toast: Option<Toast>,
}

#[derive(Debug)]
struct Toast {
    message: String,
    tone: Tone,
    shown: Instant,
}

const TOAST_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Screen),
    SystemTheme(theme::Mode),
    /// The sidebar's light/dark switch.
    SetDark(bool),
    RefreshDisk,
    DiskRefreshed(Option<volume::VolumeInfo>, Vec<monitor::Sample>),
    Tick,
    Overview(overview::Message),
    Explore(explore::Message),
    Cleanup(cleanup::Message),
    Changes(changes::Message),
    Settings(settings::Message),
    Updater(updater::Message),
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let settings = Settings::load();
        let (overview, overview_task) = overview::State::new();
        let (cleanup, cleanup_task) = cleanup::State::new(&settings);
        let (changes, changes_task) = changes::State::new();
        let monitor_task = sync_monitor(&settings);
        let update_task = updater::startup_check(settings.check_for_updates);

        let mut app = Self {
            screen: Screen::Overview,
            shared: Shared { settings, full_disk_access: mac::has_full_disk_access(), ..Shared::default() },
            system_dark: false,
            overview,
            explore: explore::State::default(),
            cleanup,
            changes,
            updater: updater::State::default(),
            toast: None,
        };
        if let Some(from) = poof_core::update::take_update_note() {
            let current = poof_core::update::Version::current();
            app.show_toast(format!("Updated to Poof {current} (from {from})."), Tone::Success);
        }
        let boot = Task::batch([
            system::theme().map(Message::SystemTheme),
            refresh_disk(),
            overview_task.map(Message::Overview),
            cleanup_task.map(Message::Cleanup),
            changes_task.map(Message::Changes),
            monitor_task,
            update_task.map(Message::Updater),
        ]);
        (app, boot)
    }

    pub fn theme(&self) -> Theme {
        style::theme(self.is_dark())
    }

    /// Whether the dark theme is showing, after resolving "Match System".
    fn is_dark(&self) -> bool {
        match self.shared.settings.appearance {
            Appearance::System => self.system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            system::theme_changes().map(Message::SystemTheme),
            // Keep the free-space figures live while the app is open.
            time::every(Duration::from_secs(60)).map(|_| Message::RefreshDisk),
            self.explore.subscription().map(Message::Explore),
            if self.shared.settings.check_for_updates {
                time::every(Duration::from_secs(24 * 60 * 60))
                    .map(|_| Message::Updater(updater::Message::Check { manual: false }))
            } else {
                Subscription::none()
            },
            if self.toast.is_some() {
                time::every(Duration::from_millis(500)).map(|_| Message::Tick)
            } else {
                Subscription::none()
            },
        ])
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(screen) => self.navigate(screen),
            Message::SystemTheme(mode) => {
                self.system_dark = mode == theme::Mode::Dark;
                Task::none()
            }
            Message::SetDark(dark) => {
                let mut settings = self.shared.settings.clone();
                settings.appearance = if dark { Appearance::Dark } else { Appearance::Light };
                self.on_event(Event::SettingsChanged(settings))
            }
            Message::RefreshDisk => {
                self.shared.full_disk_access = mac::has_full_disk_access();
                refresh_disk()
            }
            Message::DiskRefreshed(volume, samples) => {
                self.shared.volume = volume.or(self.shared.volume);
                if !samples.is_empty() {
                    self.shared.samples = samples;
                }
                Task::none()
            }
            Message::Tick => {
                if self.toast.as_ref().is_some_and(|t| t.shown.elapsed() > TOAST_DURATION) {
                    self.toast = None;
                }
                Task::none()
            }
            Message::Overview(m) => {
                let action = self.overview.update(m);
                self.handle(action, Message::Overview)
            }
            Message::Explore(m) => {
                let action = self.explore.update(m, &self.shared);
                self.handle(action, Message::Explore)
            }
            Message::Cleanup(m) => {
                let action = self.cleanup.update(m, &self.shared.settings);
                self.handle(action, Message::Cleanup)
            }
            Message::Changes(m) => {
                let action = self.changes.update(m);
                self.handle(action, Message::Changes)
            }
            Message::Updater(m) => {
                let action = self.updater.update(m);
                self.handle(action, Message::Updater)
            }
            Message::Settings(m) => {
                let action = settings::update(m, &self.shared.settings);
                self.handle(action, Message::Settings)
            }
        }
    }

    /// Runs a screen's task and reacts to the event it emitted, if any.
    fn handle<M: Send + 'static>(&mut self, action: Action<M>, wrap: fn(M) -> Message) -> Task<Message> {
        let task = action.task.map(wrap);
        match action.event {
            Some(event) => Task::batch([task, self.on_event(event)]),
            None => task,
        }
    }

    fn on_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::Navigate(screen) => self.navigate(screen),
            Event::Explore(path) => {
                self.screen = Screen::Explore;
                self.explore.open_path(&path).map(Message::Explore)
            }
            Event::Removed { bytes, trashed, failed } => {
                let message = match (trashed, failed) {
                    (true, 0) => {
                        format!(
                            "Moved {} to the Trash. Empty the Trash to get the space back.",
                            ByteSize(bytes)
                        )
                    }
                    (false, 0) => format!("Freed {}.", ByteSize(bytes)),
                    (_, n) => format!("Removed {}. {n} item(s) couldn't be removed.", ByteSize(bytes)),
                };
                self.show_toast(message, if failed == 0 { Tone::Success } else { Tone::Warning });
                Task::batch([
                    refresh_disk(),
                    self.cleanup.refresh(&self.shared.settings).map(Message::Cleanup),
                ])
            }
            Event::Scanned => self.changes.reload().map(Message::Changes),
            Event::CheckForUpdates => self.update(Message::Updater(updater::Message::Check { manual: true })),
            Event::ReviewUpdate => self.update(Message::Updater(updater::Message::Review)),
            Event::Toast(message, tone) => {
                self.show_toast(message, tone);
                Task::none()
            }
            Event::SettingsChanged(settings) => {
                let monitor_changed = settings.monitor_enabled != self.shared.settings.monitor_enabled;
                let stale_changed = settings.stale_project_days != self.shared.settings.stale_project_days;
                let updates_enabled = settings.check_for_updates && !self.shared.settings.check_for_updates;
                if let Err(e) = settings.save() {
                    self.show_toast(format!("Could not save settings: {e}"), Tone::Danger);
                }
                self.shared.settings = settings;
                let mut tasks = Vec::new();
                if monitor_changed {
                    tasks.push(sync_monitor(&self.shared.settings));
                }
                if stale_changed {
                    tasks.push(self.cleanup.refresh(&self.shared.settings).map(Message::Cleanup));
                }
                if updates_enabled {
                    tasks.push(updater::startup_check(true).map(Message::Updater));
                }
                Task::batch(tasks)
            }
        }
    }

    fn navigate(&mut self, screen: Screen) -> Task<Message> {
        self.screen = screen;
        // Re-check privacy access when returning from System Settings.
        self.shared.full_disk_access = mac::has_full_disk_access();
        match screen {
            Screen::Changes => self.changes.reload().map(Message::Changes),
            _ => Task::none(),
        }
    }

    fn show_toast(&mut self, message: String, tone: Tone) {
        self.toast = Some(Toast { message, tone, shown: Instant::now() });
    }

    // ------------------------------------------------------------------ view

    pub fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match self.screen {
            Screen::Overview => {
                self.overview.view(&self.shared, self.cleanup.reclaimable()).map(Message::Overview)
            }
            Screen::Explore => self.explore.view(&self.shared).map(Message::Explore),
            Screen::Cleanup => self.cleanup.view(&self.shared).map(Message::Cleanup),
            Screen::Changes => self.changes.view(&self.shared).map(Message::Changes),
            Screen::Settings => settings::view(&self.shared, &self.updater).map(Message::Settings),
        };
        // Leave room for the transparent title bar and its traffic lights.
        let main =
            container(column![space().height(30), content]).width(Fill).height(Fill).style(style::page);
        let layout: Element<'_, Message> = row![self.sidebar(), main].into();

        // The toast layer always exists (empty when idle) so showing a toast
        // doesn't rebuild the layout and reset scroll positions.
        let toast: Element<'_, Message> = self.toast.as_ref().map_or_else(|| space().into(), toast_view);
        let layout: Element<'_, Message> = stack![layout, toast].into();
        let dialog = match self.screen {
            Screen::Explore => self.explore.dialog().map(|d| d.map(Message::Explore)),
            Screen::Cleanup => self.cleanup.dialog().map(|d| d.map(Message::Cleanup)),
            _ => None,
        }
        .or_else(|| self.updater.dialog().map(|d| d.map(Message::Updater)));
        widget::modal(layout, dialog)
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let brand = row![
            svg(svg::Handle::from_memory(include_bytes!("../assets/brand/mark.svg").as_slice()))
                .width(34)
                .height(34),
            column![
                text("POOF").size(17).font(font::BOLD),
                text("Clean up. Breathe easy.").size(10.5).style(style::faint),
            ]
            .spacing(1),
        ]
        .spacing(gap::S)
        .align_y(Alignment::Center);

        let nav = Screen::ALL.iter().fold(column![].spacing(2), |col, &screen| {
            let active = self.screen == screen;
            let tint = if active { style::icon(|t| t.accent) } else { style::icon(|t| t.muted) };
            let badge = (screen == Screen::Cleanup)
                .then(|| self.cleanup.reclaimable())
                .flatten()
                .filter(|&b| b > 0)
                .map(|b| caption(ByteSize(b).to_string()));
            let item = row![
                screen.icon().view(16).style(tint),
                text(screen.label()).size(size::BODY).font(font::MEDIUM),
                space::horizontal(),
            ]
            .push(badge)
            .spacing(10)
            .align_y(Alignment::Center);
            col.push(
                button(item)
                    .width(Fill)
                    .padding([8, 10])
                    .style(style::nav_item(active))
                    .on_press(Message::Navigate(screen)),
            )
        });

        let disk = self.shared.volume.map(|v| {
            container(
                column![
                    row![
                        caption("Macintosh HD"),
                        space::horizontal(),
                        caption(format!("{:.0}%", v.used_fraction() * 100.0))
                    ],
                    widget::meter(v.used_fraction(), 6.0),
                    text(format!("{} available", ByteSize(v.available)))
                        .size(size::BODY)
                        .font(font::SEMIBOLD),
                ]
                .spacing(gap::S),
            )
            .padding(gap::M)
            .style(style::well)
        });

        container(
            column![space().height(40), brand, space().height(gap::XL), nav, space::vertical(),]
                .push(self.updater.notice().map(|notice| notice.map(Message::Updater)))
                .push(self.theme_switch())
                .push(disk)
                .spacing(gap::S)
                .padding([gap::L, gap::M])
                .width(Fill),
        )
        .width(224)
        .height(Fill)
        .style(style::sidebar)
        .into()
    }
}

impl App {
    /// A two-segment sun/moon switch. Picking one overrides "Match System",
    /// which stays available in Settings.
    fn theme_switch(&self) -> Element<'_, Message> {
        let dark = self.is_dark();
        let segment = |icon: Icon, label: &'static str, for_dark: bool| {
            let active = dark == for_dark;
            let tint = if active { style::icon(|t| t.accent) } else { style::icon(|t| t.muted) };
            let content = row![icon.view(14).style(tint), text(label).size(size::CAPTION).font(font::MEDIUM)]
                .spacing(6)
                .align_y(Alignment::Center);
            button(container(content).center_x(Fill))
                .width(Fill)
                .padding([5, 0])
                .style(style::nav_item(active))
                .on_press(Message::SetDark(for_dark))
        };
        container(row![segment(Icon::Sun, "Light", false), segment(Icon::Moon, "Dark", true)].spacing(2))
            .padding(3)
            .style(style::well)
            .into()
    }
}

fn toast_view(toast: &Toast) -> Element<'_, Message> {
    let tone = toast.tone;
    let icon = match tone {
        Tone::Success => Icon::Check,
        _ => Icon::AlertTriangle,
    };
    let bubble = container(
        row![
            icon.view(16).style(style::icon_tone(tone)),
            text(&toast.message).size(size::BODY).font(font::MEDIUM)
        ]
        .spacing(gap::S)
        .align_y(Alignment::Center),
    )
    .padding([10, 16])
    .style(style::toast(tone));
    container(bubble)
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::End)
        .padding(28)
        .into()
}

/// Reads disk capacity and records a free-space sample, off the UI thread.
fn refresh_disk() -> Task<Message> {
    task::blocking(
        || {
            let volume = volume::query(Path::new("/")).ok();
            let _ = monitor::record();
            (volume, monitor::samples())
        },
        |(volume, samples)| Message::DiskRefreshed(volume, samples),
    )
}

/// Installs or removes the background monitor to match the settings.
fn sync_monitor(settings: &Settings) -> Task<Message> {
    let enabled = settings.monitor_enabled;
    task::blocking(
        move || {
            if enabled {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                monitor::install_agent(&exe).map_err(|e| e.to_string())
            } else if monitor::agent_installed() {
                monitor::uninstall_agent().map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        },
        |result| match result {
            Ok(()) => Message::Tick,
            Err(e) => Message::Settings(settings::Message::MonitorFailed(e)),
        },
    )
}

/// Window configuration: a modern, full-height layout under a transparent
/// title bar.
pub fn window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(1200.0, 800.0),
        min_size: Some(iced::Size::new(980.0, 660.0)),
        platform_specific: window::settings::PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        },
        ..window::Settings::default()
    }
}
