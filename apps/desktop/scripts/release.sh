#!/usr/bin/env bash
# Starts a release on GitHub Actions and follows it:
#   scripts/release.sh            # next patch version
#   scripts/release.sh minor      # or major
#   scripts/release.sh 1.4.0      # an exact version
#   scripts/release.sh --preview  # print the changelog the next release would get (needs git-cliff)
#
# The workflow builds macOS and Linux, then (only if every build passed) bumps the version,
# updates CHANGELOG.md, tags, and publishes the GitHub release that installed apps update from.
set -euo pipefail

cd "$(dirname "$0")/../../.."
REPO="ZainW/mrow"
ARG="${1:-patch}"

if [[ "$ARG" == "--preview" ]]; then
  git-cliff --config cliff.toml --unreleased --strip all
  exit 0
fi

if [[ "$ARG" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  INPUTS=(-f "version=$ARG")
elif [[ "$ARG" =~ ^(patch|minor|major)$ ]]; then
  INPUTS=(-f "bump=$ARG")
else
  echo "usage: scripts/release.sh [patch|minor|major|X.Y.Z|--preview]" >&2
  exit 2
fi

git fetch -q origin main
if [[ -n "$(git log origin/main..HEAD --oneline 2>/dev/null)" && "$(git rev-parse --abbrev-ref HEAD)" == "main" ]]; then
  echo "Your main has commits origin doesn't; push them first (releases build origin/main)." >&2
  exit 1
fi

gh workflow run release.yml -R "$REPO" --ref main "${INPUTS[@]}"
sleep 3
RUN="$(gh run list -R "$REPO" --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
echo "https://github.com/$REPO/actions/runs/$RUN"
gh run watch -R "$REPO" "$RUN" --exit-status
