#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: REPRISE_BIN=/path/to/reprise scripts/repro-track-list-fresh-start.sh --db /path/to/reprise.db

Starts Reprise in an isolated Xvfb display, private D-Bus session and scratch
XDG roots, then captures the track list five seconds after its window appears.
The supplied database and adjacent -wal/-shm files are copied into the scratch
data root; this script never opens the originals in place.
EOF
}

database=
while (($# > 0)); do
  case "$1" in
    --db)
      (($# >= 2)) || { usage >&2; exit 2; }
      database=$2
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n $database ]] || { echo "--db is required" >&2; usage >&2; exit 2; }
[[ -f $database ]] || { echo "Database does not exist: $database" >&2; exit 2; }

reprise_bin=${REPRISE_BIN:-target/release/reprise}
[[ -x $reprise_bin ]] || {
  echo "REPRISE_BIN is not executable: $reprise_bin" >&2
  exit 2
}

for command in Xvfb openbox dbus-run-session xdotool import; do
  command -v "$command" >/dev/null || {
    echo "Required command is unavailable: $command" >&2
    exit 2
  }
done

run_dir=$(mktemp -d "${TMPDIR:-/tmp}/reprise-fresh-start.XXXXXX")
mkdir -p "$run_dir/data/reprise" "$run_dir/config" "$run_dir/cache"
cp -- "$database" "$run_dir/data/reprise/reprise.db"
for suffix in -wal -shm; do
  [[ -f ${database}${suffix} ]] || continue
  cp -- "${database}${suffix}" "$run_dir/data/reprise/reprise.db${suffix}"
done

display_fifo=$run_dir/display-fifo
mkfifo "$display_fifo"
Xvfb -displayfd 3 -screen 0 1920x1200x24 -nolisten tcp \
  >"$run_dir/xvfb.log" 2>&1 3>"$display_fifo" &
xvfb_pid=$!
read -r display_number <"$display_fifo"
display=:$display_number
DISPLAY=$display openbox >"$run_dir/openbox.log" 2>&1 &
openbox_pid=$!

app_pid=
bus_pid=
cleanup() {
  [[ -z $app_pid ]] || kill "$app_pid" 2>/dev/null || true
  [[ -z $bus_pid ]] || wait "$bus_pid" 2>/dev/null || true
  kill "$openbox_pid" "$xvfb_pid" 2>/dev/null || true
  wait "$openbox_pid" "$xvfb_pid" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

dbus-run-session -- bash -c '
  env \
    DISPLAY="$1" \
    XDG_DATA_HOME="$2/data" \
    XDG_CONFIG_HOME="$2/config" \
    XDG_CACHE_HOME="$2/cache" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GSK_RENDERER=cairo \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SCROLL_PROBE=1 \
    "$3" >"$2/app.log" 2>&1 &
  child=$!
  printf "%s\n" "$child" >"$2/app.pid"
  wait "$child"
' bash "$display" "$run_dir" "$reprise_bin" &
bus_pid=$!

for _ in {1..100}; do
  [[ -s $run_dir/app.pid ]] && break
  sleep 0.1
done
[[ -s $run_dir/app.pid ]] || {
  echo "Reprise did not start; inspect $run_dir/app.log" >&2
  exit 1
}
read -r app_pid <"$run_dir/app.pid"

window_id=
for _ in {1..200}; do
  window_id=$(DISPLAY=$display xdotool search --onlyvisible --pid "$app_pid" 2>/dev/null | head -n 1 || true)
  [[ -n $window_id ]] && break
  kill -0 "$app_pid" 2>/dev/null || break
  sleep 0.1
done
[[ -n $window_id ]] || {
  echo "No Reprise window appeared; inspect $run_dir/app.log" >&2
  exit 1
}

sleep 5
DISPLAY=$display import -window "$window_id" "$run_dir/fresh-start-plus-5s.png"

echo "Screenshot: $run_dir/fresh-start-plus-5s.png"
echo "Log:        $run_dir/app.log"
