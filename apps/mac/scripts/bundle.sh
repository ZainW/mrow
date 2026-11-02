#!/usr/bin/env bash
# Builds "Mrow.app" into dist/, signed ad hoc. Nothing is uploaded anywhere.
#   CONFIGURATION=debug scripts/bundle.sh    # faster, unoptimised build
set -euo pipefail

cd "$(dirname "$0")/.."
CONFIGURATION="${CONFIGURATION:-release}"
APP="dist/Mrow.app"

[[ -f Resources/AppIcon.icns ]] || swift scripts/make-icon.swift Resources/AppIcon.icns
swift build -c "$CONFIGURATION" --product Mrow
BIN="$(swift build -c "$CONFIGURATION" --show-bin-path)/Mrow"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# SwiftPM records the deployment target as the SDK version, which makes macOS run the app in
# its compatibility look. Record the SDK it was actually built with, as Mdow does.
SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
xcrun vtool -set-build-version macos 15.0 "$SDK_VERSION" -replace -output "$APP/Contents/MacOS/Mrow" "$BIN"
cp Resources/Info.plist "$APP/Contents/Info.plist"
cp Resources/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
# Inter for the interface and Geist Mono for code, registered at launch (Theme.registerFonts).
ditto Resources/Fonts "$APP/Contents/Resources/Fonts"
codesign --force --timestamp=none --sign - "$APP"
echo "Built $APP"
