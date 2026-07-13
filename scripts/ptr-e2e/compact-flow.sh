#!/usr/bin/env bash

MPRIS_BUS_NAME="org.mpris.MediaPlayer2.reprise"
MPRIS_OBJECT_PATH="/org/mpris/MediaPlayer2"
MPRIS_PLAYER_INTERFACE="org.mpris.MediaPlayer2.Player"

mpris_property() {
  local property="$1"
  gdbus call \
    --address "$(<"$DBUS_ADDRESS_FILE")" \
    --dest "$MPRIS_BUS_NAME" \
    --object-path "$MPRIS_OBJECT_PATH" \
    --method org.freedesktop.DBus.Properties.Get \
    "$MPRIS_PLAYER_INTERFACE" "$property"
}

mpris_call() {
  local method="$1"
  gdbus call \
    --address "$(<"$DBUS_ADDRESS_FILE")" \
    --dest "$MPRIS_BUS_NAME" \
    --object-path "$MPRIS_OBJECT_PATH" \
    --method "$MPRIS_PLAYER_INTERFACE.$method" >/dev/null
}

mpris_volume() {
  mpris_property Volume | sed -E 's/.*<([-0-9.]+)>.*/\1/'
}

mpris_position() {
  mpris_property Position | sed -E 's/.*<int64 ([-0-9]+)>.*/\1/'
}

assert_mpris_volume() {
  local expected="$1" description="$2" actual
  actual="$(mpris_volume)"
  if awk -v actual="$actual" -v expected="$expected" \
    'BEGIN { difference = actual - expected; if (difference < 0) difference = -difference; exit difference > 0.0001 }'; then
    log_step "MPRIS check OK: $description ($actual)"
  else
    log_fail "$description (expected $expected, got $actual)"
  fi
}

scroll_window_relative() {
  local relative_x="$1" relative_y="$2" button="$3"
  local geometry window_x window_y
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  window_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  window_y="$(sed -n 's/^Y=//p' <<<"$geometry")"
  xdotool mousemove --sync "$((window_x + relative_x))" "$((window_y + relative_y))" \
    click "$button" >/dev/null 2>&1
}

select_compact_layout() {
  local target_y="$1" menu_y="$2" layout_y="$3"
  click_window_from_right 105 "$menu_y"
  sleep 0.2
  click_window_from_right 105 "$layout_y"
  sleep 0.2
  click_window_from_right 105 "$target_y"
}

run_compact_flow() {
  log_step "flow 5: native Compact menu, layouts, and scroll volume…"
  local header_button_y=28 marker position_before volume_before

  marker=$(log_marker)
  click_window_from_right "$COMPACT_BUTTON_FROM_RIGHT" "$header_button_y"
  sleep 0.4
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Compact.*layout=Bar" \
    "full-header button entered Compact Bar"
  assert_window_within 740 230 "Bar compact geometry after leaving maximized Library"
  screenshot "08-compact-bar"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/08-compact-bar.png"

  marker=$(log_marker)
  key "space"
  sleep 0.2
  assert_log_contains_since "$marker" "applying state change.*state=Paused" \
    "Space paused playback before scroll-volume assertions"
  volume_before="$(mpris_volume)"
  position_before="$(mpris_position)"
  scroll_window_relative 180 90 5
  sleep 0.3
  assert_mpris_volume "$(awk -v value="$volume_before" 'BEGIN { value -= 0.05; if (value < 0) value = 0; printf "%.2f", value }')" \
    "one downward wheel step on Bar metadata changed volume by five percent"
  if [ "$(mpris_position)" = "$position_before" ]; then
    log_step "MPRIS check OK: free-region volume scroll left paused seek position unchanged"
  else
    log_fail "free-region volume scroll changed the paused seek position"
  fi

  local volume_after_free
  volume_after_free="$(mpris_volume)"
  scroll_window_relative 500 120 5
  sleep 0.3
  assert_mpris_volume "$volume_after_free" "scroll over Compact seek did not change volume"
  if [ "$(mpris_position)" = "$position_before" ]; then
    log_step "MPRIS check OK: scroll over Compact seek was a complete no-op"
  else
    log_fail "scroll over Compact seek changed the paused position"
  fi

  click_window_from_right 105 28
  sleep 0.3
  screenshot "09-compact-visible-menu"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/09-compact-visible-menu.png"

  marker=$(log_marker)
  key "Escape"
  sleep 0.3
  select_compact_layout 124 28 100
  sleep 0.4
  assert_log_contains_since "$marker" "compact layout changed.*layout=Cover" \
    "visible menu selected Cover"
  assert_window_within 440 620 "Cover compact geometry"
  screenshot "10-compact-cover"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/10-compact-cover.png"

  marker=$(log_marker)
  select_compact_layout 156 28 100
  sleep 0.4
  assert_log_contains_since "$marker" "compact layout changed.*layout=Pill" \
    "visible menu selected Pill"
  assert_window_within 780 140 "Pill compact geometry"
  screenshot "11-compact-pill"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/11-compact-pill.png"

  click_window_from_right 105 48
  marker=$(log_marker)
  sleep 0.3
  screenshot "11b-compact-pill-visible-menu"
  assert_screenshots_differ \
    "$PTR_E2E_OUT_DIR/11-compact-pill.png" \
    "$PTR_E2E_OUT_DIR/11b-compact-pill-visible-menu.png" \
    "Pill visible button opened the compact menu"
  key "Escape"
  sleep 0.3
  select_compact_layout 219 48 124
  sleep 0.4
  assert_log_contains_since "$marker" "compact layout changed.*layout=Card" \
    "visible menu selected Card"
  assert_window_within 580 360 "Card compact geometry"
  screenshot "12-compact-card"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/12-compact-card.png"

  click_window_relative 20 200 3
  sleep 0.3
  screenshot "13-compact-right-click-menu"
  assert_screenshots_differ \
    "$PTR_E2E_OUT_DIR/12-compact-card.png" \
    "$PTR_E2E_OUT_DIR/13-compact-right-click-menu.png" \
    "right click opened the compact menu"
  key "Escape"
  sleep 0.2
  click_window_from_right 105 28
  sleep 0.2
  key "Escape"
  screenshot "13b-compact-card-menu-closed"
  key "shift+F10"
  sleep 0.3
  screenshot "14-compact-keyboard-menu"
  assert_screenshots_differ \
    "$PTR_E2E_OUT_DIR/13b-compact-card-menu-closed.png" \
    "$PTR_E2E_OUT_DIR/14-compact-keyboard-menu.png" \
    "Shift+F10 opened the compact menu"

  marker=$(log_marker)
  key "Home"
  key "Return"
  sleep 0.5
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Library.*layout=Card" \
    "menu-only Return to Library action restored Library"
  screenshot "15-library-restored"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/15-library-restored.png"

  marker=$(log_marker)
  key "ctrl+m"
  sleep 0.4
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Compact.*layout=Card" \
    "Ctrl+M restored Compact Card"
  screenshot "16-compact-card-shortcut"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/16-compact-card-shortcut.png"
  marker=$(log_marker)
  key "ctrl+m"
  sleep 0.5
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Library.*layout=Card" \
    "Ctrl+M restored Library View"
}
