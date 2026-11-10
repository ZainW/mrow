#!/usr/bin/env bash
# Installs the latest Mrow release, for people who don't build it themselves:
#   curl -fsSL https://raw.githubusercontent.com/ZainW/mrow/main/apps/desktop/scripts/get.sh | bash
#
# macOS (Apple silicon): Mrow.app into ~/Applications (DEST=… to change).
# Linux (x86_64, arm64): bin, desktop entry and icon into ~/.local (PREFIX=… to change).
# Checks the download against the SHA-256 in the release's manifest. Once installed, Mrow
# updates itself.
set -euo pipefail

REPO="ZainW/mrow"
BASE="https://github.com/$REPO/releases/latest/download"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) KEY="macos-aarch64" ;;
  Linux-x86_64) KEY="linux-x86_64" ;;
  Linux-aarch64 | Linux-arm64) KEY="linux-aarch64" ;;
  *) echo "No Mrow build for $(uname -s) $(uname -m). Build it from source: https://github.com/$REPO" >&2; exit 1 ;;
esac

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

curl -fsSL "$BASE/latest.json" -o "$WORK/latest.json"
# The manifest is flat enough to read without jq: "<key>": { "url": "…", "sha256": "…" }
ENTRY="$(tr -d '\n ' < "$WORK/latest.json" | grep -o "\"$KEY\":{[^}]*}")"
URL="$(sed -n 's/.*"url":"\([^"]*\)".*/\1/p' <<<"$ENTRY")"
SUM="$(sed -n 's/.*"sha256":"\([^"]*\)".*/\1/p' <<<"$ENTRY")"
VERSION="$(tr -d '\n ' < "$WORK/latest.json" | sed -n 's/.*"version":"\([^"]*\)".*/\1/p')"
[[ -n "$URL" && -n "$SUM" ]] || { echo "The latest release has no $KEY build." >&2; exit 1; }

echo "Downloading Mrow $VERSION ($KEY)…"
curl -fsSL "$URL" -o "$WORK/mrow.tar.gz"
if command -v sha256sum >/dev/null; then
  GOT="$(sha256sum "$WORK/mrow.tar.gz" | cut -d' ' -f1)"
else
  GOT="$(shasum -a 256 "$WORK/mrow.tar.gz" | cut -d' ' -f1)"
fi
[[ "$GOT" == "$SUM" ]] || { echo "Checksum mismatch; not installing." >&2; exit 1; }
tar -xzf "$WORK/mrow.tar.gz" -C "$WORK"

if [[ "$KEY" == macos-* ]]; then
  DEST="${DEST:-$HOME/Applications}"
  mkdir -p "$DEST"
  pkill -x Mrow 2>/dev/null || true
  rm -rf "$DEST/Mrow.app"
  ditto "$WORK/Mrow.app" "$DEST/Mrow.app"
  echo "Installed $DEST/Mrow.app"
  open "$DEST/Mrow.app"
else
  PREFIX="${PREFIX:-$HOME/.local}"
  mkdir -p "$PREFIX"
  cp -R "$WORK/mrow-$KEY/." "$PREFIX/"
  command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
  echo "Installed $PREFIX/bin/mrow"
fi
