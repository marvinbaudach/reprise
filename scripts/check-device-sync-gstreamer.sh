#!/usr/bin/env bash
set -euo pipefail

required=(
  uridecodebin
  audioconvert
  audioresample
  opusenc
  oggmux
  lamemp3enc
  id3v2mux
  filesink
)
missing=()

for factory in "${required[@]}"; do
  if ! gst-inspect-1.0 --exists "$factory"; then
    missing+=("$factory")
  fi
done

if ((${#missing[@]} > 0)); then
  echo "Missing GStreamer factories required for Android Opus/MP3 sync: ${missing[*]}" >&2
  echo "Install the GStreamer base and good plug-in sets for this distribution." >&2
  exit 1
fi

echo "Android Opus/MP3 sync GStreamer factories are available"
