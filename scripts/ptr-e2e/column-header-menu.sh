#!/usr/bin/env bash

# Drives GtkColumnViewColumn's native header menu through real secondary
# clicks. Helpers and geometry variables are supplied by run.sh.
run_column_header_menu_flow() {
  log_step "flow 1b: right-click column visibility menu…"

  MARKER=$(log_marker)
  click_window_relative "$TITLE_HEADER_X" "$COLUMN_HEADER_Y" 3
  sleep 0.3
  screenshot "03-column-header-menu"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/03-column-header-menu.png"
  # Cover and Title are fixed/disabled. Click the first optional entry,
  # Artist, to prove secondary-click delivery through the native GTK menu.
  click_at "$HEADER_MENU_ARTIST_X" "$HEADER_MENU_ARTIST_Y"
  sleep 0.5
  screenshot "03b-after-hide-artist"
  assert_log_contains_since "$MARKER" \
    'column header visibility changed.*column="artist".*visible=false' \
    "right-click header menu hid Artist"
  assert_db_query_true \
    "SELECT instr(substr(value, instr(value, ';') + 1), 'artist') = 0 FROM settings WHERE key = 'ui.column_layout';" \
    "header menu persisted Artist as hidden"

  # A visibility menu stays open across toggles by design, so the menu is still
  # up here. Without this Escape the next right-click only dismisses it and
  # never reopens it, and the restore below then clicks into the bare table.
  key "Escape"
  sleep 0.3

  MARKER=$(log_marker)
  click_window_relative "$TITLE_HEADER_X" "$COLUMN_HEADER_Y" 3
  sleep 0.3
  # The reopened menu is its own evidence: hiding a column may move its entry
  # within the list, so the restore click cannot be assumed to land where the
  # hide click did.
  screenshot "03b2-header-menu-reopened"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/03b2-header-menu-reopened.png"
  click_at "$HEADER_MENU_ARTIST_X" "$HEADER_MENU_ARTIST_Y"
  sleep 0.5
  assert_log_contains_since "$MARKER" \
    'column header visibility changed.*column="artist".*visible=true' \
    "right-click header menu restored Artist"
  assert_db_query_true \
    "SELECT instr(substr(value, instr(value, ';') + 1), 'artist') > 0 FROM settings WHERE key = 'ui.column_layout';" \
    "header menu persisted Artist as visible"

  # Leave no popover behind for the following flows to click through.
  key "Escape"
  sleep 0.2
}
