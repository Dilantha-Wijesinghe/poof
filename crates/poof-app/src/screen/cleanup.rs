//! Clean Up: reviewable, rule-based cleanup of caches, logs and build junk.
//!
//! Nothing is removed without an explicit selection and confirmation.
//! "Safe" items are preselected because they regenerate on their own;
//! "Review" items never are.

use std::collections::HashSet;
use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, row, space, text};
use iced::{Alignment, Element, Fill, Task};
use poof_core::cleanup::{self, CATALOG, Category, Finding, Safety};
use poof_core::remove::{self, Item, Method};
use poof_core::settings::Settings;
use poof_core::size::ByteSize;
use poof_core::{paths, system as mac, time};

use super::{Action, Event, Shared};
use crate::icon::Icon;
use crate::style::{self, Kind, Tone, size, space as gap};
use crate::widget::{
    Dialog, badge, body, btn, caption, card, divider, empty_state, icon_button, label, meter, muted,
    page_header, page_scroll, section, single_line,
};
use crate::{font, task};

/// Items listed per rule when expanded.
const MAX_ITEMS: usize = 100;

#[derive(Debug, Default)]
pub struct State {
    findings: Option<Vec<Finding>>,
    analyzing: bool,
    selected: HashSet<PathBuf>,
    expanded: HashSet<&'static str>,
    confirming: bool,
    cleaning: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Analyze,
    Analyzed(Vec<Finding>),
    ToggleRule(&'static str, bool),
    ToggleItem(PathBuf, bool),
    Expand(&'static str),
    Clean,
    Confirm,
    Cancel,
    Cleaned { bytes: u64, trashed: bool, failed: usize },
    Reveal(PathBuf),
    GrantAccess,
}

impl State {
    pub fn new(settings: &Settings) -> (Self, Task<Message>) {
        let mut state = Self::default();
        let task = state.refresh(settings);
        (state, task)
    }

    /// Re-evaluates every rule in the background.
    pub fn refresh(&mut self, settings: &Settings) -> Task<Message> {
        self.analyzing = true;
        let options = cleanup::Options { stale_project_days: settings.stale_project_days };
        task::blocking(move || cleanup::find(CATALOG, options), Message::Analyzed)
    }

    /// Bytes that can be reclaimed without any judgement call (Safe rules).
    pub fn reclaimable(&self) -> Option<u64> {
        self.findings
            .as_ref()
            .map(|f| f.iter().filter(|f| f.rule.safety == Safety::Safe).map(Finding::total).sum())
    }

    fn selected_bytes(&self) -> u64 {
        self.selected_items().map(|(_, c)| c.size).sum()
    }

    fn selected_items(&self) -> impl Iterator<Item = (&Finding, &cleanup::Candidate)> {
        self.findings
            .iter()
            .flatten()
            .flat_map(|f| f.items.iter().map(move |c| (f, c)))
            .filter(|(_, c)| self.selected.contains(&c.path))
    }

    fn finding(&self, id: &str) -> Option<&Finding> {
        self.findings.as_ref()?.iter().find(|f| f.rule.id == id)
    }

    pub fn update(&mut self, message: Message, settings: &Settings) -> Action<Message> {
        match message {
            Message::Analyze => self.refresh(settings).into(),
            Message::Analyzed(findings) => {
                self.analyzing = false;
                // Preselect only what is safe to remove without looking.
                self.selected = findings
                    .iter()
                    .filter(|f| f.rule.safety == Safety::Safe)
                    .flat_map(|f| f.items.iter().map(|c| c.path.clone()))
                    .collect();
                self.findings = Some(findings);
                Action::none()
            }
            Message::ToggleRule(id, on) => {
                let paths: Vec<PathBuf> = self
                    .finding(id)
                    .map(|f| f.items.iter().map(|c| c.path.clone()).collect())
                    .unwrap_or_default();
                for path in paths {
                    if on {
                        self.selected.insert(path);
                    } else {
                        self.selected.remove(&path);
                    }
                }
                Action::none()
            }
            Message::ToggleItem(path, on) => {
                if on {
                    self.selected.insert(path);
                } else {
                    self.selected.remove(&path);
                }
                Action::none()
            }
            Message::Expand(id) => {
                if !self.expanded.remove(id) {
                    self.expanded.insert(id);
                }
                Action::none()
            }
            Message::Clean => {
                self.confirming = self.selected_bytes() > 0;
                Action::none()
            }
            Message::Cancel => {
                self.confirming = false;
                Action::none()
            }
            Message::Confirm => {
                self.confirming = false;
                self.cleaning = true;
                let to_trash = settings.cleanup_to_trash;
                let batches: Vec<(Method, Vec<Item>)> = self
                    .findings
                    .iter()
                    .flatten()
                    .map(|f| {
                        let method =
                            if to_trash && !f.rule.permanent { Method::Trash } else { Method::Delete };
                        let items = f
                            .items
                            .iter()
                            .filter(|c| self.selected.contains(&c.path))
                            .map(|c| Item { path: c.path.clone(), size: c.size })
                            .collect();
                        (method, items)
                    })
                    .collect();
                task::blocking(
                    move || {
                        let outcomes: Vec<_> =
                            batches.iter().map(|(method, items)| remove::remove(items, *method)).collect();
                        (
                            outcomes.iter().map(|o| o.bytes).sum(),
                            outcomes.iter().map(|o| o.failed.len()).sum(),
                        )
                    },
                    move |(bytes, failed)| Message::Cleaned { bytes, trashed: to_trash, failed },
                )
                .into()
            }
            Message::Cleaned { bytes, trashed, failed } => {
                self.cleaning = false;
                // The app refreshes findings in response to this event.
                Action::event(Event::Removed { bytes, trashed, failed })
            }
            Message::Reveal(path) => {
                let _ = mac::reveal_in_finder(&path);
                Action::none()
            }
            Message::GrantAccess => {
                let _ = mac::open_full_disk_access_settings();
                Action::none()
            }
        }
    }

    pub fn dialog(&self) -> Option<Dialog<Message>> {
        if !self.confirming {
            return None;
        }
        let mut per_rule: Vec<(&str, u64)> = Vec::new();
        let mut needs_review = false;
        for (finding, candidate) in self.selected_items() {
            needs_review |= finding.rule.safety == Safety::Review;
            match per_rule.iter_mut().find(|(name, _)| *name == finding.rule.name) {
                Some(entry) => entry.1 += candidate.size,
                None => per_rule.push((finding.rule.name, candidate.size)),
            }
        }
        per_rule.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
        let mut details: Vec<String> =
            per_rule.iter().take(6).map(|(name, bytes)| format!("{name}: {}", ByteSize(*bytes))).collect();
        if per_rule.len() > 6 {
            details.push(format!("and {} more", per_rule.len() - 6));
        }
        if needs_review {
            details.push("Some items are marked Review. Make sure you don't need them.".into());
        }
        Some(Dialog {
            title: format!("Clean up {}?", ByteSize(self.selected_bytes())),
            message: "Apps recreate caches and build files when they need them. \
                      Each removed item is listed in ~/Library/Logs/Poof/operations.log."
                .into(),
            details,
            confirm: "Clean Up".into(),
            destructive: true,
            on_confirm: Message::Confirm,
            on_cancel: Message::Cancel,
        })
    }

    pub fn view<'a>(&'a self, shared: &'a Shared) -> Element<'a, Message> {
        let selected = self.selected_bytes();
        let clean_label = if self.cleaning { "Cleaning…" } else { "Clean Up" };
        let actions = row![
            btn("Rescan", Some(Icon::Refresh), Kind::Secondary)
                .on_press_maybe((!self.analyzing).then_some(Message::Analyze)),
            btn(clean_label, Some(Icon::Sparkles), Kind::Primary).on_press_maybe(
                (selected > 0 && !self.cleaning && !self.analyzing).then_some(Message::Clean)
            ),
        ]
        .spacing(gap::S);
        let header =
            page_header("Clean Up", "Clear out caches, logs and other files your Mac doesn't need.", actions);

        let Some(findings) = &self.findings else {
            return column![
                container(header).padding([8, 32]),
                empty_state::<Message>(
                    Icon::Search,
                    "Looking for files you can remove…",
                    "This usually takes a few seconds.",
                    None
                ),
            ]
            .into();
        };
        if findings.is_empty() {
            return column![
                container(header).padding([8, 32]),
                empty_state(
                    Icon::Check,
                    "Nothing to clean",
                    "Poof didn't find any caches or build files worth removing.",
                    None
                ),
            ]
            .into();
        }

        let found: u64 = findings.iter().map(Finding::total).sum();
        let mode = if shared.settings.cleanup_to_trash {
            "Selected items go to the Trash. You can change this in Settings."
        } else {
            "Selected items are deleted right away instead of going to the Trash. You can change this in Settings."
        };
        let summary = card(
            column![
                row![
                    text(format!("{} selected", ByteSize(selected))).size(size::HEADING).font(font::SEMIBOLD),
                    muted(format!("of {} found", ByteSize(found))),
                    space::horizontal(),
                    badge("Safe", Tone::Success),
                    caption("rebuilds automatically"),
                    badge("Review", Tone::Warning),
                    caption("check before removing"),
                ]
                .spacing(gap::S)
                .align_y(Alignment::Center),
                meter(selected as f32 / found.max(1) as f32, 8.0),
                caption(mode),
            ]
            .spacing(gap::M),
        );

        let mut content = column![header, summary].spacing(gap::XL);
        for category in Category::ALL {
            let group: Vec<&Finding> = findings.iter().filter(|f| f.rule.category == category).collect();
            if group.is_empty() {
                continue;
            }
            let rules = group.iter().enumerate().fold(column![].spacing(gap::M), |col, (i, f)| {
                let col = if i > 0 { col.push(divider()) } else { col };
                col.push(self.rule_view(f))
            });
            content = content.push(column![section(category.label()), card(rules)].spacing(gap::S));
        }
        page_scroll(content)
    }

    fn rule_view<'a>(&'a self, finding: &'a Finding) -> Element<'a, Message> {
        let rule = finding.rule;
        let chosen = finding.items.iter().filter(|c| self.selected.contains(&c.path)).count();
        let all = chosen == finding.items.len() && chosen > 0;
        let (tone, safety) = match rule.safety {
            Safety::Safe => (Tone::Success, "Safe"),
            Safety::Review => (Tone::Warning, "Review"),
        };
        let expanded = self.expanded.contains(rule.id);

        let trailing: Element<'a, Message> = if finding.items.is_empty() && finding.unreadable {
            btn("Needs Full Disk Access", Some(Icon::Lock), Kind::Secondary)
                .on_press(Message::GrantAccess)
                .into()
        } else {
            let selection =
                (chosen > 0 && !all).then(|| caption(format!("{chosen} of {}", finding.items.len())));
            row![]
                .push(selection)
                .push(text(ByteSize(finding.total()).to_string()).size(size::LABEL).font(font::SEMIBOLD))
                .push(
                    button(if expanded { Icon::X.view(14) } else { Icon::ChevronRight.view(14) })
                        .padding(6)
                        .style(style::button(Kind::Ghost))
                        .on_press(Message::Expand(rule.id)),
                )
                .spacing(gap::M)
                .align_y(Alignment::Center)
                .into()
        };

        let header = row![
            checkbox(all)
                .on_toggle_maybe(
                    (!finding.items.is_empty()).then_some(move |on| Message::ToggleRule(rule.id, on))
                )
                .style(style::checkbox),
            column![
                row![label(rule.name), badge(safety, tone)].spacing(gap::S).align_y(Alignment::Center),
                caption(rule.description),
            ]
            .spacing(3)
            .width(Fill),
            trailing,
        ]
        .spacing(gap::M)
        .align_y(Alignment::Center);

        if !expanded || finding.items.is_empty() {
            return header.into();
        }
        let items = finding.items.iter().take(MAX_ITEMS).fold(column![].spacing(2), |col, c| {
            let on = self.selected.contains(&c.path);
            let path = c.path.clone();
            col.push(
                row![
                    checkbox(on)
                        .on_toggle(move |v| Message::ToggleItem(path.clone(), v))
                        .style(style::checkbox)
                        .size(14),
                    single_line(body(paths::display(&c.path))),
                    caption(time::relative(c.modified)),
                    text(ByteSize(c.size).to_string())
                        .size(size::BODY)
                        .font(font::MEDIUM)
                        .width(80)
                        .align_x(Alignment::End),
                    icon_button(Icon::ExternalLink, "Show in Finder", Some(Message::Reveal(c.path.clone()))),
                ]
                .spacing(gap::M)
                .align_y(Alignment::Center),
            )
        });
        let more = (finding.items.len() > MAX_ITEMS)
            .then(|| caption(format!("and {} smaller items", finding.items.len() - MAX_ITEMS)));
        let list = container(items.push(more)).padding([8, 12]).width(Fill).style(style::well);
        column![header, row![space().width(30), list]].spacing(gap::M).into()
    }
}
