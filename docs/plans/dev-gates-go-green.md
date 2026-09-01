---
slug: dev-gates-go-green
worktree: /home/marvin/Projects/reprise-dev-gates-go-green
branch: feature/dev-gates-go-green
phase: refactored
codex_session:
created: 2026-09-01
---
# The dev gates go green again

## Why

Every CI run on `dev` since at least `496a9a5c36` (2026-09-01 00:35) is red. Two
independent causes, both arriving with **#758** ("The app asks before it fetches
artist photos"), neither of them a defect in the product code. While they stand,
a red dev run carries no information: nobody can tell a real regression from
this background noise.

## Task 1 — Teach the traceability gate that `[android]` is a level

`scripts/check-ux-traceability.sh` fails with:

```
ERROR: [active] rule NET-4b has no rule-named test
```

The rule is declared in `docs/ux-rules.md:2982` as
`- **NET-4b** [active] [android] — …`.

The tests for it **already exist and are already named correctly**, in
`android/app/src/test/java/io/github/marvinbaudach/reprise/ArtistPhotoOfferTest.kt`:

```kotlin
fun net_4b_offersArtistPhotosOnTheFreshPopulatedPath()
fun net_4b_doesNotOfferArtistPhotosWhileTheGateIsOn()
fun net_4b_doesNotOfferArtistPhotosAfterTheQuestionWasSettled()
fun net_4b_doesNotOfferArtistPhotosForAnEmptyLibrary()
```

The gate cannot see them. Two reasons, both in the script:

1. Line 31 hardcodes the recognised levels as `(core|gtk|e2e|web|manual)`.
   `[android]` matches none of them, so `level_of[NET-4b]` stays empty, the
   `manual` branch is skipped, and the rule falls into the "must have a
   rule-named test" branch (lines 86-96).
2. Test discovery scans `crates` for Rust `#[test]` functions, `scripts/cua-e2e`
   for kebab-case ids, and `showroom/tests` for `test('…`. Nothing scans
   `android/`.

So this is a gate that never learned about a level the repository started using.
The fix is in the script, not in the rule and not in the tests:

- add `android` to the level regex on line 31;
- add an Android discovery pass that scans
  `android/app/src/test/**/*.kt` for test functions named
  `(${prefixes})_[0-9]+[a-z]?_…`, mirroring the existing Rust pass —
  including its requirement that a `@Test` annotation sits within the few lines
  above, so a helper that merely happens to carry the name does not count as
  coverage.

Do **not** "fix" this by renaming a test, by marking NET-4b `[manual]`, or by
listing it in `RELEASING.md`. Its tests are automated and they pass; the gate is
what is wrong.

While you are in that script: check whether any other rule currently declares a
level outside the hardcoded list. If one does, it is silently in the same trap —
report it, and cover it with the same change if it is the same `[android]` level.

## Task 2 — Baseline the new lint finding

`:app:lintDebug` fails with:

```
android/app/src/test/java/io/github/marvinbaudach/reprise/ArtistPhotoProgressBarTest.kt:420:
Error: Constructing a view model in a composable [ViewModelConstructorInComposable]
```

Line 420 is `surfaceState = MobileSurfaceViewModel(),` inside the
`compose.setContent { }` lambda of the private helper
`showBrowseWithArtistOffer` (lines 404-443), used by the two `net_4b_…`
progress-bar tests.

`android/build.gradle.kts:15-21` sets `abortOnError = true` and
`warningsAsErrors = true` against `android/app/lint-baseline.xml`. That baseline
**already carries 12 entries for exactly this lint rule**, every one of them in
a test file — `ArtistDetailSurfaceTest.kt`, `ArtistPortraitSurfaceTest.kt`,
`LibraryRatingVisibilityTest.kt`, `NowPlayingQueueTest.kt`,
`TrackContextMenuTest.kt`. The established answer in this repository for a test
that constructs a view model directly is the baseline; production composables
take `surfaceState: MobileSurfaceViewModel = viewModel()` as a default
parameter, and a test that must inject a specific instance cannot use that.

So: add the missing entry for `ArtistPhotoProgressBarTest.kt:420`, consistent
with the 12 that are already there.

Prefer the project's own mechanism for this (a gradle baseline-update task) over
hand-editing the XML. Whichever you use, the committed diff to
`lint-baseline.xml` must contain **only** the new `ArtistPhotoProgressBarTest`
entry. If a regenerated baseline drops or rewrites unrelated entries, discard it
and add the single entry by hand instead — this branch is not the place to
churn twelve unrelated suppressions.

## Verification

- `scripts/check-ux-traceability.sh` exits 0, and its output no longer names
  NET-4b.
- Prove the gate change is not vacuous: temporarily rename one `net_4b_…` test
  in `ArtistPhotoOfferTest.kt`, confirm the script fails again for NET-4b,
  restore the name, confirm it passes. Report what the failure looked like. A
  discovery pass that reports coverage it cannot actually see would be the same
  class of defect as the one being fixed.
- `npm --prefix android run lint` (i.e. `./gradlew --max-workers=2 :app:lintDebug
  :lint-contract:lintDebug`) exits 0.
- `cargo fmt --check` and the shell-script gate over the changed script, if the
  repo has one, exit 0.
- Each exit status captured directly, never read through a pipe.

No Rust production code changes, no gettext work, no UI strings.

## Parallelität

**One strand.** Two files, two independent one-line-ish fixes; a cut would cost
two worktrees and two landings for nothing.
