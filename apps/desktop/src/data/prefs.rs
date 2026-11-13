//! Settings and small bits of state, kept in one JSON file. The CLIs run off the main thread
//! and read settings too, so everything sits behind one lock.
//!
//! On first launch the Swift app's defaults (`com.zain.mrow`) are imported, so hosts, paths,
//! tabs, read state and viewed files carry over.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::forge::Forge;
use super::inbox;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn name(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Blank means "find glab on its own".
    pub glab_path: String,
    /// Blank means glab's default host.
    pub hostname: String,
    pub gh_path: String,
    /// Blank means github.com; set for GitHub Enterprise.
    pub github_host: String,
    pub gitlab_enabled: bool,
    pub github_enabled: bool,
    pub active_forge: Forge,
    /// How far back the Done list looks.
    pub done_days: i64,
    pub refresh_minutes: i64,
    pub show_drafts: bool,
    pub oldest_first: bool,
    /// Banners at all: the master switch over `inbox_levels`.
    pub notify: bool,
    /// Per kind of event: a banner and the inbox, the inbox only, or off. Missing kinds take
    /// their default.
    pub inbox_levels: BTreeMap<inbox::Kind, inbox::Level>,
    /// The inbox, newest first.
    pub inbox: Vec<inbox::Event>,
    /// What the last refresh saw, to tell what changed since. None before the first.
    pub inbox_snapshot: Option<inbox::Snapshot>,
    /// Check for, download and stage new versions in the background (release builds only).
    pub auto_update: bool,
    pub sidebar_width: f64,
    pub ask_width: f64,
    pub ask_visible: bool,
    pub sidebar_visible: bool,
    pub appearance: Appearance,
    /// The agent CLI for Ask; blank finds `cursor-agent`/`agent` on its own.
    pub agent_path: String,
    /// Ask: the command for each other agent ("claude", "opencode", "pi"), with its arguments,
    /// when it isn't found on its own. Claude's is its ACP adapter.
    pub agent_commands: BTreeMap<String, String>,
    /// Ask: comments the agent drafted that the user posted, by `Proposal::key`, so a reopened
    /// chat doesn't offer them again.
    pub ask_posted: BTreeSet<String>,
    /// Ask: the agent new chats use.
    pub ask_agent: String,
    /// Ask: each agent's models, picks and effort settings, by agent ("cursor", "claude"…).
    /// Cursor's moved here from the `ask_model…` fields below, which are only read once.
    pub ask_rosters: BTreeMap<String, Value>,
    /// When each merge request was last opened here (seconds since 1970), by global ID.
    pub last_seen: BTreeMap<String, f64>,
    /// Merge requests open in tabs, in order, by global ID.
    pub tabs: Vec<String>,
    /// Files marked viewed: merge request → file → the diff's digest when marked.
    pub viewed_files: BTreeMap<String, BTreeMap<String, i64>>,
    /// "id@seconds" for each review request already notified about, from before the inbox: read
    /// once to seed `inbox_snapshot`.
    pub known_requests: BTreeSet<String>,
    pub known_requests_initialized: bool,
    /// Ask: project path → local clone.
    pub checkouts: BTreeMap<String, String>,
    /// Ask: merge request ID → agent session ID, as "agent\tsession" (a bare ID is Cursor's).
    pub ask_sessions: BTreeMap<String, String>,
    pub ask_model: String,
    pub ask_recent_models: Vec<String>,
    pub ask_efforts: BTreeMap<String, String>,
    /// Ask: models the agent offered last time, so the picker opens filled.
    pub ask_models: Vec<Value>,
    pub ask_models_fetched: f64,
    /// Ask: each model's effort setting as Cursor offers it, learned from sessions on it.
    pub ask_effort_options: BTreeMap<String, Value>,
}

/// The queue's width until someone drags it: about a quarter of the default window.
const SIDEBAR_WIDTH: f64 = 300.0;

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            glab_path: String::new(),
            hostname: String::new(),
            gh_path: String::new(),
            github_host: String::new(),
            gitlab_enabled: true,
            github_enabled: true,
            active_forge: Forge::Gitlab,
            done_days: 14,
            refresh_minutes: 5,
            show_drafts: true,
            oldest_first: false,
            notify: true,
            inbox_levels: BTreeMap::new(),
            inbox: Vec::new(),
            inbox_snapshot: None,
            auto_update: true,
            sidebar_width: SIDEBAR_WIDTH,
            ask_width: 400.0,
            ask_visible: false,
            sidebar_visible: true,
            appearance: Appearance::System,
            agent_path: String::new(),
            agent_commands: BTreeMap::new(),
            ask_posted: BTreeSet::new(),
            ask_agent: String::new(),
            ask_rosters: BTreeMap::new(),
            last_seen: BTreeMap::new(),
            tabs: Vec::new(),
            viewed_files: BTreeMap::new(),
            known_requests: BTreeSet::new(),
            known_requests_initialized: false,
            checkouts: BTreeMap::new(),
            ask_sessions: BTreeMap::new(),
            ask_model: String::new(),
            ask_recent_models: Vec::new(),
            ask_efforts: BTreeMap::new(),
            ask_models: Vec::new(),
            ask_models_fetched: 0.0,
            ask_effort_options: BTreeMap::new(),
        }
    }
}

impl Prefs {
    pub fn is_enabled(&self, forge: Forge) -> bool {
        match forge {
            Forge::Gitlab => self.gitlab_enabled,
            Forge::Github => self.github_enabled,
        }
    }

    pub fn enabled_forges(&self) -> Vec<Forge> {
        Forge::ALL.into_iter().filter(|f| self.is_enabled(*f)).collect()
    }
}

struct Shared {
    prefs: Prefs,
    /// Snapshots and benchmarks never write.
    persist: bool,
}

/// Set before the first read for runs that must not read or write the person's settings.
static EPHEMERAL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static SHARED: LazyLock<RwLock<Shared>> = LazyLock::new(|| {
    let ephemeral = EPHEMERAL.load(std::sync::atomic::Ordering::Relaxed);
    let prefs = if ephemeral { Prefs::default() } else { load() };
    RwLock::new(Shared { prefs, persist: !ephemeral })
});

/// A copy of the settings (cheap enough: they're read per refresh, not per frame).
pub fn get() -> Prefs {
    SHARED.read().unwrap().prefs.clone()
}

/// Reads one value without copying the rest.
pub fn read<T>(f: impl FnOnce(&Prefs) -> T) -> T {
    f(&SHARED.read().unwrap().prefs)
}

/// Changes settings and saves them.
pub fn update(f: impl FnOnce(&mut Prefs)) {
    let mut shared = SHARED.write().unwrap();
    f(&mut shared.prefs);
    if shared.persist {
        save(&shared.prefs);
    }
}

/// Stops saving: snapshots, benchmarks and tests look without touching the user's state.
pub fn set_ephemeral(prefs: Option<Prefs>) {
    EPHEMERAL.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut shared = SHARED.write().unwrap();
    shared.persist = false;
    if let Some(prefs) = prefs {
        shared.prefs = prefs;
    }
}

pub fn folder() -> PathBuf {
    super::paths::config_dir()
}

fn file() -> PathBuf {
    folder().join("settings.json")
}

fn load() -> Prefs {
    if let Ok(bytes) = std::fs::read(file()) {
        if let Ok(mut prefs) = serde_json::from_slice::<Prefs>(&bytes) {
            // 340 was the width before; a file still holding exactly that was never dragged.
            if prefs.sidebar_width == 340.0 {
                prefs.sidebar_width = SIDEBAR_WIDTH;
            }
            return prefs;
        }
    }
    let prefs = import_swift_defaults().unwrap_or_default();
    save(&prefs);
    prefs
}

fn save(prefs: &Prefs) {
    let _ = std::fs::create_dir_all(folder());
    let Ok(json) = serde_json::to_vec_pretty(prefs) else { return };
    // Write beside and rename, so a crash mid-write never leaves half a file.
    let tmp = file().with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(tmp, file());
    }
}

/// Only macOS had the Swift app, so elsewhere there's nothing to import.
#[cfg(not(target_os = "macos"))]
fn import_swift_defaults() -> Option<Prefs> {
    None
}

/// The Swift app's UserDefaults, as JSON through `defaults export` and `plutil`.
#[cfg(target_os = "macos")]
fn import_swift_defaults() -> Option<Prefs> {
    let plist = std::process::Command::new("/usr/bin/defaults").args(["export", "com.zain.mrow", "-"]).output().ok()?;
    if !plist.status.success() || plist.stdout.is_empty() {
        return None;
    }
    let mut child = std::process::Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    use std::io::Write;
    child.stdin.take()?.write_all(&plist.stdout).ok()?;
    let out = child.wait_with_output().ok()?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    Some(from_swift(&v))
}

/// Maps the Swift app's keys (mostly the same names) onto `Prefs`.
#[cfg(any(target_os = "macos", test))]
pub fn from_swift(v: &Value) -> Prefs {
    let mut p = Prefs::default();
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let b = |k: &str| v.get(k).and_then(Value::as_bool);
    let n = |k: &str| v.get(k).and_then(Value::as_f64);
    if let Some(x) = s("glabPath") {
        p.glab_path = x
    }
    if let Some(x) = s("hostname") {
        p.hostname = x
    }
    if let Some(x) = s("ghPath") {
        p.gh_path = x
    }
    if let Some(x) = s("githubHost") {
        p.github_host = x
    }
    if let Some(x) = s("agentPath") {
        p.agent_path = x
    }
    if let Some(x) = b("gitlabEnabled") {
        p.gitlab_enabled = x
    }
    if let Some(x) = b("githubEnabled") {
        p.github_enabled = x
    }
    if let Some(x) = b("showDrafts") {
        p.show_drafts = x
    }
    if let Some(x) = b("oldestFirst") {
        p.oldest_first = x
    }
    if let Some(x) = b("notify") {
        p.notify = x
    }
    if let Some(x) = b("askVisible") {
        p.ask_visible = x
    }
    if let Some(x) = b("knownRequestsInitialized") {
        p.known_requests_initialized = x
    }
    if let Some(x) = n("doneDays") {
        p.done_days = x as i64
    }
    if let Some(x) = n("refreshMinutes") {
        p.refresh_minutes = x as i64
    }
    if let Some(x) = n("sidebarWidth") {
        p.sidebar_width = x
    }
    if let Some(x) = n("askWidth") {
        p.ask_width = x
    }
    if let Some(f) = s("activeForge").and_then(|f| Forge::from_raw(&f)) {
        p.active_forge = f
    }
    p.appearance = match s("appearance").as_deref() {
        Some("light") => Appearance::Light,
        Some("dark") => Appearance::Dark,
        _ => Appearance::System,
    };
    if let Some(Value::Object(m)) = v.get("lastSeen") {
        p.last_seen = m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_f64()?))).collect();
    }
    if let Some(Value::Array(a)) = v.get("tabs") {
        p.tabs = a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
    }
    if let Some(Value::Array(a)) = v.get("knownRequests") {
        p.known_requests = a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
    }
    if let Some(Value::Object(m)) = v.get("viewedFiles") {
        p.viewed_files = m
            .iter()
            .filter_map(|(k, v)| {
                let files = v.as_object()?.iter().filter_map(|(f, d)| Some((f.clone(), d.as_i64()?))).collect();
                Some((k.clone(), files))
            })
            .collect();
    }
    let strings = |k: &str| -> BTreeMap<String, String> {
        v.get(k).and_then(Value::as_object).map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default()
    };
    p.checkouts = strings("checkouts");
    p.ask_sessions = strings("askSessions");
    p.ask_efforts = strings("askEfforts");
    if let Some(x) = s("askModel") {
        p.ask_model = x
    }
    if let Some(Value::Array(a)) = v.get("askRecentModels") {
        p.ask_recent_models = a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
    }
    p
}
