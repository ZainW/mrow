//! The merge requests I opened: whose move each one is, the one line that says why, and how
//! they stack. Everything here is worked out from the shared models, so it holds for both forges.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::forge::Forge;
use super::models::{Date, MergeRequest, PipelineStatus, ReviewState, Reviewer};

/// Asked this long ago with no review yet: the wait has gone quiet.
const QUIET_DAYS: i64 = 3;

/// Where one of my merge requests stands, from my side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Standing {
    /// Nothing blocks the merge.
    Ready,
    /// Changes requested, a failed pipeline, conflicts or open threads.
    NeedsYou,
    Waiting,
    /// Waiting, and the reviewers have said nothing for days.
    Quiet,
    /// Would be ready, but it sits on a request that hasn't merged.
    Held,
    Draft,
}

impl Standing {
    /// The filters above the list. Quiet and held requests count as waiting.
    pub const FILTERS: [Standing; 4] = [Standing::Ready, Standing::NeedsYou, Standing::Waiting, Standing::Draft];

    pub fn label(self) -> &'static str {
        match self {
            Standing::Ready => "Ready to merge",
            Standing::NeedsYou => "Needs you",
            Standing::Waiting => "Waiting",
            Standing::Quiet => "Gone quiet",
            Standing::Held => "Held",
            Standing::Draft => "Draft",
        }
    }

    /// As it's counted in a folded stack: "1 held, 2 waiting".
    pub fn word(self) -> &'static str {
        match self {
            Standing::Ready => "ready",
            Standing::NeedsYou => "needs you",
            Standing::Waiting => "waiting",
            Standing::Quiet => "quiet",
            Standing::Held => "held",
            Standing::Draft => "draft",
        }
    }

    /// Whether the next move is mine.
    pub fn on_me(self) -> bool {
        matches!(self, Standing::Ready | Standing::NeedsYou)
    }

    /// The filter it falls under.
    pub fn group(self) -> Standing {
        match self {
            Standing::Quiet | Standing::Held => Standing::Waiting,
            other => other,
        }
    }

    /// Sorting: what can certainly be acted on comes first.
    pub fn rank(self) -> u8 {
        match self {
            Standing::Ready => 0,
            Standing::NeedsYou => 1,
            Standing::Quiet => 2,
            Standing::Held | Standing::Waiting => 3,
            Standing::Draft => 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub standing: Standing,
    /// The next step, in a line: "Maya asked for changes · 2 threads open".
    pub step: String,
}

fn first_name(name: &str) -> &str {
    name.split(' ').next().unwrap_or(name)
}

/// "40 minutes", "5 hours", "3 days".
fn span(since: Date) -> String {
    let minutes = (Utc::now() - since).num_minutes().max(0);
    let unit = |n: i64, name: &str| format!("{n} {name}{}", if n == 1 { "" } else { "s" });
    match minutes {
        0..60 => unit(minutes.max(1), "minute"),
        60..1440 => unit(minutes / 60, "hour"),
        _ => unit(minutes / 1440, "day"),
    }
}

fn capitalized(text: String) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn threads(n: i64) -> String {
    format!("{n} thread{} open", if n == 1 { "" } else { "s" })
}

/// "Maya", "Maya and Tomás", "Maya and 2 others".
fn names(people: &[&str]) -> String {
    match people {
        [] => String::new(),
        [one] => one.to_string(),
        [a, b] => format!("{a} and {b}"),
        [a, rest @ ..] => format!("{a} and {} others", rest.len()),
    }
}

/// Where one of my merge requests stands. `below` is the open request it's stacked on, if any.
pub fn status(item: &MergeRequest, below: Option<&MergeRequest>) -> Status {
    let is = |standing, step: String| Status { standing, step };
    let open_threads = item.unresolved_threads.unwrap_or(0);
    let pending: Vec<&str> = item.reviewers.iter().filter(|r| r.state.needs_review() && !r.approved).map(|r| first_name(&r.person.name)).collect();
    let running = matches!(item.pipeline, Some(PipelineStatus::Running | PipelineStatus::Pending));

    if item.draft {
        let step = if item.reviewers.is_empty() { "Draft · nobody asked yet" } else { "Draft · not ready for review" };
        return is(Standing::Draft, step.into());
    }
    match item.merge_status.as_deref() {
        Some("conflict") => return is(Standing::NeedsYou, "Has conflicts".into()),
        Some("need_rebase") => return is(Standing::NeedsYou, "Needs a rebase".into()),
        _ => {}
    }
    if item.pipeline == Some(PipelineStatus::Failed) {
        let step = match &item.failed_job {
            Some(job) => format!("Pipeline failed · {job}"),
            None => "Pipeline failed".into(),
        };
        return is(Standing::NeedsYou, step);
    }
    if let Some(asker) = item.reviewers.iter().find(|r| r.state == ReviewState::RequestedChanges) {
        let mut step = format!("{} asked for changes", first_name(&asker.person.name));
        if open_threads > 0 {
            step.push_str(&format!(" · {}", threads(open_threads)));
        }
        return is(Standing::NeedsYou, step);
    }
    let mergeable = item.merge_status.as_deref() == Some("mergeable");
    // Open threads are worth knowing about, but they only hold the merge when the forge says so.
    if open_threads > 0 && !mergeable {
        return is(Standing::NeedsYou, threads(open_threads));
    }
    let approvals = item.approved_by.len();
    if mergeable && pending.is_empty() {
        // With threads open they take the approval count's place, so the line still fits the list.
        let approved = match (open_threads, approvals) {
            (0, 0) => String::new(),
            (0, n) => format!("Approved by {n} · "),
            _ => format!("{} · ", threads(open_threads)),
        };
        return match below {
            Some(b) => is(Standing::Held, capitalized(format!("{approved}held until {} merges", b.reference()))),
            None => is(Standing::Ready, capitalized(format!("{approved}ready to merge"))),
        };
    }
    let mut step = if !pending.is_empty() {
        format!("Waiting on {} · {}", names(&pending), span(item.updated_at))
    } else if item.reviewers.is_empty() {
        "No reviewers yet".into()
    } else if approvals > 0 {
        match item.merge_status_label() {
            Some(label) if !running => format!("Approved by {approvals} · {}", label.to_lowercase()),
            _ => format!("Approved by {approvals}"),
        }
    } else {
        item.merge_status_label().unwrap_or("Waiting").to_string()
    };
    if running {
        step.push_str(" · pipeline running");
    }
    let quiet = !pending.is_empty() && (Utc::now() - item.updated_at).num_days() >= QUIET_DAYS;
    if item.auto_merge {
        let mut chars = step.chars();
        step = format!("Auto-merge on · {}", chars.next().map(|c| c.to_lowercase().chain(chars).collect::<String>()).unwrap_or_default());
    }
    is(if quiet { Standing::Quiet } else { Standing::Waiting }, step)
}

/// What merging a stack does next, one part at a time from the bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum WalkStep {
    /// Every part has merged.
    Done,
    /// The next part isn't ready, but nothing is asked of anyone: its pipeline is running, or
    /// the part below it is still merging.
    Wait,
    /// The part below merged; the next one still targets its branch and has to move down.
    Retarget(String),
    Merge,
    /// The next part needs a person, so the walk ends here, with why.
    Stop(String),
}

/// The next step for a stack being merged onto `target`. `next` is its lowest part still open,
/// and `all` is everything open, for seeing whether the part below `next` is still there.
pub fn walk_step(next: Option<&MergeRequest>, target: &str, all: &[&MergeRequest]) -> WalkStep {
    let Some(next) = next else { return WalkStep::Done };
    if next.target_branch != target {
        return if below(next, all).is_some() { WalkStep::Wait } else { WalkStep::Retarget(target.to_string()) };
    }
    let status = status(next, below(next, all));
    let waiting_on_someone = next.reviewers.iter().any(|r| r.state.needs_review() && !r.approved);
    match status.standing {
        Standing::Ready => WalkStep::Merge,
        Standing::Held => WalkStep::Wait,
        Standing::Waiting | Standing::Quiet if !waiting_on_someone => WalkStep::Wait,
        _ => WalkStep::Stop(status.step),
    }
}

/// The line under a review's title: who asked, or what I said. None for finished ones.
pub fn review_reason(item: &MergeRequest) -> Option<String> {
    if !item.is_open() {
        return None;
    }
    if item.my_state.needs_review() {
        if item.requested_at_is_guess {
            return Some(format!("{} opened this", first_name(&item.author.name)));
        }
        let asker = item.requested_by.as_ref().unwrap_or(&item.author);
        let verb = if item.was_re_requested() { "asked again" } else { "asked for your review" };
        return Some(format!("{} {verb}", first_name(&asker.name)));
    }
    Some(
        match item.my_state {
            ReviewState::Approved => "You approved",
            ReviewState::RequestedChanges => "You asked for changes",
            _ => "You commented",
        }
        .into(),
    )
}

/// The reason line of a request that came back to To review after I asked changes.
pub fn came_back_reason(item: &MergeRequest, moved: bool) -> String {
    if moved { "You moved this back".into() } else { format!("{} pushed new commits", first_name(&item.author.name)) }
}

/// The line under a request waiting on its author, in the fold.
pub fn waiting_line(_item: &MergeRequest) -> String {
    "No new commits".into()
}

// MARK: Stacks

/// A chain of merge requests, each targeting the branch of the one before it. Bottom first.
#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    /// The bottom request's ID, which names the stack while it lasts.
    pub key: String,
    pub name: String,
    pub ids: Vec<String>,
}

/// The open request `item` is stacked on: the one whose source branch it targets.
pub fn below<'a>(item: &MergeRequest, all: &[&'a MergeRequest]) -> Option<&'a MergeRequest> {
    all.iter().copied().find(|other| {
        other.id != item.id
            && other.is_open()
            && other.forge == item.forge
            && other.project_path == item.project_path
            && other.source_branch == item.target_branch
    })
}

/// Every stack of two or more among `all`, found by matching branches within a project.
pub fn stacks(all: &[&MergeRequest]) -> Vec<Stack> {
    let open: Vec<&MergeRequest> = all.iter().copied().filter(|m| m.is_open()).collect();
    let mut found = Vec::new();
    for bottom in open.iter().copied().filter(|m| below(m, &open).is_none()) {
        let mut ids = vec![bottom.id.clone()];
        climb(bottom, &open, &mut ids);
        if ids.len() > 1 {
            let titles: Vec<&str> = ids.iter().filter_map(|id| open.iter().find(|m| &m.id == id)).map(|m| m.title.as_str()).collect();
            found.push(Stack { key: bottom.id.clone(), name: stack_name(&titles).unwrap_or_else(|| bottom.source_branch.clone()), ids });
        }
    }
    found
}

/// Adds what's stacked on `under`, each followed by what's stacked on it. A request with two
/// on top of it lists both, lowest number first.
fn climb(under: &MergeRequest, open: &[&MergeRequest], ids: &mut Vec<String>) {
    let mut above: Vec<&MergeRequest> = open.iter().copied().filter(|m| below(m, open).is_some_and(|b| b.id == under.id)).collect();
    above.sort_by_key(|m| m.iid.parse::<i64>().unwrap_or(0));
    for m in above {
        if !ids.contains(&m.id) {
            ids.push(m.id.clone());
            climb(m, open, ids);
        }
    }
}

/// What the titles share at the start: "Ledger 1: add…" and "Ledger 2: paginate…" give "Ledger".
fn stack_name(titles: &[&str]) -> Option<String> {
    let first: Vec<&str> = titles.first()?.split_whitespace().collect();
    let mut shared = first.len();
    for title in &titles[1..] {
        let words: Vec<&str> = title.split_whitespace().collect();
        shared = shared.min(first.iter().zip(&words).take_while(|(a, b)| a == b).count());
    }
    let name = first[..shared].join(" ");
    let name = name.trim_end_matches(|c: char| !c.is_alphanumeric()).trim_start_matches("Draft:").trim();
    (name.chars().count() >= 3).then(|| name.to_string())
}

/// The sentence under the next step: what happened, or what will. `above` is what's stacked on it.
pub fn explain(item: &MergeRequest, status: &Status, below: Option<&MergeRequest>, above: Option<&MergeRequest>) -> String {
    let pending: Vec<&str> = item.reviewers.iter().filter(|r| r.state.needs_review() && !r.approved).map(|r| r.person.name.as_str()).collect();
    let (has, was) = if pending.len() == 1 { ("hasn't", "was") } else { ("haven't", "were") };
    match status.standing {
        Standing::Ready => {
            let open = item.unresolved_threads.unwrap_or(0);
            let first = match open {
                0 => "Nothing blocks the merge.",
                1 => "The open thread doesn't block the merge.",
                _ => "The open threads don't block the merge.",
            };
            match above {
                Some(a) => format!("{first} {} moves onto {} when this merges.", a.reference(), item.target_branch),
                None => first.into(),
            }
        }
        Standing::Held => match below {
            Some(b) => format!("It sits on {}, which hasn't merged. It becomes ready the moment that does.", b.reference()),
            None => "It sits on a request that hasn't merged.".into(),
        },
        Standing::NeedsYou => match item.merge_status.as_deref() {
            Some("conflict") => format!("It conflicts with {}. Rebase it, or merge {} in.", item.target_branch, item.target_branch),
            Some("need_rebase") if item.kind() == Forge::Github => format!("{} has moved on. Update the branch before it can merge.", item.target_branch),
            Some("need_rebase") => format!("{} has moved on. Rebase it before it can merge.", item.target_branch),
            _ if item.pipeline == Some(PipelineStatus::Failed) => "The pipeline failed on the latest push.".into(),
            _ => match item.reviewers.iter().find(|r| r.state == ReviewState::RequestedChanges) {
                Some(asker) => format!("{} asked for changes. Push a fix or reply, then ask again.", asker.person.name),
                None => format!("Reviewers left threads that are still open, and {} won't merge it until they're resolved.", item.kind().name()),
            },
        },
        Standing::Waiting | Standing::Quiet if item.auto_merge => format!("{} merges it by itself once everything passes.", item.kind().name()),
        Standing::Quiet => format!("{} {was} asked {} ago and {has} reviewed yet. Ask again, or add someone else.", names(&pending), span(item.updated_at)),
        Standing::Waiting if pending.is_empty() => "Nothing for you to do right now.".into(),
        Standing::Waiting => format!("Nothing for you to do. {} {has} reviewed yet.", names(&pending)),
        Standing::Draft => "Nobody is asked to review a draft. Mark it ready when you want reviews.".into(),
    }
}

// MARK: Managing

/// Who "Ask Again" asks: the reviewers a quiet request is still waiting on. Nobody when it
/// hasn't gone quiet; until then, asking again is nagging.
pub fn ask_again(item: &MergeRequest, status: &Status) -> Vec<Reviewer> {
    if status.standing != Standing::Quiet || item.auto_merge {
        return vec![];
    }
    item.reviewers.iter().filter(|r| r.state.needs_review() && !r.approved).cloned().collect()
}

/// Whether its branch has to catch up with the target before it can merge: the forge offers a
/// rebase (GitLab) or an update (GitHub). A conflict needs a person, so it isn't offered then.
pub fn needs_update(item: &MergeRequest) -> bool {
    item.is_open() && item.merge_status.as_deref() == Some("need_rebase")
}

/// Whether a reviewer can be taken off the request.
pub fn can_remove(item: &MergeRequest, reviewer: &Reviewer) -> bool {
    item.kind().removes_reviewed() || reviewer.state == ReviewState::Unreviewed
}

/// The ways GitLab reads a title as a draft's, longest first; matched without case.
const DRAFT_PREFIXES: [&str; 6] = ["draft -", "draft:", "[draft]", "(draft)", "[wip]", "wip:"];

/// A title split into its draft prefix (with the space after it) and the rest. GitLab keeps a
/// draft's flag in its title, so the prefix is set aside while editing and put back on save:
/// an edit never turns a draft ready, or a ready one into a draft, without saying so.
pub fn title_parts(item: &MergeRequest) -> (&str, &str) {
    let title = item.title.as_str();
    if item.kind() != Forge::Gitlab || !item.draft {
        return ("", title);
    }
    let lower = title.to_ascii_lowercase();
    let Some(prefix) = DRAFT_PREFIXES.iter().find(|p| lower.starts_with(*p)) else { return ("", title) };
    let rest = title[prefix.len()..].trim_start();
    (&title[..title.len() - rest.len()], rest)
}

/// The title to save from what was typed: the draft prefix put back, the rest trimmed.
pub fn edited_title(item: &MergeRequest, typed: &str) -> String {
    format!("{}{}", title_parts(item).0, typed.trim())
}
