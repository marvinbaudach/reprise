#!/usr/bin/env bash
# Fail on any `cargo audit` finding that is not on the accepted list.
#
# `cargo audit` exits 0 for warnings (unmaintained, unsound, yanked), so a bare
# call never stopped a new advisory of those kinds. This runs it with
# `--deny warnings` and one `--ignore` per id of the single accepted list,
# .cargo/audit.toml, which AGENTS.md and RELEASING.md must repeat exactly
# (scripts/check-audit-list.sh). The scan runs from a scratch directory, so only
# the ids passed here are ignored, whatever a developer's own audit.toml says.
#
# When the live advisory refresh fails, the scan repeats against the cached
# database with the same list and the same flags, so the fallback can never pass
# what the online run would reject; it only sees advisories published before the
# cache was last refreshed, which is printed. With no cached database the gate
# fails. A list entry that matches no advisory is a failure after a live refresh
# and a notice after a cached one, since an old cache can lack a recent id.
#
# Usage: check-dependency-audit.sh [Cargo.lock [accepted-list.toml]]
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

if [[ $# -gt 2 ]]; then
  printf 'Usage: %s [Cargo.lock [accepted-list.toml]]\n' "$0" >&2
  exit 2
fi
lock_file=$(realpath "${1:-$repo_root/Cargo.lock}")
list_file=$(realpath "${2:-$repo_root/.cargo/audit.toml}")
db_dir=${CARGO_HOME:-$HOME/.cargo}/advisory-db

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
cd "$scratch"

mapfile -t accepted < <(python3 -I - "$list_file" <<'PY'
import sys
import tomllib

try:
    with open(sys.argv[1], "rb") as stream:
        ids = tomllib.load(stream)["advisories"]["ignore"]
except (OSError, KeyError, tomllib.TOMLDecodeError) as error:
    sys.exit(f"check-dependency-audit.sh: cannot read the accepted list {sys.argv[1]}: {error}")
if not ids or not all(isinstance(i, str) for i in ids):
    sys.exit(f"check-dependency-audit.sh: {sys.argv[1]} has no accepted advisory ids")
print("\n".join(ids))
PY
)
[[ ${#accepted[@]} -gt 0 ]] || exit 1

scan_flags=(--deny warnings --db "$db_dir" --file "$lock_file")
for id in "${accepted[@]}"; do
  scan_flags+=(--ignore "$id")
done

source_of_result=live
if ! cargo audit "${scan_flags[@]}"; then
  echo "live advisory refresh or scan failed; rechecking the cached database" >&2
  if [[ ! -d $db_dir ]]; then
    echo "no cached advisory database at $db_dir, and the live refresh failed; the audit cannot run" >&2
    exit 1
  fi
  cargo audit --no-fetch "${scan_flags[@]}"
  source_of_result=cached
  echo "NOTE: this result comes from the cached advisory database, not a live refresh;" \
    "an advisory published since the cache was last updated is not covered" >&2
fi

# Which accepted ids still match something? The cache is fresh after the scan
# above (or already the best there is), so this one skips the network.
cargo audit --json --no-fetch --db "$db_dir" --file "$lock_file" >found.json 2>found.err || true
stale=$(python3 -I - found.json "${accepted[@]}" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], encoding="utf-8") as stream:
        report = json.load(stream)
except (OSError, ValueError) as error:
    sys.exit(f"check-dependency-audit.sh: no readable audit report to look for stale entries: {error}")

found = {item["advisory"]["id"] for item in report["vulnerabilities"]["list"]}
for items in report["warnings"].values():
    found |= {item["advisory"]["id"] for item in items if item.get("advisory")}
print("\n".join(sorted(set(sys.argv[2:]) - found)))
PY
)
if [[ -n $stale ]]; then
  printf 'accepted advisory no longer matches anything in Cargo.lock: %s\n' "$stale" >&2
  if [[ $source_of_result == live ]]; then
    echo "remove it from .cargo/audit.toml, AGENTS.md and RELEASING.md" >&2
    exit 1
  fi
  echo "NOTE: not a failure, because the cached database may predate it" >&2
fi

echo "Dependency audit passed ($source_of_result database, ${#accepted[@]} accepted advisories)"
