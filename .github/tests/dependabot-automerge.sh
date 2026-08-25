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
    '^on:\n  pull_request:\n    branches:\n      - dev\n    types:\n      - opened\n      - reopened\n      - synchronize\n  workflow_run:\n    workflows:\n      - CI\n    branches:\n      - dev\n    types:\n      - completed\n' \
    "$workflow" || \
    fail "auto-merge must react to dev pull requests and to CI finishing on dev"
rg --multiline --quiet \
    '^permissions:\n  contents: write\n  pull-requests: write\n' \
    "$workflow" || fail "auto-merge needs only contents and pull-request write access"
rg --quiet \
    "github\.event\.pull_request\.user\.login == 'dependabot\[bot\]'" \
    "$workflow" || fail "the job must accept only Dependabot pull requests"
rg --quiet "github\.event\.pull_request\.base\.ref == 'dev'" "$workflow" || \
    fail "the job must reject pull requests targeting any branch except dev"
rg --quiet "github\.event_name == 'workflow_run'" "$workflow" || \
    fail "the CI-finished trigger must be allowed past the pull-request guard"
rg --quiet "github\.repository == 'marvinbaudach/reprise'" "$workflow" || \
    fail "the job must be bound to this repository"
rg --quiet 'gh pr merge --auto --squash "\$url"' "$workflow" || \
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
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import yaml

path = pathlib.Path(sys.argv[1])
with path.open(encoding="utf-8") as stream:
    workflow = yaml.safe_load(stream)
steps = workflow["jobs"]["reconcile-automerge"]["steps"]
names = [step.get("name") for step in steps]

health = "Read dev's last quality gate"
reconcile = "Arm or disarm every affected pull request"
assert health in names, f"auto-merge must read dev's health first, got {names}"
assert reconcile in names, f"auto-merge must reconcile the pull requests, got {names}"
assert names.index(health) < names.index(reconcile), (
    "dev's health must be read before anything is armed or disarmed, not after"
)

run = steps[names.index(health)]["run"]
assert "repos/$GITHUB_REPOSITORY/commits/dev/check-runs" in run, (
    "the health check must read dev's own check runs, not the pull request's"
)

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
# A wrong jq selector would be caught above, but the branch that decides what to
# do with the answer would not: the whole defect this replaces was a step that
# said no and changed nothing. So the reconcile script is extracted and run for
# real against a stubbed `gh`, and what it dispatched is what is asserted.
reconcile_run = steps[names.index(reconcile)]["run"]
assert "--disable-auto" in reconcile_run, (
    "a broken dev must disarm auto-merge, not merely decline to arm it"
)

stub_dir = tempfile.mkdtemp()
log = pathlib.Path(stub_dir, "calls.log")
listed = pathlib.Path(stub_dir, "open-prs.txt")
GH_STUB = r"""#!/usr/bin/env bash
printf '%s\n' "$*" >> "$GH_STUB_LOG"
# The listing is answered with real JSON put through the step's own --jq. A stub
# that printed the answer directly would accept whatever selector the workflow
# asked for, including one that returns a single bump and leaves the rest armed.
if [[ $1 == pr && $2 == list ]]; then
  selector=.
  while [[ $# -gt 0 ]]; do
    if [[ $1 == --jq ]]; then selector=$2; fi
    shift
  done
  jq -R -s 'split("\n") | map(select(length > 0) | {url: .})' "$GH_STUB_LIST" |
    jq -r "$selector"
  exit 0
fi
# A pull request that was never armed makes the real gh exit non-zero. The step
# has to survive that: reaching the disarmed state is the point, and already
# being there is not a failure.
if [[ $1 == pr && $2 == merge && $3 == --disable-auto ]]; then exit 1; fi
exit 0
"""
pathlib.Path(stub_dir, "gh").write_text(GH_STUB, encoding="utf-8")
pathlib.Path(stub_dir, "gh").chmod(0o755)


def reconcile_with(event, conclusion, open_prs=(), event_pr=""):
    log.write_text("", encoding="utf-8")
    listed.write_text("".join(f"{url}\n" for url in open_prs), encoding="utf-8")
    result = subprocess.run(
        ["bash", "-e", "-c", reconcile_run],
        env={
            "PATH": stub_dir + ":" + os.environ["PATH"],
            "GH_STUB_LOG": str(log),
            "GH_STUB_LIST": str(listed),
            "GITHUB_EVENT_NAME": event,
            "GITHUB_REPOSITORY": "marvinbaudach/reprise",
            "CONCLUSION": conclusion,
            "EVENT_PR_URL": event_pr,
        },
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, (
        f"reconcile failed for {event}/{conclusion!r}: {result.stderr}"
    )
    return [line for line in log.read_text(encoding="utf-8").splitlines() if line]


BUMP_A = "https://github.com/marvinbaudach/reprise/pull/669"
BUMP_B = "https://github.com/marvinbaudach/reprise/pull/676"


def merges(calls):
    return [call for call in calls if call.startswith("pr merge")]


assert merges(reconcile_with("pull_request", "success", event_pr=BUMP_A)) == [
    f"pr merge --auto --squash {BUMP_A}"
], "a green dev must arm the pull request that triggered the run"

assert merges(reconcile_with("pull_request", "failure", event_pr=BUMP_A)) == [
    f"pr merge --disable-auto {BUMP_A}"
], "a red dev must disarm the pull request rather than leave it armed"

assert merges(reconcile_with("pull_request", "", event_pr=BUMP_A)) == [
    f"pr merge --disable-auto {BUMP_A}"
], "an unfinished gate says nothing about dev, so it must not leave a bump armed"

# The case #676 got wrong on 2026-08-25: it was armed by an earlier run, dev
# then had no green gate, and nothing went back to take the arming away.
assert merges(
    reconcile_with("workflow_run", "failure", open_prs=(BUMP_A, BUMP_B))
) == [
    f"pr merge --disable-auto {BUMP_A}",
    f"pr merge --disable-auto {BUMP_B}",
], "CI failing on dev must disarm every bump still waiting, not just a new one"

assert merges(
    reconcile_with("workflow_run", "success", open_prs=(BUMP_A, BUMP_B))
) == [
    f"pr merge --auto --squash {BUMP_A}",
    f"pr merge --auto --squash {BUMP_B}",
], "CI going green on dev must re-arm the bumps an earlier refusal left standing"

assert merges(reconcile_with("workflow_run", "success")) == [], (
    "with nothing open there is nothing to arm"
)

PY
