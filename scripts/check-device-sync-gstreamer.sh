#!/usr/bin/env bash
set -euo pipefail

required=(
  uridecodebin
  audioconvert
  audioresample
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
  echo "Missing GStreamer factories required for Android MP3 sync: ${missing[*]}" >&2
  echo "Install GStreamer Good Plug-ins (gst-plugins-good on Arch Linux)." >&2
  exit 1
fi

echo "Android MP3 sync GStreamer factories are available"
