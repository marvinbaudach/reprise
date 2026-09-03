#!/usr/bin/env bash
# Encode the finished cut into the ladder the showroom ships.
#
# The bed is carried in every output. The page still starts the film muted —
# no browser autoplays sound and no landing page should — but the track has to
# be there for the reader who turns it on.
#
# Two codecs, because neither covers the field alone: VP9 carries the flat UI
# gradients at roughly half the bytes of H.264, and H.264 is the fallback for
# the Safari versions that will not take VP9 in an MP4-less <video>.
set -euo pipefail

SRC=${1:-$HOME/Videos/reprise-showreel/reprise-showreel-58s-scored.mp4}
# public/, not media/: Vite copies public/ into dist and pages.yml uploads dist,
# so this directory is what decides whether the film is in the deploy at all.
# The film was parked outside it while it was unfinished; it is on the page now.
OUT=${2:-$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)/showroom/public/media/showreel}
# Shot 02 — Podcasts, at 7.5-12.0 s in the 58.2 s cut — where the whole window is
# lit and the callout is legible. The hook frame looked stronger in the cut and
# worse as a still: its scrim dims the UI.
POSTER_AT=${POSTER_AT:-9.50}

[[ -f $SRC ]] || { echo "no source: $SRC" >&2; exit 1; }
mkdir -p "$OUT"
log=${SCRATCH:-/tmp}/encode-web.log
: >"$log"

# H.264: high profile, yuv420p and faststart so the moov atom is at the front
# and the browser can start on the first bytes instead of the whole file.
h264() { # width crf name
  ffmpeg -y -v error -i "$SRC" \
    -vf "scale=$1:-2:flags=lanczos" \
    -c:v libx264 -profile:v high -pix_fmt yuv420p \
    -crf "$2" -preset slow -g 60 -movflags +faststart \
    -c:a aac -b:a 128k -ar 48000 \
    "$OUT/$3.mp4" >>"$log" 2>&1
}

# VP9 in two passes: single-pass CRF leaves VP9 well short of what it can do at
# this bitrate, and the film is short enough that the second pass is cheap.
vp9() { # width crf name
  local pass="${SCRATCH:-/tmp}/vp9-$3"
  ffmpeg -y -v error -i "$SRC" -an -vf "scale=$1:-2:flags=lanczos" \
    -c:v libvpx-vp9 -b:v 0 -crf "$2" -row-mt 1 -tile-columns 2 \
    -pass 1 -passlogfile "$pass" -f null /dev/null >>"$log" 2>&1
  ffmpeg -y -v error -i "$SRC" -vf "scale=$1:-2:flags=lanczos" \
    -c:v libvpx-vp9 -b:v 0 -crf "$2" -row-mt 1 -tile-columns 2 \
    -pass 2 -passlogfile "$pass" -c:a libopus -b:a 96k "$OUT/$3.webm" >>"$log" 2>&1
  rm -f "$pass"-*.log
}

h264 1920 24 showreel-1080
h264 1280 26 showreel-720
vp9  1920 33 showreel-1080
vp9  1280 35 showreel-720

# The poster is the frame the page shows before playback and the one a
# link preview picks up, so it comes from the cut itself rather than a restage.
ffmpeg -y -v error -ss "$POSTER_AT" -i "$SRC" -frames:v 1 \
  -vf "scale=1920:-2:flags=lanczos" -q:v 2 "$OUT/showreel-poster.jpg" >>"$log" 2>&1
ffmpeg -y -v error -ss "$POSTER_AT" -i "$SRC" -frames:v 1 \
  -vf "scale=1920:-2:flags=lanczos" -c:v libwebp -quality 82 "$OUT/showreel-poster.webp" >>"$log" 2>&1

for f in "$OUT"/showreel-*; do
  printf '%8s  %s\n' "$(du -h "$f" | cut -f1)" "$(basename "$f")"
done
