#!/usr/bin/env bash
# FIL-1a/FIL-1d: the search chip's × removes the query on the FIRST click.
# Sourced by run.sh after the harness globals are initialized.
#
# This flow exists because the failure it covers is invisible to every
# signal-seam test: `search_chip::build`'s `clicked` handler is correct, and
# `emit_clicked` on it clears the query every time. On a real desktop the
# press moved focus out of the search entry, the chrome collapsed the search
# strip from an idle callback while the button was still held, the whole
# filter row travelled ~40 px upward, and the release therefore landed
# outside the button — no `clicked`, no removal, and the user had to click a
# second time. Only real pointer events can see that.
#
# The chip is the Library's, but the strip it moves is the shared chrome, so
# this covers every section that puts a filter row under the search strip:
# Music, Podcasts, YouTube, Radio, Concerts, Releases.

# Measured from sc-02 on the mapped 1600x900 harness window with the search
# strip open and the onboarding banner shown: the filter row sits directly
# under the banner, and the chip is its first pill. Window-relative.
SEARCH_CHIP_X=${SEARCH_CHIP_X:-400}
SEARCH_CHIP_Y=${SEARCH_CHIP_Y:-176}

# Splits the click so the intermediate state is observable: press, look,
# release, look. A healthy build still shows the strip — and the chip under
# the cursor — in `sc-04-pressed`; the bug shows the strip already gone and
# the chip lifted out from under the pointer.
search_chip_split_click() {
  local x="$1" y="$2"
  local geometry window_x window_y
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  window_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  window_y="$(sed -n 's/^Y=//p' <<<"$geometry")"
  xdotool mousemove --sync "$((window_x + x))" "$((window_y + y))" >/dev/null 2>&1
  sleep 0.2
  screenshot "sc-03-before-press"
  xdotool mousedown 1 >/dev/null 2>&1
  sleep 0.4
  screenshot "sc-04-pressed"
  xdotool mouseup 1 >/dev/null 2>&1
  sleep 0.8
  screenshot "sc-05-released"
}

run_search_chip_flow() {
  start_flow "sc: the search chip's × takes one click…"
  key "ctrl+f"
  sleep 0.6
  screenshot "sc-01-strip-open"
  # Per-keystroke delay: xdotool's default burst drops characters into a
  # GtkSearchEntry often enough to make the query — and with it the chip's
  # width and position — non-deterministic.
  xdotool type --delay 80 -- "zz" >/dev/null 2>&1
  # The list has to have re-queried before the chip is worth clicking: the
  # chip is built from the applied filter, not from the entry text.
  wait_for_log_pattern 'model query set.*filter="zz"' 'the typed query reached the list'
  sleep 0.4
  screenshot "sc-02-query-typed"

  MARKER=$(log_marker)
  search_chip_split_click "$SEARCH_CHIP_X" "$SEARCH_CHIP_Y"
  sleep 0.6
  screenshot "sc-06-after-one-click"

  # The count is what proves the removal actually re-ran the query: "zz"
  # matches none of the fixture tracks, so an applied empty filter is the
  # only way back to all five.
  assert_log_contains_since "$MARKER" 'query matched 5 tracks' \
    'one click on the search chip removed the query'
  assert_log_absent_since "$MARKER" 'model query set.*filter="zz"' \
    'the query re-applied after the chip was clicked'
}
