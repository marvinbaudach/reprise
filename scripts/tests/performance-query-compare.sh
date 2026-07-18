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
  "playback_ids": {"median_us": 300},
  "title_window_query_plan": {
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
  "playback_ids": {"median_us": 30},
  "title_window_query_plan": {
    "details": ["SCAN tracks USING INDEX idx_tracks_present_title_nocase"],
    "uses_temp_sort": false,
    "index_name": "idx_tracks_present_title_nocase"
  }
}' >"$fixture_root/candidate/queries-100000.json"

comparison=$(scripts/performance-query-compare.sh \
  "$fixture_root/baseline" "$fixture_root/candidate")
jq -e '
  .schema_version == 1
  and .baseline_commit == "before"
  and .candidate_commit == "after"
  and .tracks[0].generated_tracks == 100000
  and .tracks[0].database_bytes.delta_percent == 10
  and .tracks[0].database_open.delta_percent == 2
  and .tracks[0].first_window.delta_percent == -90
  and .tracks[0].middle_window.delta_percent == -90
  and .tracks[0].final_window.delta_percent == -90
  and .tracks[0].playback_ids.delta_percent == -90
  and .tracks[0].query_plan.before.uses_temp_sort
  and (.tracks[0].query_plan.after.uses_temp_sort | not)
  and .tracks[0].query_plan.after.index_name == "idx_tracks_present_title_nocase"
' <<<"$comparison" >/dev/null

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
