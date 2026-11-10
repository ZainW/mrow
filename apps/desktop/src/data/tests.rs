//! The Swift app's self-test checks for parsing and mapping, ported.

use chrono::Utc;

use super::diff::{DiffLine, LineKind};
use super::forge::Forge;
use super::gitlab::{ActivityNote, request_notes};
use super::models::*;

#[test]
fn crlf_diff_splits_into_lines() {
    let crlf = DiffLine::parse("@@ -1,2 +1,2 @@\r\n-a\r\n+b\r\n c\r\n");
    assert_eq!(crlf.iter().map(|l| l.kind).collect::<Vec<_>>(), [LineKind::Hunk, LineKind::Removed, LineKind::Added, LineKind::Context]);
    assert_eq!(crlf.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["@@ -1,2 +1,2 @@", "a", "b", "c"]);
    assert_eq!((crlf[3].old_number, crlf[3].new_number), (Some(2), Some(2)));
}

#[test]
fn marker_before_a_combining_mark() {
    let lines = DiffLine::parse("@@ -1 +1 @@\n+\u{301}x\n");
    let last = lines.last().unwrap();
    assert_eq!(last.kind, LineKind::Added);
    assert_eq!(last.text, "\u{301}x");
}

#[test]
fn request_notes_match_whole_handles() {
    let note = |body: &str| vec![ActivityNote::new(body, Utc::now())];
    assert!(request_notes(&note("requested review from @alex.smith"), "alex").is_empty(), "@alex doesn't match @alex.smith");
    assert!(!request_notes(&note("requested review from @alex."), "alex").is_empty(), "@alex. matches");
    assert!(!request_notes(&note("requested review from @bo and @alex"), "alex").is_empty(), "@alex in a list");
    assert!(request_notes(&note("requested review from @bo and removed review request for @alex"), "alex").is_empty());
}

const PULL: &str = r#"
  {"id":"PR_1","number":7,"title":"t","url":"https://github.com/acme/web/pull/7",
   "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-03T00:00:00Z","mergedAt":null,"closedAt":null,
   "state":"OPEN","isDraft":false,"headRefName":"f","baseRefName":"main","mergeStateStatus":"BEHIND",
   "totalCommentsCount":2,"body":"b","additions":1,"deletions":2,"changedFiles":1,
   "repository":{"nameWithOwner":"acme/web","name":"web"},"author":{"login":"ann","name":"Ann"},
   "reviewRequests":{"nodes":[{"requestedReviewer":{"login":"me","name":"Me"}},
                              {"requestedReviewer":{"slug":"core","name":"Core"}}]},
   "latestReviews":{"nodes":[{"state":"APPROVED","author":{"login":"me"}},
                             {"state":"CHANGES_REQUESTED","author":{"login":"bob"}}]},
   "labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},
   "commits":{"nodes":[{"commit":{"statusCheckRollup":{"state":"FAILURE"}}}]},
   "timelineItems":{"nodes":[{},
     {"createdAt":"2026-01-01T01:00:00Z","actor":{"login":"ann"},"requestedReviewer":{"login":"me"}},
     {"createdAt":"2026-01-02T01:00:00Z","actor":{"login":"ann"},"requestedReviewer":{"slug":"core"}},
     {"createdAt":"2026-01-03T01:00:00Z","actor":{"login":"ann"},"requestedReviewer":{"login":"ME"}}]}}"#;

#[test]
fn github_pull_maps_onto_the_shared_model() {
    let pull = super::github::build_from_json(PULL, "me", false).expect("pull request decodes");
    assert_eq!(pull.my_state, ReviewState::Unreviewed, "a re-request puts it back to review");
    assert_eq!(pull.short_reference(), "web#7");
    assert_eq!(pull.kind(), Forge::Github);
    assert_eq!(pull.reference(), "#7");
    let project = pull.project_url().unwrap();
    assert!(pull.web_url.starts_with(&project) && project.ends_with('/'), "project URL for relative links");
    assert_eq!(pull.requested_at.to_rfc3339(), "2026-01-03T01:00:00+00:00");
    assert!(pull.was_re_requested(), "asked at the latest request naming me");
    assert_eq!(pull.reviewers.iter().map(|r| r.person.username.as_str()).collect::<Vec<_>>(), ["me", "core", "bob"]);
    assert_eq!(pull.pipeline, Some(PipelineStatus::Failed));
    assert_eq!(pull.merge_status.as_deref(), Some("need_rebase"));
    assert_eq!(pull.labels[0].color, "#d73a4a");
    assert_eq!(pull.labels[0].text_color, "#ffffff");
    assert_eq!(super::github::text_color("fbca04"), "#1f2328", "dark text on a light label");
}

#[test]
fn digest_is_fnv1a_and_stable() {
    assert_eq!(fnv1a("+a\n"), fnv1a("+a\n"));
    assert_ne!(fnv1a("+a\n"), fnv1a("+b\n"));
    assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325_u64 as i64);
}

#[test]
fn swift_defaults_import() {
    let v = serde_json::json!({
        "hostname": "gitlab.example.com", "githubEnabled": false, "doneDays": 30, "activeForge": "github",
        "tabs": ["a", "b"], "lastSeen": {"a": 1.5}, "viewedFiles": {"a": {"x→x": -42}}, "appearance": "dark"
    });
    let p = super::prefs::from_swift(&v);
    assert_eq!(p.hostname, "gitlab.example.com");
    assert!(!p.github_enabled && p.gitlab_enabled);
    assert_eq!(p.done_days, 30);
    assert_eq!(p.active_forge, Forge::Github);
    assert_eq!(p.tabs, ["a", "b"]);
    assert_eq!(p.last_seen["a"], 1.5);
    assert_eq!(p.viewed_files["a"]["x→x"], -42);
    assert_eq!(p.appearance, super::prefs::Appearance::Dark);
}

#[test]
fn filter_matches_every_word() {
    let pull = super::github::build_from_json(PULL, "me", false).unwrap();
    assert!(super::store::matches(&pull, "web ann"));
    assert!(super::store::matches(&pull, "bug"));
    assert!(!super::store::matches(&pull, "web zed"));
}

/// A toy lexer: `let` is a keyword and `/*` opens a comment that runs to the end.
fn toy_lex(code: &str) -> Vec<(std::ops::Range<usize>, &'static str)> {
    let comment = code.find("/*");
    let mut out: Vec<_> = code.match_indices("let").filter(|(i, _)| comment.is_none_or(|c| *i < c)).map(|(i, _)| (i..i + 3, "keyword")).collect();
    if let Some(c) = comment {
        out.push((c..code.len(), "comment"));
    }
    out
}

#[test]
fn hunks_are_lexed_apart() {
    let lines = DiffLine::parse("@@ -1,2 +1,2 @@\n let a = 1\n+/* start\n@@ -10,1 +10,1 @@\n+let b = 2\n");
    let styled = super::styler::styles(&lines, toy_lex);
    assert_eq!(styled[4].tokens.first().map(|t| t.1), Some("keyword"));
    assert_eq!(styled[4].tokens.first().map(|t| t.0.clone()), Some(0..3));
}

#[test]
fn one_changed_word_on_each_side() {
    let marks = super::styler::changed_ranges("let total = price * count;", "let total = price * quantity;").unwrap();
    assert_eq!((marks.0.len(), marks.1.len()), (1, 1));
    assert_eq!(&"let total = price * count;"[marks.0[0].clone()], "count");
    assert_eq!(&"let total = price * quantity;"[marks.1[0].clone()], "quantity");
    assert!(super::styler::changed_ranges("abc", "xyz completely different").is_none());
}

#[test]
fn emoji_survive() {
    let lines = DiffLine::parse("@@ -1 +1 @@\n+😀 \"é\"\n");
    assert_eq!(lines[1].text, "😀 \"é\"");
    let styled = super::styler::styles(&lines, |code: &str| vec![(0..code.len(), ())]);
    for (range, _) in &styled[1].tokens {
        assert!(lines[1].text.is_char_boundary(range.start) && lines[1].text.is_char_boundary(range.end));
    }
    let marks = super::styler::changed_ranges("😀 é a", "😀 é b").unwrap();
    for r in marks.0 {
        assert!("😀 é a".is_char_boundary(r.start) && "😀 é a".is_char_boundary(r.end));
    }
}

#[test]
fn details_sections() {
    use super::markup::{Section, parse};
    let nested = parse(
        "Intro\n\n<details>\n<summary>🟡 Other (1)</summary><blockquote>\n\n<details>\n<summary>inner</summary>\n\nbody\n\n</details>\n\n</blockquote></details>\n\nAfter",
    );
    assert_eq!(nested.len(), 3, "{nested:?}");
    let Section::Details { summary, open, body } = &nested[1] else { panic!("{nested:?}") };
    assert_eq!(summary, "🟡 Other (1)");
    assert!(!open);
    let Some(Section::Details { summary: inner, .. }) = parse(body).into_iter().next() else { panic!("{body:?}") };
    assert_eq!(inner, "inner");

    assert_eq!(parse("<details open><summary>x</summary>y</details>"), vec![Section::Details { summary: "x".into(), open: true, body: "y".into() }]);
}

#[test]
fn html_becomes_markdown() {
    use super::markup::{Section, clean, parse};
    let fence = "```html\n<b>bold</b>\n```";
    assert_eq!(parse(fence), vec![Section::Markdown(fence.into())]);
    let Section::Markdown(list) = &parse("<ul><li>one <code>x</code></li><li>two</li></ul>")[0] else { panic!() };
    let items: Vec<&str> = list.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    assert_eq!(items, ["- one `x`", "- two"]);
    assert_eq!(
        clean(r#"<a href="https://c.dev/x"><img alt="Open" width="114" height="28" src="https://c.dev/o.png"></a>"#),
        "[![Open](https://c.dev/o.png){width=114 height=28}](https://c.dev/x)"
    );
}

#[test]
fn image_sizes_are_read() {
    let (text, sizes) = super::markup::strip_sizes("Before:\n![image.png](/uploads/abc/image.png){width=900 height=117} after ![ok](https://x.dev/b.svg) done");
    assert!(!text.contains("{width"), "{text}");
    assert!(sizes.iter().any(|(src, w, h)| src == "/uploads/abc/image.png" && *w == Some(900.0) && *h == Some(117.0)), "{sizes:?}");
    let (_, percent) = super::markup::strip_sizes(r#"![wide](/uploads/s/w.png){width="100%"}"#);
    assert!(percent.iter().all(|(_, w, _)| w.is_none()), "{percent:?}");
}

#[test]
fn image_sources_resolve() {
    use super::images::ImageSource;
    let base = Some("https://gitlab.com/group/sub/project");
    let upload = |p: &str, s: &str, f: &str| Some(ImageSource::Upload { project: p.into(), secret: s.into(), file: f.into() });
    assert_eq!(ImageSource::resolve("/uploads/s3cr3t/my%20shot.png", base), upload("group/sub/project", "s3cr3t", "my shot.png"));
    assert_eq!(ImageSource::resolve("https://gitlab.com/group/sub/project/-/uploads/s3cr3t/a.png", base), upload("group/sub/project", "s3cr3t", "a.png"));
    assert_eq!(ImageSource::resolve("/-/project/42/uploads/s3cr3t/a.png", base), upload("42", "s3cr3t", "a.png"));
    assert_eq!(ImageSource::resolve("https://example.com/a.png", base), Some(ImageSource::Web("https://example.com/a.png".into())));
}
