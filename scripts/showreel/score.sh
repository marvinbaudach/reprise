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
# A composed bed is already on the film's grid: its sections were written at the
# film's own edit times. Measuring its tempo and correcting the estimation error
# would trim half a second off the head and slide every section boundary away
# from the cut it was written for. SHOWREEL_ALIGN=0 leaves such material alone.
if [ "${SHOWREEL_ALIGN:-1}" = 0 ]; then
  FACTOR=1.0
  PHASE=0.0
else
  read -r FACTOR PHASE < <(
    python3 scripts/showreel/align-bed.py "$TRACK" "$BPM" "$DUR" |
      awk '/^atempo/ {f=$2} /^downbeat/ {p=$2} END {print f, p}'
  )
fi
# SHOWREEL_WINDOW overrides which stretch of the track goes under the film.
#
# pick-window maximises correlation with the arc, and correlation has no opinion
# about where a piece of music begins. Against this track it chose 19.5 s, which
# is one and a half seconds into the full section — the film opened at full
# level, in the middle of a phrase, and sounded like a track already playing
# that someone had faded up. The track's own intro runs to 17.5 s and it is
# there to be used: a window that puts the lift on the arc's own full point
# gives the film a beginning instead of an entrance.
if [ -n "${SHOWREEL_WINDOW:-}" ]; then
  START=$SHOWREEL_WINDOW
  MATCH=forced
else
  read -r START MATCH < <(
    python3 scripts/showreel/pick-window.py "$TRACK" "$DUR" "$BPM" |
      awk '/^start/ {s=$2} /^match/ {m=$2} END {print s, m}'
  )
fi
# The window is counted in beats from the file's start; the grid itself sits a
# fraction of a beat in. Both offsets apply, and both shrink with the stretch.
TRIM=$(python3 -c "print(f'{(${START} + ${PHASE}) / ${FACTOR}:.4f}')")
printf 'align: atempo=%s window=%ss phase=%ss trim=%ss match=%s target=%ss\n' \
  "$FACTOR" "$START" "$PHASE" "$TRIM" "$MATCH" "$DUR"

# atempo holds quality between 0.5 and 2.0; anything a generator returns for a
# 100 BPM request lands far inside that, so one stage is enough.
# apad is a courtesy for a track that ends a fraction short, not a licence to
# finish the film in silence: a 133 BPM take stretched to the grid left 45 s of
# music under a 60 s picture and nobody heard about it until the film was cut.
AVAIL=$(python3 -c "
import subprocess
d = float(subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration',
                          '-of', 'default=nw=1:nk=1', '$TRACK'],
                         capture_output=True, text=True).stdout)
print(f'{d / ${FACTOR} - ${TRIM}:.3f}')")
python3 -c "
import sys
if ${AVAIL} < ${DUR} - 1.0:
    sys.exit('score.sh: $TRACK gives ${AVAIL}s of music for a ${DUR}s film -- '
             'the rest would be silence')"

ffmpeg -y -v error -i "$TRACK" \
  -af "atempo=${FACTOR},atrim=start=${TRIM},asetpts=N/SR/TB,apad" \
  -t "$DUR" -ac 2 -ar 48000 "$WORK/fitted.wav"

# The film's shape, applied to the track rather than asked of the generator.
# SHOWREEL_ARC is the depth: 0 leaves the cue at one level, 1 is the arc as
# pick-window scored it. Off by default, because a track that already breathes
# does not want a second hand on the fader.
ARC=${SHOWREEL_ARC:-0}
GAIN=
if [ "$ARC" != 0 ]; then
  GAIN="volume=eval=frame:volume='$(python3 scripts/showreel/arc-gain.py "$DUR" "$ARC")',"
  printf 'arc: depth=%s\n' "$ARC"
fi

# The drop. The arc already ducks hard into the handover and slams back on the
# cut to the phone; SHOWREEL_DROP adds the other half of that gesture, which is
# the one an ear reads as tension: over the four beats before the release the
# music crossfades into a heavily lowpassed copy of itself, so the top end
# closes down, and on the release frame it is dry again. Level and filter move
# together, which is what a build is.
#
# The release is not a number typed in here. It is the breakpoint where the arc
# returns to full after the handover's dip, so if the edit moves, this moves.
DROP=${SHOWREEL_DROP:-0}
BUILD=
if [ "$DROP" != 0 ]; then
  read -r T0 T1 < <(python3 -c "
import importlib.util
spec = importlib.util.spec_from_file_location('pw', 'scripts/showreel/pick-window.py')
pw = importlib.util.module_from_spec(spec); spec.loader.exec_module(pw)
steps = pw.arc_steps($DUR)
dip = min(range(len(steps)), key=lambda i: steps[i][1] if 0 < i < len(steps) - 1 else 9)
release = steps[dip + 1][0]
print(f'{release - 2.4:.4f} {release:.4f}')")
  RAMP="if(lt(t,${T0}),0,if(lt(t,${T1}),(t-${T0})/(${T1}-${T0}),0))"
  BUILD="asplit[dry][wet];[wet]lowpass=f=320,volume=eval=frame:volume='${RAMP}'[wetg];\
[dry]volume=eval=frame:volume='1-(${RAMP})'[dryg];[dryg][wetg]amix=inputs=2:normalize=0,"
  printf 'drop: filter closes %s -> %s, dry on the release\n' "$T0" "$T1"
fi

# A hard cut into music reads as a mistake; a long fade reads as an apology.
# 60 ms in, and out across the end card only.
ffmpeg -y -v error -i "$WORK/fitted.wav" \
  -filter_complex "[0:a]${BUILD}${GAIN}afade=t=in:st=0:d=0.06,afade=t=out:st=$(python3 -c "print(max(0, $DUR - 1.2))"):d=1.2[a]" \
  -map '[a]' "$WORK/shaped.wav"

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
