# Mrow

The GitLab merge requests and GitHub pull requests waiting on your review, in a native app for macOS
and Linux. It's written in Rust and drawn with [Rune](https://github.com/anomalyco/rune), Anomaly's
GPU UI library. Warm paper in light mode, neutral ink in dark, one clay accent; Inter for the
interface and Geist Mono for code.

Everything goes through the `glab` and `gh` CLIs, which keep their own tokens. Mrow never sees
them, stores nothing but a few preferences, and talks to nothing but those CLIs (and, for the Ask
panel, Cursor's `cursor-agent`). The one exception is updates: release builds fetch
`latest.json` from this repository's GitHub releases with `curl` every few hours, sending no
more than a `Mrow/<version>` user agent (Settings → Updates turns that off).

## Install

The latest release, for macOS (Apple silicon) or Linux (x86_64, arm64):

```sh
curl -fsSL https://mrow.wania.app/install | bash
```

Release builds update themselves: a new version downloads in the background, is checked against
the signature and checksum in its release, and goes in place when you click **Restart to Update**
in the sidebar (or in Settings). `mrow --update` does it from a terminal, and `mrow --version`
says which you have. If Mrow sits somewhere it can't write (say `/usr/bin`), it says so and
points you at the release instead.

## Build it

You need a Rust toolchain and a signed-in `glab` (`glab auth login`) and/or `gh` (`gh auth login`).
Run these from `apps/desktop`.

**macOS 15+**

```sh
scripts/install.sh       # release build into ~/Applications/Mrow.app, then opens it
scripts/bundle.sh        # dist/Mrow.app, without installing
```

**Linux** (X11 or Wayland, with a Vulkan driver)

```sh
# Debian/Ubuntu build packages; Fedora: libxkbcommon-devel wayland-devel
sudo apt-get install libxkbcommon-dev libwayland-dev pkg-config
scripts/package-linux.sh             # dist/mrow-linux-<arch>.tar.gz
scripts/package-linux.sh --install   # also into ~/.local (bin, desktop entry, icon)
```

At run time it needs `libxkbcommon-x11-0` and a Vulkan driver (`mesa-vulkan-drivers` covers
Intel, AMD and software rendering).

**Linux tarball from a Mac**: `scripts/docker-linux.sh` builds it in a Debian container, for the
container's architecture (arm64 on Apple silicon). Use `PLATFORM=linux/amd64` for x86_64.

On Linux, settings live in `~/.config/mrow` and scratch files in `~/.cache/mrow` (or the
`XDG_*` folders). Notifications work on both (on Linux through the desktop's notification daemon); the Dock
badge is macOS-only for now.

## What it does

- **Two accounts, one switcher**: GitLab and GitHub each have their own queue, and the switcher at
  the top of the sidebar flips between them. Both refresh together, so the badge and notifications
  cover both. Turn either off, or point it at a self-hosted or Enterprise host, in Settings.
- **Four lists**: *To review* (your turn), *Reviewed* (you approved, commented or asked for
  changes, still open), *Mine* (the ones you opened) and *Done* (merged or closed recently). The
  review lists are grouped by when you were asked: Today, Yesterday, This week, This month, Older.
- **Mine**: your own requests sorted by whose move it is: *Your move* (ready to merge first, then
  changes requested, a failed pipeline, conflicts, open threads), *Waiting on reviewers* and
  *Drafts*. Each row says its next step, and the marks above the list filter by standing. A
  request of yours opens on that next step, with the button for it: **Merge** (it asks once more
  first), **Merge When Ready** (the forge's auto-merge) or **Mark Ready**. Its page ends with
  what's left before it merges; a line there goes to what it names (the open thread, the
  pipeline, the part below it in a stack).
- **Stacks**: requests that target each other's branches show as one group, bottom first. A stack
  shows the parts that are on you and folds the rest into a count; click the fold or the stack's
  name (or press →) to see every part. **Merge the Stack** on a ready bottom part merges it,
  then each part above it as soon as that one is approved and green, and stops at the first that
  isn't. Mrow has to stay open while it does.
- **Reviews**: one page read top to bottom: the ask, the description rendered natively, every
  file's diff with syntax highlighting and line numbers (comment threads under their line), the
  conversation with a comment box, and your verdict with Approve / Revoke. A contents rail
  beside it lists those parts and every file, marks the one you're reading and jumps on a click.
  Each file shows its thread count, filled when there's a reply you haven't seen.
- **Tabs**: keep several reviews open; a single click previews, a double click keeps.
- **Ask**: a chat with Cursor's agent about the open review, beside the card.
- **Inbox**: the bell at the top of the sidebar counts what changed since you last looked: a
  review asked of you or asked again, and on your own requests an approval, changes asked for,
  a failed pipeline, new threads, ready to merge, merged or closed. Click a row to open its
  request; mark one or all done. Mrow works these out by comparing each refresh with the last,
  so the first refresh finds nothing and your own approvals and merges don't count.
- **Notifications**: one per request when something set to Notify happens (review requests,
  changes asked for, a failed pipeline, ready to merge, by default). Settings → Notifications
  sets each kind to Notify, Inbox only or Off.
- Unread dots (on Mine, from the inbox), a badge with what's waiting on you, and a refresh every
  5 minutes (configurable) and whenever you come back to the app.

GitHub keeps no per-reviewer state, so yours is pieced together from review requests and your
latest review. Revoking an approval dismisses your review, which needs maintainer rights there.

## Keys

On Linux, Ctrl stands in for ⌘, and the account switch is Alt+1 / Alt+2.

| | |
|---|---|
| ⌃1 ⌃2 | Switch account |
| ⌘1 ⌘2 ⌘3 ⌘4 | To review, Reviewed, Mine, Done |
| → ← | In Mine: show every part of a stack, fold it again |
| ⌥⌘1 ⌥⌘2 ⌥⌘3 | Top of the page, the changes, the conversation |
| J K | Next / previous part of the page (each file is one) |
| ⌥⌘] ⌥⌘[ | Next / previous review |
| ⌘K or ⌘F | Filter the list |
| ⌘R | Refresh |
| ⇧⌘I | Inbox |
| ⌘O | Open in GitLab / GitHub |
| ⇧⌘A | Approve |
| ⇧⌘C | Copy link |
| ⌘T, ⌘W, ⌥⌘W | Keep in a tab, close the tab, close the others |
| ⌃Tab, ⇧⌘] ⇧⌘[ | Next / previous tab |
| ⌘↩ | Post the comment |
| ⌃⌘S | Toggle the sidebar |
| ⌘L, ⇧⌘L, ⌥⌘L | Ask Cursor, new chat, choose the model |

## Development

```sh
cargo build && cargo test
target/debug/mrow --queue github        # live, read-only check of a forge mapping (or gitlab)
target/debug/mrow --scene to-review --size 1240x820 --snap out.png   # fictional scene to PNG
```

`src/data` holds forge access (`cli.rs`, `gitlab.rs`, `github.rs`), the models and the store;
`src/data/paths.rs` is where per-platform folders and tool lookup live. `src/ui` is the view.
Views stay forge-agnostic: use `kind.name()`, `.noun()` and `.reference_mark()`.

`src/data/update.rs` is the updater. Only builds made with `MROW_UPDATES=1` (the release
workflow sets it) update themselves. To try an update before publishing it, point a release
build at a folder holding `latest.json`, its `.sig` and the archive:
`MROW_UPDATE_FEED=file:///path/to/feed mrow --update`. The manifest must still be signed with
the release key.
