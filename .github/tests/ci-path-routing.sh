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
expect_routes false false false quality/run-python-lint.mjs
expect_routes false false false ruff.toml
expect_routes false false false .yamllint.yaml
expect_routes false false false .markdownlint-cli2.jsonc
expect_routes true false true unexpected-product-root/new-source.rs

[[ $("$classifier" --suite-skip pull_request refs/pull/12/merge \
    contributor marvinbaudach head dev) == true ]] || \
    fail "every pull request must skip the expensive external suites"
[[ $("$classifier" --suite-skip push refs/heads/dev \
    marvinbaudach marvinbaudach same same) == false ]] || \
    fail "a dev push must always run its selected suites"
[[ $("$classifier" --suite-skip push refs/heads/main \
    marvinbaudach marvinbaudach same same) == true ]] || \
    fail "an exact owner promotion may reuse the dev evidence on main"
[[ $("$classifier" --suite-skip push refs/heads/main \
    contributor marvinbaudach same same) == false ]] || \
    fail "a non-owner main push must never reuse dev evidence"
[[ $("$classifier" --suite-skip push refs/heads/main \
    marvinbaudach marvinbaudach head dev) == false ]] || \
    fail "a main revision different from dev must run every selected suite"

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
rg --quiet "needs\.changes\.outputs\.suite_skip != 'true'" "$workflow" || \
    fail "routed jobs do not honour the authenticated suite reuse"
rg --quiet 'ci-paths\.sh --suite-skip' "$workflow" || \
    fail "the workflow does not use the tested suite-reuse classifier"
rg --quiet 'ACTOR:' "$workflow" || fail "the workflow does not authenticate the push actor"
rg --quiet 'REF_NAME:' "$workflow" || fail "the workflow does not bind reuse to main"
rg --quiet 'dev_sha=\$\(git rev-parse --verify origin/dev\)' "$workflow" || \
    fail "the workflow does not require exact dev identity"
rg --quiet 'ci-paths\.sh --diff' "$workflow" || \
    fail "the workflow does not use the tested path classifier"
rg --quiet 'require-ci-results\.sh' "$workflow" || \
    fail "the Quality gate does not use the tested result aggregator"
rg --fixed-strings --quiet 'GITHUB_ACTIONS=false "$contract"' "$workflow" || \
    fail "the PR base job must run event-sensitive workflow tests in static contract mode"
rg --quiet 'check-project-quality\.sh --project --showroom' "$workflow" || \
    fail "the base job must run project and Showroom source quality"
rg --quiet 'uses: actions/setup-node@v7' "$workflow" || \
    fail "the base source-quality job must install the pinned Node generation"
rg --quiet 'node-version: "26\.7\.0"' "$workflow" || \
    fail "the base source-quality job must use the project Node Current pin"
rg --quiet 'uses: astral-sh/setup-uv@v7\.6\.0' "$workflow" || \
    fail "the base source-quality job must install uv through the pinned action"
rg --quiet 'version: "0\.12\.3"' "$workflow" || \
    fail "the base source-quality job must use the verified uv pin"
core_workflow=$(sed -n '/^  core-suite:/,/^  quality:/p' "$workflow")
rg --quiet 'uses: actions/setup-node@v7' <<<"$core_workflow" || \
    fail "the Core suite must install the pinned Node generation before the complete gate"
rg --quiet 'node-version: "26\.7\.0"' <<<"$core_workflow" || \
    fail "the Core suite must use the project Node Current pin"
rg --quiet 'uses: astral-sh/setup-uv@v7\.6\.0' <<<"$core_workflow" || \
    fail "the Core suite must install uv through the pinned action"
rg --quiet 'version: "0\.12\.3"' <<<"$core_workflow" || \
    fail "the Core suite must use the verified uv pin"
rg --quiet 'check-project-quality\.sh --android' "$workflow" || \
    fail "the Android job must run Android source quality"
rg --multiline --quiet \
    'name: Run the Android JVM unit suite\n        run: scripts/check-android-suite\.sh\n\n      - name: Run Android source quality\n        run: scripts/check-project-quality\.sh --android' \
    "$workflow" || \
    fail "Android CI must generate UniFFI bindings before source lint"
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
rg --quiet "needs\['suite-skip'\]\.outputs\.suite_skip != 'true'" "$cross_target" || \
    fail "PR reuse and exact owner promotions must suppress duplicate cross-target compilation"
rg --quiet 'dev_sha=\$\(git rev-parse --verify origin/dev\)' "$cross_target" || \
    fail "cross-target reuse does not require exact dev identity"
if rg --quiet '^  pull_request:' "$showroom"; then
    fail "Showroom must build only after a merge reaches main"
fi
if rg --quiet 'owner-skip|suite-skip|ci-paths\.sh' "$showroom"; then
    fail "the main-only Showroom publication must never contain a CI skip path"
fi
if rg --quiet -- "- '\.github/workflows/pages\.yml'" "$showroom"; then
    fail "CI-only edits must not start the Showroom build"
fi
rg --multiline --quiet \
    'working-directory: showroom\n        run: npm run lint' "$showroom" || \
    fail "the Showroom build must lint before publishing"
rg --quiet 'check-display-tests\.sh --rule-named' scripts/check-merge-readiness.sh || \
    fail "the merge gate must keep rule-owned display coverage, not every low-risk display test"

echo "CI path-routing contracts passed"
