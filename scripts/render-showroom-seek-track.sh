#!/usr/bin/env bash
# Build the Showroom seek track from the app's real waveform and spectrogram pipeline.
#
# Usage:
#   scripts/render-showroom-seek-track.sh /path/to/song.flac
#
# The ignored Rust test writes measured intermediates under target/. This script
# packs the duration, waveform peaks and centroid curve into the fixed 2004-byte
# browser asset; only seek-track.bin is retained.

set -euo pipefail

readonly BUCKET_COUNT=1000
readonly DURATION_BYTE_COUNT=4
readonly EXPECTED_BYTE_COUNT=$((DURATION_BYTE_COUNT + 2 * BUCKET_COUNT))
readonly SCRATCH_PREFIX='showroom-seek-track'

usage() {
  echo "Usage: $0 AUDIO_FILE" >&2
  exit 2
}

[[ $# -eq 1 ]] || usage

readonly input_path=$1
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
readonly script_dir
repository_root=$(cd -- "$script_dir/.." && pwd -P)
readonly repository_root
readonly scratch_parent="$repository_root/target/$SCRATCH_PREFIX"
readonly output_path="$repository_root/showroom/public/media/showroom/seek-track.bin"

[[ -f $input_path ]] || {
  echo "Input does not exist: $input_path" >&2
  exit 1
}
input_dir=$(cd -- "$(dirname -- "$input_path")" && pwd -P)
readonly input_dir
source_path="$input_dir/$(basename -- "$input_path")"
readonly source_path

mkdir -p -- "$scratch_parent"
scratch_dir=$(mktemp -d "$scratch_parent/render.XXXXXX")
readonly scratch_dir

cleanup() {
  if [[ -d $scratch_dir && $scratch_dir == "$scratch_parent"/render.* ]]; then
    rm -rf -- "$scratch_dir"
  fi
}
trap cleanup EXIT

(
  cd -- "$repository_root"
  REPRISE_SEEK_SOURCE="$source_path" \
    REPRISE_SEEK_OUT="$scratch_dir/extract" \
    cargo test -p reprise-platform-linux dump_showroom_seek_track_measurement \
      -- --ignored --nocapture
)

python3 - \
  "$scratch_dir/extract/duration-ms.txt" \
  "$scratch_dir/extract/waveform-peaks.bin" \
  "$scratch_dir/extract/centroid-curve.bin" \
  "$output_path" \
  "$BUCKET_COUNT" \
  "$EXPECTED_BYTE_COUNT" <<'PY'
from pathlib import Path
import os
import struct
import sys

duration_path, peaks_path, centroids_path, output_path = map(Path, sys.argv[1:5])
bucket_count = int(sys.argv[5])
expected_byte_count = int(sys.argv[6])

duration_ms = int(duration_path.read_text())
if not 0 < duration_ms <= 0xFFFFFFFF:
    raise SystemExit(f"duration {duration_ms} ms does not fit a positive u32")

peaks = peaks_path.read_bytes()
centroids = centroids_path.read_bytes()
if len(peaks) != bucket_count:
    raise SystemExit(f"extractor produced {len(peaks)} peaks; expected {bucket_count}")
if len(centroids) != bucket_count:
    raise SystemExit(f"extractor produced {len(centroids)} centroids; expected {bucket_count}")

track = struct.pack("<I", duration_ms) + peaks + centroids
if len(track) != expected_byte_count:
    raise SystemExit(f"packed {len(track)} bytes; expected {expected_byte_count}")

output_path.parent.mkdir(parents=True, exist_ok=True)
temporary_path = output_path.with_suffix(f"{output_path.suffix}.tmp")
temporary_path.write_bytes(track)
os.replace(temporary_path, output_path)
minutes, remaining_ms = divmod(duration_ms, 60_000)
seconds = remaining_ms / 1_000
print(
    f"wrote {output_path}: {len(track)} bytes; "
    f"duration {duration_ms} ms ({minutes}:{seconds:06.3f})"
)
PY
