#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

if output=$(scripts/performance-query-compare.sh 2>&1); then
  echo "query comparison must require baseline and candidate directories" >&2
  exit 1
fi
if [[ $output != *"usage: scripts/performance-query-compare.sh BASELINE_DIR CANDIDATE_DIR"* ]]; then
  echo "missing query comparison usage text" >&2
  exit 1
fi

fixture_root=$(mktemp -d /tmp/reprise-performance-query-compare.XXXXXX)
trap 'rm -r -- "$fixture_root"' EXIT
for side in baseline candidate; do
  mkdir -p "$fixture_root/$side"
done
printf '%s\n' '{"commit":"before","track_counts":[100000]}' \
  >"$fixture_root/baseline/manifest.json"
printf '%s\n' '{"commit":"after","track_counts":[100000]}' \
  >"$fixture_root/candidate/manifest.json"
printf '%s\n' '{
  "generated_tracks": 100000,
  "database_bytes": 1000,
  "startup": {"median_us": 100},
  "first_window": {"median_us": 50},
  "middle_window": {"median_us": 100},
  "final_window": {"median_us": 200},
  "album_final_window": {"median_us": 500},
  "filtered_count": {"median_us": 120},
  "library_stats": {"median_us": 60},
  "playback_ids": {"median_us": 300},
  "write_batch_rows": 10000,
  "insert_batch": {"median_us": 1000},
  "metadata_update_batch": {"median_us": 800},
  "hide_batch": {"median_us": 400},
  "restore_batch": {"median_us": 500},
  "title_window_query_plan": {
    "details": ["SCAN tracks", "USE TEMP B-TREE FOR ORDER BY"],
    "uses_temp_sort": true,
    "index_name": null
  },
  "album_window_query_plan": {
    "details": ["SCAN tracks", "USE TEMP B-TREE FOR ORDER BY"],
    "uses_temp_sort": true,
    "index_name": null
  }
}' >"$fixture_root/baseline/queries-100000.json"
printf '%s\n' '{
  "generated_tracks": 100000,
  "database_bytes": 1100,
  "startup": {"median_us": 102},
  "first_window": {"median_us": 5},
  "middle_window": {"median_us": 10},
  "final_window": {"median_us": 20},
  "album_final_window": {"median_us": 20},
  "filtered_count": {"median_us": 126},
  "library_stats": {"median_us": 63},
  "playback_ids": {"median_us": 30},
  "write_batch_rows": 10000,
  "insert_batch": {"median_us": 1120},
  "metadata_update_batch": {"median_us": 1000},
  "hide_batch": {"median_us": 600},
  "restore_batch": {"median_us": 800},
  "title_window_query_plan": {
    "details": ["SCAN tracks USING INDEX idx_tracks_present_title_nocase"],
    "uses_temp_sort": false,
    "index_name": "idx_tracks_present_title_nocase"
  },
  "album_window_query_plan": {
    "details": ["SCAN tracks USING INDEX idx_tracks_present_album_order"],
    "uses_temp_sort": false,
    "index_name": "idx_tracks_present_album_order"
  }
}' >"$fixture_root/candidate/queries-100000.json"

comparison=$(scripts/performance-query-compare.sh \
  "$fixture_root/baseline" "$fixture_root/candidate")
jq -e '
  .schema_version == 4
  and .baseline_commit == "before"
  and .candidate_commit == "after"
  and .tracks[0].generated_tracks == 100000
  and .tracks[0].database_bytes.delta_percent == 10
  and .tracks[0].database_open.delta_percent == 2
  and .tracks[0].first_window.delta_percent == -90
  and .tracks[0].middle_window.delta_percent == -90
  and .tracks[0].final_window.delta_percent == -90
  and .tracks[0].album_final_window.delta_percent == -96
  and .tracks[0].filtered_count.delta_percent == 5
  and .tracks[0].library_stats.delta_percent == 5
  and .tracks[0].playback_ids.delta_percent == -90
  and .tracks[0].write_batch_rows == 10000
  and .tracks[0].insert_batch.delta_percent == 12
  and .tracks[0].metadata_update_batch.delta_percent == 25
  and .tracks[0].hide_batch.delta_percent == 50
  and .tracks[0].restore_batch.delta_percent == 60
  and .tracks[0].query_plan.before.uses_temp_sort
  and (.tracks[0].query_plan.after.uses_temp_sort | not)
  and .tracks[0].query_plan.after.index_name == "idx_tracks_present_title_nocase"
  and .tracks[0].album_query_plan.before.uses_temp_sort
  and (.tracks[0].album_query_plan.after.uses_temp_sort | not)
  and .tracks[0].album_query_plan.after.index_name == "idx_tracks_present_album_order"
' <<<"$comparison" >/dev/null

jq '.write_batch_rows = 9999' "$fixture_root/candidate/queries-100000.json" \
  >"$fixture_root/candidate/queries-100000-wrong.json"
mv "$fixture_root/candidate/queries-100000-wrong.json" \
  "$fixture_root/candidate/queries-100000.json"
if output=$(scripts/performance-query-compare.sh \
  "$fixture_root/baseline" "$fixture_root/candidate" 2>&1); then
  echo "query comparison must reject different write batch sizes" >&2
  exit 1
fi
if [[ $output != *"query benchmark reports use different write batch sizes for 100000 tracks"* ]]; then
  echo "query comparison did not explain the write-batch mismatch" >&2
  exit 1
fi
jq '.write_batch_rows = 10000' "$fixture_root/candidate/queries-100000.json" \
  >"$fixture_root/candidate/queries-100000-wrong.json"
mv "$fixture_root/candidate/queries-100000-wrong.json" \
  "$fixture_root/candidate/queries-100000.json"

jq '.generated_tracks = 99999' "$fixture_root/candidate/queries-100000.json" \
  >"$fixture_root/candidate/queries-100000-wrong.json"
mv "$fixture_root/candidate/queries-100000-wrong.json" \
  "$fixture_root/candidate/queries-100000.json"
if output=$(scripts/performance-query-compare.sh \
  "$fixture_root/baseline" "$fixture_root/candidate" 2>&1); then
  echo "query comparison must reject a report for the wrong track count" >&2
  exit 1
fi
if [[ $output != *"query benchmark report does not match 100000 tracks"* ]]; then
  echo "query comparison did not explain the track-count mismatch" >&2
  exit 1
fi

echo "Performance query comparison checks passed"
