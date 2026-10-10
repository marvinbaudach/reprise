#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
config="$repo_root/.github/dependabot.yml"

group="all-dependencies"

fail() {
    printf 'Dependabot target contract failed: %s\n' "$1" >&2
    exit 1
}

rg --multiline --quiet \
    -- "^multi-ecosystem-groups:\n  ${group}:\n    schedule:\n      interval: weekly\n      day: monday\n    target-branch: dev\n    commit-message:\n      prefix: \"chore\"\n" \
    "$config" || fail "the bundled group must run weekly on monday, target dev and prefix chore"

# Dependabot rejects the whole file when a grouped stream repeats these keys,
# then keeps running the last valid config. That is how #569 went unapplied
# for seven weeks, so the keys live on the group only.
for key in target-branch commit-message; do
    [[ "$(rg --count "^    ${key}:" "$config")" == "1" ]] || \
        fail "${key} must be set once, on the ${group} group, never on a grouped stream"
done

require_stream() {
    local ecosystem="$1"
    local directory="$2"

    rg --multiline --quiet \
        -- "- package-ecosystem: ${ecosystem}\n    directory: ${directory}\n    multi-ecosystem-group: ${group}\n" \
        "$config" || fail "${ecosystem} updates in ${directory} must join the ${group} group"
}

require_stream github-actions /
require_stream cargo /
require_stream gradle /android
require_stream npm /showroom

[[ "$(rg --count '^[[:space:]]+- package-ecosystem:' "$config")" == "4" ]] || \
    fail "every configured update stream must be covered by this contract"

[[ "$(rg --count '^[[:space:]]+multi-ecosystem-group: ' "$config")" == "4" ]] || \
    fail "every update stream must join the bundled group so one pull request carries them all"
