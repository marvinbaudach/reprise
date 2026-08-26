#!/usr/bin/env bash
# The film: the desktop half, then the phone half, no transition between them.
# Both halves already share codec, canvas and frame rate, so the join is a
# stream copy and re-running this costs nothing.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

GNOME="$SHOWREEL_DIR/reprise-gnome.mp4"
ANDROID="$SHOWREEL_DIR/reprise-android.mp4"
OUT="${1:-$SHOWREEL_DIR/reprise-showreel-60s.mp4}"
showreel_require "$GNOME" "$ANDROID"

LIST="$SHOWREEL_WORK/showreel-list.txt"
printf "file '%s'\nfile '%s'\n" "$GNOME" "$ANDROID" >"$LIST"

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf '%s  %s s (gnome %s + android %s)\n' \
  "$OUT" "$(showreel_duration "$OUT")" \
  "$(showreel_duration "$GNOME")" "$(showreel_duration "$ANDROID")"
