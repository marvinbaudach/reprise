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
  log_step "flow 6: native Preferences cards and Library Window controls…"

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

  screenshot "18-preferences-appearance"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/18-preferences-appearance.png"
  assert_db_query_true \
    "SELECT COUNT(*) = 0 FROM settings WHERE key = 'ui.color_scheme';" \
    "Appearance did not persist a manual color scheme"

  # The five native header tabs retain their natural width. Layout is centered
  # slightly left of the window midpoint because the close button occupies the
  # trailing header slot.
  click_preferences_relative "$preference_window" "$((width / 2 - 20))" 28
  sleep 0.3
  screenshot "19-preferences-layout"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/19-preferences-layout.png"

  click_preferences_relative "$preference_window" "$((width / 2 - 135))" 200
  sleep 0.2
  assert_db_value "player_bar_position" "top" "Top Player Bar card persisted"
  click_preferences_relative "$preference_window" "$((width / 2 + 135))" 200
  sleep 0.2
  assert_db_value "player_bar_position" "bottom" "Bottom Player Bar card persisted"

  local switch_x=$((width - 100))
  click_preferences_relative "$preference_window" "$switch_x" 360
  sleep 0.2
  assert_db_value "ui.sidebar_visible" "0" "Layout switch hid the sidebar"
  click_preferences_relative "$preference_window" "$switch_x" 416
  sleep 0.2
  assert_db_value "ui.browse_visible" "0" "Layout switch hid the filter bar"
  click_preferences_relative "$preference_window" "$switch_x" 472
  sleep 0.2
  assert_db_value "ui.info_panel_visible" "0" "Layout switch hid the information panel"
  click_preferences_relative "$preference_window" "$switch_x" 528
  sleep 0.2
  assert_db_value "ui.status_visible" "0" "Layout switch hid the status line"

  # Visit every remaining top-level page. Their control semantics have focused
  # Rust/display coverage; this pointer flow proves the native tabs stay
  # reachable without the removed bottom navigation obscuring page content.
  click_preferences_relative "$preference_window" "$((width / 2 + 95))" 28
  sleep 0.3
  screenshot "20-preferences-library"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/20-preferences-library.png"
  click_preferences_relative "$preference_window" "$((width / 2 + 205))" 28
  sleep 0.3
  screenshot "21-preferences-plugins"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/21-preferences-plugins.png"
  click_preferences_relative "$preference_window" "$((width / 2 - 255))" 28
  sleep 0.3
  screenshot "22-preferences-playback"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/22-preferences-playback.png"

  screenshot "23-preferences-final"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/23-preferences-final.png"
}
