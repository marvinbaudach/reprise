#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
usage: scripts/glass-cua-visual.sh [OUTPUT_ROOT]

Runs an isolated CUA visual pass with 120 synthetic albums. It captures the
album grid at its start, beneath the shared header glass, and at the final
reachable row with the player bar at the bottom and the player bar at the top.
The runner never uses the live desktop or normal XDG profile. It retains PNG and JSON evidence
below OUTPUT_ROOT (default: /tmp/reprise-glass-cua-evidence).
EOF
}

if [[ ${1:-} == --help ]]; then
  usage
  exit 0
fi
if (( $# > 1 )); then
  usage >&2
  exit 2
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# shellcheck source=cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"

output_root=${1:-/tmp/reprise-glass-cua-evidence}
session=reprise-glass-visual
screen_resolution=1600x900x24
binary="$repo_root/target/debug/reprise"
seed_binary="$repo_root/target/debug/examples/scalability_baseline"

required_command() {
  command -v "$1" >/dev/null || {
    echo "required command is unavailable: $1" >&2
    exit 2
  }
}

window_id_from_response() {
  jq -r '
    [.. | objects
      | select(.window_id? != null)
      | select(((.title? // "") + " " + (.class? // "")
        + " " + (.wm_class? // "")) | ascii_downcase | contains("reprise"))
      | .window_id][0] // empty
  '
}

wait_for_window() {
  local pid=$1 response window_id
  for _ in $(seq 1 80); do
    kill -0 "$pid" 2>/dev/null || return 1
    response=$(cua_driver list_windows "$(jq -nc --argjson pid "$pid" '{pid: $pid}')")
    window_id=$(window_id_from_response <<<"$response")
    if [[ -n $window_id ]]; then
      printf '%s\n' "$window_id"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

capture_state() {
  local pid=$1 window_id=$2 stem=$3
  local json_path="$CUA_E2E_OUT_DIR/$stem.json"
  local png_path="$CUA_E2E_OUT_DIR/$stem.png"
  local payload
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg session "$session" \
    --arg screenshot_out_file "$png_path" \
    '{pid: $pid, window_id: $window_id, session: $session,
      screenshot_out_file: $screenshot_out_file}')
  cua_driver get_window_state "$payload" >"$json_path"
  if jq -e '.degraded == true' "$json_path" >/dev/null; then
    echo "CUA snapshot is degraded: $stem" >&2
    return 1
  fi
  if [[ ! -s $png_path ]]; then
    echo "CUA snapshot did not retain PNG evidence: $stem" >&2
    return 1
  fi
  printf '%s\n' "$json_path"
}

assert_scroll_delivered() {
  local action_path=$1

  if ! jq -e '
    (
      .effect == "confirmed"
      or .effect == "unverifiable"
      or (
        .effect? == null
        and .delivery_mode? == "foreground"
        and .verified? == false
        and .code? == null
        and .error? == null
      )
    )
    and .escalation.recommended? == null
  ' "$action_path" >/dev/null 2>&1; then
    echo "CUA scroll was not delivered cleanly: $action_path" >&2
    return 1
  fi
}

scroll_grid() {
  local pid=$1 window_id=$2 direction=$3 amount=$4 stem=$5
  local action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  local after_path before_path payload

  before_path=$(capture_state "$pid" "$window_id" "$stem-before")
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg direction "$direction" \
    --argjson amount "$amount" \
    --arg session "$session" \
    '{pid: $pid, window_id: $window_id, x: 780, y: 460,
      direction: $direction, amount: $amount, by: "page", session: $session,
      delivery_mode: "foreground"}')
  if ! cua_driver scroll "$payload" >"$action_path"; then
    echo "CUA scroll command failed: $stem" >&2
    return 1
  fi
  assert_scroll_delivered "$action_path"
  sleep 0.4
  after_path=$(capture_state "$pid" "$window_id" "$stem-after")
  if cmp -s "${before_path%.json}.png" "${after_path%.json}.png"; then
    echo "CUA scroll left the rendered window unchanged: $stem" >&2
    return 1
  fi
  printf '%s\n' "$after_path"
}

run_position() {
  local position=$1
  local first_album="Album 00119"
  local last_album="Album 00000"
  local profile_root="$scratch_root/profile-$position"
  local app_log="$CUA_E2E_OUT_DIR/$position-app.log"
  local album_path app_pid end_path start_path window_id

  mkdir -p "$profile_root/data/reprise" "$profile_root/cache" "$profile_root/config"
  "$seed_binary" \
    --db "$profile_root/data/reprise/reprise.db" \
    --tracks 120 --iterations 1 \
    >"$CUA_E2E_OUT_DIR/$position-seed.json"

  env \
    XDG_DATA_HOME="$profile_root/data" \
    XDG_CACHE_HOME="$profile_root/cache" \
    XDG_CONFIG_HOME="$profile_root/config" \
    GIO_USE_VFS=local \
    GIO_USE_VOLUME_MONITOR=unix \
    GTK_USE_PORTAL=0 \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SMOKE_BAR_POSITION="$position" \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS=90 \
    REPRISE_LOG=debug \
    "$binary" >"$app_log" 2>&1 &
  app_pid=$!
  current_app_pid=$app_pid
  if ! window_id=$(wait_for_window "$app_pid"); then
    echo "$position: Reprise window did not appear" >&2
    tail -n 80 "$app_log" >&2 || true
    return 1
  fi

  cua_resize_window "$app_pid" "$window_id" 1440 800 "$position-window"
  cua_wait_for_label \
    "$app_pid" "$window_id" "Albums" "$position-tracks-ready" >/dev/null
  start_path=$(capture_state "$app_pid" "$window_id" "$position-tracks-start")
  assert_snapshot_contains "$start_path" "Albums"
  cua_click_label "$app_pid" "$window_id" "Albums" "$position-open-albums"
  album_path=$(cua_wait_for_label \
    "$app_pid" "$window_id" "$first_album" "$position-albums-ready")
  assert_snapshot_contains "$album_path" "$first_album"
  capture_state "$app_pid" "$window_id" "$position-albums-start" >/dev/null

  scroll_grid \
    "$app_pid" "$window_id" down 1 "$position-albums-under-header" >/dev/null
  end_path=$(scroll_grid \
    "$app_pid" "$window_id" down 50 "$position-albums-at-end")
  assert_snapshot_contains "$end_path" "$last_album"
  scroll_grid \
    "$app_pid" "$window_id" up 1 "$position-albums-above-end" >/dev/null

  kill -TERM "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
  current_app_pid=""
  if rg -i 'Gtk-CRITICAL|GLib-CRITICAL|panicked at|BorrowError|BorrowMutError' \
    "$app_log"; then
    echo "$position: app emitted a critical or panic" >&2
    return 1
  fi
}

private_cleanup() {
  local exit_code=$?
  if [[ -n ${current_app_pid:-} ]] && kill -0 "$current_app_pid" 2>/dev/null; then
    kill -TERM "$current_app_pid" 2>/dev/null || true
    wait "$current_app_pid" 2>/dev/null || true
  fi
  cua_driver end_session "$(jq -nc --arg session "$session" '{session: $session}')" \
    >/dev/null 2>&1 || true
  [[ -z ${cua_pid:-} ]] || kill -TERM "$cua_pid" 2>/dev/null || true
  [[ -z ${registry_pid:-} ]] || kill -TERM "$registry_pid" 2>/dev/null || true
  [[ -z ${atspi_pid:-} ]] || kill -TERM "$atspi_pid" 2>/dev/null || true
  [[ -z ${cua_pid:-} ]] || wait "$cua_pid" 2>/dev/null || true
  [[ -z ${registry_pid:-} ]] || wait "$registry_pid" 2>/dev/null || true
  [[ -z ${atspi_pid:-} ]] || wait "$atspi_pid" 2>/dev/null || true
  exit "$exit_code"
}

run_private() {
  trap private_cleanup EXIT
  current_app_pid=""

  /usr/lib/at-spi-bus-launcher --launch-immediately --a11y=1 --screen-reader=1 \
    >"$CUA_E2E_OUT_DIR/at-spi.log" 2>&1 &
  atspi_pid=$!
  sleep 0.3
  /usr/lib/at-spi2-registryd >"$CUA_E2E_OUT_DIR/at-spi-registryd.log" 2>&1 &
  registry_pid=$!
  sleep 0.3

  export CUA_DRIVER_SOCKET="$scratch_root/cua-driver.sock"
  cua_driver serve --no-overlay >"$CUA_E2E_OUT_DIR/cua-driver.log" 2>&1 &
  cua_pid=$!
  for _ in $(seq 1 40); do
    cua_driver status >/dev/null 2>&1 && break
    sleep 0.25
  done
  cua_driver status >/dev/null
  cua_driver start_session \
    "$(jq -nc --arg session "$session" '{session: $session}')" >/dev/null

  run_position bottom
  run_position top
  echo "CUA_EVIDENCE=$CUA_E2E_OUT_DIR"
}

if [[ ${1:-} == --private ]]; then
  run_private
  exit 0
fi

for command in cua-driver Xvfb openbox dbus-run-session jq rg wmctrl; do
  required_command "$command"
done
if [[ ! -x /usr/lib/at-spi-bus-launcher || ! -x /usr/lib/at-spi2-registryd ]]; then
  echo "private AT-SPI launchers are unavailable" >&2
  exit 2
fi

echo "== Build debug app and synthetic-library seed tool =="
cargo build --locked -p reprise-gnome
cargo build --locked -p reprise-core --example scalability_baseline

scratch_root=$(mktemp -d /tmp/reprise-glass-cua.XXXXXX)
export scratch_root CUA_E2E_SESSION="$session"
CUA_E2E_OUT_DIR="$output_root/run-$(date -u +%Y%m%dT%H%M%SZ)-$$"
export CUA_E2E_OUT_DIR
mkdir -p "$CUA_E2E_OUT_DIR"
{
  printf 'reprise_commit=%s\n' "$(git rev-parse HEAD)"
  printf 'cua_driver=%s\n' "$(cua-driver --version)"
  printf 'display_backend=x11-xvfb\n'
  printf 'synthetic_albums=120\n'
} >"$CUA_E2E_OUT_DIR/run-manifest.txt"
echo "[glass-cua] evidence: $CUA_E2E_OUT_DIR"

display_file="$scratch_root/display"
Xvfb -displayfd 8 -screen 0 "$screen_resolution" -nolisten tcp \
  8>"$display_file" >"$CUA_E2E_OUT_DIR/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 1 40); do
  [[ -s $display_file ]] && break
  kill -0 "$xvfb_pid" 2>/dev/null || break
  sleep 0.1
done
display_number=$(tr -d '[:space:]' <"$display_file")
if [[ -z $display_number ]]; then
  echo "Xvfb did not allocate a private display" >&2
  sed -n '1,80p' "$CUA_E2E_OUT_DIR/xvfb.log" >&2 || true
  exit 1
fi
export DISPLAY=":$display_number"
openbox >"$CUA_E2E_OUT_DIR/openbox.log" 2>&1 &
openbox_pid=$!
sleep 0.5
if ! kill -0 "$openbox_pid" 2>/dev/null; then
  echo "Openbox did not start on the private display" >&2
  sed -n '1,80p' "$CUA_E2E_OUT_DIR/openbox.log" >&2 || true
  exit 1
fi

cleanup() {
  local exit_code=$?
  kill -TERM "$openbox_pid" "$xvfb_pid" 2>/dev/null || true
  wait "$openbox_pid" "$xvfb_pid" 2>/dev/null || true
  rm -rf -- "$scratch_root"
  exit "$exit_code"
}
trap cleanup EXIT

runtime_root="$scratch_root/runtime"
mkdir -m 700 "$runtime_root"
XDG_RUNTIME_DIR="$runtime_root" dbus-run-session -- env \
  XDG_RUNTIME_DIR="$runtime_root" \
  XDG_DATA_HOME="$scratch_root/root-data" \
  XDG_CACHE_HOME="$scratch_root/root-cache" \
  XDG_CONFIG_HOME="$scratch_root/root-config" \
  GIO_USE_VFS=local \
  GIO_USE_VOLUME_MONITOR=unix \
  GTK_USE_PORTAL=0 \
  GDK_BACKEND=x11 \
  WAYLAND_DISPLAY= \
  GTK_A11Y=atspi \
  NO_AT_BRIDGE=0 \
  REPRISE_AUDIO_SINK=fakesink \
  CUA_E2E_OUT_DIR="$CUA_E2E_OUT_DIR" \
  CUA_E2E_SESSION="$session" \
  scratch_root="$scratch_root" \
  bash "$0" --private
