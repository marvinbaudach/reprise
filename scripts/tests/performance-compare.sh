#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

if output=$(scripts/performance-compare.sh 2>&1); then
  echo "performance comparison must require baseline and candidate directories" >&2
  exit 1
fi
if [[ $output != *"usage: scripts/performance-compare.sh BASELINE_DIR CANDIDATE_DIR"* ]]; then
  echo "missing performance comparison usage text" >&2
  exit 1
fi

fixture_root=$(mktemp -d /tmp/reprise-performance-compare.XXXXXX)
trap 'rm -r -- "$fixture_root"' EXIT
for side in baseline candidate; do
  mkdir -p "$fixture_root/$side"
done
printf '%s\n' '{"commit":"before","track_counts":[10000]}' >"$fixture_root/baseline/manifest.json"
printf '%s\n' '{"commit":"after","track_counts":[10000]}' >"$fixture_root/candidate/manifest.json"
printf '%s\n' '{"spawn_to_accessible_window":{"median_us":1000}}' >"$fixture_root/baseline/startup-10000.json"
printf '%s\n' '{"spawn_to_accessible_window":{"median_us":800}}' >"$fixture_root/candidate/startup-10000.json"
printf '%s\n' '{"rss_delta":{"median_bytes":200000}}' >"$fixture_root/baseline/queue-memory-10000.json"
printf '%s\n' '{"rss_delta":{"median_bytes":160000}}' >"$fixture_root/candidate/queue-memory-10000.json"
printf '%s\n' '{"action_to_changed_snapshot":{"median_us":5000}}' >"$fixture_root/baseline/scroll-10000.json"
printf '%s\n' '{"action_to_changed_snapshot":{"median_us":4000}}' >"$fixture_root/candidate/scroll-10000.json"
printf '%s\n' '{"final_window":{"median_us":100}}' >"$fixture_root/baseline/seed-10000.json"
printf '%s\n' '{"final_window":{"median_us":80}}' >"$fixture_root/candidate/seed-10000.json"
printf '%s\n' '{"row_widgets":24,"cell_widgets":168,"cached_tracks":400}' >"$fixture_root/baseline/runtime-10000-1.json"
printf '%s\n' '{"row_widgets":20,"cell_widgets":140,"cached_tracks":200}' >"$fixture_root/candidate/runtime-10000-1.json"

comparison=$(scripts/performance-compare.sh \
  "$fixture_root/baseline" "$fixture_root/candidate")
jq -e '
  .baseline_commit == "before"
  and .candidate_commit == "after"
  and .tracks[0].generated_tracks == 10000
  and .tracks[0].startup.delta_percent == -20
  and .tracks[0].final_window.delta_percent == -20
  and .tracks[0].queue_rss.delta_percent == -20
  and .tracks[0].scroll.delta_percent == -20
  and .tracks[0].gtk.row_widgets.before == 24
  and .tracks[0].gtk.row_widgets.after == 20
' <<<"$comparison" >/dev/null

echo "Performance comparison checks passed"
