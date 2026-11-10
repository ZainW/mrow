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
