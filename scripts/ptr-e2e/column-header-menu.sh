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

  MARKER=$(log_marker)
  click_window_relative "$TITLE_HEADER_X" "$COLUMN_HEADER_Y" 3
  sleep 0.3
  click_at "$HEADER_MENU_ARTIST_X" "$HEADER_MENU_ARTIST_Y"
  sleep 0.5
  assert_log_contains_since "$MARKER" \
    'column header visibility changed.*column="artist".*visible=true' \
    "right-click header menu restored Artist"
  assert_db_query_true \
    "SELECT instr(substr(value, instr(value, ';') + 1), 'artist') > 0 FROM settings WHERE key = 'ui.column_layout';" \
    "header menu persisted Artist as visible"
}
