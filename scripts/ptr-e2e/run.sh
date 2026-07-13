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
#   2. "Window not found by NAME." The app's WM_CLASS is `org.reprise.Reprise`
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
#      caught (see `src/ui/rating.rs`'s module doc comment). Reprise's own
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

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=artist-news.sh
source "$REPO_ROOT/scripts/ptr-e2e/artist-news.sh"
# shellcheck source=compact-flow.sh
source "$REPO_ROOT/scripts/ptr-e2e/compact-flow.sh"
FIXTURE_PATH="$REPO_ROOT/crates/reprise-core/tests/fixtures/sine.flac"
APP_ID="org.reprise.Reprise"
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

cat > "$XDG_CONFIG_HOME_SCRATCH/gtk-4.0/settings.ini" <<'EOF'
[Settings]
gtk-icon-theme-name=Papirus-Dark
EOF

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
  REPRISE_AUDIO_SINK=fakesink \
  REPRISE_MUSICBRAINZ_FIXTURE_DIR="$MUSICBRAINZ_FIXTURES" \
  REPRISE_MUSICBRAINZ_FIXTURE_LOG="$MUSICBRAINZ_LOG" \
  REPRISE_SMOKE_ARTIST_NEWS="$PTR_E2E_NEWS_ONLY" \
  REPRISE_DBUS_ADDRESS_FILE="$DBUS_ADDRESS_FILE" \
  REPRISE_LOG=debug \
  dbus-run-session -- sh -c \
  'printf "%s" "$DBUS_SESSION_BUS_ADDRESS" > "$REPRISE_DBUS_ADDRESS_FILE"; exec "$@"' sh \
  cargo run --quiet --manifest-path "$REPO_ROOT/Cargo.toml" "${CARGO_PROFILE_FLAG[@]}" \
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
click_window_relative() {
  local relative_x="$1" relative_y="$2" button="${3:-1}"
  local geometry window_x window_y
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  window_x="$(sed -n 's/^X=//p' <<<"$geometry")"
  window_y="$(sed -n 's/^Y=//p' <<<"$geometry")"
  xdotool mousemove "$((window_x + relative_x))" "$((window_y + relative_y))" \
    click "$button" >/dev/null 2>&1
}

click_window_from_right() {
  local right_offset="$1" relative_y="$2" button="${3:-1}"
  local geometry width
  geometry="$(xdotool getwindowgeometry --shell "$WINDOW_ID" 2>/dev/null)"
  width="$(sed -n 's/^WIDTH=//p' <<<"$geometry")"
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
  actual="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" "$query")"
  if [ "$actual" = "1" ]; then
    log_step "database check OK: $description"
  else
    log_fail "$description (query returned '$actual')"
  fi
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

# --- Flow 1: star-rating click reaches the real widget -----------------------

log_step "flow 1: star-rating click…"
screenshot "01-initial-track-list"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/01-initial-track-list.png"
# Close the default-visible Information overlay before exercising row input.
click_window_from_right "$INFO_TOGGLE_FROM_RIGHT" 28
sleep 1
click_at "$ROW0_TITLE_CELL_X" "$ROW0_TITLE_CELL_Y"
sleep 0.3
MARKER=$(log_marker)
click_at "$ROW0_RATING_STAR1_X" "$ROW0_RATING_STAR1_Y"
sleep 1
screenshot "02-after-star-click"
# `RatingWidget`'s click handler logs via `tracing::debug!(... "rating
# changed")` in src/ui/track_list.rs — this is the exact line a signal-seam
# test (calling `click_star_for_test`/`emit_clicked` directly) cannot prove:
# it only exists if the real pointer click was actually delivered to the
# button inside the ColumnView cell.
assert_log_contains_since "$MARKER" "rating changed" "star click delivered a rating change (src/ui/track_list.rs on_rating_changed)"
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
key "Down"
key "Down"
key "Return"
sleep 0.4
assert_log_contains_since "$MARKER" "tag editor presented" "keyboard context-menu navigation opened Edit tags"
screenshot "04-keyboard-tag-editor"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/04-keyboard-tag-editor.png"
click_at 800 466
key "ctrl+a"
type_text "0"
MARKER=$(log_marker)
key "Return"
sleep 0.3
assert_log_contains_since "$MARKER" "tag editor rejected an invalid year or track number" "invalid Year plus Enter was rejected without applying"
screenshot "05-invalid-year-rejected"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/05-invalid-year-rejected.png"
key "Escape"
sleep 0.2

# --- Flow 3: queue reorder exposes and applies a real drop target -----------

log_step "flow 3: Queue insertion target and drag reorder…"
MARKER=$(log_marker)
key "shift+F10"
key "Down"
key "Return"
sleep 0.2
assert_log_contains_since "$MARKER" "tracks added to queue.*queue_len=1" "keyboard context menu added the first track to Queue"
click_at "$ROW1_TITLE_CELL_X" "$ROW1_TITLE_CELL_Y"
MARKER=$(log_marker)
key "shift+F10"
key "Down"
key "Return"
sleep 0.2
assert_log_contains_since "$MARKER" "tracks added to queue.*queue_len=2" "keyboard context menu added the second track to Queue"
assert_log_contains_since "$MARKER" "sidebar refresh.*up next changed" "Queue mutation refreshed the sidebar count"
click_at "$SIDEBAR_QUEUE_X" "$SIDEBAR_QUEUE_Y"
sleep 0.3
screenshot "06-queue-before-reorder"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/06-queue-before-reorder.png"
MARKER=$(log_marker)
QUEUE_ROW0_TITLE_X=355
QUEUE_ROW0_TITLE_Y=106
QUEUE_ROW1_TITLE_X=355
QUEUE_ROW1_TITLE_Y=157
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
TRACK_ID_A="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" \
  'SELECT id FROM tracks WHERE missing = 0 ORDER BY id ASC LIMIT 1 OFFSET 2;')"
TRACK_ID_X="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" \
  'SELECT id FROM tracks WHERE missing = 0 ORDER BY id ASC LIMIT 1 OFFSET 1;')"
TRACK_ID_Y="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" \
  'SELECT id FROM tracks WHERE missing = 0 ORDER BY id ASC LIMIT 1 OFFSET 0;')"
TRACK_ID_B="$(sqlite3 "$XDG_DATA_HOME_SCRATCH/reprise/reprise.db" \
  'SELECT id FROM tracks WHERE missing = 0 ORDER BY id ASC LIMIT 1 OFFSET 3;')"
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
# --- Flow 6: real Preferences menu item --------------------------------------

log_step "flow 6: Preferences dialog…"
maximize_window
click_window_from_right "$PRIMARY_MENU_FROM_RIGHT" 28
sleep 0.3
screenshot "17-main-menu"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/17-main-menu.png"

# Preferences is the second row in the primary menu.
MARKER=$(log_marker)
key "Home"
key "Down"
key "Return"
sleep 1.5
assert_log_contains_since "$MARKER" "preferences dialog presented" "primary-menu click opened Preferences"
screenshot "18-preferences"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/18-preferences.png"

click_at 702 823
sleep 0.3
screenshot "19-preferences-layout"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/19-preferences-layout.png"
click_at 1034 209
sleep 0.2
assert_db_value "ui.sidebar_visible" "0" "Layout switch hid the sidebar"
click_at 1034 264
sleep 0.2
assert_db_value "ui.status_visible" "0" "Layout switch hid the status line"
click_at 804 823
sleep 0.7
screenshot "20-preferences-library"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/20-preferences-library.png"
MARKER=$(log_marker)
click_at 800 209
sleep 1.5
assert_log_contains_since "$MARKER" "scan complete" "Library Preferences triggered a completed rescan"
click_at 907 823
sleep 0.3
screenshot "21-preferences-plugins"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/21-preferences-plugins.png"
click_at 1011 823
sleep 0.3
screenshot "22-preferences-playback"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/22-preferences-playback.png"

# Drive the real Playback controls, then prove the removed duplicate plugin
# rows cannot mutate either core playback setting.
click_at 1034 194
sleep 0.4
assert_db_value "playback.equalizer_enabled" "1" "Playback switch enabled the equalizer"
click_at 1000 323
sleep 0.3
assert_db_query_true \
  "SELECT value <> '0,0,0,0,0,0,0,0,0,0' FROM settings WHERE key = 'playback.equalizer_bands';" \
  "real scale click persisted a non-flat equalizer curve"

click_at 907 823
sleep 0.7
click_at 1034 290
sleep 0.4
assert_db_value "playback.equalizer_enabled" "1" "Plugins has no duplicate Equalizer switch"
click_at 1034 345
sleep 0.4
assert_db_query_true \
  "SELECT COUNT(*) = 0 FROM settings WHERE key = 'playback.replay_gain_mode';" \
  "Plugins has no duplicate ReplayGain switch"

click_at 1011 823
sleep 0.7
screenshot "23-preferences-synchronized"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/23-preferences-synchronized.png"

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
