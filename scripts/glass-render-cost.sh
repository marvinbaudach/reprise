#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'EOF'
usage: scripts/glass-render-cost.sh OUTPUT_DIR

Runs a paired baseline/glass measurement with 120 frames per mode and writes
raw samples plus a fail-closed p95 summary. It also measures the album glow's
one-time 1200-to-32 px downscale/blur and cached lookup cost. The frame metric
is CPU wall time from GDK before-paint to after-paint, not GPU completion time.
EOF
}

if [[ ${1:-} == --help ]]; then
  usage
  exit 0
fi
if [[ $# -ne 1 ]]; then
  usage >&2
  exit 2
fi

output_dir=$1
if [[ -e $output_dir ]]; then
  echo "output directory already exists: $output_dir" >&2
  exit 2
fi
for command in cargo dbus-run-session xvfb-run jq mktemp; do
  if ! command -v "$command" >/dev/null; then
    echo "required command is unavailable: $command" >&2
    exit 2
  fi
done

scratch_root=$(mktemp -d /tmp/reprise-glass-cost.XXXXXX)
trap 'rm -rf "$scratch_root"' EXIT
mkdir -p "$output_dir"

env XDG_DATA_HOME="$scratch_root/glow-data" XDG_CACHE_HOME="$scratch_root/glow-cache" \
  cargo run --quiet --release -p reprise-core --example album-glow-cost \
  >"$output_dir/album-glow.json"
cargo build --quiet --release -p reprise-gnome
cargo build --quiet --release -p reprise-core --example scalability_baseline

run_mode() {
  local mode=$1
  local profile="$scratch_root/$mode"
  mkdir -p "$profile/data/reprise" "$profile/cache"
  target/release/examples/scalability_baseline \
    --db "$profile/data/reprise/reprise.db" --tracks 10000 --iterations 1 \
    >"$output_dir/$mode-seed.json"
  dbus-run-session -- xvfb-run -a env \
    XDG_DATA_HOME="$profile/data" XDG_CACHE_HOME="$profile/cache" \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
    GSK_RENDERER=gl REPRISE_GLASS_PERF_MODE="$mode" \
    REPRISE_GLASS_PERF_REPORT="$output_dir/$mode.json" \
    REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=5 \
    target/release/reprise >"$output_dir/$mode.log" 2>&1
  if [[ ! -s $output_dir/$mode.json ]]; then
    echo "$mode run did not produce a frame report" >&2
    tail -n 80 "$output_dir/$mode.log" >&2 || true
    exit 1
  fi
}

run_mode baseline
run_mode glass

jq -n --slurpfile baseline "$output_dir/baseline.json" \
  --slurpfile glass "$output_dir/glass.json" \
  -f scripts/glass-render-summary.jq >"$output_dir/summary.json"

if ! jq -e '.pass == true' "$output_dir/summary.json" >/dev/null; then
  echo "glass render-cost budget failed" >&2
  jq . "$output_dir/summary.json" >&2
  exit 1
fi
jq . "$output_dir/summary.json"
jq . "$output_dir/album-glow.json"
