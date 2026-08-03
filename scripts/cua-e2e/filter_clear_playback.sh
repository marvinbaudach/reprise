#!/usr/bin/env bash

run_play_11_filter_clear_continuation_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/filter-clear-playback-fixture"
  local continued_path

  echo "[cua-e2e] play-11: cleared Music filter continues from random library"
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
  cua_double_click_label \
    "$APP_PID" "$WINDOW_ID" "Filtered Needle" play-11-start-filtered
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" play-11-filtered-playing >/dev/null

  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" escape play-11-search-clear
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Library Alpha" play-11-unfiltered >/dev/null

  for _ in $(seq 1 80); do
    if rg --quiet --fixed-strings \
      "filtered queue exhausted after filter clear; continuing from random library snapshot" \
      "$APP_LOG"; then
      break
    fi
    sleep 0.25
  done
  assert_app_log_contains \
    "$APP_LOG" \
    "filtered queue exhausted after filter clear; continuing from random library snapshot" \
    "play-11-filter-clear"
  continued_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" play-11-random-playing)
  assert_snapshot_contains "$continued_path" "Library Alpha"
  assert_snapshot_contains "$continued_path" "Library Beta"

  finish_scenario play-11-filter-clear \
    "dev scan complete" \
    "queue set from view" \
    "filtered queue exhausted after filter clear; continuing from random library snapshot"
}
