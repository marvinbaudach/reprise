#!/usr/bin/env bash
# Desktop footage onto the same 1920x1080 canvas the phone segments use, so the
# two platforms can sit in one timeline without a resolution change mid-film.
#
# Two sources: take 1 carries the sidebar tour, take 2 the three pickup scenes
# (podcast subscribe, search, lyrics). Take 2 was shot with a SCROLL-LOG debug
# badge pinned in the header bar, so its segments patch that corner over with a
# slice of empty header bar from further right — same gradient, same row, so the
# seam is invisible and the badge never blinks on mid-film. That is DEBADGE;
# drop it only if take 2 is re-shot without the badge.
#
# Four shots are tighter than the first cut, which ran 63.4 s against a name
# that says 60. Every second taken out is a hold measured off the takes rather
# than content: search, podcast-add and lyrics freeze for their last second,
# and the layout shot spent 2.9 s waiting for the click that is its whole point
# — its in-point moved instead, so the switch lands early and the result reads.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

IN1="$SHOWREEL_DIR/roh-gnome-take1.mp4"
IN2="$SHOWREEL_DIR/roh-gnome-take2.mp4"
PLATE="$SHOWREEL_DIR/welcome-plate.png"
OUT="$SHOWREEL_DIR/reprise-gnome.mp4"
showreel_require "$IN1" "$IN2" "$PLATE"

O="$SHOWREEL_WORK/cuts"
LIST="$O/gnome-list.txt"
mkdir -p -- "$O"
rm -f -- "$O"/g-*.mp4 "$LIST"

CROP="crop=2880:1747:0:53"
DEBADGE="split[a][b];[b]crop=180:88:600:0[p];[a][p]overlay=128:0"
CANVAS="scale=1780:-2,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:color=#0d1014,format=yuv420p,fps=30"

still() { # name image duration
  ffmpeg -v error -loop 1 -t "$3" -i "$2" -vf "$CANVAS" -an \
    -c:v libx264 -preset medium -crf 18 -y "$O/g-$1.mp4"
  printf "file '%s'\n" "$O/g-$1.mp4" >>"$LIST"
}
seg() { # name start duration   (take 1)
  ffmpeg -v error -ss "$2" -t "$3" -i "$IN1" -vf "$CROP,$CANVAS" -an \
    -c:v libx264 -preset medium -crf 18 -y "$O/g-$1.mp4"
  printf "file '%s'\n" "$O/g-$1.mp4" >>"$LIST"
}
seg2() { # name start duration  (take 2, badge patched out)
  ffmpeg -v error -ss "$2" -t "$3" -i "$IN2" -vf "$CROP,$DEBADGE,$CANVAS" -an \
    -c:v libx264 -preset medium -crf 18 -y "$O/g-$1.mp4"
  printf "file '%s'\n" "$O/g-$1.mp4" >>"$LIST"
}

still 00-welcome "$PLATE" 2.5
seg  01-library      6.5 2.5
seg2 02-search      76.5 3.0
seg  03-releases    14.0 2.0
seg  04-concerts    21.0 2.0
seg  05-podcasts    27.5 2.0
seg2 06-podcast-add 50.5 3.0
seg  07-youtube     34.5 2.0
seg  08-sync        41.5 2.5
seg  09-doctor      49.5 2.5
seg  10-visuals     57.5 3.0
seg2 11-lyrics      95.5 3.0
seg  12-stats       76.0 2.5
seg  13-layout      87.5 4.5
seg  14-plugins     96.5 2.5

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf 'gnome %s s\n' "$(showreel_duration "$OUT")"
