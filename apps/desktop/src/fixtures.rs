//! A made-up queue for snapshots and tests: two accounts with a handful of reviews, diffs and
//! threads. Everything here is fictional.

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, Utc};

use crate::data::forge::Forge;
use crate::data::inbox::{Event, Kind};
use crate::data::models::*;
use crate::data::store::Account;

fn ago(hours: i64) -> Date {
    Utc::now() - Duration::hours(hours)
}

fn person(username: &str, name: &str) -> Person {
    let mut p = Person::new(username, name);
    // A generic identicon, fetched only when a headless run allows it (MROW_ONLINE_IMAGES).
    if username == "maya.lind" {
        p.avatar_url = Some("https://www.gravatar.com/avatar/6f1c2a?d=identicon&s=64".into());
    }
    p
}

struct Spec {
    forge: Forge,
    iid: &'static str,
    title: &'static str,
    project: &'static str,
    author: (&'static str, &'static str),
    hours: i64,
    state: &'static str,
    draft: bool,
    my_state: ReviewState,
    pipeline: Option<PipelineStatus>,
    branch: &'static str,
    counts: (i64, i64, i64),
}

fn mr(s: Spec, description: &str) -> MergeRequest {
    let author = person(s.author.0, s.author.1);
    let host = match s.forge {
        Forge::Gitlab => "https://gitlab.example.com",
        Forge::Github => "https://github.com",
    };
    let path = if s.forge == Forge::Gitlab { "-/merge_requests" } else { "pull" };
    let name = s.project.rsplit('/').next().unwrap_or(s.project);
    MergeRequest {
        id: format!("{}:{}!{}", s.forge.raw(), s.project, s.iid),
        iid: s.iid.into(),
        title: s.title.into(),
        web_url: format!("{host}/{}/{path}/{}", s.project, s.iid),
        project_path: s.project.into(),
        project_name: name.into(),
        author: author.clone(),
        created_at: ago(s.hours + 20),
        updated_at: ago(s.hours.saturating_sub(1).max(0)),
        merged_at: (s.state == "merged").then(|| ago(s.hours)),
        closed_at: (s.state == "closed").then(|| ago(s.hours)),
        state: s.state.into(),
        draft: s.draft,
        source_branch: s.branch.into(),
        target_branch: "main".into(),
        merge_status: Some(if s.state == "opened" { "can_be_merged".into() } else { "unchecked".into() }),
        comment_count: 3,
        description: description.into(),
        reviewers: vec![
            Reviewer { person: person("robin", "Robin Okafor"), state: s.my_state, approved: s.my_state == ReviewState::Approved },
            Reviewer { person: person("tomas.v", "Tomás Varga"), state: ReviewState::Approved, approved: true },
        ],
        approved_by: if s.my_state == ReviewState::Approved { vec!["robin".into(), "tomas.v".into()] } else { vec!["tomas.v".into()] },
        additions: s.counts.0,
        deletions: s.counts.1,
        file_count: s.counts.2,
        pipeline: s.pipeline,
        labels: if s.iid == "214" {
            vec![
                Label { title: "backend".into(), color: "#3C6A9E".into(), text_color: "#FFFFFF".into() },
                Label { title: "needs-qa".into(), color: "#E9D8A6".into(), text_color: "#1E1C19".into() },
            ]
        } else {
            vec![]
        },
        requested_at: ago(s.hours),
        first_requested_at: ago(s.hours + if s.iid == "88" { 72 } else { 0 }),
        requested_by: Some(author),
        requested_at_is_guess: false,
        my_state: s.my_state,
        forge: s.forge,
        unresolved_threads: None,
        failed_job: None,
        auto_merge: false,
        merge_method: None,
    }
}

const DESCRIPTION: &str = r#"Ledger exports used to stream every row through one writer, so a 2 GB export held the lock for minutes and the dashboard timed out behind it.

This moves the export onto its own **read replica** connection and writes CSV in batches of `5_000` rows. The API answers with a job id at once; the file lands in the bucket when it's ready.

## What changed

- `ExportJob` owns the cursor and the batch size
- The writer flushes after each batch instead of each row
- Failed jobs keep their partial file for an hour, then go

```rust
let job = ExportJob::new(ledger, Batch::rows(5_000));
queue.push(job).await?;
```

| Rows | Before | After |
| ---: | -----: | ----: |
| 10k | 1.2 s | 0.3 s |
| 1M | 94 s | 11 s |

<details>
<summary>How I tested it</summary>

Ran the export against a copy of the staging ledger (about 1.4M rows) with the dashboard open in another tab. It stayed responsive the whole time.

</details>

> Follow-up: the import side has the same single-writer problem.
"#;

const DIFF_RS: &str = "@@ -12,14 +12,22 @@ use crate::ledger::Ledger;
 /// Writes a ledger out as CSV.
 pub struct ExportJob {
     ledger: LedgerId,
-    cursor: Option<RowId>,
+    cursor: Cursor,
+    batch: Batch,
 }

 impl ExportJob {
-    pub fn new(ledger: LedgerId) -> Self {
-        ExportJob { ledger, cursor: None }
+    pub fn new(ledger: LedgerId, batch: Batch) -> Self {
+        ExportJob { ledger, cursor: Cursor::start(), batch }
     }

-    pub async fn run(&mut self, out: &mut Writer) -> Result<()> {
-        while let Some(row) = self.next_row().await? {
-            out.write_row(&row)?;
+    /// Streams the rows in batches, flushing after each one, so a slow
+    /// reader never holds the writer for long.
+    pub async fn run(&mut self, out: &mut Writer) -> Result<usize> {
+        let mut written = 0;
+        while let Some(rows) = self.next_batch().await? {
+            out.write_rows(&rows)?;
+            out.flush()?;
+            written += rows.len();
         }
-        Ok(())
+        Ok(written)
     }
 }
";

const DIFF_TS: &str = "@@ -1,9 +1,12 @@
-import { startExport } from \"./api\";
+import { startExport, exportStatus } from \"./api\";

 export async function exportLedger(id: string) {
-  const file = await startExport(id);
-  return file.url;
+  const job = await startExport(id);
+  // The export runs in the background now; poll until the file is there.
+  const done = await exportStatus(job.id, { every: 2_000 });
+  return done.url;
 }
";

const DIFF_MD: &str = "@@ -0,0 +1,6 @@
+# Exports
+
+Exports run in the background on a read replica. The API returns a job id;
+poll `GET /exports/:id` until `state` is `done`.
+
+Partial files from failed jobs are kept for an hour.
";

/// One of Robin's own merge requests.
struct Own {
    iid: &'static str,
    title: &'static str,
    project: &'static str,
    /// Source and target branch; a target that is another's source stacks it on that one.
    branches: (&'static str, &'static str),
    /// Minutes since anything happened on it.
    minutes: i64,
    draft: bool,
    merge: &'static str,
    pipeline: PipelineStatus,
    reviewers: &'static [(&'static str, &'static str, ReviewState)],
    threads: i64,
    failed: Option<&'static str>,
}

/// Robin's open merge requests: two ready, two that need work, some waiting, a draft, and two
/// stacks (one with its bottom ready, one with nothing to do).
fn mine() -> Vec<MergeRequest> {
    use PipelineStatus::{Failed, Running, Success};
    use ReviewState::{Approved, RequestedChanges, Unreviewed};
    const MAYA: (&str, &str) = ("maya.lind", "Maya Lindqvist");
    const SAM: (&str, &str) = ("sam", "Samuel Achterberg");
    const DEV: (&str, &str) = ("dev.k", "Devika Rao");
    const TOMAS: (&str, &str) = ("tomas.v", "Tomás Varga");
    let both = &[(MAYA.0, MAYA.1, Approved), (SAM.0, SAM.1, Approved)];
    let own = [
        Own {
            iid: "231",
            title: "Drop the legacy export endpoint",
            project: "fieldnotes/ledger",
            branches: ("robin/drop-legacy-export", "main"),
            minutes: 12,
            draft: false,
            merge: "mergeable",
            pipeline: Success,
            reviewers: &[(TOMAS.0, TOMAS.1, Approved), (SAM.0, SAM.1, Approved)],
            threads: 0,
            failed: None,
        },
        Own {
            iid: "240",
            title: "Ledger paging 1: add cursor columns",
            project: "fieldnotes/ledger",
            branches: ("robin/paging-1", "main"),
            minutes: 25,
            draft: false,
            merge: "mergeable",
            pipeline: Success,
            reviewers: both,
            threads: 0,
            failed: None,
        },
        Own {
            iid: "241",
            title: "Ledger paging 2: paginate the endpoint",
            project: "fieldnotes/ledger",
            branches: ("robin/paging-2", "robin/paging-1"),
            minutes: 190,
            draft: false,
            merge: "mergeable",
            pipeline: Success,
            reviewers: both,
            threads: 0,
            failed: None,
        },
        Own {
            iid: "242",
            title: "Ledger paging 3: switch the client to cursors",
            project: "fieldnotes/ledger",
            branches: ("robin/paging-3", "robin/paging-2"),
            minutes: 26 * 60,
            draft: false,
            merge: "not_approved",
            pipeline: Success,
            reviewers: &[(SAM.0, SAM.1, Unreviewed)],
            threads: 0,
            failed: None,
        },
        Own {
            iid: "243",
            title: "Ledger paging 4: remove offset paging",
            project: "fieldnotes/ledger",
            branches: ("robin/paging-4", "robin/paging-3"),
            minutes: 27 * 60,
            draft: true,
            merge: "draft_status",
            pipeline: Success,
            reviewers: &[],
            threads: 0,
            failed: None,
        },
        Own {
            iid: "92",
            title: "Cache tenant settings per request",
            project: "fieldnotes/hooks",
            branches: ("robin/tenant-cache", "main"),
            minutes: 40,
            draft: false,
            merge: "ci_must_pass",
            pipeline: Failed,
            reviewers: &[(DEV.0, DEV.1, Unreviewed)],
            threads: 0,
            failed: Some("test:integration"),
        },
        Own {
            iid: "90",
            title: "Verify webhook signatures before queueing",
            project: "fieldnotes/hooks",
            branches: ("robin/verify-signatures", "main"),
            minutes: 130,
            draft: false,
            merge: "discussions_not_resolved",
            pipeline: Success,
            reviewers: &[(MAYA.0, MAYA.1, RequestedChanges), (TOMAS.0, TOMAS.1, Approved)],
            threads: 2,
            failed: None,
        },
        Own {
            iid: "307",
            title: "Stream audit log exports",
            project: "fieldnotes/billing",
            branches: ("robin/audit-stream", "main"),
            minutes: 3 * 24 * 60 + 90,
            draft: false,
            merge: "not_approved",
            pipeline: Success,
            reviewers: &[(TOMAS.0, TOMAS.1, Unreviewed)],
            threads: 0,
            failed: None,
        },
        Own {
            iid: "61",
            title: "Exports rework 1: move the job to the worker",
            project: "fieldnotes/desktop",
            branches: ("robin/exports-1", "main"),
            minutes: 5 * 60,
            draft: false,
            merge: "not_approved",
            pipeline: Running,
            reviewers: &[(DEV.0, DEV.1, Unreviewed)],
            threads: 0,
            failed: None,
        },
        Own {
            iid: "62",
            title: "Exports rework 2: drop the synchronous path",
            project: "fieldnotes/desktop",
            branches: ("robin/exports-2", "robin/exports-1"),
            minutes: 5 * 60 + 10,
            draft: true,
            merge: "draft_status",
            pipeline: Success,
            reviewers: &[],
            threads: 0,
            failed: None,
        },
        Own {
            iid: "233",
            title: "Spike: move sessions to Redis",
            project: "fieldnotes/ledger",
            branches: ("robin/redis-sessions", "main"),
            minutes: 28 * 60,
            draft: true,
            merge: "draft_status",
            pipeline: Success,
            reviewers: &[],
            threads: 0,
            failed: None,
        },
    ];
    own.into_iter()
        .map(|o| {
            let spec = Spec {
                forge: Forge::Gitlab,
                iid: o.iid,
                title: o.title,
                project: o.project,
                author: ("robin", "Robin Okafor"),
                hours: o.minutes / 60,
                state: "opened",
                draft: o.draft,
                my_state: ReviewState::Unknown,
                pipeline: Some(o.pipeline),
                branch: o.branches.0,
                counts: (48, 12, 4),
            };
            let mut m = mr(spec, "Cursor paging for the ledger, in parts small enough to review in one sitting.");
            m.target_branch = o.branches.1.into();
            m.updated_at = Utc::now() - Duration::minutes(o.minutes);
            m.merge_status = Some(o.merge.into());
            m.reviewers =
                o.reviewers.iter().map(|(u, n, state)| Reviewer { person: person(u, n), state: *state, approved: *state == ReviewState::Approved }).collect();
            m.approved_by = m.reviewers.iter().filter(|r| r.approved).map(|r| r.person.username.clone()).collect();
            m.labels.clear();
            m.requested_by = None;
            m.requested_at_is_guess = true;
            m.unresolved_threads = Some(o.threads);
            m.failed_job = o.failed.map(String::from);
            m
        })
        .collect()
}

fn file(old: &str, new: &str, diff: &str, new_file: bool) -> FileDiff {
    FileDiff {
        old_path: old.into(),
        new_path: new.into(),
        diff: diff.into(),
        new_file,
        renamed_file: false,
        deleted_file: false,
        too_large: false,
        collapsed: false,
        generated_file: false,
        lines: vec![],
        added: 0,
        removed: 0,
        digest: 0,
    }
    .prepared()
}

fn note(id: i64, who: (&str, &str), body: &str, hours: i64, position: Option<Position>) -> Note {
    Note { id, body: body.into(), author: person(who.0, who.1), created_at: ago(hours), system: false, resolvable: Some(true), resolved: Some(false), position }
}

pub fn load() -> (BTreeMap<Forge, Account>, HashMap<String, Vec<FileDiff>>, HashMap<String, Vec<Discussion>>) {
    let maya = ("maya.lind", "Maya Lindqvist");
    let dev = ("dev.k", "Devika Rao");
    let sam = ("sam", "Samuel Achterberg");
    let gitlab_open = vec![
        mr(
            Spec {
                forge: Forge::Gitlab,
                iid: "214",
                title: "Stream ledger exports in batches on a read replica",
                project: "fieldnotes/ledger",
                author: maya,
                hours: 3,
                state: "opened",
                draft: false,
                my_state: ReviewState::Unreviewed,
                pipeline: Some(PipelineStatus::Success),
                branch: "maya/batched-exports",
                counts: (31, 9, 3),
            },
            DESCRIPTION,
        ),
        mr(
            Spec {
                forge: Forge::Gitlab,
                iid: "88",
                title: "Retry webhook deliveries with jittered backoff",
                project: "fieldnotes/hooks",
                author: dev,
                hours: 30,
                state: "opened",
                draft: false,
                my_state: ReviewState::Unreviewed,
                pipeline: Some(PipelineStatus::Running),
                branch: "dev/webhook-backoff",
                counts: (64, 12, 4),
            },
            "Deliveries that fail now retry up to five times, waiting 2ⁿ seconds plus up to a second of jitter.",
        ),
        mr(
            Spec {
                forge: Forge::Gitlab,
                iid: "301",
                title: "Draft: move invoice PDFs to the new renderer",
                project: "fieldnotes/billing",
                author: sam,
                hours: 80,
                state: "opened",
                draft: true,
                my_state: ReviewState::Unreviewed,
                pipeline: Some(PipelineStatus::Failed),
                branch: "sam/pdf-renderer",
                counts: (412, 380, 22),
            },
            "",
        ),
        mr(
            Spec {
                forge: Forge::Gitlab,
                iid: "190",
                title: "Show the sync status in the menu bar",
                project: "fieldnotes/desktop",
                author: dev,
                hours: 120,
                state: "opened",
                draft: false,
                my_state: ReviewState::Approved,
                pipeline: Some(PipelineStatus::Success),
                branch: "dev/menu-status",
                counts: (48, 3, 2),
            },
            "A small status item: a dot while syncing, a warning when the last sync failed.",
        ),
        mr(
            Spec {
                forge: Forge::Gitlab,
                iid: "176",
                title: "Fix the timezone of recurring entries around DST",
                project: "fieldnotes/ledger",
                author: maya,
                hours: 400,
                state: "opened",
                draft: false,
                my_state: ReviewState::RequestedChanges,
                pipeline: Some(PipelineStatus::Success),
                branch: "maya/dst-recurring",
                counts: (22, 18, 2),
            },
            "Recurring entries created in March landed an hour early after the clocks changed.",
        ),
    ];
    let gitlab_done = vec![mr(
        Spec {
            forge: Forge::Gitlab,
            iid: "169",
            title: "Cache account lookups per request",
            project: "fieldnotes/ledger",
            author: sam,
            hours: 50,
            state: "merged",
            draft: false,
            my_state: ReviewState::Approved,
            pipeline: Some(PipelineStatus::Success),
            branch: "sam/account-cache",
            counts: (40, 11, 3),
        },
        "",
    )];
    let github_open = vec![mr(
        Spec {
            forge: Forge::Github,
            iid: "42",
            title: "Add a --dry-run flag to the import command",
            project: "fieldnotes/cli",
            author: ("jo-w", "Jo Whitfield"),
            hours: 6,
            state: "opened",
            draft: false,
            my_state: ReviewState::Unreviewed,
            pipeline: Some(PipelineStatus::Pending),
            branch: "dry-run",
            counts: (57, 4, 3),
        },
        "Prints what an import would change without writing anything.",
    )];
    let me = person("robin", "Robin Okafor");
    let mut accounts = BTreeMap::new();
    let now = Some(Utc::now());
    accounts.insert(
        Forge::Gitlab,
        Account { me: Some(me.clone()), open: gitlab_open.clone(), done: gitlab_done, mine: mine(), mine_error: None, error: None, last_refreshed: now },
    );
    accounts.insert(Forge::Github, Account { me: Some(me), open: github_open, done: vec![], mine: vec![], mine_error: None, error: None, last_refreshed: now });

    let first = &gitlab_open[0];
    let mut diffs = HashMap::new();
    diffs.insert(
        first.id.clone(),
        vec![
            file("src/export/job.rs", "src/export/job.rs", DIFF_RS, false),
            file("web/src/exports.ts", "web/src/exports.ts", DIFF_TS, false),
            file("/dev/null", "docs/exports.md", DIFF_MD, true),
        ],
    );
    let on_line =
        Position { new_path: Some("src/export/job.rs".into()), old_path: Some("src/export/job.rs".into()), new_line: Some(28), old_line: None, head_sha: None };
    let mut threads = HashMap::new();
    threads.insert(
        first.id.clone(),
        vec![
            Discussion {
                id: "t1".into(),
                individual_note: false,
                notes: vec![
                    note(1, ("tomas.v", "Tomás Varga"), "Should the flush be optional? On a local disk this is a lot of `fsync`s.", 2, Some(on_line.clone())),
                    note(2, maya, "It's a buffered writer, so `flush` hands the batch to the OS; it doesn't sync. I'll add a comment.", 1, Some(on_line)),
                ],
            },
            Discussion {
                id: "t2".into(),
                individual_note: true,
                notes: vec![note(
                    3,
                    dev,
                    "Looks good to me. The numbers in the table match what I saw on staging.\n\nOne thing: can the job id go in the response header too?",
                    2,
                    None,
                )],
            },
            // On a file this change doesn't touch, so it has no line to sit under.
            Discussion {
                id: "t3".into(),
                individual_note: false,
                notes: vec![Note {
                    resolved: Some(true),
                    ..note(
                        4,
                        ("tomas.v", "Tomás Varga"),
                        "`Batch::rows` could take a `NonZeroUsize`, so a zero batch can't loop forever.",
                        26,
                        Some(Position {
                            new_path: Some("src/export/batch.rs".into()),
                            old_path: Some("src/export/batch.rs".into()),
                            new_line: Some(14),
                            old_line: None,
                            head_sha: None,
                        }),
                    )
                }],
            },
        ],
    );
    (accounts, diffs, threads)
}

/// The inbox: four new events (on Robin's own requests and a review asked of them) and some
/// already dealt with, one about a request that has since merged.
pub fn events() -> Vec<Event> {
    let event = |forge: Forge, project: &str, iid: &str, kind: Kind, title: &str, subject: &str, minutes: i64, done: bool| {
        let request = format!("{}:{project}!{iid}", forge.raw());
        let at = Utc::now() - Duration::minutes(minutes);
        let name = project.rsplit('/').next().unwrap_or(project);
        let host = if forge == Forge::Gitlab { "https://gitlab.example.com" } else { "https://github.com" };
        Event {
            id: format!("{request}|{kind:?}|{}", at.timestamp()),
            request,
            forge,
            kind,
            title: title.into(),
            subject: subject.into(),
            reference: format!("{name}{}{iid}", forge.reference_mark()),
            url: format!("{host}/{project}/-/merge_requests/{iid}"),
            at,
            done,
        }
    };
    use Forge::{Github, Gitlab};
    vec![
        event(Gitlab, "fieldnotes/ledger", "231", Kind::Ready, "Ready to merge", "Drop the legacy export endpoint", 12, false),
        event(Gitlab, "fieldnotes/hooks", "92", Kind::PipelineFailed, "Pipeline failed · test:integration", "Cache tenant settings per request", 40, false),
        event(
            Gitlab,
            "fieldnotes/hooks",
            "90",
            Kind::ChangesRequested,
            "Maya Lindqvist asked for changes",
            "Verify webhook signatures before queueing",
            130,
            false,
        ),
        event(
            Gitlab,
            "fieldnotes/ledger",
            "214",
            Kind::ReviewAsked,
            "Maya Lindqvist asked for your review",
            "Stream ledger exports in batches on a read replica",
            180,
            false,
        ),
        event(Gitlab, "fieldnotes/hooks", "90", Kind::Threads, "2 new threads", "Verify webhook signatures before queueing", 150, true),
        event(Gitlab, "fieldnotes/ledger", "240", Kind::Approved, "Samuel Achterberg approved", "Ledger paging 1: add cursor columns", 300, true),
        event(Github, "fieldnotes/cli", "42", Kind::ReviewAsked, "Jo Whitfield asked for your review", "Add a --dry-run flag to the import command", 360, true),
        event(Gitlab, "fieldnotes/ledger", "229", Kind::Closed, "Merged", "Rename the export settings", 22 * 60, true),
        event(Gitlab, "fieldnotes/hooks", "88", Kind::AskedAgain, "Devika Rao asked again", "Retry webhook deliveries with jittered backoff", 30 * 60, true),
    ]
}

/// A large review for timing: `files` Rust files of `lines` lines each.
pub fn big_diff(files: usize, lines: usize) -> Vec<FileDiff> {
    (0..files)
        .map(|f| {
            let mut diff = format!("@@ -1,{lines} +1,{lines} @@ impl Ledger {{\n");
            for i in 0..lines {
                match i % 7 {
                    0 => diff.push_str(&format!("-    let total_{i} = price * count; // old {i}\n")),
                    1 => diff.push_str(&format!("+    let total_{i} = price * quantity; // new {i}\n")),
                    2 => diff.push_str(&format!("+    /* a comment that starts here {i}\n")),
                    3 => diff.push_str("+       and ends here */\n"),
                    _ => diff.push_str(&format!("     fn entry_{i}(&self) -> Result<Entry, Error> {{ self.rows.get({i}).cloned().ok_or(Error::Missing) }}\n")),
                }
            }
            let path = format!("src/ledger/part_{f:02}.rs");
            file(&path, &path, &diff, false)
        })
        .collect()
}
