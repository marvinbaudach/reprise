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

runtime_snapshot() {
  local pid=$1 window_id=$2 stem=$3
  local json_path="$RUNTIME_OUTPUT_DIR/$stem.json"
  local screenshot_path="$RUNTIME_OUTPUT_DIR/$stem.png"
  local payload
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg session "$RUNTIME_CUA_SESSION" \
    --arg screenshot_out_file "$screenshot_path" \
    '{pid: $pid, window_id: $window_id, session: $session,
      screenshot_out_file: $screenshot_out_file}')
  cua-driver get_window_state "$payload" >"$json_path"
  if jq -e '.degraded == true' "$json_path" >/dev/null; then
    echo "runtime snapshot has a degraded accessibility tree: $stem" >&2
    return 1
  fi
  if [[ ! -s $screenshot_path ]]; then
    echo "runtime snapshot did not retain screenshot evidence: $stem" >&2
    return 1
  fi
  printf '%s\n' "$json_path"
}

visible_track_labels() {
  jq -c '
    [(.structuredContent.elements // .elements // [])[]
      | .label
      | select(type == "string")
      | select(test("^(Needle|Track) [0-9]{6}$"))]
    | sort | unique
  ' "$1"
}

first_track_element_index() {
  jq -r '
    [(.structuredContent.elements // .elements // [])[]
      | select((.label // "") | test("^(Needle|Track) [0-9]{6}$"))
      | .element_index][0] // empty
  ' "$1"
}

run_startup_scenario() {
  local track_count=$1 profile_root="$RUNTIME_SCRATCH_ROOT/profile-$1"
  local samples_path="$RUNTIME_SCRATCH_ROOT/startup-$1.samples"
  : >"$samples_path"

  for iteration in $(seq 1 5); do
    local log_path="$RUNTIME_OUTPUT_DIR/startup-$track_count-$iteration.log"
    local snapshot_path="$RUNTIME_OUTPUT_DIR/startup-$track_count-$iteration.json"
    local screenshot_path="$RUNTIME_OUTPUT_DIR/startup-$track_count-$iteration.png"
    local runtime_report_path="$RUNTIME_OUTPUT_DIR/runtime-$track_count-$iteration.json"
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
      REPRISE_PERF_RUNTIME_REPORT="$runtime_report_path" \
      REPRISE_LOG=debug \
      "$RUNTIME_INSTALLED_BIN" >"$log_path" 2>&1 &
    app_pid=$!
    RUNTIME_APP_PID=$app_pid
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
    if [[ ! -s $screenshot_path ]]; then
      echo "startup snapshot did not retain screenshot evidence" >&2
      return 1
    fi
    wait "$app_pid"
    RUNTIME_APP_PID=""
    assert_clean_app_log "$log_path"
    if [[ ! -s $runtime_report_path ]]; then
      echo "installed app did not write runtime GTK metrics: $runtime_report_path" >&2
      return 1
    fi
    if ! jq -e --argjson tracks "$track_count" '
      .model_items == $tracks
      and .column_factories >= 1 and .column_factories <= 16
      and .visible_columns >= 1 and .visible_columns <= .column_factories
      and .cached_windows <= 8 and .cached_tracks <= 1600
      and .total_window_widgets <= 10000
      and .column_view_widgets <= 4096
      and .row_widgets >= 1 and .row_widgets <= 128
      and .cell_widgets >= 1 and .cell_widgets <= 2048
    ' "$runtime_report_path" >/dev/null; then
      echo "runtime GTK metrics exceeded a deterministic bound" >&2
      jq . "$runtime_report_path" >&2
      return 1
    fi
  done

  jq -n \
    --argjson schema_version 1 \
    --argjson generated_tracks "$track_count" \
    --slurpfile timing <(summarize_samples <"$samples_path") \
    '{schema_version: $schema_version, generated_tracks: $generated_tracks,
      spawn_to_accessible_window: $timing[0]}' \
    >"$RUNTIME_OUTPUT_DIR/startup-$track_count.json"
}

run_scroll_scenario() {
  local track_count=$1 profile_root="$RUNTIME_SCRATCH_ROOT/profile-$1"
  local log_path="$RUNTIME_OUTPUT_DIR/scroll-$track_count-app.log"
  local runtime_report_path="$RUNTIME_OUTPUT_DIR/scroll-$track_count-runtime.json"
  local samples_path="$RUNTIME_SCRATCH_ROOT/scroll-$track_count.samples"
  local app_pid window_id
  : >"$samples_path"

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
    REPRISE_SMOKE_QUIT_DELAY_SECS=6 \
    REPRISE_PERF_RUNTIME_REPORT="$runtime_report_path" \
    REPRISE_LOG=debug \
    "$RUNTIME_INSTALLED_BIN" >"$log_path" 2>&1 &
  app_pid=$!
  RUNTIME_APP_PID=$app_pid
  if ! window_id=$(wait_for_window "$app_pid"); then
    echo "installed app did not expose a scroll window for $track_count tracks" >&2
    return 1
  fi

  for iteration in $(seq 1 5); do
    local before_path before_labels element_index action_path action_payload
    local started_ns changed_ns after_path after_labels=""
    before_path=$(runtime_snapshot \
      "$app_pid" "$window_id" "scroll-$track_count-$iteration-before")
    before_labels=$(visible_track_labels "$before_path")
    element_index=$(first_track_element_index "$before_path")
    if [[ -z $element_index || $before_labels == "[]" ]]; then
      echo "scroll snapshot exposes no generated track labels" >&2
      return 1
    fi

    action_path="$RUNTIME_OUTPUT_DIR/scroll-$track_count-$iteration-action.json"
    action_payload=$(jq -nc \
      --argjson pid "$app_pid" \
      --argjson window_id "$window_id" \
      --argjson element_index "$element_index" \
      --arg session "$RUNTIME_CUA_SESSION" \
      '{pid: $pid, window_id: $window_id, element_index: $element_index,
        session: $session, direction: "down", amount: 1, by: "page"}')
    started_ns=$(date +%s%N)
    cua-driver scroll "$action_payload" >"$action_path"
    if jq -e '
      .effect == "suspected_noop"
      or ((.escalation.recommended? // null) != null)
    ' "$action_path" >/dev/null; then
      echo "CUA scroll did not land cleanly: $action_path" >&2
      return 1
    fi

    for attempt in $(seq 1 20); do
      after_path=$(runtime_snapshot \
        "$app_pid" "$window_id" "scroll-$track_count-$iteration-after-$attempt")
      after_labels=$(visible_track_labels "$after_path")
      if [[ $after_labels != "$before_labels" ]]; then
        break
      fi
      sleep 0.01
    done
    if [[ $after_labels == "$before_labels" ]]; then
      echo "visible track labels did not change after CUA scroll" >&2
      return 1
    fi
    changed_ns=$(date +%s%N)
    printf '%s\n' "$(((changed_ns - started_ns) / 1000))" >>"$samples_path"
  done

  wait "$app_pid"
  RUNTIME_APP_PID=""
  assert_clean_app_log "$log_path"
  if [[ ! -s $runtime_report_path ]]; then
    echo "scroll scenario did not write runtime GTK metrics" >&2
    return 1
  fi
  jq -n \
    --argjson schema_version 1 \
    --argjson generated_tracks "$track_count" \
    --slurpfile timing <(summarize_samples <"$samples_path") \
    '{schema_version: $schema_version, generated_tracks: $generated_tracks,
      action_to_changed_snapshot: $timing[0], display_backend: "x11-xvfb"}' \
    >"$RUNTIME_OUTPUT_DIR/scroll-$track_count.json"
}

private_session_cleanup() {
  local exit_code=$?
  if [[ -n ${RUNTIME_APP_PID:-} ]] && kill -0 "$RUNTIME_APP_PID" 2>/dev/null; then
    kill -TERM "$RUNTIME_APP_PID" 2>/dev/null || true
    wait "$RUNTIME_APP_PID" 2>/dev/null || true
  fi
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
  RUNTIME_APP_PID=""

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
    echo "== Observable CUA scroll: $track_count tracks =="
    run_scroll_scenario "$track_count"
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
cargo build --locked --release -p reprise-core --example queue_memory_baseline
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

  queue_samples="$runtime_scratch_root/queue-$track_count.jsonl"
  for _ in $(seq 1 5); do
    target/release/examples/queue_memory_baseline \
      --tracks "$track_count" >>"$queue_samples"
  done
  if ! jq -e --argjson tracks "$track_count" '
    all(.; .generated_tracks == $tracks
      and .logical_payload_bytes == ($tracks * 16)
      and .rss_delta_bytes <= (8388608 + ($tracks * 64)))
  ' --slurp "$queue_samples" >/dev/null; then
    echo "queue memory exceeded its deterministic growth budget" >&2
    jq . "$queue_samples" >&2
    exit 1
  fi
  jq -s --argjson tracks "$track_count" '
    . as $reports
    | ($reports | map(.rss_delta_bytes) | sort) as $rss
    | {schema_version: 1, generated_tracks: $tracks,
       logical_payload_bytes: $reports[0].logical_payload_bytes,
       rss_delta: {min_bytes: $rss[0], median_bytes: $rss[2],
         max_bytes: $rss[4], samples_bytes: $rss}}
  ' "$queue_samples" >"$output_dir/queue-memory-$track_count.json"
done

jq -n \
  --arg commit "$(git rev-parse HEAD)" \
  --arg profile "release-installed-destdir" \
  --arg platform "$(uname -srm)" \
  --arg rustc "$(rustc --version)" \
  --arg cargo "$(cargo --version)" \
  --arg cua_driver "$(cua-driver --version)" \
  --argjson track_counts "$(printf '%s\n' $track_counts | jq -s 'map(tonumber)')" \
  '{schema_version: 1, commit: $commit, profile: $profile,
    generated_metadata_only: true, platform: $platform, rustc: $rustc,
    cargo: $cargo, cua_driver: $cua_driver, display_backend: "x11-xvfb",
    startup_iterations: 5, queue_iterations: 5, scroll_iterations: 5,
    database_page_cache: "host-controlled-warm-after-seed",
    track_counts: $track_counts}' \
  >"$output_dir/manifest.json"

runtime_dir="$runtime_scratch_root/runtime"
mkdir -m 700 "$runtime_dir"
RUNTIME_OUTPUT_DIR=$(cd "$output_dir" && pwd)
export RUNTIME_OUTPUT_DIR RUNTIME_SCRATCH_ROOT="$runtime_scratch_root"
export RUNTIME_INSTALLED_BIN="$installed_bin" RUNTIME_TRACK_COUNTS="$track_counts"
export RUNTIME_CUA_SESSION="reprise-runtime-performance"

display_number=$((100 + $$ % 50000))
Xvfb ":$display_number" -screen 0 1600x900x24 -nolisten tcp \
  >"$output_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 1 40); do
  kill -0 "$xvfb_pid" 2>/dev/null || break
  if [[ -S "/tmp/.X11-unix/X$display_number" ]]; then
    break
  fi
  sleep 0.1
done
if ! kill -0 "$xvfb_pid" 2>/dev/null || [[ ! -S "/tmp/.X11-unix/X$display_number" ]]; then
  echo "Xvfb did not allocate a private display" >&2
  head -n 20 "$output_dir/xvfb.log" >&2 || true
  exit 1
fi
export DISPLAY=":$display_number"
openbox >"$output_dir/openbox.log" 2>&1 &
openbox_pid=$!
trap 'kill -TERM "$openbox_pid" "$xvfb_pid" 2>/dev/null || true; rm -r -- "$runtime_scratch_root"' EXIT
sleep 0.5
if ! kill -0 "$openbox_pid" 2>/dev/null; then
  echo "Openbox did not start on the private display" >&2
  head -n 20 "$output_dir/openbox.log" >&2 || true
  exit 1
fi

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
