---
slug: night-e-the-portrait-file-earns-its-size
worktree: .worktrees/night-e-portrait-file
branch: refactor/the-portrait-file-earns-its-size
phase: planned
created: 2026-09-07
base: origin/dev
owns: crates/reprise-android-ffi/src/{artist_portrait.rs,artist_portrait_tests.rs}
---
# Night package E — the portrait file earns its size

## Autonomy

**Run this end to end without asking.** Plan → code → check → refactor → land,
including the merge. Autonomous landing is authorised. Do not ask for `/check`,
`/refactor` or `/ship` between phases. Stop only for a listed **stop
condition**; then leave the worktree, add a `## Findings` section to this file,
set `phase: blocked`, and stop.

Own worktree `.worktrees/night-e-portrait-file`, branch
`refactor/the-portrait-file-earns-its-size`, off `origin/dev`. Four sibling
packages run tonight. **Touch only the files in `owns:`.** This is the smallest
package of the five; if you finish early, stop rather than looking for more.

## Why — and the correction that makes this easy

`crates/reprise-android-ffi/src/artist_portrait.rs` is **793 lines** against the
repository's hard 800-line ceiling, enforced by `scripts/check-architecture.sh`.
Seven lines of headroom means the next unrelated commit that touches it trips a
gate that has nothing to do with that commit.

The obvious reading — "a 793-line file needs decomposing" — is **wrong**, and
acting on it would produce exactly the size-driven split this repository already
has too much of. Measured on `origin/dev` @ `fe89dc51ad`:

| Region | Lines |
| --- | ---: |
| Production code (1–216) | ~217 |
| `#[cfg(test)] mod tests` (218–793) | ~575 |

**The pressure is entirely from the test module.** The production surface is
small and coherent. Moving tests to a sibling file is the established idiom
here — 46 files in this repository already do exactly that, and the mechanism is
`#[cfg(test)] #[path = "..._tests.rs"] mod tests;`, which is invisible to every
caller.

So this package is a file move, not a decomposition.

## The production surface, for orientation

You should not need to change any of it, but know what you are moving tests away
from:

- Lines 13–36 — `impl MusicLibrary` (not exported): `portrait_dir()`,
  `reduced_portrait_path()`.
- Lines 38–95 — `#[uniffi::export]`: `artists_missing_portraits()`.
- Lines 97–131 — `#[uniffi::export]`: `artist_portrait_cached()`,
  `artist_portrait_fetch()`.
- Lines 133–171 — `ArtistPortraitProgressUpdate` (`#[uniffi::Record]`),
  `ArtistPortraitProgressState` (`#[uniffi::Enum]`), and two `From` impls.
- Lines 173–176 — `ArtistPortraitProgressListener`
  (`#[uniffi::export(callback_interface)]`).
- Lines 178–216 — `#[uniffi::export]`: backfill progress, start, cancel.

The 18 tests live in the `mod tests` block from line 218.

## Tasks

### E.1 — move the tests to a sibling

Create `crates/reprise-android-ffi/src/artist_portrait_tests.rs` holding the
current test module's **contents** (not the `mod tests { }` wrapper), and replace
the block in `artist_portrait.rs` with:

```rust
#[cfg(test)]
#[path = "artist_portrait_tests.rs"]
mod tests;
```

Copy the idiom from a file that already does this rather than inventing it —
`crates/reprise-core/src/library/tag_edit.rs` and its `tag_edit_write_tests.rs`
are one example; there are 45 others.

The moved file will need its own `use super::*;` and whatever imports the tests
had. **Do not change a single assertion.** All 18 tests must pass unedited; if
one needs touching, the move went wrong.

### E.2 — check whether the FFI surface is affected

It should not be: `#[cfg(test)]` code never reaches the UniFFI scaffolding. But
this crate generates Kotlin bindings, so confirm rather than assume. After the
move, regenerate and diff:

```
scripts/check-android-suite.sh
```

That script builds the FFI for the host, regenerates the bindings and runs the
Android JVM suite. If the generated
`android/app/src/main/java/uniffi/reprise_android_ffi/reprise_android_ffi.kt`
differs in any way, stop — a test move must not change an exported surface.
(That file is gitignored and regenerated, so compare before and after within
your own run.)

### E.3 — leave the production code alone

Do **not** split the cache functions from the backfill functions, tempting as it
looks. The two clusters are related — `reduced_portrait_path()` serves both the
cached and the fetch paths — and after E.1 the file is around 220 lines, well
inside the rule, with no pressure to justify a second cut. Splitting a coherent
217-line module because it *used* to be near a ceiling is the failure mode this
package exists to avoid.

If you believe a split is genuinely warranted on its merits after the move, say
so in the PR body as a recommendation. Do not act on it tonight.

## Acceptance

- `wc -l crates/reprise-android-ffi/src/artist_portrait.rs` is roughly 220, and
  well under 800.
- The new `artist_portrait_tests.rs` is under 800 lines too. If the test file
  itself exceeds the ceiling, split it by topic — cache tests and backfill tests
  are the natural line — and say so.
- All 18 tests pass, **unedited**.
- The generated Kotlin bindings are byte-identical before and after.
- No file outside `owns:` is modified.

## Gates

```
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
scripts/check-android-suite.sh
scripts/check-architecture.sh
```

`check-architecture.sh` is the one that enforces the 800-line ceiling — it is
both the reason for this package and its proof.

**Run the Android suite only through `scripts/check-android-suite.sh`.** It
builds the FFI for the *host* and exports `LD_LIBRARY_PATH`.
`scripts/android-build.sh` builds for the device, and a raw `gradlew` after it
fails 28 Robolectric tests for reasons unrelated to the code. A fresh worktree
also needs `android/local.properties` copied in — it is gitignored and Gradle
aborts before compiling without it.

## Stop conditions

- The generated bindings change. A test move must be invisible to the FFI.
- A test needs editing to pass after the move.
- The change would touch any file outside `owns:` — in particular
  `scripts/check-architecture.sh`. A crate-level size budget for
  `reprise-android-ffi` has been discussed and is deliberately **not** part of
  tonight; it is a new gate, and it should follow this package rather than race
  it.
- `reprise-android-ffi` is already red on unmodified `origin/dev`. Check the
  control arm first.

## Landing

Squash-merge into `dev`. The title is taken verbatim; write prose. Something
like *"The portrait tests move out of the way"*. In the body, state the measured
before-and-after line counts and note that the production code was deliberately
left whole.
