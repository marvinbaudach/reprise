#!/usr/bin/env bash
# CI lints with Arch's rolling Rust, which runs ahead of the toolchain a
# developer machine defaults to. A lint that only the newer clippy knows
# (`unused_imports` on a redundant `use gtk4::prelude::*` was one) therefore
# went red on dev while the local gate was green. This repeats the "Rust lint"
# gate with the newest rustup toolchain installed here, with CI's flags.
#
# It skips, with a notice, wherever there is no newer toolchain to compare
# with. That includes CI itself: the Arch container has one toolchain, from
# pacman, and no rustup, so the "Rust lint" gate already ran on it.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source scripts/lib/rulebook.sh

command -v rustup >/dev/null 2>&1 ||
  skip_gate "rustup is not installed, so there is no second toolchain to lint with"

# Toolchain names look like `1.99.0-x86_64-unknown-linux-gnu`; a channel such as
# `stable` or `nightly` has no version in its name and is the default's own
# business, so only numbered releases are candidates.
newest=$(rustup toolchain list |
  sed -n 's/^\(1\.[0-9][0-9]*\.[0-9][0-9]*\)-.*/\1/p' |
  sort -V | tail -n 1)
default=$(rustc --version | awk '{ print $2 }')

[[ -n $newest ]] ||
  skip_gate "rustup lists no numbered toolchain"
if [[ $newest == "$default" || $(printf '%s\n%s\n' "$default" "$newest" | sort -V | tail -n 1) != "$newest" ]]; then
  skip_gate "no installed toolchain is newer than the default rustc $default"
fi
cargo "+$newest" clippy --version >/dev/null 2>&1 ||
  skip_gate "toolchain $newest has no clippy component (rustup component add clippy --toolchain $newest)"

echo "Linting with rustc $newest; the default is $default."
cargo "+$newest" clippy --locked --all-targets --workspace -- -D warnings
