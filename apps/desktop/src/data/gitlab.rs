//! GitLab through `glab`: one GraphQL query for the queue, REST for diffs, threads and actions.

use serde::Deserialize;
use serde_json::json;

use super::cli::{Cli, paged_rest, parallel};
use super::diff::{DiffLine, LineKind};
use super::forge::{Forge, QueueResult};
use super::models::*;

const MR_FIELDS: &str = "
    id iid title webUrl createdAt updatedAt mergedAt closedAt state draft
    sourceBranch targetBranch detailedMergeStatus userNotesCount description
    project { fullPath name }
    author { username name avatarUrl }
    reviewers { nodes { username name avatarUrl mergeRequestInteraction { reviewState approved } } }
    approvedBy { nodes { username } }
    diffStatsSummary { additions deletions fileCount }
    headPipeline { status }
    labels { nodes { title color textColor } }
    notes(filter: ONLY_ACTIVITY, last: 100) {
      pageInfo { hasPreviousPage startCursor }
      nodes { body createdAt author { username name avatarUrl } }
    }";

/// What only matters on my own merge requests: how many threads are open, and which job failed.
const MINE_FIELDS: &str = "
    resolvableDiscussionsCount resolvedDiscussionsCount autoMergeEnabled
    headPipeline { jobs(statuses: FAILED, first: 3) { nodes { name } } }";

/// The queue's query: the merge requests I'm asked to review, or (`mine`) the ones I wrote.
fn queue_query(mine: bool) -> String {
    let (list, extra) = if mine { ("authoredMergeRequests", MINE_FIELDS) } else { ("reviewRequestedMergeRequests", "") };
    format!(
        "query($state: MergeRequestState!, $after: String, $updatedAfter: Time) {{
      currentUser {{
        username name
        list: {list}(
          state: $state, first: 40, after: $after, updatedAfter: $updatedAfter, sort: UPDATED_DESC
        ) {{
          pageInfo {{ hasNextPage endCursor }}
          nodes {{ {MR_FIELDS} {extra} }}
        }}
      }}
    }}"
    )
}

/// Older activity for one merge request, when its newest 100 notes didn't include my request.
const MORE_NOTES: &str = "query($path: ID!, $iid: String!, $before: String) {
      project(fullPath: $path) {
        mergeRequest(iid: $iid) {
          notes(filter: ONLY_ACTIVITY, last: 100, before: $before) {
            pageInfo { hasPreviousPage startCursor }
            nodes { body createdAt author { username name avatarUrl } }
          }
        }
      }
    }";

// MARK: GraphQL payloads

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: Option<bool>,
    end_cursor: Option<String>,
    has_previous_page: Option<bool>,
    start_cursor: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    page_info: Option<PageInfo>,
    nodes: Vec<T>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Author {
    username: String,
    name: String,
    avatar_url: Option<String>,
}

impl Author {
    fn person(&self) -> Person {
        Person { username: self.username.clone(), name: self.name.clone(), avatar_url: self.avatar_url.clone() }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityNote {
    pub body: String,
    pub created_at: Date,
    author: Option<Author>,
}

impl ActivityNote {
    #[cfg(test)]
    pub fn new(body: &str, created_at: Date) -> Self {
        ActivityNote { body: body.into(), created_at, author: None }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuePayload {
    current_user: Option<QueueUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueUser {
    username: String,
    name: String,
    list: Connection<MrNode>,
}

#[derive(Deserialize)]
struct NotesPayload {
    project: Option<NotesProject>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotesProject {
    merge_request: Option<NotesMr>,
}
#[derive(Deserialize)]
struct NotesMr {
    notes: Connection<ActivityNote>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MrNode {
    id: String,
    iid: String,
    title: String,
    web_url: String,
    created_at: Date,
    updated_at: Date,
    merged_at: Option<Date>,
    closed_at: Option<Date>,
    state: String,
    draft: bool,
    source_branch: String,
    target_branch: String,
    detailed_merge_status: Option<String>,
    user_notes_count: Option<i64>,
    description: Option<String>,
    project: ProjectNode,
    author: Option<Author>,
    reviewers: Connection<ReviewerNode>,
    approved_by: Option<Connection<Username>>,
    diff_stats_summary: Option<DiffStats>,
    head_pipeline: Option<Pipeline>,
    labels: Option<Connection<LabelNode>>,
    notes: Connection<ActivityNote>,
    resolvable_discussions_count: Option<i64>,
    resolved_discussions_count: Option<i64>,
    auto_merge_enabled: Option<bool>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectNode {
    full_path: String,
    name: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ReviewerNode {
    username: String,
    name: String,
    avatar_url: Option<String>,
    merge_request_interaction: Option<Interaction>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Interaction {
    review_state: Option<String>,
    approved: Option<bool>,
}

#[derive(Deserialize, Clone)]
struct Username {
    username: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DiffStats {
    additions: i64,
    deletions: i64,
    file_count: i64,
}

#[derive(Deserialize, Clone)]
struct Pipeline {
    status: String,
    jobs: Option<Connection<JobNode>>,
}

#[derive(Deserialize, Clone)]
struct JobNode {
    name: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LabelNode {
    title: String,
    color: String,
    text_color: String,
}

fn cli() -> Cli {
    Cli::glab()
}

// MARK: Queue

pub fn queue(done_days: i64) -> Result<QueueResult, String> {
    let since = chrono::Utc::now() - chrono::Duration::days(done_days);
    let mut pages = parallel(vec![
        Box::new(|| fetch("opened", None, 5, false)),
        Box::new(move || fetch("merged", Some(since), 2, false)),
        Box::new(move || fetch("closed", Some(since), 1, false)),
        Box::new(|| fetch("opened", None, 3, true)),
    ]);
    let authored = pages.pop().unwrap();
    let closed = pages.pop().unwrap()?;
    let merged = pages.pop().unwrap()?;
    let opened = pages.pop().unwrap()?;
    let me = opened.0.ok_or("glab isn't signed in.")?;

    let all: Vec<MrNode> = opened.1.iter().chain(&merged.1).chain(&closed.1).cloned().collect();
    let extra = fetch_missing_request_notes(&all, &me.username);
    let build_all =
        |nodes: &[MrNode]| -> Vec<MergeRequest> { nodes.iter().map(|n| build(n, &me.username, extra.get(&n.id).map(Vec::as_slice).unwrap_or(&[]))).collect() };
    let open = build_all(&opened.1);
    let mut done = build_all(&merged.1);
    done.extend(build_all(&closed.1));
    // My own are a second ask: if it fails, the reviews above still show.
    let (mine, mine_error) = match authored {
        Ok(found) => (found.1.iter().map(|n| build(n, &me.username, &[])).collect(), None),
        Err(e) => (vec![], Some(e)),
    };
    Ok(QueueResult { me, open, done, mine, mine_error })
}

fn fetch(state: &str, since: Option<Date>, pages: usize, mine: bool) -> Result<(Option<Person>, Vec<MrNode>), String> {
    let mut user = None;
    let mut nodes = Vec::new();
    let mut cursor: Option<String> = None;
    let query = queue_query(mine);
    for _ in 0..pages {
        let payload: QueuePayload = cli().graphql(
            &query,
            &[
                ("state", Some(state.to_string())),
                ("after", cursor.clone()),
                ("updatedAfter", since.map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))),
            ],
            &[],
        )?;
        let Some(u) = payload.current_user else { break };
        user = Some(Person::new(&u.username, &u.name));
        let conn = u.list;
        nodes.extend(conn.nodes);
        let info = conn.page_info.unwrap_or_default();
        match (info.has_next_page, info.end_cursor) {
            (Some(true), Some(next)) => cursor = Some(next),
            _ => break,
        }
    }
    Ok((user, nodes))
}

/// Busy merge requests can have more than 100 activity notes, and only the newest 100 come
/// with the queue; page back through older ones until the review request turns up.
fn fetch_missing_request_notes(nodes: &[MrNode], me: &str) -> std::collections::HashMap<String, Vec<ActivityNote>> {
    let missing: Vec<MrNode> = nodes
        .iter()
        .filter(|n| request_notes(&n.notes.nodes, me).is_empty() && n.notes.page_info.as_ref().and_then(|p| p.has_previous_page) == Some(true))
        .take(12)
        .cloned()
        .collect();
    let jobs: Vec<Box<dyn FnOnce() -> (String, Vec<ActivityNote>) + Send>> = missing
        .into_iter()
        .map(|node| {
            Box::new(move || {
                let mut notes = Vec::new();
                let mut cursor = node.notes.page_info.as_ref().and_then(|p| p.start_cursor.clone());
                for _ in 0..5 {
                    let Some(before) = cursor.take() else { break };
                    let Ok(payload) = cli().graphql::<NotesPayload>(
                        MORE_NOTES,
                        &[("path", Some(node.project.full_path.clone())), ("iid", Some(node.iid.clone())), ("before", Some(before))],
                        &[],
                    ) else {
                        break;
                    };
                    let Some(conn) = payload.project.and_then(|p| p.merge_request).map(|m| m.notes) else { break };
                    notes.extend(conn.nodes);
                    let info = conn.page_info.unwrap_or_default();
                    cursor = if info.has_previous_page == Some(true) { info.start_cursor } else { None };
                }
                (node.id.clone(), notes)
            }) as Box<dyn FnOnce() -> _ + Send>
        })
        .collect();
    parallel(jobs).into_iter().collect()
}

/// Notes like "requested review from @a and @me", but not "removed review request for @me".
pub fn request_notes<'a>(notes: &'a [ActivityNote], me: &str) -> Vec<&'a ActivityNote> {
    notes
        .iter()
        .filter(|note| {
            let Some(start) = note.body.find("requested review from") else { return false };
            let mut tail = &note.body[start + "requested review from".len()..];
            if let Some(removed) = tail.find("removed review request") {
                tail = &tail[..removed];
            }
            mentions(tail, me)
        })
        .collect()
}

fn mentions(text: &str, username: &str) -> bool {
    let handle = format!("@{}", username.to_lowercase());
    let lower = text.to_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(&handle) {
        let end = from + at + handle.len();
        // "@zain42" shouldn't match "@zain42bot".
        let mut next = lower[end..].chars();
        let first = next.clone().next();
        // A "." only continues the handle if more of it follows ("@alex.smith", not "@alex.").
        let continues = first.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
            || (first == Some('.') && {
                next.next();
                next.next().is_some_and(char::is_alphanumeric)
            });
        if !continues {
            return true;
        }
        from = end;
    }
    false
}

fn build(node: &MrNode, me: &str, extra: &[ActivityNote]) -> MergeRequest {
    let all: Vec<ActivityNote> = node.notes.nodes.iter().chain(extra).cloned().collect();
    let mut requests = request_notes(&all, me);
    requests.sort_by_key(|n| n.created_at);
    let reviewers: Vec<Reviewer> = node
        .reviewers
        .nodes
        .iter()
        .map(|r| Reviewer {
            person: Person { username: r.username.clone(), name: r.name.clone(), avatar_url: r.avatar_url.clone() },
            state: r.merge_request_interaction.as_ref().and_then(|i| i.review_state.as_deref()).map_or(ReviewState::Unknown, ReviewState::from_gitlab),
            approved: r.merge_request_interaction.as_ref().and_then(|i| i.approved).unwrap_or(false),
        })
        .collect();
    let mine = reviewers.iter().find(|r| r.person.username.eq_ignore_ascii_case(me));
    MergeRequest {
        id: node.id.clone(),
        iid: node.iid.clone(),
        title: node.title.clone(),
        web_url: node.web_url.clone(),
        project_path: node.project.full_path.clone(),
        project_name: node.project.name.clone(),
        author: node.author.as_ref().map_or_else(|| Person::new("ghost", "Unknown"), Author::person),
        created_at: node.created_at,
        updated_at: node.updated_at,
        merged_at: node.merged_at,
        closed_at: node.closed_at,
        state: node.state.clone(),
        draft: node.draft,
        source_branch: node.source_branch.clone(),
        target_branch: node.target_branch.clone(),
        merge_status: node.detailed_merge_status.clone(),
        comment_count: node.user_notes_count.unwrap_or(0),
        description: node.description.clone().unwrap_or_default(),
        approved_by: node.approved_by.as_ref().map(|c| c.nodes.iter().map(|u| u.username.clone()).collect()).unwrap_or_default(),
        additions: node.diff_stats_summary.as_ref().map_or(0, |d| d.additions),
        deletions: node.diff_stats_summary.as_ref().map_or(0, |d| d.deletions),
        file_count: node.diff_stats_summary.as_ref().map_or(0, |d| d.file_count),
        pipeline: node.head_pipeline.as_ref().and_then(|p| PipelineStatus::from_gitlab(&p.status)),
        labels: node
            .labels
            .as_ref()
            .map(|c| c.nodes.iter().map(|l| Label { title: l.title.clone(), color: l.color.clone(), text_color: l.text_color.clone() }).collect())
            .unwrap_or_default(),
        requested_at: requests.last().map_or(node.created_at, |n| n.created_at),
        first_requested_at: requests.first().map_or(node.created_at, |n| n.created_at),
        requested_by: requests.last().and_then(|n| n.author.as_ref()).map(Author::person),
        requested_at_is_guess: requests.is_empty(),
        my_state: mine.map_or(ReviewState::Unknown, |r| r.state),
        reviewers,
        forge: Forge::Gitlab,
        auto_merge: node.auto_merge_enabled.unwrap_or(false),
        merge_method: None,
        unresolved_threads: node.resolvable_discussions_count.map(|all| (all - node.resolved_discussions_count.unwrap_or(0)).max(0)),
        failed_job: node.head_pipeline.as_ref().and_then(|p| p.jobs.as_ref()).and_then(|j| j.nodes.iter().find_map(|n| n.name.clone())),
    }
}

// MARK: Details

fn base(item: &MergeRequest) -> String {
    format!("projects/{}/merge_requests/{}", item.project_id(), item.iid)
}

#[derive(Deserialize)]
struct RestDiff {
    old_path: String,
    new_path: String,
    diff: String,
    new_file: bool,
    renamed_file: bool,
    deleted_file: bool,
    too_large: Option<bool>,
    collapsed: Option<bool>,
    generated_file: Option<bool>,
}

/// The diffs come from two endpoints, so the commits are read on both sides of the diffs: if a
/// push landed in between, try again, and failing that, leave the commits out (which turns off
/// comments on lines until a reload).
pub fn diffs(item: &MergeRequest) -> Result<(Vec<FileDiff>, Option<DiffRefs>), String> {
    for _ in 0..2 {
        let before = diff_refs(item).ok().flatten();
        let files = file_diffs(item)?;
        let after = diff_refs(item).ok().flatten();
        if before == after {
            return Ok((files, after));
        }
    }
    Ok((file_diffs(item)?, None))
}

fn file_diffs(item: &MergeRequest) -> Result<Vec<FileDiff>, String> {
    let files: Vec<RestDiff> = paged_rest(&cli(), &format!("{}/diffs", base(item)), 10)?;
    Ok(files.into_iter().map(file_diff).collect())
}

/// A recorded `/diffs` response (all pages in one array), for the benchmark.
pub fn file_diffs_from_json(raw: &[u8]) -> Result<Vec<FileDiff>, String> {
    let files: Vec<RestDiff> = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    Ok(files.into_iter().map(file_diff).collect())
}

fn file_diff(f: RestDiff) -> FileDiff {
    FileDiff {
        old_path: f.old_path,
        new_path: f.new_path,
        diff: f.diff,
        new_file: f.new_file,
        renamed_file: f.renamed_file,
        deleted_file: f.deleted_file,
        too_large: f.too_large.unwrap_or(false),
        collapsed: f.collapsed.unwrap_or(false),
        generated_file: f.generated_file.unwrap_or(false),
        lines: Vec::new(),
        added: 0,
        removed: 0,
        digest: 0,
    }
    .prepared()
}

fn diff_refs(item: &MergeRequest) -> Result<Option<DiffRefs>, String> {
    #[derive(Deserialize)]
    struct Payload {
        diff_refs: Option<DiffRefs>,
    }
    Ok(cli().api_json::<Payload>(&base(item))?.diff_refs)
}

#[derive(Deserialize)]
struct RestDiscussion {
    id: String,
    individual_note: bool,
    notes: Vec<RestNote>,
}

#[derive(Deserialize)]
struct RestNote {
    id: i64,
    body: String,
    author: RestAuthor,
    created_at: Date,
    system: bool,
    resolvable: Option<bool>,
    resolved: Option<bool>,
    position: Option<Position>,
}

#[derive(Deserialize)]
struct RestAuthor {
    username: String,
    name: String,
    avatar_url: Option<String>,
}

pub fn discussions(item: &MergeRequest) -> Result<Vec<Discussion>, String> {
    let threads: Vec<RestDiscussion> = paged_rest(&cli(), &format!("{}/discussions", base(item)), 10)?;
    Ok(threads.into_iter().map(discussion).collect())
}

/// A recorded `/discussions` response, for the benchmark.
pub fn discussions_from_json(raw: &[u8]) -> Result<Vec<Discussion>, String> {
    let threads: Vec<RestDiscussion> = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    Ok(threads.into_iter().map(discussion).collect())
}

fn discussion(d: RestDiscussion) -> Discussion {
    Discussion {
        id: d.id,
        individual_note: d.individual_note,
        notes: d
            .notes
            .into_iter()
            .map(|n| Note {
                id: n.id,
                body: n.body,
                author: Person { username: n.author.username, name: n.author.name, avatar_url: n.author.avatar_url },
                created_at: n.created_at,
                system: n.system,
                resolvable: n.resolvable,
                resolved: n.resolved,
                position: n.position,
            })
            .collect(),
    }
}

// MARK: Actions

pub fn approve(item: &MergeRequest) -> Result<(), String> {
    cli().send(&format!("{}/approve", base(item)), "POST", &json!({})).map(drop)
}

pub fn unapprove(item: &MergeRequest) -> Result<(), String> {
    cli().send(&format!("{}/unapprove", base(item)), "POST", &json!({})).map(drop)
}

/// Merges it with the merge request's own settings (squash, removing the source branch).
pub fn merge(item: &MergeRequest) -> Result<(), String> {
    cli().send(&format!("{}/merge", base(item)), "PUT", &json!({})).map(drop)
}

/// Turns auto-merge on or off: GitLab merges it once its checks pass. Both spellings of the
/// option are sent; GitLab 17.11 renamed it and ignores the one it doesn't know.
pub fn set_auto_merge(item: &MergeRequest, on: bool) -> Result<(), String> {
    if on {
        cli().send(&format!("{}/merge", base(item)), "PUT", &json!({ "auto_merge": true, "merge_when_pipeline_succeeds": true })).map(drop)
    } else {
        cli().send(&format!("{}/cancel_merge_when_pipeline_succeeds", base(item)), "POST", &json!({})).map(drop)
    }
}

pub fn mark_ready(item: &MergeRequest) -> Result<(), String> {
    let query = "mutation($path: ID!, $iid: String!) { mergeRequestSetDraft(input: { projectPath: $path, iid: $iid, draft: false }) { errors } }";
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload {
        merge_request_set_draft: Option<Outcome>,
    }
    #[derive(Deserialize)]
    struct Outcome {
        errors: Vec<String>,
    }
    let payload: Payload = cli().graphql(query, &[("path", Some(item.project_path.clone())), ("iid", Some(item.iid.clone()))], &[])?;
    match payload.merge_request_set_draft.map(|o| o.errors) {
        Some(errors) if !errors.is_empty() => Err(errors.join("\n")),
        _ => Ok(()),
    }
}

/// Points it at another branch: what a stacked merge request needs once the one below it merged.
pub fn retarget(item: &MergeRequest, branch: &str) -> Result<(), String> {
    cli().send(&base(item), "PUT", &json!({ "target_branch": branch })).map(drop)
}

pub fn comment(item: &MergeRequest, body: &str) -> Result<(), String> {
    cli().send(&format!("{}/notes", base(item)), "POST", &json!({ "body": body })).map(drop)
}

pub fn comment_on_line(item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: &DiffRefs, body: &str) -> Result<(), String> {
    let mut position = json!({
        "position_type": "text", "base_sha": refs.base_sha, "start_sha": refs.start_sha, "head_sha": refs.head_sha,
        "old_path": file.old_path, "new_path": file.new_path,
    });
    // GitLab places a comment by the side it's on: new line for added, old for removed, both for context.
    if let (Some(new), true) = (line.new_number, line.kind != LineKind::Removed) {
        position["new_line"] = json!(new);
    }
    if let (Some(old), true) = (line.old_number, line.kind != LineKind::Added) {
        position["old_line"] = json!(old);
    }
    cli().send(&format!("{}/discussions", base(item)), "POST", &json!({ "body": body, "position": position })).map(drop)
}

pub fn reply(item: &MergeRequest, thread: &Discussion, body: &str) -> Result<(), String> {
    cli().send(&format!("{}/discussions/{}/notes", base(item), thread.id), "POST", &json!({ "body": body })).map(drop)
}
