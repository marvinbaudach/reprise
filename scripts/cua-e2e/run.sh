#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

# shellcheck source=lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"

APP_ID=org.reprise.Reprise
WINDOW_CLASS_MATCH=reprise
CUA_E2E_PROFILE="${CUA_E2E_PROFILE:-debug}"
CUA_E2E_OUT_DIR="${CUA_E2E_OUT_DIR:-/tmp/reprise-cua-e2e}"
CUA_E2E_SCREEN_RES="${CUA_E2E_SCREEN_RES:-1600x900x24}"
CUA_E2E_QUIT_DELAY_SECS="${CUA_E2E_QUIT_DELAY_SECS:-15}"
export CUA_E2E_OUT_DIR CUA_E2E_SESSION="${CUA_E2E_SESSION:-reprise-acceptance}"

required_command() {
  if ! command -v "$1" >/dev/null; then
    echo "required command is unavailable: $1" >&2
    exit 2
  fi
}

assert_clean_app_log() {
  local log_path=$1 scenario=$2
  local failures='Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed'

  if rg -i "$failures" "$log_path" >/dev/null; then
    echo "$scenario emitted a GTK/GLib critical, panic, or RefCell failure ($log_path)" >&2
    rg -i "$failures" "$log_path" >&2
    return 1
  fi
}

assert_app_log_contains() {
  local log_path=$1 marker=$2 scenario=$3

  if ! rg --quiet --fixed-strings "$marker" "$log_path"; then
    echo "$scenario log is missing diagnostic marker '$marker': $log_path" >&2
    tail -n 60 "$log_path" >&2 || true
    return 1
  fi
}

window_id_from_response() {
  jq -r --arg class "$WINDOW_CLASS_MATCH" '
    [.. | objects
      | select(.window_id? != null)
      | select(((.title? // "") + " " + (.class? // "")
        + " " + (.wm_class? // "")) | ascii_downcase | contains($class))
      | .window_id][0] // empty
  '
}

wait_for_window() {
  local pid=$1 response window_id

  for _ in $(seq 1 60); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    response=$($CUA_DRIVER_BIN list_windows "$(jq -nc --argjson pid "$pid" '{pid: $pid}')")
    window_id=$(window_id_from_response <<<"$response")
    if [[ -n "$window_id" ]]; then
      printf '%s\n' "$window_id"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

wait_for_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 24); do
    snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt")
    if assert_snapshot_contains "$snapshot_path" "$label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
    sleep 0.25
  done
  echo "window never exposed expected accessible label '$label'" >&2
  return 1
}

APP_PID=""
APP_LOG=""
WINDOW_ID=""

stop_app_on_failure() {
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill -TERM "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi
}

start_scenario_app() {
  local scenario=$1 scan_dir=${2:-}
  local profile_root="$CUA_E2E_SCRATCH_ROOT/$scenario"
  local -a scan_env=(-u REPRISE_SCAN_DIR)

  mkdir -p "$profile_root/data" "$profile_root/cache" "$profile_root/config"
  if [[ -n "$scan_dir" ]]; then
    scan_env=(REPRISE_SCAN_DIR="$scan_dir")
  fi
  APP_LOG="$CUA_E2E_OUT_DIR/$scenario-app.log"
  env "${scan_env[@]}" \
    XDG_DATA_HOME="$profile_root/data" \
    XDG_CACHE_HOME="$profile_root/cache" \
    XDG_CONFIG_HOME="$profile_root/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS="$CUA_E2E_QUIT_DELAY_SECS" \
    REPRISE_LOG=debug \
    "$CUA_E2E_BIN_PATH" >"$APP_LOG" 2>&1 &
  APP_PID=$!
  if ! WINDOW_ID=$(wait_for_window "$APP_PID"); then
    echo "$scenario did not expose a Reprise window" >&2
    tail -n 60 "$APP_LOG" >&2 || true
    return 1
  fi
}

finish_scenario() {
  local scenario=$1
  shift

  if ! wait "$APP_PID"; then
    echo "$scenario did not shut down cleanly through the smoke timer" >&2
    return 1
  fi
  assert_clean_app_log "$APP_LOG" "$scenario"
  assert_app_log_contains "$APP_LOG" "starting Reprise" "$scenario"
  assert_app_log_contains "$APP_LOG" "database ready" "$scenario"
  assert_app_log_contains "$APP_LOG" "smoke-quit timer fired" "$scenario"
  for marker in "$@"; do
    assert_app_log_contains "$APP_LOG" "$marker" "$scenario"
  done
  APP_PID=""
  WINDOW_ID=""
}

run_fresh_install_scenario() {
  local initial_path empty_path

  echo "[cua-e2e] fresh install: wizard -> skip -> empty library"
  start_scenario_app fresh-install
  initial_path=$(wait_for_label "$APP_PID" "$WINDOW_ID" "Welcome to Reprise" fresh-initial)
  assert_snapshot_contains "$initial_path" "Skip for Now"
  assert_snapshot_contains "$initial_path" "Set Up Library"

  cua_click_label "$APP_PID" "$WINDOW_ID" "Skip for Now" fresh-skip
  empty_path=$(wait_for_label "$APP_PID" "$WINDOW_ID" "No music yet" fresh-empty)
  assert_snapshot_absent "$empty_path" "Welcome to Reprise"
  finish_scenario fresh-install \
    "first-run wizard presented" \
    "first-run setup completed"
}

run_populated_library_scenario() {
  local fixture_dir="$CUA_E2E_SCRATCH_ROOT/fixture-music"
  local initial_path results_path

  echo "[cua-e2e] populated library: fixture scan -> semantic search"
  mkdir -p "$fixture_dir"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_01.flac"
  cp "$repo_root/crates/reprise-core/tests/fixtures/sine.flac" "$fixture_dir/sine_02.flac"
  start_scenario_app populated-library "$fixture_dir"
  initial_path=$(wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" populated-initial)
  assert_snapshot_contains "$initial_path" "sine_01"
  assert_snapshot_contains "$initial_path" "Tracks"

  # UX PLAY-2 [e2e]-Verdrahtung: Doppelklick auf eine Row baut die Queue aus
  # der sichtbaren Liste (Log-Marker aus play_from_view) und startet Playback.
  echo "[cua-e2e] play-2-doubleclick-row: activation builds queue from view"
  cua_double_click_label "$APP_PID" "$WINDOW_ID" "sine_01" "play-2-doubleclick-row"
  assert_app_log_contains \
    "$APP_LOG" "queue set from view" "play-2-doubleclick-row"

  cua_type_text_label \
    "$APP_PID" "$WINDOW_ID" "Search all fields" "nomatch" populated-search
  results_path=$(wait_for_label "$APP_PID" "$WINDOW_ID" "No results" populated-no-results)
  assert_snapshot_contains "$results_path" "Try a different search"
  finish_scenario populated-library \
    "dev scan complete" \
    "first-run decision"
}

private_session_cleanup() {
  local exit_code=$?
  stop_app_on_failure
  "$CUA_DRIVER_BIN" end_session \
    "$(jq -nc --arg session "$CUA_E2E_SESSION" '{session: $session}')" \
    >/dev/null 2>&1 || true
  "$CUA_DRIVER_BIN" stop >/dev/null 2>&1 || true
  if [[ -n "${CUA_DAEMON_PID:-}" ]]; then
    kill -TERM "$CUA_DAEMON_PID" 2>/dev/null || true
  fi
  if [[ -n "${ATSPI_REGISTRYD_PID:-}" ]]; then
    kill -TERM "$ATSPI_REGISTRYD_PID" 2>/dev/null || true
  fi
  if [[ -n "${ATSPI_PID:-}" ]]; then
    kill -TERM "$ATSPI_PID" 2>/dev/null || true
  fi
  exit "$exit_code"
}

run_private_session() {
  trap private_session_cleanup EXIT

  /usr/lib/at-spi-bus-launcher \
    --launch-immediately --a11y=1 --screen-reader=1 \
    >"$CUA_E2E_OUT_DIR/at-spi.log" 2>&1 &
  ATSPI_PID=$!
  sleep 0.3

  /usr/lib/at-spi2-registryd \
    >"$CUA_E2E_OUT_DIR/at-spi-registryd.log" 2>&1 &
  ATSPI_REGISTRYD_PID=$!
  sleep 0.3

  export CUA_DRIVER_SOCKET="$CUA_E2E_SCRATCH_ROOT/cua-driver.sock"
  "$CUA_DRIVER_BIN" serve --no-overlay \
    >"$CUA_E2E_OUT_DIR/cua-driver.log" 2>&1 &
  CUA_DAEMON_PID=$!
  for _ in $(seq 1 40); do
    "$CUA_DRIVER_BIN" status >/dev/null 2>&1 && break
    sleep 0.25
  done
  "$CUA_DRIVER_BIN" status >/dev/null
  "$CUA_DRIVER_BIN" start_session \
    "$(jq -nc --arg session "$CUA_E2E_SESSION" '{session: $session}')" >/dev/null

  run_fresh_install_scenario
  run_populated_library_scenario
  echo "[cua-e2e] all acceptance scenarios passed"
}

if [[ "${1:-}" == "--private-session" ]]; then
  run_private_session
  exit 0
fi

for command in "$CUA_DRIVER_BIN" Xvfb openbox dbus-run-session jq rg; do
  required_command "$command"
done
if [[ ! -x /usr/lib/at-spi-bus-launcher ]]; then
  echo "AT-SPI bus launcher is unavailable: /usr/lib/at-spi-bus-launcher" >&2
  exit 2
fi

case "$CUA_E2E_PROFILE" in
  debug) CUA_E2E_BIN_PATH="$repo_root/target/debug/reprise" ;;
  release) CUA_E2E_BIN_PATH="$repo_root/target/release/reprise" ;;
  *) echo "CUA_E2E_PROFILE must be debug or release" >&2; exit 2 ;;
esac
if [[ ! -x "$CUA_E2E_BIN_PATH" ]]; then
  echo "build Reprise first: cargo build${CUA_E2E_PROFILE/release/ --release}" >&2
  exit 2
fi

CUA_E2E_SCRATCH_ROOT=$(mktemp -d /tmp/reprise-cua-e2e.XXXXXX)
export CUA_E2E_SCRATCH_ROOT CUA_E2E_BIN_PATH
output_root=$CUA_E2E_OUT_DIR
CUA_E2E_OUT_DIR="$output_root/run-$(date -u +%Y%m%dT%H%M%SZ)-$$"
export CUA_E2E_OUT_DIR
mkdir -p "$CUA_E2E_OUT_DIR"
{
  printf 'reprise_commit=%s\n' "$(git -C "$repo_root" rev-parse HEAD)"
  printf 'reprise_profile=%s\n' "$CUA_E2E_PROFILE"
  printf 'cua_driver=%s\n' "$($CUA_DRIVER_BIN --version)"
  printf 'platform=%s\n' "$(uname -srmo)"
  printf 'display_backend=x11-xvfb\n'
  printf 'created_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >"$CUA_E2E_OUT_DIR/run-manifest.txt"
echo "[cua-e2e] evidence: $CUA_E2E_OUT_DIR"

XVFB_PID=""
OPENBOX_PID=""
cleanup() {
  local exit_code=$?
  [[ -z "$OPENBOX_PID" ]] || kill -TERM "$OPENBOX_PID" 2>/dev/null || true
  [[ -z "$XVFB_PID" ]] || kill -TERM "$XVFB_PID" 2>/dev/null || true
  rm -rf "$CUA_E2E_SCRATCH_ROOT"
  exit "$exit_code"
}
trap cleanup EXIT

display_file="$CUA_E2E_SCRATCH_ROOT/display"
Xvfb -displayfd 8 -screen 0 "$CUA_E2E_SCREEN_RES" -nolisten tcp \
  8>"$display_file" >"$CUA_E2E_OUT_DIR/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 40); do
  [[ -s "$display_file" ]] && break
  kill -0 "$XVFB_PID" 2>/dev/null || break
  sleep 0.1
done
display_number=$(tr -d '[:space:]' <"$display_file")
if [[ -z "$display_number" ]]; then
  echo "Xvfb did not allocate a private display" >&2
  tail -n 20 "$CUA_E2E_OUT_DIR/xvfb.log" >&2 || true
  exit 1
fi
DISPLAY=":$display_number"
export DISPLAY
openbox >"$CUA_E2E_OUT_DIR/openbox.log" 2>&1 &
OPENBOX_PID=$!
sleep 0.5
if ! kill -0 "$OPENBOX_PID" 2>/dev/null; then
  echo "Openbox did not start on the private display" >&2
  tail -n 20 "$CUA_E2E_OUT_DIR/openbox.log" >&2 || true
  exit 1
fi

runtime_dir="$CUA_E2E_SCRATCH_ROOT/runtime"
mkdir -m 700 "$runtime_dir"
dbus-run-session -- env \
  XDG_RUNTIME_DIR="$runtime_dir" \
  XDG_DATA_HOME="$CUA_E2E_SCRATCH_ROOT/root-data" \
  XDG_CACHE_HOME="$CUA_E2E_SCRATCH_ROOT/root-cache" \
  XDG_CONFIG_HOME="$CUA_E2E_SCRATCH_ROOT/root-config" \
  GDK_BACKEND=x11 \
  WAYLAND_DISPLAY= \
  GTK_A11Y=atspi \
  NO_AT_BRIDGE=0 \
  REPRISE_AUDIO_SINK=fakesink \
  CUA_E2E_OUT_DIR="$CUA_E2E_OUT_DIR" \
  CUA_E2E_SCRATCH_ROOT="$CUA_E2E_SCRATCH_ROOT" \
  CUA_E2E_BIN_PATH="$CUA_E2E_BIN_PATH" \
  CUA_E2E_SESSION="$CUA_E2E_SESSION" \
  CUA_E2E_QUIT_DELAY_SECS="$CUA_E2E_QUIT_DELAY_SECS" \
  CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
  "$0" --private-session
