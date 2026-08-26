#!/usr/bin/env bash
# Phone footage onto a 1920x1080 canvas: the portrait frame centred, its own
# blurred enlargement behind it so the sides are not dead black.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

IN="$SHOWREEL_DIR/roh-android-take.mp4"
OUT="$SHOWREEL_DIR/reprise-android.mp4"
showreel_require "$IN"

O="$SHOWREEL_WORK/cuts"
LIST="$O/android-list.txt"
mkdir -p -- "$O"
rm -f -- "$O"/a-*.mp4 "$LIST"

seg() { # name start duration
  ffmpeg -v error -ss "$2" -t "$3" -i "$IN" \
    -filter_complex "[0:v]scale=1920:-2,boxblur=28:2,crop=1920:1080[bg];\
[0:v]scale=-2:1040[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,format=yuv420p,fps=30[v]" \
    -map "[v]" -an -c:v libx264 -preset medium -crf 18 -y "$O/a-$1.mp4"
  printf "file '%s'\n" "$O/a-$1.mp4" >>"$LIST"
}

seg 1-library     6.0  2.5
seg 2-search     13.6  4.0
seg 3-play       25.0  2.5
seg 4-visualizer 29.6  3.5
seg 5-artwork    36.0  2.5
# 6-seek: the swipe never moved the playhead — the position just runs on at
# playback speed, so the shot is the same still Now Playing as 3 and 5. It is
# kept at its recorded in-point until the phone take is re-shot; see the
# handover's open points before spending a second on it.
seg 6-seek       46.5  2.5
seg 7-queue      53.0  2.5

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf 'android %s s\n' "$(showreel_duration "$OUT")"
