#!/usr/bin/env bash

# `MTP-46`/`SET-9` acceptance: the two source modules are opt-in, and
# switching one off has to take it out of the sync it feeds, not just out of
# the sidebar. The unit tests prove the core gate and the display test proves
# the panel, both against constructed values; this one starts the real app
# three times against one profile and looks at what the running GUI offers.
#
# It moves the switch by writing the persisted setting between runs rather
# than by operating the Preferences switch, and that boundary is deliberate:
# the Preferences dialog keeps all nine pages in the accessibility tree at
# once, so a label being present there proves nothing about the page being
# shown, and driving it left the run asserting against invisible widgets. The
# switch widget writing its setting is covered by the preferences unit tests;
# what only this scenario can cover — a real app reading that setting and
# building a real device page from it — is what it therefore covers.
#
# Fixture-free otherwise: this is about module switches, not content, so it
# needs no feed, no network, and no `test-fixtures` build.

run_source_modules_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/source-modules-music"
  local device_root="$CUA_E2E_SCRATCH_ROOT/source-modules-device"

  echo "[cua-e2e] source-modules: MTP-46, a switched-off source leaves the sync"
  mkdir -p "$fixture_dir" "$device_root"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=5 \
    -metadata title="Source Modules Track" \
    -metadata artist="Reprise E2E" \
    -metadata album="Source Modules" \
    -c:a flac "$fixture_dir/source_modules.flac"

  # Phase 1 — a fresh profile. Both source modules ship off (`NET-1a`), so
  # neither may claim a row on the phone.
  source_modules_open_device_page "$fixture_dir" "$device_root" off
  assert_snapshot_absent "$SOURCE_MODULES_PAGE" "YouTube audio"
  assert_snapshot_absent "$SOURCE_MODULES_PAGE" "Podcast episodes"
  assert_snapshot_contains "$SOURCE_MODULES_PAGE" "Playlists"
  finish_scenario source-modules "dev scan complete"

  # Phase 2 — both on, the way switching them on in Preferences persists it.
  source_modules_set_setting module.podcasts.enabled 1
  source_modules_set_setting module.youtube.enabled 1
  source_modules_open_device_page "$fixture_dir" "$device_root" on
  assert_snapshot_contains "$SOURCE_MODULES_PAGE" "YouTube audio"
  assert_snapshot_contains "$SOURCE_MODULES_PAGE" "Podcast episodes"
  finish_scenario source-modules "dev scan complete"

  # Phase 3 — YouTube off, Podcasts untouched. `MTP-46`'s whole point, and
  # issue #96's: the two are peers, so one switch must not move the other.
  source_modules_set_setting module.youtube.enabled 0
  source_modules_open_device_page "$fixture_dir" "$device_root" youtube-off
  assert_snapshot_absent "$SOURCE_MODULES_PAGE" "YouTube audio"
  assert_snapshot_contains "$SOURCE_MODULES_PAGE" "Podcast episodes"
  assert_snapshot_contains "$SOURCE_MODULES_PAGE" "Playlists"
  finish_scenario source-modules "dev scan complete"
}

# Starts the app against the scenario's persistent profile, opens the
# simulated phone's page, and leaves the page snapshot in
# `$SOURCE_MODULES_PAGE`. `$3` only distinguishes the evidence file names.
source_modules_open_device_page() {
  local fixture_dir=$1 device_root=$2 phase=$3

  REPRISE_SMOKE_DEVICE_ROOT="$device_root" \
  REPRISE_SMOKE_DEVICE_PLAYLIST="Recently added" \
  REPRISE_SMOKE_DEVICE_UI_ONLY=1 \
    start_scenario_app source-modules "$fixture_dir" "" 30

  wait_for_label "$APP_PID" "$WINDOW_ID" "Toggle sidebar" "sm-$phase-ready" >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Toggle sidebar" "sm-$phase-sidebar"
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Simulated MTP Phone" "sm-$phase-card" >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Open Simulated MTP Phone" "sm-$phase-open"
  SOURCE_MODULES_PAGE=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Transfer profile" "sm-$phase-page")
}

# Writes one persisted setting into the scenario's profile database while the
# app is stopped — the same row `modules::set_enabled` writes ('1'/'0' in
# `settings`). Deliberately between runs, never while the app holds the file.
source_modules_set_setting() {
  local key=$1 value=$2
  local database="$CUA_E2E_SCRATCH_ROOT/source-modules/data/reprise/reprise.db"

  if [[ ! -f "$database" ]]; then
    echo "source-modules: expected a profile database at $database" >&2
    return 1
  fi
  sqlite3 "$database" \
    "INSERT INTO settings (key, value) VALUES ('$key', '$value')
     ON CONFLICT(key) DO UPDATE SET value = '$value';"
}
