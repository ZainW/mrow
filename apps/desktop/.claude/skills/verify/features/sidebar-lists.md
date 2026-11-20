# Sidebar lists

The sidebar shows one of four lists for the current account: To review (grouped by when you were
asked), Reviewed, Mine (grouped by whose move it is, with standing filters and stacks) and Done.
The account switcher above the lists flips between GitLab and GitHub.

## Sub-features

- `list-switch`: the four mode buttons, or Ctrl+1 to Ctrl+4, switch the list.
- `review-groups`: To review groups rows as Today / Yesterday / This week / ….
- `mine-groups`: Mine groups rows as Your move / Waiting on reviewers / Drafts, and each row gives its next step.
- `mine-standing`: the filter chips `standing-all`, `standing-ready`, `standing-needs you`,
  `standing-waiting` and `standing-draft`.
- `stacks`: a stack head (`head`, labelled "…, a stack of N. Show every part") unfolds its parts.
- `account-switch`: the `account` button switches forges.
- `filter`: the "Filter merge requests" field narrows the list.

## How to get to it (user POV)

- Click "To review" (`mode-0`), "Reviewed" (`mode-1`), "Mine" (`mode-2`) or "Done" (`mode-3`).
- Ctrl+1 to Ctrl+4 (⌘1 to ⌘4 on macOS).
- The command palette's "Show …" commands (see [command-palette](./command-palette.md)).
- Click the account name (`account`) to switch forges.

## Driving it with mrow-verify

Preconditions: the `to-review` scene.

- **Switch to Mine by click.** Use `[{"t":0.3,"type":"tap","key":"mode-2"},{"t":1.0,"type":"mark","name":"end"}]`.
  `$V texts` then shows `rows/h-Your move`, `rows/h-Waiting on reviewers` and `rows/h-Drafts`, and the a11y
  file gains `button standing-ready 'Ready to merge: 2'`.
- **Switch by key.** Use `{"type":"key","name":"3","cmd":true}`, which should give the same result as the click.
- **Filter by standing.** On Mine, tap `standing-ready`. Only ready rows remain, so check the row titles
  with `$V texts`.
- **Unfold a stack.** On Mine, tap the button whose a11y name starts "Ledger paging, a stack of 4"
  (key `head`, at its `rect` centre). Four parts are then listed instead of "3 more".
- **Filter text.** Tap `input` under `filter`, then type `ledger`. Only ledger rows remain.

## Gotchas

- `card` and `head` keys repeat once per row, so tap by `rect`.
- Each mode button's label includes its count ("Mine 4"), so match on the prefix.
- The `mine`, `mine-stack` and `github` scenes start in the end state. They're fine as
  preconditions for deeper journeys, but they don't prove the switch itself.
