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
FLOW_LOG="$TEST_ROOT/flows.log"
: > "$APP_LOG"
: > "$FAILURE_LOG"
: > "$HARNESS_LOG"
: > "$FLOW_LOG"

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

PTR_E2E_NEWS_ONLY=0
PTR_E2E_PREFERENCES_ONLY=0
PTR_E2E_COMPACT_SEEK_ONLY=0
PTR_E2E_SEARCH_CHIP_ONLY=0
PTR_E2E_COLREORDER_ONLY=0
PTR_E2E_HEADER_ONLY=0
PTR_E2E_PLAYLIST_DELETE_ONLY=0
assert_equal 9 "$(expected_flow_count)" \
  "the full suite declares all nine flows"
PTR_E2E_HEADER_ONLY=1
assert_equal 2 "$(expected_flow_count)" \
  "the header-only route declares its two prerequisite flows"
PTR_E2E_HEADER_ONLY=0
PTR_E2E_PLAYLIST_DELETE_ONLY=1
assert_equal 3 "$(expected_flow_count)" \
  "the playlist-delete route declares its three prerequisite flows"
PTR_E2E_PLAYLIST_DELETE_ONLY=0
PTR_E2E_PREFERENCES_ONLY=1
assert_equal 1 "$(expected_flow_count)" \
  "a dedicated route declares one flow"
PTR_E2E_PREFERENCES_ONLY=0

start_flow "one"
start_flow "two"
start_flow "three"
start_flow "four"
assert_equal 4 "$(flow_started_count)" \
  "flow accounting records every started flow"
assert_equal 1 "$(harness_effective_exit_code 0 0 0 4 9)" \
  "an incomplete run fails without a failed assertion"
assert_equal 0 "$(harness_effective_exit_code 0 0 0 9 9)" \
  "complete clean flow coverage passes"
assert_equal \
  "done — incomplete run (exit 1, 0 failed check(s), 4 of 9 flows ran)" \
  "$(harness_balance_message 1 0 4 9)" \
  "the closing balance exposes incomplete coverage"

: > "$FAILURE_LOG"
printf '%s\n' \
  'up next changed up_next_len=2' \
  'playback started track_id=4 gapless=true from_up_next=true' \
  'up next changed up_next_len=0' \
  'playback started track_id=5 gapless=false from_up_next=true' \
  > "$APP_LOG"
assert_manual_queue_consumption_since 0 4 5
assert_equal 0 "$(wc -l < "$FAILURE_LOG")" \
  "manual queue playback accepts 2 to 0 while preserving X before Y"

: > "$FAILURE_LOG"
printf '%s\n' \
  'up next changed up_next_len=0' \
  'playback started track_id=5 gapless=true from_up_next=true' \
  'playback started track_id=4 gapless=false from_up_next=true' \
  > "$APP_LOG"
assert_manual_queue_consumption_since 0 4 5
assert_equal 1 "$(wc -l < "$FAILURE_LOG")" \
  "manual queue playback still rejects Y before X"

PREFERENCES_FLOW_SOURCE="$(sed -n \
  '/^run_preferences_flow() {$/,/^}$/p' \
  "$REPO_ROOT/scripts/ptr-e2e/preferences.sh")"
assert_equal 0 "$(grep -Fc 'key "Home"' <<<"$PREFERENCES_FLOW_SOURCE" || true)" \
  "Preferences navigation does not spend a menu step on Home"
assert_equal 4 "$(grep -Fc 'key "Down"' <<<"$PREFERENCES_FLOW_SOURCE" || true)" \
  "Preferences navigation takes four Down steps from Compact Mode"
assert_equal 1 "$(grep -Fc 'screenshot "17-main-menu-closed"' \
  <<<"$PREFERENCES_FLOW_SOURCE" || true)" \
  "Preferences captures the main menu before F10 opens it"
assert_equal 1 "$(grep -Fc 'assert_screenshots_differ \' \
  <<<"$PREFERENCES_FLOW_SOURCE" || true)" \
  "Preferences proves the focused-menu capture differs from the closed menu"
assert_equal 1 "$(grep -Fc '17-main-menu-closed.png' \
  <<<"$PREFERENCES_FLOW_SOURCE" || true)" \
  "Preferences uses the closed-menu capture as the comparison baseline"
assert_equal 1 "$(grep -Fc '17-main-menu-preferences-focused.png' \
  <<<"$PREFERENCES_FLOW_SOURCE" || true)" \
  "Preferences compares the focused-menu capture"
PREFERENCES_CLOSED_LINE="$(grep -nF 'screenshot "17-main-menu-closed"' \
  <<<"$PREFERENCES_FLOW_SOURCE" | cut -d: -f1)"
PREFERENCES_F10_LINE="$(grep -nF 'key "F10"' \
  <<<"$PREFERENCES_FLOW_SOURCE" | cut -d: -f1)"
PREFERENCES_FOCUSED_LINE="$(grep -nF 'screenshot "17-main-menu-preferences-focused"' \
  <<<"$PREFERENCES_FLOW_SOURCE" | cut -d: -f1)"
PREFERENCES_COMPARE_LINE="$(grep -nF 'assert_screenshots_differ \' \
  <<<"$PREFERENCES_FLOW_SOURCE" | cut -d: -f1)"
PREFERENCES_RETURN_LINE="$(grep -nF 'key "Return"' \
  <<<"$PREFERENCES_FLOW_SOURCE" | cut -d: -f1)"
PREFERENCES_FLOW_ORDER_OK=0
if [ "$PREFERENCES_CLOSED_LINE" -lt "$PREFERENCES_F10_LINE" ] \
  && [ "$PREFERENCES_F10_LINE" -lt "$PREFERENCES_FOCUSED_LINE" ] \
  && [ "$PREFERENCES_FOCUSED_LINE" -lt "$PREFERENCES_COMPARE_LINE" ] \
  && [ "$PREFERENCES_COMPARE_LINE" -lt "$PREFERENCES_RETURN_LINE" ]; then
  PREFERENCES_FLOW_ORDER_OK=1
fi
assert_equal 1 "$PREFERENCES_FLOW_ORDER_OK" \
  "Preferences proves the menu opened before activating its focused item"

printf 'harness self-test passed\n'
