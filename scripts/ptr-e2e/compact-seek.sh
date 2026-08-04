#!/usr/bin/env bash
#
# Flow: the mini player's waveform actually seeks (MINI-1 "click = seek,
# drag = scrub") and scrubbing does NOT drag the window (MINI-2 exempts the
# waveform from the card's drag surface).
#
# Why this needs real pointer events: both regressions this covers are
# invisible to signal-seam tests.
#   1. An insensitive waveform is skipped by GTK hit-testing, so every press
#      lands on the card below — the seek callback is never reached.
#   2. The card's GtkWindowHandle claims the sequence once the pointer passes
#      the drag threshold and starts a window move, cancelling the scrub
#      mid-flight unless the waveform claims the sequence on press.
# Calling the handler directly would pass in both cases.

# Waveform geometry inside the 430x76 card (compact_player_layouts.rs):
# 10px padding + 52px cover + 13px spacing = x 76, width 288; the row sits at
# y 41..57, so y+50 is comfortably inside it.
COMPACT_SEEK_WAVEFORM_X0=76
COMPACT_SEEK_WAVEFORM_WIDTH=288
COMPACT_SEEK_WAVEFORM_Y=50

# Position tolerance in seconds: the track keeps playing during the
# measurement, and the pointer lands on a bar boundary, not a pixel-exact
# fraction.
COMPACT_SEEK_TOLERANCE_S=12

compact_seek_expected_s() {
  # $1 = pointer x relative to the card
  local pointer_x="$1"
  awk -v x="$pointer_x" -v x0="$COMPACT_SEEK_WAVEFORM_X0" \
      -v w="$COMPACT_SEEK_WAVEFORM_WIDTH" -v dur="$COMPACT_SEEK_FIXTURE_S" \
      'BEGIN { f = (x - x0) / w; if (f < 0) f = 0; if (f > 1) f = 1; printf "%.1f", f * dur }'
}

assert_position_near() {
  local expected_s="$1" description="$2" actual_us actual_s
  actual_us="$(mpris_position)"
  actual_s="$(awk -v v="${actual_us:-0}" 'BEGIN { printf "%.1f", v / 1000000 }')"
  if awk -v a="$actual_s" -v e="$expected_s" -v tol="$COMPACT_SEEK_TOLERANCE_S" \
    'BEGIN { d = a - e; if (d < 0) d = -d; exit d > tol }'; then
    log_step "MPRIS check OK: $description (${actual_s}s ≈ ${expected_s}s)"
  else
    log_fail "$description (expected ~${expected_s}s, got ${actual_s}s)"
  fi
}

run_compact_seek_flow() {
  log_step "flow: mini-player waveform seeks by click and scrubs by drag…"

  # Play the long fixture, then switch to compact. The generated fixture is
  # minutes long, so its scan (and waveform extraction) can still be running
  # when the window first paints — retry the activation until MPRIS reports
  # motion instead of assuming the row is already there.
  # The first row's y depends on whether the onboarding banner is showing, so
  # sweep the plausible rows instead of pinning one offset: a double-click on
  # a column header only re-sorts, which is harmless here (one track).
  local playing=0 attempt row_y position
  for attempt in $(seq 1 4); do
    for row_y in "$ROW0_TITLE_CELL_Y" 190 208 226 244; do
      double_click_at "$ROW0_TITLE_CELL_X" "$row_y"
      for _ in $(seq 1 4); do
        position="$(mpris_position)"
        if [ -n "$position" ] && [ "$position" -gt 0 ] 2>/dev/null; then
          playing=1
          log_step "playback started from row y=$row_y"
          break 3
        fi
        sleep 0.4
      done
    done
  done
  if [ "$playing" != 1 ]; then
    screenshot "19-compact-seek-no-playback"
    log_fail "the long fixture never started playing — cannot measure seeks"
    return
  fi

  local marker
  marker=$(log_marker)
  key "ctrl+m"
  local width=0
  for _ in $(seq 1 20); do
    width="$(sed -n 's/^WIDTH=//p' <<<"$(xdotool getwindowgeometry --shell "$WINDOW_ID")")"
    [ "${width:-9999}" -lt 700 ] 2>/dev/null && break
    sleep 0.4
  done
  assert_log_contains_since "$marker" "window view mode changed.*mode=Compact" \
    "Ctrl+M entered Compact for the seek checks"
  if [ "${width:-9999}" -ge 700 ]; then
    log_fail "never entered compact mode (width=${width:-?})"
    return
  fi
  xdotool windowactivate "$WINDOW_ID" 2>/dev/null
  sleep 0.5

  local geometry window_x window_y
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID")"
  window_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  window_y="$(sed -n 's/^Y=//p' <<<"$geometry")"

  # --- click = seek -----------------------------------------------------------
  local click_x=330
  xdotool mousemove --sync "$((window_x + click_x))" \
    "$((window_y + COMPACT_SEEK_WAVEFORM_Y))"
  sleep 0.3
  xdotool click 1
  sleep 1.2
  assert_position_near "$(compact_seek_expected_s "$click_x")" \
    "clicking the mini waveform seeks there (MINI-1)"
  screenshot "20-compact-after-seek-click"

  # --- drag = scrub, and the window must not move ------------------------------
  local drag_from=120 drag_to=300
  local before_x before_y after_x after_y
  before_x="$window_x"
  before_y="$window_y"
  xdotool mousemove --sync "$((window_x + drag_from))" \
    "$((window_y + COMPACT_SEEK_WAVEFORM_Y))"
  sleep 0.3
  xdotool mousedown 1
  sleep 0.2
  local step
  for step in 20 50 80 110 140 170 180; do
    xdotool mousemove --sync "$((window_x + drag_from + step))" \
      "$((window_y + COMPACT_SEEK_WAVEFORM_Y))"
    sleep 0.08
  done
  sleep 0.2
  xdotool mouseup 1
  sleep 1.2
  assert_position_near "$(compact_seek_expected_s "$drag_to")" \
    "dragging the mini waveform scrubs to the release point (MINI-1)"

  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID")"
  after_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  after_y="$(sed -n 's/^Y=//p' <<<"$geometry")"
  if [ "$after_x" = "$before_x" ] && [ "$after_y" = "$before_y" ]; then
    log_step "geometry check OK: scrubbing left the window in place (MINI-2)"
  else
    log_fail "scrubbing the waveform moved the window from ${before_x},${before_y} to ${after_x},${after_y} (MINI-2)"
  fi
  screenshot "21-compact-after-scrub"

  # --- the rest of the card still drags the window -----------------------------
  # The waveform is the exception, not the rule: pressing the free area next to
  # the title must still move the window (MINI-2).
  xdotool mousemove --sync "$((after_x + 300))" "$((after_y + 22))"
  sleep 0.3
  xdotool mousedown 1
  sleep 0.2
  for step in 20 50 80; do
    xdotool mousemove --sync "$((after_x + 300 + step))" "$((after_y + 22))"
    sleep 0.1
  done
  sleep 0.3
  xdotool mouseup 1
  sleep 0.8
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID")"
  local moved_x
  moved_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  if [ "$moved_x" != "$after_x" ]; then
    log_step "geometry check OK: the metadata row still drags the window (MINI-2)"
  else
    log_fail "dragging the card's free area no longer moves the window (MINI-2)"
  fi

  marker=$(log_marker)
  key "ctrl+m"
  sleep 0.6
  assert_log_contains_since "$marker" "window view mode changed.*mode=Library" \
    "Ctrl+M returned to the Library view"
}
