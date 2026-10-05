#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
classifier="$repo_root/.github/scripts/ci-paths.sh"
aggregator="$repo_root/.github/scripts/require-ci-results.sh"
gnome_gate="$repo_root/.github/scripts/check-gnome-ci.sh"
merge_readiness="$repo_root/scripts/check-merge-readiness.sh"
ci_quality="$repo_root/scripts/ci-quality.sh"
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
    local expected_display=$4
    shift 4
    local output
    output=$("$classifier" --paths "$@")
    rg --quiet "^android=$expected_android$" <<<"$output" || \
        fail "expected android=$expected_android for $*; got: $output"
    rg --quiet "^gnome=$expected_gnome$" <<<"$output" || \
        fail "expected gnome=$expected_gnome for $*; got: $output"
    rg --quiet "^core=$expected_core$" <<<"$output" || \
        fail "expected core=$expected_core for $*; got: $output"
    rg --quiet "^display=$expected_display$" <<<"$output" || \
        fail "expected display=$expected_display for $*; got: $output"
}

expect_routes true false false false android/app/src/main/MainActivity.kt
expect_routes true false false false crates/reprise-android-ffi/src/lib.rs
expect_routes false true false true crates/reprise-gnome/src/main.rs
expect_routes false true false true crates/reprise-platform-linux/src/lib.rs
expect_routes true false true true crates/reprise-core/src/lib.rs
expect_routes true true false true crates/reprise-view/src/lib.rs
expect_routes true false true true Cargo.lock
expect_routes false false true true crates/reprise-runtime/src/lib.rs
expect_routes false false false false docs/agents/branching.md
expect_routes false false false false .github/workflows/ci.yml
expect_routes false false false false showroom/src/App.tsx
expect_routes false false false false quality/run-python-lint.mjs
expect_routes false false false false ruff.toml
expect_routes false false false false .yamllint.yaml
expect_routes false false false false .markdownlint-cli2.jsonc
expect_routes true false true true unexpected-product-root/new-source.rs
# The core suite's workspace gate already tests the GNOME crate, so a path set
# that routes core never routes the GNOME suite; a GNOME-only set still does.
expect_routes true false true true crates/reprise-core/src/lib.rs crates/reprise-gnome/src/main.rs
expect_routes true false true true crates/reprise-view/src/lib.rs crates/reprise-core/src/lib.rs

expect_diff_routes() {
    local expected_android=$1
    local expected_gnome=$2
    local expected_core=$3
    local expected_display=$4
    local event=$5
    local ref=$6
    local output
    output=$("$classifier" --diff "$event" "$ref" HEAD HEAD)
    rg --quiet "^android=$expected_android$" <<<"$output" || \
        fail "expected android=$expected_android for $event on $ref; got: $output"
    rg --quiet "^gnome=$expected_gnome$" <<<"$output" || \
        fail "expected gnome=$expected_gnome for $event on $ref; got: $output"
    rg --quiet "^core=$expected_core$" <<<"$output" || \
        fail "expected core=$expected_core for $event on $ref; got: $output"
    rg --quiet "^display=$expected_display$" <<<"$output" || \
        fail "expected display=$expected_display for $event on $ref; got: $output"
}

expect_diff_routes true false true true push refs/heads/main
expect_diff_routes true false true true schedule refs/heads/main

[[ $("$classifier" --suite-skip pull_request refs/pull/12/merge \
    contributor marvinbaudach head dev) == true ]] || \
    fail "a human pull request must skip the expensive external suites"
[[ $("$classifier" --suite-skip pull_request refs/pull/12/merge \
    marvinbaudach marvinbaudach head dev) == true ]] || \
    fail "an owner pull request must skip the expensive external suites too"
[[ $("$classifier" --suite-skip pull_request refs/pull/12/merge \
    'dependabot[bot]' marvinbaudach head dev) == false ]] || \
    fail "a dependabot pull request merges itself, so it must run its suites"
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

"$aggregator" success success false true success false skipped false skipped false skipped false
"$aggregator" success success false false skipped true success false skipped true success false
"$aggregator" success success false true success false skipped true success true success false
"$aggregator" success success false false skipped false skipped false skipped false skipped false
"$aggregator" success skipped true false skipped false skipped false skipped false skipped false
if "$aggregator" success success false true skipped false skipped false skipped false skipped false 2>/dev/null; then
    fail "a selected Android route must not accept a skipped Android suite"
fi
if "$aggregator" success success false false skipped maybe skipped false skipped false skipped false 2>/dev/null; then
    fail "an invalid GNOME route must fail closed"
fi
if "$aggregator" success success false false skipped false skipped false skipped true failure false 2>/dev/null; then
    fail "a selected display route must not accept a failed display matrix"
fi
if "$aggregator" success failure false false skipped false skipped false skipped false skipped false 2>/dev/null; then
    fail "a failed base contract job must fail the aggregate Quality gate"
fi
if "$aggregator" success success true false skipped false skipped false skipped false skipped false 2>/dev/null; then
    fail "an owner skip must require the base contract job to be skipped"
fi

# --- Containment: a Dependabot bump with stale Flatpak sources. ---
# Its suites are skipped, base-contracts still runs, and the Quality gate stays
# red. This is NOT suite reuse: reuse skips base-contracts too, so the very check
# that is red would never run, and the aggregator turns green on that path.
[[ $("$classifier" --contain pull_request 'dependabot[bot]' 1) == true ]] || \
    fail "a Dependabot pull request with red Flatpak sources must be contained"
[[ $("$classifier" --contain pull_request 'dependabot[bot]' 0) == false ]] || \
    fail "a Dependabot pull request with green Flatpak sources must run its suites"
[[ $("$classifier" --contain pull_request 'dependabot[bot]' unexpected) == true ]] || \
    fail "an unreadable sources status must fail closed"
[[ $("$classifier" --contain pull_request contributor 1) == false ]] || \
    fail "a human pull request is never contained: it skips its suites as suite reuse already"
[[ $("$classifier" --contain push refs/heads/dev 1) == false ]] || \
    fail "a push is never contained"
[[ $("$classifier" --contain push 'dependabot[bot]' 1) == false ]] || \
    fail "only a pull request is contained, whoever pushed"
"$aggregator" success success false false skipped false skipped false skipped false skipped false
if "$aggregator" success success false false skipped false skipped false skipped false skipped true 2>/dev/null; then
    fail "a contained run must fail the Quality gate even when every result reads as skipped"
fi
if "$aggregator" success failure false false skipped false skipped false skipped false skipped true 2>/dev/null; then
    fail "a contained run must fail the Quality gate when base-contracts failed"
fi
if "$aggregator" success success true false skipped false skipped false skipped false skipped true 2>/dev/null; then
    fail "a contained run must fail the Quality gate even when suite reuse is also set"
fi
if "$aggregator" success success false false skipped false skipped false skipped false skipped maybe 2>/dev/null; then
    fail "an invalid containment value must fail closed"
fi

# Run the routing steps of the real workflows, extracted from the YAML, against a
# stubbed git and a stubbed sources check. Reading the workflow text would not
# notice a containment that is wired through `suite_skip`, which turns the gate
# green; running it does.
sandbox=$(mktemp -d "${TMPDIR:-/tmp}/ci-contain.XXXXXX")
trap 'rm -rf "$sandbox"' EXIT
mkdir -p "$sandbox/bin" "$sandbox/ws/.github/scripts" "$sandbox/ws/scripts"
command cp -f "$classifier" "$sandbox/ws/.github/scripts/ci-paths.sh"
cat > "$sandbox/bin/git" <<'STUB'
#!/usr/bin/env bash
case "$1" in
    fetch) exit 0 ;;
    rev-parse) echo 1111111111111111111111111111111111111111 ;;
    *) exit 1 ;;
esac
STUB
cat > "$sandbox/ws/scripts/check-flatpak-cargo-sources.sh" <<'STUB'
#!/usr/bin/env bash
touch "$STUB_MARKER"
exit "$STUB_SOURCES_STATUS"
STUB
chmod +x "$sandbox/bin/git" "$sandbox/ws/scripts/check-flatpak-cargo-sources.sh"

extract_step_script() {
    python3 - "$1" "$2" "$3" <<'PY'
import sys

import yaml

with open(sys.argv[1], encoding="utf-8") as stream:
    workflow = yaml.safe_load(stream)
for step in workflow["jobs"][sys.argv[2]]["steps"]:
    if step.get("id") == sys.argv[3]:
        print(step["run"])
        break
else:
    sys.exit(f"{sys.argv[1]}: job {sys.argv[2]} has no step {sys.argv[3]}")
PY
}

# run_step WORKFLOW JOB STEP ACTOR SOURCES_STATUS [SUITE_SKIP]: leaves the step's
# outputs in $step_output and whether the sources check ran in $sources_check_ran.
run_step() {
    local file=$1 job=$2 step=$3 actor=$4 status=$5 suite_skip=${6:-}
    extract_step_script "$file" "$job" "$step" > "$sandbox/step.sh" || \
        fail "cannot extract $job/$step from $(basename "$file")"
    : > "$sandbox/outputs"
    rm -f "$sandbox/marker"
    (
        cd "$sandbox/ws"
        PATH="$sandbox/bin:$PATH" EVENT_NAME=pull_request REF_NAME=refs/pull/9/merge \
            ACTOR=$actor REPOSITORY_OWNER=marvinbaudach BASE_SHA=base HEAD_SHA=head \
            SUITE_SKIP=$suite_skip STUB_SOURCES_STATUS=$status STUB_MARKER="$sandbox/marker" \
            GITHUB_OUTPUT="$sandbox/outputs" bash -e "$sandbox/step.sh" > "$sandbox/step.log" 2>&1
    ) || fail "the $job/$step step of $(basename "$file") failed: $(tail -3 "$sandbox/step.log")"
    step_output=$(cat "$sandbox/outputs")
    sources_check_ran=false
    [[ -e $sandbox/marker ]] && sources_check_ran=true
    return 0
}

output_of() {
    sed -n "s/^$1=//p" <<<"$step_output" | tail -1
}

expect_output() {
    [[ $(output_of "$1") == "$2" ]] || \
        fail "$3: expected $1=$2, got '$(output_of "$1")' in: $step_output"
}

# ci.yml: the case that matters. Dependabot PR, red Flatpak sources -> Quality gate red.
run_step "$workflow" changes routes 'dependabot[bot]' 1
case_name="Dependabot PR with red Flatpak sources"
[[ $sources_check_ran == true ]] || fail "$case_name: the sources check never ran"
expect_output suite_skip false "$case_name must keep base-contracts, so it must not be suite reuse"
expect_output contained true "$case_name must be contained"
for surface in android gnome core display; do
    expect_output "$surface" false "$case_name must skip the $surface suite"
done
# base-contracts runs and fails on the same check; the gate must be red whether it
# reports that failure or, hypothetically, a success.
for base_result in failure success; do
    if "$aggregator" success "$base_result" "$(output_of suite_skip)" \
        "$(output_of android)" skipped "$(output_of gnome)" skipped \
        "$(output_of core)" skipped "$(output_of display)" skipped \
        "$(output_of contained)" 2>/dev/null; then
        fail "$case_name must end with a red Quality gate (base-contracts: $base_result)"
    fi
done

run_step "$workflow" changes routes 'dependabot[bot]' 0
expect_output suite_skip false "Dependabot PR with green Flatpak sources"
expect_output contained false "Dependabot PR with green Flatpak sources"
expect_output core true "Dependabot PR with green Flatpak sources must still run its suites"
"$aggregator" success success "$(output_of suite_skip)" \
    "$(output_of android)" success "$(output_of gnome)" skipped \
    "$(output_of core)" success "$(output_of display)" success "$(output_of contained)" || \
    fail "Dependabot PR with green Flatpak sources and green suites must pass the gate"

run_step "$workflow" changes routes contributor 1
expect_output suite_skip true "human PR"
expect_output contained false "human PR"
[[ $sources_check_ran == false ]] || fail "a human pull request must not run the sources check in the routing job"

# cross-target.yml: the same decision, and its compilation job honours it.
run_step "$cross_target" suite-skip containment 'dependabot[bot]' 1 false
[[ $sources_check_ran == true ]] || fail "cross-target: the sources check never ran"
expect_output contained true "cross-target: a Dependabot PR with red Flatpak sources"
run_step "$cross_target" suite-skip containment 'dependabot[bot]' 0 false
expect_output contained false "cross-target: a Dependabot PR with green Flatpak sources"
run_step "$cross_target" suite-skip containment contributor 1 true
expect_output contained false "cross-target: a human PR"
[[ $sources_check_ran == false ]] || fail "cross-target: a suite-reuse run must not run the sources check"
run_step "$cross_target" suite-skip authorization 'dependabot[bot]' 1
expect_output suite_skip false "cross-target: a Dependabot PR with red Flatpak sources must not be suite reuse"

python3 - "$workflow" "$cross_target" <<'PY' || fail "the containment wiring in the workflows is wrong"
import re
import sys

import yaml


def squash(text) -> str:
    return " ".join(str(text).split())


with open(sys.argv[1], encoding="utf-8") as stream:
    ci = yaml.safe_load(stream)
with open(sys.argv[2], encoding="utf-8") as stream:
    cross = yaml.safe_load(stream)

jobs = ci["jobs"]
# base-contracts is the check that goes red, so containment must never switch it off.
assert squash(jobs["base-contracts"]["if"]) == "needs.changes.outputs.suite_skip != 'true'", (
    "base-contracts must run unless the run is suite reuse; containment must not skip it"
)
for name in ("android-unit-suite", "gnome-suite", "core-suite", "display-tests"):
    assert squash(jobs[name]["if"]).startswith("needs.changes.outputs.suite_skip != 'true' &&"), name
assert jobs["changes"]["outputs"]["contained"] == "${{ steps.routes.outputs.contained }}"
assert jobs["changes"]["outputs"]["suite_skip"] == "${{ steps.routes.outputs.suite_skip }}", (
    "the routing job's suite_skip must be the classifier's verdict alone, never containment"
)

routes = next(s for s in jobs["changes"]["steps"] if s.get("id") == "routes")["run"]
assert len(re.findall(r"^\s*suite_skip=", routes, flags=re.MULTILINE)) == 1, (
    "suite_skip must have exactly one producer: ci-paths.sh --suite-skip"
)
assert re.search(r"^\s*suite_skip=\$\(\s*\.github/scripts/ci-paths\.sh --suite-skip", routes, flags=re.MULTILINE)

gate = next(s for s in jobs["quality"]["steps"] if s["name"] == "Require every selected gate")
assert gate["env"]["CONTAINED"] == "${{ needs.changes.outputs.contained }}"
assert squash(gate["run"]).endswith('"$DISPLAY_ROUTE" "$DISPLAY_RESULT" "$CONTAINED"'), (
    "the Quality gate must hand the containment verdict to the aggregator last"
)
assert gate["env"]["SUITE_SKIP"] == "${{ needs.changes.outputs.suite_skip }}", (
    "the gate's suite reuse input must be the routing job's suite_skip alone"
)

assert cross["jobs"]["suite-skip"]["outputs"]["contained"] == "${{ steps.containment.outputs.contained }}"
assert cross["jobs"]["suite-skip"]["outputs"]["suite_skip"] == "${{ steps.authorization.outputs.suite_skip }}", (
    "cross-target's suite_skip must be the classifier's verdict alone, never containment"
)
assert squash(cross["jobs"]["cross-target"]["if"]) == (
    "needs['suite-skip'].outputs.suite_skip != 'true' && "
    "needs['suite-skip'].outputs.contained != 'true'"
), "the cross-target job must skip both suite reuse and a contained bump"
PY

rg --multiline --quiet \
    '^  quality:\n    name: Quality gate\n    needs: \[changes, base-contracts, android-unit-suite, gnome-suite, core-suite, display-tests\]\n    if: always\(\)' \
    "$workflow" || fail "Quality gate must aggregate every routed job and always report"
rg --quiet '^  base-contracts:$' "$workflow" || \
    fail "the always-on base and contract job is missing"
rg --quiet '^  gnome-suite:$' "$workflow" || \
    fail "the routed GNOME quality suite is missing"
rg --quiet '^  core-suite:$' "$workflow" || \
    fail "the routed Core quality suite is missing"
rg --quiet '^  display-tests:$' "$workflow" || \
    fail "the routed display-test matrix is missing"
rg --quiet "needs\.changes\.outputs\.android == 'true'" "$workflow" || \
    fail "the Android suite is not routed by the Android classifier output"
rg --quiet "needs\.changes\.outputs\.gnome == 'true'" "$workflow" || \
    fail "the GNOME suite is not routed by the GNOME classifier output"
rg --quiet "needs\.changes\.outputs\.core == 'true'" "$workflow" || \
    fail "the Core suite is not routed by the Core classifier output"
rg --quiet "needs\.changes\.outputs\.display == 'true'" "$workflow" || \
    fail "the display matrix is not routed by the display classifier output"
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
rg --multiline --quiet \
    '"\$EVENT_NAME"\s+"\$REF_NAME" "\$BASE_SHA" "\$HEAD_SHA"' "$workflow" || \
    fail "the workflow does not pass the tested ref to diff routing"
rg --multiline --quiet \
    "schedule:\n    - cron: '17 3 \* \* \*'" "$workflow" || \
    fail "the nightly full-suite schedule is missing"
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
rg --quiet 'uses: astral-sh/setup-uv@v10\.0\.1' "$workflow" || \
    fail "the base source-quality job must install uv through the pinned action"
rg --quiet 'version: "0\.12\.3"' "$workflow" || \
    fail "the base source-quality job must use the verified uv pin"
android_workflow=$(sed -n '/^  android-unit-suite:/,/^  gnome-suite:/p' "$workflow")
rg --multiline --quiet \
    'uses: actions/setup-java@v6\n        with:\n          distribution: temurin\n          java-version: "21"\n          cache: gradle' \
    <<<"$android_workflow" || fail "the Android JVM suite must use setup-java's Gradle cache"
core_workflow=$(sed -n '/^  core-suite:/,/^  display-tests:/p' "$workflow")
rg --quiet 'uses: actions/setup-node@v7' <<<"$core_workflow" || \
    fail "the Core suite must install the pinned Node generation before the complete gate"
rg --quiet 'node-version: "26\.7\.0"' <<<"$core_workflow" || \
    fail "the Core suite must use the project Node Current pin"
rg --quiet 'uses: astral-sh/setup-uv@v10\.0\.1' <<<"$core_workflow" || \
    fail "the Core suite must install uv through the pinned action"
rg --quiet 'version: "0\.12\.3"' <<<"$core_workflow" || \
    fail "the Core suite must use the verified uv pin"
rg --quiet 'uses: Swatinem/rust-cache@v2' <<<"$core_workflow" || \
    fail "the Core suite must cache its Rust dependency build graph"
if [[ $(rg -c 'uses: Swatinem/rust-cache@v2' "$workflow") -ne 1 ]]; then
    fail "rust-cache must appear in core-suite only"
fi
rg --quiet 'check-project-quality\.sh --android' "$workflow" || \
    fail "the Android job must run Android source quality"
rg --multiline --quiet \
    'name: Run the Android JVM unit suite\n        run: scripts/check-android-suite\.sh\n\n      - name: Run Android source quality\n        run: scripts/check-project-quality\.sh --android' \
    "$workflow" || \
    fail "Android CI must generate UniFFI bindings before source lint"
rg --quiet 'check-gnome-ci\.sh' "$workflow" || \
    fail "GNOME-only changes must use the targeted GNOME gate"
if rg --quiet 'check-display-tests\.sh' "$gnome_gate"; then
    fail "the GNOME gate must leave all display tests to the display matrix"
fi
skip_gates_literal=$(sed -n "s/^MERGE_READINESS_SKIP_GATES=\\\$'\\(.*\\)' \\\\$/\\1/p" "$ci_quality")
[[ -n $skip_gates_literal ]] || \
    fail "the complete workspace skip list could not be parsed; expected one MERGE_READINESS_SKIP_GATES=\$'...\\n...' \\ assignment line"
mapfile -t skip_gates < <(sed 's/\\n/\n/g' <<<"$skip_gates_literal")
mapfile -t readiness_gates < <(sed -n 's/^gate "\([^"]*\)" -- .*/\1/p' "$merge_readiness")
required_display_skip='Rule-owned display tests'
display_skip_found=false
for skip_gate in "${skip_gates[@]}"; do
    if [[ $skip_gate == "$required_display_skip" ]]; then
        display_skip_found=true
    fi
    matched=false
    for readiness_gate in "${readiness_gates[@]}"; do
        if [[ $skip_gate == "$readiness_gate" ]]; then
            matched=true
            break
        fi
    done
    [[ $matched == true ]] || \
        fail "skip-list entry does not match a merge-readiness gate: $skip_gate"
done
[[ $display_skip_found == true ]] || \
    fail "the complete workspace gate must skip display tests owned by the matrix"
display_workflow=$(awk '
    /^  display-tests:$/ { in_display_job = 1 }
    in_display_job && /^  [a-z][a-z0-9-]*:$/ && $0 != "  display-tests:" { exit }
    in_display_job { print }
' "$workflow")
rg --multiline --quiet \
    'strategy:\n      fail-fast: false\n      matrix:\n        shard: \[1, 2\]\n    runs-on:' \
    <<<"$display_workflow" || \
    fail "the display matrix must collect both shard outcomes"
rg --fixed-strings --quiet \
    'name: Display tests ${{ matrix.shard }}/${{ strategy.job-total }}' \
    <<<"$display_workflow" || fail "the display matrix title must derive its shard total"
rg --fixed-strings --quiet 'DISPLAY_SHARD: ${{ matrix.shard }}' \
    <<<"$display_workflow" || fail "the display matrix must expose the selected shard"
rg --fixed-strings --quiet 'DISPLAY_SHARD_COUNT: ${{ strategy.job-total }}' \
    <<<"$display_workflow" || fail "the display matrix must derive its shard total"
rg --fixed-strings --quiet \
    'scripts/check-display-tests.sh --shard "$DISPLAY_SHARD/$DISPLAY_SHARD_COUNT"' \
    <<<"$display_workflow" || fail "the display matrix must execute the selected shard"
rg --multiline --quiet \
    'name: Verify display-test shard partition\n        if: matrix\.shard == 1\n        env:\n          DISPLAY_SHARD_COUNT: \$\{\{ strategy\.job-total \}\}\n        run: \|' \
    <<<"$display_workflow" || \
    fail "one display shard must verify that the matrix partitions the full suite"
rg --fixed-strings --quiet \
    'scripts/check-display-tests.sh --list > "$listing_root/unsharded"' \
    <<<"$display_workflow" || fail "the partition check must list the unsharded suite"
rg --fixed-strings --quiet \
    'for ((shard = 1; shard <= DISPLAY_SHARD_COUNT; shard++)); do' \
    <<<"$display_workflow" || fail "the partition check must iterate over the live matrix size"
rg --fixed-strings --quiet \
    'scripts/check-display-tests.sh --shard "$shard/$DISPLAY_SHARD_COUNT" --list > "$listing_root/shard-$shard"' \
    <<<"$display_workflow" || fail "the partition check must list every shard"
rg --fixed-strings --quiet '(( shard_total == unsharded_count ))' \
    <<<"$display_workflow" || fail "the partition check must compare relational counts"
rg --fixed-strings --quiet 'uniq -d "$listing_root/sorted-union"' \
    <<<"$display_workflow" || fail "the partition check must reject duplicate tests"
rg --fixed-strings --quiet \
    'cmp --silent "$listing_root/unsharded" "$listing_root/sorted-union"' \
    <<<"$display_workflow" || fail "the partition check must compare the sorted union byte-for-byte"
rg --quiet 'cargo test --locked -p reprise-view -p reprise-android-ffi' "$workflow" || \
    fail "Android CI must test its shared Rust presentation and FFI crates"
rg --quiet '^      DISPLAY_TEST_JOBS: 4$' <<<"$display_workflow" || \
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
