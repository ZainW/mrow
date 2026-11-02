# Mrow

The pull and merge requests waiting on your review, in a calm native Mac app. Works with
**GitLab** (through `glab`) and **GitHub** (through `gh`), with an account switcher to flip between
them. Tokens stay in the CLIs' keychain; Mrow never sees them.

This is a monorepo:

| Path | What |
|---|---|
| [`apps/mac`](apps/mac) | The macOS app (Swift, AppKit + SwiftUI, SwiftPM). See its [README](apps/mac/README.md) for features, keys and internals. |
| [`apps/web`](apps/web) | The project website (TanStack Start). |

## Quick start

```sh
# Mac app: needs macOS 15+, Swift toolchain, and `glab auth login` and/or `gh auth login`
cd apps/mac
scripts/install.sh                     # release build into ~/Applications, then opens it
swift build && .build/debug/Mrow --selftest

# Website: needs Node 24 and pnpm
pnpm install
pnpm dev                               # apps/web dev server
```

Root scripts: `pnpm dev`, `pnpm build:web`, `pnpm typecheck`, `pnpm mac:build`, `pnpm mac:test`,
`pnpm mac:run`.

## CI

GitHub Actions builds and self-tests the Mac app (`.github/workflows/mac.yml`) and typechecks and
builds the website (`.github/workflows/web.yml`), each only when its folder changes. Nothing is
signed, packaged or deployed yet.

## Private data

Snapshots (`apps/mac/screenshots/`) and benchmark recordings (`apps/mac/bench/fixtures/`) are made
from a real review queue, so they're git-ignored. Keep it that way: use fictional data for anything
committed, including the website.
