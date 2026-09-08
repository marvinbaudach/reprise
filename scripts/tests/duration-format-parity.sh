#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

rust_path=crates/reprise-core/src/format.rs
kotlin_path=android/app/src/test/java/io/github/marvinbaudach/reprise/DurationFormatTest.kt
mkdir -p \
  "$fixture/scripts/lib" \
  "$fixture/$(dirname "$rust_path")" \
  "$fixture/$(dirname "$kotlin_path")"
cp "$repo_root/scripts/check-duration-format-parity.sh" "$fixture/scripts/"
cp "$repo_root/scripts/lib/source_code.py" "$fixture/scripts/lib/"
cp "$repo_root/$rust_path" "$fixture/$rust_path"
cp "$repo_root/$kotlin_path" "$fixture/$kotlin_path"

mapfile -t duplicate_lines < <(
  rg --line-number 'formatDuration\(3_753_000\)' "$fixture/$kotlin_path" \
    | cut -d: -f1
)
[[ ${#duplicate_lines[@]} -eq 2 ]] || {
  echo "duration format parity test: expected two duplicate-key assertions" >&2
  exit 1
}

sed -i \
  '0,/assertEquals("1:02:33", formatDuration(3_753_000))/s//assertEquals("1:02:34", formatDuration(3_753_000))/' \
  "$fixture/$kotlin_path"

if "$fixture/scripts/check-duration-format-parity.sh" > "$fixture/output" 2>&1; then
  echo "duration format parity test: a conflicting duplicate assertion passed" >&2
  exit 1
fi

for line in "${duplicate_lines[@]}"; do
  grep -Fq "$kotlin_path:$line" "$fixture/output"
done

echo "duration format parity: conflicting duplicate assertions name both locations"
