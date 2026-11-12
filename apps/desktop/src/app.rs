//! The window's state: the review store plus everything the interface keeps for itself (which
//! panels are open, drafts, collapsed files).

use std::collections::{HashMap, HashSet};

use rune::TextEdit;

use crate::data::ask::proposals::Proposal;
use crate::data::diff::DiffLine;
use crate::data::models::{Discussion, FileDiff, MergeRequest};
use crate::data::prefs;
use crate::data::store::{Loadable, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum DetailTab {
    #[default]
    Overview,
    Changes,
    Discussion,
}

impl DetailTab {
    pub const ALL: [DetailTab; 3] = [DetailTab::Overview, DetailTab::Changes, DetailTab::Discussion];

    pub fn title(self) -> &'static str {
        match self {
            DetailTab::Overview => "Overview",
            DetailTab::Changes => "Changes",
            DetailTab::Discussion => "Discussion",
        }
    }
}

/// Where a comment being written goes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DraftKey {
    /// A new comment on the merge request.
    Comment(String),
    /// A reply in a thread: (merge request, thread).
    Reply(String, String),
    /// A comment on a diff line: (merge request, file id, line index).
    Line(String, String, usize),
}

pub struct App {
    pub store: Store,
    pub sidebar_visible: bool,
    pub settings_open: bool,
    pub filter: TextEdit,
    /// The tab each merge request was last left on.
    pub detail_tabs: HashMap<String, DetailTab>,
    pub sidebar_width: f32,
    /// The width when a resize drag started.
    pub drag_from: f32,
    /// Files folded away, as "merge request id|file id".
    pub collapsed: HashSet<String>,
    pub drafts: HashMap<DraftKey, TextEdit>,
    /// The diff line with its comment box open.
    pub line_comment: Option<DraftKey>,
    /// Threads with their reply box open: "merge request id|thread id".
    pub replying: HashSet<String>,
    /// What was posted and is waiting on its ticket, so the draft clears only when it lands.
    pub posting: HashMap<u64, DraftKey>,
    /// A copy button's "copied" check, and when it goes back.
    pub copied: Option<(String, f64)>,
    /// `--bench`'s driver, which changes something every frame.
    pub bench: Option<crate::bench::Driver>,
    /// When the queue next refreshes on its own, on the frame clock.
    pub next_refresh: f64,
    pub now: f64,
    /// The last badge put on the Dock icon.
    pub badge: Option<usize>,
    pub settings: SettingsDraft,
    /// Whether an account setting changed while the sheet was open.
    pub settings_dirty: bool,
    pub images: crate::ui::images::Images,
    /// `<details>` sections opened or closed, by document and position.
    pub details: HashMap<String, bool>,
    /// Long or generated files whose lines were asked for: "merge request id|file id".
    pub expanded_large: HashSet<String>,
    /// Merge requests whose viewed files were folded when their changes first arrived.
    pub folded_viewed: HashSet<String>,
    /// Whether the first poll ran (and started the first refresh).
    pub started: bool,
    /// "Ask": its chats, and the panel beside the card.
    pub ask: crate::data::ask::AskStore,
    pub ask_visible: bool,
    pub ask_width: f32,
    pub ask_drag_from: f32,
    /// Activity lines opened to their steps, and long thoughts shown whole: "chat|entry id".
    pub ask_open: HashSet<String>,
    /// Comments the agent drafted, by `Proposal::key`: the text as edited in the chat, whether
    /// its editor is open, and where its posting stands (posted ones are kept in settings).
    pub proposal_edits: HashMap<String, TextEdit>,
    pub proposal_editing: HashSet<String>,
    pub proposal_states: HashMap<String, ProposalState>,
    /// Drafted comments being posted, by the store's ticket.
    proposal_tickets: HashMap<u64, String>,
    pub updater: crate::data::update::Updater,
}

/// Where a comment the agent drafted stands. Nothing posts until the user clicks Post.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalState {
    Posting,
    Posted,
    Failed,
}

/// Where a drafted comment will go once it's approved.
pub enum ProposalTarget {
    Line(FileDiff, DiffLine),
    /// Its line, once the diff has loaded.
    Waiting,
    /// The merge request itself; `missed` is the file and line it named when the diff doesn't
    /// have them (or it named a file and no line).
    Whole {
        missed: Option<(String, Option<i64>)>,
    },
}

/// The settings sheet's text fields, applied as they change.
pub struct SettingsDraft {
    pub glab_path: TextEdit,
    pub hostname: TextEdit,
    pub gh_path: TextEdit,
    pub github_host: TextEdit,
}

impl SettingsDraft {
    pub fn load() -> SettingsDraft {
        let p = prefs::get();
        SettingsDraft {
            glab_path: TextEdit::new(p.glab_path),
            hostname: TextEdit::new(p.hostname),
            gh_path: TextEdit::new(p.gh_path),
            github_host: TextEdit::new(p.github_host),
        }
    }
}

impl Default for App {
    fn default() -> Self {
        let p = prefs::get();
        App {
            store: Store::default(),
            sidebar_visible: p.sidebar_visible,
            settings_open: false,
            filter: TextEdit::new(""),
            detail_tabs: HashMap::new(),
            sidebar_width: p.sidebar_width as f32,
            drag_from: 0.0,
            collapsed: HashSet::new(),
            drafts: HashMap::new(),
            line_comment: None,
            replying: HashSet::new(),
            posting: HashMap::new(),
            copied: None,
            bench: None,
            next_refresh: 0.0,
            now: 0.0,
            badge: None,
            settings: SettingsDraft::load(),
            settings_dirty: false,
            images: Default::default(),
            details: HashMap::new(),
            expanded_large: HashSet::new(),
            folded_viewed: HashSet::new(),
            started: false,
            ask: Default::default(),
            ask_visible: p.ask_visible,
            ask_width: p.ask_width as f32,
            ask_drag_from: 0.0,
            ask_open: HashSet::new(),
            proposal_edits: HashMap::new(),
            proposal_editing: HashSet::new(),
            proposal_states: p.ask_posted.iter().map(|k| (k.clone(), ProposalState::Posted)).collect(),
            proposal_tickets: HashMap::new(),
            updater: Default::default(),
        }
    }
}

impl App {
    pub fn tab(&self, item: &MergeRequest) -> DetailTab {
        self.detail_tabs.get(&item.id).copied().unwrap_or_default()
    }

    pub fn set_tab(&mut self, tab: DetailTab) {
        if let Some(id) = self.store.selected_id.clone() {
            self.detail_tabs.insert(id, tab);
        }
    }

    pub fn draft(&self, key: &DraftKey) -> Option<&TextEdit> {
        self.drafts.get(key)
    }

    pub fn draft_mut(&mut self, key: &DraftKey) -> &mut TextEdit {
        self.drafts.entry(key.clone()).or_insert_with(|| TextEdit::new(""))
    }

    pub fn draft_text(&self, key: &DraftKey) -> String {
        self.drafts.get(key).map(|e| e.text.trim().to_string()).unwrap_or_default()
    }

    pub fn is_collapsed(&self, item: &MergeRequest, file: &str) -> bool {
        self.collapsed.contains(&format!("{}|{file}", item.id))
    }

    pub fn toggle_collapsed(&mut self, item_id: &str, file: &str) {
        let key = format!("{item_id}|{file}");
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
    }

    pub fn is_replying(&self, item: &MergeRequest, thread: &Discussion) -> bool {
        self.replying.contains(&format!("{}|{}", item.id, thread.id))
    }

    pub fn copied(&self, what: &str) -> bool {
        matches!(&self.copied, Some((w, until)) if w == what && *until > self.now)
    }

    pub fn set_ask_visible(&mut self, visible: bool) {
        self.ask_visible = visible;
        prefs::update(|p| p.ask_visible = visible);
    }

    pub fn set_sidebar_visible(&mut self, visible: bool) {
        self.sidebar_visible = visible;
        prefs::update(|p| p.sidebar_visible = visible);
    }

    /// Posts the draft under `key`. The box keeps its text until the forge takes it.
    pub fn post(&mut self, key: DraftKey) {
        let body = self.draft_text(&key);
        if body.is_empty() || self.store.busy.is_some() {
            return;
        }
        let ticket = match &key {
            DraftKey::Comment(id) => {
                let Some(item) = self.store.item(id).cloned() else { return };
                self.store.comment(&item, body)
            }
            DraftKey::Reply(id, thread) => {
                let Some(item) = self.store.item(id).cloned() else { return };
                let threads = self.store.discussions.get(id).and_then(|l| l.value());
                let Some(thread) = threads.and_then(|t| t.iter().find(|d| &d.id == thread)).cloned() else { return };
                self.store.reply(&item, &thread, body)
            }
            DraftKey::Line(id, file, line) => {
                let Some(item) = self.store.item(id).cloned() else { return };
                let files = self.store.diffs.get(id).and_then(|l| l.value());
                let Some(file) = files.and_then(|f| f.iter().find(|d| &d.id() == file)).cloned() else { return };
                let Some(line) = file.lines.get(*line).cloned() else { return };
                let refs = self.store.diff_refs.get(id).cloned();
                self.store.comment_on_line(&item, &file, &line, refs, body)
            }
        };
        self.posting.insert(ticket, key);
    }

    /// Where a drafted comment goes: its line when the diff has it, else the merge request.
    pub fn proposal_target(&self, id: &str, proposal: &Proposal) -> ProposalTarget {
        let Some(path) = &proposal.path else { return ProposalTarget::Whole { missed: None } };
        let missed = ProposalTarget::Whole { missed: Some((path.clone(), proposal.line)) };
        let Some(number) = proposal.line else { return missed };
        // A diff that hasn't loaded yet isn't one without the line: wait rather than post wide.
        let files = match self.store.diffs.get(id) {
            Some(Loadable::Failed(_)) => return missed,
            Some(l) => match l.value() {
                Some(files) => files,
                None => return ProposalTarget::Waiting,
            },
            None => return ProposalTarget::Waiting,
        };
        let same = |p: &str| p == path || p.ends_with(&format!("/{path}")) || path.ends_with(&format!("/{p}"));
        let Some(file) = files.iter().find(|f| same(&f.new_path)).or_else(|| files.iter().find(|f| same(&f.old_path))) else { return missed };
        let found = file
            .lines
            .iter()
            .position(|l| l.new_number == Some(number))
            .or_else(|| file.lines.iter().position(|l| l.old_number == Some(number) && l.new_number.is_none()));
        match found {
            Some(index) => ProposalTarget::Line(file.clone(), file.lines[index].clone()),
            None => missed,
        }
    }

    /// The text a drafted comment posts: as edited, else as drafted, with the place it was
    /// meant for when that line isn't in the diff.
    pub fn proposal_text(&self, key: &str, id: &str, proposal: &Proposal) -> String {
        if let Some(edit) = self.proposal_edits.get(key) {
            return edit.text.clone();
        }
        match self.proposal_target(id, proposal) {
            ProposalTarget::Whole { missed: Some((path, Some(line))) } => format!("`{path}` line {line}:\n\n{}", proposal.body),
            ProposalTarget::Whole { missed: Some((path, None)) } => format!("`{path}`:\n\n{}", proposal.body),
            _ => proposal.body.clone(),
        }
    }

    /// Posts a drafted comment, on the user's click: the one way an agent's comment reaches the forge.
    pub fn post_proposal(&mut self, key: &str, id: &str, proposal: &Proposal) {
        let body = self.proposal_text(key, id, proposal).trim().to_string();
        let busy = matches!(self.proposal_states.get(key), Some(ProposalState::Posting | ProposalState::Posted));
        if body.is_empty() || busy || proposal.open || self.store.busy.is_some() {
            return;
        }
        let Some(item) = self.store.item(id).cloned() else { return };
        let ticket = match self.proposal_target(id, proposal) {
            ProposalTarget::Line(file, line) => {
                let refs = self.store.diff_refs.get(id).cloned();
                self.store.comment_on_line(&item, &file, &line, refs, body)
            }
            ProposalTarget::Whole { .. } => self.store.comment(&item, body),
            ProposalTarget::Waiting => return,
        };
        self.proposal_editing.remove(key);
        self.proposal_tickets.insert(ticket, key.to_string());
        self.proposal_states.insert(key.to_string(), ProposalState::Posting);
    }

    /// Clears the drafts whose comments landed, and closes their boxes.
    pub fn settle_posts(&mut self) -> bool {
        let landed: Vec<u64> = self.proposal_tickets.keys().copied().filter(|t| self.store.finished.contains_key(t)).collect();
        for ticket in &landed {
            let ok = self.store.finished.remove(ticket).unwrap_or(false);
            if let Some(key) = self.proposal_tickets.remove(ticket) {
                if ok {
                    let saved = key.clone();
                    prefs::update(|p| {
                        p.ask_posted.insert(saved);
                    });
                }
                self.proposal_states.insert(key, if ok { ProposalState::Posted } else { ProposalState::Failed });
            }
        }
        let done: Vec<u64> = self.posting.keys().copied().filter(|t| self.store.finished.contains_key(t)).collect();
        for ticket in &done {
            let ok = self.store.finished.remove(ticket).unwrap_or(false);
            let Some(key) = self.posting.remove(ticket) else { continue };
            if !ok {
                continue;
            }
            self.drafts.remove(&key);
            match &key {
                DraftKey::Line(..) => {
                    if self.line_comment.as_ref() == Some(&key) {
                        self.line_comment = None;
                    }
                }
                DraftKey::Reply(id, thread) => {
                    self.replying.remove(&format!("{id}|{thread}"));
                }
                DraftKey::Comment(_) => {}
            }
        }
        !done.is_empty() || !landed.is_empty()
    }
}
