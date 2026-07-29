#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
runner="$repo_root/scripts/reprise-worktree-gc.sh"
closer="$repo_root/scripts/close-worktree.sh"

fixture=$(mktemp -d "${TMPDIR:-/tmp}/reprise-worktree-gc.XXXXXX")
cleanup_fixture() {
  if [[ -n ${active_pid:-} ]]; then
    kill "$active_pid" 2>/dev/null || true
    wait "$active_pid" 2>/dev/null || true
  fi
  find "$fixture" -xdev -depth -delete 2>/dev/null || true
}
trap cleanup_fixture EXIT

repo="$fixture/reprise"
state_root="$fixture/state"
mkdir -p "$repo"
git -C "$repo" init --initial-branch=dev --quiet
git -C "$repo" config user.name "Worktree GC Test"
git -C "$repo" config user.email "worktree-gc@example.invalid"
printf '# Fixture\n' > "$repo/README.md"
git -C "$repo" add README.md
git -C "$repo" commit --quiet -m "fixture"

stale_worktree="$fixture/stale"
dirty_worktree="$fixture/dirty"
unmerged_worktree="$fixture/unmerged"
locked_worktree="$fixture/locked"
active_worktree="$fixture/active"
protected_worktree="$fixture/protected"
git -C "$repo" worktree add --quiet -b test/stale "$stale_worktree" dev
git -C "$repo" worktree add --quiet -b test/dirty "$dirty_worktree" dev
git -C "$repo" worktree add --quiet -b test/unmerged "$unmerged_worktree" dev
git -C "$repo" worktree add --quiet -b test/locked "$locked_worktree" dev
git -C "$repo" worktree add --quiet -b test/active "$active_worktree" dev
git -C "$repo" branch main dev
git -C "$repo" worktree add --quiet "$protected_worktree" main
printf 'unfinished\n' >> "$dirty_worktree/README.md"
printf 'unique\n' >> "$unmerged_worktree/README.md"
git -C "$unmerged_worktree" add README.md
git -C "$unmerged_worktree" commit --quiet -m "unique work"
git -C "$repo" worktree lock --reason "active test agent" "$locked_worktree"
(cd "$active_worktree" && exec sleep 30) &
active_pid=$!
for _ in {1..1000}; do
  [[ $(readlink "/proc/$active_pid/cwd" 2>/dev/null || true) == "$active_worktree" ]] &&
    break
done
[[ $(readlink "/proc/$active_pid/cwd") == "$active_worktree" ]]

report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --repo "$repo"
)

rg -Fq "candidate no_unique_commits $stale_worktree" <<<"$report"
rg -Fq "keep dirty $dirty_worktree" <<<"$report"
rg -Fq "keep unmerged $unmerged_worktree" <<<"$report"
rg -Fq "keep locked $locked_worktree" <<<"$report"
rg -Fq "keep active $active_worktree" <<<"$report"
rg -Fq "keep protected $protected_worktree" <<<"$report"
[[ -d $stale_worktree ]]
[[ -d $dirty_worktree ]]

apply_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --repo "$repo" --apply
)

rg -Fq "removed no_unique_commits $stale_worktree" <<<"$apply_report"
rg -Fq "keep dirty $dirty_worktree" <<<"$apply_report"
rg -Fq "keep unmerged $unmerged_worktree" <<<"$apply_report"
rg -Fq "keep locked $locked_worktree" <<<"$apply_report"
rg -Fq "keep active $active_worktree" <<<"$apply_report"
rg -Fq "keep protected $protected_worktree" <<<"$apply_report"
[[ $(find "$state_root/runs" -type f | wc -l) -eq 1 ]]
rg -Fq "removed no_unique_commits $stale_worktree" "$state_root"/runs/*.log
[[ ! -d $stale_worktree ]]
[[ -d $dirty_worktree ]]
[[ -d $unmerged_worktree ]]
[[ -d $locked_worktree ]]
[[ -d $active_worktree ]]
[[ -d $protected_worktree ]]
! git -C "$repo" show-ref --verify --quiet refs/heads/test/stale
git -C "$repo" show-ref --verify --quiet refs/heads/test/dirty
git -C "$repo" show-ref --verify --quiet refs/heads/test/unmerged
git -C "$repo" show-ref --verify --quiet refs/heads/test/locked
git -C "$repo" show-ref --verify --quiet refs/heads/test/active
git -C "$repo" show-ref --verify --quiet refs/heads/main
kill "$active_pid"
wait "$active_pid" 2>/dev/null || true
active_pid=

fake_bin="$fixture/bin"
mkdir -p "$fake_bin"
cat > "$fake_bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$FAKE_GH_PROOF"
EOF
chmod +x "$fake_bin/gh"

merged_worktree="$fixture/merged"
git -C "$repo" worktree add --quiet -b test/merged "$merged_worktree" dev
printf 'merged work\n' >> "$merged_worktree/README.md"
git -C "$merged_worktree" add README.md
git -C "$merged_worktree" commit --quiet -m "merged work"
merged_head=$(git -C "$merged_worktree" rev-parse HEAD)

test_must_fail=$(
  if env \
    FAKE_GH_PROOF=$'OPEN\tdev\ttest/merged\t'"$merged_head" \
    PATH="$fake_bin:$PATH" \
    REPRISE_GC_STATE_ROOT="$state_root" \
    "$closer" \
      --repo "$repo" \
      --worktree "$merged_worktree" \
      --pr 42 \
      --defer 2>"$fixture/invalid-proof.err"; then
    echo no
  else
    echo yes
  fi
)
[[ $test_must_fail == yes ]]
rg -Fq "pull request 42 is not merged" "$fixture/invalid-proof.err"
[[ -d $merged_worktree ]]
[[ ! -d $state_root/pending ]]

wrong_head=$(printf '0%.0s' {1..40})
test_must_fail=$(
  if env \
    FAKE_GH_PROOF=$'MERGED\tdev\ttest/merged\t'"$wrong_head" \
    PATH="$fake_bin:$PATH" \
    REPRISE_GC_STATE_ROOT="$state_root" \
    "$closer" \
      --repo "$repo" \
      --worktree "$merged_worktree" \
      --pr 42 \
      --defer 2>"$fixture/wrong-head.err"; then
    echo no
  else
    echo yes
  fi
)
[[ $test_must_fail == yes ]]
rg -Fq "does not match worktree HEAD $merged_head" "$fixture/wrong-head.err"
[[ ! -d $state_root/pending ]]

FAKE_GH_PROOF=$'MERGED\tdev\ttest/merged\t'"$merged_head" \
  PATH="$fake_bin:$PATH" \
  REPRISE_GC_STATE_ROOT="$state_root" \
  "$closer" \
  --repo "$merged_worktree" \
  --worktree "$merged_worktree" \
  --pr 42 \
  --defer

[[ -d $merged_worktree ]]
[[ $(find "$state_root/pending" -type f | wc -l) -eq 1 ]]

merged_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --repo "$repo" --apply
)
rg -Fq "removed verified_merged_pr $merged_worktree" <<<"$merged_report"
[[ ! -d $merged_worktree ]]
! git -C "$repo" show-ref --verify --quiet refs/heads/test/merged
[[ $(find "$state_root/pending" -type f | wc -l) -eq 0 ]]

busy_merged_worktree="$fixture/busy-merged"
git -C "$repo" worktree add --quiet -b test/busy-merged "$busy_merged_worktree" dev
printf 'busy merged work\n' >> "$busy_merged_worktree/README.md"
git -C "$busy_merged_worktree" add README.md
git -C "$busy_merged_worktree" commit --quiet -m "busy merged work"
busy_merged_head=$(git -C "$busy_merged_worktree" rev-parse HEAD)
(cd "$busy_merged_worktree" && exec sleep 30) &
active_pid=$!
for _ in {1..1000}; do
  [[ $(readlink "/proc/$active_pid/cwd" 2>/dev/null || true) == "$busy_merged_worktree" ]] &&
    break
done
[[ $(readlink "/proc/$active_pid/cwd") == "$busy_merged_worktree" ]]

busy_close_report=$(
  FAKE_GH_PROOF=$'MERGED\tdev\ttest/busy-merged\t'"$busy_merged_head" \
    PATH="$fake_bin:$PATH" \
    REPRISE_GC_STATE_ROOT="$state_root" \
    "$closer" \
    --repo "$repo" \
    --worktree "$busy_merged_worktree" \
    --pr 43
)
rg -Fq "deferred active_worktree $busy_merged_worktree" <<<"$busy_close_report"
[[ -d $busy_merged_worktree ]]
git -C "$repo" show-ref --verify --quiet refs/heads/test/busy-merged
[[ $(find "$state_root/pending" -type f | wc -l) -eq 1 ]]

kill "$active_pid"
wait "$active_pid" 2>/dev/null || true
active_pid=
busy_merged_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --repo "$repo" --apply
)
rg -Fq "removed verified_merged_pr $busy_merged_worktree" <<<"$busy_merged_report"
[[ ! -d $busy_merged_worktree ]]
! git -C "$repo" show-ref --verify --quiet refs/heads/test/busy-merged
[[ $(find "$state_root/pending" -type f | wc -l) -eq 0 ]]

cache_worktree="$fixture/cache"
git -C "$repo" worktree add --quiet -b test/cache "$cache_worktree" dev
cat > "$cache_worktree/Cargo.toml" <<'EOF'
[package]
name = "worktree-gc-fixture"
version = "0.0.0"
edition = "2024"
EOF
printf '/target\n' > "$cache_worktree/.gitignore"
mkdir -p "$cache_worktree/src"
printf 'pub fn fixture() {}\n' > "$cache_worktree/src/lib.rs"
git -C "$cache_worktree" add Cargo.toml .gitignore src/lib.rs
git -C "$cache_worktree" commit --quiet -m "add cargo fixture"
mkdir -p "$cache_worktree/target/debug"
printf 'regenerable\n' > "$cache_worktree/target/debug/artifact"
touch --date='10 days ago' \
  "$cache_worktree/target" \
  "$cache_worktree/target/debug" \
  "$cache_worktree/target/debug/artifact"

cache_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep \
    --repo "$repo" \
    --target-max-age-days 7 \
    --target-min-kib 0
)
rg -Fq "candidate stale_target $cache_worktree/target" <<<"$cache_report"
[[ -d $cache_worktree/target ]]

cache_apply_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep \
    --repo "$repo" \
    --target-max-age-days 7 \
    --target-min-kib 0 \
    --apply
)
rg -Fq "cleaned stale_target $cache_worktree/target" <<<"$cache_apply_report"
[[ ! -d $cache_worktree/target ]]
[[ -d $cache_worktree ]]
git -C "$repo" show-ref --verify --quiet refs/heads/test/cache

scope="$fixture/scope"
standalone="$scope/.worktrees/standalone"
no_dev_repo="$scope/.worktrees/no-dev"
nested_stale="$standalone/.claude/worktrees/stale-agent"
outside_scope="$fixture/outside-scope"
mkdir -p "$scope"
git -C "$scope" init --initial-branch=dev --quiet
git -C "$scope" config user.name "Worktree GC Test"
git -C "$scope" config user.email "worktree-gc@example.invalid"
printf 'scope\n' > "$scope/README.md"
git -C "$scope" add README.md
git -C "$scope" commit --quiet -m "scope fixture"
git -C "$scope" worktree add --quiet -b test/outside "$outside_scope" dev

mkdir -p "$standalone"
git -C "$standalone" init --initial-branch=dev --quiet
git -C "$standalone" config user.name "Worktree GC Test"
git -C "$standalone" config user.email "worktree-gc@example.invalid"
printf 'standalone\n' > "$standalone/README.md"
git -C "$standalone" add README.md
git -C "$standalone" commit --quiet -m "standalone fixture"
mkdir -p "$no_dev_repo"
git -C "$no_dev_repo" init --initial-branch=main --quiet
git -C "$no_dev_repo" config user.name "Worktree GC Test"
git -C "$no_dev_repo" config user.email "worktree-gc@example.invalid"
printf 'no dev\n' > "$no_dev_repo/README.md"
git -C "$no_dev_repo" add README.md
git -C "$no_dev_repo" commit --quiet -m "no-dev fixture"
mkdir -p "$(dirname "$nested_stale")"
git -C "$standalone" worktree add \
  --quiet -b worktree-stale-agent "$nested_stale" dev

scope_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --scope "$scope"
)
rg -Fq "candidate no_unique_commits $nested_stale" <<<"$scope_report"
rg -Fq "keep no_dev_ref $no_dev_repo" <<<"$scope_report"
rg -Fq "keep outside_scope $outside_scope" <<<"$scope_report"
[[ -d $nested_stale ]]
[[ -d $outside_scope ]]

scope_apply_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --scope "$scope" --apply
)
rg -Fq "removed no_unique_commits $nested_stale" <<<"$scope_apply_report"
[[ ! -d $nested_stale ]]
[[ -d $outside_scope ]]
git -C "$scope" show-ref --verify --quiet refs/heads/test/outside
! git -C "$standalone" show-ref \
  --verify --quiet refs/heads/worktree-stale-agent

echo "Worktree GC safety: OK"
