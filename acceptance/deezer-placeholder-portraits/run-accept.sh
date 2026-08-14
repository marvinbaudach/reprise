#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
acceptance_root="$repo_root/acceptance/deezer-placeholder-portraits"

# Reuse the repository's private X11/D-Bus/AT-SPI lifecycle and CUA helpers.
# shellcheck source=../../scripts/cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"
# shellcheck source=../../scripts/cua-common/session.sh
source "$repo_root/scripts/cua-common/session.sh"

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
  exit "$exit_code"
}

wait_for_portrait_image() {
  local label=$1 portrait_dir=$2
  local deadline=$((SECONDS + 60))

  while ((SECONDS < deadline)); do
    if [[ -d "$portrait_dir" ]] \
      && find "$portrait_dir" -maxdepth 1 -type f ! -name '*.notfound' -print -quit \
        | grep -q .; then
      return 0
    fi
    sleep 1
  done

  echo "$label reached the 60-second portrait wait cap; running the hard cache checks" >&2
}

run_private_acceptance() {
  local label=$1 binary=$2 output_dir=$3
  local app_log="$output_dir/app.log"
  local portrait_dir="$XDG_CACHE_HOME/reprise/artist-portraits"
  local initial_snapshot window_id final_snapshot

  trap private_run_cleanup EXIT
  export CUA_E2E_OUT_DIR="$output_dir/cua"
  export CUA_DRIVER_SOCKET="$output_dir/private-cua-driver.sock"
  export CUA_E2E_SESSION="deezer-portrait-$label"
  mkdir -p "$CUA_E2E_OUT_DIR"

  if [[ -e "$portrait_dir" ]]; then
    echo "$label portrait cache must be absent before launch: $portrait_dir" >&2
    return 1
  fi
  printf 'portrait_cache_absent_before_launch=true\n' >"$output_dir/cache-before.txt"

  cua_common_start_driver "$output_dir" "$CUA_DRIVER_SOCKET" "$CUA_E2E_SESSION"

  env \
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
  initial_snapshot=$(cua_snapshot "$ACCEPT_APP_PID" "$window_id" "$label-initial")
  assert_snapshot_contains "$initial_snapshot" "My Stats"
  cua_click_label "$ACCEPT_APP_PID" "$window_id" "My Stats" "$label-open-stats"

  final_snapshot=$(cua_wait_for_label \
    "$ACCEPT_APP_PID" "$window_id" "The Devil Wears Prada" "$label-stats-ready")
  cua_click_label \
    "$ACCEPT_APP_PID" "$window_id" "Show more top artists" "$label-expand-top-artists"
  final_snapshot=$(cua_wait_for_label \
    "$ACCEPT_APP_PID" "$window_id" "Hide more top artists" "$label-stats-expanded")
  assert_snapshot_contains "$final_snapshot" "Oceano"
  # Page rendering does not prove that a bounded worker completed both network
  # calls. Wait for positive cache evidence before applying the hard checks.
  wait_for_portrait_image "$label" "$portrait_dir"
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
  trap - EXIT
}

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

for command in cargo cua-driver dbus-run-session find git import jq openbox rg \
  rustc sha256sum sqlite3 tar timeout wmctrl Xvfb; do
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
  mkdir -p "$profile_root/data/reprise" "$profile_root/cache" \
    "$profile_root/config" "$profile_root/state"
  # SQLite online backup reads the source with the read-only flag and copies
  # all committed WAL frames without checkpointing or writing the source.
  sqlite3 -readonly "$source_db" ".backup '$database'"
  sqlite3 "$database" <<'SQL'
INSERT INTO settings(key, value) VALUES('online-sources-enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
INSERT INTO settings(key, value) VALUES('module.artwork.enabled', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
SQL
  sqlite3 -readonly -header -column "$database" \
    "SELECT key, value FROM settings WHERE key IN ('online-sources-enabled', 'module.artwork.enabled') ORDER BY key" \
    >"$output_dir/$label/settings-proof.txt"
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
  printf 'database_copy_method=sqlite_online_backup_read_only\n'
  printf 'display_backend=x11-xvfb-openbox\n'
  printf 'created_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'placeholder_reference_1=%s\n' "$(realpath "${placeholder_references[0]}")"
  printf 'placeholder_reference_2=%s\n' "$(realpath "${placeholder_references[1]}")"
} >"$output_dir/run-manifest.txt"
sha256sum "${placeholder_references[@]}" >"$output_dir/placeholder-reference-sha256.txt"
if [[ $(cut -d' ' -f1 "$output_dir/placeholder-reference-sha256.txt" | sort -u | wc -l) -ne 2 ]]; then
  echo "the two placeholder references must be byte-distinct" >&2
  exit 2
fi

scratch_root="$output_dir/session"
mkdir -p "$scratch_root"
export CUA_DRIVER_BIN="${CUA_DRIVER_BIN:-cua-driver}"
export CUA_E2E_SESSION=deezer-portrait-acceptance
export CUA_E2E_OUT_DIR="$output_dir"
export CUA_E2E_SCRATCH_ROOT="$scratch_root"
export CUA_E2E_SCREEN_RES=1800x1300x24

display_cleanup() {
  local exit_code=$?
  cua_common_stop_display
  exit "$exit_code"
}
trap display_cleanup EXIT
cua_common_start_display "$output_dir" "$scratch_root" "$CUA_E2E_SCREEN_RES"

run_isolated() {
  local label=$1 binary=$2 profile_root=$3
  local runtime_dir="$scratch_root/runtime-$label"
  cua_common_exec_private "$runtime_dir" "$profile_root" env \
    XDG_STATE_HOME="$profile_root/state" \
    CUA_E2E_WM_PID="$CUA_COMMON_OPENBOX_PID" \
    CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
    "$0" --private-run "$label" "$binary" "$output_dir/$label"
}

run_isolated before "$baseline_binary" "$before_profile"
run_isolated after "$candidate_binary" "$after_profile"
cua_common_stop_display
trap - EXIT

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

before_prada=$(cache_file_for "$before_profile" "The Devil Wears Prada")
before_oceano=$(cache_file_for "$before_profile" "Oceano")
after_prada=$(cache_file_for "$after_profile" "The Devil Wears Prada")
after_oceano=$(cache_file_for "$after_profile" "Oceano")
reference_hashes=$(cut -d' ' -f1 "$output_dir/placeholder-reference-sha256.txt")
before_prada_hash=$(sha256sum "$before_prada" | cut -d' ' -f1)
before_oceano_hash=$(sha256sum "$before_oceano" | cut -d' ' -f1)
after_prada_hash=$(sha256sum "$after_prada" | cut -d' ' -f1)
after_oceano_hash=$(sha256sum "$after_oceano" | cut -d' ' -f1)

if ! grep -Fxq "$before_prada_hash" <<<"$reference_hashes"; then
  echo "origin/dev did not reproduce the known placeholder for The Devil Wears Prada" >&2
  exit 1
fi
for portrait_hash in "$before_oceano_hash" "$after_prada_hash" "$after_oceano_hash"; do
  if grep -Fxq "$portrait_hash" <<<"$reference_hashes"; then
    echo "a portrait that must be real still matches a placeholder reference" >&2
    exit 1
  fi
done

{
  printf 'before_prada=%s  %s\n' "$before_prada_hash" "$before_prada"
  printf 'before_oceano=%s  %s\n' "$before_oceano_hash" "$before_oceano"
  printf 'after_prada=%s  %s\n' "$after_prada_hash" "$after_prada"
  printf 'after_oceano=%s  %s\n' "$after_oceano_hash" "$after_oceano"
  printf 'before_prada_matches_known_placeholder=true\n'
  printf 'after_prada_differs_from_known_placeholders=true\n'
  printf 'after_oceano_differs_from_known_placeholders=true\n'
} >"$output_dir/named-cache-proof.txt"

cat >"$output_dir/MANUAL-REVIEW.md" <<'EOF'
# Deezer portrait visible acceptance review

- Compare `before/my-stats.png` and `after/my-stats.png` at the same ranks.
- Both screenshots are captured after expanding the ranking; confirm the
  `Hide more top artists` control and Oceano are visible in the retained CUA evidence.
- Before: rank 3, The Devil Wears Prada, must show the known grey silhouette.
- After: ranks 1 through 10 show no grey person silhouette.
- After: rank 3 and rank 10 show photographs, not initials or album covers.
- Treat only rank 3 as visual evidence for E1/E2; Oceano is cache-recovery evidence.
- Confirm the other eight ranks show the same identities before and after, or record every change.
- Read `settings-proof.txt`, `cache-before.txt`, `cache-listing.txt`, and
  `named-cache-proof.txt` alongside the screenshots. The empty cache plus the
  named images created afterward is the positive portrait-request proof.
- Confirm both application processes ended; the script waits for each smoke timer.
EOF

echo "acceptance evidence ready for independent visual review: $output_dir"
