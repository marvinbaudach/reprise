#!/usr/bin/env bash
# Measure the two Android Now Playing visualizer arms without accepting invalid runs.

set -euo pipefail

readonly PACKAGE_NAME="${REPRISE_ANDROID_PACKAGE:-io.github.marvinbaudach.reprise}"
readonly REMOTE_UI_DUMP="/sdcard/reprise-scene-window.xml"
active_logcat_pid=''

stop_logcat() {
  if [[ -n $active_logcat_pid ]]; then
    kill "$active_logcat_pid" 2>/dev/null || true
    wait "$active_logcat_pid" 2>/dev/null || true
    active_logcat_pid=''
  fi
}

trap stop_logcat EXIT

fail() {
  printf 'android-scene-framerate: %s\n' "$*" >&2
  exit 1
}

usage() {
  printf 'Usage: %s [WINDOW_SECONDS] <cover|spectrum>\n' "$0" >&2
  exit 2
}

window_seconds=10
case $# in
  1)
    first_arm=$1
    ;;
  2)
    window_seconds=$1
    first_arm=$2
    ;;
  *)
    usage
    ;;
esac

[[ $window_seconds =~ ^[1-9][0-9]*$ ]] || fail "the window must be a positive whole number of seconds"
case $first_arm in
  cover)
    control_arm=spectrum
    ;;
  spectrum)
    control_arm=cover
    ;;
  *)
    fail "the arm label must be cover or spectrum so its control arm is unambiguous"
    ;;
esac

command -v adb >/dev/null 2>&1 || fail "adb is unavailable"
command -v python3 >/dev/null 2>&1 || fail "python3 is unavailable"

if [[ -n ${REPRISE_SCENE_EVIDENCE_DIR:-} ]]; then
  evidence_dir=$REPRISE_SCENE_EVIDENCE_DIR
  mkdir -p "$evidence_dir"
else
  evidence_dir=$(mktemp -d "${TMPDIR:-/tmp}/reprise-scene-framerate.XXXXXX")
fi

devices_file="$evidence_dir/adb-devices.txt"
adb devices > "$devices_file"
device_count=$(awk '$2 == "device" { count += 1 } END { print count + 0 }' "$devices_file")
[[ $device_count -eq 1 ]] || fail "exactly one ready adb device is required; found $device_count (see $devices_file)"

assert_app_resumed() {
  local output_file=$1
  adb shell dumpsys activity activities > "$output_file"
  awk -v package="$PACKAGE_NAME" '
    {
      line = tolower($0)
      if ((index(line, "resumedactivity") || index(line, "topresumedactivity")) &&
          index($0, package)) {
        found = 1
      }
    }
    END { exit found ? 0 : 1 }
  ' "$output_file" || fail "$PACKAGE_NAME is not the resumed activity (see $output_file)"
}

capture_session() {
  local output_file=$1
  local full_file="${output_file%.txt}-full.txt"
  adb shell dumpsys media_session > "$full_file"
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

session_field() {
  local field=$1
  local input_file=$2
  python3 - "$field" "$input_file" <<'PY'
import pathlib
import re
import sys

field, path = sys.argv[1:]
text = pathlib.Path(path).read_text(encoding="utf-8", errors="replace")
if field == "state":
    match = re.search(r"state=PlaybackState\s*\{state=(\d+)", text)
    if match is None:
        match = re.search(r"\bstate=(\d+)\b", text)
elif field == "position":
    match = re.search(r"\bposition=(\d+)\b", text)
elif field == "track":
    match = re.search(r"^\s*description=(.+)$", text, re.MULTILINE)
else:
    raise SystemExit(f"unsupported session field: {field}")
if match is None:
    raise SystemExit(f"missing {field} in {path}")
print(match.group(1).strip())
PY
}

assert_playing() {
  local session_file=$1
  local state
  state=$(session_field state "$session_file") || fail "the playback state could not be read from $session_file"
  [[ $state == 3 ]] || fail "the playback state is $state, not PLAYING, in $session_file"
}

screen_position_ms() {
  local ui_file=$1
  python3 - "$ui_file" <<'PY'
import re
import sys
import xml.etree.ElementTree as ET

root = ET.parse(sys.argv[1]).getroot()
pattern = re.compile(r"(?<![-−\d:])((?:\d+:)?\d{1,2}:\d{2})(?![\d:])")
for node in root.iter():
    for attribute in ("text", "content-desc"):
        value = node.attrib.get(attribute, "")
        match = pattern.search(value)
        if match is None:
            continue
        parts = [int(part) for part in match.group(1).split(":")]
        if len(parts) == 2:
            minutes, seconds = parts
            hours = 0
        else:
            hours, minutes, seconds = parts
        print(((hours * 60 + minutes) * 60 + seconds) * 1000)
        raise SystemExit(0)
raise SystemExit(f"no non-negative on-screen time label found in {sys.argv[1]}")
PY
}

gfx_fields() {
  local input_file=$1
  python3 - "$input_file" <<'PY'
import pathlib
import re
import sys

text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
patterns = (
    r"Total frames rendered:\s*([^\n]+)",
    r"Janky frames:\s*([^\n]+)",
    r"50th percentile:\s*([^\n]+)",
    r"90th percentile:\s*([^\n]+)",
    r"95th percentile:\s*([^\n]+)",
)
values = []
for pattern in patterns:
    match = re.search(pattern, text)
    if match is None:
        raise SystemExit(f"missing gfxinfo field {pattern!r} in {sys.argv[1]}")
    values.append(match.group(1).strip())
print("\t".join(values))
PY
}

gc_bytes_freed() {
  local input_file=$1
  python3 - "$input_file" <<'PY'
import pathlib
import re
import sys

text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
units = {"B": 1, "KB": 1024, "MB": 1024**2, "GB": 1024**3}
total = 0
pattern = re.compile(r"(\d+)\((\d+)([KMGT]?B)\)\s+(?:AllocSpace|LOS) objects")
for match in pattern.finditer(text):
    total += int(match.group(2)) * units.get(match.group(3), 1)
print(total)
PY
}

prompt_for_arm() {
  local arm=$1
  if [[ ${REPRISE_SCENE_ASSUME_READY:-0} == 1 ]]; then
    return
  fi
  printf 'Select the %s visualizer on the same playing track, keep Reprise visible, then press Enter.\n' "$arm" >&2
  IFS= read -r _ < /dev/tty || fail "could not read the arm confirmation from the terminal"
}

declare -a result_rows=()
reference_track=''

measure_arm() {
  local arm=$1
  local arm_dir="$evidence_dir/$arm"
  local pid logcat_pid start_epoch remaining elapsed
  local state_start state_end position_mid ui_position delta verdict
  local track_start track_mid track_end gfx_values gc_bytes
  mkdir -p "$arm_dir"

  prompt_for_arm "$arm"
  assert_app_resumed "$arm_dir/activity-start.txt"
  pid=$(adb shell pidof "$PACKAGE_NAME")
  [[ $pid =~ ^[0-9]+$ ]] || fail "a single running PID for $PACKAGE_NAME is required; got '$pid'"

  capture_session "$arm_dir/media-start.txt"
  assert_playing "$arm_dir/media-start.txt"
  state_start=$(session_field state "$arm_dir/media-start.txt")
  track_start=$(session_field track "$arm_dir/media-start.txt") || fail "the start track identity is absent from $arm_dir/media-start.txt"
  if [[ -n $reference_track && $track_start != "$reference_track" ]]; then
    fail "the control arm is not playing the same track as the first arm"
  fi

  adb shell dumpsys gfxinfo "$PACKAGE_NAME" reset > "$arm_dir/gfx-reset.txt"
  adb logcat --pid="$pid" -v epoch > "$arm_dir/logcat.txt" 2>&1 &
  logcat_pid=$!
  active_logcat_pid=$logcat_pid
  start_epoch=$(date +%s)

  sleep $((window_seconds / 2))
  adb exec-out screencap -p > "$arm_dir/window.png"
  adb shell uiautomator dump "$REMOTE_UI_DUMP" > "$arm_dir/uiautomator.txt"
  adb pull "$REMOTE_UI_DUMP" "$arm_dir/window.xml" > "$arm_dir/uiautomator-pull.txt"
  adb shell rm -f "$REMOTE_UI_DUMP" > "$arm_dir/uiautomator-cleanup.txt"
  capture_session "$arm_dir/media-mid.txt"

  elapsed=$(($(date +%s) - start_epoch))
  remaining=$((window_seconds - elapsed))
  if ((remaining > 0)); then
    sleep "$remaining"
  fi

  capture_session "$arm_dir/media-end.txt"
  adb shell dumpsys gfxinfo "$PACKAGE_NAME" > "$arm_dir/gfx.txt"
  assert_app_resumed "$arm_dir/activity-end.txt"
  kill "$logcat_pid" 2>/dev/null || true
  wait "$logcat_pid" 2>/dev/null || true
  active_logcat_pid=''

  assert_playing "$arm_dir/media-mid.txt"
  assert_playing "$arm_dir/media-end.txt"
  state_end=$(session_field state "$arm_dir/media-end.txt")
  [[ $state_start == "$state_end" ]] || fail "the playback states at the ends of the $arm window disagree"

  track_mid=$(session_field track "$arm_dir/media-mid.txt") || fail "the middle track identity is absent from $arm_dir/media-mid.txt"
  track_end=$(session_field track "$arm_dir/media-end.txt") || fail "the end track identity is absent from $arm_dir/media-end.txt"
  [[ $track_start == "$track_mid" && $track_start == "$track_end" ]] || fail "the track changed during the $arm window"
  if [[ -z $reference_track ]]; then
    reference_track=$track_start
  fi

  position_mid=$(session_field position "$arm_dir/media-mid.txt") || fail "the middle playback position is absent from $arm_dir/media-mid.txt"
  ui_position=$(screen_position_ms "$arm_dir/window.xml") || fail "the on-screen position could not be read from $arm_dir/window.xml"
  delta=$((ui_position - position_mid))
  if ((delta < 0)); then
    delta=$((-delta))
  fi
  verdict=IN_SYNC
  if ((delta > window_seconds * 1000)); then
    verdict=DESYNCED
  fi

  gfx_values=$(gfx_fields "$arm_dir/gfx.txt") || fail "the gfxinfo verdict is incomplete in $arm_dir/gfx.txt"
  gc_bytes=$(gc_bytes_freed "$arm_dir/logcat.txt")
  result_rows+=("$arm\t$verdict\tPLAYING@$state_start\tPLAYING@$state_end\t$position_mid\t$ui_position\t$delta\t$gfx_values\t$gc_bytes")
}

measure_arm "$first_arm"
measure_arm "$control_arm"

printf '\nEvidence: %s\n' "$evidence_dir"
printf 'arm\tstatus\tstart\tend\tmedia_ms\tui_ms\tdelta_ms\tframes\tjanky\tp50\tp90\tp95\tgc_bytes\n'
printf '%b\n' "${result_rows[@]}"
