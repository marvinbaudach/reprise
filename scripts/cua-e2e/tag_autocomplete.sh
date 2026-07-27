#!/usr/bin/env bash

write_tag_autocomplete_fixture() {
  local path=$1 title=$2 artist=$3

  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=2 \
    -metadata title="$title" \
    -metadata artist="$artist" \
    -metadata album="Autocomplete Album" \
    -c:a flac "$path"
}

run_tag_autocomplete_surface_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/tag-autocomplete-fixture-music"
  local dialog_path display_path

  echo "[cua-e2e] tag-autocomplete-surface: field-aligned themed dropdown"
  mkdir -p "$fixture_dir"
  write_tag_autocomplete_fixture \
    "$fixture_dir/00_seed.flac" "Autocomplete Seed" "Replace Me"
  write_tag_autocomplete_fixture \
    "$fixture_dir/01_cogitations.flac" "Cogitations Track" "Cogitations"
  write_tag_autocomplete_fixture \
    "$fixture_dir/02_dissonance.flac" "Dissonance Track" "Cognitive Dissonance"
  write_tag_autocomplete_fixture \
    "$fixture_dir/03_cognac.flac" "Cognac Track" "Radio Cognac"

  start_scenario_app \
    tag-autocomplete-surface "$fixture_dir" "open:1" \
    "$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS"
  dialog_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Artist" tag-autocomplete-dialog)
  assert_snapshot_contains "$dialog_path" "Cogitations Track"

  # The Title entry owns initial focus. Keyboard traversal avoids confusing the
  # editor's "Artist" entry with the identically named library column beneath
  # the modal dialog.
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" tab tag-autocomplete-artist-focus
  cua_hotkey_focused \
    "$APP_PID" "$WINDOW_ID" tag-autocomplete-select-all ctrl a
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" backspace tag-autocomplete-clear
  cua_type_text_window "$APP_PID" "$WINDOW_ID" "Cog" tag-autocomplete-type
  sleep 0.5

  display_path="$CUA_E2E_OUT_DIR/tag-autocomplete-open-display.png"
  import -window root "$display_path"
  if [[ ! -s "$display_path" ]]; then
    echo "autocomplete scenario did not retain full-display evidence" >&2
    return 1
  fi

  cua_click_label "$APP_PID" "$WINDOW_ID" "Cancel" tag-autocomplete-cancel
  finish_scenario tag-autocomplete-surface \
    "dev scan complete" \
    "tag editor presented"
}
