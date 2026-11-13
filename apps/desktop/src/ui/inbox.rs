//! The inbox: a bell at the top of the sidebar counting what's new, and the popover it opens
//! with what changed on your requests and reviews, new first.

use rune::{Align, El, ScrollSpec, Side, Tx, col, el, icon, row, text};

use crate::app::App;
use crate::data::inbox::{Event, Kind};
use crate::ui::components::{Chrome, ChromeButton, keys, long_date, message, rule, short_time};
use crate::ui::icons as I;
use crate::ui::theme::{MEDIUM, Pal, SEMIBOLD, mono, shade, ui};

/// The bell's key, which the popover hangs from and hands focus back to.
const BELL: &str = "inbox-bell";

/// The colour of an event's title, as Mine colours a next step.
pub fn kind_color(p: &Pal, kind: Kind) -> rune::Color {
    match kind {
        Kind::Ready | Kind::Approved => p.green,
        Kind::ChangesRequested | Kind::PipelineFailed | Kind::Threads => p.red,
        Kind::ReviewAsked | Kind::AskedAgain => p.accent,
        Kind::Closed => p.secondary,
    }
}

pub fn toggle(s: &mut App, tx: &mut Tx) {
    // Opened and closed often, from the keyboard too: it lands at once.
    tx.instant();
    if !s.sidebar_visible {
        s.set_sidebar_visible(true);
    }
    s.settings_open = false;
    s.inbox_open = !s.inbox_open;
}

/// The bell, with a count of what's new on it.
pub fn bell(s: &App, p: &Pal) -> El {
    let count = s.store.news_count();
    let open = s.inbox_open;
    let mut b = ChromeButton::new(BELL, "Inbox")
        .icon(&I::BELL, 15.0)
        .hint(keys("⇧⌘I"))
        .on(open)
        .build(p)
        .label(if count > 0 { format!("Inbox, {count} new") } else { "Inbox".to_string() })
        .expanded(open);
    if count > 0 {
        let shown = if count > 99 { "99+".to_string() } else { count.to_string() };
        let badge = el()
            .h(15.0)
            .min_w(15.0)
            .px(4.0)
            .radius(7.5)
            .fill(p.accent)
            .center()
            .child(text(shown).font(ui(11.0, SEMIBOLD).features("'tnum' 1")).color(p.paper).single_line());
        // A ring of the sidebar's colour keeps the count off the bell's outline.
        b = b.child(el().key("count").pin(Some(14.0), Some(2.0), None, None).padding(1.5).radius(9.0).fill(p.chrome).inert().child(badge));
    }
    b.on_press_tx(toggle)
}

fn caption(p: &Pal, title: &str) -> El {
    el().w_full().px(10.0).pt(10.0).pb(4.0).child(text(title.to_string()).font(ui(12.0, MEDIUM)).color(p.tertiary).single_line())
}

/// The popover under the bell, as a layer over the window.
pub fn panel(s: &App, p: &Pal) -> El {
    let store = &s.store;
    let width = (s.sidebar_width - 16.0).max(300.0);
    let new: Vec<&Event> = store.inbox.iter().filter(|e| !e.done).collect();
    let earlier: Vec<&Event> = store.inbox.iter().filter(|e| e.done).collect();
    let mut head = row()
        .key("inbox-head")
        .w_full()
        .h(44.0)
        .pl(16.0)
        .pr(8.0)
        .gap(8.0)
        .center_y()
        .child(text("Inbox").font(ui(13.0, SEMIBOLD)).color(p.text).single_line())
        .child(el().grow(1.0));
    if !new.is_empty() {
        head = head.child(ChromeButton::new("mark-all-done", "Mark All Done").style(Chrome::Text).build(p).on_press_tx(|s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.store.mark_all_done();
        }));
    }
    let body = if store.inbox.is_empty() {
        col().key("inbox-empty").w_full().py(12.0).child(message(p, Some(&I::BELL), &["Nothing in your inbox."]))
    } else {
        let mut list = col().key("inbox-rows").role("list").label("Events").focus_group().w_full().px(6.0).pb(6.0);
        for (title, events) in [("New", &new), ("Earlier", &earlier)] {
            if events.is_empty() {
                continue;
            }
            list = list.child(caption(p, title));
            for e in events.iter() {
                list = list.child(event_row(p, e));
            }
        }
        col().key("inbox-scroll").w_full().shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).scroll_edge_fade(16.0).scroll_margin(16.0).child(list)
    };
    col()
        .key("inbox")
        .role("dialog")
        .label("Inbox")
        .w(width)
        .max_h(560.0)
        .min_h(0.0)
        .radius(10.0)
        .fill(p.elevated)
        .border(1.0, p.line)
        .shadow(8.0, 28.0, shade(if p.dark { 0.45 } else { 0.14 }))
        .instant_changes()
        .child(head)
        .child(rule(p))
        .child(body)
        .focus_return(BELL)
        .modal_tx(true, |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.inbox_open = false;
        })
        .anchored_fit(BELL, Side::Bottom, Align::End, 4.0)
}

/// One event: what happened, coloured by kind, then the request it happened to. A press opens
/// the request; a new one can be marked done from the check on hover, or with Delete.
fn event_row(p: &Pal, e: &Event) -> El {
    let new = !e.done;
    let time = text(short_time(e.at)).font(ui(12.0, MEDIUM).features("'tnum' 1")).color(p.tertiary).single_line().help(long_date(e.at));
    let mut corner = el().h(16.0).min_w(22.0).child(row().w_full().justify_end().child(if new { time.group_opacity(1.0, 0.0) } else { time }));
    if new {
        let id = e.id.clone();
        // The time gives way to the check while the pointer is on the row.
        corner = corner.child(
            el().key("done")
                .role("button")
                .label("Mark Done")
                .focusable(false)
                .size(22.0, 22.0)
                .pin(None, Some(-3.0), Some(-5.0), None)
                .radius(6.0)
                .center()
                .group_opacity(0.0, 1.0)
                .hover(p.hover)
                .press(p.hover)
                .help(keys("Mark Done  ⌫"))
                .child(icon(&I::CHECK, 12.0).color(p.secondary))
                .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                    tx.instant();
                    s.store.mark_done(&id);
                }),
        );
    }
    let title_row = row()
        .gap(8.0)
        .items_start()
        .child(text(e.title.clone()).font(ui(13.0, MEDIUM)).color(kind_color(p, e.kind)).single_line().truncate().grow(1.0).shrink().min_w(0.0))
        .child(corner);
    let body = col()
        .gap(3.0)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .child(title_row)
        .child(text(e.subject.clone()).font(ui(13.0, 400.0)).color(if new { p.text } else { p.secondary }).single_line().truncate())
        .child(text(e.reference.clone()).font(mono(12.0, 400.0)).color(p.tertiary).single_line().truncate());
    let dot = el().size(6.0, 6.0).mt(5.5).radius(3.0).fill(if new { p.accent } else { p.accent.alpha(0.0) });
    let (open_id, done_id) = (e.id.clone(), e.id.clone());
    row()
        .key(e.id.clone())
        .role("button")
        .label(format!("{}. {}, {}", e.title, e.subject, e.reference))
        .focusable(true)
        .no_focus_ring()
        .w_full()
        .items_start()
        .gap(8.0)
        .pl(10.0)
        .pr(12.0)
        .py(8.0)
        .radius(7.0)
        .hover_group()
        .hover(p.hover)
        .press(p.hover)
        .focus_fill(p.hover)
        .child(dot)
        .child(body)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.inbox_open = false;
            if let Some(url) = s.store.open_event(&open_id) {
                let _ = tx.open_url(&url);
            }
            if let Some(id) = &s.store.selected_id {
                tx.scroll_into_view(format!("rows/{id}"));
            }
            tx.focus("queue", false);
        })
        .on_key(move |s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| match ev.key {
            rune::Key::Backspace | rune::Key::Delete if !ev.mods.cmd && !ev.mods.ctrl => {
                tx.instant();
                s.store.mark_done(&done_id);
                true
            }
            _ => false,
        })
}
