//! The merge requests I opened: the Mine list in the sidebar (status marks, filters, stacks
//! that fold to what's on me) and what a request of mine adds to its page (the next step and its
//! stack at the top, what's left before it merges at the end).

use std::rc::Rc;

use rune::{El, GrowthSpec, PointerPress, Tx, col, el, icon, row, text};

use crate::app::{App, Confirm};
use crate::data::mine::{self, Standing, Status};
use crate::data::models::{MergeRequest, PipelineStatus, ReviewState};
use crate::data::store::{MineLine, Rail, Store};
use crate::ui::components::{action_button, filled_mark, short_time, state_icon};
use crate::ui::icons as I;
use crate::ui::menus;
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, READING_WIDTH, SEMIBOLD, mono, ui};

// MARK: Marks

pub fn standing_color(p: &Pal, standing: Standing) -> rune::Color {
    match standing {
        Standing::Ready => p.green,
        Standing::NeedsYou => p.red,
        Standing::Quiet => p.accent,
        Standing::Waiting | Standing::Held | Standing::Draft => p.tertiary,
    }
}

/// The mark for where a request of mine stands. `ground` is what it's drawn on, for the tick
/// knocked out of the ready disc.
pub fn standing_mark(p: &Pal, standing: Standing, size: f32, ground: rune::Color) -> El {
    let c = standing_color(p, standing);
    let mark = match standing {
        Standing::Ready => filled_mark(&I::DISC, &I::SMALL_CHECK, c, ground, size),
        Standing::NeedsYou => icon(&I::EXCLAIM_CIRCLE, size).color(c),
        Standing::Waiting | Standing::Quiet => icon(&I::CLOCK, size).color(c),
        Standing::Held => icon(&I::HELD, size).color(c),
        Standing::Draft => icon(&I::DASHED_CIRCLE, size).color(c),
    };
    mark.help(standing.label())
}

// MARK: Sidebar

/// The status filters above the Mine list: everything, or only the requests standing one way.
pub fn filters(store: &Store, p: &Pal) -> El {
    let chip = |key: &str, on: bool, label: &str| {
        let c =
            row().key(key.to_string()).role("button").label(label.to_string()).h(26.0).gap(5.0).center_y().radius(13.0).help(label.to_string()).press(p.hover);
        if on { c.fill(p.elevated).border(1.0, p.strong_line) } else { c.border(1.0, p.line).hover(p.hover) }
    };
    let ink = |on: bool| if on { p.text } else { p.secondary };
    let all = store.mine_filter.is_none();
    let mut r = row().key("mine-filters").gap(4.0).center_y().child(
        chip("standing-all", all, "Everything of yours").px(11.0).child(text("All").font(ui(12.0, MEDIUM)).color(ink(all)).single_line()).on_press_tx(
            |s: &mut App, tx: &mut Tx| {
                tx.instant();
                s.store.mine_filter = None;
            },
        ),
    );
    for standing in Standing::FILTERS {
        let on = store.mine_filter == Some(standing);
        let count = store.mine_count(standing);
        r = r.child(
            chip(&format!("standing-{}", standing.word()), on, standing.label())
                .pl(7.0)
                .pr(10.0)
                .child(standing_mark(p, standing, 12.0, if on { p.elevated } else { p.chrome }))
                .child(text(count.to_string()).font(ui(12.0, MEDIUM).features("'tnum' 1")).color(ink(on)).single_line())
                .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                    // A filter is flipped often while triaging: the list changes at once.
                    tx.instant();
                    s.store.mine_filter = if on { None } else { Some(standing) };
                }),
        );
    }
    r
}

/// The Mine list's rows. The caller wraps them in the queue's scroll view.
pub fn rows(s: &App, p: &Pal) -> Option<El> {
    let store = &s.store;
    let lines = store.mine_lines();
    if lines.is_empty() {
        return None;
    }
    let key = |line: &MineLine| match line {
        MineLine::Header(t) => format!("h-{t}"),
        MineLine::Item { item, .. } => item.id.clone(),
        MineLine::StackHead { key, .. } => format!("stack-{key}"),
        MineLine::Folded { key, text, .. } => format!("fold-{key}-{text}"),
        MineLine::Whole { key, .. } => format!("whole-{key}"),
    };
    let keys: Rc<Vec<String>> = Rc::new(lines.iter().map(key).collect());
    let flags: Vec<(bool, bool)> = lines
        .iter()
        .map(|l| match l {
            MineLine::Item { item, .. } => (store.selected_id.as_deref() == Some(item.id.as_str()), store.has_news(&item.id)),
            _ => (false, false),
        })
        .collect();
    let lines = Rc::new(lines);
    let pal = *p;
    let version: Vec<&str> = keys.iter().map(String::as_str).collect();
    let list_keys = keys.clone();
    Some(col().key("rows").w_full().pb(8.0).growth(GrowthSpec::default()).virtual_rows_version(&version).virtual_rows(
        lines.len(),
        72.0,
        move |_: &App, i| std::borrow::Cow::Owned(list_keys[i].clone()),
        move |_: &App, i| match &lines[i] {
            MineLine::Header(t) => crate::ui::sidebar::section_header(&pal, t),
            MineLine::Item { item, status, rail, place } => item_row(&pal, item, status, *rail, *place, flags[i].0, flags[i].1),
            MineLine::StackHead { key, name, count, open, merging } => stack_head(&pal, key, name, *count, *open, *merging),
            MineLine::Folded { key, text, rail } => folded(&pal, key, text, *rail),
            MineLine::Whole { key, name, text } => whole(&pal, key, name, text),
        },
    ))
}

/// Where a stack's rail runs, measured from the row's outer edge: through the centre of the
/// dot at the left of each card.
const RAIL_X: f32 = 20.25;
/// The centre of an item row's dot, from the row's top.
const NODE_Y: f32 = 17.0;

/// The rail's line through one row, in two pieces that stop short of the node: up from all
/// but the first row, down from all but the last.
fn rail_lines(p: &Pal, rail: Rail, node_y: f32) -> Vec<El> {
    const CLEAR: f32 = 6.5;
    let line = || el().w(1.5).fill(p.strong_line).inert();
    let mut lines = Vec::new();
    if !rail.first {
        lines.push(line().pin(Some(RAIL_X), Some(0.0), None, None).h(node_y - CLEAR));
    }
    if !rail.last {
        lines.push(line().pin(Some(RAIL_X), Some(node_y + CLEAR), None, Some(0.0)));
    }
    lines
}

fn item_row(p: &Pal, item: &MergeRequest, status: &Status, rail: Option<Rail>, place: Option<(usize, usize)>, selected: bool, unread: bool) -> El {
    let quiet = status.standing == Standing::Quiet;
    let title_row = row()
        .gap(6.0)
        .items_start()
        .child(
            text(item.title.clone())
                .font(ui(13.0, MEDIUM).lines(1.3))
                .color(if item.draft { p.secondary } else { p.text })
                .wrap()
                .max_lines(2)
                .grow(1.0)
                .shrink()
                .min_w(0.0),
        )
        .child(
            text(short_time(item.updated_at))
                .font(ui(12.0, MEDIUM).features("'tnum' 1"))
                .color(if quiet { p.accent } else { p.tertiary })
                .single_line()
                .pt(1.5)
                .help(format!("Last activity {}", crate::ui::components::long_date(item.updated_at))),
        );
    let step_color = match status.standing {
        Standing::Ready | Standing::NeedsYou | Standing::Quiet => standing_color(p, status.standing),
        _ => p.secondary,
    };
    let step = text(status.step.clone()).font(ui(12.0, 400.0)).color(step_color).single_line().truncate();
    let mut meta = row()
        .gap(6.0)
        .center_y()
        .child(text(item.short_reference()).font(mono(12.0, 400.0)).color(p.tertiary).single_line().truncate().shrink().min_w(0.0))
        .child(el().grow(1.0).min_w(4.0));
    if let Some((at, of)) = place {
        meta = meta.child(
            el().h(16.0)
                .px(5.0)
                .radius(4.0)
                .fill(p.hover)
                .center()
                .help("Its place in a stack, from the bottom")
                .child(text(format!("{at} of {of}")).font(ui(11.0, MEDIUM).features("'tnum' 1")).color(p.secondary).single_line()),
        );
    }
    meta = meta.child(standing_mark(p, status.standing, 12.0, p.chrome));
    let body = col().gap(4.0).grow(1.0).shrink().min_w(0.0).child(title_row).child(step).child(meta);
    // On a rail the dot is the stack's node: a ring, filled when there's something unread.
    let dot = match rail {
        Some(_) => el()
            .size(7.0, 7.0)
            .mt(5.5)
            .ml(-0.5)
            .radius(3.5)
            .fill(if unread { p.accent } else { p.accent.alpha(0.0) })
            .border(1.5, if unread { p.accent } else { p.tertiary }),
        None => el().size(6.0, 6.0).mt(6.0).radius(3.0).fill(if unread { p.accent } else { p.accent.alpha(0.0) }),
    };
    let card = row().key("card").w_full().items_start().gap(8.0).pl(10.0).pr(12.0).py(8.0).radius(7.0).child(dot).child(body);
    let card = if selected { card.fill(p.selected) } else { card.hover(p.hover) };
    let (id, id2, id3) = (item.id.clone(), item.id.clone(), item.id.clone());
    let mut outer = el().key(item.id.clone()).role("listitem").label(format!("{}. {}", item.title, status.step)).w_full().px(8.0).pb(1.0);
    for line in rail.map(|r| rail_lines(p, r, NODE_Y)).unwrap_or_default() {
        outer = outer.child(line);
    }
    outer
        .child(card)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            if tx.mods().cmd {
                s.store.open_tab(&id, false);
            } else {
                s.store.select(Some(id.clone()));
            }
            tx.focus("queue", false);
        })
        .on_double_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.store.open_tab(&id2, true);
        })
        .on_secondary_press(move |s: &mut App, at: PointerPress, tx: &mut Tx| menus::row(s, &id3, at, tx))
}

fn toggle(key: &str) -> impl Fn(&mut App, &mut Tx) + 'static {
    let key = key.to_string();
    move |s: &mut App, tx: &mut Tx| {
        // Rows in a long list, often opened from the keyboard: they land at once.
        tx.instant();
        s.store.toggle_stack(&key);
    }
}

fn chevron(p: &Pal, open: bool) -> El {
    icon(&I::CHEVRON_RIGHT, 10.0).key("chevron").color(p.tertiary).group_color(p.text).rotate(if open { std::f32::consts::FRAC_PI_2 } else { 0.0 })
}

/// A stack's name above its rows; a press shows every part, or folds them again.
fn stack_head(p: &Pal, key: &str, name: &str, count: usize, open: bool, merging: bool) -> El {
    let label = if open { "Fold the parts that aren't on you" } else { "Show every part" };
    el().key(format!("stack-{key}")).w_full().px(8.0).pt(4.0).child(
        row()
            .key("head")
            .role("button")
            .label(format!("{name}, a stack of {count}. {label}"))
            .w_full()
            .h(26.0)
            .pl(8.0)
            .pr(12.0)
            .gap(6.0)
            .center_y()
            .radius(6.0)
            .hover_group()
            .hover(p.hover)
            .press(p.hover)
            .help(label)
            .child(el().size(10.0, 10.0).center().child(chevron(p, open)))
            .child(text(name.to_string()).font(ui(12.0, SEMIBOLD)).color(p.secondary).single_line().truncate().shrink().min_w(0.0))
            .child(
                text(if merging { format!("stack of {count} · merging") } else { format!("stack of {count}") })
                    .font(ui(12.0, 400.0))
                    .color(if merging { p.accent } else { p.tertiary })
                    .single_line(),
            )
            .on_press_tx(toggle(key)),
    )
}

/// The parts of a stack that aren't on me, as one line on the rail. A press shows them.
fn folded(p: &Pal, key: &str, summary: &str, rail: Rail) -> El {
    const NODE: f32 = 13.0;
    let dot = el().size(5.0, 5.0).ml(0.5).radius(2.5).fill(p.tertiary);
    let card = row()
        .key("card")
        .role("button")
        .label(format!("{summary}. Show them"))
        .w_full()
        .h(26.0)
        .gap(8.5)
        .pl(10.0)
        .pr(12.0)
        .center_y()
        .radius(7.0)
        .hover_group()
        .hover(p.hover)
        .help("Show every part")
        .child(dot)
        .child(
            text(summary.to_string()).font(ui(12.0, 400.0)).color(p.tertiary).group_color(p.secondary).single_line().truncate().grow(1.0).shrink().min_w(0.0),
        );
    let mut outer = el().key(format!("fold-{key}-{summary}")).w_full().px(8.0).pb(1.0);
    for line in rail_lines(p, rail, NODE) {
        outer = outer.child(line);
    }
    outer.child(card).on_press_tx(toggle(key))
}

/// A stack with nothing on me, as a single row. A press unfolds it.
fn whole(p: &Pal, key: &str, name: &str, summary: &str) -> El {
    let body = col()
        .gap(4.0)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .child(text(name.to_string()).font(ui(13.0, MEDIUM).lines(1.3)).color(p.secondary).single_line().truncate())
        .child(text(summary.to_string()).font(ui(12.0, 400.0)).color(p.tertiary).single_line().truncate());
    let card = row()
        .key("card")
        .role("button")
        .label(format!("{name}. {summary}. Show its parts"))
        .w_full()
        .items_start()
        .gap(6.0)
        .pl(8.0)
        .pr(12.0)
        .py(8.0)
        .radius(7.0)
        .hover_group()
        .hover(p.hover)
        .help("Show its parts")
        .child(el().size(10.0, 17.0).center().child(chevron(p, false)))
        .child(body);
    el().key(format!("whole-{key}")).w_full().px(8.0).pb(1.0).child(card).on_press_tx(toggle(key))
}

// MARK: The request

fn caption(p: &Pal, label: impl Into<String>) -> El {
    text(label.into()).font(ui(12.0, MEDIUM)).color(p.tertiary).single_line()
}

/// The next step for one of my merge requests: where it stands, why, and the button that moves it.
pub fn next_step(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let store = &s.store;
    let status = store.status_of(item);
    let stack = store.stack_of(item);
    let at = stack.as_ref().and_then(|(_, parts)| parts.iter().position(|m| m.id == item.id));
    let below = at.filter(|i| *i > 0).and_then(|i| stack.as_ref().map(|(_, parts)| &parts[i - 1]));
    let above = at.and_then(|i| stack.as_ref().and_then(|(_, parts)| parts.get(i + 1)));
    let headline = row()
        .gap(8.0)
        .center_y()
        .child(standing_mark(p, status.standing, 15.0, p.well))
        .child(text(status.step.clone()).font(ui(14.0, SEMIBOLD)).color(p.text).wrap().grow(1.0).shrink().min_w(0.0));
    let why = text(mine::explain(item, &status, below, above)).font(ui(13.0, 400.0).lines(1.45)).color(p.secondary).wrap();
    let card =
        col().key("next-step").w_full().max_w(READING_WIDTH).gap(8.0).padding(14.0).radius(10.0).fill(p.well).border(1.0, p.line).child(headline).child(why);
    let idle = store.busy.is_none();
    let id = item.id.clone();
    // A button that does something to this merge request, looked up fresh when pressed.
    let act = move |b: El, run: fn(&mut App, &MergeRequest)| {
        let id = id.clone();
        b.on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            if s.store.busy.is_none()
                && let Some(item) = s.store.item(&id).cloned()
            {
                run(s, &item);
            }
        })
    };
    let note = |line: String| text(line).font(ui(13.0, 400.0).lines(1.45)).color(p.text).wrap().w_full();
    let mut actions = Vec::new();
    let confirm = s.confirm.as_ref().filter(|(id, _)| id == &item.id).map(|(_, kind)| *kind);
    let mut card = card;
    if store.walk_of(&item.id).is_some() {
        // Its stack is merging: say so, and offer the way out. Nothing else can be started.
        card = card.child(note("Merging the stack: each part merges as soon as it's ready. Keep Mrow open until it's done.".into()));
        actions.push(act(action_button(p, "stop-walk", Some(&I::CLOSE), "Stop", false, true), |s, item| s.store.stop_walk(&item.id)));
    } else if let (Some(kind), Standing::Ready) = (confirm, status.standing) {
        // Merging can't be taken back, so it takes a second press.
        let (question, title): (String, &str) = match (kind, above) {
            (Confirm::Stack, Some(a)) => (
                format!(
                    "Merge {} into {} now, then {} and each part above it once it's approved and green? It stops at the first part that isn't.",
                    item.reference(),
                    item.target_branch,
                    a.reference()
                ),
                "Merge the Stack",
            ),
            _ => (format!("Merge {} into {}?", item.reference(), item.target_branch), "Merge"),
        };
        card = card.child(note(question));
        actions.push(act(
            action_button(p, "confirm-merge", Some(&I::MERGE), title, true, idle),
            match kind {
                Confirm::Stack => |s, item| {
                    s.confirm = None;
                    s.store.merge_stack(item);
                },
                Confirm::Merge => |s, item| {
                    s.confirm = None;
                    s.store.merge_here(item);
                },
            },
        ));
        actions.push(action_button(p, "cancel-merge", None, "Cancel", false, true).on_press_tx(|s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.confirm = None;
        }));
    } else {
        let ask = |b: El, kind: Confirm| {
            let id = item.id.clone();
            b.on_press_tx(move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                s.confirm = Some((id.clone(), kind));
            })
        };
        match status.standing {
            Standing::Ready => {
                actions.push(ask(action_button(p, "merge", Some(&I::MERGE), "Merge", true, idle), Confirm::Merge));
                if above.is_some_and(|a| store.is_mine(a)) {
                    actions.push(ask(action_button(p, "merge-stack", None, "Merge the Stack…", false, idle), Confirm::Stack));
                }
            }
            Standing::Held => {
                if let Some(b) = below {
                    let id = b.id.clone();
                    actions.push(action_button(p, "go-below", None, &format!("Go to {}", b.reference()), false, true).on_press_tx(
                        move |s: &mut App, tx: &mut Tx| {
                            tx.instant();
                            s.store.select(Some(id.clone()));
                        },
                    ));
                }
            }
            Standing::NeedsYou if item.pipeline == Some(PipelineStatus::Failed) => {
                let url = pipeline_url(item);
                actions.push(action_button(p, "pipeline", Some(&I::ARROW_UP_RIGHT), "Open the Pipeline", true, true).on_press_tx(
                    move |_: &mut App, tx: &mut Tx| {
                        let _ = tx.open_url(&url);
                    },
                ));
            }
            Standing::NeedsYou if item.unresolved_threads.unwrap_or(0) > 0 => {
                actions.push(
                    action_button(p, "threads", Some(&I::BUBBLE_TEXT), "Go to the Threads", true, true).on_press_tx(crate::ui::detail::jump_to_open_thread),
                );
            }
            Standing::Waiting | Standing::Quiet if item.auto_merge => {
                actions.push(act(action_button(p, "auto-merge-off", None, "Turn Auto-Merge Off", false, idle), |s, item| s.store.set_auto_merge(item, false)));
            }
            Standing::Waiting | Standing::Quiet => {
                actions.push(
                    act(action_button(p, "auto-merge", Some(&I::MERGE), "Merge When Ready", false, idle), |s, item| s.store.set_auto_merge(item, true))
                        .help(format!("{} merges it by itself once everything passes", item.kind().name())),
                );
            }
            Standing::Draft => {
                actions.push(act(action_button(p, "ready", Some(&I::CHECK), "Mark Ready", false, idle), |s, item| s.store.mark_ready(item)));
            }
            Standing::NeedsYou => {}
        }
    }
    if actions.is_empty() { card } else { card.child(row().pt(4.0).gap(8.0).center_y().flex_wrap().children(actions)) }
}

/// The stack a merge request belongs to, in the order it merges: the branch it lands on, then
/// each part from the bottom up, as the Mine list shows it.
pub fn stack_section(s: &App, p: &Pal, item: &MergeRequest) -> Option<El> {
    let store = &s.store;
    let (stack, parts) = store.stack_of(item)?;
    let base = parts.first().map(|m| m.target_branch.clone()).unwrap_or_default();
    let mut list = col()
        .key("stack")
        .w_full()
        .max_w(READING_WIDTH)
        .padding(3.0)
        .radius(10.0)
        .border(1.0, p.line)
        .child(el().h(24.0).pl(31.0).center_y().child(text(base).font(mono(12.0, 400.0)).color(p.tertiary).single_line()));
    for part in &parts {
        let here = part.id == item.id;
        let (mark, note) = if store.is_mine(part) {
            let status = store.status_of(part);
            (standing_mark(p, status.standing, 13.0, p.paper), status.standing.label().to_string())
        } else {
            (state_icon(p, part.my_state, 13.0), part.author.name.clone())
        };
        let mut line = row()
            .key(part.id.clone())
            .w_full()
            .h(30.0)
            .px(9.0)
            .gap(9.0)
            .center_y()
            .radius(7.0)
            .child(mark)
            .child(text(part.reference()).font(mono(12.0, 400.0)).color(p.tertiary).single_line())
            // One weight for every part, so moving through the stack never reflows a title.
            .child(text(part.title.clone()).font(ui(13.0, 400.0)).color(p.text).single_line().truncate().grow(1.0).shrink().min_w(0.0))
            .child(text(if here { "This one".to_string() } else { note }).font(ui(12.0, 400.0)).color(p.tertiary).single_line());
        if here {
            line = line.fill(p.selected);
        } else {
            let id = part.id.clone();
            line = line.role("button").label(format!("Go to {}", part.title)).hover(p.hover).press(p.hover).on_press_tx(move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                s.store.select(Some(id.clone()));
            });
        }
        list = list.child(line);
    }
    Some(col().w_full().gap(8.0).child(caption(p, format!("Stack · {}", stack.name))).child(list))
}

/// The page on the forge that lists a merge request's pipelines or checks.
fn pipeline_url(item: &MergeRequest) -> String {
    let page = if item.kind() == crate::data::forge::Forge::Gitlab { "pipelines" } else { "checks" };
    format!("{}/{page}", item.web_url)
}

/// Where a line of the checklist goes when pressed.
enum Go {
    Pipeline(String),
    OpenThread,
    Request(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Check {
    Ok,
    Bad,
    Wait,
}

/// What stands between one of my merge requests and its merge. A line that names something to
/// look at goes there when pressed.
pub fn checklist(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let store = &s.store;
    let mut lines: Vec<(Check, &str, String, Option<Go>)> = Vec::new();
    let pending: Vec<&str> = item.reviewers.iter().filter(|r| r.state.needs_review() && !r.approved).map(|r| r.person.name.as_str()).collect();
    let approved = item.approved_by.len();
    let asked_changes = item.reviewers.iter().find(|r| r.state == ReviewState::RequestedChanges);
    let approvals = match (asked_changes, pending.as_slice()) {
        (Some(r), _) => (Check::Bad, format!("{approved} approved · {} asked for changes", r.person.name)),
        (None, []) if approved > 0 => (Check::Ok, format!("{approved} approved")),
        (None, []) => (Check::Wait, "nobody asked yet".into()),
        (None, [one]) => (Check::Wait, format!("{approved} approved · waiting on {one}")),
        (None, more) => (Check::Wait, format!("{approved} approved · waiting on {}", more.len())),
    };
    lines.push((approvals.0, "Approvals", approvals.1, None));
    if let Some(pipe) = item.pipeline {
        let check = match pipe {
            PipelineStatus::Success | PipelineStatus::Skipped => Check::Ok,
            PipelineStatus::Failed | PipelineStatus::Canceled => Check::Bad,
            _ => Check::Wait,
        };
        let detail = match (&item.failed_job, pipe) {
            (Some(job), PipelineStatus::Failed) => format!("{job} failed"),
            _ => pipe.label().to_lowercase(),
        };
        lines.push((check, "Pipeline", detail, Some(Go::Pipeline(pipeline_url(item)))));
    }
    if let Some(open) = item.unresolved_threads {
        lines.push(if open > 0 {
            (Check::Bad, "Threads", format!("{open} open"), Some(Go::OpenThread))
        } else {
            (Check::Ok, "Threads", "none open".into(), None)
        });
    }
    match item.merge_status.as_deref() {
        Some("conflict") => lines.push((Check::Bad, "Conflicts", format!("with {}", item.target_branch), None)),
        Some("need_rebase") => lines.push((Check::Bad, "Conflicts", "needs a rebase".into(), None)),
        Some(_) => lines.push((Check::Ok, "Conflicts", "none".into(), None)),
        None => {}
    }
    let all = store.stack_of(item);
    if let Some(below) = all.as_ref().and_then(|(_, parts)| parts.iter().position(|m| m.id == item.id).filter(|i| *i > 0).map(|i| &parts[i - 1])) {
        lines.push((Check::Wait, "Stack", format!("waits for {}", below.reference()), Some(Go::Request(below.id.clone()))));
    }
    let mut list = col().key("checks").w_full().max_w(READING_WIDTH);
    let count = lines.len();
    for (i, (check, title, detail, go)) in lines.into_iter().enumerate() {
        let mark = match check {
            Check::Ok => filled_mark(&I::DISC, &I::SMALL_CHECK, p.green, p.paper, 14.0),
            Check::Bad => icon(&I::EXCLAIM_CIRCLE, 14.0).color(p.red),
            Check::Wait => icon(&I::CLOCK, 14.0).color(p.tertiary),
        };
        let ink = if check == Check::Bad { p.red } else { p.secondary };
        let mut line = row()
            .key(format!("check-{}", title.to_lowercase()))
            .h(32.0)
            .gap(10.0)
            .center_y()
            .child(mark)
            .child(text(title).font(ui(13.0, MEDIUM)).color(p.text).single_line())
            .child(text(detail.clone()).font(ui(13.0, 400.0)).color(ink).group_color(p.text).single_line().truncate().shrink().min_w(0.0));
        if let Some(go) = go {
            line = line
                .role("button")
                .label(format!("{title}: {detail}"))
                .hover_group()
                .child(icon(&I::CHEVRON_RIGHT, 10.0).color(p.tertiary).group_color(p.text))
                .on_press_tx(move |s: &mut App, tx: &mut Tx| match &go {
                    Go::Pipeline(url) => {
                        let _ = tx.open_url(url);
                    }
                    Go::OpenThread => crate::ui::detail::jump_to_open_thread(s, tx),
                    Go::Request(id) => {
                        tx.instant();
                        s.store.select(Some(id.clone()));
                    }
                });
        }
        list = list.child(line);
        if i + 1 < count {
            list = list.child(crate::ui::components::rule(p));
        }
    }
    list
}
