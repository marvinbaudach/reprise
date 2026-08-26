#!/usr/bin/env bash
# Measure visible Android spectrum updates without confusing them with render rate.

set -euo pipefail

readonly PACKAGE_NAME="${REPRISE_ANDROID_PACKAGE:-io.github.marvinbaudach.reprise}"
readonly DEFAULT_CROP="660:230:210:880"
readonly REMOTE_VIDEO="/sdcard/reprise-visualizer-data-rate.mp4"

stayon_changed=0
original_stayon=''

fail() {
  printf 'android-visualizer-data-rate: %s\n' "$*" >&2
  exit 1
}

session_field() {
  local field=$1
  local input_file=$2
  python3 - "$field" "$input_file" <<'PY'
import pathlib
import re
import sys

field, path = sys.argv[1:]
text = pathlib.Path(path).read_text(encoding="utf-8", errors="replace")
patterns = {
    "state": (
        r"^\s*state=PlaybackState\s*\{state="
        r"(?:(?P<symbol>[A-Z][A-Z0-9_]*)\()?"
        r"(?P<value>\d+)(?(symbol)\))(?=,|\s|\}|$)"
    ),
    # dumpsys prints the description on the metadata line, not on one of
    # its own: "metadata: size=9, description=Title, Artist, Album". An
    # anchor at the start of the line therefore never matches on a real
    # device -- the underscore in the character class is deliberate
    # protection against matching a longer key that merely ends in
    # "description".
    "track": r"(?:^|[,\s])description=(?P<value>.+)$",
}
match = re.search(patterns[field], text, re.MULTILINE)
if match is None:
    raise SystemExit(f"missing {field} in {path}")
print(match.group("value").strip())
PY
}

self_test() {
  local actual

  actual=$(session_field state <(printf '%s\n' \
    'state=PlaybackState {state=3, position=20800, buffered position=53060}')) ||
    fail "the numeric playback-state fixture did not parse"
  [[ $actual == 3 ]] || fail "the numeric playback-state fixture yielded $actual, not 3"

  actual=$(session_field state <(printf '%s\n' \
    'state=PlaybackState {state=PLAYING(3), position=20800, buffered position=53060}')) ||
    fail "the symbolic playback-state fixture did not parse"
  [[ $actual == 3 ]] || fail "the symbolic playback-state fixture yielded $actual, not 3"

  if session_field state <(printf '%s\n' \
    'other_state=PlaybackState {state=PLAYING(3), position=20800}') >/dev/null 2>&1; then
    fail "a fixture without PlaybackState unexpectedly parsed"
  fi


  actual=$(session_field track <(printf '%s\n' \
    '      metadata: size=9, description=Disease, Reversionists, Disease - Single')) ||
    fail "the dumpsys metadata-line fixture did not parse"
  [[ $actual == 'Disease, Reversionists, Disease - Single' ]] ||
    fail "the metadata-line fixture yielded '$actual'"

  actual=$(session_field track <(printf '%s\n' '      description=Solo Line')) ||
    fail "the standalone description fixture did not parse"
  [[ $actual == 'Solo Line' ]] || fail "the standalone fixture yielded '$actual'"

  if session_field track <(printf '%s\n' '      queue_description=Not This') \
    >/dev/null 2>&1; then
    fail "a longer key ending in description unexpectedly parsed"
  fi

  printf 'Android visualizer session parser self-test passed\n'
}

usage() {
  cat >&2 <<EOF
Usage: $0 [WINDOW_SECONDS] [RUN_LABEL]
       $0 --self-test

Measure the currently playing Spectrum scene. Run it once per track, using
different labels for the primary and control arms. Defaults: 10 seconds and
label "spectrum".

Environment:
  REPRISE_SCENE_ASSUME_READY=1       skip the /dev/tty readiness prompt
  REPRISE_VISUALIZER_EVIDENCE_DIR    retain evidence in this directory
  REPRISE_VISUALIZER_CROP            ffmpeg crop W:H:X:Y (default $DEFAULT_CROP)
  REPRISE_ANDROID_PACKAGE            override the Android package name
EOF
  exit 2
}

# shellcheck disable=SC2329 # Called indirectly by the EXIT trap below.
cleanup() {
  adb shell rm -f "$REMOTE_VIDEO" >/dev/null 2>&1 || true
  if ((stayon_changed)); then
    if [[ -z $original_stayon || $original_stayon == null ]]; then
      adb shell settings delete global stay_on_while_plugged_in >/dev/null 2>&1 || true
    else
      adb shell settings put global stay_on_while_plugged_in "$original_stayon" >/dev/null 2>&1 || true
    fi
  fi
}

if [[ ${1:-} == --self-test ]]; then
  (($# == 1)) || fail "--self-test does not accept additional arguments"
  command -v python3 >/dev/null 2>&1 || fail "python3 is unavailable"
  self_test
  exit 0
fi

trap cleanup EXIT

window_seconds=10
run_label=spectrum
case $# in
  0) ;;
  1)
    if [[ $1 == -h || $1 == --help ]]; then
      usage
    elif [[ $1 =~ ^[1-9][0-9]*$ ]]; then
      window_seconds=$1
    else
      run_label=$1
    fi
    ;;
  2)
    window_seconds=$1
    run_label=$2
    ;;
  *) usage ;;
esac

[[ $window_seconds =~ ^[1-9][0-9]*$ ]] || fail "the window must be a positive whole number of seconds"
((window_seconds <= 180)) || fail "screenrecord supports at most 180 seconds"
[[ $run_label =~ ^[A-Za-z0-9._-]+$ ]] || fail "the run label may contain only letters, digits, dot, underscore, and hyphen"

command -v adb >/dev/null 2>&1 || fail "adb is unavailable"
command -v ffmpeg >/dev/null 2>&1 || fail "ffmpeg is unavailable"
command -v python3 >/dev/null 2>&1 || fail "python3 is unavailable"

if [[ -n ${REPRISE_VISUALIZER_EVIDENCE_DIR:-} ]]; then
  evidence_root=$REPRISE_VISUALIZER_EVIDENCE_DIR
  mkdir -p "$evidence_root"
else
  evidence_root=$(mktemp -d "${TMPDIR:-/tmp}/reprise-visualizer-data-rate.XXXXXX")
fi
evidence_dir="$evidence_root/$run_label"
mkdir -p "$evidence_dir"

devices_file="$evidence_dir/adb-devices.txt"
adb devices > "$devices_file" || fail "the adb device-list precondition failed (see $devices_file)"
device_count=$(awk '$2 == "device" { count += 1 } END { print count + 0 }' "$devices_file")
[[ $device_count -eq 1 ]] || fail "exactly one ready adb device is required; found $device_count (see $devices_file)"

assert_app_resumed() {
  local output_file=$1
  adb shell dumpsys activity activities > "$output_file" || fail "could not read resumed activity (see $output_file)"
  awk -v package="$PACKAGE_NAME" '
    BEGIN { package = tolower(package) }
    {
      line = tolower($0)
      if ((index(line, "resumedactivity") || index(line, "topresumedactivity")) &&
          index(line, package)) found = 1
    }
    END { exit found ? 0 : 1 }
  ' "$output_file" || fail "$PACKAGE_NAME is not the resumed activity (see $output_file)"
}

assert_app_focused() {
  local output_file=$1
  adb shell dumpsys window > "$output_file" || fail "could not read focused window (see $output_file)"
  awk -v package="$PACKAGE_NAME" '
    BEGIN { package = tolower(package) }
    {
      line = tolower($0)
      if (index(line, "mcurrentfocus=") || index(line, "mfocusedwindow=")) {
        if (!index(line, "=null")) {
          found = 1
          if (!index(line, package)) mismatch = 1
        }
      }
    }
    END { exit found && !mismatch ? 0 : 1 }
  ' "$output_file" || fail "$PACKAGE_NAME does not own the focused window (see $output_file)"
}

capture_session() {
  local output_file=$1
  local full_file="${output_file%.txt}-full.txt"
  adb shell dumpsys media_session > "$full_file" || fail "could not read media sessions (see $full_file)"
  awk -v package="package=$PACKAGE_NAME" '
    index($0, package) { found = 1; remaining = 50 }
    found {
      print
      remaining -= 1
      if (remaining == 0) exit
    }
    END { if (!found) exit 1 }
  ' "$full_file" > "$output_file" || fail "no media session for $PACKAGE_NAME was found (see $full_file)"
}

assert_playing() {
  local session_file=$1
  local state
  state=$(session_field state "$session_file") || fail "the playback state is unreadable in $session_file"
  [[ $state == 3 ]] || fail "the playback state is $state, not PLAYING, in $session_file"
}

if [[ ${REPRISE_SCENE_ASSUME_READY:-0} != 1 ]]; then
  printf 'Select Spectrum mode on the %s track, keep Reprise visible, then press Enter.\n' "$run_label" >&2
  IFS= read -r _ < /dev/tty || fail "could not read the readiness confirmation from the terminal"
fi

assert_app_resumed "$evidence_dir/activity-start.txt"
assert_app_focused "$evidence_dir/window-start.txt"
capture_session "$evidence_dir/media-start.txt"
assert_playing "$evidence_dir/media-start.txt"
track_start=$(session_field track "$evidence_dir/media-start.txt") || fail "the start track identity is missing"
adb exec-out screencap -p > "$evidence_dir/screen-start.png" || fail "could not capture the start screen"

original_stayon=$(adb shell settings get global stay_on_while_plugged_in | tr -d '\r') || fail "could not read the stay-awake setting"
adb shell svc power stayon usb > "$evidence_dir/stayon-usb.txt" || fail "could not keep the display awake over USB"
stayon_changed=1

adb shell rm -f "$REMOTE_VIDEO" > "$evidence_dir/remote-video-cleanup-start.txt" || fail "could not clear the remote video path"
adb shell screenrecord --time-limit "$window_seconds" --bit-rate 40000000 "$REMOTE_VIDEO" \
  > "$evidence_dir/screenrecord.txt" 2>&1 || fail "screenrecord failed (see $evidence_dir/screenrecord.txt)"

capture_session "$evidence_dir/media-end.txt"
assert_playing "$evidence_dir/media-end.txt"
assert_app_resumed "$evidence_dir/activity-end.txt"
assert_app_focused "$evidence_dir/window-end.txt"
track_end=$(session_field track "$evidence_dir/media-end.txt") || fail "the end track identity is missing"
[[ $track_start == "$track_end" ]] || fail "the track changed during the measurement window"
adb exec-out screencap -p > "$evidence_dir/screen-end.png" || fail "could not capture the end screen"

video_file="$evidence_dir/visualizer.mp4"
raw_file="$evidence_dir/bars.raw"
adb pull "$REMOTE_VIDEO" "$video_file" > "$evidence_dir/adb-pull.txt" || fail "could not pull the screen recording"
adb shell rm -f "$REMOTE_VIDEO" > "$evidence_dir/remote-video-cleanup-end.txt" || fail "could not remove the remote video"
[[ -s $video_file ]] || fail "the screen recording is empty"

crop=${REPRISE_VISUALIZER_CROP:-$DEFAULT_CROP}
ffmpeg -hide_banner -loglevel error -y -i "$video_file" \
  -filter:v "crop=$crop,scale=64:1:flags=area" \
  -pix_fmt gray -fps_mode passthrough -f rawvideo "$raw_file" \
  > "$evidence_dir/ffmpeg.txt" 2>&1 || fail "ffmpeg extraction failed (see $evidence_dir/ffmpeg.txt)"

analysis_file="$evidence_dir/analysis.txt"
python3 - "$raw_file" > "$analysis_file" <<'PY'
import collections
import math
import pathlib
import statistics
import sys

column_count = 64
rise_threshold = 6
long_gap_threshold = 15
raw = pathlib.Path(sys.argv[1]).read_bytes()
if not raw or len(raw) % column_count:
    raise SystemExit("raw frame data is empty or not divisible into 64 columns")
frames = [raw[offset : offset + column_count] for offset in range(0, len(raw), column_count)]
if len(frames) < 2:
    raise SystemExit("at least two video frames are required")

columns = [[frame[column] for frame in frames] for column in range(column_count)]
variances = [statistics.pvariance(column) for column in columns]
selected_column = max(range(column_count), key=variances.__getitem__)
series = columns[selected_column]
rises = [index for index in range(1, len(series)) if series[index] - series[index - 1] >= rise_threshold]
gaps = [right - left for left, right in zip(rises, rises[1:])]
if not gaps:
    raise SystemExit("fewer than two rises were found; choose a more active track segment")

ordered = sorted(gaps)
p95 = ordered[math.ceil(0.95 * len(ordered)) - 1]
long_gaps = sum(gap >= long_gap_threshold for gap in gaps)
long_gap_percent = 100.0 * long_gaps / len(gaps)
histogram = ",".join(f"{gap}:{count}" for gap, count in sorted(collections.Counter(gaps).items()))
verdict = "PASS" if long_gaps == 0 and p95 <= 8 else "FAIL"

print(f"frames={len(frames)}")
print(f"selected_column={selected_column}")
print(f"selected_variance={variances[selected_column]:.3f}")
print(f"rise_count={len(rises)}")
print(f"gap_count={len(gaps)}")
print(f"gaps_ge_15={long_gaps}")
print(f"gaps_ge_15_percent={long_gap_percent:.3f}")
print(f"p95_gap_frames={p95}")
print(f"histogram={histogram}")
print(f"threshold_verdict={verdict}")
PY

printf 'Evidence: %s\n' "$evidence_dir"
printf 'Track: %s\n' "$track_start"
printf 'Crop: %s\n' "$crop"
cat "$analysis_file"

threshold_verdict=''
while IFS='=' read -r key value; do
  if [[ $key == threshold_verdict ]]; then
    threshold_verdict=$value
  fi
done < "$analysis_file"

case $threshold_verdict in
  PASS) exit 0 ;;
  FAIL) exit 1 ;;
  *) fail "the threshold verdict is missing or invalid in $analysis_file" ;;
esac
