#!/usr/bin/env bash
# Keeps a literal that two sides must agree on from quietly growing a third copy.
#
# Some strings are contracts between components that no compiler checks: a
# filename the desktop writes and the phone looks up over SAF, a D-Bus address a
# client sends to and a server claims. Spell one of them differently on one side
# and nothing fails loudly — the phone simply never finds the file, or the client
# talks to a name nobody owns.
#
# This gate declares, for each such literal, exactly which files may contain it.
# It fails in both directions: a declared site that lost the literal (someone
# renamed one half), and an undeclared site that gained it (someone typed a
# fourth copy instead of importing the third). The second direction is the one
# that matters — it is how the duplication grows back after being consolidated.
#
# Only code is inspected. Line comments and doc comments are stripped first, so
# prose may name an address freely; a literal in a comment is documentation, not
# a second source of truth.
#
# To add a case: add a `literal|file[,file...]` line to the table below and say
# in the commit why those files, and only those, may carry it.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# literal|comma-separated files allowed to contain it
read -r -d '' contracts <<'TABLE' || true
reprise-listens-back.rpl|crates/reprise-core/src/device_sync/listen_report.rs,crates/reprise-core/src/device_sync/mirror_tests.rs,crates/reprise-platform-linux/src/device_sync_tests.rs,android/app/src/main/java/io/github/marvinbaudach/reprise/ListenReportWriter.kt
reprise-listens-back-ack.rpl|crates/reprise-core/src/device_sync/listen_report.rs,crates/reprise-core/src/device_sync/mirror_tests.rs,android/app/src/main/java/io/github/marvinbaudach/reprise/ListenReportWriter.kt
org.mpris.MediaPlayer2.reprise|crates/reprise-runtime-protocol/src/mpris.rs,crates/reprise-mcp/tests/playback_roundtrip.rs,crates/reprise-cli/tests/playback.rs
/org/mpris/MediaPlayer2|crates/reprise-runtime-protocol/src/mpris.rs,crates/reprise-mcp/tests/playback_roundtrip.rs
TABLE

python3 - "$contracts" <<'PY'
import pathlib
import sys

RUST_KOTLIN = {".rs", ".kt"}
ROOTS = ("crates", "android")

def code_of(path: pathlib.Path) -> str:
    """The file with comments removed while preserving quoted string contents."""
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return ""

    code = []
    index = 0
    block_depth = 0
    in_string = False
    escaped = False
    while index < len(text):
        current = text[index]
        following = text[index + 1] if index + 1 < len(text) else ""

        if block_depth:
            if current == "/" and following == "*":
                block_depth += 1
                index += 2
            elif current == "*" and following == "/":
                block_depth -= 1
                index += 2
            else:
                if current == "\n":
                    code.append(current)
                index += 1
            continue

        if in_string:
            code.append(current)
            if escaped:
                escaped = False
            elif current == "\\":
                escaped = True
            elif current == '"':
                in_string = False
            index += 1
            continue

        if current == '"':
            in_string = True
            code.append(current)
            index += 1
        elif current == "/" and following == "/":
            newline = text.find("\n", index + 2)
            if newline == -1:
                break
            code.append("\n")
            index = newline + 1
        elif current == "/" and following == "*":
            block_depth = 1
            index += 2
        else:
            code.append(current)
            index += 1

    return "".join(code)


sources = [
    p
    for root in ROOTS
    for p in pathlib.Path(root).rglob("*")
    if p.suffix in RUST_KOTLIN and p.is_file() and "/build/" not in p.as_posix()
]
code = {p.as_posix(): code_of(p) for p in sources}

problems = []
checked = 0
for line in sys.argv[1].splitlines():
    line = line.strip()
    if not line or line.startswith("#"):
        continue
    literal, declared_csv = line.split("|", 1)
    declared = [d for d in declared_csv.split(",") if d]
    checked += 1

    found = {path for path, body in code.items() if literal in body}

    for site in declared:
        if site not in code:
            problems.append(f'  "{literal}": declared site {site} does not exist')
        elif site not in found:
            problems.append(
                f'  "{literal}": declared site {site} no longer contains it — '
                "renamed on one side only?"
            )
    for extra in sorted(found - set(declared)):
        problems.append(
            f'  "{literal}": a new copy appeared in {extra} — import the shared '
            "definition instead, or declare the site in the table"
        )

if problems:
    print("shared literals: a contract is spelled in the wrong number of places", file=sys.stderr)
    print("\n".join(problems), file=sys.stderr)
    raise SystemExit(1)

print(f"  shared literals: {checked} contracts, each only where declared")
PY
