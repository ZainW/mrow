//! Everything the window shows: the review queues from each forge, which one the sidebar is on,
//! what's selected, tabs, read state, and details as they load. Plain data with methods; the
//! background work it starts comes back as `Msg`s through `land`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use chrono::{Local, Utc};

use super::bg::Bg;
use super::diff::DiffLine;
use super::forge::{Forge, QueueResult};
use super::inbox::{self, Event, Notice};
use super::mine::{self, Stack, Standing, Status, WalkStep};
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

/// The lists in the sidebar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QueueMode {
    #[default]
    ToReview,
    Reviewed,
    /// The ones I opened, by whose move it is.
    Mine,
    Done,
}

impl QueueMode {
    pub const ALL: [QueueMode; 4] = [QueueMode::ToReview, QueueMode::Reviewed, QueueMode::Mine, QueueMode::Done];

    pub fn title(self) -> &'static str {
        match self {
            QueueMode::ToReview => "To review",
            QueueMode::Reviewed => "Reviewed",
            QueueMode::Mine => "Mine",
            QueueMode::Done => "Done",
        }
    }
}

pub struct QueueSection {
    pub title: &'static str,
    pub items: Vec<MergeRequest>,
}

/// Where a row sits on a stack's rail: the line runs up from all but the first, down from all
/// but the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rail {
    pub first: bool,
    pub last: bool,
}

/// A line of the Mine list.
#[derive(Clone, Debug, PartialEq)]
pub enum MineLine {
    Header(&'static str),
    /// `place` is "2 of 3" in its stack, shown when the list is flat (filtered).
    Item {
        item: MergeRequest,
        status: Status,
        rail: Option<Rail>,
        place: Option<(usize, usize)>,
    },
    /// A stack's title, above its rows. `open` shows every part; `merging` says a walk is on.
    StackHead {
        key: String,
        name: String,
        count: usize,
        open: bool,
        merging: bool,
    },
    /// The parts of a stack that aren't on me, folded into a count: "3 more · 1 held, 2 waiting".
    Folded {
        key: String,
        text: String,
        rail: Rail,
    },
    /// A stack with nothing on me, folded to one row.
    Whole {
        key: String,
        name: String,
        text: String,
    },
}

/// A stack being merged from the bottom up: each part merges once it's ready, after moving
/// onto the branch the stack lands on. It lives only while Mrow runs.
#[derive(Clone, Debug)]
pub struct Walk {
    /// The parts still to merge, bottom first.
    pub ids: Vec<String>,
    /// The branch the whole stack lands on.
    pub target: String,
    pub forge: Forge,
    /// The parts this walk merged itself. A part that leaves the list any other way (closed,
    /// say) ends the walk: what was stacked on it still carries its commits.
    merged: Vec<String>,
    /// When it last did something; a walk that only waits gives up after `WALK_PATIENCE_MINUTES`.
    moved_at: Date,
}

/// The fewest of my own requests a forge's query can return when it has run out of pages
/// (GitHub's two pages of 30). At or past this, the list may be cut short.
const MINE_PAGE_FLOOR: usize = 60;

/// How long a walk waits on a part (a pipeline, usually) before it gives up.
const WALK_PATIENCE_MINUTES: i64 = 45;

/// One forge's side of the queue: who I am there and what's waiting.
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub me: Option<Person>,
    pub open: Vec<MergeRequest>,
    pub done: Vec<MergeRequest>,
    /// The open ones I wrote.
    pub mine: Vec<MergeRequest>,
    /// Why `mine` couldn't be fetched, while the reviews could.
    pub mine_error: Option<String>,
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
    /// Mine: only requests standing like this (quiet and held ones count as waiting).
    pub mine_filter: Option<Standing>,
    /// Mine: the stacks showing every part, by `Stack::key`. The rest fold what isn't on me.
    pub open_stacks: HashSet<String>,
    /// Stacks being merged part by part.
    pub walks: Vec<Walk>,
    pub selected_id: Option<String>,
    /// Merge requests kept open in tabs, in order. The selected one may be outside it, as a
    /// preview (a single click), the way VS Code previews a file.
    pub tabs: Vec<String>,
    pub last_seen: BTreeMap<String, f64>,
    /// What changed, newest first: the bell's list.
    pub inbox: Vec<Event>,
    /// My requests merged from here: their leaving the list is my doing, not news.
    merged_here: HashSet<String>,
    /// When each merge request was last seen before this visit to it, for the replies that came
    /// since. This session only; none for one never seen before.
    pub seen_before: HashMap<String, f64>,
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
            mine_filter: None,
            open_stacks: HashSet::new(),
            walks: Vec::new(),
            selected_id: None,
            tabs: p.tabs,
            last_seen: p.last_seen,
            inbox: p.inbox,
            merged_here: HashSet::new(),
            seen_before: HashMap::new(),
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
        self.accounts.values().flat_map(|a| a.open.iter().chain(&a.done).chain(&a.mine))
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
            QueueMode::Mine => self.mine().to_vec(),
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
        if self.mode == QueueMode::Mine {
            return self
                .mine_lines()
                .into_iter()
                .filter_map(|l| match l {
                    MineLine::Item { item, .. } => Some(item),
                    _ => None,
                })
                .collect();
        }
        self.sections().into_iter().flat_map(|s| s.items).collect()
    }

    /// The number beside a list's name. For Mine, the ones that are my move.
    pub fn count(&self, mode: QueueMode) -> usize {
        if mode == QueueMode::Mine {
            return self.mine().iter().filter(|m| self.status_of(m).standing.on_me()).count();
        }
        self.items(mode).len()
    }

    // MARK: Mine

    pub fn mine(&self) -> &[MergeRequest] {
        self.accounts.get(&self.forge).map_or(&[], |a| &a.mine)
    }

    pub fn mine_error(&self) -> Option<&str> {
        self.accounts.get(&self.forge).and_then(|a| a.mine_error.as_deref())
    }

    pub fn is_mine(&self, item: &MergeRequest) -> bool {
        self.accounts.get(&item.forge).is_some_and(|a| a.mine.iter().any(|m| m.id == item.id))
    }

    /// Everything open in an account, mine or asked of me: where stacks are looked for.
    fn open_in(&self, forge: Forge) -> Vec<&MergeRequest> {
        self.accounts.get(&forge).map(|a| a.mine.iter().chain(&a.open).collect()).unwrap_or_default()
    }

    /// Where one of my merge requests stands, knowing what it's stacked on.
    pub fn status_of(&self, item: &MergeRequest) -> Status {
        let all = self.open_in(item.forge);
        mine::status(item, mine::below(item, &all))
    }

    /// The stack a merge request is part of, with its parts, bottom first.
    pub fn stack_of(&self, item: &MergeRequest) -> Option<(Stack, Vec<MergeRequest>)> {
        let all = self.open_in(item.forge);
        let stack = mine::stacks(&all).into_iter().find(|s| s.ids.contains(&item.id))?;
        let parts = stack.ids.iter().filter_map(|id| all.iter().find(|m| &m.id == id).map(|m| (*m).clone())).collect();
        Some((stack, parts))
    }

    /// How many of mine stand like `filter` (quiet and held ones count as waiting).
    pub fn mine_count(&self, filter: Standing) -> usize {
        self.mine().iter().filter(|m| self.status_of(m).standing.group() == filter).count()
    }

    pub fn toggle_stack(&mut self, key: &str) {
        if !self.open_stacks.remove(key) {
            self.open_stacks.insert(key.to_string());
        }
    }

    /// Shows every part of the selected request's stack, or folds it again. With no stack
    /// selected it does the same to all of them, so a stack folded to one row can be opened
    /// from the keyboard.
    pub fn unfold_stacks(&mut self, open: bool) {
        let refs: Vec<&MergeRequest> = self.mine().iter().collect();
        let stacks = mine::stacks(&refs);
        let selected = self.selected_id.as_deref();
        let keys: Vec<String> = match stacks.iter().find(|s| selected.is_some_and(|id| s.ids.iter().any(|i| i == id))) {
            Some(stack) => vec![stack.key.clone()],
            None => stacks.into_iter().map(|s| s.key).collect(),
        };
        for key in keys {
            if open {
                self.open_stacks.insert(key);
            } else {
                self.open_stacks.remove(&key);
            }
        }
    }

    /// The Mine list: my move first, then what's waiting on reviewers, then drafts. A stack is
    /// one group, placed where its most actionable part belongs, showing the parts that are on
    /// me and folding the rest. With a filter or a search the list is flat.
    pub fn mine_lines(&self) -> Vec<MineLine> {
        let query = self.filter.trim().to_lowercase();
        let all: Vec<(&MergeRequest, Status)> = self.mine().iter().map(|m| (m, self.status_of(m))).collect();
        let refs: Vec<&MergeRequest> = all.iter().map(|(m, _)| *m).collect();
        let stacks = mine::stacks(&refs);
        let place = |id: &str| stacks.iter().find_map(|s| s.ids.iter().position(|i| i == id).map(|at| (at + 1, s.ids.len())));
        let newest_first =
            |a: &(&MergeRequest, Status), b: &(&MergeRequest, Status)| a.1.standing.rank().cmp(&b.1.standing.rank()).then(b.0.updated_at.cmp(&a.0.updated_at));
        let mut lines = Vec::new();

        if self.mine_filter.is_some() || !query.is_empty() {
            let mut found: Vec<&(&MergeRequest, Status)> =
                all.iter().filter(|(m, st)| self.mine_filter.is_none_or(|f| st.standing.group() == f) && (query.is_empty() || matches(m, &query))).collect();
            found.sort_by(|a, b| newest_first(a, b));
            if !found.is_empty() {
                lines.push(MineLine::Header(self.mine_filter.map_or("Matches", Standing::label)));
            }
            lines.extend(found.into_iter().map(|(m, st)| MineLine::Item { item: (*m).clone(), status: st.clone(), rail: None, place: place(&m.id) }));
            return lines;
        }

        // A unit is one request or one stack, sorted by its most actionable part.
        let mut units: Vec<(Vec<&(&MergeRequest, Status)>, Option<&Stack>)> = Vec::new();
        for stack in &stacks {
            units.push((stack.ids.iter().filter_map(|id| all.iter().find(|(m, _)| &m.id == id)).collect(), Some(stack)));
        }
        units.extend(all.iter().filter(|(m, _)| place(&m.id).is_none()).map(|one| (vec![one], None)));
        let best = |unit: &Vec<&(&MergeRequest, Status)>| unit.iter().copied().min_by(|a, b| newest_first(a, b)).map(|(m, st)| (st.standing, m.updated_at));
        units.sort_by(|a, b| {
            let (a, b) = (best(&a.0), best(&b.0));
            a.map(|x| x.0.rank()).cmp(&b.map(|x| x.0.rank())).then(b.map(|x| x.1).cmp(&a.map(|x| x.1)))
        });
        let section = |standing: Standing| match standing {
            s if s.on_me() => "Your move",
            Standing::Draft => "Drafts",
            _ => "Waiting on reviewers",
        };
        let mut title = "";
        for (parts, stack) in units {
            let Some((standing, _)) = best(&parts) else { continue };
            if section(standing) != title {
                title = section(standing);
                lines.push(MineLine::Header(title));
            }
            let Some(stack) = stack else {
                let (m, st) = parts[0];
                lines.push(MineLine::Item { item: (*m).clone(), status: st.clone(), rail: None, place: None });
                continue;
            };
            let open = self.open_stacks.contains(&stack.key);
            let shown = |(m, st): &(&MergeRequest, Status)| open || st.standing.on_me() || self.selected_id.as_deref() == Some(m.id.as_str());
            let tally = |parts: &[&(&MergeRequest, Status)]| {
                let mut counts: Vec<(&str, usize)> = Vec::new();
                for (_, st) in parts {
                    match counts.iter_mut().find(|(w, _)| *w == st.standing.word()) {
                        Some(c) => c.1 += 1,
                        None => counts.push((st.standing.word(), 1)),
                    }
                }
                counts.iter().map(|(w, n)| format!("{n} {w}")).collect::<Vec<_>>().join(", ")
            };
            if !parts.iter().any(|p| shown(p)) {
                lines
                    .push(MineLine::Whole { key: stack.key.clone(), name: stack.name.clone(), text: format!("Stack of {} · {}", parts.len(), tally(&parts)) });
                continue;
            }
            let merging = stack.ids.iter().any(|id| self.walk_of(id).is_some());
            lines.push(MineLine::StackHead { key: stack.key.clone(), name: stack.name.clone(), count: parts.len(), open, merging });
            let from = lines.len();
            let mut run: Vec<&(&MergeRequest, Status)> = Vec::new();
            let rail = Rail { first: false, last: false };
            let fold = |run: &mut Vec<&(&MergeRequest, Status)>, lines: &mut Vec<MineLine>| {
                if !run.is_empty() {
                    lines.push(MineLine::Folded { key: stack.key.clone(), text: format!("{} more · {}", run.len(), tally(run)), rail });
                    run.clear();
                }
            };
            for part in parts {
                if shown(part) {
                    fold(&mut run, &mut lines);
                    lines.push(MineLine::Item { item: (*part.0).clone(), status: part.1.clone(), rail: Some(rail), place: None });
                } else {
                    run.push(part);
                }
            }
            fold(&mut run, &mut lines);
            let last = lines.len() - 1;
            for (i, line) in lines.iter_mut().enumerate().skip(from) {
                if let MineLine::Item { rail: Some(r), .. } | MineLine::Folded { rail: r, .. } = line {
                    *r = Rail { first: i == from, last: i == last };
                }
            }
        }
        lines
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

    /// Whether to put a dot on it: for mine, a new event in the inbox (so my own pushes don't
    /// light it); for a review, activity since I last opened it.
    pub fn unread(&self, item: &MergeRequest) -> bool {
        if self.is_mine(item) { self.has_news(&item.id) } else { self.is_unread(item) }
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
        if id != self.selected_id {
            if let Some(id) = &id {
                match self.last_seen.get(id) {
                    Some(&at) => self.seen_before.insert(id.clone(), at),
                    None => self.seen_before.remove(id),
                };
            }
        }
        self.selected_id = id;
        let Some(item) = self.selected().cloned() else { return };
        self.mark_seen(&item);
        self.mark_request_done(&item.id);
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

    /// A note someone else wrote since I last looked at its merge request. Nothing is fresh on
    /// a first visit.
    pub fn is_fresh(&self, item: &MergeRequest, note: &Note) -> bool {
        let Some(&seen) = self.seen_before.get(&item.id) else { return false };
        !note.system && self.me().is_none_or(|me| me.username != note.author.username) && note.created_at.timestamp() as f64 > seen
    }

    pub fn has_fresh(&self, item: &MergeRequest, thread: &Discussion) -> bool {
        thread.notes.iter().any(|n| self.is_fresh(item, n))
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
        // The accounts that loaded, with any of mine a search still listed after it finished.
        let mut fresh: Vec<(Forge, Vec<MergeRequest>)> = Vec::new();
        for (forge, result) in results {
            let account = self.accounts.entry(forge).or_default();
            match result {
                Ok(q) => {
                    account.me = Some(q.me);
                    account.open = q.open;
                    account.done = q.done;
                    // A search can lag a merge by a few seconds and still list it.
                    let (open, ended): (Vec<MergeRequest>, Vec<MergeRequest>) = q.mine.into_iter().partition(MergeRequest::is_open);
                    account.mine = open;
                    fresh.push((forge, ended));
                    account.mine_error = q.mine_error;
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
        // Before the walks move on: a part they merged is still on record as theirs.
        effects.notify = self.note_changes(&fresh, every_loaded);
        self.advance_walks();
        effects.badge = Some(self.accounts.values().flat_map(|a| &a.open).filter(|m| m.my_state.needs_review() && !m.draft).count());
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

    /// Merges one of my merge requests, if nothing blocks it.
    pub fn merge(&mut self, item: &MergeRequest) {
        if !self.is_mine(item) || self.status_of(item).standing != Standing::Ready {
            self.show("It isn't ready to merge.");
            return;
        }
        let it = item.clone();
        self.perform(&format!("Merging {}…", item.reference()), format!("Merged {}", item.short_reference()), true, None, move || it.kind().merge(&it));
    }

    /// Merges a stack from its bottom part up: `bottom` now, then each of my parts above it
    /// once it's ready. It stops at the first part that needs someone.
    pub fn merge_stack(&mut self, bottom: &MergeRequest) {
        let Some((_, parts)) = self.stack_of(bottom) else { return };
        if parts.first().map(|m| &m.id) != Some(&bottom.id) || self.status_of(bottom).standing != Standing::Ready {
            self.show("Start from the bottom of the stack, once it's ready to merge.");
            return;
        }
        let ids: Vec<String> = parts.iter().take_while(|m| self.is_mine(m)).map(|m| m.id.clone()).collect();
        self.walks.retain(|w| !w.ids.iter().any(|id| ids.contains(id)));
        self.walks.push(Walk { ids, target: bottom.target_branch.clone(), forge: bottom.forge, merged: vec![], moved_at: Utc::now() });
        self.advance_walks();
    }

    /// The walk a merge request is part of, if its stack is being merged.
    pub fn walk_of(&self, id: &str) -> Option<&Walk> {
        self.walks.iter().find(|w| w.ids.iter().any(|i| i == id))
    }

    pub fn stop_walk(&mut self, id: &str) {
        self.walks.retain(|w| !w.ids.iter().any(|i| i == id));
        self.show("Stopped merging the stack");
    }

    /// Whether a stack is being merged, which wants the queue refreshed often.
    pub fn walking(&self) -> bool {
        !self.walks.is_empty()
    }

    /// Takes each walk's next step, from what the forge last said. One action runs at a time;
    /// its result refreshes the queue, which comes back here.
    pub(super) fn advance_walks(&mut self) {
        // A newer fetch is on its way (an action just finished): what's here may predate it.
        if self.refreshing {
            return;
        }
        let mut at = 0;
        while at < self.walks.len() {
            if self.busy.is_some() {
                return;
            }
            let walk = self.walks[at].clone();
            // An account that failed to load says nothing about what has merged.
            let Some(account) = self.accounts.get(&walk.forge).filter(|a| a.error.is_none() && a.mine_error.is_none()) else {
                at += 1;
                continue;
            };
            let left: Vec<String> = walk.ids.iter().filter(|id| account.mine.iter().any(|m| &m.id == *id)).cloned().collect();
            if walk.ids.iter().any(|id| !left.contains(id) && !walk.merged.contains(id)) {
                self.walks.remove(at);
                self.show("Stopped merging the stack: a part was closed or merged somewhere else");
                continue;
            }
            let next = left.first().and_then(|id| account.mine.iter().find(|m| &m.id == id)).cloned();
            let step = mine::walk_step(next.as_ref(), &walk.target, &self.open_in(walk.forge));
            self.walks[at].ids = left;
            let waited = (Utc::now() - walk.moved_at).num_minutes();
            match (step, next) {
                (WalkStep::Done, _) | (_, None) => {
                    self.walks.remove(at);
                    self.show("The stack is merged");
                }
                (WalkStep::Stop(why), Some(next)) => {
                    self.walks.remove(at);
                    self.show(&format!("Stopped merging the stack at {}: {why}", next.reference()));
                }
                (WalkStep::Wait, Some(next)) if waited >= WALK_PATIENCE_MINUTES => {
                    self.walks.remove(at);
                    self.show(&format!("Stopped merging the stack: {} still isn't ready after {waited} minutes", next.reference()));
                }
                (WalkStep::Wait, _) => at += 1,
                (WalkStep::Retarget(branch), Some(next)) => {
                    self.walks[at].moved_at = Utc::now();
                    let label = format!("Moving {} onto {branch}…", next.reference());
                    let done = format!("Moved {} onto {branch}", next.reference());
                    self.perform(&label, done, true, None, move || next.kind().retarget(&next, &branch));
                    return;
                }
                (WalkStep::Merge, Some(next)) => {
                    self.walks[at].moved_at = Utc::now();
                    self.walks[at].merged.push(next.id.clone());
                    let label = format!("Merging {}…", next.reference());
                    self.perform(&label, format!("Merged {}", next.short_reference()), true, None, move || next.kind().merge(&next));
                    return;
                }
            }
        }
    }

    /// Has the forge merge it by itself once everything passes, or stops that.
    pub fn set_auto_merge(&mut self, item: &MergeRequest, on: bool) {
        let it = item.clone();
        let (label, done) =
            if on { ("Turning auto-merge on…", "It merges once everything passes") } else { ("Turning auto-merge off…", "Auto-merge is off") };
        self.perform(label, done.into(), true, None, move || it.kind().set_auto_merge(&it, on));
    }

    pub fn mark_ready(&mut self, item: &MergeRequest) {
        let it = item.clone();
        self.perform("Marking it ready…", format!("{} is ready for review", item.short_reference()), true, None, move || it.kind().mark_ready(&it));
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
                        // A failed step ends a stack's merge: nothing goes on without a look.
                        Err(e) if !self.walks.is_empty() => {
                            self.walks.clear();
                            self.show(&format!("Stopped merging the stack: {e}"));
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

    /// Compares the accounts that loaded with what the last refresh saw, adds what changed to the
    /// inbox, and returns the banners to post. The first look at an account finds nothing.
    fn note_changes(&mut self, fresh: &[(Forge, Vec<MergeRequest>)], every_loaded: bool) -> Vec<Notice> {
        if self.offline {
            return vec![];
        }
        let (prev, known, initialized, levels, banners) =
            prefs::read(|p| (p.inbox_snapshot.clone(), p.known_requests.clone(), p.known_requests_initialized, p.inbox_levels.clone(), p.notify));
        let forges: Vec<Forge> = fresh.iter().map(|(f, _)| *f).collect();
        let prev = prev.unwrap_or_else(|| {
            // Mrow before the inbox kept only the review requests it had notified about.
            let waiting: Vec<&MergeRequest> = self.accounts.values().flat_map(|a| &a.open).filter(|m| m.my_state.needs_review()).collect();
            if initialized { inbox::Snapshot::from_known(&known, &waiting, &forges) } else { inbox::Snapshot::default() }
        });
        let mut merged_here = self.merged_here.clone();
        merged_here.extend(self.walks.iter().flat_map(|w| w.merged.iter().cloned()));
        let ended: Vec<Vec<MergeRequest>> =
            fresh.iter().map(|(forge, lagged)| self.accounts.get(forge).map(|a| a.done.iter().chain(lagged).cloned().collect()).unwrap_or_default()).collect();
        let looks: Vec<inbox::Look> = fresh
            .iter()
            .zip(&ended)
            .filter_map(|((forge, _), ended)| {
                let a = self.accounts.get(forge)?;
                Some(inbox::Look {
                    forge: *forge,
                    me: a.me.as_ref().map_or("", |m| m.username.as_str()),
                    open: &a.open,
                    mine: a.mine_error.is_none().then_some(a.mine.as_slice()),
                    mine_capped: a.mine.len() >= MINE_PAGE_FLOOR,
                    ended,
                })
            })
            .collect();
        let now = Utc::now();
        let (next, mut found) = inbox::detect(&prev, &looks, &merged_here, now);
        found.retain(|e| inbox::level(&levels, e.kind) != inbox::Level::Off);
        let notices = inbox::notices(&found, &levels, banners);
        inbox::add(&mut self.inbox, found);
        // A review request is dealt with once it no longer waits on me.
        let waiting: HashSet<&str> = looks.iter().flat_map(|l| l.open.iter()).filter(|m| m.my_state.needs_review()).map(|m| m.id.as_str()).collect();
        for e in self.inbox.iter_mut().filter(|e| !e.kind.on_mine() && forges.contains(&e.forge)) {
            if !waiting.contains(e.request.as_str()) {
                e.done = true;
            }
        }
        // Only a full set of lists says a request is gone.
        let whole = every_loaded && self.accounts.values().all(|a| a.error.is_none() && a.mine_error.is_none());
        let live: HashSet<String> = self.every_item().map(|m| m.id.clone()).collect();
        inbox::prune(&mut self.inbox, now, whole.then_some(&live));
        if whole {
            self.merged_here.retain(|id| live.contains(id));
        }
        let events = self.inbox.clone();
        prefs::update(|p| {
            p.inbox_snapshot = Some(next);
            p.inbox = events;
            p.known_requests.clear();
        });
        notices
    }

    // MARK: Inbox

    /// The events not dealt with yet, newest first.
    pub fn new_events(&self) -> impl Iterator<Item = &Event> {
        self.inbox.iter().filter(|e| !e.done)
    }

    /// The bell's count, across every account.
    pub fn news_count(&self) -> usize {
        self.new_events().count()
    }

    /// Whether a merge request has an event not dealt with.
    pub fn has_news(&self, id: &str) -> bool {
        self.new_events().any(|e| e.request == id)
    }

    fn save_inbox(&self) {
        if !self.offline {
            let events = self.inbox.clone();
            prefs::update(|p| p.inbox = events);
        }
    }

    pub fn mark_done(&mut self, event: &str) {
        if let Some(e) = self.inbox.iter_mut().find(|e| e.id == event && !e.done) {
            e.done = true;
            self.save_inbox();
        }
    }

    pub fn mark_all_done(&mut self) {
        if self.inbox.iter().any(|e| !e.done) {
            self.inbox.iter_mut().for_each(|e| e.done = true);
            self.save_inbox();
        }
    }

    /// Opening a request deals with everything the inbox says about it.
    fn mark_request_done(&mut self, id: &str) {
        if self.has_news(id) {
            self.inbox.iter_mut().filter(|e| e.request == id).for_each(|e| e.done = true);
            self.save_inbox();
        }
    }

    /// Shows a merge request in the list it's in (Mine for my own, To review or Reviewed for a
    /// review), bringing its account along. Returns its address when it has left every list,
    /// to open on the forge instead.
    pub fn open_request(&mut self, id: &str) -> Option<String> {
        let Some(item) = self.item(id).cloned() else {
            let url = self.inbox.iter().find(|e| e.request == id).map(|e| e.url.clone());
            self.mark_request_done(id);
            return url.filter(|u| !u.is_empty());
        };
        self.mode = if self.is_mine(&item) {
            QueueMode::Mine
        } else if !item.is_open() {
            QueueMode::Done
        } else if item.my_state.needs_review() {
            QueueMode::ToReview
        } else {
            QueueMode::Reviewed
        };
        self.select(Some(item.id));
        None
    }

    /// Opens what an event is about and marks it done.
    pub fn open_event(&mut self, event: &str) -> Option<String> {
        let request = self.inbox.iter().find(|e| e.id == event)?.request.clone();
        self.mark_done(event);
        self.open_request(&request)
    }

    /// Merges one of mine from here (see `merge`), so its leaving the list isn't news.
    pub fn merge_here(&mut self, item: &MergeRequest) {
        self.merge(item);
        if self.busy.is_some() {
            self.merged_here.insert(item.id.clone());
        }
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
    pub notify: Vec<Notice>,
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
