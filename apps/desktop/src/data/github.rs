//! GitHub through `gh`: GraphQL searches for the queue, REST for files, comments and reviews,
//! all mapped onto the same models GitLab fills.
//!
//! GitHub has no per-reviewer state like GitLab's, so it's pieced together: a pending review
//! request means it's my turn; otherwise my latest review (approved, changes requested,
//! commented, dismissed) says where it stands. "When I was asked" comes from the timeline's
//! review-requested events.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::cli::{Cli, paged_rest, parallel};
use super::diff::{DiffLine, LineKind};
use super::forge::{Forge, QueueResult};
use super::models::*;

const ACTOR: &str = "login avatarUrl ... on User { name }";

/// A pull request's fields. `mine` adds what only matters on my own: which threads are open
/// and which check failed.
fn pull_fields(mine: bool) -> String {
    let checks = if mine { " contexts(first: 40) { nodes { ... on CheckRun { name conclusion } ... on StatusContext { context state } } }" } else { "" };
    let threads = if mine { "reviewThreads(first: 100) { nodes { isResolved } } autoMergeRequest { enabledAt }" } else { "" };
    let method = if mine { " viewerDefaultMergeMethod" } else { "" };
    format!(
        "id number title url createdAt updatedAt mergedAt closedAt state isDraft
    headRefName baseRefName mergeStateStatus totalCommentsCount body
    additions deletions changedFiles
    repository {{ nameWithOwner name{method} }}
    author {{ {ACTOR} }}
    reviewRequests(first: 20) {{
      nodes {{ requestedReviewer {{ ... on User {{ login name avatarUrl }} ... on Team {{ slug name }} }} }}
    }}
    latestReviews(first: 30) {{ nodes {{ state author {{ {ACTOR} }} }} }}
    labels(first: 20) {{ nodes {{ name color }} }}
    commits(last: 1) {{ nodes {{ commit {{ statusCheckRollup {{ state{checks} }} }} }} }}
    {threads}
    timelineItems(itemTypes: [REVIEW_REQUESTED_EVENT], last: 50) {{
      nodes {{
        ... on ReviewRequestedEvent {{
          createdAt
          actor {{ {ACTOR} }}
          requestedReviewer {{ ... on User {{ login }} ... on Team {{ slug }} }}
        }}
      }}
    }}"
    )
}

fn search_query(mine: bool) -> String {
    format!(
        "query($q: String!, $after: String) {{
      viewer {{ login name avatarUrl }}
      search(query: $q, type: ISSUE, first: 30, after: $after) {{
        pageInfo {{ hasNextPage endCursor }}
        nodes {{ ... on PullRequest {{ {} }} }}
      }}
    }}",
        pull_fields(mine)
    )
}

const REVIEW_THREADS: &str = "query($owner: String!, $name: String!, $number: Int!) {
      repository(owner: $owner, name: $name) {
        pullRequest(number: $number) {
          reviewThreads(first: 100) { nodes { id isResolved comments(first: 1) { nodes { databaseId } } } }
        }
      }
    }";

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Actor {
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct SearchPayload {
    viewer: Option<Actor>,
    search: Search,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Search {
    page_info: SearchPageInfo,
    /// Search results can include issues (empty objects), and unreadable nodes come back null.
    nodes: Vec<Option<Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchPageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize, Clone)]
struct Nodes<T> {
    nodes: Vec<T>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RequestedReviewer {
    /// A user has a login, a team a slug; each may have a name.
    login: Option<String>,
    slug: Option<String>,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ReviewRequest {
    requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Deserialize, Clone)]
struct ReviewNode {
    state: String,
    author: Option<Actor>,
}

#[derive(Deserialize, Clone)]
struct LabelNode {
    name: String,
    color: String,
}

#[derive(Deserialize, Clone)]
struct CommitNode {
    commit: CommitInner,
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CommitInner {
    status_check_rollup: Option<Rollup>,
}
#[derive(Deserialize, Clone)]
struct Rollup {
    state: String,
    contexts: Option<Nodes<Option<Check>>>,
}

/// A check run (name, conclusion) or a commit status (context, state).
#[derive(Deserialize, Clone)]
struct Check {
    name: Option<String>,
    conclusion: Option<String>,
    context: Option<String>,
    state: Option<String>,
}

impl Check {
    /// Its name, if it failed.
    fn failed(&self) -> Option<&str> {
        let run = matches!(self.conclusion.as_deref(), Some("FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED"));
        let status = matches!(self.state.as_deref(), Some("FAILURE" | "ERROR"));
        if run {
            self.name.as_deref()
        } else if status {
            self.context.as_deref()
        } else {
            None
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ThreadNode {
    is_resolved: bool,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RequestEvent {
    created_at: Option<Date>,
    actor: Option<Actor>,
    requested_reviewer: Option<RequestedReviewer>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PullNode {
    id: String,
    number: i64,
    title: String,
    url: String,
    created_at: Date,
    updated_at: Date,
    merged_at: Option<Date>,
    closed_at: Option<Date>,
    state: String,
    is_draft: bool,
    head_ref_name: String,
    base_ref_name: String,
    merge_state_status: Option<String>,
    total_comments_count: Option<i64>,
    body: Option<String>,
    additions: i64,
    deletions: i64,
    changed_files: i64,
    repository: Repo,
    author: Option<Actor>,
    review_requests: Option<Nodes<ReviewRequest>>,
    latest_reviews: Option<Nodes<ReviewNode>>,
    labels: Option<Nodes<LabelNode>>,
    commits: Option<Nodes<CommitNode>>,
    timeline_items: Option<Nodes<Option<RequestEvent>>>,
    review_threads: Option<Nodes<ThreadNode>>,
    /// Present (whatever is in it) when auto-merge is on.
    auto_merge_request: Option<Value>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Repo {
    name_with_owner: String,
    name: String,
    viewer_default_merge_method: Option<String>,
}

fn cli() -> Cli {
    Cli::gh()
}

// MARK: Queue

pub fn queue(done_days: i64) -> Result<QueueResult, String> {
    let since = chrono::Utc::now() - chrono::Duration::days(done_days);
    let day = since.format("%Y-%m-%d").to_string();
    let common = "is:pr archived:false";
    let queries = [
        (format!("{common} is:open review-requested:@me"), 5),
        (format!("{common} is:open reviewed-by:@me -author:@me"), 3),
        (format!("{common} is:closed reviewed-by:@me -author:@me updated:>={day}"), 2),
        (format!("{common} is:closed review-requested:@me updated:>={day}"), 1),
    ];
    type Found = Result<(Option<Actor>, Vec<PullNode>), String>;
    let mut jobs: Vec<Box<dyn FnOnce() -> Found + Send>> =
        queries.into_iter().map(|(q, pages)| Box::new(move || search(&q, pages, false)) as Box<dyn FnOnce() -> Found + Send>).collect();
    jobs.push(Box::new(move || search(&format!("{common} is:open author:@me"), 2, true)));
    let mut results = parallel(jobs).into_iter();
    let (asked, mine, done_a, done_b) = (results.next().unwrap()?, results.next().unwrap()?, results.next().unwrap()?, results.next().unwrap()?);
    let viewer = asked.0.clone().or(mine.0.clone()).ok_or("gh isn't signed in.")?;
    let me = Person { username: viewer.login.clone(), name: viewer.name.clone().unwrap_or(viewer.login.clone()), avatar_url: viewer.avatar_url.clone() };

    // Asked through a team still counts as asked, though no request names me.
    let asked_ids: HashSet<String> = asked.1.iter().map(|n| n.id.clone()).collect();
    let open = unique(asked.1.into_iter().chain(mine.1)).iter().map(|n| build(n, &me.username, asked_ids.contains(&n.id))).collect();
    let done = unique(done_a.1.into_iter().chain(done_b.1)).iter().map(|n| build(n, &me.username, false)).collect();
    // My own are a second ask: if it fails, the reviews above still show.
    let (mine, mine_error) = match results.next().unwrap() {
        Ok(found) => (found.1.iter().map(|n| build(n, &me.username, false)).collect(), None),
        Err(e) => (vec![], Some(e)),
    };
    Ok(QueueResult { me, open, done, mine, mine_error })
}

fn search(query: &str, pages: usize, mine: bool) -> Result<(Option<Actor>, Vec<PullNode>), String> {
    let mut viewer = None;
    let mut nodes = Vec::new();
    let mut cursor: Option<String> = None;
    let q = search_query(mine);
    for _ in 0..pages {
        let payload: SearchPayload = cli().graphql(&q, &[("q", Some(query.to_string())), ("after", cursor.clone())], &[])?;
        viewer = payload.viewer;
        for node in payload.search.nodes.into_iter().flatten() {
            if node.get("id").is_some() {
                let pull: PullNode = serde_json::from_value(node).map_err(|e| format!("GitHub sent something unexpected: {e}"))?;
                nodes.push(pull);
            }
        }
        match (payload.search.page_info.has_next_page, payload.search.page_info.end_cursor) {
            (true, Some(next)) => cursor = Some(next),
            _ => break,
        }
    }
    Ok((viewer, nodes))
}

fn unique(nodes: impl Iterator<Item = PullNode>) -> Vec<PullNode> {
    let mut seen = HashSet::new();
    nodes.filter(|n| seen.insert(n.id.clone())).collect()
}

pub fn review_state(raw: &str) -> ReviewState {
    match raw {
        "APPROVED" => ReviewState::Approved,
        "CHANGES_REQUESTED" => ReviewState::RequestedChanges,
        "COMMENTED" => ReviewState::Reviewed,
        "DISMISSED" => ReviewState::Unapproved,
        "PENDING" => ReviewState::ReviewStarted,
        _ => ReviewState::Unknown,
    }
}

fn person(actor: Option<&Actor>) -> Person {
    match actor {
        None => Person::new("ghost", "Unknown"),
        Some(a) => Person { username: a.login.clone(), name: a.name.clone().unwrap_or(a.login.clone()), avatar_url: a.avatar_url.clone() },
    }
}

/// One pull request's JSON, for the self-tests.
#[cfg(test)]
pub fn build_from_json(json: &str, me: &str, pending: bool) -> Result<MergeRequest, String> {
    let node: PullNode = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(build(&node, me, pending))
}

fn build(node: &PullNode, me: &str, pending: bool) -> MergeRequest {
    let is_me = |login: Option<&str>| login.is_some_and(|l| l.eq_ignore_ascii_case(me));

    // Reviewers: pending requests first (a re-request supersedes the old review), then reviews.
    let mut reviewers = Vec::new();
    let mut listed = HashSet::new();
    let mut requested_from_me = false;
    for request in node.review_requests.as_ref().map_or(&[][..], |n| &n.nodes) {
        let Some(r) = &request.requested_reviewer else { continue };
        if let Some(login) = &r.login {
            if is_me(Some(login)) {
                requested_from_me = true;
            }
            if !listed.insert(login.to_lowercase()) {
                continue;
            }
            reviewers.push(Reviewer {
                person: Person { username: login.clone(), name: r.name.clone().unwrap_or(login.clone()), avatar_url: r.avatar_url.clone() },
                state: ReviewState::Unreviewed,
                approved: false,
            });
        } else if let Some(slug) = &r.slug {
            if !listed.insert(format!("team:{slug}")) {
                continue;
            }
            reviewers.push(Reviewer { person: Person::new(slug, r.name.as_deref().unwrap_or(slug)), state: ReviewState::Unreviewed, approved: false });
        }
    }
    let mut my_review = None;
    for review in node.latest_reviews.as_ref().map_or(&[][..], |n| &n.nodes) {
        let author = person(review.author.as_ref());
        let state = review_state(&review.state);
        if is_me(Some(&author.username)) {
            my_review = Some(state);
        }
        if !listed.insert(author.username.to_lowercase()) {
            continue;
        }
        reviewers.push(Reviewer { person: author, state, approved: state == ReviewState::Approved });
    }
    let my_state = if pending || requested_from_me {
        // Asked (again): it's my turn, whatever I said last time.
        if my_review == Some(ReviewState::Unapproved) { ReviewState::Unapproved } else { ReviewState::Unreviewed }
    } else {
        my_review.unwrap_or(ReviewState::Unknown)
    };

    // When I was asked: requests naming me, or else (asked through a team) any team request.
    let events: Vec<&RequestEvent> =
        node.timeline_items.as_ref().map(|t| t.nodes.iter().flatten().filter(|e| e.created_at.is_some()).collect()).unwrap_or_default();
    let mut requests: Vec<&RequestEvent> = events.iter().copied().filter(|e| is_me(e.requested_reviewer.as_ref().and_then(|r| r.login.as_deref()))).collect();
    if requests.is_empty() {
        requests = events.iter().copied().filter(|e| e.requested_reviewer.as_ref().is_some_and(|r| r.slug.is_some())).collect();
    }
    requests.sort_by_key(|e| e.created_at);

    let state = match node.state.as_str() {
        "MERGED" => "merged",
        "CLOSED" => "closed",
        _ => "opened",
    };
    let checks = node.commits.as_ref().and_then(|c| c.nodes.last()).and_then(|c| c.commit.status_check_rollup.as_ref());
    let rollup = checks.map(|r| r.state.as_str());
    let failed_job = checks.and_then(|r| r.contexts.as_ref()).and_then(|c| c.nodes.iter().flatten().find_map(Check::failed)).map(String::from);
    let pipeline = match rollup {
        Some("SUCCESS") => Some(PipelineStatus::Success),
        Some("FAILURE" | "ERROR") => Some(PipelineStatus::Failed),
        Some("PENDING" | "EXPECTED") => Some(PipelineStatus::Running),
        _ => None,
    };
    let merge_status = match node.merge_state_status.as_deref() {
        Some("CLEAN" | "HAS_HOOKS") => Some("mergeable"),
        Some("DIRTY") => Some("conflict"),
        Some("BEHIND") => Some("need_rebase"),
        Some("BLOCKED") => Some("blocked_status"),
        Some("DRAFT") => Some("draft_status"),
        _ => None,
    };
    let labels = node
        .labels
        .as_ref()
        .map(|l| l.nodes.iter().map(|l| Label { title: l.name.clone(), color: format!("#{}", l.color), text_color: text_color(&l.color) }).collect())
        .unwrap_or_default();
    let approved_by = reviewers.iter().filter(|r| r.approved).map(|r| r.person.username.clone()).collect();
    MergeRequest {
        id: node.id.clone(),
        iid: node.number.to_string(),
        title: node.title.clone(),
        web_url: node.url.clone(),
        project_path: node.repository.name_with_owner.clone(),
        project_name: node.repository.name.clone(),
        author: person(node.author.as_ref()),
        created_at: node.created_at,
        updated_at: node.updated_at,
        merged_at: node.merged_at,
        closed_at: node.closed_at,
        state: state.into(),
        draft: node.is_draft,
        source_branch: node.head_ref_name.clone(),
        target_branch: node.base_ref_name.clone(),
        merge_status: merge_status.map(String::from),
        comment_count: node.total_comments_count.unwrap_or(0),
        description: node.body.clone().unwrap_or_default(),
        reviewers,
        approved_by,
        additions: node.additions,
        deletions: node.deletions,
        file_count: node.changed_files,
        pipeline,
        labels,
        requested_at: requests.last().and_then(|e| e.created_at).unwrap_or(node.created_at),
        first_requested_at: requests.first().and_then(|e| e.created_at).unwrap_or(node.created_at),
        requested_by: requests.last().map(|e| person(e.actor.as_ref())),
        requested_at_is_guess: requests.is_empty(),
        my_state,
        forge: Forge::Github,
        unresolved_threads: node.review_threads.as_ref().map(|t| t.nodes.iter().filter(|n| !n.is_resolved).count() as i64),
        failed_job,
        auto_merge: node.auto_merge_request.is_some(),
        merge_method: node.repository.viewer_default_merge_method.as_ref().map(|m| m.to_lowercase()),
    }
}

/// GitHub gives a label only its background; pick black or white text by its luminance.
pub fn text_color(hex: &str) -> String {
    let Some(value) = (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten() else {
        return "#1f2328".into();
    };
    let channel = |shift: u32| {
        let c = ((value >> shift) & 0xFF) as f64 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    if luminance > 0.4 { "#1f2328".into() } else { "#ffffff".into() }
}

// MARK: Details

fn base(item: &MergeRequest) -> String {
    format!("repos/{}/pulls/{}", item.project_path, item.iid)
}

#[derive(Deserialize)]
struct GhFile {
    filename: String,
    previous_filename: Option<String>,
    status: String,
    changes: i64,
    patch: Option<String>,
}

pub fn diffs(item: &MergeRequest) -> Result<(Vec<FileDiff>, Option<DiffRefs>), String> {
    for _ in 0..2 {
        let before = refs(item).ok();
        let files = files(item)?;
        let after = refs(item).ok();
        if before == after {
            return Ok((files, after));
        }
    }
    Ok((files(item)?, None))
}

fn files(item: &MergeRequest) -> Result<Vec<FileDiff>, String> {
    let files: Vec<GhFile> = paged_rest(&cli(), &format!("{}/files", base(item)), 30)?;
    Ok(files
        .into_iter()
        .map(|f| {
            FileDiff {
                old_path: f.previous_filename.unwrap_or(f.filename.clone()),
                new_path: f.filename,
                too_large: f.patch.is_none() && f.changes > 0,
                diff: f.patch.unwrap_or_default(),
                new_file: f.status == "added",
                renamed_file: f.status == "renamed",
                deleted_file: f.status == "removed",
                collapsed: false,
                generated_file: false,
                lines: Vec::new(),
                added: 0,
                removed: 0,
                digest: 0,
            }
            .prepared()
        })
        .collect())
}

fn refs(item: &MergeRequest) -> Result<DiffRefs, String> {
    #[derive(Deserialize)]
    struct Ref {
        sha: String,
    }
    #[derive(Deserialize)]
    struct Payload {
        base: Ref,
        head: Ref,
    }
    let p: Payload = cli().api_json(&base(item))?;
    Ok(DiffRefs { base_sha: p.base.sha.clone(), start_sha: p.base.sha, head_sha: p.head.sha })
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
}

#[derive(Deserialize)]
struct GhComment {
    id: i64,
    body: Option<String>,
    user: Option<GhUser>,
    created_at: Date,
    path: Option<String>,
    line: Option<i64>,
    original_line: Option<i64>,
    side: Option<String>,
    original_commit_id: Option<String>,
    in_reply_to_id: Option<i64>,
    pull_request_review_id: Option<i64>,
    node_id: Option<String>,
}

#[derive(Deserialize)]
struct GhReview {
    id: i64,
    body: Option<String>,
    user: Option<GhUser>,
    state: String,
    submitted_at: Option<Date>,
}

fn user(u: Option<&GhUser>) -> Person {
    let login = u.map_or("ghost", |u| &u.login);
    Person::new(login, if u.is_some() { login } else { "Unknown" })
}

/// Conversation comments, review summaries and line threads, in the order they started.
pub fn discussions(item: &MergeRequest) -> Result<Vec<Discussion>, String> {
    let repo = format!("repos/{}", item.project_path);
    let (issue_path, line_path, review_path) =
        (format!("{repo}/issues/{}/comments", item.iid), format!("{}/comments", base(item)), format!("{}/reviews", base(item)));
    let item2 = item.clone();
    let issue = std::thread::spawn(move || paged_rest::<GhComment>(&cli(), &issue_path, 10));
    let lines = std::thread::spawn(move || paged_rest::<GhComment>(&cli(), &line_path, 10));
    let reviews = std::thread::spawn(move || paged_rest::<GhReview>(&cli(), &review_path, 5));
    let resolution = std::thread::spawn(move || resolved_threads(&item2));
    let comments = issue.join().unwrap()?;
    let lines = lines.join().unwrap()?;
    let summaries = reviews.join().unwrap()?;
    let resolved = resolution.join().unwrap();
    // My pending review, which only I can see: its comments come from the review itself.
    let pending = match summaries.iter().find(|r| r.state == "PENDING") {
        Some(review) => {
            let comments: Vec<GhComment> = paged_rest(&cli(), &format!("{}/reviews/{}/comments", base(item), review.id), 5)?;
            Some((review.id, comments))
        }
        None => None,
    };
    Ok(threads_of(comments, lines, summaries, resolved, pending))
}

/// Recorded REST responses (and the review threads' resolution), for the self-tests.
#[cfg(test)]
pub fn discussions_from_json(
    issue: &str,
    lines: &str,
    reviews: &str,
    resolved: HashMap<i64, (String, bool)>,
    pending: Option<&str>,
) -> Result<Vec<Discussion>, String> {
    let parse = |raw: &str| serde_json::from_str::<Vec<GhComment>>(raw).map_err(|e| e.to_string());
    let summaries: Vec<GhReview> = serde_json::from_str(reviews).map_err(|e| e.to_string())?;
    let pending = match (pending, summaries.iter().find(|r| r.state == "PENDING")) {
        (Some(raw), Some(review)) => Some((review.id, parse(raw)?)),
        _ => None,
    };
    Ok(threads_of(parse(issue)?, parse(lines)?, summaries, Some(resolved), pending))
}

/// Pieces the threads together: conversation comments, review summaries, line threads, and my
/// pending review's comments in place (a reply in its thread, a new one as a thread of its own).
fn threads_of(
    comments: Vec<GhComment>,
    mut lines: Vec<GhComment>,
    summaries: Vec<GhReview>,
    resolved: Option<HashMap<i64, (String, bool)>>,
    pending: Option<(i64, Vec<GhComment>)>,
) -> Vec<Discussion> {
    // The pull request's comments may list my pending ones too; they're taken from the review.
    if let Some((review, _)) = &pending {
        lines.retain(|c| c.pull_request_review_id != Some(*review));
    }

    let mut threads: Vec<Discussion> =
        comments.iter().map(|c| Discussion { id: format!("ic-{}", c.id), individual_note: true, notes: vec![note(c, None)], node_id: None }).collect();
    for review in &summaries {
        let body = review.body.as_deref().unwrap_or("").trim();
        let Some(at) = review.submitted_at else { continue };
        if body.is_empty() {
            continue;
        }
        let verdict = match review.state.as_str() {
            "APPROVED" => APPROVED,
            "CHANGES_REQUESTED" => CHANGES_REQUESTED,
            _ => "",
        };
        threads.push(Discussion {
            id: format!("rv-{}", review.id),
            individual_note: true,
            notes: vec![Note {
                id: review.id,
                body: format!("{verdict}{body}"),
                author: user(review.user.as_ref()),
                created_at: at,
                system: false,
                resolvable: None,
                resolved: None,
                position: None,
                pending: false,
                node_id: None,
            }],
            node_id: None,
        });
    }
    // Replies point at the thread's first comment.
    lines.sort_by_key(|c| c.created_at);
    let mut order = Vec::new();
    let mut grouped: HashMap<i64, Vec<&GhComment>> = HashMap::new();
    for c in &lines {
        let root = c.in_reply_to_id.unwrap_or(c.id);
        if !grouped.contains_key(&root) {
            order.push(root);
        }
        grouped.entry(root).or_default().push(c);
    }
    for root in order {
        let notes = &grouped[&root];
        let position = line_position(notes[0]);
        let thread = resolved.as_ref().and_then(|r| r.get(&root));
        let is_resolved = thread.map(|t| t.1);
        threads.push(Discussion {
            id: format!("rc-{root}"),
            individual_note: false,
            notes: notes.iter().map(|c| Note { resolvable: Some(is_resolved.is_some()), resolved: is_resolved, ..note(c, Some(position.clone())) }).collect(),
            node_id: thread.map(|t| t.0.clone()),
        });
    }
    threads.sort_by_key(|t| t.notes.first().map(|n| n.created_at));
    let Some((_, drafts)) = pending else { return threads };
    for c in &drafts {
        let draft = Note { pending: true, node_id: c.node_id.clone(), ..note(c, Some(line_position(c))) };
        let home = c.in_reply_to_id.and_then(|to| threads.iter_mut().find(|t| t.id.starts_with("rc-") && t.notes.iter().any(|n| n.id == to)));
        match home {
            Some(thread) => thread.notes.push(draft),
            None => threads.push(Discussion { id: format!("pc-{}", c.id), individual_note: false, notes: vec![draft], node_id: None }),
        }
    }
    threads
}

/// Where a line comment sits. A thread whose line is gone (outdated) is pinned to the commit it
/// was written on, which isn't the head any more, so it isn't placed in the diff.
fn line_position(c: &GhComment) -> Position {
    let left = c.side.as_deref() == Some("LEFT");
    Position {
        new_path: c.path.clone(),
        old_path: c.path.clone(),
        new_line: if c.line.is_some() { if left { None } else { c.line } } else { c.original_line },
        old_line: if c.line.is_some() && left { c.line } else { None },
        head_sha: if c.line.is_none() { Some(c.original_commit_id.clone().unwrap_or("outdated".into())) } else { None },
    }
}

fn note(c: &GhComment, position: Option<Position>) -> Note {
    Note {
        id: c.id,
        body: c.body.clone().unwrap_or_default(),
        author: user(c.user.as_ref()),
        created_at: c.created_at,
        system: false,
        resolvable: None,
        resolved: None,
        position,
        pending: false,
        node_id: None,
    }
}

/// Each line thread's node id and whether it's resolved, by its first comment's ID. REST doesn't
/// say, so it's one GraphQL query; without it threads show no resolved state and can't be resolved.
fn resolved_threads(item: &MergeRequest) -> Option<HashMap<i64, (String, bool)>> {
    #[derive(Deserialize)]
    struct Payload {
        repository: Option<RepoT>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RepoT {
        pull_request: Option<PullT>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PullT {
        review_threads: Nodes<ThreadT>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ThreadT {
        id: String,
        is_resolved: bool,
        comments: Nodes<CommentT>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CommentT {
        database_id: Option<i64>,
    }
    let (owner, name) = item.project_path.split_once('/')?;
    let number: i64 = item.iid.parse().ok()?;
    let payload: Payload = cli().graphql(REVIEW_THREADS, &[("owner", Some(owner.into())), ("name", Some(name.into()))], &[("number", number)]).ok()?;
    let threads = payload.repository?.pull_request?.review_threads.nodes;
    Some(threads.into_iter().filter_map(|t| Some((t.comments.nodes.first()?.database_id?, (t.id, t.is_resolved)))).collect())
}

// MARK: Actions

pub fn approve(item: &MergeRequest) -> Result<(), String> {
    cli().send(&format!("{}/reviews", base(item)), "POST", &json!({ "event": "APPROVE" })).map(drop)
}

/// GitHub has no "revoke approval"; the closest is dismissing my approving review, which the
/// repository has to allow me (maintainers and admins can).
pub fn unapprove(item: &MergeRequest) -> Result<(), String> {
    let me: GhUser = cli().api_json("user")?;
    let reviews: Vec<GhReview> = paged_rest(&cli(), &format!("{}/reviews", base(item)), 5)?;
    let mine = reviews
        .iter()
        .rev()
        .find(|r| r.state == "APPROVED" && r.user.as_ref().is_some_and(|u| u.login.eq_ignore_ascii_case(&me.login)))
        .ok_or("There's no approval of yours to revoke.")?;
    cli().send(&format!("{}/reviews/{}/dismissals", base(item), mine.id), "PUT", &json!({ "message": "Approval revoked.", "event": "DISMISS" })).map(drop)
}

/// Merges it the way the repository merges for me (a merge commit when that's unknown).
pub fn merge(item: &MergeRequest) -> Result<(), String> {
    let method = item.merge_method.as_deref().unwrap_or("merge");
    cli().send(&format!("{}/merge", base(item)), "PUT", &json!({ "merge_method": method })).map(drop)
}

/// Turns auto-merge on or off: GitHub merges it once its requirements are met.
pub fn set_auto_merge(item: &MergeRequest, on: bool) -> Result<(), String> {
    let query = if on {
        let method = item.merge_method.as_deref().unwrap_or("merge").to_uppercase();
        format!("mutation($id: ID!) {{ enablePullRequestAutoMerge(input: {{ pullRequestId: $id, mergeMethod: {method} }}) {{ clientMutationId }} }}")
    } else {
        "mutation($id: ID!) { disablePullRequestAutoMerge(input: { pullRequestId: $id }) { clientMutationId } }".to_string()
    };
    cli().graphql::<Value>(&query, &[("id", Some(item.id.clone()))], &[]).map(drop)
}

pub fn mark_ready(item: &MergeRequest) -> Result<(), String> {
    let query = "mutation($id: ID!) { markPullRequestReadyForReview(input: { pullRequestId: $id }) { clientMutationId } }";
    cli().graphql::<Value>(query, &[("id", Some(item.id.clone()))], &[]).map(drop)
}

/// Points it at another branch: what a stacked pull request needs once the one below it merged.
pub fn retarget(item: &MergeRequest, branch: &str) -> Result<(), String> {
    cli().send(&base(item), "PATCH", &json!({ "base": branch })).map(drop)
}

pub fn comment(item: &MergeRequest, body: &str) -> Result<(), String> {
    cli().send(&format!("repos/{}/issues/{}/comments", item.project_path, item.iid), "POST", &json!({ "body": body })).map(drop)
}

pub fn comment_on_line(item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: &DiffRefs, body: &str) -> Result<(), String> {
    let (number, side) = side_of(line)?;
    cli()
        .send(
            &format!("{}/comments", base(item)),
            "POST",
            &json!({ "body": body, "commit_id": refs.head_sha, "path": file.new_path, "line": number, "side": side }),
        )
        .map(drop)
}

/// A line's number and side: a removed line lives on the left (old) side, everything else on the right.
fn side_of(line: &DiffLine) -> Result<(i64, &'static str), String> {
    let left = line.kind == LineKind::Removed;
    let number = if left { line.old_number } else { line.new_number }.ok_or("GitHub can't place a comment on that line.")?;
    Ok((number, if left { "LEFT" } else { "RIGHT" }))
}

pub fn reply(item: &MergeRequest, thread: &Discussion, body: &str) -> Result<(), String> {
    match (thread.id.strip_prefix("rc-"), thread.notes.first()) {
        (Some(_), Some(root)) => cli().send(&format!("{}/comments/{}/replies", base(item), root.id), "POST", &json!({ "body": body })).map(drop),
        // Conversation comments aren't threaded on GitHub; a reply is a new comment.
        _ => comment(item, body),
    }
}

pub fn set_resolved(thread: &Discussion, resolved: bool) -> Result<(), String> {
    let id = thread.node_id.clone().ok_or("GitHub didn't say which thread this is. Refresh, then try again.")?;
    let query = if resolved {
        "mutation($id: ID!) { resolveReviewThread(input: { threadId: $id }) { clientMutationId } }"
    } else {
        "mutation($id: ID!) { unresolveReviewThread(input: { threadId: $id }) { clientMutationId } }"
    };
    cli().graphql::<Value>(query, &[("id", Some(id))], &[]).map(drop)
}

// MARK: Reviews
//
// A review in progress is GitHub's one PENDING review per person per pull request: comments go
// into it, and submitting it posts them all with the verdict.

/// The node id of my pending review on it, if I have one.
fn pending_review(item: &MergeRequest) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct Payload {
        node: Option<PullT>,
    }
    #[derive(Deserialize)]
    struct PullT {
        reviews: Option<Nodes<ReviewT>>,
    }
    #[derive(Deserialize)]
    struct ReviewT {
        id: String,
    }
    // Only its author sees a pending review, so the first is mine.
    let query = "query($id: ID!) { node(id: $id) { ... on PullRequest { reviews(states: PENDING, first: 1) { nodes { id } } } } }";
    let payload: Payload = cli().graphql(query, &[("id", Some(item.id.clone()))], &[])?;
    Ok(payload.node.and_then(|p| p.reviews).and_then(|r| r.nodes.into_iter().next()).map(|r| r.id))
}

/// Adds a thread on a line to my pending review, starting one if there's none.
pub fn draft_on_line(item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: &DiffRefs, body: &str) -> Result<(), String> {
    let (number, side) = side_of(line)?;
    let vars = [("path", Some(file.new_path.clone())), ("side", Some(side.to_string())), ("body", Some(body.to_string()))];
    match pending_review(item)? {
        Some(review) => {
            let query = "mutation($review: ID!, $path: String!, $line: Int!, $side: DiffSide!, $body: String!) {
                addPullRequestReviewThread(input: { pullRequestReviewId: $review, path: $path, line: $line, side: $side, body: $body }) { clientMutationId }
            }";
            let mut all = vars.to_vec();
            all.push(("review", Some(review)));
            cli().graphql::<Value>(query, &all, &[("line", number)]).map(drop)
        }
        None => {
            let query = "mutation($pull: ID!, $commit: GitObjectID!, $path: String!, $line: Int!, $side: DiffSide!, $body: String!) {
                addPullRequestReview(input: { pullRequestId: $pull, commitOID: $commit, threads: [{ path: $path, line: $line, side: $side, body: $body }] }) { clientMutationId }
            }";
            let mut all = vars.to_vec();
            all.push(("pull", Some(item.id.clone())));
            all.push(("commit", Some(refs.head_sha.clone())));
            cli().graphql::<Value>(query, &all, &[("line", number)]).map(drop)
        }
    }
}

pub fn edit_draft(note: &Note, body: &str) -> Result<(), String> {
    let id = note.node_id.clone().ok_or("GitHub didn't say which comment this is. Refresh, then try again.")?;
    let query =
        "mutation($id: ID!, $body: String!) { updatePullRequestReviewComment(input: { pullRequestReviewCommentId: $id, body: $body }) { clientMutationId } }";
    cli().graphql::<Value>(query, &[("id", Some(id)), ("body", Some(body.to_string()))], &[]).map(drop)
}

pub fn delete_draft(note: &Note) -> Result<(), String> {
    let id = note.node_id.clone().ok_or("GitHub didn't say which comment this is. Refresh, then try again.")?;
    let query = "mutation($id: ID!) { deletePullRequestReviewComment(input: { id: $id }) { clientMutationId } }";
    cli().graphql::<Value>(query, &[("id", Some(id))], &[]).map(drop)
}

/// Submits my pending review with the verdict, or with none pending, posts a review that's
/// only the verdict and the summary.
pub fn submit_review(item: &MergeRequest, verdict: Verdict, body: &str) -> Result<(), String> {
    let body = body.trim();
    match pending_review(item)? {
        Some(review) => {
            let query = "mutation($review: ID!, $event: PullRequestReviewEvent!, $body: String) {
                submitPullRequestReview(input: { pullRequestReviewId: $review, event: $event, body: $body }) { clientMutationId }
            }";
            let body = (!body.is_empty()).then(|| body.to_string());
            cli().graphql::<Value>(query, &[("review", Some(review)), ("event", Some(verdict.event().into())), ("body", body)], &[]).map(drop)
        }
        None => {
            let mut review = json!({ "event": verdict.event() });
            if !body.is_empty() {
                review["body"] = json!(body);
            }
            cli().send(&format!("{}/reviews", base(item)), "POST", &review).map(drop)
        }
    }
}

// MARK: Files

/// A file's bytes at a commit, as they are: the raw media type skips the JSON and its base64.
/// The commit can be on a fork's branch: the base repository keeps each pull request's head.
pub fn file_at(item: &MergeRequest, path: &str, sha: &str) -> Result<Vec<u8>, String> {
    let path = super::forge::encode_path(path, true);
    let args: Vec<String> =
        ["-H", "Accept: application/vnd.github.raw", &format!("repos/{}/contents/{path}?ref={sha}", item.project_path)].into_iter().map(String::from).collect();
    cli().api(&args, None)
}

// MARK: Managing my own

const PEOPLE: &str = "query($owner: String!, $name: String!, $query: String) {
      repository(owner: $owner, name: $name) {
        assignableUsers(query: $query, first: 100) { nodes { login name avatarUrl } }
      }
    }";

#[derive(Deserialize)]
struct PeoplePayload {
    repository: Option<PeopleRepo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeopleRepo {
    assignable_users: Nodes<Option<Actor>>,
}

/// The people who can be asked to review: the repository's assignable users, matching `query`.
pub fn find_people(item: &MergeRequest, query: &str) -> Result<Vec<Person>, String> {
    let (owner, name) = item.project_path.split_once('/').ok_or("GitHub didn't say whose repository this is.")?;
    let query = query.trim();
    let vars = [("owner", Some(owner.to_string())), ("name", Some(name.to_string())), ("query", (!query.is_empty()).then(|| query.to_string()))];
    let payload: PeoplePayload = cli().graphql(PEOPLE, &vars, &[])?;
    Ok(people_of(payload))
}

fn people_of(payload: PeoplePayload) -> Vec<Person> {
    let users = payload.repository.map(|r| r.assignable_users.nodes).unwrap_or_default();
    users.into_iter().flatten().map(|a| person(Some(&a))).collect()
}

/// A recorded `assignableUsers` answer (its `data`), for the self-tests.
#[cfg(test)]
pub fn people_from_json(json: &str) -> Result<Vec<Person>, String> {
    let payload: PeoplePayload = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(people_of(payload))
}

fn requested_reviewers(item: &MergeRequest, method: &str, login: &str) -> Result<(), String> {
    cli().send(&format!("{}/requested_reviewers", base(item)), method, &json!({ "reviewers": [login] })).map(drop)
}

pub fn add_reviewer(item: &MergeRequest, login: &str) -> Result<(), String> {
    requested_reviewers(item, "POST", login)
}

/// Withdraws a pending request. Someone who already reviewed stays listed whatever is sent, so
/// that's said rather than seeming to work.
pub fn remove_reviewer(item: &MergeRequest, reviewer: &Reviewer) -> Result<(), String> {
    if reviewer.state != ReviewState::Unreviewed {
        return Err(format!("{} already reviewed it, and GitHub keeps everyone who reviewed on the list.", reviewer.person.name));
    }
    requested_reviewers(item, "DELETE", &reviewer.person.username)
}

/// Requests another review. Asking someone whose request is still pending changes nothing on
/// GitHub, so that request is withdrawn and made again, which notifies them.
pub fn ask_again(item: &MergeRequest, reviewer: &Reviewer) -> Result<(), String> {
    if reviewer.state == ReviewState::Unreviewed {
        requested_reviewers(item, "DELETE", &reviewer.person.username)?;
    }
    requested_reviewers(item, "POST", &reviewer.person.username)
}

/// Merges the base branch into the head. GitHub does it in the background; a short wait lets
/// the refresh after it see the new commit more often than not.
pub fn update_branch(item: &MergeRequest) -> Result<(), String> {
    cli().send(&format!("{}/update-branch", base(item)), "PUT", &json!({}))?;
    std::thread::sleep(std::time::Duration::from_secs(3));
    Ok(())
}

pub fn set_open(item: &MergeRequest, open: bool) -> Result<(), String> {
    cli().send(&base(item), "PATCH", &json!({ "state": if open { "open" } else { "closed" } })).map(drop)
}

pub fn edit(item: &MergeRequest, title: Option<&str>, description: Option<&str>) -> Result<(), String> {
    let mut body = json!({});
    if let Some(title) = title {
        body["title"] = json!(title);
    }
    if let Some(description) = description {
        body["body"] = json!(description);
    }
    cli().send(&base(item), "PATCH", &body).map(drop)
}

// MARK: My comments
//
// A comment's kind is in its thread's id, as `threads_of` names them: `ic-` a conversation
// comment, `rc-` a thread of comments on lines, `rv-` a review's summary. Each kind is its own
// REST resource, addressed by the database id every note already carries.

/// What `threads_of` puts in front of a review's summary to say its verdict.
const APPROVED: &str = "**Approved**\n\n";
const CHANGES_REQUESTED: &str = "**Requested changes**\n\n";

/// Where a posted comment lives, and the method that edits it.
pub fn note_endpoint(item: &MergeRequest, thread: &Discussion, note: &Note) -> Result<(String, &'static str), String> {
    let repo = format!("repos/{}", item.project_path);
    match thread.id.split('-').next().unwrap_or_default() {
        "ic" => Ok((format!("{repo}/issues/comments/{}", note.id), "PATCH")),
        "rc" => Ok((format!("{repo}/pulls/comments/{}", note.id), "PATCH")),
        "rv" => Ok((format!("{}/reviews/{}", base(item), note.id), "PUT")),
        _ => Err("GitHub didn't say what kind of comment this is. Refresh, then try again.".into()),
    }
}

pub fn edit_note(item: &MergeRequest, thread: &Discussion, note: &Note, body: &str) -> Result<(), String> {
    let (path, method) = note_endpoint(item, thread, note)?;
    cli().send(&path, method, &json!({ "body": body })).map(drop)
}

pub fn delete_note(item: &MergeRequest, thread: &Discussion, note: &Note) -> Result<(), String> {
    if !deletes_note(thread) {
        return Err("GitHub can't delete a review's summary. Edit it instead.".into());
    }
    let (path, _) = note_endpoint(item, thread, note)?;
    cli().api(&["-X".into(), "DELETE".into(), path], None).map(drop)
}

/// A review's summary stays: GitHub only lets it be edited.
pub fn deletes_note(thread: &Discussion) -> bool {
    !thread.id.starts_with("rv-")
}

/// The verdict in front of a review's summary, and the summary I wrote.
pub fn written(thread: &Discussion, note: &Note) -> (String, String) {
    if thread.id.starts_with("rv-") {
        for verdict in [APPROVED, CHANGES_REQUESTED] {
            if let Some(rest) = note.body.strip_prefix(verdict) {
                return (verdict.to_string(), rest.to_string());
            }
        }
    }
    (String::new(), note.body.clone())
}

// MARK: Checks

#[derive(Deserialize)]
struct CheckRuns {
    check_runs: Vec<CheckRun>,
}

#[derive(Deserialize)]
struct CheckRun {
    id: i64,
    name: String,
    status: String,
    conclusion: Option<String>,
    html_url: Option<String>,
    details_url: Option<String>,
    started_at: Option<Date>,
    completed_at: Option<Date>,
    app: Option<CheckApp>,
}

#[derive(Deserialize)]
struct CheckApp {
    slug: Option<String>,
    name: Option<String>,
}

/// The check runs on the pull request's head commit, grouped by the app that ran them.
pub fn jobs(item: &MergeRequest) -> Result<Vec<Job>, String> {
    let head = refs(item)?.head_sha;
    let mut runs = Vec::new();
    for page in 1..=5 {
        let path = format!("repos/{}/commits/{head}/check-runs?per_page=100&page={page}", item.project_path);
        let batch: CheckRuns = cli().api_json(&path)?;
        let n = batch.check_runs.len();
        runs.extend(batch.check_runs);
        if n < 100 {
            break;
        }
    }
    Ok(check_jobs(runs))
}

/// A recorded `check-runs` response, for the self-tests.
#[cfg(test)]
pub fn jobs_from_json(raw: &str) -> Result<Vec<Job>, String> {
    let batch: CheckRuns = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    Ok(check_jobs(batch.check_runs))
}

/// Only GitHub Actions' runs can be re-run or read from here; another app's link out. An Actions
/// check run's id is its job's id.
fn check_jobs(mut runs: Vec<CheckRun>) -> Vec<Job> {
    runs.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.id.cmp(&b.id)));
    let jobs = runs
        .into_iter()
        .map(|r| {
            let actions = r.app.as_ref().and_then(|a| a.slug.as_deref()) == Some("github-actions");
            let status = PipelineStatus::from_check_run(&r.status, r.conclusion.as_deref());
            Job {
                id: r.id,
                name: r.name,
                stage: r.app.and_then(|a| a.name).unwrap_or_else(|| "Checks".into()),
                retryable: actions && r.status == "completed",
                has_log: actions && r.status == "completed",
                status,
                web_url: r.html_url.or(r.details_url).unwrap_or_default(),
                started_at: r.started_at,
                finished_at: r.completed_at,
                allow_failure: false,
                project: None,
            }
        })
        .collect();
    by_stage(jobs)
}

pub fn retry_job(item: &MergeRequest, job: &Job) -> Result<(), String> {
    if !job.retryable {
        return Err("Only GitHub Actions jobs can be re-run from here.".into());
    }
    cli().send(&format!("repos/{}/actions/jobs/{}/rerun", item.project_path, job.id), "POST", &json!({})).map(drop)
}

/// The job's log: GitHub answers with a redirect to the text, which `gh api` follows.
pub fn job_log(item: &MergeRequest, job: &Job) -> Result<Vec<u8>, String> {
    cli().api(&[format!("repos/{}/actions/jobs/{}/logs", item.project_path, job.id)], None)
}
