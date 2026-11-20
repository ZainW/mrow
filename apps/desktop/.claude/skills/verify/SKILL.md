---
name: verify
description: Drive the Mrow desktop app (Rust/Rune, apps/desktop) the way a user does and capture proof. Renders fictional fixture scenes headlessly, replays clicks/keys/typing from a JSON script, and saves a PNG plus the accessibility tree. Use it to prove a UI change works (sidebar lists, review tabs, command palette, inbox, Ask panel), or to check the live gh/glab queue fetch read-only.
---

# Verify Mrow

Mrow is a native desktop app, and nothing outside it can click a real window. Rune, the UI library, has a
headless runner that can. It loads a **scene** (fictional fixture data from `src/fixtures.rs`),
replays a **script** of user input against element keys, then writes a screenshot (`--snap`) and
a JSON tree (`--tree`) holding every element's key, label, text, and accessibility role and
state. That runner is the harness. It drives the real view code and the real `on_key`/`on_tap`
handlers. It doesn't drive the real forge.

Everything below runs from `apps/desktop`. The helper is
`.claude/skills/verify/scripts/mrow-verify` (shortened to `$V` below):

```sh
cd apps/desktop
V=.claude/skills/verify/scripts/mrow-verify
export MROW_VERIFY_RUN=<short-task-name>   # one evidence folder per task
```

## Launch

There's no server to keep alive. Each drive is its own short process (about 2 s, then it exits).

1. Build: `$V build` (runs `cargo build`, debug, about 45 s cold and seconds when warm).
2. List scenes: `$V scenes`. The main ones: `to-review` (the default fixture, GitLab, To review list,
   first request open on Overview), `changes`, `discussion`, `mine`, `mine-stack`, `inbox`,
   `palette`, `ask`, `submit-review`, `settings`, `github`, `empty`.
3. Drive: `$V run <name> <scene> [script.json] [extra rune args…]`. It's ready when the command
   prints three paths: `<name>.png`, `<name>.tree.json` and `<name>.a11y.txt`. A failed render
   prints the log and exits with code 2.

Headless runs are isolated by construction. Prefs are ephemeral (`prefs::set_ephemeral`), the store is
`offline`, Ask and the updater are offline, and avatars aren't fetched unless
`MROW_ONLINE_IMAGES=1`. With `--script`, Rune also records link opens without opening a browser.
A run checked on Linux wrote nothing under `~/.config/mrow` or `~/.cache/mrow`. Any number of runs can go in
parallel.

**The live app** (`target/debug/mrow` with no flags) opens a real window on the user's real
accounts. A debug build runs beside an installed Mrow. Scripts can't drive the live window, so
never launch it to "verify". Use it only when the user asks, and let them do the clicking.

## Doctor

Run this first, and again whenever something looks off:

```sh
$V doctor
```

It reports the binary and version, whether any source file is newer than the binary (`STALE` means
rebuild), whether a `to-review` render works and has the sidebar (`mode-0`), and whether `gh`
and `glab` are installed and signed in. The last line is `doctor: OK` or `doctor: NOT OK`, with exit
code 1 for NOT OK. On Linux, headless rendering needs a Vulkan driver (Mesa 24+ lavapipe works
without a GPU).

## Drive

A script is a JSON array of timed events. These are the ones that matter for Mrow:

```json
[
  {"t": 0.3, "type": "tap", "key": "mode-2"},
  {"t": 0.5, "type": "key", "name": "p", "cmd": true, "shift": true},
  {"t": 0.8, "type": "type", "text": "mine"},
  {"t": 1.4, "type": "key", "name": "Enter"},
  {"t": 1.6, "type": "key", "name": "Escape"},
  {"t": 1.8, "type": "scroll", "key": "rows", "dy": -400},
  {"t": 2.0, "type": "secondary-click", "key": "card"},
  {"t": 2.5, "type": "mark", "name": "end"}
]
```

- `tap`/`click` takes `"key"` (an element key from the tree) or `"x"`/`"y"` in points.
  Use keys. Fall back to the centre of an element's `rect` only when its key repeats (for example
  `card`, `comment`, `b0`).
- `"cmd": true` means ⌘ on macOS and Ctrl on Linux, which matches how `COMMANDS` in
  `src/ui/actions.rs` writes keys. On Linux, chords reach `on_key`. On macOS, menu-bar commands
  have not been checked headlessly yet. If a ⌘ chord does nothing there, use the visible button.
- End every script with a `mark` named `end` about 0.6 s after the last action. The tree and snap
  are taken at the end, and that leaves time for motion to settle.
- `--theme light|dark`, `--size WxH` (or `MROW_VERIFY_SIZE`) and `--at <seconds>` pass straight
  through as extra args. Snaps come out at 2× scale.

Reading the result:

- `<name>.a11y.txt` has one line per accessible node, written `role key 'name' [states]`, for
  example `tab tab-1 'Changes 3' [selected]` or `list-box-option cmd-mode-2 'Show Mine, Ctrl+3' [selected]`.
  Grep it.
- `$V texts <tree.json>` prints every drawn text run with its element path. Use it when a
  **modal** is open, because the a11y tree then contains only the dialog.
- The raw `tree.json` holds `elements[]` (`key`, `path`, `rect`, `label`, `text`, `focused`,
  `itemSelected`, `opacity`) and `accessibility` (nested). Query it with `python3 -I` when you need
  more than grep.

Live data, read-only: `$V live-queue github` (or `gitlab`) fetches the real queue through
`gh`/`glab` without opening a window and saves stdout and the exit code. It's the only check that
crosses the forge boundary, and it never writes to the forge.

## Evidence

Everything goes in `$($V where)`. The default is `~/.local/state/mrow-verify/$MROW_VERIFY_RUN/`, and
`MROW_VERIFY_OUT` overrides it. Each run leaves `<name>.png`, `<name>.tree.json`,
`<name>.a11y.txt`, `<name>.log` and a copy of the script it replayed (`<name>.script.json`).

The bar for proof:

- **Drive the user's path.** Tap the button, press the chord, type into the field. Don't open a
  scene that has already been put into the end state, because that shows the end state exists,
  not that a user can reach it. Scene setup is only for preconditions.
- **Capture before and after.** Make one run with no script (or stopped before the action) and one
  with it, and quote the a11y or text lines that changed. Read the PNG too, since an element can
  sit in the tree while being clipped or hidden visually.
- **Know what the fixtures can't prove.** The store is offline, so forge writes (approve, merge,
  comment, resolve, snooze sync) are refused: `perform` records them as failed and never
  calls `gh`/`glab`. A script can prove the UI path up to that call, but not that the forge
  changed. Proving a write needs the live app against a real request. That changes the user's
  GitLab/GitHub, so ask first, and the user drives it.
- **Keep it out of the repo.** Snaps and trees here are fictional, but snaps of the live app
  hold confidential work data. Never commit evidence or copy it into `apps/web`.

## Cleanup

Headless runs exit on their own, so there's nothing to kill. If a run hangs (for example from a
script with no `end` mark and a huge `--frames`), kill only that PID, the one your shell
started, and never `pkill mrow`, because the user's installed Mrow may be running. Delete scratch
scripts you wrote outside the evidence folder. **Keep the evidence folder.** It's the proof, and
cleanup must not remove it. Delete old runs under `~/.local/state/mrow-verify/` only when the user
asks.

## Feature map

`features/README.md` indexes one recipe per user-facing feature. Read the matching file before
driving, and cover every entry point it lists. Keep the map current with
`/maintain-verification-skill`.
