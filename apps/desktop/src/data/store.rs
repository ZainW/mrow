//! Everything the window shows: the review queues from each forge, which one the sidebar is on,
//! what's selected, tabs, read state, and details as they load. Plain data with methods; the
//! background work it starts comes back as `Msg`s through `land`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use chrono::{Local, Utc};

use super::bg::Bg;
use super::diff::{self, DiffLine, Gap, Reveal, Shown};
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
    /// A thread was resolved or reopened, already shown so; `was` is how it stood before, to
    /// go back to if the forge said no.
    Resolved {
        id: String,
        thread: String,
        was: bool,
        result: Result<(), String>,
    },
    /// An approve, comment or reply finished. `reload` names a merge request whose threads to reload.
    Done {
        result: Result<(), String>,
        message: String,
        refresh: bool,
        reload: Option<MergeRequest>,
        ticket: Option<u64>,
    },
    /// The jobs of a merge request's pipeline (or its checks).
    Jobs {
        id: String,
        request: u64,
        result: Result<Vec<Job>, String>,
    },
    /// A job was retried (or run), already shown as pending; `was` is how it stood before.
    Retried {
        id: String,
        job: i64,
        name: String,
        was: PipelineStatus,
        result: Result<(), String>,
    },
    JobLog {
        key: String,
        request: u64,
        result: Result<JobLog, String>,
    },
    /// A comment of mine was edited or deleted, already shown so; `was` is its thread before,
    /// `at` its place in the list, to put back if the forge said no.
    NoteChanged {
        id: String,
        was: Discussion,
        at: usize,
        deleted: bool,
        ticket: u64,
        result: Result<(), String>,
    },
    /// People who can review in a project, for the reviewer picker. `key` is `people_key`'s.
    People {
        key: (String, String),
        result: Result<Vec<Person>, String>,
    },
    /// A change to one of my requests, already shown; `was` is the request before it, put
    /// back if the forge said no.
    Edited {
        was: MergeRequest,
        result: Result<(), String>,
        message: String,
        ticket: u64,
    },
    /// A file's text at a commit, by `text_key`, for the lines its diff leaves out.
    FileText {
        key: String,
        request: u64,
        result: Result<String, String>,
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
    /// The pipeline's jobs (or the checks) of the requests looked at, loaded when their overview shows.
    pub jobs: HashMap<String, Loadable<Rc<Vec<Job>>>>,
    /// Jobs' logs, by `log_key`.
    pub job_logs: HashMap<String, Loadable<Rc<JobLog>>>,
    /// The logs open under their jobs, by `log_key`.
    pub open_logs: HashSet<String>,
    /// The request whose running jobs are watched, and when they're next asked for.
    jobs_poll: Option<(String, f64)>,
    /// People who can review, by project and search, for the reviewer picker.
    pub people: HashMap<(String, String), Loadable<Rc<Vec<Person>>>>,
    /// Files' texts at a commit, by `text_key`, as lines: what a diff's gaps show.
    pub file_texts: HashMap<String, Loadable<Rc<Vec<String>>>>,
    /// How much of each gap shows, by "merge request id|file id|digest", then the gap's first line.
    pub shown: HashMap<String, BTreeMap<i64, Shown>>,
    /// Counts changes to `file_texts` and `shown`, so a view knows when to list its rows again.
    pub context_version: u64,
    /// The requests whose changes and threads were loaded before they were opened, and when:
    /// their threads load again on the visit if they've aged since.
    pub fetched_ahead: HashMap<String, Date>,
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
            jobs: HashMap::new(),
            job_logs: HashMap::new(),
            open_logs: HashSet::new(),
            jobs_poll: None,
            people: HashMap::new(),
            file_texts: HashMap::new(),
            shown: HashMap::new(),
            context_version: 0,
            fetched_ahead: HashMap::new(),
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
                self.start_visit(id);
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
        if self.fetched_ahead.remove(&item.id).is_some_and(|at| stale_ahead(at, Utc::now())) {
            self.load_discussions(&item);
        }
        self.load_details_if_needed(&item);
        self.fetch_ahead();
    }

    /// Keeps when a merge request was last seen, before showing it marks it seen again: what
    /// `is_fresh` measures against. Call it wherever the selection moves to another one.
    fn start_visit(&mut self, id: &str) {
        match self.last_seen.get(id) {
            Some(&at) => self.seen_before.insert(id.to_string(), at),
            None => self.seen_before.remove(id),
        };
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

    /// How many of `files` are marked viewed as they stand now.
    pub fn viewed_count(&self, files: &[FileDiff], item: &MergeRequest) -> usize {
        match self.viewed_files.get(&item.id) {
            Some(marked) if !marked.is_empty() => files.iter().filter(|f| marked.get(&f.id()) == Some(&f.digest)).count(),
            _ => 0,
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
            // The one a launch opens too: its replies since last time are new.
            if let Some(id) = &next {
                self.start_visit(id);
            }
            self.selected_id = next;
        }
        // Changes and threads may have moved on; fetch them again when next shown.
        let selected = self.selected_id.clone();
        // A load ahead still on its way would land data from before this refresh: drop it.
        let ahead: Vec<String> = self.fetched_ahead.drain().map(|(id, _)| id).filter(|id| Some(id) != selected.as_ref()).collect();
        for id in ahead {
            self.start_request(format!("diffs {id}"));
            self.start_request(format!("threads {id}"));
        }
        self.diffs.retain(|k, _| Some(k) == selected.as_ref());
        self.discussions.retain(|k, _| Some(k) == selected.as_ref());
        self.forget_jobs(selected.as_deref());
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

    /// Resolves a thread, or reopens it. It shows so at once, and goes back if the forge refuses.
    pub fn set_resolved(&mut self, item: &MergeRequest, thread: &Discussion, resolved: bool) {
        if self.offline || !self.show_resolved(&item.id, &thread.id, resolved) {
            return;
        }
        let (it, th) = (item.clone(), thread.clone());
        let (id, thread) = (item.id.clone(), thread.id.clone());
        self.bg.spawn(move || Msg::Resolved { id, thread, was: !resolved, result: it.kind().set_resolved(&it, &th, resolved) });
    }

    /// Marks a loaded thread's notes resolved or not. False when the thread isn't there.
    fn show_resolved(&mut self, id: &str, thread: &str, resolved: bool) -> bool {
        let Some(Loadable::Loaded(threads)) = self.discussions.get(id) else { return false };
        let Some(at) = threads.iter().position(|t| t.id == thread) else { return false };
        let mut threads = threads.as_ref().clone();
        for note in threads[at].notes.iter_mut().filter(|n| n.resolvable == Some(true)) {
            note.resolved = Some(resolved);
        }
        self.discussions.insert(id.to_string(), Loadable::Loaded(Rc::new(threads)));
        true
    }

    // MARK: Reviews

    /// My comments held in the review I haven't submitted, as last loaded.
    pub fn pending_count(&self, item: &MergeRequest) -> usize {
        let threads = self.discussions.get(&item.id).and_then(Loadable::value);
        threads.map_or(0, |t| t.iter().flat_map(|t| &t.notes).filter(|n| n.pending).count())
    }

    /// Holds a comment on the whole request in my review. The ticket's result lands in `finished`.
    pub fn hold(&mut self, item: &MergeRequest, body: String) -> u64 {
        let it = item.clone();
        self.perform("Adding to your review…", "Added to your review".into(), false, Some(item.clone()), move || it.kind().draft_comment(&it, &body))
    }

    /// Holds a comment on a line in my review, like `comment_on_line`.
    pub fn hold_on_line(&mut self, item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: Option<DiffRefs>, body: String) -> u64 {
        let Some(refs) = refs else {
            self.show("The changes moved while loading. Refresh, then comment on the line again.");
            let t = self.ticket();
            self.finished.insert(t, false);
            return t;
        };
        let (it, file, line) = (item.clone(), file.clone(), line.clone());
        self.perform("Adding to your review…", "Added to your review".into(), false, Some(item.clone()), move || {
            it.kind().draft_on_line(&it, &file, &line, &refs, &body)
        })
    }

    pub fn edit_draft(&mut self, item: &MergeRequest, note: &Note, body: String) -> u64 {
        let (it, note) = (item.clone(), note.clone());
        self.perform("Saving…", "Saved".into(), false, Some(item.clone()), move || it.kind().edit_draft(&it, &note, &body))
    }

    pub fn delete_draft(&mut self, item: &MergeRequest, note: &Note) -> u64 {
        let (it, note) = (item.clone(), note.clone());
        self.perform("Deleting…", "Deleted from your review".into(), false, Some(item.clone()), move || it.kind().delete_draft(&it, &note))
    }

    /// Submits my review with a verdict and an optional summary, then reloads its threads and
    /// the queue, as approving does.
    pub fn submit_review(&mut self, item: &MergeRequest, verdict: Verdict, body: String) -> u64 {
        let done = match verdict {
            Verdict::Comment => format!("Reviewed {}", item.short_reference()),
            Verdict::Approve => format!("Approved {}", item.short_reference()),
            Verdict::RequestChanges => format!("Requested changes on {}", item.short_reference()),
        };
        let it = item.clone();
        self.perform("Submitting your review…", done, true, Some(item.clone()), move || it.kind().submit_review(&it, verdict, &body))
    }

    // MARK: My comments

    /// Whether I wrote a posted, non-system note, so I can edit or delete it.
    pub fn wrote(&self, item: &MergeRequest, note: &Note) -> bool {
        let me = self.accounts.get(&item.forge).and_then(|a| a.me.as_ref());
        !note.system && !note.pending && me.is_some_and(|me| me.username == note.author.username)
    }

    /// Replaces the text of a comment of mine with `body` (what I wrote, without the verdict a
    /// review's summary shows). It shows so at once, and goes back if the forge refuses. The
    /// ticket's result lands in `finished`.
    pub fn edit_note(&mut self, item: &MergeRequest, thread: &str, note: i64, body: String) -> u64 {
        let it = item.clone();
        let edit = body.clone();
        self.change_note(item, thread, note, Some(body), move |th, n| it.kind().edit_note(&it, th, n, &edit))
    }

    /// Deletes a comment of mine, and its thread with it when it was the only one.
    pub fn delete_note(&mut self, item: &MergeRequest, thread: &str, note: i64) -> u64 {
        let it = item.clone();
        self.change_note(item, thread, note, None, move |th, n| it.kind().delete_note(&it, th, n))
    }

    /// Shows the edit (`Some`) or deletion (`None`) at once and runs `work` on the thread and
    /// note as they stood. Snapshots and benchmarks show it without asking the forge.
    fn change_note(
        &mut self,
        item: &MergeRequest,
        thread: &str,
        note: i64,
        body: Option<String>,
        work: impl FnOnce(&Discussion, &Note) -> Result<(), String> + Send + 'static,
    ) -> u64 {
        let ticket = self.ticket();
        let deleted = body.is_none();
        let Some((was, at, before)) = self.show_note_change(item, thread, note, body) else {
            self.finished.insert(ticket, false);
            return ticket;
        };
        if self.offline {
            self.finished.insert(ticket, true);
            return ticket;
        }
        let id = item.id.clone();
        self.bg.spawn(move || {
            let result = work(&was, &before);
            Msg::NoteChanged { id, was, at, deleted, ticket, result }
        });
        ticket
    }

    /// Puts an edit or deletion into the loaded threads: the thread as it was, its place, and
    /// the note as it was. None when the note isn't there.
    fn show_note_change(&mut self, item: &MergeRequest, thread: &str, note: i64, body: Option<String>) -> Option<(Discussion, usize, Note)> {
        let Some(Loadable::Loaded(threads)) = self.discussions.get(&item.id) else { return None };
        let at = threads.iter().position(|t| t.id == thread)?;
        let was = threads[at].clone();
        let index = was.notes.iter().position(|n| n.id == note && !n.pending)?;
        let before = was.notes[index].clone();
        let mut threads = threads.as_ref().clone();
        match body {
            Some(body) => {
                let (verdict, _) = item.kind().written(&was, &before);
                threads[at].notes[index].body = format!("{verdict}{body}");
            }
            None => {
                threads[at].notes.remove(index);
                if threads[at].notes.iter().all(|n| n.system) {
                    threads.remove(at);
                }
            }
        }
        self.discussions.insert(item.id.clone(), Loadable::Loaded(Rc::new(threads)));
        Some((was, at, before))
    }

    /// Puts a thread back as it was before an edit or deletion the forge refused.
    fn restore_thread(&mut self, id: &str, was: Discussion, at: usize) {
        let Some(Loadable::Loaded(threads)) = self.discussions.get(id) else { return };
        let mut threads = threads.as_ref().clone();
        match threads.iter().position(|t| t.id == was.id) {
            Some(i) => threads[i] = was,
            None => threads.insert(at.min(threads.len()), was),
        }
        self.discussions.insert(id.to_string(), Loadable::Loaded(Rc::new(threads)));
    }

    /// `change_note` with the forge's answer given, for the self-tests.
    #[cfg(test)]
    pub fn change_note_answered(&mut self, item: &MergeRequest, thread: &str, note: i64, body: Option<String>, answer: Result<(), String>) -> u64 {
        self.offline = false;
        let ticket = self.change_note(item, thread, note, body, move |_, _| answer);
        self.offline = true;
        ticket
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
                Msg::Resolved { id, thread, was, result } => match result {
                    Ok(()) => {
                        self.show(if was { "Reopened the thread" } else { "Resolved the thread" });
                        // My own list counts the open threads.
                        if self.item(&id).is_some_and(|m| self.is_mine(m)) {
                            self.refresh();
                        }
                    }
                    Err(e) => {
                        self.show_resolved(&id, &thread, was);
                        let verb = if was { "reopen" } else { "resolve" };
                        self.show(&format!("Couldn’t {verb} the thread: {e}"));
                    }
                },
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
                Msg::Jobs { id, request, result } => self.landed_jobs(id, request, result),
                Msg::Retried { id, job, name, was, result } => self.landed_retry(id, job, &name, was, result),
                Msg::JobLog { key, request, result } => {
                    if self.requests.get(&format!("log {key}")) == Some(&request) {
                        self.job_logs.insert(key, result.map_or_else(Loadable::Failed, |log| Loadable::Loaded(Rc::new(log))));
                    }
                }
                Msg::NoteChanged { id, was, at, deleted, ticket, result } => {
                    self.finished.insert(ticket, result.is_ok());
                    match result {
                        Ok(()) => self.show(if deleted { "Deleted your comment" } else { "Saved your comment" }),
                        Err(e) => {
                            self.restore_thread(&id, was, at);
                            let verb = if deleted { "delete" } else { "save" };
                            self.show(&format!("Couldn’t {verb} your comment: {e}"));
                        }
                    }
                    // Either way, the forge has the last word.
                    if let Some(item) = self.item(&id).cloned() {
                        self.load_discussions(&item);
                    }
                }
                Msg::People { key, result } => {
                    self.people.insert(key, result.map_or_else(Loadable::Failed, |found| Loadable::Loaded(Rc::new(found))));
                }
                Msg::Edited { was, result, message, ticket } => self.landed_edit(was, result, message, ticket),
                Msg::FileText { key, request, result } => self.landed_file_text(key, request, result),
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
        if changed {
            self.fetch_ahead();
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

/// How often the jobs of the request on screen are asked for again while one runs, in seconds.
const JOBS_EVERY: f64 = 15.0;

impl Store {
    // MARK: Jobs

    /// The key of a job's log in `job_logs` and `open_logs`.
    pub fn log_key(id: &str, job: i64) -> String {
        format!("{id}|{job}")
    }

    /// Loads the jobs of the request whose overview is `shown` the first time it shows, then asks
    /// again every few seconds while one runs or waits. Returns whether anything changed, and
    /// when to look next.
    pub fn tick_jobs(&mut self, now: f64, shown: Option<&str>) -> (bool, Option<f64>) {
        if self.offline {
            return (false, None);
        }
        let Some(item) = shown.and_then(|id| self.item(id)).cloned() else { return (false, None) };
        let jobs = match self.jobs.get(&item.id) {
            None if item.pipeline.is_some() => {
                self.load_jobs(&item);
                self.jobs_poll = Some((item.id.clone(), now + JOBS_EVERY));
                return (true, None);
            }
            Some(Loadable::Loaded(jobs)) => jobs.clone(),
            _ => return (false, None),
        };
        // A refresh of the queue asks for them again at once (`forget_jobs`), running or not.
        let due = self.jobs_poll.as_ref().filter(|(id, _)| *id == item.id).map(|(_, at)| *at);
        let forced = due == Some(0.0);
        if !forced && !jobs.iter().any(Job::active) {
            self.jobs_poll = None;
            return (false, None);
        }
        let due = due.unwrap_or(now + JOBS_EVERY);
        if now < due {
            self.jobs_poll = Some((item.id.clone(), due));
            return (false, Some(due));
        }
        self.load_jobs(&item);
        // A running job's open log grows as it runs.
        let growing: Vec<&Job> = jobs.iter().filter(|j| j.active() && self.open_logs.contains(&Self::log_key(&item.id, j.id))).collect();
        for job in growing {
            self.load_log(&item, job);
        }
        let next = now + JOBS_EVERY;
        self.jobs_poll = Some((item.id.clone(), next));
        (true, Some(next))
    }

    /// After a refresh: drops the jobs of requests not selected, and has the selected one's asked
    /// for again the next time they show.
    fn forget_jobs(&mut self, selected: Option<&str>) {
        // A load that failed goes too, so the next look tries again.
        self.jobs.retain(|k, v| Some(k.as_str()) == selected && v.value().is_some());
        self.job_logs.retain(|k, _| selected.is_some_and(|s| k.split_once('|').is_some_and(|(id, _)| id == s)));
        self.open_logs.retain(|k| selected.is_some_and(|s| k.split_once('|').is_some_and(|(id, _)| id == s)));
        if let Some(id) = selected.filter(|id| self.jobs.contains_key(*id)) {
            self.jobs_poll = Some((id.to_string(), 0.0));
        }
    }

    fn load_jobs(&mut self, item: &MergeRequest) {
        if self.offline {
            return;
        }
        if self.jobs.get(&item.id).and_then(Loadable::value).is_none() {
            self.jobs.insert(item.id.clone(), Loadable::Loading);
        }
        let request = self.start_request(format!("jobs {}", item.id));
        let item = item.clone();
        self.bg.spawn(move || Msg::Jobs { id: item.id.clone(), request, result: item.kind().jobs(&item) });
    }

    fn landed_jobs(&mut self, id: String, request: u64, result: Result<Vec<Job>, String>) {
        if self.requests.get(&format!("jobs {id}")) != Some(&request) {
            return;
        }
        match result {
            Ok(jobs) => {
                self.jobs.insert(id, Loadable::Loaded(Rc::new(jobs)));
            }
            // A poll that fails keeps what showed; the next one tries again.
            Err(_) if self.jobs.get(&id).and_then(Loadable::value).is_some() => {}
            Err(e) => {
                self.jobs.insert(id, Loadable::Failed(e));
            }
        }
    }

    /// Retries a failed or canceled job, or runs a manual one. It shows as pending at once, and
    /// goes back if the forge refuses.
    pub fn retry_job(&mut self, item: &MergeRequest, job: &Job) {
        if self.offline || !self.show_job_status(&item.id, job.id, PipelineStatus::Pending) {
            return;
        }
        // A reload already on its way predates the retry.
        self.start_request(format!("jobs {}", item.id));
        let (it, j) = (item.clone(), job.clone());
        self.bg.spawn(move || Msg::Retried { id: it.id.clone(), job: j.id, name: j.name.clone(), was: j.status, result: it.kind().retry_job(&it, &j) });
    }

    fn landed_retry(&mut self, id: String, job: i64, name: &str, was: PipelineStatus, result: Result<(), String>) {
        let run = was == PipelineStatus::Manual;
        match result {
            Ok(()) => {
                self.show(&if run { format!("Started {name}") } else { format!("Retrying {name}") });
                // The new run is a new job, with a log of its own.
                let key = Self::log_key(&id, job);
                self.open_logs.remove(&key);
                self.job_logs.remove(&key);
                if let Some(item) = self.item(&id).cloned() {
                    self.load_jobs(&item);
                }
            }
            Err(e) => {
                self.show_job_status(&id, job, was);
                let verb = if run { "run" } else { "retry" };
                self.show(&format!("Couldn’t {verb} {name}: {e}"));
            }
        }
    }

    /// Shows a loaded job with another status. False when it isn't there.
    fn show_job_status(&mut self, id: &str, job: i64, status: PipelineStatus) -> bool {
        let Some(Loadable::Loaded(jobs)) = self.jobs.get(id) else { return false };
        let Some(at) = jobs.iter().position(|j| j.id == job) else { return false };
        let mut jobs = jobs.as_ref().clone();
        jobs[at].status = status;
        if status == PipelineStatus::Pending {
            jobs[at].started_at = None;
            jobs[at].finished_at = None;
        }
        self.jobs.insert(id.to_string(), Loadable::Loaded(Rc::new(jobs)));
        true
    }

    /// The job that failed my own request's pipeline and can be retried from here, once its
    /// jobs are loaded: the one the queue named, else the first that failed outright.
    pub fn failed_job(&self, item: &MergeRequest) -> Option<&Job> {
        let jobs = self.jobs.get(&item.id).and_then(Loadable::value)?;
        let named = item.failed_job.as_ref().and_then(|name| jobs.iter().find(|j| &j.name == name && j.failed_hard()));
        named.or_else(|| jobs.iter().find(|j| j.failed_hard())).filter(|j| j.offers_retry())
    }

    /// Opens a job's log under it, loading it, or closes it.
    pub fn toggle_log(&mut self, item: &MergeRequest, job: &Job) {
        let key = Self::log_key(&item.id, job.id);
        if self.open_logs.remove(&key) {
            return;
        }
        self.open_logs.insert(key.clone());
        let stale = !matches!(self.job_logs.get(&key), Some(Loadable::Loaded(_) | Loadable::Loading)) || job.active();
        if stale {
            self.load_log(item, job);
        }
    }

    fn load_log(&mut self, item: &MergeRequest, job: &Job) {
        if self.offline {
            return;
        }
        let key = Self::log_key(&item.id, job.id);
        if self.job_logs.get(&key).and_then(Loadable::value).is_none() {
            self.job_logs.insert(key.clone(), Loadable::Loading);
        }
        let request = self.start_request(format!("log {key}"));
        let (it, j) = (item.clone(), job.clone());
        self.bg.spawn(move || Msg::JobLog { key, request, result: it.kind().job_log(&it, &j) });
    }
}

/// The most people one search returns; a list this long may be missing someone.
pub const PEOPLE_PAGE: usize = 100;

/// Managing a request of my own: reviewers, its branch, closing it, its title and description.
/// What's cheap to show (a reviewer, a title) shows at once and goes back if the forge refuses.
impl Store {
    /// Where a search for people in `item`'s project is kept: the project, and the search.
    pub fn people_key(item: &MergeRequest, query: &str) -> (String, String) {
        (format!("{}:{}", item.forge.raw(), item.project_path), query.trim().to_lowercase())
    }

    pub fn people(&self, item: &MergeRequest, query: &str) -> Option<&Loadable<Rc<Vec<Person>>>> {
        self.people.get(&Store::people_key(item, query))
    }

    /// Looks for people who can review in `item`'s project, unless that search is already
    /// here or on its way. A failed one is tried again.
    pub fn find_people(&mut self, item: &MergeRequest, query: &str) {
        let key = Store::people_key(item, query);
        if self.offline || matches!(self.people.get(&key), Some(Loadable::Loading | Loadable::Loaded(_))) {
            return;
        }
        self.people.insert(key.clone(), Loadable::Loading);
        let (it, query) = (item.clone(), query.trim().to_string());
        self.bg.spawn(move || Msg::People { key, result: it.kind().find_people(&it, &query) });
    }

    /// Changes `id` wherever it's listed, and returns how it was.
    fn reshape(&mut self, id: &str, change: impl Fn(&mut MergeRequest)) -> Option<MergeRequest> {
        let was = self.item(id).cloned()?;
        for account in self.accounts.values_mut() {
            for m in account.open.iter_mut().chain(account.done.iter_mut()).chain(account.mine.iter_mut()).filter(|m| m.id == id) {
                change(m);
            }
        }
        Some(was)
    }

    /// Runs `work` with the change already shown, as `perform` runs an action. The ticket's
    /// result lands in `finished`.
    fn perform_edit(
        &mut self,
        item: &MergeRequest,
        label: &str,
        message: String,
        change: impl Fn(&mut MergeRequest),
        work: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> u64 {
        let ticket = self.ticket();
        if self.offline {
            self.finished.insert(ticket, false);
            return ticket;
        }
        let was = self.reshape(&item.id, change).unwrap_or_else(|| item.clone());
        self.busy = Some(label.into());
        self.bg.spawn(move || Msg::Edited { was, result: work(), message, ticket });
        ticket
    }

    fn landed_edit(&mut self, was: MergeRequest, result: Result<(), String>, message: String, ticket: u64) {
        self.busy = None;
        self.finished.insert(ticket, result.is_ok());
        match result {
            Ok(()) => self.show(&message),
            Err(e) => {
                self.reshape(&was.id, |m| *m = was.clone());
                self.show(&e);
            }
        }
        // Either way the forge has the last word on what changed.
        self.refresh();
    }

    pub fn add_reviewer(&mut self, item: &MergeRequest, person: &Person) {
        if item.reviewers.iter().any(|r| r.person.username.eq_ignore_ascii_case(&person.username)) {
            return;
        }
        let (it, who) = (item.clone(), person.clone());
        let first = person.name.split(' ').next().unwrap_or(&person.name).to_string();
        let added = Reviewer { person: person.clone(), state: ReviewState::Unreviewed, approved: false };
        self.perform_edit(
            item,
            &format!("Asking {first}…"),
            format!("Asked {first} to review"),
            move |m| m.reviewers.push(added.clone()),
            move || it.kind().add_reviewer(&it, &who.username),
        );
    }

    pub fn remove_reviewer(&mut self, item: &MergeRequest, reviewer: &Reviewer) {
        if !mine::can_remove(item, reviewer) {
            self.show(&format!("{} already reviewed it, and {} keeps everyone who reviewed on the list.", reviewer.person.name, item.kind().name()));
            return;
        }
        let (it, who) = (item.clone(), reviewer.clone());
        let username = reviewer.person.username.clone();
        self.perform_edit(
            item,
            &format!("Removing {}…", reviewer.person.name),
            format!("Removed {}", reviewer.person.name),
            move |m| m.reviewers.retain(|r| r.person.username != username),
            move || it.kind().remove_reviewer(&it, &who),
        );
    }

    /// Asks these reviewers for another look. It counts as activity, so a quiet request goes
    /// back to waiting at once.
    pub fn ask_again(&mut self, item: &MergeRequest, reviewers: Vec<Reviewer>) {
        if reviewers.is_empty() {
            return;
        }
        let first: Vec<&str> = reviewers.iter().map(|r| r.person.name.split(' ').next().unwrap_or(&r.person.name)).collect();
        let who = match first.as_slice() {
            [one] => one.to_string(),
            [a, b] => format!("{a} and {b}"),
            [a, rest @ ..] => format!("{a} and {} others", rest.len()),
            [] => String::new(),
        };
        let names: Vec<String> = reviewers.iter().map(|r| r.person.username.clone()).collect();
        let it = item.clone();
        self.perform_edit(
            item,
            &format!("Asking {who} again…"),
            format!("Asked {who} again"),
            move |m| {
                m.updated_at = Utc::now();
                for r in m.reviewers.iter_mut().filter(|r| names.contains(&r.person.username)) {
                    r.state = ReviewState::Unreviewed;
                }
            },
            move || reviewers.iter().try_for_each(|r| it.kind().ask_again(&it, r)),
        );
    }

    /// Rebases it (GitLab) or merges the target in (GitHub).
    pub fn update_branch(&mut self, item: &MergeRequest) {
        let it = item.clone();
        let done = format!("{} {}", item.kind().updated_branch(), item.short_reference());
        self.perform(item.kind().updating_branch(), done, true, None, move || it.kind().update_branch(&it));
    }

    /// Closes it without merging, or reopens it.
    pub fn set_open(&mut self, item: &MergeRequest, open: bool) {
        let it = item.clone();
        let (label, done) = if open { ("Reopening…", "Reopened") } else { ("Closing…", "Closed") };
        self.perform_edit(
            item,
            label,
            format!("{done} {}", item.short_reference()),
            move |m| {
                m.state = if open { "opened" } else { "closed" }.into();
                m.closed_at = (!open).then(Utc::now);
            },
            move || it.kind().set_open(&it, open),
        );
    }

    /// Saves a new title and description, sending only what changed. `title` is as typed: on a
    /// GitLab draft, its prefix goes back on (see `mine::edited_title`).
    pub fn edit_request(&mut self, item: &MergeRequest, title: &str, description: &str) -> u64 {
        let title = mine::edited_title(item, title);
        let new_title = (title != item.title).then(|| title.clone());
        let new_description = (description.trim_end() != item.description.trim_end()).then(|| description.trim_end().to_string());
        if new_title.is_none() && new_description.is_none() {
            let t = self.ticket();
            self.finished.insert(t, true);
            return t;
        }
        let it = item.clone();
        let (t, d) = (new_title.clone(), new_description.clone());
        self.perform_edit(
            item,
            "Saving…",
            "Saved".into(),
            move |m| {
                if let Some(t) = &t {
                    m.title = t.clone();
                }
                if let Some(d) = &d {
                    m.description = d.clone();
                }
            },
            move || it.kind().edit(&it, new_title.as_deref(), new_description.as_deref()),
        )
    }
}

// MARK: Loading ahead

/// How many requests load ahead at once.
const AHEAD: usize = 2;

/// How old threads loaded ahead can be when their request is opened before they load again.
const AHEAD_FRESH_SECONDS: i64 = 120;

/// Whether threads loaded ahead at `at` have aged past use by `now`.
pub fn stale_ahead(at: Date, now: Date) -> bool {
    (now - at).num_seconds() > AHEAD_FRESH_SECONDS
}

impl Store {
    /// The requests to load ahead, so moving on through the list lands at once: once the
    /// selection's own changes and threads are in, the one after it in the list, then the one
    /// before; with nothing selected, the list's first. Leaves out what's loaded or loading, and
    /// what's beyond `AHEAD` loads on their way.
    pub fn ahead(&self) -> Vec<MergeRequest> {
        fn settled<T>(l: Option<&Loadable<T>>) -> bool {
            matches!(l, Some(Loadable::Loaded(_) | Loadable::Failed(_)))
        }
        let items = self.visible_items();
        let near: Vec<usize> = match &self.selected_id {
            None => vec![0],
            Some(id) => {
                if !settled(self.diffs.get(id)) || !settled(self.discussions.get(id)) {
                    return Vec::new();
                }
                let Some(at) = items.iter().position(|m| &m.id == id) else { return Vec::new() };
                [Some(at + 1), at.checked_sub(1)].into_iter().flatten().collect()
            }
        };
        let loading = |id: &String| matches!(self.diffs.get(id), Some(Loadable::Loading)) || matches!(self.discussions.get(id), Some(Loadable::Loading));
        let room = AHEAD.saturating_sub(self.fetched_ahead.keys().filter(|id| loading(id)).count());
        near.into_iter()
            .filter_map(|i| items.get(i))
            .filter(|m| !self.diffs.contains_key(&m.id) && !self.discussions.contains_key(&m.id))
            .take(room)
            .cloned()
            .collect()
    }

    /// Starts loading `ahead`'s requests. Never offline.
    fn fetch_ahead(&mut self) {
        if self.offline {
            return;
        }
        for item in self.ahead() {
            self.load_diffs(&item);
            self.load_discussions(&item);
            self.fetched_ahead.insert(item.id.clone(), Utc::now());
        }
    }
}

// MARK: Context

/// The key of a file's text at a commit.
fn text_key(sha: &str, path: &str) -> String {
    format!("{sha}|{path}")
}

impl Store {
    /// Whether a file's diff can show the unchanged lines around its hunks: it has a new side
    /// with hunks, and the commit it was taken at is known.
    pub fn expandable(&self, item: &MergeRequest, file: &FileDiff) -> bool {
        !file.deleted_file && !file.new_file && !file.too_large && !file.lines.is_empty() && self.diff_refs.contains_key(&item.id)
    }

    /// The text of a file's new side at the head of its diff, as far as it has loaded.
    pub fn file_text(&self, item: &MergeRequest, file: &FileDiff) -> Option<&Loadable<Rc<Vec<String>>>> {
        let refs = self.diff_refs.get(&item.id)?;
        self.file_texts.get(&text_key(&refs.head_sha, &file.new_path))
    }

    /// The unchanged stretches around a file's hunks; the one below the last has a length once
    /// the file's text is in.
    pub fn gaps(&self, item: &MergeRequest, file: &FileDiff) -> Vec<Gap> {
        if !self.expandable(item, file) {
            return Vec::new();
        }
        let len = self.file_text(item, file).and_then(Loadable::value).map(|t| t.len());
        diff::gaps(&file.lines, len)
    }

    fn shown_key(item: &MergeRequest, file: &FileDiff) -> String {
        format!("{}|{}|{}", item.id, file.id(), file.digest)
    }

    /// How much of `gap` shows.
    pub fn shown(&self, item: &MergeRequest, file: &FileDiff, gap: &Gap) -> Shown {
        self.shown.get(&Self::shown_key(item, file)).and_then(|g| g.get(&gap.new_start)).copied().unwrap_or_default()
    }

    /// Shows more of a gap, loading the file's text the first time.
    pub fn reveal(&mut self, item: &MergeRequest, file: &FileDiff, gap: &Gap, how: Reveal) {
        let now = self.shown(item, file, gap).reveal(how);
        self.shown.entry(Self::shown_key(item, file)).or_default().insert(gap.new_start, now);
        self.context_version += 1;
        self.load_file_text(item, file);
    }

    /// Asks for a file's text at its diff's head, unless it's in or on its way. A failed load
    /// is asked again.
    pub fn load_file_text(&mut self, item: &MergeRequest, file: &FileDiff) {
        let Some(sha) = self.diff_refs.get(&item.id).map(|r| r.head_sha.clone()) else { return };
        let key = text_key(&sha, &file.new_path);
        if self.offline || matches!(self.file_texts.get(&key), Some(Loadable::Loading | Loadable::Loaded(_))) {
            return;
        }
        self.file_texts.insert(key.clone(), Loadable::Loading);
        self.context_version += 1;
        let request = self.start_request(format!("file {key}"));
        let (item, path) = (item.clone(), file.new_path.clone());
        self.bg.spawn(move || Msg::FileText { key, request, result: item.kind().file_at(&item, &path, &sha) });
    }

    fn landed_file_text(&mut self, key: String, request: u64, result: Result<String, String>) {
        if self.requests.get(&format!("file {key}")) != Some(&request) {
            return;
        }
        let value = match result {
            Ok(text) => Loadable::Loaded(Rc::new(diff::text_lines(&text))),
            Err(e) => Loadable::Failed(e),
        };
        self.file_texts.insert(key, value);
        self.context_version += 1;
    }

    /// Line `new` of a file's new side where its diff leaves it out, as a context line with
    /// both its numbers: what a comment on it is placed by.
    pub fn context_line(&self, item: &MergeRequest, file: &FileDiff, new: i64) -> Option<DiffLine> {
        let text = self.file_text(item, file)?.value()?;
        let gap = self.gaps(item, file).into_iter().find(|g| g.len.is_some_and(|n| (g.new_start..g.new_start + n).contains(&new)))?;
        let line = text.get(usize::try_from(new - 1).ok()?)?;
        Some(DiffLine { kind: diff::LineKind::Context, text: line.clone(), old_number: Some(gap.old_number(new)), new_number: Some(new) })
    }

    /// Puts a file's text in place, for scenes and tests, which never ask a forge.
    pub fn insert_file_text(&mut self, sha: &str, path: &str, text: &str) {
        self.file_texts.insert(text_key(sha, path), Loadable::Loaded(Rc::new(diff::text_lines(text))));
        self.context_version += 1;
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
