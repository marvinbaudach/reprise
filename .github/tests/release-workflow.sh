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
import re
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

flatpak_steps = jobs["flatpak"].get("steps", [])

flatpak_tooling = named_step("flatpak", "Install Flatpak tooling and add Flathub")
flatpak_tooling_run = flatpak_tooling.get("run", "")
assert "set -euo pipefail" in flatpak_tooling_run, "Flatpak tooling must fail closed"
assert "apt-get install --yes flatpak flatpak-builder ostree" in flatpak_tooling_run, (
    "the runner must install the Flatpak and OSTree tooling"
)
assert "flatpak --user remote-add --if-not-exists flathub" in flatpak_tooling_run, (
    "Flathub must be configured before restoring the user installation"
)
assert "flatpak --user install" not in flatpak_tooling_run, (
    "the uncached tooling step must not install runtime refs"
)
assert flatpak_tooling_run.count("flatpak --version") == 1, (
    "Flatpak tooling must report the Flatpak version once"
)
assert flatpak_tooling_run.count("ostree --version") == 1, (
    "Flatpak tooling must report the OSTree version once"
)

flatpak_cache = named_step("flatpak", "Restore cached Flatpak runtimes")
assert flatpak_cache.get("uses") == "actions/cache@v6"
assert flatpak_cache.get("continue-on-error", False) is False, (
    "restoring cached Flatpak runtimes must fail the release job"
)
assert "if" not in flatpak_cache, (
    "restoring cached Flatpak runtimes must run on the normal success path"
)
flatpak_cache_with = flatpak_cache.get("with", {})
assert flatpak_cache_with.get("path") == "~/.local/share/flatpak", (
    "only the user Flatpak installation may be cached"
)
assert "restore-keys" not in flatpak_cache_with, (
    "a prefix fallback could restore a stale or partial OSTree tree"
)

flatpak_install = named_step("flatpak", "Install GNOME 50 SDK")
flatpak_install_run = flatpak_install.get("run", "")
assert "set -euo pipefail" in flatpak_install_run, "Flatpak installation must fail closed"
assert "apt-get" not in flatpak_install_run, (
    "apt tooling must stay outside the cached runtime installation"
)
primary_install = re.search(
    r"^\s*if flatpak --user install .*? flathub \\\n(?P<refs>.*?); then$",
    flatpak_install_run,
    flags=re.MULTILINE | re.DOTALL,
)
assert primary_install, "the retried Flatpak install command must be recognizable"
flatpak_runtime_refs = tuple(
    re.findall(
        r"\borg\.[A-Za-z0-9_.-]+//[A-Za-z0-9_.-]+\b",
        primary_install.group("refs"),
    )
)
assert flatpak_runtime_refs, "the retried Flatpak install command must name runtime refs"
assert len(flatpak_runtime_refs) == len(set(flatpak_runtime_refs)), (
    "the retried Flatpak install command must not repeat runtime refs"
)
expected_flatpak_cache_key = "flatpak-user-" + "-".join(
    runtime_ref.replace("//", "-") for runtime_ref in flatpak_runtime_refs
)
assert flatpak_cache_with.get("key") == expected_flatpak_cache_key, (
    "the exact cache key must be mechanically derived from every installed runtime ref"
)
assert "for attempt in 1 2 3 4 5; do" in flatpak_install_run, (
    "Flatpak runtime installation must make exactly five bounded attempts"
)
assert "retry_delays=(20 40 60 120)" in flatpak_install_run, (
    "Flatpak retries must back off over a multi-minute window"
)
assert 'sleep "$delay"' in flatpak_install_run, (
    "Flatpak retries must use the increasing backoff delay"
)
assert flatpak_install_run.count("--no-static-deltas") == 2, (
    "both normal and diagnostic Flatpak pulls must avoid static-delta objects"
)
assert "static-delta object" in flatpak_install_run, (
    "the plain-object pull must explain the mid-transfer mirror 404 mechanism"
)
assert len(
    re.findall(
        r"^\s*(?:if )?flatpak\b.*\binstall\b",
        flatpak_install_run,
        flags=re.MULTILINE,
    )
) == 2, (
    "Flatpak installation needs one retried command and one final diagnostic command"
)
assert "--or-update" in flatpak_install_run, "Flatpak retries must resume partial installs"
assert "--ostree-verbose" in flatpak_install_run and " -v " in flatpak_install_run, (
    "the post-exhaustion Flatpak command must expose the exact failing URL"
)
assert 'verbose_log="$RUNNER_TEMP/flatpak-runtime-install.log"' in flatpak_install_run, (
    "the verbose Flatpak diagnostic must stay in the runner temporary directory"
)
assert '> "$verbose_log" 2>&1 || true' in flatpak_install_run, (
    "the verbose pull must be captured without replacing the original failure"
)
assert "starting fetch of" in flatpak_install_run, (
    "the diagnostic must collect objects whose fetch actually started"
)
assert "fetch of" in flatpak_install_run and "complete" in flatpak_install_run, (
    "the diagnostic must collect objects whose fetch completed"
)
assert 'comm -23 "$started_objects" "$completed_objects"' in flatpak_install_run, (
    "the diagnostic must report only started objects that never completed"
)
assert "queuing fetch of" not in flatpak_install_run, (
    "queued objects are not evidence that their fetch started"
)
assert "https://dl.flathub.org/repo/objects/" in flatpak_install_run, (
    "incomplete objects must be probed directly on Flathub"
)
assert "--head" in flatpak_install_run, (
    "HTTP status probes must not download served Flatpak objects"
)
assert ") || status=000" in flatpak_install_run, (
    "an HTTP probe failure must remain diagnostic rather than replacing the install failure"
)
assert "archive-z2" in flatpak_install_run and ".filez" in flatpak_install_run, (
    "archive-z2 file objects must probe the served .filez form"
)
assert "probe_extensions+=(filez)" in flatpak_install_run, (
    "logged .file objects must probe the archive-z2 .filez URL too"
)
assert "--show-commit" in flatpak_install_run and ".commit" in flatpak_install_run, (
    "the diagnostic must compare object availability with the ref commit object"
)
assert '2>/dev/null || true)' in flatpak_install_run, (
    "failure to resolve the ref commit must not replace the install failure"
)
assert '>> "$GITHUB_STEP_SUMMARY"' in flatpak_install_run and ">&2" in flatpak_install_run, (
    "the verdict must reach both the step summary and stderr"
)
assert "incomplete publish on the Flathub side" in flatpak_install_run, (
    "the verdict must name a served-commit/missing-object publish failure"
)
assert "not a fault of this workflow or this runner" in flatpak_install_run, (
    "the verdict must rule out the workflow and runner"
)
assert "re-run once Flathub republishes the ref" in flatpak_install_run, (
    "the verdict must give the operator the safe recovery action"
)
assert "exit 1" in flatpak_install_run, "exhausted Flatpak installation must fail the job"

flatpak_verify = named_step("flatpak", "Verify restored Flatpak runtimes")
assert flatpak_verify.get("continue-on-error", False) is False, (
    "runtime presence verification must fail the release job"
)
assert "if" not in flatpak_verify, (
    "runtime presence verification must run on the normal success path"
)
flatpak_verify_run = flatpak_verify.get("run", "")
assert "set -euo pipefail" in flatpak_verify_run, "runtime verification must fail closed"
assert "flatpak --user list --columns=application,branch" in flatpak_verify_run, (
    "verification must list the application and branch fields from the user installation"
)
assert "awk -F '\\t' '{ print $1 \"//\" $2 }'" in flatpak_verify_run, (
    "verification must normalize Flatpak's columns back into application//branch refs"
)
assert (
    'grep --fixed-strings --line-regexp --quiet "$required_ref" <<< "$installed_refs"'
    in flatpak_verify_run
), "verification must require an exact normalized ref rather than a substring"
for runtime_ref in flatpak_runtime_refs:
    assert runtime_ref in flatpak_verify_run, f"verification must require {runtime_ref}"
assert "Missing required Flatpak runtime" in flatpak_verify_run, (
    "a broken restored tree must fail with a clear operator message"
)
assert "exit 1" in flatpak_verify_run, "a missing runtime must fail the release job"

for earlier, later in zip(
    (
        "Check out the tested revision",
        "Install Flatpak tooling and add Flathub",
        "Reclaim disk for the Flatpak build",
        "Restore cached Flatpak runtimes",
        "Install GNOME 50 SDK",
    ),
    (
        "Install Flatpak tooling and add Flathub",
        "Reclaim disk for the Flatpak build",
        "Restore cached Flatpak runtimes",
        "Install GNOME 50 SDK",
        "Verify restored Flatpak runtimes",
    ),
    strict=True,
):
    earlier_step = named_step("flatpak", earlier)
    later_step = named_step("flatpak", later)
    assert flatpak_steps.index(earlier_step) < flatpak_steps.index(later_step), (
        f"{earlier!r} must run before {later!r}"
    )

flatpak_diagnostic_upload = named_step("flatpak", "Upload Flatpak runtime install log")
assert flatpak_diagnostic_upload.get("if") == "failure()", (
    "the verbose runtime-install log must be uploaded only after failure"
)
assert flatpak_diagnostic_upload.get("uses") == "actions/upload-artifact@v7"
assert flatpak_diagnostic_upload.get("with", {}).get("name") == (
    "flatpak-runtime-install-log"
), "the verbose runtime-install artifact name is part of the operator contract"
assert flatpak_diagnostic_upload.get("with", {}).get("path") == (
    "${{ runner.temp }}/flatpak-runtime-install.log"
), "the uploaded diagnostic must be the runner-temporary verbose log"

flatpak_cleanup = named_step("flatpak", "Reclaim disk for the Flatpak build")
flatpak_cleanup_run = flatpak_cleanup.get("run", "")
for path in (
    "/usr/local/lib/android",
    "/usr/share/dotnet",
    "/opt/ghc",
    "/usr/local/.ghcup",
    "/opt/hostedtoolcache/CodeQL",
):
    assert f"sudo rm -rf {path} || true" in flatpak_cleanup_run, (
        f"Flatpak cleanup must tolerate an absent {path}"
    )
assert "docker image prune --all --force || true" in flatpak_cleanup_run, (
    "Flatpak cleanup must discard preloaded Docker images without failing the job"
)
assert "After runner cleanup (before Flatpak runtime restore)" in flatpak_cleanup_run, (
    "the cleanup summary must identify its pre-runtime-restore measurement point"
)
assert "df -h /" in flatpak_cleanup_run
assert '>> "$GITHUB_STEP_SUMMARY"' in flatpak_cleanup_run

flatpak_build = named_step("flatpak", "Build the Flatpak repository")
flatpak_build_run = flatpak_build.get("run", "")
assert "After flatpak-builder (repository build complete)" in flatpak_build_run
assert "df -h /" in flatpak_build_run
assert '>> "$GITHUB_STEP_SUMMARY"' in flatpak_build_run

flatpak_bundle = named_step("flatpak", "Create the single-file bundle")
flatpak_bundle_run = flatpak_bundle.get("run", "")
assert 'echo "version=$version" >> "$GITHUB_OUTPUT"' in flatpak_bundle_run, (
    "Flatpak bundle step must expose its desktop version"
)
assert "After flatpak build-bundle (single-file bundle complete)" in flatpak_bundle_run
assert "df -h /" in flatpak_bundle_run
assert '>> "$GITHUB_STEP_SUMMARY"' in flatpak_bundle_run
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

decision_run = named_step(
    "gate", "Wait for the exact Quality gate and check tag absence"
).get("run", "")
conclusion_case = decision_run.split('case "$conclusion" in', 1)[1].split("esac", 1)[0]
for conclusion in ("skipped", "*"):
    branch = re.search(
        rf"^\s*{re.escape(conclusion)}\)\n(?P<body>.*?)(?=^\s*(?:\w+|\*)\)|\Z)",
        conclusion_case,
        flags=re.MULTILINE | re.DOTALL,
    )
    assert branch, f"release decision has no {conclusion!r} conclusion branch"
    body = branch.group("body")
    assert ">&2" in body, f"{conclusion} conclusion must be visible on stderr"
    assert "exit 1" in body, f"{conclusion} conclusion must fail the release run"

cancelled = re.search(
    r"^\s*cancelled\)\n(?P<body>.*?)(?=^\s*(?:\w+|\*)\))",
    conclusion_case,
    flags=re.MULTILINE | re.DOTALL,
)
assert cancelled and "exit 0" in cancelled.group("body"), (
    "a superseded promotion must remain a successful no-op"
)
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
# The workflow expression must remain a literal pattern rather than expand in this shell.
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
