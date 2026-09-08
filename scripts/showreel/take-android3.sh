#!/usr/bin/env bash
# The phone takes for the 58.2 s film. Two shots, two takes, and every take is
# walked once unrecorded before it is walked on camera.
#
#   gesture     shot 12, 9.6 s — through the library to an artist, an album,
#                               and the tap that starts it
#   nowplaying  shot 13, 7.2 s — the cover, then the tap that swaps in the
#                               spectrum, with the film's own music playing
#
# Why two takes and not one continuous one. The spectrum has to swing to the
# beat the viewer hears, so the handset must be playing the film bed during shot
# 13 — the 2026-08-29 measurement showed that an error of 0.6 s between bars and
# music is visible, and that the on-screen clock gives no hint of it. The other
# route, seeding the bed as a track of the album shot 12 plays, would put a
# foreign title in that album's track list, which is on screen for 1.8 s. A film
# cut already separates the two shots, so two takes cost nothing.
#
# Why a probe pass, and why it is not optional. Resolving each target from a live
# `uiautomator dump` is what keeps this take off the pinned coordinates that went
# stale on every earlier one — but a dump costs about a second, and shot 12 is
# 9.6 s of one continuous gesture with no cut inside it. Six dumps would be six
# seconds of the shot spent standing still. So:
#
#   probe   walks the flow with no recording, resolves every step, writes the
#           centres to a cache, and prints what the flow costs in seconds
#   take    replays the cached centres with no dumps at all, on camera
#
# The probe is also the only honest way to fail: a label that is not on screen
# stops the run before a take is spent, and prints the screen it did see.
#
# The step list is deliberately data, not code. The app's own navigation is the
# one thing here that no document can be trusted about — `take-android2.sh`
# names a search that reads "Search albums and artists" inside the Artists tab,
# the 2026-08-29 evidence names a bottom bar of Titles / Artists / Queue, and
# neither says where an album grid lives. Run `probe --list` first and put the
# labels it prints into SHOWREEL_STEPS.
#
# The device lock is the caller's, not this script's — the repository's scripts
# do not take it. Acquire it before the first adb call and hold it across the
# whole run, recording included.
#
#   scripts/showreel/take-android3.sh probe --list
#   scripts/showreel/take-android3.sh probe gesture
#   scripts/showreel/take-android3.sh take gesture
set -euo pipefail
# `bc` prints 3.6 and a German locale's printf reads that as an invalid number,
# which aborts the probe after the first step. The numbers here are timings, not
# anything a reader picks a decimal comma for.
export LC_ALL=C
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

ACTION="${1:-probe}"
MODE="${2:-gesture}"
OUT="${3:-$SHOWREEL_DIR/roh-android-$MODE.mp4}"
DUMP="$SHOWREEL_WORK/ui-$MODE.xml"
CACHE="$SHOWREEL_WORK/steps-$MODE.tsv"
timeline="$SHOWREEL_WORK/timeline-android-$MODE.tsv"

ARTIST="${SHOWREEL_ARTIST:-Lorna Shore}"      # must have three to six albums
THEME_TITLE="${SHOWREEL_THEME_TITLE:-Reprise Theme}"

# Each step is  mark <TAB> label <TAB> ui-find options <TAB> dwell.
# The dwell is what the film sees; the probe adds its own resolve time on top and
# reports the difference, because that difference is the whole reason it exists.
#
# Measured against the app on 2026-09-02, not guessed. The bottom bar is
# Titles / Artists / Queue — there is no Albums tab; an artist's albums live on
# the artist page, which is where the film's "albums" moment happens. The
# artist list holds 83 rows and Lorna Shore is deep in it: one 400 ms swipe
# moves seven rows and one 90 ms fling overshoots from B to U, so scrolling to
# it is not steerable inside a 9.6 s shot. The search is, and it shows a real
# feature of the app rather than a scrollbar. The search is per tab — in the
# Artists tab it searches artists — and BACK dismisses the keyboard while
# keeping the results, which is the only reason a shot can follow it.
STEPS_GESTURE=${SHOWREEL_STEPS_GESTURE:-'
artists	Artists	-	1.2
search	Search library	--contains	0.8
type	text:lorna	-	1.0
keyboard	key:BACK	-	0.6
artist	'"$ARTIST"'	--contains	1.6
album	I Feel the Everblack	--contains	1.6
play	Play 	--contains	2.5
'}

# Before a `take nowplaying`: leave the Now Playing square on the COVER. The
# shot is cover-first, and the tap is what brings the spectrum in — the other
# way round it swaps the spectrum away. The choice is stored on the phone and
# survives a restart, but every take flips it, so two takes in a row show
# opposite things unless the square is tapped back between them. Check with one
# screenshot of the open sheet; there is no read-out anywhere else.
#
# The result row is matched by its artist-and-album line, not by the title. The
# title is also the text standing in the search field one row above it, so
# `--contains "Reprise Theme"` resolves to the field: the take taps the search
# box, the keyboard comes back, and every step after that types into it. Seen on
# 2026-09-02, and it fails silently — the probe reports a hit either way.
STEPS_NOWPLAYING=${SHOWREEL_STEPS_NOWPLAYING:-'
titles	Titles	-	1.0
search	Search library	--contains	0.8
type	text:'"${THEME_TITLE// /%s}"'	-	1.0
keyboard	key:BACK	-	0.6
play	Reprise • Reprise	--contains	3.0
open	tap:400,2020	-	3.5
spectrum	tap:540,925	-	12.0
'}

t0=$(date +%s.%N)
now() { echo "$(date +%s.%N) - $t0" | bc; }

mark() {
  printf '%s\t%s\n' "$1" "$(now)" >>"$timeline"
  printf '[%s]\n' "$1" >&2
}

snapshot() {
  adb shell uiautomator dump /sdcard/showreel-ui.xml >/dev/null
  adb shell cat /sdcard/showreel-ui.xml >"$DUMP"
}

# A step whose target carries a prefix is a gesture, not a lookup: there is
# nothing on screen to resolve, so it is cached verbatim and replayed as-is.
#
#   swipe:x1,y1,x2,y2,ms   a scroll or a fling
#   text:lorna             typed into whatever has focus (spaces as %s)
#   key:BACK               a key event — BACK dismisses the keyboard and keeps
#                          the results, which is what a shot wants
#   tap:x,y                a tap where no label can pick the target out: the
#                          mini player carries the same title as the result row
#                          that started it, and the Now Playing square's own
#                          label is not stable across builds
gesture() {
  local kind=${1%%:*} args=${1#*:}
  # The coordinate tuples are deliberately word-split into separate arguments.
  # The directive sits in front of the whole case: shellcheck rejects one placed
  # in front of a single branch, and rejecting it makes the rest of the file
  # unparseable rather than merely unchecked.
  # shellcheck disable=SC2086
  case $kind in
    swipe) adb shell input swipe ${args//,/ } ;;
    tap) adb shell input tap ${args//,/ } ;;
    text) adb shell input text "$args" ;;
    key) adb shell input keyevent "$args" ;;
  esac
}

steps_for() {
  case $1 in
    gesture) printf '%s\n' "$STEPS_GESTURE" ;;
    nowplaying) printf '%s\n' "$STEPS_NOWPLAYING" ;;
    *) printf 'take-android3: unknown mode %s (gesture|nowplaying)\n' "$1" >&2; return 2 ;;
  esac
}

# --- probe -------------------------------------------------------------------
# Walk the flow, resolve each step, tap it, and cache the centre. Nothing is
# recorded, so a failure here costs a minute rather than a take.
probe() {
  : >"$CACHE"
  local walked=0 dwelled=0
  # The step list is read on file descriptor 3, not on stdin. `adb shell` reads
  # stdin and swallows whatever is left of it, so a loop fed on stdin walks its
  # first step and then quietly ends — a take that looks like it ran.
  while IFS=$'\t' read -r -u 3 label target opts dwell; do
    [[ -n ${label:-} ]] || continue
    local t_start
    t_start=$(now)
    # `-` is how a step says "no ui-find options". It has to be written out:
    # IFS=$'\t' collapses runs of tabs, so an empty field between two tabs
    # simply vanishes and every field after it shifts left by one.
    [[ $opts == - ]] && opts=''

    if [[ ${target:-} == swipe:* || ${target:-} == text:* || ${target:-} == key:* || ${target:-} == tap:* ]]; then
      printf '%s\t%s\t%s\n' "$label" "$target" "$dwell" >>"$CACHE"
      gesture "$target"
      printf 'probe: %-10s %-30s dwell %s\n' "$label" "$target" "$dwell" >&2
      dwelled=$(echo "$dwelled + $dwell" | bc)
      sleep "$dwell"
      continue
    fi

    snapshot
    local xy
    # shellcheck disable=SC2086  # opts is deliberately word-split
    if ! xy=$(python3 scripts/showreel/ui-find.py "$DUMP" ${target:+"$target"} $opts); then
      printf 'probe: step %s (%s %s) is not on screen. What is:\n' \
        "$label" "${target:-<any>}" "$opts" >&2
      python3 scripts/showreel/ui-find.py "$DUMP" --list >&2
      return 1
    fi
    printf '%s\t%s\t%s\n' "$label" "$xy" "$dwell" >>"$CACHE"
    # shellcheck disable=SC2086  # ui-find prints "x y", two arguments on purpose
    adb shell input tap $xy
    printf 'probe: %-10s %-8s resolve %.1fs dwell %s\n' \
      "$label" "$xy" "$(echo "$(now) - $t_start" | bc)" "$dwell" >&2
    walked=$(echo "$walked + $(now) - $t_start" | bc)
    dwelled=$(echo "$dwelled + $dwell" | bc)
    sleep "$dwell"
  done 3< <(steps_for "$MODE")

  printf 'probe: resolving costs %.1f s, the shot itself is %.1f s.\n' \
    "$walked" "$dwelled" >&2
  printf 'probe: cached %s — `take` replays it with no dumps.\n' "$CACHE" >&2
}

# Ends the recorder and waits for it to finalize the file.
#
# SIGINT does not reach scrcpy here — it keeps recording, `wait` never returns,
# and the take runs until something outside kills it. Measured on scrcpy 4.1:
# a 19 s shot produced a 119 s file. SIGTERM ends it and the mp4 is finalized;
# the bounded wait is there so a recorder that ignores that too cannot hold the
# session, and SIGKILL is deliberately not sent — it would leave the file
# without its moov atom, which is a lost take rather than a long one.
stop_scrcpy() {
  local pid=$1 waited=0
  kill -TERM "$pid" 2>/dev/null || true
  while kill -0 "$pid" 2>/dev/null && ((waited < 100)); do
    sleep 0.2
    waited=$((waited + 1))
  done
  wait "$pid" 2>/dev/null || true
}

# --- take --------------------------------------------------------------------
# Replay the cached centres. No dumps, so the seconds in the picture are the
# dwells and nothing else. The screen state is the one the probe just proved.
take() {
  [[ -s $CACHE ]] || {
    printf 'take-android3: no cache at %s — run `probe %s` first.\n' "$CACHE" "$MODE" >&2
    return 1
  }
  : >"$timeline"

  scrcpy --no-playback --no-control --max-size 1080 --record "$OUT" \
    >"$SHOWREEL_WORK/scrcpy-$MODE.log" 2>&1 &
  local scrcpy_pid=$!
  # Killing by pattern kills the calling shell here — the pattern matches this
  # script's own command line. Keep the pid.
  trap 'stop_scrcpy "$scrcpy_pid"' EXIT
  sleep 4.0   # the first second of an scrcpy file is junk; let the stream settle

  mark begin
  sleep 1.5
  # Same file descriptor 3 as the probe, and for the same reason: the taps in
  # this loop are `adb shell` calls.
  while IFS=$'\t' read -r -u 3 label xy dwell; do
    [[ -n ${label:-} ]] || continue
    mark "$label"
    if [[ $xy == swipe:* || $xy == text:* || $xy == key:* || $xy == tap:* ]]; then
      gesture "$xy"
    else
      # shellcheck disable=SC2086  # the cached centre is "x y", two arguments
      adb shell input tap $xy
    fi
    sleep "$dwell"
  done 3<"$CACHE"
  mark end
  sleep 2.0

  stop_scrcpy "$scrcpy_pid"
  trap - EXIT

  # A take that walked off its path looks exactly like one that did not, until
  # somebody watches it. Ask the screen where it ended up while the answer is
  # still cheap.
  snapshot
  printf 'take-android3: ended on —\n' >&2
  python3 scripts/showreel/ui-find.py "$DUMP" --list 2>/dev/null | head -12 >&2
  printf 'take-android3: %s (%s s), timeline %s\n' \
    "$OUT" "$(showreel_duration "$OUT")" "$timeline" >&2
}

adb wait-for-device
: >"$timeline"

case $ACTION in
  probe)
    if [[ ${2:-} == --list ]]; then
      snapshot
      python3 scripts/showreel/ui-find.py "$DUMP" --list
      exit 0
    fi
    printf 'take-android3: probing %s (no recording)\n' "$MODE" >&2
    probe
    ;;
  take)
    printf 'take-android3: recording %s -> %s\n' "$MODE" "$OUT" >&2
    take
    ;;
  *)
    printf 'usage: take-android3.sh {probe|take} {gesture|nowplaying} [out.mp4]\n' >&2
    printf '       take-android3.sh probe --list\n' >&2
    exit 2
    ;;
esac
