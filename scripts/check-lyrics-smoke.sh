#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

music="$tmp_root/music"
mkdir -p "$music" "$tmp_root/musicbrainz"
source_flac="$repo_root/crates/reprise-core/tests/fixtures/sine.flac"
for title in SmokeFirst SmokeSlow SmokeFast; do
  target="$music/$title.flac"
  cp "$source_flac" "$target"
  metaflac --remove-all-tags \
    --set-tag="TITLE=$title" \
    --set-tag="ARTIST=SmokeArtist" \
    --set-tag="ALBUM=SmokeAlbum" \
    "$target"
done

app_log="$tmp_root/app.log"
request_log="$tmp_root/requests.jsonl"
timeout 15s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  NO_AT_BRIDGE=1 GTK_A11Y=none \
  REPRISE_SCAN_DIR="$music" \
  REPRISE_LRCLIB_FIXTURE_DIR="$repo_root/crates/reprise-core/tests/fixtures/lyrics-smoke" \
  REPRISE_LRCLIB_FIXTURE_LOG="$request_log" \
  REPRISE_MUSICBRAINZ_FIXTURE_DIR="$tmp_root/musicbrainz" \
  REPRISE_SMOKE_LYRICS=1 REPRISE_SMOKE_QUIT=1 \
  REPRISE_SMOKE_QUIT_DELAY_SECS=4 REPRISE_LOG=info \
  cargo run --manifest-path "$repo_root/Cargo.toml" -p reprise-gnome \
  >"$app_log" 2>&1

grep -F 'phase="first-line" line_count=2 active_line=Some(0) latest=true' "$app_log"
grep -F 'phase="second-line" line_count=2 active_line=Some(1) latest=true' "$app_log"
grep -F 'phase="latest-track" line_count=2 active_line=None latest=true' "$app_log"
test "$(wc -l < "$request_log")" -eq 3
grep -F '"title":"SmokeFirst","artist":"SmokeArtist","album":"SmokeAlbum","duration_seconds":1' "$request_log"
grep -F '"title":"SmokeSlow","artist":"SmokeArtist","album":"SmokeAlbum","duration_seconds":1' "$request_log"
grep -F '"title":"SmokeFast","artist":"SmokeArtist","album":"SmokeAlbum","duration_seconds":1' "$request_log"

if grep -Eq 'https?://|/api/|\.flac|reprise\.db' "$request_log"; then
  echo "lyrics fixture log leaked an endpoint or local path" >&2
  exit 1
fi
if grep -Eq 'lyrics smoke state is stale|Gtk-CRITICAL|GLib-CRITICAL|panicked at|already borrowed' "$app_log"; then
  echo "lyrics smoke found stale state or a runtime critical" >&2
  exit 1
fi

echo "synchronized lyrics smoke passed"
