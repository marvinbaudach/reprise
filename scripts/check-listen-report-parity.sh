#!/usr/bin/env bash
# Rust Core and Android independently own this storage boundary. For each exact constant
# name, loose declaration counting catches an unrecognised production shape beside a stale
# strict match, and the value check then keeps the recognised declarations aligned.
#
# Deliberately, this does not guess at renamed identifiers: call sites make a real Rust
# rename fail compilation, while fuzzy near-name matching would turn a precise gate into a
# heuristic.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

rust_file=crates/reprise-core/src/device_sync/listen_report.rs
kotlin_file=android/app/src/main/java/io/github/marvinbaudach/reprise/ListenReportWriter.kt

for f in "$rust_file" "$kotlin_file"; do
  [ -f "$f" ] || {
    echo "listen-report parity: missing $f" >&2
    exit 1
  }
done

python3 - "$rust_file" "$kotlin_file" <<'PY'
import re
import sys
from pathlib import Path

rust_file, kotlin_file = sys.argv[1:]

roles = {
    "report": (
        (
            rust_file,
            "REPORT_FILE_NAME",
            r'^\s*pub\s+const\s+REPORT_FILE_NAME\s*:\s*&str\s*=\s*"([^"]+)"\s*;',
            r'^\s*(?:pub(?:\s*\([^\r\n)]*\))?\s+)?(?:const|static(?:\s+mut)?)\s+REPORT_FILE_NAME(?![A-Za-z0-9_])',
            'pub const REPORT_FILE_NAME: &str = "...";',
        ),
        (
            kotlin_file,
            "LISTEN_REPORT_FILE_NAME",
            r'^\s*internal\s+const\s+val\s+LISTEN_REPORT_FILE_NAME\s*=\s*"([^"]+)"',
            r'^\s*(?:(?:internal|private|public|protected)\s+)?const\s+val\s+LISTEN_REPORT_FILE_NAME(?![A-Za-z0-9_])',
            'internal const val LISTEN_REPORT_FILE_NAME = "..."',
        ),
    ),
    "acknowledgement": (
        (
            rust_file,
            "ACKNOWLEDGEMENT_FILE_NAME",
            r'^\s*pub\s+const\s+ACKNOWLEDGEMENT_FILE_NAME\s*:\s*&str\s*=\s*"([^"]+)"\s*;',
            r'^\s*(?:pub(?:\s*\([^\r\n)]*\))?\s+)?(?:const|static(?:\s+mut)?)\s+ACKNOWLEDGEMENT_FILE_NAME(?![A-Za-z0-9_])',
            'pub const ACKNOWLEDGEMENT_FILE_NAME: &str = "...";',
        ),
        (
            kotlin_file,
            "LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME",
            r'^\s*internal\s+const\s+val\s+LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME\s*=\s*"([^"]+)"',
            r'^\s*(?:(?:internal|private|public|protected)\s+)?const\s+val\s+LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME(?![A-Za-z0-9_])',
            'internal const val LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME = "..."',
        ),
    ),
}


def extract(path, constant, strict_pattern, loose_pattern, expected):
    source = Path(path).read_text(encoding="utf-8")
    strict_matches = re.findall(strict_pattern, source, re.MULTILINE)
    loose_count = len(re.findall(loose_pattern, source, re.MULTILINE))
    strict_count = len(strict_matches)
    if loose_count != strict_count:
        print(
            f"listen-report parity: {path}: {constant} declaration count mismatch: "
            f"loose {loose_count}, strict {strict_count}; change both sides together",
            file=sys.stderr,
        )
        raise SystemExit(1)
    if not strict_matches:
        print(
            f"listen-report parity: {path}: missing constant {constant} "
            f"(expected {expected}); "
            "change both sides together",
            file=sys.stderr,
        )
        raise SystemExit(1)
    if strict_count != 1:
        print(
            f"listen-report parity: {path}: expected exactly one {constant}, "
            f"found {strict_count}; change both sides together",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return strict_matches[0]


agreed = {}
for role, (rust, kotlin) in roles.items():
    rust_value = extract(*rust)
    kotlin_value = extract(*kotlin)
    if rust_value != kotlin_value:
        print(
            f'listen-report parity: {role} mismatch: '
            f'{rust[0]} {rust[1]}="{rust_value}"; '
            f'{kotlin[0]} {kotlin[1]}="{kotlin_value}"; '
            "change both sides together",
            file=sys.stderr,
        )
        raise SystemExit(1)
    agreed[role] = rust_value

print(
    f"  listen report parity: {agreed['report']} / {agreed['acknowledgement']}"
)
PY
