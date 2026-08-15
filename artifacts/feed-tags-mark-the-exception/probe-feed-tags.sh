#!/usr/bin/env bash
# Captures the Updates and Concerts ticket pills against a pinned control build.
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
artifact_dir="$repo_root/artifacts/feed-tags-mark-the-exception"
pinned_base=b6be7cdc61
screen_resolution=1600x900x24
current_worktree_state=${current_worktree_state:-not-recorded}
current_popover_unknown_tag=not-reached
current_popover_on_sale_tag=not-reached
current_table_all_three_ticket_values=not-reached
control_popover_unknown_tag=not-reached
control_popover_on_sale_tag=not-reached

# shellcheck source=../../scripts/cua-common/session.sh
source "$repo_root/scripts/cua-common/session.sh"
# shellcheck source=../../scripts/cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"

required_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "feed-tags probe requires '$1'" >&2
    return 1
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
  local app_pid=$1 response window_id

  for _ in $(seq 1 60); do
    kill -0 "$app_pid" 2>/dev/null || return 1
    response=$(cua_driver list_windows "$(jq -nc --argjson pid "$app_pid" '{pid: $pid}')")
    window_id=$(window_id_from_response <<<"$response")
    if [[ -n "$window_id" ]]; then
      printf '%s\n' "$window_id"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

element_token_for_label() {
  local snapshot_path=$1 label=$2

  jq -er --arg label "$label" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.label == $label)
      | select(.element_token != null)
      | {token: .element_token,
         rank: (if .role == "toggle button" then 0
                elif .role == "button" then 1
                elif .role == "list item" then 2
                else 3 end)}]
    | sort_by(.rank)
    | .[0].token
    | select(. != null)
  ' "$snapshot_path"
}

# The installed driver binds actions to a snapshot token. Keep this wrapper
# local until the shared CUA helper adopts the post-0.17 action contract.
cua_click_label() {
  local app_pid=$1 window_id=$2 label=$3 stem=$4
  local before_path action_path element_token payload

  before_path=$(cua_snapshot "$app_pid" "$window_id" "$stem-before")
  element_token=$(element_token_for_label "$before_path" "$label")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$app_pid" \
    --argjson window_id "$window_id" \
    --arg element_token "$element_token" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_token: $element_token,
      session: $session}')
  if ! cua_driver click "$payload" >"$action_path"; then
    echo "CUA token click command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$app_pid" "$window_id" "$stem-after" >/dev/null
}

profile_database() {
  printf '%s/data/reprise/reprise.db\n' "$1"
}

initialize_profile() {
  local arm=$1 app_binary=$2 profile_root=$3 app_pid database wait_status

  mkdir -p "$profile_root/data" "$profile_root/cache" "$profile_root/config"
  database=$(profile_database "$profile_root")
  env \
    XDG_DATA_HOME="$profile_root/data" \
    XDG_CACHE_HOME="$profile_root/cache" \
    XDG_CONFIG_HOME="$profile_root/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SMOKE_FIRST_RUN=skip \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS=4 \
    "$app_binary" >"$CUA_E2E_OUT_DIR/$arm-schema.log" 2>&1 &
  APP_PID=$!
  CUA_E2E_APP_PID=$APP_PID
  export CUA_E2E_APP_PID
  app_pid=$APP_PID
  for _ in $(seq 1 40); do
    [[ -f "$database" ]] && break
    kill -0 "$app_pid" 2>/dev/null || break
    sleep 0.25
  done
  if [[ ! -f "$database" ]]; then
    echo "$arm did not create its isolated database: $database" >&2
    tail -n 40 "$CUA_E2E_OUT_DIR/$arm-schema.log" >&2 || true
    return 1
  fi
  if wait "$app_pid"; then
    wait_status=0
  else
    wait_status=$?
  fi
  stop_app
  return "$wait_status"
}

seed_profile() {
  local profile_root=$1 database future_date fetched_at

  case "$(profile_database "$profile_root")" in
    "$profile_root"/data/reprise/reprise.db) ;;
    *) echo "refusing to seed a database outside the probe profile" >&2; return 1 ;;
  esac
  database=$(profile_database "$profile_root")
  case "$database" in
    "$CUA_E2E_SCRATCH_ROOT"/*) ;;
    *) echo "refusing to seed outside the run scratchpad: $database" >&2; return 1 ;;
  esac

  future_date=$(date -d '+30 days' +%F)
  fetched_at=$(date +%s)
  sqlite3 "$database" <<SQL
INSERT OR REPLACE INTO settings(key, value)
VALUES ('module.concerts.enabled', '1');
INSERT OR REPLACE INTO settings(key, value)
VALUES ('concerts.bandsintown_app_id', 'probe-fixture');
INSERT OR REPLACE INTO concert_artists(
  artist_key, artist_name, provider, provider_id, mbid_verified, is_similar,
  last_attempt_at, last_outcome, events_found
) VALUES (
  'probe-artist', 'Probe Artist', 'bandsintown', 'probe-artist', 1, 0,
  $fetched_at, 'ok', 3
);
DELETE FROM concert_events;
INSERT INTO concert_events(
  id, artist_key, artist_name, starts_at, date_key, venue, city, region,
  country, latitude, longitude, ticket_url, ticket_source, event_url,
  provider, is_similar, similar_to, fetched_at, seen_at, dedupe_key,
  ticket_availability
) VALUES
  (901, 'probe-on-sale', 'A On Sale Probe', '${future_date}T18:00:00',
   '$future_date', 'Probe Hall', 'Zurich', NULL, 'CH', NULL, NULL,
   'https://tickets.example/on-sale', 'Bandsintown',
   'https://events.example/on-sale', 'bandsintown', 0, NULL, $fetched_at,
   NULL, 'feed-tags-on-sale', 'on_sale'),
  (902, 'probe-off-sale', 'B Off Sale Probe', '${future_date}T19:00:00',
   '$future_date', 'Probe Hall', 'Zurich', NULL, 'CH', NULL, NULL,
   'https://tickets.example/off-sale', 'Bandsintown',
   'https://events.example/off-sale', 'bandsintown', 0, NULL, $fetched_at,
   NULL, 'feed-tags-off-sale', 'off_sale'),
  (903, 'probe-unknown', 'C Unknown Probe', '${future_date}T20:00:00',
   '$future_date', 'Probe Hall', 'Zurich', NULL, 'CH', NULL, NULL,
   'https://tickets.example/unknown', 'Bandsintown',
   'https://events.example/unknown', 'bandsintown', 0, NULL, $fetched_at,
   NULL, 'feed-tags-unknown', 'unknown');
SQL

  if [[ $(sqlite3 "$database" 'SELECT COUNT(*) FROM concert_events;') != 3 ]]; then
    echo "the isolated concert fixture does not contain exactly three rows" >&2
    return 1
  fi
}

start_app() {
  local arm=$1 app_binary=$2 profile_root=$3

  APP_LOG="$CUA_E2E_OUT_DIR/$arm-app.log"
  env \
    XDG_DATA_HOME="$profile_root/data" \
    XDG_CACHE_HOME="$profile_root/cache" \
    XDG_CONFIG_HOME="$profile_root/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SMOKE_FIRST_RUN=skip \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS=120 \
    "$app_binary" >"$APP_LOG" 2>&1 &
  APP_PID=$!
  CUA_E2E_APP_PID=$APP_PID
  export CUA_E2E_APP_PID
  if ! WINDOW_ID=$(wait_for_window "$APP_PID"); then
    echo "$arm did not expose a Reprise window" >&2
    tail -n 60 "$APP_LOG" >&2 || true
    return 1
  fi
}

stop_app() {
  if [[ -n "${APP_PID:-}" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    cua_common_terminate_pid "$APP_PID"
  fi
  APP_PID=""
  WINDOW_ID=""
  CUA_E2E_APP_PID=""
  export CUA_E2E_APP_PID
}

frame_sample_for_label() {
  local snapshot_path=$1 label=$2

  jq -er --arg label "$label" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.label == $label)
      | select(.frame.x != null and .frame.y != null
        and .frame.w != null and .frame.h != null)
      | [((.frame.x + 4) | floor), ((.frame.y + (.frame.h / 2)) | floor)]][0]
    | select(. != null)
    | @tsv
  ' "$snapshot_path"
}

sample_rgb() {
  local png=$1 x=$2 y=$3 rgb

  rgb=$(magick "$png" -crop "1x1+$x+$y" -depth 8 txt:- \
    | tail -1 | sed -E 's/.*\(([0-9]+),([0-9]+),([0-9]+).*/\1 \2 \3/')
  if [[ ! "$rgb" =~ ^[0-9]+\ [0-9]+\ [0-9]+$ ]]; then
    echo "could not read RGB at $x,$y from $png" >&2
    return 1
  fi
  printf '%s\n' "$rgb"
}

cua_desktop_snapshot() {
  local stem=$1 payload
  local json_path="$CUA_E2E_OUT_DIR/$stem.json"
  local screenshot_path="$CUA_E2E_OUT_DIR/$stem.png"

  payload=$(jq -nc \
    --arg session "$CUA_E2E_SESSION" \
    --arg screenshot_out_file "$screenshot_path" \
    '{session: $session, screenshot_out_file: $screenshot_out_file}')
  cua_driver get_desktop_state "$payload" >"$json_path"
  if [[ ! -s "$screenshot_path" ]]; then
    echo "CUA desktop snapshot did not retain screenshot evidence: $stem" >&2
    return 1
  fi
}

escalate_to_desktop_capture() {
  local action_path="$CUA_E2E_OUT_DIR/$1-desktop-escalation.json" payload

  payload=$(jq -nc \
    --arg session "$CUA_E2E_SESSION" \
    --arg detail "GTK popover is present in AT-SPI but omitted from its parent window capture" \
    '{session: $session, reason: "ax_tree_pixel_mismatch", detail: $detail}')
  cua_driver escalate_session "$payload" >"$action_path"
}

restore_window_capture_scope() {
  cua_driver end_session \
    "$(jq -nc --arg session "$CUA_E2E_SESSION" '{session: $session}')" >/dev/null
  CUA_E2E_SESSION="$CUA_E2E_SESSION-window"
  export CUA_E2E_SESSION
  cua_driver start_session \
    "$(jq -nc --arg session "$CUA_E2E_SESSION" \
      '{session: $session, capture_scope: "auto"}')" >/dev/null
}

rgb_difference() {
  local first=$1 second=$2 first_r first_g first_b second_r second_g second_b
  local difference max_difference=0

  read -r first_r first_g first_b <<<"$first"
  read -r second_r second_g second_b <<<"$second"
  for difference in \
    "$((first_r > second_r ? first_r - second_r : second_r - first_r))" \
    "$((first_g > second_g ? first_g - second_g : second_g - first_g))" \
    "$((first_b > second_b ? first_b - second_b : second_b - first_b))"; do
    ((difference <= max_difference)) || max_difference=$difference
  done
  printf '%s\n' "$max_difference"
}

capture_current_arm() {
  local app_binary=$1 profile_root=$2 initial_snapshot popover_snapshot table_snapshot

  initialize_profile current "$app_binary" "$profile_root"
  seed_profile "$profile_root"
  start_app current "$app_binary" "$profile_root"
  initial_snapshot=$(cua_wait_for_label "$APP_PID" "$WINDOW_ID" "Updates" current-ready)
  assert_snapshot_contains "$initial_snapshot" "Concerts"
  cua_click_label "$APP_PID" "$WINDOW_ID" "Updates" current-open-updates
  popover_snapshot=$(cua_wait_for_label "$APP_PID" "$WINDOW_ID" "B Off Sale Probe" current-popover)
  assert_snapshot_contains "$popover_snapshot" "C Unknown Probe"
  assert_snapshot_contains "$popover_snapshot" "Off sale"
  if assert_snapshot_contains "$popover_snapshot" "Unknown"; then
    current_popover_unknown_tag=observed
  else
    current_popover_unknown_tag=not-observed
    return 1
  fi
  if assert_snapshot_absent "$popover_snapshot" "On sale"; then
    current_popover_on_sale_tag=not-observed
  else
    current_popover_on_sale_tag=observed
    return 1
  fi
  cua_driver list_windows "$(jq -nc --argjson pid "$APP_PID" '{pid: $pid}')" \
    >"$CUA_E2E_OUT_DIR/current-popover-windows.json"
  escalate_to_desktop_capture current-popover
  cua_desktop_snapshot 01-popover-tags
  restore_window_capture_scope
  current_popover_snapshot=$popover_snapshot

  cua_click_label "$APP_PID" "$WINDOW_ID" "Concerts" current-open-concerts
  table_snapshot=$(cua_wait_for_label "$APP_PID" "$WINDOW_ID" "Tickets" current-table)
  assert_snapshot_contains "$table_snapshot" "A On Sale Probe"
  assert_snapshot_contains "$table_snapshot" "B Off Sale Probe"
  assert_snapshot_contains "$table_snapshot" "C Unknown Probe"
  assert_snapshot_contains "$table_snapshot" "On sale"
  assert_snapshot_contains "$table_snapshot" "Off sale"
  assert_snapshot_contains "$table_snapshot" "Unknown"
  cua_click_label "$APP_PID" "$WINDOW_ID" "Updates" current-close-updates
  table_snapshot=$(cua_snapshot "$APP_PID" "$WINDOW_ID" current-table-unobscured)
  assert_snapshot_absent "$table_snapshot" "Dismiss"
  if assert_snapshot_contains "$table_snapshot" "On sale" \
    && assert_snapshot_contains "$table_snapshot" "Off sale" \
    && assert_snapshot_contains "$table_snapshot" "Unknown"; then
    current_table_all_three_ticket_values=observed
  else
    current_table_all_three_ticket_values=not-observed
    return 1
  fi
  current_table_snapshot=$table_snapshot
  escalate_to_desktop_capture current-table
  cua_desktop_snapshot 02-concerts-table-tags
  restore_window_capture_scope
  stop_app
}

capture_control_arm() {
  local app_binary=$1 profile_root=$2 popover_snapshot

  initialize_profile control "$app_binary" "$profile_root"
  seed_profile "$profile_root"
  start_app control "$app_binary" "$profile_root"
  cua_wait_for_label "$APP_PID" "$WINDOW_ID" "Updates" control-ready >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Updates" control-open-updates
  popover_snapshot=$(cua_wait_for_label "$APP_PID" "$WINDOW_ID" "B Off Sale Probe" control-popover)
  assert_snapshot_contains "$popover_snapshot" "Off sale"
  if assert_snapshot_absent "$popover_snapshot" "Unknown"; then
    control_popover_unknown_tag=not-observed
  else
    control_popover_unknown_tag=observed
    return 1
  fi
  if assert_snapshot_absent "$popover_snapshot" "On sale"; then
    control_popover_on_sale_tag=not-observed
  else
    control_popover_on_sale_tag=observed
    return 1
  fi
  escalate_to_desktop_capture control-popover
  cua_desktop_snapshot control-01-popover-pinned-base
  control_popover_snapshot=$popover_snapshot
  stop_app
}

measure_pills() {
  local current_popover_coordinates current_table_coordinates control_coordinates

  if ! current_popover_coordinates=$(frame_sample_for_label \
    "$current_popover_snapshot" "Off sale"); then
    PROBE_BLOCKER="Updates Off sale is only nested text inside a composite row; cua-driver exposes no label frame"
    echo "$PROBE_BLOCKER" >&2
    return 1
  fi
  read -r current_popover_x current_popover_y <<<"$current_popover_coordinates"
  current_popover_rgb=$(sample_rgb \
    "$CUA_E2E_OUT_DIR/01-popover-tags.png" "$current_popover_x" "$current_popover_y")

  if ! current_table_coordinates=$(frame_sample_for_label \
    "$current_table_snapshot" "Off sale"); then
    PROBE_BLOCKER="Concerts table Off sale has no usable AT-SPI label frame"
    echo "$PROBE_BLOCKER" >&2
    return 1
  fi
  read -r current_table_x current_table_y <<<"$current_table_coordinates"
  current_table_rgb=$(sample_rgb \
    "$CUA_E2E_OUT_DIR/02-concerts-table-tags.png" "$current_table_x" "$current_table_y")

  if ! control_coordinates=$(frame_sample_for_label \
    "$control_popover_snapshot" "Off sale"); then
    PROBE_BLOCKER="Pinned-control Off sale has no usable AT-SPI label frame"
    echo "$PROBE_BLOCKER" >&2
    return 1
  fi
  read -r control_popover_x control_popover_y <<<"$control_coordinates"
  control_popover_rgb=$(sample_rgb \
    "$CUA_E2E_OUT_DIR/control-01-popover-pinned-base.png" \
    "$control_popover_x" "$control_popover_y")

  current_rgb_difference=$(rgb_difference "$current_popover_rgb" "$current_table_rgb")
  if ((current_rgb_difference > 2)); then
    echo "Off sale fill differs by $current_rgb_difference channels between popover and table" >&2
    return 1
  fi
}

write_manifest() {
  local manifest_path="$CUA_E2E_OUT_DIR/manifest.txt"

  {
    printf 'schema_version=2\n'
    printf 'created_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'current_commit=%s\n' "$current_commit"
    printf 'current_worktree_state=%s\n' "$current_worktree_state"
    printf 'control_commit=%s\n' "$pinned_base"
    printf 'scratch_path=%s\n' "$CUA_E2E_OUT_DIR"
    printf 'display_backend=x11-xvfb\n'
    printf 'screen_resolution=%s\n' "$screen_resolution"
    printf 'cua_driver_version=%s\n' "$(cua-driver --version)"
    printf 'magick_version=%s\n' "$(magick --version | head -1)"
    printf 'current_popover_off_sale_coordinate=%s,%s\n' \
      "$current_popover_x" "$current_popover_y"
    printf 'current_popover_off_sale_rgb=%s\n' "${current_popover_rgb// /,}"
    printf 'current_table_off_sale_coordinate=%s,%s\n' \
      "$current_table_x" "$current_table_y"
    printf 'current_table_off_sale_rgb=%s\n' "${current_table_rgb// /,}"
    printf 'current_popover_table_max_channel_difference=%s\n' "$current_rgb_difference"
    printf 'control_popover_off_sale_coordinate=%s,%s\n' \
      "$control_popover_x" "$control_popover_y"
    printf 'control_popover_off_sale_rgb=%s\n' "${control_popover_rgb// /,}"
    printf 'current_popover_unknown_tag=%s\n' "$current_popover_unknown_tag"
    printf 'current_popover_on_sale_tag=%s\n' "$current_popover_on_sale_tag"
    printf 'current_table_all_three_ticket_values=%s\n' \
      "$current_table_all_three_ticket_values"
    printf 'control_popover_unknown_tag=%s\n' "$control_popover_unknown_tag"
    printf 'control_popover_on_sale_tag=%s\n' "$control_popover_on_sale_tag"
    printf 'current_popover_image=%s/01-popover-tags.png\n' "$CUA_E2E_OUT_DIR"
    printf 'current_table_image=%s/02-concerts-table-tags.png\n' "$CUA_E2E_OUT_DIR"
    printf 'control_popover_image=%s/control-01-popover-pinned-base.png\n' \
      "$CUA_E2E_OUT_DIR"
  } >"$manifest_path"
  cp "$manifest_path" "$artifact_dir/manifest.txt"
}

write_blocked_manifest() {
  local manifest_path="$CUA_E2E_OUT_DIR/manifest.txt"

  {
    printf 'schema_version=2\n'
    printf 'status=blocked\n'
    printf 'created_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'current_commit=%s\n' "$current_commit"
    printf 'current_worktree_state=%s\n' "$current_worktree_state"
    printf 'control_commit=%s\n' "$pinned_base"
    printf 'scratch_path=%s\n' "$CUA_E2E_OUT_DIR"
    printf 'display_backend=x11-xvfb\n'
    printf 'screen_resolution=%s\n' "$screen_resolution"
    printf 'cua_driver_version=%s\n' "$(cua-driver --version)"
    printf 'magick_version=%s\n' "$(magick --version | head -1)"
    printf 'current_popover_unknown_tag=%s\n' "$current_popover_unknown_tag"
    printf 'current_popover_on_sale_tag=%s\n' "$current_popover_on_sale_tag"
    printf 'current_table_all_three_ticket_values=%s\n' \
      "$current_table_all_three_ticket_values"
    printf 'control_popover_unknown_tag=%s\n' "$control_popover_unknown_tag"
    printf 'control_popover_on_sale_tag=%s\n' "$control_popover_on_sale_tag"
    printf 'current_popover_image=%s/01-popover-tags.png\n' "$CUA_E2E_OUT_DIR"
    printf 'current_table_image=%s/02-concerts-table-tags.png\n' "$CUA_E2E_OUT_DIR"
    printf 'control_popover_image=%s/control-01-popover-pinned-base.png\n' \
      "$CUA_E2E_OUT_DIR"
    printf 'missing_verification=%s\n' "${PROBE_BLOCKER:-probe exited before measurements completed}"
    printf 'durable_fallback=the_popover_ticket_pills_render_exactly_as_the_table_pills\n'
  } >"$manifest_path"
  cp "$manifest_path" "$artifact_dir/manifest.txt"
}

private_cleanup() {
  local exit_code=$?

  if ((exit_code != 0)) && [[ ! -s "$CUA_E2E_OUT_DIR/manifest.txt" ]]; then
    write_blocked_manifest
  fi
  stop_app
  cua_common_stop_driver "$CUA_E2E_SESSION"
  rm -f -- "$CUA_DRIVER_SOCKET"
  exit "$exit_code"
}

run_private_session() {
  local current_binary=$1 control_binary=$2 current_profile control_profile

  trap private_cleanup EXIT
  # AF_UNIX limits socket paths to roughly 108 bytes; the evidence scratchpad
  # can be much deeper, so keep this transient socket under the worktree target.
  CUA_DRIVER_SOCKET="$repo_root/target/ft-$run_id.sock"
  export CUA_DRIVER_SOCKET
  cua_common_start_driver "$CUA_E2E_OUT_DIR" "$CUA_DRIVER_SOCKET" "$CUA_E2E_SESSION"
  current_profile="$CUA_E2E_SCRATCH_ROOT/current-profile"
  control_profile="$CUA_E2E_SCRATCH_ROOT/control-profile"
  capture_current_arm "$current_binary" "$current_profile"
  capture_control_arm "$control_binary" "$control_profile"
  measure_pills
  write_manifest
}

if [[ "${1:-}" == "--private-session" ]]; then
  shift
  pinned_base=$(git -C "$repo_root" rev-parse --verify "$pinned_base^{commit}")
  run_private_session "$@"
  exit 0
fi

for required in cargo cua-driver dbus-run-session git head jq magick openbox sed \
  sqlite3 tail tar timeout Xvfb; do
  required_command "$required"
done
for required_executable in /usr/lib/at-spi-bus-launcher /usr/lib/at-spi2-registryd; do
  if [[ ! -x "$required_executable" ]]; then
    echo "feed-tags probe requires '$required_executable'" >&2
    exit 2
  fi
done

current_commit=$(git -C "$repo_root" rev-parse HEAD)
pinned_base=$(git -C "$repo_root" rev-parse --verify "$pinned_base^{commit}")
if [[ -n "${REPRISE_FEED_TAGS_CONTROL_BINARY:-}" ]]; then
  echo "REPRISE_FEED_TAGS_CONTROL_BINARY is unsupported because its provenance cannot be verified" >&2
  exit 2
fi
if [[ -n $(git -C "$repo_root" status --porcelain) ]]; then
  current_worktree_state=dirty
else
  current_worktree_state=clean
fi
cargo build -p reprise-gnome
current_binary="$repo_root/target/debug/reprise"

scratch_base=${REPRISE_FEED_TAGS_SCRATCH_BASE:-"$HOME/.cache/reprise-scratch"}
mkdir -p "$scratch_base"
scratch_root=$(mktemp -d "$scratch_base/feed-tags.XXXXXX")
case "$scratch_root" in
  "$scratch_base"/feed-tags.*) ;;
  *) echo "mktemp returned an unexpected scratch path: $scratch_root" >&2; exit 1 ;;
esac
output_dir="$scratch_root/evidence"
run_id=${scratch_root##*.}
private_runtime="$repo_root/target/rt-$run_id"
mkdir -p "$output_dir"
control_source=""
control_target=""
control_source="$scratch_root/control-source"
control_target="$scratch_root/control-target"
mkdir -p "$control_source"
git -C "$repo_root" archive "$pinned_base" | tar -x -C "$control_source"
(
  cd "$control_source"
  REPRISE_GIT_SHA="$pinned_base" CARGO_TARGET_DIR="$control_target" \
    cargo build -p reprise-gnome
)
control_binary="$control_target/debug/reprise"

outer_cleanup() {
  local exit_code=$?

  cua_common_stop_display
  case "$control_source" in
    "$scratch_root"/control-source) rm -r -- "$control_source" ;;
  esac
  case "$control_target" in
    "$scratch_root"/control-target) rm -r -- "$control_target" ;;
  esac
  case "$private_runtime" in
    "$repo_root"/target/rt-*) rm -r -- "$private_runtime" ;;
  esac
  printf 'feed-tags evidence retained at %s\n' "$output_dir"
  exit "$exit_code"
}
trap outer_cleanup EXIT

CUA_E2E_OUT_DIR="$output_dir"
CUA_E2E_SCRATCH_ROOT="$scratch_root"
CUA_E2E_SESSION="feed-tags-$$"
CUA_DRIVER_BIN=${CUA_DRIVER_BIN:-cua-driver}
export CUA_E2E_OUT_DIR CUA_E2E_SCRATCH_ROOT CUA_E2E_SESSION CUA_DRIVER_BIN
cua_common_start_display "$output_dir" "$scratch_root" "$screen_resolution"
cua_common_exec_private \
  "$private_runtime" "$scratch_root/private-root" env \
  -u GNOME_KEYRING_CONTROL -u GNOME_KEYRING_PID \
  CUA_E2E_OUT_DIR="$output_dir" \
  CUA_E2E_SCRATCH_ROOT="$scratch_root" \
  CUA_E2E_SESSION="$CUA_E2E_SESSION" \
  CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
  current_commit="$current_commit" \
  current_worktree_state="$current_worktree_state" \
  screen_resolution="$screen_resolution" \
  run_id="$run_id" \
  "$0" --private-session "$current_binary" "$control_binary"

printf 'feed-tags probe passed; manifest: %s\n' "$artifact_dir/manifest.txt"
