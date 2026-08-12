#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# shellcheck source=harness-helpers.sh
source "$REPO_ROOT/scripts/ptr-e2e/harness-helpers.sh"

TEST_ROOT="$(mktemp -d /tmp/reprise-ptr-e2e-self-test.XXXXXX)"
trap 'rm -rf -- "$TEST_ROOT"' EXIT

APP_LOG="$TEST_ROOT/app.log"
FAILURE_LOG="$TEST_ROOT/failures.log"
HARNESS_LOG="$TEST_ROOT/run.log"
: > "$APP_LOG"
: > "$FAILURE_LOG"
: > "$HARNESS_LOG"

log_step() {
  printf '[ptr-e2e] %s\n' "$*" >> "$HARNESS_LOG"
}

log_fail() {
  printf '%s\n' "$*" >> "$FAILURE_LOG"
  printf '[ptr-e2e] FAIL: %s\n' "$*" >> "$HARNESS_LOG"
}

assert_equal() {
  local expected="$1" actual="$2" description="$3"
  if [ "$actual" != "$expected" ]; then
    printf 'FAIL: %s (expected %s, got %s)\n' \
      "$description" "$expected" "$actual" >&2
    exit 1
  fi
}

PTR_E2E_LOG_SEQUENCE_ATTEMPTS=1
PTR_E2E_LOG_SEQUENCE_INTERVAL=0

# A missing ordered sequence is one failed check, not a non-zero helper result
# that aborts its plain call site under `set -e`.
assert_log_sequence_since 0 "missing sequence" "first event" "second event"
assert_equal 1 "$(wc -l < "$FAILURE_LOG")" \
  "a missing sequence records exactly one failure"

printf 'harness self-test passed\n'
