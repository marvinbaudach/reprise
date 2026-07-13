#!/usr/bin/env bash

# Drives the compact Rating cell through real pointer input. Helpers and
# geometry variables are supplied by run.sh before this function is called.
run_rating_flow() {
  log_step "flow 1: compact star-rating chooser…"
  screenshot "01-initial-track-list"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/01-initial-track-list.png"

  # Closing Information exposes every fixed-width track column. Select the
  # first row, open its compact rating button, then choose one star from the
  # real popover. This keeps pointer delivery covered after the responsive
  # cell replaced the permanently-inline five-star strip.
  click_window_from_right "$INFO_TOGGLE_FROM_RIGHT" 28
  sleep 1
  click_at "$ROW0_TITLE_CELL_X" "$ROW0_TITLE_CELL_Y"
  sleep 0.3
  MARKER=$(log_marker)
  click_at "$ROW0_RATING_BUTTON_X" "$ROW0_RATING_BUTTON_Y"
  sleep 0.3
  screenshot "02-rating-chooser"
  click_at "$ROW0_RATING_POPOVER_STAR2_X" "$ROW0_RATING_POPOVER_STAR2_Y"
  sleep 1
  screenshot "02-after-star-click"
  assert_log_contains_since "$MARKER" "rating changed" \
    "compact rating popover delivered a rating change (track-list write-back)"
}
