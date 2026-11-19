//! The queue: the account, the three modes, the filter and the rows, grouped by when each
//! review was asked for.

use std::rc::Rc;

use rune::{El, GrowthSpec, PointerPress, PopupItem, PopupMenu, ScrollSpec, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Spinner, TextInput};

use crate::app::App;
use crate::data::models::MergeRequest;
use crate::data::prefs::{self, Appearance};
use crate::data::snooze;
use crate::data::store::{QueueMode, Store};
use crate::ui::components::{Chrome, ChromeButton, avatar, forge_mark, long_date, message, pipeline_dot, short_time, state_color, state_icon};
use crate::ui::icons as I;
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{MEDIUM, Pal, SEMIBOLD, mono, ui};
use crate::ui::{actions, menus};

/// A line in the list: a section's title, a merge request, or a line of the snoozed fold.
enum Line {
    Header(&'static str),
    Item(MergeRequest, bool, bool),
    Fold(Fold),
}

pub fn sidebar(s: &App, p: &Pal, k: &Kit, dark: bool) -> El {
    let store = &s.store;
    let mut c = col().key("sidebar").w(s.sidebar_width).h_full().min_h(0.0).fill(p.chrome);
    let top = row().key("account-bar").w_full().gap(2.0).center_y().child(account_switcher(store, p)).child(crate::ui::inbox::bell(s, p));
    c = c.child(el().px(8.0).pt(2.0).pb(6.0).child(top));
    c = c.child(mode_bar(store, p).px(8.0));
    let oldest = prefs::read(|p| p.oldest_first);
    let mut filter_row = row().key("filter-row").gap(2.0).pl(10.0).pr(6.0).pt(10.0).center_y().child(filter_box(s, p, k));
    // Mine is all one author, sorted by whose move it is: it filters by standing instead.
    if store.mode == QueueMode::Mine {
        c = c.child(filter_row.pr(10.0)).child(el().px(10.0).pt(8.0).child(crate::ui::mine::filters(store, p)));
    } else {
        filter_row = filter_row.child(author_menu(store, p)).child(
            ChromeButton::new("sort", if oldest { "Oldest first. Click for newest first" } else { "Newest first. Click for oldest first" })
                .icon(if oldest { &I::SORT_UP } else { &I::SORT_DOWN }, 13.0)
                .on(oldest)
                .build(p)
                .on_press(|_: &mut App| prefs::update(|p| p.oldest_first = !p.oldest_first)),
        );
        c = c.child(filter_row);
        if let Some(name) = selected_author(store) {
            c = c.child(el().px(10.0).pt(8.0).child(author_chip(s, p, &name)));
        }
    }
    c = c.child(list(s, p));
    c.child(footer(s, p, dark))
}

fn account_switcher(store: &Store, p: &Pal) -> El {
    let forge = store.forge;
    let subtitle = if store.error().is_some() {
        Some("Offline".to_string())
    } else {
        store.me().map(|me| {
            let host = forge.cli().host();
            if host.is_empty() { format!("@{}", me.username) } else { format!("@{} · {host}", me.username) }
        })
    };
    let mut names = col().child(text(forge.name()).font(ui(13.0, SEMIBOLD)).color(p.text).single_line());
    if let Some(sub) = subtitle {
        let color = if store.error().is_some() { p.red } else { p.tertiary };
        names = names.child(text(sub).font(ui(12.0, 400.0)).color(color).single_line().truncate());
    }
    // What waits in the other accounts, each beside its forge's mark, so the count says where.
    let others: Vec<_> = store.forges().into_iter().filter(|f| *f != forge).map(|f| (f, store.waiting_count(f))).filter(|(_, n)| *n > 0).collect();
    let mut label = format!("Account: {}", forge.name());
    for (other, n) in &others {
        label.push_str(&format!(", {n} waiting in {}", other.name()));
    }
    let mut r = row()
        .key("account")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .role("button")
        .label(label)
        .h(38.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .hover(p.hover)
        .press(p.hover)
        .help(if cfg!(target_os = "macos") { "Switch account  ⌃1 ⌃2" } else { "Switch account  Alt+1 Alt+2" })
        .child(forge_mark(p, forge, 20.0))
        .child(names.shrink().min_w(0.0))
        .child(icon(&I::CHEVRON_UP_DOWN, 10.0).color(p.tertiary))
        .child(el().grow(1.0));
    for (other, n) in others {
        r = r.child(
            row()
                .h(18.0)
                .pl(3.0)
                .pr(6.0)
                .gap(4.0)
                .center_y()
                .radius(9.0)
                .fill(p.accent_soft)
                .help(format!("{n} waiting in {}", other.name()))
                .child(forge_mark(p, other, 12.0))
                .child(text(n.to_string()).font(ui(11.0, SEMIBOLD).features("'tnum' 1")).color(p.accent).single_line()),
        );
    }
    r.on_press_tx(|s: &mut App, tx: &mut Tx| menus::accounts(s, tx))
}

fn mode_bar(store: &Store, p: &Pal) -> El {
    // Four titles and their counts fit the default width on one line; a narrower sidebar or
    // longer counts wrap them onto a second instead of clipping the last.
    let mut r = row().key("modes").w_full().gap(2.0).center_y().flex_wrap();
    for (index, mode) in QueueMode::ALL.into_iter().enumerate() {
        let count = store.count(mode);
        let title = if count > 0 && mode != QueueMode::Done { format!("{} {count}", mode.title()) } else { mode.title().to_string() };
        let b = ChromeButton::new(format!("mode-{index}"), title)
            .style(Chrome::Text)
            .pad(7.0)
            .on(store.mode == mode)
            .build(p)
            .help(crate::ui::components::keys(&format!("{}  ⌘{}", mode.title(), index + 1)))
            .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                // Switched often, from the keyboard too: the list changes at once.
                tx.instant();
                s.store.set_mode(mode);
            });
        r = r.child(b);
    }
    r
}

fn filter_box(s: &App, p: &Pal, k: &Kit) -> El {
    let noun = s.store.forge.plural_noun();
    let input = TextInput::new(&s.filter)
        .plain()
        .width(None)
        .height(28.0)
        .text_style(ui(13.0, 400.0))
        .placeholder(format!("Filter {noun}"))
        .on_edit(|s: &mut App| &mut s.filter)
        .on_change(|s: &mut App, _: &mut Tx| {
            s.store.filter = s.filter.text.clone();
        })
        .on_key(|s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| match ev.key {
            rune::Key::Escape => {
                s.filter.clear();
                s.store.filter.clear();
                tx.focus("queue", false);
                true
            }
            rune::Key::Down => {
                s.store.select_offset(1);
                tx.focus("queue", false);
                true
            }
            _ => false,
        })
        .on_submit(|_: &mut App, tx: &mut Tx| tx.focus("queue", false))
        .build(k);
    let mut b = row()
        .key("filter")
        .h(28.0)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .pl(9.0)
        .pr(4.0)
        .gap(3.0)
        .center_y()
        .radius(7.0)
        .fill(p.hover)
        .child(icon(&I::FILTER, 12.0).color(p.tertiary))
        .child(input.grow(1.0).shrink().min_w(0.0));
    if !s.filter.text.is_empty() {
        b = b.child(el().key("clear").role("button").label("Clear the filter").size(20.0, 28.0).center().help("Clear the filter").child(filled_x(p)).on_press(
            |s: &mut App| {
                s.filter.clear();
                s.store.filter.clear();
            },
        ));
    }
    b
}

fn filled_x(p: &Pal) -> El {
    crate::ui::components::filled_mark(&I::DISC, &I::SMALL_X, p.tertiary, p.chrome, 13.0)
}

fn selected_author(store: &Store) -> Option<String> {
    let username = store.author_filter.as_ref()?;
    let name = store.open().iter().chain(store.done()).find(|m| &m.author.username == username).map(|m| m.author.name.clone());
    Some(name.unwrap_or_else(|| username.clone()))
}

fn author_menu(store: &Store, p: &Pal) -> El {
    let on = store.author_filter.is_some();
    ChromeButton::new("author", "Filter by Author").icon(if on { &I::PERSON_FILL } else { &I::PERSON }, 13.0).on(on).build(p).on_press_tx(
        |s: &mut App, tx: &mut Tx| {
            let mut menu = PopupMenu::new().item(PopupItem::new("", "Everyone").checked(s.store.author_filter.is_none())).separator();
            for (person, count) in s.store.authors() {
                let on = s.store.author_filter.as_deref() == Some(person.username.as_str());
                menu = menu.item(PopupItem::new(person.username.clone(), format!("{}  {count}", person.name)).checked(on));
            }
            tx.popup_menu(menu, "author", |s: &mut App, picked, _| {
                if let Some(id) = picked {
                    s.store.author_filter = if id.is_empty() { None } else { Some(id.to_string()) };
                }
            });
        },
    )
}

fn author_chip(s: &App, p: &Pal, name: &str) -> El {
    let store = &s.store;
    let username = store.author_filter.clone().unwrap_or_default();
    let person = crate::data::models::Person::new(&username, name);
    row()
        .key("author-chip")
        .h(26.0)
        .pl(7.0)
        .pr(4.0)
        .gap(7.0)
        .center_y()
        .radius(7.0)
        .fill(p.accent_soft)
        .child(avatar(s, p, &person, 16.0))
        .child(text(format!("By {name}")).font(ui(12.0, MEDIUM)).color(p.text).single_line().truncate().grow(1.0).shrink().min_w(0.0))
        .child(
            el().key("everyone")
                .role("button")
                .label("Show everyone's")
                .size(22.0, 22.0)
                .center()
                .help("Show everyone's")
                .child(icon(&I::SMALL_X, 12.0).color(p.secondary))
                .on_press(|s: &mut App| s.store.author_filter = None),
        )
}

fn list(s: &App, p: &Pal) -> El {
    let store = &s.store;
    let empty = || el().key("queue").grow(1.0).min_h(0.0).w_full().center().focusable(true).child(empty_state(s, p));
    if store.mode == QueueMode::Mine {
        return match crate::ui::mine::rows(s, p) {
            Some(rows) => queue(rows),
            None => empty(),
        };
    }
    let sections = store.sections();
    let folds = fold_lines(store, sections.is_empty());
    if sections.is_empty() && folds.is_empty() {
        return empty();
    }
    let mut lines = Vec::new();
    for section in sections {
        lines.push(Line::Header(section.title));
        for item in section.items {
            let selected = store.selected_id.as_deref() == Some(item.id.as_str());
            let unread = store.is_unread(&item);
            lines.push(Line::Item(item, selected, unread));
        }
    }
    lines.extend(folds.into_iter().map(Line::Fold));
    let lines = Rc::new(lines);
    let pal = *p;
    let keys = lines.clone();
    let version: Vec<&str> = lines
        .iter()
        .map(|l| match l {
            Line::Header(t) => *t,
            Line::Item(m, ..) => m.id.as_str(),
            Line::Fold(f) => f.key(),
        })
        .collect();
    let rows = col().key("rows").w_full().pb(8.0).growth(GrowthSpec::default()).virtual_rows_version(&version).virtual_rows(
        lines.len(),
        62.0,
        move |_: &App, i| match &keys[i] {
            Line::Header(t) => std::borrow::Cow::Owned(format!("h-{t}")),
            Line::Item(m, ..) => std::borrow::Cow::Owned(m.id.clone()),
            Line::Fold(f) => std::borrow::Cow::Owned(f.key().to_string()),
        },
        move |s: &App, i| match &lines[i] {
            Line::Header(t) => section_header(&pal, t),
            Line::Item(m, selected, unread) => sidebar_row(&pal, m, *selected, *unread, s.sidebar_width),
            Line::Fold(f) => fold_row(&pal, f),
        },
    );
    queue(rows)
}

/// The scrolling list around a mode's rows, with the keys that move through it.
fn queue(rows: El) -> El {
    let queue = col()
        .key("queue")
        // Refreshes and filtering change the list in place, without rows growing or fading.
        .instant_changes()
        .role("list")
        .label("Queue")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .focusable(true)
        .no_focus_ring()
        .on_key(|s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| {
            if ev.mods.cmd || ev.mods.ctrl || ev.mods.alt {
                return false;
            }
            match &ev.key {
                rune::Key::Up => s.store.select_offset(-1),
                rune::Key::Down => s.store.select_offset(1),
                rune::Key::Text(t) if t == "k" => s.store.select_offset(-1),
                rune::Key::Text(t) if t == "j" => s.store.select_offset(1),
                // Right shows what's folded away (in Mine, every part of the stacks too), Left
                // folds it again: the snoozed fold is in reach of the keys.
                rune::Key::Right | rune::Key::Left => {
                    tx.instant();
                    let open = matches!(ev.key, rune::Key::Right);
                    if s.store.mode == QueueMode::Mine {
                        s.store.unfold_stacks(open);
                    }
                    s.store.snoozed_open = open;
                }
                rune::Key::Enter => {
                    if let Some(item) = s.store.selected() {
                        let _ = tx.open_url(&item.web_url);
                    }
                    return true;
                }
                _ => return false,
            }
            if let Some(id) = &s.store.selected_id {
                tx.scroll_into_view(format!("rows/{id}"));
            }
            true
        })
        .child(rows)
        .scroll(ScrollSpec { stick_bottom: false });
    crate::ui::smooth_scroll::smooth_wheel(queue, "queue")
}

pub fn section_header(p: &Pal, title: &str) -> El {
    el().w_full().px(18.0).pt(14.0).pb(4.0).child(text(title.to_string()).font(ui(12.0, MEDIUM)).color(p.tertiary).single_line())
}

fn sidebar_row(p: &Pal, item: &MergeRequest, selected: bool, unread: bool, sidebar_width: f32) -> El {
    let stale = item.is_open() && item.my_state.needs_review() && !item.draft && (chrono::Utc::now() - item.requested_at).num_seconds() > 2 * 86_400;
    let when = if item.is_open() { item.requested_at } else { item.merged_at.unwrap_or(item.updated_at) };
    let time_help = if !item.is_open() {
        let state = item.state.clone();
        let mut chars = state.chars();
        let state = chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default();
        format!("{state} {}", crate::data::store::relative(item.merged_at.or(item.closed_at).unwrap_or(item.updated_at)))
    } else {
        let asked = if item.requested_at_is_guess {
            "Opened"
        } else if item.was_re_requested() {
            "Review re-requested"
        } else {
            "Review requested"
        };
        format!("{asked} {}", long_date(item.requested_at))
    };
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
            text(short_time(when))
                .font(ui(12.0, MEDIUM).features("'tnum' 1"))
                .color(if stale { p.accent } else { p.tertiary })
                .single_line()
                .pt(1.5)
                .help(time_help),
        );
    // The reference is what identifies the request, so the author's name gives way first. Only
    // a project name long enough to take most of the line is cut, from its start, keeping the
    // number whole.
    let reference = item.short_reference();
    let reference_font = mono(12.0, 400.0);
    let whole = rune::text::measure(&reference, &[], reference_font, None).0;
    let room = (sidebar_width - 52.0) * 0.6;
    let meta = row()
        .gap(6.0)
        .center_y()
        .child(text(reference).font(reference_font).color(p.tertiary).single_line().truncate_head().min_w(whole.min(room).ceil()))
        .child(text("·").font(ui(12.0, 400.0)).color(p.tertiary))
        .child(text(item.author.name.clone()).font(ui(12.0, 400.0)).color(p.secondary).single_line().truncate().shrink().min_w(0.0))
        .child(el().grow(1.0).min_w(4.0))
        .child(badges(p, item));
    let mut body = col().gap(4.0).grow(1.0).shrink().min_w(0.0).child(title_row);
    if let Some(reason) = crate::data::mine::review_reason(item) {
        let waiting = item.my_state.needs_review() && !item.draft;
        body = body
            .child(text(reason).font(ui(12.0, 400.0)).color(if waiting && item.was_re_requested() { p.accent } else { p.secondary }).single_line().truncate());
    }
    let body = body.child(meta);
    let dot = el().size(6.0, 6.0).mt(6.0).radius(3.0).fill(if unread { p.accent } else { p.accent.alpha(0.0) });
    let card = row().key("card").w_full().items_start().gap(8.0).pl(10.0).pr(12.0).py(8.0).radius(7.0).child(dot).child(body);
    let card = if selected { card.fill(p.selected) } else { card.hover(p.hover) };
    let id = item.id.clone();
    let id2 = item.id.clone();
    let id3 = item.id.clone();
    el().key(item.id.clone())
        .role("listitem")
        .label(item.title.clone())
        .w_full()
        .px(8.0)
        .pb(1.0)
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

// MARK: The snoozed fold

/// A line of the fold at the end of To review and Mine: what's snoozed or muted out of the list.
pub enum Fold {
    /// Above the fold, when nothing else is left in the list.
    Note(&'static str),
    Head {
        snoozed: usize,
        muted: usize,
        open: bool,
    },
    Held {
        item: MergeRequest,
        back: String,
        muted: bool,
        selected: bool,
    },
}

impl Fold {
    /// Its row's key. A held request keys by its ID, as in the list, so the keys can reach it.
    pub fn key(&self) -> &str {
        match self {
            Fold::Note(_) => "snoozed-note",
            Fold::Head { .. } => "snoozed",
            Fold::Held { item, .. } => &item.id,
        }
    }
}

/// The fold's lines for the current list: a count, and when it's open each request with when it
/// comes back. `alone` says the list has nothing else, which a line above the fold says.
pub fn fold_lines(store: &Store, alone: bool) -> Vec<Fold> {
    let held = store.held_items();
    if held.is_empty() {
        return vec![];
    }
    let mut lines = Vec::new();
    if alone {
        lines.push(Fold::Note(if store.mode == QueueMode::Mine { "Nothing else of yours is open." } else { "Nothing else is waiting on you." }));
    }
    let muted = held.iter().filter(|(_, h)| h.hold.muted()).count();
    lines.push(Fold::Head { snoozed: held.len() - muted, muted, open: store.snoozed_open });
    if store.snoozed_open {
        let now = chrono::Local::now();
        lines.extend(held.into_iter().map(|(item, h)| Fold::Held {
            selected: store.selected_id.as_deref() == Some(item.id.as_str()),
            back: snooze::back_label(h.hold, &now),
            muted: h.hold.muted(),
            item,
        }));
    }
    lines
}

pub fn fold_row(p: &Pal, line: &Fold) -> El {
    match line {
        Fold::Note(note) => el().key("snoozed-note").w_full().px(18.0).pt(14.0).child(text(*note).font(ui(13.0, 400.0)).color(p.secondary).wrap()),
        Fold::Head { snoozed, muted, open } => fold_head(p, &snooze::fold_label(*snoozed, *muted), *open),
        Fold::Held { item, back, muted, selected } => held_row(p, item, back, *muted, *selected),
    }
}

/// "3 snoozed", a press away from showing them. Set like a section's title, since it is one.
fn fold_head(p: &Pal, label: &str, open: bool) -> El {
    let action = if open { "Hide them" } else { "Show them" };
    el().key("snoozed").w_full().px(8.0).pt(10.0).child(
        row()
            .key("head")
            .role("button")
            .label(format!("{label}. {action}"))
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
            .help(if open { "Hide them  ←" } else { "Show them  →" })
            .child(el().size(10.0, 10.0).center().child(crate::ui::mine::chevron(p, open)))
            .child(text(label.to_string()).font(ui(12.0, MEDIUM).features("'tnum' 1")).color(p.tertiary).group_color(p.secondary).single_line())
            .on_press_tx(|s: &mut App, tx: &mut Tx| {
                // Opened from a long list, often from the keyboard: it lands at once.
                tx.instant();
                s.store.snoozed_open = !s.store.snoozed_open;
            }),
    )
}

/// A snoozed request in the open fold: quieter than the list's rows, saying when it comes back.
/// Unsnooze (or Unmute) shows on hover and while it's selected; the row's menu and the Review
/// menu carry it too, so it's never only a hover away.
fn held_row(p: &Pal, item: &MergeRequest, back: &str, muted: bool, selected: bool) -> El {
    let verb = if muted { "Unmute" } else { "Unsnooze" };
    let id = item.id.clone();
    let undo = el()
        .key("unsnooze")
        .role("button")
        .label(format!("{verb} {}", item.title))
        .focusable(false)
        .size(22.0, 22.0)
        .pin(None, Some(-3.0), Some(-5.0), None)
        .radius(6.0)
        .center()
        .group_opacity(if selected { 1.0 } else { 0.0 }, 1.0)
        .hover(p.hover)
        .press(p.hover)
        .help(verb)
        .child(icon(&I::UNDO, 12.0).color(p.secondary))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.store.unsnooze(&id);
        });
    let title_row = row()
        .gap(8.0)
        .items_start()
        .child(text(item.title.clone()).font(ui(13.0, MEDIUM).lines(1.3)).color(p.secondary).single_line().truncate().grow(1.0).shrink().min_w(0.0))
        .child(el().size(16.0, 16.0).child(undo));
    let meta = row()
        .gap(6.0)
        .center_y()
        .child(text(item.short_reference()).font(mono(12.0, 400.0)).color(p.tertiary).single_line().truncate_head().shrink().min_w(0.0))
        .child(text("·").font(ui(12.0, 400.0)).color(p.tertiary))
        .child(text(back.to_string()).font(ui(12.0, 400.0)).color(p.tertiary).single_line());
    let body = col().gap(4.0).grow(1.0).shrink().min_w(0.0).child(title_row).child(meta);
    // Its text lines up with the list's titles, past the unread dot's column.
    let card = row().key("card").w_full().gap(8.0).pl(24.0).pr(12.0).py(8.0).radius(7.0).hover_group().child(body);
    let card = if selected { card.fill(p.selected) } else { card.hover(p.hover) };
    let (id, id2) = (item.id.clone(), item.id.clone());
    el().key(item.id.clone())
        .role("listitem")
        .label(format!("{}. {back}", item.title))
        .w_full()
        .px(8.0)
        .pb(1.0)
        .child(card)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.store.select(Some(id.clone()));
            tx.focus("queue", false);
        })
        .on_secondary_press(move |s: &mut App, at: PointerPress, tx: &mut Tx| menus::row(s, &id2, at, tx))
}

fn badges(p: &Pal, item: &MergeRequest) -> El {
    let mut r = row().gap(5.0).center_y();
    if item.draft {
        r = r.child(el().h(16.0).px(5.0).radius(4.0).fill(p.hover).center().child(text("Draft").font(ui(11.0, MEDIUM)).color(p.secondary).single_line()));
    }
    if item.was_re_requested() && item.my_state.needs_review() {
        r = r.child(icon(&I::REFRESH, 11.0).color(p.tertiary).help("Review re-requested"));
    }
    if item.state == "merged" {
        r = r.child(icon(&I::MERGE, 11.0).color(p.blue).help("Merged"));
    } else if item.state == "closed" {
        r = r.child(icon(&I::CLOSED_CIRCLE, 11.0).color(p.tertiary).help("Closed"));
    } else if item.my_state != crate::data::models::ReviewState::Unreviewed {
        let _ = state_color;
        r = r.child(state_icon(p, item.my_state, 11.0).help(item.my_state.label()));
    }
    if let (Some(pipe), true) = (item.pipeline, item.is_open()) {
        r = r.child(pipeline_dot(p, pipe));
    }
    r
}

fn empty_state(s: &App, p: &Pal) -> El {
    let store = &s.store;
    let forge = store.forge;
    if store.is_loading && store.last_refreshed().is_none() {
        return col()
            .gap(10.0)
            .items_center()
            .child(Spinner::new().size(16.0).build(&p.kit()))
            .child(text(format!("Asking {}…", forge.name())).font(ui(13.0, 400.0)).color(p.secondary));
    }
    if let (Some(error), None) = (store.error(), store.last_refreshed()) {
        return message(p, Some(&I::WARNING), &[&error]).child(
            ChromeButton::new("try-again", "Try Again").icon(&I::REFRESH, 13.0).style(Chrome::Field).build(p).on_press(|s: &mut App| s.store.refresh()),
        );
    }
    if store.mode == QueueMode::Mine && store.filter.is_empty() && store.mine_filter.is_some_and(|_| !store.mine().is_empty()) {
        return message(p, None, &["None of yours stand like that."]);
    }
    if !store.filter.is_empty() || store.author_filter.is_some() {
        let line = if store.filter.is_empty() { "Nothing from this author here.".to_string() } else { format!("Nothing matches “{}”", store.filter) };
        return message(p, None, &[&line]);
    }
    let done_days = prefs::read(|p| p.done_days);
    let (glyph, line) = match store.mode {
        QueueMode::ToReview => (&I::SEAL_CHECK, "No reviews waiting on you.".to_string()),
        QueueMode::Reviewed => (&I::TRAY, "Nothing you've reviewed is still open.".to_string()),
        QueueMode::Mine => match store.mine_error() {
            Some(error) => (&I::WARNING, format!("Couldn't load yours: {error}")),
            None => (&I::TRAY, format!("None of your {} are open.", forge.plural_noun())),
        },
        QueueMode::Done => (&I::TRAY, format!("Nothing merged or closed in the last {done_days} days.")),
    };
    message(p, Some(glyph), &[&line])
}

fn footer(s: &App, p: &Pal, dark: bool) -> El {
    let store = &s.store;
    let mut settings = ChromeButton::new("settings", "Settings").icon(&I::GEAR, 13.0).style(Chrome::Text);
    // Dragged narrow, the key gives way to when the queue last updated.
    if s.sidebar_width >= 280.0 {
        settings = settings.hint(crate::ui::components::keys("⌘,"));
    }
    let mut r = row().key("footer").padding(8.0).gap(4.0).center_y().child(settings.build(p).on_press_tx(actions::show_settings)).child(el().grow(1.0));
    if let Some(ready) = s.updater.ready() {
        r = r.child(update_button(p, &ready.version));
    } else if let Some(last) = store.last_refreshed() {
        let offline = store.error();
        let at = last.with_timezone(&chrono::Local).format("%H:%M").to_string();
        let label = match (&offline, store.from_cache(), short_time(last)) {
            (Some(_), _, _) => "Offline".to_string(),
            // Lists from the last run, until the refresh lands: say how old they are.
            (None, true, ago) if ago != "now" => format!("Updated {ago} ago"),
            (None, _, ago) => format!("Updated {ago}"),
        };
        let tip = match offline {
            Some(error) => error,
            None if store.from_cache() => format!("Saved at {at}, from the last time Mrow ran. Refreshing…"),
            None => at,
        };
        r = r.child(text(label).font(ui(12.0, 400.0)).color(if store.error().is_some() { p.red } else { p.tertiary }).single_line().help(tip));
    }
    r.child(ChromeButton::new("appearance", "Switch Appearance").icon(&I::APPEARANCE, 14.0).build(p).on_press(move |_: &mut App| {
        prefs::update(|p| p.appearance = if dark { Appearance::Light } else { Appearance::Dark });
        crate::platform::apply_appearance();
    }))
}

/// A downloaded update, waiting on a restart; it takes the place of the refresh time.
fn update_button(p: &Pal, version: &str) -> El {
    row()
        .key("restart-update")
        .role("button")
        .label("Restart to Update")
        .h(22.0)
        .px(9.0)
        .center_y()
        .radius(11.0)
        .fill(p.accent_soft)
        .hover(p.accent.alpha(0.22))
        .press(p.accent.alpha(0.28))
        .child(text("Restart to Update").font(ui(12.0, SEMIBOLD)).color(p.accent).single_line())
        .help(format!("Mrow {version} is downloaded. Restarting puts it in place."))
        .on_press_tx(actions::restart_to_update)
}
