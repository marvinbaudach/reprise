#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

require_executable() {
  local path=$1
  if [[ ! -x "$path" ]]; then
    echo "$path must exist and be executable" >&2
    exit 1
  fi
}

require_pattern() {
  local pattern=$1
  local path=$2
  if ! rg --quiet "$pattern" "$path"; then
    echo "$path must contain policy pattern: $pattern" >&2
    exit 1
  fi
}

require_executable scripts/check-architecture.sh
require_executable scripts/check-motion-tokens.sh
require_executable scripts/check-merge-readiness.sh
require_executable scripts/install-git-hooks.sh
require_executable scripts/cua-e2e/run.sh
require_executable scripts/tests/cua-e2e.sh
require_executable scripts/tests/motion-tokens.sh
require_executable .githooks/pre-push

require_pattern 'merge-base --is-ancestor' scripts/check-merge-readiness.sh
require_pattern 'status --porcelain' scripts/check-merge-readiness.sh
require_pattern 'cargo fmt --check' scripts/check-merge-readiness.sh
require_pattern 'cargo clippy --locked --all-targets --workspace -- -D warnings' scripts/check-merge-readiness.sh
require_pattern 'cargo test --locked --workspace' scripts/check-merge-readiness.sh
require_pattern 'cargo audit' scripts/check-merge-readiness.sh
require_pattern 'check-architecture.sh' scripts/check-merge-readiness.sh
require_pattern 'check-motion-tokens.sh' scripts/check-merge-readiness.sh
require_pattern 'Frontend lint' scripts/check-architecture.sh
require_pattern 'composition root must stay below 600' scripts/check-architecture.sh
require_pattern 'UI orchestrators must stay below 600' scripts/check-architecture.sh
require_pattern 'must declare feature modules instead of flattening feature directories' scripts/check-architecture.sh
require_pattern 'must own an explicit mod.rs surface' scripts/check-architecture.sh
require_pattern 'gtk4::CssProvider::new' scripts/check-architecture.sh
require_pattern 'style_context' scripts/check-architecture.sh
require_pattern 'reqwest::blocking' scripts/check-architecture.sh
require_pattern 'gst-launch-1\\.0' scripts/check-architecture.sh
require_pattern 'must not depend directly on GStreamer' scripts/check-architecture.sh
require_pattern 'must receive platform backends through core contracts' scripts/check-architecture.sh
require_pattern 'productive GNOME code must use core database facades' scripts/check-architecture.sh
require_pattern 'frontend workers must open ready-to-use databases through the core facade' scripts/check-architecture.sh
require_pattern 'must use the shared one-shot task helper' scripts/check-architecture.sh
require_pattern 'check-merge-readiness.sh' .githooks/pre-push
require_pattern 'core.hooksPath .githooks' scripts/install-git-hooks.sh
require_pattern '^## Current automated baseline' TESTING.md
require_pattern '^## Required merge gates' TESTING.md
require_pattern '^## Priority automation gaps' TESTING.md
require_pattern '^## Isolated GTK and desktop tests' TESTING.md
require_pattern '^## Manual release checks' TESTING.md
require_pattern '^## Known harness constraints' TESTING.md

scripts/tests/cua-e2e.sh
scripts/tests/motion-tokens.sh
scripts/check-architecture.sh

echo "QA linter policy checks passed"
