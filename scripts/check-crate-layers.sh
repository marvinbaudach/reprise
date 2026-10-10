#!/usr/bin/env bash
set -euo pipefail

# One table names every workspace crate: which workspace crates it may depend on
# directly, which dependency families may not appear anywhere in its normal
# tree, and whether its production code is held to the never-crash lints. A crate
# the table does not name fails the gate, and so does a row whose crate is gone.
# scripts/check-architecture.sh runs this script; it has no gate line of its own.
#
# Test seams, read only here and each one only redirects input (none skips a
# check), so scripts/tests/crate-layers.sh can prove every failure path:
#   ARCH_LINT_LAYER_TABLE  a file replacing the table below
#   ARCH_LINT_LAYER_MEMBERS  a whitespace-separated list replacing the member list
#   ARCH_LINT_PANIC_ROOT   a fixture tree laid out as crates/<crate>/src/...; the
#                          crate roots then come from its file layout and no
#                          `cargo tree` runs
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# Field 1: crate. Fields 2 and 3: direct workspace edges of the default build and
# of the all-features build (`-` for none). Field 4: dependency families banned
# from the full normal tree on every target, either one list for both builds or
# `default=<list> all=<list>`. Field 5: panic policy, `deny` or `pending`.
#
# `zvariant` in reprise-runtime-protocol is a data format, not `zbus`, and is
# allowed. reprise-platform-linux links glib through GStreamer and zbus for MPRIS,
# so neither is banned there. zbus reaches reprise-cli and reprise-mcp only with
# their `mpris` feature, which is why their all-features build may carry it.
# layer_table_begin
layer_table='
reprise-runtime-protocol | - | - | gtk4 libadwaita glib gstreamer zbus | deny
reprise-core | - | - | gtk4 libadwaita glib gstreamer zbus | deny
reprise-view | reprise-core | reprise-core | gtk4 libadwaita glib gstreamer zbus | deny
reprise-stems | reprise-core | reprise-core | gtk4 libadwaita glib gstreamer zbus | deny
reprise-android-ffi | reprise-core reprise-view | reprise-core reprise-view | gtk4 libadwaita glib gstreamer zbus | deny
reprise-cli | reprise-core | reprise-core reprise-runtime-protocol reprise-stems | default=gtk4 libadwaita glib gstreamer zbus all=gtk4 libadwaita glib gstreamer | deny
reprise-mcp | reprise-core | reprise-core reprise-runtime-protocol | default=gtk4 libadwaita glib gstreamer zbus all=gtk4 libadwaita glib gstreamer | deny
reprise-platform-linux | reprise-core reprise-runtime-protocol | reprise-core reprise-runtime-protocol | gtk4 libadwaita | deny
reprise-gnome | reprise-core reprise-view reprise-platform-linux | reprise-core reprise-view reprise-platform-linux | - | pending
'
# layer_table_end

# The exact number of never-crash lint suppressions (`#[expect(clippy::unwrap_used
# ...)]` and its five siblings) across the `deny` crates. The number is a CEILING
# and a FLOOR: a new suppression fails here, and so does a removed one until the
# budget comes down in the same commit. An `#[allow]` of these lints is never
# accepted, because an allow never goes stale.
panic_lint_budget=1

panic_lints='unwrap_used|expect_used|panic|todo|unimplemented|unreachable'
panic_lint_names=(unwrap_used expect_used panic todo unimplemented unreachable)

banned_dependency_probe_modes=(default all)

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to read the workspace members and crate roots; install it" >&2
  exit 1
fi

failed=0
fail() {
  echo "$1" >&2
  failed=1
}

trim() {
  local value=$1
  value=${value#"${value%%[![:space:]]*}"}
  value=${value%"${value##*[![:space:]]}"}
  printf '%s' "$value"
}

# `-` stands for an empty list in the table.
list_field() {
  local value
  value=$(trim "$1")
  [[ $value == - ]] && value=''
  printf '%s' "$value"
}

banned_families_for() {
  local field=$1 mode=$2 default_part all_part
  if [[ $field == default=* ]]; then
    default_part=${field#default=}
    default_part=${default_part%% all=*}
    all_part=${field#*all=}
    if [[ $mode == default ]]; then
      printf '%s' "$default_part"
    else
      printf '%s' "$all_part"
    fi
  else
    printf '%s' "$field"
  fi
}

# --- the table ---------------------------------------------------------------

if [[ -n ${ARCH_LINT_LAYER_TABLE:-} ]]; then
  if [[ ! -f $ARCH_LINT_LAYER_TABLE ]]; then
    echo "ARCH_LINT_LAYER_TABLE names $ARCH_LINT_LAYER_TABLE, which does not exist" >&2
    exit 1
  fi
  layer_table=$(<"$ARCH_LINT_LAYER_TABLE")
fi

table_crates=()
declare -A row_edges_default row_edges_all row_banned row_policy
while IFS='|' read -r crate edges_default edges_all banned policy extra; do
  crate=$(trim "$crate")
  [[ -z $crate || $crate == '#'* ]] && continue
  policy=$(trim "${policy:-}")
  if [[ -n ${extra:-} || -z $policy || ! $policy =~ ^(deny|pending)$ ]]; then
    fail "layer table row for $crate must have five fields ending in deny or pending"
    continue
  fi
  if [[ -n ${row_policy[$crate]:-} ]]; then
    fail "layer table names $crate twice"
    continue
  fi
  table_crates+=("$crate")
  row_edges_default[$crate]=$(list_field "$edges_default")
  row_edges_all[$crate]=$(list_field "$edges_all")
  row_banned[$crate]=$(trim "$banned")
  row_policy[$crate]=$policy
done <<<"$layer_table"
if (( failed != 0 )); then
  exit 1
fi

# --- the members -------------------------------------------------------------

metadata_json=''
workspace_metadata() {
  if [[ -z $metadata_json ]]; then
    local out
    if ! out=$(cargo metadata --no-deps --format-version 1 2>&1); then
      echo "cargo metadata failed; the workspace members and crate roots cannot be read:" >&2
      echo "$out" >&2
      return 1
    fi
    metadata_json=$out
  fi
  printf '%s\n' "$metadata_json"
}

members=()
if [[ -n ${ARCH_LINT_LAYER_MEMBERS:-} ]]; then
  read -r -a members <<<"$ARCH_LINT_LAYER_MEMBERS"
elif [[ -n ${ARCH_LINT_PANIC_ROOT:-} ]]; then
  while IFS= read -r member; do
    members+=("$member")
  done < <(find "$ARCH_LINT_PANIC_ROOT/crates" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)
else
  workspace_json=$(workspace_metadata) || exit 1
  while IFS= read -r member; do
    members+=("$member")
  done < <(jq -r '.packages[].name' <<<"$workspace_json" | sort)
fi
if (( ${#members[@]} == 0 )); then
  echo "no workspace members were found; the layer table cannot be checked" >&2
  exit 1
fi

echo "== Crate layers =="

for member in "${members[@]}"; do
  if [[ -z ${row_policy[$member]:-} ]]; then
    fail "workspace crate $member has no row in the layer table (scripts/check-crate-layers.sh)"
  fi
done
for crate in "${table_crates[@]}"; do
  found=0
  for member in "${members[@]}"; do
    [[ $member == "$crate" ]] && found=1
  done
  if (( found == 0 )); then
    fail "layer table row $crate names no workspace crate; remove the row"
  fi
done
if (( failed != 0 )); then
  exit 1
fi

# --- direct edges and banned families ----------------------------------------

# Runs one `cargo tree`; a failing run aborts the gate. An empty result is
# indistinguishable from "no violation", so a failure must never read as one.
run_tree() {
  local label=$1
  shift
  local out
  if ! out=$(cargo tree "$@" 2>&1); then
    echo "cargo tree failed for $label; the layers cannot be verified:" >&2
    echo "$out" >&2
    return 1
  fi
  printf '%s\n' "$out"
}

check_layers() {
  local crate=$1 mode=$2 allowed banned flags=() edges tree stray found
  if [[ $mode == all ]]; then
    allowed=${row_edges_all[$crate]}
    flags=(--all-features)
  else
    allowed=${row_edges_default[$crate]}
  fi
  banned=$(list_field "$(banned_families_for "${row_banned[$crate]}" "$mode")")

  edges=$(run_tree "$crate ($mode build) direct dependencies" \
    -p "$crate" "${flags[@]}" -e normal --prefix none --depth 1 --target all) || exit 1
  stray=$(printf '%s\n' "$edges" | awk '{ print $1 }' | sort -u \
    | rg -Fx -f <(printf '%s\n' "${members[@]}") | rg -Fxv "$crate" || true)
  for edge in $stray; do
    found=0
    for permitted in $allowed; do
      [[ $edge == "$permitted" ]] && found=1
    done
    if (( found == 0 )); then
      fail "$crate ($mode build) must not depend directly on $edge; allowed workspace edges: ${allowed:-none}"
    fi
  done

  if [[ -n $banned ]]; then
    tree=$(run_tree "$crate ($mode build) full tree" \
      -p "$crate" "${flags[@]}" -e normal --prefix none --target all) || exit 1
    found=$(printf '%s\n' "$tree" | awk '{ print $1 }' | sort -u \
      | rg -Fx -f <(printf '%s\n' $banned) || true)
    if [[ -n $found ]]; then
      fail "$crate ($mode build) must not depend on: $(printf '%s ' $found)(banned: $banned)"
    fi
  fi
}

if [[ -z ${ARCH_LINT_PANIC_ROOT:-} ]]; then
  for crate in "${table_crates[@]}"; do
    for mode in "${banned_dependency_probe_modes[@]}"; do
      check_layers "$crate" "$mode"
    done
  done
fi

# --- the panic policy --------------------------------------------------------

crate_src_dir() {
  printf '%s/crates/%s/src' "${ARCH_LINT_PANIC_ROOT:-.}" "$1"
}

# Every lib and bin root of a crate, one path per line.
crate_roots() {
  local crate=$1 root
  if [[ -n ${ARCH_LINT_PANIC_ROOT:-} ]]; then
    local src
    src=$(crate_src_dir "$crate")
    for root in "$src/lib.rs" "$src/main.rs" "$src"/bin/*.rs; do
      [[ -f $root ]] && printf '%s\n' "$root"
    done
    return 0
  fi
  local json
  json=$(workspace_metadata) || return 1
  local path
  while IFS= read -r path; do
    printf '%s\n' "${path#"$repo_root"/}"
  done < <(jq -r --arg crate "$crate" '
    .packages[] | select(.name == $crate) | .targets[]
    | select(.kind | any(. == "lib" or . == "bin")) | .src_path' <<<"$json" | sort -u)
}

# The attribute must be a `cfg_attr(not(test), deny(...))` that names all six
# lints; a stray mention of one of them elsewhere in the file proves nothing.
root_has_panic_attribute() {
  local root=$1 denied name
  denied=$(rg -U --no-line-number --no-filename -o -r '$1' \
    '(?m)^#!\[cfg_attr\(\s*not\(test\)\s*,\s*deny\(([^)]*)\)\s*\)\]' "$root" || true)
  [[ -n $denied ]] || return 1
  for name in "${panic_lint_names[@]}"; do
    [[ $denied == *"clippy::$name"* ]] || return 1
  done
}

# A suppression is an `allow` or `expect` that lists one of the six lints; other
# clippy lints may precede it in the same attribute.
suppression_pattern="(allow|expect)\\(\\s*(clippy::[a-z_]+\\s*,\\s*)*clippy::($panic_lints)\\b"
allow_pattern="allow\\(\\s*(clippy::[a-z_]+\\s*,\\s*)*clippy::($panic_lints)\\b"

count_matches() {
  local pattern=$1 dir=$2
  # rg exits 1 when nothing matches, which is a count of zero, not a failure.
  { rg -U --count-matches "$pattern" "$dir" --glob '*.rs' 2>/dev/null || true; } \
    | awk -F: '{ total += $NF } END { print total + 0 }'
}

suppression_total=0
allow_total=0
for crate in "${table_crates[@]}"; do
  if [[ ${row_policy[$crate]} == pending ]]; then
    echo "  $crate: panic policy pending"
    continue
  fi
  roots=$(crate_roots "$crate") || exit 1
  if [[ -z $roots ]]; then
    fail "$crate has a deny row but no lib or bin root was found"
    continue
  fi
  while IFS= read -r root; do
    if ! root_has_panic_attribute "$root"; then
      fail "$root must carry #![cfg_attr(not(test), deny(...))] for all six never-crash lints (${panic_lint_names[*]})"
    fi
  done <<<"$roots"
  crate_suppressions=$(count_matches "$suppression_pattern" "$(crate_src_dir "$crate")")
  crate_allows=$(count_matches "$allow_pattern" "$(crate_src_dir "$crate")")
  suppression_total=$((suppression_total + crate_suppressions))
  allow_total=$((allow_total + crate_allows))
  echo "  $crate: panic policy deny ($crate_suppressions suppressions)"
done

if (( allow_total > 0 )); then
  fail "$allow_total #[allow] of a never-crash lint found; write #[expect(…, reason)] instead of #[allow] so a stale suppression surfaces"
fi
if (( failed != 0 )); then
  exit 1
fi

if (( suppression_total > panic_lint_budget )); then
  echo "panic-lint suppressions grew from $panic_lint_budget to $suppression_total" >&2
  echo "  return an error instead of suppressing the lint" >&2
  exit 1
elif (( suppression_total < panic_lint_budget )); then
  echo "panic-lint suppressions are down to $suppression_total (budget still says $panic_lint_budget)" >&2
  echo "  lower panic_lint_budget in scripts/check-crate-layers.sh to $suppression_total" >&2
  exit 1
else
  echo "  panic-lint suppressions: $suppression_total (at budget)"
fi

echo "Crate layers passed"
