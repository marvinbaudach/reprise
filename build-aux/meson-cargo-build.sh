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

# The GTK frontend no longer carries an instrumental client surface, so it
# builds the same way regardless of -Dstem_backend; that option now only
# decides whether the separate reprise-worker binary is built and installed.
cargo_feature_args=

set -- env \
  CARGO_TARGET_DIR="$build_root/cargo-target" \
  GETTEXT_PACKAGE=reprise \
  LOCALEDIR="$localedir"


"$@" cargo build $cargo_profile_args $cargo_feature_args \
  -p reprise-gnome --manifest-path "$source_root/Cargo.toml"

cp "$build_root/cargo-target/$profile/reprise" "$output"
