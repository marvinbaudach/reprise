---
slug: android-play-view-gestures
worktree: /home/marvin/Projects/reprise/.worktrees/android-play-gestures
branch: feat/android-play-gestures
phase: planned
codex_session:
created: 2026-08-10
---
# Android: the play view loses its fullscreen twin and gains swipes

Status: **planned — not started**
Base: `origin/dev` at `6c0646343a` (#395). Every file and line reference below
was read against that commit.
Target branch: `feat/android-play-gestures`
As of: 2026-08-10

## Product goal

The Android play view becomes one screen instead of two, it never shows a teal
substitute for a cover it has not loaded yet, and it is driven by swipes rather
than by a row of small buttons.

Four complaints from the user drive this, in their words:

1. Switching songs flashes a teal background during the transition.
2. The GUI *jumps* while switching.
3. Swipe control is wanted; the visualizer and its fullscreen view are not.
4. The shuffle button in the play view "does nothing".

## Decisions taken with the user (2026-08-10)

1. **Only the fullscreen burst goes.** The cover, the reactive cover fog
   (#394, landed the day before) and the spectral seek bar all stay. The play
   view becomes exactly what `transition == 0` renders today.
2. **The header loses two buttons.** No `fullscreen` (no destination left) and
   no `keyboard_arrow_down` — the downward swipe replaces it, with predictive
   back as the second way out. Queue, sleep timer and heart stay.
3. **No cover means a generated cover, never a block of accent colour.** Title
   and artist derive a deterministic colour pair, drawn as a soft gradient with
   a restrained note symbol. The fog behind it is made from that same image, so
   the whole surface follows.
4. **Covers are cached and prefetched.** Neighbouring queue entries are decoded
   ahead of time, the last one is kept, and neither cover nor fog is ever reset
   to null while a replacement is still loading.
5. **The play view becomes true fullscreen.** While it is open the bottom
   navigation bar is hidden; the tabs are reachable again only after swiping
   the play view down. This is what removes the conflict between a horizontal
   swipe here and the library's `HorizontalPager` behind it.
6. **Track swiping lives on the cover and its fog** — the upper half down to
   just above the seek bar — not on the whole surface. The sheet additionally
   consumes every pointer event, so the pager behind it can never move.
7. **Shuffle and repeat get a visible active state.** Whether the core also
   fails to shuffle is *measured on a device* before anything in the core is
   touched.
8. **The jump is measured before it is fixed.** The user could not say what
   moves. One cause is already certain and fixed regardless: the title block is
   free to take two lines and pushes everything below it.

## What the play view looks like when this is done

Full-bleed black. Cover at 34 % height with its fog behind it, title and artist
below, spectral seek bar at 69 % height, transport at the bottom: shuffle,
previous, play, next, repeat — the outer two showing whether they are on. The
header carries queue, sleep timer and heart only. Nothing fades out on idle;
without a fullscreen mode there is nothing to fade for.

Swipes: horizontally across the cover to change track, with the cover following
the finger and the neighbour sliding in; down anywhere to dismiss, with the
sheet following; double tap left or right for ∓10 s.

# Packages

Waves are barriers; packages inside a wave own disjoint files and run in
parallel. File ownership is binding — a package touches nothing outside its
list, and where two packages need the same file the later wave gets it.

| Package | Owns |
|---|---|
| P1 | `NowPlayingScene.kt`, `NowPlayingBurst.kt`, `MainActivity.kt`, `AmbientRuntime.kt`, `scene/SceneState.kt`, `scene/CoreShape.kt`, `scene/BandEnvelopes.kt`, `crates/reprise-android-ffi/src/appearance.rs`, their tests |
| P2 | `crates/reprise-core/src/visuals/fallback_cover.rs` (new), `visuals.rs`, `crates/reprise-android-ffi/src/fallback_cover.rs` (new), `lib.rs` |
| P3 | `TrackCover.kt`, `ArtworkCache.kt` (new), `FallbackCover.kt` (new), `CoverFogBitmap.kt`, `ArtworkRequestGate.kt`, `MobileSurfaceViewModel.kt` |
| P4 | `NowPlayingGestures.kt` (new), `PlayGestureState.kt` (new), `NowPlayingSheet.kt`, `BrowseScreen.kt`, `LibraryFrame.kt`, and `NowPlayingScene.kt` after P1 has landed |
| P5 | measurement report, then whatever it names |
| P6 | measurement report, then whatever it names |

## Wave A — P1 and P2 in parallel

### P1 — retire the fullscreen visualizer

Delete `NowPlayingBurst.kt` and, with it, `NowPlayingBurstTest.kt`,
`NowPlayingBurstPixelsTest.kt` and `MainActivityVisualizerTest.kt`.

In `NowPlayingScene.kt` (715 lines today, expect roughly 380 after):

- Remove `NowPlayingView`, `NowPlayingViewSettings`,
  `AndroidNowPlayingViewSettings`, `InjectedNowPlayingViewSettings`,
  `INJECTED_NOW_PLAYING_VIEW_KEY` and `LocalNowPlayingViewSettings`.
- Remove the `transition` animation and every `lerp` that reads it; the sizes
  it interpolated become the `transition == 0` constants.
- Remove `controlsVisible`, `controlAlpha`, `wakeControls`, `touchRevision`,
  `CONTROL_IDLE_MS`, `CONTROL_FADE_MS`, `CONTROL_HIT_EPSILON`, the wake catcher
  in `SceneTransport` and the `now-playing-controls-faded` / `-visible` test
  tags. Controls are simply always there.
- `SceneProgress` keeps only `SpectralSeekSlider`; `FullscreenProgress` goes.
- `SceneTransport` loses its double roles and becomes shuffle, previous, play,
  next, repeat. Shuffle and repeat get the filled `secondaryContainer`
  treatment `ModeButton` already uses in `NowPlayingSheet.kt:344`, so an
  enabled mode is visible. This is the part of complaint 4 that is certain.
- `drawNowPlayingBurst`, `rememberBurstBloomBuffer` and the `opacity`
  parameters that only existed for the cross-fade go.
- `DriveScene` is called **unconditionally**. Today `NowPlayingScene.kt:181`
  skips it while `frameCount == 0`, which tears the frame loop down and builds
  it up again on every track change — a jank source, and one of the suspects in
  complaint 2. `DriveScene` decides internally that an empty spectrogram has
  nothing to advance. Its `transitionRunning` parameter goes with the
  transition.

In `scene/SceneState.kt`, mind the trap: the fog's **rotation speed** is
`EnergyIntegrator.advance(fogAngleA/B, level, factor)` at lines 119-120, and
that `level` is `mean(burstBands)` — the fog turns on the *burst* envelopes.
Deleting them with the burst would leave the fog standing still in a track that
is playing.

So: keep `fogEnvelopes`, `fogBands`, `fogLevel`, `fogAngleA/B` **and** the
second envelope bank with its mean. Rename that bank and its mean to
`motionEnvelopes` / `motionLevel` (`BandEnvelopes.burst()` becomes
`BandEnvelopes.motion()` with the same coefficients) so the next reader is not
told it is burst state. Drop `coreShape`, `bass` and `transient` once the burst
renderer is gone — check for other readers first, do not assume. Delete
`scene/CoreShape.kt` and `scene/CoreShapeTest.kt` when no caller remains.
`AmbientRuntime.kt` loses `burstEffects` from the render power gate and keeps
`fogRotates`.

Pin the trap with a test: with a playing track and a non-empty spectrogram,
`fogAngleA` must keep changing across frames. That is the regression this
rename is there to prevent.

`MainActivity.kt` loses `nowPlayingViewSettings`, the injected variant and the
`LocalNowPlayingViewSettings` provider (lines 105, 107, 219).

On the Rust side, `crates/reprise-android-ffi/src/appearance.rs` loses
`now_playing_view_setting`, `set_now_playing_view`,
`AndroidStoredNowPlayingView`, `AndroidNowPlayingViewChoice` and their tests;
regenerate the UniFFI bindings. The stored settings row is left in the database
untouched: it is a single unread key, and a migration to delete it would be
more risk than the row costs.

Adjust `NowPlayingSceneVerificationTest.kt`, `SceneDriverTest.kt`,
`ScenePowerGateTest.kt`, `scene/SceneStateTest.kt` and
`scene/BandEnvelopeTest.kt` to the reduced surface. Add one test: the transport
row shows shuffle as active when `playback.shuffled` is true and inactive when
it is false.

**Done when** no `NowPlayingView`, burst or fullscreen symbol remains anywhere
in `android/app/src` or the FFI crate, the suite is green, and the play view
renders cover, fog, title, seek bar and transport as before.

### P2 — fallback cover colours in the core

New `crates/reprise-core/src/visuals/fallback_cover.rs`:

```rust
pub struct FallbackCoverColours { pub top: u32, pub bottom: u32 } // 0xRRGGBB

pub fn fallback_cover_colours(title: &str, artist: &str) -> FallbackCoverColours
```

FNV-1a over `artist.trim().to_lowercase()`, a `0x1f` separator, then the title
handled the same way. The hash picks a hue on the full circle; saturation is
fixed at 0.42, lightness 0.34 for the top and 0.18 for the bottom, and the
bottom hue is shifted by +34°. Reuse `visuals::color::{hsla_to_rgb, hue_shift}`
rather than writing new colour maths.

This lives in the core, not in Kotlin, because the project's rule is that
portable decisions belong to `reprise-core` and the GTK front end has the same
gap to fill. Kotlin only draws.

Tests: the same input always yields the same pair; 200 distinct
title/artist pairs spread across at least eight of twelve hue sectors; empty
title *and* empty artist yield a defined neutral grey; every produced colour
stays below a lightness ceiling so white text and a white symbol keep a
contrast ratio of at least 3:1.

New `crates/reprise-android-ffi/src/fallback_cover.rs` exports it as a free
UniFFI function — no library handle is needed for a pure function:

```rust
#[uniffi::export]
pub fn android_fallback_cover_colours(title: String, artist: String)
    -> AndroidFallbackCoverColours
```

**Done when** the core tests pass and the generated Kotlin binding is callable.

## Wave B — P3 needs P2, P4 needs P1

### P3 — the cover never goes teal

Three places produce the flash, all the same shape. `TrackCover.kt:173` resets
the visual to `null` on every `trackUri` change; `NowPlayingScene.kt:178-179`
then hands `colorScheme.primary` to `rememberCoverFogBitmap`, which builds a
**teal fog cloud**; and `drawPlayedCover` paints the same colour as a square
(`NowPlayingScene.kt:699`).

New `ArtworkCache.kt`: an LRU over `LinkedHashMap(accessOrder = true)`, twelve
`ArtworkVisual` entries keyed by URI plus size, six `CoverFogBitmap` entries
keyed by the artwork identity, `synchronized` on every access because the
artwork workers and the main thread both touch it.

New `FallbackCover.kt`: `fallbackCoverBitmap(title, artist, sizePx)` draws the
P2 colour pair as a vertical gradient with a low-contrast note glyph, and is
cached under `title + artist + size` like any other cover.

`TrackCover.kt`:

- `loadVisual` checks the cache first and delivers a hit immediately instead of
  going through a worker, so a cached cover produces no empty frame at all.
- A miss stores its result in the cache on the way out.
- When `resolve`/`decode` yields nothing, the fallback bitmap is produced and
  cached rather than a null visual returned. `ArtworkRequest` therefore has to
  carry title and artist; extend it, and keep the existing constructor callers
  compiling with defaults.
- New `prefetch(request)`: fills the cache and delivers to nobody.
- `rememberTrackArtworkVisual` seeds its state from the cache instead of
  `null`.

`CoverFogBitmap.kt`: `rememberCoverFogBitmap` keeps the previous fog until the
new one is ready — the `remember(artwork, fallbackArgb)` key reset at line 56
is what makes it disappear today — caches the result, and returns a cross-fade
fraction that reaches 1 over 180 ms so the canvas can blend the two.

`MobileSurfaceViewModel.kt` drives the prefetch: when `currentTrackId` changes,
read the next two entries through `loadUpcomingTracks(LibraryWindowRange(0, 2))`
and prefetch their covers and fog. Held-back covers plus the LRU cover the
backwards direction.

Tests: a cached cover yields a non-null visual on first composition; a track
without artwork yields the generated cover and never `colorScheme.primary`; the
fog survives a track change instead of going null; the prefetch populates the
cache for the following track; the LRU evicts the oldest entry.

**Done when** stepping through five tracks shows no teal at any point, verified
on a screen recording, and a track with no cover file shows its generated one.

### P4 — swipe control and true fullscreen

Starts after P1 has landed, because both edit `NowPlayingScene.kt`.

**Fullscreen first.** While the play view is open the bottom bar disappears:
`BrowseScreen.kt:388` stops reserving `navigationBarHeightDp` for it, and
`LibraryBottomFrame` (`LibraryFrame.kt:126`) slides its content out —
`translationY` plus alpha, *not* removal from the layout, because taking it out
would re-lay-out the library list behind the sheet and make it jump the moment
the sheet closes. The slide runs with the sheet's existing enter/exit
animation.

**A pointer blocker** as the sheet's lowest layer consumes every event, so the
library's `HorizontalPager` (`BrowseScreen.kt:441`) can never follow a swipe
meant for the player.

New `PlayGestureState.kt` — a plain JVM state machine, no Compose imports, so
the thresholds are testable without a device: current horizontal and vertical
offset, an axis lock once a direction is established, and a settle decision
from offset plus velocity.

New `NowPlayingGestures.kt` wires it up:

- **Horizontal**, in the zone from the top of the surface down to
  `maxHeight * 0.62f` — cover and fog, ending above the seek bar. The cover
  follows the finger, the neighbouring cover slides in from the cache P3 filled,
  the fog follows at 0.35 of the distance. Release past 25 % of the width, or a
  fling of 800 dp/s, calls `next()`/`previous()`; anything less springs back.
- **Downward**, anywhere except on the seek bar and the transport row — the
  sheet follows the finger through the same `graphicsLayer` that predictive
  back already drives (`NowPlayingSheet.kt:83`). Past 20 % of the height, or a
  fling, it closes.
- **Double tap** left or right of centre seeks ∓10 s, clamped to the track, with
  a 600 ms "−10 s" / "+10 s" marker. A single tap does nothing — there is no
  wake logic left for it to trip over.
- With system animations off, offsets do not follow the finger; the thresholds
  still fire.

Tests: the state machine covers thresholds, axis lock and spring-back; Compose
tests cover drag past the threshold calling `next`, drag below it not calling
it, a downward drag closing the sheet, a double tap on the left seeking
backwards, and — the regression that motivated the fullscreen change — a
horizontal drag inside the sheet leaving `pagerState.currentPage` untouched.

**Done when** all of that passes and the navigation bar is invisible while the
play view is open.

## Wave C — P5 and P6, both measure before they change anything

### P5 — the jump

Record the track change on the emulator (`mobile_start_screen_recording`,
switch, stop) and step through the frames. Report what actually moves before
editing anything.

Two causes are already known and are fixed regardless: `SceneTitle`
(`NowPlayingScene.kt:418`) allows two lines and pushes the artist and
everything below down, so the title block gets a fixed two-line height; and the
disappearing cover and fog from P3. Whatever the recording shows beyond that
gets its own diagnosis.

### P6 — does shuffle actually shuffle?

The core path is complete: `playback_session.rs:617` shuffles the queue, writes
`shuffled` into the snapshot and notifies. So the missing active state P1 adds
may be the whole complaint — and note that the change takes effect through
`set_next`, i.e. from the following track onwards, which by itself feels like
"nothing happened". Confirm on a device: does the snapshot come back with
`shuffled = true`, and does the queue order actually change? Only then decide
whether the core needs anything.

## Verification

- `JAVA_HOME=/usr/lib/jvm/java-21-openjdk` for every Gradle run. The system
  default is JDK 26 and Robolectric dies on it with 125 red tests that look
  like your own breakage.
- `testDebugUnitTest` reports BUILD SUCCESSFUL without running a thing when it
  is up to date. Check the test *count*, and the *suite* count, not the colour.
- Android FFI tests depend on readdir order; keep `TMPDIR` on tmpfs or four
  tests go falsely red.
- Cross-compilation only through `scripts/android-build.sh`; a bare
  `cargo build` fails in ring/cc-rs.
- Visual acceptance on the emulator `pixel10xl_api37`, never on the desktop:
  play view, a track change end to end, a track without a cover, each swipe.

## Out of scope

- The queue page, the dock mode and the wide-short layout keep their current
  behaviour; only their shared header buttons change with P1.
- A swipe-up-for-queue gesture. Offered, declined.
- Removing the spectral seek bar or the analysis loader that feeds it.
- Using the generated fallback cover in GTK. The core function is written so
  that it can, but wiring it there is a separate task.

## Risks

- **`SceneState` is more entangled with the burst than it reads** — confirmed,
  not hypothetical: the fog's rotation is driven by the burst envelopes' mean.
  P1 keeps and renames that bank; see the package for the test that pins it.
- **The prefetch reaches into the queue.** `loadUpcomingTracks` goes through
  the single playback query lane; two prefetched entries per track change is
  cheap, but it must not be issued per frame.
- **Hiding the navigation bar changes a layout other screens share.** The
  slide-out has to leave the library's measured height alone; a jumping list
  behind the sheet would trade one jump complaint for another.
