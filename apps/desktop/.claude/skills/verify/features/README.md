# Mrow verification map

This folder is the maintained source for verifying Mrow's user-facing behaviour. Read this index,
then use the matching feature file as the recipe. All commands run from `apps/desktop`, with
`V=.claude/skills/verify/scripts/mrow-verify`.

## Baseline preconditions

- `$V doctor` ends in `doctor: OK` (fresh build, headless render works).
- `export MROW_VERIFY_RUN=<task>` so this task's evidence lands in one folder.
- Start from the `to-review` scene unless a recipe says otherwise. In it the GitLab account is
  `@robin`, To review has 3 requests (`!214` open, `!88`, `!301`), and the counts read Reviewed 2,
  Mine 4, and Inbox 4 new.
- Never drive the live window. Live checks go through `$V live-queue` only.

## Driving conventions

- Write each script as a JSON file and run `$V run <name> <scene> <script.json>`.
- Address elements by key (`mode-2`, `tab-1`, `palette-list`, `inbox-bell`). Keys holding
  timestamps (inbox rows) or repeated keys (`card`, `comment`) aren't stable, so match those by
  their label in the a11y file, or tap the centre of their `rect`.
- Commands use `"cmd": true`, which is Ctrl on Linux and ⌘ on macOS.
- End every script with `{"type": "mark", "name": "end"}` about 0.6 s after the last action.

## Proof and skip reporting

- Pair a before run with an after run, and quote the a11y lines (or `$V texts` lines) that changed.
- Look at the after PNG, not just the tree.
- Forge writes can't be proven against fixtures because the store is offline. Report them as
  "UI path verified; forge write unverified" unless the user drove a live run.
- Report an unreachable path with the script you tried and the precondition that was missing. Don't
  count a different entry point as covering the one you skipped.

## Feature entry contract

Each feature file has an H1 and a one-paragraph summary, then four H2s in this order:
`Sub-features`, `How to get to it (user POV)`, `Driving it with mrow-verify`, `Gotchas`.

## Features

- [Sidebar lists](./sidebar-lists.md): To review / Reviewed / Mine / Done switching, Mine's
  standing filters and stacks.
- [Review tabs](./review-tabs.md): Overview / Changes / Conversation on an open request, and
  Approve / Review.
- [Command palette](./command-palette.md): open, filter, run a command, go to a request.
- [Inbox](./inbox.md): the bell, its events, Mark All Done.
- [Ask panel](./ask-panel.md): open the agent side panel on a request (UI only, offline).
