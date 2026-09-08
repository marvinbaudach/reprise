#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

rust_path=crates/reprise-core/src/format.rs
kotlin_path=android/app/src/test/java/io/github/marvinbaudach/reprise/DurationFormatTest.kt
new_fixture() {
  local fixture
  fixture=$(mktemp -d)
  mkdir -p \
    "$fixture/scripts/lib" \
    "$fixture/$(dirname "$rust_path")" \
    "$fixture/$(dirname "$kotlin_path")"
  cp "$repo_root/scripts/check-duration-format-parity.sh" "$fixture/scripts/"
  cp "$repo_root/scripts/lib/source_code.py" "$fixture/scripts/lib/"
  cp "$repo_root/$rust_path" "$fixture/$rust_path"
  cp "$repo_root/$kotlin_path" "$fixture/$kotlin_path"
  printf '%s\n' "$fixture"
}

expect_conflicting_duplicate_to_fail() {
  local fixture
  local status
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  printf '%s\n' \
    'assert_eq!(format_duration(3_753_000), "1:02:33");' \
    > "$fixture/$rust_path"
  printf '%s\n' \
    'assertEquals("1:02:33", formatDuration(3_753_000))' \
    'assertEquals("1:02:33", formatDuration(3_753_000))' \
    > "$fixture/$kotlin_path"

  mapfile -t duplicate_lines < <(
    rg --line-number 'formatDuration\(3_753_000\)' "$fixture/$kotlin_path" \
      | cut -d: -f1
  )
  [[ ${#duplicate_lines[@]} -eq 2 ]] || {
    echo "duration format parity test: expected two duplicate-key assertions" >&2
    return 1
  }

  sed -i \
    '0,/assertEquals("1:02:33", formatDuration(3_753_000))/s//assertEquals("1:02:34", formatDuration(3_753_000))/' \
    "$fixture/$kotlin_path"

  set +e
  "$fixture/scripts/check-duration-format-parity.sh" > "$fixture/output" 2>&1
  status=$?
  set -e
  if (( status == 0 )); then
    echo "duration format parity test: a conflicting duplicate assertion passed" >&2
    return 1
  fi

  for line in "${duplicate_lines[@]}"; do
    grep -Fq "$kotlin_path:$line" "$fixture/output"
  done
  printf \
    'duration format parity: conflicting-duplicate gate exit: %s; locations: %s:%s, %s:%s\n' \
    "$status" "$kotlin_path" "${duplicate_lines[0]}" \
    "$kotlin_path" "${duplicate_lines[1]}"
}

expect_computed_value_to_fail_actionably() {
  local fixture
  local status
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  sed -i 's/format_duration(181_000)/format_duration(181_000 + OFFSET)/' \
    "$fixture/$rust_path"
  set +e
  "$fixture/scripts/check-duration-format-parity.sh" > "$fixture/output" 2>&1
  status=$?
  set -e
  if (( status == 0 )); then
    echo "duration format parity test: a computed millisecond value passed" >&2
    return 1
  fi
  grep -Fq "an assertion uses a computed millisecond value" "$fixture/output"
  grep -Fq "$rust_path:" "$fixture/output"
  grep -Fq 'cannot evaluate `181_000 + OFFSET`' "$fixture/output"
  printf 'duration format parity: computed-value gate exit: %s\n' "$status"
}

expect_commented_assertion_to_pass() {
  local fixture
  local status
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  printf '%s\n' \
    '// assert_eq!(format_duration(123_456), "2:03");' \
    >> "$fixture/$rust_path"
  set +e
  "$fixture/scripts/check-duration-format-parity.sh" > "$fixture/output" 2>&1
  status=$?
  set -e
  if (( status != 0 )); then
    cat "$fixture/output" >&2
    echo "duration format parity test: a whole-line comment became a live assertion" >&2
    return 1
  fi
  printf 'duration format parity: whole-line-comment gate exit: %s\n' "$status"
}

expect_conflicting_duplicate_to_fail
expect_computed_value_to_fail_actionably
expect_commented_assertion_to_pass

echo "duration format parity: live assertions are checked and whole-line comments are ignored"
