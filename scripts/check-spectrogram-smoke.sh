#!/usr/bin/env bash
set -euo pipefail

# NAV-15: a freshly scanned library analyzes itself, with no user action at all.
# Nothing here asks for the analysis — the app is started, and the spectrograms
# must simply be there afterwards. The unit tests cover the state machine with a
# fake run; this covers what they cannot: the autostart, a real GStreamer
# decode, and the rows in the database.

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

music="$tmp_root/music"
mkdir -p "$music"
source_flac="$repo_root/crates/reprise-core/tests/fixtures/sine.flac"
for title in SpectroOne SpectroTwo SpectroThree; do
  target="$music/$title.flac"
  cp "$source_flac" "$target"
  metaflac --remove-all-tags \
    --set-tag="TITLE=$title" \
    --set-tag="ARTIST=SpectroArtist" \
    --set-tag="ALBUM=SpectroAlbum" \
    "$target"
done

app_log="$tmp_root/app.log"
timeout 90s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  NO_AT_BRIDGE=1 GTK_A11Y=none \
  REPRISE_SCAN_DIR="$music" \
  REPRISE_SMOKE_QUIT=1 \
  REPRISE_SMOKE_QUIT_DELAY_SECS=12 REPRISE_LOG=info \
  cargo run --manifest-path "$repo_root/Cargo.toml" -p reprise-gnome \
  >"$app_log" 2>&1

db="$tmp_root/data/reprise/reprise.db"
test -f "$db" || { echo "smoke run produced no database" >&2; exit 1; }

# tracing colourises field separators, so assertions run on a stripped copy.
plain_log="$tmp_root/app.plain.log"
sed 's/\x1b\[[0-9;]*m//g' "$app_log" >"$plain_log"

grep -F 'library analysis finished' "$plain_log" >/dev/null \
  || { echo "the analysis never reported a finish" >&2; tail -40 "$plain_log" >&2; exit 1; }
grep -E 'library analysis finished.*state=Complete.*analyzed=3.*failed=0' "$plain_log" >/dev/null \
  || { echo "the analysis did not complete over all three tracks" >&2; tail -40 "$plain_log" >&2; exit 1; }

stored=$(sqlite3 "file:$db?mode=ro" "SELECT count(*) FROM track_spectrograms;")
test "$stored" -eq 3 \
  || { echo "expected 3 stored spectrograms, found $stored" >&2; exit 1; }

# The point of the whole chain: a stored spectrogram is non-empty, so the seek
# bar has something to colour with.
bytes=$(sqlite3 "file:$db?mode=ro" "SELECT min(length(data)) FROM track_spectrograms;")
test "$bytes" -gt 0 \
  || { echo "stored spectrograms are empty" >&2; exit 1; }

if grep -Eq 'Gtk-CRITICAL|GLib-CRITICAL|panicked at|already borrowed' "$plain_log"; then
  echo "spectrogram smoke found a runtime critical" >&2
  exit 1
fi

echo "spectrogram analysis smoke passed ($stored tracks, min $bytes bytes)"
