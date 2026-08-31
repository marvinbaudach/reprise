#!/usr/bin/env bash
# Drive the phone through the showreel scenes and write a timeline for the cut.
#
# Every coordinate below is pinned to a Pixel 10 Pro XL in portrait. A different
# device, a different density or a layout change invalidates all of them — check
# the take before trusting the timeline it writes.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

timeline="$SHOWREEL_WORK/timeline-android.tsv"
t0=$(date +%s.%N)
: >"$timeline"

mark() {
  printf '%s\t%s\n' "$1" "$(echo "$(date +%s.%N) - $t0" | bc)" >>"$timeline"
  printf '[%s]\n' "$1"
}
tap() { adb shell input tap "$1" "$2"; }
swipe() { adb shell input swipe "$1" "$2" "$3" "$4" "${5:-400}"; }

sleep 2.5
mark library
swipe 540 1600 540 900 700
sleep 2
swipe 540 1600 540 900 700
sleep 3

mark search
tap 892 219
sleep 1.6
adb shell input text "lorna"
sleep 2
adb shell input keyevent BACK
sleep 5 # hide the keyboard, keep the results

mark play
tap 400 540
sleep 4 # first hit starts playing

mark nowplaying
tap 300 2044
sleep 8 # mini player -> Now Playing, cover fog

mark visualizer
tap 540 913
sleep 11 # artwork -> audio-reactive scene

# This swipe did not move the playhead in the shipped take: the recording shows
# the position running on at playback speed, not jumping. Watch the take before
# cutting a seek shot out of it.
mark seek
swipe 200 1763 760 1763 900
sleep 5

mark queue
adb shell input keyevent BACK
sleep 1.5
tap 905 2219
sleep 6

mark reset
tap 170 2219
sleep 1.5
tap 985 260
sleep 1.5 # clear the search
mark end
