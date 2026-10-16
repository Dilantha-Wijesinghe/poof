//! Clean Up: reviewable, rule-based cleanup of caches, logs and build junk.
//!
//! Nothing is removed without an explicit selection and confirmation.
//! "Safe" items are preselected because they regenerate on their own;
//! "Review" items never are. Data of apps that are open is left alone until
//! they are quit, since removing it under a running app can confuse it.

use std::collections::HashSet;
use std::path::PathBuf;

use iced::widget::{button, checkbox, column, container, row, space, text};
use iced::{Alignment, Element, Fill, Task};
use poof_core::apps::{self, App};
use poof_core::cleanup::{self, CATALOG, Candidate, Category, Finding, Safety};
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
    /// Apps open when the user last asked to clean.
    open_apps: Vec<App>,
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

    fn selected_items(&self) -> impl Iterator<Item = (&Finding, &Candidate)> {
        self.findings
            .iter()
            .flatten()
            .flat_map(|f| f.items.iter().map(move |c| (f, c)))
            .filter(|(_, c)| self.selected.contains(&c.path))
    }

    /// The open app that `candidate` belongs to.
    fn open_owner(&self, candidate: &Candidate) -> Option<&App> {
        apps::owner(&candidate.path, &self.open_apps)
    }

    /// Whether `candidate` is removed by the next clean-up.
    fn will_clean(&self, candidate: &Candidate) -> bool {
        self.selected.contains(&candidate.path) && self.open_owner(candidate).is_none()
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
                self.open_apps = apps::running();
                self.confirming = self.selected_bytes() > 0;
                Action::none()
            }
            Message::Cancel => {
                self.confirming = false;
                Action::none()
            }
            Message::Confirm => {
                // An app may have been opened while the dialog was up.
                self.open_apps = apps::running();
                if !self.selected_items().any(|(_, c)| self.will_clean(c)) {
                    return Action::none();
                }
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
                            .filter(|c| self.will_clean(c))
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
        let held = totals(
            self.selected_items().filter_map(|(_, c)| Some((self.open_owner(c)?.name.as_str(), c.size))),
        );
        let cleaning: Vec<(&Finding, &Candidate)> =
            self.selected_items().filter(|(_, c)| self.will_clean(c)).collect();
        let open = and_list(held.iter().map(|(name, _)| *name));
        let (them, are) = if held.len() == 1 { ("it", "is") } else { ("them", "are") };

        if cleaning.is_empty() {
            return Some(Dialog {
                title: format!("Quit {open} first"),
                message: format!(
                    "Everything you selected belongs to {open}. Removing an app's data while it's \
                     open can make it misbehave. Quit {them}, then check again."
                ),
                details: listing(&held),
                confirm: "Check Again".into(),
                destructive: false,
                on_confirm: Message::Clean,
                on_cancel: Message::Cancel,
            });
        }

        let per_rule = totals(cleaning.iter().map(|(f, c)| (f.rule.name, c.size)));
        let mut details = listing(&per_rule);
        if cleaning.iter().any(|(f, _)| f.rule.safety == Safety::Review) {
            details.push("Some items are marked Review. Make sure you don't need them.".into());
        }
        if !held.is_empty() {
            let bytes = ByteSize(held.iter().map(|(_, b)| b).sum());
            details.push(format!(
                "Leaving out {bytes} that belongs to {open}, which {are} open. Quit {them} and clean again to include it."
            ));
        }
        Some(Dialog {
            title: format!("Clean up {}?", ByteSize(cleaning.iter().map(|(_, c)| c.size).sum())),
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

/// Sums sizes per name, largest first.
fn totals<'a>(entries: impl Iterator<Item = (&'a str, u64)>) -> Vec<(&'a str, u64)> {
    let mut totals: Vec<(&str, u64)> = Vec::new();
    for (name, bytes) in entries {
        match totals.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 += bytes,
            None => totals.push((name, bytes)),
        }
    }
    totals.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    totals
}

/// Dialog lines for `totals`, at most six of them.
fn listing(totals: &[(&str, u64)]) -> Vec<String> {
    let mut lines: Vec<String> =
        totals.iter().take(6).map(|(name, bytes)| format!("{name}: {}", ByteSize(*bytes))).collect();
    if totals.len() > 6 {
        lines.push(format!("and {} more", totals.len() - 6));
    }
    lines
}

/// `["A", "B", "C"]` → `"A, B and C"`.
fn and_list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let names: Vec<&str> = names.collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => names.concat(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state with one "user-caches" item per path, all selected, and Slack open.
    fn state(caches: &[&str]) -> State {
        let rule = CATALOG.iter().find(|r| r.id == "user-caches").unwrap();
        let items: Vec<Candidate> = caches
            .iter()
            .map(|name| Candidate {
                path: paths::expand(&format!("~/Library/Caches/{name}")),
                size: 1_000_000,
                modified: 0,
            })
            .collect();
        State {
            selected: items.iter().map(|c| c.path.clone()).collect(),
            findings: Some(vec![Finding { rule, items, unreadable: false }]),
            open_apps: vec![App {
                name: "Slack".into(),
                id: "com.tinyspeck.slackmacgap".into(),
                path: "/Applications/Slack.app".into(),
            }],
            confirming: true,
            ..State::default()
        }
    }

    #[test]
    fn open_apps_data_is_left_out() {
        let dialog = state(&["com.tinyspeck.slackmacgap", "com.example.other"]).dialog().unwrap();
        assert_eq!(dialog.title, "Clean up 1.00 MB?");
        assert!(
            dialog
                .details
                .last()
                .unwrap()
                .starts_with("Leaving out 1.00 MB that belongs to Slack, which is open.")
        );
    }

    #[test]
    fn asks_to_quit_when_only_open_apps_data_is_selected() {
        let dialog = state(&["com.tinyspeck.slackmacgap"]).dialog().unwrap();
        assert_eq!(dialog.title, "Quit Slack first");
        assert_eq!(dialog.confirm, "Check Again");
        assert!(!dialog.destructive);
    }

    #[test]
    fn totals_group_and_sort() {
        let entries = [("a", 1), ("b", 5), ("a", 7)];
        assert_eq!(totals(entries.into_iter()), [("a", 8), ("b", 5)]);
    }

    #[test]
    fn names_are_listed_in_prose() {
        assert_eq!(and_list([].into_iter()), "");
        assert_eq!(and_list(["Slack"].into_iter()), "Slack");
        assert_eq!(and_list(["Slack", "Chrome", "Edge"].into_iter()), "Slack, Chrome and Edge");
    }
}
