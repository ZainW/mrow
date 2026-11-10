//! "Ask Cursor": chats with Cursor's agent about a merge request, in its read-only ask mode,
//! reading the user's own clone. One chat per merge request, kept while switching between them.
//!
//! The chats live on the UI thread. Each request to the agent runs on a worker thread that
//! blocks on the reply and sends back a `Msg`; streamed text arrives as `acp::Update`s. Both
//! land in `poll`, so a frame's worth of tokens is shown at once.

pub mod acp;
pub mod checkouts;
pub mod citations;
pub mod context;
pub mod model;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use rune::{TextEdit, Waker};
use serde_json::{Value, json};

use crate::data::models::MergeRequest;
use crate::data::prefs;
use crate::data::store::{Loadable, Store as ReviewStore};
use model::{AgentModel, EffortOption, EffortOptions};

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";

#[derive(Clone, Debug, PartialEq)]
pub enum EntryKind {
    Question,
    Answer,
    Thought,
    /// A tool the agent ran, by ACP's kind ("read", "search"…). `done` is None while it runs.
    Tool {
        kind: String,
        done: Option<bool>,
    },
    /// What the first question carried: "Description, 41 changed files…".
    Context,
    /// A request of the agent's that Mrow turned down.
    Declined,
    /// "Stopped", "cut off at the model's limit": a quiet line after an answer.
    Note,
    Failure,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u64,
    pub kind: EntryKind,
    pub text: String,
    /// For a tool call, the agent's id for it, so later updates find it.
    pub tool_id: Option<String>,
    pub started: f64,
    /// When it last grew or finished, for "Thought for 12s".
    pub finished: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    Working,
}

/// A question waiting for the merge request's details before it goes to the agent.
struct Pending {
    question: String,
    deadline: f64,
    asked_details: bool,
}

pub struct Chat {
    pub id: String,
    pub entries: Vec<Entry>,
    pub phase: Phase,
    pub draft: TextEdit,
    /// The clone the agent reads, or None when there isn't one.
    pub checkout: Option<PathBuf>,
    pub checkout_looked: bool,
    looking: bool,
    /// When the question being answered was asked, for the working line's timer.
    pub turn_started: Option<f64>,
    /// Reopening a chat from an earlier launch, while the agent replays it.
    pub reopening: bool,
    session: Option<String>,
    shared_context: bool,
    /// The context line of the question being asked, removed if nothing was shared.
    turn_context: Option<u64>,
    pending: Option<Pending>,
    /// Bumped by a new chat, so a worker's late reply to the old one is dropped.
    generation: u64,
    next_id: u64,
}

impl Chat {
    fn new(id: &str) -> Self {
        Chat {
            id: id.to_string(),
            entries: Vec::new(),
            phase: Phase::Idle,
            draft: TextEdit::default(),
            checkout: None,
            checkout_looked: false,
            looking: false,
            turn_started: None,
            reopening: false,
            session: None,
            shared_context: false,
            turn_context: None,
            pending: None,
            generation: 0,
            next_id: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.phase == Phase::Idle
    }

    pub fn has_question(&self) -> bool {
        self.entries.iter().any(|e| e.kind == EntryKind::Question)
    }

    fn append(&mut self, kind: EntryKind, text: impl Into<String>, tool_id: Option<String>) -> u64 {
        self.next_id += 1;
        let now = wall();
        self.entries.push(Entry { id: self.next_id, kind, text: text.into(), tool_id, started: now, finished: now });
        self.next_id
    }

    fn set_text(&mut self, id: u64, text: String) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.text = text;
        }
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
    }

    /// Streamed text joins the entry before it when that's the same kind.
    fn stream(&mut self, kind: EntryKind, text: &str) {
        let joins = matches!(kind, EntryKind::Answer | EntryKind::Thought | EntryKind::Question);
        match self.entries.last_mut() {
            Some(last) if joins && last.kind == kind => {
                last.text.push_str(text);
                last.finished = wall();
            }
            _ => {
                self.append(kind, text, None);
            }
        }
    }

    fn update_tool(&mut self, tool_id: &str, title: Option<String>, done: Option<bool>) {
        let Some(entry) = self.entries.iter_mut().rev().find(|e| e.tool_id.as_deref() == Some(tool_id)) else { return };
        let EntryKind::Tool { kind, done: was } = entry.kind.clone() else { return };
        if let Some(title) = title.filter(|t| !t.is_empty()) {
            entry.text = title;
        }
        if done.is_some() {
            entry.finished = wall();
        }
        // A declined call stays marked declined, though the agent reports it finished.
        entry.kind = EntryKind::Tool { kind, done: if was == Some(false) { Some(false) } else { done.or(was) } };
    }

    /// The first question of a saved chat carried the merge request; show only the question.
    fn trim_context(&mut self) {
        let Some(index) = self.entries.iter().position(|e| e.kind == EntryKind::Question && e.text.contains(context::END)) else {
            return;
        };
        let text = &self.entries[index].text;
        let after = &text[text.find(context::END).unwrap() + context::END.len()..];
        self.entries[index].text = after.trim().to_string();
        self.next_id += 1;
        let at = self.entries[index].started;
        let context = Entry { id: self.next_id, kind: EntryKind::Context, text: "This merge request".into(), tool_id: None, started: at, finished: at };
        self.entries.insert(index, context);
    }

    fn reset(&mut self) {
        self.entries.clear();
        self.session = None;
        self.shared_context = false;
        self.turn_context = None;
        self.pending = None;
        self.phase = Phase::Idle;
        self.turn_started = None;
        self.reopening = false;
        self.generation += 1;
    }
}

/// What a worker reports.
enum Msg {
    Checkout { chat: String, path: Option<PathBuf> },
    Session { chat: String, generation: u64, session: String },
    Working { chat: String, generation: u64 },
    Answered { chat: String, generation: u64, stop: Option<String> },
    Failed { chat: String, generation: u64, error: String, starting: bool },
    Resumed { chat: String, generation: u64, ok: bool },
    Models(Vec<AgentModel>),
    ModelsLoaded(Result<(), String>),
    Absorb(Vec<Value>),
    Catalog(Option<String>),
}

/// The model, its effort and what the worker needs to put a session on them.
#[derive(Clone)]
struct ModelChoice {
    model: String,
    known: bool,
    effort: Option<EffortOptions>,
    chosen_effort: Option<String>,
}

pub struct AskStore {
    chats: HashMap<String, Chat>,
    by_session: HashMap<String, String>,
    /// Cursor's models, kept between launches so the picker has them before any chat starts.
    pub models: Vec<AgentModel>,
    pub loading_models: bool,
    pub models_error: Option<String>,
    /// The chosen model by base id: the last one picked, else Sonnet 5.5.
    pub model: String,
    /// The last few picked, newest first.
    pub recent_models: Vec<String>,
    /// Each model's effort setting as Cursor offers it.
    pub efforts: HashMap<String, EffortOptions>,
    /// The effort picked for each model.
    pub chosen_efforts: HashMap<String, String>,
    models_fetched: f64,
    catalog: Option<String>,
    describing: bool,
    /// Off for headless runs, so a test chat isn't reopened in the app.
    pub remembers_sessions: bool,
    /// Headless: never start the agent.
    pub offline: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    updates_tx: Sender<acp::Update>,
    updates: Receiver<acp::Update>,
    waker: Option<Waker>,
}

impl Default for AskStore {
    fn default() -> Self {
        let p = prefs::get();
        let saved = AgentModel::base(&p.ask_model).to_string();
        let models: Vec<AgentModel> = p.ask_models.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect();
        let models: Vec<AgentModel> = if models.iter().any(|m| m.id.contains('[')) { Vec::new() } else { models };
        let efforts = p.ask_effort_options.iter().filter_map(|(k, v)| Some((k.clone(), serde_json::from_value(v.clone()).ok()?))).collect();
        let mut chosen_efforts: HashMap<String, String> = p.ask_efforts.clone().into_iter().collect();
        if chosen_efforts.is_empty() {
            chosen_efforts.insert(DEFAULT_MODEL.into(), "medium".into());
        }
        let (tx, rx) = channel();
        let (updates_tx, updates) = channel();
        AskStore {
            chats: HashMap::new(),
            by_session: HashMap::new(),
            models_fetched: if models.is_empty() { 0.0 } else { p.ask_models_fetched },
            models,
            loading_models: false,
            models_error: None,
            model: if saved.is_empty() { DEFAULT_MODEL.into() } else { saved },
            recent_models: p.ask_recent_models.clone(),
            efforts,
            chosen_efforts,
            catalog: None,
            describing: false,
            remembers_sessions: true,
            offline: false,
            tx,
            rx,
            updates_tx,
            updates,
            waker: None,
        }
    }
}

pub fn wall() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl AskStore {
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker.clone());
        acp::CLIENT.set_sink(self.updates_tx.clone(), Some(waker));
    }

    fn spawn(&self, job: impl FnOnce(&Sender<Msg>) + Send + 'static) {
        let tx = self.tx.clone();
        let waker = self.waker.clone();
        std::thread::spawn(move || {
            job(&tx);
            if let Some(w) = waker {
                w();
            }
        });
    }

    pub fn chat(&self, id: &str) -> Option<&Chat> {
        self.chats.get(id)
    }

    pub fn chat_mut(&mut self, id: &str) -> &mut Chat {
        self.chats.entry(id.to_string()).or_insert_with(|| Chat::new(id))
    }

    /// Test and snapshot fixtures: a chat with these entries.
    pub fn insert_fixture(&mut self, id: &str, checkout: Option<PathBuf>, entries: Vec<(EntryKind, &str, f64)>, phase: Phase) {
        let chat = self.chat_mut(id);
        chat.checkout = checkout;
        chat.checkout_looked = true;
        for (kind, text, seconds) in entries {
            let id = chat.append(kind, text, None);
            if let Some(e) = chat.entries.iter_mut().find(|e| e.id == id) {
                e.started -= seconds;
            }
        }
        chat.phase = phase;
        if phase != Phase::Idle {
            chat.turn_started = Some(wall() - 7.0);
        }
    }

    fn saved_session(&self, id: &str) -> Option<String> {
        if !self.remembers_sessions {
            return None;
        }
        prefs::read(|p| p.ask_sessions.get(id).cloned())
    }

    fn save_session(&self, id: &str, session: Option<&str>) {
        if !self.remembers_sessions {
            return;
        }
        prefs::update(|p| match session {
            Some(s) => {
                p.ask_sessions.insert(id.to_string(), s.to_string());
            }
            None => {
                p.ask_sessions.remove(id);
            }
        });
    }

    /// Finds the clone (off the UI thread: it reads a few folders), then reopens a saved chat.
    pub fn prepare(&mut self, item: &MergeRequest) {
        let offline = self.offline;
        let chat = self.chat_mut(&item.id);
        if chat.checkout_looked || chat.looking {
            return;
        }
        if offline {
            chat.checkout_looked = true;
            return;
        }
        chat.looking = true;
        let (id, project) = (item.id.clone(), item.project_path.clone());
        self.spawn(move |tx| {
            let _ = tx.send(Msg::Checkout { chat: id, path: checkouts::find(&project) });
        });
    }

    pub fn choose_folder(&mut self, folder: PathBuf, item: &MergeRequest) {
        checkouts::remember(&folder, &item.project_path);
        self.chat_mut(&item.id).checkout = Some(folder);
        self.new_chat(&item.id);
    }

    pub fn new_chat(&mut self, id: &str) {
        let chat = self.chat_mut(id);
        let session = chat.session.take();
        chat.reset();
        if let Some(session) = session {
            acp::CLIENT.notify("session/cancel", json!({"sessionId": session}));
            self.by_session.remove(&session);
        }
        self.save_session(id, None);
    }

    pub fn stop(&mut self, id: &str) {
        let Some(chat) = self.chats.get(id) else { return };
        if chat.phase == Phase::Working {
            if let Some(session) = &chat.session {
                acp::CLIENT.notify("session/cancel", json!({"sessionId": session}));
            }
        }
    }

    /// Asks a question. The first one in a chat carries the merge request with it, so it waits
    /// (in `poll`) for the diff and threads.
    pub fn ask(&mut self, id: &str, question: &str) {
        let question = question.trim().to_string();
        let chat = self.chat_mut(id);
        if question.is_empty() || chat.phase != Phase::Idle {
            return;
        }
        chat.draft = TextEdit::default();
        chat.append(EntryKind::Question, question.clone(), None);
        // What the first question carries shows with it at once, its counts filled in once known.
        chat.turn_context = if chat.shared_context { None } else { Some(chat.append(EntryKind::Context, "Description, diff and threads", None)) };
        let now = wall();
        chat.turn_started = Some(now);
        chat.phase = Phase::Starting;
        chat.pending = Some(Pending { question, deadline: now + 20.0, asked_details: false });
    }

    /// Asks the last question again, after a failure.
    pub fn retry(&mut self, id: &str) {
        let last = self.chats.get(id).and_then(|c| c.entries.iter().rev().find(|e| e.kind == EntryKind::Question)).map(|e| e.text.clone());
        if let Some(q) = last {
            self.ask(id, &q);
        }
    }

    fn choice(&self) -> ModelChoice {
        ModelChoice {
            model: self.model.clone(),
            known: self.models.iter().any(|m| m.id == self.model),
            effort: self.efforts.get(&self.model).cloned(),
            chosen_effort: self.chosen_efforts.get(&self.model).cloned(),
        }
    }

    /// Sends questions whose details have arrived. Returns the next time it needs a look.
    fn send_pending(&mut self, store: &mut ReviewStore) -> Option<f64> {
        let now = wall();
        let mut wake: Option<f64> = None;
        let waiting: Vec<String> = self.chats.iter().filter(|(_, c)| c.pending.is_some()).map(|(k, _)| k.clone()).collect();
        for id in waiting {
            let item = store.item(&id).cloned();
            let chat = self.chats.get_mut(&id).unwrap();
            let needs_details = !chat.shared_context;
            let ready = match &item {
                Some(item) if needs_details => {
                    let pending = chat.pending.as_mut().unwrap();
                    if !pending.asked_details {
                        pending.asked_details = true;
                        store.load_details_if_needed(item);
                    }
                    let settled = |l: Option<bool>| l.unwrap_or(false);
                    let diffs = settled(store.diffs.get(&id).map(|l| !matches!(l, Loadable::Loading | Loadable::Idle)));
                    let threads = settled(store.discussions.get(&id).map(|l| !matches!(l, Loadable::Loading | Loadable::Idle)));
                    (diffs && threads) || now >= pending.deadline
                }
                _ => true,
            };
            if !ready {
                let deadline = chat.pending.as_ref().unwrap().deadline;
                wake = Some(wake.map_or(deadline, |w: f64| w.min(deadline)));
                continue;
            }
            let pending = chat.pending.take().unwrap();
            let mut prompt: Vec<Value> = Vec::new();
            if needs_details {
                if let Some(item) = &item {
                    let files = store.diffs.get(&id).and_then(|l| l.value()).cloned().unwrap_or_default();
                    let threads = store.discussions.get(&id).and_then(|l| l.value()).cloned().unwrap_or_default();
                    let summary = context::build(item, &files, &threads, chat.checkout.as_deref());
                    prompt.push(json!({"type": "text", "text": summary.text}));
                    let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
                    let mut shared = format!("Description, {}", plural(summary.files, "changed file"));
                    if summary.files_left_out > 0 {
                        shared += &format!(" ({} left out for size)", summary.files_left_out);
                    }
                    shared += &format!(", {}", plural(summary.threads, "thread"));
                    if let Some(c) = chat.turn_context {
                        chat.set_text(c, shared);
                    }
                }
                if item.is_none() {
                    // Nothing to share: the merge request is gone from the queue.
                    if let Some(c) = chat.turn_context.take() {
                        chat.remove(c);
                    }
                }
                chat.shared_context = true;
            }
            prompt.push(json!({"type": "text", "text": pending.question}));
            let session = chat.session.clone();
            let generation = chat.generation;
            let cwd = chat.checkout.clone().unwrap_or_else(empty_folder);
            let choice = self.choice();
            let id = id.clone();
            if self.offline {
                // Headless runs never start the agent.
                let error = "Cursor's agent doesn't run in headless runs.".to_string();
                let _ = self.tx.send(Msg::Failed { chat: id, generation, error, starting: true });
                continue;
            }
            self.spawn(move |tx| ask_worker(tx, id, generation, session, cwd, prompt, choice));
        }
        wake
    }

    /// Lands workers' results and streamed updates. Returns whether anything changed and when it
    /// next needs a look (a question waiting for details).
    pub fn poll(&mut self, store: &mut ReviewStore) -> (bool, Option<f64>) {
        let mut changed = false;
        for msg in self.rx.try_iter().collect::<Vec<_>>() {
            changed = true;
            self.land(msg);
        }
        for update in self.updates.try_iter().collect::<Vec<_>>() {
            changed = true;
            self.received(&update.session, &update.update);
        }
        let wake = self.send_pending(store);
        (changed, wake)
    }

    fn land(&mut self, msg: Msg) {
        match msg {
            Msg::Checkout { chat: id, path } => {
                let chat = self.chat_mut(&id);
                chat.looking = false;
                chat.checkout_looked = true;
                if chat.checkout.is_none() {
                    chat.checkout = path;
                }
                let fresh = chat.session.is_none() && chat.entries.is_empty() && chat.phase == Phase::Idle;
                if let (true, Some(saved)) = (fresh, self.saved_session(&id)) {
                    self.resume(&id, saved);
                }
            }
            Msg::Session { chat: id, generation, session } => {
                let chat = self.chat_mut(&id);
                if chat.generation != generation {
                    acp::CLIENT.notify("session/cancel", json!({"sessionId": session}));
                    return;
                }
                chat.session = Some(session.clone());
                self.by_session.insert(session.clone(), id.clone());
                self.save_session(&id, Some(&session));
            }
            Msg::Working { chat: id, generation } => {
                let chat = self.chat_mut(&id);
                if chat.generation == generation {
                    chat.phase = Phase::Working;
                }
            }
            Msg::Answered { chat: id, generation, stop } => {
                let chat = self.chat_mut(&id);
                if chat.generation != generation {
                    return;
                }
                match stop.as_deref() {
                    Some("cancelled") => {
                        chat.append(EntryKind::Note, "Stopped", None);
                    }
                    Some("refusal") => {
                        chat.append(EntryKind::Failure, "The model declined to answer.", None);
                    }
                    Some("max_tokens" | "max_turn_requests") => {
                        chat.append(EntryKind::Note, "The answer was cut off at the model's limit.", None);
                    }
                    _ => {}
                }
                chat.phase = Phase::Idle;
                chat.turn_started = None;
            }
            Msg::Failed { chat: id, generation, error, starting } => {
                let chat = self.chat_mut(&id);
                if chat.generation != generation {
                    return;
                }
                // Nothing was shared if it failed before the prompt went.
                if starting {
                    if let Some(c) = chat.turn_context.take() {
                        chat.remove(c);
                    }
                }
                chat.append(EntryKind::Failure, error.clone(), None);
                // A chat that never started, or whose agent stopped, starts afresh next time.
                if starting || error.starts_with("Cursor's agent stopped") {
                    if let Some(s) = chat.session.take() {
                        self.by_session.remove(&s);
                    }
                    let chat = self.chat_mut(&id);
                    chat.shared_context = false;
                }
                let chat = self.chat_mut(&id);
                chat.phase = Phase::Idle;
                chat.turn_started = None;
            }
            Msg::Resumed { chat: id, generation, ok } => {
                let chat = self.chat_mut(&id);
                if chat.generation != generation {
                    return;
                }
                chat.reopening = false;
                chat.phase = Phase::Idle;
                if ok {
                    chat.shared_context = true;
                    chat.trim_context();
                } else {
                    chat.entries.clear();
                    if let Some(s) = chat.session.take() {
                        self.by_session.remove(&s);
                    }
                }
            }
            Msg::Models(fresh) => self.keep_models(fresh),
            Msg::ModelsLoaded(result) => {
                self.loading_models = false;
                self.models_error = result.err();
            }
            Msg::Absorb(options) => self.absorb(&options),
            Msg::Catalog(session) => {
                self.catalog = session;
                self.describing = false;
            }
        }
    }

    fn resume(&mut self, id: &str, session: String) {
        let chat = self.chat_mut(id);
        chat.phase = Phase::Starting;
        chat.reopening = true;
        chat.session = Some(session.clone());
        let generation = chat.generation;
        let cwd = chat.checkout.clone().unwrap_or_else(empty_folder);
        self.by_session.insert(session.clone(), id.to_string());
        let choice = self.choice();
        let id = id.to_string();
        self.spawn(move |tx| {
            let client = acp::CLIENT.clone();
            let ok = client
                .request("session/load", json!({"sessionId": session, "cwd": cwd.to_string_lossy(), "mcpServers": []}))
                .and_then(|reply| configure(tx, &session, options(&reply), &choice))
                .is_ok();
            let _ = tx.send(Msg::Resumed { chat: id, generation, ok });
        });
    }

    // MARK: Updates

    fn received(&mut self, session: &str, update: &Value) {
        let Some(id) = self.by_session.get(session).cloned() else { return };
        let s = |k: &str| update.get(k).and_then(Value::as_str).map(str::to_string);
        let text = update.get("content").and_then(|c| c.get("text")).and_then(Value::as_str).unwrap_or("").to_string();
        let kind = s("sessionUpdate").unwrap_or_default();
        if kind == "config_option_update" {
            self.absorb(&options(update));
            return;
        }
        let chat = self.chat_mut(&id);
        match kind.as_str() {
            "agent_message_chunk" => chat.stream(EntryKind::Answer, &text),
            "agent_thought_chunk" => chat.stream(EntryKind::Thought, &text),
            "user_message_chunk" => chat.stream(EntryKind::Question, &text),
            "tool_call" => {
                let title = tool_title(update, chat.checkout.as_deref());
                let kind = EntryKind::Tool { kind: s("kind").unwrap_or_else(|| "other".into()), done: done(s("status").as_deref()) };
                chat.append(kind, title, s("toolCallId"));
            }
            "tool_call_update" => {
                let title = s("title").map(|_| tool_title(update, chat.checkout.as_deref()));
                chat.update_tool(&s("toolCallId").unwrap_or_default(), title, done(s("status").as_deref()));
            }
            "mrow_declined" => {
                let call = update.get("toolCall").cloned().unwrap_or(json!({}));
                let call_id = call.get("toolCallId").and_then(Value::as_str).unwrap_or("");
                chat.update_tool(call_id, None, Some(false));
                let what = if call.get("kind").and_then(Value::as_str) == Some("execute") { "Declined running a command" } else { "Declined a change" };
                chat.append(EntryKind::Declined, what, None);
            }
            _ => {}
        }
    }

    // MARK: Models

    /// The chosen model as Cursor lists it now, or None before the list has loaded.
    pub fn chosen_model(&self) -> Option<&AgentModel> {
        self.models.iter().find(|m| m.id == self.model)
    }

    /// The effort a model will run at: the one picked for it, else the one Cursor last used.
    pub fn effort(&self, model: &str) -> Option<&EffortOption> {
        let options = self.efforts.get(model)?;
        options.option(self.chosen_efforts.get(model).map(String::as_str)).or_else(|| options.option(Some(&options.current)))
    }

    /// Picks the model for this chat (from its next answer) and for new ones.
    pub fn choose_model(&mut self, model: &str, chat: Option<&str>) {
        self.model = model.to_string();
        self.recent_models.retain(|m| m != model);
        self.recent_models.insert(0, model.to_string());
        self.recent_models.truncate(4);
        let (m, recent) = (self.model.clone(), self.recent_models.clone());
        prefs::update(|p| {
            p.ask_model = m;
            p.ask_recent_models = recent;
        });
        let session = chat.and_then(|c| self.chats.get(c)).and_then(|c| c.session.clone());
        match session {
            Some(session) => {
                let choice = self.choice();
                self.spawn(move |tx| apply_model(tx, &session, Vec::new(), &choice));
            }
            None => self.describe(),
        }
    }

    /// Picks the chosen model's effort, for this chat and the model's next ones.
    pub fn choose_effort(&mut self, value: &str, chat: Option<&str>) {
        self.chosen_efforts.insert(self.model.clone(), value.to_string());
        let chosen: std::collections::BTreeMap<String, String> = self.chosen_efforts.clone().into_iter().collect();
        prefs::update(|p| p.ask_efforts = chosen);
        let session = chat.and_then(|c| self.chats.get(c)).and_then(|c| c.session.clone());
        if let (Some(session), Some(effort)) = (session, self.efforts.get(&self.model).cloned()) {
            let value = value.to_string();
            self.spawn(move |tx| {
                if let Ok(reply) = set_option(&session, &effort.config_id, &value) {
                    let _ = tx.send(Msg::Absorb(reply));
                }
            });
        }
    }

    /// Learns the chosen model's effort options (Cursor only says for the model a session is on)
    /// by moving the catalog session to it.
    pub fn describe(&mut self) {
        let model = self.model.clone();
        if self.offline || self.describing || self.efforts.contains_key(&model) || !self.models.iter().any(|m| m.id == model) {
            return;
        }
        self.describing = true;
        let catalog = self.catalog.clone();
        self.spawn(move |tx| {
            let mut catalog = catalog;
            for _ in 0..2 {
                let attempt = (|| -> Result<(), String> {
                    let session = match &catalog {
                        Some(s) => s.clone(),
                        None => {
                            let reply = new_session(&empty_folder())?;
                            let _ = tx.send(Msg::Models(models_in(&reply)));
                            let _ = tx.send(Msg::Absorb(options(&reply)));
                            let s = reply.get("sessionId").and_then(Value::as_str).ok_or("no session")?.to_string();
                            catalog = Some(s.clone());
                            s
                        }
                    };
                    let _ = tx.send(Msg::Absorb(set_option(&session, "model", &model)?));
                    Ok(())
                })();
                match attempt {
                    Ok(()) => break,
                    // The agent may have restarted since; its sessions went with it.
                    Err(_) => catalog = None,
                }
            }
            let _ = tx.send(Msg::Catalog(catalog));
        });
    }

    /// Fetches the list when there's none or it's a day old. Cursor only lists models for a chat,
    /// so this opens one in the empty folder.
    pub fn load_models(&mut self, force: bool) {
        let stale = wall() - self.models_fetched > 86_400.0;
        if self.offline || self.loading_models || !(force || self.models.is_empty() || stale) {
            return;
        }
        self.loading_models = true;
        self.spawn(move |tx| {
            let result = new_session(&empty_folder()).map(|reply| {
                let _ = tx.send(Msg::Models(models_in(&reply)));
                let _ = tx.send(Msg::Absorb(options(&reply)));
                let _ = tx.send(Msg::Catalog(reply.get("sessionId").and_then(Value::as_str).map(str::to_string)));
            });
            let _ = tx.send(Msg::ModelsLoaded(result));
        });
    }

    fn keep_models(&mut self, fresh: Vec<AgentModel>) {
        if fresh.is_empty() {
            return;
        }
        if fresh != self.models {
            self.models = fresh;
        }
        self.models_fetched = wall();
        let (values, fetched): (Vec<Value>, f64) = (self.models.iter().filter_map(|m| serde_json::to_value(m).ok()).collect(), self.models_fetched);
        prefs::update(|p| {
            p.ask_models = values;
            p.ask_models_fetched = fetched;
        });
    }

    /// Notes a model's effort options from a session's settings.
    fn absorb(&mut self, options: &[Value]) {
        let Some((model, effort)) = EffortOptions::from(options) else { return };
        if self.efforts.get(&model) == Some(&effort) {
            return;
        }
        if let Ok(v) = serde_json::to_value(&effort) {
            prefs::update(|p| {
                p.ask_effort_options.insert(model.clone(), v);
            });
        }
        self.efforts.insert(model, effort);
    }

    /// Stops the agent when quitting.
    pub fn shutdown(&self) {
        acp::CLIENT.stop();
    }
}

// MARK: Workers

fn options(reply: &Value) -> Vec<Value> {
    reply.get("configOptions").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn models_in(reply: &Value) -> Vec<AgentModel> {
    reply
        .get("models")
        .and_then(|m| m.get("availableModels"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    let id = m.get("modelId")?.as_str()?;
                    Some(AgentModel::new(id, m.get("name").and_then(Value::as_str).unwrap_or(id)))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn new_session(cwd: &std::path::Path) -> Result<Value, String> {
    acp::CLIENT.clone().request("session/new", json!({"cwd": cwd.to_string_lossy(), "mcpServers": []}))
}

fn set_option(session: &str, id: &str, value: &str) -> Result<Vec<Value>, String> {
    acp::CLIENT.clone().request("session/set_config_option", json!({"sessionId": session, "configId": id, "value": value})).map(|r| options(&r))
}

/// Read-only "ask" mode, and the chosen model and effort.
fn configure(tx: &Sender<Msg>, session: &str, options: Vec<Value>, choice: &ModelChoice) -> Result<(), String> {
    acp::CLIENT.clone().request("session/set_mode", json!({"sessionId": session, "modeId": "ask"}))?;
    apply_model(tx, session, options, choice);
    Ok(())
}

/// Puts a session on the chosen model, then that model's chosen effort. A failure leaves the
/// session on Cursor's choice rather than failing the question.
fn apply_model(tx: &Sender<Msg>, session: &str, options: Vec<Value>, choice: &ModelChoice) {
    let mut options = options;
    let current = options
        .iter()
        .find(|o| o.get("id").and_then(Value::as_str) == Some("model"))
        .and_then(|o| o.get("currentValue"))
        .and_then(Value::as_str)
        .map(str::to_string);
    if current.as_deref() != Some(&choice.model) && choice.known {
        match set_option(session, "model", &choice.model) {
            Ok(reply) => options = reply,
            Err(_) => return,
        }
    }
    let learned = EffortOptions::from(&options).filter(|(m, _)| *m == choice.model).map(|(_, e)| e);
    let _ = tx.send(Msg::Absorb(options));
    let Some(effort) = learned.or_else(|| choice.effort.clone()) else { return };
    let Some(chosen) = &choice.chosen_effort else { return };
    if effort.current != *chosen && effort.option(Some(chosen)).is_some() {
        if let Ok(reply) = set_option(session, &effort.config_id, chosen) {
            let _ = tx.send(Msg::Absorb(reply));
        }
    }
}

fn ask_worker(tx: &Sender<Msg>, chat: String, generation: u64, session: Option<String>, cwd: PathBuf, prompt: Vec<Value>, choice: ModelChoice) {
    let fail = |error: String, starting: bool| {
        let _ = tx.send(Msg::Failed { chat: chat.clone(), generation, error, starting });
    };
    let session = match session {
        Some(s) => s,
        None => {
            let started = new_session(&cwd).and_then(|reply| {
                let session = reply.get("sessionId").and_then(Value::as_str).ok_or("The agent didn't start a chat.")?.to_string();
                let _ = tx.send(Msg::Session { chat: chat.clone(), generation, session: session.clone() });
                let _ = tx.send(Msg::Models(models_in(&reply)));
                configure(tx, &session, options(&reply), &choice)?;
                Ok(session)
            });
            match started {
                Ok(s) => s,
                Err(e) => return fail(e, true),
            }
        }
    };
    let _ = tx.send(Msg::Working { chat: chat.clone(), generation });
    match acp::CLIENT.clone().request("session/prompt", json!({"sessionId": session, "prompt": prompt})) {
        Ok(reply) => {
            let stop = reply.get("stopReason").and_then(Value::as_str).map(str::to_string);
            let _ = tx.send(Msg::Answered { chat, generation, stop });
        }
        Err(e) => fail(e, false),
    }
}

fn done(status: Option<&str>) -> Option<bool> {
    match status {
        Some("completed") => Some(true),
        Some("failed") => Some(false),
        _ => None,
    }
}

/// "Read src/x.ts" rather than the absolute path into the clone.
pub fn tool_title(update: &Value, checkout: Option<&std::path::Path>) -> String {
    let mut title = update.get("title").and_then(Value::as_str).unwrap_or("Working").to_string();
    let Some(checkout) = checkout.and_then(|c| c.to_str()) else { return title };
    // The agent names files absolutely or from the home folder.
    let home = std::env::var("HOME").unwrap_or_default();
    title = title.replace(&format!("{checkout}/"), "");
    if let Some(from_home) = checkout.strip_prefix(&format!("{home}/")).filter(|s| !s.is_empty()) {
        title = title.replace(&format!("{from_home}/"), "");
    }
    title
}

/// Where a chat runs when there's no clone: an empty folder, so the agent has nothing to read
/// but the merge request itself.
pub fn empty_folder() -> PathBuf {
    let folder = crate::data::paths::cache_dir().join("ask");
    let _ = std::fs::create_dir_all(&folder);
    folder
}
