#!/usr/bin/env bash
set -euo pipefail

readonly DEFAULT_REPO=/home/marvin/Projects/reprise
readonly PROTECTED_BRANCH_PATTERN='^(dev|main)$'
RECLAIMED_KIB=0

usage() {
  cat <<'EOF'
Usage:
  scripts/reprise-worktree-gc.sh sweep [--repo PATH | --scope PATH] [--apply]
      [--exclude PATH]...
      [--target-max-age-days DAYS] [--target-min-kib KIB]
  scripts/reprise-worktree-gc.sh close --repo PATH --worktree PATH --pr NUMBER
      [--defer]

Without --apply, sweep is a read-only report.
EOF
}

die() {
  echo "worktree-gc: $*" >&2
  exit 1
}

canonical_directory() {
  local path=$1
  [[ -d $path ]] || return 1
  realpath "$path"
}

absolute_directory() {
  local path=$1
  canonical_directory "$path" || die "directory does not exist: $path"
}

canonical_repository_root() {
  local repo=$1
  local root
  root=$(
    git -C "$repo" worktree list --porcelain |
      sed -n 's/^worktree //p' |
      sed -n '1p'
  )
  [[ -n $root ]] || die "repository has no primary worktree: $repo"
  absolute_directory "$root"
}

resolve_base_ref() {
  local repo=$1
  local candidate
  for candidate in refs/heads/dev refs/remotes/origin/dev; do
    if git -C "$repo" rev-parse --verify --quiet "$candidate^{commit}" >/dev/null; then
      printf '%s\n' "$candidate"
      return
    fi
  done
  return 1
}

remove_branch_if_safe() {
  local repo=$1
  local branch=$2
  [[ -n $branch ]] || return 0
  if [[ $branch =~ $PROTECTED_BRANCH_PATTERN ]]; then
    die "refusing to delete protected branch: $branch"
  fi
  if git -C "$repo" show-ref --verify --quiet "refs/heads/$branch"; then
    git -C "$repo" branch -D "$branch" >/dev/null
  fi
}

marker_path_for() {
  local state_root=$1
  local path=$2
  local key
  key=$(printf '%s' "$path" | sha256sum | cut -d' ' -f1)
  printf '%s/pending/%s.record\n' "$state_root" "$key"
}

write_pending_marker() {
  local state_root=$1
  local repo=$2
  local path=$3
  local branch=$4
  local head=$5
  local pr=$6
  local marker
  marker=$(marker_path_for "$state_root" "$path")
  mkdir -p "$state_root/pending"
  {
    printf 'version=1\n'
    printf 'repo=%s\n' "$repo"
    printf 'worktree=%s\n' "$path"
    printf 'branch=%s\n' "$branch"
    printf 'head=%s\n' "$head"
    printf 'pr=%s\n' "$pr"
    printf 'base=dev\n'
    printf 'verified_at=%s\n' "$(date --iso-8601=seconds)"
  } > "$marker"
}

read_pending_marker() {
  local marker=$1
  marker_repo=
  marker_worktree=
  marker_branch=
  marker_head=
  marker_pr=
  local key value
  while IFS='=' read -r key value; do
    case "$key" in
      repo) marker_repo=$value ;;
      worktree) marker_worktree=$value ;;
      branch) marker_branch=$value ;;
      head) marker_head=$value ;;
      pr) marker_pr=$value ;;
    esac
  done < "$marker"
}

pending_marker_matches() {
  local state_root=$1
  local repo=$2
  local path=$3
  local branch=$4
  local head=$5
  local marker
  marker=$(marker_path_for "$state_root" "$path")
  [[ -f $marker ]] || return 1
  read_pending_marker "$marker"
  [[ $marker_repo == "$repo" &&
    $marker_worktree == "$path" &&
    $marker_branch == "$branch" &&
    $marker_head == "$head" &&
    $marker_pr =~ ^[0-9]+$ ]]
}

delete_pending_marker() {
  local state_root=$1
  local path=$2
  local marker
  marker=$(marker_path_for "$state_root" "$path")
  if [[ -f $marker ]]; then
    find "$marker" -maxdepth 0 -type f -delete
  fi
}

start_run_log() {
  local state_root=$1
  local run_id report
  mkdir -p "$state_root/runs"
  run_id="$(date +%Y%m%dT%H%M%S)-$$"
  report="$state_root/runs/$run_id.log"
  exec > >(tee "$report") 2>&1
}

classify_worktree() {
  local repo=$1
  local path=$2
  local head=$3
  local locked=$4
  local base_ref=$5
  local branch=$6
  local state_root=$7
  local managed_scope=$8

  if [[ $locked == true ]]; then
    printf 'locked\n'
    return
  fi
  if [[ ! -d $path ]]; then
    printf 'missing\n'
    return
  fi
  if [[ -n $managed_scope ]]; then
    local agent_root=${REPRISE_AGENT_WORKTREE_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/reprise-agent-worktrees}
    if [[ "$path/" != "$managed_scope/.worktrees/"* &&
      "$path/" != "$agent_root/"* ]]; then
      printf 'outside_scope\n'
      return
    fi
  fi
  if [[ $branch =~ $PROTECTED_BRANCH_PATTERN ]]; then
    printf 'protected\n'
    return
  fi
  if [[ -n $(git -C "$path" status --porcelain --untracked-files=all) ]]; then
    printf 'dirty\n'
    return
  fi
  if worktree_has_process "$path"; then
    printf 'active\n'
    return
  fi
  if pending_marker_matches "$state_root" "$repo" "$path" "$branch" "$head"; then
    printf 'verified_merged_pr\n'
    return
  fi
  if [[ $(git -C "$repo" rev-list --count "$base_ref..$head") -eq 0 ]]; then
    printf 'no_unique_commits\n'
    return
  fi
  printf 'unmerged\n'
}

worktree_has_process() {
  local path=$1
  local process_cwd process_link
  for process_link in /proc/[0-9]*/cwd; do
    process_cwd=$(readlink "$process_link" 2>/dev/null || true)
    if [[ "$process_cwd/" == "$path/"* ]]; then
      return 0
    fi
  done
  return 1
}

maybe_clean_artifact() {
  local artifact=$1
  local artifact_kind=$2
  local apply=$3
  local max_age_days=$4
  local min_kib=$5

  [[ -d $artifact ]] || return 0
  local recent_entry
  recent_entry=$(
    find "$artifact" -mindepth 1 -maxdepth 2 \
      -newermt "$max_age_days days ago" -print -quit
  )
  if [[ -n $recent_entry ]]; then
    return 0
  fi

  local size_kib
  size_kib=$(du -sk "$artifact" | cut -f1)
  ((size_kib >= min_kib)) || return 0

  if [[ $apply == true ]]; then
    if find "$artifact" -xdev -depth -delete; then
      RECLAIMED_KIB=$((RECLAIMED_KIB + size_kib))
      echo "cleaned $artifact_kind $artifact reclaimed=${size_kib}KiB"
    else
      local remaining_kib=0 reclaimed_kib
      if [[ -d $artifact ]]; then
        if ! remaining_kib=$(du -sk "$artifact" | cut -f1); then
          remaining_kib=$size_kib
        fi
      fi
      reclaimed_kib=$((size_kib - remaining_kib))
      ((reclaimed_kib >= 0)) || reclaimed_kib=0
      RECLAIMED_KIB=$((RECLAIMED_KIB + reclaimed_kib))
      echo "keep artifact_delete_failed $artifact reclaimed=${reclaimed_kib}KiB"
    fi
  else
    echo "candidate $artifact_kind $artifact"
  fi
}

maybe_clean_artifacts() {
  local path=$1
  local locked=$2
  local apply=$3
  local max_age_days=$4
  local min_kib=$5

  [[ $locked == false ]] || return 0
  if worktree_has_process "$path"; then
    echo "keep active_artifacts $path"
    return 0
  fi

  if [[ -f $path/Cargo.toml ]]; then
    maybe_clean_artifact \
      "$path/target" stale_target "$apply" "$max_age_days" "$min_kib"
  fi
  if [[ -f $path/android/settings.gradle.kts &&
    -f $path/android/app/build.gradle.kts ]]; then
    maybe_clean_artifact \
      "$path/android/app/build" stale_android_build \
      "$apply" "$max_age_days" "$min_kib"
    maybe_clean_artifact \
      "$path/.gradle-user-home" stale_gradle_home \
      "$apply" "$max_age_days" "$min_kib"
  fi
}

path_is_excluded() {
  local path=$1
  shift
  local canonical_path excluded_path canonical_excluded_path
  EXCLUSION_UNRESOLVED_PATH=
  canonical_path=$(canonical_directory "$path") || {
    EXCLUSION_UNRESOLVED_PATH=$path
    return 2
  }
  for excluded_path in "$@"; do
    canonical_excluded_path=$(canonical_directory "$excluded_path") || {
      EXCLUSION_UNRESOLVED_PATH=$excluded_path
      return 2
    }
    [[ $canonical_path != "$canonical_excluded_path" ]] || return 0
  done
  return 1
}

sweep_repo() {
  local repo=$1
  local apply=$2
  local state_root=$3
  local target_max_age_days=$4
  local target_min_kib=$5
  local managed_scope=$6
  local -a excluded_paths=("${@:7}")
  local base_ref primary_path
  if ! base_ref=$(resolve_base_ref "$repo"); then
    echo "keep no_dev_ref $repo"
    return
  fi
  primary_path=$(absolute_directory "$repo")

  local path='' head='' branch='' locked=false line classification exclusion_status
  while IFS= read -r line || [[ -n $line ]]; do
    if [[ -z $line ]]; then
      if [[ -n $path ]]; then
        if path_is_excluded "$path" "${excluded_paths[@]}"; then
          exclusion_status=0
        else
          exclusion_status=$?
        fi
        if ((exclusion_status == 0)); then
          echo "keep excluded $path"
        elif ((exclusion_status == 2)); then
          echo "keep unresolved_path $EXCLUSION_UNRESOLVED_PATH worktree=$path"
        elif [[ $path == "$primary_path" ]]; then
          echo "keep primary $path"
          maybe_clean_artifacts \
            "$path" "$locked" "$apply" \
            "$target_max_age_days" "$target_min_kib"
        else
          classification=$(classify_worktree \
            "$repo" "$path" "$head" "$locked" "$base_ref" "$branch" \
            "$state_root" "$managed_scope")
          if [[ $classification == no_unique_commits ||
            $classification == verified_merged_pr ]]; then
            if [[ $apply == true ]]; then
              local size_kib
              size_kib=$(du -sk "$path" | cut -f1)
              if git -C "$repo" worktree remove "$path"; then
                remove_branch_if_safe "$repo" "$branch"
                delete_pending_marker "$state_root" "$path"
                RECLAIMED_KIB=$((RECLAIMED_KIB + size_kib))
                echo "removed $classification $path reclaimed=${size_kib}KiB"
              else
                echo "keep removal_failed $path"
              fi
            else
              echo "candidate $classification $path"
            fi
          else
            echo "keep $classification $path"
            maybe_clean_artifacts \
              "$path" "$locked" "$apply" \
              "$target_max_age_days" "$target_min_kib"
          fi
        fi
      fi
      path=
      head=
      branch=
      locked=false
      continue
    fi

    case "$line" in
      "worktree "*) path=${line#worktree } ;;
      "HEAD "*) head=${line#HEAD } ;;
      "branch refs/heads/"*) branch=${line#branch refs/heads/} ;;
      locked*) locked=true ;;
    esac
  done < <(git -C "$repo" worktree list --porcelain; printf '\n')

  if [[ $apply == true ]]; then
    git -C "$repo" worktree prune --expire now
  fi
}

discover_repositories() {
  local scope=$1
  local candidate common_dir
  declare -A seen=()

  while IFS= read -r candidate; do
    git -C "$candidate" rev-parse --git-dir >/dev/null 2>&1 || continue
    common_dir=$(
      git -C "$candidate" rev-parse --path-format=absolute --git-common-dir
    )
    [[ -z ${seen[$common_dir]+present} ]] || continue
    seen[$common_dir]=1
    printf '%s\n' "$candidate"
  done < <(
    printf '%s\n' "$scope"
    if [[ -d $scope/.worktrees ]]; then
      find "$scope/.worktrees" -xdev -maxdepth 10 \
        \( -type d -name target -prune \) -o \
        \( -type d -name .git -print -prune \) |
        sed 's#/.git$##'
    fi
  )
}

registered_worktree_fields() {
  local repo=$1
  local wanted_path=$2
  local path='' head='' branch='' locked=false line
  local found=
  while IFS= read -r line || [[ -n $line ]]; do
    if [[ -z $line ]]; then
      if [[ $path == "$wanted_path" ]]; then
        found=$(printf '%s\t%s\t%s' "$head" "$branch" "$locked")
      fi
      path=
      head=
      branch=
      locked=false
      continue
    fi
    case "$line" in
      "worktree "*) path=${line#worktree } ;;
      "HEAD "*) head=${line#HEAD } ;;
      "branch refs/heads/"*) branch=${line#branch refs/heads/} ;;
      locked*) locked=true ;;
    esac
  done < <(git -C "$repo" worktree list --porcelain; printf '\n')
  [[ -n $found ]] || return 1
  printf '%s\n' "$found"
}

close_worktree() {
  local repo=$1
  local path=$2
  local pr=$3
  local defer=$4
  local state_root=$5

  local fields head branch locked
  fields=$(registered_worktree_fields "$repo" "$path") ||
    die "worktree is not registered in repository: $path"
  IFS=$'\t' read -r head branch locked <<<"$fields"
  [[ -n $branch ]] || die "refusing to close a detached worktree: $path"
  [[ ! $branch =~ $PROTECTED_BRANCH_PATTERN ]] ||
    die "refusing to close protected branch: $branch"
  [[ $locked == false ]] || die "worktree is locked: $path"
  [[ -z $(git -C "$path" status --porcelain --untracked-files=all) ]] ||
    die "worktree is dirty: $path"

  command -v gh >/dev/null 2>&1 ||
    die "gh is required to verify the merged pull request"
  local proof state base head_ref head_oid
  proof=$(
    cd "$repo"
    gh pr view "$pr" \
      --json state,baseRefName,headRefName,headRefOid \
      --jq '[.state, .baseRefName, .headRefName, .headRefOid] | @tsv'
  ) || die "could not verify pull request $pr"
  IFS=$'\t' read -r state base head_ref head_oid <<<"$proof"

  [[ $state == MERGED ]] || die "pull request $pr is not merged"
  [[ $base == dev ]] ||
    die "pull request $pr targets '$base', not dev"
  [[ $head_ref == "$branch" ]] ||
    die "pull request $pr head '$head_ref' does not match branch '$branch'"
  [[ $head_oid == "$head" ]] ||
    die "pull request $pr head $head_oid does not match worktree HEAD $head"

  local current_dir
  current_dir=$(pwd -P)
  if [[ $defer == true || "$current_dir/" == "$path/"* ]]; then
    write_pending_marker "$state_root" "$repo" "$path" "$branch" "$head" "$pr"
    echo "deferred verified_merged_pr $path"
    return
  fi
  if worktree_has_process "$path"; then
    write_pending_marker "$state_root" "$repo" "$path" "$branch" "$head" "$pr"
    echo "deferred active_worktree $path"
    return
  fi

  if git -C "$repo" worktree remove "$path"; then
    remove_branch_if_safe "$repo" "$branch"
    delete_pending_marker "$state_root" "$path"
    echo "removed verified_merged_pr $path"
  else
    write_pending_marker "$state_root" "$repo" "$path" "$branch" "$head" "$pr"
    echo "deferred busy_worktree $path"
  fi
}

main() {
  (($# > 0)) || {
    usage >&2
    exit 2
  }

  local command=$1
  shift
  [[ $command == sweep || $command == close ]] || {
    usage >&2
    exit 2
  }

  local repo=$DEFAULT_REPO
  local repo_explicit=false
  local scope=
  local apply=false
  local worktree=
  local pr=
  local defer=false
  local -a excluded_paths=()
  local state_root=${REPRISE_GC_STATE_ROOT:-${XDG_STATE_HOME:-$HOME/.local/state}/reprise-worktree-gc}
  local target_max_age_days=7
  local target_min_kib=1048576
  while (($#)); do
    case "$1" in
      --repo)
        (($# >= 2)) || die "--repo requires a path"
        repo=$2
        repo_explicit=true
        shift 2
        ;;
      --scope)
        (($# >= 2)) || die "--scope requires a path"
        scope=$2
        shift 2
        ;;
      --exclude)
        (($# >= 2)) || die "--exclude requires a path"
        excluded_paths+=("$2")
        shift 2
        ;;
      --apply)
        apply=true
        shift
        ;;
      --worktree)
        (($# >= 2)) || die "--worktree requires a path"
        worktree=$2
        shift 2
        ;;
      --pr)
        (($# >= 2)) || die "--pr requires a number"
        pr=$2
        shift 2
        ;;
      --defer)
        defer=true
        shift
        ;;
      --target-max-age-days)
        (($# >= 2)) || die "--target-max-age-days requires a number"
        target_max_age_days=$2
        shift 2
        ;;
      --target-min-kib)
        (($# >= 2)) || die "--target-min-kib requires a number"
        target_min_kib=$2
        shift 2
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
  done

  case "$command" in
    sweep)
      [[ -z $worktree && -z $pr && $defer == false ]] ||
        die "close-only arguments were passed to sweep"
      [[ $repo_explicit == false || -z $scope ]] ||
        die "--repo and --scope are mutually exclusive"
      [[ $target_max_age_days =~ ^[0-9]+$ ]] ||
        die "--target-max-age-days must be a non-negative integer"
      [[ $target_min_kib =~ ^[0-9]+$ ]] ||
        die "--target-min-kib must be a non-negative integer"
      local excluded_index
      for excluded_index in "${!excluded_paths[@]}"; do
        excluded_paths[excluded_index]=$(absolute_directory \
          "${excluded_paths[$excluded_index]}")
      done
      if [[ $apply == true ]]; then
        mkdir -p "$state_root"
        exec 9>"$state_root/gc.lock"
        flock --nonblock 9 || die "another worktree cleanup is running"
        start_run_log "$state_root"
      fi
      if [[ -n $scope ]]; then
        scope=$(absolute_directory "$scope")
        git -C "$scope" rev-parse --git-dir >/dev/null 2>&1 ||
          die "scope root is not a Git repository: $scope"
        while IFS= read -r discovered_repo; do
          echo "repo $discovered_repo"
          sweep_repo \
            "$discovered_repo" "$apply" "$state_root" \
            "$target_max_age_days" "$target_min_kib" "$scope" \
            "${excluded_paths[@]}"
        done < <(discover_repositories "$scope")
      else
        repo=$(absolute_directory "$repo")
        git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 ||
          die "not a Git repository: $repo"
        repo=$(canonical_repository_root "$repo")
        sweep_repo \
          "$repo" "$apply" "$state_root" \
          "$target_max_age_days" "$target_min_kib" "" \
          "${excluded_paths[@]}"
      fi
      if [[ $apply == true ]]; then
        echo "reclaimed_kib $RECLAIMED_KIB"
      fi
      ;;
    close)
      [[ -z $scope ]] || die "--scope is only valid with sweep"
      ((${#excluded_paths[@]} == 0)) || die "--exclude is only valid with sweep"
      repo=$(absolute_directory "$repo")
      git -C "$repo" rev-parse --git-dir >/dev/null 2>&1 ||
        die "not a Git repository: $repo"
      repo=$(canonical_repository_root "$repo")
      [[ $apply == false ]] || die "--apply is only valid with sweep"
      [[ -n $worktree ]] || die "close requires --worktree"
      [[ $pr =~ ^[0-9]+$ ]] || die "close requires a numeric --pr"
      worktree=$(absolute_directory "$worktree")
      mkdir -p "$state_root"
      exec 9>"$state_root/gc.lock"
      flock --nonblock 9 || die "another worktree cleanup is running"
      close_worktree "$repo" "$worktree" "$pr" "$defer" "$state_root"
      ;;
  esac
}

main "$@"
