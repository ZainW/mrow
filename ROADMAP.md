# Roadmap

What Mrow could do next. Nothing here is promised or scheduled; the order within a section is
roughly by value.

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

- Request changes (today the only verdict is Approve).
- Resolve and unresolve threads.
- Batched reviews: comments held as pending and submitted together with a verdict.
- Multi-line comments and suggestion blocks.
- Merge, for requests that are ready.

### Re-reviews

- Changes since your last review, and a list of commits.
- On a re-request, show the threads you opened that got a reply or whose lines changed.

### Reading diffs

- Mark the changed words within a changed line.
- Expand the context around a hunk; a side-by-side mode for wide windows.
- Find in the diff (⌘F filters the list today).
- A keyboard flow: next and previous file, thread and unresolved thread, mark viewed, comment.
- Image diffs.

### Queue

- Snooze and mute.
- Filter by project or label, group by repository, one list across both accounts.
- Failed jobs by name, with links, behind the pipeline chip.
- A setting for whether team review requests and mentions count as your turn.

### Platform

- Keep the last queue and opened diffs on disk, so the app opens filled.
- A command palette built from `COMMANDS`.
- More than one account per forge (github.com and an Enterprise host, two GitLab hosts).

### Ask

- "Review this": a summary, the risky parts and drafted line comments, as a pending review.
- "Explain this hunk", from a button on the hunk.
