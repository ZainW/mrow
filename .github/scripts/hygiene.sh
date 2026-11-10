#!/usr/bin/env bash
# Catches what shouldn't land in a public repo, in what changed since <base>, or in every
# tracked file without one:
#   .github/scripts/hygiene.sh origin/main
# - recordings and screenshots of real work queues (confidential)
# - images outside the places that hold the app's and site's own assets
# - credentials and private keys
# - files over 2 MB
# and checks the update key is still a 32-byte hex key.
set -euo pipefail

BASE="${1:-}"
cd "$(git rev-parse --show-toplevel)"
fail=0
problem() {
  echo "::error file=$1::$2"
  fail=1
}

# Plain read loops rather than mapfile, so macOS's bash 3 runs it too.
added=()
changed=()
if [[ -n "$BASE" ]] && git rev-parse -q --verify "$BASE^{commit}" >/dev/null; then
  while IFS= read -r f; do added+=("$f"); done < <(git diff --name-only --diff-filter=AR "$BASE"...HEAD)
  while IFS= read -r f; do changed+=("$f"); done < <(git diff --name-only --diff-filter=ACMR "$BASE"...HEAD)
else
  while IFS= read -r f; do added+=("$f"); done < <(git ls-files)
  changed=(${added[@]+"${added[@]}"})
fi

for f in ${added[@]+"${added[@]}"}; do
  case "$f" in
    apps/desktop/screenshots/* | apps/desktop/bench/* | */results-*.json)
      problem "$f" "Screenshots and bench recordings come from real work queues; keep them out of the repo." ;;
    apps/desktop/assets/* | apps/desktop/Resources/* | apps/desktop/packaging/* | apps/web/public/*) ;;
    *.png | *.jpg | *.jpeg | *.gif | *.webp | *.mov | *.mp4)
      problem "$f" "Images belong in apps/desktop/assets, Resources or packaging, or apps/web/public, made from fictional data." ;;
  esac
  if [[ -f "$f" ]] && (( $(wc -c <"$f") > 2000000 )); then
    problem "$f" "Over 2 MB."
  fi
done

secrets='(glpat-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{40,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)'
for f in ${changed[@]+"${changed[@]}"}; do
  [[ -f "$f" ]] || continue
  if grep -IEnq "$secrets" "$f"; then
    problem "$f" "Looks like a credential or private key."
  fi
done

if ! grep -Eqx '[0-9a-f]{64}' apps/desktop/packaging/update-key.hex; then
  problem apps/desktop/packaging/update-key.hex "The update key must be 64 lowercase hex characters (an Ed25519 public key)."
fi

if (( fail )); then
  exit 1
fi
echo "Clean: ${#changed[@]} changed files."
