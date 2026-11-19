//! What the window last showed, kept on disk so a launch opens filled: each account's lists as
//! the last refresh fetched them, and the changes and threads of the requests opened lately. The
//! live refresh replaces all of it once it lands.
//!
//! Two kinds of file under `paths::cache_dir()/saved`: `queue.json`, and one file per request in
//! `details/`, the most recently used kept (`MAX_DETAILS`, `MAX_DETAIL_BYTES`). Both carry
//! `VERSION`; a file of another version, or one that doesn't parse, is ignored and written over.
//! Writes go beside the file and rename onto it, so a crash never leaves half of one. Only what
//! the forge CLIs returned is kept; Mrow never sees a token, so none can end up here.
//!
//! Only the window's own store opens one (`Store::restore`): snapshots, benchmarks and tests that
//! don't ask for it never read or write these files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::forge::Forge;
use super::models::{Date, DiffRefs, Discussion, FileDiff, MergeRequest, Person, fnv1a};

/// Bumped whenever what's written changes shape; other versions are ignored.
pub const VERSION: u32 = 1;
/// The most requests whose details are kept.
pub const MAX_DETAILS: usize = 20;
/// The most bytes of details kept, all told.
pub const MAX_DETAIL_BYTES: u64 = 64 << 20;
/// A request's details bigger than this aren't kept: reading them back would take longer than
/// it saves.
pub const MAX_DETAIL_FILE: usize = 8 << 20;

/// Where the files go.
#[derive(Clone, Debug)]
pub struct Disk {
    dir: PathBuf,
}

impl Disk {
    /// The person's own: under the platform's cache folder.
    pub fn standard() -> Disk {
        Disk::at(super::paths::cache_dir().join("saved"))
    }

    pub fn at(dir: impl Into<PathBuf>) -> Disk {
        Disk { dir: dir.into() }
    }

    fn queue_file(&self) -> PathBuf {
        self.dir.join("queue.json")
    }

    fn details_dir(&self) -> PathBuf {
        self.dir.join("details")
    }

    /// One request's file, named by a hash of its ID (IDs hold slashes and colons).
    fn detail_file(&self, id: &str) -> PathBuf {
        self.details_dir().join(format!("{:016x}.json", fnv1a(id) as u64))
    }
}

// MARK: Queue

/// One account's lists, as the last refresh that reached it fetched them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedAccount {
    /// The host it was fetched from (blank for the CLI's default), so a changed setting doesn't
    /// show another host's lists.
    pub host: String,
    pub me: Person,
    pub open: Vec<MergeRequest>,
    pub done: Vec<MergeRequest>,
    pub mine: Vec<MergeRequest>,
    pub mine_error: Option<String>,
    pub refreshed_at: Date,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedQueue {
    pub version: u32,
    pub saved_at: Date,
    /// The request on screen.
    pub selected: Option<String>,
    pub accounts: BTreeMap<Forge, SavedAccount>,
}

/// Just the version, read before the rest so a file of another shape is told apart from a
/// broken one only by what it says.
#[derive(Deserialize)]
struct Head {
    version: u32,
}

/// `bytes` as a `T` of this version, or nothing.
fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Option<T> {
    let head: Head = serde_json::from_slice(bytes).ok()?;
    if head.version != VERSION {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}

impl Disk {
    pub fn read_queue(&self) -> Option<SavedQueue> {
        parse(&std::fs::read(self.queue_file()).ok()?)
    }

    /// Writes the queue off the UI thread. Writes land in the order they were asked for.
    pub fn write_queue(&self, queue: &SavedQueue) {
        let Ok(bytes) = serde_json::to_vec(queue) else { return };
        let turn = QUEUE_TURN.fetch_add(1, Ordering::Relaxed) + 1;
        let path = self.queue_file();
        io(move || write_queue_turn(&path, &bytes, turn));
    }

    /// Writes it now, on this thread: when quitting. Writes asked for before it are skipped.
    pub fn write_queue_now(&self, queue: &SavedQueue) {
        if let Ok(bytes) = serde_json::to_vec(queue) {
            write_queue_turn(&self.queue_file(), &bytes, QUEUE_TURN.fetch_add(1, Ordering::Relaxed) + 1);
        }
    }
}

/// The last queue write asked for, and the last one written: an older one never lands over a newer.
static QUEUE_TURN: AtomicU64 = AtomicU64::new(0);
static QUEUE_WRITTEN: Mutex<u64> = Mutex::new(0);

fn write_queue_turn(path: &Path, bytes: &[u8], turn: u64) {
    let mut last = QUEUE_WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    if turn > *last {
        write_atomic(path, bytes);
        *last = turn;
    }
}

// MARK: Details

/// A request's changes and threads, as loaded while its `updated_at` was this.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub updated_at: Date,
    pub files: Vec<FileDiff>,
    pub refs: Option<DiffRefs>,
    pub threads: Vec<Discussion>,
}

#[derive(Serialize)]
struct DetailOut<'a> {
    version: u32,
    id: &'a str,
    updated_at: Date,
    files: &'a [FileDiff],
    refs: Option<&'a DiffRefs>,
    threads: &'a [Discussion],
}

#[derive(Deserialize)]
struct DetailIn {
    id: String,
    updated_at: Date,
    files: Vec<FileDiff>,
    refs: Option<DiffRefs>,
    threads: Vec<Discussion>,
}

impl Disk {
    /// The details kept for `id`, when they were loaded while it was last updated at
    /// `updated_at`: anything since (a push, a comment) may have changed them. Reading one counts
    /// as using it. A file that doesn't parse is removed.
    pub fn read_detail(&self, id: &str, updated_at: Date) -> Option<Detail> {
        let path = self.detail_file(id);
        let bytes = std::fs::read(&path).ok()?;
        let Some(saved) = parse::<DetailIn>(&bytes) else {
            let _ = std::fs::remove_file(&path);
            return None;
        };
        if saved.id != id || saved.updated_at != updated_at {
            return None;
        }
        touch(&path);
        Some(Detail {
            updated_at: saved.updated_at,
            files: saved.files.into_iter().map(FileDiff::prepared).collect(),
            refs: saved.refs,
            threads: saved.threads,
        })
    }

    /// Keeps a request's details, off the UI thread, then lets the least recently used go.
    pub fn write_detail(&self, id: &str, updated_at: Date, files: &[FileDiff], refs: Option<&DiffRefs>, threads: &[Discussion]) {
        let out = DetailOut { version: VERSION, id, updated_at, files, refs, threads };
        let Ok(bytes) = serde_json::to_vec(&out) else { return };
        let path = self.detail_file(id);
        if bytes.len() > MAX_DETAIL_FILE {
            // Too big to be worth reading back; an older, smaller copy is out of date too.
            io(move || {
                let _ = std::fs::remove_file(path);
            });
            return;
        }
        let dir = self.details_dir();
        io(move || {
            write_atomic(&path, &bytes);
            prune(&dir, MAX_DETAILS, MAX_DETAIL_BYTES);
        });
    }
}

/// Keeps the most recently used details within `count` files and `bytes` in all.
pub fn prune(dir: &Path, count: usize, bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let used = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => files.push((used, meta.len(), path)),
            // A write that never got renamed: a crash, or another Mrow's write still going.
            Some("tmp") if now.duration_since(used).unwrap_or_default() > Duration::from_secs(600) => {
                let _ = std::fs::remove_file(path);
            }
            _ => {}
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
    let mut total = 0;
    for (i, (_, len, path)) in files.into_iter().enumerate() {
        total += len;
        if i >= count || total > bytes {
            let _ = std::fs::remove_file(path);
        }
    }
}

// MARK: Writing

/// Runs file work off the UI thread; inline in the self-tests, so they can look straight after.
fn io(work: impl FnOnce() + Send + 'static) {
    if cfg!(test) {
        work();
    } else {
        std::thread::spawn(work);
    }
}

/// Writes beside `path` and renames onto it. The temporary name is this write's own, so two
/// writes of one file never share one.
fn write_atomic(path: &Path, bytes: &[u8]) {
    static N: AtomicU64 = AtomicU64::new(0);
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!("{name}.{}.{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, path).is_ok() {
        return;
    }
    let _ = std::fs::remove_file(tmp);
}

/// Marks a file used now, for `prune`.
fn touch(path: &Path) {
    if let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Sets when a file was last used, for the self-tests.
#[cfg(test)]
pub fn set_used(disk: &Disk, id: &str, at: SystemTime) {
    if let Ok(file) = std::fs::File::options().write(true).open(disk.detail_file(id)) {
        let _ = file.set_modified(at);
    }
}

/// Whether details are kept for `id`, for the self-tests.
#[cfg(test)]
pub fn has_detail(disk: &Disk, id: &str) -> bool {
    disk.detail_file(id).exists()
}

/// Where the queue goes, for the self-tests.
#[cfg(test)]
pub fn queue_path(disk: &Disk) -> PathBuf {
    disk.queue_file()
}

/// The store's side of it: where to write, and what it needs to know to write only what's true.
#[derive(Default)]
pub struct Saved {
    /// None for every store but the window's.
    pub disk: Option<Disk>,
    /// The accounts whose lists came from disk and haven't been fetched since.
    pub restored: std::collections::BTreeSet<Forge>,
    /// The `updated_at` the last details load of each kind was asked under, by request key
    /// ("diffs <id>").
    pub asked: std::collections::HashMap<String, Date>,
    /// The same for what's loaded now: a load asked since doesn't count until it lands.
    pub basis: std::collections::HashMap<String, Date>,
    /// A fingerprint of what was last kept of each request's details, so an unchanged reload
    /// isn't written again.
    pub kept: std::collections::HashMap<String, u64>,
}

impl Saved {
    /// The load numbered last for `key` landed: what's shown is now what it was asked under.
    pub fn landed(&mut self, key: String) {
        if let Some(at) = self.asked.get(&key).copied() {
            self.basis.insert(key, at);
        }
    }

    /// Lets go of what's known about requests other than `selected`, whose details are gone.
    pub fn forget(&mut self, selected: Option<&str>) {
        let keep = |key: &String| key.split_once(' ').is_some_and(|(_, id)| Some(id) == selected);
        self.asked.retain(|k, _| keep(k));
        self.basis.retain(|k, _| keep(k));
        self.kept.retain(|id, _| Some(id.as_str()) == selected);
    }
}

/// Tells one set of details from another without writing them out.
pub fn fingerprint(updated_at: Date, files: &[FileDiff], refs: Option<&DiffRefs>, threads: &[Discussion]) -> u64 {
    use std::fmt::Write;
    let mut text = format!("{}|{refs:?}", updated_at.timestamp_micros());
    for f in files {
        let _ = write!(text, "|{}:{}:{}{}", f.id(), f.digest, f.too_large as u8, f.collapsed as u8);
    }
    text.push_str(&serde_json::to_string(threads).unwrap_or_default());
    fnv1a(&text) as u64
}
