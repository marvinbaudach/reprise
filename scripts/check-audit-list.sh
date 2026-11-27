#!/usr/bin/env bash
# Fail when the accepted `cargo audit` advisories of .cargo/audit.toml (the list
# the audit gate runs with) and the ones the prose names differ, in either
# direction. The prose is the "Accepted `cargo audit` advisories" section of
# AGENTS.md and the "accepted audit warnings" paragraph of RELEASING.md; both may
# write a run of ids as `RUSTSEC-2026-0574` through `RUSTSEC-2026-0582`.
#
# Usage: check-audit-list.sh [accepted-list.toml [AGENTS.md [RELEASING.md]]]
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ $# -gt 3 ]]; then
  printf 'Usage: %s [accepted-list.toml [AGENTS.md [RELEASING.md]]]\n' "$0" >&2
  exit 2
fi

python3 -I - "${1:-.cargo/audit.toml}" "${2:-AGENTS.md}" "${3:-RELEASING.md}" <<'PY'
import re
import sys
import tomllib

list_path, agents_path, releasing_path = sys.argv[1:4]


def fail(message: str) -> None:
    sys.exit(f"check-audit-list.sh: {message}")


def read(path: str) -> str:
    try:
        with open(path, encoding="utf-8") as stream:
            return stream.read()
    except OSError as error:
        fail(f"cannot read {path}: {error}")


try:
    with open(list_path, "rb") as stream:
        listed = tomllib.load(stream)["advisories"]["ignore"]
except (OSError, KeyError, tomllib.TOMLDecodeError) as error:
    fail(f"cannot read the accepted list {list_path}: {error}")
if not listed:
    fail(f"{list_path} accepts no advisory")

id_pattern = re.compile(r"RUSTSEC-(\d{4})-(\d{4})(?:\s+through\s+RUSTSEC-\1-(\d{4}))?")


def named_in(path: str, start: str, end: str) -> set:
    text = re.sub(r"\s+", " ", read(path).replace("`", ""))
    begin = text.find(start)
    if begin < 0:
        fail(f"{path} has no section starting {start!r}")
    finish = text.find(end, begin)
    if finish < 0:
        fail(f"{path} has no {end!r} after {start!r}")
    ids = set()
    for year, first, last in id_pattern.findall(text[begin:finish]):
        for number in range(int(first), int(last or first) + 1):
            ids.add(f"RUSTSEC-{year}-{number:04d}")
    return ids


expected = set(listed)
if len(expected) != len(listed):
    fail(f"{list_path} names an advisory twice")

problems = []
for path, start, end in (
    (agents_path, "Accepted cargo audit advisories", "Any advisory not on this list"),
    (releasing_path, "The accepted audit warnings", "Any additional advisory"),
):
    named = named_in(path, start, end)
    for label, ids in (
        (f"in {list_path} but not in {path}", expected - named),
        (f"in {path} but not in {list_path}", named - expected),
    ):
        if ids:
            problems.append(f"{', '.join(sorted(ids))} {label}")

if problems:
    fail("the accepted advisory lists differ:\n  " + "\n  ".join(problems))
print(f"Accepted advisory lists agree: {len(expected)} ids in {list_path}, {agents_path}, {releasing_path}")
PY
