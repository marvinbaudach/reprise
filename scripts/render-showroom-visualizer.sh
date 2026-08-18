#!/usr/bin/env bash
# Build the Showroom visualizer band track from decoded audio.
#
# Prepared PCM skips ffmpeg:
#   scripts/render-showroom-visualizer.sh /path/to/mono-44100.f32
# Source audio is cut and decoded first; the optional second argument is the
# ffmpeg start timestamp (default 00:00:00):
#   scripts/render-showroom-visualizer.sh /path/to/song.flac 00:01:36
#
# The f32 input format is raw little-endian mono at 44.1 kHz, exactly what
# REPRISE_VIS_PCM expects. Only visualizer-track.bin is retained; CSV and PCM
# intermediates stay under target/ and are removed when the command exits.

set -euo pipefail

readonly SAMPLE_DURATION_SECONDS=6
readonly SAMPLE_RATE=44100
readonly DEFAULT_START_TIME='00:00:00'
readonly SCRATCH_PREFIX='showroom-visualizer'

usage() {
  echo "Usage: $0 INPUT[.f32] [START_TIME]" >&2
  exit 2
}

[[ $# -ge 1 && $# -le 2 ]] || usage

readonly input_path=$1
readonly start_time=${2:-$DEFAULT_START_TIME}
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
readonly script_dir
repository_root=$(cd -- "$script_dir/.." && pwd -P)
readonly repository_root
readonly scratch_parent="$repository_root/target/$SCRATCH_PREFIX"

[[ -f $input_path ]] || {
  echo "Input does not exist: $input_path" >&2
  exit 1
}

mkdir -p -- "$scratch_parent"
scratch_dir=$(mktemp -d "$scratch_parent/render.XXXXXX")
readonly scratch_dir

cleanup() {
  if [[ -d $scratch_dir && $scratch_dir == "$scratch_parent"/render.* ]]; then
    rm -rf -- "$scratch_dir"
  fi
}
trap cleanup EXIT

pcm_path=''
if [[ $input_path == *.f32 ]]; then
  input_dir=$(cd -- "$(dirname -- "$input_path")" && pwd -P)
  pcm_path="$input_dir/$(basename -- "$input_path")"
  echo "Using prepared PCM: $pcm_path"
else
  command -v ffmpeg >/dev/null 2>&1 || {
    echo 'ffmpeg is required for non-.f32 input' >&2
    exit 1
  }
  pcm_path="$scratch_dir/source.f32"
  ffmpeg -hide_banner -loglevel error -y \
    -ss "$start_time" -t "$SAMPLE_DURATION_SECONDS" -i "$input_path" \
    -ac 1 -ar "$SAMPLE_RATE" -f f32le "$pcm_path"
fi
readonly pcm_path

(
  cd -- "$repository_root"
  REPRISE_VIS_PCM="$pcm_path" \
    REPRISE_VIS_OUT="$scratch_dir/extract" \
    cargo test -p reprise-gnome dump_song_visualizer_stream -- --ignored --nocapture
)

python3 - \
  "$scratch_dir/extract/bands.csv" \
  "$scratch_dir/extract/pressure.csv" \
  "$repository_root/showroom/public/media/showroom/visualizer-track.bin" <<'PY'
from pathlib import Path
import os
import sys

BAND_COUNT = 64
BYTES_PER_FRAME = BAND_COUNT + 1
FRAME_COUNT = 259
SEAM_FRAME_COUNT = 11
PRESSURE_VALUE_COUNT = 3
KICK_INDEX = 0
UINT8_MAX = 255

bands_path, pressure_path, output_path = map(Path, sys.argv[1:])
band_rows = [
    [float(value) for value in line.split(",")]
    for line in bands_path.read_text().splitlines()
]
pressure_rows = [
    [float(value) for value in line.split(",")]
    for line in pressure_path.read_text().splitlines()
]

if len(band_rows) < FRAME_COUNT or len(pressure_rows) < FRAME_COUNT:
    raise SystemExit(
        f"extractor produced {len(band_rows)} band and {len(pressure_rows)} pressure rows; "
        f"need {FRAME_COUNT}"
    )
if any(len(row) != BAND_COUNT for row in band_rows[:FRAME_COUNT]):
    raise SystemExit(f"every band row must contain {BAND_COUNT} values")
if any(len(row) != PRESSURE_VALUE_COUNT for row in pressure_rows[:FRAME_COUNT]):
    raise SystemExit("every pressure row must contain kick, impact and aura")

source_frames = [
    (*band_rows[index], pressure_rows[index][KICK_INDEX])
    for index in range(FRAME_COUNT)
]
seam_start = FRAME_COUNT - SEAM_FRAME_COUNT

def loop_frame(index: int) -> tuple[float, ...]:
    if index < seam_start:
        return source_frames[index]
    seam_index = index - seam_start
    mix = (seam_index + 1) / SEAM_FRAME_COUNT
    tail = source_frames[index]
    head = source_frames[seam_index]
    return tuple(
        tail_value + (head_value - tail_value) * mix
        for tail_value, head_value in zip(tail, head, strict=True)
    )

def quantize(value: float) -> int:
    return max(0, min(UINT8_MAX, int(value * UINT8_MAX + 0.5)))

track = bytes(
    quantize(value)
    for index in range(FRAME_COUNT)
    for value in loop_frame(index)
)
expected_size = FRAME_COUNT * BYTES_PER_FRAME
if len(track) != expected_size:
    raise SystemExit(f"packed {len(track)} bytes; expected {expected_size}")

output_path.parent.mkdir(parents=True, exist_ok=True)
temporary_path = output_path.with_suffix(f"{output_path.suffix}.tmp")
temporary_path.write_bytes(track)
os.replace(temporary_path, output_path)
print(f"wrote {output_path}: {len(track)} bytes ({FRAME_COUNT} frames x {BYTES_PER_FRAME} bytes)")
PY
