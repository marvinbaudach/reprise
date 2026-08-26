#!/usr/bin/env bash
# The desktop half: a title card, then fifteen shots on the 1920x1080 canvas the
# phone half also uses, so the film never changes resolution mid-play.
#
# Two sources: take 1 carries the sidebar tour, take 2 the three pickup scenes
# (podcast subscribe, search, lyrics). Take 2 was shot with a SCROLL-LOG debug
# badge pinned in the header bar, so its segments patch that corner over with a
# slice of empty header bar from further right — same gradient, same row, so the
# seam is invisible and the badge never blinks on mid-film. That is DEBADGE;
# drop it only if take 2 is re-shot without the badge.
#
# Shot lengths come from measuring the takes, not from taste: search,
# podcast-add and lyrics were each holding a frozen frame for their last
# second, and the layout shot spent 2.9 s waiting for the click that is its
# whole point, so its in-point moved rather than its length being cut.
#
# fps=30 has to come first. The screencast is variable-rate (r_frame_rate reads
# 10000/1, really about 22.6), and zoompan counts input frames — fed VFR it
# renders the wrong number of them and the shot runs long.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh
source scripts/showreel/film.sh

IN1="$SHOWREEL_DIR/roh-gnome-take1.mp4"
IN2="$SHOWREEL_DIR/roh-gnome-take2.mp4"
PLATE="$SHOWREEL_DIR/welcome-plate.png"
CARD="$SHOWREEL_DIR/card-title.png"
OUT="$SHOWREEL_DIR/reprise-gnome.mp4"
showreel_require "$IN1" "$IN2" "$PLATE" "$CARD"

O="$SHOWREEL_WORK/cuts"
LIST="$O/gnome-list.txt"
mkdir -p -- "$O"
rm -f -- "$O"/g-*.mp4 "$LIST"

CROP="crop=2880:1747:0:53"
DEBADGE="split[a][b];[b]crop=180:88:600:0[p];[a][p]overlay=128:0"
STAGE_W=1628
PAD="pad=1920:1080:146:0:color=$FILM_GROUND"
ENC=(-an -c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p)

frames() { python3 -c "print(round($1*30))"; }

card() { # name image duration direction
  ffmpeg -v error -loop 1 -t "$3" -i "$2" \
    -vf "fps=30,$(film_push "$(frames "$3")" "$4" 1920 1080),format=yuv420p" \
    "${ENC[@]}" -y "$O/g-$1.mp4"
}
still() { # name image duration direction caption
  ffmpeg -v error -loop 1 -t "$3" -i "$2" \
    -vf "fps=30,$(film_push "$(frames "$3")" "$4" "$STAGE_W" "$FILM_STAGE_H"),$PAD,$(film_rail "$5" "$3"),format=yuv420p" \
    "${ENC[@]}" -y "$O/g-$1.mp4"
}
shot() { # source name start duration direction caption [dip]
  local src=$1 pre=""
  shift
  local name=$1 start=$2 dur=$3 dir=$4 cap=$5 dip=${6:-}
  local input="$IN1"
  [[ $src == T2 ]] && {
    input="$IN2"
    pre="$DEBADGE,"
  }
  ffmpeg -v error -ss "$start" -t "$dur" -i "$input" \
    -vf "fps=30,$CROP,$pre$(film_push "$(frames "$dur")" "$dir" "$STAGE_W" "$FILM_STAGE_H"),$PAD,$(film_rail "$cap" "$dur")$(film_dip "$dip" "$dur"),format=yuv420p" \
    "${ENC[@]}" -y "$O/g-$name.mp4"
}
listed() { printf "file '%s'\n" "$O/g-$1.mp4" >>"$LIST"; }

card  00-card "$CARD" 1.5 in
still 01-welcome "$PLATE" 2.0 out 'First run'
shot T1 02-library      6.5 2.5 in  'Your library, local first'
shot T2 03-search      76.5 3.0 out 'Search every field at once'
shot T1 04-releases    14.0 2.0 in  'Releases'
shot T1 05-concerts    21.0 2.0 out 'Concerts'
shot T1 06-podcasts    27.5 2.0 in  'Podcasts'
shot T2 07-podcast-add 50.5 3.0 out 'Subscribe to a show'
shot T1 08-youtube     34.5 2.0 in  'YouTube channels as audio'
shot T1 09-sync        41.5 2.5 out 'Sync to your phone'
shot T1 10-doctor      49.5 2.0 in  'Library Doctor'
shot T1 11-visuals     57.5 3.0 out 'Audio-reactive visuals'
shot T2 12-lyrics      95.5 3.0 in  'Lyrics'
shot T1 13-stats       76.0 2.5 out 'Your listening, counted'
shot T1 14-layout      87.5 4.5 in  'Move the player bar'
shot T1 15-plugins     96.5 2.5 out 'Online sources are plugins' out

# The opening breathes: the card dissolves into the welcome screen and that
# into the library. Every later join is a hard cut, which costs no runtime.
ffmpeg -v error -i "$O/g-00-card.mp4" -i "$O/g-01-welcome.mp4" -i "$O/g-02-library.mp4" \
  -filter_complex "[0][1]xfade=transition=fade:duration=0.4:offset=1.1[a];\
[a][2]xfade=transition=fade:duration=0.3:offset=2.8" \
  "${ENC[@]}" -y "$O/g-opening.mp4"

printf "file '%s'\n" "$O/g-opening.mp4" >"$LIST"
for name in 03-search 04-releases 05-concerts 06-podcasts 07-podcast-add 08-youtube \
  09-sync 10-doctor 11-visuals 12-lyrics 13-stats 14-layout 15-plugins; do
  listed "$name"
done

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf 'gnome %s s\n' "$(showreel_duration "$OUT")"
