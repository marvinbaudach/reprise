#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/dependabot-flatpak-sources.yml"
ci_workflow="$repo_root/.github/workflows/ci.yml"
cross_target="$repo_root/.github/workflows/cross-target.yml"

fail() {
    printf 'Dependabot Flatpak sources contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing .github/workflows/dependabot-flatpak-sources.yml"

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
if rg --quiet 'pull_request_target|workflow_run|^  push:' "$workflow"; then
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

# The regeneration must be the invocation the repository documents and the
# check script prints, or the file it writes would not satisfy that check.
rg --multiline --quiet \
    'flatpak-cargo-generator\.py" \\\n            Cargo\.lock -o flatpak/cargo-sources\.json' \
    "$workflow" || \
    fail "the generator must run as: flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json"
rg --fixed-strings --quiet \
    'flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json' \
    "$repo_root/flatpak/README.md" || \
    fail "flatpak/README.md no longer documents the invocation this job runs"

# The no-op run has to stay a no-op: that is what ends the loop, because a
# token push starts a new run of this very job.
rg --fixed-strings --quiet 'git diff --quiet -- flatpak/cargo-sources.json' "$workflow" || \
    fail "the job must detect whether the regeneration changed anything"
rg --fixed-strings --quiet "if: steps.regenerate.outputs.changed == 'true'" "$workflow" || \
    fail "the push must happen only when the sources changed"

python3 - "$workflow" <<'PY' || fail "the token must reach the push step and nothing else"
import pathlib
import sys

import yaml

with pathlib.Path(sys.argv[1]).open(encoding="utf-8") as stream:
    workflow = yaml.safe_load(stream)

steps = workflow["jobs"]["regenerate"]["steps"]
holders = [
    step["name"]
    for step in steps
    if "REPRISE_AUTOMERGE_TOKEN" in yaml.safe_dump(step)
]
assert holders == ["Push the regenerated sources to the bump's branch"], (
    f"only the push step may see REPRISE_AUTOMERGE_TOKEN, got {holders}"
)
assert "REPRISE_AUTOMERGE_TOKEN" not in yaml.safe_dump(
    {key: value for key, value in workflow["jobs"]["regenerate"].items() if key != "steps"}
), "the token must not be set job-wide"

names = [step["name"] for step in steps]
assert names.index("Fetch the pinned generator") < names.index(
    "Regenerate flatpak/cargo-sources.json"
), "the generator must be fetched and verified before it runs"

checkout = steps[0]
assert checkout["uses"].startswith("actions/checkout@"), checkout
assert checkout["with"]["persist-credentials"] is False, (
    "checkout must not leave a credential in the clone"
)
assert "token" not in checkout["with"], "checkout must not be handed the push token"
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
