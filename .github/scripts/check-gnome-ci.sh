#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

if [[ ${CI:-} == true ]]; then
    git config --global --add safe.directory "$repo_root"
fi

# scripts/check-architecture.sh is repo-wide - it caps every Rust file and
# resolves every documentation path cited from crates/ and scripts/ - so it
# runs in the base-contracts job, which is not routed by changed paths.
echo "== GNOME contracts =="
scripts/check-accessibility-semantics.sh
scripts/check-input-parity.sh
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
scripts/check-appstream.sh
scripts/check-flatpak-manifest.sh
scripts/check-gnome-idioms.sh
scripts/check-ai-hygiene.sh
scripts/check-motion-tokens.sh

echo "== GNOME formatting and lint =="
cargo fmt --check
cargo clippy --locked --all-targets \
    -p reprise-view -p reprise-platform-linux -p reprise-gnome -- -D warnings

echo "== GNOME documentation =="
env RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps \
    -p reprise-view -p reprise-platform-linux -p reprise-gnome

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

echo "== GNOME and shared-view tests =="
env XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
    REPRISE_AUDIO_SINK=fakesink \
    cargo test --locked -p reprise-view -p reprise-gnome

echo "== Linux platform tests (serialized GStreamer) =="
env XDG_DATA_HOME="$tmp_root/data" XDG_CACHE_HOME="$tmp_root/cache" \
    REPRISE_AUDIO_SINK=fakesink \
    cargo test --locked -p reprise-platform-linux -- --test-threads=1

echo "GNOME quality checks passed"
