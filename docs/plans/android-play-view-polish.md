---
slug: android-play-view-polish
worktree: ~/Projects/reprise/.worktrees/android-play-gestures
branch: feat/android-play-gestures
phase: planned
codex_session:
created: 2026-08-10
---
# Android: the queue leaves, the shadow softens, the fog keeps breathing

Status: **planned** — follows the play-view rebuild in the same worktree, on top
of its four commits.
As of: 2026-08-10

Three things the user found on the device, in one pass because they all live in
the play view.

# Q — the queue leaves the play view

Nothing takes its place. The user was told what that costs — the queue is
reachable nowhere else on the phone, so reordering, swipe-to-remove and "play
this one now" go with it — and chose it anyway. The code behind those actions
goes too, rather than rotting behind a button nobody can press.

**Remove:** `NowPlayingQueue.kt` and `NowPlayingQueueTest.kt` whole;
`QueuePageButton` and every call, in `NowPlayingSheet.kt` (stacked *and*
wide-short) and in the header of `NowPlayingScene.kt`; `nowPlayingQueueVisible`
and `showNowPlayingQueue` in `MobileSurfaceViewModel.kt` with the block that
drew the queue over the cover; `playUpcomingTrackNow`, `moveUpcomingTrack` and
`removeUpcomingTrack` from `PlaybackControls.kt`,
`ActivityPlaybackControls.kt`, `ReprisePlaybackService.kt` and their UniFFI
wrappers; the swipe-to-remove branch and `QUEUE_REMOVE_FRACTION` in
`LibraryTrackRows.kt` *if* the queue page was its only caller — check first;
whatever in `MainActivityConfigurationTest.kt` and `MobileSurfaceStateTest.kt`
existed only to cover the above.

**Keep, do not touch:**

- **`loadUpcomingTracks`.** The artwork prefetch reads the next queue entries
  through it; removing it puts the teal flash back.
- **`reprise-core`.** Only the Android FFI wrappers go — the desktop drives the
  same queue through the same core functions. A red core test means something
  was removed that was not the phone's.

Afterwards grep the removed names across `android/app/src` and the FFI crate:
no reference may survive, test tags and strings included.

# S — the cover's shadow is a staircase

`NowPlayingScene.kt:427-433` fakes a drop shadow with five stacked
`drawRoundRect`s, each 2 dp wider and 3 dp lower than the last. Against the
bright fog of a light cover — the user's screenshot shows a white Lorna Shore
sleeve — every step is a visible hard edge. It reads as five shadows, because
it is five shadows.

Replace it with one soft shadow. Preferred: prepare a blurred black rounded-rect
texture once, the same way `CoverFogBitmap` prepares the fog — off the main
thread, drawn per frame only as a scaled bitmap. That keeps the rule this screen
already follows, that no filtering happens inside a frame, and it does not
depend on which blur the hardware canvas honours. A `BlurMaskFilter` through
`drawIntoCanvas` is acceptable if it is verified to blur on a hardware canvas
rather than silently doing nothing.

Shape: roughly 28 dp of blur, 14 dp lower than the cover, around 0.45 alpha at
the centre — tune it against a white cover, which is the case that exposed it.

Pin it: a pixel test that samples a vertical line below the cover and asserts
the alpha falls **monotonically**. Five stacked rectangles fail that; one blur
passes it.

# F — the fog must stay alive while paused, and move more with music

Today the rotation is `EnergyIntegrator.advance(angle, motionLevel, factor)`
(`SceneState.kt:100-101`), so with no signal there is no movement at all — and
the frame loop stops outright while paused (`SceneDriver.kt:154`). A paused
screen is a still photograph.

Two changes:

1. **A base drift, independent of the signal.** The fog turns on its own, slowly
   — about one full turn every four minutes for the wide layer, the tight one
   counter-rotating as it does now.
2. **A wider musical range on top.** Under strong music the rotation must be at
   least five times the base drift, so the difference between silence and a
   chorus is obvious rather than a nuance. The two layers keep their opposing
   factors, so the haze churns instead of spinning as one disc.

The frame loop therefore runs while paused as well, but throttled: no more than
20 frames a second with no signal, and still behind the existing power gates —
screen off or system animations disabled stops it completely. Battery is the
reason the loop stops today, so the throttle is not optional.

Tests: with playback paused and a non-empty spectrogram, `fogAngleA` keeps
changing across frames; with a strong signal the per-second change is at least
five times the paused one; with `sceneFramesAllowed` false nothing advances;
the paused loop requests no more than 20 frames a second.

## Verification for all three

`JAVA_HOME=/usr/lib/jvm/java-21-openjdk`; delete
`android/app/build/test-results/testDebugUnitTest` first, confirm the XMLs are
fresh afterwards, and report suite *and* test counts. The baseline to beat is
**49 suites, 229 tests, 0 failures** — measured on this branch on 2026-08-10,
after the play-view rebuild. `cargo test -p reprise-android-ffi` and `-p
reprise-core` stay green.
