//! Keeping Poof up to date: background checks, the sidebar notice, and the
//! confirm-and-install flow. The heavy lifting lives in `poof_core::update`.

use std::path::PathBuf;

use iced::widget::{column, container, row, space};
use iced::{Alignment, Element, Fill, Task};
use poof_core::system as mac;
use poof_core::update::{self, Release, Version};

use crate::icon::Icon;
use crate::screen::{Action, Event};
use crate::style::{self, Kind, Tone, space as gap};
use crate::task;
use crate::widget::{Dialog, btn, caption, label};

#[derive(Debug, Clone, Default)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(Release),
    Failed(String),
}

#[derive(Debug, Default)]
pub struct State {
    pub status: Status,
    confirming: bool,
    /// The user asked for this check, so report the outcome either way.
    manual: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Check { manual: bool },
    Checked(Result<Option<Release>, String>),
    Review,
    Cancel,
    Install,
    Installed(Result<PathBuf, String>),
}

impl State {
    pub fn update(&mut self, message: Message) -> Action<Message> {
        match message {
            Message::Check { manual } => {
                if matches!(self.status, Status::Checking | Status::Installing(_)) {
                    return Action::none();
                }
                self.manual = manual;
                self.status = Status::Checking;
                task::blocking(
                    || update::check(Version::current()).map_err(|e| e.to_string()),
                    Message::Checked,
                )
                .into()
            }
            Message::Checked(result) => {
                let manual = std::mem::take(&mut self.manual);
                match result {
                    Ok(Some(release)) => {
                        self.status = Status::Available(release);
                        Action::none()
                    }
                    Ok(None) => {
                        self.status = Status::UpToDate;
                        toast_if(
                            manual,
                            format!("You're on the latest version ({}).", Version::current()),
                            Tone::Success,
                        )
                    }
                    Err(e) => {
                        self.status = if manual { Status::Failed(e.clone()) } else { Status::Idle };
                        toast_if(manual, format!("Couldn't check for updates: {e}."), Tone::Warning)
                    }
                }
            }
            Message::Review => {
                self.confirming = matches!(self.status, Status::Available(_));
                Action::none()
            }
            Message::Cancel => {
                self.confirming = false;
                Action::none()
            }
            Message::Install => self.install(),
            Message::Installed(Ok(bundle)) => {
                // Start the new version once this one has quit; it confirms the update.
                update::remember_update(Version::current());
                match update::relaunch(&bundle, std::process::id()) {
                    Ok(()) => Action::task(iced::exit()),
                    Err(e) => Action::event(Event::Toast(
                        format!("Updated. Please reopen Poof ({e})."),
                        Tone::Success,
                    )),
                }
            }
            Message::Installed(Err(e)) => {
                if let Status::Installing(release) = std::mem::take(&mut self.status) {
                    self.status = Status::Available(release);
                }
                Action::event(Event::Toast(format!("The update didn't install: {e}"), Tone::Danger))
            }
        }
    }

    fn install(&mut self) -> Action<Message> {
        self.confirming = false;
        let Status::Available(release) = &self.status else { return Action::none() };
        let release = release.clone();
        let Some(bundle) = update::current_bundle() else {
            // Running from source: there's no app bundle to replace.
            let _ = mac::open_url(&release.page);
            return Action::event(Event::Toast(
                "This copy of Poof can't update itself. Opening the download page.".into(),
                Tone::Warning,
            ));
        };
        self.status = Status::Installing(release.clone());
        task::blocking(
            move || update::install(&release, &bundle).map(|()| bundle).map_err(|e| e.to_string()),
            Message::Installed,
        )
        .into()
    }

    /// A short description of the update state, for Settings.
    pub fn summary(&self) -> String {
        let current = Version::current();
        match &self.status {
            Status::Idle => format!("You have Poof {current}."),
            Status::Checking => "Checking for updates…".into(),
            Status::UpToDate => format!("Poof {current} is the latest version."),
            Status::Available(r) => format!("Poof {} is available. You have {current}.", r.version),
            Status::Installing(r) => format!("Installing Poof {}…", r.version),
            Status::Failed(e) => format!("Couldn't check for updates: {e}."),
        }
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.status, Status::Checking | Status::Installing(_))
    }

    pub fn has_update(&self) -> bool {
        matches!(self.status, Status::Available(_))
    }

    /// The notice shown in the sidebar while an update is available.
    pub fn notice(&self) -> Option<Element<'_, Message>> {
        let (title, action) = match &self.status {
            Status::Available(r) => (format!("Poof {} is out", r.version), Some(Message::Review)),
            Status::Installing(r) => (format!("Installing {}…", r.version), None),
            _ => return None,
        };
        let content = column![
            row![Icon::Download.view(14).style(style::icon(|t| t.accent)), label(title)]
                .spacing(gap::S)
                .align_y(Alignment::Center),
            caption("A new version is ready to install."),
            space().height(2),
            btn("Update", None, Kind::Primary).on_press_maybe(action).width(Fill),
        ]
        .spacing(gap::XS);
        Some(container(content).padding(gap::M).width(Fill).style(style::banner(Tone::Accent)).into())
    }

    pub fn dialog(&self) -> Option<Dialog<Message>> {
        let Status::Available(release) = &self.status else { return None };
        if !self.confirming {
            return None;
        }
        let mut details = release.highlights(6);
        if details.is_empty() {
            details.push("Bug fixes and improvements.".into());
        }
        Some(Dialog {
            title: format!("Update to Poof {}?", release.version),
            message: format!(
                "Poof will download the update, check its signature and restart. You have {}.",
                Version::current()
            ),
            details,
            confirm: "Install and Restart".into(),
            destructive: false,
            on_confirm: Message::Install,
            on_cancel: Message::Cancel,
        })
    }
}

fn toast_if<M>(show: bool, message: String, tone: Tone) -> Action<M> {
    if show { Action::event(Event::Toast(message, tone)) } else { Action::none() }
}

/// Runs a check at startup, if enabled.
pub fn startup_check(enabled: bool) -> Task<Message> {
    if enabled { Task::done(Message::Check { manual: false }) } else { Task::none() }
}
