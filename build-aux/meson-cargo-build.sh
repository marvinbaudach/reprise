#!/bin/sh
set -eu

source_root=$1
build_root=$2
output=$3
localedir=$4
stem_backend=${5:-false}
prefix=${6:-/usr/local}
profile=${MESON_BUILD_PROFILE:-release}

if [ "$profile" = debug ]; then
  cargo_profile_args=
else
  cargo_profile_args=--release
fi

# The experimental stem-separation backend is a meson/devel build choice
# (-Dstem_backend, default on). The bare `cargo build` that
# scripts/check-architecture.sh probes passes no features and stays core-only;
# only this build opts reprise-gnome into --features stem-backend, so the
# default-build dependency probe keeps passing. --features needs an explicit
# package because the workspace root is virtual.
if [ "$stem_backend" = true ]; then
  cargo_feature_args="--features stem-backend"
else
  cargo_feature_args=
fi

set -- env \
  CARGO_TARGET_DIR="$build_root/cargo-target" \
  GETTEXT_PACKAGE=reprise \
  LOCALEDIR="$localedir"

bundled_ort="$prefix/lib/reprise/libonnxruntime.so.1.22.0"
if [ "$stem_backend" = true ] && [ -f "$bundled_ort" ]; then
  bundled_ort_sha256=$(sha256sum "$bundled_ort" | cut -d' ' -f1)
  set -- "$@" \
    REPRISE_BUNDLED_ORT_DYLIB="$bundled_ort" \
    REPRISE_BUNDLED_ORT_DYLIB_SHA256="$bundled_ort_sha256"
fi

"$@" cargo build $cargo_profile_args $cargo_feature_args \
  -p reprise-gnome --manifest-path "$source_root/Cargo.toml"

cp "$build_root/cargo-target/$profile/reprise" "$output"
