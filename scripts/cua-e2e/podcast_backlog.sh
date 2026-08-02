#!/usr/bin/env bash

# Acceptance for the grouped list at a realistic size. `source_content.sh`
# subscribes to a one-episode fixture feed, which proves the add flow but says
# nothing about the thing the list-view fix pass was about: what a show with a
# real backlog looks like. A one-episode group cannot show the ten-episode
# window, cannot show a row that is idle rather than downloading, and cannot
# show several shows fitting on one screen.
#
# The backlog is written straight into the profile database rather than served
# through a fixture feed. Sixty episodes across four sources would otherwise
# mean four fixture feeds, a refresh cycle, and a wait for all of them — while
# what is under test here is purely how stored rows are rendered.

PODCAST_BACKLOG_EPISODES_PER_SOURCE=15

# Seeds three shows and one channel with a backlog each, entirely offline.
#
# `added_at` is deliberately later than every episode's `first_seen_at`, which
# is exactly the shape a first fetch leaves behind: the backlog was already
# there when the subscription was created, so none of it counts as new. That is
# what makes `0 new` here an assertion about the rule rather than about an
# empty library.
podcast_backlog_seed() {
  local database="$CUA_E2E_SCRATCH_ROOT/podcast-backlog/data/reprise/reprise.db"

  if [[ ! -f "$database" ]]; then
    echo "podcast-backlog: expected a profile database at $database" >&2
    return 1
  fi

  sqlite3 "$database" <<'SQL'
INSERT INTO settings (key, value) VALUES ('module.podcasts.enabled', '1')
  ON CONFLICT(key) DO UPDATE SET value = '1';
INSERT INTO settings (key, value) VALUES ('module.youtube.enabled', '1')
  ON CONFLICT(key) DO UPDATE SET value = '1';
INSERT INTO podcast_subscriptions
  (id, kind, feed_url, title, author, image_url, added_at)
VALUES
  (1, 'rss', 'https://example.test/grim',
   'Grim Dystopian: Earballs', 'Grim Dystopian', NULL, 2000),
  (2, 'rss', 'https://example.test/systems', 'Systems Weekly', 'Ada Lovelace', NULL, 2000),
  (3, 'rss', 'https://example.test/quiet', 'The Quiet Hour', 'Quiet Media', NULL, 2000),
  (4, 'youtube', 'https://www.youtube.com/channel/UClofi',
   'Lo-Fi Cave', 'Lo-Fi Cave', NULL, 2000);
SQL

  python3 - "$database" "$PODCAST_BACKLOG_EPISODES_PER_SOURCE" <<'PY'
import sqlite3
import sys

database, per_source = sys.argv[1], int(sys.argv[2])
titles = {
    1: ["Mental Anguish (The Band That Wouldn't Die)",
        "Filthy Earballs, Volume Nine",
        "A Long One About Nothing In Particular"],
    2: ["The Build Is Broken Again",
        "Everything Is a Queue",
        "On Call, Off Grid"],
    3: ["Rain on a Tin Roof",
        "Forty Winks",
        "The Long Way Round"],
    4: ["Midnight Drive Mix | Music for Gaming & Focus",
        "Rainy Window Beats | Music for Gaming & Focus",
        "Deep Focus Session 12 | Music for Gaming & Focus"],
}
# One duration on each side of the hour boundary, so the rendered rows have to
# show both "53 min" and "2 h 05" rather than one ambiguous clock format.
durations = {0: 7_500, 1: 3_180, 2: 3_180}
published_base = 1_784_900_000
rows = []
for subscription, subject in titles.items():
    for slot in range(per_source):
        rows.append((
            subscription * 100 + slot,
            subscription,
            f"g{subscription}-{slot}",
            subject[slot % len(subject)],
            f"https://example.test/{subscription}-{slot}.mp3",
            published_base - slot * 86_400,
            durations[slot % 3],
            1_000,
        ))

connection = sqlite3.connect(database)
connection.executemany(
    "INSERT INTO podcast_episodes (id, subscription_id, guid, title, audio_url,"
    " published_at, duration_secs, position_ms, first_seen_at)"
    " VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)",
    rows,
)
connection.commit()
connection.close()
PY
}

# `assert_snapshot_contains` matches a label in full, which the meta line under
# an episode title cannot satisfy: it is assembled from date, duration and
# status, and its date half moves with the calendar. This matches the part that
# is under test — the trailing duration — without pinning today's date into the
# expectation.
podcast_backlog_assert_meta_ends_with() {
  local snapshot=$1 suffix=$2

  if ! jq -e --arg suffix " · $suffix" \
    '[.elements[]? | select((.label // "") | endswith($suffix))] | length > 0' \
    "$snapshot" >/dev/null; then
    echo "no meta line ending in '· $suffix': $snapshot" >&2
    return 1
  fi
}

run_podcast_backlog_scenario() {
  local music="$CUA_E2E_SCRATCH_ROOT/podcast-backlog-music"
  local fixtures="$CUA_E2E_SCRATCH_ROOT/podcast-backlog-fixtures"
  local shows=3
  local podcast_total=$((PODCAST_BACKLOG_EPISODES_PER_SOURCE * shows))
  local failure_heading="Couldn't refresh 3 sources"
  local snapshot

  echo "[cua-e2e] podcast-backlog: shows with a real backlog stay scannable"
  mkdir -p "$music" "$fixtures"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=5 \
    -metadata title="Podcast Backlog Track" -metadata artist="Reprise E2E" \
    -c:a flac "$music/podcast_backlog.flac"

  # Phase 1 exists to create the profile database; the module is still off, so
  # nothing podcast-shaped is reachable yet.
  REPRISE_PODCASTS_FIXTURE_DIR="$fixtures" \
  REPRISE_RADIO_FIXTURE_DIR="$fixtures" \
  REPRISE_SMOKE_SOURCE=podcasts \
    start_scenario_app podcast-backlog "$music" "" 25
  wait_for_label "$APP_PID" "$WINDOW_ID" "Toggle sidebar" pb-off-ready >/dev/null
  finish_scenario podcast-backlog "dev scan complete"

  podcast_backlog_seed

  # Phase 2: the list with everything in it.
  REPRISE_PODCASTS_FIXTURE_DIR="$fixtures" \
  REPRISE_RADIO_FIXTURE_DIR="$fixtures" \
  REPRISE_SMOKE_SOURCE=podcasts \
    start_scenario_app podcast-backlog "$music" "" 60
  wait_for_label "$APP_PID" "$WINDOW_ID" "Toggle sidebar" pb-on-ready >/dev/null

  # POD-19: the empty fixture directory makes all three cached shows fail
  # their startup refresh without touching a network. The compact banner keeps
  # its retry, Details and labelled close actions in one summary row. Expand
  # Details first so dismissing proves the large state in the reported bug can
  # always be removed, not only the initially collapsed state.
  snapshot=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "$failure_heading" pb-refresh-failure)
  assert_snapshot_contains "$snapshot" "Try again"
  assert_snapshot_contains "$snapshot" "Details"
  assert_snapshot_contains "$snapshot" "Dismiss"
  cua_click_label "$APP_PID" "$WINDOW_ID" "Details" pb-refresh-details
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" "Copy" pb-refresh-details-open)
  assert_snapshot_contains "$snapshot" "Dismiss"
  cua_click_label "$APP_PID" "$WINDOW_ID" "Dismiss" pb-refresh-dismiss
  wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "$failure_heading" pb-refresh-dismissed >/dev/null

  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" \
    "Grim Dystopian: Earballs" pb-collapsed)
  # Three shows are on screen at once while collapsed — the failure this whole
  # pass started from was a single show filling the window.
  assert_snapshot_contains "$snapshot" "Systems Weekly"
  assert_snapshot_contains "$snapshot" "The Quiet Hour"
  assert_snapshot_contains "$snapshot" "$shows shows · $podcast_total episodes · 0 new"
  # The author repeats the start of the title, so it must not get its own row.
  assert_snapshot_absent "$snapshot" "0.0 MB"
  assert_snapshot_absent "$snapshot" "latest —"

  # The expander carries the show's name, so it is addressable as itself. It
  # was nameless until this scenario went looking for it — a screen reader
  # heard "toggle button" and nothing else.
  cua_click_label "$APP_PID" "$WINDOW_ID" "Grim Dystopian: Earballs" pb-expand
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" \
    "Show all $PODCAST_BACKLOG_EPISODES_PER_SOURCE episodes" pb-expanded)
  # An expanded group shows its window, not its whole backlog, and its rows
  # carry no state text at all while nothing is downloading.
  assert_snapshot_absent "$snapshot" "Not downloaded"
  # Durations read as plain language on both sides of the hour.
  podcast_backlog_assert_meta_ends_with "$snapshot" "53 min"
  podcast_backlog_assert_meta_ends_with "$snapshot" "2 h 05"
  assert_snapshot_absent "$snapshot" "0:53"

  finish_scenario podcast-backlog "dev scan complete"

  # Phase 3: the YouTube page is its own surface and counts channels.
  REPRISE_PODCASTS_FIXTURE_DIR="$fixtures" \
  REPRISE_RADIO_FIXTURE_DIR="$fixtures" \
  REPRISE_SMOKE_SOURCE=youtube \
    start_scenario_app podcast-backlog "$music" "" 45
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" "Lo-Fi Cave" pb-youtube)
  assert_snapshot_contains "$snapshot" \
    "1 channel · $PODCAST_BACKLOG_EPISODES_PER_SOURCE episodes · 0 new"
  assert_snapshot_contains "$snapshot" "Add channel"
  assert_snapshot_absent "$snapshot" "0.0 MB"
  assert_snapshot_absent "$snapshot" "Add YouTube channel"

  finish_scenario podcast-backlog "dev scan complete"
}
