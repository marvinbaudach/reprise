#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

if output=$(scripts/performance-runtime-baseline.sh 2>&1); then
  echo "runtime performance baseline must require an explicit output directory" >&2
  exit 1
fi
if [[ $output != *"usage: scripts/performance-runtime-baseline.sh OUTPUT_DIR [--quick]"* ]]; then
  echo "missing runtime performance usage text" >&2
  exit 1
fi

help=$(scripts/performance-runtime-baseline.sh --help)
for required in "installed app" "10,000" "100,000" "GTK" "queue memory" "scroll"; do
  if [[ $help != *"$required"* ]]; then
    echo "runtime performance help must mention: $required" >&2
    exit 1
  fi
done

existing_output=$(mktemp -d /tmp/reprise-runtime-performance-existing.XXXXXX)
trap 'rmdir "$existing_output"' EXIT
if output=$(scripts/performance-runtime-baseline.sh "$existing_output" --quick 2>&1); then
  echo "runtime performance baseline must not overwrite an existing output directory" >&2
  exit 1
fi
if [[ $output != *"output directory already exists"* ]]; then
  echo "existing runtime output rejection must explain the conflict" >&2
  exit 1
fi

echo "Runtime performance baseline CLI checks passed"
