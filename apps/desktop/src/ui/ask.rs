//! "Ask": a chat about the merge request beside the card, with a coding agent (Cursor, Claude,
//! opencode or pi) that can read and search the clone. A greeting and the composer in the middle
//! until the first question, then the transcript with the composer at the foot. Comments the
//! agent drafts show as cards, posted only when the user approves each one.

use std::path::PathBuf;

use rune::text::Span;
use rune::{Cursor, Drag, El, FileDialogOptions, Phase as DragPhase, PopupItem, PopupMenu, ScrollSpec, Tx, col, el, icon, rich, row, text};
use rune_kit::{Kit, TextArea};

use crate::app::{App, DraftKey, ProposalState, ProposalTarget};
use crate::data::ask::agents::AgentKind;
use crate::data::ask::citations;
use crate::data::ask::model::{AgentModel, MAKERS, featured};
use crate::data::ask::proposals::{self, Proposal, Segment};
use crate::data::ask::{Chat, Entry, EntryKind, Phase, wall};
use crate::data::models::MergeRequest;
use crate::data::prefs;
use crate::ui::components::{ChromeButton, action_button, copy_button, rule};
use crate::ui::icons as I;
use crate::ui::markdown::{self, Doc};
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{CARD_RADIUS, MEDIUM, Pal, SEMIBOLD, mono, serif, ui};

pub const COMPOSER: &str = "ask-composer";

const SUGGESTIONS: [(&str, &rune::Icon, &str); 4] = [
    ("Summarize", &I::LIST, "Summarize what this changes and why"),
    ("What could break?", &I::WARNING, "What could break, and where should I look first?"),
    ("Missing tests", &I::TESTS, "Are any tests missing for this change?"),
    ("Trickiest part", &I::PUZZLE, "Explain the trickiest part of the diff"),
];

/// Claude Code's working glyph: an asterisk that blooms and folds while the agent works.
const SPARK: [&str; 10] = ["·", "✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳", "✢"];

fn spark(p: &Pal, s: &App, animating: bool, size: f32) -> El {
    let glyph = if animating { SPARK[((s.now / 0.12) as usize) % SPARK.len()] } else { "✻" };
    // Every frame in the same box, so the text beside it doesn't jiggle.
    el().size(size + 2.0, size + 2.0).center().a11y_hidden().child(text(glyph).font(ui(size, SEMIBOLD)).color(p.accent))
}

/// The panel, with its resize handle on the left edge.
pub fn panel(s: &App, p: &Pal, k: &Kit, item: &MergeRequest) -> El {
    let chat = s.ask.chat(&item.id);
    let empty = chat.is_none_or(Chat::is_empty);
    let mut c = col()
        .key("ask")
        .role("complementary")
        .label(format!("Ask {}", s.ask.agent_for(&item.id).name()))
        .w(s.ask_width)
        .h_full()
        .radius(CARD_RADIUS)
        .fill(p.paper)
        .border(1.0, p.line)
        .clip_rounded(CARD_RADIUS)
        .child(header(s, p, item, empty))
        .child(rule(p));
    if empty {
        c = c
            .child(el().grow(1.0).min_h(12.0))
            .child(greeting(s, p, item))
            .child(el().w_full().px(12.0).child(composer(s, p, k, item, chat)))
            .child(chips(p, item))
            .child(el().grow(1.0).min_h(12.0));
    } else {
        let chat = chat.unwrap();
        c = c.child(transcript(s, p, k, item, chat)).child(el().w_full().px(12.0).pb(12.0).child(composer(s, p, k, item, Some(chat))));
    }
    row().key("ask-wrap").h_full().child(resize_handle()).child(c)
}

fn resize_handle() -> El {
    el().key("ask-resize").w(8.0).h_full().absolute(-6.0, 0.0).cursor(Cursor::ResizeColumn).help("Drag to resize").on_drag(
        |s: &mut App, d: &mut Drag, _: &mut Tx| match d.phase {
            DragPhase::Down => s.ask_drag_from = s.ask_width,
            DragPhase::Move => s.ask_width = (s.ask_drag_from - d.dx).clamp(320.0, 680.0),
            DragPhase::Up => {
                let w = s.ask_width as f64;
                prefs::update(|p| p.ask_width = w);
            }
        },
    )
}

fn header(s: &App, p: &Pal, item: &MergeRequest, empty: bool) -> El {
    let has_entries = s.ask.chat(&item.id).is_some_and(|c| !c.entries.is_empty());
    let mut r = row().key("ask-header").w_full().h(40.0).pl(14.0).pr(8.0).gap(6.0).center_y().child(agent_chip(s, p, item));
    if !empty {
        r = r.child(text(item.reference()).font(mono(12.0, 400.0)).color(p.tertiary).single_line().help(item.title.clone()));
    }
    let id = item.id.clone();
    r.child(el().grow(1.0))
        .child(
            ChromeButton::new("ask-new", "New Chat")
                .icon(&I::PENCIL_SQUARE, 13.0)
                .hint(crate::ui::components::keys("⇧⌘L"))
                .enabled(has_entries)
                .build(p)
                .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                    s.ask.new_chat(&id);
                    focus_composer(tx);
                }),
        )
        .child(
            ChromeButton::new("ask-close", "Close")
                .icon(&I::CLOSE, 12.0)
                .hint(crate::ui::components::keys("⌘L"))
                .build(p)
                .on_press(|s: &mut App| s.set_ask_visible(false)),
        )
}

/// "Ask Cursor ▾": picks the agent. Picking another for a chat that has started starts afresh.
fn agent_chip(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let agent = s.ask.agent_for(&item.id);
    let started = s.ask.chat(&item.id).is_some_and(|c| !c.is_empty());
    let id = item.id.clone();
    row()
        .key("ask-agent")
        .role("button")
        .label(format!("Ask {}", agent.name()))
        .help(if started { "Choose the agent. Another agent starts a new chat." } else { "Choose the agent" })
        .h(28.0)
        .ml(-6.0)
        .px(6.0)
        .gap(6.0)
        .center_y()
        .radius(7.0)
        .hover(p.hover)
        .press_scale(0.97)
        .child(icon(&I::BUBBLE_TEXT, 13.0).color(p.accent))
        .child(text(format!("Ask {}", agent.name())).font(ui(13.0, SEMIBOLD)).color(p.text).single_line())
        .child(icon(&I::CHEVRON_DOWN, 8.0).color(p.tertiary))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| open_agent_menu(s, tx, &id))
}

pub fn open_agent_menu(s: &mut App, tx: &mut Tx, chat: &str) {
    let current = s.ask.agent_for(chat);
    let mut menu = PopupMenu::new();
    for kind in AgentKind::ALL {
        menu = menu.item(PopupItem::new(kind.raw(), format!("{} — {}", kind.name(), kind.blurb())).checked(kind == current));
    }
    let chat = chat.to_string();
    tx.popup_menu(menu, "ask-agent", move |s: &mut App, picked, tx: &mut Tx| {
        let Some(kind) = picked.as_deref().and_then(AgentKind::from_raw) else { return };
        s.ask.choose_agent(kind, Some(&chat));
        focus_composer(tx);
    });
}

pub fn focus_composer(tx: &mut Tx) {
    tx.focus(format!("{COMPOSER}/input"), false);
}

fn greeting(s: &App, p: &Pal, item: &MergeRequest) -> El {
    col()
        .key("ask-greeting")
        .w_full()
        .px(28.0)
        .pb(18.0)
        .gap(8.0)
        .items_center()
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(spark(p, s, false, 22.0))
                .child(text(format!("Ask about {}", item.reference())).font(serif(22.0, SEMIBOLD)).color(p.text).single_line()),
        )
        .child(
            text(format!(
                "{} reads the description, the diff, the threads and your clone. It can't edit files or run commands, and the comments it drafts post only when you approve them.",
                s.ask.agent_for(&item.id).name()
            ))
            .font(ui(13.0, 400.0).lines(1.45))
            .color(p.secondary)
            .wrap()
            .align(rune::Align::Center),
        )
}

fn chips(p: &Pal, item: &MergeRequest) -> El {
    let mut r = row().key("ask-chips").w_full().px(16.0).pt(12.0).gap(6.0).flex_wrap().justify_center();
    for (label, glyph, prompt) in SUGGESTIONS {
        let id = item.id.clone();
        r = r.child(
            row()
                .key(format!("suggest-{label}"))
                .role("button")
                .label(label)
                .help(prompt)
                .h(30.0)
                .px(11.0)
                .gap(6.0)
                .center_y()
                .radius(15.0)
                .fill(p.paper)
                .border(1.0, p.strong_line)
                .hover(p.hover)
                .hover_group()
                .press_scale(0.97)
                .child(icon(glyph, 11.0).color(p.tertiary).group_color(p.accent))
                .child(text(label).font(ui(12.0, 400.0)).color(p.secondary).single_line())
                .on_press_tx(move |s: &mut App, tx: &mut Tx| send(s, tx, &id, prompt)),
        );
    }
    r
}

fn send(s: &mut App, tx: &mut Tx, id: &str, question: &str) {
    s.ask.ask(id, question);
    tx.scroll_to_bottom(format!("ask-transcript-{id}"));
}

// MARK: Transcript

/// The chat as Claude lays it out: your question with what it carried, then the agent's turn,
/// with its reading and thinking folded into one line between stretches of answer.
enum Item<'a> {
    Question(&'a Entry, Option<&'a Entry>),
    Activity(Vec<&'a Entry>),
    Answer(&'a Entry),
    Note(&'a Entry),
    Failure(&'a Entry),
}

fn build(entries: &[Entry]) -> Vec<Item<'_>> {
    // What the first question carried sits after it while asking, before it in a reopened chat.
    let mut contexts: Vec<(u64, &Entry)> = Vec::new();
    for (i, e) in entries.iter().enumerate().filter(|(_, e)| e.kind == EntryKind::Context) {
        if i > 0 && entries[i - 1].kind == EntryKind::Question {
            contexts.push((entries[i - 1].id, e));
        } else if i + 1 < entries.len() && entries[i + 1].kind == EntryKind::Question {
            contexts.push((entries[i + 1].id, e));
        }
    }
    let mut items: Vec<Item<'_>> = Vec::new();
    let mut steps: Vec<&Entry> = Vec::new();
    fn flush<'a>(steps: &mut Vec<&'a Entry>, items: &mut Vec<Item<'a>>) {
        if !steps.is_empty() {
            items.push(Item::Activity(std::mem::take(steps)));
        }
    }
    for e in entries {
        match e.kind {
            EntryKind::Thought | EntryKind::Tool { .. } | EntryKind::Declined => steps.push(e),
            EntryKind::Context => {}
            EntryKind::Question => {
                flush(&mut steps, &mut items);
                items.push(Item::Question(e, contexts.iter().find(|(q, _)| *q == e.id).map(|(_, c)| *c)));
            }
            // The blank lines an answer sometimes sends before a tool call aren't worth a block.
            EntryKind::Answer if e.text.trim().is_empty() => {}
            EntryKind::Answer => {
                flush(&mut steps, &mut items);
                items.push(Item::Answer(e));
            }
            EntryKind::Note => {
                flush(&mut steps, &mut items);
                items.push(Item::Note(e));
            }
            EntryKind::Failure => {
                flush(&mut steps, &mut items);
                items.push(Item::Failure(e));
            }
        }
    }
    flush(&mut steps, &mut items);
    items
}

fn transcript(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, chat: &Chat) -> El {
    let items = build(&chat.entries);
    let working = chat.phase != Phase::Idle;
    let mut c = col().w_full().px(16.0).pt(16.0).pb(8.0).gap(16.0);
    let count = items.len();
    for (index, it) in items.iter().enumerate() {
        let last = index + 1 == count;
        c = c.child(match it {
            Item::Question(e, context) => question(p, item, e, *context),
            Item::Activity(steps) => activity(s, p, chat, steps, working && last),
            Item::Answer(e) => {
                let copy = ends_turn(&items, index, working).then(|| turn_answer(&items, index, chat));
                answer(s, p, k, item, chat, e, working && last, copy, last && !working)
            }
            Item::Note(e) => note(p, e),
            Item::Failure(e) => failure(p, item, e, last && !working),
        });
    }
    let shows_working = working && !matches!(items.last(), Some(Item::Activity(_) | Item::Answer(_)));
    if shows_working {
        let label = if chat.reopening {
            "Opening the earlier chat".to_string()
        } else if chat.phase == Phase::Starting {
            format!("Starting {}", chat.agent.name())
        } else {
            verb(chat.turn_started).to_string()
        };
        let since = if chat.reopening { None } else { chat.turn_started };
        c = c.child(working_line(s, p, &label, since, chat.phase == Phase::Working));
    }
    let key = format!("ask-transcript-{}", item.id);
    let view = col().key(key.clone()).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: true }).child(c);
    crate::ui::smooth_scroll::smooth_wheel(view, &key)
}

/// Whether this answer is the turn's last: nothing but its steps comes before the next question.
fn ends_turn(items: &[Item<'_>], index: usize, working: bool) -> bool {
    for later in &items[index + 1..] {
        match later {
            Item::Answer(_) => return false,
            Item::Question(..) => return true,
            _ => {}
        }
    }
    !working
}

/// Every stretch of answer since the turn's question, as one markdown text.
fn turn_answer(items: &[Item<'_>], index: usize, chat: &Chat) -> String {
    let mut parts: Vec<String> = Vec::new();
    for earlier in items[..=index].iter().rev() {
        match earlier {
            Item::Question(..) => break,
            Item::Answer(e) => parts.insert(0, citations::expand(&e.text, chat.checkout.as_deref()).trim().to_string()),
            _ => {}
        }
    }
    parts.join("\n\n")
}

fn question(p: &Pal, item: &MergeRequest, e: &Entry, context: Option<&Entry>) -> El {
    let mut c = col().key(format!("q-{}", e.id)).w_full().radius(10.0).fill(p.well.alpha(0.55)).border(1.0, p.line).clip_rounded(10.0);
    if let Some(context) = context {
        c = c
            .child(
                row()
                    .w_full()
                    .h(26.0)
                    .px(12.0)
                    .gap(6.0)
                    .center_y()
                    .help(format!("Shared with the first question: {}", context.text))
                    .child(icon(&I::PULL, 10.0).color(p.accent))
                    .child(text(item.reference()).font(mono(12.0, MEDIUM)).color(p.secondary).single_line())
                    .child(text(context.text.clone()).font(ui(12.0, 400.0)).color(p.tertiary).single_line().truncate().shrink().min_w(0.0)),
            )
            .child(rule(p));
    }
    c.child(text(e.text.clone()).font(ui(14.0, 400.0).lines(1.4)).color(p.text).wrap().selectable(true).px(12.0).py(10.0).w_full())
}

#[allow(clippy::too_many_arguments)]
fn answer(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, chat: &Chat, e: &Entry, live: bool, copy: Option<String>, latest: bool) -> El {
    let mut c = col().key(format!("a-{}", e.id)).w_full().gap(6.0).hover_group();
    let segments = if proposals::has_comments(&e.text) { proposals::split(&e.text) } else { vec![Segment::Text(e.text.clone())] };
    for (index, segment) in segments.into_iter().enumerate() {
        c = c.child(match segment {
            Segment::Text(text) => {
                let source = expanded(&text, chat.checkout.as_deref());
                let key = format!("ask-{}-{}-{index}", item.id, e.id);
                markdown::markdown(s, p, k, Doc { key: &key, source: &source, base: item.project_url(), size: 14.0, serif: true })
            }
            Segment::Comment(proposal) => proposal_card(s, p, k, item, &proposal.key(&item.id), proposal),
        });
    }
    if live {
        c = c.child(spark(p, s, true, 14.0).mt(2.0));
    } else if let Some(copy) = copy {
        // Both keep the markdown: Copy for anywhere, Comment for this merge request's comment box.
        let id = item.id.clone();
        let body = copy.clone();
        let comment = row()
            .key(format!("comment-{}", e.id))
            .role("button")
            .label("Comment")
            .help(format!("Put this answer in the {}'s comment box, to edit and post", item.kind().noun()))
            .h(22.0)
            .px(5.0)
            .gap(4.0)
            .center_y()
            .radius(6.0)
            .hover(p.hover)
            .child(icon(&I::BUBBLE_TEXT, 11.0).color(p.tertiary))
            .child(text("Comment").font(ui(12.0, 400.0)).color(p.tertiary))
            .on_press_tx(move |s: &mut App, tx: &mut Tx| draft_comment(s, tx, &id, &body));
        let buttons = row()
            .gap(4.0)
            .center_y()
            .ml(-5.0)
            .child(copy_button(s, p, &format!("answer-{}", e.id), copy, "Copy Answer"))
            .child(text("Copy").font(ui(12.0, 400.0)).color(p.tertiary));
        // An answer with drafted comments posts through their cards instead.
        let buttons = if proposals::has_comments(&e.text) { buttons } else { buttons.child(comment.ml(6.0)) };
        c = c.child(if latest { buttons } else { buttons.group_opacity(0.0, 1.0) });
    }
    c
}

/// A comment the agent drafted, shown whole before anything posts: where it goes, its text
/// (editable here), and Post. Posting is the user's click, never the agent's.
fn proposal_card(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, key: &str, proposal: Proposal) -> El {
    let state = s.proposal_states.get(key).copied();
    let editing = s.proposal_editing.contains(key) && state != Some(ProposalState::Posted);
    let short = |path: &str| path.rsplit('/').next().unwrap_or(path).to_string();
    let target = s.proposal_target(&item.id, &proposal);
    let waiting = matches!(target, ProposalTarget::Waiting);
    let (place, warning) = match target {
        ProposalTarget::Waiting => (format!("On {} line {}", short(proposal.path.as_deref().unwrap_or("")), proposal.line.unwrap_or(0)), None),
        ProposalTarget::Line(file, line) => {
            let number = line.new_number.or(line.old_number).unwrap_or(0);
            (format!("On {} line {number}", short(&file.new_path)), None)
        }
        ProposalTarget::Whole { missed: None } => (format!("On {}", item.reference()), None),
        ProposalTarget::Whole { missed: Some((path, Some(line))) } => (
            format!("On {}", item.reference()),
            Some(format!("Line {line} of {} isn't in the diff, so this posts on the {} with its place written in.", short(&path), item.kind().noun())),
        ),
        ProposalTarget::Whole { missed: Some((path, None)) } => (
            format!("On {}", item.reference()),
            Some(format!("It names {} but no line, so it posts on the {} with the file written in.", short(&path), item.kind().noun())),
        ),
    };
    let status = match state {
        _ if proposal.open => "Drafting…".to_string(),
        None if waiting => "Loading the diff…".into(),
        Some(ProposalState::Posting) => "Posting…".into(),
        Some(ProposalState::Posted) => format!("Posted to {}", item.kind().name()),
        Some(ProposalState::Failed) => "Couldn't post".into(),
        None => "Not posted".into(),
    };
    let tint = match state {
        Some(ProposalState::Posted) => p.green,
        Some(ProposalState::Failed) => p.red,
        _ => p.tertiary,
    };
    let mut head = row()
        .w_full()
        .h(30.0)
        .px(12.0)
        .gap(6.0)
        .center_y()
        .child(icon(&I::BUBBLE_TEXT, 11.0).color(p.accent))
        .child(text(format!("Draft comment · {place}")).font(ui(12.0, MEDIUM)).color(p.secondary).single_line().truncate().shrink().min_w(0.0))
        .child(el().grow(1.0));
    if state == Some(ProposalState::Posted) {
        head = head.child(icon(&I::CHECK, 11.0).color(tint));
    }
    head = head.child(text(status).font(ui(12.0, 400.0)).color(tint).single_line());
    let body_text = s.proposal_text(key, &item.id, &proposal);
    let body = if editing {
        let edit_key = key.to_string();
        let empty = rune::TextEdit::new("");
        let edit = s.proposal_edits.get(key).unwrap_or(&empty);
        el().w_full().padding(8.0).child(
            TextArea::new(edit)
                .width(None)
                .placeholder("The comment, as it will be posted")
                .auto_grow(4, 16)
                .on_edit(move |s: &mut App| s.proposal_edits.entry(edit_key.clone()).or_insert_with(|| rune::TextEdit::new("")))
                .build(k)
                .border(1.0, p.line)
                .radius(8.0)
                .padding(10.0),
        )
    } else {
        let doc_key = format!("proposal-{key}");
        el().w_full().px(12.0).py(10.0).child(markdown::markdown(
            s,
            p,
            k,
            Doc { key: &doc_key, source: &body_text, base: item.project_url(), size: 14.0, serif: false },
        ))
    };
    let mut card = col()
        .key(format!("proposal-{key}"))
        .w_full()
        .radius(10.0)
        .fill(p.well.alpha(0.55))
        .border(1.0, if state == Some(ProposalState::Posted) { p.green.alpha(0.35) } else { p.strong_line })
        .clip_rounded(10.0)
        .child(head)
        .child(rule(p))
        .child(body);
    if let Some(warning) = warning.filter(|_| state != Some(ProposalState::Posted)) {
        card = card.child(
            row()
                .w_full()
                .px(12.0)
                .pb(8.0)
                .gap(6.0)
                .items_start()
                .child(icon(&I::WARNING, 11.0).color(p.amber).mt(2.0))
                .child(text(warning).font(ui(12.0, 400.0).lines(1.4)).color(p.secondary).wrap().grow(1.0).shrink().min_w(0.0)),
        );
    }
    if proposal.open || matches!(state, Some(ProposalState::Posting | ProposalState::Posted)) {
        return card;
    }
    let can_post = !body_text.trim().is_empty() && s.store.busy.is_none() && !waiting;
    let (edit_key, edit_text) = (key.to_string(), body_text.clone());
    let edit_button =
        action_button(p, format!("edit-{key}"), Some(&I::PENCIL_SQUARE), if editing { "Done" } else { "Edit" }, false, true).on_press(move |s: &mut App| {
            if !s.proposal_editing.remove(&edit_key) {
                s.proposal_edits.entry(edit_key.clone()).or_insert_with(|| rune::TextEdit::new(edit_text.clone()));
                s.proposal_editing.insert(edit_key.clone());
            }
        });
    let (post_key, id, posted) = (key.to_string(), item.id.clone(), proposal.clone());
    let post_label = if state == Some(ProposalState::Failed) { "Try Again" } else { "Post" };
    let post_button = action_button(p, format!("post-{key}"), Some(&I::BUBBLE_TEXT), post_label, true, can_post)
        .help(format!("Post this comment to {} as you", item.kind().name()))
        .on_press(move |s: &mut App| s.post_proposal(&post_key, &id, &posted));
    card.child(row().w_full().px(12.0).pb(10.0).gap(8.0).center_y().child(el().grow(1.0)).child(edit_button).child(post_button))
}

/// Adds an answer to the merge request's comment draft and shows that box, at the end of the
/// conversation, with the caret in it. Nothing posts until the comment is sent from there.
fn draft_comment(s: &mut App, tx: &mut Tx, id: &str, body: &str) {
    let key = DraftKey::Comment(id.to_string());
    let draft = s.draft_mut(&key);
    let text = if draft.text.trim().is_empty() { body.to_string() } else { format!("{}\n\n{body}", draft.text.trim_end()) };
    *draft = rune::TextEdit::new(text);
    crate::ui::detail::compose(tx, id);
}

thread_local! {
    /// Answers with their citations filled, by text: an answer is drawn every frame it streams.
    static EXPANDED: std::cell::RefCell<std::collections::HashMap<(u64, Option<PathBuf>), std::rc::Rc<str>>> = Default::default();
}

fn expanded(text: &str, checkout: Option<&std::path::Path>) -> std::rc::Rc<str> {
    if !text.contains("```") {
        return text.into();
    }
    let key = (crate::data::models::fnv1a(text) as u64, checkout.map(PathBuf::from));
    EXPANDED.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 400 {
            m.clear();
        }
        m.entry(key).or_insert_with(|| captioned(&citations::expand(text, checkout)).into()).clone()
    })
}

/// A citation's fence names its file and lines ("```src/x.rs lines 3–9"); shown, that becomes a
/// caption over a fence typed by the file's extension, so the code is coloured.
fn captioned(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let cited = line.strip_prefix("```").and_then(|info| {
            let (path, lines) = info.split_once(" line")?;
            let ext = path.rsplit_once('.').map(|(_, e)| e).filter(|e| !e.contains('/'))?;
            Some((path.to_string(), format!("line{lines}"), ext.to_string()))
        });
        match cited {
            Some((path, lines, ext)) => {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                out.push(format!("`{name}` · {lines}"));
                out.push(String::new());
                out.push(format!("```{ext}"));
            }
            None => out.push(line.to_string()),
        }
    }
    out.join("\n")
}

fn note(p: &Pal, e: &Entry) -> El {
    row()
        .gap(6.0)
        .center_y()
        .child(icon(if e.text == "Stopped" { &I::STOP } else { &I::EXCLAIM_CIRCLE }, 12.0).color(p.tertiary))
        .child(text(e.text.clone()).font(ui(12.0, 400.0)).color(p.tertiary).wrap())
}

fn failure(p: &Pal, item: &MergeRequest, e: &Entry, retry: bool) -> El {
    let mut c = col().key(format!("f-{}", e.id)).w_full().padding(11.0).gap(9.0).radius(10.0).fill(p.red.alpha(0.07)).border(1.0, p.red.alpha(0.22)).child(
        row()
            .w_full()
            .gap(8.0)
            .items_start()
            .child(icon(&I::WARNING, 12.0).color(p.red).mt(2.0))
            .child(text(e.text.clone()).font(ui(13.0, 400.0).lines(1.4)).color(p.text).wrap().selectable(true).grow(1.0).shrink().min_w(0.0)),
    );
    if retry {
        let id = item.id.clone();
        c = c.child(
            row()
                .key("ask-retry")
                .role("button")
                .label("Try again")
                .self_start()
                .ml(20.0)
                .h(26.0)
                .px(10.0)
                .gap(5.0)
                .center_y()
                .radius(7.0)
                .fill(p.elevated)
                .border(1.0, p.line)
                .hover(p.hover)
                .press_scale(0.97)
                .child(icon(&I::REFRESH, 11.0).color(p.text))
                .child(text("Try again").font(ui(12.0, MEDIUM)).color(p.text))
                .on_press(move |s: &mut App| s.ask.retry(&id)),
        );
    }
    c
}

// MARK: Activity

fn duration(seconds: f64) -> String {
    let whole = seconds.round() as i64;
    if whole < 60 { format!("{whole}s") } else { format!("{}m {}s", whole / 60, whole % 60) }
}

/// "Read src/a/b/x.ts (419 - 463)" → ("Read", "x.ts (419 - 463)"): the verb, then the rest with
/// paths cut to their file names (the whole title is the tooltip).
pub fn split_title(title: &str) -> (String, String) {
    let shorten = |t: &str| -> String {
        t.split(' ').map(|w| if w.contains('/') && !w.starts_with('"') { w.rsplit('/').next().unwrap_or(w) } else { w }).collect::<Vec<_>>().join(" ")
    };
    let mut words = title.splitn(2, ' ');
    let first = words.next().unwrap_or("");
    if first.is_empty() || !first.chars().all(char::is_alphabetic) {
        return (String::new(), shorten(title));
    }
    let mut c = first.chars();
    let verb = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
    (verb, words.next().map(shorten).unwrap_or_default())
}

fn live_label(steps: &[&Entry]) -> String {
    let Some(last) = steps.last() else { return "Thinking".into() };
    match &last.kind {
        EntryKind::Tool { kind, done: None } => {
            let (_, rest) = split_title(&last.text);
            match kind.as_str() {
                "read" if rest.is_empty() => "Reading".into(),
                "read" => format!("Reading {rest}"),
                "search" if rest.is_empty() => "Searching the code".into(),
                "search" => format!("Searching {rest}"),
                "fetch" => format!("Fetching {rest}"),
                _ => last.text.clone(),
            }
        }
        _ => "Thinking".into(),
    }
}

/// "Thought for 12s, read 2 files and searched the code".
pub fn summary(steps: &[&Entry]) -> String {
    let (mut reads, mut searches, mut fetches, mut others, mut declined) = (Vec::new(), 0, 0, 0, 0);
    let (mut thinking, mut thought) = (0.0, false);
    for step in steps {
        match &step.kind {
            EntryKind::Thought => {
                thought = true;
                thinking += step.finished - step.started;
            }
            EntryKind::Tool { kind, .. } => match kind.as_str() {
                "read" => reads.push(split_title(&step.text).1.split(' ').next().unwrap_or("").to_string()),
                "search" => searches += 1,
                "fetch" => fetches += 1,
                _ => others += 1,
            },
            EntryKind::Declined => declined += 1,
            _ => {}
        }
    }
    let plural = |n: usize, one: &str, many: &str| if n == 1 { one.to_string() } else { many.replace('#', &n.to_string()) };
    let mut parts: Vec<String> = Vec::new();
    if thought {
        parts.push(if thinking >= 1.0 { format!("thought for {}", duration(thinking)) } else { "thought".into() });
    }
    match reads.as_slice() {
        [one] if !one.is_empty() => parts.push(format!("read {one}")),
        [] => {}
        many => parts.push(format!("read {} files", many.len())),
    }
    if searches > 0 {
        parts.push(plural(searches, "searched the code", "searched # times"));
    }
    if fetches > 0 {
        parts.push(plural(fetches, "fetched 1 page", "fetched # pages"));
    }
    if others > 0 {
        parts.push(plural(others, "used 1 tool", "used # tools"));
    }
    if declined > 0 {
        parts.push(plural(declined, "declined 1 request", "declined # requests"));
    }
    let joined = match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        2 => format!("{} and {}", parts[0], parts[1]),
        n => format!("{}, and {}", parts[..n - 1].join(", "), parts[n - 1]),
    };
    let mut c = joined.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn activity(s: &App, p: &Pal, chat: &Chat, steps: &[&Entry], live: bool) -> El {
    let key = format!("{}|{}", chat.id, steps[0].id);
    let open = s.ask_open.contains(&key);
    let label = if live { live_label(steps) } else { summary(steps) };
    let mut head = row().key(format!("act-{}", steps[0].id)).role("button").label(label.clone()).w_full().gap(6.0).center_y().help(if open {
        "Hide the steps"
    } else {
        "Show the steps"
    });
    if live {
        head = head.child(spark(p, s, true, 12.0));
    }
    head = head.child(text(label).font(ui(13.0, 400.0)).color(p.secondary).single_line().truncate().shrink().min_w(0.0));
    if live {
        if let Some(since) = chat.turn_started {
            head = head.child(elapsed(p, since, true));
        }
    }
    let toggle = key.clone();
    head =
        head.child(icon(if open { &I::CHEVRON_DOWN } else { &I::CHEVRON_RIGHT }, 9.0).color(p.tertiary)).child(el().grow(1.0)).on_press(move |s: &mut App| {
            if !s.ask_open.remove(&toggle) {
                s.ask_open.insert(toggle.clone());
            }
        });
    let mut c = col().w_full().child(head);
    if open {
        let mut list = col().w_full().pt(10.0);
        for (i, step) in steps.iter().enumerate() {
            let last = i + 1 == steps.len();
            list = list.child(step_row(s, p, chat, step, live && last, last));
        }
        c = c.child(list);
    }
    c
}

fn step_row(s: &App, p: &Pal, chat: &Chat, step: &Entry, live: bool, last: bool) -> El {
    let failed = matches!(step.kind, EntryKind::Tool { done: Some(false), .. } | EntryKind::Declined);
    let glyph: &rune::Icon = match &step.kind {
        EntryKind::Thought => &I::BRAIN,
        EntryKind::Declined => &I::EXCLAIM_CIRCLE,
        EntryKind::Tool { kind, .. } => match kind.as_str() {
            "read" => &I::DOC_ARROW,
            "search" => &I::SEARCH,
            "fetch" => &I::OPEN_EXTERNAL,
            "execute" => &I::TERMINAL,
            "edit" | "delete" | "move" => &I::PENCIL_SQUARE,
            _ => &I::CPU,
        },
        _ => &I::PLAIN_CIRCLE,
    };
    let mut gutter =
        col().w(16.0).self_stretch().items_center().child(el().h(18.0).center().child(icon(glyph, 11.0).color(if failed { p.red } else { p.tertiary })));
    if !last {
        // A rule runs down to the next step.
        gutter = gutter.child(el().w(1.0).grow(1.0).mt(2.0).fill(p.line));
    }
    let content = match &step.kind {
        EntryKind::Thought => {
            let seconds = step.finished - step.started;
            let title = if live {
                "Thinking…".to_string()
            } else if seconds >= 1.0 {
                format!("Thought for {}", duration(seconds))
            } else {
                "Thought".into()
            };
            let key = format!("{}|thought-{}", chat.id, step.id);
            let whole = s.ask_open.contains(&key);
            let body = step.text.trim().to_string();
            let long = body.chars().count() > 280;
            let mut c = col().w_full().gap(4.0).child(el().h(18.0).center_y().child(text(title).font(ui(12.0, MEDIUM)).color(p.secondary))).child({
                let t = text(body).font(ui(12.0, 400.0).lines(1.45)).color(p.tertiary).wrap().selectable(true);
                if whole { t } else { t.max_lines(4) }
            });
            if long && !whole {
                c = c.child(text("Show all").key(format!("show-{}", step.id)).role("button").font(ui(12.0, MEDIUM)).color(p.accent).on_press(
                    move |s: &mut App| {
                        s.ask_open.insert(key.clone());
                    },
                ));
            }
            c
        }
        EntryKind::Tool { done, .. } => {
            let (verb, rest) = split_title(&step.text);
            let line = if rest.is_empty() { verb.clone() } else { format!("{verb}  {rest}") };
            let spans = vec![Span::new(0, verb.len()).weight(MEDIUM).color(p.secondary), Span::new(verb.len(), line.len()).mono().size(12.0).color(p.tertiary)];
            let mut r = row()
                .w_full()
                .min_h(18.0)
                .gap(5.0)
                .center_y()
                .help(step.text.clone())
                .child(rich(line, spans).font(ui(12.0, 400.0)).max_lines(2).truncate().grow(1.0).shrink().min_w(0.0));
            if done.is_none() {
                r = r.child(rune_kit::Spinner::new().size(10.0).build(&crate::ui::look(p.dark).k));
            }
            col().w_full().child(r)
        }
        EntryKind::Declined => {
            col().child(el().min_h(18.0).center_y().child(text(format!("{}: not allowed in Mrow", step.text)).font(ui(12.0, 400.0)).color(p.secondary).wrap()))
        }
        _ => col(),
    };
    row().w_full().gap(9.0).child(gutter).child(content.grow(1.0).shrink().min_w(0.0).pb(if last { 0.0 } else { 10.0 }))
}

/// One of Claude Code's verbs, the same for the whole of a turn.
fn verb(turn: Option<f64>) -> &'static str {
    const VERBS: [&str; 8] = ["Thinking", "Pondering", "Considering", "Mulling", "Reviewing", "Cogitating", "Deliberating", "Puzzling"];
    turn.map_or(VERBS[0], |t| VERBS[(t as i64).rem_euclid(VERBS.len() as i64) as usize])
}

/// "✻ Pondering… 12s · esc to stop", until the first word or step.
fn working_line(s: &App, p: &Pal, label: &str, since: Option<f64>, stoppable: bool) -> El {
    let mut r = row()
        .key("ask-working")
        .role("status")
        .label(format!("{label}…"))
        .gap(7.0)
        .center_y()
        .child(spark(p, s, true, 13.0))
        .child(text(format!("{label}…")).font(ui(13.0, MEDIUM)).color(p.secondary).single_line());
    if let Some(since) = since {
        r = r.child(elapsed(p, since, stoppable));
    }
    r
}

/// "12s · esc to stop", counting up from the turn's start.
fn elapsed(p: &Pal, since: f64, stoppable: bool) -> El {
    let seconds = (wall() - since).max(0.0).floor();
    let label = if seconds < 1.0 { String::new() } else { format!("{}{}", duration(seconds), if stoppable { " · esc to stop" } else { "" }) };
    text(label).font(ui(12.0, 400.0).features("'tnum' 1")).color(p.tertiary).single_line()
}

// MARK: Composer

fn composer(s: &App, p: &Pal, k: &Kit, item: &MergeRequest, chat: Option<&Chat>) -> El {
    let empty_edit = rune::TextEdit::new("");
    let edit = chat.map_or(&empty_edit, |c| &c.draft);
    let working = chat.is_some_and(|c| c.phase == Phase::Working);
    let idle = chat.is_none_or(|c| c.phase == Phase::Idle);
    let can_send = idle && !edit.text.trim().is_empty();
    let placeholder = if chat.is_some_and(Chat::has_question) { "Reply…".to_string() } else { format!("How can I help with {}?", item.reference()) };
    let (edit_id, key_id) = (item.id.clone(), item.id.clone());
    // A bare editor: the composer is the field's frame.
    let field = rune::text_input(edit, ui(14.0, 400.0).lines(1.4))
        .key("input")
        .w_full()
        .color(p.text)
        .placeholder(placeholder.clone(), p.tertiary)
        .caret(p.text, k.selection())
        .label(placeholder)
        .on_edit(move |s: &mut App| &mut s.ask.chat_mut(&edit_id).draft)
        .on_input_key(move |s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| match ev.key {
            // ↩ sends; ⌥↩ (or ⇧↩) is a new line.
            rune::Key::Enter if !ev.mods.alt && !ev.mods.shift => {
                let draft = s.ask.chat_mut(&key_id).draft.text.clone();
                send(s, tx, &key_id, &draft);
                true
            }
            rune::Key::Escape if s.ask.chat(&key_id).is_some_and(|c| c.phase == Phase::Working) => {
                s.ask.stop(&key_id);
                true
            }
            _ => false,
        });
    let field = rune::auto_grow(field, 1, 10);
    let (send_id, stop_id) = (item.id.clone(), item.id.clone());
    let active = can_send || working;
    let send_button = el()
        .key("ask-send")
        .role("button")
        .label(if working { "Stop" } else { "Send" })
        .help(if working { "Stop  esc".to_string() } else { crate::ui::components::keys("Send  ↩    New line  ⌥↩") })
        .size(28.0, 28.0)
        .ml(4.0)
        .radius(8.0)
        .center()
        .fill(p.accent.alpha(if active { 1.0 } else { 0.35 }))
        .child(icon(if working { &I::STOP } else { &I::ARROW_UP }, if working { 11.0 } else { 13.0 }).color(rune::Color::WHITE));
    let send_button = if !active {
        send_button.disabled(true)
    } else if working {
        send_button.press_scale(0.97).on_press(move |s: &mut App| s.ask.stop(&stop_id))
    } else {
        send_button.press_scale(0.97).on_press_tx(move |s: &mut App, tx: &mut Tx| {
            let draft = s.ask.chat_mut(&send_id).draft.text.clone();
            send(s, tx, &send_id, &draft);
        })
    };
    col()
        .key(COMPOSER)
        .w_full()
        .radius(16.0)
        .fill(p.elevated)
        .border(1.0, if edit.focused { p.accent.alpha(0.45) } else { p.strong_line })
        .shadow(2.0, 8.0, crate::ui::theme::shade(0.05))
        .child(
            el().key("ask-field")
                .w_full()
                .px(14.0)
                .pt(12.0)
                .pb(8.0)
                .cursor(Cursor::Text)
                .on_press_tx(|_: &mut App, tx: &mut Tx| focus_composer(tx))
                .child(field),
        )
        .child(
            row()
                .w_full()
                .px(8.0)
                .pb(8.0)
                .gap(2.0)
                .center_y()
                .child(clone_chip(p, item, chat, s.ask.agent_for(&item.id)))
                .child(el().grow(1.0).min_w(6.0))
                .child(model_chip(s, p, item))
                .child(send_button),
        )
}

/// A quiet button in the composer's bottom row: an icon and a few words, filled on hover.
fn composer_chip(p: &Pal, key: &str, glyph: Option<&'static rune::Icon>, title: String, trailing: Option<String>, tint: rune::Color) -> El {
    let mut r = row().key(key.to_string()).role("button").label(title.clone()).h(28.0).px(8.0).gap(5.0).center_y().radius(8.0).hover(p.hover).press_scale(0.97);
    if let Some(g) = glyph {
        r = r.child(icon(g, 12.0).color(tint));
    }
    r = r.child(text(title).font(ui(12.0, MEDIUM)).color(tint).single_line().truncate().shrink().min_w(0.0));
    if let Some(t) = trailing {
        r = r.child(text(t).font(ui(12.0, 400.0)).color(p.tertiary).single_line());
    }
    r.child(icon(&I::CHEVRON_DOWN, 8.0).color(p.tertiary))
}

/// Which clone the agent reads: click it to pick another.
fn clone_chip(p: &Pal, item: &MergeRequest, chat: Option<&Chat>, agent: AgentKind) -> El {
    let checkout = chat.and_then(|c| c.checkout.clone());
    let looked = chat.is_some_and(|c| c.checkout_looked);
    let started = chat.is_some_and(|c| !c.entries.is_empty());
    let title = match (&checkout, looked) {
        (Some(c), _) => c.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        (None, true) => "No clone".into(),
        (None, false) => "Looking…".into(),
    };
    let read = format!("{} It doesn't post to {}.", agent.reach(), item.kind().name());
    let help = match &checkout {
        None => format!("No clone of {} found, so {} sees only the {}. Click to choose one.\n{read}", item.project_path, agent.name(), item.kind().noun()),
        Some(c) => {
            let home = std::env::var("HOME").unwrap_or_default();
            let shown = c.to_string_lossy().replacen(&home, "~", 1);
            format!(
                "{} reads {shown} as it is, whatever branch it's on; the {}'s own changes come from the diff.\nClick to choose another clone{}\n{read}",
                agent.name(),
                item.kind().noun(),
                if started { " (starts a new chat)." } else { "." }
            )
        }
    };
    let tint = if checkout.is_none() && looked { p.amber } else { p.secondary };
    let (project, id, directory) = (item.project_path.clone(), item.id.clone(), checkout.clone());
    composer_chip(p, "ask-clone", Some(&I::FOLDER), title, None, tint).help(help).on_press_tx(move |s: &mut App, tx: &mut Tx| {
        let home = std::env::var("HOME").map(PathBuf::from).ok();
        let options = FileDialogOptions {
            title: format!("Choose your local clone of {project}{}", if started { ". This starts a new chat." } else { "" }),
            directory: directory.clone().or(home),
            ..Default::default()
        };
        let id = id.clone();
        let _ = s;
        tx.open_folder(options, move |s: &mut App, result, tx: &mut Tx| {
            if let Ok(Some(folder)) = result {
                if let Some(item) = s.store.item(&id).cloned() {
                    s.ask.choose_folder(folder, &item);
                    focus_composer(tx);
                }
            }
        });
    })
}

fn model_chip(s: &App, p: &Pal, item: &MergeRequest) -> El {
    let roster = s.ask.roster(s.ask.agent_for(&item.id));
    let title = match roster.chosen_model() {
        Some(m) => m.short_title(),
        None if roster.model.is_empty() => "Default model".into(),
        None => AgentModel::new(&roster.model, "").short_title(),
    };
    let effort = roster.effort(&roster.model).map(|e| e.name.clone());
    let id = item.id.clone();
    composer_chip(p, "ask-model", None, title, effort, p.secondary)
        .help(crate::ui::components::keys("Choose the model and effort  ⌥⌘L"))
        .label("Model")
        .on_press_tx(move |s: &mut App, tx: &mut Tx| open_model_menu(s, tx, &id))
}

/// The agent's models as a menu: all of them when there are a few; else the newest of a few
/// families with a line on when each suits and "More Models" by maker for the rest. Then the
/// chosen model's effort levels.
pub fn open_model_menu(s: &mut App, tx: &mut Tx, chat: &str) {
    let agent = s.ask.agent_for(chat);
    s.ask.load_models(agent, false);
    s.ask.describe(agent);
    let ask = s.ask.roster(agent);
    let mut menu = PopupMenu::new();
    if ask.models.is_empty() {
        let line = match (&ask.error, ask.loading) {
            (Some(e), false) => e.clone(),
            _ => format!("Asking {} for its models…", agent.name()),
        };
        menu = menu.item(PopupItem::new("none", line).enabled(false)).separator().item(PopupItem::new("refresh", "Try Again"));
    } else {
        let few = ask.models.len() <= 10;
        let mut list = if few { ask.models.clone() } else { featured(&ask.models) };
        for id in std::iter::once(&ask.model).chain(&ask.recent) {
            if let Some(m) = ask.models.iter().find(|m| &m.id == id) {
                if !list.contains(m) {
                    list.push(m.clone());
                }
            }
        }
        if !few {
            list.truncate(8);
        }
        for m in &list {
            let detail = m
                .blurb()
                .filter(|_| agent == AgentKind::Cursor)
                .map(str::to_string)
                .unwrap_or_else(|| if ask.recent.contains(&m.id) { format!("{} · used recently", m.maker()) } else { m.maker().to_string() });
            menu = menu.item(PopupItem::new(format!("m:{}", m.id), format!("{} — {detail}", m.title())).checked(m.id == ask.model));
        }
        if !few {
            let mut more = PopupMenu::new();
            for maker in MAKERS {
                let models: Vec<&AgentModel> = ask.models.iter().filter(|m| m.maker() == maker).collect();
                if models.is_empty() {
                    continue;
                }
                let mut sub = PopupMenu::new();
                for m in models {
                    sub = sub.item(PopupItem::new(format!("m:{}", m.id), m.title()).checked(m.id == ask.model));
                }
                more = more.submenu(maker, sub);
            }
            menu = menu.submenu(format!("More Models ({})", ask.models.len()), more);
        }
        menu = menu.separator();
        let name = ask.chosen_model().map(AgentModel::title).unwrap_or_else(|| "This model".into());
        match ask.efforts.get(&ask.model) {
            Some(options) if !options.values.is_empty() => {
                let current = ask.effort(&ask.model).map(|e| e.value.clone());
                menu = menu.item(PopupItem::new("effort", "Effort: faster ↔ more thorough").enabled(false));
                for o in &options.values {
                    menu = menu.item(PopupItem::new(format!("e:{}", o.value), o.name.clone()).checked(current.as_deref() == Some(&o.value)));
                }
            }
            Some(_) => menu = menu.item(PopupItem::new("effort", format!("{name} sets its own effort.")).enabled(false)),
            None if ask.model.is_empty() => {}
            None => menu = menu.item(PopupItem::new("effort", format!("Asking {} for its effort levels…", agent.name())).enabled(false)),
        }
        menu = menu.separator().item(PopupItem::new("refresh", "Refresh the List"));
    }
    let chat = chat.to_string();
    tx.popup_menu(menu, "ask-model", move |s: &mut App, picked, _: &mut Tx| {
        let Some(picked) = picked else { return };
        if let Some(model) = picked.strip_prefix("m:") {
            s.ask.choose_model(model, Some(&chat));
        } else if let Some(value) = picked.strip_prefix("e:") {
            s.ask.choose_effort(value, Some(&chat));
        } else if picked == "refresh" {
            let agent = s.ask.agent_for(&chat);
            s.ask.load_models(agent, true);
        }
    });
}

/// The working glyph and timers move while any chat works: a few redraws a second.
pub fn animating(s: &App) -> bool {
    s.ask_visible && s.store.selected_id.as_ref().and_then(|id| s.ask.chat(id)).is_some_and(|c| c.phase != Phase::Idle)
}
