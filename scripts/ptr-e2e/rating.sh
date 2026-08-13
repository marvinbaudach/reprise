#!/usr/bin/env bash

# Drives the inline Rating cell through real pointer input. Helpers and
# geometry variables are supplied by run.sh before this function is called.
run_rating_flow() {
  start_flow "1: inline star-rating field…"
  screenshot "01-initial-track-list"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/01-initial-track-list.png"

  # Information is already closed in this fixture, exposing every fixed-width
  # track column. Enter row 0's Rating cell without clicking: its motion
  # controller reveals the inline buttons, which need a short settle before the
  # pointer can press star 2. There is no Information toggle any more; its
  # retired offset resolved to the
  # main menu button in the current header bar, whose autohide popover then
  # covered the right half of the table and swallowed the star click as a
  # click-outside dismissal.
  # The panel it was meant to close is not open in this fixture profile anyway:
  # 01-initial-track-list.png already shows every column through Rating.
  xdotool mousemove --sync "$ROW0_RATING_STAR2_X" "$ROW0_RATING_STAR_Y" \
    >/dev/null 2>&1
  sleep 0.5
  screenshot "02-rating-stars-revealed"

  MARKER=$(log_marker)
  click_at "$ROW0_RATING_STAR2_X" "$ROW0_RATING_STAR_Y"
  sleep 1
  screenshot "02-after-star-click"
  assert_log_contains_since "$MARKER" "rating changed" \
    "inline star button delivered a rating change (track-list write-back)"
  assert_db_query_true \
    "SELECT COUNT(*) = 1 FROM tracks WHERE title = 'sine_01' AND rating = 2 AND missing_since IS NULL;" \
    "row 0's two-star rating persisted"
}
