#!/usr/bin/env bash

# PLAY-11 has two moments where an exhausted, filter-born snapshot may hand
# off to the whole library, and they are mutually exclusive per run — so each
# gets its own scenario:
#
#   * the filter is cleared while the last hit still plays  -> bound in at once
#   * the filter was cleared too early to help              -> handed off at the end
#
# Both markers are asserted positively in their own scenario and negatively in
# the other, because "the right path ran" and "the other path did not" are
# different claims and only the pair proves the trigger moved.
PLAY_11_BOUND_MARKER="library filter cleared on an exhausted queue; bound in a random library continuation"
PLAY_11_EXHAUSTED_MARKER="filtered queue exhausted after filter clear; continuing from random library snapshot"

# Waits for a log marker to show up, so the assertion that follows fails on a
# missing marker rather than on a race.
wait_for_app_log_marker() {
  local log_path=$1 marker=$2 attempts=${3:-80}

  for _ in $(seq 1 "$attempts"); do
    if rg --quiet --fixed-strings "$marker" "$log_path"; then
      return 0
    fi
    sleep 0.25
  done
  return 0
}

run_play_11_filter_clear_continuation_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/filter-clear-playback-fixture"
  local continued_path app_log

  echo "[cua-e2e] play-11: clearing the filter binds the continuation in at once"
  mkdir -p "$fixture_dir"
  for fixture in \
    "Filtered Needle|440|needle.flac" \
    "Library Alpha|550|alpha.flac" \
    "Library Beta|660|beta.flac"; do
    IFS='|' read -r title frequency filename <<<"$fixture"
    ffmpeg -hide_banner -loglevel error -y \
      -f lavfi -i "sine=frequency=$frequency:duration=12" \
      -metadata title="$title" -metadata artist="Reprise E2E" \
      -c:a flac "$fixture_dir/$filename"
  done

  start_scenario_app \
    play-11-filter-clear "$fixture_dir" "" 35
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Filtered Needle" play-11-library >/dev/null

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" play-11-search-open
  cua_type_text_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" "needle" play-11-search-type
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Library Alpha" play-11-filtered >/dev/null

  # The one hit is also the last one, so the snapshot is exhausted from the
  # first frame — the state PLAY-11's immediate binding is about.
  cua_double_click_label \
    "$APP_PID" "$WINDOW_ID" "Filtered Needle" play-11-start-filtered
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" play-11-filtered-playing >/dev/null

  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" escape play-11-search-clear
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Library Alpha" play-11-unfiltered >/dev/null

  wait_for_app_log_marker "$APP_LOG" "$PLAY_11_BOUND_MARKER"
  assert_app_log_contains \
    "$APP_LOG" "$PLAY_11_BOUND_MARKER" "play-11-filter-clear"
  continued_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" play-11-random-playing)
  assert_snapshot_contains "$continued_path" "Library Alpha"
  assert_snapshot_contains "$continued_path" "Library Beta"

  app_log=$APP_LOG
  finish_scenario play-11-filter-clear \
    "dev scan complete" \
    "queue set from view" \
    "$PLAY_11_BOUND_MARKER"

  # The fixture is 12s and the smoke timer 35s, so the title really ran out
  # inside this run: the end-of-title handoff had every chance to fire and
  # must not have, because the queue it would have rebuilt was already there.
  assert_app_log_absent \
    "$app_log" "$PLAY_11_EXHAUSTED_MARKER" "play-11-filter-clear"
}

run_play_11_late_filter_clear_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/filter-clear-late-fixture"
  local app_log

  echo "[cua-e2e] play-11: a filter cleared too early still hands off at the end"
  mkdir -p "$fixture_dir"
  for fixture in \
    "Needle One|440|needle-one.flac" \
    "Needle Two|470|needle-two.flac" \
    "Library Alpha|550|alpha.flac" \
    "Library Beta|660|beta.flac"; do
    IFS='|' read -r title frequency filename <<<"$fixture"
    ffmpeg -hide_banner -loglevel error -y \
      -f lavfi -i "sine=frequency=$frequency:duration=6" \
      -metadata title="$title" -metadata artist="Reprise E2E" \
      -c:a flac "$fixture_dir/$filename"
  done

  start_scenario_app \
    play-11-late-filter-clear "$fixture_dir" "" 45
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Needle One" play-11-late-library >/dev/null

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" play-11-late-search-open
  cua_type_text_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" "needle" play-11-late-search-type
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Library Alpha" play-11-late-filtered >/dev/null

  # Two hits, started at the first: the snapshot still has a future, so
  # clearing the filter now must leave it alone (PLAY-8) and the handoff has
  # to wait for the second hit to finish.
  cua_double_click_label \
    "$APP_PID" "$WINDOW_ID" "Needle One" play-11-late-start-filtered
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" play-11-late-playing >/dev/null

  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" escape play-11-late-search-clear
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Library Alpha" play-11-late-unfiltered >/dev/null

  wait_for_app_log_marker "$APP_LOG" "$PLAY_11_EXHAUSTED_MARKER" 120
  assert_app_log_contains \
    "$APP_LOG" "$PLAY_11_EXHAUSTED_MARKER" "play-11-late-filter-clear"

  app_log=$APP_LOG
  finish_scenario play-11-late-filter-clear \
    "dev scan complete" \
    "queue set from view" \
    "$PLAY_11_EXHAUSTED_MARKER"

  # Nothing was ever exhausted at the moment the filter went away, so the
  # immediate binding must have kept its hands off the running snapshot.
  assert_app_log_absent \
    "$app_log" "$PLAY_11_BOUND_MARKER" "play-11-late-filter-clear"
}
