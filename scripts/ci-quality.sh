#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ ${CI:-} == true ]]; then
  git config --global --add safe.directory "$repo_root"
fi

base_branch=${GITHUB_BASE_REF:-${GITHUB_REF_NAME:-main}}
head_branch=${GITHUB_HEAD_REF:-${GITHUB_REF_NAME:-}}

if [[ ${GITHUB_EVENT_NAME:-} == pull_request \
  && $base_branch == main \
  && $head_branch != dev \
  && $head_branch != hotfix/* ]]; then
  if git cat-file -e origin/main:.github/workflows/ci.yml 2>/dev/null; then
    echo "main accepts only dev promotions or emergency hotfix/* pull requests" >&2
    exit 1
  fi
  echo "Allowing the one-time CI bootstrap before main contains this workflow"
fi

base_ref="origin/$base_branch"
if ! git rev-parse --verify --quiet "$base_ref^{commit}" >/dev/null; then
  echo "CI base $base_ref is unavailable; checkout must use fetch-depth: 0" >&2
  exit 1
fi

MERGE_READINESS_BASE_REF=$base_ref scripts/check-merge-readiness.sh --no-fetch
