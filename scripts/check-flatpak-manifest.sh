#!/usr/bin/env bash
# GP-14: the Flatpak manifest passes flatpak-builder-lint.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

manifest=$(find . -maxdepth 1 -name '*.Reprise.yml' -o -maxdepth 1 -name '*.Reprise.yaml' \
  -o -maxdepth 1 -name '*.Reprise.json' | head -1)
[[ -n $manifest ]] || { echo "ERROR: no Flatpak manifest in the repository root" >&2; exit 1; }

# flatpak-builder-lint ships inside the org.flatpak.Builder flatpak. Prefer
# the flatpak-provided one, fall back to a native binary.
if flatpak info org.flatpak.Builder >/dev/null 2>&1; then
  lint=(flatpak run --command=flatpak-builder-lint org.flatpak.Builder)
elif command -v flatpak-builder-lint >/dev/null 2>&1; then
  lint=(flatpak-builder-lint)
else
  echo "ERROR: flatpak-builder-lint is required by this gate." >&2
  echo "       Install it with: flatpak install -y flathub org.flatpak.Builder" >&2
  exit 1
fi

if ! output=$("${lint[@]}" manifest "$manifest" 2>&1); then
  report_violation GP-14 "flatpak-builder-lint manifest failed on $manifest:
$output"
fi

rulebook_exit
