#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

if output=$(scripts/performance-baseline.sh 2>&1); then
  echo "performance baseline must require an explicit output directory" >&2
  exit 1
fi
if [[ $output != *"usage: scripts/performance-baseline.sh OUTPUT_DIR [--quick]"* ]]; then
  echo "missing usage text for an absent output directory" >&2
  exit 1
fi

help=$(scripts/performance-baseline.sh --help)
for required in "10,000" "100,000" "--quick" "generated metadata" "committed writes"; do
  if [[ $help != *"$required"* ]]; then
    echo "performance baseline help must mention: $required" >&2
    exit 1
  fi
done

existing_output=$(mktemp -d /tmp/reprise-performance-existing.XXXXXX)
trap 'rmdir "$existing_output"' EXIT
if output=$(scripts/performance-baseline.sh "$existing_output" --quick 2>&1); then
  echo "performance baseline must not overwrite an existing output directory" >&2
  exit 1
fi
if [[ $output != *"output directory already exists"* ]]; then
  echo "existing output directory rejection must explain the conflict" >&2
  exit 1
fi

echo "Performance baseline CLI checks passed"
