//! A merge request on its card, as one document read top to bottom: the ask (its header), the
//! description, the changes file by file with each line's threads under that line, the
//! conversation (the threads that aren't on a line, and a comment box), and your verdict. One of
//! my own reads the same way, from where it stands to what's left before it merges. It's
//! all one virtualized list in one scroll view, so the page scrolls together while only the rows
//! on screen are built: a diff of thousands of lines costs what a screenful does. Beside it, when
//! the card is wide enough, a contents rail lists those parts and every file, follows the reading
//! and jumps.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use crate::ui::smooth_scroll::{smooth_wheel, smooth_wheel_watching};
use rune::text::Span;
use rune::{El, GrowthSpec, PointerPress, PopupAnchor, PopupItem, PopupMenu, ScrollSpec, Tx, col, el, icon, rich, row, text};
use rune_kit::{Kit, Spinner, TextArea};

use crate::app::{App, DraftKey};
use crate::data::diff::{DiffLine, LineKind};
use crate::data::models::{Discussion, FileDiff, MergeRequest, Note};
use crate::data::store::{Loadable, Viewed, relative};
use crate::ui::components::{
    action_button, avatar, chip, comment_count, filled_mark, keys, long_date, month_day, pipeline_dot, progress_bar, rule, state_icon,
};
use crate::ui::diff;
use crate::ui::icons as I;
use crate::ui::markdown::{self, Doc};
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, READING_WIDTH, SEMIBOLD, mono, serif, ui};
use crate::ui::{Look, actions, look};

/// Files longer than this wait for a click before their lines are drawn.
pub const LARGE_FILE: usize = 400;
const PAGE_WIDTH: f32 = 1100.0;
/// The contents rail's width, the room between it and the page, and the card's inset around both.
const RAIL_WIDTH: f32 = 156.0;
const RAIL_GAP: f32 = 20.0;
const RAIL_INSET: f32 = 24.0;
/// The narrowest the page's column gets beside the rail; a narrower card goes without the rail.
const COLUMN_MIN: f32 = 600.0;
/// Where the page and the rail start: the room above the first row.
const PAGE_TOP: f32 = 34.0;

/// A part of the page, in the rail's order. Each file is a part of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Part {
    #[default]
    Ask,
    Description,
    Changes,
    File(usize),
    Conversation,
    Verdict,
}

impl Part {
    /// The part `by` places on, in a page listing `files` files, stopping at either end.
    pub fn step(self, by: i64, files: usize) -> Part {
        let index = match self {
            Part::Ask => 0,
            Part::Description => 1,
            Part::Changes => 2,
            Part::File(f) => 3 + f.min(files.saturating_sub(1)),
            Part::Conversation => 3 + files,
            Part::Verdict => 4 + files,
        };
        match (index as i64 + by).clamp(0, 4 + files as i64) as usize {
            0 => Part::Ask,
            1 => Part::Description,
            2 => Part::Changes,
            i if i < 3 + files => Part::File(i - 3),
            i if i == 3 + files => Part::Conversation,
            _ => Part::Verdict,
        }
    }
}

/// Where the reader is on a merge request's page, from what its scroll view shows.
#[derive(Clone, Debug, Default)]
pub struct Spy {
    /// The part the first row on screen belongs to.
    top: Part,
    /// The page shows its end.
    bottom: bool,
    /// Where the last jump went, kept while the page's end stops that part reaching the top.
    aimed: Option<Part>,
    pub current: Part,
}

impl Spy {
    /// Works out the current part again; true when it changed.
    fn settle(&mut self) -> bool {
        let read = if self.bottom && self.top != Part::Ask { Part::Verdict } else { self.top };
        let current = self.aimed.unwrap_or(read);
        std::mem::replace(&mut self.current, current) != current
    }
}

/// The part of the page `item` shows now.
pub fn current_part(s: &App, item: &str) -> Part {
    s.spies.get(item).map_or(Part::Ask, |spy| spy.current)
}

/// One row of the page.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Header,
    Description,
    /// The changes' or the conversation's title and the line under it.
    Heading(Part),
    Loading(Part),
    Failed(Part, String),
    Empty(&'static str),
    FileList,
    FileHeader(usize),
    Notice(usize, String, bool),
    Top(usize),
    Line(usize, usize),
    Footer(usize),
    Thread(usize),
    Composer,
    Verdict,
}

impl Row {
    fn part(&self) -> Part {
        match self {
            Row::Header => Part::Ask,
            Row::Description => Part::Description,
            Row::Heading(part) | Row::Loading(part) | Row::Failed(part, _) => *part,
            Row::Empty(_) | Row::FileList => Part::Changes,
            Row::FileHeader(f) | Row::Notice(f, ..) | Row::Top(f) | Row::Line(f, _) | Row::Footer(f) => Part::File(*f),
            Row::Thread(_) | Row::Composer => Part::Conversation,
            Row::Verdict => Part::Verdict,
        }
    }

    fn key(&self, files: &[FileDiff], threads: &[Discussion]) -> String {
        match self {
            Row::Header => "header".into(),
            Row::Description => "description".into(),
            Row::Heading(part) => section_key(*part).into(),
            Row::Loading(part) => format!("loading-{}", section_key(*part)),
            Row::Failed(part, _) => format!("failed-{}", section_key(*part)),
            Row::Empty(_) => "empty".into(),
            Row::FileList => "files".into(),
            Row::FileHeader(f) => format!("file:{}", files[*f].id()),
            Row::Notice(f, ..) => format!("notice:{}", files[*f].id()),
            Row::Top(f) => format!("top:{}", files[*f].id()),
            Row::Line(f, l) => format!("line:{f}:{l}"),
            Row::Footer(f) => format!("footer:{}", files[*f].id()),
            Row::Thread(t) => format!("thread:{}", threads[*t].id),
            Row::Composer => "composer".into(),
            Row::Verdict => "verdict".into(),
        }
    }
}

/// The key of the row a part (other than a file) starts on.
fn section_key(part: Part) -> &'static str {
    match part {
        Part::Ask => "header",
        Part::Description => "description",
        Part::Changes | Part::File(_) => "changes",
        Part::Conversation => "conversation",
        Part::Verdict => "verdict",
    }
}

/// Everything a row needs that isn't on `App`, worked out once per frame.
struct Page {
    item: MergeRequest,
    rows: Rc<Vec<Row>>,
    files: Rc<Vec<FileDiff>>,
    /// The changes arrived (possibly with no files), and the threads.
    changes: bool,
    talk: bool,
    threads: Rc<Vec<Discussion>>,
    placed: Rc<Placed>,
    look: Rc<Look>,
    /// The contents rail shows beside the page.
    rail: bool,
    /// I opened it: it starts with where it stands and ends with what's left before it merges.
    mine: bool,
}

/// How far in from the card's left edge the contents rail starts, or `None` when the card is too
/// narrow for it. On a very wide card the rail and the page centre together.
pub fn rail_inset([_, _, w, _]: [f32; 4]) -> Option<f32> {
    let column = w - 2.0 - RAIL_INSET - RAIL_WIDTH - RAIL_GAP - RAIL_INSET;
    (column >= COLUMN_MIN).then(|| RAIL_INSET + ((column - PAGE_WIDTH) / 2.0).max(0.0).floor())
}

/// The page of `item`, with the contents rail `rail` points in from the card's edge if it shows.
pub fn detail(s: &App, p: &Pal, _k: &Kit, item: &MergeRequest, rail: Option<f32>) -> El {
    let look = look(p.dark);
    let store = &s.store;
    let diffs = store.diffs.get(&item.id);
    let talk = store.discussions.get(&item.id);
    let files: Rc<Vec<FileDiff>> = diffs.and_then(Loadable::value).cloned().unwrap_or_default();
    let threads: Rc<Vec<Discussion>> = talk.and_then(Loadable::value).cloned().unwrap_or_default();
    let head = store.diff_refs.get(&item.id).map(|r| r.head_sha.clone());
    let made = page_rows(s, item, diffs, talk, head, rail.is_some());
    let count = made.rows.len();
    let rows = made.rows.clone();
    let page = Rc::new(Page {
        item: item.clone(),
        rows: made.rows,
        files,
        changes: matches!(diffs, Some(Loadable::Loaded(_))),
        talk: matches!(talk, Some(Loadable::Loaded(_))),
        threads,
        placed: made.placed,
        look,
        rail: rail.is_some(),
        mine: store.is_mine(item),
    });
    let key_list = made.keys;
    let built = page.clone();
    let (seen, bottomed) = (item.id.clone(), item.id.clone());
    let list = col()
        .key("page-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows_version(&(item.id.as_str(), made.version))
        .virtual_rows(count, 22.0, move |_: &App, i| Cow::Owned(key_list[i].clone()), move |s: &App, i| build_row(s, &built, i))
        .on_visible_rows(move |s: &mut App, range: Range<usize>| {
            let spy = s.spies.entry(seen.clone()).or_default();
            spy.top = rows.get(range.start).map_or(Part::Ask, Row::part);
            if let Some(aim) = spy.aimed {
                let first = rows.partition_point(|r| r.part() < aim);
                if !(spy.bottom && range.contains(&first)) {
                    spy.aimed = None;
                }
            }
            spy.settle()
        });
    let key = format!("page-{}", item.id);
    let scroll = col().key(key.clone()).h_full().min_h(0.0).child(list).scroll(ScrollSpec { stick_bottom: false }).on_at_bottom(move |s: &mut App, at| {
        let spy = s.spies.entry(bottomed.clone()).or_default();
        spy.bottom = at;
        spy.settle();
    });
    let scroll = match &s.bench {
        Some(_) => smooth_wheel_watching(scroll, &key, crate::bench::note_scroll),
        None => smooth_wheel(scroll, &key),
    };
    match rail {
        Some(inset) => row().key("review").w_full().h_full().min_h(0.0).child(contents(s, p, &page, inset)).child(scroll.grow(1.0).min_w(0.0)),
        None => scroll.w_full(),
    }
}

/// The page's rows and their keys, and where its threads go.
#[derive(Clone)]
struct PageRows {
    rows: Rc<Vec<Row>>,
    keys: Rc<Vec<String>>,
    placed: Rc<Placed>,
    /// Counts the times the rows were worked out, so the list re-reads its keys only then.
    version: u64,
}

/// What a page's rows are made of. Changes and threads compare by pointer: the store shares them.
struct Inputs {
    item: String,
    diffs: Option<Loadable<Rc<Vec<FileDiff>>>>,
    talk: Option<Loadable<Rc<Vec<Discussion>>>>,
    head: Option<String>,
    /// How each file is folded: 0 open, 1 collapsed, 2 a large file expanded.
    folds: Vec<u8>,
    rail: bool,
}

impl Inputs {
    fn same(&self, other: &Inputs) -> bool {
        fn same<T>(a: &Option<Loadable<Rc<T>>>, b: &Option<Loadable<Rc<T>>>) -> bool {
            match (a, b) {
                (Some(Loadable::Loaded(a)), Some(Loadable::Loaded(b))) => Rc::ptr_eq(a, b),
                (Some(Loadable::Failed(a)), Some(Loadable::Failed(b))) => a == b,
                (None | Some(Loadable::Idle | Loadable::Loading), None | Some(Loadable::Idle | Loadable::Loading)) => true,
                _ => false,
            }
        }
        self.item == other.item
            && same(&self.diffs, &other.diffs)
            && same(&self.talk, &other.talk)
            && self.head == other.head
            && self.folds == other.folds
            && self.rail == other.rail
    }
}

thread_local! {
    /// The last page's rows, kept while what they're made of stays the same: a big diff has tens
    /// of thousands of rows, too many to list again every frame.
    static PAGE_ROWS: RefCell<Option<(Inputs, PageRows)>> = const { RefCell::new(None) };
    static VERSION: Cell<u64> = const { Cell::new(0) };
}

fn page_rows(
    s: &App,
    item: &MergeRequest,
    diffs: Option<&Loadable<Rc<Vec<FileDiff>>>>,
    talk: Option<&Loadable<Rc<Vec<Discussion>>>>,
    head: Option<String>,
    rail: bool,
) -> PageRows {
    let files: &[FileDiff] = diffs.and_then(Loadable::value).map_or(&[], |f| f.as_slice());
    let folds = files
        .iter()
        .map(|file| {
            if s.is_collapsed(item, &file.id()) {
                1
            } else if s.expanded_large.contains(&format!("{}|{}", item.id, file.id())) {
                2
            } else {
                0
            }
        })
        .collect();
    let inputs = Inputs { item: item.id.clone(), diffs: diffs.cloned(), talk: talk.cloned(), head, folds, rail };
    let cached = PAGE_ROWS.with(|c| c.borrow().as_ref().and_then(|(was, rows)| was.same(&inputs).then(|| rows.clone())));
    if let Some(rows) = cached {
        return rows;
    }
    let threads: &[Discussion] = talk.and_then(Loadable::value).map_or(&[], |t| t.as_slice());
    let placed = Placed::new(threads, inputs.head.as_deref(), files);
    let mut rows = vec![Row::Header, Row::Description, Row::Heading(Part::Changes)];
    match diffs {
        Some(Loadable::Loaded(_)) if files.is_empty() => rows.push(Row::Empty("No changes.")),
        Some(Loadable::Loaded(_)) => {
            if !rail {
                rows.push(Row::FileList);
            }
            for (f, file) in files.iter().enumerate() {
                rows.push(Row::FileHeader(f));
                if inputs.folds[f] == 1 {
                    continue;
                }
                if file.too_large || (file.diff.is_empty() && file.collapsed) {
                    rows.push(Row::Notice(f, format!("This diff is too large to show here. Open it in {}.", item.kind().name()), false));
                } else if file.lines.is_empty() {
                    let text = if file.renamed_file { "Renamed without changes." } else { "No changes to show (binary or mode change)." };
                    rows.push(Row::Notice(f, text.into(), false));
                } else if (file.lines.len() > LARGE_FILE || file.generated_file) && inputs.folds[f] != 2 {
                    rows.push(Row::Notice(f, format!("{} lines. Show the diff", file.lines.len()), true));
                } else {
                    rows.push(Row::Top(f));
                    rows.extend((0..file.lines.len()).map(|l| Row::Line(f, l)));
                }
                rows.push(Row::Footer(f));
            }
        }
        Some(Loadable::Failed(e)) => rows.push(Row::Failed(Part::Changes, e.clone())),
        _ => rows.push(Row::Loading(Part::Changes)),
    }
    rows.push(Row::Heading(Part::Conversation));
    match talk {
        Some(Loadable::Loaded(_)) => {
            rows.extend(placed.general.iter().map(|&t| Row::Thread(t)));
            rows.push(Row::Composer);
        }
        Some(Loadable::Failed(e)) => rows.push(Row::Failed(Part::Conversation, e.clone())),
        _ => rows.push(Row::Loading(Part::Conversation)),
    }
    rows.push(Row::Verdict);
    let keys = rows.iter().map(|r| r.key(files, threads)).collect();
    let version = VERSION.with(|v| {
        v.set(v.get() + 1);
        v.get()
    });
    let made = PageRows { rows: Rc::new(rows), keys: Rc::new(keys), placed: Rc::new(placed), version };
    PAGE_ROWS.with(|c| *c.borrow_mut() = Some((inputs, made.clone())));
    made
}

/// Scrolls the selected merge request's page so `part` starts at the top of the card, unfolding
/// a file first.
pub fn jump(s: &mut App, tx: &mut Tx, part: Part) {
    let Some(item) = s.store.selected().cloned() else { return };
    let files = s.store.diffs.get(&item.id).and_then(Loadable::value).cloned().unwrap_or_default();
    let (part, key) = match part {
        Part::File(f) if f < files.len() => {
            let id = files[f].id();
            s.collapsed.remove(&format!("{}|{id}", item.id));
            (part, format!("file:{id}"))
        }
        Part::File(_) => (Part::Changes, section_key(Part::Changes).to_string()),
        _ => (part, section_key(part).to_string()),
    };
    tx.instant();
    // A row far down a big diff isn't built yet: revealing it builds it, then it lands on top.
    tx.scroll_into_view_in("page-rows", key.clone());
    tx.scroll_key_to(format!("page-rows/{key}"), 0.0);
    // The rail may be scrolled itself, with a long list of files.
    tx.scroll_into_view_in("contents", rail_key(part));
    let spy = s.spies.entry(item.id).or_default();
    spy.aimed = Some(part);
    spy.settle();
}

/// The key of a part's row in the rail.
fn rail_key(part: Part) -> String {
    match part {
        Part::File(f) => format!("rail-file-{f}"),
        _ => format!("rail-{}", if part == Part::Ask { "ask" } else { section_key(part) }),
    }
}

/// J and K: the next or previous part of the page, each file a part. False with nothing open.
pub fn step(s: &mut App, tx: &mut Tx, by: i64) -> bool {
    let Some(id) = s.store.selected_id.clone() else { return false };
    let files = s.store.diffs.get(&id).and_then(Loadable::value).map_or(0, |f| f.len());
    let to = current_part(s, &id).step(by, files);
    jump(s, tx, to);
    true
}

/// Shows the comment box at the end of the conversation with the caret in it.
pub fn compose(tx: &mut Tx, id: &str) {
    tx.instant();
    tx.scroll_into_view_in("page-rows", "composer");
    tx.focus(draft_field_key(&DraftKey::Comment(id.to_string())), false);
}

/// Centres a row's content in the page's column, with the page's margins; beside the rail, the
/// column starts at the rail's gap instead.
fn frame(page: &Page, i: usize, content: El) -> El {
    let last = i + 1 == page.rows.len();
    let mut outer = if page.rail { row().w_full().pl(RAIL_GAP).pr(RAIL_INSET) } else { row().w_full().px(44.0).justify_center() };
    if i == 0 {
        outer = outer.pt(PAGE_TOP);
    }
    if last {
        outer = outer.pb(60.0);
    }
    outer.child(col().w_full().max_w(PAGE_WIDTH).child(content))
}

fn build_row(s: &App, page: &Page, i: usize) -> El {
    let p = &page.look.p;
    let k = &*page.look.k;
    let item = &page.item;
    // The first thing under a heading sits closer to it than to what came before.
    let first = i > 0 && matches!(page.rows[i - 1], Row::Heading(_));
    let content = match &page.rows[i] {
        Row::Header => header(s, p, item, !page.rail),
        Row::Description => {
            let body = if item.description.trim().is_empty() {
                text("No description.").font(serif(16.0, 400.0).italic()).color(p.tertiary)
            } else {
                markdown::markdown(s, p, k, Doc { key: &item.id, source: &item.description, base: item.project_url(), size: 16.0, serif: true })
            };
            el().w_full().max_w(READING_WIDTH).pt(30.0).child(body)
        }
        Row::Heading(Part::Changes) => {
            let n = page.files.len();
            let lede = match n {
                0 => None,
                1 => Some("1 file. Mark it viewed when you finish it.".to_string()),
                n => Some(format!("{n} files. Mark each one as you finish it.")),
            };
            heading(p, "The changes", lede)
        }
        Row::Heading(_) => {
            heading(p, "Conversation", Some(format!("Comments on the {} as a whole. Comments on a line are under that line.", item.kind().noun())))
        }
        Row::Loading(part) => {
            row().gap(8.0).center_y().child(Spinner::new().size(14.0).build(k)).child(
                text(format!("Loading the {}…", if *part == Part::Changes { "changes" } else { "conversation" })).font(ui(13.0, 400.0)).color(p.secondary),
            )
        }
        Row::Failed(part, message) => row().gap(6.0).items_start().child(icon(&I::WARNING, 13.0).color(p.red)).max_w(READING_WIDTH).child(
            text(format!("Couldn't load the {}: {message}", if *part == Part::Changes { "changes" } else { "conversation" }))
                .font(ui(13.0, 400.0))
                .color(p.red)
                .wrap()
                .grow(1.0)
                .shrink()
                .min_w(0.0),
        ),
        Row::Empty(message) => el().child(text(*message).font(ui(13.0, 400.0)).color(p.tertiary)),
        Row::FileList => file_list(s, p, page),
        Row::FileHeader(f) => file_header(s, p, page, *f),
        Row::Notice(f, message, expands) => notice(p, page, *f, message, *expands),
        Row::Top(_) => el().w_full().h(4.0).fill(p.paper).border_sides([0.0, 1.0, 0.0, 1.0], p.strong_line),
        Row::Line(f, l) => line_row(s, p, k, page, *f, *l),
        Row::Footer(_) => {
            el().w_full().pb(14.0).child(el().w_full().h(10.0).fill(p.paper).corners([0.0, 0.0, 10.0, 10.0]).border_sides([0.0, 1.0, 1.0, 1.0], p.strong_line))
        }
        Row::Thread(t) => {
            let thread = &page.threads[*t];
            el().w_full().max_w(READING_WIDTH).pt(if first { 0.0 } else { 16.0 }).child(thread_card(s, p, k, item, thread, true))
        }
        Row::Composer => el().w_full().max_w(READING_WIDTH).pt(if first { 0.0 } else { 24.0 }).child(composer(
            s,
            p,
            k,
            DraftKey::Comment(item.id.clone()),
            "Leave a comment. Markdown works.",
            "Comment",
            false,
        )),
        Row::Verdict if page.mine => before_it_merges(s, p, item),
        Row::Verdict => verdict(s, p, page),
    };
    frame(page, i, content)
}

/// A part's title over a rule, and a line saying what's in it.
fn heading(p: &Pal, title: &str, lede: Option<String>) -> El {
    let mut c = col()
        .w_full()
        .pt(44.0)
        .pb(18.0)
        .child(rule(p))
        .child(text(title.to_string()).font(serif(22.0, SEMIBOLD).lines(1.25)).color(p.text).pt(22.0).selectable(true));
    if let Some(lede) = lede {
        c = c.child(text(lede).font(ui(13.0, 400.0).lines(1.5)).color(p.secondary).wrap().max_w(READING_WIDTH).pt(6.0));
    }
    c
}

/// Your verdict: where the review stands in one sentence, then Approve.
fn verdict(s: &App, p: &Pal, page: &Page) -> El {
    let item = &page.item;
    let open = page.threads.iter().filter(|t| is_open(t)).count();
    let viewed = page.files.iter().filter(|f| s.store.viewed(f, item) == Viewed::Yes).count();
    let left = page.files.len() - viewed;
    let files = match left {
        _ if !page.changes || page.files.is_empty() => None,
        0 => Some("You’ve viewed every file".to_string()),
        1 => Some("1 file still to view".to_string()),
        n => Some(format!("{n} files still to view")),
    };
    let threads = match open {
        _ if !page.talk => None,
        0 => Some("no threads are open".to_string()),
        1 => Some("1 thread is open".to_string()),
        n => Some(format!("{n} threads are open")),
    };
    let sentence = match (files, threads) {
        (Some(files), Some(threads)) => format!("{files}, and {threads}."),
        (Some(files), None) => format!("{files}."),
        (None, Some(threads)) => format!("{}{}.", threads[..1].to_uppercase(), &threads[1..]),
        (None, None) => "There’s nothing to sum up yet.".to_string(),
    };
    let mut c = col()
        .w_full()
        .max_w(READING_WIDTH)
        .gap(14.0)
        .child(heading(p, "Your verdict", None).pb(0.0))
        .child(text(sentence).font(serif(16.0, 400.0).lines(1.6)).color(p.text).wrap().selectable(true));
    if !item.is_open() {
        let state = if item.state == "merged" { "was merged" } else { "is closed" };
        return c.child(text(format!("This {} {state}, so there’s nothing to approve.", item.kind().noun())).font(ui(13.0, 400.0)).color(p.secondary));
    }
    let mut actions = row().gap(8.0).center_y().child(approval_button(s, p, item, true));
    if let Some(busy) = &s.store.busy {
        actions =
            actions.child(Spinner::new().size(14.0).build(&p.kit()).ml(4.0)).child(text(busy.clone()).font(ui(12.0, 400.0)).color(p.secondary).single_line());
    }
    if i_approved(s, item) {
        c = c.child(
            row()
                .gap(6.0)
                .center_y()
                .child(filled_mark(&I::DISC, &I::SMALL_CHECK, p.green, p.paper, 14.0))
                .child(text("You approved this.").font(ui(13.0, MEDIUM)).color(p.green)),
        );
    }
    c.child(actions)
}

/// The end of a request of my own: what stands between it and its merge, or how it ended.
fn before_it_merges(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let c = col().w_full().max_w(READING_WIDTH).gap(14.0);
    if item.is_open() {
        return c.child(heading(p, "Before it merges", None).pb(0.0)).child(crate::ui::mine::checklist(s, p, item));
    }
    let end = if item.state == "merged" { "was merged" } else { "was closed without merging" };
    c.child(heading(p, "How it ended", None).pb(0.0))
        .child(text(format!("This {} {end}.", item.kind().noun())).font(serif(16.0, 400.0).lines(1.6)).color(p.text).wrap().selectable(true))
}

/// Jumps to the first thread someone can still resolve: the file it's on, or the conversation.
pub fn jump_to_open_thread(s: &mut App, tx: &mut Tx) {
    let Some(id) = s.store.selected_id.clone() else { return };
    let files = s.store.diffs.get(&id).and_then(Loadable::value).cloned().unwrap_or_default();
    let threads = s.store.discussions.get(&id).and_then(Loadable::value).cloned().unwrap_or_default();
    let head = s.store.diff_refs.get(&id).map(|r| r.head_sha.as_str());
    let placed = Placed::new(&threads, head, &files);
    let file = placed.by_file.iter().position(|on| on.iter().any(|&t| is_open(&threads[t])));
    jump(s, tx, file.map_or(Part::Conversation, Part::File));
}

/// A thread someone can still resolve.
fn is_open(thread: &Discussion) -> bool {
    let notes: Vec<&Note> = thread.notes.iter().filter(|n| !n.system).collect();
    notes.first().and_then(|n| n.resolvable) == Some(true) && !notes.iter().all(|n| n.resolved == Some(true))
}

/// Approve (the accent one in the verdict, `prominent`), or Revoke Approval once approved.
fn approval_button(s: &App, p: &Pal, item: &MergeRequest, prominent: bool) -> El {
    let idle = s.store.busy.is_none();
    if i_approved(s, item) {
        let it = item.clone();
        action_button(p, "revoke", Some(&I::UNDO), "Revoke Approval", false, idle).on_press(move |s: &mut App| {
            if s.store.busy.is_none() {
                s.store.unapprove(&it)
            }
        })
    } else {
        action_button(p, "approve", Some(&I::CHECK), "Approve", prominent, idle).help(keys("Approve  ⇧⌘A")).on_press_tx(actions::approve)
    }
}

// MARK: Contents

/// The rail of contents beside the page: its parts and every file, the one being read marked
/// with a clay edge; each row jumps there.
fn contents(s: &App, p: &Pal, page: &Page, inset: f32) -> El {
    let item = &page.item;
    let current = current_part(s, &item.id);
    let files = &page.files;
    let mut c = col().w_full().pl(inset).pt(PAGE_TOP).pb(24.0).child(text("Contents").font(ui(12.0, MEDIUM)).color(p.secondary).pl(12.0).pb(8.0));
    let label = |title: &str, part: Part| {
        text(title.to_string()).font(ui(13.0, 400.0)).color(if part == current { p.text } else { p.secondary }).group_color(p.text).single_line().truncate()
    };
    // My own starts with where it stands (its mark here too) and ends with what's left.
    let (first, last, standing) = match (page.mine, item.is_open()) {
        (true, true) => {
            let standing = s.store.status_of(item).standing;
            ("Where it stands", "Before it merges", vec![crate::ui::mine::standing_mark(p, standing, 12.0, p.paper)])
        }
        (true, false) => ("Where it stands", "How it ended", vec![]),
        (false, _) => ("The ask", "Your verdict", vec![]),
    };
    c = c.child(rail_row(p, &rail_key(Part::Ask), Part::Ask, current, 10.0, label(first, Part::Ask), standing));
    c = c.child(rail_row(p, &rail_key(Part::Description), Part::Description, current, 10.0, label("Description", Part::Description), vec![]));
    let mut tally = vec![];
    if page.changes && !files.is_empty() {
        let viewed = files.iter().filter(|f| s.store.viewed(f, item) == Viewed::Yes).count();
        tally.push(text(format!("{viewed} of {}", files.len())).font(ui(12.0, 400.0).features("'tnum' 1")).color(p.secondary).single_line());
    }
    c = c.child(rail_row(p, &rail_key(Part::Changes), Part::Changes, current, 10.0, label("The changes", Part::Changes), tally));
    for (f, file) in files.iter().enumerate() {
        let part = Part::File(f);
        let state = s.store.viewed(file, item);
        let name = file.new_path.rsplit('/').next().unwrap_or(&file.new_path).to_string();
        let tint = if part == current {
            p.text
        } else if state == Viewed::Yes {
            p.tertiary
        } else {
            p.secondary
        };
        let threads = &page.placed.by_file[f];
        let fresh = threads.iter().any(|&t| s.store.has_fresh(item, &page.threads[t]));
        let name = text(name).font(mono(12.0, 400.0)).color(tint).group_color(p.text).single_line().truncate();
        c = c.child(
            rail_row(
                p,
                &rail_key(part),
                part,
                current,
                20.0,
                name,
                comment_count(p, threads.len(), fresh).into_iter().chain([viewed_mark(p, state, p.paper)]).collect(),
            )
            .label(file.new_path.clone())
            .help(file.new_path.clone()),
        );
    }
    let general = &page.placed.general;
    let fresh = general.iter().any(|&t| s.store.has_fresh(item, &page.threads[t]));
    c = c.child(rail_row(
        p,
        &rail_key(Part::Conversation),
        Part::Conversation,
        current,
        10.0,
        label("Conversation", Part::Conversation),
        comment_count(p, general.len(), fresh).into_iter().collect(),
    ));
    c = c.child(rail_row(p, &rail_key(Part::Verdict), Part::Verdict, current, 10.0, label(last, Part::Verdict), vec![]));
    // Nobody approves their own.
    if item.is_open() && !page.mine {
        c = c.child(el().pt(18.0).pl(12.0).child(approval_button(s, p, item, false).key("rail-approve")));
    }
    col().key("contents").role("navigation").label("Contents").w(RAIL_WIDTH + inset).h_full().min_h(0.0).child(c).scroll(ScrollSpec { stick_bottom: false })
}

/// One row of the rail: a 2-point edge (clay on the part being read, kept on every row so
/// nothing shifts), the label `indent` points in, and what's at its end.
fn rail_row(p: &Pal, key: &str, part: Part, current: Part, indent: f32, label: El, end: Vec<El>) -> El {
    let on = part == current;
    let mut edge = el().w(2.0).h(26.0);
    if on {
        edge = edge.fill(p.accent);
    }
    let mut r = row()
        .key(key.to_string())
        .role("button")
        .w_full()
        .h(26.0)
        .pr(10.0)
        .gap(6.0)
        .center_y()
        .hover_group()
        .child(edge)
        .child(label.ml(indent - 6.0).grow(1.0).shrink().min_w(0.0));
    for e in end {
        r = r.child(e);
    }
    r.on_press_tx(move |s: &mut App, tx: &mut Tx| jump(s, tx, part))
}

/// Viewed, changed since, or not yet, its marks knocked out in `ink` (the surface under them).
fn viewed_mark(p: &Pal, state: Viewed, ink: rune::Color) -> El {
    match state {
        Viewed::Yes => filled_mark(&I::DISC, &I::SMALL_CHECK, p.green, ink, 12.0).help("Viewed"),
        Viewed::Changed => filled_mark(&I::DISC, &I::DOT, p.amber, ink, 12.0).help("Changed since you viewed it"),
        Viewed::No => icon(&I::PLAIN_CIRCLE, 12.0).color(p.line).help("Not viewed yet"),
    }
}

// MARK: Header

fn header(s: &App, p: &Pal, item: &MergeRequest, with_actions: bool) -> El {
    let mut top = row()
        .gap(8.0)
        .center_y()
        .child(text(item.project_path.clone()).font(mono(12.0, 400.0)).color(p.tertiary).single_line().truncate().shrink().min_w(0.0))
        .child(text(format!("{}{}", item.kind().reference_mark(), item.iid)).font(mono(12.0, MEDIUM)).color(p.secondary).single_line());
    if item.draft {
        top = top.child(chip(p, None, "Draft", p.secondary, None, false));
    }
    if item.state == "merged" {
        top = top.child(chip(p, Some(&I::MERGE), "Merged", p.blue, None, false));
    }
    if item.state == "closed" {
        top = top.child(chip(p, Some(&I::CLOSE), "Closed", p.red, None, false));
    }
    let title = text(item.title.clone()).font(serif(27.0, SEMIBOLD).lines(1.3)).color(p.text).wrap().selectable(true).w_full().max_w(READING_WIDTH);
    let mine = s.store.is_mine(item);
    let mut c = col()
        .w_full()
        .gap(14.0)
        .child(top)
        .child(title)
        .child(if mine { opened_line(s, p, item) } else { request_line(s, p, item) })
        .child(chips(p, item, mine));
    if mine && item.is_open() {
        c = c.child(crate::ui::mine::next_step(s, p, item).mt(4.0));
    }
    if let Some(stack) = crate::ui::mine::stack_section(s, p, item) {
        c = c.child(stack.pt(2.0));
    }
    c = c.child(reviewers(s, p, item));
    // An open request of mine has its actions in the next step, above.
    if !with_actions || (mine && item.is_open()) { c } else { c.child(action_bar(s, p, item).pt(4.0)) }
}

/// "You opened this 2 days ago", for a merge request of my own.
fn opened_line(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let line = format!("You opened this {}", relative(item.created_at));
    row()
        .gap(8.0)
        .center_y()
        .child(avatar(s, p, &item.author, 22.0))
        .child(rich(line, vec![Span::new(0, 3).weight(SEMIBOLD).color(p.text)]).font(ui(13.0, 400.0)).color(p.secondary).help(long_date(item.created_at)))
}

fn request_line(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let asker = item.requested_by.clone().unwrap_or_else(|| item.author.clone());
    let (name, rest) = if item.requested_at_is_guess {
        (item.author.name.clone(), format!(" opened this {}", relative(item.created_at)))
    } else {
        let verb = if item.was_re_requested() { " re-requested your review " } else { " requested your review " };
        (asker.name.clone(), format!("{verb}{}", relative(item.requested_at)))
    };
    let line = format!("{name}{rest}");
    let spans = vec![Span::new(0, name.len()).weight(SEMIBOLD).color(p.text)];
    let mut r = row()
        .gap(8.0)
        .center_y()
        .flex_wrap()
        .child(avatar(s, p, &asker, 22.0))
        .child(rich(line, spans).font(ui(13.0, 400.0)).color(p.secondary).help(long_date(item.requested_at)));
    if item.requested_by.as_ref().map(|r| &r.username) != Some(&item.author.username) {
        r = r.child(text(format!("· by {}", item.author.name)).font(ui(13.0, 400.0)).color(p.tertiary));
    }
    if item.was_re_requested() {
        r = r.child(text(format!("· first asked {}", month_day(item.first_requested_at))).font(ui(13.0, 400.0)).color(p.tertiary));
    }
    r
}

fn chips(p: &Pal, item: &MergeRequest, mine: bool) -> El {
    let branch = item.source_branch.clone();
    let mut r = row().w_full().gap(6.0).flex_wrap().child(
        chip(p, Some(&I::BRANCH), &format!("{} → {}", item.source_branch, item.target_branch), p.secondary, None, true)
            .key("branch")
            .role("button")
            .help("Click to copy the source branch")
            .hover(p.hover)
            .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                tx.copy(branch.clone());
                s.store.show(&format!("Copied {branch}"));
            }),
    );
    let files = if item.file_count == 1 { "file" } else { "files" };
    // The same green and red as the file list, so the totals read as the sum of its rows.
    r = r.child(
        row()
            .h(22.0)
            .px(8.0)
            .gap(6.0)
            .center_y()
            .radius(6.0)
            .fill(p.hover)
            .child(text(format!("+{}", item.additions)).font(mono(12.0, 400.0)).color(p.green).single_line())
            .child(text(format!("−{}", item.deletions)).font(mono(12.0, 400.0)).color(p.red).single_line())
            .child(text(format!("· {} {files}", item.file_count)).font(mono(12.0, 400.0)).color(p.secondary).single_line()),
    );
    if let Some(pipe) = item.pipeline {
        r = r.child(
            row()
                .h(22.0)
                .px(8.0)
                .gap(6.0)
                .center_y()
                .radius(6.0)
                .fill(p.hover)
                .child(pipeline_dot(p, pipe))
                .child(text(format!("Pipeline {}", pipe.label().to_lowercase())).font(ui(12.0, MEDIUM)).color(p.secondary).single_line()),
        );
    }
    // On my own, the next step says where the merge stands, stack and all.
    if item.is_open() && !mine {
        if let Some(status) = item.merge_status_label() {
            let tint = if item.merge_status.as_deref() == Some("conflict") { p.red } else { p.secondary };
            r = r.child(chip(p, None, status, tint, None, false));
        }
    }
    for label in &item.labels {
        let (bg, fg) = p.label_colors(&label.color, &label.text_color);
        r = r.child(chip(p, None, &label.title, fg, Some(bg.alpha(0.9)), false));
    }
    r
}

fn reviewers(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let me = s.store.me().map(|m| m.username.clone());
    let mut r = row().gap(14.0).center_y().flex_wrap().child(text("Reviewers").font(ui(12.0, MEDIUM)).color(p.tertiary));
    for reviewer in &item.reviewers {
        let name = if Some(&reviewer.person.username) == me.as_ref() { "You".to_string() } else { reviewer.person.name.clone() };
        r = r.child(
            row()
                .gap(6.0)
                .center_y()
                .help(reviewer.state.label())
                .child(avatar(s, p, &reviewer.person, 18.0))
                .child(text(name).font(ui(12.0, MEDIUM)).color(p.text).single_line())
                .child(state_icon(p, reviewer.state, 12.0)),
        );
    }
    r
}

fn i_approved(s: &App, item: &MergeRequest) -> bool {
    let Some(me) = s.store.me() else { return false };
    item.approved_by.iter().any(|a| a.eq_ignore_ascii_case(&me.username))
}

fn action_bar(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let mut r = row().gap(8.0).center_y();
    // Nobody approves their own.
    if item.is_open() && !s.store.is_mine(item) {
        r = r.child(approval_button(s, p, item, true));
    }
    for link in link_buttons(p, item) {
        r = r.child(link);
    }
    if let Some(busy) = &s.store.busy {
        r = r.child(Spinner::new().size(14.0).build(&p.kit()).ml(4.0)).child(text(busy.clone()).font(ui(12.0, 400.0)).color(p.secondary).single_line());
    }
    r
}

/// "Open in GitLab" and "Copy Branch", in that order.
pub fn link_buttons(p: &Pal, item: &MergeRequest) -> Vec<El> {
    let url = item.web_url.clone();
    let branch = item.source_branch.clone();
    vec![
        action_button(p, "open-forge", Some(&I::ARROW_UP_RIGHT), &format!("Open in {}", item.kind().name()), false, true).on_press_tx(
            move |_: &mut App, tx: &mut Tx| {
                let _ = tx.open_url(&url);
            },
        ),
        action_button(p, "copy-branch", Some(&I::COPY), "Copy Branch", false, true).on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.copy(branch.clone());
            s.store.show(&format!("Copied {branch}"));
        }),
    ]
}

// MARK: Changes

fn file_list(s: &App, p: &Pal, page: &Page) -> El {
    let item = &page.item;
    let files = &page.files;
    let viewed = files.iter().filter(|f| s.store.viewed(f, item) == Viewed::Yes).count();
    let done = viewed == files.len() && viewed > 0;
    let mut c = col().w_full().py(6.0).radius(10.0).fill(p.well).border(1.0, p.line).child(
        row()
            .h(26.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .child(text(format!("{viewed} of {} viewed", files.len())).font(ui(12.0, MEDIUM)).color(if done { p.green } else { p.secondary }))
            .child(el().grow(1.0))
            .child(progress_bar(p, if files.is_empty() { 0.0 } else { viewed as f32 / files.len() as f32 }, 90.0)),
    );
    for (f, file) in files.iter().enumerate() {
        let state = s.store.viewed(file, item);
        let (glyph, tint) = if file.new_file {
            (&I::DOC_PLUS, p.green)
        } else if file.deleted_file {
            (&I::DOC_MINUS, p.red)
        } else if file.renamed_file {
            (&I::DOC_ARROW, p.tertiary)
        } else {
            (&I::PENCIL_SQUARE, p.tertiary)
        };
        c = c.child(
            row()
                .key(format!("file-entry-{f}"))
                .role("button")
                .label(file.new_path.clone())
                .h(26.0)
                .px(12.0)
                .gap(8.0)
                .center_y()
                .hover(p.hover)
                .child(el().w(14.0).center().child(icon(glyph, 12.0).color(tint)))
                .child(
                    text(file.new_path.clone())
                        .font(mono(12.0, 400.0))
                        .color(if state == Viewed::Yes { p.tertiary } else { p.text })
                        .single_line()
                        .truncate_head()
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0),
                )
                .child(counts(p, file).opacity(if state == Viewed::Yes { 0.6 } else { 1.0 }))
                .child(viewed_mark(p, state, p.well))
                .on_press_tx(move |s: &mut App, tx: &mut Tx| jump(s, tx, Part::File(f))),
        );
    }
    el().w_full().pb(14.0).child(c)
}

fn counts(p: &Pal, file: &FileDiff) -> El {
    let mut r = row().gap(6.0);
    if file.added > 0 {
        r = r.child(text(format!("+{}", file.added)).font(mono(12.0, MEDIUM)).color(p.green).single_line());
    }
    if file.removed > 0 {
        r = r.child(text(format!("−{}", file.removed)).font(mono(12.0, MEDIUM)).color(p.red).single_line());
    }
    r
}

fn file_header(s: &App, p: &Pal, page: &Page, f: usize) -> El {
    let item = &page.item;
    let file = &page.files[f];
    let id = file.id();
    let open = !s.is_collapsed(item, &id);
    let state = s.store.viewed(file, item);
    let mut name = row()
        .key("fold")
        .role("button")
        .label(file.new_path.clone())
        .h(36.0)
        .pl(12.0)
        .pr(10.0)
        .gap(8.0)
        .center_y()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .child(icon(if open { &I::CHEVRON_DOWN } else { &I::CHEVRON_RIGHT }, 11.0).color(p.tertiary));
    if file.renamed_file && file.old_path != file.new_path {
        name = name
            .child(text(file.old_path.clone()).font(mono(12.0, 400.0)).color(p.tertiary).single_line().truncate_head().shrink().min_w(0.0))
            .child(text("→").font(ui(12.0, 400.0)).color(p.tertiary));
    }
    name = name.child(
        text(file.new_path.clone())
            .font(mono(12.0, MEDIUM))
            .color(if state == Viewed::Yes { p.secondary } else { p.text })
            .single_line()
            .truncate_head()
            .shrink()
            .min_w(0.0),
    );
    if file.new_file {
        name = name.child(chip(p, None, "New", p.green, None, false));
    }
    if file.deleted_file {
        name = name.child(chip(p, None, "Deleted", p.red, None, false));
    }
    if file.generated_file {
        name = name.child(chip(p, None, "Generated", p.secondary, None, false));
    }
    name = name.child(el().grow(1.0));
    let threads = &page.placed.by_file[f];
    let fresh = threads.iter().any(|&t| s.store.has_fresh(item, &page.threads[t]));
    if let Some(count) = comment_count(p, threads.len(), fresh) {
        name = name.child(count);
    }
    name = name.child(counts(p, file));
    let mr = item.id.clone();
    let fid = id.clone();
    let name = name.on_press(move |s: &mut App| s.toggle_collapsed(&mr, &fid));
    let path = file.new_path.clone();
    let mut box_ =
        row().key(format!("file-{f}")).w_full().center_y().fill(p.paper).child(name).child(viewed_toggle(p, page, f, state).mr(7.0)).on_secondary_press(
            move |_: &mut App, at: PointerPress, tx: &mut Tx| {
                let path = path.clone();
                tx.popup_menu(PopupMenu::new().item(PopupItem::new("copy", "Copy Path")), PopupAnchor::Point(at.x, at.y), move |s: &mut App, picked, tx| {
                    if picked == Some("copy") {
                        actions::copy(s, tx, path.clone());
                    }
                });
            },
        );
    box_ = if open {
        box_.corners([10.0, 10.0, 0.0, 0.0]).border_sides([1.0, 1.0, 0.0, 1.0], p.strong_line)
    } else {
        box_.radius(10.0).border(1.0, p.strong_line)
    };
    el().w_full().pb(if open { 0.0 } else { 14.0 }).child(box_)
}

fn viewed_toggle(p: &Pal, page: &Page, f: usize, state: Viewed) -> El {
    let item = page.item.clone();
    let file = page.files[f].clone();
    let on = state == Viewed::Yes;
    let mark = if on { filled_mark(&I::ROUNDED_FILL, &I::SMALL_CHECK, p.accent, p.paper, 13.0) } else { icon(&I::SQUARE, 13.0).color(p.tertiary) };
    let mut b = row()
        .key("viewed")
        .role("checkbox")
        .checked(on)
        .label("Viewed")
        .h(24.0)
        .px(7.0)
        .gap(5.0)
        .center_y()
        .radius(6.0)
        .child(mark)
        .child(
            text(if state == Viewed::Changed { "Changed since viewed" } else { "Viewed" })
                .font(ui(12.0, MEDIUM))
                .color(if state == Viewed::Changed { p.amber } else { p.secondary })
                .single_line(),
        )
        .help(if on { "Mark as not viewed" } else { "Mark as viewed and fold the file" })
        .on_press(move |s: &mut App| {
            let now = s.store.viewed(&file, &item) != Viewed::Yes;
            s.store.set_viewed(now, &file, &item);
            let key = format!("{}|{}", item.id, file.id());
            if now {
                s.collapsed.insert(key);
            } else {
                s.collapsed.remove(&key);
            }
        });
    if on {
        b = b.fill(p.accent_soft);
    } else {
        b = b.hover(p.hover);
    }
    b
}

fn notice(p: &Pal, page: &Page, f: usize, message: &str, expands: bool) -> El {
    let mut t = el()
        .w_full()
        .py(14.0)
        .center()
        .fill(p.paper)
        .border_sides([0.0, 1.0, 0.0, 1.0], p.strong_line)
        .child(text(message.to_string()).font(ui(12.0, 400.0)).color(if expands { p.accent } else { p.secondary }));
    if expands {
        let key = format!("{}|{}", page.item.id, page.files[f].id());
        t = t.role("button").hover(p.hover).on_press(move |s: &mut App| {
            s.expanded_large.insert(key.clone());
        });
    }
    t
}

fn line_row(s: &App, p: &Pal, k: &Kit, page: &Page, f: usize, l: usize) -> El {
    let item = &page.item;
    let file = &page.files[f];
    let line = &file.lines[l];
    let commentable = item.is_open() && matches!(line.kind, LineKind::Context | LineKind::Added | LineKind::Removed);
    let styled = diff::line_spans(p, k, file, l);
    let mut c = col().w_full().fill(p.paper).border_sides([0.0, 1.0, 0.0, 1.0], p.strong_line).child(diff::line_view(
        p,
        line,
        styled,
        commentable,
        item.id.clone(),
        file.id(),
        l,
    ));
    for thread in page.placed.on(line, file) {
        c = c.child(el().w_full().px(14.0).py(8.0).fill(p.well).child(thread_card(s, p, k, item, &page.threads[thread], false).max_w(760.0)));
    }
    let key = DraftKey::Line(item.id.clone(), file.id(), l);
    if s.line_comment.as_ref() == Some(&key) {
        let number = line.new_number.or(line.old_number).unwrap_or(0);
        c = c.child(
            el().w_full()
                .px(14.0)
                .py(10.0)
                .fill(p.well)
                .child(composer(s, p, k, key, &format!("Comment on line {number}. Markdown works."), "Comment", true).max_w(760.0)),
        );
    }
    c
}

/// Where each thread goes: under the diff line it's about when a line row shows that line,
/// else in the conversation, so none goes missing (general comments, and positions that are
/// outdated or not in the diff).
#[derive(Default)]
struct Placed {
    by_line: HashMap<String, Vec<usize>>,
    /// The threads under each file's lines.
    by_file: Vec<Vec<usize>>,
    /// The threads for the conversation.
    general: Vec<usize>,
}

impl Placed {
    fn new(threads: &[Discussion], head: Option<&str>, files: &[FileDiff]) -> Placed {
        let mut placed = Placed { by_file: vec![Vec::new(); files.len()], ..Placed::default() };
        for (index, thread) in threads.iter().enumerate() {
            match line_of(thread, head, files) {
                Some((key, f)) => {
                    placed.by_line.entry(key).or_default().push(index);
                    placed.by_file[f].push(index);
                }
                None => placed.general.push(index),
            }
        }
        placed
    }

    fn on(&self, line: &DiffLine, file: &FileDiff) -> Vec<usize> {
        if self.by_line.is_empty() {
            return Vec::new();
        }
        let key = match line.kind {
            LineKind::Added | LineKind::Context => line.new_number.map(|n| format!("n|{}|{n}", file.new_path)),
            LineKind::Removed => line.old_number.map(|n| format!("o|{}|{n}", file.old_path)),
            _ => None,
        };
        key.and_then(|k| self.by_line.get(&k).cloned()).unwrap_or_default()
    }
}

/// The line key `Placed::on` finds a thread under, and its file, when the diff has that line
/// and the thread was left on this version of it.
fn line_of(thread: &Discussion, head: Option<&str>, files: &[FileDiff]) -> Option<(String, usize)> {
    let position = thread.notes.iter().find(|n| !n.system).and_then(|n| n.position.as_ref())?;
    if let (Some(head), Some(at)) = (head, position.head_sha.as_deref()) {
        if at != head {
            return None;
        }
    }
    let (new, path, number) = match (position.new_line, &position.new_path, position.old_line, &position.old_path) {
        (Some(line), Some(path), ..) => (true, path, line),
        (_, _, Some(line), Some(path)) => (false, path, line),
        _ => return None,
    };
    let f = files.iter().position(|f| if new { &f.new_path == path } else { &f.old_path == path })?;
    let shown = files[f].lines.iter().any(|l| match l.kind {
        LineKind::Added | LineKind::Context => new && l.new_number == Some(number),
        LineKind::Removed => !new && l.old_number == Some(number),
        _ => false,
    });
    shown.then(|| (format!("{}|{path}|{number}", if new { "n" } else { "o" }), f))
}

// MARK: Discussion

pub fn thread_card(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, thread: &Discussion, shows_position: bool) -> El {
    let notes: Vec<&Note> = thread.notes.iter().filter(|n| !n.system).collect();
    let resolved = notes.first().and_then(|n| n.resolvable) == Some(true) && notes.iter().all(|n| n.resolved == Some(true));
    let mut c = col().key(format!("thread-{}", thread.id)).w_full().radius(10.0).fill(p.paper).border(1.0, p.strong_line).clip_rounded(10.0);
    if shows_position {
        if let Some(position) = notes.first().and_then(|n| n.position.as_ref()) {
            if let Some(path) = position.new_path.clone().or_else(|| position.old_path.clone()) {
                let at = position.new_line.or(position.old_line).map(|l| format!(":{l}")).unwrap_or_default();
                let mut head = row()
                    .w_full()
                    .px(14.0)
                    .py(8.0)
                    .gap(6.0)
                    .center_y()
                    .fill(p.well)
                    .child(icon(&I::DOC_ARROW, 11.0).color(p.secondary))
                    .child(text(format!("{path}{at}")).font(mono(12.0, 400.0)).color(p.secondary).single_line().truncate_head().shrink().min_w(0.0))
                    .child(el().grow(1.0));
                if resolved {
                    head = head.child(chip(p, Some(&I::CHECK), "Resolved", p.green, None, false));
                }
                c = c.child(head).child(rule(p));
            }
        }
    }
    for (index, note) in notes.iter().enumerate() {
        if index > 0 {
            c = c.child(el().w_full().pl(46.0).child(rule(p)));
        }
        c = c.child(note_view(s, p, k, item, note));
    }
    if !thread.individual_note || notes.first().and_then(|n| n.position.as_ref()).is_some() {
        c = c.child(rule(p));
        let key = DraftKey::Reply(item.id.clone(), thread.id.clone());
        if s.is_replying(item, thread) {
            c = c.child(el().w_full().padding(10.0).child(composer(s, p, k, key, "Reply…", "Reply", true)));
        } else {
            let open = format!("{}|{}", item.id, thread.id);
            let mut reply = row()
                .key("reply")
                .role("button")
                .label("Reply")
                .w_full()
                .h(32.0)
                .px(14.0)
                .gap(6.0)
                .center_y()
                .hover(p.hover)
                .child(icon(&I::UNDO, 11.0).color(p.secondary))
                .child(text("Reply").font(ui(12.0, MEDIUM)).color(p.secondary))
                .child(el().grow(1.0));
            if !shows_position && resolved {
                reply = reply.child(text("Resolved").font(ui(12.0, 400.0)).color(p.secondary));
            }
            let focus_key = draft_field_key(&key);
            c = c.child(reply.on_press_tx(move |s: &mut App, tx: &mut Tx| {
                s.replying.insert(open.clone());
                tx.focus(focus_key.clone(), false);
            }));
        }
    }
    if resolved { c.opacity(0.7) } else { c }
}

fn note_view(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, note: &Note) -> El {
    let mut meta = row()
        .gap(6.0)
        .center_y()
        .child(text(note.author.name.clone()).font(ui(13.0, SEMIBOLD)).color(p.text).single_line())
        .child(text(relative(note.created_at)).font(ui(12.0, 400.0)).color(p.tertiary).single_line().help(long_date(note.created_at)));
    if s.store.is_fresh(item, note) {
        // A word as well as the dot, so it isn't told by colour alone.
        meta = meta.child(
            row()
                .gap(4.0)
                .center_y()
                .help("New since you last looked")
                .child(el().size(5.0, 5.0).radius(2.5).fill(p.accent))
                .child(text("New").font(ui(12.0, SEMIBOLD)).color(p.accent).single_line()),
        );
    }
    row().w_full().padding(14.0).gap(10.0).items_start().child(avatar(s, p, &note.author, 22.0)).child(
        col().gap(6.0).grow(1.0).shrink().min_w(0.0).child(meta).child(markdown::markdown(
            s,
            p,
            k,
            Doc { key: &format!("note-{}", note.id), source: &note.body, base: item.project_url(), size: 14.0, serif: false },
        )),
    )
}

/// The key path of a draft's text field, for focusing it.
pub fn draft_field_key(key: &DraftKey) -> String {
    format!("{}/input", composer_key(key))
}

fn composer_key(key: &DraftKey) -> String {
    match key {
        DraftKey::Comment(id) => format!("compose-{id}"),
        DraftKey::Reply(id, thread) => format!("reply-{id}-{thread}"),
        DraftKey::Line(id, file, line) => format!("line-{id}-{}-{line}", crate::data::models::fnv1a(file)),
    }
}

/// A comment box: the text, "⌘↩ to post", and Cancel (for replies and line comments) and post.
pub fn composer(s: &App, p: &Pal, k: &Kit, key: DraftKey, placeholder: &str, submit: &str, compact: bool) -> El {
    let empty = TextEditHolder::empty();
    let edit = s.draft(&key).unwrap_or(&empty.0);
    let can_post = !edit.text.trim().is_empty() && s.store.busy.is_none();
    let edit_key = key.clone();
    let post_key = key.clone();
    let key_key = key.clone();
    let field = TextArea::new(edit)
        .width(None)
        .placeholder(placeholder)
        .auto_grow(if compact { 3 } else { 4 }, 12)
        .on_edit(move |s: &mut App| s.draft_mut(&edit_key))
        .on_key(move |s: &mut App, ev: &rune::KeyEvent, _: &mut Tx| match ev.key {
            rune::Key::Enter if ev.mods.cmd => {
                s.post(key_key.clone());
                true
            }
            rune::Key::Escape => {
                cancel(s, &key_key);
                true
            }
            _ => false,
        })
        .build(k)
        // A hairline like the thread cards above it; the kit's input shadow vanishes on this fill.
        .border(1.0, p.line)
        .radius(10.0)
        .padding(if compact { 8.0 } else { 12.0 });
    let mut buttons =
        row().w_full().gap(8.0).center_y().child(text(crate::ui::components::keys("⌘↩ to post")).font(ui(12.0, 400.0)).color(p.tertiary)).child(el().grow(1.0));
    if !matches!(key, DraftKey::Comment(_)) {
        let cancel_key = key.clone();
        buttons = buttons.child(action_button(p, "cancel", None, "Cancel", false, true).on_press(move |s: &mut App| cancel(s, &cancel_key)));
    }
    buttons = buttons.child(action_button(p, "post", Some(&I::BUBBLE_TEXT), submit, true, can_post).on_press(move |s: &mut App| s.post(post_key.clone())));
    col().key(composer_key(&key)).w_full().gap(8.0).child(field).child(buttons)
}

fn cancel(s: &mut App, key: &DraftKey) {
    match key {
        DraftKey::Comment(_) => {}
        DraftKey::Reply(id, thread) => {
            s.replying.remove(&format!("{id}|{thread}"));
        }
        DraftKey::Line(..) => {
            if s.line_comment.as_ref() == Some(key) {
                s.line_comment = None;
            }
        }
    }
}

/// An empty draft for a box nobody has typed in yet.
struct TextEditHolder(rune::TextEdit);

impl TextEditHolder {
    fn empty() -> TextEditHolder {
        TextEditHolder(rune::TextEdit::new(""))
    }
}

/// Opens the comment box under a diff line and puts the caret in it.
pub fn comment_on_line(s: &mut App, tx: &mut Tx, item: String, file: String, line: usize) {
    let key = DraftKey::Line(item, file, line);
    tx.focus(draft_field_key(&key), false);
    s.line_comment = Some(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_step_in_the_rails_order_and_stop_at_the_ends() {
        assert_eq!(Part::Ask.step(-1, 3), Part::Ask);
        assert_eq!(Part::Changes.step(1, 3), Part::File(0));
        assert_eq!(Part::File(2).step(1, 3), Part::Conversation);
        assert_eq!(Part::Conversation.step(-1, 3), Part::File(2));
        assert_eq!(Part::Changes.step(1, 0), Part::Conversation);
        assert_eq!(Part::Verdict.step(1, 3), Part::Verdict);
    }

    #[test]
    fn every_thread_shows_once_on_a_line_or_in_the_conversation() {
        let (_, diffs, threads) = crate::fixtures::load();
        let (id, files) = diffs.into_iter().find(|(_, files)| files[0].new_path == "src/export/job.rs").unwrap();
        let threads = &threads[&id];
        let placed = Placed::new(threads, None, &files);
        let on_lines: usize = placed.by_file.iter().map(Vec::len).sum();
        assert_eq!(on_lines + placed.general.len(), threads.len());
        // One on a line of job.rs; a general note, and one on a file the diff doesn't have.
        assert_eq!(placed.by_file[0].len(), 1);
        assert_eq!(placed.general.len(), 2);
        // The fixture's positions name no commit, so they hold for any head.
        assert_eq!(Placed::new(threads, Some("abc"), &files).general.len(), 2);
        let lines: Vec<_> = files[0].lines.iter().filter(|l| !placed.on(l, &files[0]).is_empty()).collect();
        assert_eq!(lines.len(), 1);
    }
}
