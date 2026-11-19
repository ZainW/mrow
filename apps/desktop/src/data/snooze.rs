//! Snoozing and muting requests in To review and Mine. A snoozed request leaves its list and the
//! waiting count until tomorrow morning or until it changes, then comes back on its own. A muted
//! one stays out, re-requests included, until it's unmuted.
//!
//! What changed is read off the shared models only, so it means the same on both forges: for a
//! review, a new push (the diff's size moves), a new comment, or being asked again; for one of
//! mine, a new comment, an approval or review, a failed pipeline, or more open threads. My own
//! pushes don't wake my own requests.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};

use super::forge::Forge;
use super::inbox::Event;
use super::models::{Date, MergeRequest, PipelineStatus, ReviewState};

/// The hour a snooze "until tomorrow" ends, local time.
pub const MORNING_HOUR: u32 = 9;
/// Before this hour it's still last night: "tomorrow" is the morning about to come.
const NIGHT_ENDS_HOUR: u32 = 5;
/// How long a hold outlives its request leaving every list, in case it was only a hiccup.
const GONE_DAYS: i64 = 2;
/// A re-request within this many seconds of the last one is the same ask (as in the inbox).
const SAME_ASK_SECONDS: i64 = 60;

/// How long a request stays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "until", content = "at")]
pub enum Hold {
    /// Until a moment (tomorrow morning), or until I'm asked again, whichever comes first.
    Time(Date),
    /// Until the request changes.
    Change,
    /// Until it's unmuted.
    Mute,
}

impl Hold {
    pub fn muted(self) -> bool {
        self == Hold::Mute
    }
}

/// What a request looked like when it was held: what "it changed" is measured against.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mark {
    /// When I was last asked, in seconds; None when that was only a guess.
    pub asked: Option<i64>,
    pub comments: i64,
    /// Lines added and removed and files touched: a push moves them.
    pub size: (i64, i64, i64),
    pub approved_by: BTreeSet<String>,
    /// Each reviewer's state, by username.
    pub reviews: BTreeMap<String, ReviewState>,
    pub failed: bool,
    pub threads: Option<i64>,
}

impl Mark {
    pub fn of(item: &MergeRequest) -> Mark {
        Mark {
            asked: (!item.requested_at_is_guess).then(|| item.requested_at.timestamp()),
            comments: item.comment_count,
            size: (item.additions, item.deletions, item.file_count),
            approved_by: item.approved_by.iter().cloned().collect(),
            reviews: item.reviewers.iter().map(|r| (r.person.username.clone(), r.state)).collect(),
            failed: item.pipeline == Some(PipelineStatus::Failed),
            threads: item.unresolved_threads,
        }
    }
}

/// One held request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snooze {
    pub forge: Forge,
    pub hold: Hold,
    /// When it was held.
    pub since: Date,
    /// One of mine (then my own pushes aren't news), rather than a review asked of me.
    pub mine: bool,
    pub mark: Mark,
    /// When a full refresh first found it in none of the open lists.
    #[serde(default)]
    pub missing_since: Option<Date>,
}

impl Snooze {
    pub fn new(item: &MergeRequest, hold: Hold, mine: bool, now: Date) -> Snooze {
        Snooze { forge: item.forge, hold, since: now, mine, mark: Mark::of(item), missing_since: None }
    }
}

/// Why a snooze ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    Time,
    Changed,
}

/// The morning a snooze "until tomorrow" ends: 9:00 on the next working day. Friday, Saturday
/// and Sunday all wait for Monday, since "tomorrow" on a Friday afternoon means the next day of
/// work. Before 5:00 it's still the night before, so the coming morning counts as tomorrow.
pub fn morning_after<Tz: TimeZone>(now: &DateTime<Tz>) -> Date {
    let mut day = now.date_naive();
    if now.hour() >= NIGHT_ENDS_HOUR {
        day = day.succ_opt().unwrap_or(day);
    }
    while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
        day = day.succ_opt().unwrap_or(day);
    }
    let at = day.and_time(NaiveTime::from_hms_opt(MORNING_HOUR, 0, 0).expect("a time"));
    // A morning a clock change skips falls back to an hour later.
    let local = now.timezone().from_local_datetime(&at).earliest().or_else(|| now.timezone().from_local_datetime(&(at + Duration::hours(1))).earliest());
    local.map_or_else(|| now.to_utc() + Duration::days(1), |t| t.to_utc())
}

/// "Monday" when tomorrow morning is after a weekend, otherwise "Tomorrow": the menu's word.
pub fn morning_word<Tz: TimeZone>(now: &DateTime<Tz>) -> &'static str {
    let at = morning_after(now).with_timezone(&now.timezone());
    let days = (at.date_naive() - now.date_naive()).num_days();
    if days > 1 { "Monday" } else { "Tomorrow" }
}

/// When it comes back, for the snoozed fold: "Back tomorrow at 9:00", "Back when it changes".
pub fn back_label<Tz: TimeZone>(hold: Hold, now: &DateTime<Tz>) -> String {
    match hold {
        Hold::Mute => "Muted".into(),
        Hold::Change => "Back when it changes".into(),
        Hold::Time(at) => match day(at, now) {
            Day::Today => format!("Back at {}", until(at, now)),
            Day::Date(_) => format!("Back on {}", until(at, now)),
            _ => format!("Back {}", until(at, now)),
        },
    }
}

/// "9:00", "tomorrow at 9:00", "Monday at 9:00", "Dec 3 at 9:00": what follows "Snoozed until".
pub fn until<Tz: TimeZone>(at: Date, now: &DateTime<Tz>) -> String {
    let local = at.with_timezone(&now.timezone());
    let time = format!("{}:{:02}", local.hour(), local.minute());
    match day(at, now) {
        Day::Today => time,
        Day::Tomorrow => format!("tomorrow at {time}"),
        Day::Weekday(name) | Day::Date(name) => format!("{name} at {time}"),
    }
}

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The day a moment falls on, said from `now`.
enum Day {
    Today,
    Tomorrow,
    /// Within the week: "Monday".
    Weekday(String),
    /// Further off: "Dec 3".
    Date(String),
}

fn day<Tz: TimeZone>(at: Date, now: &DateTime<Tz>) -> Day {
    let local = at.with_timezone(&now.timezone());
    match (local.date_naive() - now.date_naive()).num_days() {
        ..=0 => Day::Today,
        1 => Day::Tomorrow,
        2..=6 => Day::Weekday(WEEKDAYS[local.weekday().num_days_from_monday() as usize].into()),
        _ => Day::Date(format!("{} {}", MONTHS[local.month0() as usize], local.day())),
    }
}

/// Whether `item` has changed since `snooze` held it, the way that matters to its role.
pub fn changed(snooze: &Snooze, item: &MergeRequest) -> bool {
    let was = &snooze.mark;
    let now = Mark::of(item);
    if now.comments > was.comments {
        return true;
    }
    if snooze.mine {
        // A reviewer saying something; an approval reset by my own push isn't that.
        let said = |state: &ReviewState| matches!(state, ReviewState::Reviewed | ReviewState::RequestedChanges | ReviewState::Approved);
        let reviewed = now.reviews.iter().any(|(who, state)| said(state) && was.reviews.get(who) != Some(state));
        let more_threads = matches!((was.threads, now.threads), (Some(a), Some(b)) if b > a);
        reviewed || !now.approved_by.is_subset(&was.approved_by) || (now.failed && !was.failed) || more_threads
    } else {
        // A size of nothing is a list that left the stats out, not a push.
        let pushed = was.size != (0, 0, 0) && now.size != (0, 0, 0) && now.size != was.size;
        asked_again(was, &now) || pushed
    }
}

fn asked_again(was: &Mark, now: &Mark) -> bool {
    matches!((was.asked, now.asked), (Some(a), Some(b)) if b > a + SAME_ASK_SECONDS) || (was.asked.is_none() && now.asked.is_some())
}

/// Whether a held request comes back now, and why. `item` is the request as the last refresh
/// saw it, when it saw it. A mute never ends by itself; a snooze until tomorrow also ends when
/// I'm asked to review again, since that's someone asking for me in particular.
pub fn wakes(snooze: &Snooze, item: Option<&MergeRequest>, now: Date) -> Option<Wake> {
    match snooze.hold {
        Hold::Mute => None,
        Hold::Time(at) if now >= at => Some(Wake::Time),
        Hold::Time(_) => item.filter(|m| !snooze.mine && asked_again(&snooze.mark, &Mark::of(m))).map(|_| Wake::Changed),
        Hold::Change => item.filter(|m| changed(snooze, m)).map(|_| Wake::Changed),
    }
}

/// Ends every hold that's due, returning the requests that came back and why. `item` finds a
/// request in the lists; a time snooze ends without one.
pub fn wake<'a>(holds: &mut BTreeMap<String, Snooze>, item: impl Fn(&str) -> Option<&'a MergeRequest>, now: Date) -> Vec<(String, Wake)> {
    let woken: Vec<(String, Wake)> = holds.iter().filter_map(|(id, s)| wakes(s, item(id), now).map(|w| (id.clone(), w))).collect();
    for (id, _) in &woken {
        holds.remove(id);
    }
    woken
}

/// The soonest a time snooze ends, for waking the window then.
pub fn next_wake(holds: &BTreeMap<String, Snooze>) -> Option<Date> {
    holds
        .values()
        .filter_map(|s| match s.hold {
            Hold::Time(at) => Some(at),
            _ => None,
        })
        .min()
}

/// Drops the holds of one account whose requests have finished or left every list: `open` is
/// every open request a full refresh of `forge` listed, `ended` the merged and closed ones it saw.
/// One missing for `GONE_DAYS` is let go; one that comes back before then keeps its hold.
pub fn prune(holds: &mut BTreeMap<String, Snooze>, forge: Forge, open: &HashSet<&str>, ended: &HashSet<&str>, now: Date) {
    holds.retain(|id, s| {
        if s.forge != forge {
            return true;
        }
        if ended.contains(id.as_str()) {
            return false;
        }
        if open.contains(id.as_str()) {
            s.missing_since = None;
            return true;
        }
        let since = *s.missing_since.get_or_insert(now);
        now - since < Duration::days(GONE_DAYS)
    });
}

/// Keeps what a refresh found on held requests out of the way: a muted request's events go,
/// a snoozed one's are kept in the inbox as already dealt with, so neither rings nor lights the
/// bell. Run it after `wake`, so a change that ended a snooze is told as usual.
pub fn quiet(found: &mut Vec<Event>, holds: &BTreeMap<String, Snooze>) {
    found.retain(|e| !holds.get(&e.request).is_some_and(|s| s.hold.muted()));
    for e in found.iter_mut().filter(|e| holds.contains_key(&e.request)) {
        e.done = true;
    }
}

/// "3 snoozed", "2 snoozed, 1 muted", "1 muted": the fold at the end of a list.
pub fn fold_label(snoozed: usize, muted: usize) -> String {
    match (snoozed, muted) {
        (s, 0) => format!("{s} snoozed"),
        (0, m) => format!("{m} muted"),
        (s, m) => format!("{s} snoozed, {m} muted"),
    }
}
