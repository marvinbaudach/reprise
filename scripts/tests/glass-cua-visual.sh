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
  'run_position bottom' \
  'run_position top' \
  'synthetic albums' \
  'Album 00119' \
  'albums-under-header' \
  'albums-at-end'; do
  if ! rg --quiet --fixed-strings "$required_pattern" "$runner"; then
    echo "$runner is missing visual-CUA contract: $required_pattern" >&2
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
