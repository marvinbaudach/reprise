#!/usr/bin/env bash
set -euo pipefail

# A gate that judges the whole repository must run on every change, not only
# when one frontend's paths move. `scripts/check-architecture.sh` caps every
# Rust file in the workspace and resolves every documentation path cited from
# `crates/` and `scripts/` — so a change touching only `crates/reprise-core`
# can break it.
#
# It used to run from `.github/scripts/check-gnome-ci.sh`, and the gnome-suite
# job is gated on `needs.changes.outputs.gnome == 'true'`. On 2026-08-18 that
# combination hid a real failure: #570 pushed a core test file past the
# 800-line cap, the GNOME suite reported it red, and every dev commit after it
# skipped the job — so `dev` read green for five commits while the gate stayed
# broken. This contract keeps the repo-wide gates in the unrouted job.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/ci.yml"
gnome_script="$repo_root/.github/scripts/check-gnome-ci.sh"

fail() {
    printf 'Repo-wide gate routing contract failed: %s\n' "$1" >&2
    exit 1
}

readonly REPO_WIDE_GATES=(
    scripts/check-architecture.sh
)

# The unrouted job. `base-contracts` runs whenever the change is not
# documentation-only; it carries no `outputs.gnome`/`outputs.core` condition.
base_contracts=$(awk '
    /^  base-contracts:/ { inside = 1; next }
    inside && /^  [a-z]/ { exit }
    inside { print }
' "$workflow")

[[ -n $base_contracts ]] || fail "base-contracts job not found in $workflow"

grep -Eq "^\s+if: needs\.changes\.outputs\.suite_skip != 'true'\s*$" <<<"$base_contracts" || \
    fail "base-contracts must stay routed only by suite_skip, never by a frontend path"

grep -q 'outputs\.gnome\|outputs\.core\|outputs\.android' <<<"$base_contracts" && \
    fail "base-contracts must not gain a per-frontend route"

for gate in "${REPO_WIDE_GATES[@]}"; do
    grep -Fq "$gate" <<<"$base_contracts" || \
        fail "$gate judges the whole repository and must run from base-contracts"
    if [[ -f $gnome_script ]] && grep -Eq "^[[:space:]]*$gate" "$gnome_script"; then
        fail "$gate must not also run from the path-routed GNOME suite"
    fi
done

printf 'Repo-wide gates run from the unrouted job.\n'
