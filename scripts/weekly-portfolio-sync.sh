#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

reprise_repo=${REPRISE_REPO:-/home/marvin/Projects/reprise}
bewerbung_repo=${BEWERBUNG_REPO:-/home/marvin/Projects/bewerbung}
codex_bin=${CODEX_BIN:-/home/marvin/.local/bin/codex}
prompt_file=${WEEKLY_SYNC_PROMPT:-$repo_root/docs/automation/weekly-portfolio-sync.md}
state_root=${WEEKLY_SYNC_STATE_ROOT:-${XDG_STATE_HOME:-$HOME/.local/state}/reprise-portfolio-sync}
sync_id=${WEEKLY_SYNC_ID:-$(date +%G-W%V)}
branch="automation/reprise-portfolio-$sync_id"

mkdir -p "$state_root/$sync_id"
result_file="$state_root/$sync_id/result.md"
codex_output="$state_root/$sync_id/codex-final.md"

exec 9>"$state_root/sync.lock"
if ! flock --nonblock 9; then
  echo "Another weekly portfolio sync is already running." >&2
  exit 1
fi

require_git_repo() {
  local path=$1
  local label=$2
  if ! git -C "$path" rev-parse --git-dir >/dev/null 2>&1; then
    echo "$label is not a Git repository: $path" >&2
    exit 1
  fi
  if ! git -C "$path" show-ref --verify --quiet refs/heads/main; then
    echo "$label has no local main branch: $path" >&2
    exit 1
  fi
}

require_git_repo "$reprise_repo" Reprise
require_git_repo "$bewerbung_repo" Bewerbung

if [[ ! -x $codex_bin ]]; then
  echo "Codex executable is missing: $codex_bin" >&2
  exit 1
fi
if [[ ! -f $prompt_file ]]; then
  echo "Weekly sync prompt is missing: $prompt_file" >&2
  exit 1
fi

reprise_exists=false
bewerbung_exists=false
git -C "$reprise_repo" show-ref --verify --quiet "refs/heads/$branch" && reprise_exists=true
git -C "$bewerbung_repo" show-ref --verify --quiet "refs/heads/$branch" && bewerbung_exists=true

if [[ $reprise_exists == true ]] &&
  [[ $(git -C "$reprise_repo" rev-list --count "main..$branch") -eq 0 ]]; then
  git -C "$reprise_repo" branch -d "$branch" >/dev/null
  reprise_exists=false
fi
if [[ $bewerbung_exists == true ]] &&
  [[ $(git -C "$bewerbung_repo" rev-list --count "main..$branch") -eq 0 ]]; then
  git -C "$bewerbung_repo" branch -d "$branch" >/dev/null
  bewerbung_exists=false
fi

if [[ $reprise_exists == true || $bewerbung_exists == true ]]; then
  if [[ $reprise_exists == true && $bewerbung_exists == true ]]; then
    {
      echo "# Weekly portfolio sync $sync_id"
      echo
      echo "Already prepared; no second run was started."
      echo
      echo "Reprise branch: $branch"
      echo "Bewerbung branch: $branch"
    } > "$result_file"
    exit 0
  fi
  echo "Only one repository contains $branch; resolve the partial prior run first." >&2
  exit 1
fi

if [[ -n ${WEEKLY_SYNC_RUN_ROOT:-} ]]; then
  run_root=$WEEKLY_SYNC_RUN_ROOT
  mkdir -p "$run_root"
else
  run_root=$(mktemp -d "${TMPDIR:-/tmp}/reprise-portfolio-sync.XXXXXX")
fi
reprise_worktree="$run_root/reprise"
bewerbung_worktree="$run_root/bewerbung"

reprise_added=false
bewerbung_added=false
cleanup_worktrees() {
  local exit_status=$?
  if [[ $bewerbung_added == true && -d $bewerbung_worktree ]] &&
    [[ -z $(git -C "$bewerbung_worktree" status --porcelain 2>/dev/null) ]]; then
    git -C "$bewerbung_repo" worktree remove "$bewerbung_worktree" >/dev/null 2>&1 || true
  fi
  if [[ $reprise_added == true && -d $reprise_worktree ]] &&
    [[ -z $(git -C "$reprise_worktree" status --porcelain 2>/dev/null) ]]; then
    git -C "$reprise_repo" worktree remove "$reprise_worktree" >/dev/null 2>&1 || true
  fi
  return "$exit_status"
}
trap cleanup_worktrees EXIT

git -C "$reprise_repo" worktree add --quiet -b "$branch" "$reprise_worktree" main
reprise_added=true
git -C "$bewerbung_repo" worktree add --quiet -b "$branch" "$bewerbung_worktree" main
bewerbung_added=true

prompt=$(<"$prompt_file")
prompt=${prompt//\{\{REPRISE_WORKTREE\}\}/$reprise_worktree}
prompt=${prompt//\{\{BEWERBUNG_WORKTREE\}\}/$bewerbung_worktree}

set +e
"$codex_bin" --ask-for-approval never exec \
  --ephemeral \
  --sandbox workspace-write \
  -C "$reprise_worktree" \
  --add-dir "$bewerbung_worktree" \
  -o "$codex_output" \
  "$prompt"
codex_status=$?
set -e

reprise_dirty=$(git -C "$reprise_worktree" status --porcelain)
bewerbung_dirty=$(git -C "$bewerbung_worktree" status --porcelain)
if [[ -n $reprise_dirty || -n $bewerbung_dirty ]]; then
  {
    echo "# Weekly portfolio sync $sync_id"
    echo
    echo "Status: failed with uncommitted changes; worktrees were preserved for recovery."
    echo "Codex exit status: $codex_status"
    echo
    echo "Reprise worktree: $reprise_worktree"
    echo "Bewerbung worktree: $bewerbung_worktree"
    echo "Reprise branch: $branch"
    echo "Bewerbung branch: $branch"
  } > "$result_file"
  echo "Weekly sync left uncommitted changes; see $result_file" >&2
  exit 1
fi

if ((codex_status != 0)); then
  {
    echo "# Weekly portfolio sync $sync_id"
    echo
    echo "Status: Codex failed without leaving uncommitted changes."
    echo "Codex exit status: $codex_status"
    echo
    echo "Reprise branch: $branch"
    echo "Bewerbung branch: $branch"
  } > "$result_file"
  exit "$codex_status"
fi

reprise_commits=$(git -C "$reprise_repo" rev-list --count "main..$branch")
bewerbung_commits=$(git -C "$bewerbung_repo" rev-list --count "main..$branch")

git -C "$bewerbung_repo" worktree remove "$bewerbung_worktree"
bewerbung_added=false
git -C "$reprise_repo" worktree remove "$reprise_worktree"
reprise_added=false

reprise_state="$branch ($reprise_commits new commit(s))"
bewerbung_state="$branch ($bewerbung_commits new commit(s))"
if ((reprise_commits == 0)); then
  git -C "$reprise_repo" branch -d "$branch" >/dev/null
  reprise_state="no changes"
fi
if ((bewerbung_commits == 0)); then
  git -C "$bewerbung_repo" branch -d "$branch" >/dev/null
  bewerbung_state="no changes"
fi

{
  echo "# Weekly portfolio sync $sync_id"
  echo
  echo "Status: complete"
  echo "Reprise branch: $reprise_state"
  echo "Bewerbung branch: $bewerbung_state"
  if [[ -s $codex_output ]]; then
    echo
    echo "## Codex report"
    echo
    cat "$codex_output"
  fi
} > "$result_file"

echo "Weekly portfolio sync complete: $result_file"
