#!/usr/bin/env bash
# Headless capture of the Updates popover, with a contrast measurement.
#
# Usage:
#   cargo build -p reprise-gnome
#   scripts/shoot-updates-popover.sh
#   scripts/measure-contrast.py "$OUT/01-updates-popover.png" --regions regions.tsv
#
# Env: REPRISE_BIN, REPRISE_SHOT_DIR, UPDATES_CLICK_X / UPDATES_CLICK_Y.
#
# Known limit: the ✦ trigger is clicked by offset from the window's right
# edge, because it carries no accessible name that xdotool can search for.
# Change the header bar and this offset needs re-measuring from the first
# capture, which the script leaves behind as `00-headerbar.png` for exactly
# that reason. It warns instead of passing silently when the popover does not
# open.
#
# It only appears once the New Releases module is switched on and has rows, so
# the app is started once to create its schema, stopped, seeded directly in
# SQLite, then restarted. Everything lives in a throwaway XDG root with its own
# D-Bus, so the real library is never touched and no running instance is
# hijacked.
set -euo pipefail

WORKTREE="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${REPRISE_BIN:-$WORKTREE/target/debug/reprise}"
OUT_DIR="${REPRISE_SHOT_DIR:-/tmp/reprise-updates-shots}"
mkdir -p "$OUT_DIR"
[ -x "$BIN" ] || { echo "missing binary: $BIN"; exit 2; }

SCRATCH="$(mktemp -d /tmp/reprise-updates.XXXXXX)"
cleanup() {
  [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null || true
  [ -n "${OPENBOX_PID:-}" ] && kill "$OPENBOX_PID" 2>/dev/null || true
  [ -n "${XVFB_PID:-}" ] && kill "$XVFB_PID" 2>/dev/null || true
  rm -rf "$SCRATCH"
}
trap cleanup EXIT

exec {display_fd}<> <(:)
Xvfb -displayfd "$display_fd" -screen 0 1600x900x24 -nolisten tcp >"$SCRATCH/xvfb.log" 2>&1 &
XVFB_PID=$!
read -r -u "$display_fd" DISPLAY_NUM
export DISPLAY=":$DISPLAY_NUM"

unset WAYLAND_DISPLAY
export GDK_BACKEND=x11
export XDG_DATA_HOME="$SCRATCH/data" XDG_CACHE_HOME="$SCRATCH/cache" XDG_CONFIG_HOME="$SCRATCH/config"
mkdir -p "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME"
DB="$XDG_DATA_HOME/reprise/reprise.db"

openbox >"$SCRATCH/openbox.log" 2>&1 &
OPENBOX_PID=$!
sleep 1

start_app() {
  dbus-run-session -- "$BIN" >"$SCRATCH/app-$1.log" 2>&1 &
  APP_PID=$!
  for _ in $(seq 1 60); do
    xdotool search --class reprise >/dev/null 2>&1 && return 0
    sleep 0.5
  done
  echo "[updates] window never appeared"; tail -20 "$SCRATCH/app-$1.log"; return 1
}

stop_app() {
  kill "$APP_PID" 2>/dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.5
  done
  # A survivor leaves its window mapped, and the second run then paints on top
  # of it — two overlapping frames in the capture, and clicks land on whichever
  # xdotool happened to list last.
  kill -9 "$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
  APP_PID=""
  for _ in $(seq 1 20); do
    xdotool search --class reprise >/dev/null 2>&1 || break
    sleep 0.5
  done
  sleep 1
}

echo "[updates] first run: let the app create its schema"
start_app first
for _ in $(seq 1 40); do [ -f "$DB" ] && break; sleep 0.5; done
[ -f "$DB" ] || { echo "[updates] no database at $DB"; exit 1; }
sleep 3
stop_app

echo "[updates] seeding the module switch and a few releases"
now=$(date +%s)
# The columns are read off the live table rather than copied from the source:
# `fallback_accent` was dropped in a later migration, and a hardcoded column
# list breaks silently on the next one.
echo "[updates] columns: $(sqlite3 "$DB" "SELECT group_concat(name, ', ') FROM pragma_table_info('new_releases');")"

sqlite3 "$DB" "INSERT OR REPLACE INTO settings(key, value) VALUES ('module.new_releases.enabled', '1');"
sqlite3 "$DB" "DELETE FROM new_releases;"

seed_release() { # mbid artist title type date
  sqlite3 "$DB" "INSERT INTO new_releases
    (release_group_mbid, artist_name, artist_mbid, title, release_type,
     first_release_date, fetched_at, seen_at, hidden)
   VALUES ('$1', '$2', 'mbid-$1', '$3', '$4', '$5', $now, NULL, 0);"
}
seed_release aaaa0001 'Death Do Us Part' 'Hope Arisen From Fallen Dreams' Album "$(date -d '-4 days' +%F)"
seed_release aaaa0002 'Dal Av' 'Glass Palace' EP "$(date -d '-30 days' +%F)"
seed_release aaaa0003 'What Lies Below' 'Leech' EP "$(date -d '-60 days' +%F)"
seed_release aaaa0004 'Gone Cold' 'Out of Time' EP "$(date -d '-90 days' +%F)"
seed_release aaaa0005 'I Am Mook' 'Hollow Allegory' EP "$(date -d '+20 days' +%F)"
echo "[updates] rows: $(sqlite3 "$DB" 'SELECT COUNT(*) FROM new_releases;')"

echo "[updates] second run"
start_app second
WINDOW=$(xdotool search --class reprise | tail -1)
xdotool windowactivate "$WINDOW" 2>/dev/null || true
sleep 2
# Escape rather than a coordinate: the first-run dialog is centred in the
# window, so any pixel guess has to track the window origin.
xdotool key --window "$WINDOW" Escape; sleep 1
xdotool key Escape; sleep 2

eval "$(xdotool getwindowgeometry --shell "$WINDOW")"
echo "[updates] window ${WIDTH}x${HEIGHT} at ${X},${Y}"
scrot -o "$OUT_DIR/00-headerbar.png"

# The ✦ trigger sits 233px from the window's right edge — measured off the
# first capture, and only present because the module is switched on. Enabling
# it adds a button, so every offset shifts from the module-off layout.
CLICK_X=${UPDATES_CLICK_X:-$((X + WIDTH - 233))}
CLICK_Y=${UPDATES_CLICK_Y:-$((Y + 32))}
xdotool mousemove "$CLICK_X" "$CLICK_Y"; sleep 0.4
xdotool click 1; sleep 2
scrot -o "$OUT_DIR/01-updates-popover.png"
echo "[updates] clicked ${CLICK_X},${CLICK_Y}"
if cmp -s "$OUT_DIR/00-headerbar.png" "$OUT_DIR/01-updates-popover.png"; then
  echo "[updates] WARNING: capture unchanged — the popover did not open"
fi

cp "$SCRATCH/app-second.log" "$OUT_DIR/app.log" 2>/dev/null || true
