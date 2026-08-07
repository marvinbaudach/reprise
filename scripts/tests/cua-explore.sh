#!/usr/bin/env bash
set -euo pipefail
export PYTHONDONTWRITEBYTECODE=1

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

python3 scripts/tests/cua-explore.py
python3 scripts/tests/cua-explore-review.py
python3 scripts/tests/cua-explore-audit-adversarial.py
python3 scripts/tests/cua-explore-hover.py
python3 scripts/tests/cua-explore-agent.py
python3 scripts/tests/cua-explore-readiness.py
python3 scripts/tests/cua-explore-hover-probe.py
python3 scripts/tests/cua-explore-geometry.py
python3 scripts/cua-explore/protocol.py validate-mission \
  scripts/cua-explore/missions/first-time-exploration.json >/dev/null

runner=scripts/cua-explore/run.sh
if [[ ! -x $runner ]]; then
  echo "$runner must exist and be executable" >&2
  exit 1
fi
agent=scripts/cua-explore/agents/reprise_ux_agent.py
if [[ ! -x $agent ]]; then
  echo "$agent must exist and be executable" >&2
  exit 1
fi
if ! rg --quiet --fixed-strings -- '--agent-command-json' scripts/cua-explore/README.md || \
   ! rg --quiet --fixed-strings 'agents/reprise_ux_agent.py' scripts/cua-explore/README.md; then
  echo "exploratory README must document the bundled reasoning agent command" >&2
  exit 1
fi
help=$($runner --help)
for phrase in "opt-in" "not ordinary CI" "fresh output" "100,000" "512" "--hover-smoke" "--gtk-animations"; do
  if [[ $help != *"$phrase"* ]]; then
    echo "exploratory runner help must mention: $phrase" >&2
    exit 1
  fi
done
missions=$($runner --list-missions)
for mission in first-time-exploration hover-affordance-sweep large-library-stress offline-recovery section-search-isolation pointer-layout-reachability; do
  if ! rg --quiet --fixed-strings --line-regexp "$mission" <<<"$missions"; then
    echo "exploratory runner must list mission: $mission" >&2
    exit 1
  fi
done
$runner --validate-only \
  scripts/cua-explore/missions/large-library-stress.json >/dev/null
if $runner scripts/cua-explore/missions/large-library-stress.json \
  /tmp/reprise-cua-explore-must-not-exist; then
  echo "large-library stress must require an external reasoning agent" >&2
  exit 1
fi
if [[ -e /tmp/reprise-cua-explore-must-not-exist ]]; then
  echo "agent preflight must fail before creating evidence" >&2
  exit 1
fi

for required in \
  'source "$repo_root/scripts/cua-common/session.sh"' \
  'cua_common_start_display' \
  'cua_common_exec_private' \
  'REPRISE_AUDIO_SINK=fakesink' \
  'GDK_BACKEND=x11' \
  'WAYLAND_DISPLAY=' \
  'XDG_DATA_HOME=' \
  'XDG_CACHE_HOME='; do
  if ! rg --quiet --fixed-strings "$required" "$runner" scripts/cua-common/session.sh; then
    echo "exploratory runner is missing isolation contract: $required" >&2
    exit 1
  fi
done
for required in \
  'REPRISE_CUA_SCRATCH_BASE:-$HOME/.cache/reprise-scratch' \
  'complete-workload' \
  'verified workload checkpoints'; do
  if ! rg --quiet --fixed-strings "$required" "$runner" scripts/cua-explore; then
    echo "exploratory runner is missing reviewed workload contract: $required" >&2
    exit 1
  fi
done
if rg --quiet --fixed-strings '/tmp/reprise-cua-explore-run' "$runner"; then
  echo "large generated profiles must not use the RAM-backed /tmp scratch" >&2
  exit 1
fi
validate_base_line=$(rg -n --fixed-strings 'fixtures.py" validate-base' "$runner" | cut -d: -f1)
create_base_line=$(rg -n --fixed-strings 'mkdir -p "$scratch_base"' "$runner" | cut -d: -f1)
if [[ -z $validate_base_line || -z $create_base_line || $validate_base_line -ge $create_base_line ]]; then
  echo "scratch base must be validated before the runner creates it" >&2
  exit 1
fi

for required in 'unshare' '--map-current-user' '--net'; do
  if ! rg --quiet --fixed-strings -- "$required" "$runner" scripts/cua-explore/runner.py; then
    echo "exploratory app must use a private network namespace: $required" >&2
    exit 1
  fi
done
# Mapping the app to root breaks D-Bus EXTERNAL authentication against the
# user-owned private session bus.
if rg --quiet --fixed-strings -- '--map-root-user' "$runner" scripts/cua-explore/runner.py; then
  echo "exploratory app must not map root inside its network namespace" >&2
  exit 1
fi
host_network=$(readlink /proc/self/ns/net)
private_network=$(unshare --user --map-current-user --net readlink /proc/self/ns/net)
if [[ $host_network == "$private_network" ]]; then
  echo "exploratory app network namespace must differ from the host" >&2
  exit 1
fi
if command -v dbus-run-session >/dev/null && command -v dbus-send >/dev/null; then
  if ! timeout 20 dbus-run-session -- unshare --user --map-current-user --net \
    dbus-send --session --print-reply --dest=org.freedesktop.DBus \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames >/dev/null; then
    echo "exploratory app namespace must retain private session-bus access" >&2
    exit 1
  fi
else
  echo "skipping exploratory namespace D-Bus probe: dbus-run-session or dbus-send missing"
fi

for required in \
  'org.freedesktop.secrets' \
  'org.freedesktop.impl.portal.Secret' \
  'XDG_DATA_DIRS="$stub_root:'; do
  if ! rg --quiet --fixed-strings "$required" scripts/cua-common/session.sh; then
    echo "private session must neutralise the secret service: $required" >&2
    exit 1
  fi
done
# The stub must make activation fail immediately instead of waiting out the
# bus timeout. Measured on a developer host: 25s without it, 18ms with it.
stub_root=$(mktemp -d)
mkdir -p "$stub_root/dbus-1/services"
printf '[D-BUS Service]\nName=org.freedesktop.secrets\nExec=/bin/false\n' \
  >"$stub_root/dbus-1/services/org.freedesktop.secrets.service"
secrets_started=$(date +%s%N)
XDG_DATA_DIRS="$stub_root:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
  timeout 30 dbus-run-session -- dbus-send --session --print-reply \
  --dest=org.freedesktop.secrets /org/freedesktop/secrets \
  org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1 || true
secrets_elapsed=$(( ($(date +%s%N) - secrets_started) / 1000000 ))
rm -rf "$stub_root"
if (( secrets_elapsed > 5000 )); then
  echo "secret-service stub must fail fast, took ${secrets_elapsed}ms" >&2
  exit 1
fi
echo "secret-service activation fails in ${secrets_elapsed}ms with the stub"

if rg --quiet --fixed-strings 'cua-explore' .github/workflows; then
  echo "exploratory UX runs must stay out of ordinary CI workflows" >&2
  exit 1
fi

if python3 scripts/cua-explore/protocol.py validate-action \
  scripts/cua-explore/missions/first-time-exploration.json <<'EOF'
{"schema_version":1,"state_id":"stale","kind":"shell","command":"rm -rf /"}
EOF
then
  echo "exploratory action gateway must reject arbitrary shell commands" >&2
  exit 1
fi

echo "CUA exploratory QA contract passed"
