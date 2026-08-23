#!/usr/bin/env bash
# Measure the two Android Now Playing visualizer arms without accepting invalid runs.

set -euo pipefail

readonly PACKAGE_NAME="${REPRISE_ANDROID_PACKAGE:-io.github.marvinbaudach.reprise}"
readonly REMOTE_UI_DUMP="/sdcard/reprise-scene-window.xml"
readonly POSITION_NODE_ID="now-playing-position"
readonly WINDOW_TOLERANCE_MS=500
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
if ! adb devices > "$devices_file"; then
  fail "the adb device-list precondition failed (see $devices_file)"
fi
device_count=$(awk '$2 == "device" { count += 1 } END { print count + 0 }' "$devices_file")
[[ $device_count -eq 1 ]] || fail "exactly one ready adb device is required; found $device_count (see $devices_file)"

assert_app_resumed() {
  local output_file=$1
  if ! adb shell dumpsys activity activities > "$output_file"; then
    fail "the resumed-activity precondition could not read dumpsys activity (see $output_file)"
  fi
  awk -v package="$PACKAGE_NAME" '
    {
      line = tolower($0)
      if ((index(line, "resumedactivity") || index(line, "topresumedactivity")) &&
          index($0, package)) {
        found = 1
      }
    }
    END { exit found ? 0 : 1 }
  ' "$output_file" || fail "the resumed-activity precondition failed: $PACKAGE_NAME is not resumed (see $output_file)"
}

assert_app_focused() {
  local output_file=$1
  if ! adb shell dumpsys window > "$output_file"; then
    fail "the window-focus precondition could not read dumpsys window (see $output_file)"
  fi
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
  ' "$output_file" || fail "the window-focus precondition failed: $PACKAGE_NAME does not own the focused window (see $output_file)"
}

capture_session() {
  local output_file=$1
  local full_file="${output_file%.txt}-full.txt"
  if ! adb shell dumpsys media_session > "$full_file"; then
    fail "the media-session precondition could not read dumpsys media_session (see $full_file)"
  fi
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
  python3 - "$ui_file" "$POSITION_NODE_ID" <<'PY'
import re
import sys
import xml.etree.ElementTree as ET

path, position_id = sys.argv[1:]
root = ET.parse(path).getroot()
pattern = re.compile(r"(?<![-−\d:])((?:\d+:)?\d{1,2}:\d{2})(?![\d:])")
position_nodes = []
for node in root.iter():
    resource_id = node.attrib.get("resource-id", "")
    if resource_id == position_id or resource_id.endswith(f":id/{position_id}"):
        position_nodes.append(node)
if not position_nodes:
    raise SystemExit(
        f"no UI node with playback-position resource-id {position_id!r} found in {path}"
    )
for node in position_nodes:
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
raise SystemExit(
    f"the playback-position node {position_id!r} has no non-negative time label in {path}"
)
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

monotonic_ms() {
  python3 - <<'PY'
import time

print(time.monotonic_ns() // 1_000_000)
PY
}

sleep_for_ms() {
  local milliseconds=$1
  local duration
  if ((milliseconds <= 0)); then
    return
  fi
  printf -v duration '%d.%03d' "$((milliseconds / 1000))" "$((milliseconds % 1000))"
  sleep "$duration" || fail "the measurement clock could not sleep for $milliseconds ms"
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
  local pid logcat_pid window_start_ms now_ms remaining_ms actual_window_ms
  local nominal_window_ms duration_delta_ms
  local state_start state_end position_mid ui_position delta verdict
  local track_start track_mid track_end gfx_values gc_bytes
  mkdir -p "$arm_dir"
  nominal_window_ms=$((window_seconds * 1000))

  prompt_for_arm "$arm"
  assert_app_resumed "$arm_dir/activity-start.txt"
  assert_app_focused "$arm_dir/window-focus-start.txt"
  if ! pid=$(adb shell pidof "$PACKAGE_NAME"); then
    fail "the app-PID precondition failed: $PACKAGE_NAME is not running"
  fi
  [[ $pid =~ ^[0-9]+$ ]] || fail "a single running PID for $PACKAGE_NAME is required; got '$pid'"

  capture_session "$arm_dir/media-start.txt"
  assert_playing "$arm_dir/media-start.txt"
  state_start=$(session_field state "$arm_dir/media-start.txt")
  track_start=$(session_field track "$arm_dir/media-start.txt") || fail "the start track identity is absent from $arm_dir/media-start.txt"
  if [[ -n $reference_track && $track_start != "$reference_track" ]]; then
    fail "the control arm is not playing the same track as the first arm"
  fi

  if ! adb shell dumpsys gfxinfo "$PACKAGE_NAME" reset > "$arm_dir/gfx-reset.txt"; then
    fail "the gfxinfo-reset precondition failed for $PACKAGE_NAME (see $arm_dir/gfx-reset.txt)"
  fi
  window_start_ms=$(monotonic_ms) || fail "the measurement clock could not record the $arm start"
  adb logcat --pid="$pid" -v epoch > "$arm_dir/logcat.txt" 2>&1 &
  logcat_pid=$!
  active_logcat_pid=$logcat_pid

  sleep_for_ms $((nominal_window_ms / 2))
  if ! adb exec-out screencap -p > "$arm_dir/window.png"; then
    fail "the in-window screenshot precondition failed (see $arm_dir/window.png)"
  fi
  if ! adb shell uiautomator dump "$REMOTE_UI_DUMP" > "$arm_dir/uiautomator.txt"; then
    fail "the UI-dump precondition failed (see $arm_dir/uiautomator.txt)"
  fi
  if ! adb pull "$REMOTE_UI_DUMP" "$arm_dir/window.xml" > "$arm_dir/uiautomator-pull.txt"; then
    fail "the UI-dump pull precondition failed (see $arm_dir/uiautomator-pull.txt)"
  fi
  if ! adb shell rm -f "$REMOTE_UI_DUMP" > "$arm_dir/uiautomator-cleanup.txt"; then
    fail "the UI-dump cleanup precondition failed (see $arm_dir/uiautomator-cleanup.txt)"
  fi
  capture_session "$arm_dir/media-mid.txt"

  now_ms=$(monotonic_ms) || fail "the measurement clock could not record the $arm midpoint"
  remaining_ms=$((nominal_window_ms - (now_ms - window_start_ms)))
  sleep_for_ms "$remaining_ms"

  now_ms=$(monotonic_ms) || fail "the measurement clock could not record the $arm end"
  actual_window_ms=$((now_ms - window_start_ms))
  if ! adb shell dumpsys gfxinfo "$PACKAGE_NAME" > "$arm_dir/gfx.txt"; then
    fail "the gfxinfo-result precondition failed for $PACKAGE_NAME (see $arm_dir/gfx.txt)"
  fi
  capture_session "$arm_dir/media-end.txt"
  assert_app_resumed "$arm_dir/activity-end.txt"
  assert_app_focused "$arm_dir/window-focus-end.txt"
  kill "$logcat_pid" 2>/dev/null || true
  wait "$logcat_pid" 2>/dev/null || true
  active_logcat_pid=''

  assert_playing "$arm_dir/media-mid.txt"
  assert_playing "$arm_dir/media-end.txt"
  state_end=$(session_field state "$arm_dir/media-end.txt")
  [[ $state_start == "$state_end" ]] || fail "the playback states at the ends of the $arm window disagree"

  duration_delta_ms=$((actual_window_ms - nominal_window_ms))
  if ((duration_delta_ms < 0)); then
    duration_delta_ms=$((-duration_delta_ms))
  fi
  if ((duration_delta_ms > WINDOW_TOLERANCE_MS)); then
    fail "the measurement-window precondition failed: the $arm gfxinfo window lasted $actual_window_ms ms, nominal is $nominal_window_ms ms with a +/-$WINDOW_TOLERANCE_MS ms tolerance"
  fi

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
  if ((delta > actual_window_ms)); then
    verdict=DESYNCED
  fi

  gfx_values=$(gfx_fields "$arm_dir/gfx.txt") || fail "the gfxinfo verdict is incomplete in $arm_dir/gfx.txt"
  gc_bytes=$(gc_bytes_freed "$arm_dir/logcat.txt") || fail "the GC-byte verdict could not be read from $arm_dir/logcat.txt"
  result_rows+=("$arm\t$verdict\t$actual_window_ms\tPLAYING@$state_start\tPLAYING@$state_end\t$position_mid\t$ui_position\t$delta\t$gfx_values\t$gc_bytes")
}

measure_arm "$first_arm"
measure_arm "$control_arm"

printf '\nEvidence: %s\n' "$evidence_dir"
printf 'Nominal window: %d ms; allowed deviation: +/-%d ms\n' "$((window_seconds * 1000))" "$WINDOW_TOLERANCE_MS"
printf 'arm\tstatus\twindow_ms\tstart\tend\tmedia_ms\tui_ms\tdelta_ms\tframes\tjanky\tp50\tp90\tp95\tgc_bytes\n'
printf '%b\n' "${result_rows[@]}"
