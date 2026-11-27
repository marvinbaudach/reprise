#!/usr/bin/env bash
# Proves the merge gate checks flatpak/cargo-sources.json against the real
# Cargo.lock: the gate names the script with no arguments, so it reads the real
# files; those files agree today; and a lockfile that moved without regenerated
# sources fails. The stale case is #1388's: a locked version the sources do not
# carry. The failure case asserts a non-zero exit AND the message that names the
# problem.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

die() {
  echo "flatpak-cargo-sources-gate test failed: $1" >&2
  exit 1
}

gate_script=scripts/check-merge-
gate_script=${gate_script}readiness.sh
# Without this line the checks below would prove the script works while the
# gate never calls it, which is exactly how #1388 got through.
grep -Eq '^gate "Flatpak cargo sources" -- scripts/check-flatpak-cargo-sources\.sh$' "$gate_script" ||
  die "$gate_script must run scripts/check-flatpak-cargo-sources.sh, with no arguments, as its own gate"

scripts/check-flatpak-cargo-sources.sh >"$scratch/real.log" 2>&1 ||
  die "the real Cargo.lock and flatpak/cargo-sources.json disagree: $(cat "$scratch/real.log")"

# The lock of a checksummed package moves to a version the sources never saw.
stale_lock=$scratch/Cargo.lock
awk '
  /^name = "url"$/ { in_url = 1 }
  in_url && /^version = / { print "version = \"0.0.0-stale\""; in_url = 0; moved = 1; next }
  { print }
  END { exit moved ? 0 : 1 }
' Cargo.lock >"$stale_lock" || die "Cargo.lock has no url package to make stale"

status=0
scripts/check-flatpak-cargo-sources.sh "$stale_lock" flatpak/cargo-sources.json \
  >"$scratch/stale.log" 2>&1 || status=$?
[[ $status -ne 0 ]] || die "a lockfile change without regenerated sources must fail"
grep -q 'check-flatpak-cargo-sources.sh:' "$scratch/stale.log" ||
  die "the stale-lock failure did not come from the sources check: $(cat "$scratch/stale.log")"

echo "Flatpak cargo sources gate self-test passed"
