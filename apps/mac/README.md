# Mrow

A personal Mac app for the GitLab merge requests and GitHub pull requests waiting on your review, dressed like
[Mdow](https://mdow.wania.app): warm paper in light mode, neutral ink in dark, one clay accent,
Charter for titles and descriptions, Inter for the interface and Geist Mono for code.

Everything goes through the `glab` and `gh` CLIs, which keep their tokens in the keychain. Mrow never
sees them, stores nothing but a few preferences in UserDefaults, and talks to nothing but those CLIs.

## Run it

Needs macOS 15+, the Swift toolchain (Command Line Tools are enough) and a signed-in `glab`
(`glab auth login`) and/or `gh` (`gh auth login`). Run these from `apps/mac`.

```sh
scripts/install.sh                       # release build into ~/Applications, then opens it
CONFIGURATION=debug scripts/bundle.sh    # dist/Mrow.app, without installing
```

## What it does

- **Two accounts, one switcher**: GitLab and GitHub each have their own queue. The switcher at the
  top of the sidebar flips between them like a team picker (⌃1 / ⌃2), shows who you're signed in as
  and what's waiting in the other account. Both refresh together, so the Dock badge and
  notifications cover both. Turn either off, or point it at a self-hosted / Enterprise host, in
  Settings.

- **Gathers** (GitLab) every open merge request where you're a reviewer with one GraphQL query, and finds
  *when you were asked* from GitLab's "requested review from @you" activity note. A re-request
  counts as a new ask and moves the merge request back up. If no note turns up (rare), the
  creation date stands in and the row's tooltip says "Opened".
- **Gathers** (GitHub) pull requests from four searches: `review-requested:@me` (it's your turn,
  including through a team), `reviewed-by:@me` (still open), and the closed ones of each for Done.
  When you were asked comes from the timeline's review-requested events. GitHub keeps no per-reviewer
  state, so yours is pieced together: a pending request means it's your turn, otherwise your latest
  review (approved, changes requested, commented, dismissed) says where it stands.
- **Sorts and groups** the sidebar by that moment: Today, Yesterday, This week, This month, Older.
  Requests waiting on you for more than two days show their age in clay.
- **Three lists**: *To review* (your review state is unreviewed, started or reset by new commits),
  *Reviewed* (you approved, commented or asked for changes, still open) and *Done* (merged or
  closed in the last 14 days, configurable).
- **Reviews**: the description rendered natively, every file's diff with line numbers, comment
  threads with their file and line, a comment box, and Approve / Revoke Approval.
- Unread dots for merge requests that changed since you last opened them, a Dock badge with the
  count waiting on you, a notification for each new or re-requested review, and refresh every
  5 minutes (configurable) and whenever you come back to the app.

### GitHub differences

- **Revoke Approval** dismisses your approving review, which GitHub only allows people who can
  maintain the repository; otherwise it shows GitHub's error.
- Replies to conversation comments post a new comment (GitHub doesn't thread those); replies to line
  threads stay in the thread. Review summaries with a body show up as comments.
- Images attached in private repositories (`github.com/user-attachments/…`) need a browser session,
  so they may not load in the app.
- Benchmarks (`--bench-record`, `--bench`) are GitLab-only.

## Keys

| | |
|---|---|
| ⌃1 ⌃2 | Switch account (GitLab, GitHub) |
| ⌘1 ⌘2 ⌘3 | To review, Reviewed, Done |
| ⌥⌘1 ⌥⌘2 ⌥⌘3 | Overview, Changes, Discussion |
| ↑ ↓ or j k | Move through the list (when it has focus) |
| ⌥⌘] ⌥⌘[ | Next / previous merge request |
| ⌘K or ⌘F | Filter the list |
| ⌘R | Refresh |
| ⌘O or ↩ | Open in GitLab / GitHub |
| ⇧⌘A | Approve |
| ⇧⌘C | Copy link |
| ⌘↩ | Post the comment |
| ⌃⌘S | Toggle the sidebar |
| ⌘L | Ask Cursor: open the chat, move into it, or close it from inside |
| ⇧⌘L / ⌥⌘L | New chat / choose the model |
| ←→ (model picker) | Change the chosen model's effort |
| esc (in the chat) | Stop the answer |

## Layout

- `Sources/Mrow/Data`: `CLI` runs `glab` / `gh`; `Forge` defines the `ReviewProvider` protocol each
  forge implements (`GitLabProvider` with `Queries`, `GitHubProvider`); `ReviewStore` holds every
  account's queue, the active one, selection and tabs, and sends actions to the item's provider;
  `Preferences` is UserDefaults.
- `Sources/Mrow/Theme`: Mdow's `Palette` and fonts.
- `Sources/Mrow/Views`: the window (`RootView`), sidebar, detail card, diffs, and a small markdown
  renderer.
- `Mrow --snapshot <folder>` renders the first merge request's card to PNGs in light and dark
  without touching the screen, for checking the look from a script.

## Checking it

- `Mrow --selftest` runs the parser, highlighter, mention and GitHub mapping checks and prints `all passed`.
- `Mrow --queue gitlab|github` fetches one account's queue (read-only), prints each item's state,
  request time and reviewers, loads the first one's diffs and threads, and quits.
- `MROW_FORGE=github Mrow --snapshot <folder>` snapshots that account instead of the last one used.
- `Mrow --bench-record bench/fixtures` saves your current GitLab queue, diffs and threads (read-only glab calls)
  so later runs need no network.
- `Mrow --bench "$PWD/bench/fixtures" <label>` replays that recording offline, times the pure work
  (parsing, highlighting, markdown) and real frames (switching, opening and scrolling changes,
  opening a discussion, typing in the filter), and writes `results-<label>.json`.
- `python3 scripts/bench-compare.py before.json after.json` prints the two runs side by side.

Frames are timed as layout, display and a Core Animation flush. `cacheDisplay` re-renders the whole
view on the CPU and used to be most of the measured time, so it's only used for the check PNG.

## Ask Cursor

⌘L (or the speech-bubble button) opens a chat beside the merge request, run by Cursor's `agent acp`.
The first question carries the description, the diff (up to ~150 KB, smallest files first) and the
threads; the agent reads your clone (found from `.git/config` remotes under `~/caribou`, `~/code`…, or
picked with "Change…") as it is checked out. It's read-only three ways: chats run in Cursor's ask
mode, Mrow offers the agent no file or terminal access, and it declines every permission request
(shown in the chat). Commands on your own Cursor allowlist (`~/.cursor/cli-config.json`) still run
without asking. One chat per merge request; it's kept and reopened after relaunch. The panel is resizable from its left edge, the transcript
follows a streaming answer until you scroll up, and answers and code blocks have copy buttons.

- `Mrow --ask <folder> [project] [question]` asks a real question about the first merge request to
  review, prints the chat and renders the panel. `MROW_ACP_LOG=<file>` logs every ACP message.
