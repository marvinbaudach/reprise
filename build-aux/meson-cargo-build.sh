#!/bin/sh
set -eu

source_root=$1
build_root=$2
output=$3
localedir=$4
stem_backend=${5:-false}
prefix=${6:-/usr/local}
worker_path=${7:-"$prefix/libexec/reprise-worker"}
profile=${MESON_BUILD_PROFILE:-release}

if [ "$profile" = debug ]; then
  cargo_profile_args=
else
  cargo_profile_args=--release
fi

# The experimental stem worker is a Meson/devel build choice
# (-Dstem_backend, default on). The GTK feature only enables its client surface;
# reprise-stems and ONNX Runtime live exclusively in the separately built
# reprise-worker process.
if [ "$stem_backend" = true ]; then
  cargo_feature_args="--features stem-backend"
else
  cargo_feature_args=
fi

set -- env \
  CARGO_TARGET_DIR="$build_root/cargo-target" \
  GETTEXT_PACKAGE=reprise \
  LOCALEDIR="$localedir"

if [ "$stem_backend" = true ]; then
  set -- "$@" REPRISE_INSTRUMENTAL_WORKER="$worker_path"
  bundled_ort="$prefix/lib/reprise/libonnxruntime.so.1.22.0"
  if [ -f "$bundled_ort" ]; then
    bundled_ort_sha256=$(sha256sum "$bundled_ort" | cut -d' ' -f1)
    set -- "$@" \
      REPRISE_BUNDLED_ORT_DYLIB="$bundled_ort" \
      REPRISE_BUNDLED_ORT_DYLIB_SHA256="$bundled_ort_sha256"
  fi
fi

"$@" cargo build $cargo_profile_args $cargo_feature_args \
  -p reprise-gnome --manifest-path "$source_root/Cargo.toml"

cp "$build_root/cargo-target/$profile/reprise" "$output"
