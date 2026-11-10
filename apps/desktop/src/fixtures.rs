//! A made-up queue for snapshots and tests: two accounts with a handful of reviews, diffs and
//! threads. Everything here is fictional.

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, Utc};

use crate::data::forge::Forge;
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
    accounts.insert(Forge::Gitlab, Account { me: Some(me.clone()), open: gitlab_open.clone(), done: gitlab_done, error: None, last_refreshed: now });
    accounts.insert(Forge::Github, Account { me: Some(me), open: github_open, done: vec![], error: None, last_refreshed: now });

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
        ],
    );
    (accounts, diffs, threads)
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
