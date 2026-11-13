//! The inbox: what changed on the requests in the queue, found by comparing each refresh with a
//! small snapshot of the one before. A review asked of me, and on my own requests an approval,
//! changes asked for, a failed pipeline, new threads, ready to merge, or leaving the list.
//!
//! Detection is a pure function of the snapshot and the queue, so it is tested without a store.
//! The first look at an account only takes the snapshot: what was already there is no news.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::Duration;
use serde::{Deserialize, Serialize};

use super::forge::Forge;
use super::mine::{self, Standing};
use super::models::{Date, MergeRequest, PipelineStatus, ReviewState};

/// The most events kept.
pub const CAP: usize = 100;
/// How long an event is kept.
pub const KEEP_DAYS: i64 = 14;
/// How long an event is kept once its request has left every list, unless the event is about
/// that leaving.
const GONE_DAYS: i64 = 2;
/// Banners one refresh may post, one per request.
const MAX_NOTICES: usize = 5;
/// A re-request within this many seconds of the last one is the same ask.
const SAME_ASK_SECONDS: i64 = 60;

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    ReviewAsked,
    AskedAgain,
    ChangesRequested,
    PipelineFailed,
    Ready,
    Approved,
    Threads,
    /// One of mine left the open list: merged or closed.
    Closed,
}

impl Kind {
    pub const ALL: [Kind; 8] =
        [Kind::ReviewAsked, Kind::AskedAgain, Kind::ChangesRequested, Kind::PipelineFailed, Kind::Ready, Kind::Approved, Kind::Threads, Kind::Closed];

    /// Its name in Settings.
    pub fn label(self) -> &'static str {
        match self {
            Kind::ReviewAsked => "Review asked",
            Kind::AskedAgain => "Asked again",
            Kind::ChangesRequested => "Changes asked",
            Kind::PipelineFailed => "Pipeline failed",
            Kind::Ready => "Ready to merge",
            Kind::Approved => "Approved",
            Kind::Threads => "New threads",
            Kind::Closed => "Merged or closed",
        }
    }

    /// The turn changing is worth a banner; the rest only goes in the inbox.
    pub fn default_level(self) -> Level {
        match self {
            Kind::ReviewAsked | Kind::AskedAgain | Kind::ChangesRequested | Kind::PipelineFailed | Kind::Ready => Level::Notify,
            Kind::Approved | Kind::Threads | Kind::Closed => Level::Inbox,
        }
    }

    /// Which of several on one request a banner leads with: lowest first.
    fn weight(self) -> u8 {
        match self {
            Kind::ChangesRequested => 0,
            Kind::PipelineFailed => 1,
            Kind::Ready => 2,
            Kind::ReviewAsked => 3,
            Kind::AskedAgain => 4,
            Kind::Approved => 5,
            Kind::Threads => 6,
            Kind::Closed => 7,
        }
    }

    /// About a request I opened, rather than one I was asked to review.
    pub fn on_mine(self) -> bool {
        !matches!(self, Kind::ReviewAsked | Kind::AskedAgain)
    }
}

/// How far an event reaches: a banner and the inbox, the inbox only, or nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Notify,
    Inbox,
    Off,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Notify, Level::Inbox, Level::Off];

    pub fn name(self) -> &'static str {
        match self {
            Level::Notify => "Notify",
            Level::Inbox => "Inbox",
            Level::Off => "Off",
        }
    }
}

/// The level set for `kind`, or its default.
pub fn level(levels: &BTreeMap<Kind, Level>, kind: Kind) -> Level {
    levels.get(&kind).copied().unwrap_or(kind.default_level())
}

/// One thing that changed on a merge request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// The request, the kind and the moment: the same change found twice is one event.
    pub id: String,
    /// The merge request's ID.
    pub request: String,
    pub forge: Forge,
    pub kind: Kind,
    /// "Maya Lindqvist approved", "Pipeline failed · test:integration".
    pub title: String,
    /// The request's title.
    pub subject: String,
    /// Its short reference, "ledger!231".
    pub reference: String,
    /// Where it is on the forge, for a request that has left every list.
    pub url: String,
    pub at: Date,
    /// Dealt with: opened, or marked done.
    pub done: bool,
}

impl Event {
    fn new(item: &Own, request: &str, kind: Kind, title: String, at: Date) -> Event {
        Event {
            id: format!("{request}|{kind:?}|{}", at.timestamp()),
            request: request.to_string(),
            forge: item.forge,
            kind,
            title,
            subject: item.title.clone(),
            reference: item.reference.clone(),
            url: item.url.clone(),
            at,
            done: false,
        }
    }
}

/// What the last refresh saw, kept between launches.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    /// The accounts whose reviews have been looked at once; a first look is no news.
    pub review_forges: BTreeSet<Forge>,
    /// The same for my own requests, which can fail to load on their own.
    pub mine_forges: BTreeSet<Forge>,
    /// Each review waiting on me, by ID.
    pub reviews: BTreeMap<String, Asked>,
    /// Each of my open requests, by ID.
    pub mine: BTreeMap<String, Own>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    pub forge: Forge,
    /// When I was last asked, in seconds.
    pub at: i64,
}

/// One of my requests, as much as telling what changed needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Own {
    pub forge: Forge,
    pub standing: Standing,
    pub approved_by: BTreeSet<String>,
    /// Who has asked for changes.
    pub changes_by: BTreeSet<String>,
    pub failed: bool,
    pub threads: Option<i64>,
    pub title: String,
    pub reference: String,
    pub url: String,
}

impl Default for Own {
    fn default() -> Self {
        Own {
            forge: Forge::default(),
            standing: Standing::Waiting,
            approved_by: BTreeSet::new(),
            changes_by: BTreeSet::new(),
            failed: false,
            threads: None,
            title: String::new(),
            reference: String::new(),
            url: String::new(),
        }
    }
}

impl Own {
    fn of(item: &MergeRequest, standing: Standing) -> Own {
        Own {
            forge: item.forge,
            standing,
            approved_by: item.approved_by.iter().cloned().collect(),
            changes_by: item.reviewers.iter().filter(|r| r.state == ReviewState::RequestedChanges).map(|r| r.person.username.clone()).collect(),
            failed: item.pipeline == Some(PipelineStatus::Failed),
            threads: item.unresolved_threads,
            title: item.title.clone(),
            reference: item.short_reference(),
            url: item.web_url.clone(),
        }
    }
}

impl Snapshot {
    /// The snapshot before there was one: the review requests already notified about
    /// ("id@seconds", as Mrow kept them before the inbox), for the accounts that loaded.
    pub fn from_known(known: &BTreeSet<String>, waiting: &[&MergeRequest], forges: &[Forge]) -> Snapshot {
        let mut snapshot = Snapshot { review_forges: forges.iter().copied().collect(), ..Snapshot::default() };
        for entry in known {
            let Some(at) = entry.rfind('@') else { continue };
            let (id, seconds) = (&entry[..at], entry[at + 1..].parse::<i64>());
            if let (Some(item), Ok(seconds)) = (waiting.iter().find(|m| m.id == id), seconds) {
                snapshot.reviews.insert(id.to_string(), Asked { forge: item.forge, at: seconds });
            }
        }
        snapshot
    }
}

/// One account's side of a refresh that loaded.
pub struct Look<'a> {
    pub forge: Forge,
    /// My username there.
    pub me: &'a str,
    pub open: &'a [MergeRequest],
    /// My open requests; None when they couldn't be fetched, which says nothing about them.
    pub mine: Option<&'a [MergeRequest]>,
    /// My requests filled every page asked for, so there may be more: one missing from the
    /// list may only have slipped past its end, and says nothing about having merged.
    pub mine_capped: bool,
    /// Finished requests this refresh saw (the Done list, and any of mine a search still
    /// listed after it merged), for telling a merge from a close.
    pub ended: &'a [MergeRequest],
}

/// "Maya Lindqvist", "Maya Lindqvist and Tomás Varga", "Maya Lindqvist and 2 others".
fn names(people: &[String]) -> String {
    match people {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, rest @ ..] => format!("{a} and {} others", rest.len()),
    }
}

/// What changed between `prev` and the accounts in `looks`, and the snapshot to keep. An account
/// missing from `looks` (it failed, or is off) keeps its part of the snapshot. `merged_here` are
/// requests merged from Mrow: my own doing, so their leaving is no news.
pub fn detect(prev: &Snapshot, looks: &[Look], merged_here: &HashSet<String>, now: Date) -> (Snapshot, Vec<Event>) {
    let mut next = prev.clone();
    let mut events = Vec::new();
    for look in looks {
        let forge = look.forge;

        // Reviews asked of me.
        let waiting: Vec<&MergeRequest> = look.open.iter().filter(|m| m.my_state.needs_review()).collect();
        let first = !prev.review_forges.contains(&forge);
        for m in &waiting {
            let asker = m.requested_by.as_ref().unwrap_or(&m.author);
            let at = if m.requested_at_is_guess { now } else { m.requested_at };
            let kind = match prev.reviews.get(&m.id) {
                _ if first => continue,
                None if m.was_re_requested() && !m.requested_at_is_guess => Kind::AskedAgain,
                None => Kind::ReviewAsked,
                // A failed note lookup can swap the request time for the creation time; that's no new ask.
                Some(asked) if !m.requested_at_is_guess && m.requested_at.timestamp() > asked.at + SAME_ASK_SECONDS => Kind::AskedAgain,
                Some(_) => continue,
            };
            let title = match kind {
                Kind::AskedAgain => format!("{} asked again", asker.name),
                _ => format!("{} asked for your review", asker.name),
            };
            events.push(Event::new(&Own::of(m, Standing::Waiting), &m.id, kind, title, at));
        }
        next.reviews.retain(|_, a| a.forge != forge);
        for m in &waiting {
            // Kept from before when it's the same ask, so a guessed time never moves it.
            let at = match prev.reviews.get(&m.id) {
                Some(a) if m.requested_at_is_guess || m.requested_at.timestamp() <= a.at + SAME_ASK_SECONDS => a.at,
                _ => m.requested_at.timestamp(),
            };
            next.reviews.insert(m.id.clone(), Asked { forge, at });
        }
        next.review_forges.insert(forge);

        // My own requests.
        let Some(mine) = look.mine else { continue };
        let first = !prev.mine_forges.contains(&forge);
        let all: Vec<&MergeRequest> = mine.iter().chain(look.open).collect();
        let mut seen: BTreeMap<String, Own> = BTreeMap::new();
        for m in mine {
            let own = Own::of(m, mine::status(m, mine::below(m, &all)).standing);
            if let Some(before) = prev.mine.get(&m.id).filter(|_| !first) {
                events.extend(changes(m, before, &own, look.me, now));
            }
            seen.insert(m.id.clone(), own);
        }
        if !first && !look.mine_capped {
            for (id, before) in prev.mine.iter().filter(|(id, o)| o.forge == forge && !seen.contains_key(*id)) {
                if merged_here.contains(id) {
                    continue;
                }
                let title = match look.ended.iter().find(|m| &m.id == id).map(|m| m.state.as_str()) {
                    Some("merged") => "Merged",
                    Some("closed") => "Closed",
                    _ => "Merged or closed",
                };
                events.push(Event::new(before, id, Kind::Closed, title.into(), now));
            }
        }
        next.mine.retain(|_, o| o.forge != forge);
        next.mine.extend(seen);
        next.mine_forges.insert(forge);
    }
    (next, events)
}

/// What changed on one of my requests since `before`. My own doing (approving my own) isn't news.
fn changes(item: &MergeRequest, before: &Own, now_own: &Own, me: &str, now: Date) -> Vec<Event> {
    let mut events = Vec::new();
    let name = |username: &String| item.reviewers.iter().find(|r| &r.person.username == username).map_or_else(|| username.clone(), |r| r.person.name.clone());
    let event = |kind, title: String| Event::new(now_own, &item.id, kind, title, now);
    let ready = now_own.standing == Standing::Ready && before.standing != Standing::Ready;

    let asked: Vec<String> = now_own.changes_by.difference(&before.changes_by).filter(|u| *u != me).map(name).collect();
    if !asked.is_empty() {
        events.push(event(Kind::ChangesRequested, format!("{} asked for changes", names(&asked))));
    }
    if now_own.failed && !before.failed {
        let title = match &item.failed_job {
            Some(job) => format!("Pipeline failed · {job}"),
            None => "Pipeline failed".into(),
        };
        events.push(event(Kind::PipelineFailed, title));
    }
    if ready {
        events.push(event(Kind::Ready, "Ready to merge".into()));
    }
    // An approval that makes it ready is told by "Ready to merge".
    let approvals: Vec<String> = now_own.approved_by.difference(&before.approved_by).filter(|u| *u != me).map(name).collect();
    if !approvals.is_empty() && !ready {
        events.push(event(Kind::Approved, format!("{} approved", names(&approvals))));
    }
    if let (Some(was), Some(is)) = (before.threads, now_own.threads)
        && is > was
    {
        let more = is - was;
        events.push(event(Kind::Threads, format!("{more} new thread{}", if more == 1 { "" } else { "s" })));
    }
    events
}

/// Adds new events to the inbox, newest first, skipping any it already holds.
pub fn add(inbox: &mut Vec<Event>, found: Vec<Event>) {
    for e in found {
        if !inbox.iter().any(|old| old.id == e.id) {
            inbox.push(e);
        }
    }
    inbox.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.kind.weight().cmp(&b.kind.weight())));
}

/// Keeps the inbox small: the newest `CAP`, none older than `KEEP_DAYS`, and none long about a
/// request that has left every list. `live` is every request in the lists, when all loaded.
pub fn prune(inbox: &mut Vec<Event>, now: Date, live: Option<&HashSet<String>>) {
    inbox.retain(|e| {
        let age = now - e.at;
        let gone = live.is_some_and(|l| !l.contains(&e.request)) && e.kind != Kind::Closed;
        age < Duration::days(KEEP_DAYS) && !(gone && age >= Duration::days(GONE_DAYS))
    });
    inbox.truncate(CAP);
}

/// A banner to post.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    /// The merge request it opens.
    pub request: String,
    pub title: String,
    pub body: String,
}

/// One banner per request with an event set to notify among those a refresh found, led by its
/// most pressing one and counting the rest. `banners` is the master switch.
pub fn notices(found: &[Event], levels: &BTreeMap<Kind, Level>, banners: bool) -> Vec<Notice> {
    if !banners {
        return vec![];
    }
    let mut groups: Vec<Vec<&Event>> = Vec::new();
    for e in found.iter().filter(|e| level(levels, e.kind) != Level::Off) {
        match groups.iter_mut().find(|g| g[0].request == e.request) {
            Some(g) => g.push(e),
            None => groups.push(vec![e]),
        }
    }
    let mut led: Vec<(&Event, usize)> = groups
        .iter()
        .filter_map(|g| g.iter().copied().filter(|e| level(levels, e.kind) == Level::Notify).min_by_key(|e| e.kind.weight()).map(|lead| (lead, g.len())))
        .collect();
    led.sort_by_key(|(lead, _)| lead.kind.weight());
    led.into_iter()
        .take(MAX_NOTICES)
        .map(|(lead, n)| {
            let count = if n > 1 { format!(" · {n} updates") } else { String::new() };
            Notice { request: lead.request.clone(), title: lead.title.clone(), body: format!("{}{count}\n{}", lead.reference, lead.subject) }
        })
        .collect()
}
