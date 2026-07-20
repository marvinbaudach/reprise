#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

# shellcheck source=lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"
# shellcheck source=navigation_playback.sh
source "$repo_root/scripts/cua-e2e/navigation_playback.sh"

APP_ID=org.reprise.Reprise
WINDOW_CLASS_MATCH=reprise
CUA_E2E_PROFILE="${CUA_E2E_PROFILE:-debug}"
CUA_E2E_OUT_DIR="${CUA_E2E_OUT_DIR:-/tmp/reprise-cua-e2e}"
CUA_E2E_SCREEN_RES="${CUA_E2E_SCREEN_RES:-1600x900x24}"
CUA_E2E_QUIT_DELAY_SECS="${CUA_E2E_QUIT_DELAY_SECS:-15}"
CUA_E2E_KEYBOARD_QUIT_DELAY_SECS="${CUA_E2E_KEYBOARD_QUIT_DELAY_SECS:-180}"
export CUA_E2E_OUT_DIR CUA_E2E_SESSION="${CUA_E2E_SESSION:-reprise-acceptance}"

required_command() {
  if ! command -v "$1" >/dev/null; then
    echo "required command is unavailable: $1" >&2
    exit 2
  fi
}

assert_clean_app_log() {
  local log_path=$1 scenario=$2
  local failures='Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed'

  if rg -i "$failures" "$log_path" >/dev/null; then
    echo "$scenario emitted a GTK/GLib critical, panic, or RefCell failure ($log_path)" >&2
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
  assert_snapshot_contains "$initial_path" "Tracks"

  # This is an isolated copy below the run's mktemp root, never user music.
  # Removing it exercises the real watcher and makes the Issues surface part
  # of the same keyboard-only inventory sweep.
  rm -- "$fixture_dir/sine_02.flac"
  missing_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Missing files" populated-missing)
  assert_snapshot_contains "$missing_path" "Missing files"

  CUA_E2E_FOCUS_STATE="$CUA_E2E_OUT_DIR/populated-library-focus-state.txt" \
  CUA_E2E_APP_PID="$APP_PID" \
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

run_navigation_playback_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/navigation-playback-fixture-music"
  local base_track="$CUA_E2E_SCRATCH_ROOT/navigation-playback-base.flac"
  local detail_path playing_path back_path panel_path album_log_line artist_log_line
  local target="Navigation Track 14"
  local music_target="Navigation Track 10"

  echo "[cua-e2e] nav-12-back-restores-focus"
  echo "[cua-e2e] nav-13-playback-is-not-navigation"
  prepare_navigation_playback_fixture "$fixture_dir" "$base_track"

  start_scenario_app \
    navigation-playback "$fixture_dir" "" "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  cua_resize_window \
    "$APP_PID" "$WINDOW_ID" 1200 800 navigation-normalized-window
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Navigation Track 01" navigation-library >/dev/null
  panel_path=$(cua_snapshot \
    "$APP_PID" "$WINDOW_ID" navigation-initial-panel-state)
  if snapshot_exposes_label "$panel_path" "Queue is empty"; then
    cua_click_label \
      "$APP_PID" "$WINDOW_ID" "Toggle Now Playing panel" navigation-panel-close
  fi

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Back to previous view" navigation-back-disabled
  assert_snapshot_contains \
    "$CUA_E2E_OUT_DIR/navigation-back-disabled-after.json" "+ Add filter"

  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "$target" navigation-track-focus-current >/dev/null
  cua_focus_label_via_key \
    "$APP_PID" "$WINDOW_ID" "$music_target" up navigation-track-target >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter navigation-track-activate
  playing_path="$CUA_E2E_OUT_DIR/navigation-track-activate-after.json"
  assert_snapshot_contains "$playing_path" "$music_target"
  assert_focus_evidence_label "$playing_path" "$music_target"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Albums" navigation-albums
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Navigation Album" navigation-album-grid >/dev/null
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Sentinel Album" navigation-album-grid-sentinel >/dev/null
  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "N" navigation-album-focus >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter navigation-album-tracks
  detail_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Navigation Track 01" navigation-album-detail)
  assert_snapshot_contains "$detail_path" "Back to previous view"
  assert_snapshot_absent "$detail_path" "Sentinel Track"

  cua_focus_label_via_key \
    "$APP_PID" "$WINDOW_ID" "$target" down navigation-album-target >/dev/null
  album_log_line=$(wc -l <"$APP_LOG")
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter navigation-album-activate
  playing_path="$CUA_E2E_OUT_DIR/navigation-album-activate-after.json"
  assert_snapshot_contains "$playing_path" "Back to previous view"
  assert_snapshot_contains "$playing_path" "$target"
  assert_snapshot_absent "$playing_path" "Sentinel Track"
  assert_snapshot_contains "$playing_path" "Pause (Space)"
  assert_focus_evidence_label "$playing_path" "$target"
  assert_app_log_contains_since \
    "$APP_LOG" "$album_log_line" "queue set from view" navigation-album-activate
  assert_no_library_source_since \
    "$APP_LOG" "$album_log_line" \
    "album activation navigated back to the Library root"

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Back to previous view" navigation-album-back
  back_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Navigation Album" navigation-album-restored)
  assert_focus_evidence_label "$back_path" "N"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Artists" navigation-artists
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Navigation Artist" navigation-artist-master >/dev/null
  cua_hotkey \
    "$APP_PID" "$WINDOW_ID" navigation-artist-detail shift tab
  cua_focus_label_via_key \
    "$APP_PID" "$WINDOW_ID" "NA" up navigation-artist-master-row >/dev/null
  detail_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" \
    "1 Navigation Track 10 Navigation Album 1 play 0:30" \
    navigation-artist-tracks)
  assert_snapshot_contains "$detail_path" "Navigation Album 16 tracks"

  # Artist top tracks are native row buttons, not the shared track table.
  # Their activation must play in artist context without changing the mode.
  artist_log_line=$(wc -l <"$APP_LOG")
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" \
    "1 Navigation Track 10 Navigation Album 1 play 0:30" \
    navigation-artist-top-track-activate
  playing_path="$CUA_E2E_OUT_DIR/navigation-artist-top-track-activate-after.json"
  assert_snapshot_contains "$playing_path" "Artists"
  assert_snapshot_contains "$playing_path" "Navigation Artist"
  assert_snapshot_contains "$playing_path" "Pause (Space)"
  assert_app_log_contains_since \
    "$APP_LOG" "$artist_log_line" "queue set from view" navigation-artist-top-track-activate
  assert_no_library_source_since \
    "$APP_LOG" "$artist_log_line" \
    "artist top-track activation navigated back to the Library root"

  # Exercise the reported Artist -> Album -> lower-track playback path.
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Navigation Album 16 tracks" navigation-artist-album
  detail_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "$target" navigation-artist-album-tracks)
  assert_snapshot_absent "$detail_path" "Sentinel Track"
  assert_app_log_contains \
    "$APP_LOG" "source=album:Navigation Album:Navigation Artist" navigation-artist-album
  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "$target" navigation-artist-album-target >/dev/null
  artist_log_line=$(wc -l <"$APP_LOG")
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter navigation-artist-album-activate
  playing_path="$CUA_E2E_OUT_DIR/navigation-artist-album-activate-after.json"
  assert_snapshot_contains "$playing_path" "$target"
  assert_snapshot_absent "$playing_path" "Sentinel Track"
  assert_focus_evidence_label "$playing_path" "$target"
  assert_no_library_source_since \
    "$APP_LOG" "$artist_log_line" \
    "artist-derived album activation navigated back to the Library root"

  finish_scenario navigation-playback \
    "dev scan complete" \
    "queue set from view"
}

run_song_visuals_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/song-visuals-fixture-music"
  local cover_path="$CUA_E2E_SCRATCH_ROOT/song-visuals-cover.png"
  local initial_path panel_path visual_path dialog_path closed_path

  echo "[cua-e2e] ac-9/ac-10/ac-11: player info dialog and album-accent visuals"
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
  assert_snapshot_contains "$visual_path" "Rings"
  assert_snapshot_contains "$visual_path" "Flow"
  assert_snapshot_contains "$visual_path" "Pulse"
  assert_snapshot_contains "$visual_path" "F11 Fullscreen · color follows the cover accent"
  assert_snapshot_absent "$visual_path" "Audio Character"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Song analysis" song-analysis-open
  dialog_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Audio Character" song-analysis-dialog)
  assert_snapshot_contains "$dialog_path" "Local audio analysis is disabled"
  cua_press_key_window "$APP_PID" "$WINDOW_ID" escape song-analysis-close
  closed_path=$(wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Audio Character" song-analysis-closed)
  assert_focus_evidence_label "$closed_path" "Song analysis"

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
  assert_snapshot_contains "$saved_path" "Tracks"
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

  echo "[cua-e2e] library-doctor: opt in -> review -> apply -> disabled revert"
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
  wait_for_label "$APP_PID" "$WINDOW_ID" "Enable Library Doctor" doctor-plugin >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Enable Library Doctor" doctor-enable
  wait_for_label "$APP_PID" "$WINDOW_ID" "Run Scan Now" doctor-run-ready >/dev/null
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
  wait_for_label "$APP_PID" "$WINDOW_ID" "Enable Library Doctor" doctor-plugin-enabled >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Enable Library Doctor" doctor-disable
  cua_hotkey "$APP_PID" "$WINDOW_ID" doctor-disabled-close ctrl w
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Preferences" doctor-disabled-close-complete >/dev/null
  cua_activate_main_menu_item \
    "$APP_PID" "$WINDOW_ID" "Library Doctor" doctor-disabled-entry
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Revert Last Cleanup" doctor-revert-available >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Revert Last Cleanup" doctor-revert-disabled
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Tags reverted · $fixture_count tracks" doctor-reverted \
    >/dev/null

  finish_scenario library-doctor \
    "dev scan complete" \
    "Library Doctor write completed"
}

private_session_cleanup() {
  local exit_code=$?
  stop_app_on_failure
  cua_driver end_session \
    "$(jq -nc --arg session "$CUA_E2E_SESSION" '{session: $session}')" \
    >/dev/null 2>&1 || true
  if [[ -n "${CUA_DAEMON_PID:-}" ]]; then
    kill -TERM "$CUA_DAEMON_PID" 2>/dev/null || true
  fi
  if [[ -n "${ATSPI_REGISTRYD_PID:-}" ]]; then
    kill -TERM "$ATSPI_REGISTRYD_PID" 2>/dev/null || true
  fi
  if [[ -n "${ATSPI_PID:-}" ]]; then
    kill -TERM "$ATSPI_PID" 2>/dev/null || true
  fi
  exit "$exit_code"
}

start_private_cua_daemon() {
  rm -f -- "$CUA_DRIVER_SOCKET"
  cua_driver serve --no-overlay \
    >>"$CUA_E2E_OUT_DIR/cua-driver.log" 2>&1 &
  CUA_DAEMON_PID=$!
  for _ in $(seq 1 40); do
    cua_driver status >/dev/null 2>&1 && break
    sleep 0.25
  done
  cua_driver status >/dev/null
  cua_driver start_session \
    "$(jq -nc --arg session "$CUA_E2E_SESSION" '{session: $session}')" >/dev/null
}

restart_private_cua_daemon() {
  kill -TERM "$CUA_DAEMON_PID" 2>/dev/null || true
  wait "$CUA_DAEMON_PID" 2>/dev/null || true
  start_private_cua_daemon
}

run_private_session() {
  trap private_session_cleanup EXIT

  /usr/lib/at-spi-bus-launcher \
    --launch-immediately --a11y=1 --screen-reader=1 \
    >"$CUA_E2E_OUT_DIR/at-spi.log" 2>&1 &
  ATSPI_PID=$!
  sleep 0.3

  /usr/lib/at-spi2-registryd \
    >"$CUA_E2E_OUT_DIR/at-spi-registryd.log" 2>&1 &
  ATSPI_REGISTRYD_PID=$!
  sleep 0.3

  export CUA_DRIVER_SOCKET="$CUA_E2E_SCRATCH_ROOT/cua-driver.sock"
  start_private_cua_daemon

  case "${CUA_E2E_ONLY:-all}" in
    all)
      run_fresh_install_scenario
      run_populated_library_scenario
      run_navigation_playback_scenario
      run_tag_1_no_jump_after_save_scenario
      run_tag_3_multi_dialog_structure_scenario
      run_library_doctor_scenario
      run_song_visuals_scenario
      ;;
    populated-library)
      run_populated_library_scenario
      ;;
    navigation-playback)
      run_navigation_playback_scenario
      ;;
    fresh-install)
      run_fresh_install_scenario
      ;;
    tag-1-no-jump-after-save)
      run_tag_1_no_jump_after_save_scenario
      ;;
    tag-3-multi-dialog-structure)
      run_tag_3_multi_dialog_structure_scenario
      ;;
    library-doctor)
      run_library_doctor_scenario
      ;;
    song-visuals)
      run_song_visuals_scenario
      ;;
    *)
      echo "unknown CUA_E2E_ONLY scenario: $CUA_E2E_ONLY" >&2
      return 2
      ;;
  esac
  echo "[cua-e2e] all acceptance scenarios passed"
}

if [[ "${1:-}" == "--private-session" ]]; then
  run_private_session
  exit 0
fi

for command in "$CUA_DRIVER_BIN" Xvfb openbox cargo dbus-run-session ffmpeg jq rg wmctrl; do
  required_command "$command"
done
if [[ ! -x /usr/lib/at-spi-bus-launcher ]]; then
  echo "AT-SPI bus launcher is unavailable: /usr/lib/at-spi-bus-launcher" >&2
  exit 2
fi

case "$CUA_E2E_PROFILE" in
  debug)
    (cd "$repo_root" && cargo build --locked -p reprise-gnome)
    CUA_E2E_BIN_PATH="$repo_root/target/debug/reprise"
    ;;
  release)
    (cd "$repo_root" && cargo build --locked -p reprise-gnome --release)
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

XVFB_PID=""
OPENBOX_PID=""
cleanup() {
  local exit_code=$?
  [[ -z "$OPENBOX_PID" ]] || kill -TERM "$OPENBOX_PID" 2>/dev/null || true
  [[ -z "$XVFB_PID" ]] || kill -TERM "$XVFB_PID" 2>/dev/null || true
  rm -rf "$CUA_E2E_SCRATCH_ROOT"
  exit "$exit_code"
}
trap cleanup EXIT

display_file="$CUA_E2E_SCRATCH_ROOT/display"
Xvfb -displayfd 8 -screen 0 "$CUA_E2E_SCREEN_RES" -nolisten tcp \
  8>"$display_file" >"$CUA_E2E_OUT_DIR/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 40); do
  [[ -s "$display_file" ]] && break
  kill -0 "$XVFB_PID" 2>/dev/null || break
  sleep 0.1
done
display_number=$(tr -d '[:space:]' <"$display_file")
if [[ -z "$display_number" ]]; then
  echo "Xvfb did not allocate a private display" >&2
  tail -n 20 "$CUA_E2E_OUT_DIR/xvfb.log" >&2 || true
  exit 1
fi
DISPLAY=":$display_number"
export DISPLAY
openbox >"$CUA_E2E_OUT_DIR/openbox.log" 2>&1 &
OPENBOX_PID=$!
# Lets cua_driver fail fast if the WM dies mid-run instead of delivering keys
# into the void (see lib.sh).
export CUA_E2E_WM_PID="$OPENBOX_PID"
sleep 0.5
if ! kill -0 "$OPENBOX_PID" 2>/dev/null; then
  echo "Openbox did not start on the private display" >&2
  tail -n 20 "$CUA_E2E_OUT_DIR/openbox.log" >&2 || true
  exit 1
fi

runtime_dir="$CUA_E2E_SCRATCH_ROOT/runtime"
mkdir -m 700 "$runtime_dir"
dbus-run-session -- env \
  XDG_RUNTIME_DIR="$runtime_dir" \
  XDG_DATA_HOME="$CUA_E2E_SCRATCH_ROOT/root-data" \
  XDG_CACHE_HOME="$CUA_E2E_SCRATCH_ROOT/root-cache" \
  XDG_CONFIG_HOME="$CUA_E2E_SCRATCH_ROOT/root-config" \
  GDK_BACKEND=x11 \
  WAYLAND_DISPLAY= \
  GTK_A11Y=atspi \
  NO_AT_BRIDGE=0 \
  REPRISE_AUDIO_SINK=fakesink \
  CUA_E2E_OUT_DIR="$CUA_E2E_OUT_DIR" \
  CUA_E2E_SCRATCH_ROOT="$CUA_E2E_SCRATCH_ROOT" \
  CUA_E2E_BIN_PATH="$CUA_E2E_BIN_PATH" \
  CUA_E2E_SESSION="$CUA_E2E_SESSION" \
  CUA_E2E_QUIT_DELAY_SECS="$CUA_E2E_QUIT_DELAY_SECS" \
  CUA_E2E_KEYBOARD_QUIT_DELAY_SECS="$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS" \
  CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
  "$0" --private-session
