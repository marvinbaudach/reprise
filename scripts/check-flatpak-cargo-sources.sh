#!/usr/bin/env bash
# Verify that every checksummed Cargo.lock package has a current Flatpak
# archive source identity and that no stale identity remains vendored.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

lock_file=${1:-Cargo.lock}
sources_file=${2:-flatpak/cargo-sources.json}
if [[ $# -gt 2 ]]; then
  printf 'Usage: %s [Cargo.lock [flatpak/cargo-sources.json]]\n' "$0" >&2
  exit 2
fi

python3 - "$lock_file" "$sources_file" <<'PY'
import json
import sys
import tomllib
from pathlib import Path

lock_path = Path(sys.argv[1])
sources_path = Path(sys.argv[2])


def fail(message: str) -> None:
    sys.exit(f"check-flatpak-cargo-sources.sh: {message}")


try:
    with lock_path.open("rb") as lock_stream:
        lock_data = tomllib.load(lock_stream)
except (OSError, tomllib.TOMLDecodeError) as error:
    fail(f"cannot read {lock_path}: {error}")

try:
    with sources_path.open(encoding="utf-8") as sources_stream:
        sources_data = json.load(sources_stream)
except (OSError, json.JSONDecodeError) as error:
    fail(f"cannot read {sources_path}: {error}")

if not isinstance(sources_data, list):
    fail(f"{sources_path} must contain a JSON array")

lock_packages = set()
for package in lock_data.get("package", []):
    if "checksum" not in package:
        continue
    name = package.get("name")
    version = package.get("version")
    if not isinstance(name, str) or not isinstance(version, str):
        fail(f"{lock_path} has a checksummed package without a name and version")
    lock_packages.add(f"{name}-{version}")

vendor_prefix = "cargo/vendor/"
vendored_packages = set()
for source in sources_data:
    if not isinstance(source, dict) or source.get("type") != "archive":
        continue
    destination = source.get("dest")
    if not isinstance(destination, str) or not destination.startswith(vendor_prefix):
        fail(f"{sources_path} has an archive without a {vendor_prefix}<name>-<version> dest")
    package = destination.removeprefix(vendor_prefix)
    if not package or "/" in package:
        fail(f"{sources_path} has an invalid archive dest: {destination}")
    vendored_packages.add(package)

missing = sorted(lock_packages - vendored_packages)
orphaned = sorted(vendored_packages - lock_packages)
if missing or orphaned:
    print(
        f"check-flatpak-cargo-sources.sh: {sources_path} does not match "
        f"{lock_path}",
        file=sys.stderr,
    )
    print("Missing from Flatpak Cargo sources:", file=sys.stderr)
    if missing:
        for package in missing:
            print(f"  - {package}", file=sys.stderr)
    else:
        print("  (none)", file=sys.stderr)
    print("Orphaned in Flatpak Cargo sources:", file=sys.stderr)
    if orphaned:
        for package in orphaned:
            print(f"  - {package}", file=sys.stderr)
    else:
        print("  (none)", file=sys.stderr)
    print(
        "Regenerate with: flatpak-cargo-generator.py Cargo.lock "
        "-o flatpak/cargo-sources.json",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    f"Flatpak Cargo sources match {lock_path}: "
    f"{len(lock_packages)} checksummed packages"
)
PY
