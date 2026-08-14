#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
acceptance_root="$repo_root/acceptance/deezer-placeholder-portraits"
readonly UNIX_SOCKET_PATH_MAX=107
readonly CUA_MAX_ELEMENTS=500
# Every sidebar row reports an AT-SPI frame of 0x0, so the row can only be hit
# by a measured pixel. The y therefore depends on how many rows sit above it,
# which changes with the library's playlists — re-measure it from
# <run>/before/cua/before-open-stats-before.png whenever the sidebar shrinks or
# grows. Measured 2026-08-14 at 15:30 CEST: rows are pitched 38 px apart and
# My Stats is the twelfth and last one.
readonly MY_STATS_CLICK_X=100
readonly MY_STATS_CLICK_Y=615
readonly SHOW_MORE_ARTISTS_CLICK_X=390
readonly SHOW_MORE_ARTISTS_CLICK_Y=640
readonly RENDERED_TOP_ARTIST_RANKS=20
readonly PORTRAIT_REPAINT_MARGIN_SECONDS=2
# The fingerprint exists for the silhouette that hides behind an ordinary,
# artist-specific image identifier: the baseline's fixed list cannot reach those.
# Four artists in this library carry one (measured 2026-08-14, see the table in
# docs/plans/portrait-placeholder-fingerprint.md). Only a rendered rank fetches a
# portrait at all, and those four sit at ranks 40, 122, 131 and — with zero plays
# — in no ranking whatsoever. Rendering that far would need a product switch the
# baseline arm cannot carry (its tree comes from `git archive origin/dev`, no
# patches) plus scrolling this harness does not do. The run copy therefore
# receives listen events that lift the four into the rendered ranking instead.
# Both arms get the identical seeded copy, so the before/after difference is
# untouched; the seeding is declared in each arm's seeded-ranking-proof.txt.
readonly SEEDED_SILHOUETTE_ARTISTS=("Aetheriality" "In Your Grave" "Our Vices" "Wake Me")
# Slot spacing below the fifteenth real artist: small enough to leave ranks 1-15
# alone, large enough to keep the four in a stable order.
readonly SEED_RANK_STEP_MS=1000
# Spread each top-up over this many events, so a seeded artist reads like
# listening history instead of one implausible three-quarter-hour play.
readonly SEED_EVENTS_PER_ARTIST=12
private_runtime_root=""
ACCEPT_CUA_MAX_DEPTH=20

# Reuse the repository's private X11/D-Bus/AT-SPI lifecycle and CUA helpers.
# shellcheck source=../../scripts/cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"
# shellcheck source=../../scripts/cua-common/session.sh
source "$repo_root/scripts/cua-common/session.sh"

# The complete Reprise tree includes large virtualized Library and queue
# surfaces. Asking cua-driver to walk it without a depth bound exceeds its
# deadline and degrades to X11 window metadata even though AT-SPI is healthy.
# Each scenario phase raises this only as far as its required controls live.
cua_snapshot() {
  local pid=$1 window_id=$2 stem=$3
  local json_path="$CUA_E2E_OUT_DIR/$stem.json"
  local screenshot_path="$CUA_E2E_OUT_DIR/$stem.png"
  local payload

  mkdir -p "$CUA_E2E_OUT_DIR"
  payload=$(snapshot_payload \
    "$pid" "$window_id" "$CUA_E2E_SESSION" "$screenshot_path")
  if ! cua_driver get_window_state "$payload" >"$json_path"; then
    echo "CUA snapshot command failed at $stem; evidence: $json_path" >&2
    return 1
  fi
  if ! jq -e . "$json_path" >/dev/null 2>&1; then
    echo "CUA snapshot returned invalid JSON at $stem; evidence: $json_path" >&2
    return 1
  fi
  assert_accessible_snapshot "$json_path" "$stem" || return 1
  if [[ ! -s "$screenshot_path" ]]; then
    echo "CUA snapshot retained no screenshot at $stem: $screenshot_path" >&2
    return 1
  fi
  printf '%s\n' "$json_path"
}

cua_wait_for_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 24); do
    if snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt"); then
      if assert_snapshot_contains "$snapshot_path" "$label" 2>/dev/null; then
        printf '%s\n' "$snapshot_path"
        return 0
      fi
    fi
    sleep 0.25
  done
  echo "window never exposed expected accessible label '$label'; last evidence: $CUA_E2E_OUT_DIR/$stem-24.json" >&2
  return 1
}

# The isolated GTK tree exposes labels reliably, but reports every sidebar
# action at (0, 0); cua-driver consequently cannot activate those controls by
# accessibility token. The window is fixed at 1560x1160, so retain screenshots
# around the two manually measured pixel actions and prove their outcomes from
# the AT-SPI tree immediately afterward.
x11_click_pixel() {
  local pid=$1 window_id=$2 x=$3 y=$4 stem=$5
  local action_path

  cua_snapshot "$pid" "$window_id" "$stem-before" >/dev/null
  action_path="$CUA_E2E_OUT_DIR/$stem-action.txt"
  if ! xdotool mousemove --sync --window "$window_id" "$x" "$y" click 1; then
    echo "private X11 pixel click failed at $stem; evidence: $action_path" >&2
    return 1
  fi
  printf 'method=xdotool\nwindow_id=%s\nx=%s\ny=%s\n' \
    "$window_id" "$x" "$y" >"$action_path"
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null
}

usage() {
  cat <<'EOF'
usage: acceptance/deezer-placeholder-portraits/run-accept.sh \
  --source-db PATH \
  --placeholder-reference PATH \
  --placeholder-reference PATH \
  --confirm-read-only-copy \
  [--output-dir PATH]

Runs the before/after My Stats acceptance on private Xvfb, Openbox, D-Bus,
AT-SPI, and XDG roots. The source database is opened read-only and copied with
SQLite's online backup API, which includes committed WAL frames. The copied
database is the only database modified.

The two placeholder references must be the byte-distinct Deezer silhouettes
identified during the survey. They are read only and are never removed.

The output directory must be inside this worktree. If omitted, evidence is
written below acceptance/deezer-placeholder-portraits/runs/.

This script builds origin/dev and the current HEAD locally. It uses the real
Deezer service and therefore is not an offline test.
EOF
}

required_command() {
  if ! command -v "$1" >/dev/null; then
    echo "required command is unavailable: $1" >&2
    exit 2
  fi
}

validate_unix_socket_path() {
  local socket_path=$1 path_bytes
  path_bytes=$(LC_ALL=C printf '%s' "$socket_path" | wc -c)
  if ((path_bytes > UNIX_SOCKET_PATH_MAX)); then
    echo "CUA driver socket path is $path_bytes bytes; it exceeds the $UNIX_SOCKET_PATH_MAX-byte AF_UNIX limit: $socket_path" >&2
    return 1
  fi
}

allocate_private_runtime_root() {
  local runtime_base=${XDG_RUNTIME_DIR:-/tmp}
  local longest_socket="$runtime_base/reprise-deezer.XXXXXX/runtime-before/cua-driver.sock"
  if [[ ! -d "$runtime_base" || ! -w "$runtime_base" ]] \
    || ! validate_unix_socket_path "$longest_socket" >/dev/null 2>&1; then
    runtime_base=/tmp
  fi
  private_runtime_root=$(mktemp -d "$runtime_base/reprise-deezer.XXXXXX")
}

cleanup_private_runtime_root() {
  if [[ -z "$private_runtime_root" || ! -d "$private_runtime_root" ]]; then
    private_runtime_root=""
    return 0
  fi
  if ! find "$private_runtime_root" -xdev -depth -delete; then
    return 1
  fi
  private_runtime_root=""
}

remove_private_driver_socket() {
  if [[ -n "${CUA_DRIVER_SOCKET:-}" ]] \
    && [[ -e "$CUA_DRIVER_SOCKET" || -S "$CUA_DRIVER_SOCKET" ]]; then
    find "$CUA_DRIVER_SOCKET" -maxdepth 0 -delete
  fi
}

self_test_private_paths() {
  local max_socket known_bad_socket
  max_socket="/tmp/$(printf 'x%.0s' {1..102})"
  known_bad_socket="/tmp/$(printf 'x%.0s' {1..103})"
  local error_output

  validate_unix_socket_path "$max_socket"
  if error_output=$(validate_unix_socket_path "$known_bad_socket" 2>&1); then
    echo "known overlong CUA socket path passed validation" >&2
    return 1
  fi
  if [[ "$error_output" != *"107-byte AF_UNIX limit"* ]]; then
    echo "overlong CUA socket path did not produce the expected diagnostic" >&2
    return 1
  fi

  allocate_private_runtime_root
  local allocated_root=$private_runtime_root
  local label runtime_dir socket_path path_bytes
  for label in before after; do
    runtime_dir="$private_runtime_root/runtime-$label"
    socket_path="$runtime_dir/cua-driver.sock"
    validate_unix_socket_path "$socket_path"
    path_bytes=$(LC_ALL=C printf '%s' "$socket_path" | wc -c)
    if ((path_bytes > 107)); then
      echo "allocated CUA socket path exceeds the independent 107-byte limit" >&2
      return 1
    fi
    mkdir -p "$runtime_dir"
    : >"$socket_path"
  done
  cleanup_private_runtime_root
  if [[ -e "$allocated_root" ]]; then
    echo "private runtime root survived cleanup: $allocated_root" >&2
    return 1
  fi

  echo "private_path_self_test=passed"
}

self_test_private_atspi() {
  local fixture_root degraded_json healthy_json diagnostic payload
  mkdir -p "$acceptance_root/runs"
  fixture_root=$(mktemp -d "$acceptance_root/runs/atspi-contract.XXXXXX")
  degraded_json="$fixture_root/degraded.json"
  healthy_json="$fixture_root/healthy.json"
  printf '%s\n' '{"degraded":true,"degraded_reason":"synthetic AT-SPI failure"}' >"$degraded_json"
  printf '%s\n' '{"degraded":false}' >"$healthy_json"

  if diagnostic=$(assert_accessible_snapshot "$degraded_json" contract-degraded 2>&1); then
    echo "degraded snapshot passed the harness contract" >&2
    return 1
  fi
  if [[ "$diagnostic" != *"synthetic AT-SPI failure"* \
    || "$diagnostic" != *"$degraded_json"* ]]; then
    echo "degraded snapshot omitted its reason or evidence path" >&2
    return 1
  fi
  assert_accessible_snapshot "$healthy_json" contract-healthy

  payload=$(snapshot_payload 7 11 contract-session "$fixture_root/snapshot.png")
  jq -e --argjson depth "$ACCEPT_CUA_MAX_DEPTH" \
    --argjson elements "$CUA_MAX_ELEMENTS" \
    '.max_depth == $depth and .max_elements == $elements' <<<"$payload" >/dev/null
  [[ "$MY_STATS_CLICK_X,$MY_STATS_CLICK_Y" == "100,615" ]]
  [[ "$SHOW_MORE_ARTISTS_CLICK_X,$SHOW_MORE_ARTISTS_CLICK_Y" == "390,640" ]]
  find "$fixture_root" -xdev -depth -delete
  echo "private_atspi_self_test=passed"
}

self_test_rendered_portrait_wait() {
  local fixture_root diagnostic
  fixture_root=$(mktemp -d /tmp/reprise-portrait-wait.XXXXXX)
  mkdir -p "$fixture_root/cache"
  : >"$fixture_root/cache/unrelated.tmp"
  : >"$fixture_root/cache/1111111111111111.jpg"
  : >"$fixture_root/cache/2222222222222222.notfound"

  if diagnostic=$(wait_for_rendered_portraits \
    contract "$fixture_root/cache" 1 3 2>&1); then
    echo "rendered portrait wait passed without every rank settling" >&2
    return 1
  fi
  if [[ "$diagnostic" != *"2 of 3 rendered portraits"* ]]; then
    echo "rendered portrait wait omitted its observed and expected counts" >&2
    return 1
  fi

  : >"$fixture_root/cache/3333333333333333.png"
  wait_for_rendered_portraits contract "$fixture_root/cache" 1 3
  find "$fixture_root" -xdev -depth -delete
  echo "rendered_portrait_wait_self_test=passed"
}

snapshot_payload() {
  local pid=$1 window_id=$2 session=$3 screenshot_path=$4
  jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg session "$session" \
    --arg screenshot_out_file "$screenshot_path" \
    --argjson max_depth "$ACCEPT_CUA_MAX_DEPTH" \
    --argjson max_elements "$CUA_MAX_ELEMENTS" \
    '{pid: $pid, window_id: $window_id, session: $session,
      screenshot_out_file: $screenshot_out_file,
      max_depth: $max_depth, max_elements: $max_elements}'
}

assert_accessible_snapshot() {
  local json_path=$1 stem=$2 reason
  if jq -e '.degraded == true' "$json_path" >/dev/null; then
    reason=$(jq -r '.degraded_reason // "no degraded reason supplied"' "$json_path")
    echo "CUA snapshot degraded at $stem: $reason; evidence: $json_path" >&2
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
  local pid=$1 response window_id

  for _ in $(seq 1 80); do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    response=$(cua_driver list_windows "$(jq -nc --argjson pid "$pid" '{pid: $pid}')")
    window_id=$(window_id_from_response <<<"$response")
    if [[ -n "$window_id" ]]; then
      printf '%s\n' "$window_id"
      return 0
    fi
    sleep 0.25
  done
  return 1
}

private_atspi_address() {
  local output_dir=$1 reply address owner
  reply=$(gdbus call --session --dest org.a11y.Bus \
    --object-path /org/a11y/bus --method org.a11y.Bus.GetAddress)
  address=$(sed -n "s/^('\([^']*\)',)/\1/p" <<<"$reply")
  if [[ -z "$address" ]]; then
    echo "private AT-SPI bus returned no address: $reply" >&2
    return 1
  fi
  for _ in $(seq 1 40); do
    owner=$(gdbus call --address "$address" --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner org.a11y.atspi.Registry 2>/dev/null \
      || true)
    if [[ "$owner" == "(true,)" ]]; then
      printf 'address=%s\nregistry_owner=true\n' "$address" >"$output_dir/atspi-bus.txt"
      printf '%s\n' "$address"
      return 0
    fi
    sleep 0.1
  done
  printf 'address=%s\nregistry_owner=false\n' "$address" >"$output_dir/atspi-bus.txt"
  echo "private AT-SPI registry never owned its bus name; evidence: $output_dir/atspi-bus.txt" >&2
  return 1
}

private_run_cleanup() {
  local exit_code=$?
  if [[ -n "${ACCEPT_APP_PID:-}" ]] && kill -0 "$ACCEPT_APP_PID" 2>/dev/null; then
    kill -TERM "$ACCEPT_APP_PID" 2>/dev/null || true
    for _ in {1..20}; do
      if ! kill -0 "$ACCEPT_APP_PID" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$ACCEPT_APP_PID" 2>/dev/null; then
      kill -KILL "$ACCEPT_APP_PID" 2>/dev/null || true
    fi
    wait "$ACCEPT_APP_PID" 2>/dev/null || true
  fi
  cua_common_stop_driver "$CUA_E2E_SESSION"
  remove_private_driver_socket
  exit "$exit_code"
}

portrait_outcome_count() {
  local portrait_dir=$1
  if [[ ! -d "$portrait_dir" ]]; then
    echo 0
    return
  fi
  find "$portrait_dir" -maxdepth 1 -type f -regextype posix-extended \
    -regex '.*/[0-9a-f]{16}\.(jpg|jpeg|png|webp|gif|bmp|notfound)' \
    -printf '%f\n' | wc -l
}

wait_for_rendered_portraits() {
  local label=$1 portrait_dir=$2 wait_seconds=$3 expected=$4
  local deadline=$((SECONDS + wait_seconds)) observed=0

  while ((SECONDS < deadline)); do
    observed=$(portrait_outcome_count "$portrait_dir")
    if ((observed >= expected)); then
      return 0
    fi
    sleep 1
  done

  echo "$label cached only $observed of $expected rendered portraits within ${wait_seconds}s" >&2
  return 1
}

run_private_acceptance() {
  local label=$1 binary=$2 output_dir=$3
  local app_log="$output_dir/app.log"
  local portrait_dir="$XDG_CACHE_HOME/reprise/artist-portraits"
  local window_id final_snapshot atspi_address settled_count settled_utc capture_utc

  export CUA_E2E_OUT_DIR="$output_dir/cua"
  export CUA_E2E_SESSION="deezer-portrait-$label"
  if [[ -z "${CUA_DRIVER_SOCKET:-}" ]]; then
    echo "private CUA driver socket path was not provided" >&2
    return 2
  fi
  validate_unix_socket_path "$CUA_DRIVER_SOCKET"
  trap private_run_cleanup EXIT
  mkdir -p "$CUA_E2E_OUT_DIR"

  if [[ -e "$portrait_dir" ]]; then
    echo "$label portrait cache must be absent before launch: $portrait_dir" >&2
    return 1
  fi
  printf 'portrait_cache_absent_before_launch=true\n' >"$output_dir/cache-before.txt"

  cua_common_start_driver "$output_dir" "$CUA_DRIVER_SOCKET" "$CUA_E2E_SESSION"
  atspi_address=$(private_atspi_address "$output_dir")

  env \
    AT_SPI_BUS_ADDRESS="$atspi_address" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_LOG=debug \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS=90 \
    "$binary" >"$app_log" 2>&1 &
  ACCEPT_APP_PID=$!
  export CUA_E2E_APP_PID="$ACCEPT_APP_PID"

  if ! window_id=$(wait_for_window "$ACCEPT_APP_PID"); then
    echo "$label did not expose a Reprise window" >&2
    tail -n 80 "$app_log" >&2 || true
    return 1
  fi
  wmctrl -ir "$window_id" -e 0,0,0,1560,1160
  ACCEPT_CUA_MAX_DEPTH=20
  cua_wait_for_label \
    "$ACCEPT_APP_PID" "$window_id" "My Stats" "$label-atspi-ready" >/dev/null
  x11_click_pixel \
    "$ACCEPT_APP_PID" "$window_id" \
    "$MY_STATS_CLICK_X" "$MY_STATS_CLICK_Y" "$label-open-stats"

  ACCEPT_CUA_MAX_DEPTH=40
  # Prove the pixel landed before waiting on any artist. A stale coordinate is
  # otherwise indistinguishable from a missing artist: the run spins out the
  # retries below and blames the ranking for a click that never opened the view.
  if ! cua_wait_for_label \
    "$ACCEPT_APP_PID" "$window_id" "Show more top artists" \
    "$label-stats-opened" >/dev/null; then
    echo "the My Stats click at ($MY_STATS_CLICK_X, $MY_STATS_CLICK_Y) did not open My Stats; re-measure the sidebar row in $CUA_E2E_OUT_DIR/$label-open-stats-before.png" >&2
    return 1
  fi
  final_snapshot=$(cua_wait_for_label \
    "$ACCEPT_APP_PID" "$window_id" "The Devil Wears Prada" "$label-stats-ready")
  x11_click_pixel \
    "$ACCEPT_APP_PID" "$window_id" \
    "$SHOW_MORE_ARTISTS_CLICK_X" "$SHOW_MORE_ARTISTS_CLICK_Y" \
    "$label-expand-top-artists"
  final_snapshot=$(cua_wait_for_label \
    "$ACCEPT_APP_PID" "$window_id" "Hide more top artists" "$label-stats-expanded")
  assert_snapshot_contains "$final_snapshot" "Oceano"
  # The empty isolated cache receives one terminal image or .notfound outcome
  # for every expanded top-artist rank. Wait for all rendered ranks, then give
  # GTK a repaint margin measured from the last completed portrait fetch.
  wait_for_rendered_portraits \
    "$label" "$portrait_dir" 60 "$RENDERED_TOP_ARTIST_RANKS"
  settled_count=$(portrait_outcome_count "$portrait_dir")
  settled_utc=$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)
  sleep "$PORTRAIT_REPAINT_MARGIN_SECONDS"
  capture_utc=$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)
  printf 'rendered_ranks=%s\nsettled_outcomes=%s\nsettled_utc=%s\nrepaint_margin_seconds=%s\ncapture_ready_utc=%s\n' \
    "$RENDERED_TOP_ARTIST_RANKS" "$settled_count" "$settled_utc" \
    "$PORTRAIT_REPAINT_MARGIN_SECONDS" "$capture_utc" \
    >"$output_dir/portrait-settle.txt"
  final_snapshot=$(cua_snapshot "$ACCEPT_APP_PID" "$window_id" "$label-stats-final")
  assert_snapshot_contains "$final_snapshot" "The Devil Wears Prada"
  assert_snapshot_contains "$final_snapshot" "Oceano"
  import -window "$window_id" "$output_dir/my-stats.png"

  if [[ ! -d "$portrait_dir" ]]; then
    echo "$label created no portrait cache after opening My Stats" >&2
    return 1
  fi
  find "$portrait_dir" -maxdepth 1 -type f -printf '%f\t%s bytes\n' \
    | sort >"$output_dir/cache-listing.txt"
  find "$portrait_dir" -maxdepth 1 -type f ! -name '*.notfound' -print0 \
    | sort -z | xargs -0 -r sha256sum >"$output_dir/cache-sha256.txt"
  if [[ ! -s "$output_dir/cache-sha256.txt" ]]; then
    echo "$label produced no portrait image; the acceptance cannot be green" >&2
    return 1
  fi
  if rg --quiet 'artist portrait request failed' "$app_log"; then
    echo "$label logged a portrait request failure" >&2
    rg 'artist portrait request failed' "$app_log" >&2
    return 1
  fi

  wait "$ACCEPT_APP_PID"
  ACCEPT_APP_PID=""
  export CUA_E2E_APP_PID=""
  cua_common_stop_driver "$CUA_E2E_SESSION"
  remove_private_driver_socket
  trap - EXIT
}

# Mirrors the ranking arithmetic of crates/reprise-core/src/library/stats_screen.rs:240:
# the effective album artist, and the play time clamped to the track duration.
# Shared by the seeding step and its self test so both measure the same thing.
seed_ranking_cte() {
  cat <<'SQL'
WITH events AS (
  SELECT CASE WHEN TRIM(album_artist) <> '' THEN album_artist ELSE artist END AS raw,
         CASE WHEN duration_ms > 0 THEN MIN(ms_played, duration_ms) ELSE ms_played END AS clamped
  FROM listen_events
),
totals AS (
  SELECT raw, SUM(clamped) AS total FROM events WHERE TRIM(raw) <> '' GROUP BY raw
)
SQL
}

# Lift SEEDED_SILHOUETTE_ARTISTS into the rendered ranking of one run copy.
# Anchors on the copy's own ranking rather than on measured numbers: ranks move
# with every play, and a hard-coded total rots exactly the way MY_STATS_CLICK_Y
# did. The four land immediately below the fifteenth real artist, so every rank
# the earlier evidence names keeps its place.
seed_silhouette_ranks() {
  local database=$1 label=$2
  local proof="$output_dir/$label/seeded-ranking-proof.txt"
  local cte quoted artist escaped slots index anchors anchor keep_total
  local existing target topup per_event remainder has_history verified
  local expected_rank rank name line
  local -a report=()

  cte=$(seed_ranking_cte)
  slots=${#SEEDED_SILHOUETTE_ARTISTS[@]}
  quoted=""
  for artist in "${SEEDED_SILHOUETTE_ARTISTS[@]}"; do
    escaped=${artist//\'/\'\'}
    quoted+="${quoted:+, }'$escaped'"
  done

  anchors=$(sqlite3 "$database" "$cte,
others AS (
  SELECT total, ROW_NUMBER() OVER (ORDER BY total DESC) AS rank
  FROM totals WHERE raw NOT IN ($quoted)
)
SELECT COALESCE((SELECT total FROM others WHERE rank = 15), 0) || '|' ||
       COALESCE((SELECT total FROM others WHERE rank = 16), 0);")
  keep_total=${anchors%%|*}
  anchor=${anchors##*|}
  if [[ -z "$anchor" ]] || (( anchor <= 0 )); then
    echo "the run copy ranks fewer than 16 unseeded artists; seeding would rewrite the visible ranking" >&2
    return 1
  fi
  if (( keep_total <= anchor + slots * SEED_RANK_STEP_MS )); then
    echo "ranks 15 and 16 are $((keep_total - anchor)) ms apart, too close to hold $slots seeded artists" >&2
    return 1
  fi

  index=0
  for artist in "${SEEDED_SILHOUETTE_ARTISTS[@]}"; do
    escaped=${artist//\'/\'\'}
    index=$((index + 1))
    target=$((anchor + (slots - index + 1) * SEED_RANK_STEP_MS))
    existing=$(sqlite3 "$database" "$cte
SELECT COALESCE((SELECT total FROM totals WHERE raw = '$escaped'), 0);")
    topup=$((target - existing))
    per_event=$((topup / SEED_EVENTS_PER_ARTIST))
    if (( per_event <= 0 )); then
      echo "$artist already reaches rank ${index} territory on its own; the ranking moved under this run" >&2
      return 1
    fi
    remainder=$((topup - per_event * SEED_EVENTS_PER_ARTIST))
    has_history=$(sqlite3 "$database" \
      "SELECT COUNT(*) FROM listen_events
       WHERE (CASE WHEN TRIM(album_artist) <> '' THEN album_artist ELSE artist END) = '$escaped';")
    # An artist with history is topped up through a copy of its own newest event,
    # so the seeded rows fold into the same ranking group instead of forming a
    # second one beside it. Aetheriality has no history at all — that is why no
    # raised rank cap could ever have shown it — and gets literal rows.
    if (( has_history > 0 )); then
      sqlite3 "$database" <<SQL
WITH RECURSIVE seq(i) AS (
  SELECT 1 UNION ALL SELECT i + 1 FROM seq WHERE i < $SEED_EVENTS_PER_ARTIST
),
template AS (
  SELECT track_id, title, artist, album, album_artist, genre, path, artist_mbid
  FROM listen_events
  WHERE (CASE WHEN TRIM(album_artist) <> '' THEN album_artist ELSE artist END) = '$escaped'
  ORDER BY played_at DESC
  LIMIT 1
)
INSERT INTO listen_events(
  track_id, played_at, ms_played, title, artist, album, album_artist,
  genre, duration_ms, path, artist_mbid)
SELECT template.track_id,
       COALESCE((SELECT MAX(played_at) FROM listen_events), unixepoch()) - seq.i * 3600,
       $per_event + CASE WHEN seq.i = 1 THEN $remainder ELSE 0 END,
       template.title, template.artist, template.album, template.album_artist,
       template.genre, 0, template.path, template.artist_mbid
FROM seq, template;
SQL
    else
      sqlite3 "$database" <<SQL
WITH RECURSIVE seq(i) AS (
  SELECT 1 UNION ALL SELECT i + 1 FROM seq WHERE i < $SEED_EVENTS_PER_ARTIST
)
INSERT INTO listen_events(
  track_id, played_at, ms_played, title, artist, album, album_artist,
  genre, duration_ms, path, artist_mbid)
SELECT 0,
       COALESCE((SELECT MAX(played_at) FROM listen_events), unixepoch()) - seq.i * 3600,
       $per_event + CASE WHEN seq.i = 1 THEN $remainder ELSE 0 END,
       '', '$escaped', '', '$escaped', '', 0, '', NULL
FROM seq;
SQL
    fi
    report+=("$artist|$existing|$target|$topup|$has_history")
  done

  verified=$(sqlite3 "$database" "$cte,
ranked AS (
  SELECT raw, total, ROW_NUMBER() OVER (ORDER BY total DESC) AS rank FROM totals
)
SELECT rank || '|' || raw || '|' || total FROM ranked
WHERE raw IN ($quoted) ORDER BY rank;")
  expected_rank=16
  while IFS='|' read -r rank name _; do
    if [[ -n "$rank" ]]; then
      if (( rank != expected_rank )); then
        echo "seeded artist $name landed at rank $rank, expected $expected_rank" >&2
        return 1
      fi
      expected_rank=$((expected_rank + 1))
    fi
  done <<<"$verified"
  if (( expected_rank != 16 + slots )); then
    echo "only $((expected_rank - 16)) of $slots seeded artists reached the rendered ranking" >&2
    return 1
  fi

  {
    printf 'seeded_ranking=true\n'
    printf 'seeded_artists=%s\n' "${SEEDED_SILHOUETTE_ARTISTS[*]}"
    printf 'reason=only a rendered rank fetches a portrait; these four carry the artist-specific silhouette\n'
    printf 'rank_15_total_ms=%s\n' "$keep_total"
    printf 'rank_16_anchor_total_ms=%s\n' "$anchor"
    printf 'events_per_seeded_artist=%s\n' "$SEED_EVENTS_PER_ARTIST"
    printf '\n%-16s %14s %14s %14s %s\n' artist real_ms target_ms injected_ms prior_events
    for line in "${report[@]}"; do
      IFS='|' read -r name existing target topup has_history <<<"$line"
      printf '%-16s %14s %14s %14s %s\n' \
        "$name" "$existing" "$target" "$topup" "$has_history"
    done
    printf '\nresulting ranks (rank|artist|total_ms):\n%s\n' "$verified"
    printf '\nneighbourhood after seeding (rank artist total_ms):\n'
    sqlite3 "$database" "$cte,
ranked AS (
  SELECT raw, total, ROW_NUMBER() OVER (ORDER BY total DESC) AS rank FROM totals
)
SELECT rank || '  ' || raw || '  ' || total FROM ranked
WHERE rank BETWEEN 12 AND 22 ORDER BY rank;"
  } >"$proof"
}

self_test_seeded_ranking() {
  local fixture_root roomy tight diagnostic
  fixture_root=$(mktemp -d /tmp/reprise-portrait-seed.XXXXXX)
  roomy="$fixture_root/roomy.db"
  tight="$fixture_root/tight.db"
  mkdir -p "$fixture_root/contract"
  output_dir="$fixture_root"

  # Thirty artists 100 s apart, plus real but small histories for three of the
  # four. Aetheriality stays absent, the way it is absent from the real ranking.
  sqlite3 "$roomy" <<'SQL'
CREATE TABLE listen_events(
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL,
  played_at INTEGER NOT NULL,
  ms_played INTEGER NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  artist TEXT NOT NULL DEFAULT '',
  album TEXT NOT NULL DEFAULT '',
  album_artist TEXT NOT NULL DEFAULT '',
  genre TEXT NOT NULL DEFAULT '',
  duration_ms INTEGER NOT NULL DEFAULT 0,
  path TEXT NOT NULL DEFAULT '',
  artist_mbid TEXT
);
WITH RECURSIVE seq(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM seq WHERE i < 31)
INSERT INTO listen_events(track_id, played_at, ms_played, artist, album_artist)
SELECT i, 1750000000 + i, 3000000 - i * 100000, 'Artist ' || i, 'Artist ' || i FROM seq;
INSERT INTO listen_events(track_id, played_at, ms_played, artist, album_artist)
VALUES (900, 1750000500, 120000, 'In Your Grave', 'In Your Grave'),
       (901, 1750000501, 90000, 'Our Vices', 'Our Vices'),
       (902, 1750000502, 60000, 'Wake Me', 'Wake Me');
SQL
  seed_silhouette_ranks "$roomy" contract
  if ! grep -q '^16|Aetheriality|' <<<"$(sqlite3 "$roomy" "$(seed_ranking_cte),
ranked AS (SELECT raw, total, ROW_NUMBER() OVER (ORDER BY total DESC) AS rank FROM totals)
SELECT rank || '|' || raw || '|' || total FROM ranked WHERE rank BETWEEN 16 AND 19 ORDER BY rank;")"; then
    echo "the artist without any history did not reach the rendered ranking" >&2
    return 1
  fi
  if [[ $(sqlite3 "$roomy" \
    "SELECT COUNT(*) FROM listen_events WHERE artist = 'Wake Me';") -ne \
    $((SEED_EVENTS_PER_ARTIST + 1)) ]]; then
    echo "the top-up was not spread over $SEED_EVENTS_PER_ARTIST events" >&2
    return 1
  fi

  # A guard that cannot fail is not a guard: squeeze rank 15 against rank 16 and
  # demand a refusal instead of a silently rewritten ranking.
  sqlite3 "$tight" <<'SQL'
CREATE TABLE listen_events(
  id INTEGER PRIMARY KEY,
  track_id INTEGER NOT NULL,
  played_at INTEGER NOT NULL,
  ms_played INTEGER NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  artist TEXT NOT NULL DEFAULT '',
  album TEXT NOT NULL DEFAULT '',
  album_artist TEXT NOT NULL DEFAULT '',
  genre TEXT NOT NULL DEFAULT '',
  duration_ms INTEGER NOT NULL DEFAULT 0,
  path TEXT NOT NULL DEFAULT '',
  artist_mbid TEXT
);
WITH RECURSIVE seq(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM seq WHERE i < 31)
INSERT INTO listen_events(track_id, played_at, ms_played, artist, album_artist)
SELECT i, 1750000000 + i, 3000000 - i * 500, 'Artist ' || i, 'Artist ' || i FROM seq;
SQL
  if diagnostic=$(seed_silhouette_ranks "$tight" contract 2>&1); then
    echo "seeding accepted a gap too small to hold the four artists" >&2
    return 1
  fi
  if [[ "$diagnostic" != *"too close to hold"* ]]; then
    echo "the refusal did not name the closed gap: $diagnostic" >&2
    return 1
  fi

  find "$fixture_root" -xdev -depth -delete
  echo "seeded_ranking_self_test=passed"
}

if [[ "${1:-}" == "--self-test-seeded-ranking" ]]; then
  self_test_seeded_ranking
  exit 0
fi

if [[ "${1:-}" == "--self-test-private-paths" ]]; then
  self_test_private_paths
  exit 0
fi

if [[ "${1:-}" == "--self-test-private-atspi" ]]; then
  self_test_private_atspi
  exit 0
fi

if [[ "${1:-}" == "--self-test-rendered-portrait-wait" ]]; then
  self_test_rendered_portrait_wait
  exit 0
fi

if [[ "${1:-}" == "--private-run" ]]; then
  shift
  run_private_acceptance "$@"
  exit 0
fi

source_db=""
output_dir=""
confirmed=false
placeholder_references=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-db)
      source_db=${2:-}
      shift 2
      ;;
    --placeholder-reference)
      placeholder_references+=("${2:-}")
      shift 2
      ;;
    --output-dir)
      output_dir=${2:-}
      shift 2
      ;;
    --confirm-read-only-copy)
      confirmed=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$confirmed" != true || -z "$source_db" || ${#placeholder_references[@]} -ne 2 ]]; then
  usage >&2
  exit 2
fi
if [[ ! -f "$source_db" ]]; then
  echo "source database is not a regular file: $source_db" >&2
  exit 2
fi
for reference in "${placeholder_references[@]}"; do
  if [[ ! -f "$reference" ]]; then
    echo "placeholder reference is not a regular file: $reference" >&2
    exit 2
  fi
done

for command in cargo cua-driver dbus-run-session find gdbus git import jq mktemp openbox rg sed \
  rustc sha256sum sqlite3 tar timeout wmctrl xdotool Xvfb; do
  required_command "$command"
done
if [[ ! -x /usr/lib/at-spi-bus-launcher || ! -x /usr/lib/at-spi2-registryd ]]; then
  echo "private AT-SPI launchers are unavailable" >&2
  exit 2
fi

cd "$repo_root"
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "acceptance requires a clean tracked worktree so candidate HEAD is exact" >&2
  exit 2
fi
git rev-parse --verify origin/dev >/dev/null

if [[ -z "$output_dir" ]]; then
  output_dir="$acceptance_root/runs/$(date -u +%Y%m%dT%H%M%SZ)"
fi
output_dir=$(realpath -m "$output_dir")
case "$output_dir" in
  "$repo_root"/*) ;;
  *)
    echo "output directory must stay inside this worktree: $output_dir" >&2
    exit 2
    ;;
esac
if [[ -e "$output_dir" ]]; then
  echo "output directory already exists: $output_dir" >&2
  exit 2
fi
mkdir -p "$output_dir"

cache_key_source="$output_dir/cache-key.rs"
cache_key_binary="$output_dir/cache-key"
cat >"$cache_key_source" <<'RUST'
use std::hash::{Hash, Hasher};

fn main() {
    let name = std::env::args().nth(1).expect("artist name");
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.as_bytes().hash(&mut hasher);
    println!("{:016x}", hasher.finish());
}
RUST
rustc "$cache_key_source" -o "$cache_key_binary"

baseline_source="$output_dir/build/origin-dev"
mkdir -p "$baseline_source"
git archive origin/dev | tar -x -C "$baseline_source"

(cd "$baseline_source" && cargo build --offline --locked -p reprise-gnome) \
  >"$output_dir/build-origin-dev.log" 2>&1
cargo build --offline --locked -p reprise-gnome \
  >"$output_dir/build-candidate.log" 2>&1
baseline_binary="$baseline_source/target/debug/reprise"
candidate_binary="$repo_root/target/debug/reprise"
if [[ ! -x "$baseline_binary" || ! -x "$candidate_binary" ]]; then
  echo "one of the acceptance binaries was not built" >&2
  exit 1
fi

prepare_profile() {
  local profile_root=$1 label=$2
  local database="$profile_root/data/reprise/reprise.db"
  local isolated_music_root="$profile_root/music"
  local copy_error="$output_dir/$label/database-copy-error.txt"
  mkdir -p "$profile_root/data/reprise" "$profile_root/cache" \
    "$profile_root/config" "$profile_root/state" "$isolated_music_root"
  # SQLite online backup reads the source with the read-only flag and copies
  # all committed WAL frames without checkpointing or writing the source.
  if sqlite3 -readonly "$source_db" ".backup '$database'" 2>"$copy_error"; then
    printf 'sqlite_online_backup_read_only\n' >"$output_dir/$label/database-copy-method.txt"
  elif [[ ! -s "${source_db}-wal" ]]; then
    find "$database" -maxdepth 0 -type f -delete
    sqlite3 -readonly "file:$source_db?mode=ro&immutable=1" \
      ".backup '$database'"
    printf 'sqlite_immutable_backup_read_only_no_wal\n' \
      >"$output_dir/$label/database-copy-method.txt"
  else
    echo "read-only SQLite backup failed while a WAL exists; evidence: $copy_error" >&2
    return 1
  fi
  # Seed before the path rewrite below, so the seeded events are repointed at the
  # isolated music root together with every real one.
  seed_silhouette_ranks "$database" "$label"
  sqlite3 "$database" <<SQL
INSERT INTO settings(key, value) VALUES('online-sources-enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
INSERT INTO settings(key, value) VALUES('module.artwork.enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
UPDATE tracks SET path = '$isolated_music_root/track-' || id || '.missing';
UPDATE listen_events SET path = '$isolated_music_root/event-' || id || '.missing';
INSERT INTO settings(key, value) VALUES('library_root', '$isolated_music_root')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
UPDATE settings
SET value = json_set(
  value,
  '\$.maximized', json('false'),
  '\$.window_width', 1560,
  '\$.window_height', 1160,
  '\$.browser_place', json_extract(value, '\$.library_root'),
  '\$.queue', json('{"ids":[],"order":[],"position":null,"repeat":"Off","shuffled":false}'),
  '\$.up_next', json('[]'),
  '\$.current_up_next', NULL,
  '\$.active_episode', NULL,
  '\$.play_origin', NULL,
  '\$.play_origin_label', NULL,
  '\$.play_origin_place', NULL,
  '\$.clean_exit', json_object(
    'completed_at', unixepoch(),
    'library_root', '$isolated_music_root'
  )
)
WHERE key = 'ui.session.v1';
SQL
  sqlite3 -readonly -header -column "$database" \
    "SELECT key, value FROM settings WHERE key IN ('library_root', 'online-sources-enabled', 'module.artwork.enabled') ORDER BY key" \
    >"$output_dir/$label/settings-proof.txt"
  sqlite3 -readonly -header -column "$database" \
    "SELECT json_extract(value, '$.browser_place') AS startup_place, json_array_length(json_extract(value, '$.queue.ids')) AS queued_tracks, json_extract(value, '$.clean_exit.library_root') AS clean_exit_root FROM settings WHERE key = 'ui.session.v1'" \
    >"$output_dir/$label/session-isolation-proof.txt"
}

mkdir -p "$output_dir/before" "$output_dir/after"
before_profile="$output_dir/profiles/before"
after_profile="$output_dir/profiles/after"
prepare_profile "$before_profile" before
prepare_profile "$after_profile" after

{
  printf 'origin_dev=%s\n' "$(git rev-parse origin/dev)"
  printf 'candidate_head=%s\n' "$(git rev-parse HEAD)"
  printf 'source_database=%s\n' "$(realpath "$source_db")"
  printf 'before_database_copy_method=%s\n' "$(<"$output_dir/before/database-copy-method.txt")"
  printf 'after_database_copy_method=%s\n' "$(<"$output_dir/after/database-copy-method.txt")"
  printf 'display_backend=x11-xvfb-openbox\n'
  printf 'created_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'seeded_ranking_artists=%s\n' "${SEEDED_SILHOUETTE_ARTISTS[*]}"
  printf 'seeded_ranking_proof=before/seeded-ranking-proof.txt after/seeded-ranking-proof.txt\n'
  printf 'placeholder_reference_1=%s\n' "$(realpath "${placeholder_references[0]}")"
  printf 'placeholder_reference_2=%s\n' "$(realpath "${placeholder_references[1]}")"
} >"$output_dir/run-manifest.txt"
sha256sum "${placeholder_references[@]}" >"$output_dir/placeholder-reference-sha256.txt"
if [[ $(cut -d' ' -f1 "$output_dir/placeholder-reference-sha256.txt" | sort -u | wc -l) -ne 2 ]]; then
  echo "the two placeholder references must be byte-distinct" >&2
  exit 2
fi

scratch_root="$output_dir/session"
export CUA_DRIVER_BIN="${CUA_DRIVER_BIN:-cua-driver}"
export CUA_E2E_SESSION=deezer-portrait-acceptance
export CUA_E2E_OUT_DIR="$output_dir"
export CUA_E2E_SCRATCH_ROOT="$scratch_root"
export CUA_E2E_SCREEN_RES=1800x1300x24

display_cleanup() {
  local exit_code=$?
  cua_common_stop_display
  if ! cleanup_private_runtime_root; then
    echo "failed to remove private runtime root: $private_runtime_root" >&2
    exit_code=1
  fi
  trap - EXIT
  exit "$exit_code"
}
trap display_cleanup EXIT
allocate_private_runtime_root
mkdir -p "$scratch_root"
cua_common_start_display "$output_dir" "$scratch_root" "$CUA_E2E_SCREEN_RES"

run_isolated() {
  local label=$1 binary=$2 profile_root=$3
  local runtime_dir="$private_runtime_root/runtime-$label"
  local socket_path="$runtime_dir/cua-driver.sock"
  validate_unix_socket_path "$socket_path"
  cua_common_exec_private "$runtime_dir" "$profile_root" env \
    XDG_STATE_HOME="$profile_root/state" \
    CUA_E2E_WM_PID="$CUA_COMMON_OPENBOX_PID" \
    CUA_DRIVER_SOCKET="$socket_path" \
    CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
    "$0" --private-run "$label" "$binary" "$output_dir/$label"
}

run_isolated before "$baseline_binary" "$before_profile"
run_isolated after "$candidate_binary" "$after_profile"
cua_common_stop_display
cleanup_private_runtime_root
trap - EXIT

cache_file_for() {
  local profile_root=$1 artist=$2 key
  local -a matches
  key=$("$cache_key_binary" "$artist")
  mapfile -t matches < <(find "$profile_root/cache/reprise/artist-portraits" \
    -maxdepth 1 -type f -name "$key.*" ! -name '*.notfound' -print)
  if [[ ${#matches[@]} -ne 1 ]]; then
    echo "expected one cached image for $artist, found ${#matches[@]}" >&2
    return 1
  fi
  printf '%s\n' "${matches[0]}"
}

negative_marker_for() {
  local profile_root=$1 artist=$2 key marker
  local -a images
  key=$("$cache_key_binary" "$artist")
  marker="$profile_root/cache/reprise/artist-portraits/$key.notfound"
  if [[ ! -f "$marker" ]]; then
    echo "expected a negative portrait marker for $artist" >&2
    return 1
  fi
  mapfile -t images < <(find "$profile_root/cache/reprise/artist-portraits" \
    -maxdepth 1 -type f -name "$key.*" ! -name '*.notfound' -print)
  if [[ ${#images[@]} -ne 0 ]]; then
    echo "expected no cached image for $artist, found ${#images[@]}" >&2
    return 1
  fi
  printf '%s\n' "$marker"
}

before_prada=$(cache_file_for "$before_profile" "The Devil Wears Prada")
before_oceano=$(cache_file_for "$before_profile" "Oceano")
after_prada=$(cache_file_for "$after_profile" "The Devil Wears Prada")
after_oceano_marker=$(negative_marker_for "$after_profile" "Oceano")
reference_hashes=$(cut -d' ' -f1 "$output_dir/placeholder-reference-sha256.txt")
before_prada_hash=$(sha256sum "$before_prada" | cut -d' ' -f1)
before_oceano_hash=$(sha256sum "$before_oceano" | cut -d' ' -f1)
after_prada_hash=$(sha256sum "$after_prada" | cut -d' ' -f1)

# The Devil Wears Prada is the control arm, no longer the subject. Its silhouette
# hides behind the empty-string MD5, which is one of the two structural
# identifiers that survive on the baseline, so origin/dev already rejects that
# candidate before downloading anything and caches the same photograph the
# candidate does. Demanding the silhouette here — the shape this oracle had when
# the baseline still predated that identifier list — is unsatisfiable by
# construction and says nothing about the fingerprint under test. What it must
# say instead: the fingerprint leaves an artist alone that was already correct.
if [[ "$before_prada_hash" != "$after_prada_hash" ]]; then
  echo "the fingerprint disturbed The Devil Wears Prada, which the baseline already resolved: $before_prada_hash became $after_prada_hash" >&2
  exit 1
fi
for portrait_hash in "$before_oceano_hash" "$after_prada_hash"; do
  if grep -Fxq "$portrait_hash" <<<"$reference_hashes"; then
    echo "a portrait that must be real still matches a placeholder reference" >&2
    exit 1
  fi
done

# The four artists the seeded ranking brought into view. Their silhouette hides
# behind an ordinary, artist-specific identifier, so the baseline downloads and
# caches it as if it were a portrait while the candidate has to refuse it. This
# is precisely the difference a fixed identifier list cannot produce: it is the
# same drawing, served under a name nobody can enumerate in advance.
declare -A seeded_before_hash=()
declare -A seeded_after_marker=()
for seeded_artist in "${SEEDED_SILHOUETTE_ARTISTS[@]}"; do
  seeded_before_file=$(cache_file_for "$before_profile" "$seeded_artist")
  seeded_before_hash["$seeded_artist"]=$(sha256sum "$seeded_before_file" | cut -d' ' -f1)
  seeded_after_marker["$seeded_artist"]=$(negative_marker_for "$after_profile" "$seeded_artist")
done

{
  printf 'before_prada=%s  %s\n' "$before_prada_hash" "$before_prada"
  printf 'before_oceano=%s  %s\n' "$before_oceano_hash" "$before_oceano"
  printf 'after_prada=%s  %s\n' "$after_prada_hash" "$after_prada"
  printf 'after_oceano_negative_marker=%s\n' "$after_oceano_marker"
  printf 'prada_unchanged_by_the_fingerprint=true\n'
  printf 'after_prada_differs_from_known_placeholders=true\n'
  printf 'before_oceano_differs_from_known_placeholders=true\n'
  printf 'after_oceano_has_negative_marker=true\n'
  printf 'after_oceano_has_cached_image=false\n'
  printf '\n# seeded silhouette artists: cached by the baseline, refused by the candidate\n'
  for seeded_artist in "${SEEDED_SILHOUETTE_ARTISTS[@]}"; do
    printf 'before_%s=%s\n' "$seeded_artist" "${seeded_before_hash[$seeded_artist]}"
    printf 'after_%s_negative_marker=%s\n' \
      "$seeded_artist" "${seeded_after_marker[$seeded_artist]}"
  done
  printf 'seeded_artists_cached_before=%s\n' "${#seeded_before_hash[@]}"
  printf 'seeded_artists_refused_after=%s\n' "${#seeded_after_marker[@]}"
} >"$output_dir/named-cache-proof.txt"

cat >"$output_dir/MANUAL-REVIEW.md" <<'EOF'
# Deezer portrait visible acceptance review

- Compare `before/my-stats.png` and `after/my-stats.png` at the same ranks.
- Both screenshots are captured after expanding the ranking; confirm the
  `Hide more top artists` control and Oceano are visible in the retained CUA evidence.
- Ranks move with the listening history; find the artists by name, not by number.
- The run copy is seeded. Aetheriality, In Your Grave, Our Vices and Wake Me were
  lifted into the rendered ranking with synthetic listen events, because only a
  rendered rank fetches a portrait and those four sit at ranks 40, 122, 131 and —
  with zero plays — nowhere at all. Read `before/seeded-ranking-proof.txt`: it
  names every injected millisecond, and both arms received the identical copy.
  Ranks 1-15 are untouched, so the listening history the screenshots show above
  the seeded block is the real one.
- The before arm therefore shows four grey person silhouettes at ranks 16-19 and
  the after arm shows initials in their place. That contrast is the point of the
  whole change: those four silhouettes arrive under ordinary, artist-specific
  image identifiers, which no fixed list can enumerate in advance.
- A silhouette anywhere *else* — at any rank outside 16-19, in either arm — is a
  finding. The baseline already rejects the two structural identifiers.
- Check in the screenshot that the four really sit at ranks 16-19. The seeding
  anchors on a SQL ranking that groups by the effective album artist only, while
  the view folds spelling variants and MBIDs together first, so the two can drift
  apart. They agreed on 2026-08-14. If they ever disagree, the four will still be
  rendered — the SQL check refuses to place them above rank 16 — but the rank
  numbers in these notes stop matching the picture.
- Oceano is the only intended difference: a photograph before, initials after. Its
  most popular exact-name candidate now reaches content validation, is rejected as
  the known silhouette, and must not fall back to the pictured namesake.
- The Devil Wears Prada is the control: the same photograph in both arms. It hides
  behind the empty-string MD5, which the baseline already catches at selection.
- Confirm every other artist shows the same identity in both arms, or record each
  change. Only the artists rendered in the ranking are fetched at all — silhouettes
  further down the library are covered by the corpus measurement in
  `docs/evidence/portrait-placeholder-fingerprint/rust-separation.txt`, not here.
- Read `settings-proof.txt`, `cache-before.txt`, `cache-listing.txt`, and
  `named-cache-proof.txt` alongside the screenshots. The empty cache plus the
  named images created afterward is the positive portrait-request proof.
- Confirm both application processes ended; the script waits for each smoke timer.
EOF

echo "acceptance evidence ready for independent visual review: $output_dir"
