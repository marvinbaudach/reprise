#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/dependabot-automerge.yml"

fail() {
    printf 'Dependabot auto-merge contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing .github/workflows/dependabot-automerge.yml"

rg --multiline --quiet \
    '^on:\n  pull_request:\n    branches:\n      - dev\n    types:\n      - opened\n      - reopened\n      - synchronize\n' \
    "$workflow" || fail "only dev pull requests may trigger auto-merge"
rg --multiline --quiet \
    '^permissions:\n  contents: write\n  pull-requests: write\n' \
    "$workflow" || fail "auto-merge needs only contents and pull-request write access"
rg --quiet \
    "github\.event\.pull_request\.user\.login == 'dependabot\[bot\]'" \
    "$workflow" || fail "the job must accept only Dependabot pull requests"
rg --quiet "github\.event\.pull_request\.base\.ref == 'dev'" "$workflow" || \
    fail "the job must reject pull requests targeting any branch except dev"
rg --quiet "github\.repository == 'marvinbaudach/reprise'" "$workflow" || \
    fail "the job must be bound to this repository"
rg --quiet 'gh pr merge --auto --squash "\$PR_URL"' "$workflow" || \
    fail "Dependabot pull requests must use the repository squash policy"
rg --fixed-strings --quiet \
    'GH_TOKEN: ${{ secrets.REPRISE_AUTOMERGE_TOKEN }}' \
    "$workflow" || fail "auto-merge must act through the owner-scoped Dependabot secret"

if rg --quiet 'actions/checkout|pull_request_target|secrets\.GITHUB_TOKEN' "$workflow"; then
    fail "the privileged workflow must not load pull request code or use the blocked Actions token"
fi
