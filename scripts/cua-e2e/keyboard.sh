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

# Returns the app to the state every scenario assumes: Tracks visible, no
# search filter, nothing popped up.
#
# The manifest drives twelve scenarios against ONE app instance. Nothing used to
# reset between them, and it stayed invisible only because the runner died on
# the first red surface. Once it ran the whole list, one leaked search filter
# from `app-shell` cascaded: `tracks` searched a list filtered to zero rows,
# `albums` pressed Enter on the wrong target and opened the main menu, `artists`
# ran trapped inside that popover and toggled Compact Mode, and the last two
# scenarios then measured a 430x76 mini-player. Five failures, one leak.
#
# This must verify rather than hope. A reset that silently fails would make the
# next surface fail for a reason that has nothing to do with what it tests —
# exactly the confusion it exists to prevent.
reset_surface_baseline() {
  local pid=$1 window_id=$2 stem=$3 state_path

  # Escape closes a popover or dialog; a second one collapses a revealed search
  # bar. Both are no-ops when nothing is open.
  cua_press_key_window "$pid" "$window_id" escape "$stem-reset-1" || return 1
  cua_press_key_window "$pid" "$window_id" escape "$stem-reset-2" || return 1

  # Escape does not undo navigation, and several scenarios end on a different
  # view — `issues-import` finishes inside Missing files. Use the documented
  # Back accelerator (help.rs lists <Alt>Left as "Back to previous view"),
  # which is what a keyboard user would reach for and is one keystroke rather
  # than a traversal. Repeat it: a scenario may be several places deep, and
  # Back on an empty history is a no-op.
  #
  # An earlier version of this comment claimed the sidebar is absent from the
  # accessibility tree on that view and therefore cannot be tabbed to. That was
  # wrong — Music, Queue and My Stats are all present, at the same counts as in
  # the working sidebar scenario. The tab-based reset failed because the run's
  # window manager had died and no key was reaching the app at all, not because
  # of anything about this view. Alt+Left remains the better reset; the reason
  # given for it was not.
  local attempt
  for attempt in 1 2 3 4; do
    cua_hotkey "$pid" "$window_id" "$stem-reset-back-$attempt" alt left || return 1
  done

  # Verify against something only the Tracks view has. "Tracks" is the switcher
  # button and is present in every view, and `sine_01` shows up in the album
  # cards and the Now Playing panel too — an earlier version of this check used
  # both and therefore passed while the app sat on Albums, which is precisely
  # the silent-reset failure this function exists to prevent. Column headers
  # only exist on the track table: 6 occurrences there, 0 on Albums.
  state_path=$(cua_wait_for_label "$pid" "$window_id" "Title" "$stem-reset-state") || {
    echo "reset before '$stem' did not restore the Tracks baseline; the previous" >&2
    echo "surface left state behind that this scenario cannot run against" >&2
    return 1
  }
  assert_snapshot_contains "$state_path" "sine_01" || return 1
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

# The view switcher is a toggle group, which GTK exposes as a SINGLE tab stop:
# Tab reaches the group, arrow keys move between members. Tabbing for "Albums"
# or "Artists" directly therefore never lands — the old version walked the ring
# repeatedly and pressed Enter on wherever it stopped, once on the unlabeled
# main-menu button, which opened a popover and left Compact Mode on two
# scenarios later.
#
# The group is entered by tabbing to the ACTIVE member, which is labelled after
# the current view — "Tracks" while the track table is showing (measured: it is
# stop 14 of the ring there). Hence the reset above must genuinely return to
# Tracks first; when it silently did not, this traversal failed for that reason
# rather than for anything about the switcher.
keyboard_albums() {
  local pid=$1 window_id=$2 focus_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" Tracks acc-albums-group)
  assert_focus_evidence_label "$focus_path" Tracks
  focus_path=$(cua_focus_label_via_key \
    "$pid" "$window_id" Albums right acc-albums-focus)
  assert_focus_evidence_label "$focus_path" Albums
  cua_press_key_window "$pid" "$window_id" enter acc-albums-open
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-albums-open-after.json" Albums
  assert_after_has_focus acc-albums-open
}

keyboard_artists() {
  local pid=$1 window_id=$2 focus_path
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" Tracks acc-artists-group)
  assert_focus_evidence_label "$focus_path" Tracks
  focus_path=$(cua_focus_label_via_key \
    "$pid" "$window_id" Artists right acc-artists-focus)
  assert_focus_evidence_label "$focus_path" Artists
  cua_press_key_window "$pid" "$window_id" enter acc-artists-open
  assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-artists-open-after.json" Artists
  assert_after_has_focus acc-artists-open
}

keyboard_player_now_playing() {
  local pid=$1 window_id=$2 focus_path state_path
  # This scenario waits for "Pause (Space)", which only exists while something
  # plays — nothing here started playback, so it depended on a previous surface
  # having left a track running. Establish the precondition instead: focus a
  # track and activate it.
  focus_path=$(cua_focus_label_via_tab \
    "$pid" "$window_id" sine_01 acc-player-precondition)
  assert_focus_evidence_label "$focus_path" sine_01
  cua_press_key_window "$pid" "$window_id" enter acc-player-precondition-play
  cua_wait_for_label "$pid" "$window_id" "Pause (Space)" acc-player-playing >/dev/null

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
  # Run every surface and report one balance sheet, rather than dying on the
  # first red one. Stopping early cannot establish how many of the remaining
  # surfaces pass, and it hid genuine failures for a long time: while the
  # app-shell Escape defect stood, `sidebar` onwards had never run once, so
  # "one failure" read as the whole story when it was the first of an unknown
  # number. Same rule the Rust display runner follows (see RELEASING.md).
  local -a failed=()
  local passed=0
  while IFS=$'\t' read -r surface scenario; do
    [[ -z "$surface" || "$surface" == \#* ]] && continue
    echo "[cua-keyboard] $surface"
    if ! reset_surface_baseline "$pid" "$window_id" "$surface"; then
      echo "[cua-keyboard] $surface: reset failed, surface not exercised" >&2
      failed+=("$surface (reset)")
      continue
    fi
    if "$scenario" "$pid" "$window_id"; then
      passed=$((passed + 1))
    else
      failed+=("$surface")
    fi
  done <"$manifest"

  echo "[cua-keyboard] surfaces passed: $passed, failed: ${#failed[@]}"
  if ((${#failed[@]} > 0)); then
    printf '[cua-keyboard] FAILED surface: %s\n' "${failed[@]}" >&2
    return 1
  fi
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
