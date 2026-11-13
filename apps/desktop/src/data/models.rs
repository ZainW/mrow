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
}
