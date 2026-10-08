#!/usr/bin/env bash
# CI runs every test as root, in a container. A file or directory that a test
# makes unreadable with `chmod 000` or `0o500` stays readable and writable to
# root, so a test built on that passed on a developer machine and failed in CI.
# This reruns the tests that touch file permissions inside a user namespace in
# which the caller is uid 0 (`unshare -r`), which has the same effect for them.
#
# It reruns only the permission-sensitive modules, not the suites: they are
# unit tests, the libraries are already built by the two test gates before it
# (the cargo selections are theirs, plus `--lib`, so nothing compiles here), and
# the gate stays at a few seconds instead of doubling the test time.
#
# The modules cannot be found by running the suites as root, so they are listed
# below, and a file that starts changing permissions without being listed fails
# the gate instead of quietly escaping it. A test that skips itself as root
# passes here without proving anything; prefer an injected failure, as
# scanner_cue_exclusion_tests.rs does.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source scripts/lib/rulebook.sh

if ((EUID == 0)); then
  skip_gate "already running as uid 0, so the ordinary test gates are the root run (this is CI)"
fi
command -v unshare >/dev/null 2>&1 ||
  skip_gate "unshare is not installed, so root cannot be reproduced"
unshare -r true >/dev/null 2>&1 ||
  skip_gate "unshare -r is not permitted here (unprivileged user namespaces are off)"

# `source file` -> test-name prefix of the module that file is compiled into.
# Core and platform tests are filtered by separate cargo invocations below, so
# the prefixes are kept apart by crate.
core_modules=(
  'crates/reprise-core/src/cover_tests.rs=cover::tests::'
  'crates/reprise-core/src/library/scanner_tests.rs=library::scanner::tests::'
  'crates/reprise-core/src/library/scanner_import_errors_tests.rs=library::scanner::import_errors_tests::'
  'crates/reprise-core/src/podcasts/pipeline_tag_tests.rs=podcasts::pipeline::tag_tests::'
  'crates/reprise-core/src/podcasts/ytdlp_process_tests.rs=podcasts::ytdlp::process_tests::'
)
# device_sync_directory_tests.rs is a child module of device_sync_tests.rs.
platform_modules=(
  'crates/reprise-platform-linux/src/device_sync_tests.rs=device_sync::tests::'
  'crates/reprise-platform-linux/src/device_sync_directory_tests.rs=device_sync::tests::'
)

listed_files=()
core_filters=()
platform_filters=()
for entry in "${core_modules[@]}"; do
  listed_files+=("${entry%%=*}")
  core_filters+=("${entry#*=}")
done
for entry in "${platform_modules[@]}"; do
  listed_files+=("${entry%%=*}")
  platform_filters+=("${entry#*=}")
done

unlisted=0
while IFS= read -r file; do
  listed=false
  for known in "${listed_files[@]}"; do
    [[ $file == "$known" ]] && listed=true
  done
  [[ $listed == true ]] && continue
  echo "$file changes file permissions but is not in the root-test list of $0" >&2
  unlisted=1
done < <(git grep -lE 'set_permissions|PermissionsExt|set_readonly' -- 'crates/*.rs')
if ((unlisted != 0)); then
  echo "add its module to core_modules or platform_modules, or reproduce the failure without permissions" >&2
  exit 1
fi

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT
root_env=(env "XDG_DATA_HOME=$tmp_root/data" "XDG_CACHE_HOME=$tmp_root/cache" REPRISE_AUDIO_SINK=fakesink)

# Same selections as the "Workspace tests" and "Linux platform tests" gates.
unshare -r "${root_env[@]}" \
  cargo test --locked --workspace --exclude reprise-platform-linux --lib -- "${core_filters[@]}"
unshare -r "${root_env[@]}" \
  cargo test --locked -p reprise-platform-linux --lib -- --test-threads=1 "${platform_filters[@]}"
