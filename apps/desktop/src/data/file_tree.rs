//! The changed files of a request grouped by folder, for the list at the top of the Changes tab.
//!
//! A folder holding a single folder and nothing else shares its row with it (`src/ledger/`, not
//! `src/` then `ledger/`), and one holding a single file gives that file its row
//! (`docs/exports.md`): a row only for a folder that has a choice in it. Folders and files keep
//! the order the request lists its files in (the diff's order under the list), each where its
//! first file is.

use std::collections::HashMap;

use super::models::FileDiff;

/// A list this short stays flat, each file under its whole path: its paths fit on their rows,
/// and a row for each folder would make it longer without making it easier to scan.
pub const FLAT_UP_TO: usize = 8;
/// Folders start open as deep as keeps the list within this many rows, so the first diff starts
/// on the first screen. Fourteen rows of 26 points leave the top of the first diff in sight on a
/// window 700 points tall.
pub const ROW_BUDGET: usize = 14;

/// A folder's row, or a file's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Node {
    Folder(usize),
    File(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    /// Its whole path, ending in a slash: what remembers whether it's open.
    pub path: String,
    /// Where its row's label starts in `path`: after the folder it's in.
    pub label_at: usize,
    /// How many folders it's in.
    pub depth: usize,
    pub children: Vec<Node>,
    /// Every file in it, however deep, in the request's order.
    pub files: Vec<usize>,
    pub added: usize,
    pub removed: usize,
}

impl Folder {
    /// What its row shows: its path from the folder it's in.
    pub fn label(&self) -> &str {
        &self.path[self.label_at..]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileTree {
    pub folders: Vec<Folder>,
    /// The rows at the top level.
    pub roots: Vec<Node>,
    /// For each file, where its row's label starts in its path, and how many folders it's in.
    pub label_at: Vec<usize>,
    pub depth: Vec<usize>,
    /// Folders shallower than this start open.
    pub open_depth: usize,
}

/// A folder of the tree being built, before chains are merged: its children by name, in order.
#[derive(Default)]
struct Raw {
    order: Vec<RawChild>,
    dirs: HashMap<String, usize>,
}

enum RawChild {
    Dir(usize),
    File(usize),
}

impl FileTree {
    /// The tree of `files`, each under the path it has now (a renamed file under its new one).
    /// A short list (`FLAT_UP_TO`) is flat.
    pub fn new(files: &[FileDiff]) -> FileTree {
        let paths: Vec<&str> = files.iter().map(FileDiff::path).collect();
        let counts: Vec<(usize, usize)> = files.iter().map(|f| (f.added, f.removed)).collect();
        Self::of(&paths, &counts)
    }

    /// The tree of files at `paths`, each with its lines (added, removed).
    pub fn of(paths: &[&str], counts: &[(usize, usize)]) -> FileTree {
        let mut tree = FileTree { label_at: vec![0; paths.len()], depth: vec![0; paths.len()], ..FileTree::default() };
        if paths.len() <= FLAT_UP_TO {
            tree.roots = (0..paths.len()).map(Node::File).collect();
            return tree;
        }
        // The folders by name, then merged into rows.
        let mut raw = vec![Raw::default()];
        let mut ends = vec![0];
        for (f, path) in paths.iter().enumerate() {
            let mut at = 0;
            let mut end = 0;
            let mut rest = *path;
            while let Some(slash) = rest.find('/') {
                end += slash + 1;
                let name = &rest[..slash];
                rest = &rest[slash + 1..];
                at = match raw[at].dirs.get(name) {
                    Some(&dir) => dir,
                    None => {
                        let dir = raw.len();
                        raw.push(Raw::default());
                        ends.push(end);
                        raw[at].dirs.insert(name.to_string(), dir);
                        raw[at].order.push(RawChild::Dir(dir));
                        dir
                    }
                };
            }
            raw[at].order.push(RawChild::File(f));
        }
        let mut build = Build { raw: &raw, ends: &ends, paths, counts, tree: &mut tree };
        let roots = build.children(0, 0, 0, None);
        tree.roots = roots;
        tree.open_depth = tree.fitting_depth();
        tree
    }

    /// Whether folder `d` starts open.
    pub fn opens(&self, d: usize) -> bool {
        self.folders[d].depth < self.open_depth
    }

    /// The rows in sight, folder by folder: a folder's children follow it while `open` says it's
    /// open.
    pub fn rows(&self, open: impl Fn(usize) -> bool) -> Vec<Node> {
        let mut rows = Vec::new();
        self.push_rows(&self.roots, &open, &mut rows);
        rows
    }

    fn push_rows(&self, nodes: &[Node], open: &impl Fn(usize) -> bool, rows: &mut Vec<Node>) {
        for &node in nodes {
            rows.push(node);
            if let Node::Folder(d) = node {
                if open(d) {
                    self.push_rows(&self.folders[d].children, open, rows);
                }
            }
        }
    }

    /// The deepest level folders can be open to with the list within `ROW_BUDGET` rows; none
    /// open if even the top level is longer.
    fn fitting_depth(&self) -> usize {
        let deepest = self.folders.iter().map(|f| f.depth + 1).max().unwrap_or(0);
        (1..=deepest).take_while(|&depth| self.rows(|d| self.folders[d].depth < depth).len() <= ROW_BUDGET).last().unwrap_or(0)
    }
}

struct Build<'a> {
    raw: &'a [Raw],
    ends: &'a [usize],
    paths: &'a [&'a str],
    counts: &'a [(usize, usize)],
    tree: &'a mut FileTree,
}

impl Build<'_> {
    /// The rows of raw folder `at`'s children, labelled from `label_at`, `depth` folders deep,
    /// each file in `parent` too.
    fn children(&mut self, at: usize, label_at: usize, depth: usize, parent: Option<usize>) -> Vec<Node> {
        let mut nodes = Vec::new();
        for child in &self.raw[at].order {
            let node = match *child {
                RawChild::File(f) => self.file(f, label_at, depth),
                RawChild::Dir(dir) => self.dir(dir, label_at, depth),
            };
            nodes.push(node);
        }
        if let Some(parent) = parent {
            for node in &nodes {
                let (files, added, removed) = match *node {
                    Node::File(f) => (vec![f], self.counts[f].0, self.counts[f].1),
                    Node::Folder(d) => {
                        let folder = &self.tree.folders[d];
                        (folder.files.clone(), folder.added, folder.removed)
                    }
                };
                let folder = &mut self.tree.folders[parent];
                folder.files.extend(files);
                folder.added += added;
                folder.removed += removed;
            }
        }
        nodes
    }

    fn file(&mut self, f: usize, label_at: usize, depth: usize) -> Node {
        self.tree.label_at[f] = label_at;
        self.tree.depth[f] = depth;
        Node::File(f)
    }

    /// Raw folder `dir` as a row, or, when it holds one thing alone, as that thing's row.
    fn dir(&mut self, mut dir: usize, label_at: usize, depth: usize) -> Node {
        loop {
            match self.raw[dir].order.as_slice() {
                [RawChild::Dir(only)] => dir = *only,
                [RawChild::File(only)] => return self.file(*only, label_at, depth),
                _ => break,
            }
        }
        let first = self.first_file(dir);
        let d = self.tree.folders.len();
        self.tree.folders.push(Folder {
            path: self.paths[first][..self.ends[dir]].to_string(),
            label_at,
            depth,
            children: vec![],
            files: vec![],
            added: 0,
            removed: 0,
        });
        let children = self.children(dir, self.ends[dir], depth + 1, Some(d));
        self.tree.folders[d].children = children;
        Node::Folder(d)
    }

    /// A file somewhere in raw folder `dir`, whose path spells the folder's.
    fn first_file(&self, mut dir: usize) -> usize {
        loop {
            match self.raw[dir].order.first() {
                Some(RawChild::File(f)) => return *f,
                Some(RawChild::Dir(d)) => dir = *d,
                None => unreachable!("a folder is made for a file in it"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(paths: &[&str]) -> FileTree {
        let counts: Vec<(usize, usize)> = (0..paths.len()).map(|f| (f + 1, 1)).collect();
        FileTree::of(paths, &counts)
    }

    /// Each row in sight with every folder open, as its label indented two spaces a level.
    fn outline(t: &FileTree, paths: &[&str]) -> Vec<String> {
        t.rows(|_| true)
            .into_iter()
            .map(|n| match n {
                Node::Folder(d) => format!("{}{}", "  ".repeat(t.folders[d].depth), t.folders[d].label()),
                Node::File(f) => format!("{}{}", "  ".repeat(t.depth[f]), &paths[f][t.label_at[f]..]),
            })
            .collect()
    }

    /// Nine files, one over the flat list's length, under `folder`.
    fn nine(folder: &str) -> Vec<String> {
        (0..9).map(|i| format!("{folder}f{i}.rs")).collect()
    }

    #[test]
    fn no_files_no_rows() {
        let t = tree(&[]);
        assert!(t.rows(|_| true).is_empty());
        assert!(t.folders.is_empty());
    }

    #[test]
    fn one_file_is_one_row_under_its_whole_path() {
        let paths = ["src/deep/down/lib.rs"];
        let t = tree(&paths);
        assert_eq!(t.rows(|_| true), [Node::File(0)]);
        assert_eq!(outline(&t, &paths), ["src/deep/down/lib.rs"]);
    }

    #[test]
    fn a_short_list_stays_flat() {
        let paths = ["a/x.rs", "a/y.rs", "b/z.rs", "c/d/e.rs", "f.rs", "g/h.rs", "g/i.rs", "j/k.rs"];
        assert_eq!(paths.len(), FLAT_UP_TO);
        let t = tree(&paths);
        assert!(t.folders.is_empty());
        assert_eq!(outline(&t, &paths), paths);
    }

    #[test]
    fn a_chain_of_lone_folders_shares_one_row() {
        let owned = nine("src/ledger/");
        let paths: Vec<&str> = owned.iter().map(String::as_str).collect();
        let t = tree(&paths);
        assert_eq!(t.folders.len(), 1);
        assert_eq!(t.folders[0].label(), "src/ledger/");
        assert_eq!(t.folders[0].files, (0..9).collect::<Vec<_>>());
        assert_eq!(outline(&t, &paths)[1], "  f0.rs");
    }

    #[test]
    fn deep_chains_merge_at_every_level() {
        let paths = [
            "a/b/c/one.rs",
            "a/b/c/two.rs",
            "a/b/c/d/e/three.rs",
            "a/b/c/d/e/four.rs",
            "a/b/c/d/f/five.rs",
            "a/b/c/d/f/six.rs",
            "a/b/g/h/seven.rs",
            "a/b/g/h/eight.rs",
            "a/b/g/i/nine.rs",
        ];
        let t = tree(&paths);
        assert_eq!(
            outline(&t, &paths),
            [
                "a/b/",
                "  c/",
                "    one.rs",
                "    two.rs",
                "    d/",
                "      e/",
                "        three.rs",
                "        four.rs",
                "      f/",
                "        five.rs",
                "        six.rs",
                "  g/",
                "    h/",
                "      seven.rs",
                "      eight.rs",
                "    i/nine.rs",
            ]
        );
        assert_eq!(
            t.folders.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["a/b/", "a/b/c/", "a/b/c/d/", "a/b/c/d/e/", "a/b/c/d/f/", "a/b/g/", "a/b/g/h/"]
        );
    }

    #[test]
    fn root_files_sit_among_folders_in_the_requests_order() {
        let mut owned = vec!["Cargo.toml".to_string()];
        owned.extend(nine("src/"));
        owned.push("README.md".into());
        let paths: Vec<&str> = owned.iter().map(String::as_str).collect();
        let t = tree(&paths);
        assert_eq!(t.roots, [Node::File(0), Node::Folder(0), Node::File(10)]);
        assert_eq!((t.depth[0], t.label_at[0]), (0, 0));
        assert_eq!((t.depth[1], t.label_at[1]), (1, "src/".len()));
    }

    #[test]
    fn folders_sum_what_is_in_them() {
        let paths = ["a/x/1.rs", "a/x/2.rs", "a/y/3.rs", "a/y/4.rs", "b/5.rs", "b/6.rs", "b/7.rs", "c/8.rs", "c/9.rs"];
        let t = tree(&paths);
        let a = &t.folders[0];
        assert_eq!(a.path, "a/");
        assert_eq!(a.files, [0, 1, 2, 3]);
        assert_eq!((a.added, a.removed), (1 + 2 + 3 + 4, 4));
        let b = t.folders.iter().find(|f| f.path == "b/").unwrap();
        assert_eq!((b.added, b.removed), (5 + 6 + 7, 3));
    }

    #[test]
    fn renamed_files_go_where_they_are_now() {
        let mut files: Vec<FileDiff> = nine("src/new/")
            .into_iter()
            .map(|p| FileDiff {
                old_path: p.clone(),
                new_path: p,
                diff: String::new(),
                new_file: false,
                renamed_file: false,
                deleted_file: false,
                too_large: false,
                collapsed: false,
                generated_file: false,
                lines: vec![],
                added: 0,
                removed: 0,
                digest: 0,
            })
            .collect();
        files[3].old_path = "lib/old/f3.rs".into();
        files[3].renamed_file = true;
        let t = FileTree::new(&files);
        assert_eq!(t.folders.len(), 1);
        assert_eq!(t.folders[0].path, "src/new/");
        assert!(t.folders[0].files.contains(&3));
    }

    #[test]
    fn folders_open_as_deep_as_fits() {
        // Four folders of three: open, sixteen rows; folded, four.
        let owned: Vec<String> = ["a", "b", "c", "d"].iter().flat_map(|d| (0..3).map(move |i| format!("{d}/{i}.rs"))).collect();
        let paths: Vec<&str> = owned.iter().map(String::as_str).collect();
        let t = tree(&paths);
        assert_eq!(t.open_depth, 0);
        assert!(!t.opens(0));
        assert_eq!(t.rows(|d| t.opens(d)).len(), 4);
        // Three folders of three fit open.
        let t = tree(&paths[..9]);
        assert!((0..3).all(|d| t.opens(d)));
        assert_eq!(t.rows(|d| t.opens(d)).len(), 12);
    }

    #[test]
    fn the_top_opens_when_only_deeper_folders_overflow() {
        // `src/` holds three folders of five: its children show, folded.
        let owned: Vec<String> = ["x", "y", "z"].iter().flat_map(|d| (0..5).map(move |i| format!("src/{d}/{i}.rs"))).chain(["top.rs".to_string()]).collect();
        let paths: Vec<&str> = owned.iter().map(String::as_str).collect();
        let t = tree(&paths);
        assert_eq!(t.open_depth, 1);
        let rows = t.rows(|d| t.opens(d));
        assert_eq!(rows.len(), 1 + 3 + 1);
        assert_eq!(t.folders[0].label(), "src/");
    }

    #[test]
    fn a_thousand_files_fold_to_their_top() {
        let owned: Vec<String> = (0..1000).map(|i| format!("pkg/m{:02}/sub{}/f{i}.rs", i % 40, i % 3)).collect();
        let paths: Vec<&str> = owned.iter().map(String::as_str).collect();
        let t = tree(&paths);
        assert_eq!(t.rows(|_| true).len(), 1000 + 1 + 40 + 120);
        assert_eq!(t.open_depth, 0, "forty folders in pkg/ don't fit, so it starts folded");
        assert_eq!(t.rows(|d| t.opens(d)), [Node::Folder(0)]);
    }
}
