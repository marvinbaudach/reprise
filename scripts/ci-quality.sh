#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

base_branch=${GITHUB_BASE_REF:-${GITHUB_REF_NAME:-main}}
head_branch=${GITHUB_HEAD_REF:-${GITHUB_REF_NAME:-}}

if [[ ${GITHUB_EVENT_NAME:-} == pull_request && $base_branch == main && $head_branch != dev ]]; then
  echo "main accepts promotion pull requests from dev only" >&2
  exit 1
fi

base_ref="origin/$base_branch"
if ! git rev-parse --verify --quiet "$base_ref^{commit}" >/dev/null; then
  echo "CI base $base_ref is unavailable; checkout must use fetch-depth: 0" >&2
  exit 1
fi

MERGE_READINESS_BASE_REF=$base_ref scripts/check-merge-readiness.sh --no-fetch
