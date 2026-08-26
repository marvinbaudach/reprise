#!/usr/bin/env bash
# The film: title card and desktop, a dip to black at the platform change, the
# phone, then the end card. The dip is the only place the film goes dark — it
# is what tells the eye the platform changed rather than the view.
#
# The dips are already baked into the last and first shots of each half (see
# film_dip), so this is a stream copy: no generation loss, and re-running it
# costs nothing.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh
source scripts/showreel/film.sh

GNOME="$SHOWREEL_DIR/reprise-gnome.mp4"
ANDROID="$SHOWREEL_DIR/reprise-android.mp4"
CARD="$SHOWREEL_DIR/card-end.png"
OUT="${1:-$SHOWREEL_DIR/reprise-showreel-60s.mp4}"
showreel_require "$GNOME" "$ANDROID" "$CARD"

O="$SHOWREEL_WORK/cuts"
LIST="$O/film-list.txt"
mkdir -p -- "$O"
END_DUR=1.7

ffmpeg -v error -loop 1 -t "$END_DUR" -i "$CARD" \
  -vf "fps=30,$(film_push "$(python3 -c "print(round($END_DUR*30))")" out 1920 1080)$(film_dip in "$END_DUR"),format=yuv420p" \
  -an -c:v libx264 -preset medium -crf 18 -pix_fmt yuv420p -y "$O/z-end.mp4"

printf "file '%s'\nfile '%s'\nfile '%s'\n" "$GNOME" "$ANDROID" "$O/z-end.mp4" >"$LIST"
ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"

printf '%s  %s s (gnome %s + android %s + card %s)\n' "$OUT" "$(showreel_duration "$OUT")" \
  "$(showreel_duration "$GNOME")" "$(showreel_duration "$ANDROID")" "$(showreel_duration "$O/z-end.mp4")"
