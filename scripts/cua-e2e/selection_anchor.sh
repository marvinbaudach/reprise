#!/usr/bin/env bash

# NAV-17 pointer and screenshot acceptance for selection-anchor restoration.
# Sourced by run.sh after the shared CUA and app-lifecycle helpers.

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
  local row rgb r g

  for row in 1 2 3 4 5; do
    rgb=$(magick "$png" -crop "1x1+800+$(anchor_row_y "$row")" -depth 8 txt:- \
      | tail -1 | sed -E 's/.*\(([0-9]+),([0-9]+),([0-9]+).*/\1 \2 \3/')
    read -r r g _ <<<"$rgb"
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
