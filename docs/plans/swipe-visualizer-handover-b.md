---
slug: swipe-visualizer-handover-b
worktree: /home/marvin/Projects/reprise-swipe-visualizer-handover-b
branch: feature/swipe-visualizer-handover-b
phase: shipped
codex_session:
created: 2026-10-07
---
# Strand B (Kotlin): neighbour cards mirror the live engine

Mother plan: `swipe-visualizer-handover.md`. Read its Problem, Goal and Decisions first. This
strand implements decisions 2 and 3.

## File ownership

- `android/app/src/main/java/io/github/marvinbaudach/reprise/{NowPlayingScene,NowPlayingSceneModel,VisualizerScene,SceneDriver}.kt`
- any Kotlin file that only constructed the removed neighbour engine
- their tests under `android/app/src/test/**`

Do **not** touch `crates/**`. If the removal needs an FFI change, stop and report it. Do not work
around it; the change moves to strand A.

## Tasks (test first)

1. **Neighbours mirror the live engine (decision 2).**
   - Failing Robolectric test in `NowPlayingSceneEngineTest` or `NowPlayingPanelsTest`: a panel
     adjacent to the live panel, whose track **has** stored spectrogram frames, draws the live
     engine's tinted scene bytes.
   - It never reads `SceneDriver.fallbackBands` at the shared playhead, both while dragged and
     after `controls.next()` has reset the playhead, before `currentIndex` flips.
   - Implement it in `NowPlayingScene.kt` / `NowPlayingSceneModel.kt` (today's branch at
     `NowPlayingScene.kt:381-391`, `NowPlayingSceneModel.kt:155-175`).
2. **Remove the neighbour's private engine (decision 3).**
   - First establish whether anything other than the neighbour preview reads the private
     `AndroidVisualEngine` built by `NativeVisualSceneEngineFactory` (`VisualizerScene.kt:110`,
     chosen at `NowPlayingScene.kt:465-468`) and its stored-frame driver. Candidates: frozen
     scenes and `FrozenSceneBytes`, pause and stop (AC-27), and the cover/spectrum toggle.
   - If nothing reads it: remove it, together with the stored-frame neighbour feed, and delete or
     rewrite the tests that only pinned it.
   - If something does: keep it. Write the reason into this file under "Outcome", and keep only
     the mirror from task 1.
3. **Regression pins.** The existing `NowPlayingPanelsTest`, `NowPlayingSceneEngineTest`,
   `NowPlayingPanelFrozenSceneIdentityTest`, `FrozenSceneBytesTest` and `VisualizerSceneDriverTest`
   stay green, or are changed only where they pinned the removed behaviour. Name each changed
   test in the commit message.

## Gates

`scripts/check-android-suite.sh`, never raw gradlew: `android-build.sh` builds for the device and
makes Robolectric tests falsely red. Run the Rust gates only if a shared file changed, which this
strand must not do.

## Outcome

Removed. No reader other than the neighbour preview used the private stored-frame engine:
`visualSceneFactoryForPanel` now returns a factory only for the live panel, so a neighbour owns no
engine and mirrors `liveScene.engine` (tinted) whenever it is on screen. `FrozenSceneBytes` keeps
the mirrored picture across the neighbour's move into the live slot. `NativeVisualSceneEngineFactory`
itself stays — `MainActivity` uses it as the live factory.
