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
    tx.focus("filter/input", true);
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

pub fn approve(s: &mut App, _: &mut Tx) {
    let Some(item) = s.store.selected().cloned() else { return };
    if item.is_open() && s.store.busy.is_none() {
        s.store.approve(&item);
    }
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
    cmd(V, "mode-0", "To review", Some("cmd+1"), |s, tx| mode(s, tx, QueueMode::ToReview)),
    cmd(V, "mode-1", "Reviewed", Some("cmd+2"), |s, tx| mode(s, tx, QueueMode::Reviewed)),
    cmd(V, "mode-2", "Mine", Some("cmd+3"), |s, tx| mode(s, tx, QueueMode::Mine)),
    cmd(V, "mode-3", "Done", Some("cmd+4"), |s, tx| mode(s, tx, QueueMode::Done)),
    sep("View"),
    // Ctrl+1 is "To review" off macOS, so the accounts take Alt there.
    Command { other: Some("alt+1"), ..cmd(V, "account-0", "Account 1", Some("ctrl+1"), |s, tx| account(s, tx, 0)) },
    Command { other: Some("alt+2"), ..cmd(V, "account-1", "Account 2", Some("ctrl+2"), |s, tx| account(s, tx, 1)) },
    sep("View"),
    cmd(V, "go-top", "Overview", Some("cmd+alt+1"), |s, tx| detail_tab(s, tx, Tab::Overview)),
    cmd(V, "go-changes", "Changes", Some("cmd+alt+2"), |s, tx| detail_tab(s, tx, Tab::Changes)),
    cmd(V, "go-conversation", "Conversation", Some("cmd+alt+3"), |s, tx| detail_tab(s, tx, Tab::Conversation)),
    // J and K, matched in `on_key` on every platform: a menu key equivalent would take the letter
    // from text fields.
    cmd(V, "next-section", "Next File", None, next_file),
    cmd(V, "previous-section", "Previous File", None, previous_file),
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
    sep("Review"),
    cmd(R, "mark-read", "Mark All as Read", None, mark_all_read),
    cmd(W, "next-tab", "Show Next Tab", Some("cmd+shift+bracketright"), next_tab),
    cmd(W, "previous-tab", "Show Previous Tab", Some("cmd+shift+bracketleft"), previous_tab),
];

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

/// J and K step through the changes file by file. Plain letters: Rune offers them to a focused text
/// field first and to `on_key` only when no field takes them, so they never steal typing.
const NEXT_SECTION: rune::Chord = rune::Chord::new(rune::ChordKey::Char('j'));
const PREVIOUS_SECTION: rune::Chord = rune::Chord::new(rune::ChordKey::Char('k'));

/// Keys the menu bar doesn't carry: ⌃Tab between tabs, Escape out of the settings sheet, J and
/// K between the changed files, and off macOS every command's keys.
pub fn on_key(s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
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
        rune::Key::Escape if s.confirm.is_some() => {
            s.confirm = None;
            true
        }
        _ if !s.settings_open && !s.inbox_open && NEXT_SECTION.matches_tx(ev, tx) => crate::ui::detail::step(s, tx, 1),
        _ if !s.settings_open && !s.inbox_open && PREVIOUS_SECTION.matches_tx(ev, tx) => crate::ui::detail::step(s, tx, -1),
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
    fn j_and_k_are_plain_letters_on_every_platform() {
        let key = |text: &str, mods: rune::Mods| rune::KeyEvent { key: rune::Key::Text(text.into()), mods };
        for platform in [rune::KeyPlatform::Mac, rune::KeyPlatform::Linux] {
            assert!(NEXT_SECTION.matches_on(&key("j", rune::Mods::default()), platform));
            assert!(PREVIOUS_SECTION.matches_on(&key("k", rune::Mods::default()), platform));
            let shift = rune::Mods { shift: true, ..Default::default() };
            assert!(!NEXT_SECTION.matches_on(&key("J", shift), platform), "Shift+J is typing, not a jump");
        }
    }
}
