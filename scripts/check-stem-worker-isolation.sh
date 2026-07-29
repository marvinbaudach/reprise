#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# The GTK process must not link either the stem crate or the native inference
# runtime. Instrumental rendering belongs exclusively in the worker.
gnome_tree=$(cargo tree -p reprise-gnome --all-features -e normal --prefix none)
if printf '%s\n' "$gnome_tree" | rg --quiet '^(reprise-stems|ort) '; then
  echo "reprise-gnome must not link reprise-stems or ort; rendering belongs in reprise-worker" >&2
  exit 1
fi

# The release build must produce and install a dedicated worker executable.
rg --quiet "'reprise-worker'" meson.build
rg --quiet "install_dir: get_option\\('libexecdir'\\)" meson.build
rg --quiet 'reprise-cli.*--features worker|--features worker.*reprise-cli' \
  build-aux/meson-cargo-worker-build.sh
rg --quiet 'check-packaged-instrumental-e2e.sh' scripts/check-release.sh

# The regular GTK build must not opt back into the removed frontend feature.
if rg --quiet -- '--features stem-backend|REPRISE_INSTRUMENTAL_WORKER' \
  build-aux/meson-cargo-build.sh; then
  echo "GTK build still enables the removed instrumental frontend" >&2
  exit 1
fi
