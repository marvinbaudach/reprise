#!/bin/sh
set -eu

source_root=$1
build_root=$2
output=$3
prefix=${4:-/usr/local}
profile=${MESON_BUILD_PROFILE:-release}

if [ "$profile" = debug ]; then
  cargo_profile_args=
else
  cargo_profile_args=--release
fi

set -- env CARGO_TARGET_DIR="$build_root/cargo-target"

bundled_ort="$prefix/lib/reprise/libonnxruntime.so.1.22.0"
if [ -f "$bundled_ort" ]; then
  bundled_ort_sha256=$(sha256sum "$bundled_ort" | cut -d' ' -f1)
  set -- "$@" \
    REPRISE_BUNDLED_ORT_DYLIB="$bundled_ort" \
    REPRISE_BUNDLED_ORT_DYLIB_SHA256="$bundled_ort_sha256"
fi

"$@" cargo build $cargo_profile_args -p reprise-cli --features worker \
  --manifest-path "$source_root/Cargo.toml"

cp "$build_root/cargo-target/$profile/reprise-cli" "$output"
