#!/usr/bin/env bash

# NAV-17: a Shift selection starts at its anchor rather than row zero.
# Helpers and geometry variables are supplied by run.sh before this function
# is called.
run_selection_anchor_flow() {
  log_step "flow: Shift click reaches the selection anchor…"
  screenshot "01-selection-anchor-initial"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/01-selection-anchor-initial.png"

  # Start row 1 with a real double-click, establishing a known playing row and
  # range origin. Pure/display seams separately prove the no-user-anchor
  # fallback; this flow's unique job is to prove that real pointer delivery
  # reaches the capture-phase cell gesture before GTK's row machinery.
  double_click_at "$ROW1_TITLE_CELL_X" "$ROW1_TITLE_CELL_Y"
  sleep 1
  screenshot "02-playing-row-1"

  MARKER=$(log_marker)
  shift_click_at "$ROW3_TITLE_CELL_X" "$ROW3_TITLE_CELL_Y"
  sleep 0.5
  screenshot "03-after-shift-click"
  assert_log_contains_since "$MARKER" "selection anchor range applied" \
    "Shift click reached the cell gesture instead of GTK's row machinery"
  assert_log_contains_since "$MARKER" "selection anchor range applied.*start=1" \
    "the applied range began at row 1"
}
