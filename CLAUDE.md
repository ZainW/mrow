# Mrow

Monorepo: `apps/desktop` (Rust app on Anomaly's Rune, macOS and Linux) and `apps/web` (TanStack
Start site, pnpm).

## Commands

- App (`apps/desktop`): `cargo build`, `cargo test`.
  - Install on macOS: `scripts/install.sh` builds `dist/Mrow.app` (bundle id `com.zain.mrow`) and
    replaces `~/Applications/Mrow.app`.
  - Linux: `scripts/package-linux.sh [--install]` on Linux makes `dist/mrow-linux-<arch>.tar.gz`;
    `scripts/docker-linux.sh` builds the same from macOS in Docker (`PLATFORM=linux/amd64` for
    x86_64). Headless renders on Linux need Mesa 24+ lavapipe (Ubuntu 24.04, not Debian bookworm).
  - Live read-only check: `target/debug/mrow --queue github` (or `gitlab`). Cursor agent check:
    `target/debug/mrow --ask-models [question]`.
  - Fictional scenes to PNG: `target/debug/mrow --scene <name> --size 1240x820 --snap <png>` (scenes
    include `to-review`, `changes`, `discussion`, `big-diff`, `ask`, `ask-chat`, `mine`, `mine-stack`). Headless runs stay
    offline; set `MROW_ONLINE_IMAGES=1` to fetch avatars and pictures. Write these to a scratch
    dir, never into the repo.
  - Frame timings: `cargo build --profile perf --features profile`, then run with
    `RUNE_FRAME_LOG=frames.jsonl` and `--script`/`--frames`. `MROW_ACP_LOG=<file>` logs the agent protocol.
  - Benchmarks: `target/perf/mrow --bench <folder> [label]` (perf profile, `profile` feature) replays
    a recorded queue (the old Swift app's `--bench-record` format) and writes `results-<label>.json`.
    Recordings are confidential; keep them in a scratch dir.
- Web: `pnpm install`, `pnpm --filter @mrow/web typecheck`, `pnpm --filter @mrow/web build`.
- Formatting: `cargo fmt` (`apps/desktop/rustfmt.toml`); PRs fail on `cargo fmt --check` and on
  any compiler warning.

## Releases and CI

- Releases are manual only: `apps/desktop/scripts/release.sh [patch|minor|major|X.Y.Z]` runs
  `.github/workflows/release.yml`, which builds macOS arm64 + Linux x86_64/arm64, then bumps
  the version, writes `CHANGELOG.md` (git-cliff, `cliff.toml`), tags `vX.Y.Z` and publishes the
  GitHub release with a signed `latest.json`. Commit messages are the changelog: use
  conventional commits (`feat:`, `fix:`, `perf:` appear; `(web)`-scoped and chores don't).
- CI minutes are scarce: PR checks stay static and fast (fmt, `cargo check`, web typecheck,
  actionlint/shellcheck, `.github/scripts/hygiene.sh`), path-filtered, never on push/merge. Don't
  add builds, test matrices or macOS jobs to PR checks.
- The updater (`src/data/update.rs`) is on only in builds with `MROW_UPDATES=1`. It verifies
  `latest.json` against `packaging/update-key.hex` (Ed25519; the private key is the
  `UPDATE_SIGNING_KEY` secret). Changing that key file without the matching secret strands every
  installed copy.

## App architecture

- All forge access goes through CLIs (`glab`, `gh`) via `src/data/cli.rs`. Never handle tokens.
- `src/data/gitlab.rs` and `github.rs` map each API onto the shared models in `src/data/models.rs`
  (`MergeRequest` is used for GitHub PRs too; `item.kind` is its forge). Views must stay
  forge-agnostic: use `kind.name()`, `.noun()` and `.reference_mark()` instead of hard-coding
  "GitLab", "merge request" or "!".
- Platform differences live in `src/data/paths.rs` (folders, tool lookup) and `src/platform.rs`
  (Dock badge, appearance). Commands and their keys are one table, `COMMANDS` in
  `src/ui/actions.rs`: the macOS menu bar on macOS, `on_key` elsewhere. Write shortcut hints with
  Mac glyphs through `components::keys`, which relabels them off macOS.
- `src/data/mine.rs` works out where each of my own merge requests stands (`Standing`), its
  one-line next step, and stacks (requests targeting each other's branches), from the shared
  models only. The store turns that into the Mine list's lines; `src/ui/mine.rs` draws them.
- High-frequency changes (tab switches, list moves, per-line hover buttons) land instantly; keep
  motion for occasional transitions.

## Rules

- Screenshots and bench fixtures contain confidential work data. Never commit them or copy them
  into `apps/web`; use fictional data.
