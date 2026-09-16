---
slug: the-seek-bar-follows-the-finger
worktree: /home/marvin/Projects/reprise-the-seek-bar-follows-the-finger
branch: feature/the-seek-bar-follows-the-finger
phase: reviewed
codex_session:
created: 2026-09-15
---
# Android: the seek bar follows the finger

## Problem

On the Pixel 10 Pro XL (app 0.1.128, 2026-09-13 build; none of the files below
changed since) the now-playing seek bar ignores nearly every drag and most
taps. The user's words: dragging the head does not work, only tapping into the
bar, and that unreliably.

Reproduced 2026-09-15 with synthesized input on the open sheet, track paused
at 4:58 total, bar spanning x = 64…1016 px at y = 1768:

| gesture                                            | head afterwards                    |
|----------------------------------------------------|------------------------------------|
| tap at 50 %, pointer perfectly still               | 2:29 — correct                     |
| tap at 50 % with a 6 px wobble before the up       | unchanged                          |
| `input swipe` 25 % → 75 % over 1 s (small moves)   | unchanged                          |
| DOWN, MOVE +8 px, MOVE → 50 %, MOVE → 70 %, held   | unchanged, even while still down   |
| DOWN, MOVE +98 px (past slop), MOVE → 70 %, held   | follows live, 3:12 while held      |

A finger's first MOVE after the DOWN is almost always below touch slop, so a
real drag practically never starts; a fast flick occasionally clears slop in
its first event and works — hence "unzuverlässig". A tap only lands when the
finger does not move at all between down and up.

## Root cause

`Modifier.nowPlayingGestures`
(`android/app/src/main/java/io/github/marvinbaudach/reprise/NowPlayingGestures.kt`,
loop at ~185–200) calls `change.consume()` on **every** pointer event of every
gesture, whether or not it claimed an axis:

```kotlin
// This parent observes children first, then consumes the remainder so
// the library pager behind the sheet never receives the same stream.
change.consume()
if (!change.pressed) break
```

The Material 3 `Slider` under it (`NowPlayingSheet.kt:519`, `SpectralSeekSlider`)
is a normal Compose `draggable` plus `detectTapGestures`. Both, while still
inside the touch-slop window, re-check the event on the `Final` pass and give
up as soon as *anyone else* consumed it (`awaitPointerSlopOrCancellation` /
`waitForUpOrCancellation` in foundation). Children see the `Main` pass first,
so a drag that has already passed slop keeps going — which is exactly the one
case that works on the device. Everything shorter than slop is cancelled by
the parent's unconditional consume before it can become a drag or a tap.

The seek-band exclusion (`SEEK_EXCLUSION_START/END`, `verticalAllowed`) only
stops the parent from *acting* on those events; it never stopped it from
*consuming* them.

The comment's rationale — the library `HorizontalPager` behind the sheet — is
not what the consume achieves. The sheet is a *sibling* in front of the pager
(`BrowseScreen.kt:1018`, `AnimatedVisibility` next to `libraryScaffold`), and
Compose stops hit-testing siblings once a node in front was hit. Consumption
only matters along the hit path (the hit node and its ancestors); no ancestor
of the sheet carries a pointer handler. The existing guard
`MobileBottomTabsTest.nowPlayingHidesTheNavigationBarAndConsumesThePagerSwipe`
proves the pager stays put for a cover swipe — and it will keep proving it,
plus two more bands, after the change (task 3). The plan verifies this rather
than assuming it.

`thumb = {}` (the visible handle) and the 234-line gesture layer arrived in the
same commit, `1f8eacadc8` (#796). With Material 3 semantics a press anywhere
on the track jumps to the finger and follows it, so "grabbing the marker"
works the moment the drag reaches the slider — no thumb is needed for that.

## Decisions (grilled 2026-09-15)

1. **The parent consumes only what it owns.** From the first event of a
   gesture up to the moment `PlayGestureState` picks an axis, every change
   passes through unconsumed. Once `state.axis != NONE` the parent owns the
   stream and consumes every remaining change including the up, as today. A
   gesture that never gets an axis (seek band, transport band, a tap) is never
   consumed by the parent. The alternative — keep consuming everything and
   carve out the measured seek bounds — was rejected: more plumbing, and it
   would leave a wobbly tap on the transport buttons broken the same way.
2. **A cancelled scrub still does not seek.** `DragInteraction.Cancel` means
   the system took the pointer away (an incoming call, the shade, a track
   change re-keying the slider) — the listener did not release deliberately.
   #633 decided that and `aCancelledSeekGestureReturnsTheHeadToThePlaybackPosition`
   pins it; this plan keeps it. What changes: after a cancel the head returns
   to the last playback snapshot **at once** instead of showing the abandoned
   scrub value until the next tick happens to arrive (with playback paused
   that is never).
3. **No visible thumb.** Restoring one is a design change (M3 insets the
   value range by half the thumb width, so `SpectralSeekTrack`'s fraction
   maths would have to follow). Out of scope; a follow-up plan if the user
   still misses a handle to *see* once grabbing works.
4. **Measured seek bounds replace the fraction band.** Task 3 found the
   `WIDE_SHORT` seek centre at 0.248 of the gesture node's height, outside the
   planned 0.64…0.76 band. `SpectralSeekSlider` therefore reports its bounds
   in the gesture node's coordinate frame. A down inside those bounds, with an
   8 dp vertical touch margin, is eligible for neither parent axis. The
   transport-height exclusion remains in place for vertical drags.
5. **Device proof after the merge, by the orchestrator.** Build, install and
   the five gestures from the table, under `device-lock`. Agreed in the grill;
   no further question before doing it.
6. **One strand.** See *Parallelität*.

## Design

### `NowPlayingGestures.kt` — consume behind ownership

In the event loop, replace the unconditional `change.consume()` with

```kotlin
// Consume only what this layer owns. Until an axis is chosen the stream
// belongs to whichever child wants it — the seek slider's drag and tap
// detectors abort during their slop window as soon as anyone else has
// consumed the event. The pager behind the sheet needs no consume: the
// sheet is the hit sibling in front of it, and Compose stops hit-testing
// siblings behind a hit node (MobileBottomTabsTest proves it per band).
if (state.axis != PlayGestureAxis.NONE) change.consume()
```

The same modifier receives the measured seek bounds. It refuses both parent
axes when the down lies inside the seek rectangle expanded vertically by 8 dp;
no layout fraction remains.

Nothing else in the loop changes: `childConsumed`, `dragBy` guarded by
`!change.isConsumed`, the tap and double-tap branch and the `finally` stay.
Consuming from the first owned event onwards is what makes a child that had
not yet passed slop back off (the cover has no such children today, but the
rule is general).

Delete the old two-line comment; the replacement above is the documentation.

### `NowPlayingState.kt` / `MobileSurfaceViewModel.kt` / `NowPlayingSheet.kt` — cancel restores the snapshot

`SeekPositionState` gains the last snapshot it saw:

```kotlin
internal data class SeekPositionState(
    val positionMs: Long,
    val isDragging: Boolean,
    val snapshotPositionMs: Long = positionMs,
) {
    fun acceptSnapshot(positionMs: Long): SeekPositionState =
        if (isDragging) copy(snapshotPositionMs = positionMs.coerceAtLeast(0))
        else fromSnapshot(positionMs)

    fun dragTo(positionMs: Long): SeekPositionState =
        copy(positionMs = positionMs.coerceAtLeast(0), isDragging = true)

    fun release(): SeekPositionState = copy(isDragging = false)   // unchanged

    fun cancel(): SeekPositionState = fromSnapshot(snapshotPositionMs)
    …
}
```

`MobileSurfaceViewModel.cancelScrub(trackId)` mirrors `releaseScrub` (same
track-id guard, returns `Unit`, writes `scrubPosition = current.cancel()`).
The `DragInteraction.Cancel` collector in `SpectralSeekSlider` calls
`cancelScrub` instead of `releaseScrub`. `onValueChangeFinished` is untouched.

`MobileSurfaceStateTest.anInterruptedScrubKeepsItsTimeAcrossAWidthChangeWithoutSeeking`
and `BrowseSurfaceTest.seekDragOwnsTheHeadUntilRelease` keep passing as
written (`acceptSnapshot` while dragging still returns the scrub position;
`release` still keeps it).

## Tasks

**Code phase, before Codex starts (orchestrator, not Codex):** the worktree
is fresh, and three gitignored, generated inputs are missing there. Copy them
from the main checkout `/home/marvin/Projects/reprise`:
`android/app/src/main/java/uniffi/`, `android/app/src/main/jniLibs/`,
`android/local.properties`. Without them Gradle fails with
`Unresolved reference uniffi` long before any test runs.

Every Gradle run below is
`JAVA_HOME=/usr/lib/jvm/java-21-openjdk TMPDIR=/tmp android/gradlew --project-dir android :app:testDebugUnitTest --tests '<class>' …`
from the worktree root. Gradle prints BUILD SUCCESSFUL even when it ran
nothing; after every run count the executed tests in
`android/app/build/test-results/testDebugUnitTest/TEST-*.xml` (`tests=` minus
`skipped=`) and quote the number.

### Task 1 — the red tests (full sheet, not the slider alone)

`android/app/src/test/java/io/github/marvinbaudach/reprise/NowPlayingGesturesTest.kt`.
The two existing seek tests mount `TestSeekSlider` — the slider alone — which
is why the suite stayed green through this bug. The new tests mount
`testNowPlayingSheet(controls = GestureRecordingControls())` and act on
`compose.onNodeWithTag("now-playing-seek")`. The class runs at
`w500dp-h1000dp` with Robolectric's default density, so 1 px = 1 dp and touch
slop is 8 px:

- `aSlowDragOnTheSeekBarMovesTheHeadAndSeeksOnRelease` — `down` at 20 % of
  the node's width, then `moveBy(Offset(4f, 0f))` three times (each step below
  slop), then `moveTo` 60 %, then `up()`. Assert `controls.seekPositions.single()`
  is within 6 000 ms of 60 000 (M3 subtracts the slop from the first delta, so
  the landing point is a few px short of the finger) and that
  `now-playing-position` reads the same value.
- `aTapWithAWobbleStillSeeks` — `down` at 50 %, `moveBy(Offset(3f, 0f))`,
  `up()`. Assert `controls.seekPositions.single()` is within 2 000 ms of
  50 000.

Run the class. **Both new tests must fail** on the untouched gesture layer —
quote the failing assertion (`seekPositions` empty). If they pass, the test
does not reproduce the device finding and must be fixed before task 2; a
green-first test proves nothing here.

### Task 2 — consume behind ownership

`NowPlayingGestures.kt` as in Design. Re-run `NowPlayingGesturesTest`: the two
new tests green, and all of `coverDragPastThresholdSkipsToTheNextTrack`,
`coverDragBelowThresholdSpringsBackWithoutChangingTrack`,
`downwardDragClosesTheSheet`,
`doubleTapOnTheLeftSeeksBackTenSecondsAndShowsItsMarker`,
`singleTapOnTheCoverSwitchesToTheSpectrumAndBack`,
`singleTapOutsideTheCoverDoesNotSwitch` still green — those are the parent's
own gestures and must not have lost anything.

### Task 3 — the pager stays put in every band, in both layouts

`android/app/src/test/java/io/github/marvinbaudach/reprise/MobileBottomTabsTest.kt`.
Extend `nowPlayingHidesTheNavigationBarAndConsumesThePagerSwipe` — or add
sibling tests using the same setup — so that a leftward swipe starting in
each of these bands leaves `library-destination-TITLES` selected and the
transport displayed:

- the cover band (today's assertion; `height * 0.3f`),
- the seek band (`height * 0.70f`),
- the transport band (`height - 40.dp`).

Use explicit `down/moveTo/up` at those y's rather than the centre-anchored
`swipeLeft()`, so the band is what is being tested.

In `NowPlayingGesturesTest`, prove the measured boundary through behavior in
both layouts. Down on `now-playing-seek` at 20 % width, move to 60 %, and
release. The one seek must land near 60 % of the duration, the current track
must not change, and the sheet must not close. Keep the method-level
`@Config(qualifiers = "w916dp-h412dp-land")` on the `WIDE_SHORT` case and the
`surfaceLayout` parameter on `testNowPlayingSheet`.

### Task 4 — a cancel returns the head to the last snapshot

`NowPlayingState.kt`, `MobileSurfaceViewModel.kt`, `NowPlayingSheet.kt` as in
Design. Tests:

- `BrowseSurfaceTest.aCancelledScrubShowsTheLastSnapshotNotTheAbandonedValue`
  — `fromSnapshot(12_000).dragTo(48_000).acceptSnapshot(13_000).cancel()`
  has `positionMs == 13_000` and `isDragging == false`; `release()` on the same
  dragging state still has `48_000`.
- `NowPlayingGesturesTest.aCancelledSeekGestureReturnsTheHeadToThePlaybackPosition`:
  add one assertion *before* the snapshot update — right after
  `interactions.cancelDrag()` the slider's progress is already back at the
  initial 20 000, not at the dragged value. Keep the existing final assertion.

### Task 5 — the whole Android suite

From the worktree root, with `ANDROID_HOME=/home/marvin/.local/share/android-sdk`
and `JAVA_HOME` as above: `scripts/check-android-suite.sh`. It rebuilds the
host bindings itself, runs `:app:testDebugUnitTest` and `:app:assembleDebug`,
and refuses a result below its floor of 334 executed tests. Quote its final
count. Then `npm --prefix android run lint` (the Android source-quality
stage; Android Lint compiles the Kotlin, so it also catches a typo the suite
would not reach).

Redirect both logs to `$SCRATCH/<name>.log` and answer from `grep`, never
`cat` a log back.

### Task 5 verification — 2026-09-15

- `scripts/check-android-suite.sh`: 104 suites, 625 tests executed, 0 skipped,
  0 failures, and 0 errors; the debug APK assembled successfully.
- `npm --prefix android run lint`: successful with 0 errors and 0 warnings.
  Its two informational autoboxing hints predate this branch.
- The five-row physical-device check remains assigned to the orchestrator
  after merge, as decision 5 requires.

## Done when

- Both task-1 tests were red before task 2 and are green after it.
- The parent's own gestures (task 2 list) and the pager guards (task 3, three
  bands) are green.
- The measured-bounds ownership test is green in both layouts.
- Task 4's two assertions are green.
- `check-android-suite.sh` reports ≥ 334 executed tests, all passed, and
  `npm --prefix android run lint` is clean.
- After the merge, on the device (orchestrator, decision 5): a slow drag from
  anywhere on the bar moves the head live and seeks on release; a tap with a
  small wobble seeks. Reported as the same five-row table as in *Problem*.

## Out of scope

- A visible thumb (decision 3).
- The desktop `WaveformSeek`; it has no such parent.

## Parallelität

The candidate cut was **A: gesture consumption** (`NowPlayingGestures.kt`,
`NowPlayingGesturesTest.kt`, `MobileBottomTabsTest.kt`) versus **B: cancel
semantics** (`NowPlayingState.kt`, `MobileSurfaceViewModel.kt`,
`NowPlayingSheet.kt`, `BrowseSurfaceTest.kt`). The file groups are disjoint
only if B's sheet-level assertion (task 4, second bullet) leaves
`NowPlayingGesturesTest.kt` — and that assertion is the one that shows the
cancel path through the real slider, so it belongs where it is.

More decisive: B is ~25 lines, and both strands' evidence is the same
Android suite run plus Android Lint. Two worktrees mean two Gradle daemons
and two Robolectric suites on one machine for no wall-clock gain, and the
second `check-android-suite.sh` would need its own cargo release build of the
FFI crate. **Not cut — one strand, tasks 1–5 in order** (grill, decision 6).

Post-merge cross-checks: none beyond the device check in *Done when*.
