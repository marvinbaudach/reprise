#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/performance-compare.sh BASELINE_DIR CANDIDATE_DIR

Compares two complete Reprise performance artifact directories and writes a
stable JSON report. Negative percentage deltas are improvements for timings
and memory. Both manifests must contain identical track counts.
EOF
}

if [[ ${1:-} == "--help" ]]; then
  usage
  exit 0
fi
if (( $# != 2 )); then
  usage >&2
  exit 2
fi

baseline_dir=$1
candidate_dir=$2
for directory in "$baseline_dir" "$candidate_dir"; do
  if [[ ! -d $directory || ! -f $directory/manifest.json ]]; then
    echo "performance artifact directory is incomplete: $directory" >&2
    exit 2
  fi
done

baseline_counts=$(jq -c '.track_counts' "$baseline_dir/manifest.json")
candidate_counts=$(jq -c '.track_counts' "$candidate_dir/manifest.json")
if [[ $baseline_counts != "$candidate_counts" ]]; then
  echo "performance manifests use different track counts" >&2
  exit 2
fi

comparison_rows=$(mktemp /tmp/reprise-performance-comparison.XXXXXX)
trap 'rm -f -- "$comparison_rows"' EXIT

for track_count in $(jq -r '.track_counts[]' "$baseline_dir/manifest.json"); do
  required_files=(
    "startup-$track_count.json"
    "queue-memory-$track_count.json"
    "scroll-$track_count.json"
    "seed-$track_count.json"
    "runtime-$track_count-1.json"
  )
  for filename in "${required_files[@]}"; do
    if [[ ! -f $baseline_dir/$filename || ! -f $candidate_dir/$filename ]]; then
      echo "performance comparison is missing $filename" >&2
      exit 2
    fi
  done

  jq -n \
    --argjson generated_tracks "$track_count" \
    --slurpfile baseline_startup "$baseline_dir/startup-$track_count.json" \
    --slurpfile candidate_startup "$candidate_dir/startup-$track_count.json" \
    --slurpfile baseline_queue "$baseline_dir/queue-memory-$track_count.json" \
    --slurpfile candidate_queue "$candidate_dir/queue-memory-$track_count.json" \
    --slurpfile baseline_scroll "$baseline_dir/scroll-$track_count.json" \
    --slurpfile candidate_scroll "$candidate_dir/scroll-$track_count.json" \
    --slurpfile baseline_query "$baseline_dir/seed-$track_count.json" \
    --slurpfile candidate_query "$candidate_dir/seed-$track_count.json" \
    --slurpfile baseline_gtk "$baseline_dir/runtime-$track_count-1.json" \
    --slurpfile candidate_gtk "$candidate_dir/runtime-$track_count-1.json" '
      def delta($before; $after):
        {before: $before, after: $after,
         delta: ($after - $before),
         delta_percent: (if $before == 0 then null
           else (((($after - $before) * 10000 / $before) | round) / 100) end)};
      {generated_tracks: $generated_tracks,
       startup: delta($baseline_startup[0].spawn_to_accessible_window.median_us;
         $candidate_startup[0].spawn_to_accessible_window.median_us),
       final_window: delta($baseline_query[0].final_window.median_us;
         $candidate_query[0].final_window.median_us),
       queue_rss: delta($baseline_queue[0].rss_delta.median_bytes;
         $candidate_queue[0].rss_delta.median_bytes),
       scroll: delta($baseline_scroll[0].action_to_changed_snapshot.median_us;
         $candidate_scroll[0].action_to_changed_snapshot.median_us),
       gtk: {
         row_widgets: delta($baseline_gtk[0].row_widgets; $candidate_gtk[0].row_widgets),
         cell_widgets: delta($baseline_gtk[0].cell_widgets; $candidate_gtk[0].cell_widgets),
         cached_tracks: delta($baseline_gtk[0].cached_tracks; $candidate_gtk[0].cached_tracks)
       }}
    ' >>"$comparison_rows"
done

jq -s \
  --arg baseline_commit "$(jq -r '.commit' "$baseline_dir/manifest.json")" \
  --arg candidate_commit "$(jq -r '.commit' "$candidate_dir/manifest.json")" \
  '{schema_version: 1, baseline_commit: $baseline_commit,
    candidate_commit: $candidate_commit, tracks: .}' \
  "$comparison_rows"
