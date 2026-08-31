---
slug: android-swipe-discards-the-whole-card
worktree: /home/marvin/Projects/reprise-android-swipe-discards-the-whole-card
branch: feature/android-swipe-discards-the-whole-card
phase: shipped
codex_session:
created: 2026-08-31
---
# The swipe discards the whole card

## What the user reported

> "Swipe für den aktuellen Song bei visualization ist nicht korrekt. Man sieht
> kurz das Cover des nächsten Songs. Eigentlich sollte die gesamte Karte
> weggeworfen werden"

Android only. There is no swipe or carousel call site under
`crates/reprise-gnome/src`; the gesture exists solely in the Compose app.

## What it actually is

The earlier note
[android-now-playing-swipe-shows-next-cover.TODO.md](./android-now-playing-swipe-shows-next-cover.TODO.md)
guessed a pager with pages and proposed "bind each page to its own track for the
page's lifetime". **That guess was wrong and its fix would have produced a
different bug.** There is no pager and there are no pages: one
`NowPlayingScene` is mounted (`NowPlayingSheet.kt:210-220`) and a single Canvas
draw position is offset by `horizontalOffsetPx`, with a neighbour preview painted
beside it (`NowPlayingScene.kt:210-228`).

The drag itself is clean. `NowPlayingGestures.kt:57-143` accumulates
`rawHorizontalOffset` while the finger is down and `track` does not change.
`settle()` is called once, after the pointer lifts (`:104-111`).

**Two independent defects fire at release**, and both must close:

**(a) The committed swipe never leaves the screen.** `NowPlayingSheet.kt:141-152`
calls `controls.next()` / `.previous()` and then animates `horizontalOffset` to
**`0f`** — the same target as a rejected swipe. There is no "animate off, then
flip" step. The outgoing card is told to return to centre at the same moment the
upstream track advances.

**(b) The content swap is not gated on the offset.** `track` is one shared value
(`BrowseScreen.kt:472-493` → `:711-719`), and the flip needs two async hops after
`controls.next()`: `playback.currentTrackId` changes, then a `loadTrack` round
trip. Whichever frame that lands on, `horizontalOffsetPx` is usually still ≠ 0 —
default `spring()` runs a few hundred ms, and `settle()` commits on a quarter of
the screen width *or* a fling velocity (`PlayGestureState.kt:72-79`), so a flick
can commit with the card nearly centred. That displaced frame showing the new
content is the reported flash.

**The invariant to establish:**

> A nonzero `horizontalOffsetPx` and a track flip must never be observable in the
> same frame.

## Three consumers, not one

A fix that re-binds only the cover bitmap leaves two visible glitches, so all
three move together:

- **Cover** — `artwork` (`NowPlayingScene.kt:96-101`), drawn at
  `playedCenter = center + horizontalOffsetPx` (`:170-173, 229-235`).
- **Fog** — `rememberCoverFogTransition(artwork?.image, …)` (`:118`), whose
  `LaunchedEffect(artwork, …)` (`CoverFogBitmap.kt:120-139`) starts a new
  crossfade the instant `artwork` changes. `NowPlayingFog.kt:76-79` documents
  that the film follows the drag offset, so it glitches at the same displaced
  position.
- **Visualizer** — `rememberVisualSceneEngine(trackId = track.id, …)` (`:122-126`)
  → `DisposableEffect(engine, trackId) { engine?.noteTrackChanged() }`
  (`:304-307`) resets the spectrum on the same flip.

Fourth, related discontinuity: `neighbours.next` is reset to `null`
(`NowPlayingGestures.kt:42-45`) before the reload completes, so the incoming
preview slot blanks in the frame the centre swaps.

## The fix

The user's own wording — *"die gesamte Karte weggeworfen"* — is the exit
animation, so take it, with the latch as the backstop it needs:

**1 — Exit before the flip.** For a committed `NEXT` / `PREVIOUS` in
`NowPlayingSheet.kt:141-152`, animate `horizontalOffset` to `∓width` **first**,
then call `controls.next()` / `.previous()`, then `snapTo(0f)`. `SPRING_BACK`
keeps today's animation to `0f` unchanged.

**2 — Latch the outgoing content for the whole settle window.** Alone, task 1
has its own failure mode: if the async flip lags past the exit animation, the
*old* cover reappears at centre after `snapTo(0f)`. So `remember` the track
identity that was current when `onSettle` fired and keep the centre draw slot
pinned to it while the settle window is open, releasing to the live `track` only
once the offset animation has completed and the new value has arrived. The latch
must feed **all three** consumers above, not just the `ImageBitmap`.

**3 — Respect reduced motion.** `PlayGestureState.horizontalOffset` returns `0f`
unconditionally when `animationsEnabled` is false (`PlayGestureState.kt:34-38`)
while `settle()` still commits from the raw offset (`:74-77`). With animations
off there is no window and no exit animation to run — task 1 must not introduce a
delay before `controls.next()` on that path, or the swipe would feel broken for
exactly the users who turned motion off.

## Deliberately not in scope

- **The `previous` asymmetry.** `rememberPlayGestureNeighbours`
  (`NowPlayingGestures.kt:29-48`) starts `previous = null` and fills it only after
  `track.id` has changed once, so a freshly opened sheet draws no incoming
  preview on a PREVIOUS swipe. Real, but a separate defect: it is about the
  preview slot, not about the flash, and fixing it here would hide which change
  caused which effect.
- **Gesture physics.** The quarter-width and fling thresholds
  (`PlayGestureState.kt:72-79`) stay as they are. The bug is what is drawn during
  the window, not when the window opens.

## Verification

**On-device discriminator, before any code changes.** Turn animations off. The
swipe still commits, but `horizontalOffset` never leaves 0, so the flash must
become impossible. If it still flashes with animations off, this whole diagnosis
is wrong and the plan stops here. Run this arm first — it is one toggle and it
either confirms the mechanism or saves the entire implementation.

**The regression test.** Contrary to the earlier note's assumption, this does not
need a screen recording — the existing Robolectric harness can express it.
`NowPlayingGesturesTest.kt` already drives `performTouchInput { down/moveTo }`
plus `compose.mainClock.advanceTimeBy(...)` (`:170, 190, 199, 218, 220, 244`), and
`NowPlayingSceneVerificationTest.kt` / `VisualizerScenePixelsTest.kt` /
`LibraryPositionRecompositionTest.kt` establish pixel sampling via
`GraphicsMode.Mode.NATIVE` and `captureToImage()`.

Compose them: drive a swipe past the NEXT threshold, let a fake `controls.next()`
flip the track after a delay that simulates the async hops, and assert that on
**every** clock tick where `horizontalOffset != 0` the centre cover rect still
matches the *outgoing* artwork. Assert the fog and the visualizer's
`noteTrackChanged()` in the same window — a cover-only assertion passes a
cover-only fix while two consumers still glitch.

Honest limit, to be stated in the test's own comment: the fake picks its own flip
timing, so the test proves the invariant holds *regardless of when the flip
lands*. It is a regression pin, not a reproduction of real device latency.

**Then the phone**, same swipe, animations on: no frame in which a displaced card
shows the incoming track.

## Parallel execution

**No cut. One strand.** Tasks 1 and 2 are one compile in one file
(`NowPlayingSheet.kt`) plus the draw slot it feeds, and task 2 exists precisely to
cover task 1's failure mode — split, each half is a worse bug than the one being
fixed. Task 3 is a branch inside task 1. The test reads the behaviour all three
produce.
