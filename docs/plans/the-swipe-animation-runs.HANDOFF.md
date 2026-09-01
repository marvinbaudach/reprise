# Handoff — the swipe animation runs

Branch `feature/the-swipe-animation-runs`, worktree
`/home/marvin/Projects/reprise-swipe-animation-runs`, branched from
`1f8eacadc8` (#796, the redesign). Full evidence in
`the-swipe-animation-runs.findings.md` next to this file.

## What was wrong and what changed

Two defects, both found on the device, both fixed and verified there.

1. **Every title sat 152 px left of centre at rest**, the first glyphs clipped
   off the screen. `nowPlayingTitleTranslation` carried a constant
   `-(ratio - 1) * width / 2` that compensated for nothing.
   `NowPlayingScene.kt` — the constant is gone, the parameter it needed with it.

2. **The 480 ms settle never rendered.** Two causes, both required:
   - `NowPlayingGestures.kt:123` `pointerInput(animationsEnabled)` froze the
     gesture callbacks at the first composition, so `onSettle` kept an ancient
     `currentIndex` and every settle after the first track change aimed at a
     panel the queue had left. The six callbacks now go through
     `rememberUpdatedState`, as the values beside them already did.
   - `NowPlayingPositionReconciler` only protected a running settle when the
     track id had changed in that same update. The transport publishes the index
     and the track a frame apart, and in that window the reconciler snapped —
     cancelling the settle through the Animatable's mutex.

## State

- Both fixes verified on a **release** build on `59100DLCQ006SB`: title centre
  541 (was 389), 31 motion frames over 550 ms with a clean ease-out tail (was 8
  frames, done at 256 ms with a jump), and the outgoing card no longer returns
  to the centre mid-exit.
- `scripts/check-android-suite.sh` **passes, no failures**.
  `NowPlayingPositionStateTest` and `NowPlayingPanelsTest` are extended by five
  assertions between them. Run the suite through that script, never through a
  bare `./gradlew :app:testDebugUnitTest`: in a fresh worktree the plain task
  fails every Robolectric class with `ExceptionInInitializerError at
  NativeLibrary.java:325`, because only the script builds
  `target/release/libreprise_android_ffi.so` and exports `LD_LIBRARY_PATH`.
- **Not committed yet.** Nothing is pushed.

## Still open

- **The incoming track shows a placeholder note instead of its cover**, on the
  neighbour panel during the drag and on the new current panel for at least
  1.5 s after the commit. The design has the neighbour carrying its own artwork.
  Mapped but not verified: the prefetch warms `AndroidArtworkSize.LIST`
  (`MobileSurfaceViewModel.kt:277-296`) while the panels request
  `NOW_PLAYING` (`NowPlayingScene.kt:333-338`), and those are separate LRU
  shelves keyed `(trackUri, size)` (`ArtworkCache.kt:13-20`). If that holds, the
  prefetch cannot warm the shelf the panels read from, and `seedVisual` draws
  the fallback until the async load returns. **Measure before believing it.**
- The user's own 0.1.74 is **not** back on the phone; the fixed release APK
  built here is, with a library it rescanned itself. Their own app data (play
  counts, favourites, listening journal) is **not** in that install — a release
  build is not debuggable, so `run-as` cannot restore into it. The backup is
  intact and untouched. Restore from
  `~/.cache/reprise-swipe-arms/restore-2026-09-01-2130/` (`user-0.1.74.apk` plus
  `appdata.tar`, taken 21:32, play counts and favourites included). The SAF grant dies with
  every uninstall but can be re-granted entirely over adb — tap "Choose folder
  again", "Use this folder", "Allow"; no need to hand the phone over.
- The phone's rotation is locked to portrait and `svc power stayon usb` is set.
  Both were set for measuring and should be undone.

## Traps that cost time here

- `pointerInput(key)` keeps its callbacks as long as the key holds. Values read
  through `rememberUpdatedState` beside stale callbacks look correct and hide it.
- `screenrecord` at 24 Mbit/s is itself load, and a `uiautomator dump` loop
  during a capture halves the frame rate. Take pacing numbers from
  `framestats` with nothing else running.
- A debug APK's frame times say nothing about a release one; an early pass here
  reported a 40 fps stutter that vanished entirely in a release build.
- The screen sleeping mid-capture looks exactly like a black animation.
- A worktree can disappear under you: `reprise-swipe-b-measure` was removed by
  another session mid-measurement.
