//! Native pop-up menus: the account switcher, More, and the context menus on rows and tabs.

use rune::{PointerPress, PopupAnchor, PopupItem, PopupMenu, Tx};

use crate::app::App;
use crate::data::forge::Forge;
use crate::data::models::MergeRequest;
use crate::data::prefs;
use crate::data::snooze::{self, Hold};
use crate::ui::actions;

pub fn accounts(s: &mut App, tx: &mut Tx) {
    let mut menu = PopupMenu::new();
    for (index, forge) in s.store.forges().into_iter().enumerate() {
        let account = s.store.accounts.get(&forge);
        let detail = match account {
            Some(a) if a.error.is_some() => "Offline".to_string(),
            Some(a) if a.me.is_some() => {
                let host = forge.cli().host();
                let host = if host.is_empty() { forge.default_host().to_string() } else { host };
                format!("@{} · {host}", a.me.as_ref().unwrap().username)
            }
            _ => format!("Signing in through {}…", forge.cli().name),
        };
        let waiting = s.store.waiting_count(forge);
        let mut title = format!("{} — {detail}", forge.name());
        if waiting > 0 {
            title.push_str(&format!("  ({waiting} waiting)"));
        }
        let key: rune::Shortcut = format!("ctrl+{}", index + 1).parse().expect("shortcut");
        menu = menu.item(PopupItem::new(forge.raw(), title).checked(forge == s.store.forge).key_equivalent(key));
    }
    let all = Forge::ALL.iter().all(|f| s.store.forges().contains(f));
    menu = menu.separator().item(PopupItem::new("settings", if all { "Account Settings…" } else { "Add an Account…" }));
    tx.popup_menu(menu, "account", |s: &mut App, picked, tx| match picked {
        Some("settings") => actions::show_settings(s, tx),
        Some(raw) => {
            if let Some(f) = Forge::from_raw(raw) {
                s.store.switch_forge(f);
            }
        }
        None => {}
    });
}

/// Copy items shared by the row, tab and More menus.
fn copy_items(menu: PopupMenu) -> PopupMenu {
    menu.item(PopupItem::new("copy-link", "Copy Link"))
        .item(PopupItem::new("copy-reference", "Copy Reference"))
        .item(PopupItem::new("copy-branch", "Copy Branch Name"))
}

fn run_copy(s: &mut App, tx: &mut Tx, id: &str, picked: &str) -> bool {
    let Some(item) = s.store.item(id) else { return false };
    let value = match picked {
        "copy-link" => item.web_url.clone(),
        "copy-reference" => item.full_reference(),
        "copy-branch" => item.source_branch.clone(),
        _ => return false,
    };
    actions::copy(s, tx, value);
    true
}

/// Snoozing, for the row and the bar's More menus: until tomorrow (Monday, from Friday on) or
/// until it changes; Unsnooze on one that's snoozed. Nothing on a request that can't be.
pub fn snooze_items(s: &App, menu: PopupMenu, item: &MergeRequest) -> PopupMenu {
    match s.store.hold_of(&item.id).map(|h| h.hold) {
        Some(Hold::Mute) => menu,
        Some(_) => menu.item(PopupItem::new("unsnooze", "Unsnooze")),
        None if s.store.can_snooze(item) => {
            let morning = snooze::morning_word(&chrono::Local::now());
            menu.item(PopupItem::new("snooze-morning", format!("Snooze Until {morning}"))).item(PopupItem::new("snooze-change", "Snooze Until It Changes"))
        }
        None => menu,
    }
}

/// Mute or Unmute, kept apart from the snooze items: it's the stronger, rarer one.
pub fn mute_item(s: &App, menu: PopupMenu, item: &MergeRequest) -> PopupMenu {
    match s.store.hold_of(&item.id).map(|h| h.hold) {
        Some(Hold::Mute) => menu.item(PopupItem::new("unmute", "Unmute")),
        _ if s.store.can_snooze(item) => menu.item(PopupItem::new("mute", "Mute")),
        _ => menu,
    }
}

/// Whether a request has any of the items above, snoozing's or muting's.
pub fn snoozes(s: &App, item: &MergeRequest) -> bool {
    s.store.can_snooze(item) || s.store.is_held(item)
}

/// Whether `snooze_items` adds any: a muted request has only Unmute.
pub fn snooze_shown(s: &App, item: &MergeRequest) -> bool {
    s.store.hold_of(&item.id).map_or(s.store.can_snooze(item), |h| !h.hold.muted())
}

/// Runs a pick from `snooze_items` or `mute_item` on the request `id`. False for other picks.
pub fn run_snooze(s: &mut App, tx: &mut Tx, id: &str, picked: &str) -> bool {
    let hold = match picked {
        "snooze-morning" => Hold::Time(snooze::morning_after(&chrono::Local::now())),
        "snooze-change" => Hold::Change,
        "mute" => Hold::Mute,
        "unsnooze" | "unmute" => {
            tx.instant();
            s.store.unsnooze(id);
            return true;
        }
        _ => return false,
    };
    // The row leaves the list at once, and the selection moves on with it.
    tx.instant();
    s.store.snooze(id, hold);
    if let Some(next) = &s.store.selected_id {
        tx.scroll_into_view(format!("rows/{next}"));
    }
    true
}

pub fn row(s: &mut App, id: &str, at: PointerPress, tx: &mut Tx) {
    let Some(item) = s.store.item(id) else { return };
    let mut menu = PopupMenu::new();
    menu = if s.store.tabs.iter().any(|t| t == id) {
        menu.item(PopupItem::new("close-tab", "Close Tab"))
    } else {
        menu.item(PopupItem::new("open-tab", "Open in New Tab"))
    };
    menu = menu.item(PopupItem::new("open", format!("Open in {}", item.kind().name()))).separator();
    if snooze_shown(s, item) {
        menu = snooze_items(s, menu, item).separator();
    }
    let only = s.store.author_filter.as_deref() == Some(item.author.username.as_str());
    menu = menu.item(PopupItem::new("author", if only { "Show Everyone's".to_string() } else { format!("Show Only from {}", item.author.name) })).separator();
    menu = copy_items(menu);
    if snoozes(s, item) {
        menu = mute_item(s, menu.separator(), item);
    }
    menu = crate::ui::manage::row_menu(s, menu, item);
    let id = id.to_string();
    tx.popup_menu(menu, PopupAnchor::Point(at.x, at.y), move |s: &mut App, picked, tx| {
        let Some(picked) = picked else { return };
        if run_copy(s, tx, &id, picked) || run_snooze(s, tx, &id, picked) || crate::ui::manage::run_pick(s, tx, &id, picked) {
            return;
        }
        let Some(item) = s.store.item(&id).cloned() else { return };
        match picked {
            "close-tab" => {
                tx.instant();
                s.store.close_tab(&id);
            }
            "open-tab" => {
                tx.instant();
                s.store.open_tab(&id, false);
            }
            "open" => {
                let _ = tx.open_url(&item.web_url);
            }
            "author" => {
                s.store.author_filter =
                    if s.store.author_filter.as_deref() == Some(item.author.username.as_str()) { None } else { Some(item.author.username.clone()) };
            }
            _ => {}
        }
    });
}

pub fn tab(s: &mut App, id: &str, preview: bool, at: PointerPress, tx: &mut Tx) {
    let Some(item) = s.store.item(id) else { return };
    let mut menu = PopupMenu::new();
    if preview {
        menu = menu.item(PopupItem::new("keep", "Keep Open in Tab"));
    }
    menu = menu.item(PopupItem::new("close", if preview { "Close Preview" } else { "Close Tab" }));
    if !preview {
        menu = menu.item(PopupItem::new("close-others", "Close Other Tabs"));
    }
    menu = menu.separator().item(PopupItem::new("open", format!("Open in {}", item.kind().name())));
    let id = id.to_string();
    tx.popup_menu(menu, PopupAnchor::Point(at.x, at.y), move |s: &mut App, picked, tx| match picked {
        Some("keep") => {
            tx.instant();
            s.store.open_tab(&id, true);
        }
        Some("close") => {
            tx.instant();
            s.store.close_tab(&id);
        }
        Some("close-others") => {
            tx.instant();
            s.store.close_other_tabs(&id);
        }
        Some("open") => {
            if let Some(item) = s.store.item(&id) {
                let _ = tx.open_url(&item.web_url);
            }
        }
        _ => {}
    });
}

pub fn more(s: &mut App, tx: &mut Tx) {
    let p = prefs::get();
    let mut menu = PopupMenu::new();
    let selected = s.store.selected_id.clone();
    if selected.is_some() {
        menu = copy_items(menu).separator();
    }
    menu = menu
        .submenu(
            "Sort",
            PopupMenu::new()
                .item(PopupItem::new("newest", "Newest Request First").checked(!p.oldest_first))
                .item(PopupItem::new("oldest", "Waiting Longest First").checked(p.oldest_first)),
        )
        .item(PopupItem::new("drafts", "Show Drafts").checked(p.show_drafts))
        .separator()
        .item(PopupItem::new("read", "Mark All as Read"))
        .item(PopupItem::new("settings", "Settings…"));
    tx.popup_menu(menu, "more", move |s: &mut App, picked, tx| {
        let Some(picked) = picked else { return };
        if let Some(id) = &selected {
            if run_copy(s, tx, &id.clone(), picked) {
                return;
            }
        }
        match picked {
            "newest" => prefs::update(|p| p.oldest_first = false),
            "oldest" => prefs::update(|p| p.oldest_first = true),
            "drafts" => prefs::update(|p| p.show_drafts = !p.show_drafts),
            "read" => s.store.mark_all_seen(),
            "settings" => actions::show_settings(s, tx),
            _ => {}
        }
    });
}
