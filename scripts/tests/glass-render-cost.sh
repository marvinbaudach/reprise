#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

if output=$(scripts/glass-render-cost.sh 2>&1); then
  echo "glass render-cost runner must require an output directory" >&2
  exit 1
fi
if [[ $output != *"usage: scripts/glass-render-cost.sh OUTPUT_DIR"* ]]; then
  echo "missing glass render-cost usage text" >&2
  exit 1
fi

help=$(scripts/glass-render-cost.sh --help)
for required in "120 frames" "baseline" "glass" "p95" "album glow"; do
  if [[ $help != *"$required"* ]]; then
    echo "glass render-cost help must mention: $required" >&2
    exit 1
  fi
done

for required_pattern in \
  'dbus-run-session -- xvfb-run -a env' \
  'XDG_DATA_HOME=' \
  'XDG_CACHE_HOME=' \
  'GDK_BACKEND=x11' \
  'WAYLAND_DISPLAY=' \
  'REPRISE_AUDIO_SINK=fakesink' \
  'REPRISE_GLASS_PERF_MODE=' \
  'scalability_baseline'; do
  if ! rg --quiet --fixed-strings "$required_pattern" scripts/glass-render-cost.sh; then
    echo "glass runner is missing isolation/measurement contract: $required_pattern" >&2
    exit 1
  fi
done

fixture_root=$(mktemp -d /tmp/reprise-glass-summary.XXXXXX)
trap 'rm -rf "$fixture_root"' EXIT
jq -n '{renderer:"GskGLRenderer", samples_us:[range(0;120) | 12000]}' \
  >"$fixture_root/baseline.json"
jq -n '{renderer:"GskGLRenderer", samples_us:[range(0;120) | 14000]}' \
  >"$fixture_root/glass.json"
jq -n --slurpfile baseline "$fixture_root/baseline.json" \
  --slurpfile glass "$fixture_root/glass.json" \
  -f scripts/glass-render-summary.jq >"$fixture_root/summary.json"
jq -e '
  .pass == true and .baseline_frames == 120 and .glass_frames == 120
  and .baseline_p95_us == 12000 and .glass_p95_us == 14000
  and .overhead_p95_us == 2000
' "$fixture_root/summary.json" >/dev/null

echo "Glass render-cost CLI checks passed"
