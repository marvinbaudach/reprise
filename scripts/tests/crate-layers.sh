#!/usr/bin/env bash
# Proves scripts/check-crate-layers.sh fails for every rule it states, with the
# message that names the problem, and passes on a clean tree and on the real
# workspace. It drives the script through its three test seams
# (ARCH_LINT_LAYER_TABLE, ARCH_LINT_LAYER_MEMBERS, ARCH_LINT_PANIC_ROOT) and never
# through check-architecture.sh, so an unrelated red cannot hide the message a case
# is waiting for. The seams only redirect input; the cases below are what show
# that none of them makes the gate blind.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
script=$repo_root/scripts/check-crate-layers.sh
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

die() {
  echo "crate-layers test failed: $1" >&2
  shift
  for line in "$@"; do
    echo "$line" >&2
  done
  exit 1
}

# The budget and the attribute come from the real files, so lowering the budget
# or rewording the attribute never needs an edit here.
budget=$(sed -n 's/^panic_lint_budget=\([0-9][0-9]*\)$/\1/p' "$script")
[[ -n $budget ]] || die "scripts/check-crate-layers.sh has no panic_lint_budget=<N> line of its own"
attribute=$(awk '/^#!\[cfg_attr\($/ { printing = 1 } printing { print } printing && /^\)\]$/ { exit }' \
  "$repo_root/crates/reprise-view/src/lib.rs")
[[ -n $attribute ]] || die "crates/reprise-view/src/lib.rs carries no never-crash attribute to copy"

# Runs the script with the given environment; sets $status and $output.
run_layers() {
  status=0
  output=$(env "$@" "$script" 2>&1) || status=$?
}

expect_failure() {
  local label=$1 expected=$2
  shift 2
  run_layers "$@"
  if (( status == 0 )); then
    die "$label: exited 0, expected a failure" "$output"
  fi
  if ! grep -qF -- "$expected" <<<"$output"; then
    die "$label: the failure did not say: $expected" "$output"
  fi
}

expect_success() {
  local label=$1 expected=$2
  shift 2
  run_layers "$@"
  if (( status != 0 )); then
    die "$label: exited $status, expected a pass" "$output"
  fi
  if ! grep -qF -- "$expected" <<<"$output"; then
    die "$label: the output did not say: $expected" "$output"
  fi
}

# A three-crate workspace: two held to the never-crash lints and one pending.
fixture_table=$scratch/table.txt
cat >"$fixture_table" <<'TABLE'
fixture-lib | - | - | - | deny
fixture-cli | fixture-lib | fixture-lib | - | deny
fixture-ui | fixture-lib | fixture-lib | - | pending
TABLE
fixture_members='fixture-lib fixture-cli fixture-ui'

# make_fixture <dir> <expect-count> <allow-count>: every deny root carries the
# attribute, and fixture-lib holds the requested number of suppressions.
make_fixture() {
  local dir=$1 expects=$2 allows=$3 index
  mkdir -p "$dir/crates/fixture-lib/src" "$dir/crates/fixture-cli/src/bin" \
    "$dir/crates/fixture-ui/src"
  printf '%s\n\nmod extra;\n' "$attribute" >"$dir/crates/fixture-lib/src/lib.rs"
  printf '%s\n\nfn main() {}\n' "$attribute" >"$dir/crates/fixture-cli/src/main.rs"
  printf '%s\n\nfn main() {}\n' "$attribute" >"$dir/crates/fixture-cli/src/bin/extra.rs"
  printf 'fn main() {}\n' >"$dir/crates/fixture-ui/src/main.rs"
  : >"$dir/crates/fixture-lib/src/extra.rs"
  for ((index = 0; index < expects; index++)); do
    # Alternate the plain form with one that lists another lint first, which the
    # count must see too.
    if (( index % 2 == 0 )); then
      printf '#[expect(clippy::unwrap_used, reason = "fixture")]\nfn e%d() {}\n' "$index" \
        >>"$dir/crates/fixture-lib/src/extra.rs"
    else
      printf '#[expect(\n    clippy::too_many_arguments,\n    clippy::expect_used,\n    reason = "fixture"\n)]\nfn e%d() {}\n' "$index" \
        >>"$dir/crates/fixture-lib/src/extra.rs"
    fi
  done
  for ((index = 0; index < allows; index++)); do
    printf '#[allow(clippy::unwrap_used, reason = "fixture")]\nfn a%d() {}\n' "$index" \
      >>"$dir/crates/fixture-lib/src/extra.rs"
  done
}

fixture_env() {
  printf '%s\n' \
    "ARCH_LINT_LAYER_TABLE=$fixture_table" \
    "ARCH_LINT_LAYER_MEMBERS=$fixture_members" \
    "ARCH_LINT_PANIC_ROOT=$1"
}

load_fixture_env() {
  fixture_env_lines=()
  while IFS= read -r line; do
    fixture_env_lines+=("$line")
  done < <(fixture_env "$1")
}

# --- the green path on a clean fixture ---------------------------------------

clean=$scratch/clean
make_fixture "$clean" "$budget" 0
load_fixture_env "$clean"
expect_success "clean fixture" "panic-lint suppressions: $budget (at budget)" "${fixture_env_lines[@]}"
expect_success "clean fixture, pending crate" "fixture-ui: panic policy pending" "${fixture_env_lines[@]}"

# --- 1. a member with no row -------------------------------------------------

load_fixture_env "$clean"
expect_failure "member without a row" "workspace crate fixture-rogue has no row in the layer table" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_MEMBERS=$fixture_members fixture-rogue"

# --- 2. a row with no member -------------------------------------------------

expect_failure "row without a member" "layer table row fixture-ui names no workspace crate" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_MEMBERS=fixture-lib fixture-cli"

# --- 4. a deny crate root without the attribute ------------------------------

missing_lib=$scratch/missing-lib
make_fixture "$missing_lib" "$budget" 0
printf 'mod extra;\n' >"$missing_lib/crates/fixture-lib/src/lib.rs"
load_fixture_env "$missing_lib"
expect_failure "lib root without the attribute" "fixture-lib/src/lib.rs must carry" "${fixture_env_lines[@]}"

missing_bin=$scratch/missing-bin
make_fixture "$missing_bin" "$budget" 0
printf 'fn main() {}\n' >"$missing_bin/crates/fixture-cli/src/bin/extra.rs"
load_fixture_env "$missing_bin"
expect_failure "bin root without the attribute" "fixture-cli/src/bin/extra.rs must carry" "${fixture_env_lines[@]}"

# Naming a lint somewhere is not carrying the policy: five of the six lints, or a
# cfg_attr that is not scoped to non-test builds, must not satisfy the check.
partial=$scratch/partial
make_fixture "$partial" "$budget" 0
printf '#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented))]\n\nmod extra;\n' \
  >"$partial/crates/fixture-lib/src/lib.rs"
load_fixture_env "$partial"
expect_failure "attribute missing one lint" "fixture-lib/src/lib.rs must carry" "${fixture_env_lines[@]}"

unscoped=$scratch/unscoped
make_fixture "$unscoped" "$budget" 0
printf '#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented, clippy::unreachable)]\n\nmod extra;\n' \
  >"$unscoped/crates/fixture-lib/src/lib.rs"
load_fixture_env "$unscoped"
expect_failure "attribute not scoped to non-test builds" "fixture-lib/src/lib.rs must carry" "${fixture_env_lines[@]}"

# --- 5. a suppression count of N+1 -------------------------------------------

grown=$scratch/grown
make_fixture "$grown" "$((budget + 1))" 0
load_fixture_env "$grown"
expect_failure "one suppression over budget" \
  "panic-lint suppressions grew from $budget to $((budget + 1))" "${fixture_env_lines[@]}"

# --- 6. a suppression count of N-1 -------------------------------------------

if (( budget > 0 )); then
  shrunk=$scratch/shrunk
  make_fixture "$shrunk" "$((budget - 1))" 0
  load_fixture_env "$shrunk"
  expect_failure "one suppression under budget" \
    "panic-lint suppressions are down to $((budget - 1)) (budget still says $budget)" \
    "${fixture_env_lines[@]}"
else
  echo "panic_lint_budget is 0: nothing can be removed, so the shrink case is skipped"
fi

# --- 7. a single #[allow] of a never-crash lint ------------------------------

allowed=$scratch/allowed
make_fixture "$allowed" 0 1
load_fixture_env "$allowed"
expect_failure "an allow of a never-crash lint" \
  "write #[expect(…, reason)] instead of #[allow] so a stale suppression surfaces" \
  "${fixture_env_lines[@]}"

# --- 3. a stray direct edge, on the real workspace ---------------------------

# The real table with reprise-view's permitted edges emptied: reprise-core is then
# a stray edge. Cutting the table out of the script keeps this case honest when a
# row changes.
real_table=$scratch/real-table.txt
sed -n '/^# layer_table_begin$/,/^# layer_table_end$/p' "$script" \
  | sed -n "/^layer_table='\$/,/^'\$/p" | sed '1d;$d' >"$real_table"
[[ -s $real_table ]] || die "could not cut the layer table out of $script"
grep -q '^reprise-view |' "$real_table" || die "the layer table has no reprise-view row"
stray_table=$scratch/stray-table.txt
sed 's/^reprise-view | [^|]* | [^|]* |/reprise-view | - | - |/' "$real_table" >"$stray_table"
expect_failure "a stray direct edge" \
  "reprise-view (default build) must not depend directly on reprise-core" \
  "ARCH_LINT_LAYER_TABLE=$stray_table"

# --- the green path on the real workspace ------------------------------------

run_layers
if (( status != 0 )); then
  die "the real workspace failed the crate layer check" "$output"
fi
for crate in reprise-runtime-protocol reprise-core reprise-view reprise-stems \
  reprise-android-ffi reprise-cli reprise-mcp reprise-platform-linux reprise-gnome; do
  named=$(grep -c "^  $crate: " <<<"$output" || true)
  if (( named != 1 )); then
    die "the real run names $crate $named times, expected once" "$output"
  fi
done
grep -qF "reprise-gnome: panic policy pending" <<<"$output" \
  || die "the real run does not show reprise-gnome as pending" "$output"
grep -qF "panic-lint suppressions: $budget (at budget)" <<<"$output" \
  || die "the real run is not at the pinned budget of $budget" "$output"

echo "Crate layer tests passed"
