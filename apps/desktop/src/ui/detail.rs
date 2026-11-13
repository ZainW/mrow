//! A merge request on its card: the header, the tabs, and the tab's content, all in one
//! virtualized list in one scroll view, so the whole page scrolls together (as in the Swift app)
//! while only the rows on screen are built. A diff of thousands of lines costs what a screenful
//! does.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ui::smooth_scroll::{smooth_wheel, smooth_wheel_watching};
use rune::text::Span;
use rune::{El, GrowthSpec, PointerPress, PopupAnchor, PopupItem, PopupMenu, ScrollSpec, Tx, col, el, icon, rich, row, text};
use rune_kit::{Kit, Spinner, TextArea};

use crate::app::{App, DetailTab, DraftKey};
use crate::data::diff::{DiffLine, LineKind};
use crate::data::models::{Discussion, FileDiff, MergeRequest, Note};
use crate::data::store::{Loadable, Viewed, relative};
use crate::ui::components::{action_button, avatar, chip, filled_mark, long_date, month_day, pipeline_dot, progress_bar, rule, state_icon};
use crate::ui::diff;
use crate::ui::icons as I;
use crate::ui::markdown::{self, Doc};
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, READING_WIDTH, SEMIBOLD, mono, serif, ui};
use crate::ui::{Look, actions, look};

/// Files longer than this wait for a click before their lines are drawn.
pub const LARGE_FILE: usize = 400;
const PAGE_WIDTH: f32 = 1100.0;

/// One row of the page.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Header,
    Tabs,
    Description,
    Loading(&'static str),
    Failed(&'static str, String),
    Empty(&'static str),
    FileList,
    FileHeader(usize),
    Notice(usize, String, bool),
    Top(usize),
    Line(usize, usize),
    Footer(usize),
    Thread(usize),
    Composer,
}

impl Row {
    fn key(&self, files: &[FileDiff]) -> String {
        match self {
            Row::Header => "header".into(),
            Row::Tabs => "tabs".into(),
            Row::Description => "description".into(),
            Row::Loading(_) => "loading".into(),
            Row::Failed(..) => "failed".into(),
            Row::Empty(_) => "empty".into(),
            Row::FileList => "files".into(),
            Row::FileHeader(f) => format!("file:{}", files[*f].id()),
            Row::Notice(f, ..) => format!("notice:{}", files[*f].id()),
            Row::Top(f) => format!("top:{}", files[*f].id()),
            Row::Line(f, l) => format!("line:{f}:{l}"),
            Row::Footer(f) => format!("footer:{}", files[*f].id()),
            Row::Thread(t) => format!("thread:{t}"),
            Row::Composer => "composer".into(),
        }
    }
}

/// Everything a row needs that isn't on `App`, worked out once per frame.
struct Page {
    item: MergeRequest,
    tab: DetailTab,
    rows: Rc<Vec<Row>>,
    files: Rc<Vec<FileDiff>>,
    threads: Rc<Vec<Discussion>>,
    placed: Placed,
    look: Rc<Look>,
    width: f32,
}

pub fn detail(s: &App, p: &Pal, _k: &Kit, item: &MergeRequest) -> El {
    let look = look(p.dark);
    let tab = s.tab(item);
    let store = &s.store;
    let files: Rc<Vec<FileDiff>> = match store.diffs.get(&item.id) {
        Some(Loadable::Loaded(f)) => f.clone(),
        _ => Rc::default(),
    };
    let threads: Rc<Vec<Discussion>> = match store.discussions.get(&item.id) {
        Some(Loadable::Loaded(t)) => t.clone(),
        _ => Rc::default(),
    };
    let head = store.diff_refs.get(&item.id).map(|r| r.head_sha.clone());
    let placed = Placed::new(&threads, head.as_deref());
    let mut rows = vec![Row::Header, Row::Tabs];
    let mut cached = None;
    match tab {
        DetailTab::Overview => rows.push(Row::Description),
        DetailTab::Changes => match store.diffs.get(&item.id) {
            Some(Loadable::Loaded(_)) if files.is_empty() => rows.push(Row::Empty("No changes.")),
            Some(Loadable::Loaded(_)) => cached = Some(changes_page(s, item, &files)),
            Some(Loadable::Failed(e)) => rows.push(Row::Failed("changes", e.clone())),
            _ => rows.push(Row::Loading("changes")),
        },
        DetailTab::Discussion => match store.discussions.get(&item.id) {
            Some(Loadable::Loaded(_)) => {
                if threads.is_empty() {
                    rows.push(Row::Empty("No comments yet."));
                }
                rows.extend((0..threads.len()).map(Row::Thread));
                rows.push(Row::Composer);
            }
            Some(Loadable::Failed(e)) => rows.push(Row::Failed("discussion", e.clone())),
            _ => rows.push(Row::Loading("discussion")),
        },
    }
    // One column for every tab, so the header and tabs stay put when switching; reading rows
    // (description, threads) keep a shorter measure inside it, left-aligned.
    let width = PAGE_WIDTH;
    let (rows, keys) = cached.unwrap_or_else(|| {
        let keys = rows.iter().map(|r| r.key(&files)).collect();
        (Rc::new(rows), Rc::new(keys))
    });
    let count = rows.len();
    let page = Rc::new(Page { item: item.clone(), tab, rows, files, threads, placed, look, width });
    let version = (item.id.clone(), tab, keys.len(), keys.last().cloned());
    let key_list = keys.clone();
    let list = col().key("page-rows").w_full().growth(GrowthSpec::default()).virtual_rows_version(&version).virtual_rows(
        count,
        22.0,
        move |_: &App, i| Cow::Owned(key_list[i].clone()),
        move |s: &App, i| build_row(s, &page, i),
    );
    let key = format!("page-{}", item.id);
    let page = col().key(key.clone()).w_full().h_full().min_h(0.0).child(list).scroll(ScrollSpec { stick_bottom: false });
    match &s.bench {
        Some(_) => smooth_wheel_watching(page, &key, crate::bench::note_scroll),
        None => smooth_wheel(page, &key),
    }
}

thread_local! {
    /// The last changes page's rows and their keys, kept while its files and their folding stay
    /// the same: a big diff has tens of thousands of rows, too many to list again every frame.
    static CHANGES_PAGE: RefCell<Option<(String, Rc<Vec<FileDiff>>, Vec<u8>, PageRows)>> = const { RefCell::new(None) };
}

type PageRows = (Rc<Vec<Row>>, Rc<Vec<String>>);

/// The changes page's rows (header, tabs, file list, then every file) and their keys.
fn changes_page(s: &App, item: &MergeRequest, files: &Rc<Vec<FileDiff>>) -> PageRows {
    // How each file is folded: 0 open, 1 collapsed, 2 a large file expanded.
    let folds: Vec<u8> = files
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
    let cached =
        CHANGES_PAGE.with(|c| c.borrow().as_ref().and_then(|(id, f, fo, page)| (id == &item.id && Rc::ptr_eq(f, files) && fo == &folds).then(|| page.clone())));
    if let Some(page) = cached {
        return page;
    }
    let mut rows = vec![Row::Header, Row::Tabs, Row::FileList];
    for (f, file) in files.iter().enumerate() {
        rows.push(Row::FileHeader(f));
        if folds[f] == 1 {
            continue;
        }
        if file.too_large || (file.diff.is_empty() && file.collapsed) {
            rows.push(Row::Notice(f, format!("This diff is too large to show here. Open it in {}.", item.kind().name()), false));
        } else if file.lines.is_empty() {
            let text = if file.renamed_file { "Renamed without changes." } else { "No changes to show (binary or mode change)." };
            rows.push(Row::Notice(f, text.into(), false));
        } else if (file.lines.len() > LARGE_FILE || file.generated_file) && folds[f] != 2 {
            rows.push(Row::Notice(f, format!("{} lines. Show the diff", file.lines.len()), true));
        } else {
            rows.push(Row::Top(f));
            rows.extend((0..file.lines.len()).map(|l| Row::Line(f, l)));
        }
        rows.push(Row::Footer(f));
    }
    let keys = rows.iter().map(|r| r.key(files)).collect();
    let page = (Rc::new(rows), Rc::new(keys));
    CHANGES_PAGE.with(|c| *c.borrow_mut() = Some((item.id.clone(), files.clone(), folds, page.clone())));
    page
}

/// Centres a row's content in the page's column, with the page's margins.
fn frame(page: &Page, i: usize, content: El) -> El {
    let last = i + 1 == page.rows.len();
    let mut outer = row().w_full().px(44.0).justify_center();
    if i == 0 {
        outer = outer.pt(34.0);
    }
    if last {
        outer = outer.pb(60.0);
    }
    outer.child(col().w_full().max_w(page.width).child(content))
}

fn build_row(s: &App, page: &Page, i: usize) -> El {
    let p = &page.look.p;
    let k = &*page.look.k;
    let item = &page.item;
    let content = match &page.rows[i] {
        Row::Header => header(s, p, item),
        Row::Tabs => tab_bar(s, p, page),
        Row::Description => {
            let body = if item.description.trim().is_empty() {
                text("No description.").font(serif(15.5, 400.0).italic()).color(p.tertiary)
            } else {
                markdown::markdown(s, p, k, Doc { key: &item.id, source: &item.description, base: item.project_url(), size: 15.5, serif: true })
            };
            el().w_full().max_w(READING_WIDTH).pt(22.0).child(body)
        }
        Row::Loading(what) => row()
            .pt(22.0)
            .gap(8.0)
            .center_y()
            .child(Spinner::new().size(14.0).build(k))
            .child(text(format!("Loading {what}…")).font(ui(12.5, 400.0)).color(p.secondary)),
        Row::Failed(what, message) => row()
            .pt(22.0)
            .gap(6.0)
            .items_start()
            .child(icon(&I::WARNING, 13.0).color(p.red))
            .max_w(READING_WIDTH)
            .child(text(format!("Couldn't load the {what}: {message}")).font(ui(12.5, 400.0)).color(p.red).wrap().grow(1.0).shrink().min_w(0.0)),
        Row::Empty(message) => el().pt(22.0).child(text(*message).font(ui(13.0, 400.0)).color(p.tertiary)),
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
            let top = if *t == 0 { 22.0 } else { 16.0 };
            el().w_full().max_w(READING_WIDTH).pt(top).child(thread_card(s, p, k, item, thread, true))
        }
        Row::Composer => {
            let top = if page.threads.is_empty() { 16.0 } else { 24.0 };
            el().w_full().max_w(READING_WIDTH).pt(top).child(composer(
                s,
                p,
                k,
                DraftKey::Comment(item.id.clone()),
                "Leave a comment. Markdown works.",
                "Comment",
                false,
            ))
        }
    };
    frame(page, i, content)
}

// MARK: Header

fn header(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let mut top = row()
        .gap(8.0)
        .center_y()
        .child(text(item.project_path.clone()).font(mono(11.5, 400.0)).color(p.tertiary).single_line().truncate().shrink().min_w(0.0))
        .child(text(format!("{}{}", item.kind().reference_mark(), item.iid)).font(mono(11.5, MEDIUM)).color(p.secondary).single_line());
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
    if mine && item.is_open() {
        c = c.child(crate::ui::mine::checklist(s, p, item).pt(2.0));
    }
    c = c.child(reviewers(s, p, item));
    // An open request of mine has its actions in the next step, above.
    if mine && item.is_open() { c } else { c.child(action_bar(s, p, item).pt(4.0)) }
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
            .child(text(format!("+{}", item.additions)).font(mono(11.0, 400.0)).color(p.green).single_line())
            .child(text(format!("−{}", item.deletions)).font(mono(11.0, 400.0)).color(p.red).single_line())
            .child(text(format!("· {} {files}", item.file_count)).font(mono(11.0, 400.0)).color(p.secondary).single_line()),
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
                .child(text(format!("Pipeline {}", pipe.label().to_lowercase())).font(ui(11.5, MEDIUM)).color(p.secondary).single_line()),
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
    let mut r = row().gap(14.0).center_y().flex_wrap().child(text("Reviewers").font(ui(11.5, MEDIUM)).color(p.tertiary));
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
    let idle = s.store.busy.is_none();
    let mut r = row().gap(8.0).center_y();
    // Nobody approves their own.
    if item.is_open() && !s.store.is_mine(item) {
        if i_approved(s, item) {
            let it = item.clone();
            r = r.child(action_button(p, "revoke", Some(&I::UNDO), "Revoke Approval", false, idle).on_press(move |s: &mut App| {
                if s.store.busy.is_none() {
                    s.store.unapprove(&it)
                }
            }));
        } else {
            r = r.child(
                action_button(p, "approve", Some(&I::CHECK), "Approve", true, idle)
                    .help(crate::ui::components::keys("Approve  ⇧⌘A"))
                    .on_press_tx(actions::approve),
            );
        }
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

fn tab_bar(s: &App, p: &Pal, page: &Page) -> El {
    let item = &page.item;
    let mut r = row().key("detail-tabs").w_full().gap(2.0).pb(8.0).center_y();
    for (index, tab) in DetailTab::ALL.into_iter().enumerate() {
        let title = match tab {
            DetailTab::Overview => "Overview".to_string(),
            DetailTab::Changes if item.file_count > 0 => format!("Changes  {}", item.file_count),
            DetailTab::Changes => "Changes".to_string(),
            DetailTab::Discussion => {
                let threads = s.store.discussions.get(&item.id).and_then(|l| l.value()).map_or(0, |t| t.len());
                if threads > 0 { format!("Discussion  {threads}") } else { "Discussion".to_string() }
            }
        };
        r = r.child(
            crate::ui::components::ChromeButton::new(format!("tab-{index}"), title)
                .style(crate::ui::components::Chrome::Text)
                .on(page.tab == tab)
                .build(p)
                .help(crate::ui::components::keys(&format!("{}  ⌥⌘{}", tab.title(), index + 1)))
                .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                    tx.instant();
                    s.set_tab(tab);
                    tx.scroll_to_instant(format!("page-{}", s.store.selected_id.clone().unwrap_or_default()), 0.0, 0.0);
                }),
        );
    }
    col().w_full().pt(26.0).child(r).child(rule(p))
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
            .child(text(format!("{viewed} of {} viewed", files.len())).font(ui(11.5, MEDIUM)).color(if done { p.green } else { p.secondary }))
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
        let mark = match state {
            Viewed::Yes => filled_mark(&I::DISC, &I::SMALL_CHECK, p.green, p.well, 12.0).help("Viewed"),
            Viewed::Changed => filled_mark(&I::DISC, &I::DOT, p.amber, p.well, 12.0).help("Changed since you viewed it"),
            Viewed::No => icon(&I::PLAIN_CIRCLE, 12.0).color(p.line).help("Not viewed yet"),
        };
        let id = file.id();
        let mr = item.id.clone();
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
                        .font(mono(11.5, 400.0))
                        .color(if state == Viewed::Yes { p.tertiary } else { p.text })
                        .single_line()
                        .truncate_head()
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0),
                )
                .child(counts(p, file).opacity(if state == Viewed::Yes { 0.6 } else { 1.0 }))
                .child(mark)
                .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                    s.collapsed.remove(&format!("{mr}|{id}"));
                    tx.scroll_key_to(format!("page-rows/file:{id}"), 0.0);
                }),
        );
    }
    el().w_full().pt(22.0).pb(14.0).child(c)
}

fn counts(p: &Pal, file: &FileDiff) -> El {
    let mut r = row().gap(6.0);
    if file.added > 0 {
        r = r.child(text(format!("+{}", file.added)).font(mono(11.0, MEDIUM)).color(p.green).single_line());
    }
    if file.removed > 0 {
        r = r.child(text(format!("−{}", file.removed)).font(mono(11.0, MEDIUM)).color(p.red).single_line());
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
            .child(text("→").font(ui(10.0, 400.0)).color(p.tertiary));
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
    let threads = page.placed.count(&file.new_path);
    if threads > 0 {
        name = name.child(
            row()
                .gap(4.0)
                .center_y()
                .help(format!("{threads} comment thread{} on this file", if threads == 1 { "" } else { "s" }))
                .child(icon(&I::BUBBLE_TEXT, 11.0).color(p.secondary))
                .child(text(threads.to_string()).font(mono(11.0, MEDIUM)).color(p.secondary)),
        );
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
                .font(ui(11.5, MEDIUM))
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

/// Comment threads placed on the diff line they're about.
#[derive(Default)]
pub struct Placed {
    by_line: HashMap<String, Vec<usize>>,
    by_path: HashMap<String, usize>,
}

impl Placed {
    pub fn new(threads: &[Discussion], head: Option<&str>) -> Placed {
        let mut placed = Placed::default();
        for (index, thread) in threads.iter().enumerate() {
            let Some(position) = thread.notes.iter().find(|n| !n.system).and_then(|n| n.position.as_ref()) else { continue };
            if let (Some(head), Some(at)) = (head, position.head_sha.as_deref()) {
                if at != head {
                    continue;
                }
            }
            let key = match (position.new_line, &position.new_path, position.old_line, &position.old_path) {
                (Some(line), Some(path), ..) => format!("n|{path}|{line}"),
                (_, _, Some(line), Some(path)) => format!("o|{path}|{line}"),
                _ => continue,
            };
            placed.by_line.entry(key).or_default().push(index);
            let path = position.new_path.clone().or_else(|| position.old_path.clone()).unwrap_or_default();
            *placed.by_path.entry(path).or_default() += 1;
        }
        placed
    }

    pub fn on(&self, line: &DiffLine, file: &FileDiff) -> Vec<usize> {
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

    pub fn count(&self, path: &str) -> usize {
        self.by_path.get(path).copied().unwrap_or(0)
    }
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
                    .child(text(format!("{path}{at}")).font(mono(11.0, 400.0)).color(p.secondary).single_line().truncate_head().shrink().min_w(0.0))
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
                reply = reply.child(text("Resolved").font(ui(11.5, 400.0)).color(p.secondary));
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
    row().w_full().padding(14.0).gap(10.0).items_start().child(avatar(s, p, &note.author, 22.0)).child(
        col()
            .gap(6.0)
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .child(
                row()
                    .gap(6.0)
                    .center_y()
                    .child(text(note.author.name.clone()).font(ui(12.5, SEMIBOLD)).color(p.text).single_line())
                    .child(text(relative(note.created_at)).font(ui(12.0, 400.0)).color(p.tertiary).single_line().help(long_date(note.created_at))),
            )
            .child(markdown::markdown(
                s,
                p,
                k,
                Doc { key: &format!("note-{}", note.id), source: &note.body, base: item.project_url(), size: 13.5, serif: false },
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
        row().w_full().gap(8.0).center_y().child(text(crate::ui::components::keys("⌘↩ to post")).font(ui(11.5, 400.0)).color(p.tertiary)).child(el().grow(1.0));
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
