#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

fetch_main=1
case "${1:-}" in
  "") ;;
  --no-fetch) fetch_main=0 ;;
  *)
    echo "Usage: $0 [--no-fetch]" >&2
    exit 2
    ;;
esac

base_ref=${MERGE_READINESS_BASE_REF:-origin/main}

if (( fetch_main != 0 )); then
  echo "== Refresh $base_ref =="
  git fetch --quiet origin main
fi

if ! git rev-parse --verify --quiet "$base_ref^{commit}" >/dev/null; then
  echo "merge-readiness base $base_ref does not exist" >&2
  exit 1
fi

if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "merge-readiness requires a clean worktree, including untracked files" >&2
  git status --short >&2
  exit 1
fi

if ! git merge-base --is-ancestor "$base_ref" HEAD; then
  echo "branch is stale: merge or rebase the latest $base_ref before pushing" >&2
  exit 1
fi

echo "== Branch diff =="
git diff --check "$base_ref"...HEAD

scripts/check-architecture.sh

echo "== UX traceability =="
scripts/check-ux-traceability.sh

echo "== Motion tokens =="
scripts/check-motion-tokens.sh

echo "== Rust formatting =="
cargo fmt --check

echo "== Rust lint =="
cargo clippy --locked --all-targets --workspace -- -D warnings

echo "== Rust documentation =="
env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

echo "== Workspace tests =="
env XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
  cargo test --locked --workspace

echo "== Rule-named display tests =="
scripts/check-display-tests.sh --rule-named

echo "== Dependency audit =="
cargo audit

echo "Merge-readiness checks passed against $base_ref"
