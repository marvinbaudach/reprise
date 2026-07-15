#!/usr/bin/env bash

find_preferences_window() {
  local active=""
  for _ in $(seq 1 30); do
    active="$(xdotool getactivewindow 2>/dev/null || true)"
    # The title is translated, so identify the transient by its active X11
    # window id instead of matching English UI text.
    if [ -n "$active" ] && [ "$active" != "$WINDOW_ID" ]; then
      echo "$active"
      return 0
    fi
    sleep 0.1
  done
  return 1
}

click_preferences_relative() {
  local window_id="$1" relative_x="$2" relative_y="$3"
  local geometry window_x window_y
  geometry="$(xdotool getwindowgeometry --shell "$window_id" 2>/dev/null)"
  window_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  window_y="$(sed -n 's/^Y=//p' <<<"$geometry")"
  click_at "$((window_x + relative_x))" "$((window_y + relative_y))"
}

preferences_width() {
  local window_id="$1" geometry
  geometry="$(xdotool getwindowgeometry --shell "$window_id" 2>/dev/null)"
  sed -n 's/^WIDTH=//p' <<<"$geometry"
}

run_preferences_flow() {
  log_step "flow 6: Preferences page sidebar, cards, and Library Window controls…"

  # Compact layouts can replace the mapped surface. Always address the
  # currently active Library window before opening its primary menu.
  WINDOW_ID="$(xdotool getactivewindow 2>/dev/null)"
  maximize_window
  click_window_from_right "$PRIMARY_MENU_FROM_RIGHT" 28
  sleep 0.3
  screenshot "17-main-menu"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/17-main-menu.png"

  local marker preference_window width
  marker=$(log_marker)
  key "Home"
  key "Return"
  sleep 0.8
  assert_log_contains_since "$marker" "preferences window presented" \
    "primary-menu keyboard navigation opened Preferences"
  if ! preference_window="$(find_preferences_window)"; then
    log_fail "Preferences window did not become the active transient"
    return
  fi
  width="$(preferences_width "$preference_window")"

  # --- Vertical settings sidebar geometry (redesign) ----------------------
  # The redesign replaced the horizontal top ViewSwitcher with a vertical
  # NavigationSplitView sidebar: a `.navigation-sidebar` ListBox on the LEFT
  # drives the content ViewStack on the RIGHT. Page switching is now a click on
  # the target page's SIDEBAR ROW, not a header tab.
  #
  # Offsets are relative to the preferences window origin (its own transient
  # window, default 760x680 — NOT the maximized main window). The Y values are
  # the row centers measured from an 18-preferences-appearance capture; rows are
  # evenly spaced (~38px) below the "Preferences" sidebar header, in PAGE_ORDER:
  # Playback, Appearance, Layout, Library, Synchronization, Plugins. SIDEBAR_X
  # sits on the row label, comfortably inside the ~190px-wide sidebar.
  local SIDEBAR_X=70
  local ROW_PLAYBACK=75
  local ROW_APPEARANCE=113
  local ROW_LAYOUT=151
  local ROW_LIBRARY=189
  local ROW_SYNC=227
  local ROW_PLUGINS=264

  screenshot "18-preferences-appearance"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/18-preferences-appearance.png"
  assert_db_query_true \
    "SELECT COUNT(*) = 0 FROM settings WHERE key = 'ui.color_scheme';" \
    "Appearance did not persist a manual color scheme"

  # Switch to the Layout page via its sidebar row.
  click_preferences_relative "$preference_window" "$SIDEBAR_X" "$ROW_LAYOUT"
  sleep 0.3
  screenshot "19-preferences-layout"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/19-preferences-layout.png"

  # In-page controls live in the content area to the RIGHT of the sidebar.
  # The Player Bar group shows two choice cards (Top | Bottom) side by side,
  # near the top of the content region.
  local CARD_TOP_X=345
  local CARD_BOTTOM_X=598
  local CARD_Y=190
  click_preferences_relative "$preference_window" "$CARD_TOP_X" "$CARD_Y"
  sleep 0.2
  assert_db_value "player_bar_position" "top" "Top Player Bar card persisted"
  click_preferences_relative "$preference_window" "$CARD_BOTTOM_X" "$CARD_Y"
  sleep 0.2
  assert_db_value "player_bar_position" "bottom" "Bottom Player Bar card persisted"

  # The four Library Window switch rows stack below the cards; clicking a row
  # toggles its switch. Their trailing toggles sit near the content's right edge.
  local SWITCH_X=$((width - 90))
  local SWITCH_SIDEBAR_Y=361
  local SWITCH_FILTER_Y=416
  local SWITCH_INFO_Y=471
  local SWITCH_STATUS_Y=526
  click_preferences_relative "$preference_window" "$SWITCH_X" "$SWITCH_SIDEBAR_Y"
  sleep 0.2
  assert_db_value "ui.sidebar_visible" "0" "Layout switch hid the sidebar"
  click_preferences_relative "$preference_window" "$SWITCH_X" "$SWITCH_FILTER_Y"
  sleep 0.2
  assert_db_value "ui.browse_visible" "0" "Layout switch hid the filter bar"
  click_preferences_relative "$preference_window" "$SWITCH_X" "$SWITCH_INFO_Y"
  sleep 0.2
  assert_db_value "ui.info_panel_visible" "0" "Layout switch hid the information panel"
  click_preferences_relative "$preference_window" "$SWITCH_X" "$SWITCH_STATUS_Y"
  sleep 0.2
  assert_db_value "ui.status_visible" "0" "Layout switch hid the status line"

  # Visit every remaining top-level page via its sidebar row. Their control
  # semantics have focused Rust/display coverage; this pointer flow proves each
  # page stays reachable through the vertical settings sidebar.
  click_preferences_relative "$preference_window" "$SIDEBAR_X" "$ROW_LIBRARY"
  sleep 0.3
  screenshot "20-preferences-library"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/20-preferences-library.png"
  click_preferences_relative "$preference_window" "$SIDEBAR_X" "$ROW_PLUGINS"
  sleep 0.3
  screenshot "21-preferences-plugins"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/21-preferences-plugins.png"
  click_preferences_relative "$preference_window" "$SIDEBAR_X" "$ROW_PLAYBACK"
  sleep 0.3
  screenshot "22-preferences-playback"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/22-preferences-playback.png"

  screenshot "23-preferences-final"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/23-preferences-final.png"
}
