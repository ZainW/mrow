# Review tabs

An open request shows three tabs (Overview, Changes and Conversation) under a bar with "Open in
GitLab/GitHub", More, Review (with a count of pending comments) and Approve. The Changes tab lists
files, and each line has a comment button.

## Sub-features

- `tab-switch`: `tab-0` Overview, `tab-1` Changes N, `tab-2` Conversation N.
- `file-list`: `file-entry-<i>` jumps to a file, and `fold` collapses it.
- `line-comment`: the per-line "Comment on this line" button opens a draft.
- `review-panel`: Review (or Approve while comments are pending) opens "Finish your review" with
  `verdict-COMMENT`, `verdict-REQUEST_CHANGES` and `verdict-APPROVE`.
- `start-file`: Overview's "Start with" (`start-changes`) jumps to the first file not viewed yet.

## How to get to it (user POV)

- Click a tab, or use the tab chords ⌥⌘1 to ⌥⌘3 (`COMMANDS` in `src/ui/actions.rs`), or Ctrl+Tab
  to cycle.
- Click a request row in the sidebar to open it.
- In Conversation, a thread's header jumps to its line in Changes.

## Driving it with mrow-verify

Preconditions: the `to-review` scene, with !214 open on Overview and 3 pending comments.

- **Switch tabs.** Use `[{"t":0.3,"type":"tap","key":"tab-2"},{"t":1.0,"type":"mark","name":"end"}]`. The a11y
  file shows `tab tab-2 'Conversation 6, with something new' [selected]`, with `tab-0` and `tab-1`
  `[unselected]`.
- **Changes.** Tap `tab-1`. The result shows `button file-entry-0 'src/export/job.rs'` and comment buttons.
- **Open the review panel.** Tap `approve` (or `review-button`). The a11y file shows only
  `dialog review-panel 'Finish your review' [modal]` and the line `'3 pending comments post with it.'`.
- **Submit.** Tapping a verdict proves the UI path only. The fixture store is offline and refuses the
  post (see SKILL.md, Evidence).

## Gotchas

- With pending comments, Approve opens the review panel instead of approving straight away. That's
  expected.
- A modal hides the rest of the a11y tree, so use `$V texts` for the page behind it.
- `comment` and `b0`-style keys repeat, so tap by `rect`.
