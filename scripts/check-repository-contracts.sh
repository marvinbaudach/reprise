#!/usr/bin/env bash
# Runs the repository and workflow contracts under .github/tests/.
#
# CI's base job and the local merge gate both call this script, so the two can
# no longer run different sets. While the loop sat inline in the workflow, the
# merge gate never ran it, and a broken release-workflow contract (#1311) turned
# red only after it had landed on dev.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

shopt -s nullglob
contracts=(.github/tests/*.sh)
if (( ${#contracts[@]} == 0 )); then
  echo "no repository contracts found under .github/tests/" >&2
  exit 1
fi

for contract in "${contracts[@]}"; do
  # The script self-tests already run this one, through the tail of qa-linters.sh.
  [[ $contract == .github/tests/flatpak-cargo-sources.sh ]] && continue
  echo "-- $contract"
  GITHUB_ACTIONS=false "$contract"
done
