#!/usr/bin/env bash
# Rust Core and Android independently own their storage boundary, so the filenames are
# duplicated; this parity gate is the mechanism that keeps the protocol names aligned.
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
            'pub const REPORT_FILE_NAME: &str = "...";',
        ),
        (
            kotlin_file,
            "LISTEN_REPORT_FILE_NAME",
            r'^\s*internal\s+const\s+val\s+LISTEN_REPORT_FILE_NAME\s*=\s*"([^"]+)"',
            'internal const val LISTEN_REPORT_FILE_NAME = "..."',
        ),
    ),
    "acknowledgement": (
        (
            rust_file,
            "ACKNOWLEDGEMENT_FILE_NAME",
            r'^\s*pub\s+const\s+ACKNOWLEDGEMENT_FILE_NAME\s*:\s*&str\s*=\s*"([^"]+)"\s*;',
            'pub const ACKNOWLEDGEMENT_FILE_NAME: &str = "...";',
        ),
        (
            kotlin_file,
            "LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME",
            r'^\s*internal\s+const\s+val\s+LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME\s*=\s*"([^"]+)"',
            'internal const val LISTEN_REPORT_ACKNOWLEDGEMENT_FILE_NAME = "..."',
        ),
    ),
}


def extract(path, constant, pattern, expected):
    matches = re.findall(pattern, Path(path).read_text(encoding="utf-8"), re.MULTILINE)
    if not matches:
        print(
            f"listen-report parity: {path}: missing constant {constant} "
            f"(expected {expected}); "
            "change both sides together",
            file=sys.stderr,
        )
        raise SystemExit(1)
    if len(matches) != 1:
        print(
            f"listen-report parity: {path}: expected exactly one {constant}, "
            f"found {len(matches)}; change both sides together",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return matches[0]


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
