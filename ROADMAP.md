# Roadmap

What Mrow could do next. Nothing here is promised or scheduled; the order within a section is
roughly by value.

## Shipped

- **v0.1.0**: the review queue from GitLab and GitHub on macOS and Linux; Ask (Claude, opencode
  or pi) drafting comments you approve before they post; Mine, stacks, merging, the inbox and
  notifications; one Mrow per user on Linux; a review as one page with a contents rail; Charis
  SIL as the reading face and type on whole points; the site at mrow.wania.app.
- **v0.1.1**: a disk image beside the archive, and the macOS release signed with a Developer ID
  and notarized.
- **On main, not released**: a review in three tabs (Overview, Changes, Conversation) under a
  fixed bar holding Approve and "Open in <forge>", with ⌥⌘1/2/3 between tabs and J and K file by
  file. The site deploys from main and after every release.
- **On main, not released**: finishing a review in the app. Threads resolve and reopen; comments
  can be held in a pending review on the forge (edited, deleted, shown in place) and submitted
  with Comment, Request Changes or Approve and a summary (⇧⌘R).
- **On main, not released**: fewer trips to the forge. A request's pipeline jobs (checks on GitHub) on
  its Overview, grouped by stage, each retried on its own or run when manual, with its log
  inline; the unchanged lines around a diff's hunks, twenty at a time or all; editing and
  deleting my own posted comments; on my own requests, adding, removing and asking reviewers
  again, rebasing (updating the branch on GitHub), closing and reopening, and editing the title
  and description; and the next request's changes and threads loaded ahead.
- **In this branch**: a command palette (⇧⌘P) built from `COMMANDS`, which also jumps to a
  request by title or number and says why a command it hides can't run; N and P through open
  threads (⇧N and ⇧P through all of them), V to mark a file viewed and go on to the next, R to
  reply and C to comment, with the thread or file you're on marked; the Changes tab's file list
  grouped by folder and folded to fit the first screen, built only where it's in sight; snoozing
  a request until tomorrow morning or until it changes, and muting it, out of the list, the
  counts and the banners; opening filled with the last queue and recently opened requests from
  disk; and a polish pass (quiet text that reads, the request number kept whole in a narrow
  sidebar, the other account's count with its forge's mark, a narrower reading column).

Loose ends from these:

- This branch's keys, the palette's focus hand-off, folder clicks and the open snoozed fold
  were tested in logic but never driven through the window: headless renders hung on the GPU
  partway. The disk cache hasn't been run against a forge, and nothing ran on macOS.
- The clay accent and the red are close in dark mode, so an unread dot and a failed-pipeline
  dot read alike. Settling it is a palette decision.

- The site describes the tabs and finishing a review ahead of the release that ships them.
- Resolving, pending reviews and submitting haven't been run against either forge. GitLab's
  Request Changes needs 17.x; GitHub can't hold a comment on the whole pull request in a
  review, so there the summary stands in. Replies still post at once.
- The tabs haven't been run on macOS (menu bar included), against live forge data, or through
  the frame-time benchmark.
- None of this branch's forge calls have been run live. GitLab reviewer changes use
  `mergeRequestSetReviewers` with `operationMode`; after a rebase, a `merge_error` left over
  from an earlier merge could read as the rebase failing. GitHub only re-runs Actions jobs,
  names a check's stage after its app, can't take a comment on a line outside the diff, and
  merges the base in rather than rebasing. A closed request leaves Mine at the next refresh.

## In progress: your own requests, stacks, an inbox, notifications

Mrow shows the reviews waiting on you. These add the other half: the requests you opened, how
they stack, and one place that says what changed.

Built so far: the Mine list with its sections, status marks and filters; the next step and
"Before it merges" on a request of yours; stacks among your own requests, folded to what's on
you; a reason line on review rows; merging (with a second press to confirm), "Merge When Ready"
(the forge's own auto-merge), marking a draft ready, and merging a stack from the bottom up.

The inbox behind the bell: a review asked of you or asked again, and on your own requests an
approval, changes asked for, a failed pipeline, new threads, ready to merge (a held part too,
once the one below merges), and merged or closed. It is worked out by comparing each refresh
with the one before, so it only knows what the queue shows, and it can't tell a merge from a
close unless the Done list or a lagging search says which. One row per event, kept 14 days (100
at most); opening one goes to its request, and each can be marked done. Unread dots on Mine
come from it, so your own pushes don't light them. Notifications come from the same events, one
banner per request, with a Notify, Inbox or Off setting for each kind.

Merging a stack: the bottom merges, then each part above it is moved onto the branch the stack
lands on and merged once it's approved and green. It stops at the first part that needs a
person, when a step fails, or after 45 minutes of waiting on one part. It runs only while Mrow
is open and isn't remembered across launches.

Still to build, in this order:

1. Re-requesting a review after a push. (A nudge for quiet reviewers is parked: what it should
   do isn't settled.)
2. Inbox events the queue can't show: a reply in your thread and a mention. Opening an event
   on the thing that changed (the thread, the job), snoozing, and one row per request.
3. Actions on the banner (Open threads, Merge, Open job), and a waiting count on Linux.
4. Stacks in the review lists, including parts you weren't asked to review (they aren't
   fetched today, so a stack only shows the parts Mrow already knows).
5. Reason lines that need more than the queue has: "pushed 3 commits and asked again".

Banners on Linux have been posted to a real daemon, but a click on one hasn't been tried.
The GitLab side of Mine is written but hasn't been run against a GitLab host yet, and none of
the merge actions have been run against a real request on either forge.

### Mine

A list of the merge and pull requests you opened, sorted by whose move it is.

- **Sections**: *Your move* (changes requested, a failed pipeline, conflicts, replies waiting on
  you, or approved and ready to merge), *Waiting on reviewers*, *Drafts*, and recently landed.
- **One next step per request**, in the row and at the top of the request: "Maya asked for
  changes · 2 threads open", "Pipeline failed · test:integration", "Approved by 2 · ready to
  merge", "Waiting on Tomás · 3 days".
- **What blocks the merge**, as a short checklist: approvals, pipeline, unresolved threads,
  conflicts. Each line jumps to the thing it names.
- **Actions that fit an author**: merge, merge when ready, re-request a review after pushing,
  nudge a reviewer who has been quiet for days, mark a draft ready, open the failed job.
- A request leaves *Your move* as soon as you act on it, and comes back when the answer arrives.
- **A status mark on every row** (ready to merge, needs you, waiting, gone quiet, held, draft),
  and the same marks as filters above the list, each with its count. Requests that are ready to
  merge sort to the top of *Your move*: they are the ones that can certainly be acted on.

Needs from the forge layer: a queue of authored requests, failed job names, thread counts,
merge, re-request, and draft/ready.

### Stacks

A stack is a chain of requests where each one targets the branch of the one below it. Both
`source_branch` and `target_branch` are already on the model, so stacks within one project can be
found without new calls.

- **In Mine**, a stack is one group joined by a rail, bottom first, placed where its most
  actionable request belongs. Under a filter the list is flat and each row says "2 of 3".
- **Stacks fold to what is on you.** A stack shows the parts that need you and the one you have
  open. The rest (drafts, parts you weren't asked to review, parts waiting on someone else) fold
  into one line that counts them: "3 more · 1 held, 1 waiting, 1 draft". A stack with nothing on
  you folds to a single row. Either unfolds with a click.
- **In a request**, a stack section lists every part with its status and jumps between them.
- **Stack-aware next steps**: only the bottom can merge; an approved request above it is *held*
  and says which one it waits for. "Merge, then the next when it is green" walks the stack down.
- **For reviews**, the same section shows where the request you were asked about sits, including
  parts you weren't asked to review, and Changes shows only that part.
- **Notifications**: "the request below yours merged, and yours is next".

Not covered by branch matching: stacks across forks or projects.

### Inbox

A list of what changed, across both accounts and both roles, where the lists above say what
state things are in.

- One row per request, holding its latest events: a review request, a reply in your thread, a
  mention, an approval, changes requested, a failed pipeline, a merge.
- Opening a row lands on the thing that changed (the thread, the job, the new commits).
- Mark done, or snooze until tomorrow or until the request changes again.
- It replaces unread dots as the record of what you haven't looked at, and every notification
  has a row here, so a missed banner is never lost.

Decided: Mine is a fourth list beside To review, Reviewed and Done. The inbox is a popover behind
a bell at the top of the sidebar, not the home screen. Every row in every list carries a reason
line ("Ines pushed 3 commits and asked again"), including reviews.

### Notifications

- Notify when the turn changes, and stay quiet otherwise: a review request or re-request,
  changes requested on yours, yours ready to merge, a failed pipeline on yours, a reply in a
  thread you are in, a mention.
- Inbox only: an approval that doesn't complete the set, yours merged or closed by someone else.
- Silent: pipelines passing or running, comments in threads you are not in.
- Per-event settings (notify, inbox only, off), one banner for several updates to the same
  request, and actions on the banner (Open threads, Merge, Open job).
- Linux: banners go to the desktop's notification daemon over D-Bus (rune-notify has no Linux
  side). A waiting count is still macOS-only.

## Later

### Finish a review inside the app

- Replies held in a pending review, not only new threads.
- Multi-line comments and suggestion blocks.
- Merge, for requests that are ready.

### Re-reviews

- Changes since your last review, and a list of commits.
- On a re-request, show the threads you opened that got a reply or whose lines changed.

### Reading diffs

- Mark the changed words within a changed line.
- Expand the context around a hunk; a side-by-side mode for wide windows.
- Find in the diff (⌘F filters the list today).
- A comment on a whole file (C comments on the request today).
- Image diffs.

### Queue

- Filter by project or label, group by repository, one list across both accounts.
- Failed jobs by name, with links, behind the pipeline chip.
- A setting for whether team review requests and mentions count as your turn.

### Platform

- More than one account per forge (github.com and an Enterprise host, two GitLab hosts).

### Ask

- "Review this": a summary, the risky parts and drafted line comments, as a pending review.
- "Explain this hunk", from a button on the hunk.
