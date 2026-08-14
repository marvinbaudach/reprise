#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
runner="$repo_root/scripts/reprise-worktree-gc.sh"
closer="$repo_root/scripts/close-worktree.sh"

refute() {
  if "$@"; then
    printf 'refute failed: %s\n' "$*" >&2
    exit 1
  fi
}

fixture=$(mktemp -d "${TMPDIR:-/tmp}/reprise-worktree-gc.XXXXXX")
can_enforce_delete_failure() {
  local probe_root=$1
  local restricted_dir="$probe_root/restricted"

  mkdir -p "$restricted_dir"
  printf 'probe delete failure\n' > "$restricted_dir/artifact"
  chmod 555 "$restricted_dir"
  if find "$probe_root" -xdev -depth -delete 2>/dev/null; then
    return 1
  fi
  chmod u+w "$restricted_dir"
  find "$probe_root" -xdev -depth -delete
}

delete_failure_probe="$fixture/delete-failure-probe"
if can_enforce_delete_failure "$delete_failure_probe"; then
  delete_failure_is_enforceable=true
else
  delete_failure_is_enforceable=false
fi
[[ ! -e $delete_failure_probe ]]

start_active_probe() {
  local path=$1
  (cd "$path" && exec tail -f /dev/null) &
  active_pid=$!
  while [[ $(readlink "/proc/$active_pid/cwd" 2>/dev/null || true) != "$path" ]]; do
    kill -0 "$active_pid" 2>/dev/null || {
      wait "$active_pid" 2>/dev/null || true
      active_pid=
      return 1
    }
  done
}

stop_active_probe() {
  kill "$active_pid"
  wait "$active_pid" 2>/dev/null || true
  active_pid=
}

cleanup_fixture() {
  if [[ -n ${active_pid:-} ]]; then
    kill "$active_pid" 2>/dev/null || true
    wait "$active_pid" 2>/dev/null || true
  fi
  if [[ -n ${undeletable_artifact_dir:-} ]]; then
    chmod u+w "$undeletable_artifact_dir" 2>/dev/null || true
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
start_active_probe "$active_worktree"

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
refute git -C "$repo" show-ref --verify --quiet refs/heads/test/stale
git -C "$repo" show-ref --verify --quiet refs/heads/test/dirty
git -C "$repo" show-ref --verify --quiet refs/heads/test/unmerged
git -C "$repo" show-ref --verify --quiet refs/heads/test/locked
git -C "$repo" show-ref --verify --quiet refs/heads/test/active
git -C "$repo" show-ref --verify --quiet refs/heads/main
stop_active_probe

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
refute git -C "$repo" show-ref --verify --quiet refs/heads/test/merged
[[ $(find "$state_root/pending" -type f | wc -l) -eq 0 ]]

busy_merged_worktree="$fixture/busy-merged"
git -C "$repo" worktree add --quiet -b test/busy-merged "$busy_merged_worktree" dev
printf 'busy merged work\n' >> "$busy_merged_worktree/README.md"
git -C "$busy_merged_worktree" add README.md
git -C "$busy_merged_worktree" commit --quiet -m "busy merged work"
busy_merged_head=$(git -C "$busy_merged_worktree" rev-parse HEAD)
start_active_probe "$busy_merged_worktree"

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

stop_active_probe
busy_merged_report=$(
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep --repo "$repo" --apply
)
rg -Fq "removed verified_merged_pr $busy_merged_worktree" <<<"$busy_merged_report"
[[ ! -d $busy_merged_worktree ]]
refute git -C "$repo" show-ref --verify --quiet refs/heads/test/busy-merged
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
refute git -C "$standalone" show-ref \
  --verify --quiet refs/heads/worktree-stale-agent

artifact_repo="$fixture/artifact-scope"
artifact_agent_root="$fixture/artifact-agent-worktrees"
dirty_artifact_worktree="$artifact_repo/.worktrees/dirty"
outside_artifact_worktree="$fixture/artifact-outside"
excluded_artifact_worktree="$artifact_repo/.worktrees/excluded"
excluded_outside_worktree="$fixture/artifact-excluded-outside"
active_artifact_worktree="$artifact_agent_root/active"
locked_artifact_worktree="$artifact_repo/.worktrees/locked"
fresh_artifact_worktree="$artifact_repo/.worktrees/fresh"
unrelated_artifact_worktree="$artifact_repo/.worktrees/unrelated"
unresolved_artifact_worktree="$artifact_repo/.worktrees/unresolved"
failing_artifact_worktree="$artifact_repo/.worktrees/delete-failure"
after_failure_worktree="$artifact_repo/.worktrees/zz-after-delete-failure"
mkdir -p "$artifact_repo"
git -C "$artifact_repo" init --initial-branch=dev --quiet
git -C "$artifact_repo" config user.name "Worktree GC Test"
git -C "$artifact_repo" config user.email "worktree-gc@example.invalid"
cat > "$artifact_repo/Cargo.toml" <<'EOF'
[package]
name = "worktree-gc-artifact-fixture"
version = "0.0.0"
edition = "2024"
EOF
printf '/target\n/android/app/build\n/.gradle-user-home\n' \
  > "$artifact_repo/.gitignore"
mkdir -p "$artifact_repo/src"
printf 'pub fn fixture() {}\n' > "$artifact_repo/src/lib.rs"
mkdir -p "$artifact_repo/android/app"
printf 'rootProject.name = "fixture"\ninclude(":app")\n' \
  > "$artifact_repo/android/settings.gradle.kts"
printf 'plugins {}\n' > "$artifact_repo/android/app/build.gradle.kts"
git -C "$artifact_repo" add \
  Cargo.toml .gitignore src/lib.rs \
  android/settings.gradle.kts android/app/build.gradle.kts
git -C "$artifact_repo" commit --quiet -m "artifact fixture"
git -C "$artifact_repo" worktree add \
  --quiet -b test/dirty-artifact "$dirty_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/outside-artifact "$outside_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/excluded-artifact "$excluded_artifact_worktree" dev
excluded_artifact_git_dir=$(git -C "$excluded_artifact_worktree" \
  rev-parse --path-format=absolute --git-dir)
excluded_artifact_listed_path="$artifact_repo/.worktrees//excluded"
printf '%s/.git\n' "$excluded_artifact_listed_path" \
  > "$excluded_artifact_git_dir/gitdir"
git -C "$artifact_repo" worktree add \
  --quiet -b test/excluded-outside "$excluded_outside_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/active-artifact "$active_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/locked-artifact "$locked_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/fresh-artifact "$fresh_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/unrelated-artifact "$unrelated_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/unresolved-artifact "$unresolved_artifact_worktree" dev
find "$unresolved_artifact_worktree" -xdev -depth -delete
git -C "$artifact_repo" worktree add \
  --quiet -b test/delete-failure "$failing_artifact_worktree" dev
git -C "$artifact_repo" worktree add \
  --quiet -b test/after-delete-failure "$after_failure_worktree" dev
git -C "$artifact_repo" worktree lock \
  --reason "active test agent" "$locked_artifact_worktree"
printf '// primary tracked work in progress\n' >> "$artifact_repo/src/lib.rs"
mkdir -p "$artifact_repo/.gradle-user-home/caches"
printf 'regenerable\n' > "$artifact_repo/.gradle-user-home/caches/artifact"
touch --date='10 days ago' \
  "$artifact_repo/.gradle-user-home" \
  "$artifact_repo/.gradle-user-home/caches" \
  "$artifact_repo/.gradle-user-home/caches/artifact"
printf '// tracked work in progress\n' >> "$dirty_artifact_worktree/src/lib.rs"
printf 'untracked work in progress\n' > "$dirty_artifact_worktree/notes.txt"
mkdir -p "$dirty_artifact_worktree/target/debug"
printf 'regenerable\n' > "$dirty_artifact_worktree/target/debug/artifact"
mkdir -p "$dirty_artifact_worktree/android/app/build/outputs"
printf 'regenerable\n' \
  > "$dirty_artifact_worktree/android/app/build/outputs/artifact"
mkdir -p "$dirty_artifact_worktree/.gradle-user-home/caches"
printf 'regenerable\n' \
  > "$dirty_artifact_worktree/.gradle-user-home/caches/artifact"
touch --date='10 days ago' \
  "$dirty_artifact_worktree/target" \
  "$dirty_artifact_worktree/target/debug" \
  "$dirty_artifact_worktree/target/debug/artifact" \
  "$dirty_artifact_worktree/android/app/build" \
  "$dirty_artifact_worktree/android/app/build/outputs" \
  "$dirty_artifact_worktree/android/app/build/outputs/artifact" \
  "$dirty_artifact_worktree/.gradle-user-home" \
  "$dirty_artifact_worktree/.gradle-user-home/caches" \
  "$dirty_artifact_worktree/.gradle-user-home/caches/artifact"
mkdir -p "$outside_artifact_worktree/target/debug"
printf 'regenerable\n' > "$outside_artifact_worktree/target/debug/artifact"
touch --date='10 days ago' \
  "$outside_artifact_worktree/target" \
  "$outside_artifact_worktree/target/debug" \
  "$outside_artifact_worktree/target/debug/artifact"
mkdir -p "$excluded_artifact_worktree/target/debug"
printf 'regenerable\n' > "$excluded_artifact_worktree/target/debug/artifact"
mkdir -p "$excluded_outside_worktree/android/app/build/outputs"
printf 'regenerable\n' \
  > "$excluded_outside_worktree/android/app/build/outputs/artifact"
touch --date='10 days ago' \
  "$excluded_artifact_worktree/target" \
  "$excluded_artifact_worktree/target/debug" \
  "$excluded_artifact_worktree/target/debug/artifact" \
  "$excluded_outside_worktree/android/app/build" \
  "$excluded_outside_worktree/android/app/build/outputs" \
  "$excluded_outside_worktree/android/app/build/outputs/artifact"
mkdir -p "$active_artifact_worktree/target/debug"
printf 'regenerable\n' > "$active_artifact_worktree/target/debug/artifact"
mkdir -p "$locked_artifact_worktree/target/debug"
printf 'regenerable\n' > "$locked_artifact_worktree/target/debug/artifact"
touch --date='10 days ago' \
  "$active_artifact_worktree/target" \
  "$active_artifact_worktree/target/debug" \
  "$active_artifact_worktree/target/debug/artifact" \
  "$locked_artifact_worktree/target" \
  "$locked_artifact_worktree/target/debug" \
  "$locked_artifact_worktree/target/debug/artifact"
printf '// fresh tracked work in progress\n' \
  >> "$fresh_artifact_worktree/src/lib.rs"
mkdir -p \
  "$fresh_artifact_worktree/target/debug" \
  "$fresh_artifact_worktree/android/app/build/outputs" \
  "$fresh_artifact_worktree/.gradle-user-home/caches"
printf 'fresh\n' > "$fresh_artifact_worktree/target/debug/artifact"
printf 'fresh\n' \
  > "$fresh_artifact_worktree/android/app/build/outputs/artifact"
printf 'fresh\n' \
  > "$fresh_artifact_worktree/.gradle-user-home/caches/artifact"
git -C "$unrelated_artifact_worktree" rm --quiet \
  Cargo.toml android/settings.gradle.kts android/app/build.gradle.kts
git -C "$unrelated_artifact_worktree" commit --quiet \
  -m "remove project markers"
mkdir -p \
  "$unrelated_artifact_worktree/target/debug" \
  "$unrelated_artifact_worktree/android/app/build/outputs" \
  "$unrelated_artifact_worktree/.gradle-user-home/caches"
printf 'unrelated\n' > "$unrelated_artifact_worktree/target/debug/artifact"
printf 'unrelated\n' \
  > "$unrelated_artifact_worktree/android/app/build/outputs/artifact"
printf 'unrelated\n' \
  > "$unrelated_artifact_worktree/.gradle-user-home/caches/artifact"
touch --date='10 days ago' \
  "$unrelated_artifact_worktree/target" \
  "$unrelated_artifact_worktree/target/debug" \
  "$unrelated_artifact_worktree/target/debug/artifact" \
  "$unrelated_artifact_worktree/android/app/build" \
  "$unrelated_artifact_worktree/android/app/build/outputs" \
  "$unrelated_artifact_worktree/android/app/build/outputs/artifact" \
  "$unrelated_artifact_worktree/.gradle-user-home" \
  "$unrelated_artifact_worktree/.gradle-user-home/caches" \
  "$unrelated_artifact_worktree/.gradle-user-home/caches/artifact"
printf '// preserve failing worktree\n' >> "$failing_artifact_worktree/src/lib.rs"
mkdir -p \
  "$failing_artifact_worktree/target/removable" \
  "$failing_artifact_worktree/target/restricted" \
  "$failing_artifact_worktree/android/app/build/outputs"
dd if=/dev/zero \
  of="$failing_artifact_worktree/target/removable/artifact" \
  bs=1024 count=64 status=none
printf 'cannot remove\n' \
  > "$failing_artifact_worktree/target/restricted/artifact"
printf 'remove after target failure\n' \
  > "$failing_artifact_worktree/android/app/build/outputs/artifact"
printf '// preserve later worktree\n' >> "$after_failure_worktree/src/lib.rs"
mkdir -p "$after_failure_worktree/target/debug"
printf 'remove from later worktree\n' \
  > "$after_failure_worktree/target/debug/artifact"
touch --date='10 days ago' \
  "$failing_artifact_worktree/target" \
  "$failing_artifact_worktree/target/removable" \
  "$failing_artifact_worktree/target/removable/artifact" \
  "$failing_artifact_worktree/target/restricted" \
  "$failing_artifact_worktree/target/restricted/artifact" \
  "$failing_artifact_worktree/android/app/build" \
  "$failing_artifact_worktree/android/app/build/outputs" \
  "$failing_artifact_worktree/android/app/build/outputs/artifact" \
  "$after_failure_worktree/target" \
  "$after_failure_worktree/target/debug" \
  "$after_failure_worktree/target/debug/artifact"
failing_target_kib_before=$(du -sk "$failing_artifact_worktree/target" | cut -f1)
undeletable_artifact_dir="$failing_artifact_worktree/target/restricted"
if [[ $delete_failure_is_enforceable == true ]]; then
  chmod 555 "$undeletable_artifact_dir"
fi
start_active_probe "$active_artifact_worktree"
expected_reclaimed_kib=0
for artifact in \
  "$artifact_repo/.gradle-user-home" \
  "$dirty_artifact_worktree/target" \
  "$dirty_artifact_worktree/android/app/build" \
  "$dirty_artifact_worktree/.gradle-user-home" \
  "$outside_artifact_worktree/target" \
  "$failing_artifact_worktree/android/app/build" \
  "$after_failure_worktree/target"; do
  artifact_kib=$(du -sk "$artifact" | cut -f1)
  expected_reclaimed_kib=$((expected_reclaimed_kib + artifact_kib))
done

dirty_artifact_report=$(
  REPRISE_AGENT_WORKTREE_ROOT="$artifact_agent_root" \
  REPRISE_GC_STATE_ROOT="$state_root" \
    "$runner" sweep \
    --scope "$artifact_repo" \
    --exclude "$excluded_artifact_worktree" \
    --exclude "$excluded_outside_worktree" \
    --target-max-age-days 7 \
    --target-min-kib 0 \
    --apply
)
stop_active_probe
if [[ $delete_failure_is_enforceable == true ]]; then
  failing_target_kib_after=$(du -sk "$failing_artifact_worktree/target" | cut -f1)
  expected_reclaimed_kib=$((
    expected_reclaimed_kib + failing_target_kib_before - failing_target_kib_after
  ))
else
  expected_reclaimed_kib=$((expected_reclaimed_kib + failing_target_kib_before))
fi
rg -Fq "keep dirty $dirty_artifact_worktree" <<<"$dirty_artifact_report"
rg -Fq "keep primary $artifact_repo" <<<"$dirty_artifact_report"
rg -Fq \
  "cleaned stale_gradle_home $artifact_repo/.gradle-user-home" \
  <<<"$dirty_artifact_report"
rg -Fq \
  "cleaned stale_target $dirty_artifact_worktree/target" \
  <<<"$dirty_artifact_report"
rg -Fq \
  "cleaned stale_android_build $dirty_artifact_worktree/android/app/build" \
  <<<"$dirty_artifact_report"
rg -Fq \
  "cleaned stale_gradle_home $dirty_artifact_worktree/.gradle-user-home" \
  <<<"$dirty_artifact_report"
rg -Fq \
  "keep outside_scope $outside_artifact_worktree" \
  <<<"$dirty_artifact_report"
rg -Fq \
  "cleaned stale_target $outside_artifact_worktree/target" \
  <<<"$dirty_artifact_report"
[[ ! -d $dirty_artifact_worktree/target ]]
[[ ! -d $artifact_repo/.gradle-user-home ]]
[[ ! -d $dirty_artifact_worktree/android/app/build ]]
[[ ! -d $dirty_artifact_worktree/.gradle-user-home ]]
[[ ! -d $outside_artifact_worktree/target ]]
[[ -d $outside_artifact_worktree ]]
git -C "$artifact_repo" show-ref \
  --verify --quiet refs/heads/test/outside-artifact
rg -Fq "keep excluded $excluded_artifact_listed_path" \
  <<<"$dirty_artifact_report"
rg -Fq "keep excluded $excluded_outside_worktree" \
  <<<"$dirty_artifact_report"
[[ -d $excluded_artifact_worktree/target ]]
[[ -d $excluded_outside_worktree/android/app/build ]]
git -C "$artifact_repo" show-ref \
  --verify --quiet refs/heads/test/excluded-artifact
git -C "$artifact_repo" show-ref \
  --verify --quiet refs/heads/test/excluded-outside
rg -Fq "keep active_artifacts $active_artifact_worktree" \
  <<<"$dirty_artifact_report"
rg -Fq "keep active $active_artifact_worktree" \
  <<<"$dirty_artifact_report"
refute rg -Fq "keep outside_scope $active_artifact_worktree" \
  <<<"$dirty_artifact_report"
rg -Fq "keep locked $locked_artifact_worktree" \
  <<<"$dirty_artifact_report"
rg -Fq "keep unresolved_path $unresolved_artifact_worktree" \
  <<<"$dirty_artifact_report"
if [[ $delete_failure_is_enforceable == true ]]; then
  rg -Fq \
    "keep artifact_delete_failed $failing_artifact_worktree/target" \
    <<<"$dirty_artifact_report"
  [[ -d $failing_artifact_worktree/target/restricted ]]
  [[ ! -d $failing_artifact_worktree/target/removable ]]
  echo "Delete-failure assertion: exercised"
else
  rg -Fq \
    "cleaned stale_target $failing_artifact_worktree/target" \
    <<<"$dirty_artifact_report"
  [[ ! -d $failing_artifact_worktree/target ]]
  echo "SKIPPED: artifact delete-failure assertion cannot be enforced in this environment; this gate did not run"
fi
rg -Fq \
  "cleaned stale_android_build $failing_artifact_worktree/android/app/build" \
  <<<"$dirty_artifact_report"
rg -Fq \
  "cleaned stale_target $after_failure_worktree/target" \
  <<<"$dirty_artifact_report"
[[ -d $active_artifact_worktree/target ]]
[[ -d $locked_artifact_worktree/target ]]
[[ -d $fresh_artifact_worktree/target ]]
[[ -d $fresh_artifact_worktree/android/app/build ]]
[[ -d $fresh_artifact_worktree/.gradle-user-home ]]
[[ -d $unrelated_artifact_worktree/target ]]
[[ -d $unrelated_artifact_worktree/android/app/build ]]
[[ -d $unrelated_artifact_worktree/.gradle-user-home ]]
[[ ! -d $failing_artifact_worktree/android/app/build ]]
[[ ! -d $after_failure_worktree/target ]]
git -C "$artifact_repo" show-ref \
  --verify --quiet refs/heads/test/unresolved-artifact
rg -Fq '// tracked work in progress' "$dirty_artifact_worktree/src/lib.rs"
rg -Fq '// primary tracked work in progress' "$artifact_repo/src/lib.rs"
rg -Fq 'untracked work in progress' "$dirty_artifact_worktree/notes.txt"
rg -Fxq "reclaimed_kib $expected_reclaimed_kib" \
  <<<"$dirty_artifact_report"

echo "Worktree GC safety: OK"
