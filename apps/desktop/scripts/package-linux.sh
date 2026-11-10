#!/usr/bin/env bash
# Builds a release binary on Linux and packs it with its desktop entry and icon into
# dist/mrow-linux-<arch>.tar.gz. Nothing is uploaded anywhere.
#   scripts/package-linux.sh             # build and pack
#   scripts/package-linux.sh --install   # also install into ~/.local (PREFIX=… to change)
#
# Needs the Rune build packages (Debian/Ubuntu: libxkbcommon-dev libwayland-dev pkg-config),
# and at run time libxkbcommon-x11-0 and a Vulkan driver (mesa-vulkan-drivers).
set -euo pipefail

cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != "Linux" ]]; then
  echo "package-linux.sh runs on Linux; on macOS use scripts/docker-linux.sh" >&2
  exit 1
fi

ARCH="$(uname -m)"
NAME="mrow-linux-$ARCH"
STAGE="dist/$NAME"

cargo build --release --locked
rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/icons/hicolor/512x512/apps"
install -m 755 target/release/mrow "$STAGE/bin/mrow"
install -m 644 packaging/linux/com.zain.mrow.desktop "$STAGE/share/applications/"
install -m 644 packaging/linux/com.zain.mrow.png "$STAGE/share/icons/hicolor/512x512/apps/"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "Built dist/$NAME.tar.gz"

if [[ "${1:-}" == "--install" ]]; then
  PREFIX="${PREFIX:-$HOME/.local}"
  cp -R "$STAGE/." "$PREFIX/"
  command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
  echo "Installed $PREFIX/bin/mrow"
fi
