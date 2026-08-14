#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
checker="$repo_root/scripts/check-android-suite.sh"

if [[ ! -x "$checker" ]]; then
  echo "Android suite checker is not executable: $checker" >&2
  exit 1
fi

fixture_parent=${ANDROID_SUITE_TEST_TMPDIR:-${TMPDIR:-/tmp}}
mkdir -p "$fixture_parent"
fixture_root=$(mktemp -d "$fixture_parent/android-suite-parser.XXXXXX")
trap 'find "$fixture_root" -xdev -depth -delete' EXIT

write_suite() {
  local path=$1
  local tests=$2
  local failures=$3
  local errors=$4
  local skips=$5
  local modified_at=$6

  printf '%s\n' \
    '<?xml version="1.0" encoding="UTF-8"?>' \
    "<testsuite name=\"fixture\" tests=\"$tests\" failures=\"$failures\" errors=\"$errors\" skipped=\"$skips\" time=\"0.1\">" \
    '</testsuite>' >"$path"
  touch -d "@$modified_at" "$path"
}

assert_parse() {
  local expected_status=$1
  local expected_output=$2
  local start_time=$3
  local results_dir=$4
  local output
  local status

  set +e
  output=$(bash "$checker" --parse-results "$start_time" "$results_dir" 2>&1)
  status=$?
  set -e

  if (( status != expected_status )); then
    echo "Expected parser status $expected_status, got $status: $output" >&2
    exit 1
  fi
  if [[ "$output" != "$expected_output" ]]; then
    echo "Unexpected parser output: $output" >&2
    echo "Expected: $expected_output" >&2
    exit 1
  fi
}

assert_decision() {
  local expected_status=$1
  local expected_output=$2
  local summary=$3
  local output
  local status

  set +e
  output=$(bash "$checker" --decide "$summary" 2>&1)
  status=$?
  set -e

  if (( status != expected_status )); then
    echo "Expected decision status $expected_status, got $status: $output" >&2
    exit 1
  fi
  if [[ "$output" != "$expected_output" ]]; then
    echo "Unexpected decision output: $output" >&2
    echo "Expected: $expected_output" >&2
    exit 1
  fi
}

fresh_dir="$fixture_root/fresh"
mkdir -p "$fresh_dir"
write_suite "$fresh_dir/TEST-first.xml" 2 0 0 1 1000
write_suite "$fresh_dir/TEST-second.xml" 3 0 0 0 1001
assert_parse 0 \
  'suites=2 tests=5 failures=0 errors=0 skips=1 verdict=fresh' \
  1000 "$fresh_dir"

stale_dir="$fixture_root/stale"
mkdir -p "$stale_dir"
write_suite "$stale_dir/TEST-fresh.xml" 4 0 0 0 1001
write_suite "$stale_dir/TEST-stale.xml" 2 1 0 0 999
assert_parse 2 \
  'suites=2 tests=6 failures=1 errors=0 skips=0 verdict=stale' \
  1000 "$stale_dir"

empty_dir="$fixture_root/empty"
mkdir -p "$empty_dir"
assert_parse 3 \
  'suites=0 tests=0 failures=0 errors=0 skips=0 verdict=empty' \
  1000 "$empty_dir"

assert_parse 4 \
  'suites=0 tests=0 failures=0 errors=0 skips=0 verdict=missing' \
  1000 "$fixture_root/missing"

assert_decision 0 \
  'Android unit-suite gate passed' \
  'suites=66 tests=334 failures=0 errors=0 skips=0 verdict=fresh'
assert_decision 1 \
  'Android test floor missed: executed 0, required at least 334 (measured at dd67122fc7)' \
  'suites=66 tests=334 failures=0 errors=0 skips=334 verdict=fresh'
assert_decision 1 \
  'Android test floor missed: executed 333, required at least 334 (measured at dd67122fc7)' \
  'suites=66 tests=333 failures=0 errors=0 skips=0 verdict=fresh'
assert_decision 1 \
  'Android unit suite failed: 1 failures and 0 errors' \
  'suites=66 tests=334 failures=1 errors=0 skips=0 verdict=fresh'
assert_decision 1 \
  'Android unit suite failed: 0 failures and 1 errors' \
  'suites=66 tests=334 failures=0 errors=1 skips=0 verdict=fresh'

echo "Android suite parser and decision tests passed"
