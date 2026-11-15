//! A request's pipeline on its overview (GitHub's checks on a pull request): the jobs by stage,
//! each with its status, how long it ran, Retry where it failed, and its log under it when its
//! name is pressed. The jobs load when the overview first shows and again while one runs (see
//! `Store::tick_jobs`).

use chrono::Utc;
use rune::{El, ScrollSpec, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Spinner};

use crate::app::App;
use crate::data::models::{Job, JobLog, LOG_LINES, MergeRequest, PipelineStatus, duration_label};
use crate::data::store::{Loadable, Store};
use crate::ui::components::{action_button, filled_mark, rule};
use crate::ui::icons as I;
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, READING_WIDTH, SEMIBOLD, mono, serif, ui};

/// How tall an open log gets before it scrolls.
const LOG_HEIGHT: f32 = 320.0;
/// The room at a job's end for Retry or Run.
const ACTION_SLOT: f32 = 84.0;

/// Whether the overview has a pipeline to show.
pub fn shows(s: &App, item: &MergeRequest) -> bool {
    item.pipeline.is_some() || s.store.jobs.contains_key(&item.id)
}

/// "Pipeline", "Checks".
fn title(item: &MergeRequest) -> String {
    let noun = item.kind().jobs_noun();
    format!("{}{}", noun[..1].to_uppercase(), &noun[1..])
}

/// The overview's pipeline section: a heading with the tally, then each stage's jobs.
pub fn section(s: &App, p: &Pal, k: &Kit, item: &MergeRequest) -> El {
    let noun = item.kind().jobs_noun();
    let jobs = s.store.jobs.get(&item.id);
    let mut head = row().w_full().pt(22.0).gap(12.0).center_y().child(text(title(item)).font(serif(22.0, SEMIBOLD).lines(1.25)).color(p.text).selectable(true));
    if let Some(jobs) = jobs.and_then(Loadable::value).filter(|j| !j.is_empty()) {
        head = head.child(el().grow(1.0)).child(text(tally(jobs)).font(ui(13.0, 400.0).features("'tnum' 1")).color(p.secondary).single_line());
    }
    let body = match jobs {
        None | Some(Loadable::Idle | Loadable::Loading) => {
            row().gap(8.0).center_y().child(Spinner::new().size(14.0).build(k)).child(quiet(p, format!("Loading the {noun}…")))
        }
        Some(Loadable::Failed(message)) => row()
            .gap(6.0)
            .items_start()
            .child(icon(&I::WARNING, 13.0).color(p.red).mt(2.0))
            .child(text(format!("Couldn’t load the {noun}: {message}")).font(ui(13.0, 400.0)).color(p.red).wrap().grow(1.0).shrink().min_w(0.0)),
        Some(Loadable::Loaded(jobs)) if jobs.is_empty() => quiet(
            p,
            match item.kind().jobs_noun() {
                "checks" => "No checks ran on the latest commit.".to_string(),
                _ => "The latest pipeline has no jobs.".to_string(),
            },
        ),
        Some(Loadable::Loaded(jobs)) => stages(s, p, k, item, jobs),
    };
    col().key("pipeline").w_full().max_w(READING_WIDTH).pt(44.0).child(rule(p)).child(head).child(body.mt(14.0))
}

fn quiet(p: &Pal, line: String) -> El {
    text(line).font(ui(13.0, 400.0)).color(p.secondary).wrap()
}

/// "1 failed · 2 running · 9 passed": what needs a look first.
fn tally(jobs: &[Job]) -> String {
    let count = |f: &dyn Fn(&Job) -> bool| jobs.iter().filter(|j| f(j)).count();
    let parts = [
        (count(&|j| j.failed_hard()), "failed"),
        (count(&|j| j.status == PipelineStatus::Running), "running"),
        (count(&|j| j.status == PipelineStatus::Pending), "pending"),
        (count(&|j| j.status == PipelineStatus::Manual), "manual"),
        (count(&|j| j.status == PipelineStatus::Success), "passed"),
    ];
    parts.iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect::<Vec<_>>().join(" · ")
}

/// Each stage's name over its jobs, in the order they run.
fn stages(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, jobs: &[Job]) -> El {
    let mut c = col().w_full().gap(18.0);
    let slot = jobs.iter().any(|j| j.offers_retry() || j.offers_run());
    let mut at = 0;
    while at < jobs.len() {
        let stage = &jobs[at].stage;
        let end = jobs[at..].iter().position(|j| &j.stage != stage).map_or(jobs.len(), |n| at + n);
        let mut list = col().key(format!("stage-{stage}")).w_full().padding(3.0).radius(10.0).border(1.0, p.line);
        for job in &jobs[at..end] {
            list = list.child(job_row(s, p, k, item, job, slot));
        }
        let mut group = col().w_full().gap(8.0);
        if !stage.is_empty() {
            group = group.child(text(stage.clone()).font(ui(12.0, MEDIUM)).color(p.tertiary).single_line());
        }
        c = c.child(group.child(list));
        at = end;
    }
    c
}

/// A job's mark: the pipeline's colours, each status its own shape so colour isn't all it says.
fn mark(p: &Pal, k: &Kit, job: &Job) -> El {
    const SIZE: f32 = 14.0;
    let glyph = match job.status {
        PipelineStatus::Success => filled_mark(&I::DISC, &I::SMALL_CHECK, p.green, p.paper, SIZE),
        PipelineStatus::Failed if job.allow_failure => icon(&I::WARNING, SIZE).color(p.amber),
        PipelineStatus::Failed => icon(&I::EXCLAIM_CIRCLE, SIZE).color(p.red),
        PipelineStatus::Running => Spinner::new().size(SIZE - 1.0).build(k),
        PipelineStatus::Pending => icon(&I::CLOCK, SIZE).color(p.tertiary),
        PipelineStatus::Canceled => icon(&I::CLOSED_CIRCLE, SIZE).color(p.tertiary),
        PipelineStatus::Skipped => icon(&I::DASHED_CIRCLE, SIZE).color(p.tertiary),
        PipelineStatus::Manual => icon(&I::HELD, SIZE).color(p.tertiary),
    };
    el().size(SIZE, SIZE).center().child(glyph).help(status_word(job))
}

fn status_word(job: &Job) -> &'static str {
    match job.status {
        PipelineStatus::Failed if job.allow_failure => "Failed, allowed to fail",
        PipelineStatus::Manual => "Manual: waits for someone to run it",
        status => status.label(),
    }
}

/// What goes at the end of a job's line: how long it ran, and a word where the mark needs one.
fn detail(job: &Job) -> String {
    let time = job.seconds(Utc::now()).map(duration_label);
    let word = match job.status {
        PipelineStatus::Failed if job.allow_failure => Some("allowed to fail"),
        PipelineStatus::Pending => Some("pending"),
        PipelineStatus::Manual => Some("manual"),
        PipelineStatus::Skipped => Some("skipped"),
        PipelineStatus::Canceled => Some("canceled"),
        _ => None,
    };
    match (word, time) {
        (Some(w), Some(t)) => format!("{w} · {t}"),
        (Some(w), None) => w.to_string(),
        (None, Some(t)) => t,
        (None, None) => String::new(),
    }
}

/// One job: its name opens its log (or its page, where there's no log to read here), and a
/// failed one has Retry, a manual one Run.
fn job_row(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, job: &Job, slot: bool) -> El {
    let key = Store::log_key(&item.id, job.id);
    let open = s.store.open_logs.contains(&key);
    let name_ink = if job.failed_hard() { p.red } else { p.text };
    let mut name = row()
        .key("name")
        .h(34.0)
        .pl(9.0)
        .pr(6.0)
        .gap(9.0)
        .center_y()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .radius(7.0)
        .child(mark(p, k, job))
        .child(text(job.name.clone()).font(ui(13.0, MEDIUM)).color(name_ink).single_line().truncate().shrink().min_w(0.0));
    let (it, j) = (item.clone(), job.clone());
    if job.has_log {
        name = name
            .role("button")
            .label(format!("{} log of {}", if open { "Hide the" } else { "Show the" }, job.name))
            .help(if open { "Hide the log" } else { "Show the log" })
            .hover_group()
            .hover(p.hover)
            .press(p.hover)
            .child(icon(if open { &I::CHEVRON_DOWN } else { &I::CHEVRON_RIGHT }, 10.0).color(p.tertiary).group_color(p.text))
            .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                // Opened and closed often while reading a failure: no motion.
                tx.instant();
                s.store.toggle_log(&it, &j);
            });
    } else if !job.web_url.is_empty() {
        let url = job.web_url.clone();
        let forge = item.kind().name();
        name = name
            .role("button")
            .label(format!("Open {} on {forge}", job.name))
            .help(format!("Open on {forge}"))
            .hover_group()
            .hover(p.hover)
            .press(p.hover)
            .child(icon(&I::ARROW_UP_RIGHT, 11.0).color(p.tertiary).group_color(p.text))
            .on_press_tx(move |_: &mut App, tx: &mut Tx| {
                let _ = tx.open_url(&url);
            });
    }
    let detail = detail(job);
    let detail_ink = if job.status == PipelineStatus::Failed && job.allow_failure { p.amber } else { p.tertiary };
    let mut line = row().key("line").w_full().gap(8.0).center_y().child(name);
    if !detail.is_empty() {
        line = line.child(text(detail).font(mono(12.0, 400.0).features("'tnum' 1")).color(detail_ink).single_line());
    }
    let action = if job.offers_retry() {
        Some(("retry", &I::REFRESH, "Retry"))
    } else if job.offers_run() {
        Some(("run", &I::PLAY, "Run"))
    } else {
        None
    };
    match action {
        Some((verb, glyph, label)) => {
            let (it, j) = (item.clone(), job.clone());
            let b =
                action_button(p, verb, Some(glyph), label, false, true).label(format!("{label} {}", job.name)).on_press_tx(move |s: &mut App, tx: &mut Tx| {
                    tx.instant();
                    s.store.retry_job(&it, &j);
                });
            line = line.child(row().w(ACTION_SLOT).justify_end().child(b.mr(2.0)));
        }
        // The times line up in a column: where some line has a button, the others keep its room.
        None if slot => line = line.child(el().w(ACTION_SLOT)),
        None => line = line.child(el().w(4.0)),
    }
    let mut c = col().key(format!("job-{}", job.id)).w_full().child(line);
    if open {
        c = c.child(log_view(s, p, k, item, job, &key));
    }
    c
}

/// A job's log under it: the last lines, scrolled to the end, where a failure says why.
fn log_view(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, job: &Job, key: &str) -> El {
    let body = match s.store.job_logs.get(key) {
        None | Some(Loadable::Idle | Loadable::Loading) => {
            row().padding(12.0).gap(8.0).center_y().child(Spinner::new().size(13.0).build(k)).child(quiet(p, "Loading the log…".into()))
        }
        Some(Loadable::Failed(message)) => row()
            .padding(12.0)
            .gap(6.0)
            .items_start()
            .child(icon(&I::WARNING, 13.0).color(p.red).mt(2.0))
            .child(text(format!("Couldn’t load the log: {message}")).font(ui(13.0, 400.0)).color(p.red).wrap().grow(1.0).shrink().min_w(0.0)),
        Some(Loadable::Loaded(log)) => log_text(p, item, job, log),
    };
    el().key("log").w_full().pt(2.0).pb(4.0).px(6.0).child(col().w_full().radius(7.0).fill(p.well).border(1.0, p.line).child(body))
}

fn log_text(p: &Pal, item: &MergeRequest, job: &Job, log: &JobLog) -> El {
    let lines = if log.text.is_empty() { "The log is empty.".to_string() } else { log.text.clone() };
    let scroll = col()
        .key(format!("log-scroll-{}-{}", item.id, job.id))
        .w_full()
        .max_h(LOG_HEIGHT)
        .px(12.0)
        .py(10.0)
        .child(text(lines).font(mono(12.0, 400.0).lines(1.5)).color(p.text).wrap().selectable(true).w_full())
        .scroll(ScrollSpec { stick_bottom: true });
    let mut c = col().w_full().child(scroll);
    if log.cut || !job.web_url.is_empty() {
        let mut foot = row().w_full().h(30.0).px(12.0).gap(8.0).center_y().border_sides([1.0, 0.0, 0.0, 0.0], p.line);
        if log.cut {
            foot = foot.child(text(format!("The last {LOG_LINES} lines")).font(ui(12.0, 400.0)).color(p.tertiary).single_line());
        }
        foot = foot.child(el().grow(1.0));
        if !job.web_url.is_empty() {
            let url = job.web_url.clone();
            foot = foot.child(
                row()
                    .key("full-log")
                    .role("button")
                    .label("Open the full log")
                    .gap(4.0)
                    .center_y()
                    .hover_group()
                    .child(text("Open the full log").font(ui(12.0, MEDIUM)).color(p.secondary).group_color(p.text).single_line())
                    .child(icon(&I::ARROW_UP_RIGHT, 10.0).color(p.secondary).group_color(p.text))
                    .on_press_tx(move |_: &mut App, tx: &mut Tx| {
                        let _ = tx.open_url(&url);
                    }),
            );
        }
        c = c.child(foot);
    }
    c
}
