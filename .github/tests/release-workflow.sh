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
PY

require() {
    local pattern=$1 message=$2
    rg --quiet --multiline "$pattern" "$workflow" || fail "$message"
}

reject() {
    local pattern=$1 message=$2
    ! rg --quiet --multiline "$pattern" "$workflow" || fail "$message"
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
require 'apksigner verify --print-certs' "APK signature verification is missing"
require 'aapt2 dump badging' "APK version assertion is missing"
[[ $(rg -c 'sha256sum' "$workflow") -ge 2 ]] || fail "both applications need SHA-256 files"

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
