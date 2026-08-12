#!/usr/bin/env bash

# Shared, display-free helpers for the pointer harness and its self-test.

ANSI_STRIP_RE='s/\x1b\[[0-9;]*[a-zA-Z]//g'

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
