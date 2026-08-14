---
slug: android-ci-gates
worktree: /home/marvin/Projects/reprise-android-ci-gates
branch: feature/android-ci-gates
phase: coded
codex_session:
created: 2026-08-14
---
# Android: the half of the app no gate has ever looked at

Read against `dd67122fc7` (`origin/dev`, 2026-08-14), which is this branch's
base. Every claim below carries its file and line; check them before changing
anything, because the point of this plan is that nothing else does.

## Rules for the implementer — read first

**Do not touch a device, `adb`, the emulator, or `cua-driver`. Nothing here
needs one.** Every gate this plan builds runs on a host with a JDK and the
Android SDK, and the whole point is that it needs no phone.

**`BUILD SUCCESSFUL` is not evidence, and on this project it is a known liar.**
Gradle reports `:app:testDebugUnitTest` as up-to-date, exits 0 in seconds, and
runs nothing; the result XML is then an hour old. Wave 1 exists because of that.
Every claim you make about a test run must quote suite, test, failure and error
counts read out of XML files you have proven are fresh.

**The `Files:` lists below are a starting point, not a fence.** Adjacent files
may be changed where the contract demands it — name them in the commit message.
Stop only if the *contract* is wrong, not because a path is missing from a list.

## The complaint

Reprise ships two frontends. One of them is covered by a quality gate that runs
`cargo fmt`, `clippy`, `cargo doc`, the whole workspace test suite, eleven
project-specific lint scripts and `cargo audit` on every push and pull request
(`scripts/check-merge-readiness.sh:51-116`). The other one — 24,903 lines of
Kotlin and 66 test suites — is checked by nothing at all.

## What is actually there

- `scripts/ci-quality.sh:31` is the whole CI gate. It calls
  `scripts/check-merge-readiness.sh`. Neither `gradle` nor `android` appears
  anywhere in that script's 119 lines. **The 66 Robolectric suites have never
  run in CI.**
- `.github/workflows/ci.yml:19-24` runs that gate inside a
  `container: archlinux:latest`. There is no JDK and no Android SDK in it, and
  `android-sdk` is not in Arch's repositories. Adding a Gradle step to the
  existing gate is therefore not a small change — it is the wrong change.
- `.github/workflows/cross-target.yml:82-90` type-checks for Android, but only
  `-p reprise-core`. **`reprise-android-ffi` — the crate that ships inside the
  APK — is never compiled for an Android target by anything automated.** It has
  a target-gated dependency (`tracing-android`, under
  `[target.'cfg(target_os = "android")'.dependencies]`,
  `crates/reprise-android-ffi/Cargo.toml:33-39`) that a host build never sees.
- `.github/workflows/cross-target.yml:9-25` carries a `paths` filter listing
  `crates/reprise-core/**` and the manifests. A change confined to
  `crates/reprise-android-ffi/**` or `android/**` triggers that workflow not at
  all.
- Nothing pins the JDK. `android/app/build.gradle.kts:57-59` sets
  `sourceCompatibility`/`targetCompatibility = VERSION_21`, which is the
  *bytecode* level, not the JVM the tests run on. `android/gradle.properties`
  has no `org.gradle.java.home`; there is no toolchain block. On this machine
  the default JDK is 26, and under it every Robolectric test fails with
  `Unsupported class file major version 70` — 125 of 224 on an unmodified tree,
  measured 2026-08-09. A hand-run without `JAVA_HOME` set is not a test result.
- `android/app/src/` contains `main` and `test` only. There is no `androidTest`
  source set and no benchmark module, so nothing on-device is automated either.
  That is out of scope here — this plan buys the JVM half, which is the half
  that can run on every pull request.

## Decisions

**D1 — the Kotlin suite gets its own job, not a step in the existing gate.**
The gate runs in an Arch container by design (it needs GTK, GStreamer,
libadwaita). The Android suite needs a JDK and the Android SDK, which the
GitHub-hosted `ubuntu-24.04` image already ships. Two jobs, two images.

**D2 — the UniFFI bindings are generated from a host build, not a cross build.**
`scripts/android-build.sh:52-54` generates them from the freshly built
`.so` for the *Android* target, which drags in the NDK and a cross compile. The
Kotlin unit tests never load the library — `PlaybackServiceLifetimeTest` overrides
`openCoreSession` precisely because it cannot (`ReprisePlaybackService.kt:129-142`)
— they only need the generated Kotlin to *compile against*. `uniffi-bindgen
generate --library` reads metadata out of any cdylib, and no `#[uniffi::export]`
in the crate sits inside a `cfg` block: the only target-gated code is the
subscriber plumbing in `logging.rs:44,74,121`, while the exported
`init_logging` at `logging.rs:64-65` is unconditional. The exported surface is
therefore target-independent, and the suite job builds the host cdylib and skips
the NDK entirely.

**This is the one assumption in the plan that must be proven, not believed.**
Wave 1 Step 1 proves it by diffing host-generated against NDK-generated
bindings. If they differ, the job falls back to `scripts/android-build.sh` and
the CI job needs the NDK — say so in the commit message and move on; the rest
of the plan is unaffected.

**D3 — no `paths` filter on the new suite job.** A filtered suite is how a suite
goes missing. The cross-target job keeps its filter (it is expensive and
genuinely narrow) but the filter is widened to cover the Android crate.

**D4 — the floor is measured, not copied from this document.** Wave 1's script
refuses to pass below a recorded number of executed tests. Measure that number
on your own base commit and write it into the script together with the commit
it was measured at. A budget nobody maintains is a budget nobody believes —
same reasoning as `scripts/check-frontend-thinness.sh:6-12`.

---

## Wave 0 — pin the test JVM in the build

Without this, every later wave measures whatever JDK happened to be on the
PATH, and a green local run means nothing.

**Files:**
- Modify: `android/app/build.gradle.kts` (around the `testOptions` block,
  `:67-71`)

**Contract:** `./gradlew :app:testDebugUnitTest` runs its tests on a Java 21
JVM regardless of the `JAVA_HOME` it is invoked with, and fails with a clear
"no matching toolchain" message when no JDK 21 is installed. It must not fall
back to the ambient JDK, and it must not silently skip.

AGP 9 ships Kotlin support built in and forbids `org.jetbrains.kotlin.android`
alongside it (`android/build.gradle.kts:2-4`), so reach for the plugin-agnostic
mechanism: configure the `Test` tasks' `javaLauncher` from the
`javaToolchains` service for `JavaLanguageVersion.of(21)`. Do not assume a
`kotlin { jvmToolchain(...) }` block exists in this build — verify before using
it.

**Steps:**

- [x] **Step 1: Prove the trap is real on your tree.** Run the suite under a
      JDK 26 and watch it fail, so you know the counter-proof in Step 3 means
      something:

      JAVA_HOME=$(ls -d /usr/lib/jvm/java-2[2-9]-openjdk 2>/dev/null | tail -1) \
        ./gradlew --project-dir android :app:testDebugUnitTest 2>&1 \
        | tee "$SCRATCH/jdk-before.log" | grep -c 'major version 70'

      Expected: a non-zero count. If no JDK above 21 is installed, say so in the
      commit message and skip to Step 2 — the pin is still correct.

- [x] **Step 2: Add the toolchain pin** to `android/app/build.gradle.kts`, with
      a comment naming what it prevents (`Unsupported class file major version
      70`, Robolectric 4.16.1 cannot instrument Java 26 class files).

- [x] **Step 3: Counter-proof.** Repeat the exact command from Step 1. Expected
      now: zero occurrences of `major version 70`, and the suite runs. Quote the
      before and after counts.

- [x] **Step 4: Commit.** `build: pin the Android unit tests to a Java 21 toolchain`

---

## Wave 1 — a suite script that cannot report green without running

**Files:**
- Create: `scripts/check-android-suite.sh`
- Create: `scripts/tests/check-android-suite.sh` (parser unit tests, following
  the pattern of `scripts/tests/android-theme.sh`)

**Contract:** one command that generates the bindings, runs the Kotlin unit
suite, and refuses to exit 0 unless it can prove the tests actually executed on
this invocation. It hard-fails when the Android SDK is missing. It has no skip
path — a missing prerequisite is a failure, never a pass.

**Steps:**

- [x] **Step 1: Prove D2.** Generate the bindings both ways and diff them:

      # NDK route (the status quo)
      scripts/android-build.sh
      cp -r android/app/src/main/java/uniffi "$SCRATCH/uniffi-ndk"
      rm -rf android/app/src/main/java/uniffi
      # host route (what the script will do)
      cargo build --locked --release -p reprise-android-ffi
      cargo run --locked --release --bin uniffi-bindgen -p reprise-android-ffi -- \
        generate --library target/release/libreprise_android_ffi.so \
        --language kotlin --out-dir android/app/src/main/java
      diff -r "$SCRATCH/uniffi-ndk" android/app/src/main/java/uniffi; echo "diff exit: $?"

      Expected: exit 0, no differences. Record the outcome in the commit
      message. If they differ, the script uses `scripts/android-build.sh`
      instead and Wave 2's job needs the NDK — note it and continue.

- [x] **Step 2: Write the freshness check first, and its unit tests.** The
      parser takes a start timestamp and a directory of JUnit XML and answers
      four numbers plus a verdict. Cover: all files fresh; one file stale; an
      empty directory; a directory that does not exist. All four must be
      distinguishable, and only the first is a pass. Run
      `scripts/tests/check-android-suite.sh` and watch it fail before the
      implementation exists.

- [x] **Step 3: Write the script.** In order:
      1. Fail unless `ANDROID_HOME`/`ANDROID_SDK_ROOT` points at a real
         directory. No default, no skip.
      2. Generate the bindings (route decided in Step 1).
      3. `rm -rf android/app/build/test-results/testDebugUnitTest` — deleting
         the directory is what forces the run. **`--rerun` alone does not**; it
         failed to force a run on 2026-08-04 and the deletion succeeded.
      4. Record the start time, run `:app:testDebugUnitTest`, then
         `:app:assembleDebug` (which needs no `.so` — it proves manifest merge,
         resource linking and the Compose compile, and it is nearly free once
         the tests have compiled).
      5. Count suites, tests, failures, errors and skips from the XML; assert
         every file's mtime is at or after the start time; assert the test count
         is at or above the recorded floor.
      6. Print the four numbers on one line so a reader — and a reviewer — can
         quote them without opening anything.

- [x] **Step 4: Prove it can go red, three ways.** A green gate that has never
      been shown to fail is not a gate.
      1. **Compile break:** point a reference at something that does not exist
         (the established trick: `R.color.reprise_teal` → `R.color.does_not_exist`),
         run, expect red, revert.
      2. **Stale results:** run the script once, then re-run only the Gradle
         command by hand so the XML is old, then run the freshness check against
         a later start time. Expect the stale verdict.
      3. **Floor:** raise the floor above the real count by one, run, expect red,
         set it back.
      Quote all three outcomes.

- [x] **Step 5: Measure and record the floor** (D4) on your base commit, with
      the commit hash in a comment beside it.

- [x] **Step 6: Commit.** `ci: add a self-proving Android unit-suite gate`

---

## Wave 2 — run it on every pull request

**Files:**
- Modify: `.github/workflows/ci.yml` (new job beside `quality`, `:19`)

**Contract:** a job named so that a human scanning the checks list can tell what
it covers, running on plain `ubuntu-24.04` (**no container** — D1), with
`actions/setup-java` at 21 (temurin), calling `scripts/check-android-suite.sh`.
No `paths` filter (D3).

**Steps:**

- [x] **Step 1: Add the job.** Reuse the Cargo cache configuration already in
      `ci.yml:48-56` — the host cdylib build is the slow part.
- [x] **Step 2: Push the branch and read the run.** Not `gh pr checks` alone:
      **it exits 0 when a PR has no checks at all**, so "green" and "absent"
      look identical from the exit code. List the checks by name and confirm the
      new one is among them.
- [x] **Step 3: Confirm what the job actually ran** by reading the counts the
      script printed in the log — the same four numbers, from CI this time.
      A `cancelled` conclusion is not a failure: concurrency cancels superseded
      runs, and the verdict belongs to the successor run.
- [x] **Step 4: Commit.** `ci: run the Android unit suite on every change`

---

## Wave 3 — compile the shipped crate for the shipped target

**Files:**
- Modify: `.github/workflows/cross-target.yml` (`paths` at `:9-25`, the Android
  step at `:82-90`)

**Contract:** `reprise-android-ffi` is type-checked for `aarch64-linux-android`,
and a change confined to that crate or to `android/**` triggers the workflow.

**Steps:**

- [x] **Step 1: Widen the `paths` filter** to include
      `crates/reprise-android-ffi/**` and `android/**`, in both the
      `pull_request` and the `push` block. Extend the comment at the top of the
      file, which currently explains the filter in terms of the portable engine
      only.
- [x] **Step 2: Add the crate to the Android check.** The NDK environment is
      already set up in that step; the change is the package list. Prefer a
      second `cargo check` invocation over widening the first, so a failure
      names which crate broke.
- [x] **Step 3: Prove it can go red.** Temporarily add a host-only dependency
      or a `std::os::unix`-flavoured call to the crate, push, confirm the job
      fails, revert. Quote the failure.
- [x] **Step 4: Commit.** `ci: type-check reprise-android-ffi for Android`

---

## Traps

- **The Gradle up-to-date lie.** Covered by Wave 1, but it applies to *your own*
  verification runs too, including compile tasks: `compileDebugKotlin` reported
  `UP-TO-DATE` on 2026-08-04, and `touch` does not defeat it (Gradle hashes
  content). Delete the results directory.
- **JDK.** Before suspecting your own change when the suite goes broadly red,
  `grep 'major version 70'`. After Wave 0 this should be impossible; before it,
  it is the first hypothesis.
- **`TMPDIR`.** The Rust tests in `reprise-android-ffi` depend on `readdir`
  order and are green with `TMPDIR=/tmp`. CI already has that; a local run in a
  worktree with a redirected `TMPDIR` may not.
- **A formerly flaky neighbour.** The cover-cache warning test in
  `reprise-android-ffi` used to be load-flaky — red in a full workspace run,
  green alone. `dd67122fc7` (#468), this branch's base, fixed the race against
  `tracing`'s callsite cache. If you still see it red, that is news worth
  reporting, not something to route around.
- **Generated bindings are gitignored on purpose** (`.gitignore`, the Android
  block). Do not commit `android/app/src/main/java/uniffi/` or
  `android/app/src/main/jniLibs/` — a checked-in binding drifts from the Rust
  signatures it mirrors without anything failing.
- **Closing the worktree** can trip over `gradlew.bat`'s CRLF blob. That is a
  known repo quirk, not a sign your branch is dirty.

## What this plan does not do

It does not add instrumentation tests, a macrobenchmark module, or anything that
needs a device. It does not fix the library-wide mutex that `MusicLibrary::scan`
holds for the whole folder walk (`crates/reprise-android-ffi/src/lib.rs:114-141`),
it does not land `feature/android-list-scroll-performance`, and it does not
rename the `de.reprise.spike` package. Each of those is its own plan. This one
buys the thing all of them need: a gate that notices when they break something.
