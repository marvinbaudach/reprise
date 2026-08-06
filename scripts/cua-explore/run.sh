#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
# shellcheck source=../cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"
# shellcheck source=../cua-common/session.sh
source "$repo_root/scripts/cua-common/session.sh"

usage() {
  cat <<'EOF'
usage:
  scripts/cua-explore/run.sh --list-missions
  scripts/cua-explore/run.sh --validate-only MISSION.json
  scripts/cua-explore/run.sh --hover-smoke MISSION.json FRESH_OUTPUT_DIR [options]
  scripts/cua-explore/run.sh MISSION.json FRESH_OUTPUT_DIR [options]

Runs opt-in exploratory UX agents in a disposable X11/D-Bus/XDG profile. It
is not ordinary CI: the maintainer triggers it before promoting a tested dev
snapshot to main. OUTPUT_DIR must be a fresh output directory and is retained
as evidence.

Profiles cover up to 100,000 generated catalog rows. The stress profile also
contains 512 independent writable audio fixtures for real batch tag editing.

Options:
  --seed N                    deterministic built-in explorer seed (default 1)
  --profile debug|release     app build profile (default debug)
  --agent-command-json JSON   external JSONL agent argv; no shell is used
  --agent-timeout SECONDS     response timeout for an external agent
  --gtk-animations on|off     private GTK animation setting (default on)
  --window-origin X,Y         override desktop window origin for hover smoke
EOF
}

required_command() {
  if ! command -v "$1" >/dev/null; then
    echo "required command is unavailable: $1" >&2
    exit 2
  fi
}

mission_dir="$repo_root/scripts/cua-explore/missions"
hover_smoke=false
if [[ ${1:-} == --hover-smoke ]]; then
  hover_smoke=true
  shift
fi
case "${1:-}" in
  --help|-h)
    usage
    exit 0
    ;;
  --list-missions)
    find "$mission_dir" -maxdepth 1 -type f -name '*.json' -printf '%f\n' \
      | sed 's/\.json$//' | sort
    exit 0
    ;;
  --validate-only)
    if [[ $# != 2 ]]; then
      usage >&2
      exit 2
    fi
    python3 "$repo_root/scripts/cua-explore/protocol.py" validate-mission "$2" \
      >/dev/null
    profile=$(python3 - "$2" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["profile"])
PY
)
    python3 "$repo_root/scripts/cua-explore/fixtures.py" plan "$profile" >/dev/null
    echo "exploratory mission is valid: $(basename "$2" .json)"
    exit 0
    ;;
  --private-session)
    shift
    private_output=$1
    private_scratch=$2
    private_session=$3
    shift 3
    export CUA_DRIVER_SOCKET="$private_scratch/explore-cua-driver.sock"
    cleanup_private() {
      local exit_code=$?
      cua_common_stop_driver "$private_session"
      exit "$exit_code"
    }
    trap cleanup_private EXIT
    cua_common_start_driver "$private_output" "$CUA_DRIVER_SOCKET" "$private_session"
    python3 "$repo_root/scripts/cua-explore/runner.py" \
      --socket "$CUA_DRIVER_SOCKET" --session "$private_session" "$@"
    exit 0
    ;;
esac

if (( $# < 2 )); then
  usage >&2
  exit 2
fi
mission=$1
output_dir=$2
shift 2
seed=1
build_profile=debug
agent_command_json=""
agent_timeout=30
gtk_animations=on
window_origin=""
while (( $# )); do
  case "$1" in
    --seed) seed=$2; shift 2 ;;
    --profile) build_profile=$2; shift 2 ;;
    --agent-command-json) agent_command_json=$2; shift 2 ;;
    --agent-timeout) agent_timeout=$2; shift 2 ;;
    --gtk-animations) gtk_animations=$2; shift 2 ;;
    --window-origin) window_origin=$2; shift 2 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done
if [[ $build_profile != debug && $build_profile != release ]]; then
  echo "--profile must be debug or release" >&2
  exit 2
fi
if [[ $gtk_animations != on && $gtk_animations != off ]]; then
  echo "--gtk-animations must be on or off" >&2
  exit 2
fi
if [[ -z $agent_command_json ]] && python3 - "$mission" <<'PY'
import json, pathlib, sys
mission = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
raise SystemExit(0 if mission["agent"] == "required" else 1)
PY
then
  echo "this mission requires --agent-command-json and verified workload checkpoints" >&2
  exit 2
fi
if [[ -e $output_dir ]]; then
  echo "fresh output directory required; path already exists: $output_dir" >&2
  exit 2
fi
python3 "$repo_root/scripts/cua-explore/protocol.py" validate-mission "$mission" \
  >/dev/null
if [[ -n $(git -C "$repo_root" status --porcelain) ]]; then
  echo "exploratory pre-main runs require a clean Git worktree" >&2
  exit 2
fi
for command in cargo cua-driver Xvfb openbox dbus-run-session jq python3 rg timeout unshare wmctrl; do
  required_command "$command"
done
for executable in /usr/lib/at-spi-bus-launcher /usr/lib/at-spi2-registryd; do
  if [[ ! -x $executable ]]; then
    echo "required accessibility executable is unavailable: $executable" >&2
    exit 2
  fi
done

profile=$(python3 - "$mission" <<'PY'
import json, pathlib, sys
print(json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))["profile"])
PY
)
profile_flag=()
profile_dir=debug
if [[ $build_profile == release ]]; then
  profile_flag=(--release)
  profile_dir=release
fi
cargo build --locked -p reprise-gnome --features test-fixtures "${profile_flag[@]}"
cargo build --locked -p reprise-core --example scalability_baseline "${profile_flag[@]}"
app_binary="$repo_root/target/$profile_dir/reprise"
seed_binary="$repo_root/target/$profile_dir/examples/scalability_baseline"

scratch_base=${REPRISE_CUA_SCRATCH_BASE:-$HOME/.cache/reprise-scratch}
python3 "$repo_root/scripts/cua-explore/fixtures.py" validate-base \
  "$scratch_base" >/dev/null
mkdir -p "$scratch_base"
scratch_root=$(mktemp -d "$scratch_base/reprise-cua-explore-run.XXXXXX")
profile_root="$scratch_root/reprise-cua-explore-profile"
cleanup_outer() {
  local exit_code=$?
  cua_common_stop_display
  rm -r -- "$scratch_root"
  exit "$exit_code"
}
trap cleanup_outer EXIT
python3 "$repo_root/scripts/cua-explore/fixtures.py" prepare \
  "$profile" "$profile_root" --seed-binary "$seed_binary" >/dev/null
mkdir -p "$(dirname "$output_dir")"
mkdir "$output_dir"
output_dir=$(cd "$output_dir" && pwd)
commit=$(git -C "$repo_root" rev-parse HEAD)
session_id="reprise-explore-$$"
{
  printf 'schema_version=1\n'
  printf 'mission=%s\n' "$(basename "$mission" .json)"
  printf 'profile=%s\n' "$profile"
  printf 'seed=%s\n' "$seed"
  printf 'commit=%s\n' "$commit"
  printf 'display_backend=x11-xvfb\n'
  printf 'generated_data_only=true\n'
  printf 'ordinary_ci=false\n'
  printf 'app_network_namespace=true\n'
  printf 'gtk_animations=%s\n' "$gtk_animations"
} >"$output_dir/run-manifest.txt"

cua_common_start_display "$output_dir" "$scratch_root" "1600x900x24"
private_runtime="$scratch_root/runtime"
private_root="$scratch_root/root-profile"
private_args=(
  --mission "$mission"
  --profile-root "$profile_root"
  --evidence-dir "$output_dir"
  --app-binary "$app_binary"
  --seed "$seed"
  --commit "$commit"
  --agent-timeout "$agent_timeout"
  --gtk-animations "$gtk_animations"
)
if [[ $hover_smoke == true ]]; then
  private_args+=(--hover-smoke)
fi
if [[ -n $window_origin ]]; then
  private_args+=(--window-origin "$window_origin")
fi
if [[ -n $agent_command_json ]]; then
  private_args+=(--agent-command-json "$agent_command_json")
fi
cua_common_exec_private "$private_runtime" "$private_root" env \
  -u GNOME_KEYRING_CONTROL -u GNOME_KEYRING_PID \
  CUA_DRIVER_BIN="$CUA_DRIVER_BIN" \
  "$0" --private-session "$output_dir" "$scratch_root" "$session_id" \
  "${private_args[@]}"

echo "Exploratory UX evidence written to $output_dir"
