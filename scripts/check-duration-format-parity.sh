#!/usr/bin/env bash
# Holds the two duration formatters to one contract.
#
# `reprise_core::format::format_duration` and Kotlin's `formatDuration` encode
# the same rule — `m:ss`, or `h:mm:ss` past the hour. The Kotlin copy exists on
# purpose: formatting a list row through the FFI would cost a JNI crossing per
# row per frame. Two copies of one decision is the shape this project has been
# bitten by before, and a doc comment asking the next reader to keep them equal
# is a plea, not a mechanism. This script is the mechanism.
#
# The rule it enforces is one-directional: every case the Rust tests assert
# must also be asserted on the Kotlin side with the same expected string. The
# Kotlin test may assert more (it covers the hour boundary and long album
# totals, which Rust does not) — extra coverage is never a failure. Only a case
# Rust pins and Kotlin does not, or pins differently, fails here.
#
# Drift caught: before this gate, Kotlin had no hour branch at all, so a
# 1:02:33 episode read "62:33" and a 74-minute album read "74:00".
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

rust_file=crates/reprise-core/src/format.rs
kotlin_file=android/app/src/test/java/io/github/marvinbaudach/reprise/DurationFormatTest.kt

for f in "$rust_file" "$kotlin_file"; do
  [ -f "$f" ] || { echo "duration-format parity: missing $f" >&2; exit 1; }
done

python3 - "$rust_file" "$kotlin_file" <<'PY'
import re
import sys

rust_path, kotlin_path = sys.argv[1], sys.argv[2]


def value(expr: str) -> int:
    """Evaluate an integer literal or a product of them: `74 * 60 * 1_000L`."""
    cleaned = expr.replace("_", "").replace("L", "").strip()
    if not re.fullmatch(r"-?\d+(\s*\*\s*-?\d+)*", cleaned):
        raise ValueError(f"unsupported millisecond expression: {expr!r}")
    product = 1
    for part in cleaned.split("*"):
        product *= int(part)
    return product


rust = dict()
for ms, expected in re.findall(
    r"assert_eq!\(\s*format_duration\(([^)]*)\)\s*,\s*\"([^\"]*)\"\s*\)",
    open(rust_path, encoding="utf-8").read(),
):
    rust[value(ms)] = expected

kotlin = dict()
for expected, ms in re.findall(
    r"assertEquals\(\s*\"([^\"]*)\"\s*,\s*formatDuration\(([^)]*)\)\s*\)",
    open(kotlin_path, encoding="utf-8").read(),
):
    kotlin[value(ms)] = expected

if not rust:
    print("duration-format parity: no assertions found in the Rust tests", file=sys.stderr)
    print(f"  the extractor expects `assert_eq!(format_duration(<ms>), \"<text>\")` in {rust_path}", file=sys.stderr)
    raise SystemExit(1)

problems = []
for ms, expected in sorted(rust.items()):
    if ms not in kotlin:
        problems.append(f"  {ms} ms → \"{expected}\" is asserted in Rust but nowhere on the Kotlin side")
    elif kotlin[ms] != expected:
        problems.append(
            f"  {ms} ms → Rust says \"{expected}\", Kotlin says \"{kotlin[ms]}\""
        )

if problems:
    print("duration-format parity: the two formatters disagree", file=sys.stderr)
    print("\n".join(problems), file=sys.stderr)
    print(f"  add the missing case to {kotlin_path}, or change both contracts together", file=sys.stderr)
    raise SystemExit(1)

extra = len(kotlin) - len(rust)
print(f"  duration format: {len(rust)} Rust cases all pinned in Kotlin ({extra} extra Kotlin cases)")
PY
