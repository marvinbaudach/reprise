#!/bin/sh
# Builds the activatable runtime service (plan §9.4). Kept beside the app and
# worker scripts rather than folded into them: the three binaries share a
# cargo target directory but nothing else, and a single script taking a
# package name would hide which build profile and features each one needs.
set -eu

source_root=$1
build_root=$2
output=$3
profile=${MESON_BUILD_PROFILE:-release}

if [ "$profile" = debug ]; then
  cargo_profile_args=
else
  cargo_profile_args=--release
fi

env CARGO_TARGET_DIR="$build_root/cargo-target" \
  cargo build $cargo_profile_args \
  -p reprise-platform-linux --bin reprise-runtime \
  --manifest-path "$source_root/Cargo.toml"

cp "$build_root/cargo-target/$profile/reprise-runtime" "$output"
