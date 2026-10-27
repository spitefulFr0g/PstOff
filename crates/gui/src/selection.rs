//! Folder ticks for one OST. Everything starts ticked; unticking a folder
//! unticks its subfolders, and ticking a folder ticks its subfolders and the
//! ancestors it needs to exist in the PST.
use pstoff_engine::{Folder, FolderRef};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    On,
    Off,
    /// Ticked, with at least one unticked subfolder.
    Mixed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub items: u64,
    pub bytes: u64,
    pub folders: u64,
    pub all_folders: u64,
    /// Ticked folders the engine already reported as unreadable.
    pub unreadable: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Selection {
    unticked: HashSet<u32>,
    collapsed: HashSet<u32>,
}

impl Selection {
    pub fn is_ticked(&self, id: &FolderRef) -> bool {
        !self.unticked.contains(&id.0)
    }

    pub fn tick(&self, folder: &Folder) -> Tick {
        if !self.is_ticked(&folder.id) {
            Tick::Off
        } else if folder.children.iter().any(|c| self.tick(c) != Tick::On) {
            Tick::Mixed
        } else {
            Tick::On
        }
    }

    /// Clicking an unticked or partly ticked folder ticks it fully; clicking a
    /// fully ticked folder unticks it with its subfolders.
    pub fn toggle(&mut self, tree: &[Folder], id: &FolderRef) {
        let Some(path) = path_to(tree, id) else {
            return;
        };
        let folder = path[path.len() - 1];
        if self.tick(folder) == Tick::On {
            visit(folder, &mut |f| {
                self.unticked.insert(f.id.0);
            });
        } else {
            visit(folder, &mut |f| {
                self.unticked.remove(&f.id.0);
            });
            for ancestor in &path {
                self.unticked.remove(&ancestor.id.0);
            }
        }
    }

    pub fn set_all(&mut self, tree: &[Folder], ticked: bool) {
        self.unticked.clear();
        if !ticked {
            for folder in tree {
                visit(folder, &mut |f| {
                    self.unticked.insert(f.id.0);
                });
            }
        }
    }

    /// The folders to hand to `convert`: each unticked folder whose parent is
    /// ticked. The engine leaves out an excluded folder with its subfolders.
    pub fn exclusions(&self, tree: &[Folder]) -> Vec<FolderRef> {
        fn walk(s: &Selection, folders: &[Folder], out: &mut Vec<FolderRef>) {
            for f in folders {
                if s.is_ticked(&f.id) {
                    walk(s, &f.children, out);
                } else {
                    out.push(f.id.clone());
                }
            }
        }
        let mut out = Vec::new();
        walk(self, tree, &mut out);
        out
    }

    pub fn totals(&self, tree: &[Folder]) -> Totals {
        let mut t = Totals::default();
        for folder in tree {
            visit(folder, &mut |f| {
                t.all_folders += 1;
                if self.is_ticked(&f.id) {
                    t.folders += 1;
                    t.items += f.item_count;
                    t.bytes += f.bytes;
                    t.unreadable += u64::from(f.unreadable.is_some());
                }
            });
        }
        t
    }

    pub fn is_expanded(&self, id: &FolderRef) -> bool {
        !self.collapsed.contains(&id.0)
    }

    pub fn toggle_expanded(&mut self, id: &FolderRef) {
        if !self.collapsed.remove(&id.0) {
            self.collapsed.insert(id.0);
        }
    }

    /// The rows the tree shows, depth first, skipping collapsed subtrees.
    pub fn visible_rows<'a>(&self, tree: &'a [Folder]) -> Vec<(usize, &'a Folder)> {
        fn walk<'a>(
            s: &Selection,
            folders: &'a [Folder],
            depth: usize,
            out: &mut Vec<(usize, &'a Folder)>,
        ) {
            for f in folders {
                out.push((depth, f));
                if s.is_expanded(&f.id) {
                    walk(s, &f.children, depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(self, tree, 0, &mut out);
        out
    }
}

fn visit<'a>(folder: &'a Folder, f: &mut impl FnMut(&'a Folder)) {
    f(folder);
    for child in &folder.children {
        visit(child, f);
    }
}

/// The folder and its ancestors, root first.
pub fn path_to<'a>(tree: &'a [Folder], id: &FolderRef) -> Option<Vec<&'a Folder>> {
    for folder in tree {
        if &folder.id == id {
            return Some(vec![folder]);
        }
        if let Some(mut path) = path_to(&folder.children, id) {
            path.insert(0, folder);
            return Some(path);
        }
    }
    None
}

/// A readable folder path such as `Root - Mailbox / IPM_SUBTREE / Inbox`.
pub fn folder_path(tree: &[Folder], id: &FolderRef) -> String {
    match path_to(tree, id) {
        Some(path) => path
            .iter()
            .map(|f| {
                if f.name.is_empty() {
                    "(unnamed)"
                } else {
                    f.name.as_str()
                }
            })
            .collect::<Vec<_>>()
            .join(" / "),
        None => format!("Folder 0x{:x}", id.0),
    }
}
