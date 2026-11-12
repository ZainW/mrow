//! Comments the agent drafts for the merge request. The agent has no way to post: it writes each
//! comment in a fence, and the chat shows it as a card the user edits and posts, or leaves.
//!
//! ````text
//! ```comment
//! A comment on the whole merge request.
//! ```
//!
//! ```comment src/export/job.rs:42
//! A comment on line 42 of the new version of that file.
//! ```
//! ````
//!
//! A comment holding a code block (a GitHub `suggestion`) uses a longer fence around it.

/// The instructions that go with the first question.
pub const INSTRUCTIONS: &str = "## Drafting review comments\n\n\
You can't post anything. When I ask you to draft review comments, or when one would help, write each comment in its own fenced block with the info string `comment`, and I'll decide whether to post it:\n\n\
````\n```comment\nA comment on the whole merge request.\n```\n\n```comment src/export/job.rs:42\nA comment on line 42 of the new version of src/export/job.rs.\n```\n````\n\n\
Write the path from the repository root, as the diff names it. Use a line number from the new version of the file, or from the old version for a removed line, and pick a line that's in the diff. If a comment holds a code block, fence the comment with four backticks. Write the comment exactly as it should be posted, with nothing else in the block.\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub path: Option<String>,
    pub line: Option<i64>,
    pub body: String,
    /// Still streaming: its fence hasn't closed.
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Comment(Proposal),
}

/// The opening fence of a comment: its fence and what follows `comment`.
fn opening(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    let mark = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let count = trimmed.chars().take_while(|c| *c == mark).count();
    if count < 3 {
        return None;
    }
    let info = trimmed[count..].trim();
    let rest = info.strip_prefix("comment")?;
    if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    Some((mark.to_string().repeat(count), rest.trim()))
}

/// "src/x.rs:42" → (path, line). A bare path, or a bad line, is a comment on the whole thing.
fn target(info: &str) -> (Option<String>, Option<i64>) {
    let info = info.trim().trim_matches('`');
    if info.is_empty() {
        return (None, None);
    }
    match info.rsplit_once(':') {
        Some((path, line)) => match line.trim().parse::<i64>() {
            Ok(n) if n > 0 && !path.is_empty() => (Some(path.to_string()), Some(n)),
            _ => (None, None),
        },
        None => (None, None),
    }
}

/// An answer split into its text and the comments drafted in it, in order.
pub fn split(text: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut lines = text.split('\n');
    while let Some(line) = lines.next() {
        let Some((fence, info)) = opening(line) else {
            prose.push(line);
            continue;
        };
        if !prose.is_empty() {
            out.push(Segment::Text(prose.join("\n")));
            prose.clear();
        }
        let (path, number) = target(info);
        let mut body: Vec<&str> = Vec::new();
        let mut open = true;
        for inner in lines.by_ref() {
            let t = inner.trim();
            if t.len() >= fence.len() && t.chars().all(|c| Some(c) == fence.chars().next()) {
                open = false;
                break;
            }
            body.push(inner);
        }
        out.push(Segment::Comment(Proposal { path, line: number, body: body.join("\n").trim().to_string(), open }));
    }
    if !prose.is_empty() {
        out.push(Segment::Text(prose.join("\n")));
    }
    out
}

impl Proposal {
    /// Names this comment across launches: the merge request, its place and its text as drafted.
    /// A reopened chat gives its entries new ids, so this is how a posted comment stays posted.
    pub fn key(&self, item: &str) -> String {
        let line = self.line.map(|l| l.to_string()).unwrap_or_default();
        let place = self.path.as_deref().unwrap_or("");
        format!("{item}|{:016x}", crate::data::models::fnv1a(&format!("{place}:{line}\n{}", self.body)))
    }
}

pub fn has_comments(text: &str) -> bool {
    text.split('\n').any(|l| opening(l).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_split_out_of_answers() {
        let text = "Two things.\n\n```comment\nGeneral note.\n```\n\nAnd:\n\n````comment src/a.rs:12\nUse this:\n```suggestion\nlet x = 1;\n```\n````\nDone.";
        let parts = split(text);
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[1], Segment::Comment(Proposal { path: None, line: None, body: "General note.".into(), open: false }));
        match &parts[3] {
            Segment::Comment(p) => {
                assert_eq!((p.path.as_deref(), p.line, p.open), (Some("src/a.rs"), Some(12), false));
                assert!(p.body.ends_with("let x = 1;\n```"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(parts[4], Segment::Text("Done.".into()));
    }

    #[test]
    fn a_comment_keeps_its_key_when_the_chat_reopens() {
        let draft = |text: &str| match split(text).remove(0) {
            Segment::Comment(p) => p,
            other => panic!("{other:?}"),
        };
        let first = draft("```comment a.rs:3\nCheck this.\n```");
        // Replayed after a relaunch: the same text, under a new entry id.
        assert_eq!(first.key("gid://mr/1"), draft("```comment a.rs:3\nCheck this.\n```").key("gid://mr/1"));
        assert_ne!(first.key("gid://mr/1"), draft("```comment a.rs:4\nCheck this.\n```").key("gid://mr/1"));
        assert_ne!(first.key("gid://mr/1"), first.key("gid://mr/2"));
    }

    #[test]
    fn streaming_comments_stay_open_and_others_are_code() {
        let parts = split("```comment x.rs:3\nhalf a tho");
        assert_eq!(parts, [Segment::Comment(Proposal { path: Some("x.rs".into()), line: Some(3), body: "half a tho".into(), open: true })]);
        assert!(!has_comments("```commentary\nno\n```"));
        assert!(!has_comments("```rust\nlet comment = 1;\n```"));
        assert_eq!(target("x.rs:abc"), (None, None));
    }
}
