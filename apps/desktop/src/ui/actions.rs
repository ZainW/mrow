//! The window's commands, as plain functions so the menu bar, the buttons and the keyboard all
//! run the same code.

use rune::{MenuItem, Tx};

use crate::app::App;
use crate::data::prefs;
use crate::data::store::QueueMode;
use crate::ui::detail::{Jump, Tab};

pub fn refresh(s: &mut App, _: &mut Tx) {
    s.store.refresh();
}

pub fn show_settings(s: &mut App, tx: &mut Tx) {
    s.settings = crate::app::SettingsDraft::load();
    s.settings_open = true;
    tx.blur();
}

/// ⌘L: shows the chat beside the card with the caret in its composer, or hides it.
pub fn toggle_ask(s: &mut App, tx: &mut Tx) {
    if s.store.selected().is_none() {
        return;
    }
    tx.instant();
    let visible = !s.ask_visible;
    s.set_ask_visible(visible);
    if visible {
        crate::ui::ask::focus_composer(tx);
    }
}

/// ⇧⌘L: a fresh chat about the selected merge request.
pub fn new_chat(s: &mut App, tx: &mut Tx) {
    let Some(id) = s.store.selected_id.clone() else { return };
    tx.instant();
    s.ask.new_chat(&id);
    s.set_ask_visible(true);
    crate::ui::ask::focus_composer(tx);
}

/// ⌥⌘L: the model menu, from the composer's model button.
pub fn choose_model(s: &mut App, tx: &mut Tx) {
    let Some(id) = s.store.selected_id.clone() else { return };
    s.set_ask_visible(true);
    crate::ui::ask::open_model_menu(s, tx, &id);
}

pub fn toggle_sidebar(s: &mut App, tx: &mut Tx) {
    tx.instant();
    let v = !s.sidebar_visible;
    s.set_sidebar_visible(v);
    s.inbox_open &= v;
}

/// Shows the sidebar and puts the caret in its filter.
pub fn focus_filter(s: &mut App, tx: &mut Tx) {
    if !s.sidebar_visible {
        s.set_sidebar_visible(true);
    }
    s.settings_open = false;
    // Once shown: from the palette, its closing would otherwise hand focus back where it was.
    tx.focus_when_shown("filter/input", true);
    tx.select_all_text("filter/input");
}

pub fn open_in_browser(s: &mut App, tx: &mut Tx) {
    if let Some(item) = s.store.selected() {
        let _ = tx.open_url(&item.web_url);
    }
}

pub fn copy_link(s: &mut App, tx: &mut Tx) {
    let Some(url) = s.store.selected().map(|m| m.web_url.clone()) else { return };
    tx.copy(url);
    s.store.show("Copied link");
}

pub fn copy(s: &mut App, tx: &mut Tx, value: String) {
    tx.copy(value);
    s.store.show("Copied");
}

/// Approves at once; with comments held in a review, opens the review panel instead, so they
/// don't go out unseen.
pub fn approve(s: &mut App, tx: &mut Tx) {
    let Some(item) = s.store.selected().cloned() else { return };
    if s.store.pending_count(&item) > 0 {
        if s.review_panel.is_none() {
            review(s, tx);
        }
    } else if item.is_open() && s.store.busy.is_none() {
        s.store.approve(&item);
    }
}

/// Opens the panel that finishes the review, with the caret in its summary, or closes it.
pub fn review(s: &mut App, tx: &mut Tx) {
    let Some(item) = s.store.selected().cloned() else { return };
    tx.instant();
    if s.review_panel.take().is_some() || !crate::ui::detail::offers_review(s, &item) {
        return;
    }
    s.review_panel = Some(item.id);
    tx.focus_when_shown("review-summary/input", false);
}

pub fn mark_all_read(s: &mut App, _: &mut Tx) {
    s.store.mark_all_seen();
}

pub fn next_item(s: &mut App, tx: &mut Tx) {
    tx.instant();
    s.store.select_offset(1);
    reveal(s, tx);
}

pub fn previous_item(s: &mut App, tx: &mut Tx) {
    tx.instant();
    s.store.select_offset(-1);
    reveal(s, tx);
}

fn reveal(s: &App, tx: &mut Tx) {
    if let Some(id) = &s.store.selected_id {
        tx.scroll_into_view(format!("rows/{id}"));
    }
}

pub fn keep_in_tab(s: &mut App, tx: &mut Tx) {
    tx.instant();
    if let Some(id) = s.store.selected_id.clone() {
        s.store.open_tab(&id, true);
    }
}

/// Closes the merge request's tab; with nothing open, the window.
pub fn close_tab(s: &mut App, tx: &mut Tx) {
    if s.settings_open {
        s.settings_open = false;
    } else if let Some(id) = s.store.selected_id.clone() {
        tx.instant();
        s.store.close_tab(&id);
    } else {
        tx.close_window();
    }
}

pub fn close_other_tabs(s: &mut App, tx: &mut Tx) {
    tx.instant();
    if let Some(id) = s.store.selected_id.clone() {
        s.store.close_other_tabs(&id);
    }
}

pub fn next_tab(s: &mut App, tx: &mut Tx) {
    tx.instant();
    s.store.select_tab(1);
}

pub fn previous_tab(s: &mut App, tx: &mut Tx) {
    tx.instant();
    s.store.select_tab(-1);
}

fn mode(s: &mut App, tx: &mut Tx, mode: QueueMode) {
    tx.instant();
    s.store.set_mode(mode);
}

fn account(s: &mut App, tx: &mut Tx, index: usize) {
    tx.instant();
    let forges = s.store.forges();
    if let Some(f) = forges.get(index) {
        s.store.switch_forge(*f);
    }
}

fn detail_tab(s: &mut App, tx: &mut Tx, tab: Tab) {
    crate::ui::detail::go(s, tx, Jump::Tab(tab));
}

fn next_file(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::step(s, tx, 1);
}

fn previous_file(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::step(s, tx, -1);
}

fn next_open_thread(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::step_thread(s, tx, true, true);
}

fn previous_open_thread(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::step_thread(s, tx, false, true);
}

fn next_thread(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::step_thread(s, tx, true, false);
}

fn previous_thread(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::step_thread(s, tx, false, false);
}

fn toggle_viewed(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::toggle_viewed(s, tx);
}

fn reply(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::reply_to_picked(s, tx);
}

fn comment(s: &mut App, tx: &mut Tx) {
    crate::ui::detail::write_comment(s, tx);
}

pub fn check_for_updates(s: &mut App, _: &mut Tx) {
    if let Some(line) = s.updater.check_now() {
        s.store.show(&line);
    }
}

/// Puts a downloaded update in place and starts it.
pub fn restart_to_update(s: &mut App, tx: &mut Tx) {
    let Some(ready) = s.updater.ready().cloned() else { return };
    if let Err(e) = crate::data::update::install(&ready) {
        s.store.show(&e);
        return;
    }
    #[cfg(target_os = "macos")]
    tx.relaunch();
    #[cfg(not(target_os = "macos"))]
    match crate::data::update::start_replacement() {
        Ok(()) => tx.quit(),
        Err(e) => s.store.show(&format!("Mrow {} is in place; {e}. Quit and open it again.", ready.version)),
    }
}

pub fn release_notes(s: &mut App, tx: &mut Tx) {
    let url = s.updater.ready().map_or_else(|| format!("{}/tag/v{}", crate::data::update::RELEASES, crate::data::update::VERSION), |r| r.notes_url.clone());
    let _ = tx.open_url(&url);
}

/// Holds the selected request out of its list, as the row menu's items do.
fn hold_selected(s: &mut App, tx: &mut Tx, picked: &str) {
    let Some(item) = s.store.selected().cloned() else { return };
    let held = s.store.hold_of(&item.id).map(|h| h.hold);
    let fits = match picked {
        "unsnooze" => held.is_some_and(|h| !h.muted()),
        "unmute" => held.is_some_and(|h| h.muted()),
        // A snooze can be moved or made a mute; a mute only ends with Unmute.
        _ => !held.is_some_and(|h| h.muted()) && s.store.can_snooze(&item),
    };
    if fits {
        crate::ui::menus::run_snooze(s, tx, &item.id, picked);
    } else if picked == "unsnooze" || picked == "unmute" {
        s.store.show(if picked == "unmute" { "It isn’t muted." } else { "It isn’t snoozed." });
    } else if held.is_some_and(|h| h.muted()) {
        s.store.show("It’s muted. Unmute it first.");
    } else {
        let noun = item.kind().noun();
        s.store.show(&cannot_snooze(noun));
    }
}

fn cannot_snooze(noun: &str) -> String {
    format!("Only reviews waiting on you and your own open {noun}s can be snoozed.")
}

pub fn snooze_until_tomorrow(s: &mut App, tx: &mut Tx) {
    hold_selected(s, tx, "snooze-morning");
}

pub fn snooze_until_it_changes(s: &mut App, tx: &mut Tx) {
    hold_selected(s, tx, "snooze-change");
}

pub fn unsnooze(s: &mut App, tx: &mut Tx) {
    hold_selected(s, tx, "unsnooze");
}

/// Moves the selected request I asked changes on back to To review, as the row menu's item does.
pub fn review_again(s: &mut App, tx: &mut Tx) {
    let Some(id) = s.store.selected().filter(|m| s.store.waits(m)).map(|m| m.id.clone()) else {
        s.store.show("It isn’t waiting on its author.");
        return;
    };
    tx.instant();
    s.store.review_again(&id);
}

pub fn mute(s: &mut App, tx: &mut Tx) {
    hold_selected(s, tx, "mute");
}

pub fn unmute(s: &mut App, tx: &mut Tx) {
    hold_selected(s, tx, "unmute");
}

pub fn toggle_drafts(_: &mut App, _: &mut Tx) {
    prefs::update(|p| p.show_drafts = !p.show_drafts);
}

/// One command: where it sits in the menu bar, and its keys. `keys` is the macOS menu shortcut;
/// off macOS, where there's no native menu bar, the same keys with Ctrl for ⌘ are matched in
/// `on_key`, unless `other` gives that platform its own.
struct Command {
    menu: Option<&'static str>,
    id: &'static str,
    title: &'static str,
    keys: Option<&'static str>,
    other: Option<&'static str>,
    run: fn(&mut App, &mut Tx),
}

/// A separator in a menu, written as a command with no id.
const fn sep(menu: &'static str) -> Command {
    Command { menu: Some(menu), id: "", title: "", keys: None, other: None, run: |_, _| {} }
}

const fn cmd(menu: Option<&'static str>, id: &'static str, title: &'static str, keys: Option<&'static str>, run: fn(&mut App, &mut Tx)) -> Command {
    Command { menu, id, title, keys, other: None, run }
}

const F: Option<&str> = Some("File");
const V: Option<&str> = Some("View");
const R: Option<&str> = Some("Review");
const W: Option<&str> = Some("Window");

/// The Swift app's commands, with the same key equivalents.
const COMMANDS: &[Command] = &[
    cmd(None, "settings", "Settings…", Some("cmd+comma"), show_settings),
    cmd(None, "check-updates", "Check for Updates…", None, check_for_updates),
    cmd(F, "filter", "Filter", Some("cmd+k"), focus_filter),
    cmd(F, "find", "Find", Some("cmd+f"), focus_filter),
    sep("File"),
    cmd(F, "keep", "Keep Open in Tab", Some("cmd+t"), keep_in_tab),
    cmd(F, "close-tab", "Close Tab", Some("cmd+w"), close_tab),
    cmd(F, "close-others", "Close Other Tabs", Some("cmd+alt+w"), close_other_tabs),
    cmd(V, "palette", "Command Palette…", Some("cmd+shift+p"), crate::ui::palette::toggle),
    sep("View"),
    cmd(V, "mode-0", "To review", Some("cmd+1"), |s, tx| mode(s, tx, QueueMode::ToReview)),
    cmd(V, "mode-1", "Reviewed", Some("cmd+2"), |s, tx| mode(s, tx, QueueMode::Reviewed)),
    cmd(V, "mode-2", "Mine", Some("cmd+3"), |s, tx| mode(s, tx, QueueMode::Mine)),
    cmd(V, "mode-3", "Done", Some("cmd+4"), |s, tx| mode(s, tx, QueueMode::Done)),
    sep("View"),
    // Ctrl+1 is "To review" off macOS, so the accounts take Alt there.
    Command { other: Some("alt+1"), ..cmd(V, "account-0", "Account 1", Some("ctrl+1"), |s, tx| account(s, tx, 0)) },
    Command { other: Some("alt+2"), ..cmd(V, "account-1", "Account 2", Some("ctrl+2"), |s, tx| account(s, tx, 1)) },
    sep("View"),
    cmd(V, "go-overview", "Overview", Some("cmd+alt+1"), |s, tx| detail_tab(s, tx, Tab::Overview)),
    cmd(V, "go-changes", "Changes", Some("cmd+alt+2"), |s, tx| detail_tab(s, tx, Tab::Changes)),
    cmd(V, "go-conversation", "Conversation", Some("cmd+alt+3"), |s, tx| detail_tab(s, tx, Tab::Conversation)),
    // The keys through a review, all plain letters matched in `on_key` on every platform (see
    // `LETTERS`): a menu key equivalent would take the letter from text fields.
    cmd(V, "next-file", "Next File", None, next_file),
    cmd(V, "previous-file", "Previous File", None, previous_file),
    cmd(V, "next-open-thread", "Next Open Thread", None, next_open_thread),
    cmd(V, "previous-open-thread", "Previous Open Thread", None, previous_open_thread),
    cmd(V, "next-thread", "Next Thread", None, next_thread),
    cmd(V, "previous-thread", "Previous Thread", None, previous_thread),
    cmd(V, "toggle-viewed", "Toggle File Viewed", None, toggle_viewed),
    cmd(V, "reply", "Reply to Thread", None, reply),
    cmd(V, "comment", "Write a Comment", None, comment),
    sep("View"),
    cmd(V, "sidebar", "Toggle Sidebar", Some("cmd+ctrl+s"), toggle_sidebar),
    cmd(V, "ask", "Ask an Agent", Some("cmd+l"), toggle_ask),
    cmd(V, "new-chat", "New Chat", Some("cmd+shift+l"), new_chat),
    cmd(V, "choose-model", "Choose Model…", Some("cmd+alt+l"), choose_model),
    cmd(V, "inbox", "Inbox", Some("cmd+shift+i"), crate::ui::inbox::toggle),
    cmd(V, "drafts", "Show Drafts", None, toggle_drafts),
    cmd(V, "refresh", "Refresh", Some("cmd+r"), refresh),
    cmd(R, "next", "Next Review", Some("cmd+alt+bracketright"), next_item),
    cmd(R, "previous", "Previous Review", Some("cmd+alt+bracketleft"), previous_item),
    sep("Review"),
    cmd(R, "open", "Open in Browser", Some("cmd+o"), open_in_browser),
    cmd(R, "copy-link", "Copy Link", Some("cmd+shift+c"), copy_link),
    cmd(R, "approve", "Approve", Some("cmd+shift+a"), approve),
    cmd(R, "review", "Finish Review…", Some("cmd+shift+r"), review),
    sep("Review"),
    cmd(R, "edit-title", "Edit Title and Description", None, crate::ui::manage::edit_title),
    cmd(R, "edit-reviewers", "Edit Reviewers…", None, crate::ui::manage::edit_reviewers),
    cmd(R, "ask-again", "Ask Reviewers Again", None, crate::ui::manage::ask_again),
    cmd(R, "update-branch", "Update Branch", None, crate::ui::manage::update_branch),
    cmd(R, "close-request", "Close Without Merging…", None, crate::ui::manage::close),
    cmd(R, "reopen", "Reopen", None, crate::ui::manage::reopen),
    sep("Review"),
    // Snoozing and muting. Mute has no key: it's the stronger, rarer one.
    cmd(R, "snooze-morning", "Snooze Until Morning", Some("cmd+shift+s"), snooze_until_tomorrow),
    cmd(R, "snooze-change", "Snooze Until It Changes", None, snooze_until_it_changes),
    cmd(R, "unsnooze", "Unsnooze", None, unsnooze),
    cmd(R, "review-again", "Review Again", None, review_again),
    cmd(R, "mute", "Mute", None, mute),
    cmd(R, "unmute", "Unmute", None, unmute),
    sep("Review"),
    cmd(R, "mark-read", "Mark All as Read", None, mark_all_read),
    cmd(W, "next-tab", "Show Next Tab", Some("cmd+shift+bracketright"), next_tab),
    cmd(W, "previous-tab", "Show Previous Tab", Some("cmd+shift+bracketleft"), previous_tab),
];

/// A command as the palette lists it: its title, and its keys as this platform writes them.
pub struct Listed {
    pub id: &'static str,
    pub title: String,
    pub keys: Option<String>,
    pub run: fn(&mut App, &mut Tx),
}

/// The commands the palette offers, in the table's order: those that would do something now.
/// The rest are left out rather than dimmed, so every row the keyboard lands on runs.
pub fn listed(s: &App) -> Vec<Listed> {
    COMMANDS
        .iter()
        .filter(|c| !c.id.is_empty() && c.id != "palette" && not_now(s, c.id).is_none())
        .map(|c| Listed { id: c.id, title: title(s, c), keys: hint(c).map(|k| crate::ui::components::keys(&glyphs(k))), run: c.run })
        .collect()
}

/// The menu's title, or one that says more out of the menu's context: which account, which list,
/// which tab.
fn title(s: &App, c: &Command) -> String {
    let account = match c.id {
        "account-0" => 0,
        "account-1" => 1,
        // Friday's morning after is Monday's.
        "snooze-morning" => return format!("Snooze Until {}", crate::data::snooze::morning_word(&chrono::Local::now())),
        "mode-0" | "mode-1" | "mode-2" | "mode-3" => return format!("Show {}", c.title),
        "go-overview" | "go-changes" | "go-conversation" => return format!("Show {}", c.title),
        "inbox" => return "Show Inbox".into(),
        "filter" => return "Filter the List".into(),
        "find" => return "Find in the List".into(),
        _ => return c.title.to_string(),
    };
    s.store.forges().get(account).map_or_else(|| c.title.to_string(), |f| format!("Switch to {}", f.name()))
}

/// The keys `on_key` or the menu bar answers to for `c` on this platform, as a chord.
fn hint(c: &Command) -> Option<&'static str> {
    match c.id {
        "next-file" => Some("j"),
        "previous-file" => Some("k"),
        "next-open-thread" => Some("n"),
        "previous-open-thread" => Some("p"),
        "next-thread" => Some("shift+n"),
        "previous-thread" => Some("shift+p"),
        "toggle-viewed" => Some("v"),
        "reply" => Some("r"),
        "comment" => Some("c"),
        _ if cfg!(not(target_os = "macos")) => c.other.or(c.keys),
        _ => c.keys,
    }
}

/// "cmd+shift+a" as the Mac writes it, "⇧⌘A", for `components::keys` to relabel elsewhere.
fn glyphs(chord: &str) -> String {
    let parts: Vec<&str> = chord.split('+').collect();
    let (key, mods) = parts.split_last().expect("a key");
    let mut out = String::new();
    for (name, glyph) in [("ctrl", '⌃'), ("alt", '⌥'), ("shift", '⇧'), ("cmd", '⌘')] {
        if mods.contains(&name) {
            out.push(glyph);
        }
    }
    match *key {
        "comma" => out.push(','),
        "bracketleft" => out.push('['),
        "bracketright" => out.push(']'),
        k => out.push_str(&k.to_uppercase()),
    }
    out
}

/// Why the command `id` would do nothing in this state, or None when it would run. Most act on
/// the selected request; some on a request of mine, or one I can still review.
fn not_now(s: &App, id: &str) -> Option<String> {
    let store = &s.store;
    let noun = store.forge.noun();
    let idle = store.busy.is_none();
    let wait = || "Wait for the last change to land.".to_string();
    // Commands that need nothing selected.
    match id {
        "account-0" | "account-1" => {
            let index = if id == "account-0" { 0 } else { 1 };
            return match store.forges().get(index) {
                None => Some("Add a second account in Settings first.".into()),
                Some(f) if *f == store.forge => Some(format!("You’re on {} already.", f.name())),
                Some(_) => None,
            };
        }
        "next" | "previous" => return store.visible_items().is_empty().then(|| "The list is empty.".into()),
        "next-tab" | "previous-tab" => {
            let previewing = store.selected().is_some_and(|m| !store.tabs.contains(&m.id));
            let room = store.tabs.len() > 1 || (previewing && !store.tabs.is_empty());
            return (!room).then(|| "There’s no other tab to show.".into());
        }
        "keep"
        | "close-tab"
        | "close-others"
        | "open"
        | "copy-link"
        | "ask"
        | "new-chat"
        | "choose-model"
        | "go-overview"
        | "go-changes"
        | "go-conversation"
        | "next-file"
        | "previous-file"
        | "approve"
        | "review"
        | "edit-title"
        | "edit-reviewers"
        | "update-branch"
        | "close-request"
        | "ask-again"
        | "reopen"
        | "next-open-thread"
        | "previous-open-thread"
        | "next-thread"
        | "previous-thread"
        | "toggle-viewed"
        | "reply"
        | "comment"
        | "snooze-morning"
        | "snooze-change"
        | "unsnooze"
        | "review-again"
        | "mute"
        | "unmute" => {}
        _ => return None,
    }
    let Some(item) = store.selected() else { return Some(format!("Open a {noun} first.")) };
    let mine = store.is_mine(item);
    let held = store.hold_of(&item.id).map(|h| h.hold);
    let closed = || format!("This {noun} isn’t open.");
    let only_mine = || format!("That works on your own {noun}s only.");
    match id {
        "keep" if store.tabs.contains(&item.id) => Some("It’s open in a tab already.".into()),
        "close-others" if !store.tabs.iter().any(|t| *t != item.id) => Some("No other tabs are open.".into()),
        "approve" if store.pending_count(item) > 0 => None,
        "approve" | "review" if !item.is_open() => Some(closed()),
        "approve" if mine => Some(format!("You can’t approve your own {noun}.")),
        "approve" if !idle => Some(wait()),
        "review" if !crate::ui::detail::offers_review(s, item) => Some(format!("Leave a comment to send with the review first.")),
        "edit-title" | "edit-reviewers" | "update-branch" | "close-request" | "ask-again" | "reopen" if !mine => Some(only_mine()),
        "edit-title" | "edit-reviewers" | "update-branch" | "close-request" | "ask-again" if !item.is_open() => Some(closed()),
        "ask-again" if crate::ui::manage::waiting_on(item).is_empty() => Some("Nobody is waiting to review it.".into()),
        "reopen" if item.state != "closed" => Some(format!("This {noun} isn’t closed.")),
        "update-branch" | "close-request" | "ask-again" | "reopen" if !idle => Some(wait()),
        "toggle-viewed" | "previous-file" if s.tab(&item.id) != Tab::Changes => Some("Open the Changes tab first.".into()),
        "review-again" if !store.waits(item) => Some("It isn’t waiting on its author.".into()),
        "unsnooze" if !held.is_some_and(|h| !h.muted()) => Some("It isn’t snoozed.".into()),
        "unmute" if !held.is_some_and(|h| h.muted()) => Some("It isn’t muted.".into()),
        "snooze-morning" | "snooze-change" | "mute" if held.is_some_and(|h| h.muted()) => Some("It’s muted. Unmute it first.".into()),
        "snooze-morning" | "snooze-change" | "mute" if !store.can_snooze(item) => Some(cannot_snooze(noun)),
        _ => None,
    }
}

/// Commands the palette leaves out now, with why, for a query that finds only those.
pub fn unavailable(s: &App) -> Vec<(String, String)> {
    COMMANDS.iter().filter(|c| !c.id.is_empty() && c.id != "palette").filter_map(|c| Some((title(s, c), not_now(s, c.id)?))).collect()
}

/// The menu bar, built from `COMMANDS`.
pub fn menu_items() -> Vec<MenuItem<App>> {
    COMMANDS
        .iter()
        .map(|c| {
            let item = if c.id.is_empty() {
                MenuItem::separator()
            } else {
                let mut item = MenuItem::new(c.title, c.run).id(c.id);
                if let Some(keys) = c.keys {
                    item = item.key_equivalent(keys.parse::<rune::Shortcut>().expect("shortcut"));
                }
                item
            };
            match c.menu {
                Some(m) => item.menu(m),
                None => item,
            }
        })
        .collect()
}

/// The keys each command answers to where there's no native menu bar to carry them.
fn chords() -> &'static [(rune::Chord, fn(&mut App, &mut Tx))] {
    static CHORDS: std::sync::OnceLock<Vec<(rune::Chord, fn(&mut App, &mut Tx))>> = std::sync::OnceLock::new();
    CHORDS.get_or_init(|| {
        COMMANDS
            .iter()
            .filter_map(|c| {
                let keys = c.other.map(str::to_string).or_else(|| c.keys.map(|k| k.replace("cmd", "mod")))?;
                Some((rune::Chord::parse(&keys).expect("chord"), c.run))
            })
            .collect()
    })
}

/// The keys through a review: J and K file by file, N and P from open thread to open thread (as
/// GitLab's own N and P go through unresolved ones), ⇧N and ⇧P through every thread, V to mark
/// the file viewed and go on, R to reply to the thread, C to comment. Plain letters: Rune offers
/// them to a focused text field first and to `on_key` only when no field takes them, so they never
/// steal typing. Each returns whether it took the key.
const fn letter(c: char) -> rune::Chord {
    rune::Chord::new(rune::ChordKey::Char(c))
}

const fn shifted(c: char) -> rune::Chord {
    letter(c).with(rune::ChordMod::Shift)
}

const NEXT_FILE: rune::Chord = letter('j');
const PREVIOUS_FILE: rune::Chord = letter('k');

type Letter = (rune::Chord, fn(&mut App, &mut Tx) -> bool);

const LETTERS: &[Letter] = &[
    (NEXT_FILE, |s, tx| crate::ui::detail::step(s, tx, 1)),
    (PREVIOUS_FILE, |s, tx| crate::ui::detail::step(s, tx, -1)),
    (letter('n'), |s, tx| crate::ui::detail::step_thread(s, tx, true, true)),
    (letter('p'), |s, tx| crate::ui::detail::step_thread(s, tx, false, true)),
    (shifted('n'), |s, tx| crate::ui::detail::step_thread(s, tx, true, false)),
    (shifted('p'), |s, tx| crate::ui::detail::step_thread(s, tx, false, false)),
    (letter('v'), crate::ui::detail::toggle_viewed),
    (letter('r'), crate::ui::detail::reply_to_picked),
    (letter('c'), crate::ui::detail::write_comment),
];

/// Keys the menu bar doesn't carry: ⌃Tab between tabs, Escape out of the settings sheet, the
/// letters through a review, and off macOS every command's keys.
pub fn on_key(s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let letter = if s.settings_open || s.inbox_open { None } else { LETTERS.iter().find(|(chord, _)| chord.matches_tx(ev, tx)).map(|(_, run)| *run) };
    match &ev.key {
        rune::Key::Tab if ev.mods.ctrl => {
            if ev.mods.shift {
                previous_tab(s, tx)
            } else {
                next_tab(s, tx)
            }
            true
        }
        rune::Key::Escape if s.inbox_open => {
            tx.instant();
            s.inbox_open = false;
            true
        }
        rune::Key::Escape if s.settings_open => {
            s.settings_open = false;
            true
        }
        rune::Key::Escape if s.review_panel.is_some() => {
            tx.instant();
            s.review_panel = None;
            true
        }
        rune::Key::Escape if s.confirm_delete.is_some() => {
            tx.instant();
            s.confirm_delete = None;
            true
        }
        rune::Key::Escape if s.confirm.is_some() => {
            s.confirm = None;
            true
        }
        _ if letter.is_some() => letter.is_some_and(|run| run(s, tx)),
        _ if cfg!(not(target_os = "macos")) => match chords().iter().find(|(chord, _)| chord.matches_tx(ev, tx)) {
            Some((_, run)) => {
                run(s, tx);
                true
            }
            None => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_keys_parse_and_never_clash() {
        let chords = chords();
        assert_eq!(chords.len(), COMMANDS.iter().filter(|c| c.keys.is_some()).count());
        for (i, (a, _)) in chords.iter().enumerate() {
            for (b, _) in &chords[i + 1..] {
                assert!(!a.same_keys(b, rune::KeyPlatform::Linux), "{a} and {b} are the same keys on Linux");
            }
        }
    }

    #[test]
    fn the_tabs_tips_name_their_commands_keys() {
        for tab in Tab::ALL {
            let command = COMMANDS.iter().find(|c| c.title == tab.title()).expect("a command for each tab");
            let glyphs = command.keys.unwrap().replace("cmd+alt+", "⌥⌘");
            assert_eq!(glyphs, tab.hint(), "{}", tab.title());
        }
    }

    #[test]
    fn the_palette_writes_keys_as_the_menu_bar_does() {
        assert_eq!(glyphs("cmd+shift+a"), "⇧⌘A");
        assert_eq!(glyphs("cmd+alt+bracketright"), "⌥⌘]");
        assert_eq!(glyphs("cmd+ctrl+s"), "⌃⌘S");
        assert_eq!(glyphs("cmd+comma"), "⌘,");
        assert_eq!(glyphs("j"), "J");
        let palette = COMMANDS.iter().find(|c| c.id == "palette").expect("the palette is a command");
        assert_eq!(glyphs(palette.keys.unwrap()), "⇧⌘P");
    }

    #[test]
    fn j_and_k_are_plain_letters_on_every_platform() {
        let key = |text: &str, mods: rune::Mods| rune::KeyEvent { key: rune::Key::Text(text.into()), mods };
        for platform in [rune::KeyPlatform::Mac, rune::KeyPlatform::Linux] {
            assert!(NEXT_FILE.matches_on(&key("j", rune::Mods::default()), platform));
            assert!(PREVIOUS_FILE.matches_on(&key("k", rune::Mods::default()), platform));
            let shift = rune::Mods { shift: true, ..Default::default() };
            assert!(!NEXT_FILE.matches_on(&key("J", shift), platform), "Shift+J is typing, not a jump");
        }
    }

    #[test]
    fn the_review_letters_are_plain_and_shifted_letters_on_every_platform() {
        let key = |text: &str, mods: rune::Mods| rune::KeyEvent { key: rune::Key::Text(text.into()), mods };
        let shift = rune::Mods { shift: true, ..Default::default() };
        let cmd = rune::Mods { cmd: true, ..Default::default() };
        let ctrl = rune::Mods { ctrl: true, ..Default::default() };
        let taking = |ev: &rune::KeyEvent, platform| LETTERS.iter().filter(|(chord, _)| chord.matches_on(ev, platform)).count();
        for platform in [rune::KeyPlatform::Mac, rune::KeyPlatform::Linux] {
            for c in ["j", "k", "n", "p", "v", "r", "c"] {
                assert_eq!(taking(&key(c, rune::Mods::default()), platform), 1, "{c} is one command's key");
                assert_eq!(taking(&key(c, cmd), platform), 0, "⌘{c} belongs to the menu, not the review");
                assert_eq!(taking(&key(c, ctrl), platform), 0, "Ctrl+{c} belongs to the menu, not the review");
            }
            for c in ["N", "P"] {
                assert_eq!(taking(&key(c, shift), platform), 1, "⇧{c} goes through every thread");
            }
            for c in ["J", "K", "V", "R", "C"] {
                assert_eq!(taking(&key(c, shift), platform), 0, "Shift+{c} is typing, not a command");
            }
        }
    }

    #[test]
    fn every_review_letter_has_its_menu_command() {
        // The plain letters' commands sit in the View menu with no key equivalent, so the menu
        // bar never takes the letter from a text field.
        for id in
            ["next-file", "previous-file", "next-open-thread", "previous-open-thread", "next-thread", "previous-thread", "toggle-viewed", "reply", "comment"]
        {
            let command = COMMANDS.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("a command {id}"));
            assert_eq!(command.keys, None, "{id}");
            assert_eq!(command.menu, V, "{id}");
        }
    }
}
