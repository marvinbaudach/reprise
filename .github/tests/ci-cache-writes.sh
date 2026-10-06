#!/usr/bin/env bash
# Pins who writes CI caches. Pull requests only read them, and only the run that
# the policy below names writes them: dev for ci.yml and cross-target.yml, a push
# for release.yml (which pushes on main alone). A pull request's own entries are
# readable by nothing else, so writing them only evicts what dev reads.
#
# Every explicit save sits directly behind the step that fills its path and hangs
# on that step's outcome, so a failed download or install is never frozen under
# the exact key, a cancelled run saves nothing, and a red test step further down
# cannot skip it (a step after a failed one runs only under its own status
# function, and the saves have none). The one step allowed between a filler and
# its save is named in GAPS below.
#
# Takes workflow files as arguments so a mutated copy can be checked; without
# arguments it checks every workflow under .github/workflows/. The policy is
# looked up by file name.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ $# -gt 0 ]]; then
    workflows=("$@")
else
    shopt -s nullglob
    workflows=("$repo_root"/.github/workflows/*.yml "$repo_root"/.github/workflows/*.yaml)
fi

python3 - "${workflows[@]}" <<'PY' || { printf 'CI cache writes contract failed\n' >&2; exit 1; }
import pathlib
import re
import sys

import yaml

DEV = "github.ref == 'refs/heads/dev'"
PUSH = "github.event_name == 'push'"

# The one condition each workflow's saves may run under. A workflow that is not
# listed must not use actions/cache at all.
WRITERS = {
    "ci.yml": DEV,
    "cross-target.yml": DEV,
    "release.yml": PUSH,
}

# The one stated exception to "only the writer writes": the built-in caches of
# actions/setup-node and actions/setup-java have no save switch, so they write on
# any ref on a key miss, Dependabot pull requests included (npm twice and gradle
# in ci.yml, npm in pages.yml). They are small and accepted; they are pinned here
# so a new one cannot slip in unseen. astral-sh/setup-uv is no exception: its
# default is a cache on every ref, and it does have a switch, checked below.
BUILTIN_CACHES = {
    "ci.yml": ["gradle", "npm", "npm"],
    "pages.yml": ["npm"],
}

# The steps allowed to sit between a save's filling step and the save itself,
# keyed by workflow, job and save. The Flatpak runtimes are verified after the
# install and before they are frozen, so a runtime that failed its check is
# never cached. Nothing else may come between.
GAPS = {
    ("release.yml", "flatpak", "Save cached Flatpak runtimes"): ["Verify restored Flatpak runtimes"],
}

STATUS_FUNCTIONS = re.compile(r"\b(always|cancelled|failure|success)\(\)")


def squash(text) -> str:
    return " ".join(str(text).split())


def block(step) -> tuple[str, str]:
    options = step.get("with", {})
    return squash(options.get("key", "")), squash(options.get("path", ""))


errors = []
for name in sys.argv[1:]:
    path = pathlib.Path(name)
    text = path.read_text(encoding="utf-8")
    workflow = yaml.safe_load(text)
    writer = WRITERS.get(path.name)
    jobs = workflow.get("jobs", {})

    if "actions/cache@" in text:
        errors.append(f"{path.name}: a plain actions/cache step restores and saves on every ref")
    if writer is None and re.search(r"actions/cache/(restore|save)@", text):
        errors.append(f"{path.name}: this workflow has no cache policy; add one to WRITERS before it caches")

    for job_id, job in jobs.items():
        for step in job.get("steps", []):
            if not str(step.get("uses", "")).startswith("astral-sh/setup-uv@"):
                continue
            options = step.get("with", {})
            saves_on = squash(options.get("save-cache", "")).replace("${{ ", "").replace(" }}", "")
            if options.get("enable-cache") is False:
                continue
            if writer is None or saves_on != writer:
                wanted = (f"enable-cache: false or save-cache: ${{{{ {writer} }}}}" if writer
                          else "enable-cache: false, as this workflow has no cache policy")
                errors.append(f"{path.name}:{job_id}: setup-uv saves a cache on every ref by default; "
                              f"it needs {wanted}")

    builtin = sorted(
        str(step["with"]["cache"])
        for job in jobs.values()
        for step in job.get("steps", [])
        if "cache" in step.get("with", {}) and str(step.get("uses", "")).startswith("actions/setup-")
    )
    if builtin != sorted(BUILTIN_CACHES.get(path.name, [])):
        errors.append(f"{path.name}: setup-* built-in caches are pinned as {sorted(BUILTIN_CACHES.get(path.name, []))}, found {builtin}")

    for job_id, job in jobs.items():
        steps = job.get("steps", [])
        where = f"{path.name}:{job_id}"
        restores = [s for s in steps if str(s.get("uses", "")).startswith("actions/cache/restore@")]
        saves = [s for s in steps if str(s.get("uses", "")).startswith("actions/cache/save@")]

        restored = sorted(block(s) for s in restores)
        saved = sorted(block(s) for s in saves)
        if restored != saved:
            errors.append(f"{where}: every cache restore needs a save with the same key and path; "
                          f"restores {restored}, saves {saved}")

        for save in saves if writer else []:
            label = f"{where}: save '{save.get('name')}'"
            condition = squash(save.get("if", ""))
            terms = sorted(term.strip() for term in condition.split("&&"))
            if "||" in condition or STATUS_FUNCTIONS.search(condition):
                errors.append(f"{label} must be one plain conjunction with no status function, got {condition!r}")
            if save.get("continue-on-error"):
                errors.append(f"{label} must not continue on error")

            paired = [r for r in restores if block(r) == block(save)]
            if len(paired) != 1 or not paired[0].get("id"):
                errors.append(f"{label} has no single identified restore with its key and path")
                continue
            restore = paired[0]
            hit = f"steps.{restore['id']}.outputs.cache-hit != 'true'"

            lo, hi = (next(i for i, s in enumerate(steps) if s is x) for x in (restore, save))
            fillers = [
                s for i, s in enumerate(steps)
                if "run" in s and s.get("id") and lo < i < hi
                and f"steps.{s['id']}.outcome == 'success'" in terms
            ]
            if len(fillers) != 1:
                errors.append(f"{label} must hang on the outcome of exactly one run step between its "
                              f"restore and itself, got {condition!r}")
                continue
            filler = fillers[0]
            between = [s.get("name") for s in steps[next(i for i, s in enumerate(steps) if s is filler) + 1:hi]]
            allowed = GAPS.get((path.name, job_id, save.get("name")), [])
            if between != allowed:
                errors.append(f"{label} must sit directly behind its filling step '{filler.get('name')}', "
                              f"so a red step in between cannot skip it; steps between: {between}, allowed: {allowed}")
            if filler.get("continue-on-error"):
                errors.append(f"{label}: the filling step '{filler.get('name')}' must not continue on error")
            expected = sorted([writer, hit, f"steps.{filler['id']}.outcome == 'success'"])
            if terms != expected:
                errors.append(f"{label} must be exactly {expected}, got {terms}")

        for step in steps:
            if not str(step.get("uses", "")).startswith("Swatinem/rust-cache@"):
                continue
            options = step.get("with", {})
            if squash(options.get("save-if", "")).replace("${{ ", "").replace(" }}", "") != writer:
                errors.append(f"{where}: rust-cache save-if must be exactly: {writer}")
            if options.get("cache-on-failure") is not True:
                errors.append(f"{where}: rust-cache must set cache-on-failure: true")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
PY

echo "CI cache writes contract passed"
