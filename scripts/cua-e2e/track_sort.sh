#!/usr/bin/env bash

# Repeated-sort CUA regression for the now-playing marker registry. This file
# is sourced by run.sh after the shared CUA helpers; it deliberately owns only
# this focused scenario so the main runner stays below the code-file limit.

TRACK_SORT_FIXTURE_COUNT=24
TRACK_SORT_TOGGLE_COUNT=24
TRACK_SORT_WINDOW_WIDTH=1200
TRACK_SORT_WINDOW_HEIGHT=800
TRACK_SORT_TITLE_HEADER_LOCAL_X=348
TRACK_SORT_TITLE_HEADER_LOCAL_Y=142

prepare_track_sort_fixture() {
  local fixture_dir=$1 base_track=$2 index

  mkdir -p "$fixture_dir"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=120 \
    -c:a flac "$base_track"
  ffmpeg -hide_banner -loglevel error -y \
    -i "$base_track" -map 0:a -c:a copy \
    -metadata title="Playing Sentinel" \
    -metadata artist="AAA Playing Artist" \
    -metadata album_artist="AAA Playing Artist" \
    -metadata album="Sort Regression" \
    "$fixture_dir/playing_sentinel.flac"
  for index in $(seq 1 "$TRACK_SORT_FIXTURE_COUNT"); do
    ffmpeg -hide_banner -loglevel error -y \
      -i "$base_track" -map 0:a -c:a copy \
      -metadata title="Sort Track $(printf '%02d' "$index")" \
      -metadata artist="Sort Artist" \
      -metadata album_artist="Sort Artist" \
      -metadata album="Sort Regression" \
      "$fixture_dir/sort_track_$(printf '%02d' "$index").flac"
  done
}

track_sort_title_header_point() {
  local snapshot_path=$1

  jq -er \
    --argjson width "$TRACK_SORT_WINDOW_WIDTH" \
    --argjson height "$TRACK_SORT_WINDOW_HEIGHT" \
    --argjson local_x "$TRACK_SORT_TITLE_HEADER_LOCAL_X" \
    --argjson local_y "$TRACK_SORT_TITLE_HEADER_LOCAL_Y" '
      [(.structuredContent.elements // .elements // [])[]
        | select(.role == "window")
        | select(.frame.w == $width and .frame.h == $height)
        | [$local_x, $local_y]][0]
      | select(. != null)
      | @tsv
    ' "$snapshot_path"
}

assert_sort_direction_since() {
  local log_path=$1 first_line=$2 expected_direction=$3 scenario=$4
  local latest_sort

  latest_sort=$(tail -n "+$((first_line + 1))" "$log_path" \
    | rg 'query matched 25 tracks.*field.*title' \
    | tail -n 1)
  if [[ -z "$latest_sort" ]] \
    || ! rg --quiet "dir.*$expected_direction" <<<"$latest_sort"; then
    echo "$scenario did not apply title/$expected_direction sorting" >&2
    return 1
  fi
}

assert_snapshot_label_count_at_least() {
  local snapshot_path=$1 label=$2 minimum=$3

  if ! jq -e --arg label "$label" --argjson minimum "$minimum" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.label == $label)]
    | length >= $minimum
  ' "$snapshot_path" >/dev/null; then
    echo "snapshot exposes fewer than $minimum '$label' labels: $snapshot_path" >&2
    return 1
  fi
}

run_track_sort_playing_marker_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/track-sort-fixture-music"
  local base_track="$CUA_E2E_SCRATCH_ROOT/track-sort-base.flac"
  local header_state ascending_path descending_path
  local header_x header_y sort_log_line round sort_query_count

  echo "[cua-e2e] nav-10a: repeated title sorts keep the playing marker responsive"
  prepare_track_sort_fixture "$fixture_dir" "$base_track"
  start_scenario_app \
    track-sort-playing-marker "$fixture_dir" "" "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Playing Sentinel" track-sort-library >/dev/null
  cua_focus_label_via_key \
    "$APP_PID" "$WINDOW_ID" "Playing Sentinel" down track-sort-focus >/dev/null
  cua_press_key_window "$APP_PID" "$WINDOW_ID" enter track-sort-play
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Pause (Space)" track-sort-playing >/dev/null

  # Clear the row selection before sorting so TAG-1's legitimate
  # selection/scroll restoration does not pin the sentinel to the viewport
  # when it moves between the first and last title positions.
  cua_hotkey "$APP_PID" "$WINDOW_ID" track-sort-clear-selection ctrl shift a

  header_state=$(cua_snapshot \
    "$APP_PID" "$WINDOW_ID" track-sort-title-header-state)
  if ! read -r header_x header_y <<<"$(track_sort_title_header_point "$header_state")"; then
    echo "track-sort title-header point requires a 1200x800 window: $header_state" >&2
    return 1
  fi

  # AT-SPI exposes every GtkColumnView header descendant at the native
  # window origin. The retained 1200x800 screenshot locates Title at this
  # pinned window-local point; cua-driver owns the screen-point conversion.
  sort_log_line=$(wc -l <"$APP_LOG")
  cua_click_window_point \
    "$APP_PID" "$WINDOW_ID" "$header_x" "$header_y" track-sort-title-initial
  ascending_path="$CUA_E2E_OUT_DIR/track-sort-title-initial-after.json"
  assert_sort_direction_since "$APP_LOG" "$sort_log_line" asc track-sort-title-initial
  assert_snapshot_contains "$ascending_path" "Playing Sentinel"
  assert_snapshot_label_count_at_least "$ascending_path" "Playing Sentinel" 2

  for round in $(seq 1 $((TRACK_SORT_TOGGLE_COUNT / 2))); do
    cua_click_window_point \
      "$APP_PID" "$WINDOW_ID" "$header_x" "$header_y" \
      "track-sort-$round-descending"
    descending_path="$CUA_E2E_OUT_DIR/track-sort-$round-descending-after.json"
    assert_sort_direction_since \
      "$APP_LOG" "$sort_log_line" desc "track-sort-$round-descending"
    assert_snapshot_contains "$descending_path" "Sort Track 24"
    assert_snapshot_contains "$descending_path" "Playing Sentinel"
    assert_snapshot_contains "$descending_path" "Pause (Space)"

    cua_click_window_point \
      "$APP_PID" "$WINDOW_ID" "$header_x" "$header_y" \
      "track-sort-$round-ascending"
    ascending_path="$CUA_E2E_OUT_DIR/track-sort-$round-ascending-after.json"
    assert_sort_direction_since \
      "$APP_LOG" "$sort_log_line" asc "track-sort-$round-ascending"
    assert_snapshot_contains "$ascending_path" "Playing Sentinel"
    assert_snapshot_label_count_at_least "$ascending_path" "Playing Sentinel" 2
    assert_snapshot_contains "$ascending_path" "Pause (Space)"
  done

  sort_query_count=$(tail -n "+$((sort_log_line + 1))" "$APP_LOG" \
    | rg -c 'query matched 25 tracks.*field.*title')
  if ((sort_query_count != TRACK_SORT_TOGGLE_COUNT + 1)); then
    echo "expected $((TRACK_SORT_TOGGLE_COUNT + 1)) title-sort queries, got $sort_query_count" >&2
    return 1
  fi

  finish_scenario track-sort-playing-marker \
    "dev scan complete" \
    "queue set from view" \
    "query matched 25 tracks"
}
