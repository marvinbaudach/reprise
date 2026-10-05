#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

lock_file="$tmp_root/Cargo.lock"
matching_sources="$tmp_root/cargo-sources-matching.json"
drifted_sources="$tmp_root/cargo-sources-drifted.json"

printf '%s\n' \
  'version = 4' \
  '' \
  '[[package]]' \
  'name = "rusqlite"' \
  'version = "0.40.2"' \
  'source = "registry+https://github.com/rust-lang/crates.io-index"' \
  'checksum = "23f2a97da3e3873c73cb2a2e71b35c40ff95e0b1eefa8d72d8499a6928c3b5b3"' \
  > "$lock_file"

checksum=23f2a97da3e3873c73cb2a2e71b35c40ff95e0b1eefa8d72d8499a6928c3b5b3
jq --null-input --arg checksum "$checksum" '[
  {
    "type": "archive",
    "archive-type": "tar-gzip",
    "url": "https://static.crates.io/crates/rusqlite/rusqlite-0.40.2.crate",
    "sha256": $checksum,
    "dest": "cargo/vendor/rusqlite-0.40.2"
  },
  {
    "type": "inline",
    "contents": ({"package": $checksum, "files": {}} | tojson),
    "dest": "cargo/vendor/rusqlite-0.40.2",
    "dest-filename": ".cargo-checksum.json"
  },
  {
    "type": "inline",
    "contents": "[source.vendored-sources]\ndirectory = \"cargo/vendor\"\n\n[source.crates-io]\nreplace-with = \"vendored-sources\"\n",
    "dest": "cargo",
    "dest-filename": "config"
  }
]' > "$matching_sources"

scripts/check-flatpak-cargo-sources.sh "$lock_file" "$matching_sources"

sed 's/rusqlite-0\.40\.2/rusqlite-0.40.1/' \
  "$matching_sources" > "$drifted_sources"

set +e
failure_output=$(
  scripts/check-flatpak-cargo-sources.sh "$lock_file" "$drifted_sources" 2>&1
)
failure_status=$?
set -e

[[ $failure_status -eq 1 ]] || {
  printf 'expected drifted Cargo sources to fail with exit 1, got %s\n' \
    "$failure_status" >&2
  exit 1
}
grep -Fq 'Missing from Flatpak Cargo sources:' <<< "$failure_output"
grep -Fq 'rusqlite-0.40.2' <<< "$failure_output"
grep -Fq 'Orphaned in Flatpak Cargo sources:' <<< "$failure_output"
grep -Fq 'rusqlite-0.40.1' <<< "$failure_output"
grep -Fq \
  'flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json' \
  <<< "$failure_output"

# The file is produced by third-party code and read by the release build, so
# the check validates content, not just names: each tampered copy must fail.
rejects() {
  local label=$1 filter=$2 expected=$3 tampered output status
  tampered="$tmp_root/tampered.json"
  jq "$filter" "$matching_sources" > "$tampered"
  set +e
  output=$(scripts/check-flatpak-cargo-sources.sh "$lock_file" "$tampered" 2>&1)
  status=$?
  set -e
  [[ $status -eq 1 ]] || {
    printf 'tampered sources (%s) must fail with exit 1, got %s\n' \
      "$label" "$status" >&2
    exit 1
  }
  grep -Fq -- "$expected" <<< "$output" || {
    printf 'tampered sources (%s) failed without %q:\n%s\n' \
      "$label" "$expected" "$output" >&2
    exit 1
  }
}

rejects 'unknown entry type' \
  '. + [{"type": "shell", "commands": ["true"]}]' 'not allowed'
rejects 'extra inline file' \
  '. + [{"type": "inline", "contents": "x", "dest": "cargo", "dest-filename": "build.sh"}]' \
  'not allowed'
rejects 'extra archive key' '.[0] += {"post-install": "true"}' 'keys'
rejects 'extra inline key' '.[1] += {"perms": "0755"}' 'keys'
rejects 'archive checksum differs from Cargo.lock' \
  '.[0].sha256 = ("0" * 64)' 'sha256 does not equal'
rejects 'archive from another host' \
  '.[0].url = "https://example.com/rusqlite-0.40.2.crate"' 'url is'
rejects 'archive of another version' \
  '.[0].url |= sub("0\\.40\\.2\\.crate"; "0.40.1.crate")' 'url is'
rejects 'checksum file names another package' \
  '.[1].contents = ({"package": ("0" * 64), "files": {}} | tojson)' \
  '.cargo-checksum.json contents differ'
rejects 'checksum file lists files' \
  '.[1].contents = ({"package": "'"$checksum"'", "files": {"build.rs": "0"}} | tojson)' \
  '.cargo-checksum.json contents differ'
rejects 'checksum file is not JSON' '.[1].contents = "not json"' 'not JSON'
rejects 'altered Cargo config' \
  '.[2].contents += "[net]\noffline = false\n"' 'not allowed'
rejects 'Cargo config twice' '. + [.[2]]' 'exactly once'
rejects 'Cargo config missing' 'del(.[2])' 'exactly once'
rejects 'duplicate archive' '. + [.[0]]' 'twice'
rejects 'duplicate checksum file' '. + [.[1]]' 'two checksum files'
rejects 'checksum file without archive' 'del(.[0])' 'Missing from Flatpak Cargo sources'

printf 'Flatpak Cargo source contracts passed\n'
