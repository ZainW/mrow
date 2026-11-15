//! The shared models both forges map onto. `MergeRequest` stands for GitHub pull requests too;
//! `item.kind()` says which forge it came from.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::diff::DiffLine;
use super::forge::Forge;

pub type Date = DateTime<Utc>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Person {
    pub username: String,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

impl Person {
    pub fn new(username: &str, name: &str) -> Self {
        Person { username: username.into(), name: name.into(), avatar_url: None }
    }

    pub fn initials(&self) -> String {
        let letters: String = self.name.split(' ').filter(|p| !p.is_empty()).take(2).filter_map(|p| p.chars().next()).collect();
        if letters.is_empty() { self.username.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default() } else { letters.to_uppercase() }
    }
}

/// A reviewer's progress, as GitLab reports it per reviewer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReviewState {
    Unreviewed,
    ReviewStarted,
    Reviewed,
    RequestedChanges,
    Approved,
    /// Approval was reset, usually by new commits, so it needs another look.
    Unapproved,
    Unknown,
}

impl ReviewState {
    pub fn from_gitlab(raw: &str) -> Self {
        match raw {
            "UNREVIEWED" => Self::Unreviewed,
            "REVIEW_STARTED" => Self::ReviewStarted,
            "REVIEWED" => Self::Reviewed,
            "REQUESTED_CHANGES" => Self::RequestedChanges,
            "APPROVED" => Self::Approved,
            "UNAPPROVED" => Self::Unapproved,
            _ => Self::Unknown,
        }
    }

    /// Whether the ball is in the reviewer's court.
    pub fn needs_review(self) -> bool {
        matches!(self, Self::Unreviewed | Self::ReviewStarted | Self::Unapproved | Self::Unknown)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unreviewed => "Not reviewed",
            Self::ReviewStarted => "Reviewing",
            Self::Reviewed => "Commented",
            Self::RequestedChanges => "Requested changes",
            Self::Approved => "Approved",
            Self::Unapproved => "Approval reset",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Reviewer {
    pub person: Person,
    pub state: ReviewState,
    pub approved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Label {
    pub title: String,
    pub color: String,
    pub text_color: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineStatus {
    Success,
    Failed,
    Running,
    Pending,
    Canceled,
    Skipped,
    Manual,
}

impl PipelineStatus {
    pub fn from_gitlab(raw: &str) -> Option<Self> {
        Some(match raw {
            "SUCCESS" => Self::Success,
            "FAILED" => Self::Failed,
            "RUNNING" | "CANCELING" => Self::Running,
            "CANCELED" => Self::Canceled,
            "SKIPPED" => Self::Skipped,
            "MANUAL" => Self::Manual,
            "PENDING" | "CREATED" | "PREPARING" | "SCHEDULED" | "WAITING_FOR_RESOURCE" | "WAITING_FOR_CALLBACK" => Self::Pending,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "Passed",
            Self::Failed => "Failed",
            Self::Running => "Running",
            Self::Canceled => "Canceled",
            Self::Skipped => "Skipped",
            Self::Manual => "Manual",
            Self::Pending => "Pending",
        }
    }
}

/// One merge request (or GitHub pull request) I've been asked to review, with the moment I was asked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergeRequest {
    pub id: String,
    pub iid: String,
    pub title: String,
    pub web_url: String,
    pub project_path: String,
    pub project_name: String,
    pub author: Person,
    pub created_at: Date,
    pub updated_at: Date,
    pub merged_at: Option<Date>,
    pub closed_at: Option<Date>,
    pub state: String,
    pub draft: bool,
    pub source_branch: String,
    pub target_branch: String,
    pub merge_status: Option<String>,
    pub comment_count: i64,
    pub description: String,
    pub reviewers: Vec<Reviewer>,
    pub approved_by: Vec<String>,
    pub additions: i64,
    pub deletions: i64,
    pub file_count: i64,
    pub pipeline: Option<PipelineStatus>,
    pub labels: Vec<Label>,
    /// The last time someone asked me to review it: a re-request moves it back to the top.
    pub requested_at: Date,
    /// The first time I was added, when that differs from the last request.
    pub first_requested_at: Date,
    pub requested_by: Option<Person>,
    /// No "requested review from" note was found, so `requested_at` is the creation date.
    pub requested_at_is_guess: bool,
    pub my_state: ReviewState,
    pub forge: Forge,
    /// Threads still open. Only asked for on my own requests.
    #[serde(default)]
    pub unresolved_threads: Option<i64>,
    /// The first failed job or check, by name. Only asked for on my own requests.
    #[serde(default)]
    pub failed_job: Option<String>,
    /// Set to merge by itself once everything passes. Only asked for on my own requests.
    #[serde(default)]
    pub auto_merge: bool,
    /// GitHub: how this repository merges for me ("merge", "squash" or "rebase").
    #[serde(default)]
    pub merge_method: Option<String>,
}

impl MergeRequest {
    pub fn kind(&self) -> Forge {
        self.forge
    }

    pub fn is_open(&self) -> bool {
        self.state == "opened"
    }

    pub fn was_re_requested(&self) -> bool {
        (self.requested_at - self.first_requested_at).num_seconds() > 60
    }

    /// "backend-service!5374" on GitLab, "web#42" on GitHub.
    pub fn short_reference(&self) -> String {
        format!("{}{}{}", self.project_name, self.forge.reference_mark(), self.iid)
    }

    pub fn full_reference(&self) -> String {
        format!("{}{}{}", self.project_path, self.forge.reference_mark(), self.iid)
    }

    /// "#42" or "!5374", for headings and prompts.
    pub fn reference(&self) -> String {
        format!("{}{}", self.forge.reference_mark(), self.iid)
    }

    /// "https://gitlab.com/group/project/" or "https://github.com/owner/repo/", for relative links.
    pub fn project_url(&self) -> Option<String> {
        let s = &self.web_url;
        let at = s.find("/-/merge_requests/").or_else(|| s.find("/pull/"))?;
        Some(format!("{}/", &s[..at]))
    }

    /// The project path escaped for REST URLs.
    pub fn project_id(&self) -> String {
        self.project_path.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
    }

    pub fn merge_status_label(&self) -> Option<&'static str> {
        Some(match self.merge_status.as_deref()? {
            "mergeable" => "Ready to merge",
            "conflict" => "Has conflicts",
            "need_rebase" => "Needs rebase",
            "not_approved" => "Awaiting approval",
            "discussions_not_resolved" => "Unresolved threads",
            "ci_must_pass" | "ci_still_running" => "Waiting on pipeline",
            "draft_status" => "Draft",
            "blocked_status" => "Blocked",
            _ => return None,
        })
    }

    pub fn finished_at(&self) -> Date {
        self.merged_at.or(self.closed_at).unwrap_or(self.updated_at)
    }
}

/// One file's changes, parsed once off the main thread so drawing never re-parses the diff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub diff: String,
    pub new_file: bool,
    pub renamed_file: bool,
    pub deleted_file: bool,
    pub too_large: bool,
    pub collapsed: bool,
    pub generated_file: bool,
    #[serde(skip)]
    pub lines: Vec<DiffLine>,
    #[serde(skip)]
    pub added: usize,
    #[serde(skip)]
    pub removed: usize,
    /// Tells two versions of the same file apart, for noticing a file changed after it was
    /// marked viewed. Stable across launches.
    #[serde(skip)]
    pub digest: i64,
}

impl FileDiff {
    pub fn id(&self) -> String {
        format!("{}→{}", self.old_path, self.new_path)
    }

    pub fn prepared(mut self) -> Self {
        self.lines = DiffLine::parse(&self.diff);
        self.digest = fnv1a(&self.diff);
        self.added = self.lines.iter().filter(|l| l.kind == super::diff::LineKind::Added).count();
        self.removed = self.lines.iter().filter(|l| l.kind == super::diff::LineKind::Removed).count();
        self
    }
}

/// FNV-1a over the UTF-8 bytes, as the Swift app stored it (so viewed marks carry over).
pub fn fnv1a(text: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash as i64
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiffRefs {
    pub base_sha: String,
    pub start_sha: String,
    pub head_sha: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Discussion {
    pub id: String,
    pub individual_note: bool,
    pub notes: Vec<Note>,
    /// What the forge resolves it by, when that isn't `id`: GitHub's review thread node.
    #[serde(default)]
    pub node_id: Option<String>,
}

impl Discussion {
    pub fn position(&self) -> Option<&Position> {
        self.notes.first().and_then(|n| n.position.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub new_path: Option<String>,
    pub old_path: Option<String>,
    pub new_line: Option<i64>,
    pub old_line: Option<i64>,
    /// The commit the line numbers belong to; an older one means the thread is outdated.
    pub head_sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub body: String,
    pub author: Person,
    pub created_at: Date,
    pub system: bool,
    pub resolvable: Option<bool>,
    pub resolved: Option<bool>,
    pub position: Option<Position>,
    /// Held in my review until I submit it: nobody else sees it yet.
    #[serde(default)]
    pub pending: bool,
    /// GitHub's GraphQL id, which editing or deleting a pending comment needs.
    #[serde(default)]
    pub node_id: Option<String>,
}

/// How a review ends when it's submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verdict {
    Comment,
    Approve,
    RequestChanges,
}

impl Verdict {
    pub const ALL: [Verdict; 3] = [Verdict::Comment, Verdict::RequestChanges, Verdict::Approve];

    pub fn title(self) -> &'static str {
        match self {
            Verdict::Comment => "Comment",
            Verdict::Approve => "Approve",
            Verdict::RequestChanges => "Request Changes",
        }
    }

    /// GitHub's name for it, in a review's `event`.
    pub fn event(self) -> &'static str {
        match self {
            Verdict::Comment => "COMMENT",
            Verdict::Approve => "APPROVE",
            Verdict::RequestChanges => "REQUEST_CHANGES",
        }
    }
}

/// One job of a merge request's latest pipeline, or one check run on a pull request's head
/// commit. Its status reads like a pipeline's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    /// GitLab's stage; on GitHub, the app that ran the check ("GitHub Actions").
    pub stage: String,
    pub status: PipelineStatus,
    pub web_url: String,
    pub started_at: Option<Date>,
    pub finished_at: Option<Date>,
    /// It may fail without failing the pipeline.
    pub allow_failure: bool,
    /// The forge can start it again from here: a retry once it's done, a run when it's manual.
    pub retryable: bool,
    /// Its log can be read here (GitHub keeps only Actions' logs).
    pub has_log: bool,
    /// GitLab: the project the pipeline ran in, when it isn't the merge request's own (a fork's
    /// merge request runs its pipeline in the fork).
    pub project: Option<i64>,
}

impl PipelineStatus {
    /// A GitLab job's REST status ("failed", "waiting_for_resource"…).
    pub fn from_gitlab_job(raw: &str) -> Self {
        Self::from_gitlab(&raw.to_uppercase()).unwrap_or(Self::Pending)
    }

    /// A GitHub check run's `status` and, once completed, its `conclusion`.
    pub fn from_check_run(status: &str, conclusion: Option<&str>) -> Self {
        match (status, conclusion) {
            ("completed", Some("success")) => Self::Success,
            ("completed", Some("cancelled")) => Self::Canceled,
            ("completed", Some("skipped" | "neutral" | "stale")) => Self::Skipped,
            ("completed", _) => Self::Failed,
            ("in_progress", _) => Self::Running,
            _ => Self::Pending,
        }
    }
}

impl Job {
    /// Still going, or waiting its turn.
    pub fn active(&self) -> bool {
        matches!(self.status, PipelineStatus::Running | PipelineStatus::Pending)
    }

    /// Failed, and not one that's allowed to.
    pub fn failed_hard(&self) -> bool {
        self.status == PipelineStatus::Failed && !self.allow_failure
    }

    pub fn offers_retry(&self) -> bool {
        self.retryable && matches!(self.status, PipelineStatus::Failed | PipelineStatus::Canceled)
    }

    pub fn offers_run(&self) -> bool {
        self.retryable && self.status == PipelineStatus::Manual
    }

    /// How long it ran, or has been running by `now`.
    pub fn seconds(&self, now: Date) -> Option<i64> {
        let start = self.started_at?;
        Some((self.finished_at.unwrap_or(now) - start).num_seconds().max(0))
    }
}

/// "38s", "4m 12s", "1h 5m".
pub fn duration_label(seconds: i64) -> String {
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {}s", s / 60, s % 60),
        s => format!("{}h {}m", s / 3600, s % 3600 / 60),
    }
}

/// Groups jobs by stage, keeping the order each stage first comes up in.
pub fn by_stage(mut jobs: Vec<Job>) -> Vec<Job> {
    let mut stages: Vec<String> = Vec::new();
    for job in &jobs {
        if !stages.contains(&job.stage) {
            stages.push(job.stage.clone());
        }
    }
    jobs.sort_by_key(|j| stages.iter().position(|s| *s == j.stage));
    jobs
}

/// The end of a job's log, made readable: colours and cursor codes gone, each line as a terminal
/// would have left it after carriage returns, GitHub's timestamps off the front.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JobLog {
    pub text: String,
    /// Lines before these were left out.
    pub cut: bool,
}

/// How many lines of a log show inline.
pub const LOG_LINES: usize = 200;

impl JobLog {
    pub fn tail(raw: &[u8], lines: usize) -> JobLog {
        let raw = String::from_utf8_lossy(raw);
        let clean = strip_ansi(&raw);
        let mut all: Vec<&str> = clean
            .split('\n')
            .filter_map(|line| {
                let line = line.strip_suffix('\r').unwrap_or(line);
                // What a carriage return wrote over is gone on a terminal too (GitLab's section
                // markers sit in front of one).
                let line = line.rsplit('\r').next().unwrap_or(line);
                // A section's end marker stands alone on its line.
                (!line.starts_with("section_end:") && !line.starts_with("section_start:")).then(|| strip_timestamp(line))
            })
            .collect();
        while all.last().is_some_and(|l| l.trim().is_empty()) {
            all.pop();
        }
        let cut = all.len() > lines;
        let start = all.len().saturating_sub(lines);
        JobLog { text: all[start..].join("\n"), cut }
    }
}

/// Drops ANSI escape sequences: CSI (colours, erasing), OSC (titles, links) and the two-byte rest.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// GitHub starts each line of an Actions log with when it was written.
fn strip_timestamp(line: &str) -> &str {
    match line.split_once(' ') {
        Some((stamp, rest)) if stamp.len() >= 20 && stamp.ends_with('Z') && chrono::DateTime::parse_from_rfc3339(stamp).is_ok() => rest,
        _ => line,
    }
}
