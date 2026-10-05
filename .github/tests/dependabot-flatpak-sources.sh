#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# A workflow path argument lets a mutated copy be checked; the contract is
# proven by showing that each mutation fails it.
workflow="${1:-$repo_root/.github/workflows/dependabot-flatpak-sources.yml}"
ci_workflow="$repo_root/.github/workflows/ci.yml"
cross_target="$repo_root/.github/workflows/cross-target.yml"

fail() {
    printf 'Dependabot Flatpak sources contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing workflow file $workflow"

# The generator is the one dependency fetched from outside the repository, and
# it runs on a runner that holds a token able to push. Its commit and hash are
# recorded together: moving one without the other fails the download check, and
# moving both is a reviewed change to this file.
generator_commit=74697c75b630d7330e77250fc13cb5ea688d9479
generator_sha256=0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380

rg --multiline --quiet \
    '^on:\n  pull_request:\n    branches:\n      - dev\n    types:\n      - opened\n      - reopened\n      - synchronize\n    paths:\n      - Cargo\.lock\n\npermissions:\n  contents: read\n' \
    "$workflow" || \
    fail "the job must react to dev pull requests that change Cargo.lock, with read-only Actions permissions"
if rg --quiet 'pull_request_target|workflow_run' "$workflow"; then
    fail "the job must run from the pull_request event only, which is what hands it the Dependabot secrets"
fi
rg --fixed-strings --quiet \
    "github.event.pull_request.user.login == 'dependabot[bot]'" "$workflow" || \
    fail "the job must accept only pull requests authored by Dependabot"
rg --fixed-strings --quiet "github.event.pull_request.base.ref == 'dev'" "$workflow" || \
    fail "the job must reject pull requests targeting any branch except dev"
rg --fixed-strings --quiet \
    'github.event.pull_request.head.repo.full_name == github.repository' "$workflow" || \
    fail "the job must reject pull requests from forks"
rg --fixed-strings --quiet \
    "startsWith(github.event.pull_request.head.ref, 'dependabot/')" "$workflow" || \
    fail "the job must accept only Dependabot's own branches"
rg --fixed-strings --quiet 'ref: ${{ github.event.pull_request.head.sha }}' "$workflow" || \
    fail "the regenerate job must check out the pull request's head commit, not its branch name"
rg --fixed-strings --quiet "github.repository == 'marvinbaudach/reprise'" "$workflow" || \
    fail "the job must be bound to this repository"
if rg --quiet '^[^#]*github\.actor' "$workflow"; then
    fail "the guard must read the pull request's author: github.actor is the token's owner after the first push"
fi

rg --fixed-strings --quiet "GENERATOR_COMMIT: $generator_commit" "$workflow" || \
    fail "the generator must be pinned to commit $generator_commit"
rg --fixed-strings --quiet "GENERATOR_SHA256: $generator_sha256" "$workflow" || \
    fail "the generator's recorded sha256 must be $generator_sha256"
rg --fixed-strings --quiet \
    'flatpak-builder-tools/$GENERATOR_COMMIT/cargo/flatpak-cargo-generator.py' "$workflow" || \
    fail "the generator must be fetched from flatpak-builder-tools at the pinned commit"
rg --fixed-strings --quiet \
    'echo "$GENERATOR_SHA256  $generator" | sha256sum --check --strict' "$workflow" || \
    fail "the fetched generator must be verified against the recorded sha256"
if rg --quiet 'flatpak-builder-tools/(master|main|HEAD)' "$workflow"; then
    fail "the generator must never be fetched from a moving ref"
fi

# The regeneration must produce what the documented invocation produces, or the
# file it writes would not satisfy the check script. It runs from outside the
# checkout with uv's config discovery off and its resolution frozen at a fixed
# date, so neither the pull request nor a later PyPI release can steer uv.
rg --fixed-strings --quiet \
    'flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json' \
    "$repo_root/flatpak/README.md" || \
    fail "flatpak/README.md no longer documents the invocation this job reproduces"
rg --multiline --quiet \
    'cd "\$RUNNER_TEMP" \|\| exit 1\n          uv run --no-config --exclude-newer 2026-10-05T00:00:00Z \\\n            --script "\$RUNNER_TEMP/flatpak-cargo-generator\.py" \\\n            "\$GITHUB_WORKSPACE/Cargo\.lock" \\\n            -o "\$RUNNER_TEMP/regenerated/cargo-sources\.json"' \
    "$workflow" || \
    fail "the generator must run from the temp directory as: uv run --no-config --exclude-newer <date> --script <generator> <workspace>/Cargo.lock -o <temp>/cargo-sources.json"

# The no-op run has to stay a no-op: that is what ends the loop, because a
# token push starts a new run of this very job.
rg --fixed-strings --quiet 'cmp --silent "$RUNNER_TEMP/regenerated/cargo-sources.json"' "$workflow" || \
    fail "the job must detect whether the regeneration changed anything"
rg --fixed-strings --quiet "if: needs.regenerate.outputs.changed == 'true'" "$workflow" || \
    fail "the push job must run only when the sources changed"
rg --fixed-strings --quiet \
    'if [[ $(git log -1 --format='"'%ae %s'"' HEAD) == "$bot $subject" ]]; then' \
    "$workflow" || \
    fail "the push job must refuse to push on top of its own regeneration commit"
rg --fixed-strings --quiet \
    "if [[ \$changes != ' M flatpak/cargo-sources.json' ]]; then" "$workflow" || \
    fail "the push must refuse to commit anything but flatpak/cargo-sources.json"
rg --fixed-strings --quiet "if [[ \$listing != 'f cargo-sources.json' ]]; then" "$workflow" || \
    fail "the downloaded artifact must be exactly one regular file named cargo-sources.json"
rg --fixed-strings --quiet 'git -c core.hooksPath=/dev/null commit' "$workflow" || \
    fail "the commit must run with hooks disabled"
rg --fixed-strings --quiet 'git -c core.hooksPath=/dev/null push' "$workflow" || \
    fail "the push must run with hooks disabled"
rg --fixed-strings --quiet \
    'scripts/check-flatpak-cargo-sources.sh Cargo.lock "$RUNNER_TEMP/regenerated/cargo-sources.json"' "$workflow" || \
    fail "the handed-over artifact must be validated against Cargo.lock before it is committed"

python3 - "$workflow" "$generator_commit" "$generator_sha256" <<'PY' || fail "the job split does not isolate the token from the generator"
import pathlib
import re
import sys

import yaml

with pathlib.Path(sys.argv[1]).open(encoding="utf-8") as stream:
    workflow = yaml.safe_load(stream)

generator_commit, generator_sha256 = sys.argv[2], sys.argv[3]

# PyYAML reads the bare key `on` as the boolean True.
assert list(workflow[True]) == ["pull_request"], (
    f"the workflow must trigger on pull_request alone, got {list(workflow[True])}"
)
jobs = workflow["jobs"]
assert sorted(jobs) == ["push", "regenerate"], (
    f"the work must be split into exactly a regenerate and a push job, got {sorted(jobs)}"
)
regenerate, push = jobs["regenerate"], jobs["push"]
assert push["needs"] == "regenerate", "the push job must wait for the regeneration"

# The token lives in the push job alone, and only in its last step.
assert "REPRISE_AUTOMERGE_TOKEN" not in yaml.safe_dump(regenerate), (
    "the regenerate job runs third-party code and must hold no secret"
)
assert "secrets." not in yaml.safe_dump(regenerate), (
    "the regenerate job must not read any secret"
)
push_steps = push["steps"]
holders = [s["name"] for s in push_steps if "REPRISE_AUTOMERGE_TOKEN" in yaml.safe_dump(s)]
assert holders == [push_steps[-1]["name"]], (
    f"only the push job's last step may see REPRISE_AUTOMERGE_TOKEN, got {holders}"
)
assert "REPRISE_AUTOMERGE_TOKEN" not in yaml.safe_dump(
    {key: value for key, value in push.items() if key != "steps"}
), "the token must not be set job-wide"

# The push job executes nothing from the pull request and nothing from PyPI.
assert "uv" not in yaml.safe_dump(push).replace("runs-on", ""), (
    "the push job must not install or run uv"
)
for step in push_steps:
    assert "setup-uv" not in step.get("uses", ""), "the push job must not set up uv"
    assert "flatpak-cargo-generator" not in yaml.safe_dump(step), (
        "the push job must not run the generator"
    )
assert not [s for s in push_steps if "uses" in s], (
    "the push job must run no third-party action: a bumped tag would run before the token step"
)
assert any("gh run download" in yaml.safe_dump(step) for step in push_steps), (
    "the push job must take the regenerated file from the artifact"
)

# The generator is fetched and verified before it runs, and only the
# regenerate job does either.
names = [step["name"] for step in regenerate["steps"]]
assert names.index("Fetch the pinned generator") < names.index(
    "Regenerate flatpak/cargo-sources.json"
), "the generator must be fetched and verified before it runs"
assert regenerate["outputs"]["changed"] == "${{ steps.regenerate.outputs.changed }}"

# The push job builds on the commit the sources were generated from, so a
# branch that moved in between makes the push fail instead of mixing states.
assert regenerate["outputs"]["sha"] == "${{ steps.revision.outputs.sha }}"
assert push["steps"][0]["env"]["SHA"] == "${{ needs.regenerate.outputs.sha }}", (
    "the push job must check out the commit the regenerate job recorded"
)
revision = [s for s in regenerate["steps"] if s.get("id") == "revision"]
assert len(revision) == 1 and "git rev-parse HEAD" in revision[0]["run"]
assert regenerate["steps"].index(revision[0]) < names.index(
    "Fetch the pinned generator"
), "the commit must be recorded before any third-party code runs"

# The checkout leaves no credential in its clone and is handed no token.
checkout = regenerate["steps"][0]
assert checkout["uses"].startswith("actions/checkout@"), checkout
assert checkout["with"]["persist-credentials"] is False, (
    "checkout must not leave a credential in the clone"
)
assert "token" not in checkout["with"], "checkout must not be handed a token"
# --- What each guard means, not just that its words appear somewhere. ---

# Every condition is one conjunction of exactly these terms: an `||`, a missing
# term or a term replaced by `true` widens who can reach the token.
condition = " ".join(str(regenerate["if"]).split())
assert "||" not in condition, f"the regenerate guard must not contain ||: {condition}"
assert sorted(term.strip() for term in condition.split("&&")) == sorted([
    "github.repository == 'marvinbaudach/reprise'",
    "github.event.pull_request.user.login == 'dependabot[bot]'",
    "github.event.pull_request.base.ref == 'dev'",
    "github.event.pull_request.head.repo.full_name == github.repository",
    "startsWith(github.event.pull_request.head.ref, 'dependabot/')",
]), f"the regenerate guard must be exactly the five required terms, got: {condition}"
assert push["if"] == "needs.regenerate.outputs.changed == 'true'"

# Permissions are pinned as whole maps, so no job quietly widens them.
assert workflow["permissions"] == {"contents": "read"}, workflow["permissions"]
assert "permissions" not in regenerate, "the regenerate job must inherit contents: read"
assert push["permissions"] == {"actions": "read", "contents": "read"}, push["permissions"]

# A newer run replaces the one in flight, never queues behind it.
assert workflow["concurrency"]["cancel-in-progress"] is True, (
    "concurrency must cancel the run in flight"
)
assert workflow["concurrency"]["group"] == (
    "dependabot-flatpak-sources-${{ github.event.pull_request.number }}"
)

# The handed-over artifact is the single file the push job downloads by name.
uploads = [s for s in regenerate["steps"] if str(s.get("uses", "")).startswith("actions/upload-artifact@")]
assert len(uploads) == 1, "the regenerate job must upload exactly one artifact"
upload = uploads[0]["with"]
assert upload["path"] == "${{ runner.temp }}/regenerated/cargo-sources.json", (
    f"the artifact must be the single sources file, not a directory: {upload['path']}"
)
assert upload["if-no-files-found"] == "error"
downloads = [
    match.group(1)
    for step in push_steps
    for match in re.finditer(r"gh run download \"\$GITHUB_RUN_ID\" --name (\S+)", step.get("run", ""))
]
assert downloads == [upload["name"]], (
    f"the push job must download the artifact the regenerate job uploads: {downloads} vs {upload['name']}"
)

# The push is a plain fast-forward of the bump's own branch.
pushes = [s["run"] for s in push_steps if "git -c core.hooksPath=/dev/null push" in s.get("run", "")]
assert len(pushes) == 1, "the push job must push exactly once"
command = pushes[0].split("core.hooksPath=/dev/null push", 1)[1]
assert not re.search(r"--force|--mirror|--delete|--prune|\s-f\b|\s-d\b", command), (
    f"the push must never be forced or destructive: {command}"
)
refspecs = re.findall(r'"(\+?HEAD:[^"]*)"', command)
assert refspecs == ["HEAD:refs/heads/$BRANCH"], f"the push must send HEAD to the bump's branch only: {refspecs}"

# The secret is read in exactly one place: the push job's push step.
rest = yaml.safe_dump({
    "workflow": {k: v for k, v in workflow.items() if k != "jobs"},
    "regenerate": regenerate,
    "push": {**push, "steps": push_steps[:-1]},
})
assert "secrets." not in rest, "no secret may be read outside the push job's push step"
assert yaml.safe_dump(push_steps[-1]).count("secrets.") == 1
assert re.findall(r"secrets\.(\w+)", yaml.safe_dump(workflow)) == ["REPRISE_AUTOMERGE_TOKEN"]
assert "github.token" not in yaml.safe_dump(push_steps[-1]), (
    "the Actions token must not be in the step that holds the push token"
)
assert "github.token" not in yaml.safe_dump(regenerate), "the regenerate job needs no token"

# The generator runs pinned and isolated, with no moving dependency.
generate = next(s for s in regenerate["steps"] if s.get("id") == "regenerate")["run"]
assert "--no-config" in generate and "--exclude-newer" in generate, (
    "the generator must run with uv config discovery off and resolution frozen at a date"
)
assert re.search(r"--exclude-newer \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", generate)
fetch = next(s for s in regenerate["steps"] if s["name"] == "Fetch the pinned generator")
assert fetch["env"] == {
    "GENERATOR_COMMIT": generator_commit,
    "GENERATOR_SHA256": generator_sha256,
}, "the generator's commit and sha256 must be the recorded pair"
setup_uv = [s for s in regenerate["steps"] if str(s.get("uses", "")).startswith("astral-sh/setup-uv@")]
assert len(setup_uv) == 1 and setup_uv[0]["with"]["version"] == "0.12.3", (
    "uv itself must be pinned to a version"
)
PY

# A push by the token's owner makes that owner the event's actor. If routing
# read the actor, the re-triggered run would treat the bump as a human pull
# request, skip every suite, and let auto-merge arm on untested code.
for file in "$ci_workflow" "$cross_target"; do
    rg --fixed-strings --quiet \
        'ACTOR: ${{ github.event.pull_request.user.login || github.actor }}' "$file" || \
        fail "$(basename "$file") must route on the pull request's author, falling back to the actor for pushes"
done

echo "Dependabot Flatpak sources contracts passed"
