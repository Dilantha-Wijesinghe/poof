//! Fast, parallel disk-usage scanning.
//!
//! Correctness rules (the same ones `du` follows):
//! * Sizes are bytes **allocated on disk** (`st_blocks × 512`), not the
//!   logical length, so sparse files such as `Docker.raw` and iCloud
//!   placeholders are counted for what they really cost.
//! * Hard links are counted once, by `(device, inode)`.
//! * Symlinks are never followed and other file systems are never entered,
//!   which also avoids counting APFS firmlinks twice.
//!
//! To keep memory bounded on large disks, files smaller than
//! [`Options::min_file_size`] are folded into one "smaller files" entry per
//! directory, and directories smaller than that keep no children at all.

mod tree;

pub use tree::{FileKind, Node, NodeId, NodeKind, Tree};

use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

use rayon::prelude::*;

use crate::{Error, Result, time};
use tree::{Builder, RawNode};

#[derive(Debug, Clone)]
pub struct Options {
    /// Files below this size are grouped rather than tracked individually.
    pub min_file_size: u64,
    /// Directories that are never entered.
    pub skip: Vec<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self { min_file_size: 1 << 20, skip: Vec::new() }
    }
}

/// Live counters a UI can poll while a scan runs on another thread, plus a
/// cancellation flag. Share it behind an `Arc`.
#[derive(Debug, Default)]
pub struct Progress {
    files: AtomicU64,
    bytes: AtomicU64,
    cancelled: AtomicBool,
    current: Mutex<PathBuf>,
}

impl Progress {
    pub fn files(&self) -> u64 {
        self.files.load(Relaxed)
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Relaxed)
    }

    /// The directory most recently entered.
    pub fn current(&self) -> PathBuf {
        self.current.lock().map(|p| p.clone()).unwrap_or_default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Relaxed)
    }
}

/// Scans `root` into a [`Tree`]. Blocking and CPU/IO heavy: run it off the UI
/// thread. Returns [`Error::Cancelled`] if `progress.cancel()` was called.
pub fn scan(root: &Path, options: &Options, progress: &Progress) -> Result<Tree> {
    let meta = fs::symlink_metadata(root).map_err(Error::io("read", root))?;
    let walker = Walker::new(&meta, options, progress);
    let name = root.file_name().map_or_else(|| root.to_string_lossy(), |n| n.to_string_lossy());
    let raw = walker.visit(root, name.into(), &meta);
    if progress.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let unreadable = walker.unreadable.load(Relaxed);
    Ok(Builder::build(raw, root.to_path_buf(), time::now(), unreadable))
}

/// Total bytes allocated by `path` and everything below it. Unreadable
/// entries are skipped silently.
pub fn measure(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    let options = Options { min_file_size: u64::MAX, skip: Vec::new() };
    let progress = Progress::default();
    Walker::new(&meta, &options, &progress).visit(path, "".into(), &meta).size
}

struct Walker<'a> {
    device: u64,
    options: &'a Options,
    progress: &'a Progress,
    hard_links: Mutex<HashSet<(u64, u64)>>,
    unreadable: AtomicU64,
}

impl<'a> Walker<'a> {
    fn new(root: &Metadata, options: &'a Options, progress: &'a Progress) -> Self {
        Self {
            device: root.dev(),
            options,
            progress,
            hard_links: Mutex::default(),
            unreadable: AtomicU64::new(0),
        }
    }

    /// Allocated size, or 0 if this inode was already counted via another link.
    fn allocated(&self, meta: &Metadata) -> u64 {
        if meta.nlink() > 1 && !meta.is_dir() {
            let mut seen = self.hard_links.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if !seen.insert((meta.dev(), meta.ino())) {
                return 0;
            }
        }
        meta.blocks() * 512
    }

    fn visit(&self, path: &Path, name: Box<str>, meta: &Metadata) -> RawNode {
        let size = self.allocated(meta);
        let mut node = RawNode {
            name,
            kind: if meta.is_dir() { NodeKind::Directory } else { NodeKind::File },
            size,
            files: u64::from(!meta.is_dir()),
            modified: meta.mtime(),
            unreadable: false,
            children: Vec::new(),
        };
        if meta.is_dir() && !self.progress.is_cancelled() {
            self.visit_children(path, &mut node);
        } else {
            self.progress.files.fetch_add(1, Relaxed);
            self.progress.bytes.fetch_add(size, Relaxed);
        }
        node
    }

    fn visit_children(&self, path: &Path, node: &mut RawNode) {
        if let Ok(mut current) = self.progress.current.try_lock() {
            path.clone_into(&mut current);
        }
        let Ok(entries) = fs::read_dir(path) else {
            node.unreadable = true;
            self.unreadable.fetch_add(1, Relaxed);
            return;
        };

        let mut subdirs = Vec::new();
        let mut grouped = Grouped::default();
        for entry in entries.flatten() {
            // `DirEntry::metadata` does not follow symlinks (lstat semantics).
            let Ok(meta) = entry.metadata() else { continue };
            let child_path = entry.path();
            if meta.is_dir() {
                if meta.dev() == self.device && !self.options.skip.contains(&child_path) {
                    subdirs.push((child_path, entry.file_name(), meta));
                }
                continue;
            }
            let child = self.visit(&child_path, entry.file_name().to_string_lossy().into(), &meta);
            if child.size >= self.options.min_file_size {
                node.children.push(child);
            } else {
                grouped.add(&child);
            }
        }

        let dirs: Vec<RawNode> = subdirs
            .into_par_iter()
            .map(|(p, name, meta)| self.visit(&p, name.to_string_lossy().into(), &meta))
            .collect();
        node.children.extend(dirs);
        if let Some(group) = grouped.into_node() {
            node.children.push(group);
        }

        node.size += node.children.iter().map(|c| c.size).sum::<u64>();
        node.files += node.children.iter().map(|c| c.files).sum::<u64>();
        if node.size < self.options.min_file_size {
            // Nothing worth drilling into: keep the totals, drop the detail.
            node.children = Vec::new();
        }
    }
}

/// Accumulates files too small to be listed on their own.
#[derive(Default)]
struct Grouped {
    count: u64,
    size: u64,
    modified: i64,
}

impl Grouped {
    fn add(&mut self, file: &RawNode) {
        self.count += 1;
        self.size += file.size;
        self.modified = self.modified.max(file.modified);
    }

    fn into_node(self) -> Option<RawNode> {
        (self.count > 0).then(|| RawNode {
            name: format!("{} smaller file{}", self.count, if self.count == 1 { "" } else { "s" }).into(),
            kind: NodeKind::Grouped,
            size: self.size,
            files: self.count,
            modified: self.modified,
            unreadable: false,
            children: Vec::new(),
        })
    }
}

/// Builds a tree from `(relative path, size)` file entries, for tests in
/// other modules.
#[cfg(test)]
pub(crate) fn tree_for_tests(files: &[(&str, u64)]) -> Tree {
    fn insert(dir: &mut RawNode, parts: &[&str], size: u64) {
        dir.size += size;
        dir.files += 1;
        if let [name] = parts {
            dir.children.push(RawNode { kind: NodeKind::File, files: 1, size, ..empty(name) });
            return;
        }
        let index = if let Some(i) = dir.children.iter().position(|c| &*c.name == parts[0]) {
            i
        } else {
            dir.children.push(empty(parts[0]));
            dir.children.len() - 1
        };
        insert(&mut dir.children[index], &parts[1..], size);
    }
    fn empty(name: &str) -> RawNode {
        RawNode {
            name: name.into(),
            kind: NodeKind::Directory,
            size: 0,
            files: 0,
            modified: 0,
            unreadable: false,
            children: Vec::new(),
        }
    }
    let mut root = empty("root");
    for (path, size) in files {
        insert(&mut root, &path.split('/').collect::<Vec<_>>(), *size);
    }
    Builder::build(root, PathBuf::from("/root"), 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&vec![7u8; bytes]).unwrap();
    }

    fn options(min: u64) -> Options {
        Options { min_file_size: min, skip: Vec::new() }
    }

    #[test]
    fn sizes_match_allocated_blocks_and_group_small_files() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("big.bin"), 300_000);
        write(&dir.path().join("a/one.txt"), 10);
        write(&dir.path().join("a/two.txt"), 10);

        let tree = scan(dir.path(), &options(100_000), &Progress::default()).unwrap();
        let root = tree.get(tree.root());
        assert_eq!(root.files, 3);
        assert!(root.size >= 300_000, "allocated size covers content");

        let (_, big) = tree.children(tree.root()).next().unwrap();
        assert_eq!(&*big.name, "big.bin");

        let a = tree.children(tree.root()).find(|(_, n)| &*n.name == "a").unwrap().1;
        // `a` is below the threshold so its two files are summarised, not listed.
        assert_eq!(a.files, 2);
        assert!(a.children.is_empty());
    }

    #[test]
    fn small_files_are_grouped_in_large_directories() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("big.bin"), 200_000);
        for i in 0..3 {
            write(&dir.path().join(format!("s{i}.txt")), 10);
        }
        let tree = scan(dir.path(), &options(100_000), &Progress::default()).unwrap();
        let grouped = tree.children(tree.root()).find(|(_, n)| n.kind == NodeKind::Grouped).unwrap().1;
        assert_eq!(&*grouped.name, "3 smaller files");
        assert_eq!(grouped.files, 3);
    }

    #[test]
    fn hard_links_are_counted_once() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.bin");
        write(&original, 500_000);
        fs::hard_link(&original, dir.path().join("link.bin")).unwrap();
        let single = fs::symlink_metadata(&original).unwrap().blocks() * 512;
        let dir_overhead = fs::symlink_metadata(dir.path()).unwrap().blocks() * 512;
        assert_eq!(measure(dir.path()), single + dir_overhead);
    }

    #[test]
    fn symlinks_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("target/data.bin"), 400_000);
        std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("alias")).unwrap();
        // Threshold 0 lists every entry; APFS symlinks may occupy zero blocks.
        let tree = scan(dir.path(), &options(0), &Progress::default()).unwrap();
        let alias = tree.children(tree.root()).find(|(_, n)| &*n.name == "alias").unwrap().1;
        assert_eq!(alias.kind, NodeKind::File);
        assert!(alias.size < 400_000);
    }

    #[test]
    fn skipped_directories_are_excluded() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("keep/a.bin"), 100_000);
        write(&dir.path().join("skip/b.bin"), 100_000);
        let opts = Options { min_file_size: 1, skip: vec![dir.path().join("skip")] };
        let tree = scan(dir.path(), &opts, &Progress::default()).unwrap();
        assert!(tree.children(tree.root()).all(|(_, n)| &*n.name != "skip"));
    }

    #[test]
    fn cancelled_scans_report_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let progress = Progress::default();
        progress.cancel();
        assert!(matches!(scan(dir.path(), &Options::default(), &progress), Err(Error::Cancelled)));
    }

    #[test]
    fn measure_of_missing_path_is_zero() {
        assert_eq!(measure(Path::new("/definitely/not/here")), 0);
    }
}
