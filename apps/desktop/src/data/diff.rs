//! Unified diff lines, parsed once per file.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    Hunk,
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    pub old_number: Option<i64>,
    pub new_number: Option<i64>,
}

impl DiffLine {
    pub fn parse(diff: &str) -> Vec<DiffLine> {
        let mut lines = Vec::new();
        let (mut old, mut new) = (0i64, 0i64);
        // Split on the newline byte and drop a trailing CR, so CRLF files split per line.
        for raw in diff.split('\n') {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let marker = line.as_bytes().first().copied();
            if line.starts_with("@@") {
                // @@ -343,8 +343,8 @@ context
                let parts: Vec<&str> = line.split(' ').filter(|p| !p.is_empty()).collect();
                if parts.len() > 2 {
                    old = number(parts[1]);
                    new = number(parts[2]);
                }
                lines.push(DiffLine { kind: LineKind::Hunk, text: line.into(), old_number: None, new_number: None });
            } else if marker == Some(b'+') {
                lines.push(DiffLine { kind: LineKind::Added, text: after_marker(line), old_number: None, new_number: Some(new) });
                new += 1;
            } else if marker == Some(b'-') {
                lines.push(DiffLine { kind: LineKind::Removed, text: after_marker(line), old_number: Some(old), new_number: None });
                old += 1;
            } else if marker == Some(b'\\') {
                lines.push(DiffLine { kind: LineKind::Note, text: line.into(), old_number: None, new_number: None });
            } else if !line.is_empty() || !lines.is_empty() {
                lines.push(DiffLine { kind: LineKind::Context, text: after_marker(line), old_number: Some(old), new_number: Some(new) });
                old += 1;
                new += 1;
            }
        }
        // The diff ends with a newline, which leaves one empty context line behind.
        if lines.last().is_some_and(|l| l.kind == LineKind::Context && l.text.is_empty()) {
            lines.pop();
        }
        lines
    }
}

fn number(part: &str) -> i64 {
    part.get(1..).and_then(|p| p.split(',').next()).and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// The line without its one-byte "+", "-" or " " marker.
fn after_marker(line: &str) -> String {
    line.char_indices().nth(1).map_or("", |(i, _)| &line[i..]).to_string()
}

// MARK: Context

/// Unchanged lines a diff leaves out of a file: above its first hunk, between two hunks, or
/// below its last. Within a gap the old and new sides differ by a constant: the changes above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gap {
    /// The diff line it sits above: a hunk's header, or `lines.len()` for the gap below the last.
    pub at: usize,
    /// The first hidden line's number on the new side, and on the old.
    pub new_start: i64,
    pub old_start: i64,
    /// How many lines it hides; none below the last hunk until the file's length is known.
    pub len: Option<i64>,
}

impl Gap {
    /// The old side's number of hidden line `new`.
    pub fn old_number(&self, new: i64) -> i64 {
        new + self.old_start - self.new_start
    }

    /// Above the first hunk: it only grows up from the hunk under it.
    pub fn is_first(&self) -> bool {
        self.new_start == 1
    }

    /// Below the last hunk of a diff of `lines`: it only grows down from the hunk over it.
    pub fn is_last(&self, lines: usize) -> bool {
        self.at == lines
    }
}

/// The gaps around a diff's hunks, given the new file's line count when it's known. Each holds
/// at least one line, except the one below the last hunk while the length is unknown.
pub fn gaps(lines: &[DiffLine], file_len: Option<usize>) -> Vec<Gap> {
    let mut gaps = Vec::new();
    // The first line after the hunks so far, on each side.
    let (mut old_next, mut new_next) = (1i64, 1i64);
    let mut any = false;
    for (at, line) in lines.iter().enumerate() {
        if line.kind != LineKind::Hunk {
            continue;
        }
        let Some((old_start, _, new_start, _)) = hunk_range(&line.text) else { continue };
        // The hunk's own lines, which the numbers shown were counted from, over its header's counts.
        let body = lines[at + 1..].iter().take_while(|l| l.kind != LineKind::Hunk);
        let (old_count, new_count) = body.fold((0, 0), |(old, new), l| match l.kind {
            LineKind::Context => (old + 1, new + 1),
            LineKind::Removed => (old + 1, new),
            LineKind::Added => (old, new + 1),
            _ => (old, new),
        });
        // A side with no lines names the line before the hunk: "+4,0" sits after line 4.
        let old_start = if old_count == 0 { old_start + 1 } else { old_start };
        let new_start = if new_count == 0 { new_start + 1 } else { new_start };
        if new_start > new_next {
            gaps.push(Gap { at, new_start: new_next, old_start: old_next, len: Some(new_start - new_next) });
        }
        old_next = old_start + old_count;
        new_next = new_start + new_count;
        any = true;
    }
    if any {
        let len = file_len.map(|n| n as i64 + 1 - new_next);
        if len.is_none_or(|n| n > 0) {
            gaps.push(Gap { at: lines.len(), new_start: new_next, old_start: old_next, len });
        }
    }
    gaps
}

/// "@@ -12,14 +12,22 @@ …" as (old start, old count, new start, new count). A side written
/// without a count has one line.
pub fn hunk_range(header: &str) -> Option<(i64, i64, i64, i64)> {
    let mut parts = header.split(' ').filter(|p| !p.is_empty());
    if parts.next() != Some("@@") {
        return None;
    }
    let side = |part: Option<&str>, sign: char| -> Option<(i64, i64)> {
        let mut numbers = part?.strip_prefix(sign)?.split(',');
        let start = numbers.next()?.parse().ok()?;
        let count = numbers.next().map_or(Some(1), |c| c.parse().ok())?;
        Some((start, count))
    };
    let (old_start, old_count) = side(parts.next(), '-')?;
    let (new_start, new_count) = side(parts.next(), '+')?;
    Some((old_start, old_count, new_start, new_count))
}

/// How many lines a step down or up shows.
pub const REVEAL_STEP: i64 = 20;

/// Which way a gap's expander grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// From the hunk above, down.
    Down,
    /// From the hunk below, up.
    Up,
    All,
}

/// How much of a gap shows: lines grown down from its top, and up from its bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub top: i64,
    pub bottom: i64,
}

impl Shown {
    pub fn reveal(self, how: Reveal) -> Shown {
        match how {
            Reveal::Down => Shown { top: self.top.saturating_add(REVEAL_STEP), ..self },
            Reveal::Up => Shown { bottom: self.bottom.saturating_add(REVEAL_STEP), ..self },
            Reveal::All => Shown { top: i64::MAX / 2, bottom: 0 },
        }
    }

    /// The new side's lines showing at the gap's top and at its bottom, and how many stay
    /// hidden between them: all of it while the gap's length is unknown.
    pub fn split(self, gap: &Gap) -> (std::ops::Range<i64>, Option<i64>, std::ops::Range<i64>) {
        let Some(len) = gap.len else { return (gap.new_start..gap.new_start, None, gap.new_start..gap.new_start) };
        let top = self.top.clamp(0, len);
        let bottom = self.bottom.clamp(0, len - top);
        let end = gap.new_start + len;
        (gap.new_start..gap.new_start + top, Some(len - top - bottom), end - bottom..end)
    }
}

/// A file's text as lines, without the empty one after its last newline.
pub fn text_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).to_string()).collect();
    if text.ends_with('\n') || text.is_empty() {
        lines.pop();
    }
    lines
}
