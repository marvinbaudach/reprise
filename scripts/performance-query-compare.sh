#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/performance-query-compare.sh BASELINE_DIR CANDIDATE_DIR

Compares two generated-metadata query benchmark directories and writes stable
JSON. Negative timing deltas are improvements; positive database-byte deltas
are storage costs. Query and committed-write deltas use identical track and
write-batch counts in both reports.
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
    echo "query benchmark directory is incomplete: $directory" >&2
    exit 2
  fi
done

baseline_counts=$(jq -c '.track_counts' "$baseline_dir/manifest.json")
candidate_counts=$(jq -c '.track_counts' "$candidate_dir/manifest.json")
if [[ $baseline_counts != "$candidate_counts" ]]; then
  echo "query benchmark manifests use different track counts" >&2
  exit 2
fi

comparison_rows=$(mktemp /tmp/reprise-performance-query-comparison.XXXXXX)
trap 'rm -f -- "$comparison_rows"' EXIT

for track_count in $(jq -r '.track_counts[]' "$baseline_dir/manifest.json"); do
  baseline_query="$baseline_dir/queries-$track_count.json"
  candidate_query="$candidate_dir/queries-$track_count.json"
  if [[ ! -f $baseline_query || ! -f $candidate_query ]]; then
    echo "query benchmark comparison is missing queries-$track_count.json" >&2
    exit 2
  fi
  for report in "$baseline_query" "$candidate_query"; do
    if ! jq -e --argjson tracks "$track_count" '
      .generated_tracks == $tracks
      and (.database_bytes | type == "number")
      and (.startup.median_us | type == "number")
      and (.first_window.median_us | type == "number")
      and (.middle_window.median_us | type == "number")
      and (.final_window.median_us | type == "number")
      and (.album_final_window.median_us | type == "number")
      and (.filtered_count.median_us | type == "number")
      and (.library_stats.median_us | type == "number")
      and (.playback_ids.median_us | type == "number")
      and (.write_batch_rows | type == "number")
      and (.insert_batch.median_us | type == "number")
      and (.metadata_update_batch.median_us | type == "number")
      and (.hide_batch.median_us | type == "number")
      and (.restore_batch.median_us | type == "number")
      and (.title_window_query_plan.details | type == "array")
      and (.title_window_query_plan.uses_temp_sort | type == "boolean")
      and (.album_window_query_plan.details | type == "array")
      and (.album_window_query_plan.uses_temp_sort | type == "boolean")
    ' "$report" >/dev/null; then
      echo "query benchmark report does not match $track_count tracks: $report" >&2
      exit 2
    fi
  done
  baseline_write_batch=$(jq -r '.write_batch_rows' "$baseline_query")
  candidate_write_batch=$(jq -r '.write_batch_rows' "$candidate_query")
  if [[ $baseline_write_batch != "$candidate_write_batch" ]]; then
    echo "query benchmark reports use different write batch sizes for $track_count tracks" >&2
    exit 2
  fi

  jq -n \
    --argjson generated_tracks "$track_count" \
    --slurpfile baseline "$baseline_query" \
    --slurpfile candidate "$candidate_query" '
      def delta($before; $after):
        {before: $before, after: $after,
         delta: ($after - $before),
         delta_percent: (if $before == 0 then null
           else (((($after - $before) * 10000 / $before) | round) / 100) end)};
      {generated_tracks: $generated_tracks,
       write_batch_rows: $baseline[0].write_batch_rows,
       database_bytes: delta($baseline[0].database_bytes;
         $candidate[0].database_bytes),
       database_open: delta($baseline[0].startup.median_us;
         $candidate[0].startup.median_us),
       first_window: delta($baseline[0].first_window.median_us;
         $candidate[0].first_window.median_us),
       middle_window: delta($baseline[0].middle_window.median_us;
         $candidate[0].middle_window.median_us),
       final_window: delta($baseline[0].final_window.median_us;
         $candidate[0].final_window.median_us),
       album_final_window: delta($baseline[0].album_final_window.median_us;
         $candidate[0].album_final_window.median_us),
       filtered_count: delta($baseline[0].filtered_count.median_us;
         $candidate[0].filtered_count.median_us),
       library_stats: delta($baseline[0].library_stats.median_us;
         $candidate[0].library_stats.median_us),
       playback_ids: delta($baseline[0].playback_ids.median_us;
         $candidate[0].playback_ids.median_us),
       insert_batch: delta($baseline[0].insert_batch.median_us;
         $candidate[0].insert_batch.median_us),
       metadata_update_batch: delta($baseline[0].metadata_update_batch.median_us;
         $candidate[0].metadata_update_batch.median_us),
       hide_batch: delta($baseline[0].hide_batch.median_us;
         $candidate[0].hide_batch.median_us),
       restore_batch: delta($baseline[0].restore_batch.median_us;
         $candidate[0].restore_batch.median_us),
       query_plan: {
         before: $baseline[0].title_window_query_plan,
         after: $candidate[0].title_window_query_plan
       },
       album_query_plan: {
         before: $baseline[0].album_window_query_plan,
         after: $candidate[0].album_window_query_plan
       }}
    ' >>"$comparison_rows"
done

jq -s \
  --arg baseline_commit "$(jq -r '.commit' "$baseline_dir/manifest.json")" \
  --arg candidate_commit "$(jq -r '.commit' "$candidate_dir/manifest.json")" \
  '{schema_version: 4, baseline_commit: $baseline_commit,
    candidate_commit: $candidate_commit, tracks: .}' \
  "$comparison_rows"
