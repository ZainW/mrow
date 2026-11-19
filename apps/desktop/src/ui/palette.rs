//! The command palette (⇧⌘P): every command in `actions::COMMANDS` that would do something now,
//! found by typing part of its title, and below them the requests in the current list, found by
//! title or number. It opens and closes at once: it's opened many times a day.

use rune::{El, ScrollSpec, TextEdit, Tx, col, el, icon, row, text};
use rune_kit::{Kit, TextInput};

use crate::app::App;
use crate::ui::actions::{self, Listed};
use crate::ui::components::{keys, rule};
use crate::ui::icons as I;
use crate::ui::theme::{HEADER_HEIGHT, MEDIUM, Pal, mono, shade, ui};

/// The query's field, focused when the palette opens.
const FIELD: &str = "palette-query/input";
/// The list the rows sit in, for revealing the one the keyboard is on.
const LIST: &str = "palette-list";
/// The most requests it lists: enough to find one by a word, few enough to read.
const MOST_REQUESTS: usize = 20;

/// What's typed, and the row the keyboard is on.
pub struct Palette {
    pub query: TextEdit,
    pub pick: usize,
}

/// One row: a command, or a request in the current list to go to.
pub enum Entry {
    Command(Listed),
    Request { id: String, reference: String, title: String },
}

impl Entry {
    fn key(&self) -> String {
        match self {
            Entry::Command(c) => format!("cmd-{}", c.id),
            Entry::Request { id, .. } => format!("go-{id}"),
        }
    }
}

pub fn toggle(s: &mut App, tx: &mut Tx) {
    tx.instant();
    if s.palette.take().is_none() {
        s.palette = Some(Palette { query: TextEdit::new(""), pick: 0 });
        tx.focus(FIELD, false);
    }
}

fn close(s: &mut App, tx: &mut Tx) {
    tx.instant();
    s.palette = None;
}

/// How well `query` fits `hay`, higher is better; None when it doesn't. In order: `hay` starts
/// with it, a word in `hay` does, its letters start the words ("cl" for Copy Link), it's
/// somewhere inside, or its letters are, in order, with word starts counting for more.
pub fn fit(query: &str, hay: &str) -> Option<i32> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let h: Vec<char> = hay.to_lowercase().chars().collect();
    let starts_word = |i: usize| i == 0 || !h[i - 1].is_alphanumeric();
    let at = |i: usize| h[i..].starts_with(&q);
    let found: Vec<usize> = (0..h.len()).filter(|&i| at(i)).collect();
    if found.first() == Some(&0) {
        return Some(5000);
    }
    if let Some(&i) = found.iter().find(|&&i| starts_word(i)) {
        return Some(4000 - i as i32);
    }
    let letters: Vec<char> = q.iter().copied().filter(|c| !c.is_whitespace()).collect();
    let mut words = (0..h.len()).filter(|&i| starts_word(i) && h[i].is_alphanumeric());
    if letters.iter().all(|c| words.any(|i| h[i] == *c)) {
        return Some(3000);
    }
    if let Some(&i) = found.first() {
        return Some(2000 - i as i32);
    }
    // Letters in order, each taken at the next word start it can be, else the next place.
    let (mut from, mut score, mut last) = (0, 1000, None::<usize>);
    for c in letters {
        let word = (from..h.len()).find(|&i| h[i] == c && starts_word(i));
        let any = (from..h.len()).find(|&i| h[i] == c)?;
        let i = word.filter(|&w| w == any || last.is_none_or(|l| w - l < 4)).unwrap_or(any);
        if starts_word(i) {
            score += 10;
        }
        if let Some(l) = last {
            score -= (i - l - 1).min(10) as i32;
        }
        last = Some(i);
        from = i + 1;
    }
    Some(score)
}

/// How well `query` names a request: by its number ("231", "!231"), or by its title.
fn request_fit(query: &str, reference_mark: &str, iid: &str, title: &str) -> Option<i32> {
    let number = query.trim().trim_start_matches(reference_mark);
    if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) && iid.starts_with(number) {
        return Some(if iid == number { 5100 } else { 5000 });
    }
    fit(query, title)
}

/// Best first, keeping the given order between equals, and dropping what doesn't fit.
fn ranked<T>(items: Vec<T>, score: impl Fn(&T) -> Option<i32>) -> Vec<T> {
    let mut scored: Vec<(i32, T)> = items.into_iter().filter_map(|t| score(&t).map(|n| (n, t))).collect();
    scored.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    scored.into_iter().map(|(_, t)| t).collect()
}

/// The rows for what's typed: the commands that fit (all of them, in the table's order, before
/// anything is typed), then the requests in the current list that do.
pub fn entries(s: &App) -> Vec<Entry> {
    let query = s.palette.as_ref().map_or("", |p| p.query.text.as_str()).trim();
    let commands = ranked(actions::listed(s), |c| fit(query, &c.title));
    let mut out: Vec<Entry> = commands.into_iter().map(Entry::Command).collect();
    if query.is_empty() {
        return out;
    }
    let requests = ranked(s.store.visible_items(), |m| request_fit(query, m.kind().reference_mark(), &m.iid, &m.title));
    out.extend(requests.into_iter().take(MOST_REQUESTS).map(|m| Entry::Request { id: m.id.clone(), reference: m.reference(), title: m.title }));
    out
}

fn picked(s: &App, count: usize) -> usize {
    s.palette.as_ref().map_or(0, |p| p.pick).min(count.saturating_sub(1))
}

/// Moves the keyboard's row by `by`, round the ends as a menu does, and brings it into view.
fn step(s: &mut App, tx: &mut Tx, by: i64) {
    tx.instant();
    let list = entries(s);
    if list.is_empty() {
        return;
    }
    let next = (picked(s, list.len()) as i64 + by).rem_euclid(list.len() as i64) as usize;
    if let Some(p) = &mut s.palette {
        p.pick = next;
    }
    tx.scroll_into_view(format!("{LIST}/{}", list[next].key()));
}

/// Closes the palette, then runs the row at `index`.
fn run(s: &mut App, tx: &mut Tx, index: usize) {
    let Some(entry) = entries(s).into_iter().nth(index) else { return };
    close(s, tx);
    match entry {
        Entry::Command(c) => (c.run)(s, tx),
        Entry::Request { id, .. } => {
            s.store.select(Some(id.clone()));
            tx.scroll_into_view(format!("rows/{id}"));
        }
    }
}

/// ⌃N and ⌃P, as in a Mac text field's list (and Emacs): Ctrl+N and Ctrl+P elsewhere.
const NEXT: rune::Chord = rune::Chord::new(rune::ChordKey::Char('n')).with(rune::ChordMod::Ctrl);
const PREVIOUS: rune::Chord = rune::Chord::new(rune::ChordKey::Char('p')).with(rune::ChordMod::Ctrl);

/// ↑ and ↓ (and ⌃N, ⌃P) move through the rows from the field, which keeps the caret.
fn palette_keys(s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
    match ev.key {
        rune::Key::Down => step(s, tx, 1),
        rune::Key::Up => step(s, tx, -1),
        _ if NEXT.matches_tx(ev, tx) => step(s, tx, 1),
        _ if PREVIOUS.matches_tx(ev, tx) => step(s, tx, -1),
        _ => return false,
    }
    true
}

/// What the list says when nothing fits: why, when the query names a command that can't run now
/// (the palette lists only those that can).
fn empty_line(s: &App, query: &str) -> String {
    let hidden = ranked(actions::unavailable(s), |(title, _)| fit(query, title).filter(|&n| n >= 3000));
    match hidden.first() {
        Some((title, why)) => format!("{} can’t run now. {why}", title.trim_end_matches('…')),
        None => format!("Nothing matches “{query}”."),
    }
}

fn caption(p: &Pal, title: &str) -> El {
    el().w_full().px(10.0).pt(8.0).pb(4.0).child(text(title.to_string()).font(ui(12.0, MEDIUM)).color(p.tertiary).single_line())
}

/// A row, filled while the keyboard is on it; the pointer's hover is the lighter fill.
fn entry_row(p: &Pal, entry: &Entry, index: usize, on: bool) -> El {
    let (label, body) = match entry {
        Entry::Command(c) => {
            let mut r = row()
                .gap(12.0)
                .center_y()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .child(text(c.title.clone()).font(ui(13.0, 400.0)).color(p.text).single_line().truncate().grow(1.0).shrink().min_w(0.0));
            if let Some(k) = &c.keys {
                r = r.child(text(k.clone()).font(mono(12.0, 400.0)).color(if on { p.secondary } else { p.tertiary }).single_line());
            }
            let label = match &c.keys {
                Some(k) => format!("{}, {k}", c.title),
                None => c.title.clone(),
            };
            (label, r)
        }
        Entry::Request { reference, title, .. } => {
            let r = row()
                .gap(8.0)
                .center_y()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .child(text(reference.clone()).font(mono(12.0, MEDIUM).features("'tnum' 1")).color(p.secondary).single_line())
                .child(text(title.clone()).font(ui(13.0, 400.0)).color(p.text).single_line().truncate().grow(1.0).shrink().min_w(0.0));
            // A long title truncates: its tip has the rest.
            (format!("Go to {reference}, {title}"), r.help(title.clone()))
        }
    };
    let mut r = row()
        .key(entry.key())
        .role("option")
        .label(label)
        .focus_selected(on)
        .w_full()
        .h(32.0)
        .px(10.0)
        .center_y()
        // The panel's 12 less the list's 6 inset, so the corners run parallel.
        .radius(6.0)
        .child(body)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| run(s, tx, index));
    r = if on { r.fill(p.selected) } else { r.hover(p.hover).press(p.hover) };
    r
}

/// The palette, near the top of the window, over everything else.
pub fn panel(s: &App, p: &Pal, k: &Kit) -> Option<El> {
    let palette = s.palette.as_ref()?;
    let list = entries(s);
    let on = picked(s, list.len());
    let noun = s.store.forge.noun();
    let field = TextInput::new(&palette.query)
        .plain()
        .width(None)
        .height(30.0)
        .text_style(ui(14.0, 400.0))
        .spellcheck(false)
        .placeholder(format!("Run a command, or find a {noun} by title or number"))
        .label(format!("Search commands and {noun}s"))
        .active_descendant(list.get(on).map(Entry::key))
        .on_edit(|s: &mut App| &mut s.palette.get_or_insert_with(|| Palette { query: TextEdit::new(""), pick: 0 }).query)
        .on_change(|s: &mut App, _: &mut Tx| {
            if let Some(p) = &mut s.palette {
                p.pick = 0;
            }
        })
        .on_key(palette_keys)
        .on_submit(move |s: &mut App, tx: &mut Tx| {
            let count = entries(s).len();
            if count > 0 {
                run(s, tx, picked(s, count));
            }
        })
        .build(k);
    let head = row()
        .key("palette-query")
        .w_full()
        .h(48.0)
        .pl(16.0)
        .pr(12.0)
        .gap(10.0)
        .center_y()
        .child(icon(&I::SEARCH, 14.0).color(p.tertiary))
        .child(field.grow(1.0).shrink().min_w(0.0));

    let query = palette.query.text.trim();
    let mut rows = col().key(LIST).role("listbox").label(format!("Commands and {noun}s")).w_full().px(6.0).py(6.0);
    // Captions only once requests join the commands, to tell the two apart.
    let first_request = list.iter().position(|e| matches!(e, Entry::Request { .. }));
    if first_request.is_some_and(|i| i > 0) {
        rows = rows.child(caption(p, "Commands"));
    }
    if list.is_empty() {
        rows = rows.child(el().w_full().px(10.0).py(8.0).child(text(empty_line(s, query)).font(ui(13.0, 400.0)).color(p.secondary).wrap()));
    }
    for (i, entry) in list.iter().enumerate() {
        if first_request == Some(i) {
            rows = rows.child(caption(p, "Go to"));
        }
        rows = rows.child(entry_row(p, entry, i, i == on));
    }
    let scroller = col().key("palette-scroll").w_full().shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows);
    let hint = text(keys("↑↓ to choose   ↩ to run   esc to close")).font(ui(12.0, 400.0)).color(p.tertiary).single_line().truncate();
    let panel = col()
        .key("palette")
        .role("dialog")
        .label("Command Palette")
        .w_full()
        .max_w(600.0)
        .shrink()
        .min_h(0.0)
        .max_h(460.0)
        .radius(12.0)
        .fill(p.elevated)
        .border(1.0, p.line)
        .shadow(8.0, 28.0, shade(if p.dark { 0.45 } else { 0.14 }))
        .instant_changes()
        .child(head)
        .child(rule(p))
        .child(scroller)
        .child(rule(p))
        .child(el().w_full().px(16.0).py(9.0).child(hint))
        .modal_tx(true, close);
    // Under the header, centred on the window; the layer gives clicks outside the panel to the
    // modal, which closes it.
    // No fade in or out on any path, Escape and a click outside included: it's opened all day.
    Some(
        col()
            .key("palette-layer")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .items_center()
            .pt(HEADER_HEIGHT + 32.0)
            .px(16.0)
            .pb(16.0)
            .transition(rune::Transition::none())
            .child(panel),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order<'a>(query: &str, titles: &[&'a str]) -> Vec<&'a str> {
        ranked(titles.to_vec(), |t| fit(query, t))
    }

    #[test]
    fn nothing_typed_keeps_the_table_order() {
        let titles = ["Refresh", "Approve", "Copy Link"];
        assert_eq!(order("", &titles), titles);
        assert_eq!(order("   ", &titles), titles);
    }

    #[test]
    fn matching_ignores_case_and_drops_what_does_not_fit() {
        assert_eq!(order("APPR", &["Refresh", "Approve"]), ["Approve"]);
        assert_eq!(fit("xyz", "Approve"), None);
    }

    #[test]
    fn prefixes_and_word_starts_rank_above_scattered_letters() {
        // A prefix first, then words that start with it, the earlier word first.
        assert_eq!(order("re", &["Mark All as Read", "Finish Review…", "Refresh"]), ["Refresh", "Finish Review…", "Mark All as Read"]);
        // A word's start beats the same letters inside a word.
        assert_eq!(order("view", &["Overview", "Finish Review…", "View Changes"]), ["View Changes", "Overview", "Finish Review…"]);
        // Initials beat letters scattered through a word.
        assert_eq!(order("cl", &["Uncle", "Copy Link"]), ["Copy Link", "Uncle"]);
        // Scattered letters still fit, with word starts counting for more.
        assert!(fit("tsb", "Toggle Sidebar").is_some());
        assert!(fit("nxt", "Show Next Tab") > fit("nxt", "Unexpected Text"));
    }

    #[test]
    fn requests_are_found_by_number_with_or_without_their_mark() {
        assert_eq!(request_fit("231", "!", "231", "Remove the v1 export"), Some(5100));
        assert_eq!(request_fit("!23", "!", "231", "Remove the v1 export"), Some(5000));
        assert_eq!(request_fit("#23", "#", "231", "Remove the v1 export"), Some(5000));
        assert_eq!(request_fit("v1", "!", "231", "Remove the v1 export"), Some(4000 - 11));
        assert_eq!(request_fit("42", "!", "231", "Remove the v1 export"), None);
    }

    /// The fictional queue, with what's typed in an open palette.
    fn app(query: &str) -> App {
        crate::data::prefs::set_ephemeral(Some(crate::data::prefs::Prefs::default()));
        let mut s = App::default();
        let (accounts, diffs, threads) = crate::fixtures::load();
        s.store.load_fixture(accounts, diffs, threads);
        s.store.forge = crate::data::forge::Forge::Gitlab;
        s.palette = Some(Palette { query: TextEdit::new(query), pick: 0 });
        s
    }

    fn titles(s: &App) -> Vec<String> {
        entries(s)
            .into_iter()
            .map(|e| match e {
                Entry::Command(c) => c.title,
                Entry::Request { reference, .. } => reference,
            })
            .collect()
    }

    #[test]
    fn commands_that_cannot_run_are_left_out_and_say_why_when_asked_for() {
        let mut s = app("approve");
        s.store.select(None);
        assert!(titles(&s).is_empty());
        assert_eq!(empty_line(&s, "approve"), "Approve can’t run now. Open a merge request first.");
        assert!(!titles(&app("")).iter().any(|t| t == "Command Palette…"), "the palette doesn't list itself");

        let first = s.store.visible_items().first().map(|m| m.id.clone());
        s.store.select(first);
        assert_eq!(titles(&s).first().map(String::as_str), Some("Approve"));
    }

    #[test]
    fn requests_in_the_list_follow_the_commands() {
        let s = app("214");
        let item = s.store.visible_items().into_iter().find(|m| m.iid == "214").expect("a fixture request");
        assert_eq!(titles(&s), [item.reference()]);
        assert!(matches!(entries(&app("")).last(), Some(Entry::Command(_))), "nothing typed lists commands only");
    }
}
