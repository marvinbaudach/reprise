#!/usr/bin/env bash
# Raise the app version by one patch step, everywhere it is written down.
#
# Called by the pipeline's land.sh right before a branch is merged into dev, so
# every merge ships a version the previous one did not have. Usable by hand too.
#
#   ./scripts/bump-version.sh current            print the workspace version
#   ./scripts/bump-version.sh next [<version>]   print that version with patch+1
#   ./scripts/bump-version.sh set <version>      write <version> everywhere
#   ./scripts/bump-version.sh --base <git-ref>   patch+1 over <ref>, then write
#
# `--base` is the form the landing script uses, and it is the reason this is
# computed against a ref rather than against the working copy: the branch is
# rebased onto origin/dev immediately before the merge, so the only version that
# guarantees monotonicity is dev's own plus one. Deriving it from the branch
# would hand out a number a parallel merge may already have taken.
#
# Never lowers a version. A working copy already ahead of the computed target
# keeps what it has — a hand-made release bump is not something a landing run
# gets to undo.
#
# Writes three places. `Cargo.toml` is the single source; `Cargo.lock` carries
# the same number for every workspace member and a stale one leaves the tree
# dirty on the next build; Android's `versionCode` is an integer of its own that
# nothing derives from the version string, and it has to keep climbing or the
# installer refuses a newer APK over an older one. `versionName` needs no touch,
# it reads the workspace version through `workspacePackageValue("version")`.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

cargo_toml=Cargo.toml
cargo_lock=Cargo.lock
gradle=android/app/build.gradle.kts

die() { printf 'bump-version.sh: %s\n' "$*" >&2; exit 2; }
usage() { sed -n '3,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2; exit 2; }

[ -f "$cargo_toml" ] || die "no Cargo.toml at $root"
[ -f "$gradle" ]     || die "no $gradle — the version lives in more places than this script knows"

mode=${1:-}
[ -n "$mode" ] || usage

# --- reading -------------------------------------------------------------------

# The `version` under [workspace.package], not the first `version =` in the file:
# every dependency entry carries one too.
read_version() {
  python3 - "$1" <<'PY'
import re
import sys
text = open(sys.argv[1], encoding="utf-8").read()
section = re.search(r'^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)', text, re.M | re.S)
if not section:
    sys.exit("no [workspace.package] section")
found = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
if not found:
    sys.exit("no version in [workspace.package]")
print(found.group(1))
PY
}

read_version_code() {
  python3 - "$1" <<'PY'
import re
import sys
text = open(sys.argv[1], encoding="utf-8").read()
found = re.search(r'^\s*versionCode\s*=\s*(\d+)\s*$', text, re.M)
if not found:
    sys.exit("no versionCode line")
print(found.group(1))
PY
}

next_patch() {
  python3 - "$1" <<'PY'
import sys
parts = sys.argv[1].split(".")
if len(parts) != 3 or not all(p.isdigit() for p in parts):
    sys.exit(f"not an x.y.z version: {sys.argv[1]}")
major, minor, patch = (int(p) for p in parts)
print(f"{major}.{minor}.{patch + 1}")
PY
}

# Exits 0 when $1 is strictly greater than $2.
version_gt() {
  python3 - "$1" "$2" <<'PY'
import sys
def parse(text):
    return tuple(int(p) for p in text.split("."))
sys.exit(0 if parse(sys.argv[1]) > parse(sys.argv[2]) else 1)
PY
}

# --- writing -------------------------------------------------------------------

write_everywhere() {
  local target=$1 target_code=$2
  python3 - "$cargo_toml" "$cargo_lock" "$gradle" "$target" "$target_code" <<'PY'
import re
import sys
import pathlib

cargo_toml, cargo_lock, gradle, target, target_code = sys.argv[1:6]

# 1. The source of truth.
path = pathlib.Path(cargo_toml)
text = path.read_text(encoding="utf-8")
section = re.search(r'^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)', text, re.M | re.S)
old = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M).group(1)
start = section.start(1)
body = re.sub(r'^version\s*=\s*"[^"]+"', f'version = "{target}"',
              section.group(1), count=1, flags=re.M)
path.write_text(text[:start] + body + text[section.end(1):], encoding="utf-8")

# 2. Every workspace member that inherits it. Which ones those are is read from
#    the crates rather than hardcoded, so a new crate is covered the day it is
#    added — a hand-kept list here would go stale silently and leave the lock
#    file half-bumped.
members = []
for manifest in sorted(pathlib.Path("crates").glob("*/Cargo.toml")):
    crate = manifest.read_text(encoding="utf-8")
    package = re.search(r'^\[package\]\s*$(.*?)(?=^\[|\Z)', crate, re.M | re.S)
    if not package or "version.workspace = true" not in package.group(1):
        continue
    name = re.search(r'^name\s*=\s*"([^"]+)"', package.group(1), re.M)
    if name:
        members.append(name.group(1))

lock_path = pathlib.Path(cargo_lock)
if lock_path.exists():
    lock = lock_path.read_text(encoding="utf-8")
    def bump_block(match):
        block = match.group(0)
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        if not name or name.group(1) not in members:
            return block
        return re.sub(r'^version = "[^"]+"', f'version = "{target}"', block, count=1, flags=re.M)
    lock = re.sub(r'\[\[package\]\]\n(?:[^\n]*\n)*?(?=\n|\Z)', bump_block, lock)
    lock_path.write_text(lock, encoding="utf-8")
    missing = [m for m in members if f'name = "{m}"' in lock
               and f'name = "{m}"\nversion = "{target}"' not in lock]
    if missing:
        sys.exit(f"Cargo.lock still carries the old version for: {', '.join(missing)}")

# 3. Android's own counter.
gradle_path = pathlib.Path(gradle)
gradle_text = gradle_path.read_text(encoding="utf-8")
gradle_text, count = re.subn(r'^(\s*versionCode\s*=\s*)\d+\s*$',
                             lambda m: f"{m.group(1)}{target_code}",
                             gradle_text, count=1, flags=re.M)
if count != 1:
    sys.exit("versionCode line not found — Android would keep shipping the old code")
gradle_path.write_text(gradle_text, encoding="utf-8")

print(f"{old} -> {target} (versionCode {target_code})", file=sys.stderr)
PY
}

# --- modes ---------------------------------------------------------------------

case "$mode" in
  current)
    read_version "$cargo_toml"
    ;;

  next)
    next_patch "${2:-$(read_version "$cargo_toml")}"
    ;;

  set)
    [ -n "${2:-}" ] || usage
    write_everywhere "$2" "$(( $(read_version_code "$gradle") + 1 ))"
    printf '%s\n' "$2"
    ;;

  --base)
    ref=${2:-}
    [ -n "$ref" ] || usage
    git rev-parse --verify --quiet "$ref^{commit}" >/dev/null \
      || die "no such git ref: $ref"

    # Read the ref through files, not a pipe: the readers hand their program to
    # python3 on stdin, so stdin is already spoken for.
    scratch=$(mktemp -d)
    trap 'rm -rf "$scratch"' EXIT
    git show "$ref:$cargo_toml" > "$scratch/Cargo.toml" \
      || die "could not read $cargo_toml from $ref"
    git show "$ref:$gradle" > "$scratch/build.gradle.kts" \
      || die "could not read $gradle from $ref"

    base_version=$(read_version "$scratch/Cargo.toml")
    base_code=$(read_version_code "$scratch/build.gradle.kts")

    target=$(next_patch "$base_version")
    target_code=$(( base_code + 1 ))

    current=$(read_version "$cargo_toml")
    current_code=$(read_version_code "$gradle")
    # Never walk a version backwards: a release bump made by hand outranks the
    # step this run would have taken.
    if version_gt "$current" "$target"; then
      target=$current
    fi
    [ "$current_code" -gt "$target_code" ] && target_code=$current_code

    if [ "$current" = "$target" ] && [ "$current_code" = "$target_code" ]; then
      printf 'already at %s (versionCode %s)\n' "$target" "$target_code" >&2
      printf '%s\n' "$target"
      exit 0
    fi
    write_everywhere "$target" "$target_code"
    printf '%s\n' "$target"
    ;;

  *)
    usage
    ;;
esac
