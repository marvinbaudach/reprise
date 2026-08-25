#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$repo_root/.github/workflows/dependabot-automerge.yml"

fail() {
    printf 'Dependabot auto-merge contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing .github/workflows/dependabot-automerge.yml"

rg --multiline --quiet \
    '^on:\n  pull_request:\n    branches:\n      - dev\n    types:\n      - opened\n      - reopened\n      - synchronize\n' \
    "$workflow" || fail "only dev pull requests may trigger auto-merge"
rg --multiline --quiet \
    '^permissions:\n  contents: write\n  pull-requests: write\n' \
    "$workflow" || fail "auto-merge needs only contents and pull-request write access"
rg --quiet \
    "github\.event\.pull_request\.user\.login == 'dependabot\[bot\]'" \
    "$workflow" || fail "the job must accept only Dependabot pull requests"
rg --quiet "github\.event\.pull_request\.base\.ref == 'dev'" "$workflow" || \
    fail "the job must reject pull requests targeting any branch except dev"
rg --quiet "github\.repository == 'marvinbaudach/reprise'" "$workflow" || \
    fail "the job must be bound to this repository"
rg --quiet 'gh pr merge --auto --squash "\$PR_URL"' "$workflow" || \
    fail "Dependabot pull requests must use the repository squash policy"
rg --fixed-strings --quiet \
    'GH_TOKEN: ${{ secrets.REPRISE_AUTOMERGE_TOKEN }}' \
    "$workflow" || fail "auto-merge must act through the owner-scoped Dependabot secret"

if rg --quiet 'actions/checkout|pull_request_target|secrets\.GITHUB_TOKEN' "$workflow"; then
    fail "the privileged workflow must not load pull request code or use the blocked Actions token"
fi

# Auto-merge runs unwatched, so the base it merges onto has to be checked by
# the workflow itself. Pinning the step by text alone would let a wrong jq
# selector through, and a selector that silently returns nothing looks exactly
# like a healthy refusal -- every Dependabot pull request would stall and
# nobody would be told why. So the selector is extracted and run for real.
python3 - "$workflow" <<'PY' || fail "the dev health check does not hold"
import pathlib
import re
import subprocess
import sys
import yaml

path = pathlib.Path(sys.argv[1])
with path.open(encoding="utf-8") as stream:
    workflow = yaml.safe_load(stream)
steps = workflow["jobs"]["enable-automerge"]["steps"]
names = [step.get("name") for step in steps]

health = "Require a healthy dev before arming auto-merge"
merge = "Merge after required checks pass"
assert health in names, f"auto-merge must check dev's health first, got {names}"
assert merge in names, f"auto-merge must still arm the merge, got {names}"
assert names.index(health) < names.index(merge), (
    "the dev health check must run before auto-merge is armed, not after"
)

run = steps[names.index(health)]["run"]
assert "repos/$GITHUB_REPOSITORY/commits/dev/check-runs" in run, (
    "the health check must read dev's own check runs, not the pull request's"
)
assert "exit 1" in run, "an unhealthy dev must fail the job rather than merge quietly"

selector = re.search(r"--jq '\n(.*?)'\n", run, re.S)
assert selector, "the health check must select the conclusion with an inline jq program"
program = selector.group(1)


def conclusion(payload):
    result = subprocess.run(
        ["jq", "-r", program],
        input=payload,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


gate = '{"name": "Quality gate", "status": "completed", "completed_at": "%s", "conclusion": "%s"}'
unrelated = '{"name": "Route changed paths", "status": "completed", "completed_at": "2026-01-01T00:00:00Z", "conclusion": "success"}'


def runs(*entries):
    return '{"check_runs": [' + ", ".join(entries) + "]}"


assert conclusion(runs(gate % ("2026-01-01T00:00:00Z", "success"))) == "success", (
    "a green dev must be reported as success"
)
assert conclusion(runs(gate % ("2026-01-01T00:00:00Z", "failure"))) == "failure", (
    "a red dev must be reported as failure"
)
assert conclusion(runs(
    gate % ("2026-01-01T00:00:00Z", "failure"),
    gate % ("2026-01-02T00:00:00Z", "success"),
)) == "success", "the newest completed gate decides, whatever order they arrive in"
assert conclusion(runs(
    gate % ("2026-01-02T00:00:00Z", "success"),
    gate % ("2026-01-03T00:00:00Z", "failure"),
)) == "failure", "a later red gate must not be masked by an earlier green one"
assert conclusion(runs(unrelated)) == "", (
    "a green unrelated check must never stand in for the quality gate"
)
assert conclusion(
    runs('{"name": "Quality gate", "status": "in_progress", "conclusion": null}')
) == "", "a gate that has not finished yet says nothing about dev"
assert conclusion(runs()) == "", "no gate at all must read as unverified, not as success"

# A run that has not finished carries no conclusion, so the `// empty` default
# already keeps it out and no payload can show this filter working. It is
# pinned as text instead, because the day that default changes the filter
# becomes the only thing standing between "still running" and "green".
assert 'select(.status == "completed")' in run, (
    "the selector must discard gates that have not finished"
)
PY
