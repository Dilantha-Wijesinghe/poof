//! Explore: scan a location, then drill into it with a treemap and a sorted
//! list. Items can be revealed in Finder or moved to the Trash.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::{button, canvas, column, container, progress_bar, row, scrollable, space, text};
use iced::{Alignment, Element, Fill, FillPortion, Length, Padding, Subscription, Task, time};
use poof_core::history::{self, Summary};
use poof_core::remove::{self, Item, Method};
use poof_core::scan::{self, NodeId, NodeKind, Progress, Tree};
use poof_core::size::{ByteSize, signed};
use poof_core::{Error, paths, system as mac};

use super::{Action, Event, Shared};
use crate::icon::Icon;
use crate::style::{self, Kind, Tone, size, space as gap};
use crate::widget::treemap::{self, Treemap};
use crate::widget::{
    Dialog, badge, banner, body, btn, caption, card, heading, icon_button, muted, page_header, single_line,
};
use crate::{font, task};

/// Children listed beside the treemap.
const MAX_ROWS: usize = 300;
/// Smallest change worth flagging next to an item.
const DELTA_BADGE: u64 = 100 * 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Home,
    Disk,
    Folder(PathBuf),
}

impl Target {
    pub fn root(&self) -> PathBuf {
        match self {
            Target::Home => paths::home(),
            Target::Disk => PathBuf::from(paths::DATA_VOLUME),
            Target::Folder(path) => path.clone(),
        }
    }

    fn label(&self) -> String {
        match self {
            Target::Home => "Home".into(),
            Target::Disk => "Macintosh HD".into(),
            Target::Folder(path) => path
                .file_name()
                .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
        }
    }
}

#[derive(Debug, Default)]
pub struct State {
    phase: Phase,
}

#[derive(Debug, Default)]
enum Phase {
    #[default]
    Idle,
    Scanning(Scanning),
    Ready(Box<Browser>),
}

#[derive(Debug)]
struct Scanning {
    target: Target,
    progress: Arc<Progress>,
    started: Instant,
    /// Expected total bytes, from the last scan or the disk's used space.
    estimate: Option<u64>,
    /// Navigate here once the scan completes.
    then_open: Option<PathBuf>,
}

#[derive(Debug)]
struct Browser {
    target: Target,
    tree: Arc<Tree>,
    previous: Option<Arc<Summary>>,
    current: NodeId,
    hovered: Option<NodeId>,
    selected: Option<NodeId>,
    pending: Option<NodeId>,
    removing: bool,
}

/// A finished scan, cheap to clone through messages.
#[derive(Debug, Clone)]
pub struct Scanned {
    tree: Arc<Tree>,
    previous: Option<Arc<Summary>>,
}

#[derive(Debug, Clone)]
pub enum ScanError {
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Start(Target),
    ChooseFolder,
    FolderChosen(Option<PathBuf>),
    Tick,
    Cancel,
    Finished(Result<Scanned, ScanError>),
    Treemap(treemap::Event),
    Open(NodeId),
    Up,
    Select(NodeId),
    Reveal(NodeId),
    AskRemove(NodeId),
    ConfirmRemove,
    CancelRemove,
    Removed(NodeId, u64, Option<String>),
    NewScan,
    GrantAccess,
}

impl State {
    pub fn subscription(&self) -> Subscription<Message> {
        match self.phase {
            Phase::Scanning(_) => time::every(Duration::from_millis(100)).map(|_| Message::Tick),
            _ => Subscription::none(),
        }
    }

    /// Shows `path` — navigating within the current scan when possible,
    /// otherwise scanning it.
    pub fn open_path(&mut self, path: &Path) -> Task<Message> {
        if let Phase::Ready(browser) = &mut self.phase
            && let Ok(relative) = path.strip_prefix(browser.tree.root_path())
            && let Some(id) = browser.tree.find(relative)
        {
            browser.reveal(id);
            return Task::none();
        }
        let target = if path == paths::home() {
            Target::Home
        } else if path == Path::new(paths::DATA_VOLUME) {
            Target::Disk
        } else if path.is_dir() {
            Target::Folder(path.to_path_buf())
        } else {
            Target::Folder(path.parent().map_or_else(paths::home, Path::to_path_buf))
        };
        self.start(target, None, Some(path.to_path_buf()))
    }

    fn start(&mut self, target: Target, used: Option<u64>, then_open: Option<PathBuf>) -> Task<Message> {
        let root = target.root();
        let estimate =
            history::latest(&root).map(|s| s.total).or(if target == Target::Disk { used } else { None });
        let progress = Arc::new(Progress::default());
        self.phase = Phase::Scanning(Scanning {
            target,
            progress: progress.clone(),
            started: Instant::now(),
            estimate,
            then_open,
        });
        task::blocking(move || run_scan(&root, &progress), Message::Finished)
    }

    pub fn update(&mut self, message: Message, shared: &Shared) -> Action<Message> {
        match message {
            Message::Start(target) => self.start(target, shared.volume.map(|v| v.used()), None).into(),
            Message::ChooseFolder => {
                task::blocking(|| mac::choose_folder("Choose a folder to scan"), Message::FolderChosen).into()
            }
            Message::FolderChosen(Some(folder)) => self.start(Target::Folder(folder), None, None).into(),
            Message::FolderChosen(None) | Message::Tick => Action::none(),
            Message::Cancel => {
                if let Phase::Scanning(s) = &self.phase {
                    s.progress.cancel();
                }
                Action::none()
            }
            Message::Finished(result) => self.finished(result),
            Message::NewScan => {
                self.phase = Phase::Idle;
                Action::none()
            }
            Message::GrantAccess => {
                let _ = mac::open_full_disk_access_settings();
                Action::none()
            }
            message => match &mut self.phase {
                Phase::Ready(browser) => browser.update(message),
                _ => Action::none(),
            },
        }
    }

    fn finished(&mut self, result: Result<Scanned, ScanError>) -> Action<Message> {
        let Phase::Scanning(scanning) = std::mem::take(&mut self.phase) else {
            return Action::none();
        };
        match result {
            Ok(Scanned { tree, previous }) => {
                let current = tree.root();
                let mut browser = Browser {
                    target: scanning.target,
                    tree,
                    previous,
                    current,
                    hovered: None,
                    selected: None,
                    pending: None,
                    removing: false,
                };
                if let Some(path) = scanning.then_open
                    && let Ok(relative) = path.strip_prefix(browser.tree.root_path())
                    && let Some(id) = browser.tree.find(relative)
                {
                    browser.reveal(id);
                }
                self.phase = Phase::Ready(Box::new(browser));
                Action::event(Event::Scanned)
            }
            Err(ScanError::Cancelled) => Action::none(),
            Err(ScanError::Failed(e)) => {
                Action::event(Event::Toast(format!("Scan failed: {e}"), Tone::Danger))
            }
        }
    }

    pub fn dialog(&self) -> Option<Dialog<Message>> {
        let Phase::Ready(browser) = &self.phase else { return None };
        let id = browser.pending?;
        let node = browser.tree.get(id);
        Some(Dialog {
            title: format!("Move “{}” to the Trash?", node.name),
            message: format!("{} · {}", ByteSize(node.size), paths::display(&browser.tree.path(id))),
            details: vec![
                "You can restore it from the Trash until you empty it.".into(),
                "You get the space back when you empty the Trash.".into(),
            ],
            confirm: "Move to Trash".into(),
            destructive: false,
            on_confirm: Message::ConfirmRemove,
            on_cancel: Message::CancelRemove,
        })
    }

    pub fn view<'a>(&'a self, shared: &'a Shared) -> Element<'a, Message> {
        match &self.phase {
            Phase::Idle => idle_view(shared),
            Phase::Scanning(scanning) => scanning_view(scanning),
            Phase::Ready(browser) => browser.view(),
        }
    }
}

fn run_scan(root: &Path, progress: &Progress) -> Result<Scanned, ScanError> {
    let tree = match scan::scan(root, &scan::Options::default(), progress) {
        Ok(tree) => tree,
        Err(Error::Cancelled) => return Err(ScanError::Cancelled),
        Err(e) => return Err(ScanError::Failed(e.to_string())),
    };
    let summary = Summary::from_tree(&tree);
    let previous = history::previous(root, summary.taken_at);
    // History is a nice-to-have; a write failure must not fail the scan.
    let _ = history::save(&summary);
    Ok(Scanned { tree: Arc::new(tree), previous: previous.map(Arc::new) })
}

// ------------------------------------------------------------------ browsing

impl Browser {
    fn update(&mut self, message: Message) -> Action<Message> {
        match message {
            Message::Treemap(treemap::Event::Hover(id)) => self.hovered = id,
            Message::Treemap(treemap::Event::Open(id)) | Message::Open(id) => self.open(id),
            Message::Up => {
                if let Some(parent) = self.tree.get(self.current).parent {
                    self.open(parent);
                }
            }
            Message::Select(id) => self.selected = Some(id),
            Message::Reveal(id) => {
                let _ = mac::reveal_in_finder(&self.tree.path(id));
            }
            Message::AskRemove(id) => {
                if let Err(e) = remove::guard(&self.tree.path(id)) {
                    return Action::event(Event::Toast(e.to_string(), Tone::Warning));
                }
                self.pending = Some(id);
            }
            Message::CancelRemove => self.pending = None,
            Message::ConfirmRemove => {
                let Some(id) = self.pending.take() else { return Action::none() };
                self.removing = true;
                let item = Item { path: self.tree.path(id), size: self.tree.get(id).size };
                return task::blocking(
                    move || remove::remove(std::slice::from_ref(&item), Method::Trash),
                    move |outcome| {
                        Message::Removed(id, outcome.bytes, outcome.failed.first().map(|f| f.1.clone()))
                    },
                )
                .into();
            }
            Message::Removed(id, bytes, error) => {
                self.removing = false;
                if error.is_none() {
                    Arc::make_mut(&mut self.tree).remove(id);
                    self.selected = None;
                    self.hovered = None;
                }
                return Action::event(Event::Removed {
                    bytes,
                    trashed: true,
                    failed: usize::from(error.is_some()),
                });
            }
            _ => {}
        }
        Action::none()
    }

    fn open(&mut self, id: NodeId) {
        if self.tree.get(id).is_dir() {
            self.current = id;
            self.selected = None;
            self.hovered = None;
        }
    }

    /// Navigates to the folder containing `id` (or `id` itself if it is a
    /// folder) and selects it.
    fn reveal(&mut self, id: NodeId) {
        let node = self.tree.get(id);
        if node.is_dir() && !node.children.is_empty() {
            self.open(id);
        } else if let Some(parent) = node.parent {
            self.open(parent);
            self.selected = Some(id);
        }
    }

    /// Change in size since the previous scan, when it's significant.
    fn delta(&self, id: NodeId) -> Option<i64> {
        let previous = self.previous.as_ref()?;
        let node = self.tree.get(id);
        if node.kind == NodeKind::Grouped {
            return None;
        }
        let key = self.tree.relative_path(id).to_string_lossy().into_owned();
        let before = if let Some(size) = previous.size_of(&key) {
            size
        } else {
            // Not recorded last time: it was below the threshold (if its parent was recorded).
            let parent = node.parent.map(|p| self.tree.relative_path(p).to_string_lossy().into_owned())?;
            previous.size_of(&parent)?;
            0
        };
        let delta = node.size as i64 - before as i64;
        (delta.unsigned_abs() >= DELTA_BADGE).then_some(delta)
    }

    fn view(&self) -> Element<'_, Message> {
        let tree = &*self.tree;
        let current = tree.get(self.current);

        let crumbs =
            tree.lineage(self.current).into_iter().enumerate().fold(row![].spacing(2), |r, (i, id)| {
                let name = if i == 0 { self.target.label() } else { tree.get(id).name.to_string() };
                let r = if i > 0 {
                    r.push(Icon::ChevronRight.view(12).style(style::icon(|t| t.faint)))
                } else {
                    r
                };
                let last = id == self.current;
                let crumb = text(name).size(if last { size::HEADING } else { size::LABEL }).font(if last {
                    font::SEMIBOLD
                } else {
                    font::MEDIUM
                });
                r.push(
                    button(crumb)
                        .padding([4, 8])
                        .style(style::button(Kind::Ghost))
                        .on_press(Message::Open(id)),
                )
            });
        let crumbs = row![crumbs].align_y(Alignment::Center);
        let actions = row![
            icon_button(Icon::ArrowLeft, "Up one level", current.parent.map(|_| Message::Up)),
            btn("Rescan", Some(Icon::Refresh), Kind::Secondary).on_press(Message::Start(self.target.clone())),
            btn("New Scan", Some(Icon::Scan), Kind::Secondary).on_press(Message::NewScan),
        ]
        .spacing(gap::S)
        .align_y(Alignment::Center);

        let unreadable = (tree.unreadable > 0).then(|| {
            button(
                row![
                    Icon::Lock.view(13).style(style::icon_tone(Tone::Warning)),
                    caption(format!("Couldn't read {} folders. Click to give access.", tree.unreadable)),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([2, 6])
            .style(style::button(Kind::Ghost))
            .on_press(Message::GrantAccess)
        });
        let stats = row![caption(format!(
            "{} · {} files · scanned {}",
            ByteSize(current.size),
            group_digits(current.files),
            poof_core::time::relative(tree.scanned_at)
        ))]
        .push(
            self.previous
                .as_ref()
                .map(|p| caption(format!("· compared with {}", poof_core::time::date(p.taken_at)))),
        )
        .push(space::horizontal())
        .push(unreadable)
        .spacing(gap::S)
        .align_y(Alignment::Center);

        let map =
            canvas(Treemap { tree, node: self.current, selected: self.selected }).width(Fill).height(Fill);
        let map: Element<'_, Message> = Element::from(map).map(Message::Treemap);

        let body_row = row![
            container(map).padding(8).width(FillPortion(3)).height(Fill).style(style::card),
            container(self.list()).padding([8, 4]).width(FillPortion(2)).height(Fill).style(style::card),
        ]
        .spacing(gap::L)
        .height(Fill);

        column![
            row![crumbs, space::horizontal(), actions].align_y(Alignment::Center),
            stats,
            body_row,
            self.status_bar(),
        ]
        .spacing(gap::M)
        .padding(Padding { top: 4.0, right: 32.0, bottom: 20.0, left: 28.0 })
        .into()
    }

    fn list(&self) -> Element<'_, Message> {
        let tree = &*self.tree;
        let parent_size = tree.get(self.current).size.max(1);
        let rows = tree.children(self.current).take(MAX_ROWS).enumerate().fold(
            column![].spacing(1),
            |col, (i, (id, node))| {
                let swatch_color = node.kind != NodeKind::Grouped;
                let dot = container(space()).width(8).height(8).style(move |theme| {
                    let t = style::tokens(theme);
                    let color = if swatch_color { t.series[i % t.series.len()] } else { t.faint };
                    style::swatch(color, 4.0)(theme)
                });
                let icon = match node.kind {
                    NodeKind::Directory => Icon::Folder,
                    NodeKind::File => Icon::File,
                    NodeKind::Grouped => Icon::FileBox,
                };
                let delta = self
                    .delta(id)
                    .map(|d| badge(signed(d), if d > 0 { Tone::Warning } else { Tone::Success }));
                let share = node.size as f32 / parent_size as f32;
                let line = row![
                    dot,
                    icon.view(15),
                    column![
                        single_line(text(truncate(&node.name, 60)).size(size::BODY).font(font::MEDIUM)),
                        caption(format!("{:.1}%", share * 100.0)),
                    ]
                    .spacing(1)
                    .width(Fill),
                ]
                .push(delta)
                .push(text(ByteSize(node.size).to_string()).size(size::BODY).font(font::SEMIBOLD))
                .spacing(gap::S)
                .align_y(Alignment::Center);

                let opens = node.is_dir() && !node.children.is_empty();
                let press = if opens { Message::Open(id) } else { Message::Select(id) };
                let item = button(line)
                    .width(Fill)
                    .padding([6, 8])
                    .style(style::row_button(self.selected == Some(id)))
                    .on_press(press);
                let actions = node.has_path().then(|| {
                    row![
                        icon_button(Icon::ExternalLink, "Show in Finder", Some(Message::Reveal(id))),
                        icon_button(
                            Icon::Trash,
                            "Move to Trash",
                            (!self.removing).then_some(Message::AskRemove(id))
                        ),
                    ]
                    .spacing(0)
                });
                col.push(row![item].push(actions).spacing(2).align_y(Alignment::Center))
            },
        );
        let empty = tree.get(self.current).children.is_empty().then(|| muted("This folder is empty."));
        scrollable(column![rows].push(empty).padding([0, 8])).style(style::scrollable).height(Fill).into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let focus = self.hovered.or(self.selected);
        let content: Element<'_, Message> = match focus {
            Some(id) => {
                let tree = &*self.tree;
                let node = tree.get(id);
                let parent = tree.get(self.current).size.max(1);
                let path = if node.has_path() {
                    paths::display(&tree.path(id))
                } else {
                    "Grouped small files".into()
                };
                row![
                    body(node.name.to_string()).font(font::SEMIBOLD),
                    caption(format!(
                        "{} · {:.1}% of this folder",
                        ByteSize(node.size),
                        node.size as f32 / parent as f32 * 100.0
                    )),
                    single_line(caption(path).align_x(Alignment::End)),
                ]
                .spacing(gap::M)
                .align_y(Alignment::Center)
                .into()
            }
            None => caption("Hover over the map for details. Click a folder to open it.").into(),
        };
        container(content).height(22).align_y(Alignment::Center).into()
    }
}

// -------------------------------------------------------------- other phases

fn idle_view(shared: &Shared) -> Element<'_, Message> {
    let tile =
        |icon: Icon, title: &'static str, description: &'static str, meta: String, message: Message| {
            let mark = container(icon.view(22).style(style::icon(|t| t.accent)))
                .padding(10)
                .style(style::badge(Tone::Accent));
            button(
                column![mark, heading(title), muted(description), space::vertical(), caption(meta)]
                    .spacing(gap::S),
            )
            .padding(gap::XL)
            .width(FillPortion(1))
            .height(210)
            .style(style::tile)
            .on_press(message)
        };
    let last_scan = |target: &Target| {
        history::list(&target.root()).first().map_or_else(
            || "Not scanned yet".to_owned(),
            |&t| format!("Last scanned {}", poof_core::time::relative(t)),
        )
    };
    let used = shared.volume.map_or_else(String::new, |v| format!(" · {} used", ByteSize(v.used())));

    let tiles = row![
        tile(
            Icon::Folder,
            "Home folder",
            "Your files, app data and caches. Quick to scan and a good place to start.",
            last_scan(&Target::Home),
            Message::Start(Target::Home),
        ),
        tile(
            Icon::Disk,
            "Macintosh HD",
            "The whole disk, including other users' files and system data. Takes longer.",
            format!("{}{used}", last_scan(&Target::Disk)),
            Message::Start(Target::Disk),
        ),
        tile(
            Icon::FolderOpen,
            "Choose folder…",
            "Scan any folder or external drive.",
            String::new(),
            Message::ChooseFolder,
        ),
    ]
    .spacing(gap::L);

    let access = (!shared.full_disk_access).then(|| {
        banner(
            Icon::Lock,
            Tone::Warning,
            "Some folders can't be read yet",
            "Scans still work, but folders macOS protects are skipped until you give Poof Full Disk Access.",
            Some(btn("Open Settings", None, Kind::Secondary).on_press(Message::GrantAccess).into()),
        )
    });

    column![page_header("Explore", "Scan a folder to see what's taking up space.", space())]
        .push(access)
        .push(tiles)
        .push(caption("Poof keeps a small summary of each scan so it can show you what changed next time."))
        .spacing(gap::XL)
        .padding(Padding { top: 8.0, right: 32.0, bottom: 32.0, left: 32.0 })
        .into()
}

fn scanning_view(scanning: &Scanning) -> Element<'_, Message> {
    let progress = &scanning.progress;
    let bytes = progress.bytes();
    let fraction = scanning.estimate.map(|total| (bytes as f32 / total.max(1) as f32).min(0.99));
    let bar: Element<'_, Message> = match fraction {
        Some(f) => progress_bar(0.0..=1.0, f).girth(8).style(style::progress).into(),
        None => space().height(8).into(),
    };
    let elapsed = scanning.started.elapsed().as_secs();
    let current = paths::display(&progress.current());

    let content = column![
        container(Icon::Scan.view(26).style(style::icon(|t| t.accent)))
            .padding(14)
            .style(style::badge(Tone::Accent)),
        heading(format!("Scanning {}…", scanning.target.label())),
        row![
            column![
                text(group_digits(progress.files())).size(size::TITLE).font(font::BOLD),
                caption("files")
            ]
            .align_x(Alignment::Center),
            column![text(ByteSize(bytes).to_string()).size(size::TITLE).font(font::BOLD), caption("found")]
                .align_x(Alignment::Center),
            column![text(format!("{elapsed}s")).size(size::TITLE).font(font::BOLD), caption("elapsed")]
                .align_x(Alignment::Center),
        ]
        .spacing(gap::XXL),
        bar,
        caption(truncate_start(&current, 72)),
        btn("Cancel", Some(Icon::Stop), Kind::Secondary).on_press(Message::Cancel),
    ]
    .spacing(gap::L)
    .align_x(Alignment::Center)
    .max_width(520);

    container(card(content).padding(gap::XXL).max_width(560)).center(Length::Fill).padding(gap::XXL).into()
}

// ------------------------------------------------------------------- helpers

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let head: String = s.chars().take(max - 1).collect();
    format!("{head}…")
}

fn truncate_start(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_owned();
    }
    let tail: String = s.chars().skip(count - (max - 1)).collect();
    format!("…{tail}")
}

/// `1234567` → `1,234,567`.
fn group_digits(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_digits() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(truncate_start("abcdefghij", 5), "…ghij");
    }
}
