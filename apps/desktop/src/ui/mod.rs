//! The window: a header across the top, the sidebar, and the merge request on a card of paper.

pub mod actions;
pub mod ask;
pub mod components;
pub mod detail;
pub mod diff;
pub mod icons;
pub mod images;
pub mod markdown;
pub mod menus;
pub mod settings;
pub mod sidebar;
pub mod smooth_scroll;
pub mod theme;

use std::cell::RefCell;
use std::rc::Rc;

use rune::text::Span;
use rune::{Cursor, Cx, Drag, El, Phase, PointerPress, Tx, col, el, icon, rich, row, text};
use rune_kit::Kit;

use crate::app::App;
use crate::data::models::MergeRequest;
use crate::data::prefs::{self, Appearance};
use crate::ui::components::{Chrome, ChromeButton};
use crate::ui::icons as I;
use crate::ui::theme::{CARD_RADIUS, GUTTER, HEADER_HEIGHT, MEDIUM, Pal, SEMIBOLD, TRAFFIC_LIGHTS_WIDTH, serif, ui};

/// The palette and rune-kit theme for one appearance, built once.
pub struct Look {
    pub p: Pal,
    pub k: Rc<Kit>,
}

thread_local! {
    static LOOKS: RefCell<[Option<Rc<Look>>; 2]> = const { RefCell::new([None, None]) };
}

pub fn look(dark: bool) -> Rc<Look> {
    LOOKS.with(|l| {
        let mut l = l.borrow_mut();
        l[dark as usize]
            .get_or_insert_with(|| {
                let p = Pal::new(dark);
                Rc::new(Look { p, k: Rc::new(p.kit()) })
            })
            .clone()
    })
}

pub fn is_dark(cx: &Cx<App>) -> bool {
    match prefs::read(|p| p.appearance) {
        Appearance::System => cx.system_dark(),
        Appearance::Light => false,
        Appearance::Dark => true,
    }
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let dark = is_dark(cx);
    let look = look(dark);
    let (p, k) = (&look.p, &*look.k);
    let mut body = row().key("body").w_full().grow(1.0).min_h(0.0);
    if s.sidebar_visible {
        body = body.child(sidebar::sidebar(s, p, k, dark));
    }
    let card = card(s, p, k);
    let mut card_wrap = row().key("card-wrap").grow(1.0).shrink().min_w(0.0).h_full().pr(GUTTER).pb(GUTTER).child(card);
    if !s.sidebar_visible {
        card_wrap = card_wrap.pl(GUTTER);
    } else {
        card_wrap = card_wrap.child(resize_handle(s));
    }
    body = body.child(card_wrap);
    if s.ask_visible {
        if let Some(item) = s.store.selected() {
            body = body.child(row().key("ask-column").h_full().pr(GUTTER).pb(GUTTER).child(ask::panel(s, p, k, item)));
        }
    }
    let mut root = col().key("root").w_full().h_full().fill(p.chrome).child(header(s, p)).child(body);
    if let Some(message) = &s.store.toast {
        root = root.child(toast(p, message));
    }
    if s.settings_open {
        for layer in settings::sheet(s, p, k) {
            root = root.child(layer);
        }
    }
    root
}

fn header(s: &App, p: &Pal) -> El {
    let store = &s.store;
    let card_start = if s.sidebar_visible { s.sidebar_width } else { GUTTER };
    let buttons_end = TRAFFIC_LIGHTS_WIDTH + 2.0 * 28.0 + 2.0 * 2.0;
    let selected = store.selected();
    let open_title = format!("Open in {}", selected.map(|m| m.kind().name()).unwrap_or("Browser"));
    row()
        .key("header")
        .w_full()
        .h(HEADER_HEIGHT)
        .pl(TRAFFIC_LIGHTS_WIDTH)
        .pr(GUTTER)
        .gap(2.0)
        .center_y()
        .window_drag()
        .child(
            ChromeButton::new("toggle-sidebar", "Toggle Sidebar")
                .icon(&I::SIDEBAR, 15.0)
                .hint(crate::ui::components::keys("⌃⌘S"))
                .build(p)
                .on_press_tx(actions::toggle_sidebar),
        )
        .child(
            ChromeButton::new("refresh", "Refresh")
                .icon(&I::REFRESH, 14.0)
                .hint(crate::ui::components::keys("⌘R"))
                .spinning(store.is_loading)
                .enabled(!store.is_loading)
                .build(p)
                .on_press_tx(actions::refresh),
        )
        .child(tab_strip(s, p).pl((card_start - buttons_end).max(10.0)).grow(1.0).shrink().min_w(0.0).pr(12.0))
        .child(
            ChromeButton::new("find", "Filter")
                .icon(&I::SEARCH, 12.0)
                .style(Chrome::Field)
                .hint(crate::ui::components::keys("⌘K"))
                .width(200.0)
                .build(p)
                .mr(6.0)
                .on_press_tx(actions::focus_filter),
        )
        .child(
            ChromeButton::new("open", open_title)
                .icon(&I::OPEN_EXTERNAL, 15.0)
                .hint(crate::ui::components::keys("⌘O"))
                .enabled(selected.is_some())
                .build(p)
                .on_press_tx(actions::open_in_browser),
        )
        .child(
            ChromeButton::new("ask-toggle", format!("Ask {}", selected.map_or(s.ask.agent, |item| s.ask.agent_for(&item.id)).name()))
                .icon(&I::BUBBLE_TEXT, 14.0)
                .hint(crate::ui::components::keys("⌘L"))
                .on(s.ask_visible)
                .enabled(selected.is_some())
                .build(p)
                .on_press_tx(actions::toggle_ask),
        )
        .child(ChromeButton::new("more", "More").icon(&I::ELLIPSIS, 15.0).build(p).on_press_tx(menus::more))
}

/// The kept tabs, then the previewed merge request (selected, not kept) in a dashed outline.
fn tab_strip(s: &App, p: &Pal) -> El {
    let store = &s.store;
    let mut r = row().key("tabs").instant_changes().gap(3.0).center_y().clip_rounded(0.0);
    for item in store.tab_items() {
        let active = store.selected_id.as_deref() == Some(item.id.as_str());
        r = r.child(tab_chip(p, &item, active, false, store.is_unread(&item)));
    }
    if let Some(item) = store.selected() {
        if !store.tabs.contains(&item.id) {
            r = r.child(tab_chip(p, item, true, true, false));
        }
    }
    r
}

fn tab_chip(p: &Pal, item: &MergeRequest, active: bool, preview: bool, unread: bool) -> El {
    let reference = format!("{}{}", item.kind().reference_mark(), item.iid);
    let label = format!("{reference}  {}", item.title);
    let spans = vec![
        Span::new(0, reference.len()).mono().size(11.5).weight(MEDIUM).color(if active { p.text } else { p.secondary }),
        Span::new(reference.len(), label.len()).color(if active { p.secondary } else { p.tertiary }),
    ];
    let close_id = item.id.clone();
    let close = el()
        .key("close")
        .role("button")
        .label(if preview { "Close Preview" } else { "Close Tab" })
        .size(16.0, 16.0)
        .radius(4.0)
        .center()
        .fill(p.well)
        .group_opacity(0.0, 1.0)
        .help(if preview { "Close Preview".to_string() } else { crate::ui::components::keys("Close Tab (⌘W)") })
        .child(icon(&I::SMALL_X, 11.0).color(p.secondary))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.store.close_tab(&close_id);
        })
        .absolute(0.0, 0.0);
    let glyph =
        el().size(16.0, 16.0).center().child(icon(&I::PULL, 12.0).color(if active { p.secondary } else { p.tertiary }).group_opacity(1.0, 0.0)).child(close);
    let mut chip = row()
        .key(format!("tab-{}", item.id))
        .role("tab")
        .label(item.full_reference())
        .h(28.0)
        .pl(6.0)
        .pr(10.0)
        .gap(6.0)
        .center_y()
        .radius(8.0)
        .hover_group()
        .child(glyph)
        .child(rich(label, spans).font(ui(12.5, 400.0)).single_line().truncate().max_w(200.0));
    if unread && !active {
        chip = chip.child(el().size(5.0, 5.0).radius(2.5).fill(p.accent));
    }
    if active {
        chip = chip.fill(p.elevated).border(1.0, if preview { p.strong_line } else { p.line });
        if preview {
            chip = chip.border_dash(3.0, 2.5);
        }
    } else {
        chip = chip.hover(p.well);
    }
    let help =
        if preview { crate::ui::components::keys(&format!("{}\nDouble-click to keep it open (⌘T)", item.full_reference())) } else { item.full_reference() };
    let id = item.id.clone();
    let id2 = item.id.clone();
    let id3 = item.id.clone();
    chip.help(help)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.store.select(Some(id.clone()));
        })
        .on_double_press_tx(move |s: &mut App, tx: &mut Tx| {
            if preview {
                tx.instant();
                s.store.open_tab(&id2, true)
            }
        })
        .on_secondary_press(move |s: &mut App, at: PointerPress, tx: &mut Tx| menus::tab(s, &id3, preview, at, tx))
}

fn card(s: &App, p: &Pal, k: &Kit) -> El {
    let inner = match s.store.selected() {
        Some(item) => detail::detail(s, p, k, item),
        None => placeholder(s, p),
    };
    col()
        .key("card")
        // Switching reviews and tabs happens all day: the page changes at once, with no
        // cross-fade or rows growing in (the Ask panel beside it keeps its streaming motion).
        .instant_changes()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .h_full()
        .min_h(0.0)
        .radius(CARD_RADIUS)
        .fill(p.paper)
        .border(1.0, p.line)
        .clip_rounded(CARD_RADIUS)
        .child(inner)
}

fn placeholder(s: &App, p: &Pal) -> El {
    let store = &s.store;
    let line = if store.is_loading { "Gathering your reviews…".to_string() } else { format!("Pick a {} to review.", store.forge.noun()) };
    let mut c = col()
        .key("placeholder")
        .w_full()
        .h_full()
        .center()
        .gap(10.0)
        .child(text("Mrow").font(serif(30.0, SEMIBOLD)).color(p.text))
        .child(text(line).font(ui(13.0, 400.0)).color(p.secondary));
    if let Some(error) = store.error() {
        c = c.child(text(error).font(ui(12.0, 400.0)).color(p.red).wrap().max_w(420.0).pt(6.0));
    }
    c
}

/// The sidebar's right edge, in the gutter beside the card.
fn resize_handle(_: &App) -> El {
    el().key("sidebar-resize").w(8.0).h_full().absolute(-4.0, 0.0).cursor(Cursor::ResizeColumn).on_drag(|s: &mut App, d: &mut Drag, _: &mut Tx| match d.phase {
        Phase::Down => s.drag_from = s.sidebar_width,
        Phase::Move => s.sidebar_width = (s.drag_from + d.dx).clamp(240.0, 520.0),
        Phase::Up => {
            let w = s.sidebar_width as f64;
            prefs::update(|p| p.sidebar_width = w);
        }
    })
}

fn toast(p: &Pal, message: &str) -> El {
    row().key("toast-layer").absolute(0.0, HEADER_HEIGHT + 10.0).w_full().justify_center().inert().child(
        row()
            .key("toast")
            .role("status")
            .h(30.0)
            .px(14.0)
            .radius(15.0)
            .center_y()
            .fill(p.elevated)
            .border(1.0, p.line)
            .shadow(4.0, 14.0, crate::ui::theme::shade(if p.dark { 0.4 } else { 0.1 }))
            .child(text(message.to_string()).font(ui(12.5, MEDIUM)).color(p.text).single_line()),
    )
}
