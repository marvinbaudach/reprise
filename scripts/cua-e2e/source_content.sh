#!/usr/bin/env bash

# Acceptance for what the three network sources actually *do*, as opposed to
# `source_modules.sh`, which only covers their switches. Every request is
# answered from a fixture directory (`REPRISE_PODCASTS_FIXTURE_DIR`,
# `REPRISE_RADIO_FIXTURE_DIR`) or a fake `yt-dlp` (`REPRISE_YTDLP_BIN`), so the
# app under test reaches the network at no point — `AGENTS.md` requires that,
# and a scenario that quietly hit a real feed would also be flaky.
#
# The module switches are written straight into the profile database between
# runs, for the reason `source_modules.sh` records: `AdwPreferencesDialog`
# keeps every page in the accessibility tree at once, so driving the switch
# means asserting against widgets that are present but not shown.

# `podcasts/http.rs` names a feed fixture `feed-{component}.xml`, where the
# component maps every character outside `[A-Za-z0-9._-]` to `_`. The `.xml`
# is *appended*, so a feed URL that already ends in `.xml` produces a file
# ending `.xml.xml` — this uses a URL without a suffix to keep that off the
# page.
SOURCE_CONTENT_FEED_URL="https://example.test/feed"
SOURCE_CONTENT_FEED_FILE="feed-https___example.test_feed.xml"

run_source_podcasts_scenario() {
  local music="$CUA_E2E_SCRATCH_ROOT/source-podcasts-music"
  local fixtures="$CUA_E2E_SCRATCH_ROOT/source-podcasts-fixtures"
  local snapshot

  echo "[cua-e2e] source-podcasts: subscribe to a feed and see its episode"
  mkdir -p "$music" "$fixtures"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=5 \
    -metadata title="Source Podcasts Track" -metadata artist="Reprise E2E" \
    -c:a flac "$music/source_podcasts.flac"
  cat >"$fixtures/$SOURCE_CONTENT_FEED_FILE" <<'FEED'
<?xml version="1.0"?>
<rss xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
  <channel>
    <title>Systems Weekly</title>
    <item>
      <title>The Gate Episode</title>
      <enclosure url="https://example.test/episode.mp3" type="audio/mpeg"/>
    </item>
  </channel>
</rss>
FEED

  # Phase 1: the module ships off. `REPRISE_SMOKE_SOURCE=podcasts` routes
  # through the sidebar, which has no Podcasts row while the module is off, so
  # the page is not reachable at all — asserted here rather than assumed,
  # because it is also what makes this phase the profile's first run and so
  # the moment its database exists to be edited.
  source_content_start podcasts "$music" "$fixtures" off
  snapshot=$(cua_snapshot "$APP_PID" "$WINDOW_ID" sp-off)
  assert_snapshot_absent "$snapshot" "No podcasts yet"
  assert_snapshot_absent "$snapshot" "Add podcast"
  finish_scenario source-podcasts "dev scan complete"

  # Phase 2: switched on, the page offers subscribing and the fixture feed
  # goes all the way to a listed episode.
  source_content_set_setting podcasts module.podcasts.enabled 1
  # 45s: the add flow itself takes about three seconds, and `finish_scenario`
  # waits out whatever remains of this timer before the phase ends.
  source_content_start podcasts "$music" "$fixtures" on 45
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" "No podcasts yet" sp-empty)
  assert_snapshot_contains "$snapshot" "Add podcast"
  assert_snapshot_absent "$snapshot" "Podcasts is turned off"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Add podcast" sp-add
  wait_for_label "$APP_PID" "$WINDOW_ID" "Add Podcast" sp-dialog >/dev/null
  cua_type_text_window "$APP_PID" "$WINDOW_ID" "$SOURCE_CONTENT_FEED_URL" sp-url
  # A URL turns the primary button from "Search" into "Preview" — asserting
  # that transition is what proves the entry was actually read, rather than
  # the dialog merely being open.
  wait_for_label "$APP_PID" "$WINDOW_ID" "Preview" sp-preview-offered >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Preview" sp-preview
  wait_for_label "$APP_PID" "$WINDOW_ID" "Systems Weekly" sp-preview-shown >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Subscribe" sp-subscribe

  # The page lists shows, not episodes — the episode titles live one level in.
  # The counts are the stronger assertion anyway: a show's *name* could have
  # come from the URL, but "1 show · 1 episode · 1 new" can only be true if
  # the fixture feed was parsed. Labels are matched exactly here, so these are
  # the elements' own strings rather than fragments of the row's aggregate.
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" "Systems Weekly" sp-show)
  assert_snapshot_contains "$snapshot" "1 show · 1 episode · 1 new"
  assert_snapshot_contains "$snapshot" "1 episode · 1 new · latest — · 0.0 MB"
  assert_snapshot_absent "$snapshot" "No podcasts yet"

  # Subscribing leaves the dialog up, so close it before the phase ends.
  # Not cosmetic: while it is open the window will not close, the smoke-quit
  # timer fires without the process ever exiting, and `finish_scenario` waits
  # for an exit that never comes — which is what hung two earlier runs, once
  # for two and a half hours.
  # Cancel rather than Escape, and deliberately without asserting the label
  # disappears: a closed dialog's labels linger in the accessibility tree, the
  # same way `AdwPreferencesDialog`'s nine pages all stay in it. The proof that
  # this worked is that the phase finishes at all — with the dialog still up,
  # the app never exits and `finish_scenario` waits forever.
  cua_click_label "$APP_PID" "$WINDOW_ID" "Cancel" sp-dialog-close

  finish_scenario source-podcasts "dev scan complete"
}

run_source_youtube_scenario() {
  local music="$CUA_E2E_SCRATCH_ROOT/source-youtube-music"
  local fixtures="$CUA_E2E_SCRATCH_ROOT/source-youtube-fixtures"
  local ytdlp="$CUA_E2E_SCRATCH_ROOT/source-youtube-ytdlp"
  local snapshot

  echo "[cua-e2e] source-youtube: add a channel through a faked yt-dlp"
  mkdir -p "$music" "$fixtures"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=5 \
    -metadata title="Source YouTube Track" -metadata artist="Reprise E2E" \
    -c:a flac "$music/source_youtube.flac"
  # `REPRISE_YTDLP_BIN` needs no cargo feature — unlike the podcast and radio
  # routers, `ytdlp.rs` reads it unconditionally. A resolve is invoked as
  # `--no-warnings --flat-playlist -J <url>` and must answer with the
  # channel's own title plus its entries.
  cat >"$ytdlp" <<'YTDLP'
#!/bin/sh
printf '%s\n' '{"title":"Reprise Test Channel","entries":[
  {"id":"vid-one","title":"Long Mix One","duration":3600},
  {"id":"vid-two","title":"Long Mix Two","duration":2400}
]}'
YTDLP
  chmod +x "$ytdlp"

  # Phase 1: opt-in, so the page is unreachable while the module is off.
  source_content_start youtube "$music" "$fixtures" off 25 "$ytdlp"
  snapshot=$(cua_snapshot "$APP_PID" "$WINDOW_ID" sy-off)
  assert_snapshot_absent "$snapshot" "No channels yet"
  assert_snapshot_absent "$snapshot" "Add channel"
  finish_scenario source-youtube "dev scan complete"

  # Phase 2: switched on, a channel URL resolves through the fake binary.
  source_content_set_setting youtube module.youtube.enabled 1
  source_content_start youtube "$music" "$fixtures" on 45 "$ytdlp"
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" "No channels yet" sy-empty)
  assert_snapshot_contains "$snapshot" "Add channel"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Add channel" sy-add
  wait_for_label "$APP_PID" "$WINDOW_ID" "Add Channel" sy-dialog >/dev/null

  # `SRC-6`: a source-foreign URL is refused here rather than silently handed
  # to the other source. Typed first so the refusal is observed on the same
  # dialog that then accepts a real channel URL.
  cua_type_text_window "$APP_PID" "$WINDOW_ID" "https://example.test/feed.xml" sy-wrong-url
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "That is an RSS feed — add it under Podcasts" sy-src6 >/dev/null

  cua_hotkey_focused "$APP_PID" "$WINDOW_ID" sy-select-all ctrl a
  cua_press_key_window "$APP_PID" "$WINDOW_ID" backspace sy-clear
  cua_type_text_window \
    "$APP_PID" "$WINDOW_ID" "https://www.youtube.com/@reprisetest" sy-url
  wait_for_label "$APP_PID" "$WINDOW_ID" "Preview" sy-preview-offered >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Preview" sy-preview
  wait_for_label "$APP_PID" "$WINDOW_ID" "Reprise Test Channel" sy-preview-shown >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Subscribe" sy-subscribe

  # Two entries in, two episodes out: the count can only come from the fake
  # binary's answer, where the channel's name could have come from the URL.
  snapshot=$(wait_for_label "$APP_PID" "$WINDOW_ID" "Reprise Test Channel" sy-channel)
  assert_snapshot_contains "$snapshot" "1 channel · 2 episodes · 2 new"
  assert_snapshot_absent "$snapshot" "No channels yet"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Cancel" sy-dialog-close
  finish_scenario source-youtube "dev scan complete"
}

# `$5` is the smoke-quit delay in seconds. It matters more than it looks: at 40
# seconds the app closed itself in the middle of the add flow, and the run then
# hung for two and a half hours instead of failing, because every subsequent
# driver call waited out its own timeout against a dead process.
#
# Starts the app for `$1` against that scenario's persistent profile, with the
# fixture routers pointed at `$3`, and leaves the shell snapshot in
# `$SOURCE_CONTENT_SHELL`. `$4` only distinguishes evidence file names.
source_content_start() {
  local area=$1 music=$2 fixtures=$3 phase=$4 quit_delay=${5:-25} ytdlp=${6:-}

  # `REPRISE_SMOKE_SOURCE` opens the source's own page directly. Reaching it
  # through the sidebar instead cost two runs: the shell auto-closes its side
  # panels at this window size ("Side panels were closed to fit the window"),
  # and a `space` aimed at the sidebar toggle reached the player and started
  # playback instead.
  REPRISE_PODCASTS_FIXTURE_DIR="$fixtures" \
  REPRISE_RADIO_FIXTURE_DIR="$fixtures" \
  REPRISE_YTDLP_BIN="$ytdlp" \
  REPRISE_SMOKE_SOURCE="$area" \
    start_scenario_app "source-$area" "$music" "" "$quit_delay"

  wait_for_label "$APP_PID" "$WINDOW_ID" "Toggle sidebar" "sc-$phase-ready" >/dev/null
}

# Writes one persisted setting into `$1`'s profile database while the app is
# stopped — the same row `modules::set_enabled` writes.
source_content_set_setting() {
  local area=$1 key=$2 value=$3
  local database="$CUA_E2E_SCRATCH_ROOT/source-$area/data/reprise/reprise.db"

  if [[ ! -f "$database" ]]; then
    echo "source-$area: expected a profile database at $database" >&2
    return 1
  fi
  sqlite3 "$database" \
    "INSERT INTO settings (key, value) VALUES ('$key', '$value')
     ON CONFLICT(key) DO UPDATE SET value = '$value';"
}
