#!/usr/bin/env bash

# Shared, display-free helpers for the pointer harness and its self-test.

ANSI_STRIP_RE='s/\x1b\[[0-9;]*[a-zA-Z]//g'

expected_flow_count() {
  if [ "${PTR_E2E_NEWS_ONLY:-0}" = "1" ] \
    || [ "${PTR_E2E_PREFERENCES_ONLY:-0}" = "1" ] \
    || [ "${PTR_E2E_COMPACT_SEEK_ONLY:-0}" = "1" ] \
    || [ "${PTR_E2E_SEARCH_CHIP_ONLY:-0}" = "1" ] \
    || [ "${PTR_E2E_COLREORDER_ONLY:-0}" = "1" ]; then
    printf '1\n'
  elif [ "${PTR_E2E_HEADER_ONLY:-0}" = "1" ]; then
    printf '2\n'
  elif [ "${PTR_E2E_PLAYLIST_DELETE_ONLY:-0}" = "1" ]; then
    printf '3\n'
  else
    printf '9\n'
  fi
}

start_flow() {
  local description="$1"
  printf '%s\n' "$description" >> "$FLOW_LOG"
  log_step "flow $description"
}

flow_started_count() {
  wc -l < "$FLOW_LOG" 2>/dev/null || echo 0
}

harness_effective_exit_code() {
  local exit_code="$1" failures="$2" mismatch="$3"
  local flows_started="$4" flows_expected="$5"
  if [ "$exit_code" -ne 0 ]; then
    printf '%s\n' "$exit_code"
  elif [ "$failures" -gt 0 ] || [ "$mismatch" -ne 0 ] \
    || [ "$flows_started" -lt "$flows_expected" ]; then
    printf '1\n'
  else
    printf '0\n'
  fi
}

harness_balance_message() {
  local effective_exit_code="$1" failures="$2"
  local flows_started="$3" flows_expected="$4"
  if [ "$effective_exit_code" -eq 0 ]; then
    printf 'done — all checks passed (%s of %s flows ran)\n' \
      "$flows_started" "$flows_expected"
  elif [ "$flows_started" -lt "$flows_expected" ]; then
    printf 'done — incomplete run (exit %s, %s failed check(s), %s of %s flows ran)\n' \
      "$effective_exit_code" "$failures" "$flows_started" "$flows_expected"
  else
    printf 'done — see failures above (exit %s, %s failed check(s), %s of %s flows ran)\n' \
      "$effective_exit_code" "$failures" "$flows_started" "$flows_expected"
  fi
}

assert_log_sequence_since() {
  local since_line="$1" description="$2"
  shift 2
  local plain remaining pattern match_line all_found
  local attempts="${PTR_E2E_LOG_SEQUENCE_ATTEMPTS:-20}"
  local interval="${PTR_E2E_LOG_SEQUENCE_INTERVAL:-0.05}"
  for _ in $(seq 1 "$attempts"); do
    plain="$(tail -n "+$((since_line + 1))" "$APP_LOG" | sed -E "$ANSI_STRIP_RE")"
    remaining="$plain"
    all_found=1
    for pattern in "$@"; do
      if ! match_line="$(grep -Ein -m1 -- "$pattern" <<<"$remaining" | cut -d: -f1)"; then
        all_found=0
        break
      fi
      remaining="$(tail -n "+$((match_line + 1))" <<<"$remaining")"
    done
    if [ "$all_found" -eq 1 ]; then
      log_step "log sequence OK: $description"
      return 0
    fi
    sleep "$interval"
  done
  log_fail "log never showed in order: $description"
  # Assertion helpers report through the failure ledger. They deliberately
  # return success so a plain call under `set -e` cannot truncate the suite.
  return 0
}

assert_manual_queue_consumption_since() {
  local since_line="$1" track_id_x="$2" track_id_y="$3"
  # Both lines are driven by explicit `Next` calls against frozen playback, so
  # they arrive within a few hundred milliseconds. The budget stays generous
  # anyway: under load a single line has taken over a second to reach the log,
  # and this check has already been flaky once for exactly that reason (green in
  # runs 9 and 10, red in 11). The wait grows, the assertion does not shrink.
  PTR_E2E_LOG_SEQUENCE_ATTEMPTS=60 \
    assert_log_sequence_since "$since_line" \
    "manual tracks X and Y were consumed in their queued order" \
    "playback started.*track_id=$track_id_x.*from_up_next=true" \
    "playback started.*track_id=$track_id_y.*from_up_next=true"
  assert_log_sequence_since "$since_line" \
    "manual playback emptied Up Next" \
    "up next changed.*up_next_len=0"
}
