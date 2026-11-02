# Mrow

Monorepo: `apps/mac` (Swift macOS app, SwiftPM, macOS 15+) and `apps/web` (TanStack Start site, pnpm).

## Commands

- Mac: `cd apps/mac && swift build && .build/debug/Mrow --selftest` (must print `all passed`).
- Mac, live read-only check of a forge mapping: `.build/debug/Mrow --queue github` (or `gitlab`).
- Mac, render the window to PNGs: `MROW_FORGE=github .build/debug/Mrow --snapshot <dir>`. Write these
  to a scratch dir, never into the repo.
- Web: `pnpm install`, `pnpm --filter @mrow/web typecheck`, `pnpm --filter @mrow/web build`.

## Mac app architecture

- All forge access goes through CLIs (`glab`, `gh`) via `Data/CLI.swift`. Never handle tokens.
- `Data/Forge.swift` defines `ReviewProvider`; `GitLabProvider` and `GitHubProvider` map each API onto
  the shared models in `Data/Models.swift` (`MergeRequest` is used for GitHub PRs too; `item.kind`
  is its forge). Views must stay forge-agnostic: use `item.kind.name`, `.noun`, `.referenceMark`
  instead of hard-coding "GitLab", "merge request" or "!".
- `ReviewStore` keeps one `Account` per enabled forge and an active `forge` for the sidebar; actions
  dispatch to `item.kind.provider`.
- Add self-test checks in `App/Bench.swift` (`SelfTest`) for parsing and mapping logic.

## Rules

- Screenshots and bench fixtures contain confidential work data and are git-ignored. Never commit
  them or copy them into `apps/web`; use fictional data.
