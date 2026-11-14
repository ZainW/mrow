//! A merge request on its card: a bar of tabs, and one tab's page under it. Overview is the ask
//! (its header), the description and where the review stands; Changes lists the files, then
//! each file's lines with the threads on them; Conversation is every thread, then the comment
//! box. One of my own ends its overview with what's left before it merges instead. Approve is in
//! the bar on every tab, so approving never takes a scroll.
//!
//! Each tab is a virtualized list in its own scroll view, so only the rows on screen are built:
//! a diff of thousands of lines costs what a screenful does. All three stay mounted, the hidden
//! ones folded to no height, because Rune drops a scroll view's offset and its rows' measured
//! heights once it leaves the tree: kept, each tab is where it was left when it shows again.

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
use crate::data::diff::LineKind;
use crate::data::models::{Discussion, FileDiff, MergeRequest, Note};
use crate::data::store::{Loadable, Viewed, relative};
use crate::ui::components::{
    Chrome, ChromeButton, action_button, action_icon_button, avatar, chip, comment_count, filled_mark, keys, long_date, month_day, pipeline_dot, progress_bar,
    rule, state_icon,
};
use crate::ui::diff;
use crate::ui::icons as I;
use crate::ui::markdown::{self, Doc};
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, READING_WIDTH, SEMIBOLD, mono, serif, ui};
use crate::ui::{Look, actions, look};

/// Files longer than this wait for a click before their lines are drawn.
pub const LARGE_FILE: usize = 400;
/// The Changes tab's column; the others keep the reading measure.
const PAGE_WIDTH: f32 = 1100.0;
/// Where a tab's page starts: the room above its first row.
const PAGE_TOP: f32 = 34.0;
/// How far under the top of the Changes tab a jump to a thread's line puts it, so the lines
/// before it show too.
const LINE_ROOM: f32 = 96.0;
/// The narrowest card whose bar has room for "Open in GitLab" written out beside the tabs,
/// Revoke Approval and a busy line. A narrower one shows that button's icon alone, its tabs
/// closer together, Revoke for Revoke Approval, and the busy spinner without its line (which
/// becomes its tip).
const BAR_ROOM: f32 = 760.0;
/// The narrowest card whose bar still fits that: the tabs, the busy spinner, the forge's icon and
/// Approve written out. A narrower one (the Ask panel open in a small window) leaves the way out
/// to the window's toolbar and shows Approve as its icon.
const TIGHT_ROOM: f32 = 480.0;

/// The tabs of a merge request's card, in the bar's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tab {
    #[default]
    Overview,
    Changes,
    Conversation,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Overview, Tab::Changes, Tab::Conversation];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Changes => "Changes",
            Tab::Conversation => "Conversation",
        }
    }

    /// The keys that open the tab, in the Mac's glyphs: those of its command in `actions`.
    pub fn hint(self) -> &'static str {
        match self {
            Tab::Overview => "⌥⌘1",
            Tab::Changes => "⌥⌘2",
            Tab::Conversation => "⌥⌘3",
        }
    }

    /// The key of the tab's list of rows, the scope its rows are found in.
    fn list_key(self) -> &'static str {
        match self {
            Tab::Overview => "overview-rows",
            Tab::Changes => "changes-rows",
            Tab::Conversation => "conversation-rows",
        }
    }
}

/// The key of a merge request's scroll view on `tab`.
pub fn scroll_key(id: &str, tab: Tab) -> String {
    format!("{}-{id}", tab.list_key())
}

/// Where a jump lands: a tab where it was left, or a place on one.
#[derive(Clone, Debug, PartialEq)]
pub enum Jump {
    Tab(Tab),
    /// A file's header at the top of the Changes tab; none is the tab's own top.
    File(Option<usize>),
    /// The diff line a thread (by its id) is on, in the Changes tab.
    Line(String),
    /// A thread, by its id, in the Conversation tab.
    Thread(String),
    /// The comment box at the end of the conversation, with the caret in it.
    Composer,
}

impl Jump {
    fn tab(&self) -> Tab {
        match self {
            Jump::Tab(tab) => *tab,
            Jump::File(_) | Jump::Line(_) => Tab::Changes,
            Jump::Thread(_) | Jump::Composer => Tab::Conversation,
        }
    }
}

/// Where the reader is in a merge request's changes, from what the Changes tab shows, for J and K.
#[derive(Clone, Debug, Default)]
pub struct Spy {
    /// The file the first row on screen belongs to; none above the first file.
    top: Option<usize>,
    /// The tab shows its end.
    bottom: bool,
    /// The file the last jump went to, kept while the tab's end stops it reaching the top.
    aimed: Option<usize>,
}

/// The file `by` files on from `from`, in a list of `files`, stopping at either end. None is the
/// top of the tab, before the first file.
fn step_file(from: Option<usize>, by: i64, files: usize) -> Option<usize> {
    let at = from.map_or(0, |f| f.min(files.saturating_sub(1)) as i64 + 1);
    match (at + by).clamp(0, files as i64) {
        0 => None,
        i => Some(i as usize - 1),
    }
}

/// One row of a tab's page.
#[derive(Clone, Debug, PartialEq)]
enum Row {
    Header,
    Description,
    /// Where the review stands and where to start; on my own, what's left before it merges.
    Standing,
    Loading,
    Failed(String),
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
    /// The file a row of the changes belongs to.
    fn file(&self) -> Option<usize> {
        match self {
            Row::FileHeader(f) | Row::Notice(f, ..) | Row::Top(f) | Row::Line(f, _) | Row::Footer(f) => Some(*f),
            _ => None,
        }
    }

    fn key(&self, files: &[FileDiff], threads: &[Discussion]) -> String {
        match self {
            Row::Header => "header".into(),
            Row::Description => "description".into(),
            Row::Standing => "standing".into(),
            Row::Loading => "loading".into(),
            Row::Failed(_) => "failed".into(),
            Row::Empty(_) => "empty".into(),
            Row::FileList => "files".into(),
            Row::FileHeader(f) => format!("file:{}", files[*f].id()),
            Row::Notice(f, ..) => format!("notice:{}", files[*f].id()),
            Row::Top(f) => format!("top:{}", files[*f].id()),
            Row::Line(f, l) => format!("line:{f}:{l}"),
            Row::Footer(f) => format!("footer:{}", files[*f].id()),
            Row::Thread(t) => format!("thread:{}", threads[*t].id),
            Row::Composer => "composer".into(),
        }
    }
}

/// Everything a row needs that isn't on `App`, worked out once per frame and shared by the tabs.
struct Page {
    item: MergeRequest,
    files: Rc<Vec<FileDiff>>,
    /// The changes arrived (possibly with no files), and the threads.
    changes: bool,
    talk: bool,
    threads: Rc<Vec<Discussion>>,
    placed: Rc<Placed>,
    /// A thread has a note written since I last looked.
    fresh: bool,
    look: Rc<Look>,
    /// I opened it: its overview ends with what's left before it merges, and nobody approves it.
    mine: bool,
}

/// How much room a card's bar has for its buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Room {
    /// Icons alone, and no way out to the forge: the window's toolbar has one.
    Tight,
    /// The forge's icon, Revoke for Revoke Approval.
    Narrow,
    /// Everything written out.
    #[default]
    Wide,
}

/// The room in the bar of a card `[x, y, w, h]`.
pub fn room([_, _, w, _]: [f32; 4]) -> Room {
    if w < TIGHT_ROOM {
        Room::Tight
    } else if w < BAR_ROOM {
        Room::Narrow
    } else {
        Room::Wide
    }
}

/// The card of `item`: the bar, drawn for the `room` it has, and the open tab's page under it.
pub fn detail(s: &App, p: &Pal, _k: &Kit, item: &MergeRequest, room: Room) -> El {
    let look = look(p.dark);
    let store = &s.store;
    let diffs = store.diffs.get(&item.id);
    let talk = store.discussions.get(&item.id);
    let files: Rc<Vec<FileDiff>> = diffs.and_then(Loadable::value).cloned().unwrap_or_default();
    let threads: Rc<Vec<Discussion>> = talk.and_then(Loadable::value).cloned().unwrap_or_default();
    let head = store.diff_refs.get(&item.id).map(|r| r.head_sha.clone());
    let lists = page_rows(s, item, diffs, talk, head);
    let mine = store.is_mine(item);
    let page = Rc::new(Page {
        item: item.clone(),
        files,
        changes: matches!(diffs, Some(Loadable::Loaded(_))),
        talk: matches!(talk, Some(Loadable::Loaded(_))),
        threads,
        placed: lists.placed.clone(),
        fresh: lists.fresh,
        look,
        mine,
    });
    let open = s.tab(&item.id);
    let overview = Rc::new(vec![Row::Header, Row::Description, Row::Standing]);
    let overview_keys = Rc::new(overview.iter().map(|r| r.key(&page.files, &page.threads)).collect::<Vec<_>>());
    let pages = col()
        .key("pages")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(tab_page(s, &page, Tab::Overview, (overview_keys, overview, None), open))
        .child(tab_page(s, &page, Tab::Changes, (lists.keys, lists.rows, Some(lists.version)), open))
        .child(tab_page(s, &page, Tab::Conversation, (lists.talk_keys, lists.talk_rows, Some(lists.version)), open));
    col().key("review").w_full().h_full().min_h(0.0).child(bar(s, &page, open, room)).child(pages)
}

/// A tab's rows, their keys, and for a long list the version those keys go with.
type Rows = (Rc<Vec<String>>, Rc<Vec<Row>>, Option<u64>);

/// One tab's scroll view. The `open` tab's fills the card; the others stay mounted at no height,
/// out of reach of the pointer, the keyboard and assistive technology, so they keep their place.
fn tab_page(s: &App, page: &Rc<Page>, tab: Tab, (keys, rows, version): Rows, open: Tab) -> El {
    let id = page.item.id.clone();
    let count = rows.len();
    let built = (page.clone(), rows.clone());
    let mut list = col().key(tab.list_key()).w_full().growth(GrowthSpec::default());
    if let Some(version) = version {
        list = list.virtual_rows_version(&(id.as_str(), version));
    }
    list = list.virtual_rows(count, 22.0, move |_: &App, i| Cow::Owned(keys[i].clone()), move |s: &App, i| build_row(s, &built.0, tab, &built.1, i));
    let key = scroll_key(&id, tab);
    let view = col().key(key.clone()).w_full().min_h(0.0);
    if tab != open {
        return view.h(0.0).inert().a11y_hidden().child(list).scroll(ScrollSpec { stick_bottom: false });
    }
    if tab == Tab::Changes {
        let seen = id.clone();
        list = list.on_visible_rows(move |s: &mut App, range: Range<usize>| {
            let spy = s.spies.entry(seen.clone()).or_default();
            spy.top = rows.get(range.start).and_then(Row::file);
            if let Some(aim) = spy.aimed {
                let header = rows.partition_point(|r| r.file() < Some(aim));
                if !(spy.bottom && range.contains(&header)) {
                    spy.aimed = None;
                }
            }
            false
        });
    }
    let mut view = view.grow(1.0).role("tabpanel").label(tab.title()).child(list).scroll(ScrollSpec { stick_bottom: false });
    if tab == Tab::Changes {
        view = view.on_at_bottom(move |s: &mut App, at| s.spies.entry(id.clone()).or_default().bottom = at);
    }
    match &s.bench {
        Some(_) => smooth_wheel_watching(view, &key, crate::bench::note_scroll),
        None => smooth_wheel(view, &key),
    }
}

/// The bar over the page: the tabs, then what's under way, the way out to the forge and Approve.
fn bar(s: &App, page: &Page, open: Tab, room: Room) -> El {
    let p = &page.look.p;
    let item = &page.item;
    let narrow = room < Room::Wide;
    // The tabs give way before the buttons do: on a sliver of a card they're cut short (their
    // keys still switch them) and Approve stays in reach.
    let mut tabs = row()
        .key("tabs")
        .role("tablist")
        .label(format!("Parts of the {}", item.kind().noun()))
        .focus_group()
        .gap(2.0)
        .center_y()
        .shrink()
        .min_w(0.0)
        .clip_rounded(0.0);
    for (index, tab) in Tab::ALL.into_iter().enumerate() {
        let count = match tab {
            Tab::Overview => 0,
            Tab::Changes if page.changes => page.files.len(),
            Tab::Changes => item.file_count.max(0) as usize,
            Tab::Conversation => page.threads.len(),
        };
        let fresh = tab == Tab::Conversation && page.fresh;
        let mut label = tab.title().to_string();
        let mut button =
            ChromeButton::new(format!("tab-{index}"), tab.title()).style(Chrome::Text).on(tab == open).dot(fresh).pad(if narrow { 6.0 } else { 10.0 });
        if count > 0 {
            button = button.count(count);
            label = format!("{label} {count}");
        }
        if fresh {
            label.push_str(", with something new");
        }
        tabs = tabs.child(
            button
                .build(p)
                .role("tab")
                .label(label)
                .focus_selected(tab == open)
                .help(keys(&format!("{}  {}", tab.title(), tab.hint())))
                .on_press_tx(move |s: &mut App, tx: &mut Tx| go(s, tx, Jump::Tab(tab))),
        );
    }
    let mut r = row().key("bar").w_full().h(48.0).px(12.0).gap(8.0).center_y().child(tabs).child(el().grow(1.0));
    if let Some(busy) = &s.store.busy {
        r = r.child(Spinner::new().size(14.0).build(&page.look.k).help(busy.clone()));
        if !narrow {
            r = r.child(text(busy.clone()).font(ui(12.0, 400.0)).color(p.secondary).single_line().truncate().shrink().min_w(0.0));
        }
    }
    if room > Room::Tight {
        r = r.child(forge_button(p, item, narrow));
    }
    if approves(item, page.mine) {
        r = r.child(approval_button(s, p, item, room));
    }
    col().w_full().child(r).child(rule(p))
}

/// Whether the bar offers Approve: on an open request that isn't mine (nobody approves their
/// own), whichever tab is open.
fn approves(item: &MergeRequest, mine: bool) -> bool {
    item.is_open() && !mine
}

/// The way out to the forge; on a narrow card its icon alone, named in its label and tip.
fn forge_button(p: &Pal, item: &MergeRequest, narrow: bool) -> El {
    let title = format!("Open in {}", item.kind().name());
    let button = if narrow {
        action_icon_button(p, "open-forge", &I::ARROW_UP_RIGHT, &title, false, true)
    } else {
        action_button(p, "open-forge", Some(&I::ARROW_UP_RIGHT), &title, false, true)
    };
    button.on_press_tx(actions::open_in_browser)
}

/// The rows of the Changes tab and of the Conversation tab with their keys, where the threads
/// go, and whether one has something new.
#[derive(Clone)]
struct PageRows {
    rows: Rc<Vec<Row>>,
    keys: Rc<Vec<String>>,
    talk_rows: Rc<Vec<Row>>,
    talk_keys: Rc<Vec<String>>,
    placed: Rc<Placed>,
    fresh: bool,
    /// Counts the times the rows were worked out, so the list re-reads its keys only then.
    version: u64,
}

/// What those rows are made of. Changes and threads compare by pointer: the store shares them.
struct Inputs {
    item: String,
    /// When I last looked at it before this visit, which decides what's new.
    seen: Option<f64>,
    diffs: Option<Loadable<Rc<Vec<FileDiff>>>>,
    talk: Option<Loadable<Rc<Vec<Discussion>>>>,
    head: Option<String>,
    /// How each file is folded: 0 open, 1 collapsed, 2 a large file expanded.
    folds: Vec<u8>,
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
            && self.seen == other.seen
            && same(&self.diffs, &other.diffs)
            && same(&self.talk, &other.talk)
            && self.head == other.head
            && self.folds == other.folds
    }
}

thread_local! {
    /// The last card's rows, kept while what they're made of stays the same: a big diff has tens
    /// of thousands of rows, too many to list again every frame, or on every switch of tab.
    static PAGE_ROWS: RefCell<Option<(Inputs, PageRows)>> = const { RefCell::new(None) };
    static VERSION: Cell<u64> = const { Cell::new(0) };
}

fn page_rows(
    s: &App,
    item: &MergeRequest,
    diffs: Option<&Loadable<Rc<Vec<FileDiff>>>>,
    talk: Option<&Loadable<Rc<Vec<Discussion>>>>,
    head: Option<String>,
) -> PageRows {
    let files: &[FileDiff] = diffs.and_then(Loadable::value).map_or(&[], |f| f.as_slice());
    // This runs for every file each time the view does: nothing folded costs nothing, and
    // otherwise one key per file serves both lookups.
    let folds = if s.collapsed.is_empty() && s.expanded_large.is_empty() {
        vec![0; files.len()]
    } else {
        files
            .iter()
            .map(|file| {
                let key = format!("{}|{}", item.id, file.id());
                if s.collapsed.contains(&key) {
                    1
                } else if s.expanded_large.contains(&key) {
                    2
                } else {
                    0
                }
            })
            .collect()
    };
    let inputs = Inputs { item: item.id.clone(), seen: s.store.seen_before.get(&item.id).copied(), diffs: diffs.cloned(), talk: talk.cloned(), head, folds };
    let cached = PAGE_ROWS.with(|c| c.borrow().as_ref().and_then(|(was, rows)| was.same(&inputs).then(|| rows.clone())));
    if let Some(rows) = cached {
        return rows;
    }
    let threads: &[Discussion] = talk.and_then(Loadable::value).map_or(&[], |t| t.as_slice());
    let placed = Placed::new(threads, inputs.head.as_deref(), files);
    let mut rows = Vec::new();
    match diffs {
        Some(Loadable::Loaded(_)) if files.is_empty() => rows.push(Row::Empty("No changes.")),
        Some(Loadable::Loaded(_)) => {
            rows.push(Row::FileList);
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
                } else if waits_for_click(file) && inputs.folds[f] != 2 {
                    rows.push(Row::Notice(f, format!("{} lines. Show the diff", file.lines.len()), true));
                } else {
                    rows.push(Row::Top(f));
                    rows.extend((0..file.lines.len()).map(|l| Row::Line(f, l)));
                }
                rows.push(Row::Footer(f));
            }
        }
        Some(Loadable::Failed(e)) => rows.push(Row::Failed(e.clone())),
        _ => rows.push(Row::Loading),
    }
    let keys = rows.iter().map(|r| r.key(files, threads)).collect();
    let talk_rows = conversation_rows(talk, threads.len());
    let talk_keys = talk_rows.iter().map(|r| r.key(files, threads)).collect();
    let fresh = inputs.seen.is_some() && threads.iter().any(|t| s.store.has_fresh(item, t));
    let version = VERSION.with(|v| {
        v.set(v.get() + 1);
        v.get()
    });
    let made = PageRows {
        rows: Rc::new(rows),
        keys: Rc::new(keys),
        talk_rows: Rc::new(talk_rows),
        talk_keys: Rc::new(talk_keys),
        placed: Rc::new(placed),
        fresh,
        version,
    };
    PAGE_ROWS.with(|c| *c.borrow_mut() = Some((inputs, made.clone())));
    made
}

/// A long or generated file, whose lines wait for a click.
fn waits_for_click(file: &FileDiff) -> bool {
    file.lines.len() > LARGE_FILE || file.generated_file
}

/// The Conversation tab's rows: every thread, those on a line too, then the comment box.
fn conversation_rows(talk: Option<&Loadable<Rc<Vec<Discussion>>>>, threads: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    match talk {
        Some(Loadable::Loaded(_)) => {
            if threads == 0 {
                rows.push(Row::Empty("No comments yet."));
            }
            rows.extend((0..threads).map(Row::Thread));
            rows.push(Row::Composer);
        }
        Some(Loadable::Failed(e)) => rows.push(Row::Failed(e.clone())),
        _ => rows.push(Row::Loading),
    }
    rows
}

/// Shows the tab `jump` is on, and scrolls to the place it names there: a file unfolds first,
/// and a line's file shows all its lines.
pub fn go(s: &mut App, tx: &mut Tx, jump: Jump) {
    let Some(id) = s.store.selected_id.clone() else { return };
    tx.instant();
    s.detail_tabs.insert(id.clone(), jump.tab());
    let files = s.store.diffs.get(&id).and_then(Loadable::value).cloned().unwrap_or_default();
    match jump {
        Jump::Tab(_) => {}
        Jump::File(f) => {
            s.spies.entry(id.clone()).or_default().aimed = f;
            match f.and_then(|f| files.get(f)) {
                Some(file) => {
                    s.collapsed.remove(&format!("{id}|{}", file.id()));
                    land(tx, Tab::Changes, format!("file:{}", file.id()), 0.0);
                }
                None => tx.scroll_to_instant(scroll_key(&id, Tab::Changes), 0.0, 0.0),
            }
        }
        Jump::Line(thread) => {
            let threads = s.store.discussions.get(&id).and_then(Loadable::value).cloned().unwrap_or_default();
            let head = s.store.diff_refs.get(&id).map(|r| r.head_sha.as_str());
            let Some((f, l)) = threads.iter().find(|t| t.id == thread).and_then(|t| line_of(t, head, &files)) else { return };
            let key = format!("{id}|{}", files[f].id());
            s.collapsed.remove(&key);
            if waits_for_click(&files[f]) {
                s.expanded_large.insert(key);
            }
            land(tx, Tab::Changes, format!("line:{f}:{l}"), LINE_ROOM);
        }
        Jump::Thread(thread) => land(tx, Tab::Conversation, format!("thread:{thread}"), 0.0),
        Jump::Composer => {
            tx.scroll_into_view_in(Tab::Conversation.list_key(), "composer");
            tx.focus(draft_field_key(&DraftKey::Comment(id)), false);
        }
    }
}

/// Scrolls `tab`'s row `key` to `at` points under the top of the page. A row far down a big diff
/// isn't built yet: revealing it builds it, then it lands.
fn land(tx: &mut Tx, tab: Tab, key: String, at: f32) {
    tx.scroll_into_view_in(tab.list_key(), key.clone());
    tx.scroll_key_to(format!("{}/{key}", tab.list_key()), at);
}

/// J and K: on the Changes tab, the next or previous file's header at the top; on another tab J
/// opens the Changes tab. False with nothing open.
pub fn step(s: &mut App, tx: &mut Tx, by: i64) -> bool {
    let Some(id) = s.store.selected_id.clone() else { return false };
    if s.tab(&id) != Tab::Changes {
        if by > 0 {
            go(s, tx, Jump::Tab(Tab::Changes));
        }
        return true;
    }
    let files = s.store.diffs.get(&id).and_then(Loadable::value).map_or(0, |f| f.len());
    let from = s.spies.get(&id).and_then(|spy| spy.aimed.or(spy.top));
    go(s, tx, Jump::File(step_file(from, by, files)));
    true
}

/// The first file of `item` I haven't viewed, or have and it changed since.
pub fn first_unviewed(s: &App, item: &MergeRequest) -> Option<usize> {
    let files = s.store.diffs.get(&item.id).and_then(Loadable::value)?;
    files.iter().position(|f| s.store.viewed(f, item) != Viewed::Yes)
}

/// Centres a row's content in its tab's column, the reading measure or the changes' wider one,
/// with the page's margins.
fn frame(tab: Tab, count: usize, i: usize, content: El) -> El {
    let width = if tab == Tab::Changes { PAGE_WIDTH } else { READING_WIDTH };
    let mut outer = row().w_full().px(44.0).justify_center();
    if i == 0 {
        outer = outer.pt(PAGE_TOP);
    }
    if i + 1 == count {
        outer = outer.pb(60.0);
    }
    outer.child(col().w_full().max_w(width).child(content))
}

fn build_row(s: &App, page: &Page, tab: Tab, rows: &[Row], i: usize) -> El {
    let p = &page.look.p;
    let k = &*page.look.k;
    let item = &page.item;
    let what = if tab == Tab::Changes { "changes" } else { "conversation" };
    let content = match &rows[i] {
        Row::Header => header(s, p, item),
        Row::Description => {
            let body = if item.description.trim().is_empty() {
                text("No description.").font(serif(16.0, 400.0).italic()).color(p.tertiary)
            } else {
                markdown::markdown(s, p, k, Doc { key: &item.id, source: &item.description, base: item.project_url(), size: 16.0, serif: true })
            };
            el().w_full().max_w(READING_WIDTH).pt(30.0).child(body)
        }
        Row::Standing if !item.is_open() => how_it_ended(p, item),
        Row::Standing if page.mine => before_it_merges(s, p, item),
        Row::Standing => standing(s, p, page),
        Row::Loading => row()
            .gap(8.0)
            .center_y()
            .child(Spinner::new().size(14.0).build(k))
            .child(text(format!("Loading the {what}…")).font(ui(13.0, 400.0)).color(p.secondary)),
        Row::Failed(message) => row()
            .gap(6.0)
            .items_start()
            .child(icon(&I::WARNING, 13.0).color(p.red))
            .max_w(READING_WIDTH)
            .child(text(format!("Couldn’t load the {what}: {message}")).font(ui(13.0, 400.0)).color(p.red).wrap().grow(1.0).shrink().min_w(0.0)),
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
            let head = if page.placed.at[*t].is_some() { Head::Link } else { Head::Place };
            el().w_full().pt(if i == 0 { 0.0 } else { 16.0 }).child(thread_card(s, p, k, item, &page.threads[*t], head))
        }
        Row::Composer => el().w_full().pt(if i == 0 { 0.0 } else { 24.0 }).child(composer(
            s,
            p,
            k,
            DraftKey::Comment(item.id.clone()),
            "Leave a comment. Markdown works.",
            "Comment",
            false,
        )),
    };
    frame(tab, rows.len(), i, content)
}

/// A part's title over a rule.
fn heading(p: &Pal, title: &str) -> El {
    col().w_full().pt(44.0).child(rule(p)).child(text(title.to_string()).font(serif(22.0, SEMIBOLD).lines(1.25)).color(p.text).pt(22.0).selectable(true))
}

/// Where the review stands, in one sentence: what's left to view and which threads are open.
fn sum_up(s: &App, page: &Page) -> String {
    let item = &page.item;
    let open = page.threads.iter().filter(|t| is_open(t)).count();
    let viewed = s.store.viewed_count(&page.files, item);
    let files = match page.files.len() - viewed {
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
    match (files, threads) {
        (Some(files), Some(threads)) => format!("{files}, and {threads}."),
        (Some(files), None) => format!("{files}."),
        (None, Some(threads)) => format!("{}{}.", threads[..1].to_uppercase(), &threads[1..]),
        (None, None) => "There’s nothing to sum up yet.".to_string(),
    }
}

/// The end of the overview of a request I'm reviewing: where the review stands, and the way into
/// the changes at the first file not viewed yet.
fn standing(s: &App, p: &Pal, page: &Page) -> El {
    let (title, jump) = match first_unviewed(s, &page.item) {
        Some(f) => {
            let path = &page.files[f].new_path;
            (format!("Start with {}", path.rsplit('/').next().unwrap_or(path)), Jump::File(Some(f)))
        }
        None => ("See the Changes".to_string(), Jump::Tab(Tab::Changes)),
    };
    let start = action_button(p, "start-changes", None, &title, false, true).on_press_tx(move |s: &mut App, tx: &mut Tx| go(s, tx, jump.clone()));
    col().w_full().pt(34.0).child(rule(p)).child(
        row()
            .w_full()
            .pt(20.0)
            .gaps(12.0, 16.0)
            .center_y()
            .flex_wrap()
            .child(text(sum_up(s, page)).font(serif(16.0, 400.0).lines(1.6)).color(p.text).wrap().selectable(true).grow(1.0).shrink().min_w(0.0))
            .child(start),
    )
}

/// The end of the overview of an open request of my own: what stands between it and its merge.
fn before_it_merges(s: &App, p: &Pal, item: &MergeRequest) -> El {
    col().w_full().gap(14.0).child(heading(p, "Before it merges")).child(crate::ui::mine::checklist(s, p, item))
}

/// The end of the overview of a request that's merged or closed, mine or not: there's nothing
/// left to review or to do.
fn how_it_ended(p: &Pal, item: &MergeRequest) -> El {
    let end = if item.state == "merged" { "was merged" } else { "was closed without merging" };
    col()
        .w_full()
        .gap(14.0)
        .child(heading(p, "How it ended"))
        .child(text(format!("This {} {end}.", item.kind().noun())).font(serif(16.0, 400.0).lines(1.6)).color(p.text).wrap().selectable(true))
}

/// The Conversation tab at the first thread someone can still resolve.
pub fn jump_to_open_thread(s: &mut App, tx: &mut Tx) {
    let Some(id) = s.store.selected_id.clone() else { return };
    let threads = s.store.discussions.get(&id).and_then(Loadable::value).cloned().unwrap_or_default();
    let open = threads.iter().find(|t| is_open(t)).map(|t| t.id.clone());
    go(s, tx, open.map_or(Jump::Tab(Tab::Conversation), Jump::Thread));
}

/// Whether a thread that can be resolved has been; none for one that can't (a plain comment).
fn resolved(thread: &Discussion) -> Option<bool> {
    let mut notes = thread.notes.iter().filter(|n| !n.system).peekable();
    (notes.peek()?.resolvable == Some(true)).then(|| notes.all(|n| n.resolved == Some(true)))
}

/// A thread someone can still resolve.
fn is_open(thread: &Discussion) -> bool {
    resolved(thread) == Some(false)
}

/// Approve, or Revoke Approval once approved, as the `room` allows: written out, then Revoke
/// for short, then their icons alone, named in full in their labels and tips.
fn approval_button(s: &App, p: &Pal, item: &MergeRequest, room: Room) -> El {
    let idle = s.store.busy.is_none();
    if i_approved(s, item) {
        let it = item.clone();
        let revoke = match room {
            Room::Wide => action_button(p, "revoke", Some(&I::UNDO), "Revoke Approval", false, idle),
            Room::Narrow => action_button(p, "revoke", Some(&I::UNDO), "Revoke", false, idle).label("Revoke Approval").help("Revoke Approval"),
            Room::Tight => action_icon_button(p, "revoke", &I::UNDO, "Revoke Approval", false, idle),
        };
        revoke.on_press(move |s: &mut App| {
            if s.store.busy.is_none() {
                s.store.unapprove(&it)
            }
        })
    } else {
        let approve = match room {
            Room::Tight => action_icon_button(p, "approve", &I::CHECK, "Approve", true, idle),
            _ => action_button(p, "approve", Some(&I::CHECK), "Approve", true, idle),
        };
        approve.help(keys("Approve  ⇧⌘A")).on_press_tx(actions::approve)
    }
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

fn header(s: &App, p: &Pal, item: &MergeRequest) -> El {
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
    c.child(reviewers(s, p, item))
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

// MARK: Changes

fn file_list(s: &App, p: &Pal, page: &Page) -> El {
    let item = &page.item;
    let files = &page.files;
    let viewed = s.store.viewed_count(files, item);
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
                .on_press_tx(move |s: &mut App, tx: &mut Tx| go(s, tx, Jump::File(Some(f)))),
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
    for &thread in page.placed.on(f, l) {
        c = c.child(el().w_full().px(14.0).py(8.0).fill(p.well).child(thread_card(s, p, k, item, &page.threads[thread], Head::Bare).max_w(760.0)));
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

/// Where each thread sits in the changes: under the diff line it's about, when a line row shows
/// that line. The rest (general comments, and positions outdated or not in the diff) are in the
/// conversation only, where every thread is.
#[derive(Default)]
struct Placed {
    /// Each thread's file and line, by the thread's index.
    at: Vec<Option<(usize, usize)>>,
    by_line: HashMap<(usize, usize), Vec<usize>>,
    /// The threads under each file's lines.
    by_file: Vec<Vec<usize>>,
}

impl Placed {
    fn new(threads: &[Discussion], head: Option<&str>, files: &[FileDiff]) -> Placed {
        let mut placed = Placed { by_file: vec![Vec::new(); files.len()], ..Placed::default() };
        placed.at = threads.iter().map(|thread| line_of(thread, head, files)).collect();
        for (index, at) in placed.at.iter().enumerate() {
            if let Some((f, l)) = *at {
                placed.by_line.entry((f, l)).or_default().push(index);
                placed.by_file[f].push(index);
            }
        }
        placed
    }

    /// The threads under line `l` of file `f`.
    fn on(&self, f: usize, l: usize) -> &[usize] {
        self.by_line.get(&(f, l)).map_or(&[], Vec::as_slice)
    }
}

/// The file and the line of it a thread is on, when the diff shows that line and the thread was
/// left on this version of it.
fn line_of(thread: &Discussion, head: Option<&str>, files: &[FileDiff]) -> Option<(usize, usize)> {
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
    let l = files[f].lines.iter().position(|l| match l.kind {
        LineKind::Added | LineKind::Context => new && l.new_number == Some(number),
        LineKind::Removed => !new && l.old_number == Some(number),
        _ => false,
    })?;
    Some((f, l))
}

// MARK: Discussion

/// What a thread card shows above its notes.
#[derive(Clone, Copy, PartialEq)]
enum Head {
    /// Nothing: it sits under its line.
    Bare,
    /// The file and line it was left on.
    Place,
    /// That place, as a way to its line in the changes.
    Link,
}

fn thread_card(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, thread: &Discussion, shows: Head) -> El {
    let notes: Vec<&Note> = thread.notes.iter().filter(|n| !n.system).collect();
    let resolved = resolved(thread) == Some(true);
    let mut c = col().key(format!("thread-{}", thread.id)).w_full().radius(10.0).fill(p.paper).border(1.0, p.strong_line).clip_rounded(10.0);
    if shows != Head::Bare {
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
                if shows == Head::Link {
                    let id = thread.id.clone();
                    head = head
                        .key("show-line")
                        .role("button")
                        .label(format!("{path}{at}"))
                        .help("Show this line in the changes")
                        .hover_group()
                        .hover(p.hover)
                        .child(icon(&I::CHEVRON_RIGHT, 10.0).color(p.tertiary).group_color(p.text))
                        .on_press_tx(move |s: &mut App, tx: &mut Tx| go(s, tx, Jump::Line(id.clone())));
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
            if shows == Head::Bare && resolved {
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
    fn files_step_one_at_a_time_and_stop_at_the_ends() {
        assert_eq!(step_file(None, 1, 3), Some(0));
        assert_eq!(step_file(Some(0), 1, 3), Some(1));
        assert_eq!(step_file(Some(2), 1, 3), Some(2), "J on the last file stays there");
        assert_eq!(step_file(Some(1), -1, 3), Some(0));
        assert_eq!(step_file(Some(0), -1, 3), None, "K on the first file goes to the top of the tab");
        assert_eq!(step_file(None, -1, 3), None);
        assert_eq!(step_file(None, 1, 0), None, "no files, nowhere to go");
        assert_eq!(step_file(Some(7), -1, 3), Some(1), "a file the diff no longer has counts as its last");
    }

    #[test]
    fn jumps_open_the_tab_their_place_is_on() {
        assert_eq!(Jump::File(Some(2)).tab(), Tab::Changes);
        assert_eq!(Jump::Line("t1".into()).tab(), Tab::Changes);
        assert_eq!(Jump::Thread("t1".into()).tab(), Tab::Conversation);
        assert_eq!(Jump::Composer.tab(), Tab::Conversation);
    }

    #[test]
    fn every_thread_is_in_the_conversation_and_those_on_a_line_under_it_once() {
        let (_, diffs, threads) = crate::fixtures::load();
        let (id, files) = diffs.into_iter().find(|(_, files)| files[0].new_path == "src/export/job.rs").unwrap();
        let threads = &threads[&id];
        let talk = Some(Loadable::Loaded(Rc::new(threads.clone())));
        let rows = conversation_rows(talk.as_ref(), threads.len());
        let listed: Vec<usize> = rows.iter().filter_map(|r| if let Row::Thread(t) = r { Some(*t) } else { None }).collect();
        assert_eq!(listed, (0..threads.len()).collect::<Vec<_>>(), "every thread, in order");
        assert_eq!(rows.last(), Some(&Row::Composer), "it ends with the comment box");
        assert_eq!(conversation_rows(talk.as_ref(), 0)[0], Row::Empty("No comments yet."));

        let placed = Placed::new(threads, None, &files);
        // One on a line of job.rs; a general note, and one on a file the diff doesn't have.
        assert_eq!(placed.at.iter().flatten().count(), 1);
        assert_eq!(placed.by_file[0].len(), 1);
        let lines: Vec<usize> = (0..files[0].lines.len()).filter(|&l| !placed.on(0, l).is_empty()).collect();
        assert_eq!(lines.len(), 1, "under one line only");
        assert_eq!(placed.at[placed.on(0, lines[0])[0]], Some((0, lines[0])), "a jump to its line finds the row it's under");
        // The fixture's positions name no commit, so they hold for any head.
        assert_eq!(Placed::new(threads, Some("abc"), &files).at, placed.at);
    }

    #[test]
    fn the_bar_tightens_as_the_card_narrows() {
        assert_eq!(room([0.0, 0.0, 900.0, 600.0]), Room::Wide);
        assert_eq!(room([0.0, 0.0, 600.0, 600.0]), Room::Narrow);
        // The Ask panel open beside the default sidebar in an 1100pt window.
        assert_eq!(room([0.0, 0.0, 384.0, 600.0]), Room::Tight);
    }

    #[test]
    fn approve_shows_on_others_open_requests() {
        let (accounts, ..) = crate::fixtures::load();
        let account = accounts.values().next().unwrap();
        let open = account.open.iter().find(|m| m.is_open()).unwrap();
        let merged = account.done.iter().find(|m| !m.is_open()).unwrap();
        assert!(approves(open, false));
        assert!(!approves(open, true), "nobody approves their own");
        assert!(!approves(merged, false));
    }
}
