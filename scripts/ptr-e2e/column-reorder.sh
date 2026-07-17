#!/usr/bin/env bash

# Drives the custom header drag-reorder gesture (`column_header_dnd.rs`)
# through real, smooth pointer movement. GTK's native column-header drag is
# broken in 4.22 (see that module's doc comment), so Reprise reimplements it
# with its own capture-phase GestureDrag; a signal-seam test cannot see that
# class of bug because it never delivers a real event sequence, so — same
# reasoning as every other flow in this harness (see run.sh's top-of-file
# comment) — this drives it with actual xdotool pointer motion.
#
# NOT part of the default flow chain: later flows (queue reorder, tag editor,
# …) depend on fixed pixel geometry (geometry.sh) that a persisted column
# reorder would shift out from under them. Opt in via PTR_E2E_COLREORDER_ONLY.

# geometry.sh's COLUMN_HEADER_Y (120) and TITLE_HEADER_X (500) predate the
# redesign's permanent filter row and current column widths: the header row
# now sits at ~y=140, and x=500 lands on the ARTIST header (Title spans
# ~290..450 — measured live from a maximized 1600x900 capture). This flow
# carries its own measured values until the harness-wide geometry
# recalibration (tracked as follow-up in the ledger) folds them back into
# geometry.sh.
COLREORDER_HEADER_Y=145
COLREORDER_TITLE_X=385

# Multi-step drag: a single mousedown -> teleport -> mouseup does not
# reliably drive GtkGestureDrag's recognizer (observed empirically elsewhere
# in this harness, see `drag_and_hold`'s own comment) — this instead walks
# ~20 small steps with a short settle delay between each, so the gesture sees
# real intermediate motion cross its own click-vs-drag threshold.
smooth_header_drag() {
  local from_x="$1" to_x="$2" y="$3"
  local geometry window_x window_y abs_from_x abs_to_x abs_y
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  window_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  window_y="$(sed -n 's/^Y=//p' <<<"$geometry")"
  abs_from_x=$((window_x + from_x))
  abs_to_x=$((window_x + to_x))
  abs_y=$((window_y + y))

  local steps=20
  local delta=$(( (abs_to_x - abs_from_x) / steps ))

  xdotool mousemove "$abs_from_x" "$abs_y"
  xdotool mousedown 1
  sleep 0.05

  local x="$abs_from_x"
  for _ in $(seq 1 "$steps"); do
    x=$((x + delta))
    xdotool mousemove --sync "$x" "$abs_y"
    sleep 0.04
  done
  # Land exactly on the target in case integer step division left a
  # remainder short of it.
  xdotool mousemove --sync "$abs_to_x" "$abs_y"
  sleep 0.04

  xdotool mouseup 1
  sleep 0.2
}

run_column_reorder_flow() {
  log_step "flow: header drag reorders columns, plain click still sorts…"

  # --- Plain click still sorts (reimplemented activate_sort) --------------
  # Run BEFORE any drag mutates header geometry, at the Title header's
  # measured position (see COLREORDER_TITLE_X above). Artist is the app's default
  # primary sort column at startup, so a plain (zero-movement) click on the
  # not-yet-primary Title header must switch the sort to Title/ascending —
  # proving our capture-phase claim-on-press gesture still lets a genuine
  # click fall through to a real sort, exactly like the native header click
  # did before this module took over (only its *drag* path was broken).
  local marker
  marker=$(log_marker)
  click_window_relative "$COLREORDER_TITLE_X" "$COLREORDER_HEADER_Y" 1
  sleep 0.3
  assert_log_contains_since "$marker" "query matched.*field=title" \
    "plain click on the Title header still sorts (reimplemented activate_sort)"

  # --- Drag reorder ---------------------------------------------------------
  marker=$(log_marker)
  smooth_header_drag "$COLREORDER_TITLE_X" 900 "$COLREORDER_HEADER_Y"
  sleep 0.3
  screenshot "24-column-header-dragged"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/24-column-header-dragged.png"
  assert_log_contains_since "$marker" "column order persisted after header drag" \
    "header drag reordered and persisted the column order"
  # `ui.column_layout` is `order;visible` (see column-header-menu.sh) —
  # dragging Title away from its default slot right before Artist must break
  # that adjacency in the order half (before the `;`).
  assert_db_query_true \
    "SELECT instr(substr(value, 1, instr(value, ';') - 1), 'title,artist') = 0 FROM settings WHERE key = 'ui.column_layout';" \
    "dragged column order no longer has Title immediately before Artist"

  # --- Drag back — a second persisted reorder proves the gesture is
  # reusable, not a one-shot. Exact final geometry is not asserted (the
  # *_ONLY flow exits right after this function, so no later flow depends on
  # exactly where columns land).
  marker=$(log_marker)
  smooth_header_drag 900 "$COLREORDER_TITLE_X" "$COLREORDER_HEADER_Y"
  sleep 0.3
  assert_log_contains_since "$marker" "column order persisted after header drag" \
    "reverse header drag persisted a second column reorder"
}
