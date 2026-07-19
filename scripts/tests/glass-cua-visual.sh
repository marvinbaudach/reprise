#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

runner=scripts/glass-cua-visual.sh

if [[ ! -x $runner ]]; then
  echo "$runner must exist and be executable" >&2
  exit 1
fi

for required_pattern in \
  'XDG_RUNTIME_DIR=' \
  'dbus-run-session' \
  'XDG_DATA_HOME=' \
  'XDG_CACHE_HOME=' \
  'XDG_CONFIG_HOME=' \
  'GDK_BACKEND=x11' \
  'WAYLAND_DISPLAY=' \
  'REPRISE_AUDIO_SINK=fakesink' \
  'at-spi2-registryd' \
  'cua_driver get_window_state' \
  'cua_driver scroll' \
  'delivery_mode: "foreground"' \
  'assert_scroll_delivered' \
  'assert_glass_region_changed' \
  '--arg by "$by"' \
  'by: $by' \
  'glass pixels stayed unchanged' \
  'glass-rmse.tsv' \
  'cmp -s' \
  'run_position bottom' \
  'run_position top' \
  'synthetic albums' \
  'Album 00119' \
  '"Albums" "$position-tracks-ready"' \
  'albums-under-header' \
  'down 50 page "$position-albums-at-end"' \
  'verify_glass_regions "$position"' \
  'albums-at-end'; do
  if ! rg --quiet --fixed-strings -- "$required_pattern" "$runner"; then
    echo "$runner is missing visual-CUA contract: $required_pattern" >&2
    exit 1
  fi
done

for calibrated_scroll in \
  'down 1 line "$position-albums-under-header"' \
  'down 50 page "$position-albums-at-end"' \
  'up 1 line "$position-albums-above-end"'; do
  if ! rg --quiet --fixed-strings "$calibrated_scroll" "$runner"; then
    echo "$runner is missing calibrated Glass scroll: $calibrated_scroll" >&2
    exit 1
  fi
done

if rg --quiet --fixed-strings 'down 80 "$position-albums-at-end"' "$runner"; then
  echo "$runner must keep CUA page-scroll amounts within the driver schema" >&2
  exit 1
fi

resize_line=$(rg -n --fixed-strings \
  'cua_resize_window "$app_pid" "$window_id" 1440 800' "$runner" | cut -d: -f1)
ready_line=$(rg -n --fixed-strings \
  '"Albums" "$position-tracks-ready"' "$runner" | cut -d: -f1)
snapshot_line=$(rg -n --fixed-strings \
  'start_path=$(capture_state "$app_pid" "$window_id" "$position-tracks-start")' \
  "$runner" | cut -d: -f1)
click_line=$(rg -n --fixed-strings \
  'cua_click_label "$app_pid" "$window_id" "Albums"' "$runner" | cut -d: -f1)
if ! (( resize_line < ready_line \
  && ready_line < snapshot_line \
  && snapshot_line < click_line )); then
  echo "$runner must wait for the Albums accessibility tree before evidence and input" >&2
  exit 1
fi

for fixture_order in \
  'first_album="Album 00119"' \
  'last_album="Album 00000"'; do
  if ! rg --quiet --fixed-strings "$fixture_order" "$runner"; then
    echo "$runner has the wrong Recently-added fixture order: $fixture_order" >&2
    exit 1
  fi
done

if rg --quiet --fixed-strings 'DISPLAY=:0' "$runner"; then
  echo "$runner must never fall back to the live desktop" >&2
  exit 1
fi

help=$($runner --help)
for required_text in 'synthetic albums' 'player bar at the bottom' \
  'player bar at the top' 'retains PNG and JSON evidence'; do
  if [[ $help != *"$required_text"* ]]; then
    echo "$runner help is missing: $required_text" >&2
    exit 1
  fi
done

echo "Glass CUA visual runner contract passed"
