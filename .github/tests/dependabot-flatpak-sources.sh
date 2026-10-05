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
if rg --ignore-case --quiet '^[^#]*github\.actor' "$workflow"; then
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
# date, so neither the pull request nor a later PyPI release can steer uv. Its
# dependencies come from a hashed lock beside the script, and --locked refuses
# anything the lock does not already say.
rg --fixed-strings --quiet \
    'flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json' \
    "$repo_root/flatpak/README.md" || \
    fail "flatpak/README.md no longer documents the invocation this job reproduces"
rg --multiline --quiet \
    'cp "\$GITHUB_WORKSPACE/\.github/flatpak-cargo-generator\.lock" \\\n            "\$RUNNER_TEMP/flatpak-cargo-generator\.py\.lock"\n          cd "\$RUNNER_TEMP" \|\| exit 1\n          uv run --no-config --locked --exclude-newer 2026-09-28T00:00:00Z \\\n            --script "\$RUNNER_TEMP/flatpak-cargo-generator\.py" \\\n            "\$GITHUB_WORKSPACE/Cargo\.lock" \\\n            -o "\$RUNNER_TEMP/regenerated/cargo-sources\.json"' \
    "$workflow" || \
    fail "the generator must run from the temp directory as: the hashed lock copied beside the generator, then uv run --no-config --locked --exclude-newer <date> --script <generator> <workspace>/Cargo.lock -o <temp>/cargo-sources.json"

# The no-op run has to stay a no-op: that is what ends the loop, because a
# token push starts a new run of this very job.
rg --fixed-strings --quiet 'cmp --silent "$RUNNER_TEMP/regenerated/cargo-sources.json"' "$workflow" || \
    fail "the job must detect whether the regeneration changed anything"
rg --fixed-strings --quiet "if: needs.regenerate.outputs.changed == 'true'" "$workflow" || \
    fail "the push job must run only when the sources changed"
rg --fixed-strings --quiet \
    'if [[ $(git log -1 --format='"'%ae %s'"' HEAD) == "$BOT_EMAIL $COMMIT_SUBJECT" ]]; then' \
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
rg --multiline --quiet \
    '"\$RUNNER_TEMP/base/scripts/check-flatpak-cargo-sources\.sh" \\\n            "\$GITHUB_WORKSPACE/Cargo\.lock" "\$RUNNER_TEMP/regenerated/cargo-sources\.json"' "$workflow" || \
    fail "the handed-over artifact must be validated against Cargo.lock before it is committed"

python3 - "$workflow" "$generator_commit" "$generator_sha256" "$repo_root/.github/flatpak-cargo-generator.lock" <<'PY' || fail "the job split does not isolate the token from the generator"
import pathlib
import re
import sys
import tomllib

import yaml


def squash_ws(text):
    return " ".join(str(text).split())


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

# Context names are case-insensitive in Actions (`SECRETS`, `Github.Token`), so
# every match below ignores case.
def dump(node):
    return yaml.safe_dump(node)


def mentions(node, word):
    return re.search(rf"\b{re.escape(word)}\b", dump(node), re.IGNORECASE) is not None


# The token lives in the push job alone, and only in its last step.
assert not mentions(regenerate, "REPRISE_AUTOMERGE_TOKEN"), (
    "the regenerate job runs third-party code and must hold no secret"
)
assert not mentions(regenerate, "secrets"), (
    "the regenerate job must not read any secret, in any spelling"
)
push_steps = push["steps"]
holders = [s["name"] for s in push_steps if mentions(s, "REPRISE_AUTOMERGE_TOKEN")]
assert holders == [push_steps[-1]["name"]], (
    f"only the push job's last step may see REPRISE_AUTOMERGE_TOKEN, got {holders}"
)
assert not mentions(
    {key: value for key, value in push.items() if key != "steps"}, "REPRISE_AUTOMERGE_TOKEN"
), "the token must not be set job-wide"
assert not mentions(
    {key: value for key, value in push.items() if key != "steps"}, "secrets"
), "no secret may be set job-wide"

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
checkouts = [
    step
    for job in jobs.values()
    for step in job["steps"]
    if str(step.get("uses", "")).startswith("actions/checkout@")
]
assert checkouts and checkouts[0] is regenerate["steps"][0], (
    "the regenerate job must start from a checkout"
)
# Every action here runs in a job that executes PyPI code or hands over its
# result, so a tag that could move is not enough: each is pinned to a commit.
for job in jobs.values():
    for step in job["steps"]:
        if "uses" in step:
            assert re.fullmatch(r"[\w.-]+/[\w.-]+@[0-9a-f]{40}", step["uses"]), (
                f"every action must be pinned to a 40-hex commit, got {step['uses']}"
            )
for checkout in checkouts:
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
upload_step = uploads[0]
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

# The push job runs no script from the pull request: the validator is fetched
# from the base commit into its own directory, and every other `scripts/` path
# in the job must be reached through that copy.
base_fetch = next(s for s in push_steps if s["name"] == "Fetch the validator from the base commit")
assert base_fetch["env"] == {"BASE_SHA": "${{ github.event.pull_request.base.sha }}"}, (
    "the validator must come from the pull request's base commit"
)
assert 'git -C "$RUNNER_TEMP/base" fetch --quiet --depth=1' in base_fetch["run"]
assert push_steps.index(base_fetch) < [s["name"] for s in push_steps].index(
    "Validate the regenerated sources against Cargo.lock"
)
for step in push_steps:
    local = re.findall(r"(?<![\w/$}])(?:\./)?scripts/[\w./-]+", step.get("run", ""))
    assert not local, f"the push job must not run code from the pull request worktree: {local}"
    assert "$GITHUB_WORKSPACE/scripts" not in step.get("run", "")
validate = next(s for s in push_steps if s["name"].startswith("Validate the regenerated"))
assert "$RUNNER_TEMP/base/scripts/check-flatpak-cargo-sources.sh" in validate["run"], (
    "the validator must be the base commit's copy"
)
assert "secrets." not in yaml.safe_dump(validate) and "token" not in yaml.safe_dump(validate).lower()

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
# `secrets.X`, `secrets['X']` and `toJSON(secrets)` all hand over a secret, so the
# word itself is counted, in the parsed workflow where comments no longer count.
assert not re.search(r"\bsecrets\b", rest, re.IGNORECASE), (
    "no secret may be read outside the push job's push step"
)
assert re.findall(r"\bsecrets\b", yaml.safe_dump(push_steps[-1]), re.IGNORECASE) == ["secrets"]
assert re.findall(r"\bsecrets\b", yaml.safe_dump(workflow), re.IGNORECASE) == ["secrets"], (
    "the whole workflow may read exactly one secret"
)
assert push_steps[-1]["env"]["PUSH_TOKEN"] == "${{ secrets.REPRISE_AUTOMERGE_TOKEN }}"
assert not mentions(push_steps[-1], "github.token"), (
    "the Actions token must not be in the step that holds the push token"
)
assert not mentions(regenerate, "github.token"), "the regenerate job needs no token"

# The generator runs pinned and isolated, with no moving dependency.
generate = next(s for s in regenerate["steps"] if s.get("id") == "regenerate")["run"]
assert "--no-config" in generate and "--locked" in generate, (
    "the generator must run with uv config discovery off and its hashed lock enforced"
)
LOCK_CUTOFF = "2026-09-28T00:00:00Z"
assert f"--exclude-newer {LOCK_CUTOFF}" in generate, (
    f"the cut-off must be {LOCK_CUTOFF}: more than a week behind this change, and the lock's own"
)
assert "--frozen" not in generate and "--no-sync" not in generate and "--with" not in generate, (
    "--frozen skips the lock's consistency check, and nothing may add a dependency beside it"
)
# The lock is what the job installs from: every package is a pinned PyPI release
# and every file it may install carries a sha256, so uv refuses a substitute.
lock = tomllib.loads(pathlib.Path(sys.argv[4]).read_text(encoding="utf-8"))
assert lock["options"]["exclude-newer"] == LOCK_CUTOFF, "the lock must be cut at the pinned date"
assert {item["name"] for item in lock["manifest"]["requirements"]} == {"aiohttp", "pyyaml", "tomlkit"}, (
    "the lock must cover exactly the generator's three direct dependencies"
)
for package in lock["package"]:
    assert package["source"] == {"registry": "https://pypi.org/simple"}, (
        f"{package['name']} must come from PyPI, not {package['source']}"
    )
    files = [*package.get("wheels", []), *([package["sdist"]] if "sdist" in package else [])]
    assert files, f"{package['name']} lists no file to install"
    for entry in files:
        assert re.fullmatch(r"sha256:[0-9a-f]{64}", entry.get("hash", "")), (
            f"{package['name']}: every file in the lock needs a sha256, got {entry}"
        )
fetch = next(s for s in regenerate["steps"] if s["name"] == "Fetch the pinned generator")
assert fetch["env"] == {
    "GENERATOR_COMMIT": generator_commit,
    "GENERATOR_SHA256": generator_sha256,
}, "the generator's commit and sha256 must be the recorded pair"
setup_uv = [s for s in regenerate["steps"] if str(s.get("uses", "")).startswith("astral-sh/setup-uv@")]
assert len(setup_uv) == 1 and setup_uv[0]["with"]["version"] == "0.12.3", (
    "uv itself must be pinned to a version"
)
assert re.fullmatch(r"astral-sh/setup-uv@[0-9a-f]{40}", setup_uv[0]["uses"]), (
    "setup-uv runs before the generator and must be pinned to a commit, not a tag"
)

# --- Weakening a step without touching its words (S2, S3, T4). ---
def walk(node):
    if isinstance(node, dict):
        for key, value in node.items():
            yield key, value
            yield from walk(value)
    elif isinstance(node, list):
        for value in node:
            yield from walk(value)


keys = [key for key, _ in walk(workflow)]
assert "continue-on-error" not in keys, "no step or job may continue on error"
assert "shell" not in keys and "defaults" not in keys, (
    "no step may swap the shell: the default one runs with -e, a custom one may not"
)
scripts = "\n".join(
    str(step.get("run", "")) for job in jobs.values() for step in job["steps"]
)
# Tracing is the way a masked value leaks: the base64 header is not masked until
# the step has built it, and `set -o xtrace` or `bash -x` prints it before that.
# The one allowed switch is `set +x`, which turns tracing off.
for forbidden in (
    r"set\s+\+e", r"set\s+-\w*x", r"\bxtrace\b", r"BASH_XTRACEFD", r"\bPS4\b",
    r"(?<![\w-])-\w*x\b", r"GIT_TRACE", r"GIT_CURL_VERBOSE", r"\|\|\s*(true|:)\b",
):
    assert not re.search(forbidden, scripts), f"the scripts must not contain {forbidden}"

# The sha256 check is the last thing the fetch step does, and nothing makes its
# failure acceptable.
fetch_lines = [line.strip() for line in fetch["run"].strip().splitlines()]
assert fetch_lines[-1] == 'echo "$GENERATOR_SHA256  $generator" | sha256sum --check --strict', (
    f"the fetch step must end with the sha256 check, got {fetch_lines[-1]!r}"
)
assert not any("||" in line for line in fetch_lines), "the fetch step must not tolerate a failing command"
assert names.index("Fetch the pinned generator") + 1 == names.index("Regenerate flatpak/cargo-sources.json"), (
    "nothing may sit between the verified fetch and the run"
)

# The change flag is what starts, or ends, the push job.
assert squash_ws(generate).endswith(squash_ws("""
    if cmp --silent "$RUNNER_TEMP/regenerated/cargo-sources.json" \\
      "$GITHUB_WORKSPACE/flatpak/cargo-sources.json"; then
      echo "flatpak/cargo-sources.json already matches Cargo.lock"
      echo "changed=false" >> "$GITHUB_OUTPUT"
    else
      echo "changed=true" >> "$GITHUB_OUTPUT"
    fi
""")), "changed=false must follow an identical file and changed=true a different one, and nothing may follow"
assert upload_step["if"] == "steps.regenerate.outputs.changed == 'true'", (
    "the artifact is uploaded only when the sources changed"
)

# Timeouts bound a hung run that holds a runner and, in push, a token.
assert regenerate.get("timeout-minutes") == 10, "the regenerate job must time out after 10 minutes"
assert push.get("timeout-minutes") == 5, "the push job must time out after 5 minutes"

# --- The push: target, fail-closed token, token transport (S3, S5). ---
push_step = push_steps[-1]
assert push_step["env"] == {
    "PUSH_TOKEN": "${{ secrets.REPRISE_AUTOMERGE_TOKEN }}",
    "BRANCH": "${{ github.event.pull_request.head.ref }}",
}, f"the push step's environment must be exactly the token and the bump's branch: {push_step['env']}"
assert squash_ws(push_step["run"]).startswith(squash_ws("""
    if [[ -z $PUSH_TOKEN ]]; then
      echo "REPRISE_AUTOMERGE_TOKEN is not available to this run" >&2
      exit 1
    fi
""")), "the push step must open by failing closed when its token is missing"
run = push_step["run"]
assert not re.search(r"x-access-token:\$|https://[^\s\"']*@", run), (
    "the token must not be embedded in a URL: the process list would show it"
)
assert '"https://github.com/${GITHUB_REPOSITORY}.git"' in run, "the push URL must carry no credential"
for line in (
    'echo "::add-mask::$header"',
    "GIT_CONFIG_COUNT=1 \\",
    "GIT_CONFIG_KEY_0='http.https://github.com/.extraheader' \\",
    'GIT_CONFIG_VALUE_0="AUTHORIZATION: basic $header" \\',
):
    assert line in run, f"the token must reach git as a masked header in its environment: missing {line}"
assert run.index('echo "::add-mask::$header"') < run.index("GIT_CONFIG_COUNT=1"), (
    "the header must be masked before it is used"
)
assert not re.search(r"PUSH_TOKEN.*git -c", run), "the token must not sit on a command line"

# --- The push job, pinned whole: every command it runs is on this list. ---
# The job holds a token that can push, so a command added to it, a guard
# neutralised without touching its words (`|| echo skip`, a `!` in front of the
# validator, a forged `changes=`), or a changed bot identity must break this
# contract instead of passing a text search. Comments do not count.
for job_name, job in jobs.items():
    assert job["runs-on"] == "ubuntu-24.04", (
        f"the {job_name} job must run on the GitHub-hosted ubuntu-24.04, not {job['runs-on']}"
    )
for step in push_steps:
    assert "||" not in step.get("run", ""), (
        f"no command of the push job may be made optional with ||: {step['name']}"
    )
assert push["env"] == {
    "BOT_EMAIL": "41898282+github-actions[bot]@users.noreply.github.com",
    "COMMIT_SUBJECT": "The Flatpak Cargo sources follow Cargo.lock",
}, "the bot identity must be defined once, in the push job's environment, and nowhere else"


def uncommented(run):
    return squash_ws("\n".join(line for line in str(run).splitlines() if not line.lstrip().startswith("#")))


PUSH_BODIES = {
    'Check out the commit the sources were built from': r'''
git init --quiet .
git fetch --quiet --depth=1 "https://github.com/${GITHUB_REPOSITORY}" "$SHA"
git checkout --quiet --detach FETCH_HEAD
if [[ $(git rev-parse HEAD) != "$SHA" ]]; then
  echo "checked out $(git rev-parse HEAD), not the recorded $SHA" >&2
  exit 1
fi
''',
    'Refuse to chase a regeneration that does not settle': r'''
if [[ $(git log -1 --format='%ae %s' HEAD) == "$BOT_EMAIL $COMMIT_SUBJECT" ]]; then
  echo "the tip is already the regeneration commit and the sources" \
    "still differ; not pushing again" >&2
  exit 1
fi
''',
    'Fetch the regenerated sources': r'''
gh run download "$GITHUB_RUN_ID" --name cargo-sources \
  --dir "$RUNNER_TEMP/regenerated"
listing=$(find "$RUNNER_TEMP/regenerated" -mindepth 1 -printf '%y %P\n')
if [[ $listing != 'f cargo-sources.json' ]]; then
  printf 'expected the artifact to hold only cargo-sources.json, got:\n%s\n' \
    "$listing" >&2
  exit 1
fi
''',
    'Fetch the validator from the base commit': r'''
git init --quiet "$RUNNER_TEMP/base"
git -C "$RUNNER_TEMP/base" fetch --quiet --depth=1 \
  "https://github.com/${GITHUB_REPOSITORY}" "$BASE_SHA"
git -C "$RUNNER_TEMP/base" checkout --quiet --detach FETCH_HEAD
''',
    'Validate the regenerated sources against Cargo.lock': r'''
"$RUNNER_TEMP/base/scripts/check-flatpak-cargo-sources.sh" \
  "$GITHUB_WORKSPACE/Cargo.lock" "$RUNNER_TEMP/regenerated/cargo-sources.json"
''',
    "Replace the sources and push them to the bump's branch": r'''
if [[ -z $PUSH_TOKEN ]]; then
  echo "REPRISE_AUTOMERGE_TOKEN is not available to this run" >&2
  exit 1
fi
regenerated="$RUNNER_TEMP/regenerated/cargo-sources.json"
if [[ -L flatpak ]]; then
  echo "flatpak is a symlink; refusing to write through it" >&2
  exit 1
fi
if [[ -L flatpak/cargo-sources.json ]]; then
  echo "flatpak/cargo-sources.json is a symlink; refusing to write through it" >&2
  exit 1
fi
cp --remove-destination "$regenerated" flatpak/cargo-sources.json
changes=$(git status --porcelain --untracked-files=all)
if [[ $changes != ' M flatpak/cargo-sources.json' ]]; then
  printf 'expected only flatpak/cargo-sources.json to change, got:\n%s\n' \
    "$changes" >&2
  exit 1
fi
git config user.name 'github-actions[bot]'
git config user.email "$BOT_EMAIL"
git add flatpak/cargo-sources.json
git -c core.hooksPath=/dev/null commit --message "$COMMIT_SUBJECT"
{ set +x; } 2>/dev/null
header=$(printf 'x-access-token:%s' "$PUSH_TOKEN" | base64 --wrap=0)
echo "::add-mask::$header"
GIT_CONFIG_COUNT=1 \
  GIT_CONFIG_KEY_0='http.https://github.com/.extraheader' \
  GIT_CONFIG_VALUE_0="AUTHORIZATION: basic $header" \
  git -c core.hooksPath=/dev/null push \
  "https://github.com/${GITHUB_REPOSITORY}.git" \
  "HEAD:refs/heads/$BRANCH"
''',
}
assert [step["name"] for step in push_steps] == list(PUSH_BODIES), (
    "the push job's steps must be exactly: " + ", ".join(PUSH_BODIES)
)
for step in push_steps:
    assert uncommented(step["run"]) == squash_ws(PUSH_BODIES[step["name"]]), (
        f"the push job's step '{step['name']}' must run exactly the pinned commands, got: {uncommented(step['run'])}"
    )
PY

# --- Run the real push-job steps, extracted from the YAML, in a throwaway repo. ---
# The text pins above say what the steps contain; this says what they do. Only
# `git push` is stubbed. The same bot identity must come out of the commit step
# that the loop guard recognises, or every push would start another run.
sandbox=$(mktemp -d "${TMPDIR:-/tmp}/flatpak-sources-push.XXXXXX")
trap 'rm -rf "$sandbox"' EXIT
mkdir -p "$sandbox/bin" "$sandbox/home"
real_git=$(command -v git)

python3 - "$workflow" "$sandbox" <<'PY' || fail "cannot extract the push job's steps from $workflow"
import pathlib
import shlex
import sys

import yaml

with open(sys.argv[1], encoding="utf-8") as stream:
    push = yaml.safe_load(stream)["jobs"]["push"]
out = pathlib.Path(sys.argv[2])
steps = {step["name"]: step for step in push["steps"]}
(out / "guard.sh").write_text(steps["Refuse to chase a regeneration that does not settle"]["run"])
(out / "token.sh").write_text(push["steps"][-1]["run"])
(out / "job.env").write_text(
    "".join(f"export {key}={shlex.quote(str(value))}\n" for key, value in push["env"].items())
)
PY

# A git that behaves as git does, except that a push is recorded and not sent.
cat > "$sandbox/bin/git" <<STUB
#!/usr/bin/env bash
for argument in "\$@"; do
    if [[ \$argument == push ]]; then
        { printf 'ARG %s\n' "\$@"; env | sed -n '/^GIT_CONFIG_/s/^/ENV /p'; } > "$sandbox/push.log"
        exit 0
    fi
done
exec "$real_git" "\$@"
STUB
chmod +x "$sandbox/bin/git"

fake_token=ghp_FakeTokenForTheContractTest0123456789
fake_header=$(printf 'x-access-token:%s' "$fake_token" | base64 --wrap=0)
# The file is written by the extraction above, so there is nothing to follow.
# shellcheck disable=SC1091
source "$sandbox/job.env"

# new_checkout: a checkout of a bump, as the push job leaves it before the token
# step, plus the artifact the earlier steps validated.
new_checkout() {
    rm -rf "$sandbox/ws" "$sandbox/temp" "$sandbox/outside" "$sandbox/push.log"
    mkdir -p "$sandbox/ws/flatpak" "$sandbox/temp/regenerated" "$sandbox/outside"
    printf 'previous sources\n' > "$sandbox/ws/flatpak/cargo-sources.json"
    printf 'regenerated sources\n' > "$sandbox/temp/regenerated/cargo-sources.json"
    printf 'untouched\n' > "$sandbox/outside/sentinel"
    (
        cd "$sandbox/ws"
        "$real_git" init --quiet .
        "$real_git" add -A
        "$real_git" -c user.name=Dependabot -c user.email=support@github.com commit --quiet --message 'Bump a crate'
    )
}

# commit_all MESSAGE: commits the checkout as a human would.
commit_all() {
    "$real_git" -C "$sandbox/ws" add -A
    "$real_git" -C "$sandbox/ws" -c user.name=Someone -c user.email=someone@example.com \
        commit --quiet --allow-empty --message "$1"
}

# run_script SCRIPT [BASH OPTION] [TOKEN]: runs a step the way Actions does, with
# the job's environment and nothing inherited from this machine.
run_script() {
    local script=$1 option=${2:--e} token=${3-$fake_token}
    (
        cd "$sandbox/ws"
        env -i PATH="$sandbox/bin:$PATH" HOME="$sandbox/home" GIT_CONFIG_NOSYSTEM=1 \
            BOT_EMAIL="$BOT_EMAIL" COMMIT_SUBJECT="$COMMIT_SUBJECT" \
            RUNNER_TEMP="$sandbox/temp" GITHUB_WORKSPACE="$sandbox/ws" \
            GITHUB_REPOSITORY=marvinbaudach/reprise PUSH_TOKEN="$token" \
            BRANCH=dependabot/cargo/a-crate-1.2.3 \
            bash --noprofile --norc "$option" "$sandbox/$script"
    ) > "$sandbox/stdout" 2> "$sandbox/stderr"
}

tip() { "$real_git" -C "$sandbox/ws" log -1 --format="$1"; }

# 1. The commit step makes what the guard recognises, and the push is what it should be.
new_checkout
run_script token.sh || fail "the token step failed on a clean checkout: $(tail -n 3 "$sandbox/stderr")"
[[ $(tip %s) == 'The Flatpak Cargo sources follow Cargo.lock' ]] || \
    fail "the commit step must make the regeneration commit, got '$(tip %s)'"
[[ $(tip %ae) == '41898282+github-actions[bot]@users.noreply.github.com' ]] || \
    fail "the commit must be authored by the bot, got '$(tip %ae)'"
[[ $(cat "$sandbox/ws/flatpak/cargo-sources.json") == 'regenerated sources' ]] || \
    fail "the step must install the regenerated file"
[[ -f $sandbox/push.log ]] || fail "the step must push"
rg --fixed-strings --line-regexp --quiet 'ARG HEAD:refs/heads/dependabot/cargo/a-crate-1.2.3' "$sandbox/push.log" || \
    fail "the push must send HEAD to the bump's branch"
rg --fixed-strings --line-regexp --quiet 'ARG https://github.com/marvinbaudach/reprise.git' "$sandbox/push.log" || \
    fail "the push URL must carry no credential"
rg --fixed-strings --quiet "ENV GIT_CONFIG_VALUE_0=AUTHORIZATION: basic $fake_header" "$sandbox/push.log" || \
    fail "the token must reach git as a basic-auth header in its environment"
if rg --fixed-strings --quiet "$fake_token" "$sandbox/push.log"; then
    fail "the token must not appear in the push's arguments or environment, only its base64 header"
fi
if run_script guard.sh; then
    fail "the loop guard must refuse to push on top of the commit the push step just made"
fi
rg --fixed-strings --quiet 'not pushing again' "$sandbox/stderr" || \
    fail "the loop guard must refuse for its own reason, got: $(tail -n 3 "$sandbox/stderr")"
# The controls: a tip that is not the bot's commit passes the guard, and so does
# one that has the bot's address but another subject.
new_checkout
run_script guard.sh || fail "the loop guard must not stop an ordinary bump: $(tail -n 3 "$sandbox/stderr")"
"$real_git" -C "$sandbox/ws" -c user.name=x -c "user.email=$BOT_EMAIL" \
    commit --quiet --allow-empty --message 'Some other subject'
run_script guard.sh || fail "the loop guard must match the subject as well as the author"

# 2. A symlink in the bump's checkout must not lead the copy out of it.
new_checkout
rm "$sandbox/ws/flatpak/cargo-sources.json"
ln -s "$sandbox/outside/sentinel" "$sandbox/ws/flatpak/cargo-sources.json"
commit_all 'Link the file'
if run_script token.sh; then fail "the step must refuse a symlink at flatpak/cargo-sources.json"; fi
[[ $(cat "$sandbox/outside/sentinel") == untouched ]] || fail "the copy followed a symlink out of the checkout"
[[ ! -e $sandbox/push.log ]] || fail "nothing may be pushed after a symlink was found"
new_checkout
rm -r "$sandbox/ws/flatpak"
ln -s "$sandbox/outside" "$sandbox/ws/flatpak"
printf 'outside\n' > "$sandbox/outside/cargo-sources.json"
commit_all 'Link the directory'
if run_script token.sh; then fail "the step must refuse a symlink at flatpak"; fi
[[ $(cat "$sandbox/outside/cargo-sources.json") == outside ]] || \
    fail "the copy followed a symlinked directory out of the checkout"

# 3. Anything else in the checkout stops the step before it commits.
new_checkout
printf 'stray\n' > "$sandbox/ws/stray.txt"
head_before=$(tip %H)
if run_script token.sh; then fail "the step must refuse to commit when another path differs"; fi
[[ $(tip %H) == "$head_before" && ! -e $sandbox/push.log ]] || \
    fail "nothing may be committed or pushed after a stray path was found"

# 4. No token, no push; and tracing never prints the header.
new_checkout
if run_script token.sh -e ''; then fail "the step must fail closed without a token"; fi
[[ ! -e $sandbox/push.log ]] || fail "nothing may be pushed without a token"
new_checkout
run_script token.sh -ex || fail "the token step failed under bash -x: $(tail -n 3 "$sandbox/stderr")"
if rg --fixed-strings --quiet "$fake_header" "$sandbox/stderr"; then
    fail "a trace of the token step printed the unmasked base64 header"
fi
[[ $(rg --fixed-strings --count "$fake_header" "$sandbox/stdout") == 1 ]] || \
    fail "the header must reach the log exactly once"
rg --fixed-strings --quiet "::add-mask::$fake_header" "$sandbox/stdout" || \
    fail "the header must reach the log only inside the add-mask command"

# A push by the token's owner makes that owner the event's actor. If routing
# read the actor, the re-triggered run would treat the bump as a human pull
# request, skip every suite, and let auto-merge arm on untested code.
for file in "$ci_workflow" "$cross_target"; do
    rg --fixed-strings --quiet \
        'ACTOR: ${{ github.event.pull_request.user.login || github.actor }}' "$file" || \
        fail "$(basename "$file") must route on the pull request's author, falling back to the actor for pushes"
done

echo "Dependabot Flatpak sources contracts passed"
