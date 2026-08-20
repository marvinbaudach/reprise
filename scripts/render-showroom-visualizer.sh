#!/bin/bash
# Renders the showroom visualizer asset from PCM audio.
#
# Usage:
#   ./scripts/render-showroom-visualizer.sh <pcm-file> [output-duration-seconds]
#
# The PCM file must be raw mono f32 44.1 kHz. The script:
#   1. Runs the extractor test to quantize bands and kick
#   2. Trims to the requested duration (default 6 seconds)
#   3. Blends the tail into the head for seamless looping
#   4. Writes the binary asset to showroom/public/media/showroom/visualizer-track.bin

set -e

script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(dirname "$script_dir")"

PCM_FILE="${1:?Usage: render-showroom-visualizer.sh <pcm-file> [duration-seconds]}"
DURATION_SECONDS="${2:-6}"
SAMPLE_RATE=44100
CHUNK_SAMPLES=1024
FPS=$(echo "scale=3; $SAMPLE_RATE / $CHUNK_SAMPLES" | bc)

# Validate input file
if [[ ! -f "$PCM_FILE" ]]; then
    echo "Error: PCM file not found: $PCM_FILE" >&2
    exit 1
fi

# Create temporary working directory
WORK_DIR=$(mktemp -d)
trap "rm -rf $WORK_DIR" EXIT

echo "Extracting visualizer data from $PCM_FILE..."
echo "  Duration: ${DURATION_SECONDS}s"
echo "  Sample rate: ${SAMPLE_RATE} Hz"
echo "  Chunk size: ${CHUNK_SAMPLES} samples"
echo "  Frame rate: ~${FPS} fps"

# Run the extractor test
cd "$repo_root"
export REPRISE_VIS_PCM="$PCM_FILE"
export REPRISE_VIS_OUT="$WORK_DIR"

# Build and run the test
cargo test --package reprise-gnome --lib now_playing::song_visualizer::tests::extract_showroom_visualizer_asset \
    --release -- --ignored --nocapture

# Check that the output files were created
if [[ ! -f "$WORK_DIR/bands.u8" ]] || [[ ! -f "$WORK_DIR/kick.u8" ]]; then
    echo "Error: Extractor did not produce output files" >&2
    exit 1
fi

TOTAL_FRAMES=$(( $(wc -c < "$WORK_DIR/kick.u8") ))
TOTAL_DURATION=$(echo "scale=3; $TOTAL_FRAMES / $FPS" | bc)

echo "Extracted $TOTAL_FRAMES frames (~${TOTAL_DURATION}s)"

# Calculate frame count for the requested duration
FRAMES_TO_KEEP=$(printf "%.0f" $(echo "$DURATION_SECONDS * $FPS" | bc))
BLEND_FRAMES=11

if (( FRAMES_TO_KEEP > TOTAL_FRAMES )); then
    echo "Warning: requested ${DURATION_SECONDS}s but only ${TOTAL_DURATION}s available; using full duration"
    FRAMES_TO_KEEP=$TOTAL_FRAMES
fi

# Python script to blend and pack the asset
python3 - "$WORK_DIR" "$DURATION_SECONDS" "$FRAMES_TO_KEEP" "$BLEND_FRAMES" "$repo_root" "$FPS" << 'INNER_PYTHON'
import sys

work_dir = sys.argv[1]
duration = float(sys.argv[2])
frames_to_keep = int(sys.argv[3])
blend_frames = int(sys.argv[4])
repo_root = sys.argv[5]
fps = float(sys.argv[6])

# Read quantized bands and kick
with open(f"{work_dir}/bands.u8", "rb") as f:
    bands_raw = f.read()
with open(f"{work_dir}/kick.u8", "rb") as f:
    kick_raw = f.read()

BAND_COUNT = 64
FRAME_SIZE = BAND_COUNT + 1  # 64 bands + 1 kick

total_frames = len(kick_raw)
print(f"Total frames available: {total_frames}")
print(f"Frames to keep: {frames_to_keep}")

# Extract frames to keep
frames = []
for i in range(frames_to_keep):
    band_start = i * BAND_COUNT
    band_end = band_start + BAND_COUNT
    if band_end <= len(bands_raw):
        kick = kick_raw[i] if i < len(kick_raw) else 0
        frame = bands_raw[band_start:band_end] + bytes([kick])
        frames.append(frame)

print(f"Kept {len(frames)} frames")

# Blend tail into head for seamless looping
if blend_frames > 0 and len(frames) >= blend_frames * 2:
    tail_start = len(frames) - blend_frames
    print(f"Blending last {blend_frames} frames (frames {tail_start}-{len(frames)-1}) into first {blend_frames} frames")

    for i in range(blend_frames):
        tail_frame = frames[tail_start + i]
        head_frame = frames[i]

        # Blend each band: (head + tail) / 2
        blended = bytearray(FRAME_SIZE)
        for b in range(BAND_COUNT):
            head_val = head_frame[b]
            tail_val = tail_frame[b]
            blended[b] = (int(head_val) + int(tail_val)) // 2
        blended[BAND_COUNT] = (int(head_frame[BAND_COUNT]) + int(tail_frame[BAND_COUNT])) // 2

        frames[i] = bytes(blended)

# Write the asset
output_path = f"{repo_root}/showroom/public/media/showroom/visualizer-track.bin"
with open(output_path, "wb") as f:
    for frame in frames:
        f.write(frame)

output_size = len(frames) * FRAME_SIZE
final_duration = len(frames) / fps
print(f"Wrote {output_path}")
print(f"  Frames: {len(frames)}")
print(f"  Duration: ~{final_duration:.2f}s at {fps:.3f} fps")
print(f"  Size: {output_size} bytes ({output_size / 1024:.1f} KB)")

INNER_PYTHON

echo "✓ Asset rendering complete"
ls -lh "$repo_root/showroom/public/media/showroom/visualizer-track.bin"
