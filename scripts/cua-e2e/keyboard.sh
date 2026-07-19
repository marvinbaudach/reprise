#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
manifest="$repo_root/scripts/cua-e2e/keyboard-surfaces.tsv"

# shellcheck source=lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"

assert_after_has_focus() {
  local stem=$1
  assert_unique_focus "$CUA_E2E_OUT_DIR/$stem-after.json"
}

keyboard_app_shell() {
  local pid=$1 window_id=$2 search_result_path
  cua_hotkey "$pid" "$window_id" acc-shell-search ctrl f
  cua_type_text_window "$pid" "$window_id" nomatch acc-shell-search-type
  search_result_path=$(cua_wait_for_label \
    "$pid" "$window_id" "No results" acc-shell-search-results)
  assert_snapshot_contains "$search_result_path" "No results"
  cua_press_key_window "$pid" "$window_id" escape acc-shell-search-clear
  cua_wait_for_label_absent \
    "$pid" "$window_id" "No results" acc-shell-search-cleared >/dev/null
  cua_press_key_window "$pid" "$window_id" escape acc-shell-escape
  assert_after_has_focus acc-shell-escape
  assert_focus_evidence_not \
    "$CUA_E2E_OUT_DIR/acc-shell-escape-after.json" \
    GtkText
}

keyboard_sidebar() {
  local pid=$1 window_id=$2 focus_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" "Music" acc-sidebar-focus)
  assert_focus_evidence_label "$focus_path" "Music"
  cua_press_key_window "$pid" "$window_id" down acc-sidebar-roving
  assert_after_has_focus acc-sidebar-roving
  assert_focus_evidence_label \
    "$CUA_E2E_OUT_DIR/acc-sidebar-roving-after.json" \
    "Queue"
}

keyboard_tracks_playlist_queue() {
  local pid=$1 window_id=$2 focus_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" sine_01 acc-tracks-focus)
  assert_focus_evidence_label "$focus_path" sine_01
  cua_press_key_window "$pid" "$window_id" enter acc-tracks-enter
  assert_after_has_focus acc-tracks-enter
  cua_hotkey "$pid" "$window_id" acc-tracks-context shift f10
  assert_after_has_focus acc-tracks-context
  cua_press_key_focused "$pid" "$window_id" escape acc-tracks-context-close
  assert_after_has_focus acc-tracks-context-close
}

keyboard_albums() {
  local pid=$1 window_id=$2 focus_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" Albums acc-albums-focus)
  assert_focus_evidence_label "$focus_path" Albums
  cua_press_key_window "$pid" "$window_id" enter acc-albums-open
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-albums-open-after.json" Albums
  assert_after_has_focus acc-albums-open
}

keyboard_artists() {
  local pid=$1 window_id=$2 focus_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" Artists acc-artists-focus)
  assert_focus_evidence_label "$focus_path" Artists
  cua_press_key_window "$pid" "$window_id" enter acc-artists-open
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-artists-open-after.json" Artists
  assert_after_has_focus acc-artists-open
}

keyboard_player_now_playing() {
  local pid=$1 window_id=$2 focus_path state_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" "Pause (Space)" acc-player-focus)
  assert_focus_evidence_label "$focus_path" "Pause (Space)"
  cua_press_key_window "$pid" "$window_id" space acc-player-toggle
  state_path=$(cua_wait_for_label \
    "$pid" "$window_id" "Play (Space)" acc-player-paused)
  assert_snapshot_contains "$state_path" "Play (Space)"
  assert_after_has_focus acc-player-toggle
  cua_hotkey "$pid" "$window_id" acc-player-now-playing ctrl l
  assert_after_has_focus acc-player-now-playing
}

keyboard_issues_import() {
  local pid=$1 window_id=$2 collection_path focus_path
  collection_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" "Music" acc-issues-main-collection)
  assert_focus_evidence_label "$collection_path" "Music"
  focus_path=$(cua_focus_label_via_key \
    "$pid" "$window_id" "Missing files" down acc-issues-focus)
  assert_focus_evidence_label "$focus_path" "Missing files"
  cua_press_key_window "$pid" "$window_id" enter acc-issues-open
  assert_after_has_focus acc-issues-open
}

keyboard_device_sync() {
  local pid=$1 window_id=$2 focus_path
  cua_hotkey "$pid" "$window_id" acc-device-menu f10
  focus_path=$(cua_focus_label_via_key \
    "$pid" "$window_id" "Sync Device…" down acc-device-focus)
  assert_focus_evidence_label "$focus_path" "Sync Device…"
  cua_press_key_window "$pid" "$window_id" enter acc-device-open
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-device-open-after.json" Synchronization
  assert_after_has_focus acc-device-open
}

keyboard_preferences() {
  local pid=$1 window_id=$2
  cua_press_key_focused "$pid" "$window_id" tab acc-preferences-tab
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-preferences-tab-after.json" Preferences
  assert_after_has_focus acc-preferences-tab
  cua_hotkey "$pid" "$window_id" acc-preferences-close ctrl w
  assert_after_has_focus acc-preferences-close
}

keyboard_modals() {
  local pid=$1 window_id=$2
  cua_hotkey "$pid" "$window_id" acc-help-open ctrl shift /
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-help-open-after.json" Help
  assert_after_has_focus acc-help-open
  cua_hotkey "$pid" "$window_id" acc-help-close ctrl w
  assert_after_has_focus acc-help-close
}

keyboard_stats() {
  local pid=$1 window_id=$2 collection_path focus_path
  collection_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" "Music" acc-stats-main-collection)
  assert_focus_evidence_label "$collection_path" Music
  focus_path=$(cua_focus_label_via_key \
    "$pid" "$window_id" "My Stats" down acc-stats-focus)
  assert_focus_evidence_label "$focus_path" "My Stats"
  cua_press_key_window "$pid" "$window_id" enter acc-stats-open
  assert_after_has_focus acc-stats-open
  cua_hotkey "$pid" "$window_id" acc-stats-return alt left
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-stats-return-after.json" Tracks
  assert_after_has_focus acc-stats-return
}

keyboard_compact_minimal() {
  local pid=$1 window_id=$2
  cua_resize_window "$pid" "$window_id" 700 650 acc-narrow-window
  cua_hotkey "$pid" "$window_id" acc-compact-open ctrl m
  assert_after_has_focus acc-compact-open
  assert_snapshot_absent "$CUA_E2E_OUT_DIR/acc-compact-open-after.json" "Search all fields"
  cua_hotkey "$pid" "$window_id" acc-compact-close ctrl m
  assert_after_has_focus acc-compact-close
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-compact-close-after.json" Tracks
  cua_resize_window "$pid" "$window_id" 1200 760 acc-wide-window
  assert_unique_focus "$CUA_E2E_OUT_DIR/acc-wide-window-after-resize.json"
}

check_manifest() {
  local surface scenario
  local count=0
  declare -A seen=()

  while IFS=$'\t' read -r surface scenario; do
    [[ -z "$surface" || "$surface" == \#* ]] && continue
    if [[ -z "$scenario" ]] || ! declare -F "$scenario" >/dev/null; then
      echo "keyboard manifest scenario is missing: $surface -> $scenario" >&2
      return 1
    fi
    if [[ -n "${seen[$surface]:-}" ]]; then
      echo "keyboard manifest contains duplicate surface: $surface" >&2
      return 1
    fi
    seen[$surface]=1
    count=$((count + 1))
  done <"$manifest"

  if ((count == 0)); then
    echo "keyboard manifest is empty" >&2
    return 1
  fi
}

run_manifest() {
  local pid=$1 window_id=$2 surface scenario

  check_manifest
  echo "[cua-keyboard] acc-1-keyboard-only-surface-sweep"
  echo "[cua-keyboard] acc-3-tab-order-and-roving-collections"
  echo "[cua-keyboard] acc-4-standard-keys-respect-local-controls"
  echo "[cua-keyboard] acc-5-transients-and-navigation-restore-focus"
  echo "[cua-keyboard] acc-8-direct-manipulation-has-keyboard-equivalence"
  while IFS=$'\t' read -r surface scenario; do
    [[ -z "$surface" || "$surface" == \#* ]] && continue
    echo "[cua-keyboard] $surface"
    "$scenario" "$pid" "$window_id"
  done <"$manifest"
}

case "${1:-}" in
  --check-manifest)
    check_manifest
    ;;
  --run)
    if (($# != 3)); then
      echo "usage: $0 --run PID WINDOW_ID" >&2
      exit 2
    fi
    run_manifest "$2" "$3"
    ;;
  *)
    echo "usage: $0 --check-manifest | --run PID WINDOW_ID" >&2
    exit 2
    ;;
esac
