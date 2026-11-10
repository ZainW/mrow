//! Where the user's own clone of a project lives, so Cursor's agent can read the code around a
//! change. Found by reading `.git/config` remotes in the usual places; a folder picked by hand
//! wins. Nothing here runs git or changes the checkout.

use std::path::{Path, PathBuf};

use crate::data::prefs;

/// Folders whose children are searched first; then the home folder's own children, then the
/// children of every other folder in home (`~/acme/api`).
const ROOTS: [&str; 9] = ["code", "src", "dev", "Developer", "projects", "repos", "work", "git", "GitHub"];

/// Home folders that never hold clones and can be large.
const SKIP: [&str; 9] = ["Library", "Applications", "Pictures", "Music", "Movies", "Downloads", "Documents", "Desktop", "Public"];

/// The folders in home that aren't roots or skipped, by name.
fn other_folders(home: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(home) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            !n.starts_with('.') && !ROOTS.contains(&n.as_str()) && !SKIP.contains(&n.as_str())
        })
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

/// A remembered or found clone of `project` ("group/name"), or nil.
pub fn find(project: &str) -> Option<PathBuf> {
    if let Some(saved) = prefs::read(|p| p.checkouts.get(project).cloned()) {
        if Path::new(&saved).exists() {
            return Some(PathBuf::from(saved));
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let name = project.rsplit('/').next().unwrap_or(project);
    let folders = ROOTS.iter().map(|r| home.join(r)).chain(std::iter::once(home.clone())).chain(other_folders(&home));
    for folder in folders {
        let Ok(read) = std::fs::read_dir(&folder) else { continue };
        let mut children: Vec<PathBuf> = read.filter_map(|e| e.ok()).filter(|e| !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.path()).collect();
        // The plain name first (api before api-hotfix).
        children.sort_by(|a, b| {
            let (an, bn) = (a.file_name().is_some_and(|n| n == name), b.file_name().is_some_and(|n| n == name));
            bn.cmp(&an).then_with(|| a.cmp(b))
        });
        if let Some(found) = children.into_iter().find(|c| matches(c, project)) {
            return Some(found);
        }
    }
    None
}

pub fn remember(folder: &Path, project: &str) {
    let path = folder.to_string_lossy().to_string();
    prefs::update(|p| {
        p.checkouts.insert(project.to_string(), path);
    });
}

/// A clone (not a worktree: those have a `.git` file) with a remote ending in the project path.
pub fn matches(folder: &Path, project: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(folder.join(".git/config")) else { return false };
    let wanted = project.to_lowercase();
    text.lines().any(|line| {
        let trimmed = line.trim().to_lowercase();
        if !trimmed.starts_with("url") {
            return false;
        }
        let mut url = trimmed.split_once('=').map(|(_, u)| u.trim().to_string()).unwrap_or_default();
        if let Some(u) = url.strip_suffix(".git") {
            url = u.to_string();
        }
        if let Some(u) = url.strip_suffix('/') {
            url = u.to_string();
        }
        url.ends_with(&format!(":{wanted}")) || url.ends_with(&format!("/{wanted}"))
    })
}
