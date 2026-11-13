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

mod mine {
    use crate::data::forge::Forge;
    use crate::data::mine::{Standing, below, review_reason, stacks, status};
    use crate::data::models::*;
    use chrono::{Duration, Utc};

    fn request(iid: &str, title: &str, source: &str, target: &str) -> MergeRequest {
        let now = Utc::now();
        MergeRequest {
            id: format!("id-{iid}"),
            iid: iid.into(),
            title: title.into(),
            web_url: String::new(),
            project_path: "group/project".into(),
            project_name: "project".into(),
            author: Person::new("me", "Me Myself"),
            created_at: now,
            updated_at: now,
            merged_at: None,
            closed_at: None,
            state: "opened".into(),
            draft: false,
            source_branch: source.into(),
            target_branch: target.into(),
            merge_status: Some("not_approved".into()),
            comment_count: 0,
            description: String::new(),
            reviewers: vec![],
            approved_by: vec![],
            additions: 0,
            deletions: 0,
            file_count: 0,
            pipeline: Some(PipelineStatus::Success),
            labels: vec![],
            requested_at: now,
            first_requested_at: now,
            requested_by: None,
            requested_at_is_guess: true,
            my_state: ReviewState::Unknown,
            forge: Forge::Gitlab,
            unresolved_threads: Some(0),
            failed_job: None,
            auto_merge: false,
            merge_method: None,
        }
    }

    fn reviewer(name: &str, state: ReviewState) -> Reviewer {
        Reviewer { person: Person::new(&name.to_lowercase(), name), state, approved: state == ReviewState::Approved }
    }

    fn approved(mut m: MergeRequest) -> MergeRequest {
        m.reviewers = vec![reviewer("Maya Lind", ReviewState::Approved)];
        m.approved_by = vec!["maya".into()];
        m.merge_status = Some("mergeable".into());
        m
    }

    #[test]
    fn whose_move_it_is() {
        let plain = request("1", "One", "a", "main");
        assert_eq!(status(&plain, None).step, "No reviewers yet");

        let ready = approved(plain.clone());
        let st = status(&ready, None);
        assert_eq!((st.standing, st.step.as_str()), (Standing::Ready, "Approved by 1 · ready to merge"));

        // Mergeable, but someone asked hasn't looked: their turn, not mine.
        let mut asked = ready.clone();
        asked.reviewers.push(reviewer("Tomás Varga", ReviewState::Unreviewed));
        let st = status(&asked, None);
        assert_eq!(st.standing, Standing::Waiting);
        assert!(st.step.starts_with("Waiting on Tomás"), "{}", st.step);

        let mut quiet = asked.clone();
        quiet.updated_at = Utc::now() - Duration::days(4);
        assert_eq!(status(&quiet, None).standing, Standing::Quiet);

        let mut changes = asked.clone();
        changes.reviewers[1].state = ReviewState::RequestedChanges;
        changes.unresolved_threads = Some(2);
        let st = status(&changes, None);
        assert_eq!((st.standing, st.step.as_str()), (Standing::NeedsYou, "Tomás asked for changes · 2 threads open"));

        let mut failed = ready.clone();
        failed.pipeline = Some(PipelineStatus::Failed);
        failed.failed_job = Some("lint".into());
        assert_eq!(status(&failed, None).step, "Pipeline failed · lint");

        let mut conflict = ready.clone();
        conflict.merge_status = Some("conflict".into());
        assert_eq!(status(&conflict, None).standing, Standing::NeedsYou);

        let mut draft = failed.clone();
        draft.draft = true;
        assert_eq!(status(&draft, None).standing, Standing::Draft);
    }

    #[test]
    fn a_ready_request_on_an_unmerged_one_is_held() {
        let bottom = approved(request("1", "Paging 1: columns", "paging-1", "main"));
        let top = approved(request("2", "Paging 2: endpoint", "paging-2", "paging-1"));
        let all = [&bottom, &top];
        assert!(below(&bottom, &all).is_none());
        let st = status(&top, below(&top, &all));
        assert_eq!((st.standing, st.step.as_str()), (Standing::Held, "Approved by 1 · held until !1 merges"));
        assert!(st.standing.group() == Standing::Waiting && !st.standing.on_me());
    }

    #[test]
    fn merging_a_stack_goes_one_ready_part_at_a_time() {
        use crate::data::mine::{WalkStep, walk_step};
        let bottom = approved(request("1", "Paging 1", "paging-1", "main"));
        let mut next = approved(request("2", "Paging 2", "paging-2", "paging-1"));
        assert_eq!(walk_step(None, "main", &[]), WalkStep::Done);
        // The part below is still open (its merge is on the way): nothing to do yet.
        assert_eq!(walk_step(Some(&next), "main", &[&bottom, &next]), WalkStep::Wait);
        // It merged, and this part still points at its branch.
        assert_eq!(walk_step(Some(&next), "main", &[&next]), WalkStep::Retarget("main".into()));
        next.target_branch = "main".into();
        assert_eq!(walk_step(Some(&next), "main", &[&next]), WalkStep::Merge);
        // The pipeline runs again after the move; nobody is asked anything.
        next.merge_status = Some("ci_still_running".into());
        next.pipeline = Some(PipelineStatus::Running);
        assert_eq!(walk_step(Some(&next), "main", &[&next]), WalkStep::Wait);
        // Anything that needs a person ends it.
        next.pipeline = Some(PipelineStatus::Failed);
        assert_eq!(walk_step(Some(&next), "main", &[&next]), WalkStep::Stop("Pipeline failed".into()));
        next.pipeline = Some(PipelineStatus::Success);
        next.reviewers.push(reviewer("Sam Achebe", ReviewState::Unreviewed));
        assert!(matches!(walk_step(Some(&next), "main", &[&next]), WalkStep::Stop(why) if why.starts_with("Waiting on Sam")));
        next.reviewers.pop();
        next.draft = true;
        assert!(matches!(walk_step(Some(&next), "main", &[&next]), WalkStep::Stop(_)));
    }

    #[test]
    fn auto_merge_shows_in_the_step() {
        let mut m = request("1", "One", "a", "main");
        m.reviewers = vec![reviewer("Tomás Varga", ReviewState::Unreviewed)];
        m.auto_merge = true;
        let st = status(&m, None);
        assert_eq!(st.standing, Standing::Waiting);
        assert!(st.step.starts_with("Auto-merge on · waiting on Tomás"), "{}", st.step);
    }

    #[test]
    fn stacks_follow_branches_within_a_project() {
        let a = request("10", "Paging 1: columns", "paging-1", "main");
        let b = request("11", "Paging 2: endpoint", "paging-2", "paging-1");
        let c = request("12", "Paging 3: client", "paging-3", "paging-2");
        let alone = request("13", "Something else", "other", "main");
        let mut elsewhere = request("14", "Paging 9", "paging-9", "paging-3");
        elsewhere.project_path = "group/another".into();
        let mut merged = request("15", "Paging 0", "paging-0", "paging-3");
        merged.state = "merged".into();
        let found = stacks(&[&c, &alone, &a, &elsewhere, &b, &merged]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].ids, ["id-10", "id-11", "id-12"]);
        assert_eq!((found[0].key.as_str(), found[0].name.as_str()), ("id-10", "Paging"));
    }

    #[test]
    fn a_stack_with_nothing_in_common_takes_its_branch_name() {
        let a = request("1", "Add columns", "feature", "main");
        let b = request("2", "Use them", "feature-2", "feature");
        assert_eq!(stacks(&[&a, &b])[0].name, "feature");
    }

    #[test]
    fn two_requests_targeting_each_other_are_no_stack() {
        let a = request("1", "A", "x", "y");
        let b = request("2", "B", "y", "x");
        assert!(stacks(&[&a, &b]).is_empty());
    }

    #[test]
    fn review_rows_say_who_asked_or_what_i_said() {
        let mut m = request("1", "One", "a", "main");
        m.author = Person::new("maya", "Maya Lind");
        m.my_state = ReviewState::Unreviewed;
        assert_eq!(review_reason(&m).as_deref(), Some("Maya opened this"));
        m.requested_at_is_guess = false;
        m.requested_by = Some(Person::new("sam", "Sam Achebe"));
        assert_eq!(review_reason(&m).as_deref(), Some("Sam asked for your review"));
        m.first_requested_at = m.requested_at - Duration::hours(5);
        assert_eq!(review_reason(&m).as_deref(), Some("Sam asked again"));
        m.my_state = ReviewState::Approved;
        assert_eq!(review_reason(&m).as_deref(), Some("You approved"));
        m.state = "merged".into();
        assert_eq!(review_reason(&m), None);
    }
}

mod mine_list {
    use crate::data::forge::Forge;
    use crate::data::mine::Standing;
    use crate::data::prefs;
    use crate::data::store::{MineLine, QueueMode, Rail, Store};

    fn store() -> Store {
        prefs::set_ephemeral(Some(prefs::Prefs::default()));
        let (accounts, diffs, threads) = crate::fixtures::load();
        let mut store = Store::default();
        store.load_fixture(accounts, diffs, threads);
        store.forge = Forge::Gitlab;
        store.mode = QueueMode::Mine;
        store
    }

    /// Each line as a short word, to read the list's shape at a glance.
    fn shape(store: &Store) -> Vec<String> {
        store
            .mine_lines()
            .into_iter()
            .map(|line| match line {
                MineLine::Header(t) => format!("# {t}"),
                MineLine::Item { item, rail: None, place: None, .. } => item.iid,
                MineLine::Item { item, place: Some((at, of)), .. } => format!("{} ({at} of {of})", item.iid),
                MineLine::Item { item, rail: Some(Rail { first, last }), .. } => {
                    format!("{}{}{}", if first { "┌" } else { "│" }, item.iid, if last { "┘" } else { "" })
                }
                MineLine::StackHead { name, count, open, merging, .. } => {
                    format!("{} {name} ({count}){}", if open { "v" } else { ">" }, if merging { " merging" } else { "" })
                }
                MineLine::Folded { text, rail, .. } => format!("{}{text}", if rail.last { "└ " } else { "│ " }),
                MineLine::Whole { name, text, .. } => format!("> {name}: {text}"),
            })
            .collect()
    }

    #[test]
    fn a_stack_shows_what_is_on_me_and_folds_the_rest() {
        let mut store = store();
        assert_eq!(
            shape(&store),
            [
                "# Your move",
                "231",
                "> Ledger paging (4)",
                "┌240",
                "└ 3 more · 1 held, 1 waiting, 1 draft",
                "92",
                "90",
                "# Waiting on reviewers",
                "307",
                "> Exports rework: Stack of 2 · 1 waiting, 1 draft",
                "# Drafts",
                "233",
            ]
        );
        assert_eq!(store.count(QueueMode::Mine), 4);

        // The part that's open shows too, wherever it is in the stack.
        let third = store.mine().iter().find(|m| m.iid == "242").unwrap().id.clone();
        store.selected_id = Some(third);
        assert_eq!(shape(&store)[2..7], ["> Ledger paging (4)", "┌240", "│ 1 more · 1 held", "│242", "└ 1 more · 1 draft"]);

        // Right shows every part of the selected stack; Left folds it again.
        store.unfold_stacks(true);
        assert_eq!(shape(&store)[2..7], ["v Ledger paging (4)", "┌240", "│241", "│242", "│243┘"]);
        assert!(shape(&store).iter().any(|l| l.starts_with("> Exports rework")), "only the selected stack opens");
        store.unfold_stacks(false);
        store.selected_id = None;
        store.unfold_stacks(true);
        let all = shape(&store);
        let exports = all.iter().position(|l| l == "v Exports rework (2)").expect("with nothing selected, every stack opens");
        assert_eq!(all[exports + 1..exports + 3], ["┌61", "│62┘"]);
    }

    #[test]
    fn merging_a_stack_starts_at_a_ready_bottom_and_stops_where_someone_is_needed() {
        let mut store = store();
        let id = |store: &Store, iid: &str| store.mine().iter().find(|m| m.iid == iid).unwrap().clone();

        // Not from the middle, and not a request that isn't ready.
        store.merge_stack(&id(&store, "241"));
        store.merge_stack(&id(&store, "61"));
        assert!(!store.walking());

        store.merge_stack(&id(&store, "240"));
        let walk = store.walk_of(&id(&store, "243").id).expect("every part of the stack is in the walk");
        assert_eq!((walk.ids.len(), walk.target.as_str()), (4, "main"));
        assert!(shape(&store).contains(&"> Ledger paging (4) merging".to_string()));

        // The bottom merged (the walk asked for it); the second now sits on main, ready.
        fn gitlab(store: &mut Store) -> &mut crate::data::store::Account {
            store.accounts.get_mut(&Forge::Gitlab).unwrap()
        }
        gitlab(&mut store).mine.retain(|m| m.iid != "240");
        gitlab(&mut store).mine.iter_mut().find(|m| m.iid == "241").unwrap().target_branch = "main".into();
        store.advance_walks();
        assert!(store.walking());
        // That merged too; the third sits on main and waits on a reviewer, so it ends there.
        gitlab(&mut store).mine.retain(|m| m.iid != "241");
        gitlab(&mut store).mine.iter_mut().find(|m| m.iid == "242").unwrap().target_branch = "main".into();
        store.advance_walks();
        assert!(!store.walking());
        assert!(store.toast.as_deref().is_some_and(|t| t.starts_with("Stopped merging the stack at !242: Waiting on Samuel")), "{:?}", store.toast);
    }

    #[test]
    fn a_part_that_leaves_without_the_walk_merging_it_ends_the_walk() {
        let mut store = store();
        let bottom = store.mine().iter().find(|m| m.iid == "240").unwrap().clone();
        store.merge_stack(&bottom);
        // The second part was closed: the third still carries its commits, so nothing more merges.
        store.accounts.get_mut(&Forge::Gitlab).unwrap().mine.retain(|m| m.iid != "240" && m.iid != "241");
        store.advance_walks();
        assert!(!store.walking());
        assert_eq!(store.toast.as_deref(), Some("Stopped merging the stack: a part was closed or merged somewhere else"));
    }

    #[test]
    fn a_walk_whose_only_part_merged_is_done() {
        let mut store = store();
        let bottom = store.mine().iter().find(|m| m.iid == "240").unwrap().clone();
        store.accounts.get_mut(&Forge::Gitlab).unwrap().mine.retain(|m| !m.title.starts_with("Ledger paging") || m.iid == "240" || m.iid == "241");
        store.accounts.get_mut(&Forge::Gitlab).unwrap().mine.iter_mut().find(|m| m.iid == "241").unwrap().draft = true;
        store.merge_stack(&bottom);
        store.accounts.get_mut(&Forge::Gitlab).unwrap().mine.retain(|m| m.iid != "240");
        store.accounts.get_mut(&Forge::Gitlab).unwrap().mine.iter_mut().find(|m| m.iid == "241").unwrap().target_branch = "main".into();
        store.advance_walks();
        assert!(!store.walking());
        assert!(store.toast.as_deref().is_some_and(|t| t.starts_with("Stopped merging the stack at !241: Draft")), "{:?}", store.toast);
    }

    #[test]
    fn a_filter_makes_the_list_flat() {
        let mut store = store();
        store.mine_filter = Some(Standing::Waiting);
        assert_eq!(shape(&store), ["# Waiting", "307", "241 (2 of 4)", "61 (1 of 2)", "242 (3 of 4)"]);
        assert_eq!(store.mine_count(Standing::Waiting), 4);
        store.mine_filter = None;
        store.filter = "offset".into();
        assert_eq!(shape(&store), ["# Matches", "243 (4 of 4)"]);
    }
}

mod inbox {
    use std::collections::{BTreeMap, HashSet};

    use chrono::{Duration, Utc};

    use crate::data::forge::Forge;
    use crate::data::inbox::{CAP, Event, Kind, Level, Look, Snapshot, add, detect, notices, prune};
    use crate::data::models::*;

    fn request(iid: &str, author: &str) -> MergeRequest {
        let now = Utc::now() - Duration::hours(1);
        MergeRequest {
            id: format!("id-{iid}"),
            iid: iid.into(),
            title: format!("Request {iid}"),
            web_url: format!("https://example.com/{iid}"),
            project_path: "group/project".into(),
            project_name: "project".into(),
            author: Person::new(author, author),
            created_at: now,
            updated_at: now,
            merged_at: None,
            closed_at: None,
            state: "opened".into(),
            draft: false,
            source_branch: format!("branch-{iid}"),
            target_branch: "main".into(),
            merge_status: Some("not_approved".into()),
            comment_count: 0,
            description: String::new(),
            reviewers: vec![],
            approved_by: vec![],
            additions: 0,
            deletions: 0,
            file_count: 0,
            pipeline: Some(PipelineStatus::Success),
            labels: vec![],
            requested_at: now,
            first_requested_at: now,
            requested_by: Some(Person::new(author, "Maya Lind")),
            requested_at_is_guess: false,
            my_state: ReviewState::Unreviewed,
            forge: Forge::Gitlab,
            unresolved_threads: Some(0),
            failed_job: None,
            auto_merge: false,
            merge_method: None,
        }
    }

    /// One of mine, waiting on two reviewers.
    fn own(iid: &str) -> MergeRequest {
        let mut m = request(iid, "me");
        m.my_state = ReviewState::Unknown;
        m.requested_by = None;
        m.reviewers = ["maya", "sam"]
            .iter()
            .map(|u| Reviewer { person: Person::new(u, &format!("{u} name")), state: ReviewState::Unreviewed, approved: false })
            .collect();
        m
    }

    fn approve(m: &mut MergeRequest, who: &str) {
        let r = m.reviewers.iter_mut().find(|r| r.person.username == who).unwrap();
        r.state = ReviewState::Approved;
        r.approved = true;
        m.approved_by.push(who.into());
    }

    struct Queue {
        open: Vec<MergeRequest>,
        mine: Vec<MergeRequest>,
        ended: Vec<MergeRequest>,
        capped: bool,
        mine_failed: bool,
    }

    impl Queue {
        fn new(open: Vec<MergeRequest>, mine: Vec<MergeRequest>) -> Queue {
            Queue { open, mine, ended: vec![], capped: false, mine_failed: false }
        }
    }

    fn run(prev: &Snapshot, q: &Queue, merged_here: &HashSet<String>) -> (Snapshot, Vec<Event>) {
        let look =
            Look { forge: Forge::Gitlab, me: "me", open: &q.open, mine: (!q.mine_failed).then_some(q.mine.as_slice()), mine_capped: q.capped, ended: &q.ended };
        detect(prev, &[look], merged_here, Utc::now())
    }

    fn kinds(events: &[Event]) -> Vec<(String, Kind, String)> {
        events.iter().map(|e| (e.request.clone(), e.kind, e.title.clone())).collect()
    }

    #[test]
    fn the_first_look_finds_nothing() {
        let q = Queue::new(vec![request("1", "maya")], vec![own("2")]);
        let (snapshot, events) = run(&Snapshot::default(), &q, &HashSet::new());
        assert!(events.is_empty(), "{events:?}");
        assert!(snapshot.review_forges.contains(&Forge::Gitlab) && snapshot.mine_forges.contains(&Forge::Gitlab));
        assert_eq!((snapshot.reviews.len(), snapshot.mine.len()), (1, 1));
    }

    #[test]
    fn a_list_cut_short_says_nothing_about_what_left_it() {
        let none = HashSet::new();
        let mut q = Queue::new(vec![], vec![own("2"), own("3")]);
        let (snapshot, _) = run(&Snapshot::default(), &q, &none);
        // The forge ran out of pages: "3" may only have slipped past the end.
        q.mine.pop();
        q.capped = true;
        let (_, events) = run(&snapshot, &q, &none);
        assert!(events.is_empty(), "{events:?}");
        q.capped = false;
        assert_eq!(run(&snapshot, &q, &none).1.len(), 1);
    }

    #[test]
    fn each_change_is_found_once() {
        let none = HashSet::new();
        let mut q = Queue::new(vec![request("1", "maya")], vec![own("2"), own("3"), own("4"), own("5"), own("6")]);
        let (snapshot, _) = run(&Snapshot::default(), &q, &none);

        q.open.push(request("7", "sam"));
        approve(&mut q.mine[0], "maya");
        q.mine[1].reviewers[0].state = ReviewState::RequestedChanges;
        q.mine[2].pipeline = Some(PipelineStatus::Failed);
        q.mine[2].failed_job = Some("test:integration".into());
        q.mine[3].unresolved_threads = Some(2);
        // Both approve and nothing blocks it: ready, which says the approval too.
        approve(&mut q.mine[4], "maya");
        approve(&mut q.mine[4], "sam");
        q.mine[4].merge_status = Some("mergeable".into());
        let (snapshot, events) = run(&snapshot, &q, &none);
        assert_eq!(
            kinds(&events),
            [
                ("id-7".to_string(), Kind::ReviewAsked, "Maya Lind asked for your review".to_string()),
                ("id-2".into(), Kind::Approved, "maya name approved".into()),
                ("id-3".into(), Kind::ChangesRequested, "maya name asked for changes".into()),
                ("id-4".into(), Kind::PipelineFailed, "Pipeline failed · test:integration".into()),
                ("id-5".into(), Kind::Threads, "2 new threads".into()),
                ("id-6".into(), Kind::Ready, "Ready to merge".into()),
            ]
        );
        assert_eq!(events[0].reference, "project!7");

        // The same queue again is no news.
        let (snapshot, events) = run(&snapshot, &q, &none);
        assert!(events.is_empty(), "{events:?}");

        // One of mine leaves the list, and the Done list says how.
        let mut merged = q.mine.remove(4);
        merged.state = "merged".into();
        q.ended.push(merged);
        let gone = q.mine.remove(0);
        let (snapshot, events) = run(&snapshot, &q, &none);
        assert_eq!(kinds(&events), [("id-2".to_string(), Kind::Closed, "Merged or closed".to_string()), ("id-6".into(), Kind::Closed, "Merged".into())]);
        assert!(!snapshot.mine.contains_key(&gone.id));
    }

    #[test]
    fn a_re_request_is_news_and_a_guessed_time_is_not() {
        let none = HashSet::new();
        let mut q = Queue::new(vec![request("1", "maya")], vec![]);
        let (snapshot, _) = run(&Snapshot::default(), &q, &none);

        // Thirty seconds later is the same ask.
        q.open[0].requested_at += Duration::seconds(30);
        let (snapshot, events) = run(&snapshot, &q, &none);
        assert!(events.is_empty());

        // A failed note lookup puts the creation time in its place: no new ask.
        let mut guessed = q.open[0].clone();
        guessed.requested_at = Utc::now();
        guessed.requested_at_is_guess = true;
        let (_, events) = run(&snapshot, &Queue::new(vec![guessed], vec![]), &none);
        assert!(events.is_empty());

        q.open[0].requested_at += Duration::hours(2);
        let (snapshot, events) = run(&snapshot, &q, &none);
        assert_eq!(kinds(&events), [("id-1".to_string(), Kind::AskedAgain, "Maya Lind asked again".to_string())]);
        assert!(run(&snapshot, &q, &none).1.is_empty());

        // Reviewed, then asked again: back in the list as a re-request.
        let (snapshot, _) = run(&snapshot, &Queue::new(vec![], vec![]), &none);
        q.open[0].first_requested_at = q.open[0].requested_at - Duration::hours(5);
        q.open[0].requested_at += Duration::hours(1);
        assert_eq!(run(&snapshot, &q, &none).1[0].kind, Kind::AskedAgain);
    }

    #[test]
    fn my_own_doing_is_no_news() {
        let mut q = Queue::new(vec![request("1", "maya")], vec![own("2"), own("3")]);
        q.mine[0].reviewers.push(Reviewer { person: Person::new("me", "Me"), state: ReviewState::Unreviewed, approved: false });
        let (snapshot, _) = run(&Snapshot::default(), &q, &HashSet::new());

        // I approve the review and my own request, and merge the other from Mrow.
        q.open[0].my_state = ReviewState::Approved;
        approve(&mut q.mine[0], "me");
        q.mine.remove(1);
        let merged_here: HashSet<String> = ["id-3".to_string()].into();
        let (_, events) = run(&snapshot, &q, &merged_here);
        assert!(events.is_empty(), "{events:?}");
    }

    #[test]
    fn the_requests_notified_before_the_inbox_carry_over() {
        let mut q = Queue::new(vec![request("1", "maya"), request("2", "sam")], vec![own("3")]);
        let known = [format!("id-1@{}", q.open[0].requested_at.timestamp())].into();
        let waiting: Vec<&MergeRequest> = q.open.iter().collect();
        let snapshot = Snapshot::from_known(&known, &waiting, &[Forge::Gitlab]);
        q.open.push(request("4", "sam"));
        let (_, events) = run(&snapshot, &q, &HashSet::new());
        // Mine are seen for the first time; the reviews pick up where the notifications left off.
        assert_eq!(events.iter().map(|e| e.request.as_str()).collect::<Vec<_>>(), ["id-2", "id-4"]);
    }

    #[test]
    fn a_failed_fetch_of_mine_says_nothing_about_them() {
        let none = HashSet::new();
        let mut q = Queue::new(vec![], vec![own("2")]);
        let (snapshot, _) = run(&Snapshot::default(), &q, &none);
        q.mine.clear();
        q.mine_failed = true;
        let (snapshot, events) = run(&snapshot, &q, &none);
        assert!(events.is_empty());
        assert!(snapshot.mine.contains_key("id-2"), "kept for when they load again");
    }

    #[test]
    fn a_held_part_becomes_ready_when_the_one_below_merges() {
        let none = HashSet::new();
        let mut bottom = own("1");
        let mut top = own("2");
        top.target_branch = bottom.source_branch.clone();
        for m in [&mut bottom, &mut top] {
            approve(m, "maya");
            approve(m, "sam");
            m.merge_status = Some("mergeable".into());
        }
        let (snapshot, _) = run(&Snapshot::default(), &Queue::new(vec![], vec![bottom.clone(), top.clone()]), &none);
        assert_eq!(snapshot.mine["id-2"].standing, crate::data::mine::Standing::Held);
        top.target_branch = "main".into();
        let merged: HashSet<String> = ["id-1".to_string()].into();
        let (_, events) = run(&snapshot, &Queue::new(vec![], vec![top]), &merged);
        assert_eq!(kinds(&events), [("id-2".to_string(), Kind::Ready, "Ready to merge".to_string())]);
    }

    fn event(request: &str, kind: Kind, hours: i64) -> Event {
        let at = Utc::now() - Duration::hours(hours);
        Event {
            id: format!("{request}|{kind:?}|{}", at.timestamp()),
            request: request.into(),
            forge: Forge::Gitlab,
            kind,
            title: format!("{kind:?}"),
            subject: "A request".into(),
            reference: format!("project!{request}"),
            url: String::new(),
            at,
            done: false,
        }
    }

    #[test]
    fn the_inbox_keeps_the_newest_and_lets_old_and_gone_ones_go() {
        let mut inbox = Vec::new();
        add(&mut inbox, (0..CAP as i64 + 20).map(|i| event(&format!("{i}"), Kind::Threads, i)).collect());
        add(&mut inbox, vec![event("0", Kind::Threads, 0)]);
        assert_eq!(inbox.len(), CAP + 20, "the same event isn't added twice");
        prune(&mut inbox, Utc::now(), None);
        assert_eq!(inbox.len(), CAP);
        assert_eq!(inbox[0].request, "0", "newest first");

        let mut inbox =
            vec![event("a", Kind::Threads, 15 * 24), event("b", Kind::Threads, 3 * 24), event("c", Kind::Threads, 1), event("d", Kind::Closed, 3 * 24)];
        let live: HashSet<String> = HashSet::new();
        prune(&mut inbox, Utc::now(), Some(&live));
        let left: Vec<&str> = inbox.iter().map(|e| e.request.as_str()).collect();
        assert_eq!(left, ["c", "d"], "too old, and long gone, go; a recent one and the leaving itself stay");
    }

    #[test]
    fn several_events_on_one_request_make_one_banner() {
        let found = vec![event("1", Kind::Threads, 0), event("1", Kind::Ready, 0), event("1", Kind::Approved, 0), event("2", Kind::Approved, 0)];
        let levels = BTreeMap::new();
        let banners = notices(&found, &levels, true);
        assert_eq!(banners.len(), 1, "approvals and threads only go in the inbox by default: {banners:?}");
        assert_eq!(banners[0].title, "Ready");
        assert_eq!(banners[0].body, "project!1 · 3 updates\nA request");
        assert!(notices(&found, &levels, false).is_empty(), "the master switch");

        let levels: BTreeMap<Kind, Level> = [(Kind::Ready, Level::Inbox), (Kind::Approved, Level::Notify)].into();
        let banners = notices(&found, &levels, true);
        assert_eq!(banners.iter().map(|b| b.title.as_str()).collect::<Vec<_>>(), ["Approved", "Approved"]);
    }
}

mod inbox_store {
    use crate::data::forge::Forge;
    use crate::data::prefs;
    use crate::data::store::{QueueMode, Store};

    #[test]
    fn opening_an_event_goes_to_its_list_and_deals_with_it() {
        prefs::set_ephemeral(Some(prefs::Prefs::default()));
        let (accounts, diffs, threads) = crate::fixtures::load();
        let mut store = Store::default();
        store.load_fixture(accounts, diffs, threads);
        store.inbox = crate::fixtures::events();
        let new = store.news_count();
        assert!(new > 0);
        let pipeline = store.inbox.iter().find(|e| e.reference == "hooks!92").unwrap().id.clone();
        assert!(store.has_news("gitlab:fieldnotes/hooks!92"));
        assert!(store.open_event(&pipeline).is_none());
        assert_eq!((store.mode, store.selected_id.as_deref()), (QueueMode::Mine, Some("gitlab:fieldnotes/hooks!92")));
        assert_eq!(store.news_count(), new - 1);

        // A review in the other account brings it along.
        let review = store.inbox.iter().find(|e| e.reference == "cli#42").unwrap().id.clone();
        store.open_event(&review);
        assert_eq!((store.forge, store.mode), (Forge::Github, QueueMode::ToReview));

        // One that has left every list opens on the forge.
        let merged = store.inbox.iter().find(|e| e.reference == "ledger!229").unwrap().id.clone();
        assert!(store.open_event(&merged).is_some_and(|url| url.contains("229")));

        store.mark_all_done();
        assert_eq!(store.news_count(), 0);
    }
}
