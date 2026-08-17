#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/dev-promotion-source.yml"

fail() {
    printf 'dev promotion source contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing .github/workflows/dev-promotion-source.yml"

rg --multiline --quiet \
    '^on:\n  push:\n    branches:\n      - dev\n\npermissions:' \
    "$workflow" || fail "the workflow must run exclusively for pushes to dev"
rg --multiline --quiet \
    '^permissions:\n  contents: read\n' \
    "$workflow" || fail "the workflow must grant only read access to repository contents"
rg --multiline --quiet \
    '^  from-dev:\n    name: From dev\n' \
    "$workflow" || fail "the required check must be named From dev"
rg --quiet '^[[:space:]]+uses: actions/checkout@v7$' "$workflow" || \
    fail "the workflow must check out the dev revision"
rg --quiet '^[[:space:]]+run: \.github/tests/dev-promotion-source\.sh$' "$workflow" || \
    fail "the workflow must execute this contract test"

if [[ "${GITHUB_ACTIONS:-false}" == "true" ]]; then
    [[ "${GITHUB_EVENT_NAME:-}" == "push" ]] || fail "the runtime event is not push"
    [[ "${GITHUB_REF:-}" == "refs/heads/dev" ]] || fail "the runtime ref is not dev"
fi
