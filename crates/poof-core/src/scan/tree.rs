//! A compact, arena-allocated directory tree.
//!
//! Nodes live in one `Vec` and refer to each other by [`NodeId`], which keeps
//! the tree cheap to hold (a full disk is hundreds of thousands of nodes),
//! trivially `Send`, and easy to address from UI messages.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

/// Index of a node inside a [`Tree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// Position in the arena; stable for the lifetime of the tree.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Directory,
    File,
    /// Many small files folded into one entry to keep memory bounded. It has
    /// no path of its own and cannot be deleted individually.
    Grouped,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub name: Box<str>,
    pub kind: NodeKind,
    /// Bytes allocated on disk, including all descendants.
    pub size: u64,
    /// Number of files at or below this node.
    pub files: u64,
    /// Last modification time (Unix seconds).
    pub modified: i64,
    /// The directory could not be read (usually missing Full Disk Access).
    pub unreadable: bool,
    pub parent: Option<NodeId>,
    /// Children, largest first.
    pub children: Vec<NodeId>,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.kind == NodeKind::Directory
    }

    /// Whether the node maps to a real path that can be revealed or removed.
    pub fn has_path(&self) -> bool {
        self.kind != NodeKind::Grouped
    }

    pub fn file_kind(&self) -> FileKind {
        match self.kind {
            NodeKind::Grouped => FileKind::Other,
            NodeKind::Directory if self.name.ends_with(".app") => FileKind::App,
            NodeKind::Directory => FileKind::Folder,
            NodeKind::File => FileKind::from_name(&self.name),
        }
    }
}

/// Coarse content type used for colour-coding and icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    Folder,
    App,
    Video,
    Image,
    Audio,
    Archive,
    DiskImage,
    Document,
    Code,
    Other,
}

impl FileKind {
    pub fn from_name(name: &str) -> Self {
        let Some((_, ext)) = name.rsplit_once('.') else {
            return FileKind::Other;
        };
        match ext.to_ascii_lowercase().as_str() {
            "mov" | "mp4" | "m4v" | "mkv" | "avi" | "webm" | "braw" | "mts" => FileKind::Video,
            "jpg" | "jpeg" | "png" | "heic" | "gif" | "tiff" | "raw" | "dng" | "cr3" | "psd" | "webp" => {
                FileKind::Image
            }
            "mp3" | "m4a" | "wav" | "aiff" | "flac" | "aac" | "logicx" => FileKind::Audio,
            "zip" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "tar" | "zst" | "xip" => FileKind::Archive,
            "dmg" | "iso" | "img" | "pkg" | "mpkg" | "raw_disk" | "qcow2" | "vmdk" | "vdi"
            | "sparseimage" | "sparsebundle" => FileKind::DiskImage,
            "pdf" | "doc" | "docx" | "pages" | "key" | "ppt" | "pptx" | "xls" | "xlsx" | "numbers"
            | "txt" | "md" | "epub" => FileKind::Document,
            "rs" | "js" | "ts" | "swift" | "py" | "go" | "c" | "h" | "cpp" | "java" | "kt" | "json"
            | "rlib" | "o" | "a" | "dylib" | "so" | "wasm" => FileKind::Code,
            _ => FileKind::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tree {
    root_path: PathBuf,
    nodes: Vec<Node>,
    /// When the scan finished (Unix seconds).
    pub scanned_at: i64,
    /// How many directories could not be read.
    pub unreadable: u64,
}

impl Tree {
    pub(crate) fn new(root_path: PathBuf, nodes: Vec<Node>, scanned_at: i64, unreadable: u64) -> Self {
        debug_assert!(!nodes.is_empty(), "a tree always has a root");
        Self { root_path, nodes, scanned_at, unreadable }
    }

    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The node's children (largest first), paired with their ids.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = (NodeId, &Node)> {
        self.get(id).children.iter().map(|&c| (c, self.get(c)))
    }

    /// Ids from the root down to `id`, inclusive — handy for breadcrumbs.
    pub fn lineage(&self, id: NodeId) -> Vec<NodeId> {
        let mut chain: Vec<_> = std::iter::successors(Some(id), |&n| self.get(n).parent).collect();
        chain.reverse();
        chain
    }

    /// Path components relative to the scan root (empty for the root itself).
    pub fn relative_path(&self, id: NodeId) -> PathBuf {
        self.lineage(id).iter().skip(1).map(|&n| &*self.get(n).name).collect()
    }

    /// The absolute path of a node.
    pub fn path(&self, id: NodeId) -> PathBuf {
        self.root_path.join(self.relative_path(id))
    }

    /// Finds the node at `relative` (components below the root), if scanned.
    pub fn find(&self, relative: &Path) -> Option<NodeId> {
        relative.components().try_fold(self.root(), |id, part| {
            let name = part.as_os_str().to_string_lossy();
            self.children(id).find(|(_, n)| *n.name == *name).map(|(c, _)| c)
        })
    }

    /// Detaches a node from the tree after it was deleted from disk, and
    /// subtracts its size from every ancestor so totals stay correct.
    pub fn remove(&mut self, id: NodeId) {
        let Some(parent) = self.get(id).parent else {
            return; // never remove the root
        };
        let (size, files) = (self.get(id).size, self.get(id).files);
        self.nodes[parent.index()].children.retain(|&c| c != id);
        let mut cursor = Some(parent);
        while let Some(n) = cursor {
            let node = &mut self.nodes[n.index()];
            node.size = node.size.saturating_sub(size);
            node.files = node.files.saturating_sub(files);
            cursor = node.parent;
        }
        self.nodes[id.index()].parent = None;
    }

    /// The `limit` largest individual files anywhere in the tree, largest first.
    pub fn largest_files(&self, limit: usize) -> Vec<NodeId> {
        let mut heap = BinaryHeap::with_capacity(limit + 1);
        for id in self.descendants(self.root()) {
            let node = self.get(id);
            if node.kind == NodeKind::File {
                heap.push(Reverse((node.size, id)));
                if heap.len() > limit {
                    heap.pop();
                }
            }
        }
        let mut ids: Vec<_> = heap.into_iter().map(|Reverse((_, id))| id).collect();
        ids.sort_by_key(|&id| Reverse(self.get(id).size));
        ids
    }

    /// Depth-first iterator over `id` and everything below it.
    pub fn descendants(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let mut stack = vec![id];
        std::iter::from_fn(move || {
            let next = stack.pop()?;
            stack.extend(self.get(next).children.iter().rev());
            Some(next)
        })
    }

    /// Visits every directory with its depth below the root (root = 0),
    /// skipping the subtree when `visit` returns `false`.
    pub fn visit_dirs(&self, mut visit: impl FnMut(NodeId, usize) -> bool) {
        let mut stack = vec![(self.root(), 0)];
        while let Some((id, depth)) = stack.pop() {
            if visit(id, depth) {
                stack.extend(self.children(id).filter(|(_, n)| n.is_dir()).map(|(c, _)| (c, depth + 1)));
            }
        }
    }
}

/// Builds a [`Tree`] from nested nodes; used by the walker and by tests.
#[derive(Debug)]
pub(crate) struct Builder {
    nodes: Vec<Node>,
}

/// An owned, nested node as produced by the parallel walker.
#[derive(Debug, Clone)]
pub(crate) struct RawNode {
    pub name: Box<str>,
    pub kind: NodeKind,
    pub size: u64,
    pub files: u64,
    pub modified: i64,
    pub unreadable: bool,
    pub children: Vec<RawNode>,
}

impl Builder {
    pub fn build(root: RawNode, root_path: PathBuf, scanned_at: i64, unreadable: u64) -> Tree {
        let mut builder = Builder { nodes: Vec::new() };
        builder.add(root, None);
        Tree::new(root_path, builder.nodes, scanned_at, unreadable)
    }

    fn add(&mut self, mut raw: RawNode, parent: Option<NodeId>) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("fewer than 4 billion nodes"));
        raw.children.sort_unstable_by_key(|c| Reverse(c.size));
        self.nodes.push(Node {
            name: raw.name,
            kind: raw.kind,
            size: raw.size,
            files: raw.files,
            modified: raw.modified,
            unreadable: raw.unreadable,
            parent,
            children: Vec::with_capacity(raw.children.len()),
        });
        for child in raw.children {
            let child_id = self.add(child, Some(id));
            self.nodes[id.index()].children.push(child_id);
        }
        id
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn file(name: &str, size: u64) -> RawNode {
        RawNode {
            name: name.into(),
            kind: NodeKind::File,
            size,
            files: 1,
            modified: 0,
            unreadable: false,
            children: vec![],
        }
    }

    pub fn dir(name: &str, children: Vec<RawNode>) -> RawNode {
        RawNode {
            name: name.into(),
            kind: NodeKind::Directory,
            size: children.iter().map(|c| c.size).sum(),
            files: children.iter().map(|c| c.files).sum(),
            modified: 0,
            unreadable: false,
            children,
        }
    }

    pub fn sample() -> Tree {
        let root = dir(
            "root",
            vec![
                file("small.txt", 10),
                dir("a", vec![file("big.mov", 500), file("mid.zip", 200)]),
                dir("b", vec![dir("c", vec![file("deep.dmg", 300)])]),
            ],
        );
        Builder::build(root, PathBuf::from("/scan"), 0, 0)
    }

    fn find(tree: &Tree, name: &str) -> NodeId {
        tree.descendants(tree.root()).find(|&id| &*tree.get(id).name == name).unwrap()
    }

    #[test]
    fn children_are_sorted_largest_first() {
        let tree = sample();
        let names: Vec<_> = tree.children(tree.root()).map(|(_, n)| &*n.name).collect();
        assert_eq!(names, ["a", "b", "small.txt"]);
        assert_eq!(tree.get(tree.root()).size, 1010);
    }

    #[test]
    fn paths_are_rebuilt_from_lineage() {
        let tree = sample();
        let deep = find(&tree, "deep.dmg");
        assert_eq!(tree.path(deep), PathBuf::from("/scan/b/c/deep.dmg"));
        assert_eq!(tree.relative_path(tree.root()), PathBuf::new());
        assert_eq!(tree.lineage(deep).len(), 4);
    }

    #[test]
    fn finds_nodes_by_relative_path() {
        let tree = sample();
        assert_eq!(tree.find(Path::new("b/c/deep.dmg")), Some(find(&tree, "deep.dmg")));
        assert_eq!(tree.find(Path::new("")), Some(tree.root()));
        assert_eq!(tree.find(Path::new("b/missing")), None);
    }

    #[test]
    fn removing_a_node_updates_every_ancestor() {
        let mut tree = sample();
        let deep = find(&tree, "deep.dmg");
        tree.remove(deep);
        assert_eq!(tree.get(tree.root()).size, 710);
        assert_eq!(tree.get(find(&tree, "b")).size, 0);
        assert_eq!(tree.get(tree.root()).files, 3);
        assert_eq!(tree.children(find(&tree, "c")).count(), 0);
    }

    #[test]
    fn largest_files_are_ranked() {
        let tree = sample();
        let names: Vec<_> = tree.largest_files(2).into_iter().map(|id| tree.get(id).name.clone()).collect();
        assert_eq!(names, ["big.mov".into(), "deep.dmg".into()] as [Box<str>; 2]);
    }

    #[test]
    fn classifies_files_by_extension() {
        assert_eq!(FileKind::from_name("Movie.MOV"), FileKind::Video);
        assert_eq!(FileKind::from_name("Xcode.xip"), FileKind::Archive);
        assert_eq!(FileKind::from_name("noext"), FileKind::Other);
    }
}
