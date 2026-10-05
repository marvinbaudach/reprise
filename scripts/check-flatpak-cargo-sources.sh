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

lock_packages = {}
for package in lock_data.get("package", []):
    if "checksum" not in package:
        continue
    name = package.get("name")
    version = package.get("version")
    if not isinstance(name, str) or not isinstance(version, str):
        fail(f"{lock_path} has a checksummed package without a name and version")
    lock_packages[f"{name}-{version}"] = (name, version, package["checksum"])

# The file is built by third-party code from PyPI and consumed by the Flatpak
# release build, so every entry is held to the exact shapes the pinned
# generator emits for a registry-only Cargo.lock; anything else is rejected.
vendor_prefix = "cargo/vendor/"
archive_keys = {"type", "archive-type", "url", "sha256", "dest"}
inline_keys = {"type", "contents", "dest", "dest-filename"}
checksum_filename = ".cargo-checksum.json"
cargo_config_source = {
    "type": "inline",
    "contents": (
        '[source.vendored-sources]\ndirectory = "cargo/vendor"\n\n'
        '[source.crates-io]\nreplace-with = "vendored-sources"\n'
    ),
    "dest": "cargo",
    "dest-filename": "config",
}


def package_of(source: dict, what: str) -> str:
    destination = source.get("dest")
    if not isinstance(destination, str) or not destination.startswith(vendor_prefix):
        fail(f"{sources_path} has {what} without a {vendor_prefix}<name>-<version> dest")
    package = destination.removeprefix(vendor_prefix)
    if not package or "/" in package:
        fail(f"{sources_path} has an invalid {what} dest: {destination}")
    return package


archives = {}
checksum_inlines = {}
config_sources = []
for source in sources_data:
    if not isinstance(source, dict):
        fail(f"{sources_path} has an entry that is not an object")
    kind = source.get("type")
    if kind == "archive":
        if set(source) != archive_keys:
            fail(f"{sources_path} has an archive with keys {sorted(source)}")
        package = package_of(source, "an archive")
        if package in archives:
            fail(f"{sources_path} lists the archive {package} twice")
        archives[package] = source
    elif kind == "inline" and source.get("dest-filename") == checksum_filename:
        if set(source) != inline_keys:
            fail(f"{sources_path} has a checksum file with keys {sorted(source)}")
        package = package_of(source, "a checksum file")
        if package in checksum_inlines:
            fail(f"{sources_path} has two checksum files for {package}")
        checksum_inlines[package] = source
    elif kind == "inline" and source == cargo_config_source:
        config_sources.append(source)
    else:
        fail(f"{sources_path} has an entry that is not allowed: {json.dumps(source)[:200]}")

if len(config_sources) != 1:
    fail(f"{sources_path} must hold the Cargo vendoring config exactly once")

vendored_packages = set(archives)
missing = sorted(set(lock_packages) - vendored_packages)
orphaned = sorted(vendored_packages - set(lock_packages))
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

problems = []
for package, (name, version, checksum) in sorted(lock_packages.items()):
    archive = archives[package]
    expected_url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
    if archive["url"] != expected_url:
        problems.append(f"{package}: url is {archive['url']!r}, expected {expected_url!r}")
    if archive["sha256"] != checksum:
        problems.append(f"{package}: archive sha256 does not equal the Cargo.lock checksum")
    if archive["archive-type"] != "tar-gzip":
        problems.append(f"{package}: archive-type is {archive['archive-type']!r}")
    inline = checksum_inlines.get(package)
    if inline is None:
        problems.append(f"{package}: no {checksum_filename} entry")
        continue
    try:
        contents = json.loads(inline["contents"])
    except (TypeError, json.JSONDecodeError):
        problems.append(f"{package}: {checksum_filename} contents are not JSON")
        continue
    if contents != {"package": checksum, "files": {}}:
        problems.append(f"{package}: {checksum_filename} contents differ from the Cargo.lock checksum")
for package in sorted(set(checksum_inlines) - set(lock_packages)):
    problems.append(f"{package}: {checksum_filename} entry without a Cargo.lock package")
if problems:
    print(
        f"check-flatpak-cargo-sources.sh: {sources_path} has entries that do not "
        f"match {lock_path}",
        file=sys.stderr,
    )
    for problem in problems:
        print(f"  - {problem}", file=sys.stderr)
    sys.exit(1)

print(
    f"Flatpak Cargo sources match {lock_path}: "
    f"{len(lock_packages)} checksummed packages"
)
PY
