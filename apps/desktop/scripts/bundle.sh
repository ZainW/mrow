#!/usr/bin/env bash
# Builds "Mrow.app" into dist/, signed ad hoc. Nothing is uploaded anywhere.
#   PROFILE=dev scripts/bundle.sh    # faster, unoptimised build
#   MROW_UPDATES=1                   # set by the release workflow: the app updates itself
set -euo pipefail

cd "$(dirname "$0")/.."
PROFILE="${PROFILE:-release}"
APP="dist/Mrow.app"

cargo build --locked --profile "$PROFILE"
DIR="$PROFILE"
[[ "$PROFILE" == "dev" ]] && DIR="debug"
BIN="target/$DIR/mrow"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# Record the SDK it was built with, so macOS doesn't run it in a compatibility mode.
SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
xcrun vtool -set-build-version macos 15.0 "$SDK_VERSION" -replace -output "$APP/Contents/MacOS/Mrow" "$BIN"
cp Resources/Info.plist "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist"
cp Resources/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
codesign --force --timestamp=none --sign - "$APP"
echo "Built $APP ($VERSION)"
