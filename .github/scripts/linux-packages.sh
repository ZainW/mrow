#!/usr/bin/env bash
# Installs the packages Rune needs to build on Linux, on a GitHub-hosted Ubuntu runner.
# Most of the time apt spent here went to refreshing package lists and checking for services to
# restart, not to the packages: the runner image's lists are usually current enough, so lists are
# only refreshed if the install fails without them, and needrestart is skipped.
set -euo pipefail

PACKAGES=(libxkbcommon-dev libwayland-dev pkg-config)
install() {
  sudo env NEEDRESTART_SUSPEND=1 DEBIAN_FRONTEND=noninteractive \
    apt-get install -y -qq --no-install-recommends "${PACKAGES[@]}"
}

if ! install; then
  sudo apt-get update -qq
  install
fi
