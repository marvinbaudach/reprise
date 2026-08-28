#!/usr/bin/env bash
# Put a generated track under the cut: align it to the grid, fit it to length,
# master it for the web and mux it in.
#
# The alignment is the point. A model asked for 100 BPM returns something near
# it, and "near" drifts a third of a beat across half a minute — audible as
# sloppiness long before it is audible as wrong. align-bed.py measures what the
# track actually is; this applies the correction.
set -euo pipefail
cd "$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"

TRACK=${1:?usage: score.sh TRACK [VIDEO] [OUT]}
VIDEO=${2:-$HOME/Videos/reprise-showreel/reprise-showreel-31s.mp4}
OUT=${3:-${VIDEO%.mp4}-scored.mp4}
WORK=${SCRATCH:-/tmp}/score-$(basename "${TRACK%.*}")
mkdir -p "$WORK"

DUR=$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$VIDEO")
BPM=${SHOWREEL_BPM:-100}

# Two measurements, two jobs. align-bed says what tempo the track really has
# and where its beat grid sits; pick-window says which stretch of it belongs
# under this film. The generator returns 78 to 88 seconds for a 31 second
# request, so choosing the stretch is not an optimisation, it is required.
read -r FACTOR PHASE < <(
  python3 scripts/showreel/align-bed.py "$TRACK" "$BPM" "$DUR" |
    awk '/^atempo/ {f=$2} /^downbeat/ {p=$2} END {print f, p}'
)
read -r START MATCH < <(
  python3 scripts/showreel/pick-window.py "$TRACK" "$DUR" "$BPM" |
    awk '/^start/ {s=$2} /^match/ {m=$2} END {print s, m}'
)
# The window is counted in beats from the file's start; the grid itself sits a
# fraction of a beat in. Both offsets apply, and both shrink with the stretch.
TRIM=$(python3 -c "print(f'{(${START} + ${PHASE}) / ${FACTOR}:.4f}')")
printf 'align: atempo=%s window=%ss phase=%ss trim=%ss match=%s target=%ss\n' \
  "$FACTOR" "$START" "$PHASE" "$TRIM" "$MATCH" "$DUR"

# atempo holds quality between 0.5 and 2.0; anything a generator returns for a
# 100 BPM request lands far inside that, so one stage is enough.
ffmpeg -y -v error -i "$TRACK" \
  -af "atempo=${FACTOR},atrim=start=${TRIM},asetpts=N/SR/TB,apad" \
  -t "$DUR" -ac 2 -ar 48000 "$WORK/fitted.wav"

# A hard cut into music reads as a mistake; a long fade reads as an apology.
# 60 ms in, and out across the end card only.
ffmpeg -y -v error -i "$WORK/fitted.wav" \
  -af "afade=t=in:st=0:d=0.06,afade=t=out:st=$(python3 -c "print(max(0, $DUR - 1.2))"):d=1.2" \
  "$WORK/shaped.wav"

ffmpeg -v info -i "$WORK/shaped.wav" -af loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json \
  -f null - 2>&1 | sed -n '/^{/,/^}/p' >"$WORK/ln.json"
M=$(python3 -c "
import json; d = json.load(open('$WORK/ln.json'))
print(f\"measured_I={d['input_i']}:measured_TP={d['input_tp']}:measured_LRA={d['input_lra']}\"
      f\":measured_thresh={d['input_thresh']}:offset={d['target_offset']}\", end='')")

# The ceiling sits below the target on purpose: AAC adds inter-sample overshoot
# on top of whatever the limiter allowed, and a first pass at limit=0.891
# measured +0.1 dBTP on the muxed file.
ffmpeg -y -v error -i "$VIDEO" -i "$WORK/shaped.wav" -map 0:v -map 1:a -c:v copy \
  -af "loudnorm=I=-16:TP=-2.0:LRA=11:${M}:linear=true,alimiter=limit=0.7079:level=disabled" \
  -c:a aac -b:a 160k -ar 48000 -movflags +faststart -shortest "$OUT"

printf '%s  %s s\n' "$OUT" "$(ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 "$OUT")"
ffmpeg -v info -i "$OUT" -af loudnorm=print_format=summary -f null - 2>&1 |
  grep -E 'Input (Integrated|True Peak|LRA)'
