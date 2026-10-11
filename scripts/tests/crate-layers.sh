#!/usr/bin/env bash
# Proves scripts/check-crate-layers.sh fails for every rule it states, with the
# message that names the problem, and passes on a clean tree and on the real
# workspace. It drives the script through its test seams (ARCH_LINT_LAYER_TABLE,
# ARCH_LINT_LAYER_MEMBERS, ARCH_LINT_PANIC_ROOT, ARCH_LINT_PANIC_BUDGETS) and
# never through check-architecture.sh, so an
# unrelated red cannot hide the message a case is waiting for. The seams only
# redirect input; the cases below are what show that none of them makes the gate
# blind. Every failure case asserts a non-zero exit AND the expected message.
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

# The real budgets and the attribute come from the real files, so changing a
# budget or rewording the attribute never needs an edit here.
real_budgets=$(sed -n "s/^panic_lint_budgets='\\(.*\\)'\$/\\1/p" "$script")
real_budget_total=0
for entry in $real_budgets; do
  real_budget_total=$((real_budget_total + ${entry#*=}))
done
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

# A three-crate workspace, every crate held to the never-crash lints.
fixture_table=$scratch/table.txt
cat >"$fixture_table" <<'TABLE'
fixture-lib | - | - | - | deny
fixture-cli | fixture-lib | fixture-lib | - | deny
fixture-ui | fixture-lib | fixture-lib | - | deny
TABLE
fixture_members='fixture-lib fixture-cli fixture-ui'

# The budget of fixture-lib; every other fixture crate has none.
fixture_budget=2

# make_fixture <dir> <expect-count> <allow-count>: every deny root carries the
# attribute, and fixture-lib holds the requested number of suppressions.
make_fixture() {
  local dir=$1 expects=$2 allows=$3 index
  mkdir -p "$dir/crates/fixture-lib/src" "$dir/crates/fixture-cli/src/bin" \
    "$dir/crates/fixture-ui/src"
  printf '%s\n\nmod extra;\n' "$attribute" >"$dir/crates/fixture-lib/src/lib.rs"
  printf '%s\n\nfn main() {}\n' "$attribute" >"$dir/crates/fixture-cli/src/main.rs"
  printf '%s\n\nfn main() {}\n' "$attribute" >"$dir/crates/fixture-cli/src/bin/extra.rs"
  printf '%s\n\nfn main() {}\n' "$attribute" >"$dir/crates/fixture-ui/src/main.rs"
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

# make_bare_fixture <dir> <source>: no suppression of its own; the extra.rs of
# fixture-lib holds exactly <source>.
make_bare_fixture() {
  make_fixture "$1" 0 0
  printf '%s\n' "$2" >"$1/crates/fixture-lib/src/extra.rs"
}

# make_root_fixture <dir> <lib.rs text>: the fixture-lib root is the given text.
make_root_fixture() {
  make_fixture "$1" "$fixture_budget" 0
  printf '%s\n\nmod extra;\n' "$2" >"$1/crates/fixture-lib/src/lib.rs"
}

# load_fixture_env <dir> [<fixture-lib budget>]: fills fixture_env_lines.
load_fixture_env() {
  fixture_env_lines=(
    "ARCH_LINT_LAYER_TABLE=$fixture_table"
    "ARCH_LINT_LAYER_MEMBERS=$fixture_members"
    "ARCH_LINT_PANIC_ROOT=$1"
    "ARCH_LINT_PANIC_BUDGETS=fixture-lib=${2:-$fixture_budget}"
  )
}

# --- the green path on a clean fixture ---------------------------------------

clean=$scratch/clean
make_fixture "$clean" "$fixture_budget" 0
load_fixture_env "$clean"
expect_success "clean fixture" "panic-lint suppressions: $fixture_budget (at budget)" "${fixture_env_lines[@]}"
expect_success "clean fixture, every crate deny" "fixture-ui: panic policy deny" "${fixture_env_lines[@]}"

# Comments and string literals never count: a commented-out suppression, a lint
# name in a reason, and a commented-out attribute are all noise, not policy.
noise=$scratch/noise
make_bare_fixture "$noise" '// #[expect(clippy::unwrap_used)]
/* #[allow(clippy::panic)] */
#[expect(dead_code, reason = "avoids clippy::unwrap_used and #[allow(clippy::todo)]")]
fn noise() {}
const NOTE: &str = "#[allow(clippy::unreachable)]";'
load_fixture_env "$noise" 0
expect_success "comments and strings are not suppressions" "panic-lint suppressions: 0 (at budget)" \
  "${fixture_env_lines[@]}"

# --- 1. a member with no row -------------------------------------------------

load_fixture_env "$clean"
expect_failure "member without a row" "workspace crate fixture-rogue has no row in the layer table" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_MEMBERS=$fixture_members fixture-rogue"

# --- 2. a row with no member -------------------------------------------------

expect_failure "row without a member" "layer table row fixture-ui names no workspace crate" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_MEMBERS=fixture-lib fixture-cli"

# --- the table itself: duplicate, malformed, unknown policy ---------------------

duplicate_table=$scratch/duplicate-table.txt
{ cat "$fixture_table"; echo 'fixture-lib | - | - | - | deny'; } >"$duplicate_table"
expect_failure "a duplicate table row" "layer table names fixture-lib twice" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_TABLE=$duplicate_table"

malformed_table=$scratch/malformed-table.txt
{ cat "$fixture_table"; echo 'fixture-rogue | - | - | deny'; } >"$malformed_table"
expect_failure "a malformed table row" \
  "layer table row for fixture-rogue must have five fields ending in deny" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_TABLE=$malformed_table"

unknown_policy_table=$scratch/unknown-policy-table.txt
sed 's/^fixture-lib | - | - | - | deny$/fixture-lib | - | - | - | allow/' "$fixture_table" >"$unknown_policy_table"
expect_failure "an unknown panic policy" \
  "layer table row for fixture-lib must have five fields ending in deny" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_TABLE=$unknown_policy_table"

# `pending` is gone: no row may use it to escape the lints.
pending_table=$scratch/pending-table.txt
sed 's/^fixture-lib | - | - | - | deny$/fixture-lib | - | - | - | pending/' "$fixture_table" >"$pending_table"
expect_failure "a row marked pending" \
  "layer table row for fixture-lib must have five fields ending in deny" \
  "${fixture_env_lines[@]}" "ARCH_LINT_LAYER_TABLE=$pending_table"

expect_failure "a budget for a crate that is not in the table" \
  "panic-lint budget names fixture-ghost, which is not a deny crate in the layer table" \
  "${fixture_env_lines[@]}" "ARCH_LINT_PANIC_BUDGETS=fixture-ghost=1"

# --- 4. a deny crate root without the attribute ------------------------------

missing_lib=$scratch/missing-lib
make_fixture "$missing_lib" "$fixture_budget" 0
printf 'mod extra;\n' >"$missing_lib/crates/fixture-lib/src/lib.rs"
load_fixture_env "$missing_lib"
expect_failure "lib root without the attribute" "fixture-lib/src/lib.rs must carry" "${fixture_env_lines[@]}"

missing_bin=$scratch/missing-bin
make_fixture "$missing_bin" "$fixture_budget" 0
printf 'fn main() {}\n' >"$missing_bin/crates/fixture-cli/src/bin/extra.rs"
load_fixture_env "$missing_bin"
expect_failure "bin root without the attribute" "fixture-cli/src/bin/extra.rs must carry" "${fixture_env_lines[@]}"

# Naming a lint somewhere is not carrying the policy: five of the six lints, or a
# cfg_attr that is not scoped to non-test builds, must not satisfy the check.
six='clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented, clippy::unreachable'

expect_root_failure() {
  local label=$1 text=$2 dir=$scratch/root-$RANDOM
  make_root_fixture "$dir" "$text"
  load_fixture_env "$dir"
  expect_failure "$label" "fixture-lib/src/lib.rs must carry" "${fixture_env_lines[@]}"
}

expect_root_failure "attribute missing one lint" \
  '#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented))]'
expect_root_failure "attribute not scoped to non-test builds" "#![deny($six)]"
expect_root_failure "a longer lint name does not satisfy a shorter one" \
  '#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic_in_result_fn, clippy::todo, clippy::unimplemented, clippy::unreachable))]'
expect_root_failure "a line-commented attribute" "// #![cfg_attr(not(test), deny($six))]"
expect_root_failure "a block-commented attribute" "/* #![cfg_attr(not(test), deny($six))] */"
expect_root_failure "a block-commented lint in the list" \
  '#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, /* clippy::todo, */ clippy::unimplemented, clippy::unreachable))]'
expect_root_failure "a line-commented lint in the list" \
  '#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic,
    // clippy::todo,
    clippy::unimplemented, clippy::unreachable))]'
expect_root_failure "an attribute naming a seventh lint" \
  "#![cfg_attr(not(test), deny($six, clippy::indexing_slicing))]"

# --- 5. suppression counts, per crate ------------------------------------------

grown=$scratch/grown
make_fixture "$grown" "$((fixture_budget + 1))" 0
load_fixture_env "$grown"
expect_failure "one suppression over budget" \
  "fixture-lib: panic-lint suppressions grew from $fixture_budget to $((fixture_budget + 1))" \
  "${fixture_env_lines[@]}"

shrunk=$scratch/shrunk
make_fixture "$shrunk" "$((fixture_budget - 1))" 0
load_fixture_env "$shrunk"
expect_failure "one suppression under budget" \
  "fixture-lib: panic-lint suppressions are down to $((fixture_budget - 1)) (budget still says $fixture_budget)" \
  "${fixture_env_lines[@]}"

# One crate's removal must not pay for another crate's new suppression.
swapped=$scratch/swapped
make_fixture "$swapped" "$((fixture_budget - 1))" 0
printf '%s\n\n#[expect(clippy::panic, reason = "fixture")]\nfn swapped() {}\n' "$attribute" \
  >"$swapped/crates/fixture-cli/src/bin/extra_suppression.rs"
load_fixture_env "$swapped"
run_layers "${fixture_env_lines[@]}"
(( status != 0 )) || die "a removal elsewhere paid for a new suppression" "$output"
grep -qF "fixture-lib: panic-lint suppressions are down to" <<<"$output" \
  || die "the removal in fixture-lib was not reported" "$output"
grep -qF "fixture-cli: panic-lint suppressions grew from 0 to 1" <<<"$output" \
  || die "the new suppression in fixture-cli was not reported" "$output"

# --- 6. an allow, in every form it can take ------------------------------------

expect_allow_failure() {
  local label=$1 text=$2 dir=$scratch/allow-$RANDOM
  make_bare_fixture "$dir" "$text"
  load_fixture_env "$dir" 0
  expect_failure "$label" \
    "write #[expect(…, reason)] instead of #[allow] so a stale suppression surfaces" \
    "${fixture_env_lines[@]}"
}

expect_allow_failure "an allow of a never-crash lint" \
  '#[allow(clippy::unwrap_used, reason = "fixture")] fn f() {}'
expect_allow_failure "an allow listing another lint first" \
  '#[allow(dead_code, clippy::unwrap_used)] fn f() {}'
expect_allow_failure "an allow with a comment inside a multi-line attribute" \
  '#[allow(
    // justified
    clippy::unwrap_used
)]
fn f() {}'
expect_allow_failure "an inner allow" '#![allow(clippy::unreachable)]'
expect_allow_failure "an allow inside cfg_attr" '#[cfg_attr(test, allow(clippy::panic))] fn f() {}'

expect_suppression_counted() {
  local label=$1 text=$2 dir=$scratch/expect-$RANDOM
  make_bare_fixture "$dir" "$text"
  load_fixture_env "$dir" 0
  expect_failure "$label" "fixture-lib: panic-lint suppressions grew from 0 to 1" "${fixture_env_lines[@]}"
}

expect_suppression_counted "an expect listing another lint first" '#[expect(unused, clippy::panic)] fn f() {}'
expect_suppression_counted "an expect with a comment inside a multi-line attribute" \
  '#[expect(
    // justified
    clippy::todo,
    reason = "fixture"
)]
fn f() {}'
expect_suppression_counted "an expect inside cfg_attr" '#[cfg_attr(feature = "x", expect(clippy::unimplemented))] fn f() {}'

expect_restriction_failure() {
  local label=$1 text=$2 dir=$scratch/restriction-$RANDOM
  make_bare_fixture "$dir" "$text"
  load_fixture_env "$dir" 0
  expect_failure "$label" "allow or expect of clippy::restriction found" "${fixture_env_lines[@]}"
}

expect_restriction_failure "an allow of clippy::restriction" '#[allow(clippy::restriction)] fn f() {}'
expect_restriction_failure "an expect of clippy::restriction" \
  '#[expect(clippy::restriction, reason = "fixture")] fn f() {}'
expect_restriction_failure "a restriction allow inside cfg_attr" \
  '#![cfg_attr(test, allow(dead_code, clippy::restriction))]'

# --- the dependency rules, on the real workspace ----------------------------------

# The real table, cut out of the script so these cases stay honest when a row
# changes. Each case edits one row and expects the real `cargo tree` to disagree.
real_table=$scratch/real-table.txt
sed -n '/^# layer_table_begin$/,/^# layer_table_end$/p' "$script" \
  | sed -n "/^layer_table='\$/,/^'\$/p" | sed '1d;$d' >"$real_table"
[[ -s $real_table ]] || die "could not cut the layer table out of $script"
for crate in reprise-view reprise-core reprise-cli reprise-mcp; do
  grep -q "^$crate |" "$real_table" || die "the layer table has no $crate row"
done

# A stray direct edge: reprise-view's permitted edges emptied, so reprise-core is stray.
stray_table=$scratch/stray-table.txt
sed 's/^reprise-view | [^|]* | [^|]* |/reprise-view | - | - |/' "$real_table" >"$stray_table"
expect_failure "a stray direct edge" \
  "reprise-view (default build) must not depend directly on reprise-core" \
  "ARCH_LINT_LAYER_TABLE=$stray_table"

# An edge that only the all-features build has: reprise-cli's stems edge needs a feature.
all_stray_table=$scratch/all-stray-table.txt
sed '/^reprise-cli |/ s/reprise-core reprise-runtime-protocol reprise-stems/reprise-core reprise-runtime-protocol/' \
  "$real_table" >"$all_stray_table"
expect_failure "a stray edge of the all-features build" \
  "reprise-cli (all build) must not depend directly on reprise-stems" \
  "ARCH_LINT_LAYER_TABLE=$all_stray_table"

# A banned family in the default build: reprise-core certainly links serde.
banned_default_table=$scratch/banned-default-table.txt
sed 's/^reprise-core | - | - | [^|]* |/reprise-core | - | - | serde |/' "$real_table" >"$banned_default_table"
expect_failure "a banned family in the default build" \
  "reprise-core (default build) must not depend on: serde" \
  "ARCH_LINT_LAYER_TABLE=$banned_default_table"

# A banned family only the all-features build links: zbus arrives with reprise-mcp's mpris feature.
banned_all_table=$scratch/banned-all-table.txt
sed '/^reprise-mcp |/ s/ all=\(.*\) | deny$/ all=\1 zbus | deny/' "$real_table" >"$banned_all_table"
grep -q '^reprise-mcp |.* all=.* zbus | deny$' "$banned_all_table" || die "could not ban zbus in the reprise-mcp all-features build"
expect_failure "a banned family in the all-features build" \
  "reprise-mcp (all build) must not depend on: zbus" \
  "ARCH_LINT_LAYER_TABLE=$banned_all_table"

# --- the tools the gate depends on ---------------------------------------------

# `cargo metadata` failing must fail the gate, not read as an empty workspace.
fake_cargo=$scratch/fake-cargo
mkdir -p "$fake_cargo"
printf '#!/bin/sh\necho "error: fixture cargo refuses" >&2\nexit 101\n' >"$fake_cargo/cargo"
chmod +x "$fake_cargo/cargo"
expect_failure "cargo metadata failing" "cargo metadata failed" "PATH=$fake_cargo:$PATH"

# A PATH holding only the named tools, so one of jq/rg/perl is genuinely missing.
make_tool_path() {
  local dir=$scratch/path-without-$1 tool
  mkdir -p "$dir"
  for tool in bash dirname jq rg perl; do
    [[ $tool == "$1" ]] && continue
    ln -s "$(command -v "$tool")" "$dir/$tool"
  done
  printf '%s' "$dir"
}
expect_failure "jq missing" "jq is required to read the workspace members" "PATH=$(make_tool_path jq)"
expect_failure "rg missing" "rg is required to list the source files" "PATH=$(make_tool_path rg)"
expect_failure "perl missing" "perl is required to read the lint attributes" "PATH=$(make_tool_path perl)"

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
grep -qF "reprise-gnome: panic policy deny" <<<"$output" \
  || die "the real run does not show reprise-gnome under the deny policy" "$output"
grep -qF "panic-lint suppressions: $real_budget_total (at budget)" <<<"$output" \
  || die "the real run is not at the pinned budget of $real_budget_total" "$output"

echo "Crate layer tests passed"
