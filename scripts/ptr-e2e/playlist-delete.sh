#!/usr/bin/env bash

# Deletes the seeded manual playlist through its real sidebar context menu.
run_playlist_delete_flow() {
  log_step "flow 1c: confirmed sidebar playlist deletion…"
  click_window_relative "$SIDEBAR_PLAYLIST_X" "$SIDEBAR_PLAYLIST_Y"
  sleep 0.4
  click_window_relative "$SIDEBAR_PLAYLIST_X" "$SIDEBAR_PLAYLIST_Y" 3
  sleep 0.3
  screenshot "03c-playlist-context-menu"
  click_at "$SIDEBAR_PLAYLIST_DELETE_X" "$SIDEBAR_PLAYLIST_DELETE_Y"
  sleep 0.8
  screenshot "03d-playlist-delete-confirmation"

  MARKER=$(log_marker)
  click_at "$PLAYLIST_DELETE_CONFIRM_X" "$PLAYLIST_DELETE_CONFIRM_Y"
  sleep 0.7
  assert_log_contains_since "$MARKER" 'playlist deleted.*playlist_name="Pointer Playlist"' \
    "sidebar context menu deleted the playlist"
  assert_log_contains_since "$MARKER" "selected source vanished; falling back to Library" \
    "deleting the open playlist returned to Library"
  assert_db_query_true "SELECT COUNT(*) = 0 FROM playlists;" \
    "playlist row was deleted"
  assert_db_query_true "SELECT COUNT(*) = $PTR_E2E_N_TRACKS FROM tracks;" \
    "playlist deletion kept every library track"
}
