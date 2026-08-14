#!/usr/bin/env bash

# Reusable lifecycle for Reprise CUA runs. Callers own `set -e`, their
# scenario cleanup, and the application process. This layer owns only the
# private display, D-Bus/AT-SPI bridge, CUA daemon, and isolated root profile.

# 100 ms per tick — 2 s of grace before SIGKILL, the same budget the acceptance
# runs settled on for the application process.
CUA_COMMON_TERM_GRACE_TICKS=20

CUA_COMMON_XVFB_PID=""
CUA_COMMON_OPENBOX_PID=""
CUA_COMMON_ATSPI_PID=""
CUA_COMMON_ATSPI_REGISTRYD_PID=""
CUA_COMMON_DAEMON_PID=""

cua_common_start_display() {
  local output_dir=$1 scratch_root=$2 screen_res=$3
  local display_file="$scratch_root/display"

  Xvfb -displayfd 8 -screen 0 "$screen_res" -nolisten tcp \
    8>"$display_file" >"$output_dir/xvfb.log" 2>&1 &
  CUA_COMMON_XVFB_PID=$!
  for _ in $(seq 1 40); do
    [[ -s "$display_file" ]] && break
    kill -0 "$CUA_COMMON_XVFB_PID" 2>/dev/null || break
    sleep 0.1
  done
  local display_number
  display_number=$(tr -d '[:space:]' <"$display_file")
  if [[ -z "$display_number" ]]; then
    echo "Xvfb did not allocate a private display" >&2
    tail -n 20 "$output_dir/xvfb.log" >&2 || true
    return 1
  fi
  DISPLAY=":$display_number"
  export DISPLAY

  openbox >"$output_dir/openbox.log" 2>&1 &
  CUA_COMMON_OPENBOX_PID=$!
  CUA_E2E_WM_PID="$CUA_COMMON_OPENBOX_PID"
  export CUA_E2E_WM_PID
  sleep 0.5
  if ! kill -0 "$CUA_COMMON_OPENBOX_PID" 2>/dev/null; then
    echo "Openbox did not start on the private display" >&2
    tail -n 20 "$output_dir/openbox.log" >&2 || true
    return 1
  fi
}

cua_common_stop_display() {
  [[ -z "$CUA_COMMON_OPENBOX_PID" ]] \
    || kill -TERM "$CUA_COMMON_OPENBOX_PID" 2>/dev/null || true
  [[ -z "$CUA_COMMON_XVFB_PID" ]] \
    || kill -TERM "$CUA_COMMON_XVFB_PID" 2>/dev/null || true
  CUA_COMMON_OPENBOX_PID=""
  CUA_COMMON_XVFB_PID=""
}

cua_common_start_driver() {
  local output_dir=$1 socket_path=$2 session_id=$3
  local log_prefix
  log_prefix=$(basename "$socket_path" -cua-driver.sock)

  /usr/lib/at-spi-bus-launcher \
    --launch-immediately --a11y=1 --screen-reader=1 \
    >"$output_dir/$log_prefix-at-spi.log" 2>&1 &
  CUA_COMMON_ATSPI_PID=$!
  sleep 0.3

  /usr/lib/at-spi2-registryd \
    >"$output_dir/$log_prefix-at-spi-registryd.log" 2>&1 &
  CUA_COMMON_ATSPI_REGISTRYD_PID=$!
  sleep 0.3

  CUA_DRIVER_SOCKET="$socket_path"
  export CUA_DRIVER_SOCKET
  cua_common_start_daemon "$output_dir" "$session_id"
}

cua_common_start_daemon() {
  local output_dir=$1 session_id=$2

  rm -f -- "$CUA_DRIVER_SOCKET"
  env CUA_DRIVER_RS_UPDATE_CHECK=0 \
    "$CUA_DRIVER_BIN" serve --no-overlay --socket "$CUA_DRIVER_SOCKET" \
    >>"$output_dir/cua-driver.log" 2>&1 &
  CUA_COMMON_DAEMON_PID=$!
  for _ in $(seq 1 40); do
    cua_driver status >/dev/null 2>&1 && break
    sleep 0.25
  done
  cua_driver status >/dev/null
  cua_driver start_session \
    "$(jq -nc --arg session "$session_id" '{session: $session}')" >/dev/null
}

# Terminates a child and does not return until it is gone. A bare `wait` cannot be
# trusted here: a process that ignores or blocks SIGTERM never exits, and `wait`
# has no timeout, so every cleanup step behind it is skipped for the rest of the
# run. SIGKILL after a bounded grace period is what makes the return guaranteed.
cua_common_terminate_pid() {
  local pid=$1 grace_ticks=${2:-$CUA_COMMON_TERM_GRACE_TICKS}

  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 "$grace_ticks"); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -KILL "$pid" 2>/dev/null || true
  fi
  wait "$pid" 2>/dev/null || true
}

cua_common_stop_daemon() {
  local session_id=$1

  cua_driver end_session \
    "$(jq -nc --arg session "$session_id" '{session: $session}')" \
    >/dev/null 2>&1 || true
  if [[ -n "$CUA_COMMON_DAEMON_PID" ]]; then
    cua_common_terminate_pid "$CUA_COMMON_DAEMON_PID"
  fi
  CUA_COMMON_DAEMON_PID=""
}

cua_common_stop_driver() {
  local session_id=$1

  cua_common_stop_daemon "$session_id"
  [[ -z "$CUA_COMMON_ATSPI_REGISTRYD_PID" ]] \
    || kill -TERM "$CUA_COMMON_ATSPI_REGISTRYD_PID" 2>/dev/null || true
  [[ -z "$CUA_COMMON_ATSPI_PID" ]] \
    || kill -TERM "$CUA_COMMON_ATSPI_PID" 2>/dev/null || true
  CUA_COMMON_ATSPI_REGISTRYD_PID=""
  CUA_COMMON_ATSPI_PID=""
}

cua_common_restart_driver() {
  local output_dir=$1 socket_path=$2 session_id=$3

  cua_common_stop_daemon "$session_id"
  CUA_DRIVER_SOCKET="$socket_path"
  export CUA_DRIVER_SOCKET
  cua_common_start_daemon "$output_dir" "$session_id"
}

cua_common_run_private() {
  local script_path=$1 scenario_group=$2 scratch_root=$3 output_dir=$4
  local runtime_dir="$scratch_root/runtime-$scenario_group"
  local root_profile="$scratch_root/root-$scenario_group"

  cua_common_exec_private "$runtime_dir" "$root_profile" env \
    -u GNOME_KEYRING_CONTROL \
    -u GNOME_KEYRING_PID \
    CUA_E2E_OUT_DIR="$output_dir" \
    CUA_E2E_SCRATCH_ROOT="$scratch_root" \
    CUA_E2E_BIN_PATH="$CUA_E2E_BIN_PATH" \
    CUA_E2E_SESSION="$CUA_E2E_SESSION" \
    CUA_E2E_QUIT_DELAY_SECS="$CUA_E2E_QUIT_DELAY_SECS" \
    CUA_E2E_KEYBOARD_QUIT_DELAY_SECS="$CUA_E2E_KEYBOARD_QUIT_DELAY_SECS" \
    CUA_E2E_DRIVER_TIMEOUT_SECS="$CUA_E2E_DRIVER_TIMEOUT_SECS" \
    CUA_E2E_PRIVATE_GROUP="$scenario_group" \
    CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
    "$script_path" --private-session
}

cua_common_exec_private() {
  local runtime_dir=$1 root_profile=$2
  shift 2

  mkdir -m 700 "$runtime_dir"
  mkdir -p "$root_profile/data" "$root_profile/cache" "$root_profile/config"
  # There is no keyring in the private session, but the desktop portal still
  # asks for one. Without these stubs the bus waits out its activation timeout
  # (measured 25s on a developer host, 120s under the default
  # service_start_timeout) and short missions get killed mid-run. Failing the
  # activation immediately is the honest answer for a session that has no
  # secret store; Reprise only reads the keyring from the scrobbling
  # preference pages, never at startup.
  local stub_root="$root_profile/dbus-stubs"
  local stub_dir="$stub_root/dbus-1/services"
  local stub_service
  mkdir -p "$stub_dir"
  for stub_service in org.freedesktop.secrets org.freedesktop.impl.portal.Secret; do
    printf '[D-BUS Service]\nName=%s\nExec=/bin/false\n' "$stub_service" \
      >"$stub_dir/$stub_service.service"
  done
  local stub_data_dirs="$stub_root:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
  XDG_DATA_DIRS="$stub_data_dirs" \
  dbus-run-session -- env \
    XDG_DATA_DIRS="$stub_data_dirs" \
    XDG_RUNTIME_DIR="$runtime_dir" \
    XDG_DATA_HOME="$root_profile/data" \
    XDG_CACHE_HOME="$root_profile/cache" \
    XDG_CONFIG_HOME="$root_profile/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    "$@"
}
