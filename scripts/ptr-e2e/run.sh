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

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
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
XDG_CONFIG_HOME_SCRATCH="$SCRATCH_ROOT/xdg-config"
DISPLAYFD_FILE="$SCRATCH_ROOT/displayfd.txt"
APP_LOG="$SCRATCH_ROOT/app.log"
XVFB_LOG="$SCRATCH_ROOT/xvfb.log"
OPENBOX_LOG="$SCRATCH_ROOT/openbox.log"

rm -rf "$PTR_E2E_OUT_DIR"
mkdir -p "$PTR_E2E_OUT_DIR" "$MUSIC_DIR" "$XDG_DATA_HOME_SCRATCH" "$XDG_CONFIG_HOME_SCRATCH/gtk-4.0"

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
done

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
setsid dbus-run-session -- env \
  -u WAYLAND_DISPLAY \
  GDK_BACKEND=x11 \
  DISPLAY="$DISPLAY" \
  XDG_DATA_HOME="$XDG_DATA_HOME_SCRATCH" \
  XDG_CONFIG_HOME="$XDG_CONFIG_HOME_SCRATCH" \
  GTK_A11Y=none \
  NO_AT_BRIDGE=1 \
  REPRISE_SCAN_DIR="$MUSIC_DIR" \
  REPRISE_AUDIO_SINK=fakesink \
  REPRISE_LOG=debug \
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
  xdotool mousemove "$x" "$y" click 1 >/dev/null 2>&1
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
  plain="$(sed -E "$ANSI_STRIP_RE" "$APP_LOG")"
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

maximize_window

# --- Row/column geometry (this harness's known limit — see README) ----------
#
# GtkColumnView lays out non-expanding columns at their natural width,
# left-aligned; there is no accessibility bridge wired up in this headless
# session (no a11y bus), so widget geometry cannot be queried — only
# inferred from a screenshot. The pixel offsets below were measured directly
# from a `scrot` capture of this exact scan (5 sine.flac copies, Papirus-Dark,
# 1600x900) and are stable for that fixed input, but WILL need re-measuring
# if the column set, fonts, or resolution change. See README.md.
ROW0_TITLE_CELL_X=355
ROW0_TITLE_CELL_Y=165
ROW0_RATING_STAR1_X=703
ROW0_RATING_STAR1_Y=165

# --- Flow 1: star-rating click reaches the real widget -----------------------

log_step "flow 1: star-rating click…"
screenshot "01-initial-track-list"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/01-initial-track-list.png"

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
assert_log_contains_since "$MARKER" "context menu: tracks added to queue" "keyboard context menu added the first track to Queue"
click_at 355 215
MARKER=$(log_marker)
key "shift+F10"
key "Down"
key "Return"
sleep 0.2
assert_log_contains_since "$MARKER" "context menu: tracks added to queue" "keyboard context menu added the second track to Queue"
assert_log_contains_since "$MARKER" "sidebar refresh.*queue changed" "Queue mutation refreshed the sidebar count"
click_at 80 104
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

# --- Flow 4: Space toggles play/pause when the track list has focus ---------

log_step "flow 4: Space toggles play/pause…"
# Double-click a *different* row (row 1's Title cell) to both focus the
# track list (search entry must NOT have focus, or Space would type a literal
# space instead — see src/ui/shortcuts.rs's `space_should_toggle`) and start
# real playback, so the player has a state to toggle away from. Using row 1
# rather than row 0 keeps this flow's log lines distinguishable from flow 1's
# star click above.
#
# Timing here is deliberately tight (0.3s, not the leisurely 1s used
# elsewhere): the core fixture track is ~1.16s long,
# and once it reaches end-of-stream the player auto-advances to the next
# queued track — which would race with "Space paused a playing track" below
# and turn a real bug into flaky noise. Every action after this point (both
# Space presses) needs to land while the *same* activation is still playing.
MARKER=$(log_marker)
double_click_at "$QUEUE_ROW1_TITLE_X" "$QUEUE_ROW1_TITLE_Y"
sleep 0.3
assert_log_contains_since "$MARKER" "applying state change.*state=Playing" "activation started real playback (src/ui/player_controller.rs)"

MARKER=$(log_marker)
key "space"
sleep 0.3
assert_log_contains_since "$MARKER" "applying state change.*state=Paused" "Space paused a playing track"

MARKER=$(log_marker)
key "space"
sleep 0.3
assert_log_contains_since "$MARKER" "applying state change.*state=Playing" "Space resumed playback (state change to Playing)"

# --- Flow 5: visible Compact button, all layouts, menus, and shortcut --------

log_step "flow 5: visible Compact button and all four layouts…"
# Header coordinates are fixed for this harness's 1600x900 maximized
# geometry, like the row/rating coordinates documented above. The compact
# button is the view-grid icon directly to the right of the primary menu.
HEADER_BUTTON_Y=28
MARKER=$(log_marker)
click_window_from_right 437 "$HEADER_BUTTON_Y"
sleep 0.4
assert_log_contains_since "$MARKER" "window view mode changed.*mode=Compact.*layout=Bar" "full-header button entered Compact Bar"
assert_window_within 660 185 "Bar compact geometry after leaving maximized Library"
screenshot "08-compact-bar"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/08-compact-bar.png"

# Every compact header keeps the shared menu at a stable right-side offset.
# Opening it with a real pointer proves the visible entry point; keyboard
# navigation then selects each radio target in sequence.
click_window_from_right 145 28
sleep 0.3
screenshot "09-compact-visible-menu"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/09-compact-visible-menu.png"

MARKER=$(log_marker)
click_window_from_right 145 100
sleep 0.2
click_window_from_right 145 124
sleep 0.4
assert_log_contains_since "$MARKER" "compact layout changed.*layout=Cover" "visible menu selected Cover"
assert_window_within 420 560 "Cover compact geometry"
screenshot "10-compact-cover"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/10-compact-cover.png"

click_window_from_right 145 28
MARKER=$(log_marker)
sleep 0.2
click_window_from_right 145 100
sleep 0.2
click_window_from_right 145 156
sleep 0.4
assert_log_contains_since "$MARKER" "compact layout changed.*layout=Pill" "visible menu selected Pill"
assert_window_within 680 125 "Pill compact geometry"
screenshot "11-compact-pill"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/11-compact-pill.png"

click_window_from_right 145 40
MARKER=$(log_marker)
sleep 0.3
screenshot "11b-compact-pill-visible-menu"
assert_screenshots_differ \
  "$PTR_E2E_OUT_DIR/11-compact-pill.png" \
  "$PTR_E2E_OUT_DIR/11b-compact-pill-visible-menu.png" \
  "Pill visible button opened the compact menu"
click_window_from_right 151 124
sleep 0.2
click_window_from_right 151 219
sleep 0.4
assert_log_contains_since "$MARKER" "compact layout changed.*layout=Card" "visible menu selected Card"
assert_window_within 500 300 "Card compact geometry"
screenshot "12-compact-card"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/12-compact-card.png"

# Free-surface right click and Shift+F10 must display the same shared menu.
click_window_relative 20 200 3
sleep 0.3
screenshot "13-compact-right-click-menu"
assert_screenshots_differ \
  "$PTR_E2E_OUT_DIR/12-compact-card.png" \
  "$PTR_E2E_OUT_DIR/13-compact-right-click-menu.png" \
  "right click opened the compact menu"
key "Escape"
sleep 0.2
click_window_from_right 145 28
sleep 0.2
key "Escape"
screenshot "13b-compact-card-menu-closed"
key "shift+F10"
sleep 0.3
screenshot "14-compact-keyboard-menu"
assert_screenshots_differ \
  "$PTR_E2E_OUT_DIR/13b-compact-card-menu-closed.png" \
  "$PTR_E2E_OUT_DIR/14-compact-keyboard-menu.png" \
  "Shift+F10 opened the compact menu"
key "Escape"

# The direct restore button returns to Library; Ctrl+M repeats the round trip
# and must retain Card as the selected compact layout.
MARKER=$(log_marker)
click_window_from_right 105 28
sleep 0.5
assert_log_contains_since "$MARKER" "window view mode changed.*mode=Library.*layout=Card" "visible restore button returned to Library"
screenshot "15-library-restored"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/15-library-restored.png"

MARKER=$(log_marker)
key "ctrl+m"
sleep 0.4
assert_log_contains_since "$MARKER" "window view mode changed.*mode=Compact.*layout=Card" "Ctrl+M restored Compact Card"
screenshot "16-compact-card-shortcut"
assert_screenshot_not_blank "$PTR_E2E_OUT_DIR/16-compact-card-shortcut.png"
MARKER=$(log_marker)
key "ctrl+m"
sleep 0.5
assert_log_contains_since "$MARKER" "window view mode changed.*mode=Library.*layout=Card" "Ctrl+M restored Library View"

# --- Flow 6: real Preferences menu item --------------------------------------

log_step "flow 6: Preferences dialog…"
maximize_window
click_window_from_right 477 28
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
