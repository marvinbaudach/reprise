#!/bin/sh
set -eu

source_root=$1
build_root=$2
output=$3
localedir=$4
profile=${MESON_BUILD_PROFILE:-release}

if [ "$profile" = debug ]; then
  cargo_profile_args=
else
  cargo_profile_args=--release
fi

env \
  CARGO_TARGET_DIR="$build_root/cargo-target" \
  GETTEXT_PACKAGE=reprise \
  LOCALEDIR="$localedir" \
  cargo build $cargo_profile_args --manifest-path "$source_root/Cargo.toml"

cp "$build_root/cargo-target/$profile/reprise" "$output"

