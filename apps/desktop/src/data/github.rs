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
          reviewThreads(first: 100) { nodes { isResolved comments(first: 1) { nodes { databaseId } } } }
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
    let mut lines = lines.join().unwrap()?;
    let summaries = reviews.join().unwrap()?;
    let resolved = resolution.join().unwrap();

    let mut threads: Vec<Discussion> =
        comments.iter().map(|c| Discussion { id: format!("ic-{}", c.id), individual_note: true, notes: vec![note(c, None)] }).collect();
    for review in &summaries {
        let body = review.body.as_deref().unwrap_or("").trim();
        let Some(at) = review.submitted_at else { continue };
        if body.is_empty() {
            continue;
        }
        let verdict = match review.state.as_str() {
            "APPROVED" => "**Approved**\n\n",
            "CHANGES_REQUESTED" => "**Requested changes**\n\n",
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
            }],
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
        let first = notes[0];
        let left = first.side.as_deref() == Some("LEFT");
        let position = Position {
            new_path: first.path.clone(),
            old_path: first.path.clone(),
            new_line: if first.line.is_some() { if left { None } else { first.line } } else { first.original_line },
            old_line: if first.line.is_some() && left { first.line } else { None },
            // A thread whose line is gone (outdated) is pinned to the commit it was written on,
            // which isn't the head any more, so it isn't placed in the diff.
            head_sha: if first.line.is_none() { Some(first.original_commit_id.clone().unwrap_or("outdated".into())) } else { None },
        };
        let is_resolved = resolved.as_ref().and_then(|r| r.get(&root).copied());
        threads.push(Discussion {
            id: format!("rc-{root}"),
            individual_note: false,
            notes: notes.iter().map(|c| Note { resolvable: Some(is_resolved.is_some()), resolved: is_resolved, ..note(c, Some(position.clone())) }).collect(),
        });
    }
    threads.sort_by_key(|t| t.notes.first().map(|n| n.created_at));
    Ok(threads)
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
    }
}

/// Whether each line thread is resolved, by its first comment's ID. REST doesn't say, so it's
/// one GraphQL query; without it threads just show no resolved state.
fn resolved_threads(item: &MergeRequest) -> Option<HashMap<i64, bool>> {
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
    Some(threads.into_iter().filter_map(|t| Some((t.comments.nodes.first()?.database_id?, t.is_resolved))).collect())
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
    // A removed line lives on the left (old) side, everything else on the right.
    let left = line.kind == LineKind::Removed;
    let number = if left { line.old_number } else { line.new_number }.ok_or("GitHub can't place a comment on that line.")?;
    cli()
        .send(
            &format!("{}/comments", base(item)),
            "POST",
            &json!({ "body": body, "commit_id": refs.head_sha, "path": file.new_path, "line": number, "side": if left { "LEFT" } else { "RIGHT" } }),
        )
        .map(drop)
}

pub fn reply(item: &MergeRequest, thread: &Discussion, body: &str) -> Result<(), String> {
    match (thread.id.strip_prefix("rc-"), thread.notes.first()) {
        (Some(_), Some(root)) => cli().send(&format!("{}/comments/{}/replies", base(item), root.id), "POST", &json!({ "body": body })).map(drop),
        // Conversation comments aren't threaded on GitHub; a reply is a new comment.
        _ => comment(item, body),
    }
}
