#!/usr/bin/env bash

MPRIS_BUS_NAME="org.mpris.MediaPlayer2.reprise"
MPRIS_OBJECT_PATH="/org/mpris/MediaPlayer2"
MPRIS_PLAYER_INTERFACE="org.mpris.MediaPlayer2.Player"
REPRISE_PLAYER_INTERFACE="org.reprise.Player1"

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

reprise_player_call() {
  local method="$1"
  shift
  gdbus call \
    --address "$(<"$DBUS_ADDRESS_FILE")" \
    --dest "$MPRIS_BUS_NAME" \
    --object-path "$MPRIS_OBJECT_PATH" \
    --method "$REPRISE_PLAYER_INTERFACE.$method" "$@" >/dev/null
}

assert_mpris_playback_status() {
  local expected="$1" description="$2" actual=""
  for _ in $(seq 1 20); do
    actual="$(mpris_playback_status)"
    if [ "$actual" = "$expected" ]; then
      log_step "MPRIS check OK: $description ($expected)"
      return 0
    fi
    sleep 0.05
  done
  log_fail "$description (expected $expected, got $actual)"
  return 1
}

mpris_playback_status() {
  mpris_property PlaybackStatus | sed -E "s/^\(<'([^']+)'>,\)$/\1/"
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
  local rect window_x window_y
  # Same absolute-rect reasoning as `click_window_relative`, and the bounds
  # check matters most here: openbox turns a wheel step on the root window into
  # a virtual-desktop switch, which hides the app for the rest of the run.
  assert_point_in_window "$relative_x" "$relative_y" \
    "scroll at ${relative_x}x${relative_y}" || return 0
  rect="$(window_rect)"
  window_x="$(cut -d' ' -f1 <<<"$rect")"
  window_y="$(cut -d' ' -f2 <<<"$rect")"
  xdotool mousemove --sync "$((window_x + relative_x))" "$((window_y + relative_y))" \
    >/dev/null 2>&1
  # Diagnostic: says out loud where the pointer actually ended up. A wheel step
  # on the root window is silently a desktop switch, so "the volume did not
  # change" must never be the only evidence available.
  log_step "pointer probe: window rect=[$rect] target=$((window_x + relative_x)),$((window_y + relative_y)) landed on $(xdotool getmouselocation --shell 2>/dev/null | tr '\n' ' ')"
  xdotool click "$button" >/dev/null 2>&1
}

double_click_window_relative() {
  local relative_x="$1" relative_y="$2"
  local rect window_x window_y
  assert_point_in_window "$relative_x" "$relative_y" \
    "double click at ${relative_x}x${relative_y}" || return 0
  rect="$(window_rect)"
  window_x="$(cut -d' ' -f1 <<<"$rect")"
  window_y="$(cut -d' ' -f2 <<<"$rect")"
  xdotool mousemove --sync "$((window_x + relative_x))" "$((window_y + relative_y))" \
    click --repeat 2 --delay 80 1 >/dev/null 2>&1
}

run_compact_flow() {
  start_flow "5: native Compact card, context menu, and scroll volume…"
  local marker position_before volume_before compact_track_id
  local compact_rect compact_width compact_height
  local play_x metadata_x cover_x center_y

  db_scalar_into compact_track_id \
    "SELECT id FROM tracks WHERE title = 'sine_03' AND missing_since IS NULL;" \
    'flow 5 needs its own playable sine_03 fixture' || return

  marker=$(log_marker)
  # There is no Compact button in the Library header — `primary_menu.rs` packs
  # "Compact Mode" as the first entry of the first menu section and the header
  # deliberately carries no duplicate control. The old header coordinate
  # landed on empty space, and the follow-up click
  # then hit the *minimise* button, which unmapped the window and turned every
  # later screenshot into a black frame.
  # F10 plus Return walks the only route a user has, without a single pixel
  # coordinate.
  key "F10"
  sleep 0.3
  screenshot "07b-primary-menu"
  key "Return"
  sleep 0.4
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Compact.*layout=Card" \
    "primary-menu Compact Mode entered Compact Card"
  assert_window_within "$COMPACT_CARD_MAX_WIDTH" "$COMPACT_CARD_MAX_HEIGHT" \
    "mini-card geometry after leaving maximized Library"
  screenshot "08-compact-card"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/08-compact-card.png"

  compact_rect="$(window_rect)"
  compact_width="$(cut -d' ' -f3 <<<"$compact_rect")"
  compact_height="$(cut -d' ' -f4 <<<"$compact_rect")"
  if [[ ! "$compact_width" =~ ^[0-9]+$ || ! "$compact_height" =~ ^[0-9]+$ ]]; then
    log_fail "Compact points could not be derived from window rect [$compact_rect]"
    return
  fi
  play_x=$((compact_width - COMPACT_PLAY_BUTTON_FROM_RIGHT))
  metadata_x=$((compact_width / 2))
  cover_x=$COMPACT_COVER_CENTER_X
  center_y=$((compact_height / 2))
  assert_point_in_window "$play_x" "$center_y" "derived Compact play/pause point" || return
  assert_point_in_window "$metadata_x" "$center_y" "derived Compact metadata point" || return
  assert_point_in_window "$cover_x" "$center_y" "derived Compact cover point" || return

  # The short fixture from flow 4 has stopped by now. Seed a fresh, owned
  # playback context so this flow never depends on prior timing, then assert
  # the public state. MPRIS Play is deliberately idempotent here: it resumes if
  # the private command is still settling and is a no-op once Playing.
  reprise_player_call PlayTrackIds "[$compact_track_id]"
  mpris_call Play
  if ! assert_mpris_playback_status "Playing" \
    "playback running before the derived Compact play click"; then
    return
  fi
  click_window_relative "$play_x" "$center_y"
  if ! assert_mpris_playback_status "Paused" \
    "derived Compact play button flipped PlaybackStatus to Paused"; then
    mpris_call Pause
    assert_mpris_playback_status "Paused" \
      "MPRIS Pause froze playback for scroll-volume assertions" || return
  fi
  volume_before="$(mpris_volume)"
  position_before="$(mpris_position)"
  scroll_window_relative "$metadata_x" "$center_y" 5
  sleep 0.3
  assert_mpris_volume "$(awk -v value="$volume_before" 'BEGIN { value -= 0.05; if (value < 0) value = 0; printf "%.2f", value }')" \
    "one downward wheel step on Compact metadata changed volume by five percent"
  if [ "$(mpris_position)" = "$position_before" ]; then
    log_step "MPRIS check OK: metadata volume scroll left paused seek position unchanged"
  else
    log_fail "metadata volume scroll changed the paused seek position"
  fi

  # Cover/Pill/Card remain persisted settings without a render or UI-write
  # path. Do not restore layout-switching checks unless the product gains both.
  screenshot "09-compact-menu-closed"
  click_window_relative "$metadata_x" "$center_y" 3
  sleep 0.3
  screenshot "10-compact-right-click-menu"
  assert_screenshots_differ \
    "$PTR_E2E_OUT_DIR/09-compact-menu-closed.png" \
    "$PTR_E2E_OUT_DIR/10-compact-right-click-menu.png" \
    "right click inside the mini card visibly opened its context menu"

  marker=$(log_marker)
  key "Home"
  key "Return"
  sleep 0.5
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Library.*layout=Card" \
    "Restore Full Window from the mini-card context menu restored Library"
  screenshot "11-library-restored-from-menu"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/11-library-restored-from-menu.png"

  marker=$(log_marker)
  key "ctrl+m"
  sleep 0.4
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Compact.*layout=Card" \
    "Ctrl+M restored Compact Card"
  screenshot "12-compact-card-shortcut"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/12-compact-card-shortcut.png"

  marker=$(log_marker)
  double_click_window_relative "$cover_x" "$center_y"
  sleep 0.5
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Library.*layout=Card" \
    "double-clicking the derived Compact cover point restored Library"

  marker=$(log_marker)
  key "ctrl+m"
  sleep 0.4
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Compact.*layout=Card" \
    "Ctrl+M re-entered Compact Card"
  marker=$(log_marker)
  key "ctrl+m"
  sleep 0.5
  assert_log_contains_since "$marker" \
    "window view mode changed.*mode=Library.*layout=Card" \
    "Ctrl+M restored Library View"
  log_step "Library geometry after Compact exit: $(window_rect)"
}
