#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

export ANDROID_NDK_HOME=/opt/android-ndk
export ANDROID_HOME=/home/marvin/Android/Sdk
TC=/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin
export PATH="$TC:$PATH"
export CC_x86_64_linux_android="$TC/x86_64-linux-android21-clang"
export AR_x86_64_linux_android="$TC/llvm-ar"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$CC_x86_64_linux_android"

target_triple=x86_64-linux-android
jni_dir="$repo_root/android/app/src/main/jniLibs/x86_64"
library_name=libreprise_android_ffi.so

cargo build --locked --release --target "$target_triple" -p reprise-android-ffi
mkdir -p "$jni_dir"
install -m 0755 \
  "$repo_root/target/$target_triple/release/$library_name" \
  "$jni_dir/$library_name"

printf 'Built %s\n' "$jni_dir/$library_name"
