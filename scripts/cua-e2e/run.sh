#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

# shellcheck source=lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"
# shellcheck source=../cua-common/session.sh
source "$repo_root/scripts/cua-common/session.sh"
# shellcheck source=track_sort.sh
source "$repo_root/scripts/cua-e2e/track_sort.sh"
# shellcheck source=tag_autocomplete.sh
source "$repo_root/scripts/cua-e2e/tag_autocomplete.sh"
# shellcheck source=scrobbling.sh
source "$repo_root/scripts/cua-e2e/scrobbling.sh"
# shellcheck source=responsive_window.sh
source "$repo_root/scripts/cua-e2e/responsive_window.sh"
# What the sources actually do, offline via fixtures.
source "$repo_root/scripts/cua-e2e/source_content.sh"
source "$repo_root/scripts/cua-e2e/podcast_backlog.sh"
source "$repo_root/scripts/cua-e2e/filter_clear_playback.sh"
APP_ID=io.github.marvinbaudach.Reprise
WINDOW_CLASS_MATCH=reprise
CUA_E2E_PROFILE="${CUA_E2E_PROFILE:-debug}"
CUA_E2E_OUT_DIR="${CUA_E2E_OUT_DIR:-/tmp/reprise-cua-e2e}"
CUA_E2E_SCREEN_RES="${CUA_E2E_SCREEN_RES:-1600x900x24}"
CUA_E2E_QUIT_DELAY_SECS="${CUA_E2E_QUIT_DELAY_SECS:-15}"
CUA_E2E_KEYBOARD_QUIT_DELAY_SECS="${CUA_E2E_KEYBOARD_QUIT_DELAY_SECS:-150}"
CUA_E2E_DRIVER_TIMEOUT_SECS="${CUA_E2E_DRIVER_TIMEOUT_SECS:-20}"
export CUA_E2E_OUT_DIR CUA_E2E_SESSION="${CUA_E2E_SESSION:-reprise-acceptance}"
required_command() {
  if ! command -v "$1" >/dev/null; then
    echo "required command is unavailable: $1" >&2
    exit 2
  fi
}
assert_clean_app_log() {
  local log_path=$1 scenario=$2
  local failures='Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed|Failed to set text .* from markup|Entity did not end with a semicolon'

  if rg -i "$failures" "$log_path" >/dev/null; then
    echo "$scenario emitted a GTK/GLib critical, markup failure, panic, or RefCell failure ($log_path)" >&2
    rg -i "$failures" "$log_path" >&2
    return 1
  fi
}

assert_app_log_contains() {
  local log_path=$1 marker=$2 scenario=$3

  if ! rg --quiet --fixed-strings "$marker" "$log_path"; then
    echo "$scenario log is missing diagnostic marker '$marker': $log_path" >&2
    tail -n 60 "$log_path" >&2 || true
    return 1
  fi
}

assert_app_log_absent() {
  local log_path=$1 marker=$2 scenario=$3

  if rg --quiet --fixed-strings "$marker" "$log_path"; then
    echo "$scenario log carries the marker '$marker' it must not: $log_path" >&2
    rg --fixed-strings "$marker" "$log_path" >&2 || true
    return 1
  fi
}

window_id_from_response() {
  jq -r --arg class "$WINDOW_CLASS_MATCH" '
    [.. | objects
      | select(.window_id? != null)
      | select(((.title? // "") + " " + (.class? // "")
        + " " + (.wm_class? // "")) | ascii_downcase | contains($class))
      | .window_id][0] // empty
  '
}

wait_for_window() {
  local pid=$1 response window_id

  for _ in $(seq 1 60); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    response=$(cua_driver list_windows "$(jq -nc --argjson pid "$pid" '{pid: $pid}')")
    window_id=$(window_id_from_response <<<"$response")
    if [[ -n "$window_id" ]]; then
      printf '%s\n' "$window_id"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

wait_for_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 24); do
    snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt")
    if assert_snapshot_contains "$snapshot_path" "$label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
    sleep 0.25
  done
  echo "window never exposed expected accessible label '$label'" >&2
  return 1
}

wait_for_label_absent() {
  local pid=$1 window_id=$2 label=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 24); do
    snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt")
    if assert_snapshot_absent "$snapshot_path" "$label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
    sleep 0.25
  done
  echo "window still exposes unexpected accessible label '$label'" >&2
  return 1
}

cua_activate_main_menu_item() {
  local pid=$1 window_id=$2 label=$3 stem=$4
  local focus_path

  # Under X11 the GTK popover is a detached window. The main-window CUA
  # snapshot therefore cannot expose its item labels even though GTK has
  # opened the menu. The app's focus probe does observe the popup, so follow
  # the F10 contract with native Down traversal and activate the focused item.
  cua_hotkey "$pid" "$window_id" "$stem-f10" f10
  focus_path=$(cua_focus_label_via_key "$pid" "$window_id" "$label" down "$stem-focus")
  assert_focus_evidence_label "$focus_path" "$label"
  cua_press_key_window "$pid" "$window_id" enter "$stem-enter"
}

APP_PID=""
APP_LOG=""
WINDOW_ID=""

stop_app_on_failure() {
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi
}

start_scenario_app() {
  local scenario=$1 scan_dir=${2:-} tag_edit_smoke=${3:-}
  local quit_delay_secs=${4:-$CUA_E2E_QUIT_DELAY_SECS}
  local profile_root="$CUA_E2E_SCRATCH_ROOT/$scenario"
  local focus_state="$CUA_E2E_OUT_DIR/$scenario-focus-state.txt"
  local -a scenario_env=(-u REPRISE_SCAN_DIR -u REPRISE_SMOKE_TAG_EDIT)

  CUA_E2E_FOCUS_STATE="$focus_state"
  export CUA_E2E_FOCUS_STATE
  mkdir -p "$profile_root/data" "$profile_root/cache" "$profile_root/config"
  if [[ -n "$scan_dir" ]]; then
    scenario_env+=(REPRISE_SCAN_DIR="$scan_dir")
  fi
  if [[ -n "$tag_edit_smoke" ]]; then
    scenario_env+=(REPRISE_SMOKE_TAG_EDIT="$tag_edit_smoke")
  fi
  APP_LOG="$CUA_E2E_OUT_DIR/$scenario-app.log"
  env "${scenario_env[@]}" \
    XDG_DATA_HOME="$profile_root/data" \
    XDG_CACHE_HOME="$profile_root/cache" \
    XDG_CONFIG_HOME="$profile_root/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS="$quit_delay_secs" \
    REPRISE_SMOKE_FOCUS_STATE="$focus_state" \
    REPRISE_LOG=debug \
    "$CUA_E2E_BIN_PATH" >"$APP_LOG" 2>&1 &
  APP_PID=$!
  # Lets cua_driver fail fast instead of waiting out a 120s timeout per call
  # once the app is gone (see lib.sh).
  export CUA_E2E_APP_PID="$APP_PID"
  if ! WINDOW_ID=$(wait_for_window "$APP_PID"); then
    echo "$scenario did not expose a Reprise window" >&2
    tail -n 60 "$APP_LOG" >&2 || true
    return 1
  fi
}

finish_scenario() {
  local scenario=$1
  shift

  if ! wait "$APP_PID"; then
    echo "$scenario did not shut down cleanly through the smoke timer" >&2
    return 1
  fi
  assert_clean_app_log "$APP_LOG" "$scenario"
  assert_app_log_contains "$APP_LOG" "starting Reprise" "$scenario"
  assert_app_log_contains "$APP_LOG" "database ready" "$scenario"
  assert_app_log_contains "$APP_LOG" "smoke-quit timer fired" "$scenario"
  for marker in "$@"; do
    assert_app_log_contains "$APP_LOG" "$marker" "$scenario"
  done
  APP_PID=""
  CUA_E2E_APP_PID=""
  export CUA_E2E_APP_PID
  WINDOW_ID=""
}

run_fresh_install_scenario() {
  local initial_path empty_path

  echo "[cua-e2e] fresh install: wizard -> skip -> empty library"
  start_scenario_app fresh-install
  initial_path=$(wait_for_label "$APP_PID" "$WINDOW_ID" "Welcome to Reprise" fresh-initial)
  assert_snapshot_contains "$initial_path" "Skip for Now"
  assert_snapshot_contains "$initial_path" "Set Up Library"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Skip for Now" fresh-skip
  empty_path=$(wait_for_label "$APP_PID" "$WINDOW_ID" "No music yet" fresh-empty)
  assert_snapshot_absent "$empty_path" "Welcome to Reprise"
  finish_scenario fresh-install \
    "first-run wizard presented" \
    "first-run setup completed"
}

run_populated_library_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/fixture-music"
  local initial_path missing_path results_path

  echo "[cua-e2e] populated library: fixture scan -> semantic search"
  mkdir -p "$fixture_dir"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=120 \
    -c:a flac "$fixture_dir/sine_01.flac"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_02.flac"
  start_scenario_app \
    populated-library "$fixture_dir" "" "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  initial_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" populated-initial)
  assert_snapshot_contains "$initial_path" "sine_01"
  echo "[cua-e2e] browse-1-single-track-surface: one table without mode tabs"
  assert_snapshot_contains "$initial_path" "Title"
  assert_snapshot_contains "$initial_path" "Artist"
  assert_snapshot_absent "$initial_path" "Tracks"
  assert_snapshot_absent "$initial_path" "Albums"
  assert_snapshot_absent "$initial_path" "Artists"

  # This is an isolated copy below the run's mktemp root, never user music.
  # Removing it exercises the real watcher and makes the Issues surface part
  # of the same keyboard-only inventory sweep.
  rm -- "$fixture_dir/sine_02.flac"
  missing_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Missing files" populated-missing)
  assert_snapshot_contains "$missing_path" "Missing files"

  CUA_E2E_FOCUS_STATE="$CUA_E2E_OUT_DIR/populated-library-focus-state.txt" \
  CUA_E2E_APP_PID="$APP_PID" \
  CUA_E2E_DRIVER_TIMEOUT_SECS="$CUA_E2E_DRIVER_TIMEOUT_SECS" \
  CUA_E2E_KEYBOARD_GROUP=primary \
    "$repo_root/scripts/cua-e2e/keyboard.sh" --run "$APP_PID" "$WINDOW_ID"
  restart_private_cua_daemon

  # UX PLAY-2 [e2e] wiring: double-clicking a row builds the queue from the
  # visible list (log marker from play_from_view) and starts playback.
  echo "[cua-e2e] play-2-doubleclick-row: activation builds queue from view"
  cua_double_click_label "$APP_PID" "$WINDOW_ID" "sine_01" "play-2-doubleclick-row"
  assert_app_log_contains \
    "$APP_LOG" "queue set from view" "play-2-doubleclick-row"

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" populated-search-toggle
  cua_type_text_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" "nomatch" populated-search
  results_path=$(wait_for_label "$APP_PID" "$WINDOW_ID" "No results" populated-no-results)
  assert_snapshot_contains "$results_path" "Try a different search"
  finish_scenario populated-library \
    "dev scan complete" \
    "first-run decision"
}

run_populated_library_secondary_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/fixture-music-secondary"

  # cua-driver 0.8 loses its persistent AT-SPI listener during longer sweeps.
  # The outer runner therefore executes this half of the same manifest in a
  # fresh D-Bus, AT-SPI, app, and daemon lifecycle instead of allowing one
  # infrastructure failure to masquerade as four product regressions.
  mkdir -p "$fixture_dir"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" \
    "$fixture_dir/sine_01.flac"
  restart_private_cua_daemon
  start_scenario_app \
    populated-library-secondary "$fixture_dir" "" "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" populated-secondary-initial \
    >/dev/null
  CUA_E2E_FOCUS_STATE="$CUA_E2E_OUT_DIR/populated-library-secondary-focus-state.txt" \
  CUA_E2E_APP_PID="$APP_PID" \
  CUA_E2E_DRIVER_TIMEOUT_SECS="$CUA_E2E_DRIVER_TIMEOUT_SECS" \
  CUA_E2E_KEYBOARD_GROUP=secondary \
    "$repo_root/scripts/cua-e2e/keyboard.sh" --run "$APP_PID" "$WINDOW_ID"
  restart_private_cua_daemon
  finish_scenario populated-library-secondary \
    "dev scan complete" \
    "first-run decision"
}

run_android_sync_page_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/android-sync-fixture-music"
  local device_root="$CUA_E2E_SCRATCH_ROOT/android-sync-device"
  local card_path page_path

  echo "[cua-e2e] mtp-13: simulated device opens the full synchronization page"
  mkdir -p "$fixture_dir" "$device_root"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=120 \
    -metadata title="Simulated Sync Track" \
    -metadata artist="Reprise E2E" \
    -metadata album="Android Sync" \
    -c:a flac "$fixture_dir/simulated_sync.flac"
  REPRISE_SMOKE_DEVICE_ROOT="$device_root" \
  REPRISE_SMOKE_DEVICE_PLAYLIST="Recently added" \
  REPRISE_SMOKE_DEVICE_UI_ONLY=1 \
    start_scenario_app \
      android-sync-page "$fixture_dir" "" 25

  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Toggle sidebar" android-sync-sidebar-toggle-visible \
    >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Toggle sidebar" android-sync-sidebar-open
  card_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Simulated MTP Phone" android-sync-device-card)
  assert_snapshot_contains "$card_path" "Simulated MTP Phone"
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Open Simulated MTP Phone" android-sync-open-page
  page_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Transfer profile" android-sync-page)
  for label in \
    "Playlists" \
    "Recently added" \
    "Sync overview" \
    "Next synchronization" \
    "Never synchronized"; do
    assert_snapshot_contains "$page_path" "$label"
  done
  for removed_label in \
    "YouTube audio" \
    "Podcast episodes" \
    "Size limit in GiB" \
    "no size limit"; do
    assert_snapshot_absent "$page_path" "$removed_label"
  done
  assert_snapshot_absent "$page_path" "Device files"
  assert_snapshot_absent "$page_path" "Entire library"
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Recently added" android-sync-select-playlist
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "1 unique track · 2.4 MiB on device" \
    android-sync-playlist-selected \
    >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Sync now" android-sync-start

  finish_scenario android-sync-page \
    "dev scan complete" \
    "device sync started from page"
  if ! find "$device_root/Music/Reprise" -type f -name '*.opus' -print -quit \
    | rg --quiet .; then
    echo "android-sync-page did not publish the simulated Opus track" >&2
    return 1
  fi
}

run_song_visuals_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/song-visuals-fixture-music"
  local cover_path="$CUA_E2E_SCRATCH_ROOT/song-visuals-cover.png"
  local initial_path panel_path visual_path

  echo "[cua-e2e] ac-10/ac-11: album-accent song visuals"
  mkdir -p "$fixture_dir"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i color=c=0xd86a45:s=600x600 -frames:v 1 "$cover_path"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=120 \
    -i "$cover_path" \
    -map 0:a -map 1:v \
    -metadata title="Accent Fixture" \
    -metadata artist="Visual Artist" \
    -metadata album="Copper Album" \
    -c:a flac -c:v png -disposition:v:0 attached_pic \
    "$fixture_dir/accent_fixture.flac"

  start_scenario_app \
    song-visuals "$fixture_dir" "" "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  initial_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Accent Fixture" song-visuals-library)
  assert_snapshot_contains "$initial_path" "Search all fields"
  cua_focus_label_via_key \
    "$APP_PID" "$WINDOW_ID" "Accent Fixture" down song-visuals-track-focus >/dev/null
  cua_press_key_window "$APP_PID" "$WINDOW_ID" enter song-visuals-play
  wait_for_label "$APP_PID" "$WINDOW_ID" "Pause (Space)" song-visuals-playing >/dev/null

  cua_activate_main_menu_item \
    "$APP_PID" "$WINDOW_ID" "Preferences" song-visuals-preferences
  wait_for_label "$APP_PID" "$WINDOW_ID" "Plugins" song-visuals-preferences-open >/dev/null
  cua_focus_label_via_key \
    "$APP_PID" "$WINDOW_ID" "Plugins" down song-visuals-plugins-focus >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter song-visuals-plugins-enter
  wait_for_label "$APP_PID" "$WINDOW_ID" "Song Visuals" song-visuals-plugin >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Song Visuals" song-visuals-enable
  cua_hotkey "$APP_PID" "$WINDOW_ID" song-visuals-preferences-close ctrl w
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Preferences" song-visuals-preferences-closed >/dev/null

  panel_path=$(cua_snapshot "$APP_PID" "$WINDOW_ID" song-visuals-panel-state)
  if ! snapshot_exposes_label "$panel_path" "Visual"; then
    cua_click_label \
      "$APP_PID" "$WINDOW_ID" "Toggle Now Playing panel" song-visuals-panel-open
  fi
  wait_for_label "$APP_PID" "$WINDOW_ID" "Visual" song-visuals-visual-tab >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Visual" song-visuals-select-visual
  visual_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Audio-reactive song visual" song-visuals-visible)
  assert_snapshot_contains "$visual_path" "Visual"
  assert_snapshot_absent "$visual_path" "Audio Character"
  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "Visual" song-visuals-visual-focus >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" space song-visuals-space-pause
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Play (Space)" song-visuals-space-paused >/dev/null

  finish_scenario song-visuals \
    "dev scan complete" \
    "queue set from view"
}

run_tag_1_no_jump_after_save_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/tag-1-fixture-music"
  local saved_path

  echo "[cua-e2e] tag-1-no-jump-after-save: save preserves library position"
  mkdir -p "$fixture_dir"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_01.flac"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_02.flac"
  start_scenario_app \
    tag-1-no-jump-after-save "$fixture_dir" "title:CUA selection preserved"
  saved_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "CUA selection preserved" tag-1-saved)
  assert_snapshot_contains "$saved_path" "Title"
  assert_snapshot_contains "$saved_path" "Search all fields"
  finish_scenario tag-1-no-jump-after-save \
    "dev scan complete" \
    "tag-edit batch completed" \
    "query matched 2 tracks" \
    "(track tags edited)"
}

run_tag_3_multi_dialog_structure_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/tag-3-fixture-music"
  local dialog_path closed_path

  echo "[cua-e2e] tag-3-multi-dialog-structure: accessible multi-editor structure"
  mkdir -p "$fixture_dir"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_01.flac"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_02.flac"
  start_scenario_app tag-3-multi-dialog-structure "$fixture_dir" "open:2"
  dialog_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Edit 2 Tracks" tag-3-dialog)
  assert_snapshot_contains \
    "$dialog_path" "Only changed fields will be written to all selected tracks"
  assert_snapshot_contains "$dialog_path" "Save"
  assert_snapshot_contains "$dialog_path" "Cancel"
  assert_snapshot_contains "$dialog_path" "Title"
  assert_snapshot_contains "$dialog_path" "Artist"
  assert_snapshot_contains "$dialog_path" "Album"
  assert_snapshot_contains "$dialog_path" "Album artist"
  assert_snapshot_contains "$dialog_path" "Genre"
  assert_snapshot_contains "$dialog_path" "Year"
  assert_snapshot_contains "$dialog_path" "Track number"
  assert_snapshot_contains "$dialog_path" "Rating"
  assert_snapshot_contains "$dialog_path" "per track"
  assert_snapshot_contains "$dialog_path" "—"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Cancel" tag-3-cancel
  closed_path=$(wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Edit 2 Tracks" tag-3-closed)
  assert_snapshot_contains "$closed_path" "Search all fields"
  finish_scenario tag-3-multi-dialog-structure \
    "dev scan complete" \
    "tag editor presented"
}

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

# NAV-17 acceptance. The rule is about what a user sees after a shift click, so
# it is driven through real pointer input and read back from the screenshot and
# the app log — not from the seam test that already covers the pure resolver.
build_anchor_fixture() {
  local fixture_dir=$1 i

  mkdir -p "$fixture_dir"
  for i in 1 2 3 4 5; do
    ffmpeg -hide_banner -loglevel error -y \
      -f lavfi -i "sine=frequency=$((260 + i * 60)):duration=90" \
      -metadata artist="Anchor Artist" \
      -metadata album="Anchor Album" \
      -metadata title="anchor_0$i" \
      -metadata track="$i" \
      -c:a flac "$fixture_dir/anchor_0$i.flac"
  done
}

# Pointer helper for the track list. Two reasons this goes by pixel instead of
# by label: the ColumnView reports every cell at the same flattened frame
# (`y: 0` for all five rows, measured), and shift has to be held by the driver
# itself — a Shift keypress wrapped around a separate click is a different
# event sequence than a modified button press, and GTK's range selection reads
# the modifier state carried by the press.
cua_pointer_window_point() {
  local verb=$1 pid=$2 window_id=$3 x=$4 y=$5 stem=$6
  local modifier=${7:-none}
  local action_path payload

  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson x "$x" \
    --argjson y "$y" \
    --arg session "$CUA_E2E_SESSION" \
    --arg modifier "$modifier" \
    '{pid: $pid, window_id: $window_id, x: $x, y: $y, session: $session,
      delivery_mode: "foreground"}
     + (if $modifier == "none" then {} else {modifier: [$modifier]} end)')
  if ! cua_driver "$verb" "$payload" >"$action_path"; then
    echo "CUA $verb command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem" >/dev/null || return 1
}

# Window-local geometry of the track list, measured on the 1440x900 CUA window
# with the discovery banner dismissed (evidence: anchor-01b-banner-dismissed).
# Row 1 sits at 175 and every further row is 45 px below it.
ANCHOR_ROW_X=400
anchor_row_y() { echo $((130 + $1 * 45)); }

report_label_frames() {
  local snapshot_path=$1
  shift

  local label
  for label in "$@"; do
    jq -c -r --arg label "$label" '
      [(.structuredContent.elements // .elements // [])[]
        | select(.label == $label)
        | {index: .element_index, role: .role, frame: .frame}] | .[0] // "absent"
    ' "$snapshot_path" | sed "s/^/[cua-e2e]   $label /"
  done
}

assert_anchor_range() {
  local log_path=$1 start=$2 len=$3 scenario=$4

  # `REPRISE_LOG=debug` colours its output, and the escape codes sit *between*
  # the field name and its value (`start<esc>[2m=<esc>[0m2`). Without stripping
  # them no field-level assertion can ever match — which is why the neighbouring
  # log assertions in this file only ever match markers without a `=`.
  if ! sed -E $'s/\033\\[[0-9;]*m//g' "$log_path" \
    | rg --quiet "selection anchor range applied.*\bstart=$start\b.*\blen=$len\b"; then
    echo "$scenario did not apply the expected anchor range start=$start len=$len" >&2
    sed -E $'s/\033\\[[0-9;]*m//g' "$log_path" | rg "selection anchor" >&2 \
      || echo "(no selection anchor line at all)" >&2
    return 1
  fi
}

dismiss_discovery_banner() {
  local stem=$1

  # The banner pushes every row down by its own height; dismissing it first is
  # what makes the measured row geometry hold.
  cua_pointer_window_point click "$APP_PID" "$WINDOW_ID" 1376 71 "$stem"
}

# What the user actually sees. Two reasons this reads pixels: the track rows
# expose no selected state over AT-SPI (only the sidebar list items do), and the
# single-row case resolves to `SelectionOp::SelectOnly`, which writes no log line
# at all. Selected rows are painted teal (measured 43,90,91) against the list
# background (27,30,34) — green minus red separates them by more than tenfold.
assert_selected_rows() {
  local png=$1 description=$2
  shift 2
  local -a hits=()
  local row rgb r g b

  for row in 1 2 3 4 5; do
    rgb=$(magick "$png" -crop "1x1+800+$(anchor_row_y "$row")" -depth 8 txt:- \
      | tail -1 | sed -E 's/.*\(([0-9]+),([0-9]+),([0-9]+).*/\1 \2 \3/')
    read -r r g b <<<"$rgb"
    if ((g - r > 20)); then
      hits+=("$row")
    fi
  done
  if [[ "${hits[*]}" != "$*" ]]; then
    echo "$description: expected rows [$*] highlighted, the screenshot shows [${hits[*]}]: $png" >&2
    return 1
  fi
  echo "[cua-e2e] selection matches the screenshot: rows [${hits[*]}] — $description"
}

run_selection_anchor_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/fixture-anchor"
  local initial fresh

  echo "[cua-e2e] selection anchor (NAV-17): shift click starts at the playing row"
  build_anchor_fixture "$fixture_dir"

  start_scenario_app selection-anchor "$fixture_dir" "" 120
  initial=$(wait_for_label "$APP_PID" "$WINDOW_ID" "anchor_01" anchor-01-initial)
  assert_snapshot_contains "$initial" "anchor_05"
  echo "[cua-e2e] anchor row frames as AT-SPI reports them (flattened, hence pixels):"
  report_label_frames "$initial" anchor_01 anchor_03 anchor_05
  dismiss_discovery_banner anchor-01b-banner-dismissed

  # Start a song in the middle of the artist.
  cua_pointer_window_point double_click "$APP_PID" "$WINDOW_ID" \
    "$ANCHOR_ROW_X" "$(anchor_row_y 3)" anchor-02-play-row3
  assert_app_log_contains "$APP_LOG" "queue set from view" selection-anchor

  # Leave the view and come back: that is the reported situation — the user
  # never clicked a row, so no anchor of their own exists.
  cua_pointer_window_point click "$APP_PID" "$WINDOW_ID" 124 342 anchor-03-leave-view
  cua_pointer_window_point click "$APP_PID" "$WINDOW_ID" 124 115 anchor-04-return-view

  cua_pointer_window_point click "$APP_PID" "$WINDOW_ID" \
    "$ANCHOR_ROW_X" "$(anchor_row_y 5)" anchor-05-shift-click shift
  assert_anchor_range "$APP_LOG" 2 3 selection-anchor
  assert_selected_rows "$CUA_E2E_OUT_DIR/anchor-05-shift-click.png" \
    "shift click ran from the playing row, not from the top of the list" 3 4 5

  finish_scenario selection-anchor "queue set from view"

  # Second half: neither an anchor nor a playing row. A fresh instance is the
  # honest way to have both absent — nothing was ever clicked or played.
  echo "[cua-e2e] selection anchor (NAV-17): shift click without anchor or playing row"
  start_scenario_app selection-anchor-fresh "$fixture_dir" "" 90
  fresh=$(wait_for_label "$APP_PID" "$WINDOW_ID" "anchor_01" anchor-10-fresh)
  assert_snapshot_contains "$fresh" "anchor_05"
  dismiss_discovery_banner anchor-10b-banner-dismissed
  cua_pointer_window_point click "$APP_PID" "$WINDOW_ID" \
    "$ANCHOR_ROW_X" "$(anchor_row_y 4)" anchor-11-shift-click shift
  # No range line to wait for here: with neither an anchor nor a playing row the
  # rule is "mark exactly one", which resolves to `SelectOnly` and logs nothing.
  # The screenshot carries the whole claim — including that the click landed.
  assert_selected_rows "$CUA_E2E_OUT_DIR/anchor-11-shift-click.png" \
    "shift click without an anchor or a playing row marks exactly one row" 4
  assert_app_log_absent "$APP_LOG" "queue set from view" selection-anchor-fresh

  finish_scenario selection-anchor-fresh
}

private_session_cleanup() {
  local exit_code=$?
  stop_app_on_failure
  stop_scrobbling_services
  cua_common_stop_driver "$CUA_E2E_SESSION"
  exit "$exit_code"
}

start_private_cua_daemon() {
  cua_common_start_driver \
    "$CUA_E2E_OUT_DIR" "$CUA_DRIVER_SOCKET" "$CUA_E2E_SESSION"
}

restart_private_cua_daemon() {
  cua_common_restart_driver \
    "$CUA_E2E_OUT_DIR" "$CUA_DRIVER_SOCKET" "$CUA_E2E_SESSION"
}

run_private_session() {
  local private_group=${CUA_E2E_PRIVATE_GROUP:-${CUA_E2E_ONLY:-all}}
  trap private_session_cleanup EXIT

  export CUA_DRIVER_SOCKET="$CUA_E2E_SCRATCH_ROOT/$private_group-cua-driver.sock"
  start_private_cua_daemon

  case "$private_group" in
    populated-library)
      run_populated_library_scenario
      ;;
    populated-library-secondary)
      run_populated_library_secondary_scenario
      ;;
    android-sync-page)
      run_android_sync_page_scenario
      ;;
    fresh-install)
      run_fresh_install_scenario
      ;;
    selection-anchor)
      run_selection_anchor_scenario
      ;;
    tag-1-no-jump-after-save)
      run_tag_1_no_jump_after_save_scenario
      ;;
    tag-3-multi-dialog-structure)
      run_tag_3_multi_dialog_structure_scenario
      ;;
    tag-autocomplete-surface)
      run_tag_autocomplete_surface_scenario
      ;;
    library-doctor)
      run_library_doctor_scenario
      ;;
    song-visuals)
      run_song_visuals_scenario
      ;;
    track-sort-playing-marker)
      run_track_sort_playing_marker_scenario
      ;;
    scrobbling)
      run_scrobbling_scenario
      ;;
    responsive-window)
      run_responsive_window_scenario
      ;;
    source-podcasts)
      run_source_podcasts_scenario
      ;;
    source-youtube)
      run_source_youtube_scenario
      ;;
    podcast-backlog)
      run_podcast_backlog_scenario
      ;;
    play-11-filter-clear)
      run_play_11_filter_clear_continuation_scenario
      run_play_11_late_filter_clear_scenario
      ;;
    *)
      echo "unknown private CUA scenario group: $private_group" >&2
      return 2
      ;;
  esac
  echo "[cua-e2e] private scenario group passed: $private_group"
}
if [[ "${1:-}" == "--private-session" ]]; then
  run_private_session
  exit 0
fi
for command in "$CUA_DRIVER_BIN" Xvfb openbox cargo dbus-run-session ffmpeg gdbus gnome-keyring-daemon import jq python3 rg sqlite3 timeout wmctrl; do
  required_command "$command"
done
if [[ ! -x /usr/lib/at-spi-bus-launcher ]]; then
  echo "AT-SPI bus launcher is unavailable: /usr/lib/at-spi-bus-launcher" >&2
  exit 2
fi
# `test-fixtures` routes the podcast and radio HTTP clients at a directory of
# files instead of the network. The feature is inert without the matching
# environment variables — `reprise-gnome/Cargo.toml` documents it as existing
# exactly for isolated smoke/E2E binaries — so building it in costs the other
# scenarios nothing and is what lets the source scenarios run offline at all.
case "$CUA_E2E_PROFILE" in
  debug)
    (cd "$repo_root" && cargo build --locked -p reprise-gnome --features test-fixtures)
    CUA_E2E_BIN_PATH="$repo_root/target/debug/reprise"
    ;;
  release)
    (cd "$repo_root" && cargo build --locked -p reprise-gnome --release --features test-fixtures)
    CUA_E2E_BIN_PATH="$repo_root/target/release/reprise"
    ;;
  *) echo "CUA_E2E_PROFILE must be debug or release" >&2; exit 2 ;;
esac
if [[ ! -x "$CUA_E2E_BIN_PATH" ]]; then
  echo "build Reprise first: cargo build${CUA_E2E_PROFILE/release/ --release}" >&2
  exit 2
fi
CUA_E2E_SCRATCH_ROOT=$(mktemp -d /tmp/reprise-cua-e2e.XXXXXX)
export CUA_E2E_SCRATCH_ROOT CUA_E2E_BIN_PATH
output_root=$CUA_E2E_OUT_DIR
CUA_E2E_OUT_DIR="$output_root/run-$(date -u +%Y%m%dT%H%M%SZ)-$$"
export CUA_E2E_OUT_DIR
mkdir -p "$CUA_E2E_OUT_DIR"
{
  printf 'reprise_commit=%s\n' "$(git -C "$repo_root" rev-parse HEAD)"
  printf 'reprise_profile=%s\n' "$CUA_E2E_PROFILE"
  printf 'cua_driver=%s\n' "$($CUA_DRIVER_BIN --version)"
  printf 'platform=%s\n' "$(uname -srmo)"
  printf 'display_backend=x11-xvfb\n'
  printf 'created_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >"$CUA_E2E_OUT_DIR/run-manifest.txt"
echo "[cua-e2e] evidence: $CUA_E2E_OUT_DIR"

cleanup() {
  local exit_code=$?
  cua_common_stop_display
  rm -rf "$CUA_E2E_SCRATCH_ROOT"
  exit "$exit_code"
}
trap cleanup EXIT

  cua_common_start_display \
    "$CUA_E2E_OUT_DIR" "$CUA_E2E_SCRATCH_ROOT" "$CUA_E2E_SCREEN_RES"
run_private_scenario_group() {
  local scenario_group=$1
  cua_common_run_private \
    "$0" "$scenario_group" "$CUA_E2E_SCRATCH_ROOT" "$CUA_E2E_OUT_DIR"
}
case "${CUA_E2E_ONLY:-all}" in
  all)
    scenario_groups=(
      fresh-install
      populated-library
      populated-library-secondary
      android-sync-page
      tag-1-no-jump-after-save
      tag-3-multi-dialog-structure
      tag-autocomplete-surface
      library-doctor
      song-visuals
      track-sort-playing-marker
      scrobbling
      responsive-window
      source-podcasts
      source-youtube
      podcast-backlog
      play-11-filter-clear
    )
    ;;
  populated-library)
    scenario_groups=(populated-library populated-library-secondary)
    ;;
  fresh-install | selection-anchor | populated-library-secondary | android-sync-page \
    | tag-1-no-jump-after-save \
    | tag-3-multi-dialog-structure | tag-autocomplete-surface \
    | library-doctor | song-visuals \
    | track-sort-playing-marker | scrobbling | responsive-window \
    | source-podcasts | source-youtube | podcast-backlog \
    | play-11-filter-clear)
    scenario_groups=("$CUA_E2E_ONLY")
    ;;
  *)
    echo "unknown CUA_E2E_ONLY scenario: $CUA_E2E_ONLY" >&2
    exit 2
    ;;
esac
failed_groups=()
passed_groups=0
# Each group runs inside dbus-run-session ... "$0" --private-session, and that
# child owns a private_session_cleanup EXIT trap. A red group therefore tears
# down its app, D-Bus, AT-SPI and driver before the parent continues.
for scenario_group in "${scenario_groups[@]}"; do
  set +e; (set -e; run_private_scenario_group "$scenario_group")
  scenario_group_status=$?; set -e
  if ((scenario_group_status != 0)); then
    failed_groups+=("$scenario_group")
    continue
  fi
  passed_groups=$((passed_groups + 1))
done
if ((${#failed_groups[@]} > 0)); then
  echo "[cua-e2e] scenario groups passed: $passed_groups, failed: ${#failed_groups[@]}"
  printf '[cua-e2e] FAILED scenario group: %s\n' "${failed_groups[@]}" >&2
  exit 1
fi
echo "[cua-e2e] all acceptance scenarios passed ($passed_groups scenario groups)"
