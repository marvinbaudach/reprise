#!/usr/bin/env bash
# Artist & Album News fixture setup and mapped-window flow for run.sh.
# This file is sourced after the harness globals are initialized.

tag_artist_news_fixture() {
  local path="$1" index="$2"
  if (( index % 2 == 1 )); then
    metaflac --set-tag="ARTIST=Artist Alpha" --set-tag="ALBUM=Local Alpha" "$path"
  else
    metaflac --set-tag="ARTIST=Artist Beta" --set-tag="ALBUM=Local Beta" "$path"
  fi
}

write_artist_news_fixtures() {
  local future_release_date recent_release_date
  future_release_date="$(date -d '+30 days' +%F)"
  recent_release_date="$(date -d '-30 days' +%F)"

  cat > "$MUSICBRAINZ_FIXTURES/artist-Artist%20Alpha.json" <<'EOF'
{"artists":[{"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","name":"Artist Alpha","score":100}]}
EOF
  echo 1500 > "$MUSICBRAINZ_FIXTURES/artist-Artist%20Alpha.delay-ms"
  cat > "$MUSICBRAINZ_FIXTURES/release-groups-aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa.json" <<EOF
{"release-groups":[{"id":"aaaaaaaa-0000-0000-0000-000000000001","title":"Alpha Stale Result","primary-type":"Album","secondary-types":[],"first-release-date":"$future_release_date"}]}
EOF
  cat > "$MUSICBRAINZ_FIXTURES/artist-Artist%20Beta.json" <<'EOF'
{"artists":[{"id":"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb","name":"Artist Beta","score":100}]}
EOF
  cat > "$MUSICBRAINZ_FIXTURES/release-groups-bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb.json" <<EOF
{"release-groups":[
  {"id":"bbbbbbbb-0000-0000-0000-000000000001","title":"Beta Future","primary-type":"Album","secondary-types":[],"first-release-date":"$future_release_date"},
  {"id":"bbbbbbbb-0000-0000-0000-000000000002","title":"Beta Fresh","primary-type":"EP","secondary-types":[],"first-release-date":"$recent_release_date"}
]}
EOF
}

fixture_request_count() {
  if [ -f "$MUSICBRAINZ_LOG" ]; then
    wc -l < "$MUSICBRAINZ_LOG"
  else
    echo 0
  fi
}

wait_for_log_pattern() {
  local pattern="$1" description="$2"
  for _ in $(seq 1 100); do
    if sed -E "$ANSI_STRIP_RE" "$APP_LOG" | grep -qi -- "$pattern"; then
      log_step "log wait OK: $description"
      return 0
    fi
    sleep 0.2
  done
  log_fail "timed out waiting for: $description (pattern: $pattern)"
  return 1
}

assert_fixture_schedule() {
  local count
  count="$(fixture_request_count)"
  if [ "$count" -ne 4 ]; then
    log_fail "Artist News fixture expected 4 MusicBrainz calls, got $count"
    return
  fi
  if ! awk -F '\t' 'NR > 1 && $1 - previous < 1000 { exit 1 } { previous = $1 }' "$MUSICBRAINZ_LOG"; then
    log_fail "MusicBrainz fixture calls were scheduled less than one second apart"
  else
    log_step "fixture check OK: all MusicBrainz calls were at least one second apart"
  fi
  if grep -Eq "$MUSIC_DIR|sine_|\.flac|reprise\.db" "$MUSICBRAINZ_LOG"; then
    log_fail "MusicBrainz fixture log leaked a path, title, or database name"
  elif ! awk -F '\t' '
    $2 == "artist" && $3 ~ /^Artist%20(Alpha|Beta)$/ { next }
    $2 == "release-group" && $3 ~ /^(aaaaaaaa|bbbbbbbb)-/ { next }
    { exit 1 }
  ' "$MUSICBRAINZ_LOG"; then
    log_fail "MusicBrainz fixture log contained fields other than artist names or resolved MBIDs"
  else
    log_step "fixture check OK: requests contain only artist names and resolved MBIDs"
  fi
}

wait_for_painted_window() {
  local path="$PTR_E2E_OUT_DIR/.paint-probe.png"
  local stddev stddev_int
  for _ in $(seq 1 30); do
    scrot -o "$path"
    stddev="$(convert "$path" -format '%[standard-deviation]' info: 2>/dev/null || echo 0)"
    stddev_int="${stddev%%.*}"
    if [ -n "$stddev_int" ] && [ "$stddev_int" -ge 50 ]; then
      rm -f "$path"
      return 0
    fi
    sleep 0.2
  done
  rm -f "$path"
  return 1
}

run_artist_news_flow() {
  local requests_before_reopen
  log_step "flow 0: Artist News opt-in, stale-selection guard, reopen and privacy…"
  screenshot "00-before-artist-news-opt-in"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/00-before-artist-news-opt-in.png"
  if [ "$(fixture_request_count)" -ne 0 ]; then
    log_fail "Artist News made a request before explicit opt-in"
  else
    log_step "fixture check OK: disabled Artist News made zero requests"
  fi

  wait_for_log_pattern "Artist News smoke: plugin enabled" "explicit Artist News opt-in"
  assert_db_value "module.artist_news.enabled" "1" "Artist News opt-in persisted"
  wait_for_log_pattern "Artist News smoke: latest cards ready" "latest Artist Beta cards"
  screenshot "00-artist-news-beta"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/00-artist-news-beta.png"
  assert_log_contains_since 0 "artist news request dispatched.*artist=Artist Alpha" "Artist Alpha request started"
  assert_log_contains_since 0 "artist news request dispatched.*artist=Artist Beta" "selection change requested Artist Beta"
  assert_log_contains_since 0 "artist news response discarded as stale" "delayed Artist Alpha response was discarded"
  assert_log_contains_since 0 "artist news response applied" "latest Artist Beta response was applied"
  assert_fixture_schedule

  requests_before_reopen="$(fixture_request_count)"
  wait_for_log_pattern "Artist News smoke: panel closed" "Information panel close"
  wait_for_log_pattern "Artist News smoke: panel reopened" "Information panel reopen"
  screenshot "00-artist-news-reopened"
  assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/00-artist-news-reopened.png"
  if [ "$(fixture_request_count)" -ne "$requests_before_reopen" ]; then
    log_fail "closing and reopening Information unexpectedly queried MusicBrainz"
  else
    log_step "fixture check OK: close/reopen reused the visible result without a request"
  fi

  wait_for_log_pattern "Artist News smoke: plugin disabled" "Artist News disable and reselection"
  sleep 0.3
  assert_db_value "module.artist_news.enabled" "0" "Artist News disable persisted"
  if [ "$(fixture_request_count)" -ne "$requests_before_reopen" ]; then
    log_fail "disabled Artist News made a provider request after selection changed"
  else
    log_step "fixture check OK: disabled Artist News made zero additional requests"
  fi
  assert_log_absent \
    'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowMutError' \
    'GTK/GLib critical, panic, or RefCell borrow failure'
  log_step "Artist News-only run complete"
}
