#!/usr/bin/env bash

# Two-axis main-window geometry and visual evidence. Sourced by run.sh.

assert_snapshot_fits_requested_size() {
  local snapshot_path=$1 max_width=$2 max_height=$3 label=$4
  local width height

  width=$(jq -er '.screenshot_width' "$snapshot_path")
  height=$(jq -er '.screenshot_height' "$snapshot_path")
  if ((width > max_width || height > max_height)); then
    echo "$label exceeded ${max_width}x${max_height}: ${width}x${height}" >&2
    return 1
  fi
}

assert_snapshot_content_stays_inside_window() {
  local snapshot_path=$1 label=$2

  if ! jq -e '
    (.structuredContent.elements // .elements // []) as $elements
    | [$elements[] | select(.role == "window")][0].frame as $window
    | all(
        $elements[];
        (.frame.x == null or .frame.y == null or .frame.w == null or .frame.h == null)
        or (
          .frame.x >= $window.x
          and .frame.y >= $window.y
          and .frame.x + .frame.w <= $window.x + $window.w
          and .frame.y + .frame.h <= $window.y + $window.h
        )
      )
  ' "$snapshot_path" >/dev/null; then
    echo "$label contains UI allocated outside the window" >&2
    jq -r '
      (.structuredContent.elements // .elements // []) as $elements
      | [$elements[] | select(.role == "window")][0].frame as $window
      | $elements[]
      | select(
          .frame.x != null and .frame.y != null
          and .frame.w != null and .frame.h != null
          and (
            .frame.x < $window.x
            or .frame.y < $window.y
            or .frame.x + .frame.w > $window.x + $window.w
            or .frame.y + .frame.h > $window.y + $window.h
          )
        )
      | "\(.role) \(.label | @json) \(.frame)"
    ' "$snapshot_path" >&2
    return 1
  fi
}

assert_full_player_controls_are_reachable() {
  local snapshot_path=$1

  for label in "Reveal playing album" "Playback position" "Volume"; do
    assert_snapshot_contains "$snapshot_path" "$label"
  done
  if ! snapshot_exposes_label "$snapshot_path" "Play (Space)" \
    && ! snapshot_exposes_label "$snapshot_path" "Pause (Space)"; then
    echo "snapshot exposes neither Play nor Pause: $snapshot_path" >&2
    return 1
  fi
  if ! jq -e '
    [(.structuredContent.elements // .elements // [])[]
      | select(
          .role == "label"
          and ((.label // "") | test("^[−-]?[0-9]+:[0-9]{2}$"))
        )]
    | length >= 2
  ' "$snapshot_path" >/dev/null; then
    echo "snapshot does not expose both player time labels: $snapshot_path" >&2
    return 1
  fi
}

assert_player_controls_stay_inside_window() {
  local snapshot_path=$1 label=$2

  if ! jq -e '
    (.structuredContent.elements // .elements // []) as $elements
    | [$elements[] | select(.role == "window")][0].frame as $window
    | all(
        $elements[]
          | select(
              .label == "Reveal playing album"
              or .label == "Play (Space)"
              or .label == "Pause (Space)"
              or .label == "Playback position"
              or .label == "Volume"
            );
        .frame.x >= $window.x
        and .frame.y >= $window.y
        and .frame.x + .frame.w <= $window.x + $window.w
        and .frame.y + .frame.h <= $window.y + $window.h
      )
  ' "$snapshot_path" >/dev/null; then
    echo "$label clips a primary player control" >&2
    return 1
  fi
}

assert_only_track_table_overflows() {
  local snapshot_path=$1 label=$2

  if ! jq -e '
    (.structuredContent.elements // .elements // []) as $elements
    | [$elements[] | select(.role == "window")][0].frame as $window
    | [
        $elements[]
        | select(
            .frame.x != null and .frame.y != null
            and .frame.w != null and .frame.h != null
            and (
              .frame.x < $window.x
              or .frame.y < $window.y
              or .frame.x + .frame.w > $window.x + $window.w
              or .frame.y + .frame.h > $window.y + $window.h
            )
          )
      ]
    | all(.[]; .role == "row" or .role == "list")
  ' "$snapshot_path" >/dev/null; then
    echo "$label lets non-table UI overflow the window" >&2
    return 1
  fi
}

run_responsive_window_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/responsive-window-fixture"
  local long_title="Siren's Lament Dark Melodic Metalcore Instrumental Mix Cinematic Heavy Atmospheric Metal"
  local wide_path panels_closed_path panels_restored_path
  local narrow_path reopened_path short_path compact_path combined_path restored_path
  local short_panel_path

  echo "[cua-e2e] style-5/style-6/mini-5: responsive window geometry"
  mkdir -p "$fixture_dir"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=120 \
    -c:a flac "$fixture_dir/$long_title.flac"
  export REPRISE_SMOKE_FIRST_RUN=skip
  start_scenario_app responsive-window "$fixture_dir" "" 45
  wait_for_label "$APP_PID" "$WINDOW_ID" "Title" responsive-ready >/dev/null
  cua_double_click_label \
    "$APP_PID" "$WINDOW_ID" "$long_title" responsive-play-long-title
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter responsive-activate-long-title
  wait_for_label "$APP_PID" "$WINDOW_ID" "Pause (Space)" responsive-playing >/dev/null

  cua_resize_window "$APP_PID" "$WINDOW_ID" 1600 420 responsive-short
  short_path="$CUA_E2E_OUT_DIR/responsive-short-after-resize.json"
  assert_snapshot_fits_requested_size "$short_path" 1600 420 "short window"
  assert_snapshot_content_stays_inside_window "$short_path" "short window"
  assert_full_player_controls_are_reachable "$short_path"
  assert_snapshot_contains "$short_path" "Use Compact Mode"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Use Compact Mode" responsive-use-compact
  compact_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" responsive-compact)
  assert_snapshot_fits_requested_size "$compact_path" 430 76 "compact window"
  assert_snapshot_absent "$compact_path" "Search all fields"
  cua_hotkey "$APP_PID" "$WINDOW_ID" responsive-restore-library ctrl m
  wait_for_label "$APP_PID" "$WINDOW_ID" "Title" responsive-library-restored >/dev/null

  cua_resize_window "$APP_PID" "$WINDOW_ID" 1600 760 responsive-wide-panels
  wide_path="$CUA_E2E_OUT_DIR/responsive-wide-panels-after-resize.json"
  assert_snapshot_contains "$wide_path" "Recently played"
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Toggle Now Playing panel" responsive-open-wide-panel
  wait_for_label "$APP_PID" "$WINDOW_ID" "Up Next" responsive-wide-panel-open >/dev/null

  cua_resize_window "$APP_PID" "$WINDOW_ID" 1200 760 responsive-side-panels-closed
  panels_closed_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Side panels were closed to fit the window" \
    responsive-side-panels-toast)
  assert_snapshot_absent "$panels_closed_path" "Recently played"
  assert_snapshot_absent "$panels_closed_path" "Up Next"
  assert_snapshot_contains "$panels_closed_path" "Undo"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Undo" responsive-undo-side-panels
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Recently played" responsive-sidebar-restored >/dev/null
  panels_restored_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Up Next" responsive-now-playing-restored)
  assert_snapshot_contains "$panels_restored_path" "Recently played"
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Toggle Now Playing panel" responsive-close-restored-panel
  cua_click_label "$APP_PID" "$WINDOW_ID" "Toggle sidebar" responsive-close-restored-sidebar

  cua_resize_window "$APP_PID" "$WINDOW_ID" 720 760 responsive-narrow
  narrow_path="$CUA_E2E_OUT_DIR/responsive-narrow-after-resize.json"
  assert_snapshot_fits_requested_size "$narrow_path" 720 760 "narrow window"
  assert_snapshot_content_stays_inside_window "$narrow_path" "narrow window"
  assert_full_player_controls_are_reachable "$narrow_path"
  assert_snapshot_contains "$narrow_path" "Title"
  assert_snapshot_contains "$narrow_path" "Artist"
  assert_snapshot_absent "$narrow_path" "Album"
  assert_snapshot_contains "$narrow_path" "Show columns"

  # The private X11 AT-SPI bridge exposes every toast descendant at the
  # window origin, so its otherwise preferred element-index click cannot
  # target this action reliably. The window is exactly 720x760 here.
  cua_click_window_point "$APP_PID" "$WINDOW_ID" 510 710 responsive-show-columns
  reopened_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Album" responsive-columns-reopened)
  assert_snapshot_contains "$reopened_path" "Title"
  assert_snapshot_contains "$reopened_path" "Artist"
  cua_click_window_point "$APP_PID" "$WINDOW_ID" 605 710 responsive-dismiss-columns-toast
  cua_wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Show columns" responsive-columns-toast-dismissed >/dev/null

  cua_resize_window "$APP_PID" "$WINDOW_ID" 720 420 responsive-combined
  combined_path="$CUA_E2E_OUT_DIR/responsive-combined-after-resize.json"
  assert_snapshot_fits_requested_size "$combined_path" 720 420 "combined window"
  assert_full_player_controls_are_reachable "$combined_path"
  assert_player_controls_stay_inside_window "$combined_path" "combined window"
  assert_only_track_table_overflows "$combined_path" "combined window"

  cua_resize_window "$APP_PID" "$WINDOW_ID" 1200 760 responsive-restored
  restored_path="$CUA_E2E_OUT_DIR/responsive-restored-after-resize.json"
  assert_snapshot_fits_requested_size "$restored_path" 1200 760 "restored window"
  assert_full_player_controls_are_reachable "$restored_path"

  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Toggle Now Playing panel" responsive-open-panel
  wait_for_label "$APP_PID" "$WINDOW_ID" "Up Next" responsive-panel-open >/dev/null
  cua_resize_window "$APP_PID" "$WINDOW_ID" 1200 420 responsive-short-panel
  short_panel_path="$CUA_E2E_OUT_DIR/responsive-short-panel-after-resize.json"
  assert_snapshot_fits_requested_size "$short_panel_path" 1200 420 "short window with panel"
  assert_full_player_controls_are_reachable "$short_panel_path"
  assert_player_controls_stay_inside_window \
    "$short_panel_path" "short window with panel"

  finish_scenario responsive-window \
    "dev scan complete" \
    "first-run decision"
}
