# Mrow

The pull and merge requests waiting on your review, in a calm native app for macOS and Linux.
Works with **GitLab** (through `glab`) and **GitHub** (through `gh`), with an account switcher to
flip between them. Tokens stay in the CLIs; Mrow never sees them.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/ZainW/mrow/main/apps/desktop/scripts/get.sh | bash
```

That installs the latest [release](https://github.com/ZainW/mrow/releases): `Mrow.app` into
`~/Applications` on macOS (Apple silicon), or `mrow` with its desktop entry into `~/.local` on
Linux (x86_64 and arm64). Then sign in to `glab auth login` and/or `gh auth login`.

Mrow keeps itself up to date: it checks the releases every few hours, downloads and verifies a new
version in the background, and puts it in place when you click **Restart to Update**. Turn that
off in Settings → Updates. `mrow --update` does the same from a terminal.

The macOS build is signed ad hoc, not notarized. Installed with the script above it opens
normally; a copy downloaded with a browser needs right-click → Open the first time.

## This repository

| Path | What |
|---|---|
| [`apps/desktop`](apps/desktop) | The app (Rust, drawn with Anomaly's Rune). See its [README](apps/desktop/README.md) for features, keys and internals. |
| [`apps/web`](apps/web) | The project website (TanStack Start). |

```sh
# App: needs a Rust toolchain, and `glab auth login` and/or `gh auth login`
cd apps/desktop
scripts/install.sh                     # macOS: release build into ~/Applications, then opens it
scripts/package-linux.sh --install     # Linux: tarball in dist/, installed into ~/.local
scripts/docker-linux.sh                # Linux tarball from a Mac, through Docker

# Website: needs Node 24 and pnpm
pnpm install
pnpm dev                               # apps/web dev server
```

Root scripts: `pnpm dev`, `pnpm build:web`, `pnpm typecheck`, `pnpm app:build`, `pnpm app:test`,
`pnpm app:install`. Builds from source don't update themselves; they update from git.

## Checks

Pull requests run quick checks only, each when its files change: `cargo fmt --check` and a
warning-free `cargo check` for the app, a type check for the website, and actionlint, shellcheck
and a scan for secrets or confidential files for everything. Nothing builds on merge. Before
pushing:

```sh
cd apps/desktop && cargo fmt && cargo test
pnpm --filter @mrow/web typecheck
```

## Releases

Releases are made by hand and only by hand:

```sh
apps/desktop/scripts/release.sh            # next patch version (or: minor, major, 1.4.0)
apps/desktop/scripts/release.sh --preview  # the notes it would publish (needs git-cliff)
```

or Actions → **Release** → Run workflow. It builds macOS (Apple silicon) and Linux (x86_64,
arm64) from `main`, and only once all three pass does it bump the version, write
[`CHANGELOG.md`](CHANGELOG.md) from the conventional commits since the last tag, tag, and publish
the GitHub release with the archives and a signed `latest.json`, which installed apps update from.

The first release ships the version in `apps/desktop/Cargo.toml`. Commit messages are the
changelog: `feat:` lands under New, `fix:` under Fixed, `perf:` under Faster; `chore`, `ci`,
`docs`, `refactor`, `test` and anything scoped `(web)` stay out.

Update signing: the release workflow signs `latest.json` with the Ed25519 key in the
`UPDATE_SIGNING_KEY` secret, and the app checks it against the public key built into it
(`apps/desktop/packaging/update-key.hex`). The workflow refuses to publish if the two don't
match. To rotate the key, ship a release built with the new public key while the old secret is
still in place, then swap the secret.

## Private data

Snapshots and benchmark recordings are made from a real review queue, so keep them out of the
repository (in a scratch folder). Use fictional data for anything committed, including the website.

## License

[MIT](LICENSE). Inter and Geist Mono are under the SIL Open Font License (see
`apps/desktop/assets/fonts`).
