#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

lock_file="$tmp_root/Cargo.lock"
matching_sources="$tmp_root/cargo-sources-matching.json"
drifted_sources="$tmp_root/cargo-sources-drifted.json"

printf '%s\n' \
  'version = 4' \
  '' \
  '[[package]]' \
  'name = "rusqlite"' \
  'version = "0.40.2"' \
  'source = "registry+https://github.com/rust-lang/crates.io-index"' \
  'checksum = "23f2a97da3e3873c73cb2a2e71b35c40ff95e0b1eefa8d72d8499a6928c3b5b3"' \
  > "$lock_file"

printf '%s\n' \
  '[' \
  '  {' \
  '    "type": "archive",' \
  '    "dest": "cargo/vendor/rusqlite-0.40.2"' \
  '  }' \
  ']' \
  > "$matching_sources"

scripts/check-flatpak-cargo-sources.sh "$lock_file" "$matching_sources"

sed 's/rusqlite-0\.40\.2/rusqlite-0.40.1/' \
  "$matching_sources" > "$drifted_sources"

set +e
failure_output=$(
  scripts/check-flatpak-cargo-sources.sh "$lock_file" "$drifted_sources" 2>&1
)
failure_status=$?
set -e

[[ $failure_status -eq 1 ]] || {
  printf 'expected drifted Cargo sources to fail with exit 1, got %s\n' \
    "$failure_status" >&2
  exit 1
}
grep -Fq 'Missing from Flatpak Cargo sources:' <<< "$failure_output"
grep -Fq 'rusqlite-0.40.2' <<< "$failure_output"
grep -Fq 'Orphaned in Flatpak Cargo sources:' <<< "$failure_output"
grep -Fq 'rusqlite-0.40.1' <<< "$failure_output"
grep -Fq \
  'flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json' \
  <<< "$failure_output"

printf 'Flatpak Cargo source contracts passed\n'
