#!/usr/bin/env bash
# Bring a track's second drop to the cut that needs it.
#
# A generated track has its own architecture and the film has its own, and
# nothing makes them agree. This one is 90 s: a quiet intro to 18.0, a full
# section to 48.0, a breakdown to 66.0, and a second full section from 66.0.
# The film is 55.8 s and wants the intro under its title card, the full section
# under the desktop run, and the second drop on the slide to the phone at 41.4.
# Laid down flat, the second drop arrives at 55.5 — under the end card, ten
# seconds after the shot it was meant for.
#
# Choosing a different window cannot fix it: the two lifts are 48 s apart in the
# track and 34 s apart in the film. One of them lands or the other does, never
# both. So the breakdown is shortened, which is what it is for — a dub editor
# takes bars out of the quiet part rather than asking for a different take.
#
# Both cut points are bar lines at 120 BPM (multiples of 2.0 s), and the join is
# a downbeat landing on a downbeat, so nothing about the grid moves.
#
#   splice-bed.sh TRACK OUT WINDOW OUT_POINT IN_POINT LENGTH
#
# All four numbers are seconds in the *original* track. The tempo correction
# from align-bed.py is applied first and the numbers are corrected with it, so
# they can be read straight off a loudness profile of the untouched file.
set -euo pipefail

TRACK=${1:?usage: splice-bed.sh TRACK OUT WINDOW OUT_POINT IN_POINT LENGTH}
OUT=${2:?}
WINDOW=${3:?}      # where the film starts in the track
OUT_POINT=${4:?}   # where the breakdown is left
IN_POINT=${5:?}    # where it is rejoined — the drop
LENGTH=${6:?}      # the film's length

cd "$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
BPM=${SHOWREEL_BPM:-120}
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

FACTOR=$(python3 scripts/showreel/align-bed.py "$TRACK" "$BPM" "$LENGTH" |
           awk '/^atempo/ {print $2}')
FACTOR=${FACTOR:-1.0}

# atempo scales the timeline, so every point measured on the original file moves
# with it. Correcting the numbers here is what lets them be read off the
# untouched track — measuring them on a stretched copy would mean re-measuring
# after every tempo change.
read -r A_END B_START B_LEN < <(python3 -c "
w, o, i, n, f = ${WINDOW}, ${OUT_POINT}, ${IN_POINT}, ${LENGTH}, ${FACTOR}
a_end = (o - w) / f          # how much of the film segment one covers
print(f'{a_end:.4f} {i / f:.4f} {n - a_end:.4f}')")

printf 'splice: atempo=%s  A=%s..%s (%ss)  B=%s..+%ss  total=%ss\n' \
  "$FACTOR" "$(python3 -c "print(f'{${WINDOW}/${FACTOR}:.4f}')")" \
  "$(python3 -c "print(f'{${OUT_POINT}/${FACTOR}:.4f}')")" "$A_END" \
  "$B_START" "$B_LEN" "$LENGTH"

ffmpeg -v error -i "$TRACK" -af "atempo=${FACTOR}" -ac 2 -ar 48000 -y "$WORK/stretched.wav"

# The tail of segment A is inside the breakdown and quiet, so a short fade there
# removes the join without touching a transient. Segment B opens on the drop and
# gets no fade at all: fading into a downbeat is how a drop is made to sound
# like a mistake.
ffmpeg -v error -i "$WORK/stretched.wav" \
  -af "atrim=start=$(python3 -c "print(f'{${WINDOW}/${FACTOR}:.4f}')"):duration=${A_END},asetpts=N/SR/TB,afade=t=out:st=$(python3 -c "print(f'{${A_END} - 0.015:.4f}')"):d=0.015" \
  -y "$WORK/a.wav"
ffmpeg -v error -i "$WORK/stretched.wav" \
  -af "atrim=start=${B_START}:duration=${B_LEN},asetpts=N/SR/TB" \
  -y "$WORK/b.wav"

ffmpeg -v error -i "$WORK/a.wav" -i "$WORK/b.wav" \
  -filter_complex '[0:a][1:a]concat=n=2:v=0:a=1' -ac 2 -ar 48000 -y "$OUT"

printf '%s  %s s\n' "$OUT" \
  "$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$OUT")"
