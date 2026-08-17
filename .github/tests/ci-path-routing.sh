#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
classifier="$repo_root/.github/scripts/ci-paths.sh"
aggregator="$repo_root/.github/scripts/require-ci-results.sh"
gnome_gate="$repo_root/.github/scripts/check-gnome-ci.sh"
workflow="$repo_root/.github/workflows/ci.yml"
cross_target="$repo_root/.github/workflows/cross-target.yml"
showroom="$repo_root/.github/workflows/pages.yml"

fail() {
    printf 'CI path-routing contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -x "$classifier" ]] || fail "missing executable .github/scripts/ci-paths.sh"
[[ -x "$aggregator" ]] || fail "missing executable .github/scripts/require-ci-results.sh"
[[ -x "$gnome_gate" ]] || fail "missing executable .github/scripts/check-gnome-ci.sh"

expect_routes() {
    local expected_android=$1
    local expected_gnome=$2
    local expected_core=$3
    shift 3
    local output
    output=$("$classifier" --paths "$@")
    rg --quiet "^android=$expected_android$" <<<"$output" || \
        fail "expected android=$expected_android for $*; got: $output"
    rg --quiet "^gnome=$expected_gnome$" <<<"$output" || \
        fail "expected gnome=$expected_gnome for $*; got: $output"
    rg --quiet "^core=$expected_core$" <<<"$output" || \
        fail "expected core=$expected_core for $*; got: $output"
}

expect_routes true false false android/app/src/main/MainActivity.kt
expect_routes true false false crates/reprise-android-ffi/src/lib.rs
expect_routes false true false crates/reprise-gnome/src/main.rs
expect_routes false true false crates/reprise-platform-linux/src/lib.rs
expect_routes true false true crates/reprise-core/src/lib.rs
expect_routes true true false crates/reprise-view/src/lib.rs
expect_routes true false true Cargo.lock
expect_routes false false true crates/reprise-runtime/src/lib.rs
expect_routes false false false docs/agents/branching.md
expect_routes false false false .github/workflows/ci.yml
expect_routes false false false showroom/src/App.tsx
expect_routes true false true unexpected-product-root/new-source.rs

[[ $("$classifier" --owner-skip pull_request marvinbaudach marvinbaudach \
    'ci: tune routing [owner skip ci]') == true ]] || \
    fail "the repository owner must be able to request an explicit PR-only skip"
[[ $("$classifier" --owner-skip push marvinbaudach marvinbaudach \
    'ci: tune routing [owner skip ci]') == false ]] || \
    fail "an owner marker must not skip protected-branch push CI"
[[ $("$classifier" --owner-skip pull_request contributor marvinbaudach \
    'ci: tune routing [owner skip ci]') == false ]] || \
    fail "a non-owner marker must not skip CI"

"$aggregator" success success false true success false skipped false skipped
"$aggregator" success success false false skipped true success false skipped
"$aggregator" success success false true success false skipped true success
"$aggregator" success skipped true false skipped false skipped false skipped
if "$aggregator" success success false true skipped false skipped false skipped 2>/dev/null; then
    fail "a selected Android route must not accept a skipped Android suite"
fi
if "$aggregator" success success false false skipped maybe skipped false skipped 2>/dev/null; then
    fail "an invalid GNOME route must fail closed"
fi
if "$aggregator" success failure false false skipped false skipped false skipped 2>/dev/null; then
    fail "a failed base contract job must fail the aggregate Quality gate"
fi
if "$aggregator" success success true false skipped false skipped false skipped 2>/dev/null; then
    fail "an owner skip must require the base contract job to be skipped"
fi

rg --multiline --quiet \
    '^  quality:\n    name: Quality gate\n    needs: \[changes, base-contracts, android-unit-suite, gnome-suite, core-suite\]\n    if: always\(\)' \
    "$workflow" || fail "Quality gate must aggregate every routed job and always report"
rg --quiet '^  base-contracts:$' "$workflow" || \
    fail "the always-on base and contract job is missing"
rg --quiet '^  gnome-suite:$' "$workflow" || \
    fail "the routed GNOME quality suite is missing"
rg --quiet '^  core-suite:$' "$workflow" || \
    fail "the routed Core quality suite is missing"
rg --quiet "needs\.changes\.outputs\.android == 'true'" "$workflow" || \
    fail "the Android suite is not routed by the Android classifier output"
rg --quiet "needs\.changes\.outputs\.gnome == 'true'" "$workflow" || \
    fail "the GNOME suite is not routed by the GNOME classifier output"
rg --quiet "needs\.changes\.outputs\.core == 'true'" "$workflow" || \
    fail "the Core suite is not routed by the Core classifier output"
rg --quiet "needs\.changes\.outputs\.owner_skip != 'true'" "$workflow" || \
    fail "routed jobs do not honour the authenticated owner skip"
rg --quiet 'ci-paths\.sh --diff' "$workflow" || \
    fail "the workflow does not use the tested path classifier"
rg --quiet 'require-ci-results\.sh' "$workflow" || \
    fail "the Quality gate does not use the tested result aggregator"
rg --fixed-strings --quiet 'GITHUB_ACTIONS=false "$contract"' "$workflow" || \
    fail "the PR base job must run event-sensitive workflow tests in static contract mode"
rg --quiet 'check-gnome-ci\.sh' "$workflow" || \
    fail "GNOME-only changes must use the targeted GNOME gate"
rg --quiet 'cargo test --locked -p reprise-view -p reprise-android-ffi' "$workflow" || \
    fail "Android CI must test its shared Rust presentation and FFI crates"
rg --quiet '^      DISPLAY_TEST_JOBS: 4$' "$workflow" || \
    fail "display tests must use four isolated workers"
if [[ $(rg -c 'uses: actions/checkout@v7' "$workflow") -lt 6 ]]; then
    fail "every script-running job, including Quality gate, must check out the revision"
fi
rg --quiet '^      - crates/reprise-view/\*\*$' "$cross_target" || \
    fail "cross-target CI must cover the shared reprise-view crate"
if rg --quiet '^      - \.github/workflows/cross-target\.yml$' "$cross_target"; then
    fail "CI-only edits must not start the expensive cross-target workflow"
fi
rg --quiet "needs\['owner-skip'\]\.outputs\.owner_skip != 'true'" "$cross_target" || \
    fail "the repository-owner skip must also suppress cross-target compilation"
rg --quiet "needs\['owner-skip'\]\.outputs\.owner_skip != 'true'" "$showroom" || \
    fail "the repository-owner skip must also suppress the Showroom PR suite"
if rg --quiet -- "- '\.github/workflows/pages\.yml'" "$showroom"; then
    fail "CI-only edits must not start the Showroom build"
fi

echo "CI path-routing contracts passed"
