#!/usr/bin/env bash
# The phone half: the portrait frame centred on its own blurred enlargement, so
# the sides are not dead black, on the same stage-and-band the desktop uses.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh
source scripts/showreel/film.sh

IN="$SHOWREEL_DIR/roh-android-take.mp4"
OUT="$SHOWREEL_DIR/reprise-android.mp4"
showreel_require "$IN"

O="$SHOWREEL_WORK/cuts"
LIST="$O/android-list.txt"
mkdir -p -- "$O"
rm -f -- "$O"/a-*.mp4 "$LIST"

ENC=(-an -c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p)
frames() { python3 -c "print(round($1*30))"; }

shot() { # name start duration direction caption [dip]
  ffmpeg -v error -ss "$2" -t "$3" -i "$IN" \
    -filter_complex "[0:v]fps=30,split[b][f];\
[b]scale=1920:-2,boxblur=28:2,crop=1920:${FILM_STAGE_H}[bg];\
[f]scale=-2:950[fg];\
[bg][fg]overlay=(W-w)/2:(H-h)/2,$(film_push "$(frames "$3")" "$4" 1920 "$FILM_STAGE_H"),\
pad=1920:1080:0:0:color=$FILM_GROUND,$(film_rail "$5" "$3")$(film_dip "${6:-}" "$3"),format=yuv420p[v]" \
    -map '[v]' "${ENC[@]}" -y "$O/a-$1.mp4"
  printf "file '%s'\n" "$O/a-$1.mp4" >>"$LIST"
}

shot 1-library     6.0 2.5 in  'Reprise on Android' in
shot 2-search     13.6 3.5 out 'Search'
shot 3-play       25.0 2.5 in  'Play'
shot 4-visualizer 29.6 3.5 out 'The same visuals on the phone'
shot 5-artwork    36.0 2.0 in  'Cover artwork'
# 6-seek carries no caption on purpose. The swipe never moved the playhead —
# the recording shows the position running on at playback speed, not jumping —
# so the shot is a held beat of Now Playing and must not claim to be a seek.
shot 6-seek       46.5 2.0 out ''
shot 7-queue      53.0 2.5 in  'The queue' out

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf 'android %s s\n' "$(showreel_duration "$OUT")"
