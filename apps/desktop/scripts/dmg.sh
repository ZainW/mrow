#!/usr/bin/env bash
# Wraps dist/Mrow.app (from scripts/bundle.sh) in a disk image: the app beside a link to
# /Applications, to drag across. Nothing is uploaded anywhere.
#   scripts/dmg.sh [out.dmg]         # default dist/mrow-macos-aarch64.dmg
#   APP=path/to/Mrow.app             # wrap another copy of the app
set -euo pipefail

cd "$(dirname "$0")/.."
APP="${APP:-dist/Mrow.app}"
OUT="${1:-dist/mrow-macos-aarch64.dmg}"
[[ -d "$APP" ]] || { echo "No $APP; run scripts/bundle.sh first." >&2; exit 1; }

STAGE="$(mktemp -d)"
MOUNT="$(mktemp -d)"
trap 'hdiutil detach -quiet "$MOUNT" 2>/dev/null || true; rm -rf "$STAGE" "$MOUNT"' EXIT
ditto "$APP" "$STAGE/Mrow.app"
ln -s /Applications "$STAGE/Applications"

mkdir -p "$(dirname "$OUT")"
# hdiutil now and then fails with "Resource busy" on a busy machine (CI runners especially).
for attempt in 1 2 3; do
  if hdiutil create -quiet -ov -volname Mrow -fs HFS+ -format ULMO -srcfolder "$STAGE" "$OUT"; then
    break
  fi
  [[ "$attempt" -lt 3 ]] || { echo "hdiutil couldn't make $OUT" >&2; exit 1; }
  sleep 5
done

# The image opens, and the app inside still carries its signature.
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MOUNT" "$OUT"
codesign --verify --strict "$MOUNT/Mrow.app"
test -L "$MOUNT/Applications"
echo "Built $OUT ($("$MOUNT/Mrow.app/Contents/MacOS/Mrow" --version))"
