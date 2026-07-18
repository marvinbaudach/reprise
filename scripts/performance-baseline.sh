#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'EOF'
usage: scripts/performance-baseline.sh OUTPUT_DIR [--quick]

Builds release-profile scalability probes against generated metadata only.
The normal run records query and track-list results for 10,000 and 100,000
tracks. --quick records only the 10,000-track scenario.

OUTPUT_DIR must not already exist. The runner creates it and writes a manifest,
one query JSON report, and one track-list log per scenario. The Git worktree
must be clean so the manifest commit identifies the compiled sources exactly.
EOF
}

if [[ ${1:-} == "--help" ]]; then
  usage
  exit 0
fi
if (( $# < 1 || $# > 2 )); then
  usage >&2
  exit 2
fi

output_dir=$1
mode=${2:-}
if [[ -n $mode && $mode != "--quick" ]]; then
  echo "unknown option: $mode" >&2
  usage >&2
  exit 2
fi
if [[ -e $output_dir ]]; then
  echo "output directory already exists: $output_dir" >&2
  exit 2
fi
if ! command -v jq >/dev/null; then
  echo "jq is required to write the performance manifest" >&2
  exit 2
fi
if [[ -n $(git status --porcelain) ]]; then
  echo "performance baseline requires a clean Git worktree" >&2
  exit 2
fi

mkdir -p -- "$output_dir"
scratch_root=$(mktemp -d /tmp/reprise-performance-baseline.XXXXXX)
trap 'rm -r -- "$scratch_root"' EXIT

sizes=(10000 100000)
if [[ $mode == "--quick" ]]; then
  sizes=(10000)
fi

echo "== Build release probes =="
cargo build --locked --release -p reprise-core --example scalability_baseline
cargo test --locked --release -p reprise-gnome \
  generated_library_scroll_keeps_track_cache_bounded --no-run

sizes_json=$(printf '%s\n' "${sizes[@]}" | jq -s 'map(tonumber)')
jq -n \
  --arg commit "$(git rev-parse HEAD)" \
  --arg rustc "$(rustc --version)" \
  --arg cargo "$(cargo --version)" \
  --arg platform "$(uname -srm)" \
  --arg profile "release" \
  --argjson track_counts "$sizes_json" \
  '{schema_version: 1, commit: $commit, rustc: $rustc, cargo: $cargo,
    platform: $platform, profile: $profile, generated_metadata_only: true,
    track_counts: $track_counts}' > "$output_dir/manifest.json"

for track_count in "${sizes[@]}"; do
  echo "== Query baseline: $track_count tracks =="
  target/release/examples/scalability_baseline \
    --db "$scratch_root/query-$track_count.db" \
    --tracks "$track_count" \
    --iterations 7 > "$output_dir/queries-$track_count.json"

  echo "== Track-list baseline: $track_count tracks =="
  env REPRISE_PERF_TRACKS="$track_count" \
    cargo test --locked --release -p reprise-gnome \
      generated_library_scroll_keeps_track_cache_bounded -- \
      --ignored --nocapture > "$output_dir/track-list-$track_count.log" 2>&1
done

echo "Performance baseline written to $output_dir"
