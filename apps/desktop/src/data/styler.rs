//! How a diff's lines are coloured: each hunk's new side (context and added lines) and old side
//! (context and removed lines) are lexed as whole texts, so a string or comment that spans lines
//! colours right, then sliced back into lines. Paired removed and added lines also get the words
//! that changed between them marked. A port of the Swift app's `DiffStyler`, in bytes.

use std::ops::Range;

use crate::data::diff::{DiffLine, LineKind};

/// A line's coloured ranges (from the lexer, as `T`) and the byte ranges whose words changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineStyle<T> {
    pub tokens: Vec<(Range<usize>, T)>,
    pub marks: Vec<Range<usize>>,
}

/// Styles every line. `lex` colours a text (or returns nothing for an unknown language).
pub fn styles<T: Clone>(lines: &[DiffLine], lex: impl Fn(&str) -> Vec<(Range<usize>, T)>) -> Vec<LineStyle<T>> {
    let mut out: Vec<LineStyle<T>> = vec![LineStyle { tokens: Vec::new(), marks: Vec::new() }; lines.len()];
    let mut start = 0;
    for index in 0..=lines.len() {
        if index == lines.len() || lines[index].kind == LineKind::Hunk {
            if index > start {
                lex_hunk(start..index, lines, &lex, &mut out);
            }
            start = index + 1;
        }
    }
    for (index, marks) in word_marks(lines) {
        out[index].marks = marks;
    }
    out
}

fn lex_hunk<T: Clone>(range: Range<usize>, lines: &[DiffLine], lex: &impl Fn(&str) -> Vec<(Range<usize>, T)>, out: &mut [LineStyle<T>]) {
    let mut new_text = String::new();
    let mut old_text = String::new();
    // (line, on the new side, where it starts in that side's text)
    let mut starts: Vec<(usize, bool, usize)> = Vec::new();
    for index in range {
        let line = &lines[index];
        match line.kind {
            LineKind::Context => {
                starts.push((index, true, new_text.len()));
                new_text.push_str(&line.text);
                new_text.push('\n');
                old_text.push_str(&line.text);
                old_text.push('\n');
            }
            LineKind::Added => {
                starts.push((index, true, new_text.len()));
                new_text.push_str(&line.text);
                new_text.push('\n');
            }
            LineKind::Removed => {
                starts.push((index, false, old_text.len()));
                old_text.push_str(&line.text);
                old_text.push('\n');
            }
            LineKind::Hunk | LineKind::Note => {}
        }
    }
    let new_tokens = if new_text.is_empty() { Vec::new() } else { lex(&new_text) };
    let old_tokens = if old_text.is_empty() { Vec::new() } else { lex(&old_text) };
    if new_tokens.is_empty() && old_tokens.is_empty() {
        return;
    }
    let (mut cursor_new, mut cursor_old) = (0, 0);
    for (index, new, at) in starts {
        let length = lines[index].text.len();
        out[index].tokens = if new { slice(&new_tokens, at, length, &mut cursor_new) } else { slice(&old_tokens, at, length, &mut cursor_old) };
    }
}

/// The tokens over `start..start + length`, shifted to the line's own offsets. `cursor` moves
/// forward only, since lines are asked for in order.
fn slice<T: Clone>(tokens: &[(Range<usize>, T)], start: usize, length: usize, cursor: &mut usize) -> Vec<(Range<usize>, T)> {
    let end = start + length;
    while *cursor < tokens.len() && tokens[*cursor].0.end <= start {
        *cursor += 1;
    }
    let mut spans = Vec::new();
    let mut index = *cursor;
    while index < tokens.len() && tokens[index].0.start < end {
        let (range, kind) = &tokens[index];
        let from = range.start.max(start);
        let to = range.end.min(end);
        if to > from {
            spans.push((from - start..to - start, kind.clone()));
        }
        index += 1;
    }
    spans
}

/// Changed words for each removed line paired with the added line in the same place.
pub fn word_marks(lines: &[DiffLine]) -> Vec<(usize, Vec<Range<usize>>)> {
    let mut marks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind != LineKind::Removed {
            index += 1;
            continue;
        }
        let mut removed = Vec::new();
        while index < lines.len() && lines[index].kind == LineKind::Removed {
            removed.push(index);
            index += 1;
        }
        let mut added = Vec::new();
        while index < lines.len() && lines[index].kind == LineKind::Added {
            added.push(index);
            index += 1;
        }
        for (old, new) in removed.into_iter().zip(added) {
            let Some((a, b)) = changed_ranges(&lines[old].text, &lines[new].text) else { continue };
            if !a.is_empty() {
                marks.push((old, a));
            }
            if !b.is_empty() {
                marks.push((new, b));
            }
        }
    }
    marks
}

struct Word<'a> {
    text: &'a [u8],
    start: usize,
}

fn words(text: &str) -> Vec<Word<'_>> {
    fn kind(u: u8) -> u8 {
        match u {
            b' ' | b'\t' => 1,
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'_' => 2,
            // Every byte of a multi-byte character, so characters stay whole.
            128.. => 2,
            _ => 0,
        }
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let k = kind(bytes[index]);
        let mut end = index + 1;
        if k != 0 {
            while end < bytes.len() && kind(bytes[end]) == k {
                end += 1;
            }
        }
        out.push(Word { text: &bytes[index..end], start: index });
        index = end;
    }
    out
}

/// The byte ranges that differ between two versions of a line, word by word, or `None` when
/// they share too little for marking to help (then the whole line reads as changed anyway).
pub fn changed_ranges(a: &str, b: &str) -> Option<(Vec<Range<usize>>, Vec<Range<usize>>)> {
    if a == b || a.is_empty() || b.is_empty() || a.len() > 1000 || b.len() > 1000 {
        return None;
    }
    let wa = words(a);
    let wb = words(b);
    let mut head = 0;
    while head < wa.len() && head < wb.len() && wa[head].text == wb[head].text {
        head += 1;
    }
    let mut tail = 0;
    while tail < wa.len() - head && tail < wb.len() - head && wa[wa.len() - 1 - tail].text == wb[wb.len() - 1 - tail].text {
        tail += 1;
    }
    let mid_a = &wa[head..wa.len() - tail];
    let mid_b = &wb[head..wb.len() - tail];
    let mut keep_a = vec![false; mid_a.len()];
    let mut keep_b = vec![false; mid_b.len()];
    let (n, m) = (mid_a.len(), mid_b.len());
    if n > 0 && m > 0 && n * m <= 40_000 {
        // Longest common subsequence of words.
        let mut table = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                table[i * (m + 1) + j] = if mid_a[i].text == mid_b[j].text {
                    table[(i + 1) * (m + 1) + j + 1] + 1
                } else {
                    table[(i + 1) * (m + 1) + j].max(table[i * (m + 1) + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n && j < m {
            if mid_a[i].text == mid_b[j].text {
                keep_a[i] = true;
                keep_b[j] = true;
                i += 1;
                j += 1;
            } else if table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    let ranges_a = merge(mid_a, &keep_a);
    let ranges_b = merge(mid_b, &keep_b);
    let kept_a = a.len() - ranges_a.iter().map(|r| r.len()).sum::<usize>();
    let kept_b = b.len() - ranges_b.iter().map(|r| r.len()).sum::<usize>();
    if (kept_a.min(kept_b) as f64) < (a.len().min(b.len()) as f64) * 0.4 {
        return None;
    }
    Some((ranges_a, ranges_b))
}

/// Joins changed words into ranges, across a single space between them.
fn merge(words: &[Word<'_>], keep: &[bool]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if keep[index] {
            continue;
        }
        let range = word.start..word.start + word.text.len();
        if let Some(last) = ranges.last_mut() {
            let space_before = index > 0 && words[index - 1].text == b" ";
            if range.start - last.end <= 1 && (range.start == last.end || space_before) {
                last.end = range.end;
                continue;
            }
        }
        ranges.push(range);
    }
    ranges
}
