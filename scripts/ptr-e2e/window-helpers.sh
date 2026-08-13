#!/usr/bin/env bash

# Polls up to ~15s for a mapped window whose WM_CLASS contains
# $WINDOW_CLASS_MATCH, printing its X window id on success.
find_window() {
  local win=""
  for _ in $(seq 1 30); do
    win="$(xdotool search --class "$WINDOW_CLASS_MATCH" 2>/dev/null | head -1 || true)"
    if [ -n "$win" ]; then
      echo "$win"
      return 0
    fi
    sleep 0.5
  done
  return 1
}

screenshot() {
  local name="$1"
  scrot -o "$PTR_E2E_OUT_DIR/$name.png"
}

click_at() {
  local x="$1" y="$2"
  xdotool mousemove --sync "$x" "$y" sleep 0.1 click 1 >/dev/null 2>&1
}

# On-screen rect of the app window as `X Y WIDTH HEIGHT`.
#
# While the window was maximized at the full 1600x900, a wrong origin made no
# difference — every in-window offset landed on the window either way. The
# moment Compact mode shrinks it to 430x76 it decides everything: a pointer
# event delivered to the root window is where openbox reads a wheel step as
# "switch virtual desktop", which is how one scroll-volume assertion turned
# every later screenshot into a black frame showing openbox's "desktop 3"
# indicator.
window_rect() {
  local geometry
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  printf '%s %s %s %s' \
    "$(sed -n 's/^X=//p' <<<"$geometry")" \
    "$(sed -n 's/^Y=//p' <<<"$geometry")" \
    "$(sed -n 's/^WIDTH=//p' <<<"$geometry")" \
    "$(sed -n 's/^HEIGHT=//p' <<<"$geometry")"
}

# Fails the check rather than the run when an offset falls outside the window:
# a pointer event delivered to the desktop is never what a flow meant to test.
assert_point_in_window() {
  local relative_x="$1" relative_y="$2" description="$3"
  local rect width height
  rect="$(window_rect)"
  width="$(cut -d' ' -f3 <<<"$rect")"
  height="$(cut -d' ' -f4 <<<"$rect")"
  if [ -z "$width" ] || [ -z "$height" ]; then
    log_fail "$description (could not read window geometry)"
    return 1
  fi
  if [ "$relative_x" -lt 0 ] || [ "$relative_x" -ge "$width" ] \
    || [ "$relative_y" -lt 0 ] || [ "$relative_y" -ge "$height" ]; then
    log_fail "$description (offset ${relative_x}x${relative_y} outside ${width}x${height} window)"
    return 1
  fi
  return 0
}

click_window_relative() {
  local relative_x="$1" relative_y="$2" button="${3:-1}"
  local rect window_x window_y
  assert_point_in_window "$relative_x" "$relative_y" "click at ${relative_x}x${relative_y}" || return 0
  rect="$(window_rect)"
  window_x="$(cut -d' ' -f1 <<<"$rect")"
  window_y="$(cut -d' ' -f2 <<<"$rect")"
  xdotool mousemove "$((window_x + relative_x))" "$((window_y + relative_y))" \
    click "$button" >/dev/null 2>&1
}

click_window_from_right() {
  local right_offset="$1" relative_y="$2" button="${3:-1}"
  local width
  width="$(cut -d' ' -f3 <<<"$(window_rect)")"
  click_window_relative "$((width - right_offset))" "$relative_y" "$button"
}

double_click_at() {
  local x="$1" y="$2"
  xdotool mousemove "$x" "$y" click --repeat 2 --delay 80 1 >/dev/null 2>&1
}

type_text() {
  xdotool type -- "$1" >/dev/null 2>&1
}

key() {
  xdotool key "$1" >/dev/null 2>&1
}

assert_window_within() {
  local max_width="$1" max_height="$2" description="$3"
  local geometry width height
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  width="$(sed -n 's/^WIDTH=//p' <<<"$geometry")"
  height="$(sed -n 's/^HEIGHT=//p' <<<"$geometry")"
  if [ -n "$width" ] && [ -n "$height" ] \
    && [ "$width" -le "$max_width" ] && [ "$height" -le "$max_height" ]; then
    log_step "window geometry OK: $description (${width}x${height})"
  else
    log_fail "$description exceeded ${max_width}x${max_height} (got ${width:-?}x${height:-?})"
  fi
}

maximize_window() {
  local geometry current_width=0 current_height=0

  # Compact replaces the surface with a 430x76 window. Openbox can retain the
  # old maximized flags while that replacement is still settling, in which
  # case repeatedly adding the flags is a no-op. Clear and reapply them, then
  # allow up to nine seconds for the full-size surface transition.
  wmctrl -i -r "$WINDOW_ID" -b remove,maximized_vert,maximized_horz
  sleep 0.3
  wmctrl -i -r "$WINDOW_ID" -b add,maximized_vert,maximized_horz
  for _ in $(seq 1 60); do
    geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
    current_width="$(sed -n 's/^WIDTH=//p' <<<"$geometry")"
    current_height="$(sed -n 's/^HEIGHT=//p' <<<"$geometry")"
    if [ "${current_width:-0}" -ge 1500 ] && [ "${current_height:-0}" -ge 850 ]; then
      sleep 0.3
      return 0
    fi
    sleep 0.15
  done
  log_fail "Reprise window did not reach the fixed maximized harness geometry (got ${current_width:-?}x${current_height:-?})"
  return 1
}

drag_and_hold() {
  local from_x="$1" from_y="$2" to_x="$3" to_y="$4"
  xdotool mousemove "$from_x" "$from_y" mousedown 1 >/dev/null 2>&1
  sleep 0.2
  xdotool mousemove --sync "$((from_x + 20))" "$((from_y + 5))" >/dev/null 2>&1
  sleep 0.2
  xdotool mousemove --sync "$((to_x - 8))" "$to_y" >/dev/null 2>&1
  sleep 0.2
  xdotool mousemove --sync "$to_x" "$to_y" >/dev/null 2>&1
}

release_drag() {
  xdotool mouseup 1 >/dev/null 2>&1
}

# Non-trivial-image check: a solid/blank capture has a standard deviation
# near zero; a real rendered UI does not. Threshold (50) sits comfortably
# below the ~3600 measured on a real capture at this resolution/theme and
# comfortably above the ~0 a blank/solid capture would produce.
assert_screenshot_not_blank() {
  local path="$1"
  if [ ! -s "$path" ]; then
    log_fail "screenshot missing or empty: $path"
    return
  fi
  local stddev
  stddev="$(convert "$path" -format '%[standard-deviation]' info: 2>/dev/null || echo 0)"
  # Integer-truncate for a portable numeric comparison (bash has no floats).
  local stddev_int="${stddev%%.*}"
  if [ -z "$stddev_int" ] || [ "$stddev_int" -lt 50 ]; then
    log_fail "screenshot looks blank/solid (standard-deviation=$stddev): $path"
  else
    log_step "screenshot OK ($path): standard-deviation=$stddev"
  fi
}

assert_screenshots_differ() {
  local before="$1" after="$2" description="$3"
  local changed
  changed="$(compare -metric AE "$before" "$after" null: 2>&1 || true)"
  changed="${changed%% *}"
  if awk -v changed="${changed:-0}" 'BEGIN { exit !(changed > 100) }'; then
    log_step "screenshot difference OK: $description ($changed pixels)"
  else
    log_fail "$description did not visibly change the mapped UI (${changed:-0} pixels)"
  fi
}
