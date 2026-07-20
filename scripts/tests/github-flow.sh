#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

require_file() {
  local path=$1
  [[ -f $path ]] || { echo "$path must exist" >&2; exit 1; }
}

require_executable() {
  local path=$1
  [[ -x $path ]] || { echo "$path must exist and be executable" >&2; exit 1; }
}

require_pattern() {
  local pattern=$1
  local path=$2
  rg --quiet "$pattern" "$path" || {
    echo "$path must contain policy pattern: $pattern" >&2
    exit 1
  }
}

workflow=.github/workflows/ci.yml
guide=docs/agents/branching.md

require_file "$workflow"
require_file "$guide"
require_executable scripts/ci-quality.sh

require_pattern '^name: CI$' "$workflow"
require_pattern '^  pull_request:$' "$workflow"
rg --quiet --multiline '^  pull_request:\n  push:$' "$workflow" || {
  echo "$workflow must run for every pull request without a branch filter" >&2
  exit 1
}
require_pattern '^      - main$' "$workflow"
require_pattern '^      - dev$' "$workflow"
require_pattern '^  contents: read$' "$workflow"
require_pattern '^    name: Quality gate$' "$workflow"
require_pattern 'archlinux:latest' "$workflow"
require_pattern 'scripts/ci-quality.sh' "$workflow"

require_pattern 'GITHUB_EVENT_NAME' scripts/ci-quality.sh
require_pattern 'git config --global --add safe.directory' scripts/ci-quality.sh
require_pattern 'GITHUB_BASE_REF' scripts/ci-quality.sh
require_pattern 'GITHUB_HEAD_REF' scripts/ci-quality.sh
require_pattern 'git cat-file -e origin/main:.github/workflows/ci.yml' scripts/ci-quality.sh
require_pattern 'head_branch != hotfix/\\*' scripts/ci-quality.sh
require_pattern 'MERGE_READINESS_BASE_REF' scripts/ci-quality.sh
require_pattern 'check-merge-readiness.sh --no-fetch' scripts/ci-quality.sh
require_pattern '^main <- dev <- feature/' "$guide"
require_pattern 'CI / Quality gate' "$guide"
require_pattern 'Every pull request runs' "$guide"
require_pattern 'Direct pushes to `main`' "$guide"
require_pattern 'hotfix/\\*' "$guide"
require_pattern 'hotfix/' AGENTS.md
require_pattern 'package-ecosystem: github-actions' .github/dependabot.yml
if [[ $(rg -c '^    target-branch: dev$' .github/dependabot.yml) -ne 2 ]]; then
  echo ".github/dependabot.yml must target dev for every update stream" >&2
  exit 1
fi

echo "GitHub flow policy checks passed"
