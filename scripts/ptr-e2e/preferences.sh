#!/usr/bin/env bash

PREFERENCES_DIALOG_WIDTH=760
PREFERENCES_DIALOG_HEIGHT=680

preferences_dialog_rect() {
  local host_rect host_x host_y host_width host_height
  host_rect="$(window_rect)"
  read -r host_x host_y host_width host_height <<<"$host_rect"
  printf '%s %s %s %s' \
    "$((host_x + (host_width - PREFERENCES_DIALOG_WIDTH) / 2))" \
    "$((host_y + (host_height - PREFERENCES_DIALOG_HEIGHT) / 2))" \
    "$PREFERENCES_DIALOG_WIDTH" \
    "$PREFERENCES_DIALOG_HEIGHT"
}

click_preferences_dialog_relative() {
  local dialog_rect="$1" relative_x="$2" relative_y="$3"
  local dialog_x dialog_y dialog_width dialog_height
  read -r dialog_x dialog_y dialog_width dialog_height <<<"$dialog_rect"
  if [ "$relative_x" -lt 0 ] || [ "$relative_x" -ge "$dialog_width" ] \
    || [ "$relative_y" -lt 0 ] || [ "$relative_y" -ge "$dialog_height" ]; then
    log_fail "Preferences click at ${relative_x}x${relative_y} fell outside the ${dialog_width}x${dialog_height} dialog"
    return 0
  fi
  click_at "$((dialog_x + relative_x))" "$((dialog_y + relative_y))"
}

run_preferences_flow() {
  start_flow "6: Preferences page sidebar, cards, and Library Window controls…"

  # Compact layouts can replace the mapped surface. Always address the
  # currently active Library window before opening its primary menu.
  WINDOW_ID="$(xdotool getactivewindow 2>/dev/null)"
  if ! maximize_window; then
    log_step "flow 6 skipped: coordinate checks require the fixed maximized Library geometry"
    return
  fi
  screenshot "17-main-menu-closed"
  key "F10"
  sleep 0.3

  local marker dialog_rect width
  marker=$(log_marker)
  # The popover opens with Compact Mode focused, so there is deliberately no
  # Home press here: Home is not a no-op and costs one menu-navigation step.
  # Four Down presses traverse Edit Column Layout, Library Doctor, Import
  # Playlist, then Preferences; section separators do not take focus.
  key "Down"
  key "Down"
  key "Down"
  key "Down"
  screenshot "17-main-menu-preferences-focused"
  assert_screenshots_differ \
    "$PTR_E2E_OUT_DIR/17-main-menu-closed.png" \
    "$PTR_E2E_OUT_DIR/17-main-menu-preferences-focused.png" \
    "F10 visibly opened the primary menu before Preferences activation"
  key "Return"
  sleep 0.8
  assert_log_contains_since "$marker" "preferences dialog presented" \
    "primary-menu keyboard navigation opened Preferences"
  screenshot "18-preferences-appearance"
  assert_screenshots_differ \
    "$PTR_E2E_OUT_DIR/17-main-menu-preferences-focused.png" \
    "$PTR_E2E_OUT_DIR/18-preferences-appearance.png" \
    "Preferences dialog visibly replaced the focused primary menu"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/18-preferences-appearance.png"
  dialog_rect="$(preferences_dialog_rect)"
  width="$(cut -d' ' -f3 <<<"$dialog_rect")"

  # --- Vertical settings sidebar geometry (redesign) ----------------------
  # The redesign replaced the horizontal top ViewSwitcher with a vertical
  # NavigationSplitView sidebar: a `.navigation-sidebar` ListBox on the LEFT
  # drives the content ViewStack on the RIGHT. Page switching is now a click on
  # the target page's SIDEBAR ROW, not a header tab.
  #
  # Preferences is a centered 760x680 AdwDialog hosted inside the maximized
  # Library window, not a separate X11 toplevel. Offsets are relative to that
  # authored dialog rectangle. The Y values are the row centers measured from
  # an 18-preferences-appearance capture; rows are evenly spaced (~38px) below
  # the "Preferences" sidebar header, in PAGE_ORDER: Playback, Appearance,
  # Layout, Library, Synchronization, Plugins. SIDEBAR_X sits on the row label,
  # comfortably inside the ~190px-wide sidebar.
  local SIDEBAR_X=70
  local ROW_PLAYBACK=75
  local ROW_APPEARANCE=113
  local ROW_LAYOUT=151
  local ROW_LIBRARY=189
  local ROW_SYNC=227
  local ROW_PLUGINS=264

  assert_db_query_true \
    "SELECT COUNT(*) = 0 FROM settings WHERE key = 'ui.color_scheme';" \
    "Appearance did not persist a manual color scheme"

  # Switch to the Layout page via its sidebar row.
  click_preferences_dialog_relative "$dialog_rect" "$SIDEBAR_X" "$ROW_LAYOUT"
  sleep 0.3
  screenshot "19-preferences-layout"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/19-preferences-layout.png"

  # In-page controls live in the content area to the RIGHT of the sidebar.
  # The Player Bar group shows two choice cards (Top | Bottom) side by side,
  # near the top of the content region.
  local CARD_TOP_X=345
  local CARD_BOTTOM_X=598
  local CARD_Y=190
  click_preferences_dialog_relative "$dialog_rect" "$CARD_TOP_X" "$CARD_Y"
  sleep 0.2
  assert_db_value "player_bar_position" "top" "Top Player Bar card persisted"
  click_preferences_dialog_relative "$dialog_rect" "$CARD_BOTTOM_X" "$CARD_Y"
  sleep 0.2
  assert_db_value "player_bar_position" "bottom" "Bottom Player Bar card persisted"

  # The four Library Window switch rows stack below the cards; clicking a row
  # toggles its switch. Their trailing toggles sit near the content's right edge.
  local SWITCH_X=$((width - 90))
  local SWITCH_SIDEBAR_Y=361
  local SWITCH_FILTER_Y=416
  local SWITCH_INFO_Y=471
  local SWITCH_STATUS_Y=526
  click_preferences_dialog_relative "$dialog_rect" "$SWITCH_X" "$SWITCH_SIDEBAR_Y"
  sleep 0.2
  assert_db_value "ui.sidebar_visible" "0" "Layout switch hid the sidebar"
  click_preferences_dialog_relative "$dialog_rect" "$SWITCH_X" "$SWITCH_FILTER_Y"
  sleep 0.2
  assert_db_value "ui.browse_visible" "0" "Layout switch hid the filter bar"
  # Show Information Panel is the odd one out: it defaults to FALSE
  # (reprise-core `get_info_panel_visible_in`), so this row starts off while its
  # three neighbours start on — `19-preferences-layout.png` shows it before any
  # click. The click therefore SHOWS the panel; demanding "0" would demand a
  # toggle that never had anything to hide.
  click_preferences_dialog_relative "$dialog_rect" "$SWITCH_X" "$SWITCH_INFO_Y"
  sleep 0.2
  assert_db_value "ui.info_panel_visible" "1" "Layout switch showed the information panel"
  click_preferences_dialog_relative "$dialog_rect" "$SWITCH_X" "$SWITCH_STATUS_Y"
  sleep 0.2
  assert_db_value "ui.status_visible" "0" "Layout switch hid the status line"

  # Visit every remaining top-level page via its sidebar row. Their control
  # semantics have focused Rust/display coverage; this pointer flow proves each
  # page stays reachable through the vertical settings sidebar.
  click_preferences_dialog_relative "$dialog_rect" "$SIDEBAR_X" "$ROW_LIBRARY"
  sleep 0.3
  screenshot "20-preferences-library"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/20-preferences-library.png"
  click_preferences_dialog_relative "$dialog_rect" "$SIDEBAR_X" "$ROW_PLUGINS"
  sleep 0.3
  screenshot "21-preferences-plugins"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/21-preferences-plugins.png"
  click_preferences_dialog_relative "$dialog_rect" "$SIDEBAR_X" "$ROW_PLAYBACK"
  sleep 0.3
  screenshot "22-preferences-playback"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/22-preferences-playback.png"

  screenshot "23-preferences-final"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/23-preferences-final.png"
}
