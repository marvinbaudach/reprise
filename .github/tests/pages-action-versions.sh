#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/pages.yml"

fail() {
    printf 'Pages action version contract failed: %s\n' "$1" >&2
    exit 1
}

require_release() {
    local action="$1"
    local minimum_major="$2"
    local tag
    local major

    tag="$(sed -n "s|^[[:space:]]*-[[:space:]]*uses: ${action}@\(v[^[:space:]]*\)$|\1|p; s|^[[:space:]]*uses: ${action}@\(v[^[:space:]]*\)$|\1|p" "$workflow")"
    [[ -n "$tag" ]] || fail "missing ${action}"
    [[ "$tag" =~ ^v([0-9]+)\.[0-9]+\.[0-9]+$ ]] || \
        fail "${action} must use a complete release tag, found ${tag}"

    major="${BASH_REMATCH[1]}"
    (( major >= minimum_major )) || \
        fail "${action} must use Node 24 generation v${minimum_major} or newer, found ${tag}"
}

require_release actions/checkout 7
require_release actions/setup-node 7
require_release actions/upload-pages-artifact 5
require_release actions/deploy-pages 5

rg --quiet "^[[:space:]]+node-version: '26\.7\.0'$" "$workflow" || \
    fail "the showroom build must pin the current Node release 26.7.0"
