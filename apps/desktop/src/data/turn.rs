//! Whose move a review is. A request I asked changes on is the author's move until commits land
//! after my review (or I say it's mine again), so it waits in a fold at the end of To review
//! instead of sitting in Reviewed, then comes back at the top. `Turn::of` is the one place that
//! says so: the lists, their counts and the Dock badge all read it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::forge::Forge;
use super::models::{Date, MergeRequest, ReviewState};

/// Where a request I was asked to review stands with me.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// It needs my review.
    Mine,
    /// I asked for changes and it has moved since: commits landed, or I moved it back by hand.
    CameBack,
    /// I asked for changes and nothing new has come.
    Waiting,
    /// I approved or commented: Reviewed.
    Done,
}

/// A request I moved back to To review by hand, until I review again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedBack {
    pub forge: Forge,
    /// When I moved it.
    pub at: Date,
    /// My latest review's time, in seconds, when I moved it: another review ends the choice.
    pub review: Option<i64>,
}

impl MovedBack {
    pub fn new(item: &MergeRequest, now: Date) -> MovedBack {
        MovedBack { forge: item.forge, at: now, review: item.my_review_at.map(|d| d.timestamp()) }
    }

    /// Whether the choice still holds: I haven't reviewed again since.
    fn holds_for(&self, item: &MergeRequest) -> bool {
        self.review == item.my_review_at.map(|d| d.timestamp())
    }
}

impl Turn {
    pub fn of(item: &MergeRequest, moved: Option<&MovedBack>) -> Turn {
        match item.my_state {
            s if s.needs_review() => Turn::Mine,
            ReviewState::RequestedChanges if item.pushed_since_review.is_some() || moved.is_some_and(|m| m.holds_for(item)) => Turn::CameBack,
            ReviewState::RequestedChanges => Turn::Waiting,
            _ => Turn::Done,
        }
    }

    /// Whether To review lists it, and counts it.
    pub fn to_review(self) -> bool {
        matches!(self, Turn::Mine | Turn::CameBack)
    }
}

/// When a came-back request came back, and whether I did it: the time column and reason line.
pub fn came_back(item: &MergeRequest, moved: Option<&MovedBack>) -> Option<(Date, bool)> {
    match Turn::of(item, moved) {
        Turn::CameBack => match item.pushed_since_review {
            Some(at) => Some((at, false)),
            None => moved.map(|m| (m.at, true)),
        },
        _ => None,
    }
}

/// Drops the choices that have ended: I reviewed again, or the request left the open list of an
/// account that loaded. `open` is each loaded forge's open requests by ID.
pub fn prune(moved: &mut BTreeMap<String, MovedBack>, open: &[&MergeRequest], loaded: &[Forge]) {
    moved.retain(|id, m| !loaded.contains(&m.forge) || open.iter().any(|o| o.id == *id && m.holds_for(o)));
}
