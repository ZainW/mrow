//! Everything the window shows: the review queues from each forge, which one the sidebar is on,
//! what's selected, tabs, read state, and details as they load. Plain data with methods; the
//! background work it starts comes back as `Msg`s through `land`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use chrono::{Local, Utc};

use super::bg::Bg;
use super::diff::DiffLine;
use super::forge::{Forge, QueueResult};
use super::models::*;
use super::prefs;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Loadable<T> {
    #[default]
    Idle,
    Loading,
    Loaded(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Loadable::Loaded(v) => Some(v),
            _ => None,
        }
    }
}

/// The three lists in the sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QueueMode {
    #[default]
    ToReview,
    Reviewed,
    Done,
}

impl QueueMode {
    pub const ALL: [QueueMode; 3] = [QueueMode::ToReview, QueueMode::Reviewed, QueueMode::Done];

    pub fn title(self) -> &'static str {
        match self {
            QueueMode::ToReview => "To review",
            QueueMode::Reviewed => "Reviewed",
            QueueMode::Done => "Done",
        }
    }
}

pub struct QueueSection {
    pub title: &'static str,
    pub items: Vec<MergeRequest>,
}

/// One forge's side of the queue: who I am there and what's waiting.
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub me: Option<Person>,
    pub open: Vec<MergeRequest>,
    pub done: Vec<MergeRequest>,
    pub error: Option<String>,
    pub last_refreshed: Option<Date>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Viewed {
    No,
    Yes,
    /// Marked viewed, but the file has changed since.
    Changed,
}

/// What comes back from the background.
pub enum Msg {
    Queue(Vec<(Forge, Result<QueueResult, String>)>),
    Diffs {
        id: String,
        request: u64,
        result: Result<(Vec<FileDiff>, Option<DiffRefs>), String>,
    },
    Threads {
        id: String,
        request: u64,
        result: Result<Vec<Discussion>, String>,
    },
    /// An approve, comment or reply finished. `reload` names a merge request whose threads to reload.
    Done {
        result: Result<(), String>,
        message: String,
        refresh: bool,
        reload: Option<MergeRequest>,
        ticket: Option<u64>,
    },
}

pub struct Store {
    /// Every enabled forge's queue, refreshed together so the Dock badge, notifications and the
    /// account switcher's counts cover all of them.
    pub accounts: BTreeMap<Forge, Account>,
    /// The account the sidebar shows.
    pub forge: Forge,
    pub is_loading: bool,
    pub mode: QueueMode,
    pub filter: String,
    /// Only merge requests by this author (a username), when set.
    pub author_filter: Option<String>,
    pub selected_id: Option<String>,
    /// Merge requests kept open in tabs, in order. The selected one may be outside it, as a
    /// preview (a single click), the way VS Code previews a file.
    pub tabs: Vec<String>,
    pub last_seen: BTreeMap<String, f64>,
    pub viewed_files: BTreeMap<String, BTreeMap<String, i64>>,
    /// Shared, so drawing a page takes a reference rather than copying every line.
    pub diffs: HashMap<String, Loadable<Rc<Vec<FileDiff>>>>,
    pub discussions: HashMap<String, Loadable<Rc<Vec<Discussion>>>>,
    /// The commits the loaded diffs were taken between; a comment on a line needs them.
    pub diff_refs: HashMap<String, DiffRefs>,
    /// The action running on the selected merge request, e.g. "Approving…".
    pub busy: Option<String>,
    /// A short message under the header, e.g. "Approved".
    pub toast: Option<String>,
    /// When the toast goes, on the app clock.
    pub toast_until: f64,
    /// Snapshots and benchmarks: never ask a forge.
    pub offline: bool,
    refreshing: bool,
    refresh_again: bool,
    requests: HashMap<String, u64>,
    /// Finished actions whose result a view is waiting for (a comment box clears on success).
    pub finished: HashMap<u64, bool>,
    next_ticket: u64,
    pub bg: Bg<Msg>,
    /// Asked to show a toast at the next `poll`, with the clock.
    pending_toast: Option<String>,
}

impl Default for Store {
    fn default() -> Self {
        let p = prefs::get();
        Store {
            accounts: BTreeMap::new(),
            forge: p.active_forge,
            is_loading: false,
            mode: QueueMode::ToReview,
            filter: String::new(),
            author_filter: None,
            selected_id: None,
            tabs: p.tabs,
            last_seen: p.last_seen,
            viewed_files: p.viewed_files,
            diffs: HashMap::new(),
            discussions: HashMap::new(),
            diff_refs: HashMap::new(),
            busy: None,
            toast: None,
            toast_until: 0.0,
            offline: false,
            refreshing: false,
            refresh_again: false,
            requests: HashMap::new(),
            finished: HashMap::new(),
            next_ticket: 0,
            bg: Bg::default(),
            pending_toast: None,
        }
    }
}

impl Store {
    // MARK: Accounts

    pub fn open(&self) -> &[MergeRequest] {
        self.accounts.get(&self.forge).map_or(&[], |a| &a.open)
    }

    pub fn done(&self) -> &[MergeRequest] {
        self.accounts.get(&self.forge).map_or(&[], |a| &a.done)
    }

    pub fn me(&self) -> Option<&Person> {
        self.accounts.get(&self.forge).and_then(|a| a.me.as_ref())
    }

    pub fn last_refreshed(&self) -> Option<Date> {
        self.accounts.get(&self.forge).and_then(|a| a.last_refreshed)
    }

    pub fn error(&self) -> Option<String> {
        if prefs::read(|p| p.enabled_forges().is_empty()) {
            return Some("Turn on GitLab or GitHub in Settings.".into());
        }
        self.accounts.get(&self.forge).and_then(|a| a.error.clone())
    }

    /// The accounts the switcher offers.
    pub fn forges(&self) -> Vec<Forge> {
        prefs::read(|p| p.enabled_forges())
    }

    /// What's waiting on me in one account, for the switcher.
    pub fn waiting_count(&self, forge: Forge) -> usize {
        let drafts = prefs::read(|p| p.show_drafts);
        self.accounts.get(&forge).map_or(0, |a| a.open.iter().filter(|m| m.my_state.needs_review() && (drafts || !m.draft)).count())
    }

    /// Shows another account's queue, keeping the selection if it's from there, otherwise moving
    /// to that account's first tab or first item.
    pub fn switch_forge(&mut self, forge: Forge) {
        if forge == self.forge {
            return;
        }
        self.set_forge(forge);
        if self.selected().is_some_and(|s| s.kind() == forge) {
            return;
        }
        let next = self.tab_items().into_iter().find(|m| m.kind() == forge).map(|m| m.id).or_else(|| self.visible_items().first().map(|m| m.id.clone()));
        self.select(next);
    }

    fn set_forge(&mut self, forge: Forge) {
        self.forge = forge;
        if !self.offline {
            prefs::update(|p| p.active_forge = forge);
        }
        self.author_filter = None;
    }

    /// Every account's merge requests, for finding tabs and the selection whichever account they're in.
    fn every_item(&self) -> impl Iterator<Item = &MergeRequest> {
        self.accounts.values().flat_map(|a| a.open.iter().chain(&a.done))
    }

    pub fn item(&self, id: &str) -> Option<&MergeRequest> {
        self.every_item().find(|m| m.id == id)
    }

    pub fn selected(&self) -> Option<&MergeRequest> {
        self.item(self.selected_id.as_deref()?)
    }

    // MARK: Lists

    pub fn items(&self, mode: QueueMode) -> Vec<MergeRequest> {
        let (drafts, oldest) = prefs::read(|p| (p.show_drafts, p.oldest_first));
        let mut items: Vec<MergeRequest> = match mode {
            QueueMode::ToReview => self.open().iter().filter(|m| m.my_state.needs_review() && (drafts || !m.draft)).cloned().collect(),
            QueueMode::Reviewed => self.open().iter().filter(|m| !m.my_state.needs_review()).cloned().collect(),
            QueueMode::Done => self.done().to_vec(),
        };
        let key = |m: &MergeRequest| if mode == QueueMode::Done { m.finished_at() } else { m.requested_at };
        items.sort_by(|a, b| if oldest { key(a).cmp(&key(b)) } else { key(b).cmp(&key(a)) });
        items
    }

    /// The current list, filtered and grouped by when I was asked (or when it finished, for Done).
    pub fn sections(&self) -> Vec<QueueSection> {
        let query = self.filter.trim().to_lowercase();
        let mut sections: Vec<QueueSection> = Vec::new();
        for item in self.items(self.mode) {
            if !(query.is_empty() || matches(&item, &query)) {
                continue;
            }
            if self.author_filter.as_ref().is_some_and(|a| *a != item.author.username) {
                continue;
            }
            let title = bucket(if self.mode == QueueMode::Done { item.finished_at() } else { item.requested_at });
            match sections.iter_mut().find(|s| s.title == title) {
                Some(s) => s.items.push(item),
                None => sections.push(QueueSection { title, items: vec![item] }),
            }
        }
        sections
    }

    pub fn visible_items(&self) -> Vec<MergeRequest> {
        self.sections().into_iter().flat_map(|s| s.items).collect()
    }

    pub fn count(&self, mode: QueueMode) -> usize {
        self.items(mode).len()
    }

    /// Everyone with something in the current list, most merge requests first.
    pub fn authors(&self) -> Vec<(Person, usize)> {
        let mut counts: Vec<(Person, usize)> = Vec::new();
        for item in self.items(self.mode) {
            match counts.iter_mut().find(|(p, _)| p.username == item.author.username) {
                Some(c) => c.1 += 1,
                None => counts.push((item.author.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
        counts
    }

    pub fn is_unread(&self, item: &MergeRequest) -> bool {
        if !item.is_open() {
            return false;
        }
        match self.last_seen.get(&item.id) {
            None => true,
            Some(seen) => item.updated_at.timestamp() as f64 > *seen,
        }
    }

    // MARK: Selection

    /// Selects a merge request (or nothing), marking it seen and bringing its account along.
    pub fn select(&mut self, id: Option<String>) {
        self.selected_id = id;
        let Some(item) = self.selected().cloned() else { return };
        self.mark_seen(&item);
        // A tab from the other account brings its account along, so the sidebar matches.
        if item.kind() != self.forge {
            self.set_forge(item.kind());
        }
        self.load_details_if_needed(&item);
    }

    pub fn select_offset(&mut self, offset: i64) {
        let items = self.visible_items();
        if items.is_empty() {
            return;
        }
        let index = self.selected_id.as_ref().and_then(|id| items.iter().position(|m| &m.id == id));
        let next = match index {
            None => 0,
            Some(i) => (i as i64 + offset).clamp(0, items.len() as i64 - 1) as usize,
        };
        self.select(Some(items[next].id.clone()));
    }

    // MARK: Tabs

    pub fn tab_items(&self) -> Vec<MergeRequest> {
        self.tabs.iter().filter_map(|id| self.item(id).cloned()).collect()
    }

    fn save_tabs(&self) {
        if !self.offline {
            let tabs = self.tabs.clone();
            prefs::update(|p| p.tabs = tabs);
        }
    }

    /// Keeps a merge request in a tab, at the end of the queue; `activate` also shows it.
    pub fn open_tab(&mut self, id: &str, activate: bool) {
        if !self.tabs.iter().any(|t| t == id) {
            self.tabs.push(id.to_string());
            self.save_tabs();
        }
        if activate {
            self.select(Some(id.to_string()));
        }
    }

    /// Closes a tab, or the preview, moving on to the next merge request in the queue.
    pub fn close_tab(&mut self, id: &str) {
        let index = self.tabs.iter().position(|t| t == id);
        if let Some(i) = index {
            self.tabs.remove(i);
            self.save_tabs();
        }
        if self.selected_id.as_deref() != Some(id) {
            return;
        }
        if self.tabs.is_empty() {
            self.selected_id = None;
        } else {
            let i = index.unwrap_or(self.tabs.len()).min(self.tabs.len() - 1);
            self.select(Some(self.tabs[i].clone()));
        }
    }

    pub fn close_other_tabs(&mut self, id: &str) {
        self.tabs = if self.tabs.iter().any(|t| t == id) { vec![id.to_string()] } else { vec![] };
        self.save_tabs();
        self.select(Some(id.to_string()));
    }

    /// The next or previous tab; from a preview, the queue's first or last.
    pub fn select_tab(&mut self, offset: i64) {
        if self.tabs.is_empty() {
            return;
        }
        let index = self.selected_id.as_ref().and_then(|id| self.tabs.iter().position(|t| t == id));
        let next = match index {
            None => {
                if offset > 0 {
                    0
                } else {
                    self.tabs.len() - 1
                }
            }
            Some(i) => (i as i64 + offset).rem_euclid(self.tabs.len() as i64) as usize,
        };
        self.select(Some(self.tabs[next].clone()));
    }

    pub fn set_mode(&mut self, mode: QueueMode) {
        self.mode = mode;
        let visible = self.visible_items();
        if self.selected_id.as_ref().is_some_and(|id| visible.iter().any(|m| &m.id == id)) {
            return;
        }
        self.select(visible.first().map(|m| m.id.clone()));
    }

    fn mark_seen(&mut self, item: &MergeRequest) {
        let now = Utc::now().timestamp() as f64;
        self.last_seen.insert(item.id.clone(), now.max(item.updated_at.timestamp() as f64));
        // Forget merge requests that have left every list, so the store doesn't grow forever.
        let live: HashSet<String> = self.every_item().map(|m| m.id.clone()).collect();
        if !live.is_empty() {
            self.last_seen.retain(|k, _| live.contains(k));
        }
        if !self.offline {
            let seen = self.last_seen.clone();
            prefs::update(|p| p.last_seen = seen);
        }
    }

    pub fn mark_all_seen(&mut self) {
        let now = Utc::now().timestamp() as f64;
        for item in self.open().to_vec() {
            self.last_seen.insert(item.id.clone(), now.max(item.updated_at.timestamp() as f64));
        }
        if !self.offline {
            let seen = self.last_seen.clone();
            prefs::update(|p| p.last_seen = seen);
        }
    }

    // MARK: Viewed files

    pub fn viewed(&self, file: &FileDiff, item: &MergeRequest) -> Viewed {
        match self.viewed_files.get(&item.id).and_then(|f| f.get(&file.id())) {
            None => Viewed::No,
            Some(d) if *d == file.digest => Viewed::Yes,
            Some(_) => Viewed::Changed,
        }
    }

    pub fn set_viewed(&mut self, on: bool, file: &FileDiff, item: &MergeRequest) {
        let files = self.viewed_files.entry(item.id.clone()).or_default();
        if on {
            files.insert(file.id(), file.digest);
        } else {
            files.remove(&file.id());
        }
        if files.is_empty() {
            self.viewed_files.remove(&item.id);
        }
        let live: HashSet<String> = self.every_item().map(|m| m.id.clone()).collect();
        if !live.is_empty() {
            self.viewed_files.retain(|k, _| live.contains(k));
        }
        if !self.offline {
            let viewed = self.viewed_files.clone();
            prefs::update(|p| p.viewed_files = viewed);
        }
    }

    // MARK: Loading

    /// Refreshes every enabled account. A call while one runs queues one more full pass, so a
    /// caller (say, after approving) always sees data fetched after it asked.
    pub fn refresh(&mut self) {
        if self.offline {
            return;
        }
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        self.is_loading = true;
        let forges = self.forges();
        self.accounts.retain(|f, _| forges.contains(f));
        if !forges.contains(&self.forge)
            && let Some(first) = forges.first()
        {
            self.set_forge(*first);
        }
        let days = prefs::read(|p| p.done_days);
        self.bg.spawn(move || {
            let handles: Vec<_> = forges.into_iter().map(|f| (f, std::thread::spawn(move || f.queue(days)))).collect();
            Msg::Queue(handles.into_iter().map(|(f, h)| (f, h.join().unwrap_or_else(|_| Err("The fetch crashed.".into())))).collect())
        });
    }

    fn landed_queue(&mut self, results: Vec<(Forge, Result<QueueResult, String>)>) -> Effects {
        let mut effects = Effects::default();
        self.refreshing = false;
        self.is_loading = false;
        // An account that fails keeps what it had, marked offline.
        let any_ok = results.iter().any(|(_, r)| r.is_ok());
        for (forge, result) in results {
            let account = self.accounts.entry(forge).or_default();
            match result {
                Ok(q) => {
                    account.me = Some(q.me);
                    account.open = q.open;
                    account.done = q.done;
                    account.error = None;
                    account.last_refreshed = Some(Utc::now());
                }
                Err(e) => account.error = Some(e),
            }
        }
        if self.refresh_again {
            self.refresh_again = false;
            self.refresh();
        }
        if !any_ok {
            return effects;
        }
        // A merge request that's left every list closes its tab, unless an account that never
        // loaded might still have it.
        let forges = self.forges();
        let every_loaded = forges.iter().all(|f| self.accounts.get(f).is_some_and(|a| a.last_refreshed.is_some()));
        let known: HashSet<String> = self.every_item().map(|m| m.id.clone()).collect();
        if every_loaded && self.tabs.iter().any(|t| !known.contains(t)) {
            self.tabs.retain(|t| known.contains(t));
            self.save_tabs();
        }
        if self.selected().is_none() {
            let forge = self.forge;
            let next = self.tab_items().into_iter().find(|m| m.kind() == forge).map(|m| m.id).or_else(|| self.visible_items().first().map(|m| m.id.clone()));
            self.selected_id = next;
        }
        // Changes and threads may have moved on; fetch them again when next shown.
        let selected = self.selected_id.clone();
        self.diffs.retain(|k, _| Some(k) == selected.as_ref());
        self.discussions.retain(|k, _| Some(k) == selected.as_ref());
        if let Some(item) = self.selected().cloned() {
            // What's on screen has been seen, including the update my own approval made.
            self.mark_seen(&item);
            self.load_diffs(&item);
            self.load_discussions(&item);
        }
        effects.badge = Some(self.accounts.values().flat_map(|a| &a.open).filter(|m| m.my_state.needs_review() && !m.draft).count());
        effects.notify = self.new_requests();
        effects
    }

    pub fn load_details_if_needed(&mut self, item: &MergeRequest) {
        if self.offline {
            return;
        }
        if !self.diffs.contains_key(&item.id) {
            self.load_diffs(item);
        }
        if !self.discussions.contains_key(&item.id) {
            self.load_discussions(item);
        }
    }

    fn start_request(&mut self, key: String) -> u64 {
        let n = self.requests.entry(key).or_insert(0);
        *n += 1;
        *n
    }

    fn load_diffs(&mut self, item: &MergeRequest) {
        if self.offline {
            return;
        }
        if self.diffs.get(&item.id).and_then(Loadable::value).is_none() {
            self.diffs.insert(item.id.clone(), Loadable::Loading);
        }
        let request = self.start_request(format!("diffs {}", item.id));
        let item = item.clone();
        self.bg.spawn(move || Msg::Diffs { id: item.id.clone(), request, result: item.kind().diffs(&item) });
    }

    fn load_discussions(&mut self, item: &MergeRequest) {
        if self.offline {
            return;
        }
        if self.discussions.get(&item.id).and_then(Loadable::value).is_none() {
            self.discussions.insert(item.id.clone(), Loadable::Loading);
        }
        let request = self.start_request(format!("threads {}", item.id));
        let item = item.clone();
        self.bg.spawn(move || Msg::Threads { id: item.id.clone(), request, result: item.kind().discussions(&item) });
    }

    // MARK: Actions

    pub fn approve(&mut self, item: &MergeRequest) {
        let it = item.clone();
        self.perform("Approving…", format!("Approved {}", item.short_reference()), true, None, move || it.kind().approve(&it));
    }

    pub fn unapprove(&mut self, item: &MergeRequest) {
        let it = item.clone();
        self.perform("Revoking approval…", "Approval revoked".into(), true, None, move || it.kind().unapprove(&it));
    }

    /// Posts a comment on the merge request. The ticket's result lands in `finished`.
    pub fn comment(&mut self, item: &MergeRequest, body: String) -> u64 {
        let it = item.clone();
        self.perform("Posting comment…", "Comment posted".into(), true, None, move || it.kind().comment(&it, &body))
    }

    /// Starts a thread on one line of a file's diff, against the commits that diff was taken
    /// between (`refs`, kept from when the box was opened).
    pub fn comment_on_line(&mut self, item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: Option<DiffRefs>, body: String) -> u64 {
        let Some(refs) = refs else {
            self.show("The changes moved while loading. Refresh, then comment on the line again.");
            let t = self.ticket();
            self.finished.insert(t, false);
            return t;
        };
        let (it, file, line) = (item.clone(), file.clone(), line.clone());
        self.perform("Posting comment…", "Comment posted".into(), false, Some(item.clone()), move || {
            it.kind().comment_on_line(&it, &file, &line, &refs, &body)
        })
    }

    pub fn reply(&mut self, item: &MergeRequest, thread: &Discussion, body: String) -> u64 {
        let (it, thread) = (item.clone(), thread.clone());
        self.perform("Posting reply…", "Reply posted".into(), false, Some(item.clone()), move || it.kind().reply(&it, &thread, &body))
    }

    fn ticket(&mut self) -> u64 {
        self.next_ticket += 1;
        self.next_ticket
    }

    fn perform(
        &mut self,
        label: &str,
        message: String,
        refresh: bool,
        reload: Option<MergeRequest>,
        work: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> u64 {
        let ticket = self.ticket();
        if self.offline {
            self.finished.insert(ticket, false);
            return ticket;
        }
        self.busy = Some(label.into());
        self.bg.spawn(move || Msg::Done { result: work(), message, refresh, reload, ticket: Some(ticket) });
        ticket
    }

    /// Shows a short message; `poll` gives it its time on screen.
    pub fn show(&mut self, message: &str) {
        self.toast = Some(message.into());
        self.pending_toast = Some(message.into());
    }

    /// Lands background results and expires the toast. Returns what the shell should do.
    pub fn poll(&mut self, now: f64) -> (bool, Effects) {
        let mut changed = false;
        let mut effects = Effects::default();
        for msg in self.bg.drain() {
            changed = true;
            match msg {
                Msg::Queue(results) => effects.merge(self.landed_queue(results)),
                Msg::Diffs { id, request, result } => {
                    if self.requests.get(&format!("diffs {id}")) != Some(&request) {
                        continue;
                    }
                    match result {
                        Ok((files, refs)) => {
                            match refs {
                                Some(r) => self.diff_refs.insert(id.clone(), r),
                                None => self.diff_refs.remove(&id),
                            };
                            self.diffs.insert(id, Loadable::Loaded(Rc::new(files)));
                        }
                        Err(e) => {
                            self.diffs.insert(id, Loadable::Failed(e));
                        }
                    }
                }
                Msg::Threads { id, request, result } => {
                    if self.requests.get(&format!("threads {id}")) != Some(&request) {
                        continue;
                    }
                    let value = match result {
                        Ok(threads) => Loadable::Loaded(Rc::new(threads.into_iter().filter(|t| !t.notes.iter().all(|n| n.system)).collect())),
                        Err(e) => Loadable::Failed(e),
                    };
                    self.discussions.insert(id, value);
                }
                Msg::Done { result, message, refresh, reload, ticket } => {
                    self.busy = None;
                    let ok = result.is_ok();
                    match result {
                        Ok(()) => {
                            self.show(&message);
                            if let Some(item) = reload {
                                self.load_discussions(&item);
                            }
                            if refresh {
                                self.refresh();
                            }
                        }
                        Err(e) => self.show(&e),
                    }
                    if let Some(t) = ticket {
                        self.finished.insert(t, ok);
                    }
                }
            }
        }
        if let Some(message) = self.pending_toast.take() {
            // Long enough to read: "Copied" goes in a couple of seconds, an error stays longer.
            let words = message.split_whitespace().count() as f64;
            let seconds = (1.5 + words * 0.3).clamp(2.5, 8.0);
            self.toast_until = now + seconds;
            effects.wake_at = Some(self.toast_until);
            effects.announce = Some(message);
            changed = true;
        }
        if self.toast.is_some() && now >= self.toast_until && self.toast_until > 0.0 {
            self.toast = None;
            changed = true;
        }
        (changed, effects)
    }

    /// A notification for each review request that wasn't in the queue last time, or that was
    /// asked again since.
    fn new_requests(&mut self) -> Vec<MergeRequest> {
        if self.offline {
            return vec![];
        }
        let waiting: Vec<MergeRequest> = self.accounts.values().flat_map(|a| &a.open).filter(|m| m.my_state.needs_review()).cloned().collect();
        let (known_list, initialized, notify) = prefs::read(|p| (p.known_requests.clone(), p.known_requests_initialized, p.notify));
        // Stored as "id@seconds", so a re-request (a later time) notifies again.
        let known: HashMap<String, i64> = known_list
            .iter()
            .filter_map(|e| {
                let at = e.rfind('@')?;
                Some((e[..at].to_string(), e[at + 1..].parse().ok()?))
            })
            .collect();
        let next = waiting.iter().map(|m| format!("{}@{}", m.id, m.requested_at.timestamp())).collect();
        prefs::update(|p| {
            p.known_requests = next;
            p.known_requests_initialized = true;
        });
        // The first run knows nothing yet; an empty queue later is still a known state.
        if !initialized || !notify {
            return vec![];
        }
        waiting
            .into_iter()
            .filter(|m| match known.get(&m.id) {
                None => true,
                // A failed note lookup can swap the request time for the creation time; that's no new ask.
                Some(seconds) => !m.requested_at_is_guess && m.requested_at.timestamp() > seconds + 60,
            })
            .take(5)
            .collect()
    }

    /// Loads a recorded queue instead of asking a forge (snapshots, benchmarks, tests).
    pub fn load_fixture(&mut self, accounts: BTreeMap<Forge, Account>, diffs: HashMap<String, Vec<FileDiff>>, threads: HashMap<String, Vec<Discussion>>) {
        self.offline = true;
        self.accounts = accounts;
        self.diffs = diffs.into_iter().map(|(k, v)| (k, Loadable::Loaded(Rc::new(v)))).collect();
        self.discussions = threads.into_iter().map(|(k, v)| (k, Loadable::Loaded(Rc::new(v)))).collect();
    }
}

/// What the shell carries out after a poll: things outside the window.
#[derive(Default)]
pub struct Effects {
    pub badge: Option<usize>,
    pub notify: Vec<MergeRequest>,
    pub wake_at: Option<f64>,
    pub announce: Option<String>,
}

impl Effects {
    fn merge(&mut self, other: Effects) {
        self.badge = other.badge.or(self.badge);
        self.notify.extend(other.notify);
        self.wake_at = other.wake_at.or(self.wake_at);
        self.announce = other.announce.or(self.announce.take());
    }
}

pub fn matches(item: &MergeRequest, query: &str) -> bool {
    let mut hay = vec![
        item.title.to_lowercase(),
        item.full_reference().to_lowercase(),
        item.author.name.to_lowercase(),
        item.author.username.to_lowercase(),
        item.source_branch.to_lowercase(),
        item.requested_by.as_ref().map(|p| p.name.to_lowercase()).unwrap_or_default(),
    ];
    hay.extend(item.labels.iter().map(|l| l.title.to_lowercase()));
    query.split(' ').filter(|w| !w.is_empty()).all(|w| hay.iter().any(|h| h.contains(w)))
}

pub fn bucket(date: Date) -> &'static str {
    let local = date.with_timezone(&Local).date_naive();
    let today = Local::now().date_naive();
    if local == today {
        return "Today";
    }
    if today.pred_opt() == Some(local) {
        return "Yesterday";
    }
    let days = (today - local).num_days();
    if days < 7 {
        "This week"
    } else if days < 30 {
        "This month"
    } else {
        "Older"
    }
}

/// "now", "5 minutes ago", "2 weeks ago", "7 years ago", as Foundation's named relative style.
pub fn relative(date: Date) -> String {
    let secs = (Utc::now() - date).num_seconds();
    if secs < 0 {
        return "now".into();
    }
    let unit = |n: i64, name: &str| format!("{n} {name}{} ago", if n == 1 { "" } else { "s" });
    match secs {
        0..60 => "now".into(),
        60..3600 => unit(secs / 60, "minute"),
        3600..86_400 => unit(secs / 3600, "hour"),
        86_400..172_800 => "yesterday".into(),
        172_800..604_800 => unit(secs / 86_400, "day"),
        604_800..2_592_000 => {
            let w = secs / 604_800;
            if w == 1 { "last week".into() } else { unit(w, "week") }
        }
        2_592_000..31_536_000 => {
            let m = secs / 2_592_000;
            if m == 1 { "last month".into() } else { unit(m, "month") }
        }
        _ => {
            let y = secs / 31_536_000;
            if y == 1 { "last year".into() } else { unit(y, "year") }
        }
    }
}
