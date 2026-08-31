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

reject_pattern() {
  local pattern=$1
  local path=$2
  if rg --quiet -- "$pattern" "$path"; then
    echo "$path must not contain obsolete policy pattern: $pattern" >&2
    exit 1
  fi
}

require_pattern_order() {
  local before=$1
  local after=$2
  local path=$3
  local before_line after_line
  before_line=$(rg --line-number --max-count 1 "$before" "$path" | cut -d: -f1 || true)
  after_line=$(rg --line-number --max-count 1 "$after" "$path" | cut -d: -f1 || true)
  if [[ -z $before_line || -z $after_line || $before_line -ge $after_line ]]; then
    echo "$path must place $before before $after" >&2
    exit 1
  fi
}

require_executable scripts/check-architecture.sh
require_executable scripts/check-shell.sh
require_executable scripts/check-frontend-thinness.sh
require_executable scripts/check-accessibility-semantics.sh
require_executable scripts/check-input-parity.sh
require_executable scripts/check-motion-tokens.sh
require_executable scripts/check-android-theme.sh
require_executable scripts/check-merge-readiness.sh
require_executable scripts/check-project-quality.sh
require_executable scripts/check-flatpak-cargo-sources.sh
require_executable scripts/check-release-metadata.sh
require_executable scripts/install-git-hooks.sh
require_executable scripts/performance-baseline.sh
require_executable scripts/performance-compare.sh
require_executable scripts/performance-query-compare.sh
require_executable scripts/performance-runtime-baseline.sh
require_executable scripts/cua-e2e/run.sh
require_executable scripts/tests/cua-e2e.sh
require_executable scripts/tests/motion-tokens.sh
require_executable scripts/tests/performance-baseline.sh
require_executable scripts/tests/performance-compare.sh
require_executable scripts/tests/performance-query-compare.sh
require_executable scripts/tests/performance-runtime-baseline.sh
require_executable scripts/tests/readme-showcase.sh
require_executable scripts/tests/accessibility-semantics.sh
require_executable scripts/tests/input-parity.sh
require_executable scripts/tests/android-theme.sh
require_executable scripts/tests/msrv.sh
require_executable scripts/tests/github-flow.sh
require_executable .github/tests/flatpak-cargo-sources.sh
require_executable scripts/tests/project-quality.sh
require_executable scripts/tests/weekly-portfolio-sync.sh
require_executable scripts/weekly-portfolio-sync.sh
require_executable scripts/tests/worktree-gc.sh
require_executable scripts/tests/worktree-gc-schedule.sh
require_executable scripts/tests/architecture-size-limits.sh
require_executable scripts/tests/cua-explore.sh
require_executable scripts/tests/check-android-suite.sh
require_executable .github/tests/release-metadata.sh
require_executable .github/tests/release-workflow.sh
require_executable scripts/reprise-worktree-gc.sh
require_executable scripts/close-worktree.sh
require_executable scripts/install-worktree-gc-timer.sh
require_executable .githooks/pre-push

require_pattern 'merge-base --is-ancestor' scripts/check-merge-readiness.sh
require_pattern 'status --porcelain' scripts/check-merge-readiness.sh
require_pattern 'base_branch=\$\{base_ref#origin/\}' scripts/check-merge-readiness.sh
require_pattern 'refs/heads/\$\{base_branch\}:refs/remotes/origin/\$\{base_branch\}' scripts/check-merge-readiness.sh
require_pattern 'cargo fmt --check' scripts/check-merge-readiness.sh
require_pattern 'cargo clippy --locked --all-targets --workspace -- -D warnings' scripts/check-merge-readiness.sh
require_pattern 'cargo test --locked --workspace' scripts/check-merge-readiness.sh
require_pattern 'cargo audit' scripts/check-merge-readiness.sh
require_pattern 'check-shell.sh' scripts/check-merge-readiness.sh
require_pattern '^skipped_here=\(\)$' scripts/check-merge-readiness.sh
require_pattern '^is_skipped\(\) \{$' scripts/check-merge-readiness.sh
require_pattern 'MERGE_READINESS_SKIP_GATES' scripts/check-merge-readiness.sh
require_pattern 'skipped_summary\+=", \$name"' scripts/check-merge-readiness.sh
require_pattern 'echo "Skipped here, covered by another CI job: \$skipped_summary"' scripts/check-merge-readiness.sh
require_pattern 'MERGE_READINESS_SKIP_GATES' scripts/ci-quality.sh
# Both calls moved into the `gate "<name>" -- <command>` form, so neither path
# starts its own line any more. The assertion follows the call rather than the
# layout: the gate must still name the project-quality wrapper, and it must
# still hand `--rule-named` to the display suite instead of running all of it.
require_pattern 'quality_cmd=\(scripts/check-project-quality\.sh' scripts/check-merge-readiness.sh
# Naming the wrapper is not the same as running it. Without this second line the
# gate call could be deleted and the assignment left behind as dead code, and
# project quality would stop running with every assertion still green.
require_pattern 'gate "Project quality" -- "\$\{quality_cmd\[@\]\}"' scripts/check-merge-readiness.sh
require_pattern 'worktree-gc.sh' scripts/check-merge-readiness.sh
require_pattern 'worktree-gc-schedule.sh' scripts/check-merge-readiness.sh
require_pattern 'shellcheck' .github/workflows/ci.yml
require_pattern 'check-architecture.sh' scripts/check-merge-readiness.sh
require_pattern 'check-accessibility-semantics.sh' scripts/check-merge-readiness.sh
require_pattern 'check-input-parity.sh' scripts/check-merge-readiness.sh
require_pattern 'scripts/tests/msrv.sh' scripts/check-release.sh
require_pattern '^scripts/check-flatpak-cargo-sources\.sh$' scripts/check-release.sh
require_pattern '^scripts/check-release-metadata\.sh$' scripts/check-release.sh
require_pattern 'scripts/check-release-metadata\.sh --gate' .github/workflows/ci.yml
require_pattern 'scripts/check-flatpak-cargo-sources\.sh' .github/workflows/ci.yml
require_pattern 'Verify worktree hygiene' .github/workflows/ci.yml
require_pattern 'scripts/tests/worktree-gc\.sh' .github/workflows/ci.yml
require_pattern 'scripts/tests/worktree-gc-schedule\.sh' .github/workflows/ci.yml
require_pattern 'Run the script self-tests' .github/workflows/ci.yml
require_pattern 'scripts/tests/qa-linters\.sh' .github/workflows/ci.yml
require_pattern '^          scripts/check-shell\.sh$' .github/workflows/ci.yml
require_pattern '^        run: scripts/check-project-quality\.sh --project --showroom$' .github/workflows/ci.yml
require_pattern '^          scripts/check-architecture\.sh$' .github/workflows/ci.yml
require_pattern_order 'Verify worktree hygiene' 'Verify project source quality' .github/workflows/ci.yml
require_pattern_order 'Run the script self-tests' 'Verify project source quality' .github/workflows/ci.yml
require_pattern_order 'Verify repository and workflow contracts' 'Verify project source quality' .github/workflows/ci.yml
require_pattern 'check-motion-tokens.sh' scripts/check-merge-readiness.sh
require_pattern 'scripts/check-display-tests\.sh --rule-named$' scripts/check-merge-readiness.sh
reject_pattern 'scripts/check-display-tests\.sh$' scripts/check-merge-readiness.sh
reject_pattern '--motion' scripts/check-display-tests.sh
require_pattern 'mode=css' scripts/check-display-tests.sh
require_pattern 'display_test_passed' scripts/check-display-tests.sh
require_pattern 'passed_lines=\$\(grep -Ec' scripts/check-display-tests.sh
require_pattern 'DISPLAY_TEST_JOBS' scripts/check-display-tests.sh
require_pattern 'wait -n' scripts/check-display-tests.sh
require_pattern 'results_dir' scripts/check-display-tests.sh
require_pattern 'XDG_RUNTIME_DIR' scripts/check-display-tests.sh
require_pattern 'XDG_CONFIG_HOME' scripts/check-display-tests.sh
require_pattern 'GIO_USE_VFS=local' scripts/check-display-tests.sh
require_pattern 'GTK_USE_PORTAL=0' scripts/check-display-tests.sh
require_pattern 'GSK_RENDERER=cairo' scripts/check-display-tests.sh
require_pattern 'cleanup_worker_roots' scripts/check-display-tests.sh
require_pattern 'if \[\[ -f \$display_test_passed \]\]' scripts/check-display-tests.sh
require_pattern 'server-num' scripts/check-display-tests.sh
require_pattern_order 'if env' 'dbus-run-session -- xvfb-run' scripts/check-display-tests.sh
require_pattern 'DISPLAY_TEST_JOBS: 1' .github/workflows/ci.yml
require_pattern 'Frontend lint' scripts/check-architecture.sh
require_pattern 'cargo machete --with-metadata' scripts/check-frontend-thinness.sh
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
require_pattern 'check-accessibility-semantics.sh' scripts/check-architecture.sh
require_pattern 'check-input-parity.sh' scripts/check-architecture.sh
require_pattern 'check-android-theme.sh' scripts/check-architecture.sh
require_pattern 'check-merge-readiness.sh' .githooks/pre-push
require_pattern 'core.hooksPath .githooks' scripts/install-git-hooks.sh
require_pattern '^## Current automated baseline' TESTING.md
require_pattern '^## Required merge gates' TESTING.md
require_pattern '^## Priority automation gaps' TESTING.md
require_pattern '^## Isolated GTK and desktop tests' TESTING.md
require_pattern '^## Manual release checks' TESTING.md
require_pattern '^## Known harness constraints' TESTING.md
require_pattern 'must run the entire discovered list' RELEASING.md
require_pattern '1 passed' RELEASING.md
require_pattern 'zero tests' RELEASING.md
require_pattern 'keytool -genkeypair' docs/releasing-android.md
require_pattern 'upload-key-sha256\.txt' docs/releasing-android.md
require_pattern 'ANDROID_KEYSTORE_BASE64' docs/releasing-android.md
require_pattern 'ANDROID_KEYSTORE_PASSWORD' docs/releasing-android.md
require_pattern 'ANDROID_KEY_ALIAS' docs/releasing-android.md
require_pattern 'ANDROID_KEY_PASSWORD' docs/releasing-android.md
require_pattern 'android/keystore\.properties' docs/releasing-android.md
require_pattern 'cannot update' docs/releasing-android.md
require_pattern '^## Automated GitHub release channel' RELEASING.md
require_pattern 'push to `main`' RELEASING.md
require_pattern 'exact commit SHA' RELEASING.md
require_pattern 'Reprise-<desktop-version>\.flatpak' RELEASING.md
require_pattern 'Reprise-Android-<android-version>\.apk' RELEASING.md
require_pattern 'must exist before promotion' RELEASING.md
require_pattern 'docs/releasing-android\.md' RELEASING.md
require_pattern 'self-hosted Flatpak repository' RELEASING.md
require_pattern 'Showroom' RELEASING.md

scripts/tests/cua-e2e.sh
scripts/tests/motion-tokens.sh
scripts/tests/performance-baseline.sh
scripts/tests/performance-compare.sh
scripts/tests/performance-query-compare.sh
scripts/tests/performance-runtime-baseline.sh
scripts/tests/readme-showcase.sh
scripts/tests/accessibility-semantics.sh
scripts/tests/input-parity.sh
scripts/tests/android-theme.sh
scripts/tests/github-flow.sh
.github/tests/flatpak-cargo-sources.sh
scripts/tests/project-quality.sh
scripts/tests/weekly-portfolio-sync.sh
scripts/tests/worktree-gc.sh
scripts/tests/worktree-gc-schedule.sh
# These three had no caller at all — not here, not in CI, not in the merge gate.
# They were written, they pass, and nothing ever ran them.
scripts/tests/architecture-size-limits.sh
scripts/tests/cua-explore.sh
scripts/tests/check-android-suite.sh
scripts/check-architecture.sh

echo "QA linter policy checks passed"
