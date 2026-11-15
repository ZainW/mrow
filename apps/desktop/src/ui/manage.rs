//! Managing a request of my own without opening the forge: who reviews it (the picker under the
//! reviewers, and asking a quiet one again), bringing its branch up to date, closing it, and its
//! title and description, edited in place on the overview.

use rune::{Align, El, PopupItem, PopupMenu, ScrollSpec, Side, TextEdit, Tx, col, el, icon, row, text};
use rune_kit::{Kit, Spinner, TextArea, TextInput};

use crate::app::{App, Confirm, RequestEdit};
use crate::data::mine::{self, Standing};
use crate::data::models::{MergeRequest, Person, Reviewer};
use crate::data::store::{Loadable, PEOPLE_PAGE};
use crate::ui::components::{ChromeButton, action_button, avatar, keys, rule, state_icon};
use crate::ui::detail::{Jump, Tab, go, scroll_key};
use crate::ui::icons as I;
use crate::ui::theme::{MEDIUM, Pal, READING_WIDTH, SEMIBOLD, serif, shade, ui};

/// The button the reviewer picker hangs from, and hands focus back to.
pub const REVIEWERS_BUTTON: &str = "edit-reviewers";
/// The bar's button with the rest of what can be done to a request of mine.
const MORE_BUTTON: &str = "manage-more";
/// The title's field, focused when editing starts.
const TITLE_FIELD: &str = "edit-title/input";

/// The reviewers a request is waiting on: the ones "Ask Again" asks from a menu.
fn waiting_on(item: &MergeRequest) -> Vec<Reviewer> {
    item.reviewers.iter().filter(|r| r.state.needs_review() && !r.approved).cloned().collect()
}

fn first_name(person: &Person) -> &str {
    person.name.split(' ').next().unwrap_or(&person.name)
}

/// "Ask Tomás Again", "Ask Both Again", "Ask All 3 Again".
fn ask_again_title(reviewers: &[Reviewer]) -> String {
    match reviewers {
        [one] => format!("Ask {} Again", first_name(&one.person)),
        [_, _] => "Ask Both Again".into(),
        more => format!("Ask All {} Again", more.len()),
    }
}

// MARK: Commands

/// The selected request, if it's one of mine: the commands below act on nothing else.
fn my_selected(s: &mut App) -> Option<MergeRequest> {
    let item = s.store.selected().cloned()?;
    if s.store.is_mine(&item) {
        Some(item)
    } else {
        s.store.show(&format!("That works on your own {}s only.", item.kind().noun()));
        None
    }
}

/// The overview of the selected request, at its top, where the editors and the picker are.
fn to_overview(s: &mut App, tx: &mut Tx, id: &str) {
    go(s, tx, Jump::Tab(Tab::Overview));
    tx.scroll_to_instant(scroll_key(id, Tab::Overview), 0.0, 0.0);
}

pub fn edit_title(s: &mut App, tx: &mut Tx) {
    let Some(item) = my_selected(s).filter(|m| m.is_open()) else { return };
    to_overview(s, tx, &item.id);
    s.edit_request(&item.id);
    tx.focus(TITLE_FIELD, true);
}

pub fn edit_reviewers(s: &mut App, tx: &mut Tx) {
    let Some(item) = my_selected(s).filter(|m| m.is_open()) else { return };
    to_overview(s, tx, &item.id);
    s.pick_reviewers(&item.id);
}

pub fn ask_again(s: &mut App, _: &mut Tx) {
    let Some(item) = my_selected(s).filter(|m| m.is_open()) else { return };
    let waiting = waiting_on(&item);
    if waiting.is_empty() {
        s.store.show("Nobody is waiting to review it.");
    } else if s.store.busy.is_none() {
        s.store.ask_again(&item, waiting);
    }
}

pub fn update_branch(s: &mut App, _: &mut Tx) {
    let Some(item) = my_selected(s).filter(|m| m.is_open()) else { return };
    if s.store.busy.is_none() {
        s.store.update_branch(&item);
    }
}

/// Asks first, on the overview: closing takes a second press, as merging does.
pub fn close(s: &mut App, tx: &mut Tx) {
    let Some(item) = my_selected(s).filter(|m| m.is_open()) else { return };
    to_overview(s, tx, &item.id);
    s.confirm = Some((item.id.clone(), Confirm::Close));
}

pub fn reopen(s: &mut App, _: &mut Tx) {
    let Some(item) = my_selected(s) else { return };
    if item.state == "closed" && s.store.busy.is_none() {
        s.store.set_open(&item, true);
    }
}

// MARK: Menus

/// What can be done to a request of mine, for its row's menu and the bar's More menu.
pub fn menu_items(menu: PopupMenu, item: &MergeRequest, idle: bool) -> PopupMenu {
    if !item.is_open() {
        return menu.item(PopupItem::new("reopen", "Reopen").enabled(idle && item.state == "closed"));
    }
    let waiting = waiting_on(item);
    let mut menu = menu.item(PopupItem::new("edit-title", "Edit Title and Description")).item(PopupItem::new("edit-reviewers", "Edit Reviewers…"));
    if !waiting.is_empty() {
        menu = menu.item(PopupItem::new("ask-again", ask_again_title(&waiting)).enabled(idle));
    }
    menu.item(PopupItem::new("update-branch", item.kind().update_branch_title()).enabled(idle))
        .separator()
        .item(PopupItem::new("close", "Close Without Merging…").enabled(idle))
}

/// Runs a pick from `menu_items` on the request `id`, selecting it first. False for other picks.
pub fn run_pick(s: &mut App, tx: &mut Tx, id: &str, picked: &str) -> bool {
    let run: fn(&mut App, &mut Tx) = match picked {
        "edit-title" => edit_title,
        "edit-reviewers" => edit_reviewers,
        "ask-again" => ask_again,
        "update-branch" => update_branch,
        "close" => close,
        "reopen" => reopen,
        _ => return false,
    };
    if s.store.selected_id.as_deref() != Some(id) {
        tx.instant();
        s.store.select(Some(id.to_string()));
    }
    run(s, tx);
    true
}

/// The bar's More button on a request of mine, which opens the menu above.
pub fn more_button(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let title = format!("More for this {}", item.kind().noun());
    let id = item.id.clone();
    let idle = s.store.busy.is_none();
    ChromeButton::new(MORE_BUTTON, title).icon(&I::ELLIPSIS, 14.0).build(p).on_press_tx(move |s: &mut App, tx: &mut Tx| {
        let Some(item) = s.store.item(&id).cloned() else { return };
        let id = id.clone();
        tx.popup_menu(menu_items(PopupMenu::new(), &item, idle), MORE_BUTTON, move |s: &mut App, picked, tx| {
            if let Some(picked) = picked {
                run_pick(s, tx, &id, picked);
            }
        });
    })
}

/// The row menu's items for a request of mine, after a separator.
pub fn row_menu(s: &App, menu: PopupMenu, item: &MergeRequest) -> PopupMenu {
    if !s.store.is_mine(item) {
        return menu;
    }
    menu_items(menu.separator(), item, s.store.busy.is_none())
}

// MARK: Asking again

/// A quiet request's way to ask again from the Mine list: a small button on its row.
pub fn ask_again_chip(p: &Pal, item: &MergeRequest, status: &mine::Status) -> Option<El> {
    let waiting = mine::ask_again(item, status);
    if waiting.is_empty() {
        return None;
    }
    let title = ask_again_title(&waiting);
    let id = item.id.clone();
    Some(
        row()
            .key("ask-again")
            .role("button")
            .label(title.clone())
            .h(20.0)
            .px(7.0)
            .gap(4.0)
            .center_y()
            .radius(5.0)
            .border(1.0, p.accent.alpha(0.35))
            .hover(p.accent_soft)
            .press(p.accent_soft)
            .help(title)
            .child(icon(&I::BELL, 11.0).color(p.accent))
            .child(text("Ask again").font(ui(12.0, MEDIUM)).color(p.accent).single_line())
            .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                let Some(item) = s.store.item(&id).cloned() else { return };
                let waiting = mine::ask_again(&item, &s.store.status_of(&item));
                if s.store.busy.is_none() {
                    s.store.ask_again(&item, waiting);
                }
            }),
    )
}

// MARK: Reviewers

/// Edit (or Add, with nobody asked yet) beside the reviewers of a request of mine.
pub fn reviewers_button(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let open = s.reviewer_picker.as_deref() == Some(item.id.as_str());
    let title = if item.reviewers.is_empty() { "Add Reviewers" } else { "Edit" };
    let id = item.id.clone();
    let mut b = ChromeButton::new(REVIEWERS_BUTTON, title).style(crate::ui::components::Chrome::Text).icon(&I::PERSON, 13.0).pad(8.0).on(open).build(p);
    b = b.label("Edit Reviewers").expanded(open).help("Edit Reviewers");
    b.on_press_tx(move |s: &mut App, tx: &mut Tx| {
        tx.instant();
        if s.reviewer_picker.take().is_none() {
            s.pick_reviewers(&id);
        }
    })
}

/// Who the picker offers: people who can review in the project, matching the search, less those
/// already asked and the author. None while the first list loads; the error if it failed.
pub fn candidates(s: &App, item: &MergeRequest) -> Result<Option<Vec<Person>>, String> {
    let query = s.people_filter.text.trim().to_lowercase();
    let base = match s.store.people(item, "") {
        Some(Loadable::Loaded(found)) => found.clone(),
        Some(Loadable::Failed(e)) => return Err(e.clone()),
        _ => return Ok(None),
    };
    // A project with more people than one answer holds is searched on the forge too.
    let searched = if query.is_empty() { None } else { s.store.people(item, &query).and_then(Loadable::value).cloned() };
    let pool = searched.unwrap_or(base);
    let taken = |p: &Person| {
        p.username.eq_ignore_ascii_case(&item.author.username) || item.reviewers.iter().any(|r| r.person.username.eq_ignore_ascii_case(&p.username))
    };
    let words: Vec<&str> = query.split_whitespace().collect();
    let fits = |p: &Person| {
        let hay = format!("{} {}", p.name.to_lowercase(), p.username.to_lowercase());
        words.iter().all(|w| hay.contains(w))
    };
    Ok(Some(pool.iter().filter(|p| !taken(p) && fits(p)).take(50).cloned().collect()))
}

/// Whether the person the keyboard is on is in the list; the first one otherwise.
fn picked(s: &App, list: &[Person]) -> usize {
    s.people_pick.min(list.len().saturating_sub(1))
}

/// Asks the person the keyboard is on.
fn add_picked(s: &mut App) {
    let Some(item) = s.reviewer_picker.as_ref().and_then(|id| s.store.item(id)).cloned() else { return };
    let Ok(Some(list)) = candidates(s, &item) else { return };
    if let Some(person) = list.get(picked(s, &list)) {
        s.store.add_reviewer(&item, person);
    }
}

fn caption(p: &Pal, label: &str) -> El {
    el().w_full().px(8.0).pt(10.0).pb(4.0).child(text(label.to_string()).font(ui(12.0, MEDIUM)).color(p.tertiary).single_line())
}

fn quiet_line(p: &Pal, line: String) -> El {
    el().w_full().px(8.0).py(8.0).child(text(line).font(ui(13.0, 400.0)).color(p.secondary).wrap())
}

/// A reviewer already asked: ask again, or take them off.
fn reviewer_row(p: &Pal, item: &MergeRequest, reviewer: &Reviewer, s: &App) -> El {
    let first = first_name(&reviewer.person).to_string();
    let (id, who) = (item.id.clone(), reviewer.clone());
    let again = ChromeButton::new(format!("again-{}", reviewer.person.username), format!("Ask {first} Again")).icon(&I::BELL, 13.0).build(p).on_press_tx(
        move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            if let Some(item) = s.store.item(&id).cloned() {
                s.store.ask_again(&item, vec![who.clone()]);
            }
        },
    );
    let removable = mine::can_remove(item, reviewer);
    let (id, who) = (item.id.clone(), reviewer.clone());
    let remove_title = if removable { format!("Remove {first}") } else { format!("{first} reviewed it, so {} keeps them listed", item.kind().name()) };
    let remove = ChromeButton::new(format!("remove-{}", reviewer.person.username), remove_title).icon(&I::CLOSE, 12.0).enabled(removable).build(p).on_press_tx(
        move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            if let Some(item) = s.store.item(&id).cloned() {
                s.store.remove_reviewer(&item, &who);
            }
        },
    );
    row()
        .key(format!("reviewer-{}", reviewer.person.username))
        .w_full()
        .h(40.0)
        .pl(8.0)
        .pr(2.0)
        .gap(10.0)
        .center_y()
        .child(avatar(s, p, &reviewer.person, 22.0))
        .child(
            col()
                .gap(1.0)
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .child(text(reviewer.person.name.clone()).font(ui(13.0, MEDIUM)).color(p.text).single_line().truncate())
                .child(
                    row()
                        .gap(4.0)
                        .center_y()
                        .child(state_icon(p, reviewer.state, 10.0))
                        .child(text(reviewer.state.label()).font(ui(12.0, 400.0)).color(p.tertiary).single_line()),
                ),
        )
        .child(again)
        .child(remove)
}

/// Someone who could be asked; the keyboard's pick has the hover fill.
fn person_row(s: &App, p: &Pal, item: &MergeRequest, person: &Person, on: bool) -> El {
    let (id, who) = (item.id.clone(), person.clone());
    row()
        .key(format!("person-{}", person.username))
        .role("option")
        .label(format!("Ask {}", person.name))
        .focus_selected(on)
        .w_full()
        .h(40.0)
        .px(8.0)
        .gap(10.0)
        .center_y()
        .radius(7.0)
        .fill(if on { p.hover } else { p.hover.alpha(0.0) })
        .hover_group()
        .hover(p.hover)
        .press(p.hover)
        .child(avatar(s, p, person, 22.0))
        .child(
            col()
                .gap(1.0)
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .child(text(person.name.clone()).font(ui(13.0, MEDIUM)).color(p.text).single_line().truncate())
                .child(text(format!("@{}", person.username)).font(ui(12.0, 400.0)).color(p.tertiary).single_line().truncate()),
        )
        .child(icon(&I::PLUS, 12.0).color(if on { p.text } else { p.tertiary }).group_color(p.text))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            if let Some(item) = s.store.item(&id).cloned() {
                s.store.add_reviewer(&item, &who);
            }
        })
}

/// The picker under the reviewers: who's asked (ask again, remove), then a search of the people
/// who can review in the project, to add. The keyboard moves through the list from the search.
pub fn picker(s: &App, p: &Pal, k: &Kit) -> Option<El> {
    let id = s.reviewer_picker.as_ref()?;
    let item = s.store.selected().filter(|m| &m.id == id && m.is_open())?;
    let found = candidates(s, item);
    let list = found.as_ref().ok().cloned().flatten().unwrap_or_default();
    let on = picked(s, &list);
    let search_id = item.id.clone();
    let field = TextInput::new(&s.people_filter)
        .plain()
        .width(None)
        .height(30.0)
        .text_style(ui(13.0, 400.0))
        .spellcheck(false)
        .placeholder(format!("Find people in {}", item.project_name))
        .label("Find people to ask for a review")
        .active_descendant(list.get(on).map(|p| format!("person-{}", p.username)))
        .on_edit(|s: &mut App| &mut s.people_filter)
        .on_change(move |s: &mut App, _: &mut Tx| {
            s.people_pick = 0;
            let Some(item) = s.store.item(&search_id).cloned() else { return };
            let query = s.people_filter.text.trim().to_string();
            let full = s.store.people(&item, "").and_then(Loadable::value).is_some_and(|l| l.len() >= PEOPLE_PAGE);
            if full && query.chars().count() >= 2 {
                s.store.find_people(&item, &query);
            }
        })
        .on_key(|s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| match ev.key {
            rune::Key::Down => {
                tx.instant();
                s.people_pick = s.people_pick.saturating_add(1);
                true
            }
            rune::Key::Up => {
                tx.instant();
                s.people_pick = s.people_pick.saturating_sub(1);
                true
            }
            _ => false,
        })
        .on_submit(|s: &mut App, tx: &mut Tx| {
            tx.instant();
            add_picked(s);
        })
        .build(k);
    let search = row()
        .key("people-filter")
        .w_full()
        .h(32.0)
        .pl(9.0)
        .pr(4.0)
        .gap(6.0)
        .center_y()
        .radius(7.0)
        .fill(p.hover)
        .child(icon(&I::SEARCH, 12.0).color(p.tertiary))
        .child(field.grow(1.0).shrink().min_w(0.0));
    let head = col()
        .w_full()
        .padding(12.0)
        .gap(10.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(text("Reviewers").font(ui(13.0, SEMIBOLD)).color(p.text).single_line().grow(1.0))
                .child(text(item.reference()).font(ui(12.0, 400.0)).color(p.tertiary).single_line()),
        )
        .child(search);

    let mut body = col().key("picker-list").w_full().px(4.0).pb(6.0);
    body = body.child(caption(p, "Asked"));
    if item.reviewers.is_empty() {
        body = body.child(quiet_line(p, "Nobody yet.".into()));
    }
    for reviewer in &item.reviewers {
        body = body.child(reviewer_row(p, item, reviewer, s));
    }
    body = body.child(caption(p, "Add a reviewer"));
    let query = s.people_filter.text.trim();
    let mut options = col().role("listbox").label("People who can review").w_full();
    match &found {
        Err(e) => {
            body = body.child(
                row()
                    .w_full()
                    .px(8.0)
                    .py(8.0)
                    .gap(6.0)
                    .items_start()
                    .child(icon(&I::WARNING, 13.0).color(p.red))
                    .child(text(format!("Couldn’t load people: {e}")).font(ui(13.0, 400.0)).color(p.red).wrap().grow(1.0).shrink().min_w(0.0)),
            )
        }
        Ok(None) => {
            body = body.child(
                row()
                    .w_full()
                    .px(8.0)
                    .py(8.0)
                    .gap(8.0)
                    .center_y()
                    .child(Spinner::new().size(12.0).build(k))
                    .child(text("Finding people…").font(ui(13.0, 400.0)).color(p.secondary)),
            )
        }
        Ok(Some(_)) if list.is_empty() && query.is_empty() => {
            body = body.child(quiet_line(p, format!("Everyone who can review in {} is asked already.", item.project_name)));
        }
        Ok(Some(_)) if list.is_empty() => {
            body = body.child(quiet_line(p, format!("Nobody in {} matches “{query}”.", item.project_name)));
        }
        Ok(Some(_)) => {
            for (i, person) in list.iter().enumerate() {
                options = options.child(person_row(s, p, item, person, i == on));
            }
            body = body.child(options);
        }
    }
    let scroller = col().key("picker-scroll").w_full().shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body);
    let hint = text(keys("↑↓ to choose   ↩ to ask   esc to close")).font(ui(12.0, 400.0)).color(p.tertiary).single_line().truncate();
    let panel = col()
        .key("reviewer-picker")
        .role("dialog")
        .label("Reviewers")
        .w(360.0)
        .max_h(480.0)
        .min_h(0.0)
        .radius(10.0)
        .fill(p.elevated)
        .border(1.0, p.line)
        .shadow(8.0, 28.0, shade(if p.dark { 0.45 } else { 0.14 }))
        .instant_changes()
        .child(head)
        .child(rule(p))
        .child(scroller)
        .child(rule(p))
        .child(el().w_full().px(12.0).py(9.0).child(hint))
        .focus_return(REVIEWERS_BUTTON)
        .modal_tx(true, |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.reviewer_picker = None;
        })
        .anchored_fit(REVIEWERS_BUTTON, Side::Bottom, Align::Start, 6.0);
    Some(panel)
}

// MARK: Title and description

/// Edit, at the end of the line above the title of an open request of mine.
pub fn edit_button(p: &Pal, item: &MergeRequest) -> El {
    let id = item.id.clone();
    ChromeButton::new("edit-request", "Edit")
        .style(crate::ui::components::Chrome::Text)
        .icon(&I::PENCIL_SQUARE, 13.0)
        .pad(8.0)
        .build(p)
        .label("Edit Title and Description")
        .help("Edit Title and Description")
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.edit_request(&id);
            tx.focus(TITLE_FIELD, true);
        })
}

fn edit_mut(s: &mut App) -> &mut RequestEdit {
    s.request_edit.get_or_insert_with(|| RequestEdit { id: String::new(), title: TextEdit::new(""), description: TextEdit::new(""), saving: None })
}

fn cancel(s: &mut App, tx: &mut Tx) {
    tx.instant();
    s.request_edit = None;
    tx.focus("edit-request", false);
}

/// ⌘↩ saves and Escape cancels, from either field.
fn editor_keys(s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
    match ev.key {
        rune::Key::Escape => {
            cancel(s, tx);
            true
        }
        rune::Key::Enter if ev.mods.cmd => {
            s.save_request_edit();
            true
        }
        _ => false,
    }
}

/// The title as a field, in place of the heading, while it's being edited. A GitLab draft's
/// prefix sits before it, out of reach: it's put back as it was on save.
pub fn title_field(s: &App, p: &Pal, k: &Kit, item: &MergeRequest) -> Option<El> {
    let edit = s.editing_request(&item.id)?;
    let prefix = mine::title_parts(item).0.trim_end();
    let input = TextInput::new(&edit.title)
        .plain()
        .width(None)
        .height(40.0)
        .text_style(serif(24.0, SEMIBOLD))
        .placeholder("Title")
        .label(format!("Title of the {}", item.kind().noun()))
        .on_edit(|s: &mut App| &mut edit_mut(s).title)
        .on_key(editor_keys)
        .on_submit(|s: &mut App, _: &mut Tx| s.save_request_edit())
        .build(k);
    let empty = edit.title.text.trim().is_empty();
    let mut field =
        row().key("edit-title").w_full().h(48.0).px(4.0).gap(2.0).center_y().radius(8.0).fill(p.paper).border(1.0, if empty { p.red } else { p.strong_line });
    if !prefix.is_empty() {
        field = field.child(
            text(prefix.to_string()).font(serif(24.0, SEMIBOLD)).color(p.tertiary).single_line().pl(10.0).help("Stays a draft: the prefix is kept as it is"),
        );
    }
    field = field.child(input.grow(1.0).shrink().min_w(0.0));
    let mut c = col().w_full().max_w(READING_WIDTH).gap(6.0).child(field);
    if empty {
        c = c.child(
            row()
                .gap(6.0)
                .center_y()
                .child(icon(&I::WARNING, 12.0).color(p.red))
                .child(text(format!("A {} needs a title.", item.kind().noun())).font(ui(12.0, 400.0)).color(p.red).single_line()),
        );
    }
    Some(c)
}

/// The description as a text box, with Cancel and Save, while it's being edited.
pub fn description_editor(s: &App, p: &Pal, k: &Kit, item: &MergeRequest) -> Option<El> {
    let edit = s.editing_request(&item.id)?;
    let field = TextArea::new(&edit.description)
        .width(None)
        .placeholder("Describe the change. Markdown works.")
        .label(format!("Description of the {}", item.kind().noun()))
        .auto_grow(8, 28)
        .on_edit(|s: &mut App| &mut edit_mut(s).description)
        .on_key(editor_keys)
        .build(k)
        .border(1.0, p.strong_line)
        .radius(10.0)
        .padding(12.0);
    let can_save = !edit.title.text.trim().is_empty() && s.store.busy.is_none();
    let buttons = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(keys("⌘↩ to save   esc to cancel")).font(ui(12.0, 400.0)).color(p.tertiary).single_line().truncate().shrink().min_w(0.0))
        .child(el().grow(1.0))
        .child(action_button(p, "cancel-edit", None, "Cancel", false, true).on_press_tx(cancel))
        .child(action_button(p, "save-edit", None, "Save", true, can_save).on_press(|s: &mut App| s.save_request_edit()));
    Some(col().key("edit-description").w_full().max_w(READING_WIDTH).pt(30.0).gap(10.0).child(field).child(buttons))
}

// MARK: The next step

/// What the next step offers beyond merging, on a request of mine: asking a quiet reviewer
/// again, and bringing a branch that fell behind up to date.
pub fn next_step_actions(s: &App, p: &Pal, item: &MergeRequest, standing: Standing) -> Vec<El> {
    let idle = s.store.busy.is_none();
    let mut actions = Vec::new();
    let waiting = mine::ask_again(item, &s.store.status_of(item));
    if standing == Standing::Quiet && !waiting.is_empty() {
        let id = item.id.clone();
        actions.push(action_button(p, "ask-again", Some(&I::BELL), &ask_again_title(&waiting), true, idle).on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            let Some(item) = s.store.item(&id).cloned() else { return };
            let waiting = mine::ask_again(&item, &s.store.status_of(&item));
            if s.store.busy.is_none() {
                s.store.ask_again(&item, waiting);
            }
        }));
    }
    if standing == Standing::NeedsYou && mine::needs_update(item) {
        let id = item.id.clone();
        actions.push(action_button(p, "update-branch", Some(&I::BRANCH), item.kind().update_branch_title(), true, idle).on_press_tx(
            move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                if let Some(item) = s.store.item(&id).cloned()
                    && s.store.busy.is_none()
                {
                    s.store.update_branch(&item);
                }
            },
        ));
    }
    actions
}

/// The question closing asks, in the next step's card, and its two answers.
pub fn close_confirm(s: &App, p: &Pal, item: &MergeRequest) -> (String, Vec<El>) {
    let question = format!("Close {} without merging? Its branch stays, and you can reopen it from {}.", item.reference(), item.kind().name());
    let id = item.id.clone();
    let close = action_button(p, "confirm-close", Some(&I::CLOSE), "Close", true, s.store.busy.is_none()).on_press_tx(move |s: &mut App, tx: &mut Tx| {
        tx.instant();
        s.confirm = None;
        if let Some(item) = s.store.item(&id).cloned()
            && s.store.busy.is_none()
        {
            s.store.set_open(&item, false);
        }
    });
    let cancel = action_button(p, "cancel-close", None, "Cancel", false, true).on_press_tx(|s: &mut App, tx: &mut Tx| {
        tx.instant();
        s.confirm = None;
    });
    (question, vec![close, cancel])
}

/// Reopen, under how a closed request of mine ended (until the next refresh drops it).
pub fn reopen_button(s: &App, p: &Pal, item: &MergeRequest) -> Option<El> {
    if item.state != "closed" || !s.store.is_mine(item) {
        return None;
    }
    Some(action_button(p, "reopen", Some(&I::UNDO), "Reopen", false, s.store.busy.is_none()).on_press_tx(reopen))
}
