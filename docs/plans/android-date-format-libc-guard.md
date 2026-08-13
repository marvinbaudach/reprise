# Android build blocker: `nl_langinfo` is not in Bionic

## The failure

`scripts/android-build.sh` cannot get past the Rust cross-compile. On a clean
checkout of `origin/dev` (`edd458e8df`):

```
cargo build --locked --release --target x86_64-linux-android -p reprise-android-ffi

error[E0425]: cannot find value `D_FMT` in crate `libc`
   --> crates/reprise-core/src/format.rs:154:20
error[E0425]: cannot find value `T_FMT` in crate `libc`
   --> crates/reprise-core/src/format.rs:169:20
error[E0425]: cannot find type `nl_item` in crate `libc`
   --> crates/reprise-core/src/format.rs:178:25
error[E0425]: cannot find function `nl_langinfo` in crate `libc`
   --> crates/reprise-core/src/format.rs:185:25
error: could not compile `reprise-core` (lib) due to 4 previous errors
```

Because `reprise-core` does not compile, no `libreprise_android_ffi.so` is
produced, so `uniffi-bindgen` generates no Kotlin bindings, so
`:app:compileDebugKotlin` fails with dozens of `Unresolved reference 'uniffi'`
in `ThemeSelection.kt`, `TrackAnalysisLoader.kt`, `TrackCover.kt` and
`VisualizerSelection.kt`. The whole Android app — build, unit tests, emulator —
is blocked.

## Why it got in

The code arrived with `f9cd85a82e` "One date format, taken from the system
(#383)", an ancestor of both `origin/dev` and every feature branch. It guards
the `nl_langinfo` path with `#[cfg(unix)]` only. Android *is* unix, but its libc
is Bionic, which has no `nl_langinfo`, no `nl_item`, and no `D_FMT`/`T_FMT`
constants — verified against the pinned `libc 0.2.189` in `Cargo.lock`: none of
the crate's `android` modules define those symbols.

`.github/workflows/ci.yml` mentions neither `android` nor `gradle`, so CI never
cross-compiles and never caught it.

## The change

Split the cfg so the `nl_langinfo` path is compiled only where the symbol
exists, and let Android fall through to the fallback that the doc comments in
`format.rs` already promise ("On platforms without `nl_langinfo`, Reprise's ISO
date pattern is returned" / "a 24-hour pattern"):

- gate the `nl_langinfo` implementation on `all(unix, not(target_os = "android"))`
- route Android to the existing non-`nl_langinfo` fallback — do not invent a
  second fallback, and do not add an Android-specific date format
- follow the guard style already used in `crates/reprise-core/src/logging.rs`,
  which does this consistently in the same crate

Desktop behaviour must stay bit-identical: on Linux the returned patterns must
be exactly what they are today. This is a portability fix, not a behaviour
change.

## Tests

Add coverage that actually runs on the host — a `#[cfg(target_os = "android")]`
test would never execute in `cargo test` and would prove nothing:

- make the fallback path reachable as its own function and assert it yields the
  ISO date pattern and the 24-hour time pattern
- keep the existing `format` tests green unchanged

## Verification gates

All four must pass and must be reported with their actual output:

1. `cargo build --locked --release --target aarch64-linux-android -p reprise-android-ffi`
2. `cargo build --locked --release --target x86_64-linux-android -p reprise-android-ffi`
3. `cargo test -p reprise-core format`
4. `cargo clippy -p reprise-core --all-targets` — no new warnings

The Android NDK comes from `ANDROID_HOME=/home/marvin/.local/share/android-sdk`;
`scripts/android-build.sh` shows how the cross toolchain is wired. If gate 1 or 2
cannot run for a reason unrelated to this change, say so explicitly instead of
declaring success.

Do not run Gradle, do not build an APK, do not touch anything under `android/`.
This branch fixes the Rust side only.

## Out of scope

- the `PlaybackServiceLifetimeTest` / JVM-host UniFFI question
- adding Android to CI (worth doing, separate task — note it, do not do it)
- anything in the Now Playing scene
