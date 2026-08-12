#!/usr/bin/env bash
#
# Headless POINTER-level E2E harness for Reprise.
#
# Unlike the existing test suite (which drives the app through signal seams
# such as `RatingWidget::click_star_for_test`'s `emit_clicked`), this script
# injects REAL pointer/keyboard events via `xdotool` into a REAL mapped GTK4
# window running inside a throwaway Xvfb X server, and verifies the result
# through the app's own stderr log plus screenshot pixel data. This is the
# only way to catch bugs where an event never reaches the intended widget in
# the first place (e.g. a `GestureClick` on a non-interactive `Box` losing
# the event to `GtkColumnView`'s row machinery) — a signal-seam test cannot
# see that class of bug because it calls the widget's handler directly,
# skipping event delivery entirely.
#
# See scripts/ptr-e2e/README.md for usage, requirements, and known limits.
#
# ---------------------------------------------------------------------------
# Lessons baked in from earlier failed one-shot attempts at this harness:
#
#   1. "Stale Xvfb on a reused display number gives a blank capture."
#      Fixed by never guessing a display number: Xvfb itself is asked to
#      allocate one via `-displayfd`, which atomically picks the first free
#      number and reports it back to us. Two concurrent runs of this script
#      can never collide.
#
#   2. "Window not found by NAME." The app's WM_CLASS is `io.github.marvinbaudach.Reprise`
#      (the GApplication id), but window managers/toolkits are free to fold
#      that into a shorter class string (observed: `reprise.reprise`) and the
#      title bar text is the human-readable app name, not the id. Matching by
#      NAME is fragile; this script matches by CLASS via `xdotool search
#      --class`, using a substring ("reprise") that is a superset of every
#      variant WM_CLASS is known to take.
#
#   3. "No WM => window stays unmapped, capture is blank." `openbox` is
#      started on the throwaway display before the app, and this script
#      waits for it to be ready before launching Reprise.
#
#   4. "GDK_BACKEND=wayland inherited from the operator's shell silently
#      connects the app to the operator's REAL Wayland session instead of
#      the throwaway Xvfb display" — found live while building this script:
#      a plain `DISPLAY=:N cargo run` on a Wayland-session host does *not*
#      guarantee X11; GDK prefers Wayland if `WAYLAND_DISPLAY`/`GDK_BACKEND`
#      leak through, and will happily paint a real, visible window on the
#      operator's actual desktop — exactly the outcome a headless harness
#      must never risk. This script unsets `WAYLAND_DISPLAY` and forces
#      `GDK_BACKEND=x11` on the app's environment, unconditionally.
#
#   5. The isolated profile: `dbus-run-session` (own session bus — a leaked
#      bus name on the operator's real session bus would hijack their real
#      Reprise launches, since GApplication is single-instance over D-Bus)
#      wrapping a scratch `XDG_DATA_HOME` (database) and `XDG_CONFIG_HOME`
#      with a `gtk-4.0/settings.ini` forcing `gtk-icon-theme-name=Papirus-
#      Dark` — the theme under which "all stars look filled" was originally
#      caught (see `src/ui/track_list/rating.rs`'s module doc comment). Reprise's own
#      rating widget already renders with theme-independent text glyphs
#      (★/☆) specifically because of that trap, so this harness's Papirus
#      run also stands as regression cover for the fix.
#
# ---------------------------------------------------------------------------
set -euo pipefail

# --- Configuration (all overridable via environment) ------------------------

# debug builds faster and (per the task brief) is assumed already built;
# set PTR_E2E_PROFILE=release to exercise the release binary instead.
PTR_E2E_PROFILE="${PTR_E2E_PROFILE:-debug}"
# Resolution of the throwaway X server. Fixed (not "whatever the display
# happens to be") because the star-rating click below targets a hardcoded
# pixel offset derived empirically at this exact resolution — see the
# "Row/column geometry" section below for how that offset was measured and
# what breaks it.
PTR_E2E_SCREEN_RES="${PTR_E2E_SCREEN_RES:-1600x900x24}"
# How many copies of the sine fixture to scan into the library.
PTR_E2E_N_TRACKS="${PTR_E2E_N_TRACKS:-5}"
# Where screenshots and the app log are left behind for a human/controller
# to inspect after the run (NOT cleaned up on success, only stale prior runs
# are cleared at the top of a fresh run).
PTR_E2E_OUT_DIR="${PTR_E2E_OUT_DIR:-/tmp/reprise-ptr-e2e}"
PTR_E2E_NEWS_ONLY="${PTR_E2E_NEWS_ONLY:-0}"
PTR_E2E_HEADER_ONLY="${PTR_E2E_HEADER_ONLY:-0}"
PTR_E2E_PLAYLIST_DELETE_ONLY="${PTR_E2E_PLAYLIST_DELETE_ONLY:-0}"
PTR_E2E_PREFERENCES_ONLY="${PTR_E2E_PREFERENCES_ONLY:-0}"
PTR_E2E_COLREORDER_ONLY="${PTR_E2E_COLREORDER_ONLY:-0}"
# Seeking needs a track long enough that a click lands somewhere other than
# "already finished": this flow replaces the 1.16s sine fixtures with a single
# generated long one, so it runs on its own rather than inside the full sweep.
PTR_E2E_COMPACT_SEEK_ONLY="${PTR_E2E_COMPACT_SEEK_ONLY:-0}"
COMPACT_SEEK_FIXTURE_S="${COMPACT_SEEK_FIXTURE_S:-180}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=artist-news.sh
source "$REPO_ROOT/scripts/ptr-e2e/artist-news.sh"
# shellcheck source=compact-flow.sh
source "$REPO_ROOT/scripts/ptr-e2e/compact-flow.sh"
# shellcheck source=rating.sh
source "$REPO_ROOT/scripts/ptr-e2e/rating.sh"
# shellcheck source=column-header-menu.sh
source "$REPO_ROOT/scripts/ptr-e2e/column-header-menu.sh"
# shellcheck source=playlist-delete.sh
source "$REPO_ROOT/scripts/ptr-e2e/playlist-delete.sh"
# shellcheck source=preferences.sh
source "$REPO_ROOT/scripts/ptr-e2e/preferences.sh"
# shellcheck source=column-reorder.sh
source "$REPO_ROOT/scripts/ptr-e2e/column-reorder.sh"
# shellcheck source=compact-seek.sh
source "$REPO_ROOT/scripts/ptr-e2e/compact-seek.sh"
# shellcheck source=search-chip.sh
source "$REPO_ROOT/scripts/ptr-e2e/search-chip.sh"
FIXTURE_PATH="$REPO_ROOT/crates/reprise-core/tests/fixtures/sine.flac"
APP_ID="io.github.marvinbaudach.Reprise"
# Substring match for `xdotool search --class`: a superset of every WM_CLASS
# variant observed (the app id itself, and toolkit-folded forms such as
# `reprise.reprise`) — see lesson 2 above.
WINDOW_CLASS_MATCH="reprise"

case "$PTR_E2E_PROFILE" in
  debug) BIN_PATH="$REPO_ROOT/target/debug/reprise"; CARGO_PROFILE_FLAG=() ;;
  release) BIN_PATH="$REPO_ROOT/target/release/reprise"; CARGO_PROFILE_FLAG=(--release) ;;
  *) echo "PTR_E2E_PROFILE must be 'debug' or 'release', got: $PTR_E2E_PROFILE" >&2; exit 2 ;;
esac

if [ ! -x "$BIN_PATH" ]; then
  echo "FAIL: $BIN_PATH does not exist — build it first (cargo build${CARGO_PROFILE_FLAG:+ ${CARGO_PROFILE_FLAG[*]}})" >&2
  exit 2
fi

# --- Scratch layout ----------------------------------------------------------

SCRATCH_ROOT="$(mktemp -d /tmp/reprise-ptr-e2e-scratch.XXXXXX)"
MUSIC_DIR="$SCRATCH_ROOT/music"
XDG_DATA_HOME_SCRATCH="$SCRATCH_ROOT/xdg-data"
XDG_CACHE_HOME_SCRATCH="$SCRATCH_ROOT/xdg-cache"
XDG_CONFIG_HOME_SCRATCH="$SCRATCH_ROOT/xdg-config"
MUSICBRAINZ_FIXTURES="$SCRATCH_ROOT/musicbrainz"
MUSICBRAINZ_LOG="$SCRATCH_ROOT/musicbrainz-requests.log"
DISPLAYFD_FILE="$SCRATCH_ROOT/displayfd.txt"
APP_LOG="$SCRATCH_ROOT/app.log"
DBUS_ADDRESS_FILE="$SCRATCH_ROOT/dbus-address.txt"
XVFB_LOG="$SCRATCH_ROOT/xvfb.log"
OPENBOX_LOG="$SCRATCH_ROOT/openbox.log"

rm -rf "$PTR_E2E_OUT_DIR"
mkdir -p \
  "$PTR_E2E_OUT_DIR" \
  "$MUSIC_DIR" \
  "$MUSICBRAINZ_FIXTURES" \
  "$XDG_DATA_HOME_SCRATCH" \
  "$XDG_CACHE_HOME_SCRATCH" \
  "$XDG_CONFIG_HOME_SCRATCH/gtk-4.0"

# `gtk-enable-animations=0` is load-bearing, not cosmetic: with animations on,
# an AdwDialog opens through a spring transition that this environment (debug
# build on llvmpipe) renders so slowly that the screenshot 400 ms after
# `present()` catches the dialog scaled down and at a few percent opacity — it
# reads as "the dialog never painted" while it is merely still arriving. Off,
# every dialog is at its final geometry and opacity the moment it maps.
cat > "$XDG_CONFIG_HOME_SCRATCH/gtk-4.0/settings.ini" <<'EOF'
[Settings]
gtk-icon-theme-name=Papirus-Dark
gtk-enable-animations=0
EOF

if [ "$PTR_E2E_COMPACT_SEEK_ONLY" = "1" ]; then
  # A single, long track: seek assertions need room, and one row keeps the
  # "play row 0" step unambiguous. Generated rather than committed so the
  # repository stays free of a multi-megabyte audio fixture.
  if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "FAIL: PTR_E2E_COMPACT_SEEK_ONLY needs ffmpeg to generate the long fixture" >&2
    exit 2
  fi
  ffmpeg -loglevel error -f lavfi \
    -i "sine=frequency=440:duration=$COMPACT_SEEK_FIXTURE_S" \
    -metadata title="Long Sine" -metadata artist="Pointer Harness" \
    "$MUSIC_DIR/long_sine.flac"
else
  for i in $(seq 1 "$PTR_E2E_N_TRACKS"); do
    # Zero-padded index so filenames (and thus the title the scanner falls
    # back to — `sine.flac` carries no title tag, only a DESCRIPTION comment,
    # so the title is the file stem) sort predictably.
    printf -v idx "%02d" "$i"
    cp "$FIXTURE_PATH" "$MUSIC_DIR/sine_$idx.flac"
    if [ "$PTR_E2E_NEWS_ONLY" = "1" ]; then
      tag_artist_news_fixture "$MUSIC_DIR/sine_$idx.flac" "$i"
    fi
  done
fi

write_artist_news_fixtures

# --- Process bookkeeping / cleanup -------------------------------------------

XVFB_PID=""
OPENBOX_PID=""
APP_LAUNCH_PID=""
FAILURES=0

log_step() { echo "[ptr-e2e] $*"; }
log_fail() { echo "[ptr-e2e] FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }

cleanup() {
  local exit_code=$?
  log_step "cleaning up…"
  # The app was launched via `setsid`, so its PID is also its process group
  # id — killing the negative PGID takes the whole dbus-run-session/cargo/
  # reprise tree with it in one shot, however many layers deep it is.
  if [ -n "$APP_LAUNCH_PID" ]; then
    kill -TERM -- "-$APP_LAUNCH_PID" 2>/dev/null || true
    sleep 0.3
    kill -KILL -- "-$APP_LAUNCH_PID" 2>/dev/null || true
  fi
  if [ -n "$OPENBOX_PID" ]; then
    kill -KILL "$OPENBOX_PID" 2>/dev/null || true
  fi
  if [ -n "$XVFB_PID" ]; then
    kill -KILL "$XVFB_PID" 2>/dev/null || true
  fi
  # Preserved unconditionally (success or failure) so a failed run still
  # leaves the app's stderr log behind for a human/controller to inspect —
  # scratch dir removal below would otherwise take it with it.
  if [ -f "$APP_LOG" ]; then
    cp "$APP_LOG" "$PTR_E2E_OUT_DIR/app.log" 2>/dev/null || true
  fi
  rm -rf "$SCRATCH_ROOT"
  if [ "$exit_code" -eq 0 ] && [ "$FAILURES" -eq 0 ]; then
    log_step "done — all checks passed"
  else
    log_step "done — see failures above (exit $exit_code, $FAILURES failed check(s))"
  fi
  exit $(( exit_code != 0 ? exit_code : (FAILURES > 0 ? 1 : 0) ))
}
trap cleanup EXIT

# --- Xvfb + openbox -----------------------------------------------------------

log_step "starting Xvfb on a freshly-allocated display (${PTR_E2E_SCREEN_RES})…"
: > "$DISPLAYFD_FILE"
# `-displayfd FD`: Xvfb itself finds the first unused display number and
# writes it (as text, newline-terminated) to FD — no probing, no reused-
# display races (lesson 1 above). FD 8 redirected to a real scratch file so
# we can just read it back once Xvfb has written to it.
Xvfb -displayfd 8 -screen 0 "$PTR_E2E_SCREEN_RES" -nolisten tcp \
  8>"$DISPLAYFD_FILE" >"$XVFB_LOG" 2>&1 &
XVFB_PID=$!

for _ in $(seq 1 50); do
  [ -s "$DISPLAYFD_FILE" ] && break
  sleep 0.1
done
DISPLAY_NUM="$(tr -d '[:space:]' < "$DISPLAYFD_FILE")"
if [ -z "$DISPLAY_NUM" ]; then
  echo "FAIL: Xvfb never reported a display number (see $XVFB_LOG)" >&2
  exit 1
fi
export DISPLAY=":$DISPLAY_NUM"
log_step "Xvfb up on $DISPLAY (pid $XVFB_PID)"

log_step "starting openbox (a WM is required or the window never maps)…"
openbox >"$OPENBOX_LOG" 2>&1 &
OPENBOX_PID=$!
sleep 1

# --- Launch the app -----------------------------------------------------------

log_step "launching Reprise ($PTR_E2E_PROFILE profile, $PTR_E2E_N_TRACKS fixture tracks)…"
# `setsid` gives the whole tree below it a fresh process group so cleanup()
# can kill it in one shot regardless of how many processes dbus-run-session/
# cargo interpose. `env -u WAYLAND_DISPLAY GDK_BACKEND=x11` is the fix for
# lesson 4 above — without it, a Wayland-session operator's shell can steer
# the app onto their real desktop instead of this throwaway X server.
setsid env \
  -u WAYLAND_DISPLAY \
  GDK_BACKEND=x11 \
  DISPLAY="$DISPLAY" \
  XDG_DATA_HOME="$XDG_DATA_HOME_SCRATCH" \
  XDG_CACHE_HOME="$XDG_CACHE_HOME_SCRATCH" \
  XDG_CONFIG_HOME="$XDG_CONFIG_HOME_SCRATCH" \
  GTK_A11Y=none \
  NO_AT_BRIDGE=1 \
  REPRISE_SCAN_DIR="$MUSIC_DIR" \
  REPRISE_SMOKE_SEED_PLAYLIST="Pointer Playlist" \
  REPRISE_AUDIO_SINK=fakesink \
  REPRISE_MUSICBRAINZ_FIXTURE_DIR="$MUSICBRAINZ_FIXTURES" \
  REPRISE_MUSICBRAINZ_FIXTURE_LOG="$MUSICBRAINZ_LOG" \
  REPRISE_SMOKE_ARTIST_NEWS="$PTR_E2E_NEWS_ONLY" \
  REPRISE_DBUS_ADDRESS_FILE="$DBUS_ADDRESS_FILE" \
  REPRISE_LOG=debug \
  dbus-run-session -- sh -c \
  'printf "%s" "$DBUS_SESSION_BUS_ADDRESS" > "$REPRISE_DBUS_ADDRESS_FILE"; exec "$@"' sh \
  cargo run --quiet --manifest-path "$REPO_ROOT/Cargo.toml" \
  -p reprise-gnome --features test-fixtures "${CARGO_PROFILE_FLAG[@]}" \
  >"$APP_LOG" 2>&1 &
APP_LAUNCH_PID=$!

# --- Helpers -------------------------------------------------------------

# Polls up to ~15s for a mapped window whose WM_CLASS contains
# $WINDOW_CLASS_MATCH, printing its X window id on success.
find_window() {
  local win=""
  for _ in $(seq 1 30); do
    win="$(xdotool search --class "$WINDOW_CLASS_MATCH" 2>/dev/null | head -1 || true)"
    if [ -n "$win" ]; then
      echo "$win"
      return 0
    fi
    sleep 0.5
  done
  return 1
}

screenshot() {
  local name="$1"
  scrot -o "$PTR_E2E_OUT_DIR/$name.png"
}

click_at() {
  local x="$1" y="$2"
  xdotool mousemove --sync "$x" "$y" sleep 0.1 click 1 >/dev/null 2>&1
}
# On-screen rect of the app window as `X Y WIDTH HEIGHT`.
#
# While the window was maximized at the full 1600x900, a wrong origin made no
# difference — every in-window offset landed on the window either way. The
# moment Compact mode shrinks it to 580x360 it decides everything: a pointer
# event delivered to the root window is where openbox reads a wheel step as
# "switch virtual desktop", which is how one scroll-volume assertion turned
# every later screenshot into a black frame showing openbox's "desktop 3"
# indicator.
window_rect() {
  local geometry
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  printf '%s %s %s %s' \
    "$(sed -n 's/^X=//p' <<<"$geometry")" \
    "$(sed -n 's/^Y=//p' <<<"$geometry")" \
    "$(sed -n 's/^WIDTH=//p' <<<"$geometry")" \
    "$(sed -n 's/^HEIGHT=//p' <<<"$geometry")"
}

# Fails the check rather than the run when an offset falls outside the window:
# a pointer event delivered to the desktop is never what a flow meant to test.
assert_point_in_window() {
  local relative_x="$1" relative_y="$2" description="$3"
  local rect width height
  rect="$(window_rect)"
  width="$(cut -d' ' -f3 <<<"$rect")"
  height="$(cut -d' ' -f4 <<<"$rect")"
  if [ -z "$width" ] || [ -z "$height" ]; then
    log_fail "$description (could not read window geometry)"
    return 1
  fi
  if [ "$relative_x" -lt 0 ] || [ "$relative_x" -ge "$width" ] \
    || [ "$relative_y" -lt 0 ] || [ "$relative_y" -ge "$height" ]; then
    log_fail "$description (offset ${relative_x}x${relative_y} outside ${width}x${height} window)"
    return 1
  fi
  return 0
}

click_window_relative() {
  local relative_x="$1" relative_y="$2" button="${3:-1}"
  local rect window_x window_y
  assert_point_in_window "$relative_x" "$relative_y" "click at ${relative_x}x${relative_y}" || return 0
  rect="$(window_rect)"
  window_x="$(cut -d' ' -f1 <<<"$rect")"
  window_y="$(cut -d' ' -f2 <<<"$rect")"
  xdotool mousemove "$((window_x + relative_x))" "$((window_y + relative_y))" \
    click "$button" >/dev/null 2>&1
}

click_window_from_right() {
  local right_offset="$1" relative_y="$2" button="${3:-1}"
  local width
  width="$(cut -d' ' -f3 <<<"$(window_rect)")"
  click_window_relative "$((width - right_offset))" "$relative_y" "$button"
}

double_click_at() {
  local x="$1" y="$2"
  xdotool mousemove "$x" "$y" click --repeat 2 --delay 80 1 >/dev/null 2>&1
}

type_text() {
  xdotool type -- "$1" >/dev/null 2>&1
}

key() {
  xdotool key "$1" >/dev/null 2>&1
}

assert_window_within() {
  local max_width="$1" max_height="$2" description="$3"
  local geometry width height
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  width="$(sed -n 's/^WIDTH=//p' <<<"$geometry")"
  height="$(sed -n 's/^HEIGHT=//p' <<<"$geometry")"
  if [ -n "$width" ] && [ -n "$height" ] &&
     [ "$width" -le "$max_width" ] && [ "$height" -le "$max_height" ]; then
    log_step "window geometry OK: $description (${width}x${height})"
  else
    log_fail "$description exceeded ${max_width}x${max_height} (got ${width:-?}x${height:-?})"
  fi
}

maximize_window() {
  local geometry current_width=0 current_height=0
  wmctrl -i -r "$WINDOW_ID" -b add,maximized_vert,maximized_horz
  for _ in $(seq 1 30); do
    geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
    current_width="$(sed -n 's/^WIDTH=//p' <<<"$geometry")"
    current_height="$(sed -n 's/^HEIGHT=//p' <<<"$geometry")"
    if [ "${current_width:-0}" -ge 1500 ] && [ "${current_height:-0}" -ge 850 ]; then
      sleep 0.3
      return
    fi
    wmctrl -i -r "$WINDOW_ID" -b add,maximized_vert,maximized_horz
    sleep 0.1
  done
  echo "FAIL: Reprise window did not reach the fixed maximized harness geometry" >&2
  exit 1
}

drag_and_hold() {
  local from_x="$1" from_y="$2" to_x="$3" to_y="$4"
  xdotool mousemove "$from_x" "$from_y" mousedown 1 >/dev/null 2>&1
  sleep 0.2
  xdotool mousemove --sync "$((from_x + 20))" "$((from_y + 5))" >/dev/null 2>&1
  sleep 0.2
  xdotool mousemove --sync "$((to_x - 8))" "$to_y" >/dev/null 2>&1
  sleep 0.2
  xdotool mousemove --sync "$to_x" "$to_y" >/dev/null 2>&1
}

release_drag() {
  xdotool mouseup 1 >/dev/null 2>&1
}

# Non-trivial-image check: a solid/blank capture has a standard deviation
# near zero; a real rendered UI does not. Threshold (50) sits comfortably
# below the ~3600 measured on a real capture at this resolution/theme and
# comfortably above the ~0 a blank/solid capture would produce.
assert_screenshot_not_blank() {
  local path="$1"
  if [ ! -s "$path" ]; then
    log_fail "screenshot missing or empty: $path"
    return
  fi
  local stddev
  stddev="$(convert "$path" -format '%[standard-deviation]' info: 2>/dev/null || echo 0)"
  # Integer-truncate for a portable numeric comparison (bash has no floats).
  local stddev_int="${stddev%%.*}"
  if [ -z "$stddev_int" ] || [ "$stddev_int" -lt 50 ]; then
    log_fail "screenshot looks blank/solid (standard-deviation=$stddev): $path"
  else
    log_step "screenshot OK ($path): standard-deviation=$stddev"
  fi
}

assert_screenshots_differ() {
  local before="$1" after="$2" description="$3"
  local changed
  changed="$(compare -metric AE "$before" "$after" null: 2>&1 || true)"
  changed="${changed%% *}"
  if [ "${changed:-0}" -gt 100 ]; then
    log_step "screenshot difference OK: $description ($changed pixels)"
  else
    log_fail "$description did not visibly change the mapped UI (${changed:-0} pixels)"
  fi
}

# tracing_subscriber's default formatter colors each field's key/`=`/value
# separately with SGR escape codes even when stderr is redirected to a file
# in this setup (observed empirically — `state^[[0m^[[2m=^[[0mPlaying`), so
# a naive `grep "state=Playing"` never matches the raw log. Every log check
# strips ANSI escapes first so patterns can be written in plain,
# human-readable form.
ANSI_STRIP_RE='s/\x1b\[[0-9;]*[a-zA-Z]//g'

# Current line count of the app log — used as a "since" marker so a check
# only looks at NEW log activity produced by the action just taken, not at
# the whole log. This matters for flow 2: "Playing" appears once when
# activation starts playback and again when the second Space resumes it —
# without a marker, a plain "does the log contain state=Playing anywhere"
# check would trivially pass on the first occurrence even if the second
# Space silently did nothing.
log_marker() { wc -l < "$APP_LOG" 2>/dev/null || echo 0; }

assert_log_contains_since() {
  local since_line="$1" pattern="$2" description="$3"
  local plain
  plain="$(tail -n "+$((since_line + 1))" "$APP_LOG" | sed -E "$ANSI_STRIP_RE")"
  if grep -qi -- "$pattern" <<<"$plain"; then
    log_step "log check OK: $description"
    grep -i -- "$pattern" <<<"$plain" | tail -1 | sed 's/^/[ptr-e2e]   -> /'
  else
    log_fail "log never showed: $description (pattern: $pattern)"
  fi
}

assert_log_absent_since() {
  local since_line="$1" pattern="$2" description="$3"
  local plain
  plain="$(tail -n "+$((since_line + 1))" "$APP_LOG" | sed -E "$ANSI_STRIP_RE")"
  if grep -qi -- "$pattern" <<<"$plain"; then
    log_fail "log unexpectedly showed: $description (pattern: $pattern)"
    grep -i -- "$pattern" <<<"$plain" | tail -1 | sed 's/^/[ptr-e2e]   -> /' >&2
  else
    log_step "log check OK: no $description"
  fi
}

assert_log_absent() {
  local pattern="$1" description="$2"
  local plain
  # Ignore the unrelated Nautilus accessibility line in the private session log.
  plain="$(sed -E "$ANSI_STRIP_RE" "$APP_LOG" | grep -v '^[(]org[.]gnome[.]Nautilus:')"
  if grep -Eqi -- "$pattern" <<<"$plain"; then
    log_fail "log unexpectedly showed: $description (pattern: $pattern)"
    grep -Ei -- "$pattern" <<<"$plain" | tail -1 | sed 's/^/[ptr-e2e]   -> /' >&2
  else
    log_step "log check OK: no $description"
  fi
}

assert_db_value() {
  local key_name="$1" expected="$2" description="$3"
  local actual
  actual="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" \
    "SELECT value FROM settings WHERE key = '$key_name';")"
  if [ "$actual" = "$expected" ]; then
    log_step "database check OK: $description"
  else
    log_fail "$description (expected '$expected', got '$actual')"
  fi
}

assert_db_query_true() {
  local query="$1" description="$2"
  local actual
  # A malformed query has to fail this one check, not the run: under `set -e`
  # a non-zero sqlite3 exit aborts the whole suite mid-flow, and the closing
  # balance line then reports one failure for a suite that never ran.
  if ! actual="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" "$query" 2>&1)"; then
    log_fail "$description (query failed: $actual)"
    return
  fi
  if [ "$actual" = "1" ]; then
    log_step "database check OK: $description"
  else
    log_fail "$description (query returned '$actual')"
  fi
}

# Reads one scalar out of the library database. Same reasoning as
# `assert_db_query_true`: a bad query must cost one check, not the run. A bare
# `$(sqlite3 …)` under `set -e` aborts mid-flow and the balance line then
# reports "1 failed check" for flows that never executed — which is exactly how
# a dropped `missing` column hid behind a suite that looked almost green.
#
# Assigns into a caller-named variable rather than printing: `log_fail` inside
# a command substitution would bump `FAILURES` in a subshell, so the failure
# would print but never reach the tally.
db_scalar_into() {
  local target="$1" query="$2" description="$3" out
  printf -v "$target" '%s' ''
  if ! out="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" "$query" 2>&1)"; then
    log_fail "$description (query failed: $out)"
    return 1
  fi
  if [ -z "$out" ]; then
    log_fail "$description (query returned no row)"
    return 1
  fi
  printf -v "$target" '%s' "$out"
}

dismiss_onboarding_banner() {
  local failures_before="$FAILURES"
  log_step "dismissing the online-sources onboarding banner…"
  click_at "$DISCOVERY_BANNER_NOT_NOW_X" "$DISCOVERY_BANNER_NOT_NOW_Y"
  sleep 0.5
  assert_db_value "online_sources.discovery_banner_completed" "1" \
    "online-sources onboarding banner dismissal persisted"
  if [ "$FAILURES" -gt "$failures_before" ]; then
    echo "FAIL: onboarding banner dismissal was not persisted; refusing to run coordinate flows" >&2
    exit 1
  fi
  sleep 0.5
}

# --- Wait for the window, then maximize it -----------------------------------

log_step "waiting for the Reprise window (WM_CLASS matching '$WINDOW_CLASS_MATCH')…"
if ! WINDOW_ID="$(find_window)"; then
  echo "FAIL: no window with WM_CLASS matching '$WINDOW_CLASS_MATCH' appeared within ~15s" >&2
  echo "--- app log tail ---" >&2
  tail -n 40 "$APP_LOG" >&2 || true
  exit 1
fi
log_step "found window $WINDOW_ID"

# Wait for GTK's first mapped frame before asking Openbox to maximize. Sending
# the EWMH state while the surface is still blank races with GTK's initial
# natural-size publication and can leave a 1200x800 window on this 1600x900
# root, invalidating every fixed pointer coordinate below.
if ! wait_for_painted_window; then
  echo "FAIL: mapped Reprise window stayed blank for six seconds" >&2
  exit 1
fi
maximize_window
sleep 1

# --- Row/column geometry (this harness's known limit — see README) ----------
#
# GtkColumnView lays out non-expanding columns at their natural width,
# left-aligned; there is no accessibility bridge wired up in this headless
# session (no a11y bus), so widget geometry cannot be queried — only
# inferred from a screenshot. The pixel offsets below were measured directly
# from a `scrot` capture of this exact scan (5 sine.flac copies, Papirus-Dark,
# 1600x900) and are stable for that fixed input, but WILL need re-measuring
# if the column set, fonts, or resolution change. See README.md.
# shellcheck source=geometry.sh
source "$REPO_ROOT/scripts/ptr-e2e/geometry.sh"
dismiss_onboarding_banner

if [ "$PTR_E2E_NEWS_ONLY" = "1" ]; then
  # --- Flow 0: opt-in Artist News in the contextual information panel -------
  # This dedicated flow uses the app's permanent smoke seam because absolute
  # coordinates are not stable while AdwOverlaySplitView changes between its
  # pinned and overlay geometries. It still drives the production selection,
  # runtime, persistence and panel callbacks; the surrounding harness remains
  # a real mapped GTK window with screenshot verification.
  run_artist_news_flow
  exit 0
fi

if [ "$PTR_E2E_PREFERENCES_ONLY" = "1" ]; then
  run_preferences_flow
  assert_log_absent \
    'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed' \
    'GTK/GLib critical, panic, or RefCell borrow failure'
  exit 0
fi

if [ "$PTR_E2E_COMPACT_SEEK_ONLY" = "1" ]; then
  run_compact_seek_flow
  assert_log_absent \
    'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed' \
    'GTK/GLib critical, panic, or RefCell borrow failure'
  exit 0
fi

if [ "${PTR_E2E_SEARCH_CHIP_ONLY:-0}" = "1" ]; then
  run_search_chip_flow
  assert_log_absent \
    'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed' \
    'GTK/GLib critical, panic, or RefCell borrow failure'
  exit 0
fi

if [ "$PTR_E2E_COLREORDER_ONLY" = "1" ]; then
  run_column_reorder_flow
  assert_log_absent \
    'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed' \
    'GTK/GLib critical, panic, or RefCell borrow failure'
  exit 0
fi

# --- Flow 1: inline rating click reaches a real star button -----------------

run_rating_flow
# --- Flow 1b: right-click headers expose column visibility ------------------

run_column_header_menu_flow
if [ "$PTR_E2E_HEADER_ONLY" = "1" ]; then
  exit 0
fi
run_playlist_delete_flow
if [ "$PTR_E2E_PLAYLIST_DELETE_ONLY" = "1" ]; then exit 0; fi
# --- Flow 2: keyboard opens the selected row's context menu -----------------

log_step "flow 2: Shift+F10 opens the track context menu…"
click_at "$ROW0_TITLE_CELL_X" "$ROW0_TITLE_CELL_Y"
MARKER=$(log_marker)
key "shift+F10"
sleep 0.3
assert_log_contains_since "$MARKER" "track context menu opened from keyboard" "Shift+F10 opened the selected track's context menu"
screenshot "03-keyboard-context-menu"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/03-keyboard-context-menu.png"
MARKER=$(log_marker)
# Since the a11y fix the popover focuses its first item on open, so counting
# starts at "Play next": Add to queue, Add to playlist, Edit tags…. Separators
# are skipped by GTK, submenu rows are not.
key "Down"
key "Down"
key "Down"
key "Return"
sleep 0.4
assert_log_contains_since "$MARKER" "tag editor presented" "keyboard context-menu navigation opened Edit tags"
screenshot "04-keyboard-tag-editor"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/04-keyboard-tag-editor.png"
# Year sits in the left column of the fourth field row. Measured off
# `04-keyboard-tag-editor.png` with animations disabled, so the dialog is at
# its final geometry: the old (800, 466) fell in the gutter between "Album
# artist" and "Genre", left nothing focused, and Return then simply closed the
# dialog instead of exercising the Year field.
db_scalar_into YEAR_BEFORE \
  "SELECT COALESCE(CAST(year AS TEXT), '<null>') FROM tracks WHERE title = 'sine_01' AND missing_since IS NULL;" \
  'invalid-Year check needs the selected track year' || true
db_scalar_into TAG_WRITE_JOBS_BEFORE \
  'SELECT COUNT(*) FROM tag_write_jobs;' \
  'invalid-Year check needs the initial tag-write job count' || true
click_at 664 546
key "ctrl+a"
type_text "0"
MARKER=$(log_marker)
# Return follows TAG-8's field chain; Ctrl+Return then tries the Save shortcut.
# The invalid number never entered the dirty session, so Save remains disabled
# and the save-time validator is not invoked. This is current product behavior,
# not the clearer invalid-input explanation the product still needs.
key "Return"
key "ctrl+Return"
sleep 0.3
assert_log_absent_since "$MARKER" "tag editor rejected an invalid year or track number" \
  "save-time validation while invalid Year left Save disabled"
assert_db_query_true \
  "SELECT COALESCE(CAST(year AS TEXT), '<null>') = '$YEAR_BEFORE' FROM tracks WHERE title = 'sine_01' AND missing_since IS NULL;" \
  "invalid Year left the selected track year unchanged"
assert_db_query_true \
  "SELECT COUNT(*) = $TAG_WRITE_JOBS_BEFORE FROM tag_write_jobs;" \
  "invalid Year left the tag-write job count unchanged"
screenshot "05-invalid-year-dialog-open"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/05-invalid-year-dialog-open.png"
key "Escape"
sleep 0.2
screenshot "05b-library-after-invalid-year-dismiss"
assert_screenshots_differ \
  "$PTR_E2E_OUT_DIR/05-invalid-year-dialog-open.png" \
  "$PTR_E2E_OUT_DIR/05b-library-after-invalid-year-dismiss.png" \
  "invalid Year kept the tag editor open until explicit Escape"

# --- Flow 3: queue reorder exposes and applies a real drop target -----------

log_step "flow 3: Queue insertion target and drag reorder…"
# The 1.16 s fixtures would otherwise consume up_next underneath the
# assertions below. Assert the resulting state, not a state *change*: whether
# anything was playing when we get here depends on the flows above, and an
# already-idle player emits no `StateChanged` event to wait for.
mpris_call Stop
sleep 0.2
PLAYBACK_STATUS="$(mpris_property PlaybackStatus)"
if grep -q 'Stopped' <<<"$PLAYBACK_STATUS"; then
  log_step "MPRIS check OK: playback frozen before Queue mutation"
else
  log_fail "MPRIS Stop left playback running before Queue mutation (got $PLAYBACK_STATUS)"
fi

MARKER=$(log_marker)
key "shift+F10"
key "Down"
key "Return"
sleep 0.2
assert_log_contains_since "$MARKER" "items added to queue.*queue_len=1" "keyboard context menu added the first track to Queue"
click_at "$ROW1_TITLE_CELL_X" "$ROW1_TITLE_CELL_Y"
MARKER=$(log_marker)
key "shift+F10"
key "Down"
key "Return"
sleep 0.2
assert_log_contains_since "$MARKER" "items added to queue.*queue_len=2" "keyboard context menu added the second track to Queue"
assert_log_contains_since "$MARKER" "sidebar refresh.*up next changed" "Queue mutation refreshed the sidebar count"
click_at "$SIDEBAR_QUEUE_X" "$SIDEBAR_QUEUE_Y"
sleep 0.3
screenshot "06-queue-before-reorder"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/06-queue-before-reorder.png"
MARKER=$(log_marker)
# Measured from 06-queue-before-reorder.png: the Queue view carries the same
# column header band as the library plus a "Play Next" section header, so its
# first row sits at y=235 and the second at y=280 — not at the 106/157 of the
# headerless layout these values predate.
QUEUE_ROW0_TITLE_X=355
QUEUE_ROW0_TITLE_Y=235
QUEUE_ROW1_TITLE_X=355
QUEUE_ROW1_TITLE_Y=280
drag_and_hold "$QUEUE_ROW0_TITLE_X" "$QUEUE_ROW0_TITLE_Y" "$QUEUE_ROW1_TITLE_X" "$QUEUE_ROW1_TITLE_Y"
sleep 0.4
assert_log_contains_since "$MARKER" "reorder drop target entered.*source=queue" "held Queue drag entered a reorder target"
screenshot "07-queue-reorder-target"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/07-queue-reorder-target.png"
MARKER=$(log_marker)
release_drag
sleep 0.4
assert_log_contains_since "$MARKER" "queue reordered via drag and drop" "Queue drag release applied the reorder"

# --- Flow 4: manual Up Next interrupts and resumes one playback context -----

log_step "flow 4: context A → manual X → manual Y → context B…"
click_at 80 100
sleep 0.3
# Flow 3 queued sine_01 then sine_02 and dragged the first row below the
# second, so the actual manual order is sine_02 then sine_01. Bind every
# expectation to those fixture titles; database id order is unrelated to the
# queue order this flow is proving.
db_scalar_into TRACK_ID_A \
  "SELECT id FROM tracks WHERE title = 'sine_03' AND missing_since IS NULL;" \
  'flow 4 needs context track sine_03' || true
db_scalar_into TRACK_ID_X \
  "SELECT id FROM tracks WHERE title = 'sine_02' AND missing_since IS NULL;" \
  'flow 4 needs first manual track sine_02' || true
db_scalar_into TRACK_ID_Y \
  "SELECT id FROM tracks WHERE title = 'sine_01' AND missing_since IS NULL;" \
  'flow 4 needs second manual track sine_01' || true
db_scalar_into TRACK_ID_B \
  "SELECT id FROM tracks WHERE title = 'sine_04' AND missing_since IS NULL;" \
  'flow 4 needs next context track sine_04' || true
ROW2_TITLE_Y=$((ROW0_TITLE_CELL_Y + 102))
MARKER=$(log_marker)
double_click_at "$ROW0_TITLE_CELL_X" "$ROW2_TITLE_Y"
sleep 0.2
assert_log_contains_since "$MARKER" \
  "playback started.*track_id=$TRACK_ID_A.*from_up_next=false" \
  "Library activation started context A while two manual tracks stayed pending"

MARKER=$(log_marker)
mpris_call Next
sleep 0.2
assert_log_contains_since "$MARKER" \
  "playback started.*track_id=$TRACK_ID_X.*from_up_next=true" \
  "MPRIS Next consumed reordered manual track X before the context"
assert_log_contains_since "$MARKER" "up next changed.*up_next_len=1" \
  "visible Up Next count changed from two to one"

MARKER=$(log_marker)
mpris_call Next
sleep 0.2
assert_log_contains_since "$MARKER" \
  "playback started.*track_id=$TRACK_ID_Y.*from_up_next=true" \
  "second MPRIS Next consumed manual track Y"
assert_log_contains_since "$MARKER" "up next changed.*up_next_len=0" \
  "visible Up Next count changed from one to zero"

MARKER=$(log_marker)
mpris_call Next
sleep 0.2
assert_log_contains_since "$MARKER" \
  "playback started.*track_id=$TRACK_ID_B.*from_up_next=false" \
  "MPRIS Next resumed the unchanged context at B"

# Keep the original real-keyboard regression after the stronger ordering
# proof. The fixture is short, so both keypresses remain tightly bounded.
log_step "flow 4b: Space toggles play/pause…"

MARKER=$(log_marker)
key "space"
sleep 0.3
assert_log_contains_since "$MARKER" "applying state change.*state=Paused" "Space paused a playing track"

MARKER=$(log_marker)
key "space"
sleep 0.3
assert_log_contains_since "$MARKER" "applying state change.*state=Playing" "Space resumed playback (state change to Playing)"

# --- Flow 5: native Compact layouts, context menu, and scroll volume --------

run_compact_flow
run_preferences_flow

# --- Final screenshot ---------------------------------------------------------

screenshot "24-final"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/24-final.png"
assert_log_absent \
  'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed' \
  'GTK/GLib critical, panic, or RefCell borrow failure'
log_step "final screenshot: $PTR_E2E_OUT_DIR/24-final.png"
log_step "app log will be preserved at: $PTR_E2E_OUT_DIR/app.log (copied by cleanup())"

if [ "$FAILURES" -ne 0 ]; then
  echo "[ptr-e2e] $FAILURES check(s) failed" >&2
fi

# `cleanup` (EXIT trap) computes and returns the real exit code.
