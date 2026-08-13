#!/usr/bin/env bash
# Verify the Radio table's cold-start favicon path in a disposable desktop.
set -euo pipefail

script_path=$(realpath "${BASH_SOURCE[0]}")
script_root=$(cd "$(dirname "$script_path")/.." && pwd)
repo_root=${1:-$script_root}
repo_root=$(realpath "$repo_root")
out_dir=${RADIO_FAVICON_OUT_DIR:-/tmp/reprise-radio-favicons/run}
run_label=$(basename "$out_dir")
screen=${RADIO_FAVICON_SCREEN:-1400x900x24}
binary="$repo_root/target/debug/reprise"

case "$run_label" in
  before) expected_cache_files=0 ;;
  after) expected_cache_files=3 ;;
  *) expected_cache_files= ;;
esac

required_commands=(cargo convert curl dbus-run-session import openbox sqlite3 Xvfb xdotool)
for command_name in "${required_commands[@]}"; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "missing required command: $command_name" >&2
    exit 2
  fi
done
if [[ ! -f $repo_root/Cargo.toml ]]; then
  echo "not a Reprise repository root: $repo_root" >&2
  exit 2
fi

mkdir -p "$out_dir"
scratch_root=$(mktemp -d "${TMPDIR:-/tmp}/reprise-radio-favicons.XXXXXX")
xdg_data="$scratch_root/xdg-data"
xdg_config="$scratch_root/xdg-config"
xdg_cache="$scratch_root/xdg-cache"
display_file="$scratch_root/display.txt"
xvfb_log="$out_dir/xvfb-$run_label.log"
openbox_log="$out_dir/openbox-$run_label.log"
app_log="$out_dir/app-$run_label.log"
preflight="$out_dir/preflight-$run_label.txt"
cache_evidence="$out_dir/cache-$run_label.txt"
command_evidence="$out_dir/command-$run_label.txt"
database="$xdg_data/reprise/reprise.db"
cache_dir="$xdg_cache/reprise/covers/remote-images-persistent"
xvfb_pid=
openbox_pid=
app_pid=

cleanup() {
  local exit_code=$?
  if [[ -n $app_pid ]]; then
    kill -TERM -- "-$app_pid" 2>/dev/null || true
    sleep 0.2
    kill -KILL -- "-$app_pid" 2>/dev/null || true
  fi
  [[ -z $openbox_pid ]] || kill -KILL "$openbox_pid" 2>/dev/null || true
  [[ -z $xvfb_pid ]] || kill -KILL "$xvfb_pid" 2>/dev/null || true
  rm -rf -- "$scratch_root"
  exit "$exit_code"
}
trap cleanup EXIT

mkdir -p "$xdg_data" "$xdg_config/gtk-4.0" "$xdg_cache"
printf '%s\n' '[Settings]' 'gtk-enable-animations=0' \
  > "$xdg_config/gtk-4.0/settings.ini"

urls=(
  "https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/favicon-32.png"
  "https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/apple-touch-icon-180.png"
  "https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/play-store-icon-512.png"
)

quote_command() {
  printf '%q ' "$@"
  printf '\n'
}

{
  printf 'repository: %s\n' "$repo_root"
  printf 'revision: %s\n' "$(git -C "$repo_root" rev-parse HEAD)"
  printf 'invocation: '
  quote_command env RADIO_FAVICON_OUT_DIR="$out_dir" "$script_path" "$repo_root"
  printf 'build: '
  quote_command cargo build --manifest-path "$repo_root/Cargo.toml" -p reprise-gnome --bin reprise
} > "$command_evidence"

build=(cargo build --manifest-path "$repo_root/Cargo.toml" -p reprise-gnome --bin reprise)
if command -v heavy-run >/dev/null 2>&1; then
  heavy-run medium -- "${build[@]}"
else
  "${build[@]}"
fi

: > "$display_file"
Xvfb -displayfd 8 -screen 0 "$screen" -nolisten tcp \
  8>"$display_file" >"$xvfb_log" 2>&1 &
xvfb_pid=$!
for _ in $(seq 1 50); do
  [[ -s $display_file ]] && break
  sleep 0.1
done
display_number=$(tr -d '[:space:]' < "$display_file")
if [[ -z $display_number ]]; then
  echo "Xvfb did not allocate a display; see $xvfb_log" >&2
  exit 1
fi
display=":$display_number"

DISPLAY="$display" openbox >"$openbox_log" 2>&1 &
openbox_pid=$!
sleep 1

: > "$preflight"
for url in "${urls[@]}"; do
  curl -sS --max-time 20 -o /dev/null -w '%{http_code} %{num_redirects}\n' "$url" \
    >> "$preflight"
done
if [[ $(grep -c '^200 0$' "$preflight") -ne 3 ]]; then
  echo "favicon preflight failed; expected three lines of '200 0'" >&2
  cat "$preflight" >&2
  exit 1
fi

# Open the app once so Core creates and migrates the disposable database.
timeout --foreground 20s dbus-run-session -- env \
  DISPLAY="$display" GDK_BACKEND=x11 WAYLAND_DISPLAY= \
  XDG_DATA_HOME="$xdg_data" XDG_CONFIG_HOME="$xdg_config" XDG_CACHE_HOME="$xdg_cache" \
  GSETTINGS_BACKEND=memory GTK_A11Y=none NO_AT_BRIDGE=1 REPRISE_AUDIO_SINK=fakesink \
  REPRISE_SMOKE_FIRST_RUN=skip \
  REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=2 \
  "$binary" >"$out_dir/schema-$run_label.log" 2>&1

if [[ ! -f $database ]]; then
  echo "schema run did not create $database" >&2
  exit 1
fi

# settings::set_bool_in stores true as the canonical text value "1".
sqlite3 "$database" <<'SQL'
INSERT INTO settings (key, value) VALUES ('online-sources-enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
INSERT INTO settings (key, value) VALUES ('module.artwork.enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
INSERT INTO settings (key, value) VALUES ('module.radio.enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
INSERT INTO settings (key, value) VALUES ('online_sources.first_enable_completed', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
INSERT INTO settings (key, value) VALUES ('onboarding.completed', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;

INSERT INTO radio_stations
  (uuid, name, stream_url, homepage, favicon_url, added_at, removed_at)
VALUES
  ('favicon-32', 'Favicon 32', 'https://stream.invalid/favicon-32', NULL,
   'https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/favicon-32.png',
   1786651200, NULL),
  ('favicon-180', 'Favicon 180', 'https://stream.invalid/favicon-180', NULL,
   'https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/apple-touch-icon-180.png',
   1786651201, NULL),
  ('favicon-512', 'Favicon 512', 'https://stream.invalid/favicon-512', NULL,
   'https://raw.githubusercontent.com/marvinbaudach/reprise/dev/data/brand/play-store-icon-512.png',
   1786651202, NULL);
SQL

if [[ -d $cache_dir ]] && find "$cache_dir" -type f -print -quit | grep -q .; then
  echo "cold-start precondition failed: persistent image cache is not empty" >&2
  exit 1
fi

{
  printf 'app: '
  quote_command setsid dbus-run-session -- env \
    DISPLAY="$display" GDK_BACKEND=x11 WAYLAND_DISPLAY= \
    XDG_DATA_HOME="$xdg_data" XDG_CONFIG_HOME="$xdg_config" XDG_CACHE_HOME="$xdg_cache" \
    GSETTINGS_BACKEND=memory GTK_A11Y=none NO_AT_BRIDGE=1 \
    REPRISE_AUDIO_SINK=fakesink REPRISE_LOG=debug \
    REPRISE_SMOKE_SOURCE=radio REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=25 \
    "$binary"
} >> "$command_evidence"

start_time=$SECONDS
setsid dbus-run-session -- env \
  DISPLAY="$display" GDK_BACKEND=x11 WAYLAND_DISPLAY= \
  XDG_DATA_HOME="$xdg_data" XDG_CONFIG_HOME="$xdg_config" XDG_CACHE_HOME="$xdg_cache" \
  GSETTINGS_BACKEND=memory GTK_A11Y=none NO_AT_BRIDGE=1 \
  REPRISE_AUDIO_SINK=fakesink REPRISE_LOG=debug \
  REPRISE_SMOKE_SOURCE=radio REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=25 \
  "$binary" >"$app_log" 2>&1 &
app_pid=$!

window_id=
for _ in $(seq 1 200); do
  window_id=$(DISPLAY="$display" xdotool search --onlyvisible --class reprise 2>/dev/null | head -1 || true)
  [[ -n $window_id ]] && break
  kill -0 "$app_pid" 2>/dev/null || break
  sleep 0.1
done
if [[ -z $window_id ]]; then
  echo "no mapped Reprise window appeared; see $app_log" >&2
  exit 1
fi

assert_not_blank() {
  local path=$1
  local deviation deviation_integer
  [[ -s $path ]] || { echo "missing screenshot: $path" >&2; return 1; }
  deviation=$(convert "$path" -format '%[standard-deviation]' info: 2>/dev/null || printf '0')
  deviation_integer=${deviation%%.*}
  if [[ -z $deviation_integer || $deviation_integer -lt 50 ]]; then
    echo "blank screenshot (standard-deviation=$deviation): $path" >&2
    return 1
  fi
}

for second in 8 16 23; do
  while (( SECONDS - start_time < second )); do
    sleep 0.2
  done
  screenshot="$out_dir/$run_label-$(printf '%02d' "$second")s.png"
  DISPLAY="$display" import -window "$window_id" "$screenshot"
  assert_not_blank "$screenshot"
done

if ! wait "$app_pid"; then
  echo "Reprise verification run failed; see $app_log" >&2
  app_pid=
  exit 1
fi
app_pid=

plain_log="$scratch_root/app-plain.log"
sed -E 's/\x1B\[[0-9;]*[mK]//g' "$app_log" > "$plain_log"
if ! grep 'smoke: opening detail view through sidebar source routing' "$plain_log" \
  | grep -q 'source=radio'; then
  echo "app log does not prove direct Radio routing; see $app_log" >&2
  exit 1
fi

if [[ -d $cache_dir ]]; then
  cache_count=$(find "$cache_dir" -type f | wc -l)
else
  cache_count=0
fi
printf '%s\n' "$cache_count" > "$cache_evidence"
if [[ -n $expected_cache_files && $cache_count -ne $expected_cache_files ]]; then
  echo "expected $expected_cache_files persistent cache files for $run_label, found $cache_count" >&2
  exit 1
fi

printf 'Radio favicon verification completed: label=%s cache_files=%s evidence=%s\n' \
  "$run_label" "$cache_count" "$out_dir"
