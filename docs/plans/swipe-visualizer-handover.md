---
slug: swipe-visualizer-handover
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-07
strands: a,b
merge_order: a,b
---
# A swipe hands the spectrum over in one movement (#1197)

## Problem

On the phone, a committed horizontal swipe in Now Playing with the visualizer on shows three
pictures that belong to neither song before the new song's bars appear. Measured on device
(0.1.228, two runs, AFTER THE SILENCE → Interlude, logcat aligned to video, times from release):

| time | card shows | cause (code read on origin/dev `e6524d5b84`) |
|---|---|---|
| drag | the incoming song's **stored spectrogram at the outgoing song's playhead** | every panel gets the shared `playback`; `SceneDriver.fallbackBands` reads the neighbour's frames at that position (`SceneDriver.kt:227-233`, `NowPlayingScene.kt:157`), clamped to its last frame (`SpectrogramFrames.kt:23-27`) |
| −140 … +200 ms | **empty card**: bars 0, caps frozen mid-height | `controls.next()` at release (`NowPlayingSheet.kt:237`) resets the shared playhead to 0, so the neighbour now shows its quiet intro frame. It ingests only on a new frame index (`SceneDriver.kt:152-154`), and caps decay only inside `ingest` (`engine.rs:16, 274-280`), so the caps freeze. `currentIndex` flips only on the transport answer (`NowPlayingSheet.kt:278-285`, grace 1500 ms) |
| +110 … +450 ms | the **outgoing song's shape** | the #1165 adoption, applied at the flip (`NowPlayingScene.kt:418-454`); it falls by gravity (`FALL_STEP` 0.028, `smoothing.rs:3`) |
| ≈+410 … +560 ms | **drop** close to zero | a stream reset re-arms the boundary with the loud gain carried (`arm(true)`, `cava.rs:219-222`); while `Waiting`, the gain is held until one 8192-sample window has filled (`boundary.rs:379`), so the quiet intro draws at the loud song's gain |
| ≈+560 … +800 ms | **one broad over-tall frame**, taller on the right | `Goal(M)` raises the gain ×1.6 per frame (`GOAL_RISE_PER_FRAME`, `smoothing.rs:9, 106-118`). The brake check uses the pre-rise gain (`smoothing.rs:97-101`), so one frame overshoots. cava's EQ tilt makes it rise to the right |
| ≈+0.7 … 0.85 s | the new song's real bars | |

The engine-only metric from the earlier phone check (pinned frames 13–32 → 0–3) did not see any
of this.

## Contract

AC-29 [active], `docs/ux-rules.md` ≈4974-4976: *a swipe hands the new song's bars the outgoing
song's last live shape, never a decayed one.* That paragraph is the only handover rule. Nothing
governs what a neighbour card shows during the drag, and nothing governs the gain step after the
handover.

## Goal

From the first drag frame to the new song's settled bars, the card shows **one continuous
movement**: the outgoing live shape, carried across the swipe, morphs into the new song at the
new song's gain. Concretely, over release → +1.5 s:

1. **No empty card.** No frame where the visible card's bars are near zero while the outgoing
   song was audible and the incoming song is above its own floor.
2. **No foreign preview.** The incoming card never shows the incoming song at the outgoing
   song's playhead.
3. **No drop below the target.** On loud → quiet, the bars do not fall below the new song's own
   settled level before rising to it.
4. **No overshoot.** No frame's mean level is above 1.3× the larger of the two songs' settled
   levels.

## Decisions (grill, 2026-10-07)

1. **Continuity, not preview.** AC-29's handover sentence stays as written. The preview
   alternative (the incoming card shows its own opening from stored frames) was rejected: a quiet
   intro would preview as a nearly empty card, it would change AC-29, and it would not remove the
   gain step.
2. **A. A neighbour of the live panel mirrors the live engine.** The incoming card draws
   `liveScene.engine.sceneBytesTinted` while it is dragged and while it settles, even when its
   track has stored frames. This is the existing no-frames branch (`NowPlayingScene.kt:381-391`,
   `NowPlayingSceneModel.kt:155-175`), made unconditional. The card that lands already carries
   the shape the flip adopts.
3. **The neighbour's private stored-frame engine is removed** (`VisualizerScene.kt:110`,
   `NowPlayingScene.kt:465-468`), provided nothing else reads it (frozen scenes,
   `FrozenSceneBytes`, pause). If something does, it stays, and strand B records why.
4. **B. The live engine holds its last live shape from release to the flip.** A
   `resetAudioStream` between `controls.next()` and the transport answer must not decay or clear
   the displayed shape. It holds the same `adoptableBands()` source the adoption reads.
5. **C1. The adopted shape holds until the first measurement decides.** While the boundary is
   `Waiting` after a track change and the engine carries an adopted shape, the display holds that
   shape instead of letting it fall by gravity at the carried gain. That is at most one window,
   ≈0.19 s. When the window decides, the smoother moves to the new song's bars:
   - `Done` within `CARRY_BAND`: at the carried gain.
   - `Goal(M)`: at the new gain.

   The rejected options were a soft fade, which still dips on loud → quiet, and dropping the
   carry, which would bring back the swell fixed in #1176.
6. **D. A gain rise cannot overshoot.** The brake must see the post-rise gain; the other option
   is to clamp the risen gain at the measured target `M` (`smoothing.rs:97-118`).
7. **Caps follow the morph.** While the held shape moves into the new song's bars, the caps come
   down with the bars instead of falling at `PEAK_FALL` 0.018 per ingest (`engine.rs:16,
   274-280`). Normal playback keeps today's cap fall.
8. **Device acceptance against a control arm under the same load.**
   - Record the baseline on the APK installed now first, then install the fix over it. The fix
     carries a higher version code; a downgrade would need an uninstall, which is not allowed.
   - The #1198 backfill loop, if it is still running, is recorded as a caveat. It does not block
     the acceptance.
9. **Two strands:** A (Rust, decisions 4–7 plus the AC-29 text) lands first, then B (Kotlin,
   decisions 2–3). If B turns out to need `crates/reprise-android-ffi/src/visualizer*.rs`, it
   stops. The change moves to strand A, and B resumes after A has landed.

## Tasks

Strand files hold the tasks:

- `swipe-visualizer-handover-a.md`: Rust: hold through a reset, no overshoot, hold while `Waiting`, caps follow, AC-29 text.
- `swipe-visualizer-handover-b.md`: Kotlin: neighbours mirror the live engine, the private neighbour engine is removed.

## Parallelität

- **Strand A (Rust)** owns:
  - `crates/reprise-core/src/playback/cava/**`
  - `crates/reprise-core/src/visuals/engine.rs` and its tests
  - `crates/reprise-android-ffi/src/visualizer*.rs` and `crates/reprise-android-ffi/src/live_audio.rs`
  - the AC-29 section of `docs/ux-rules.md`
- **Strand B (Kotlin)** owns:
  - `android/app/src/main/java/io/github/marvinbaudach/reprise/{NowPlayingScene,NowPlayingSceneModel,VisualizerScene,SceneDriver}.kt`
  - any Kotlin file that only constructed the removed neighbour engine
  - their tests under `android/app/src/test/**`
- The groups are disjoint. B goes green without A, because the mirror works with today's engine.
- **Merge order:** A, then B.
- **Post-merge cross-checks**, after both have landed:
  1. Every test the AC-29 text names exists, in either strand.
  2. Device acceptance with `~/.cache/reprise-scratch/viz-phone/sw.sh` under the device lock, with logcat and 50 ms frame sheets.
     - Swipes: loud → quiet (AFTER THE SILENCE → Interlude) and quiet → loud (Interlude → A Dead Current), two runs each.
     - Arms: the baseline APK first, then the fixed one.
     - Pass condition: goals 1–4 hold frame by frame on the fixed arm and fail on the baseline arm.
