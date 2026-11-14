#!/usr/bin/env bash
# Wraps dist/Mrow.app (from scripts/bundle.sh) in a disk image: the app beside a link to
# /Applications, to drag across. Nothing is uploaded anywhere.
#   scripts/dmg.sh [out.dmg]         # default dist/mrow-macos-aarch64.dmg
#   APP=path/to/Mrow.app             # wrap another copy of the app
#   MROW_SIGN_IDENTITY="Developer ID Application: …"
#                                    # sign the image too (the app should be signed with the same)
#   APPLE_ID=… APPLE_APP_SPECIFIC_PASSWORD=… APPLE_TEAM_ID=…
#                                    # and have Apple notarize $APP and then the image, stapling each
#                                    # ticket, so Gatekeeper opens both offline
set -euo pipefail

cd "$(dirname "$0")/.."
APP="${APP:-dist/Mrow.app}"
OUT="${1:-dist/mrow-macos-aarch64.dmg}"
[[ -d "$APP" ]] || { echo "No $APP; run scripts/bundle.sh first." >&2; exit 1; }

STAGE="$(mktemp -d)"
MOUNT="$(mktemp -d)"
WORK="$(mktemp -d)"
trap 'hdiutil detach -quiet "$MOUNT" 2>/dev/null || true; rm -rf "$STAGE" "$MOUNT" "$WORK"' EXIT

# Sends a file to Apple and waits; stops with Apple's log (which names the file and the rule it
# broke) if Apple doesn't accept it.
notarize() {
  local result
  result="$(xcrun notarytool submit "$1" --apple-id "$APPLE_ID" --password "$APPLE_APP_SPECIFIC_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" --wait --timeout 30m --output-format json)"
  if [[ "$(plutil -extract status raw -o - - <<<"$result")" != "Accepted" ]]; then
    echo "Apple didn't accept $1: $result" >&2
    xcrun notarytool log "$(plutil -extract id raw -o - - <<<"$result")" --apple-id "$APPLE_ID" \
      --password "$APPLE_APP_SPECIFIC_PASSWORD" --team-id "$APPLE_TEAM_ID" >&2 || true
    exit 1
  fi
}

if [[ -n "${APPLE_ID:-}" ]]; then
  : "${APPLE_APP_SPECIFIC_PASSWORD:?needed to notarize}" "${APPLE_TEAM_ID:?needed to notarize}"
  # The app first, so it carries its own ticket: in the image, in the archive made from $APP,
  # and wherever it's copied, it opens without asking Apple online.
  ditto -c -k --keepParent "$APP" "$WORK/Mrow.zip"
  notarize "$WORK/Mrow.zip"
  xcrun stapler staple "$APP"
  spctl --assess --type execute -v "$APP"
fi

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

if [[ -n "${MROW_SIGN_IDENTITY:-}" ]]; then
  codesign --force --timestamp --sign "$MROW_SIGN_IDENTITY" "$OUT"
fi
if [[ -n "${APPLE_ID:-}" ]]; then
  notarize "$OUT"
  xcrun stapler staple "$OUT"
  spctl --assess --type open --context context:primary-signature -v "$OUT"
fi

# The image opens, and the app inside still carries its signature (and its ticket, if notarized).
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MOUNT" "$OUT"
codesign --verify --strict "$MOUNT/Mrow.app"
[[ -z "${APPLE_ID:-}" ]] || xcrun stapler validate -q "$MOUNT/Mrow.app"
test -L "$MOUNT/Applications"
echo "Built $OUT ($("$MOUNT/Mrow.app/Contents/MacOS/Mrow" --version))"
