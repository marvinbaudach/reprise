#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

python3 - <<'PY'
from pathlib import Path

import yaml

manifest = yaml.safe_load(Path("io.github.marvinbaudach.Reprise.yml").read_text(encoding="utf-8"))
modules = manifest["modules"]
runtime_index = next(
    (index for index, module in enumerate(modules) if module.get("name") == "onnxruntime"),
    None,
)
app_index = next(
    (index for index, module in enumerate(modules) if module.get("name") == "reprise"),
    None,
)

assert runtime_index is not None, "Flatpak must package an onnxruntime module"
assert app_index is not None, "Flatpak must package the Reprise module"
assert runtime_index < app_index, "onnxruntime must be installed before Reprise is built"

runtime = modules[runtime_index]
assert runtime["buildsystem"] == "simple"
assert runtime["only-arches"] == ["x86_64"]
assert runtime["license-files"] == ["LICENSE", "ThirdPartyNotices.txt"]

source = runtime["sources"][0]
assert source == {
    "type": "archive",
    "url": (
        "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/"
        "onnxruntime-linux-x64-1.22.0.tgz"
    ),
    "sha512": (
        "c49d927a39dc27fcdf3b41436806af74c24c79ead09289d986c359fc1380ea36"
        "3cf83d4085212b8972cb752a0fa8b9b1a06b82ad19e2d4dd6e22e44c79050386"
    ),
}

commands = "\n".join(runtime["build-commands"])
assert "libonnxruntime.so.1.22.0" in commands
assert "/app/lib/reprise" in commands
assert "/app/share/licenses/io.github.marvinbaudach.Reprise/onnxruntime" in commands
PY

# Only the worker build passes the bundled runtime through. The GTK build is
# deliberately NOT checked for it: `feat(gnome): remove instrumental frontend`
# took the instrumental surface out of the frontend, so `reprise-gnome` links
# neither `reprise-stems` nor `ort` and has nothing to point at a dylib with.
# `scripts/check-stem-worker-isolation.sh` enforces exactly that separation.
# Requiring the marker in `build-aux/meson-cargo-build.sh` would demand the
# coupling its sibling gate forbids, so do not add it back — the two checks
# would contradict each other and this one would fail on every commit.
rg --quiet 'REPRISE_BUNDLED_ORT_DYLIB' build-aux/meson-cargo-worker-build.sh
rg --quiet 'REPRISE_BUNDLED_ORT_DYLIB_SHA256' build-aux/meson-cargo-worker-build.sh
rg --quiet 'option_env!\("REPRISE_BUNDLED_ORT_DYLIB"\)' \
  crates/reprise-stems/src/provision.rs
rg --quiet 'option_env!\("REPRISE_BUNDLED_ORT_DYLIB_SHA256"\)' \
  crates/reprise-stems/src/provision.rs
