#!/usr/bin/env bash
set -euo pipefail

mapfile -t tests < <(
  cargo test -p reprise-gnome -- --ignored --list \
    | sed -n 's/: test$//p'
)

if [[ ${#tests[@]} -eq 0 ]]; then
  echo "No ignored display tests were discovered" >&2
  exit 1
fi

for test in "${tests[@]}"; do
  data_home=$(mktemp -d)
  cache_home=$(mktemp -d)
  echo "== display test: $test =="
  dbus-run-session -- xvfb-run -a env \
    XDG_DATA_HOME="$data_home" XDG_CACHE_HOME="$cache_home" \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
    cargo test -p reprise-gnome "$test" -- --ignored --exact
done
