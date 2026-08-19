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
  case "$base_ref" in
    origin/*) base_branch=${base_ref#origin/} ;;
    *)
      echo "merge-readiness can refresh only an origin/<branch> base; use --no-fetch for local refs" >&2
      exit 2
      ;;
  esac
  echo "== Refresh $base_ref =="
  git fetch --quiet origin \
    "refs/heads/${base_branch}:refs/remotes/origin/${base_branch}"
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

# Every check below runs through `gate`, so the wall in the showroom's chapter
# two can derive its cells from this file with one anchored expression instead
# of guessing which lines are checks. The preparation steps above stay outside
# it deliberately: they are preconditions, not checks, and counting them would
# inflate the number the page shows.
gate() {
  local name=$1
  shift
  if [[ ${1:-} == -- ]]; then
    shift
  fi
  echo "== $name =="
  "$@"
}

# The two checks that are not a single command each get a wrapper, so every
# caller stays one `gate` line and the derivation keeps working.
quality_cmd=(scripts/check-project-quality.sh)
# The core-suite container has neither Java nor an Android UniFFI bindgen step.
# base-contracts covers --project --showroom; android-unit-suite covers --android.
case "${MERGE_READINESS_SKIP_ANDROID_QUALITY:-}" in
  1 | true)
    echo "Skipping the Android area here; it runs in the android-unit-suite job."
    quality_cmd=(scripts/check-project-quality.sh --project --showroom)
    ;;
esac

run_audit() {
  if ! cargo audit; then
    echo "live advisory refresh unavailable; checking the cached database" >&2
    cargo audit --no-fetch
  fi
}

gate "Branch diff" -- git diff --check "$base_ref"...HEAD
gate "Shell" -- scripts/check-shell.sh
gate "Project quality" -- "${quality_cmd[@]}"
gate "Worktree GC" -- scripts/tests/worktree-gc.sh
gate "Worktree GC schedule" -- scripts/tests/worktree-gc-schedule.sh
gate "Gettext catalogues" -- scripts/tests/gettext-catalogs.sh
gate "Architecture" -- scripts/check-architecture.sh
gate "Device-sync GStreamer" -- scripts/check-device-sync-gstreamer.sh
gate "Accessibility semantics" -- scripts/check-accessibility-semantics.sh
gate "Input parity" -- scripts/check-input-parity.sh
gate "Runtime service install" -- scripts/check-runtime-service-install.sh
gate "Frontend thinness" -- scripts/check-frontend-thinness.sh
gate "UX traceability" -- scripts/check-ux-traceability.sh
gate "AppStream" -- scripts/check-appstream.sh
gate "Flatpak manifest" -- scripts/check-flatpak-manifest.sh
gate "GNOME idioms" -- scripts/check-gnome-idioms.sh
gate "AI hygiene" -- scripts/check-ai-hygiene.sh
gate "Motion tokens" -- scripts/check-motion-tokens.sh
gate "Rust formatting" -- cargo fmt --check
gate "Rust lint" -- cargo clippy --locked --all-targets --workspace -- -D warnings
gate "Rust documentation" -- env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

gate "Workspace tests" -- env XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
  REPRISE_AUDIO_SINK=fakesink \
  cargo test --locked --workspace --exclude reprise-platform-linux

# GStreamer pipelines and their GLib main-context work share process-global
# state. Running the Linux backend tests in parallel can leave one test inside
# a pipeline state transition while the stream-generation tests wait on their
# shared audio-sink lock. Keep only this package serial; the rest of the
# workspace still uses Cargo's normal parallelism.
gate "Linux platform tests" -- env XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
  REPRISE_AUDIO_SINK=fakesink \
  cargo test --locked -p reprise-platform-linux -- --test-threads=1

gate "Rule-owned display tests" -- scripts/check-display-tests.sh --rule-named

# The runtime service's own tests need a session bus. A private one, so they
# never touch the developer's running Reprise.
gate "Runtime service bus tests" -- dbus-run-session -- cargo test --locked -p reprise-platform-linux \
  --test runtime_service -- --ignored --test-threads=1

gate "Dependency audit" -- run_audit

echo "Merge-readiness checks passed against $base_ref"
