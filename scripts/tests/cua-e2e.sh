#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

runner="$repo_root/scripts/cua-e2e/run.sh"
if [[ ! -x "$runner" ]]; then
  echo "$runner must exist and be executable" >&2
  exit 1
fi
for pattern in \
  'dbus-run-session' \
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
  'CUA_DRIVER_SOCKET=' \
  'cua_driver serve --no-overlay' \
  'cua_driver status' \
  'cua_driver list_windows' \
  'sine=frequency=440:duration=120' \
  'CUA_E2E_KEYBOARD_QUIT_DELAY_SECS=.*180' \
  'restart_private_cua_daemon' \
  'wmctrl' \
  'REPRISE_SMOKE_TAG_EDIT=' \
  'REPRISE_SMOKE_FOCUS_STATE=' \
  'run_tag_1_no_jump_after_save_scenario' \
  'run_tag_3_multi_dialog_structure_scenario' \
  'run_library_doctor_scenario' \
  'cua_activate_main_menu_item' \
  'Enable Library Doctor' \
  'Review Safe Fixes' \
  'Revert Last Cleanup'
do
  if ! rg --quiet "$pattern" "$runner"; then
    echo "$runner must contain isolation/coverage pattern: $pattern" >&2
    exit 1
  fi
done
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
  albums
  artists
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
for pattern in \
  'acc-1-keyboard-only-surface-sweep' \
  'acc-3-tab-order-and-roving-collections' \
  'acc-4-standard-keys-respect-local-controls' \
  'acc-5-transients-and-navigation-restore-focus' \
  'acc-8-direct-manipulation-has-keyboard-equivalence' \
  'cua_hotkey' \
  'cua_focus_label_via_tab' \
  'cua_focus_label_via_key' \
  'cua_resize_window' \
  'assert_after_has_focus'; do
  if ! rg --quiet "$pattern" "$keyboard_runner"; then
    echo "$keyboard_runner must contain keyboard acceptance pattern: $pattern" >&2
    exit 1
  fi
done
for pattern in \
  '"$pid" "$window_id" Tracks acc-albums-tabs' \
  '"$pid" "$window_id" Albums right acc-albums-focus' \
  '"$pid" "$window_id" Albums acc-artists-tabs' \
  '"$pid" "$window_id" Artists right acc-artists-focus'; do
  if ! rg --quiet --fixed-strings "$pattern" "$keyboard_runner"; then
    echo "$keyboard_runner must navigate the view switcher as one roving tab stop: $pattern" >&2
    exit 1
  fi
done
for pattern in \
  '"$pid" "$window_id" "Pause (Space)" acc-player-focus' \
  '"$pid" "$window_id" "Play (Space)" acc-player-paused' \
  '"$pid" "$window_id" "Music" acc-issues-main-collection' \
  '"$pid" "$window_id" "Missing files" down acc-issues-focus' \
  '"$pid" "$window_id" "Music" acc-stats-main-collection' \
  '"$pid" "$window_id" "My Stats" down acc-stats-focus' \
  'cua_hotkey "$pid" "$window_id" acc-stats-return alt left' \
  'assert_snapshot_absent "$CUA_E2E_OUT_DIR/acc-compact-open-after.json" "Search all fields"' \
  'assert_snapshot_contains "$CUA_E2E_OUT_DIR/acc-compact-close-after.json" Tracks'; do
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
              {element_index: 10, role: "list item", label: "Albums", parent_index: 9,
               depth: 1, focused: $focused,
               states: (if $focused then ["focused"] else [] end)}
            ]
          },
          tree_markdown: "Library > Albums"
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

cua_press_key_label 42 7 "Albums" enter "keyboard-enter"
albums_focused="$CUA_E2E_OUT_DIR/keyboard-enter-after.json"
assert_focused_label "$albums_focused" "Albums"
assert_focus_within "$albums_focused" "Library"

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
  "$albums_focused" \
  "$CUA_E2E_OUT_DIR/keyboard-escape-after.json" \
  "Albums"

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
