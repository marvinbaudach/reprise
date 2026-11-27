#!/usr/bin/env bash
# Proves the dependency-audit gate (scripts/check-dependency-audit.sh) and the
# accepted-list cross-check (scripts/check-audit-list.sh) fail where they must:
#
#   * an advisory that is not on the accepted list fails the gate, also through
#     the cached-database fallback, which uses the same list and flags;
#   * with no advisory database at all the gate fails with a message;
#   * an accepted id that matches nothing is reported, and does not fail the gate;
#   * a developer's global audit.toml cannot widen the accepted list;
#   * the ids of .cargo/audit.toml, AGENTS.md and RELEASING.md must be one set.
#
# The gate cases drive a stub `cargo` that behaves like `cargo audit` for the
# one thing that matters here: it exits 0 for warnings unless --deny warnings is
# passed, and it honours --ignore. A last case runs the real scanner when it and
# a cached database are available, and skips with a notice otherwise.
# Every failure case asserts a non-zero exit AND the message that names it.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

die() {
  echo "audit-gate test failed: $1" >&2
  shift
  for line in "$@"; do
    echo "$line" >&2
  done
  exit 1
}

list=.cargo/audit.toml
mapfile -t accepted < <(python3 -I -c '
import sys, tomllib
print("\n".join(tomllib.load(open(sys.argv[1], "rb"))["advisories"]["ignore"]))' "$list")
[[ ${#accepted[@]} -ge 2 ]] || die "$list names fewer than two accepted advisories"
dropped=${accepted[0]}

# ---- check-audit-list.sh ----------------------------------------------------

# expect_list_failure <message fragment> <list> <agents> <releasing>
expect_list_failure() {
  local fragment=$1 status=0 output
  shift
  output=$(scripts/check-audit-list.sh "$@" 2>&1) || status=$?
  [[ $status -ne 0 ]] || die "check-audit-list.sh passed, expected a failure naming: $fragment"
  [[ $output == *"$fragment"* ]] ||
    die "check-audit-list.sh failed without naming: $fragment" "$output"
}

scripts/check-audit-list.sh >"$scratch/real-list.log" 2>&1 ||
  die "the real accepted lists disagree" "$(cat "$scratch/real-list.log")"

# An id the list lacks, and one the prose lacks: both directions must fail.
sed "s/^ignore = \[\$/ignore = [\n  \"RUSTSEC-9999-0001\",/" "$list" >"$scratch/extra.toml"
expect_list_failure 'RUSTSEC-9999-0001 in' "$scratch/extra.toml" AGENTS.md RELEASING.md
grep -v "\"$dropped\"" "$list" >"$scratch/short.toml"
expect_list_failure "$dropped in AGENTS.md but not in" "$scratch/short.toml" AGENTS.md RELEASING.md

# A range in the prose counts for every id it spans: cutting its end short must
# drop the last id from AGENTS.md.
sed 's/through `RUSTSEC-2026-0582`/through `RUSTSEC-2026-0581`/' AGENTS.md >"$scratch/agents.md"
cmp --quiet AGENTS.md "$scratch/agents.md" && die "the range edit changed nothing; the test is stale"
expect_list_failure 'RUSTSEC-2026-0582 in' "$list" "$scratch/agents.md" RELEASING.md

# RELEASING.md is held to the same list.
sed "s/\`$dropped\`/\`RUSTSEC-9999-0002\`/" RELEASING.md >"$scratch/releasing.md"
cmp --quiet RELEASING.md "$scratch/releasing.md" && die "the RELEASING.md edit changed nothing; the test is stale"
expect_list_failure "$dropped in" "$list" AGENTS.md "$scratch/releasing.md"
expect_list_failure 'RUSTSEC-9999-0002 in' "$list" AGENTS.md "$scratch/releasing.md"

# ---- check-dependency-audit.sh, against a stub scanner ------------------------

mkdir -p "$scratch/bin"
cat >"$scratch/bin/cargo" <<'STUB'
#!/usr/bin/env bash
# Stands in for `cargo audit`. STUB_FOUND: ids the "database" matches against the
# lock. STUB_FETCH_FAILS=1: every call without --no-fetch fails, as offline.
set -euo pipefail
[[ ${1:-} == audit ]] || { echo "stub cargo: unexpected $*" >&2; exit 99; }
shift
echo "$*" >>"$STUB_LOG"
json=false deny=false no_fetch=false ignored=()
while (($#)); do
  case $1 in
    --json) json=true ;;
    --no-fetch) no_fetch=true ;;
    --deny) deny=true; [[ ${2:-} == warnings ]] || { echo "stub cargo: --deny $2" >&2; exit 99; } ;;
    --ignore) ignored+=("$2"); shift ;;
    --db | --file) shift ;;
  esac
  shift
done
# Like cargo-audit: the audit.toml of the working directory replaces the global
# one in $CARGO_HOME, whose ignores otherwise apply.
config=.cargo/audit.toml
[[ -f $config ]] || config=${CARGO_HOME:-/nonexistent}/audit.toml
if [[ -f $config ]]; then
  mapfile -t configured < <(grep -o 'RUSTSEC-[0-9]*-[0-9]*' "$config" || true)
  ignored+=("${configured[@]}")
fi
if [[ ${STUB_FETCH_FAILS:-} == 1 && $no_fetch == false ]]; then
  echo "error: couldn't fetch advisory database" >&2
  exit 1
fi
if $json; then
  printf '{"vulnerabilities":{"list":[]},"warnings":{"unsound":['
  sep=''
  for id in ${STUB_FOUND:-}; do printf '%s{"advisory":{"id":"%s"}}' "$sep" "$id"; sep=,; done
  printf ']}}\n'
  exit 0
fi
status=0
for id in ${STUB_FOUND:-}; do
  listed=false
  for ok in "${ignored[@]}"; do [[ $ok == "$id" ]] && listed=true; done
  if ! $listed; then
    echo "Crate: stub  ID: $id (warning)" >&2
    $deny && status=1
  fi
done
exit "$status"
STUB
chmod +x "$scratch/bin/cargo"

mkdir -p "$scratch/cache/advisory-db"   # a cargo home that has a cached database
mkdir -p "$scratch/bare"                # a cargo home that has none

# run_gate <cargo home> <found ids> [extra env...]; sets $status and $output.
run_gate() {
  local cargo_home=$1 found=$2
  shift 2
  : >"$scratch/stub.log"
  status=0
  output=$(env PATH="$scratch/bin:$PATH" CARGO_HOME="$cargo_home" STUB_LOG="$scratch/stub.log" \
    STUB_FOUND="$found" "$@" scripts/check-dependency-audit.sh Cargo.lock "${gate_list:-$list}" 2>&1) || status=$?
}

expect_pass() {
  [[ $status -eq 0 ]] || die "$1: expected the gate to pass" "$output"
}

expect_fail() {
  local case_name=$1 fragment=$2
  [[ $status -ne 0 ]] || die "$case_name: expected the gate to fail" "$output"
  [[ $output == *"$fragment"* ]] || die "$case_name: failed without saying: $fragment" "$output"
}

all_accepted="${accepted[*]}"

run_gate "$scratch/cache" "$all_accepted"
expect_pass "every finding accepted"
grep -q -- '--deny warnings' "$scratch/stub.log" || die "the scan did not pass --deny warnings"
for id in "${accepted[@]}"; do
  grep -q -- "--ignore $id" "$scratch/stub.log" || die "the scan did not ignore the accepted $id"
done

run_gate "$scratch/cache" "$all_accepted RUSTSEC-9999-0001"
expect_fail "an advisory off the list" "RUSTSEC-9999-0001"

# The live refresh fails: the cached database is checked with the same flags and
# says so, and it still rejects what is not on the list.
run_gate "$scratch/cache" "$all_accepted" STUB_FETCH_FAILS=1
expect_pass "offline, nothing new"
[[ $output == *"cached advisory database"* ]] || die "the cached result did not say it was cached" "$output"
fallback=$(grep -- '--no-fetch' "$scratch/stub.log" | grep -v -- '--json' | head -1)
[[ $fallback == *'--deny warnings'* && $fallback == *"--ignore $dropped"* ]] ||
  die "the cached fallback did not use the same list and --deny warnings" "$fallback"

run_gate "$scratch/cache" "$all_accepted RUSTSEC-9999-0001" STUB_FETCH_FAILS=1
expect_fail "an advisory off the list, offline" "RUSTSEC-9999-0001"

run_gate "$scratch/bare" "$all_accepted" STUB_FETCH_FAILS=1
expect_fail "offline with no cached database" "no cached advisory database"

# An accepted id nothing matches is reported, live or cached, and fails neither.
others=${all_accepted#"$dropped"}
for fetch_fails in 0 1; do
  run_gate "$scratch/cache" "$others" STUB_FETCH_FAILS=$fetch_fails
  expect_pass "a stale accepted entry (fetch fails: $fetch_fails)"
  [[ $output == *"NOTICE: accepted advisory no longer matches anything in Cargo.lock: $dropped"* ]] ||
    die "the stale entry was not reported (fetch fails: $fetch_fails)" "$output"
done

# A developer's global audit.toml that accepts what the list does not must not
# widen it: the unlisted advisory still fails the gate.
mkdir -p "$scratch/global"
printf '[advisories]\nignore = ["RUSTSEC-9999-0001"]\n' >"$scratch/global/audit.toml"
ln -s "$scratch/cache/advisory-db" "$scratch/global/advisory-db"
run_gate "$scratch/global" "$all_accepted RUSTSEC-9999-0001"
expect_fail "a global audit.toml widening the list" "RUSTSEC-9999-0001"

gate_list=$scratch/empty.toml
printf '[advisories]\nignore = []\n' >"$gate_list"
run_gate "$scratch/cache" "$all_accepted"
expect_fail "an empty accepted list" "no accepted advisory ids"
unset gate_list

# ---- the real scanner -----------------------------------------------------------

if cargo audit --version >/dev/null 2>&1 && [[ -d ${CARGO_HOME:-$HOME/.cargo}/advisory-db ]]; then
  status=0
  output=$(scripts/check-dependency-audit.sh Cargo.lock "$scratch/short.toml" 2>&1) || status=$?
  [[ $status -ne 0 ]] || die "the real scanner passed with $dropped off the accepted list" "$output"
  [[ $output == *"$dropped"* ]] || die "the real scanner failed without naming $dropped" "$output"
  # The same through the real scanner, which must not honour the global file
  # either. $dropped is a live advisory; a cargo home that accepts it, with the
  # real database and tools linked in, must still fail the list without it.
  real_home=${CARGO_HOME:-$HOME/.cargo}
  mkdir -p "$scratch/real-global"
  printf '[advisories]\nignore = ["%s"]\n' "$dropped" >"$scratch/real-global/audit.toml"
  ln -s "$real_home/advisory-db" "$scratch/real-global/advisory-db"
  ln -s "$real_home/bin" "$scratch/real-global/bin"
  status=0
  output=$(env CARGO_HOME="$scratch/real-global" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    scripts/check-dependency-audit.sh Cargo.lock "$scratch/short.toml" 2>&1) || status=$?
  [[ $status -ne 0 ]] || die "a global audit.toml widened the accepted list for the real scanner" "$output"
  [[ $output == *"$dropped"* ]] || die "the real scanner failed without naming $dropped" "$output"
else
  echo "SKIP: cargo-audit or its cached database is not available; the real-scanner case did not run"
fi

echo "Audit gate self-test passed"
