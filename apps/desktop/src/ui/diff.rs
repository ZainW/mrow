//! A diff line: old and new numbers, the sign, and the code coloured by rune-kit's lexers with
//! the words that changed marked. Each file is styled once per content (its digest) and kept.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rune::text::Span;
use rune::{El, Tx, el, icon, rich, row, text};
use rune_kit::Kit;
use rune_kit::syntax::{self, Paint, SyntaxTheme};

use crate::app::{App, DraftKey};
use crate::data::diff::{DiffLine, LineKind};
use crate::data::models::FileDiff;
use crate::data::styler::{self, LineStyle};
use crate::ui::icons as I;
use crate::ui::theme::LineHeight as _;
use crate::ui::theme::{Pal, mono};

type Styles = Rc<Vec<LineStyle<Paint>>>;

thread_local! {
    /// Styled files by "id|digest", most recent last; the oldest go past `LIMIT`.
    static STYLED: RefCell<(HashMap<String, Styles>, Vec<String>)> = RefCell::new((HashMap::new(), Vec::new()));
    static THEMES: RefCell<[Option<Rc<SyntaxTheme>>; 2]> = const { RefCell::new([None, None]) };
    /// Files' texts styled whole, by "commit|path": the lines a diff's gaps show.
    static TEXTS: RefCell<HashMap<String, Styles>> = RefCell::new(HashMap::new());
}

/// How many styled texts are kept before they're all let go.
const TEXT_LIMIT: usize = 40;

const LIMIT: usize = 400;

/// The file's line styles, lexing it the first time it's drawn.
pub fn styles(file: &FileDiff) -> Styles {
    let key = format!("{}|{}", file.id(), file.digest);
    if let Some(found) = STYLED.with(|s| s.borrow().0.get(&key).cloned()) {
        return found;
    }
    let styled: Styles = Rc::new(style_file(file));
    STYLED.with(|s| {
        let mut s = s.borrow_mut();
        s.0.insert(key.clone(), styled.clone());
        s.1.push(key);
        while s.1.len() > LIMIT {
            let old = s.1.remove(0);
            s.0.remove(&old);
        }
    });
    styled
}

/// Lexes a file and marks its changed words, without the cache.
pub fn style_file(file: &FileDiff) -> Vec<LineStyle<Paint>> {
    let lang = syntax::lang_for_path(&file.new_path);
    let lex = |code: &str| -> Vec<(std::ops::Range<usize>, Paint)> {
        match lang {
            Some(l) => syntax::highlight(Some(l), code).iter().cloned().collect(),
            None => Vec::new(),
        }
    };
    styler::styles(&file.lines, lex)
}

/// Drops every styled file, so the benchmark can time a cold open.
pub fn forget_styles() {
    STYLED.with(|s| *s.borrow_mut() = (HashMap::new(), Vec::new()));
}

fn theme(p: &Pal, k: &Kit) -> Rc<SyntaxTheme> {
    THEMES.with(|t| t.borrow_mut()[p.dark as usize].get_or_insert_with(|| Rc::new(SyntaxTheme::from_kit(k))).clone())
}

/// The spans for line `l` of `file`: syntax colours, then changed-word backgrounds.
pub fn line_spans(p: &Pal, k: &Kit, file: &FileDiff, l: usize) -> Vec<Span> {
    let line = &file.lines[l];
    if matches!(line.kind, LineKind::Hunk | LineKind::Note) || line.text.is_empty() {
        return Vec::new();
    }
    let styled = styles(file);
    let Some(style) = styled.get(l) else { return Vec::new() };
    let mut spans = theme(p, k).spans(&style.tokens, 0);
    let mark = if line.kind == LineKind::Added { p.added_word } else { p.removed_word };
    spans.extend(style.marks.iter().map(|r| Span::new(r.start, r.end).bg(mark, 0.0)));
    spans
}

fn column(n: Option<i64>) -> String {
    match n {
        Some(n) => format!("{n:>5}"),
        None => "     ".to_string(),
    }
}

/// A diff line's row; `comment` is where its comment button's box writes, on a line that takes one.
pub fn line_view(p: &Pal, line: &DiffLine, spans: Vec<Span>, comment: Option<DraftKey>) -> El {
    let commentable = comment.is_some();
    let numbers = format!("{} {} ", column(line.old_number), column(line.new_number));
    let (sign, sign_color) = match line.kind {
        LineKind::Added => ("+", p.green),
        LineKind::Removed => ("−", p.red),
        _ => ("", p.tertiary),
    };
    let gutter_text = format!("{numbers}{sign}");
    let gutter_spans = vec![Span::new(0, numbers.len()).size(11.0).color(p.tertiary), Span::new(numbers.len(), gutter_text.len()).color(sign_color)];
    let mut gutter = el().w(96.0).child(rich(gutter_text, gutter_spans).font(mono(12.0, 400.0)).single_line());
    if let Some(key) = comment {
        gutter = gutter.child(
            el().key("comment")
                .role("button")
                .label("Comment on this line")
                .absolute(6.0, 0.0)
                .size(18.0, 18.0)
                .radius(5.0)
                .center()
                .fill(p.accent)
                .group_opacity(0.0, 1.0)
                .help("Comment on this line")
                .child(icon(&I::PLUS, 11.0).color(p.paper))
                .on_press_tx(move |s: &mut App, tx: &mut Tx| crate::ui::detail::comment_on_line(s, tx, key.clone())),
        );
    }
    let code = match line.kind {
        LineKind::Hunk | LineKind::Note => text(line.text.clone()).color(p.tertiary),
        _ if line.text.is_empty() => text(" ").color(p.text),
        _ => rich(line.text.clone(), spans).color(p.text),
    };
    let fill = match line.kind {
        LineKind::Added => Some(p.added_line),
        LineKind::Removed => Some(p.removed_line),
        LineKind::Hunk => Some(p.well),
        _ => None,
    };
    let mut r = row()
        .w_full()
        .items_start()
        .py(if line.kind == LineKind::Hunk { 5.0 } else { 1.0 })
        .pr(12.0)
        .child(gutter)
        .child(code.font(mono(12.0, 400.0).lines(1.5)).wrap_anywhere().selectable(true).grow(1.0).shrink().min_w(0.0));
    if let Some(f) = fill {
        r = r.fill(f);
    }
    // The comment button follows the pointer down the gutter line by line: it shows and hides at
    // once, as a cursor does, so sweeping across the code doesn't leave a trail of fading buttons.
    if commentable { r.hover_group().hover_motion(rune::Anim::INSTANT, rune::Anim::INSTANT) } else { r }
}

/// The spans for line `i` of a file's text (`lines`, by "commit|path" in `key`), lexed whole
/// the first time, so a string or comment that spans lines colours right.
pub fn text_spans(p: &Pal, k: &Kit, key: &str, path: &str, lines: &[String], i: usize) -> Vec<Span> {
    if lines.get(i).is_none_or(|l| l.is_empty()) {
        return Vec::new();
    }
    let styled = TEXTS.with(|t| t.borrow().get(key).cloned()).unwrap_or_else(|| {
        // As added lines, so the text is lexed once, as the new side.
        let as_diff: Vec<DiffLine> = lines.iter().map(|l| DiffLine { kind: LineKind::Added, text: l.clone(), old_number: None, new_number: None }).collect();
        let lang = syntax::lang_for_path(path);
        let lex = |code: &str| -> Vec<(std::ops::Range<usize>, Paint)> {
            match lang {
                Some(l) => syntax::highlight(Some(l), code).iter().cloned().collect(),
                None => Vec::new(),
            }
        };
        let styled: Styles = Rc::new(styler::styles(&as_diff, lex));
        TEXTS.with(|t| {
            let mut t = t.borrow_mut();
            if t.len() >= TEXT_LIMIT {
                t.clear();
            }
            t.insert(key.to_string(), styled.clone());
        });
        styled
    });
    styled.get(i).map_or_else(Vec::new, |style| theme(p, k).spans(&style.tokens, 0))
}
