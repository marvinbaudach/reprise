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
#   ARCH_LINT_PANIC_BUDGETS  a whitespace-separated `crate=N` list replacing the
#                          suppression budgets below
#   ARCH_LINT_PENDING_CRATES  a whitespace-separated list replacing the crates
#                          that may carry the `pending` policy
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# Field 1: crate. Fields 2 and 3: direct workspace edges of the default build and
# of the all-features build (`-` for none). Field 4: dependency families banned
# from the full normal tree on every target, either one list for both builds or
# `default=<list> all=<list>`. Field 5: panic policy, `deny` or `pending`.
#
# `@gtk` and `@glib` in the fourth field expand to the family lists below, so a
# `-sys` crate cannot slip past a ban on the crate it wraps.
#
# `zvariant` in reprise-runtime-protocol is a data format, not `zbus`, and is
# allowed. reprise-platform-linux links glib through GStreamer and zbus for MPRIS,
# so neither is banned there. zbus reaches reprise-cli and reprise-mcp only with
# their `mpris` feature, which is why their all-features build may carry it.
# layer_table_begin
layer_table='
reprise-runtime-protocol | - | - | @gtk @glib zbus | deny
reprise-core | - | - | @gtk @glib zbus | deny
reprise-view | reprise-core | reprise-core | @gtk @glib zbus | deny
reprise-stems | reprise-core | reprise-core | @gtk @glib zbus | deny
reprise-android-ffi | reprise-core reprise-view | reprise-core reprise-view | @gtk @glib zbus | deny
reprise-cli | reprise-core | reprise-core reprise-runtime-protocol reprise-stems | default=@gtk @glib zbus all=@gtk @glib | deny
reprise-mcp | reprise-core | reprise-core reprise-runtime-protocol | default=@gtk @glib zbus all=@gtk @glib | deny
reprise-platform-linux | reprise-core reprise-runtime-protocol | reprise-core reprise-runtime-protocol | @gtk | deny
reprise-gnome | reprise-core reprise-view reprise-platform-linux | reprise-core reprise-view reprise-platform-linux | - | pending
'
# layer_table_end

# Dependency families the table bans by macro. A `-sys` crate links the same
# native library as the crate it wraps, so it is banned with it.
gtk_families='gtk4 libadwaita gdk4 pango gtk4-sys libadwaita-sys gdk4-sys pango-sys'
glib_families='glib gio gstreamer glib-sys gobject-sys gio-sys gstreamer-sys'

# Only these crates may carry the `pending` panic policy. A row cannot be flipped
# to `pending` to escape the never-crash lints.
pending_allowed='reprise-gnome'
if [[ -n ${ARCH_LINT_PENDING_CRATES:-} ]]; then
  pending_allowed=$ARCH_LINT_PENDING_CRATES
fi

# The exact number of never-crash lint suppressions (`#[expect(clippy::unwrap_used
# ...)]` and its five siblings) in each `deny` crate; a crate not listed has a
# budget of 0. Each number is a CEILING and a FLOOR: a new suppression fails here,
# and so does a removed one until its budget comes down in the same commit. The
# budget is per crate, so one crate's new suppression cannot hide behind another
# crate's removal. An `#[allow]` of these lints is never accepted, because an
# allow never goes stale, and neither is an allow or expect of `clippy::restriction`.
# panic_lint_budgets_begin
panic_lint_budgets='reprise-core=1'
# panic_lint_budgets_end
if [[ -n ${ARCH_LINT_PANIC_BUDGETS:-} ]]; then
  panic_lint_budgets=$ARCH_LINT_PANIC_BUDGETS
fi

panic_lint_names=(unwrap_used expect_used panic todo unimplemented unreachable)

banned_dependency_probe_modes=(default all)

for tool in jq rg perl; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    case $tool in
      jq) purpose="read the workspace members and crate roots" ;;
      rg) purpose="list the source files the panic policy scans" ;;
      *) purpose="read the lint attributes of the source files" ;;
    esac
    echo "$tool is required to $purpose; install it" >&2
    exit 1
  fi
done

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
  if [[ $policy == pending && " $pending_allowed " != *" $crate "* ]]; then
    fail "layer table marks $crate pending; only $pending_allowed may be pending, so every other crate stays under the never-crash lints"
    continue
  fi
  table_crates+=("$crate")
  row_edges_default[$crate]=$(list_field "$edges_default")
  row_edges_all[$crate]=$(list_field "$edges_all")
  banned=$(trim "$banned")
  banned=${banned//@gtk/$gtk_families}
  row_banned[$crate]=${banned//@glib/$glib_families}
  row_policy[$crate]=$policy
done <<<"$layer_table"

declare -A row_budget
for entry in $panic_lint_budgets; do
  budget_crate=${entry%%=*}
  budget_value=${entry#*=}
  if [[ $entry != *=* || ! $budget_value =~ ^[0-9]+$ ]]; then
    fail "panic-lint budget entry '$entry' must read crate=N"
  elif [[ ${row_policy[$budget_crate]:-} != deny ]]; then
    fail "panic-lint budget names $budget_crate, which is not a deny crate in the layer table"
  else
    row_budget[$budget_crate]=$budget_value
  fi
done
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

is_member() {
  local candidate=$1 member
  for member in "${members[@]}"; do
    [[ $member == "$candidate" ]] && return 0
  done
  return 1
}

check_layers() {
  local crate=$1 mode=$2 allowed banned flags=() edges tree line name permitted found
  local -a banned_list=() stray_names=() found_names=()
  if [[ $mode == all ]]; then
    allowed=${row_edges_all[$crate]}
    flags=(--all-features)
  else
    allowed=${row_edges_default[$crate]}
  fi
  banned=$(list_field "$(banned_families_for "${row_banned[$crate]}" "$mode")")
  read -r -a banned_list <<<"$banned"

  edges=$(run_tree "$crate ($mode build) direct dependencies" \
    -p "$crate" "${flags[@]}" -e normal --prefix none --depth 1 --target all) || exit 1
  while read -r name; do
    if [[ -n $name && $name != "$crate" ]] && is_member "$name"; then
      stray_names+=("$name")
    fi
  done < <(printf '%s\n' "$edges" | awk '{ print $1 }' | sort -u)
  for name in "${stray_names[@]}"; do
    found=0
    for permitted in $allowed; do
      [[ $name == "$permitted" ]] && found=1
    done
    if (( found == 0 )); then
      fail "$crate ($mode build) must not depend directly on $name; allowed workspace edges: ${allowed:-none}"
    fi
  done

  if (( ${#banned_list[@]} > 0 )); then
    tree=$(run_tree "$crate ($mode build) full tree" \
      -p "$crate" "${flags[@]}" -e normal --prefix none --target all) || exit 1
    while read -r name; do
      for line in "${banned_list[@]}"; do
        if [[ $name == "$line" ]]; then
          found_names+=("$name")
        fi
      done
    done < <(printf '%s\n' "$tree" | awk '{ print $1 }' | sort -u)
    if (( ${#found_names[@]} > 0 )); then
      fail "$crate ($mode build) must not depend on: ${found_names[*]} (banned: $banned)"
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

# Only the production code of a crate is held to the policy: the `src` directories
# of its lib and bin targets. Examples, integration tests and benches are
# deliberately exempt -- they are not shipped, a panic there fails a test run and
# nothing else, and they carry no `#![cfg_attr(not(test), deny(...))]`.

# The root files of a crate's lib and bin targets, one path per line, from
# `cargo metadata`.
crate_target_paths() {
  local crate=$1 json path
  json=$(workspace_metadata) || return 1
  while IFS= read -r path; do
    printf '%s\n' "${path#"$repo_root"/}"
  done < <(jq -r --arg crate "$crate" '
    .packages[] | select(.name == $crate) | .targets[]
    | select(.kind | any(. == "lib" or . == "bin")) | .src_path' <<<"$json" | sort -u)
}

# The source directories of a crate, one per line: the directory of every lib and
# bin target's root file.
crate_source_dirs() {
  local crate=$1 path
  if [[ -n ${ARCH_LINT_PANIC_ROOT:-} ]]; then
    printf '%s/crates/%s/src\n' "$ARCH_LINT_PANIC_ROOT" "$crate"
    return 0
  fi
  local paths
  paths=$(crate_target_paths "$crate") || return 1
  while IFS= read -r path; do
    [[ -n $path ]] && dirname "$path"
  done <<<"$paths"
}

# Every lib and bin root of a crate, one path per line.
crate_roots() {
  local crate=$1 root
  if [[ -n ${ARCH_LINT_PANIC_ROOT:-} ]]; then
    local src="$ARCH_LINT_PANIC_ROOT/crates/$crate/src"
    for root in "$src/lib.rs" "$src/main.rs" "$src"/bin/*.rs; do
      [[ -f $root ]] && printf '%s\n' "$root"
    done
    return 0
  fi
  crate_target_paths "$crate"
}

# Every .rs file under the given directories, ignore files and hidden paths
# included. `rg --files` exits 1 when it lists nothing, which is an empty crate;
# any other non-zero exit is a real failure and aborts the gate.
rust_files() {
  local status=0
  rg --files --no-ignore --hidden --glob '*.rs' -- "$@" || status=$?
  if (( status > 1 )); then
    echo "rg failed (exit $status) while listing the .rs files under: $*" >&2
    return 1
  fi
}

# Reads Rust source with comments and string literals removed, so a lint name in
# a comment or a `reason = "…"` proves nothing. Two modes:
#   roots <file>...         prints each file that carries
#                           `#![cfg_attr(not(test), deny(...))]` naming exactly the
#                           six never-crash lints, as whole comma-separated tokens
#   suppressions <file>...  prints `<kind> <file>` once per attribute that
#                           suppresses a lint: `allow` and `expect` of one of the
#                           six lints (also inside `cfg_attr`), and `restriction`
#                           for an allow or expect of `clippy::restriction`, which
#                           would cover all six at once
read -r -d '' rust_attribute_scanner <<'PERL' || true
use strict;
use warnings;

my $mode = shift @ARGV;
my %six = map { $_ => 1 } qw(unwrap_used expect_used panic todo unimplemented unreachable);

sub strip {
    my ($s) = @_;
    my $out = '';
    my $n   = length $s;
    my $i   = 0;
    while ($i < $n) {
        my $c = substr($s, $i, 1);
        my $d = substr($s, $i, 2);
        if ($d eq '//') {
            my $nl = index($s, "\n", $i);
            $i = $nl < 0 ? $n : $nl;
        } elsif ($d eq '/*') {
            my $depth = 1;
            $i += 2;
            while ($i < $n && $depth > 0) {
                my $e = substr($s, $i, 2);
                if    ($e eq '/*') { $depth++; $i += 2 }
                elsif ($e eq '*/') { $depth--; $i += 2 }
                else               { $i++ }
            }
            $out .= ' ';
        } elsif ($c eq 'r' && ($i == 0 || substr($s, $i - 1, 1) !~ /\w/)) {
            pos($s) = $i;
            if ($s =~ /\Gr(#*)"/gc) {
                my $close = '"' . $1;
                my $end   = index($s, $close, pos($s));
                $i = $end < 0 ? $n : $end + length $close;
                $out .= '""';
            } else {
                $out .= $c;
                $i++;
            }
        } elsif ($c eq '"') {
            $i++;
            while ($i < $n) {
                my $ch = substr($s, $i, 1);
                if    ($ch eq '\\') { $i += 2 }
                elsif ($ch eq '"')  { $i++; last }
                else                { $i++ }
            }
            $out .= '""';
        } elsif ($c eq "'") {
            pos($s) = $i;
            if ($s =~ /\G'(?:\\.[^']*|[^'\\])'/gc) {
                $i = pos($s);
                $out .= "''";
            } else {
                $out .= $c;
                $i++;
            }
        } else {
            $out .= $c;
            $i++;
        }
    }
    return $out;
}

sub attributes {
    my ($t, $inner_only) = @_;
    my @found;
    my $re = $inner_only
        ? qr/#\s*!\s*(?<a>\[(?:[^\[\]]++|(?&a))*+\])/
        : qr/#\s*!?\s*(?<a>\[(?:[^\[\]]++|(?&a))*+\])/;
    while ($t =~ /$re/g) {
        my $body = $+{a};
        $body =~ s/\A\[//;
        $body =~ s/\]\z//;
        push @found, $body;
    }
    return @found;
}

sub lint_names {
    my ($text) = @_;
    my @names;
    while ($text =~ /clippy\s*::\s*(\w+)/g) { push @names, $1 }
    return @names;
}

for my $file (@ARGV) {
    open my $fh, '<', $file or die "cannot read $file: $!\n";
    my $source = do { local $/; <$fh> };
    close $fh;
    my $text = strip($source);
    if ($mode eq 'roots') {
        for my $body (attributes($text, 1)) {
            next unless $body =~ /\A\s*cfg_attr\s*\(\s*not\s*\(\s*test\s*\)\s*,\s*deny\s*\(([^()]*)\)\s*\)\s*\z/;
            my $list   = $1;
            my @tokens = grep { length } map { s/\s+//gr } split /,/, $list;
            my %seen   = map { $_ => 1 } @tokens;
            my @names  = map { /\Aclippy::(\w+)\z/ ? $1 : '' } keys %seen;
            next unless keys %seen == keys %six && !grep { !$six{$_} } @names;
            print "$file\n";
            last;
        }
    } else {
        for my $body (attributes($text, 0)) {
            my %kinds;
            while ($body =~ /\b(allow|expect)\s*(?<p>\((?:[^()]++|(?&p))*+\))/g) {
                my ($verb, $list) = ($1, $+{p});
                my @names = lint_names($list);
                $kinds{restriction} = 1 if grep { $_ eq 'restriction' } @names;
                $kinds{$verb}       = 1 if grep { $six{$_} } @names;
            }
            print "$_ $file\n" for sort keys %kinds;
        }
    }
}
PERL

root_has_panic_attribute() {
  local root=$1 carriers
  carriers=$(perl -e "$rust_attribute_scanner" roots "$root") || {
    echo "perl failed while reading $root" >&2
    exit 1
  }
  [[ -n $carriers ]]
}

# Prints one `<kind> <file>` line per suppression attribute under the directories.
crate_suppression_lines() {
  local listing
  local -a files
  listing=$(rust_files "$@") || exit 1
  [[ -n $listing ]] || return 0
  mapfile -t files < <(printf '%s\n' "$listing" | sort -u)
  perl -e "$rust_attribute_scanner" suppressions "${files[@]}" || {
    echo "perl failed while reading the suppression attributes under: $*" >&2
    exit 1
  }
}

suppression_total=0
allow_total=0
restriction_total=0
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
      fail "$root must carry #![cfg_attr(not(test), deny(...))] naming exactly the six never-crash lints (${panic_lint_names[*]})"
    fi
  done <<<"$roots"
  mapfile -t source_dirs < <(crate_source_dirs "$crate")
  if (( ${#source_dirs[@]} == 0 )); then
    fail "$crate has a deny row but no source directory was found"
    continue
  fi
  lines=$(crate_suppression_lines "${source_dirs[@]}") || exit 1
  crate_suppressions=$(grep -c '^expect ' <<<"$lines" || true)
  crate_allows=$(grep -c '^allow ' <<<"$lines" || true)
  crate_restrictions=$(grep -c '^restriction ' <<<"$lines" || true)
  budget=${row_budget[$crate]:-0}
  suppression_total=$((suppression_total + crate_suppressions))
  allow_total=$((allow_total + crate_allows))
  restriction_total=$((restriction_total + crate_restrictions))
  echo "  $crate: panic policy deny ($crate_suppressions suppressions, budget $budget)"
  if (( crate_suppressions > budget )); then
    fail "$crate: panic-lint suppressions grew from $budget to $crate_suppressions; return an error instead of suppressing the lint"
  elif (( crate_suppressions < budget )); then
    fail "$crate: panic-lint suppressions are down to $crate_suppressions (budget still says $budget); lower its entry in panic_lint_budgets in scripts/check-crate-layers.sh"
  fi
done

if (( allow_total > 0 )); then
  fail "$allow_total #[allow] of a never-crash lint found; write #[expect(…, reason)] instead of #[allow] so a stale suppression surfaces"
fi
if (( restriction_total > 0 )); then
  fail "$restriction_total allow or expect of clippy::restriction found; it switches off all six never-crash lints, name the lint that is justified instead"
fi
if (( failed != 0 )); then
  exit 1
fi

echo "  panic-lint suppressions: $suppression_total (at budget)"
echo "Crate layers passed"
