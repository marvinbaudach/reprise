#!/usr/bin/env bash
# Pins the CI cache split: pull requests only read caches, dev and the nightly
# run on main write them. An entry written on the default branch is readable
# from every branch, a pull request's own entries are readable by nothing else.
#
# Takes workflow files as arguments so a mutated copy can be checked; without
# arguments it checks ci.yml and cross-target.yml.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ $# -gt 0 ]]; then
    workflows=("$@")
else
    workflows=(
        "$repo_root/.github/workflows/ci.yml"
        "$repo_root/.github/workflows/cross-target.yml"
    )
fi

python3 - "${workflows[@]}" <<'PY' || { printf 'CI cache writes contract failed\n' >&2; exit 1; }
import pathlib
import sys

import yaml

WRITERS = (
    "github.ref == 'refs/heads/dev' || "
    "(github.ref == 'refs/heads/main' && github.event_name == 'schedule')"
)


def squash(text) -> str:
    return " ".join(str(text).split())


errors = []
for name in sys.argv[1:]:
    path = pathlib.Path(name)
    text = path.read_text(encoding="utf-8")
    workflow = yaml.safe_load(text)
    if "actions/cache@" in text:
        errors.append(f"{path.name}: a plain actions/cache step restores and saves on every branch")
    for job_id, job in workflow["jobs"].items():
        steps = job.get("steps", [])
        restores = [s for s in steps if str(s.get("uses", "")).startswith("actions/cache/restore@")]
        saves = [s for s in steps if str(s.get("uses", "")).startswith("actions/cache/save@")]
        where = f"{path.name}:{job_id}"
        restored = sorted((squash(s["with"]["key"]), squash(s["with"]["path"])) for s in restores)
        saved = sorted((squash(s["with"]["key"]), squash(s["with"]["path"])) for s in saves)
        if restored != saved:
            errors.append(f"{where}: every cache restore needs a save with the same key and path; "
                          f"restores {restored}, saves {saved}")
        for save in saves:
            condition = squash(save.get("if", ""))
            if f"({WRITERS}) &&" not in condition:
                errors.append(f"{where}: save '{save.get('name')}' must be guarded by ({WRITERS}), "
                              f"got {condition!r}")
            if "cache-hit != 'true'" not in condition:
                errors.append(f"{where}: save '{save.get('name')}' must skip when the restore hit")
            if "cargo_downloads" in condition and not condition.startswith("always() &&"):
                errors.append(f"{where}: the Cargo downloads save must run on a red run too (always())")
        for step in steps:
            if not str(step.get("uses", "")).startswith("Swatinem/rust-cache@"):
                continue
            options = step.get("with", {})
            if squash(options.get("save-if", "")).replace("${{ ", "").replace(" }}", "") != WRITERS:
                errors.append(f"{where}: rust-cache save-if must be exactly: {WRITERS}")
            if options.get("cache-on-failure") is not True:
                errors.append(f"{where}: rust-cache must set cache-on-failure: true")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
PY

echo "CI cache writes contract passed"
