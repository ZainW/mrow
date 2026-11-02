#!/usr/bin/env bash
# Builds a release bundle and copies it into ~/Applications (DEST=… to change).
set -euo pipefail

cd "$(dirname "$0")/.."
DEST="${DEST:-$HOME/Applications}"
bash scripts/bundle.sh
mkdir -p "$DEST"
pkill -x Mrow 2>/dev/null || true
rm -rf "$DEST/Mrow.app"
ditto dist/Mrow.app "$DEST/Mrow.app"
echo "Installed $DEST/Mrow.app"
open "$DEST/Mrow.app"
