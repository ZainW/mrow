//! Where review requests come from. Each forge is one "account" in the sidebar's switcher.

use serde::{Deserialize, Serialize};

use super::cli::Cli;
use super::models::{DiffRefs, Discussion, FileDiff, MergeRequest, Person};
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
}

/// One forge's review queue, as fetched.
#[derive(Clone, Debug)]
pub struct QueueResult {
    pub me: Person,
    pub open: Vec<MergeRequest>,
    pub done: Vec<MergeRequest>,
}
