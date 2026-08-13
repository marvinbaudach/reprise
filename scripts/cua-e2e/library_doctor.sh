#!/usr/bin/env bash

# Library Doctor's end-to-end scan, review, apply, reopen, and revert workflow.
# Sourced by run.sh after the shared CUA and app-lifecycle helpers.

run_library_doctor_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/library-doctor-fixture-music"
  local fixture_count=24 safe_change_count root_path review_path narrow_path applied_path

  # Each fixture produces one whitespace, missing-album-artist, and genre fix.
  safe_change_count=$((fixture_count * 3))

  echo "[cua-e2e] library-doctor: scan -> review -> apply -> reopen -> revert"
  mkdir -p "$fixture_dir"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=2 \
    -metadata title=" Doctor Track " \
    -metadata artist="Doctor Artist" \
    -metadata album="Doctor Album" \
    -metadata genre=" rock " \
    -c:a flac "$fixture_dir/doctor_01.flac"
  for number in $(seq 2 "$fixture_count"); do
    cp "$fixture_dir/doctor_01.flac" \
      "$fixture_dir/doctor_$(printf '%02d' "$number").flac"
  done

  start_scenario_app \
    library-doctor "$fixture_dir" "" "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  wait_for_label "$APP_PID" "$WINDOW_ID" "Search all fields" doctor-library >/dev/null

  cua_activate_main_menu_item \
    "$APP_PID" "$WINDOW_ID" "Library Doctor" doctor-entry
  wait_for_label "$APP_PID" "$WINDOW_ID" "Run Scan Now" doctor-page-ready >/dev/null

  echo "[cua-e2e] browse-3-sidebar-escapes-doctor: active Music is an absolute target"
  # cua-driver currently reports every descendant of this nested utility
  # page at the native window's screen origin (200,50). The runner fixes the
  # window at 1200x800, so the Music-row centre at local screenshot point
  # (110,115) is deterministic.
  cua_click_window_point \
    "$APP_PID" "$WINDOW_ID" 110 115 browse-3-sidebar-escapes-doctor
  root_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" browse-3-library-restored)
  assert_snapshot_absent "$root_path" "Run Scan Now"
  cua_activate_main_menu_item \
    "$APP_PID" "$WINDOW_ID" "Library Doctor" browse-3-doctor-reopen
  wait_for_label "$APP_PID" "$WINDOW_ID" "Run Scan Now" doctor-run-reopened >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Run Scan Now" doctor-run

  root_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Review $safe_change_count safe fixes" doctor-results)
  assert_snapshot_contains "$root_path" "Casing / Whitespace"
  assert_snapshot_contains "$root_path" "Missing Album Artist"
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Review $safe_change_count safe fixes" doctor-review

  review_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Apply $fixture_count tracks" doctor-review-wide)
  assert_snapshot_contains "$review_path" "Current"
  assert_snapshot_contains "$review_path" "Proposed"
  assert_snapshot_contains "$review_path" "Source"
  assert_snapshot_contains "$review_path" "Local"
  cua_resize_window "$APP_PID" "$WINDOW_ID" 620 760 doctor-review-narrow
  narrow_path="$CUA_E2E_OUT_DIR/doctor-review-narrow-after-resize.json"
  assert_snapshot_contains "$narrow_path" "Apply $fixture_count tracks"
  assert_snapshot_contains "$narrow_path" "Current"
  assert_snapshot_contains "$narrow_path" "Proposed"
  cua_resize_window "$APP_PID" "$WINDOW_ID" 1200 760 doctor-review-wide-again

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Apply $fixture_count tracks" doctor-apply
  applied_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Tags updated · $fixture_count tracks" doctor-applied)
  assert_snapshot_contains "$applied_path" "Revert"

  cua_activate_main_menu_item \
    "$APP_PID" "$WINDOW_ID" "Preferences" doctor-preferences
  wait_for_label "$APP_PID" "$WINDOW_ID" "Plugins" doctor-preferences-open >/dev/null
  cua_focus_label_via_key "$APP_PID" "$WINDOW_ID" "Plugins" down doctor-plugins-focus \
    >/dev/null
  cua_press_key_window "$APP_PID" "$WINDOW_ID" enter doctor-plugins-enter
  wait_for_label "$APP_PID" "$WINDOW_ID" "Run Scan Now" doctor-plugin-tool >/dev/null
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Enable Library Doctor" doctor-plugin-no-toggle >/dev/null
  cua_hotkey "$APP_PID" "$WINDOW_ID" doctor-tool-close ctrl w
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Preferences" doctor-tool-close-complete >/dev/null
  cua_activate_main_menu_item \
    "$APP_PID" "$WINDOW_ID" "Library Doctor" doctor-tool-entry
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Revert Last Cleanup" doctor-revert-available >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Revert Last Cleanup" doctor-revert
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Tags reverted · $fixture_count tracks" doctor-reverted \
    >/dev/null

  finish_scenario library-doctor \
    "dev scan complete" \
    "Library Doctor write completed"
}
