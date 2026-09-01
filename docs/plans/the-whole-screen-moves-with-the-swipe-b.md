---
slug: the-whole-screen-moves-with-the-swipe-b
worktree: /home/marvin/Projects/reprise-the-whole-screen-moves-with-the-swipe-b
branch: feature/the-whole-screen-moves-with-the-swipe-b
phase: shipped
codex_session:
created: 2026-08-31
---
# Strand B — the whole Compose surface

Mother plan: [`the-whole-screen-moves-with-the-swipe.md`](the-whole-screen-moves-with-the-swipe.md).
**Read it first, and keep it open.** Every formula, threshold, duration, easing
curve and colour token this strand needs is written there, because Codex cannot
reach the design project they came from. Nothing may be invented or looked up
elsewhere.

**Lands second, after strand A.** It needs A's symmetric window to render a
previous panel and A's `currentIndex` to seat `pos`.

`worktree.sh` branches hard off `origin/dev` (`worktree.sh:33`). **Create this
worktree only after strand A has landed in `dev`**, so the base already carries
A. Do not merge A's branch by hand. Nothing from the three abandoned swipe
branches is reused.

## File ownership

```
android/app/src/main/java/de/reprise/spike/NowPlayingSheet.kt
android/app/src/main/java/de/reprise/spike/NowPlayingScene.kt
android/app/src/main/java/de/reprise/spike/PlayGestureState.kt
android/app/src/main/java/de/reprise/spike/NowPlayingGestures.kt
android/app/src/main/java/de/reprise/spike/NowPlayingFog.kt
android/app/src/main/java/de/reprise/spike/NowPlayingShimmer.kt
android/app/src/main/java/de/reprise/spike/SceneState.kt
android/app/src/main/java/de/reprise/spike/CoverFogBitmap.kt
android/app/src/main/java/de/reprise/spike/SpectralSeekTrack.kt
android/app/src/main/java/de/reprise/spike/QueueHaptics.kt
+ new files for the top-edge accent line and sweep, and the play-button pulse
```

Do not touch the Rust crates, the FFI, `PlaybackControls.kt`,
`ReprisePlaybackService.kt`, `PlaybackUiState.kt`, `TrackAnalysisLoader.kt` or
`MobileSurfaceViewModel.kt` — strand A owns them and has already landed.

## Why this is one strand and not two

The draft cut the cues off as a third strand. The disjointness check killed it:
the seek marker needs `thumb = {}` inside `NowPlayingSheet.kt`, the accent line
and the sweep sit at the top edge of that same layout, the ring sits on the play
button, and the haptic hangs off the commit in `PlayGestureState`. The accent
line must also read `dev` live and share the `0.22` threshold **literally** — two
copies of that constant would let the pre-indicator promise a commit that does
not happen.

If one Codex run cannot carry this, split it **sequentially in this same
worktree**: B1 = tasks 1–4, B2 = tasks 5–6. Do not open a second parallel
worktree.

## Task 1 — the `pos` model replaces offset-plus-latch

Delete the `horizontalOffset` `Animatable` and the `outgoingTrack` latch. They
carry two CRITICAL defects that are not worth repairing: a card that stays stuck
off-screen when `next()` does not change the track id, and a second touch
cancelling the exit coroutine before the transport call.

In their place, the single state from the mother plan:

```
pos  = index * screenWidth + dragDelta
f    = pos / screenWidth
dev  = pos - index * screenWidth
```

`index` comes from A's `currentIndex` on `LibraryPlayback`. No layer runs an
animation of its own. **While the finger is down there is no transition at all.**
On release exactly one settle animation moves `pos` to `target * screenWidth`.

Commit rule, rubber band, velocity floor and the two replaced constants
(`TRACK_DISTANCE_FRACTION`, `TRACK_FLING_DP_PER_SECOND`) are specified in the
mother plan under "Commit rule". Note in particular the **unit trap**: the
design's `0.55 px/ms` is physical px, the existing constant is dp. Convert
explicitly and leave exactly one constant, named in one unit.

Previous/next buttons run this same path. No second code path (mother plan,
"What is explicitly not wanted").

Backwards navigation goes through the queue-order entry point strand A exposed
(G1/G2), not through `previous_from_history()`. The swipe and the previous button
are the two surfaces A deliberately left to this strand; `CoreControlledPlayer`
and `DockMode` were already routed by A.

## Task 2 — per-panel hosting (G3)

Render ±1: the centre and one neighbour on each side. Rendering ±2 is pointless —
box opacity is `max(0, 1 - dist * 0.75)` and hits 0 at `dist = 1.333`. A's
prefetch reaches ±2 so the *next* neighbour has data; rendering does not.

Each panel is its own `Box` with `graphicsLayer` (scale, rotation, alpha),
`Modifier.blur` and a saturation `ColorFilter`. Cover, visualizer and fog draw
routines must be re-hosted per panel. All per-panel scalars are in the mother
plan's table.

Panel `i` sits at `i * screenWidth - pos`. That is the only positioning rule; the
title row and the glow are the same displacement times their own factor. The
waveform row does not travel at all.

Blur is API 31+ and `minSdk` is 26; `Modifier.blur` is silently inert below it
(`OilFilmPalette.kt:58`, `AmbientSurface.kt:178` already live with this).
Accepted: on API 26-30 neighbours stay sharp but are still scaled, desaturated
and faded. Saturation works everywhere.

Neighbour content per box mode is decision G6 in the mother plan: in visualizer
mode the neighbour shows **its own cover fading out as its bars rise**, not a bar
skeleton and not a neutral plate. `NowPlayingNeighbourScaffold` is not carried
over.

## Task 3 — fog and shimmer become one per-panel layer (G7)

Delete the `previous`/`current` fog and shimmer pairs and their `fraction`
crossfade (`NowPlayingScene.kt:180-209`), and delete
`FOG_SWIPE_DISTANCE_FACTOR = 0.35f` (`:605`, applied at `:175`). Distance is the
crossfade now: one fog per panel, translated by `(i - f) * screenWidth * 0.23`,
faded by `max(0, 1 - |i - f| * 1.1)`.

The shimmer is already anchored to its fog's centre
(`drawNowPlayingShimmer(fog, center, …)`, `NowPlayingShimmer.kt:71`), so it
travels with its panel and takes the same opacity. No separate treatment.

**Do not add a hue rotation.** The design's "hue per track" is the palette the
app already derives from artwork and blends toward `VisualizerRampPalette` by
`visualizerOpacity`. Per-track fog identity is preserved by that, not by a new
mechanism.

`0.23` is one named constant, retunable in a single edit.

## Task 4 — auto-advance and queue edits (G8)

There is **no guard today** — nothing resets the offset animatables when the
track changes externally. This is new work.

- **Idle:** an external `currentTrackId` change animates `pos` to
  `index * screenWidth` with the same 480 ms settle as a commit.
- **Mid-drag:** re-anchor the drag origin against the new index so `dev` restarts
  at 0. Without this, `dev` jumps by almost a screen width and a small forward
  drag commits backwards.
- **A queue edit that leaves `currentTrackId` alone:** re-seat `index` and reload
  the panels with **no motion at all**. A player that slides because something
  was enqueued elsewhere reads as a bug.

Do not try to distinguish an automatic advance from a user-initiated one — both
arrive as a new value in the same `LibraryPlayback` snapshot and no flag exists.
None is needed.

Reuse the `generation`-counter shape from `NowPlayingQueue.kt:47,64-66` to
discard reload answers a mid-flight edit invalidated.

## Task 5 — the seek track and its marker (G5)

Pass `thumb = {}` to the `Slider` at `NowPlayingSheet.kt:363-376` and draw the
design's marker inside `SpectralSeekTrack`, which already receives `displayed`
and `durationMs`: a 3 px `accent-200` bar at the played fraction with a soft
accent glow.

**Keep the `Slider` itself.** It carries drag-to-seek, `onValueChangeFinished`,
the `enabled` gate and the accessibility semantics. Removing it silently drops
TalkBack support.

Keep the existing alpha encoding — `PLAYED_ALPHA = 0.96f` vs
`REMAINING_ALPHA = 0.34f` (`SpectralSeekTrack.kt:33-34`, applied at `:69-76`).
The design has both the tint and the marker; the app's alpha is the tint's
equivalent.

## Task 6 — the four confirmation cues

Specified in full in the mother plan. All four fire together and **only on a real
track change**, never on first render — including the waveform build, which the
prototype leaves ungated but the prose does not. The prose wins.

The Compose equivalent of the prototype's alternating animation names is keying
on the track id. Precedent that already skips the first composition:
`CoverFogBitmap.kt:120-139` guards with `if (hadCurrent)` and `snapTo`s otherwise.

The accent line reads `dev` live and **must use the same `0.22` constant the
commit rule uses** — not a copy. It reaches full scale exactly at the threshold,
which is what makes it an honest pre-indicator.

Haptics reuse `QueueHaptics.kt` (G9): add a `commit()` pulse,
`longArrayOf(0, 12)` with `HapticFeedbackType.TextHandleMove` as the fallback.
That file already runs a real `Vibrator`, falls back to `LocalHapticFeedback`,
and consults `Settings.System.HAPTIC_FEEDBACK_ENABLED` itself (`:86-92`).
`VIBRATE` is declared (`AndroidManifest.xml:29`). Do not add a second haptic path.

## Task 7 — the shadow defect

`drawPlayedCover` (`NowPlayingScene.kt:565-595`) draws the cover shadow **before**
the `if (opacity <= 0f) return` guard, and `drawCoverShadow` takes no alpha.
Every damped cover therefore keeps a full-strength shadow — a dark silhouette
with nothing inside it. With per-panel opacity reaching 0.25 and below this
becomes visible on every swipe, not just in the visualizer case where it was
first found.

## Verification

- **Reduced motion is not covered today.** The test the draft named,
  `reducedMotionAdvancesImmediatelyBecauseThereIsNoExitWindow`, does not exist
  anywhere in the tree. Write it. With `sceneAnimationsEnabled == false` the
  parallax and all four cues degrade to an immediate track change with no motion.
  The nearest existing test is `ScenePowerGateTest.kt:9-22`.
- **Rest state is bit-exact**: at `dev == 0`, no rotation matrix is applied,
  opacity is a bit-exact `1f`, translation is exactly `0f`. Assert it.
- The commit threshold and the accent line's full scale come from one constant.
  Assert they are the same constant, not that two values are equal.
- The waveform build fires exactly once per real track change and not on first
  render.
- Auto-advance mid-drag does not commit backwards — a regression test against the
  `dev`-jump described in task 4.
- A queue edit that leaves the current track alone produces no motion.
- Every regression test must be RED against the state it indicts before it is
  green against the fix. A test that was never red proves nothing.

**On-device arm.** The Pixel's three animation scales must be `1.0` before any
recording. At `0` the app honours reduced motion and *both* arms capture an empty
animation, which reads as "no change" while nothing was measured — this has
already cost this work once. The measuring script checks the scale and refuses
otherwise.

The device currently runs a debug build 0.1.74 from an abandoned branch;
reinstall before judging anything. The debug APK needs a fresh arm64
`libreprise_android_ffi.so` or the app dies on launch with `UnsatisfiedLinkError`.
Card rest bounds on that device: left 208, right ≈870, centre 540, width ≈662 of
1080 — the left edge is the reliable signal. Keep the swipe inside those bounds
(`input swipe 700 770 260 770 250`); a wider one is caught by the tab pager and
jumps to the Queue tab.

## Definition of done

The finger moves the whole screen — glow, box, title and artist — with the title
running faster than the box, and the waveform staying put while fading. Releasing
past 22 % of the screen width or above 0.55 px/ms commits with one 480 ms settle;
below that it springs back. Neighbours are really there on both sides, blurred and
desaturated, showing their own cover in either box mode. The four cues fire once
per real change and never on first paint. With animations off, none of it runs
and the track changes immediately.
