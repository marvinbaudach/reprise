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
- Committed as `756fde7066` on `feature/the-swipe-animation-runs`, pushed, and
  open as **#799** against `dev`.

## Still open

- ~~The incoming track shows a placeholder note instead of its cover.~~
  **Cause found, fixed in #801** on `fix/the-cover-does-not-fall-back`. The
  hypothesis this handoff originally recorded — separate LRU shelves for `LIST`
  and `NOW_PLAYING` — is **wrong**, and the summary at the top of
  `the-swipe-animation-runs.findings.md` lists the four pieces of evidence that
  refute it. The real cause is a downgrade: `resolveVisual` (`TrackCover.kt`)
  falls back to a *generated* cover whenever the full-size read comes back empty
  or its decode returns null, and delivers it over the real cover the seed had
  already put on screen. What looks like a placeholder note is that generated
  cover (`FallbackCover.kt`, a gradient with `drawRestrainedNote`), not
  `CoverPlaceholder`. Still unverified on a device.
- **Two smaller defects reported from the device, not yet worked on:** a white
  line at the top edge keeps moving after the cover has landed; and the
  neighbour panel shows a cover rather than the visualiser. The second is
  current behaviour by construction (`NowPlayingScene.kt`, `live = panel.index
  == currentIndex`), so changing it is a design decision and **needs the user's
  call**, not a fix.
- The user's own 0.1.74 is **not** back on the phone; the fixed release APK
  built here is, with a library it rescanned itself. Their own app data (play
  counts, favourites, listening journal) is **not** in that install — a release
  build is not debuggable, so `run-as` cannot restore into it. The backup is
  intact and untouched. Restore from
  `~/.cache/reprise-swipe-arms/restore-2026-09-01-2130/` (`user-0.1.74.apk` plus
  `appdata.tar`, taken 21:32, play counts and favourites included). The SAF grant dies with
  every uninstall but can be re-granted entirely over adb — tap "Choose folder
  again", "Use this folder", "Allow"; no need to hand the phone over.
- `svc power stayon usb` is still set and should be undone. The rotation lock is
  already gone (`accelerometer_rotation=1` as of 2026-09-02).

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
