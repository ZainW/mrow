#!/usr/bin/env bash
# Builds the Linux tarball from macOS (or anywhere with Docker) in a Debian container, for the
# container's architecture: arm64 on Apple silicon unless PLATFORM=linux/amd64.
# The cargo registry and target folder persist in Docker volumes between runs.
set -euo pipefail

cd "$(dirname "$0")/.."
PLATFORM="${PLATFORM:-}"
docker run --rm ${PLATFORM:+--platform "$PLATFORM"} \
  -v "$PWD":/src -w /src \
  -v mrow-cargo:/usr/local/cargo/registry \
  -v "mrow-target${PLATFORM:+-${PLATFORM//\//-}}":/src/target \
  rust:1-bookworm \
  bash -c 'apt-get update -qq && apt-get install -y -qq libxkbcommon-dev libwayland-dev pkg-config >/dev/null && scripts/package-linux.sh'
