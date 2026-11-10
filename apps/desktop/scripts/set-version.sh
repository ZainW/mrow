#!/usr/bin/env bash
# Sets the app's version in Cargo.toml and Cargo.lock: scripts/set-version.sh 1.2.3
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION="${1:?usage: scripts/set-version.sh <major.minor.patch>}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "not a major.minor.patch version: $VERSION" >&2
  exit 1
fi

# The first `version =` is the package's own; the lock entry follows `name = "mrow"`.
perl -0pi -e 's/^version = "[^"]*"/version = "'"$VERSION"'"/m' Cargo.toml
perl -0pi -e 's/(name = "mrow"\nversion = )"[^"]*"/$1"'"$VERSION"'"/' Cargo.lock
grep -q "^version = \"$VERSION\"" Cargo.toml
grep -A1 '^name = "mrow"$' Cargo.lock | grep -q "version = \"$VERSION\""
echo "Version $VERSION"
