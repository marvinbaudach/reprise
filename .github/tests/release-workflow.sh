#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
workflow="$repo_root/.github/workflows/release.yml"

fail() {
    printf 'Release workflow contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -f $workflow ]] || fail "$workflow does not exist"

python3 - "$workflow" <<'PY'
import pathlib
import sys
import yaml

path = pathlib.Path(sys.argv[1])
with path.open(encoding="utf-8") as stream:
    workflow = yaml.safe_load(stream)
assert isinstance(workflow, dict), "workflow root must be a mapping"
jobs = workflow.get("jobs", {})
assert set(jobs) == {"gate", "flatpak", "apk", "publish"}, jobs.keys()

def named_step(job_name, step_name):
    for step in jobs[job_name].get("steps", []):
        if step.get("name") == step_name:
            return step
    raise AssertionError(f"{job_name} job has no {step_name!r} step")

flatpak_bundle = named_step("flatpak", "Create the single-file bundle")
assert 'echo "version=$version" >> "$GITHUB_OUTPUT"' in flatpak_bundle.get("run", ""), (
    "Flatpak bundle step must expose its desktop version"
)
flatpak_upload = named_step("flatpak", "Upload Flatpak bundle")
assert flatpak_upload.get("with", {}).get("name") == (
    "reprise-flatpak-${{ steps.bundle.outputs.version }}"
), "Flatpak upload artifact name must use the bundle step version"

apk_package = named_step("apk", "Verify signature, certificate, and versions")
assert 'echo "version=$gradle_version" >> "$GITHUB_OUTPUT"' in apk_package.get("run", ""), (
    "APK package step must expose its Android version"
)
apk_upload = named_step("apk", "Upload universal APK")
assert apk_upload.get("with", {}).get("name") == (
    "reprise-apk-${{ steps.package.outputs.version }}"
), "APK upload artifact name must use the package step version"

publish = jobs["publish"]
assert "gate" in publish.get("needs", []), "publish job must have gate outputs in scope"
assert "github.event_name != 'pull_request'" in publish.get("if", ""), (
    "publish job must stay disabled for pull requests"
)
flatpak_download = named_step("publish", "Download Flatpak bundle")
assert flatpak_download.get("with", {}).get("name") == (
    "reprise-flatpak-${{ needs.gate.outputs.desktop_version }}"
), "Flatpak download artifact name must match the versioned upload"
apk_download = named_step("publish", "Download universal APK")
assert apk_download.get("with", {}).get("name") == (
    "reprise-apk-${{ needs.gate.outputs.android_version }}"
), "APK download artifact name must match the versioned upload"
PY

require() {
    local pattern=$1 message=$2
    rg --quiet --multiline -- "$pattern" "$workflow" || fail "$message"
}

reject() {
    local pattern=$1 message=$2
    ! rg --quiet --multiline -- "$pattern" "$workflow" || fail "$message"
}

require '^on:\n  push:\n    branches: \[main\]' "main promotion push trigger is missing"
require '^  workflow_dispatch:' "manual dry-run trigger is missing"
require 'dry_run:' "manual trigger has no dry_run input"
require 'default: true' "dry_run must default true"
require '^  pull_request:\n    paths:' "pull-request build trigger is missing"
require 'group: release-\$\{\{ github\.ref \}\}' "release concurrency key is wrong"
require 'cancel-in-progress: false' "release runs must never cancel one another"
require '^permissions:\n  contents: read' "workflow default must be contents read"
reject 'container:' "Flatpak must build directly on the Ubuntu host"
reject '\$\{\{ env\.ANDROID_(SDK_ROOT|NDK_LATEST_HOME) \}\}' "runner Android paths must be read by the shell, not the unavailable env context"

require "if: github.event_name != 'pull_request'" "gate must be skipped for pull requests"
for state in queued in_progress cancelled skipped success; do
    require "$state" "gate does not handle Quality gate state $state"
done
require 'scripts/bump-version\.sh current' "desktop version must use the shared parser"
require 'scripts/check-release-metadata\.sh' "gate must require full release metadata"
require 'check-runs' "gate does not query check-runs for the exact SHA"
require '-F per_page=100' "Quality gate polling must inspect up to 100 check runs"
require 'check_status=\$\?' "Quality gate polling does not capture transient API failures"
require 'if \(\(check_status != 0\)\); then' "Quality gate polling does not retry transient API failures"
require 'git/ref/tags/' "gate does not use tag absence as the publish condition"

[[ $(rg -c 'always\(\) &&' "$workflow") -eq 2 ]] || \
    fail "both build jobs must override the skipped PR gate"
require "github.event_name == 'pull_request'" "build jobs do not recognize pull-request mode"
require 'flatpak-builder --repo=repo --force-clean build-dir' "Flatpak build is missing"
require 'flatpak build-bundle repo' "single-file Flatpak bundle is missing"
[[ $(rg -c 'ANDROID_TARGET=.*scripts/android-build\.sh' "$workflow") -eq 2 ]] || \
    fail "APK job must call scripts/android-build.sh exactly twice"
require 'REPRISE_REQUIRE_RELEASE_SIGNING' "APK job does not control required signing explicitly"
require 'ANDROID_KEYSTORE_BASE64' "APK job does not receive the upload keystore"
# The workflow expression and shell variable must remain literal patterns here.
# shellcheck disable=SC2016
require 'if \[\[ \$GITHUB_EVENT_NAME == pull_request \|\| -z \$KEYSTORE_BASE64' \
    "pull-request APKs must never use the production upload key"
require 'Pull-request builds never use the production upload key' \
    "pull-request signing summary does not explain the upload-key policy"
require 'apksigner verify --print-certs' "APK signature verification is missing"
require 'aapt2 dump badging' "APK version assertion is missing"
[[ $(rg -c 'sha256sum' "$workflow") -ge 2 ]] || fail "both applications need SHA-256 files"
# shellcheck disable=SC2016
require 'EXPECTED_DESKTOP_VERSION: \$\{\{ needs\.gate\.outputs\.desktop_version \}\}' \
    "Flatpak gate output must enter the shell through env"
reject 'expected=\$\{\{ needs\.gate\.outputs\.desktop_version \}\}' \
    "Flatpak gate output must not be spliced into shell source"

create_line=$(rg -n 'gh release create' "$workflow" | cut -d: -f1)
verify_line=$(rg -n 'gh release view.*--json assets' "$workflow" | cut -d: -f1)
publish_line=$(rg -n 'gh release edit.*--draft=false.*--latest' "$workflow" | cut -d: -f1)
[[ -n $create_line && -n $verify_line && -n $publish_line ]] || \
    fail "draft, verification and publication commands must all exist"
((create_line < verify_line && verify_line < publish_line)) || \
    fail "release must be drafted, verified, then published in that order"
require 'if: failure\(\)' "failed publication has no cleanup step"
require 'gh release delete.*--cleanup-tag' "cleanup must remove both a stale draft and any tag"

echo "Release workflow contracts passed"
