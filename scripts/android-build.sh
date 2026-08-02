#!/usr/bin/env bash
# Builds the native library and the UniFFI Kotlin bindings for the Android app.
#
# Everything this produces is generated and gitignored: the `.so`, the Kotlin
# bindings, and `android/local.properties`. That is deliberate — a checked-in
# binding file drifts from the Rust signatures it mirrors without anything
# failing, and a checked-in SDK path is wrong on every machine but one.
#
# Paths come from the environment when set, so this also works where the SDK
# and NDK live somewhere else.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-/opt/android-ndk}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"

[ -d "$ANDROID_NDK_HOME" ] || { echo "NDK not found at $ANDROID_NDK_HOME — set ANDROID_NDK_HOME" >&2; exit 1; }
[ -d "$ANDROID_HOME" ] || { echo "SDK not found at $ANDROID_HOME — set ANDROID_HOME" >&2; exit 1; }

# The emulator is x86_64; arm64 is added when a real device is in play.
target_triple="${ANDROID_TARGET:-x86_64-linux-android}"
abi="${ANDROID_ABI:-x86_64}"
api="${ANDROID_API:-21}"

toolchain=$(echo "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin)
[ -d "$toolchain" ] || { echo "no llvm toolchain under $ANDROID_NDK_HOME" >&2; exit 1; }
export PATH="$toolchain:$PATH"

cc="$toolchain/${target_triple}${api}-clang"
[ -x "$cc" ] || { echo "no clang for $target_triple api $api at $cc" >&2; exit 1; }

triple_upper=$(echo "$target_triple" | tr 'a-z-' 'A-Z_')
triple_snake="${target_triple//-/_}"
export "CC_${triple_snake}=$cc"
export "AR_${triple_snake}=$toolchain/llvm-ar"
export "CARGO_TARGET_${triple_upper}_LINKER=$cc"

library_name=libreprise_android_ffi.so
jni_dir="$repo_root/android/app/src/main/jniLibs/$abi"
kotlin_dir="$repo_root/android/app/src/main/java"

cargo build --locked --release --target "$target_triple" -p reprise-android-ffi

mkdir -p "$jni_dir"
install -m 0755 "$repo_root/target/$target_triple/release/$library_name" "$jni_dir/$library_name"

# Generated from the library that was just built, so the bindings can never
# describe a different version of it than the one shipped beside them.
mkdir -p "$kotlin_dir"
cargo run --locked --release --bin uniffi-bindgen -p reprise-android-ffi -- \
  generate --library "$jni_dir/$library_name" --language kotlin --out-dir "$kotlin_dir"

# Gradle needs an SDK path and the file is gitignored, so write it if absent.
if [ ! -f "$repo_root/android/local.properties" ]; then
  printf 'sdk.dir=%s\n' "$ANDROID_HOME" > "$repo_root/android/local.properties"
  printf 'Wrote android/local.properties\n'
fi

printf 'Built %s\n' "$jni_dir/$library_name"
printf 'Generated Kotlin bindings under %s/uniffi\n' "$kotlin_dir"
