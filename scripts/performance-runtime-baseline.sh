#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'EOF'
usage: scripts/performance-runtime-baseline.sh OUTPUT_DIR [--quick]

Builds and installs the release app into a private DESTDIR, then benchmarks
the installed app with generated metadata only. The normal run measures
10,000 and 100,000 tracks; --quick measures 10,000 tracks only.

The retained evidence covers installed app startup, live GTK row/provider
counts, queue memory growth, and an isolated visible scroll response. The Git
worktree must be clean and OUTPUT_DIR must not already exist.
EOF
}

required_command() {
  if ! command -v "$1" >/dev/null; then
    echo "required command is unavailable: $1" >&2
    exit 2
  fi
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

  for _ in $(seq 1 300); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    response=$(cua-driver list_windows "$(jq -nc --argjson pid "$pid" '{pid: $pid}')")
    window_id=$(window_id_from_response <<<"$response")
    if [[ -n $window_id ]]; then
      printf '%s\n' "$window_id"
      return 0
    fi
    sleep 0.01
  done
  return 1
}

assert_clean_app_log() {
  local log_path=$1
  local failures='Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed'

  if rg -i "$failures" "$log_path" >/dev/null; then
    echo "runtime benchmark emitted a critical, panic, or borrow failure: $log_path" >&2
    rg -i "$failures" "$log_path" >&2
    return 1
  fi
  for marker in "starting Reprise" "database ready" "main window built" "smoke-quit timer fired"; do
    if ! rg --quiet --fixed-strings "$marker" "$log_path"; then
      echo "runtime benchmark log is missing '$marker': $log_path" >&2
      return 1
    fi
  done
}

summarize_samples() {
  jq -s '
    sort as $samples
    | {min_us: $samples[0], median_us: $samples[(length / 2 | floor)],
       max_us: $samples[-1], samples_us: .}
  '
}

run_startup_scenario() {
  local track_count=$1 profile_root="$RUNTIME_SCRATCH_ROOT/profile-$1"
  local samples_path="$RUNTIME_SCRATCH_ROOT/startup-$1.samples"
  : >"$samples_path"

  for iteration in $(seq 1 5); do
    local log_path="$RUNTIME_OUTPUT_DIR/startup-$track_count-$iteration.log"
    local snapshot_path="$RUNTIME_OUTPUT_DIR/startup-$track_count-$iteration.json"
    local screenshot_path="$RUNTIME_OUTPUT_DIR/startup-$track_count-$iteration.png"
    local started_ns ready_ns app_pid window_id payload
    started_ns=$(date +%s%N)
    env \
      XDG_DATA_HOME="$profile_root/data" \
      XDG_CACHE_HOME="$profile_root/cache" \
      XDG_CONFIG_HOME="$profile_root/config" \
      GDK_BACKEND=x11 \
      WAYLAND_DISPLAY= \
      GTK_A11Y=atspi \
      NO_AT_BRIDGE=0 \
      REPRISE_AUDIO_SINK=fakesink \
      REPRISE_SMOKE_QUIT=1 \
      REPRISE_SMOKE_QUIT_DELAY_SECS=1 \
      REPRISE_LOG=debug \
      "$RUNTIME_INSTALLED_BIN" >"$log_path" 2>&1 &
    app_pid=$!
    if ! window_id=$(wait_for_window "$app_pid"); then
      echo "installed app did not expose a window for $track_count tracks" >&2
      tail -n 80 "$log_path" >&2 || true
      return 1
    fi
    ready_ns=$(date +%s%N)
    printf '%s\n' "$(((ready_ns - started_ns) / 1000))" >>"$samples_path"

    payload=$(jq -nc \
      --argjson pid "$app_pid" \
      --argjson window_id "$window_id" \
      --arg session "$RUNTIME_CUA_SESSION" \
      --arg screenshot_out_file "$screenshot_path" \
      '{pid: $pid, window_id: $window_id, session: $session,
        screenshot_out_file: $screenshot_out_file}')
    cua-driver get_window_state "$payload" >"$snapshot_path"
    if jq -e '.degraded == true' "$snapshot_path" >/dev/null; then
      echo "startup snapshot has a degraded accessibility tree" >&2
      return 1
    fi
    wait "$app_pid"
    assert_clean_app_log "$log_path"
  done

  jq -n \
    --argjson schema_version 1 \
    --argjson generated_tracks "$track_count" \
    --slurpfile timing <(summarize_samples <"$samples_path") \
    '{schema_version: $schema_version, generated_tracks: $generated_tracks,
      spawn_to_accessible_window: $timing[0]}' \
    >"$RUNTIME_OUTPUT_DIR/startup-$track_count.json"
}

private_session_cleanup() {
  local exit_code=$?
  cua-driver end_session \
    "$(jq -nc --arg session "$RUNTIME_CUA_SESSION" '{session: $session}')" \
    >/dev/null 2>&1 || true
  cua-driver stop >/dev/null 2>&1 || true
  [[ -z ${CUA_DAEMON_PID:-} ]] || kill -TERM "$CUA_DAEMON_PID" 2>/dev/null || true
  [[ -z ${ATSPI_REGISTRYD_PID:-} ]] || kill -TERM "$ATSPI_REGISTRYD_PID" 2>/dev/null || true
  [[ -z ${ATSPI_PID:-} ]] || kill -TERM "$ATSPI_PID" 2>/dev/null || true
  exit "$exit_code"
}

run_private_session() {
  trap private_session_cleanup EXIT

  /usr/lib/at-spi-bus-launcher --launch-immediately --a11y=1 --screen-reader=1 \
    >"$RUNTIME_OUTPUT_DIR/at-spi.log" 2>&1 &
  ATSPI_PID=$!
  /usr/lib/at-spi2-registryd >"$RUNTIME_OUTPUT_DIR/at-spi-registryd.log" 2>&1 &
  ATSPI_REGISTRYD_PID=$!
  sleep 0.3

  export CUA_DRIVER_SOCKET="$RUNTIME_SCRATCH_ROOT/cua-driver.sock"
  cua-driver serve --no-overlay >"$RUNTIME_OUTPUT_DIR/cua-driver.log" 2>&1 &
  CUA_DAEMON_PID=$!
  for _ in $(seq 1 40); do
    cua-driver status >/dev/null 2>&1 && break
    sleep 0.25
  done
  cua-driver status >/dev/null
  cua-driver start_session \
    "$(jq -nc --arg session "$RUNTIME_CUA_SESSION" '{session: $session}')" >/dev/null

  for track_count in $RUNTIME_TRACK_COUNTS; do
    echo "== Installed startup: $track_count tracks =="
    run_startup_scenario "$track_count"
  done
}

if [[ ${1:-} == "--private-session" ]]; then
  run_private_session
  exit 0
fi

if [[ ${1:-} == "--help" ]]; then
  usage
  exit 0
fi
if (( $# < 1 || $# > 2 )); then
  usage >&2
  exit 2
fi

output_dir=$1
mode=${2:-}
if [[ -n $mode && $mode != "--quick" ]]; then
  echo "unknown option: $mode" >&2
  usage >&2
  exit 2
fi
if [[ -e $output_dir ]]; then
  echo "output directory already exists: $output_dir" >&2
  exit 2
fi
if [[ -n $(git status --porcelain) ]]; then
  echo "runtime performance baseline requires a clean Git worktree" >&2
  exit 2
fi

for command in cargo meson jq cua-driver Xvfb openbox dbus-run-session rg; do
  required_command "$command"
done
for executable in /usr/lib/at-spi-bus-launcher /usr/lib/at-spi2-registryd; do
  if [[ ! -x $executable ]]; then
    echo "required executable is unavailable: $executable" >&2
    exit 2
  fi
done

track_counts="10000 100000"
if [[ $mode == "--quick" ]]; then
  track_counts="10000"
fi

mkdir -p -- "$output_dir"
runtime_scratch_root=$(mktemp -d /tmp/reprise-runtime-performance.XXXXXX)
trap 'rm -r -- "$runtime_scratch_root"' EXIT

echo "== Build and install release app =="
meson setup "$runtime_scratch_root/build" . --prefix=/usr -Dprofile=release
DESTDIR="$runtime_scratch_root/install" meson install -C "$runtime_scratch_root/build"
cargo build --locked --release -p reprise-core --example scalability_baseline
installed_bin="$runtime_scratch_root/install/usr/bin/reprise"
if [[ ! -x $installed_bin ]]; then
  echo "installed Reprise binary is missing: $installed_bin" >&2
  exit 1
fi

for track_count in $track_counts; do
  profile_root="$runtime_scratch_root/profile-$track_count"
  mkdir -p "$profile_root/data/reprise" "$profile_root/cache" "$profile_root/config"
  target/release/examples/scalability_baseline \
    --db "$profile_root/data/reprise/reprise.db" \
    --tracks "$track_count" \
    --iterations 3 >"$output_dir/seed-$track_count.json"
done

jq -n \
  --arg commit "$(git rev-parse HEAD)" \
  --arg profile "release-installed-destdir" \
  --arg platform "$(uname -srm)" \
  --argjson track_counts "$(printf '%s\n' $track_counts | jq -s 'map(tonumber)')" \
  '{schema_version: 1, commit: $commit, profile: $profile,
    generated_metadata_only: true, platform: $platform, track_counts: $track_counts}' \
  >"$output_dir/manifest.json"

runtime_dir="$runtime_scratch_root/runtime"
mkdir -m 700 "$runtime_dir"
RUNTIME_OUTPUT_DIR=$(cd "$output_dir" && pwd)
export RUNTIME_OUTPUT_DIR RUNTIME_SCRATCH_ROOT="$runtime_scratch_root"
export RUNTIME_INSTALLED_BIN="$installed_bin" RUNTIME_TRACK_COUNTS="$track_counts"
export RUNTIME_CUA_SESSION="reprise-runtime-performance"

Xvfb -displayfd 8 -screen 0 1600x900x24 -nolisten tcp \
  8>"$runtime_scratch_root/display" >"$output_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 1 40); do
  [[ -s "$runtime_scratch_root/display" ]] && break
  kill -0 "$xvfb_pid" 2>/dev/null || break
  sleep 0.1
done
display_number=$(tr -d '[:space:]' <"$runtime_scratch_root/display")
if [[ -z $display_number ]]; then
  echo "Xvfb did not allocate a private display" >&2
  exit 1
fi
export DISPLAY=":$display_number"
openbox >"$output_dir/openbox.log" 2>&1 &
openbox_pid=$!
trap 'kill -TERM "$openbox_pid" "$xvfb_pid" 2>/dev/null || true; rm -r -- "$runtime_scratch_root"' EXIT
sleep 0.5

dbus-run-session -- env \
  XDG_RUNTIME_DIR="$runtime_dir" \
  XDG_DATA_HOME="$runtime_scratch_root/root-data" \
  XDG_CACHE_HOME="$runtime_scratch_root/root-cache" \
  XDG_CONFIG_HOME="$runtime_scratch_root/root-config" \
  GDK_BACKEND=x11 \
  WAYLAND_DISPLAY= \
  REPRISE_AUDIO_SINK=fakesink \
  RUNTIME_OUTPUT_DIR="$RUNTIME_OUTPUT_DIR" \
  RUNTIME_SCRATCH_ROOT="$RUNTIME_SCRATCH_ROOT" \
  RUNTIME_INSTALLED_BIN="$RUNTIME_INSTALLED_BIN" \
  RUNTIME_TRACK_COUNTS="$RUNTIME_TRACK_COUNTS" \
  RUNTIME_CUA_SESSION="$RUNTIME_CUA_SESSION" \
  "$0" --private-session

echo "Runtime performance baseline written to $output_dir"
