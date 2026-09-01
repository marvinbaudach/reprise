#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

project=0
showroom=0
android=0

if (( $# == 0 )); then
  project=1
  showroom=1
  android=1
else
  for selection in "$@"; do
    case "$selection" in
      --project) project=1 ;;
      --showroom) showroom=1 ;;
      --android) android=1 ;;
      *)
        echo "usage: $0 [--project] [--showroom] [--android]" >&2
        exit 64
        ;;
    esac
  done
fi

require_tool() {
  local tool=$1
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required for project source-quality checks" >&2
    exit 1
  fi
}

require_tool npm

if (( project != 0 )); then
  require_tool uvx
  if command -v reuse >/dev/null 2>&1; then
    echo "== REUSE licensing compliance =="
    reuse lint
  else
    echo "reuse is not installed; skipping REUSE licensing compliance" >&2
  fi
  echo "== Python, YAML and Markdown source quality =="
  npm --prefix quality ci
  npm --prefix quality run lint
  npm --prefix quality test
fi

if (( showroom != 0 )); then
  require_tool node
  echo "== Showroom source quality =="
  npm --prefix showroom ci
  npm --prefix showroom run lint
  # package.json has carried a `typecheck` script that no gate called. The
  # TypeScript 5.9 → 7.0 bump went through on that blind spot; a type error
  # would only have surfaced as a build failure, or not at all.
  npm --prefix showroom run typecheck
  node --test showroom/tests/lint-contract.test.mjs
  # The lint contract runs first because it needs no build and may fail fast.
  # The full suite reads dist/, so `npm test` builds before it asserts.
  npm --prefix showroom test
fi

if (( android != 0 )); then
  echo "== Android source quality =="
  npm --prefix android run lint
  npm --prefix android run test:lint
fi

echo "Project source-quality checks passed"
