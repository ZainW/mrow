//! Where review requests come from. Each forge is one "account" in the sidebar's switcher.

use serde::{Deserialize, Serialize};

use super::cli::Cli;
use super::models::{DiffRefs, Discussion, FileDiff, Job, JobLog, LOG_LINES, MergeRequest, Note, Person, Verdict};
use super::{github, gitlab};
use crate::data::diff::DiffLine;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Forge {
    #[default]
    Gitlab,
    Github,
}

impl Forge {
    pub const ALL: [Forge; 2] = [Forge::Gitlab, Forge::Github];

    pub fn from_raw(raw: &str) -> Option<Forge> {
        match raw {
            "gitlab" => Some(Forge::Gitlab),
            "github" => Some(Forge::Github),
            _ => None,
        }
    }

    pub fn raw(self) -> &'static str {
        match self {
            Forge::Gitlab => "gitlab",
            Forge::Github => "github",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Forge::Gitlab => "GitLab",
            Forge::Github => "GitHub",
        }
    }

    /// "merge request", "pull request".
    pub fn noun(self) -> &'static str {
        match self {
            Forge::Gitlab => "merge request",
            Forge::Github => "pull request",
        }
    }

    pub fn plural_noun(self) -> String {
        format!("{}s", self.noun())
    }

    /// What goes between the project and the number: "web!42", "web#42".
    pub fn reference_mark(self) -> &'static str {
        match self {
            Forge::Gitlab => "!",
            Forge::Github => "#",
        }
    }

    pub fn monogram(self) -> &'static str {
        match self {
            Forge::Gitlab => "GL",
            Forge::Github => "GH",
        }
    }

    /// Whether a review in progress can hold a comment on the whole request. GitHub's can't:
    /// there, that's what the summary is for.
    pub fn drafts_general_comments(self) -> bool {
        self == Forge::Gitlab
    }

    pub fn cli(self) -> Cli {
        match self {
            Forge::Gitlab => Cli::glab(),
            Forge::Github => Cli::gh(),
        }
    }

    pub fn default_host(self) -> &'static str {
        match self {
            Forge::Gitlab => "gitlab.com",
            Forge::Github => "github.com",
        }
    }

    // Everything Mrow asks of a forge. Each maps its own API onto the shared models, so views
    // never know which forge a merge request came from beyond its wording. These block: call
    // them off the UI thread.

    /// Open review requests, and the ones finished since `done_days` ago.
    pub fn queue(self, done_days: i64) -> Result<QueueResult, String> {
        match self {
            Forge::Gitlab => gitlab::queue(done_days),
            Forge::Github => github::queue(done_days),
        }
    }

    /// Every file's diff, with the commits it was taken between (None when they moved mid-load).
    pub fn diffs(self, item: &MergeRequest) -> Result<(Vec<FileDiff>, Option<DiffRefs>), String> {
        match self {
            Forge::Gitlab => gitlab::diffs(item),
            Forge::Github => github::diffs(item),
        }
    }

    /// Comment threads, oldest first.
    pub fn discussions(self, item: &MergeRequest) -> Result<Vec<Discussion>, String> {
        match self {
            Forge::Gitlab => gitlab::discussions(item),
            Forge::Github => github::discussions(item),
        }
    }

    pub fn approve(self, item: &MergeRequest) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::approve(item),
            Forge::Github => github::approve(item),
        }
    }

    pub fn unapprove(self, item: &MergeRequest) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::unapprove(item),
            Forge::Github => github::unapprove(item),
        }
    }

    pub fn merge(self, item: &MergeRequest) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::merge(item),
            Forge::Github => github::merge(item),
        }
    }

    /// Has the forge merge it by itself once everything passes, or stops that.
    pub fn set_auto_merge(self, item: &MergeRequest, on: bool) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::set_auto_merge(item, on),
            Forge::Github => github::set_auto_merge(item, on),
        }
    }

    /// Takes a draft out of draft.
    pub fn mark_ready(self, item: &MergeRequest) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::mark_ready(item),
            Forge::Github => github::mark_ready(item),
        }
    }

    /// Points a merge request at another target branch.
    pub fn retarget(self, item: &MergeRequest, branch: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::retarget(item, branch),
            Forge::Github => github::retarget(item, branch),
        }
    }

    pub fn comment(self, item: &MergeRequest, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::comment(item, body),
            Forge::Github => github::comment(item, body),
        }
    }

    pub fn comment_on_line(self, item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: &DiffRefs, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::comment_on_line(item, file, line, refs, body),
            Forge::Github => github::comment_on_line(item, file, line, refs, body),
        }
    }

    pub fn reply(self, item: &MergeRequest, thread: &Discussion, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::reply(item, thread, body),
            Forge::Github => github::reply(item, thread, body),
        }
    }

    pub fn set_resolved(self, item: &MergeRequest, thread: &Discussion, resolved: bool) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::set_resolved(item, thread, resolved),
            Forge::Github => github::set_resolved(thread, resolved),
        }
    }

    // A review in progress: comments held on the forge (so they outlast a relaunch and show on
    // the web) until it's submitted with a verdict.

    /// Holds a comment on the whole request; only where `drafts_general_comments`.
    pub fn draft_comment(self, item: &MergeRequest, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::draft_comment(item, body),
            Forge::Github => Err("GitHub can't hold a comment on the whole pull request in a review. Write it in the review's summary.".into()),
        }
    }

    pub fn draft_on_line(self, item: &MergeRequest, file: &FileDiff, line: &DiffLine, refs: &DiffRefs, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::draft_on_line(item, file, line, refs, body),
            Forge::Github => github::draft_on_line(item, file, line, refs, body),
        }
    }

    pub fn edit_draft(self, item: &MergeRequest, note: &Note, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::edit_draft(item, note, body),
            Forge::Github => github::edit_draft(note, body),
        }
    }

    pub fn delete_draft(self, item: &MergeRequest, note: &Note) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::delete_draft(item, note),
            Forge::Github => github::delete_draft(note),
        }
    }

    /// Posts the held comments and the summary (when there is one) with the verdict.
    pub fn submit_review(self, item: &MergeRequest, verdict: Verdict, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::submit_review(item, verdict, body),
            Forge::Github => github::submit_review(item, verdict, body),
        }
    }

    // The pipeline: its jobs (GitLab) or the head commit's checks (GitHub).

    /// "pipeline", "checks": what the forge calls the jobs that run on a request.
    pub fn jobs_noun(self) -> &'static str {
        match self {
            Forge::Gitlab => "pipeline",
            Forge::Github => "checks",
        }
    }

    /// The latest pipeline's jobs, or the head commit's check runs, in the order they run.
    pub fn jobs(self, item: &MergeRequest) -> Result<Vec<Job>, String> {
        match self {
            Forge::Gitlab => gitlab::jobs(item),
            Forge::Github => github::jobs(item),
        }
    }

    /// Starts a job again: a retry once it's done, a run when it's manual.
    pub fn retry_job(self, item: &MergeRequest, job: &Job) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::retry_job(item, job),
            Forge::Github => github::retry_job(item, job),
        }
    }

    /// The end of a job's log.
    pub fn job_log(self, item: &MergeRequest, job: &Job) -> Result<JobLog, String> {
        let raw = match self {
            Forge::Gitlab => gitlab::job_log(item, job)?,
            Forge::Github => github::job_log(item, job)?,
        };
        Ok(JobLog::tail(&raw, LOG_LINES))
    }

    // My posted comments, edited or deleted in place.

    /// Replaces the text of a comment of mine. `body` is what I wrote: see `written`.
    pub fn edit_note(self, item: &MergeRequest, thread: &Discussion, note: &Note, body: &str) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::edit_note(item, note, body),
            Forge::Github => github::edit_note(item, thread, note, body),
        }
    }

    pub fn delete_note(self, item: &MergeRequest, thread: &Discussion, note: &Note) -> Result<(), String> {
        match self {
            Forge::Gitlab => gitlab::delete_note(item, note),
            Forge::Github => github::delete_note(item, thread, note),
        }
    }

    /// Whether a comment of mine can be deleted. A GitHub review's summary can only be edited.
    pub fn deletes_note(self, thread: &Discussion) -> bool {
        match self {
            Forge::Gitlab => true,
            Forge::Github => github::deletes_note(thread),
        }
    }

    /// A note's text split into what the app put in front of it (a GitHub review's verdict)
    /// and what I wrote, which is what its editor holds.
    pub fn written(self, thread: &Discussion, note: &Note) -> (String, String) {
        match self {
            Forge::Gitlab => (String::new(), note.body.clone()),
            Forge::Github => github::written(thread, note),
        }
    }
}

/// One forge's review queue, as fetched.
#[derive(Clone, Debug)]
pub struct QueueResult {
    pub me: Person,
    pub open: Vec<MergeRequest>,
    pub done: Vec<MergeRequest>,
    /// The open ones I wrote.
    pub mine: Vec<MergeRequest>,
    /// Why `mine` is empty, when asking for it failed. The reviews still load.
    pub mine_error: Option<String>,
}
