#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# The GTK process may link the narrow model-provisioning slice, but even an
# all-features build may not link the native inference runtime. Otherwise an
# ORT crash can still terminate the music player.
gnome_tree=$(cargo tree -p reprise-gnome --all-features -e normal --prefix none)
if printf '%s\n' "$gnome_tree" | rg --quiet '^ort '; then
  echo "reprise-gnome must not link ort; rendering belongs in reprise-worker" >&2
  exit 1
fi

# The release build must produce and install a dedicated worker executable.
rg --quiet "'reprise-worker'" meson.build
rg --quiet "install_dir: get_option\\('libexecdir'\\)" meson.build
rg --quiet 'reprise-cli.*--features worker|--features worker.*reprise-cli' \
  build-aux/meson-cargo-worker-build.sh
rg --quiet 'REPRISE_INSTRUMENTAL_WORKER' build-aux/meson-cargo-build.sh
rg --quiet 'check-packaged-instrumental-e2e.sh' scripts/check-release.sh

# No backend implementation or render loop may remain in the GTK host.
rg --quiet 'use std::process.*Command' \
  crates/reprise-gnome/src/ui/instrumental/worker_host.rs
if rg --quiet 'StemSeparationBackend|run_claimed_job|run_next_job|OrtStemBackend' \
  crates/reprise-gnome/src/ui/instrumental; then
  echo "GTK instrumental code still contains in-process rendering" >&2
  exit 1
fi
