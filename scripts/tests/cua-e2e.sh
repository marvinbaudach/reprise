#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

runner="$repo_root/scripts/cua-e2e/run.sh"
if [[ ! -x "$runner" ]]; then
  echo "$runner must exist and be executable" >&2
  exit 1
fi
sort_scenario="$repo_root/scripts/cua-e2e/track_sort.sh"
tag_autocomplete_scenario="$repo_root/scripts/cua-e2e/tag_autocomplete.sh"
responsive_window_scenario="$repo_root/scripts/cua-e2e/responsive_window.sh"
helper_library="$repo_root/scripts/cua-e2e/lib.sh"
if [[ ! -f "$sort_scenario" ]]; then
  echo "$sort_scenario must exist" >&2
  exit 1
fi
for pattern in \
  'run_track_sort_playing_marker_scenario' \
  'TRACK_SORT_TOGGLE_COUNT=24' \
  'cua_click_window_point' \
  'assert_snapshot_contains "$ascending_path" "Playing Sentinel"' \
  'assert_snapshot_contains "$descending_path" "Sort Track 24"'
do
  if ! rg --quiet --fixed-strings "$pattern" "$sort_scenario"; then
    echo "$sort_scenario must contain sort-regression contract: $pattern" >&2
    exit 1
  fi
done
if [[ ! -f "$responsive_window_scenario" ]]; then
  echo "$responsive_window_scenario must exist" >&2
  exit 1
fi
for pattern in \
  'run_responsive_window_scenario' \
  '720 760 responsive-narrow' \
  '1200 420 responsive-short' \
  '720 420 responsive-combined' \
  'assert_full_player_controls_are_reachable' \
  'assert_only_track_table_overflows' \
  'responsive-play-long-title' \
  'Show columns' \
  'Use Compact Mode' \
  'responsive-short-panel' \
  'responsive-restore-library ctrl m'
do
  if ! rg --quiet --fixed-strings "$pattern" "$responsive_window_scenario"; then
    echo "$responsive_window_scenario must contain responsive geometry contract: $pattern" >&2
    exit 1
  fi
done
if [[ ! -f "$tag_autocomplete_scenario" ]]; then
  echo "$tag_autocomplete_scenario must exist" >&2
  exit 1
fi
for pattern in \
  'run_tag_autocomplete_surface_scenario' \
  'Cogitations' \
  'Cognitive Dissonance' \
  'Radio Cognac' \
  'cua_hotkey_focused' \
  'backspace tag-autocomplete-clear' \
  'cua_type_text_window' \
  'tag-autocomplete-open-display.png' \
  'import -window root'
do
  if ! rg --quiet --fixed-strings "$pattern" "$tag_autocomplete_scenario"; then
    echo "$tag_autocomplete_scenario must contain autocomplete rendering contract: $pattern" >&2
    exit 1
  fi
done
window_point_helper=$(sed -n '/^cua_click_window_point() {/,/^}/p' "$helper_library")
if ! rg --quiet --fixed-strings 'delivery_mode: "foreground"' \
  <<<"$window_point_helper"; then
  echo "window-point clicks must use foreground delivery for non-actionable headers" >&2
  exit 1
fi
scrobbling_runner="$repo_root/scripts/cua-e2e/scrobbling.sh"
if [[ ! -f "$scrobbling_runner" ]]; then
  echo "$scrobbling_runner must exist" >&2
  exit 1
fi
for pattern in \
  'source "\$repo_root/scripts/cua-e2e/scrobbling.sh"' \
  'dbus-run-session' \
  '-u GNOME_KEYRING_CONTROL' \
  '-u GNOME_KEYRING_PID' \
  'XDG_DATA_HOME=' \
  'XDG_CACHE_HOME=' \
  'GDK_BACKEND=x11' \
  'WAYLAND_DISPLAY=' \
  'GTK_A11Y=atspi' \
  'NO_AT_BRIDGE=0' \
  'REPRISE_AUDIO_SINK=fakesink' \
  'Xvfb did not allocate a private display' \
  'first-run wizard presented' \
  'first-run setup completed' \
  'dev scan complete' \
  'smoke-quit timer fired' \
  'run-manifest.txt' \
  'run_fresh_install_scenario' \
  'run_populated_library_scenario' \
  'run_private_scenario_group' \
  'CUA_E2E_PRIVATE_GROUP=' \
  'for scenario_group in' \
  'dbus-run-session ffmpeg gdbus gnome-keyring-daemon import jq python3 rg timeout wmctrl' \
  'CUA_DRIVER_SOCKET=' \
  'CUA_DRIVER_RS_UPDATE_CHECK=0' \
  'CUA_E2E_DRIVER_TIMEOUT_SECS=' \
  'CUA_E2E_KEYBOARD_GROUP=primary' \
  'CUA_E2E_KEYBOARD_GROUP=secondary' \
  '"\$CUA_DRIVER_BIN" serve --no-overlay' \
  'cua_driver status' \
  'cua_driver list_windows' \
  'sine=frequency=440:duration=120' \
  'CUA_E2E_KEYBOARD_QUIT_DELAY_SECS=.*150' \
  'restart_private_cua_daemon' \
  'wmctrl' \
  'REPRISE_SMOKE_TAG_EDIT=' \
  'REPRISE_SMOKE_FOCUS_STATE=' \
  'CUA_E2E_APP_PID=""' \
  'run_tag_1_no_jump_after_save_scenario' \
  'run_tag_3_multi_dialog_structure_scenario' \
  'run_tag_autocomplete_surface_scenario' \
  'run_library_doctor_scenario' \
  'run_song_visuals_scenario' \
  'run_scrobbling_scenario' \
  'run_responsive_window_scenario' \
  'Audio-reactive song visual' \
  'song-visuals-visual-focus' \
  '"Play \(Space\)"' \
  'song-visuals-space-paused' \
  'cua_activate_main_menu_item' \
  'Enable Library Doctor' \
  'safe_change_count' \
  'browse-3-sidebar-escapes-doctor' \
  'nav-7-rescan-progress' \
  'Revert Last Cleanup'
do
  if ! rg --quiet -- "$pattern" "$runner"; then
    echo "$runner must contain isolation/coverage pattern: $pattern" >&2
    exit 1
  fi
done
for obsolete_visual_mode in '"Grid"' '"Bars"' '"Flow"' '"Pulse"'; do
  if rg --quiet --fixed-strings "$obsolete_visual_mode" "$runner"; then
    echo "$runner must not require the removed visual mode selector: $obsolete_visual_mode" >&2
    exit 1
  fi
done
for obsolete_song_visuals_contract in \
  '"Rings"' \
  '"F11 Fullscreen · color follows the cover accent"' \
  '"Song analysis"'; do
  if rg --quiet --fixed-strings "$obsolete_song_visuals_contract" "$runner"; then
    echo "$runner must not retain removed Song Visuals contract: $obsolete_song_visuals_contract" >&2
    exit 1
  fi
done
for pattern in \
  'start_scrobbling_keyring' \
  'start_scrobbling_api' \
  'REPRISE_SMOKE_LISTENBRAINZ_API_ROOT=' \
  'REPRISE_SMOKE_LASTFM_API_ROOT=' \
  'REPRISE_SMOKE_LASTFM_AUTH_ROOT=' \
  'ListenBrainz connected after restart' \
  'Last.fm connected after restart' \
  'Reprise Smoke Track' \
  'Reprise Last.fm Smoke Track' \
  'ListenBrainz disconnected after restart' \
  'Last.fm disconnected after restart'; do
  if ! rg --quiet --fixed-strings "$pattern" "$scrobbling_runner"; then
    echo "$scrobbling_runner must cover the provider lifecycle: $pattern" >&2
    exit 1
  fi
done
python3 "$repo_root/scripts/cua-e2e/scrobbling_api.py" --self-test
if rg --quiet 'cua_click_label .*"Main menu"' "$runner"; then
  echo "$runner must open the main menu through its F10 keyboard contract" >&2
  exit 1
fi
if ! rg --quiet 'cua_hotkey "\$pid" "\$window_id" "\$stem-f10" f10' "$runner"; then
  echo "$runner must deliver F10 through the proven hotkey transport" >&2
  exit 1
fi
if ! rg --quiet 'cua_focus_label_via_key .*"\$label" down "\$stem-focus"' "$runner"; then
  echo "$runner must discover detached main-menu popups through GTK focus evidence" >&2
  exit 1
fi
if ! rg --quiet 'CUA_E2E_FOCUS_STATE="\$focus_state"' "$runner"; then
  echo "$runner must expose each scenario focus probe to CUA snapshots" >&2
  exit 1
fi
if rg --quiet 'wait_for_label .*"Library Doctor" .*menu' "$runner"; then
  echo "$runner must not expect detached popup labels in the main-window snapshot" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings 'safe_change_count=$((fixture_count * 3))' "$runner"; then
  echo "$runner must derive the Library Doctor safe-review label from its fixture" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings \
  '"$APP_PID" "$WINDOW_ID" 110 115 browse-3-sidebar-escapes-doctor' \
  "$runner"; then
  echo "$runner must use window-local screenshot coordinates for the Music row" >&2
  exit 1
fi
if rg --quiet --fixed-strings '"Review Safe Fixes"' "$runner"; then
  echo "$runner must not wait for the obsolete static Library Doctor review label" >&2
  exit 1
fi
if rg --quiet --fixed-strings \
  'assert_snapshot_contains "$reverted_path" "Library Doctor"' \
  "$runner"; then
  echo "$runner must verify the completed Library Doctor revert result, not a structural page title" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings \
  '"$APP_PID" "$WINDOW_ID" "Tags reverted · $fixture_count tracks" doctor-reverted' \
  "$runner"; then
  echo "$runner must wait for the completed Library Doctor revert result" >&2
  exit 1
fi
if rg --quiet --fixed-strings \
  'cua_click_label "$APP_PID" "$WINDOW_ID" "Plugins" doctor-plugins-page' \
  "$runner"; then
  echo "$runner must not pixel-click the geometry-less Preferences Plugins label" >&2
  exit 1
fi
for pattern in \
  'cua_focus_label_via_key "$APP_PID" "$WINDOW_ID" "Plugins" down doctor-plugins-focus' \
  'cua_press_key_window "$APP_PID" "$WINDOW_ID" enter doctor-plugins-enter'; do
  if ! rg --quiet --fixed-strings "$pattern" "$runner"; then
    echo "$runner must activate the Plugins navigation row through GTK focus: $pattern" >&2
    exit 1
  fi
done
doctor_scenario=$(sed -n '/^run_library_doctor_scenario() {/,/^}/p' "$runner")
for pattern in \
  'doctor-plugin-no-toggle' \
  'doctor-tool-close' \
  'doctor-tool-entry' \
  'doctor-revert-available'; do
  if ! rg --quiet --fixed-strings "$pattern" <<<"$doctor_scenario"; then
    echo "the always-available Library Doctor path must reopen its tool page: $pattern" >&2
    exit 1
  fi
done
line_for_doctor_step() {
  rg --line-number --max-count 1 --fixed-strings "$1" <<<"$doctor_scenario" \
    | sed 's/:.*//'
}
no_toggle_line=$(line_for_doctor_step doctor-plugin-no-toggle)
close_line=$(line_for_doctor_step doctor-tool-close)
entry_line=$(line_for_doctor_step doctor-tool-entry)
revert_line=$(line_for_doctor_step doctor-revert-available)
if ! ((no_toggle_line < close_line && close_line < entry_line && entry_line < revert_line)); then
  echo "the Library Doctor path must verify no toggle, close, reopen, then expose Revert" >&2
  exit 1
fi
for scenario_case in \
  'fresh-install)' \
  'tag-1-no-jump-after-save)' \
  'tag-3-multi-dialog-structure)'; do
  if ! rg --quiet --fixed-strings "$scenario_case" "$runner"; then
    echo "$runner must support isolated scenario: $scenario_case" >&2
    exit 1
  fi
done
if ! rg --quiet --fixed-strings 'library-doctor)' "$runner"; then
  echo "$runner must support isolated scenario: library-doctor)" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings 'song-visuals)' "$runner"; then
  echo "$runner must support isolated scenario: song-visuals)" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings 'track-sort-playing-marker)' "$runner"; then
  echo "$runner must support isolated scenario: track-sort-playing-marker)" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings 'scrobbling)' "$runner"; then
  echo "$runner must support isolated scenario: scrobbling)" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings 'cargo build --locked -p reprise-gnome' "$runner"; then
  echo "$runner must rebuild Reprise from the commit recorded in its manifest" >&2
  exit 1
fi

keyboard_runner="$repo_root/scripts/cua-e2e/keyboard.sh"
keyboard_manifest="$repo_root/scripts/cua-e2e/keyboard-surfaces.tsv"
if [[ ! -x "$keyboard_runner" ]]; then
  echo "$keyboard_runner must exist and be executable" >&2
  exit 1
fi
if [[ ! -f "$keyboard_manifest" ]]; then
  echo "$keyboard_manifest must exist" >&2
  exit 1
fi

expected_surfaces=(
  app-shell
  sidebar
  tracks-playlist-queue
  issues-import
  player-now-playing
  device-sync
  preferences
  modals
  stats
  compact-minimal
)
mapfile -t actual_surfaces < <(awk -F '\t' '!/^#/ && NF {print $1}' "$keyboard_manifest")
if [[ "${actual_surfaces[*]}" != "${expected_surfaces[*]}" ]]; then
  echo "keyboard manifest must cover every GUI surface exactly once" >&2
  printf 'expected: %s\nactual:   %s\n' \
    "${expected_surfaces[*]}" "${actual_surfaces[*]}" >&2
  exit 1
fi
"$keyboard_runner" --check-manifest
if CUA_E2E_KEYBOARD_GROUP=unknown "$keyboard_runner" --check-manifest 2>/dev/null; then
  echo "$keyboard_runner must reject unknown keyboard surface groups" >&2
  exit 1
fi
for pattern in \
  'acc-1-keyboard-only-surface-sweep' \
  'acc-3-tab-order-and-roving-collections' \
  'acc-4a-space-routes-global-and-local-controls' \
  'acc-5-transients-and-navigation-restore-focus' \
  'acc-8-direct-manipulation-has-keyboard-equivalence' \
  'cua_hotkey' \
  'cua_hotkey_focused' \
  'cua_focus_label_via_tab' \
  'cua_focus_label_via_key' \
  'cua_resize_window' \
  'reset_surface_is_at_baseline' \
  'surface_in_group' \
  'assert_after_has_focus'; do
  if ! rg --quiet "$pattern" "$keyboard_runner"; then
    echo "$keyboard_runner must contain keyboard acceptance pattern: $pattern" >&2
    exit 1
  fi
done
for pattern in \
  'reset_surface_is_at_baseline "$pid" "$window_id" "$stem"' \
  'assert_snapshot_contains "$state_path" "sine_01"'; do
  if ! rg --quiet --fixed-strings "$pattern" "$keyboard_runner"; then
    echo "the keyboard reset must restore the canonical TrackList: $pattern" >&2
    exit 1
  fi
done
if rg --quiet 'focus_active_library_tab|keyboard_(albums|artists)' "$keyboard_runner"; then
  echo "$keyboard_runner must not retain removed Library mode-switch flows" >&2
  exit 1
fi
for pattern in \
  'SPACE_TOGGLE_REGRESSION_COUNT=6' \
  '"$pid" "$window_id" "Toggle sidebar" acc-player-sidebar-toggle-focus' \
  'assert_focus_evidence_label "$state_path" "Toggle sidebar"' \
  'assert_snapshot_contains "$state_path" "Music"' \
  '"$pid" "$window_id" "Music" acc-issues-main-collection' \
  '"$pid" "$window_id" "Missing files" down acc-issues-focus' \
  '"$pid" "$window_id" "Music" acc-stats-main-collection' \
  '"$pid" "$window_id" "My Stats" down acc-stats-focus' \
  'cua_hotkey_focused "$pid" "$window_id" acc-device-close ctrl w' \
  'cua_hotkey_focused "$pid" "$window_id" acc-preferences-open ctrl comma' \
  'cua_hotkey "$pid" "$window_id" acc-stats-return alt left' \
  'assert_snapshot_absent "$CUA_E2E_OUT_DIR/acc-compact-open-after.json" "Search all fields"' \
  'assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-compact-close-after.json" Title'; do
  if ! rg --quiet --fixed-strings "$pattern" "$keyboard_runner"; then
    echo "$keyboard_runner must preserve playback while traversing later surfaces: $pattern" >&2
    exit 1
  fi
done
if rg --quiet 'keyboard_pending' "$keyboard_runner"; then
  echo "$keyboard_runner must not retain pending keyboard surfaces" >&2
  exit 1
fi
if rg --quiet 'cua_(click|double_click|pointer_action)' "$keyboard_runner"; then
  echo "$keyboard_runner must remain keyboard-only" >&2
  exit 1
fi

# shellcheck source=../cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

export CUA_E2E_OUT_DIR="$tmp_root/output"
export CUA_E2E_CALL_LOG="$tmp_root/calls.log"
export CUA_E2E_SNAPSHOT_COUNT="$tmp_root/snapshot-count"
export CUA_DRIVER_SOCKET="$tmp_root/private-cua-driver.sock"
mkdir -p "$CUA_E2E_OUT_DIR" "$tmp_root/bin"

# cua-driver indexes actionable nodes but keeps non-actionable semantic groups
# only in tree_markdown. Visible status text such as GtkColumnView's
# "No results" group must still participate in presence/absence waits.
jq -n '{
  degraded: false,
  elements: [],
  tree_markdown: "- group = \"No results\""
}' >"$tmp_root/tree-only-status.json"
if ! assert_snapshot_contains "$tmp_root/tree-only-status.json" "No results"; then
  echo "snapshot assertions must include non-indexed semantic tree nodes" >&2
  exit 1
fi
if assert_snapshot_absent "$tmp_root/tree-only-status.json" "No results" 2>/dev/null; then
  echo "absence assertions must reject non-indexed semantic tree nodes" >&2
  exit 1
fi

cat >"$tmp_root/bin/cua-driver" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail

tool=$1
payload=${2:-'{}'}
if [[ ${3:-} != --socket || ${4:-} != "$CUA_DRIVER_SOCKET" ]]; then
  echo "CUA calls must target the private daemon socket" >&2
  exit 2
fi
printf '%s\t%s\n' "$tool" "$payload" >>"$CUA_E2E_CALL_LOG"

case "$tool" in
  get_window_state)
    screenshot_path=$(jq -r '.screenshot_out_file' <<<"$payload")
    printf 'fake screenshot\n' >"$screenshot_path"
    count=0
    if [[ -f "$CUA_E2E_SNAPSHOT_COUNT" ]]; then
      count=$(<"$CUA_E2E_SNAPSHOT_COUNT")
    fi
    count=$((count + 1))
    printf '%s' "$count" >"$CUA_E2E_SNAPSHOT_COUNT"
    case "$count" in
      1)
        label="Skip for Now"
        index=2
        ;;
      2)
        label="No music yet"
        index=3
        ;;
      3)
        label="Search all fields"
        index=4
        ;;
      4|5)
        label="Search all fields"
        index=5
        ;;
      6)
        label="No results"
        index=6
        ;;
      7|8|9|15|16)
        jq -n --argjson focused "$([[ "$count" == 8 || "$count" == 9 || "$count" == 15 || "$count" == 16 ]] && echo true || echo false)" '{
          degraded: false,
          structuredContent: {
            elements: [
              {element_index: 9, role: "panel", label: "Library", parent_index: null, depth: 0},
              {element_index: 10, role: "list item", label: "sine_01", parent_index: 9,
               depth: 1, focused: $focused,
               states: (if $focused then ["focused"] else [] end)}
            ]
          },
          tree_markdown: "Library > sine_01"
        }'
        exit 0
        ;;
      10|11|12|13|14)
        jq -n --argjson show_no_results "$([[ "$count" == 13 || "$count" == 14 ]] && echo true || echo false)" '{
          degraded: false,
          structuredContent: {
            elements: ([
              {element_index: 9, role: "panel", label: "Library", parent_index: null, depth: 0},
              {element_index: 11, role: "entry", label: "Search all fields", parent_index: 9,
               depth: 1, focused: true, states: ["focused"]}
            ] + if $show_no_results then [
              {element_index: 12, role: "label", label: "No results", parent_index: 9, depth: 1}
            ] else [] end)
          },
          tree_markdown: "Library > Search all fields"
        }'
        exit 0
        ;;
    esac
    jq -n --arg label "$label" --argjson index "$index" '{
      degraded: false,
      structuredContent: {
        elements: [{element_index: $index, role: "button", label: $label,
          frame: {x: 10, y: 20, w: 100, h: 30}}]
      },
      tree_markdown: ("[element_index " + ($index | tostring) + "] " + $label)
    }'
    ;;
  click)
    if jq -e '.element_index == 2' <<<"$payload" >/dev/null; then
      jq -n '{effect: "suspected_noop", verified: false,
        escalation: {recommended: "px", reason: "test fallback"}}'
    elif jq -e '.element_index == 4' <<<"$payload" >/dev/null; then
      jq -n '{effect: "confirmed", verified: true}'
    elif jq -e '.x == 60 and .y == 35' <<<"$payload" >/dev/null; then
      jq -n '{effect: "unverifiable", verified: false}'
    else
      echo "unexpected click payload: $payload" >&2
      exit 2
    fi
    jq -e '.pid == 42 and .window_id == 7' <<<"$payload" >/dev/null
    ;;
  type_text)
    if jq -e '.element_index == 5 and .text == "nomatch"' <<<"$payload" >/dev/null; then
      jq -n '{effect: "confirmed", verified: true}'
    elif jq -e '(.element_index | not) and .text == "nomatch"
      and .delivery_mode == "foreground"' <<<"$payload" >/dev/null; then
      jq -n '{delivery_mode: "foreground", verified: false}'
    else
      echo "unexpected type_text payload: $payload" >&2
      exit 2
    fi
    jq -e '.pid == 42 and .window_id == 7' <<<"$payload" >/dev/null
    ;;
  press_key)
    if jq -e '.element_index == 10 and .key == "enter"' <<<"$payload" >/dev/null; then
      if jq -e '.delivery_mode == "foreground"' <<<"$payload" >/dev/null; then
        jq -n '{delivery_mode: "foreground", verified: false}'
      else
        jq -n '{code: "background_unavailable",
          detail: "the remaining XTest/X11 route needs the target window focused",
          suggestion: "Retry with delivery_mode foreground"}'
      fi
    elif jq -e '(.element_index | not) and .key == "escape"' <<<"$payload" >/dev/null; then
      jq -n '{effect: "confirmed", verified: true}'
    else
      echo "unexpected press_key payload: $payload" >&2
      exit 2
    fi
    jq -e '.pid == 42 and .window_id == 7' <<<"$payload" >/dev/null
    ;;
  hotkey)
    jq -e '.pid == 42 and .window_id == 7 and .keys == ["ctrl", "f"]' \
      <<<"$payload" >/dev/null
    if jq -e '.delivery_mode == "foreground"' <<<"$payload" >/dev/null; then
      jq -n '{delivery_mode: "foreground", verified: false}'
    else
      jq -n '{
        code: "background_unavailable",
        detail: "the remaining XTest/X11 route needs the target window focused",
        suggestion: "Retry with delivery_mode foreground"
      }'
    fi
    ;;
  bring_to_front)
    jq -e '.pid == 42 and .window_id == 7' <<<"$payload" >/dev/null
    jq -n '{platform: "linux", prior_active: 7, window_id: 7}'
    ;;
  *)
    echo "unexpected fake cua-driver tool: $tool" >&2
    exit 2
    ;;
esac
FAKE
chmod +x "$tmp_root/bin/cua-driver"
export CUA_DRIVER_BIN="$tmp_root/bin/cua-driver"

cua_click_label 42 7 "Skip for Now" "fresh-install-skip"
if [[ ! -s "$CUA_E2E_OUT_DIR/fresh-install-skip-after.png" ]]; then
  echo "every real CUA snapshot must retain screenshot evidence" >&2
  exit 1
fi
assert_snapshot_contains \
  "$CUA_E2E_OUT_DIR/fresh-install-skip-after.json" \
  "No music yet"

cua_click_label 42 7 "Search all fields" "populated-search-toggle"
cua_type_text_label 42 7 "Search all fields" "nomatch" "populated-search"
assert_snapshot_contains \
  "$CUA_E2E_OUT_DIR/populated-search-after.json" \
  "No results"

cua_press_key_label 42 7 "sine_01" enter "keyboard-enter"
track_focused="$CUA_E2E_OUT_DIR/keyboard-enter-after.json"
assert_focused_label "$track_focused" "sine_01"
assert_focus_within "$track_focused" "Library"

cua_hotkey 42 7 "keyboard-search" ctrl f
assert_focused_label \
  "$CUA_E2E_OUT_DIR/keyboard-search-after.json" \
  "Search all fields"

cua_type_text_window 42 7 nomatch "keyboard-search-type"
assert_snapshot_contains \
  "$CUA_E2E_OUT_DIR/keyboard-search-type-after.json" \
  "No results"

cua_press_key_window 42 7 escape "keyboard-escape"
assert_snapshot_absent \
  "$CUA_E2E_OUT_DIR/keyboard-escape-after.json" \
  "No results"
assert_focus_returns_to \
  "$track_focused" \
  "$CUA_E2E_OUT_DIR/keyboard-escape-after.json" \
  "sine_01"

jq -n '{effect: "suspected_noop"}' >"$tmp_root/suspected-noop.json"
if assert_action_landed "$tmp_root/suspected-noop.json" 2>/dev/null; then
  echo "suspected CUA no-ops must fail closed" >&2
  exit 1
fi
jq -n '{effect: "unverifiable", path: "ax", verified: false}' \
  >"$tmp_root/unverifiable-ax.json"
if ! assert_action_landed "$tmp_root/unverifiable-ax.json"; then
  echo "dispatched AX actions may be verified by the required after snapshot" >&2
  exit 1
fi
jq -n '{effect: "confirmed", escalation: {recommended: "foreground"}}' \
  >"$tmp_root/escalation.json"
if assert_action_landed "$tmp_root/escalation.json" 2>/dev/null; then
  echo "CUA escalation recommendations must fail closed" >&2
  exit 1
fi
printf '✅ Double-clicked element [38].\n' >"$tmp_root/text-success.txt"
if ! assert_action_landed "$tmp_root/text-success.txt"; then
  echo "successful human-readable CUA results must be accepted" >&2
  exit 1
fi
printf 'CUA action failed\n' >"$tmp_root/text-failure.txt"
if assert_action_landed "$tmp_root/text-failure.txt" 2>/dev/null; then
  echo "unknown human-readable CUA results must fail closed" >&2
  exit 1
fi
jq -n '{
  degraded: false,
  structuredContent: {
    elements: [{element_index: 1, role: "button", label: "Albums", states: []}]
  }
}' >"$tmp_root/missing-focus.json"
if assert_focused_label "$tmp_root/missing-focus.json" "Albums" 2>/dev/null; then
  echo "focus assertions must reject elements without focused state" >&2
  exit 1
fi
cp "$tmp_root/missing-focus.json" "$tmp_root/focus-fallback.json"
printf 'widget=GtkColumnView\n' >"$tmp_root/focus-fallback-focus.txt"
if ! assert_unique_focus "$tmp_root/focus-fallback.json"; then
  echo "GTK focus evidence must cover CUA versions without focused states" >&2
  exit 1
fi
if ! assert_focus_evidence_not "$tmp_root/focus-fallback.json" GtkText; then
  echo "focus evidence must distinguish active content from the search text" >&2
  exit 1
fi
printf 'widget=GtkListBoxRow\nlabel=Music\n' \
  >"$tmp_root/focus-fallback-focus.txt"
if ! assert_focus_evidence_label "$tmp_root/focus-fallback.json" Music; then
  echo "GTK focus evidence must identify the active collection item" >&2
  exit 1
fi

if cua_press_key_label 99 7 "Albums" enter "wrong-pid" 2>/dev/null; then
  echo "keyboard delivery to the wrong PID must fail" >&2
  exit 1
fi

mapfile -t calls < <(cut -f1 "$CUA_E2E_CALL_LOG")
expected=(
  get_window_state
  click
  click
  get_window_state
  get_window_state
  click
  get_window_state
  get_window_state
  type_text
  get_window_state
  get_window_state
  press_key
  press_key
  get_window_state
  get_window_state
  hotkey
  bring_to_front
  get_window_state
  hotkey
  get_window_state
  get_window_state
  type_text
  get_window_state
  get_window_state
  press_key
  get_window_state
  get_window_state
  press_key
)
if [[ "${calls[*]}" != "${expected[*]}" ]]; then
  echo "CUA actions must be bracketed by fresh before/after snapshots" >&2
  printf 'expected: %s\nactual:   %s\n' "${expected[*]}" "${calls[*]}" >&2
  exit 1
fi

echo "CUA E2E helper contract passed"
