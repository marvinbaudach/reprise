---
slug: the-whole-screen-moves-with-the-swipe-a
worktree: /home/marvin/Projects/reprise-the-whole-screen-moves-with-the-swipe-a
branch: feature/the-whole-screen-moves-with-the-swipe-a
phase: planned
codex_session:
created: 2026-08-31
---
# Strand A — the neighbour data

Mother plan: [`the-whole-screen-moves-with-the-swipe.md`](the-whole-screen-moves-with-the-swipe.md).
**Read it first.** It carries every number, the design tokens Codex cannot look
up, and the decisions settled in the grill (G1–G9).

**Lands first.** Strand B cannot render a previous panel without this window and
cannot seat `pos` without the absolute index.

`worktree.sh` branches hard off `origin/dev` (`worktree.sh:33`). Nothing from the
three abandoned swipe branches is reused.

## File ownership

```
crates/reprise-core/src/queue.rs
crates/reprise-core/src/queue_boundary.rs
crates/reprise-core/src/playback_session.rs
crates/reprise-android-ffi/src/**
android/app/src/main/java/de/reprise/spike/PlaybackControls.kt
android/app/src/main/java/de/reprise/spike/ActivityPlaybackControls.kt
android/app/src/main/java/de/reprise/spike/ReprisePlaybackService.kt
android/app/src/main/java/de/reprise/spike/PlaybackUiState.kt
android/app/src/main/java/de/reprise/spike/TrackAnalysisLoader.kt
android/app/src/main/java/de/reprise/spike/MobileSurfaceViewModel.kt
android/app/src/main/java/de/reprise/spike/CoreControlledPlayer.kt
android/app/src/main/java/de/reprise/spike/DockMode.kt
```

Do not touch `NowPlayingSheet.kt`, `NowPlayingScene.kt`, `PlayGestureState.kt`,
`NowPlayingGestures.kt`, `SpectralSeekTrack.kt`, the fog/shimmer files or
`QueueHaptics.kt` — strand B owns all of them.
`crates/reprise-gnome/**` is out of scope by decision G2.

## Why this strand exists

There is **no backward window API**.
`PlaybackControls.loadUpcomingTracks(LibraryWindowRange(offset, limit))`
(`PlaybackControls.kt:45`, `ReprisePlaybackService.kt:251`) fetches forward only.
Searched and not found: `loadPrevious`, `previousTracks`, `historyTracks`, any
backward window.

Today `rememberPlayGestureNeighbours` (`NowPlayingGestures.kt:29-48`, strand B's
file) gets its "previous" by remembering the last `LibraryTrack` it saw in this
composition:

```kotlin
LaunchedEffect(track.id, controls) {
    if (rememberedTrack.id != track.id) {
        previous = rememberedTrack
        rememberedTrack = track
    }
    next = null
    controls.loadUpcomingTracks(LibraryWindowRange(0, 2)) { outcome ->
        next = outcome.getOrNull()?.rows?.firstOrNull()
    }
}
```

So before the user has moved forward once, and after every app restart, there is
no previous panel at all. The design's model assumes a list indexable in both
directions. This strand makes that true.

## Tasks

### 1. A symmetric queue window

Extend the queue read to a window **around** the cursor rather than clamped
forward from it. `upcoming_tracks` (`queue_boundary.rs:56`) is already the right
loop; it is only clamped forward from `queue_window_start`
(`queue_boundary.rs:283`). The building blocks need no new data structure:
`queue.rs:605 current_order_position`, `:612 id_at_order_position`,
`:622 jump_to_order_position`, `:672 ids_in_order`.

Expose it through the FFI and up to `PlaybackControls` as a window that can take
a negative offset, or as an explicit `(before, after)` pair — pick one shape and
keep it; do not add a second forward-only call beside the existing one.

Clamp at both ends: at queue position 0 there is no previous, and the caller must
be able to tell "no previous exists" from "not loaded yet". Strand B's rubber
band depends on that distinction.

### 2. Previous follows queue order on Android (G1/G2)

`previous()` today calls `previous_from_history()` (`playback_session.rs:657` →
`history.rs:146`). On Android it must navigate `queue[cursor - 1]` instead, so
that left and right are reversible.

**GNOME keeps history.** Do not change the shared `previous()` semantics for
every caller — add the queue-order navigation as its own entry point.

This strand exposes that entry point and routes the two Android surfaces it owns
to it:

- `CoreControlledPlayer.kt:46,50` — `seekToPrevious` and
  `seekToPreviousMediaItem`, which is what the media notification, Bluetooth and
  headphone buttons, the lock screen and Android Auto call,
- `DockMode.kt:149`.

The **player swipe and the previous button call the same entry point, but they
live in strand B's files** (`NowPlayingSheet.kt`, `PlayGestureState.kt`) and are
strand B's task 1. Do not touch them here. What this strand owes B is a stable,
named entry point.

One meaning of "previous" across all four. A Bluetooth button that disagrees with
the swipe is the bug this decision exists to prevent.

Mitigating fact from the grill: with shuffle on, `set_shuffle` reorders the queue
itself and the cursor walks that shuffled order, so `queue[cursor - 1]` is
normally the track just heard anyway. The two semantics diverge only after a jump
(playing a track directly, then going back).

### 3. Plumb the absolute index

`PlaybackUiState.kt:11` already carries `currentIndex: Int?`. It is never read by
the now-playing screen. Make it reach the surface alongside `currentTrackId` in
`LibraryPlayback` (`PlaybackUiState.kt:23-28`).

This is the `index` in the mother plan's `pos = index * screenWidth + dragDelta`.
It must be the position in the *same* order the symmetric window indexes, or the
panels and the cursor disagree.

### 4. A track-id-keyed analysis cache

Two distinct sources, distinct shapes, both precomputed per track and stored in
SQLite. Do not conflate them:

- the **seek waveform**: `TrackAnalysisPort.loadBars(trackId, count)` →
  `track_render_bars` (`track_analysis.rs:61`), returning `SpectralBar`;
- the **visualizer spectrum**: `loadSpectrogram(trackId)` → `track_spectrogram`
  (`track_analysis.rs:39`), returning `SpectrogramFrames`.

Neither is cached across compositions today — `SpectralSeekTrack.kt:48-51`
re-fetches on every `(trackId, count, revision)` change, on the single
`reprise-analysis` thread. Add a cache keyed by track id in
`TrackAnalysisLoader.kt` so a prefetched result survives recomposition and the
neighbour panels do not each re-enter that thread.

Keep the cache bounded: the window is ±2, so a handful of entries with eviction
by distance from the cursor. This is not a general-purpose cache.

### 5. Prefetch ±2, artwork and both analyses

Follow the existing pattern — `prefetchUpcomingArtwork`
(`MobileSurfaceViewModel.kt:256-279`) already calls
`loadUpcomingTracks(LibraryWindowRange(0, 2))` and warms artwork through
`TrackCover.prefetch`.

Extend it to the symmetric ±2 window and to **both** analysis sources. Per
decision G6 the neighbour in visualizer mode shows its own cover fading into its
bars, so a neighbour needs artwork *and* spectrogram — artwork alone is not
enough, and neither is the spectrogram alone.

Why ±2 and not ±1: after a commit the old `index + 1` becomes the centre and a new
`index + 2` becomes the neighbour. With a ±1 window, two quick swipes in a row
show an empty neighbour — a variant of the bug this whole plan exists to fix.

Artwork has a synchronous warm-cache read (`ArtworkCache.seedArtwork` /
`TrackArtwork.seedVisual`, `TrackCover.kt:146`) that yields an immediate
placeholder. The analysis loader has no equivalent; the cache from task 4 is what
takes its place.

## Verification

- Rust unit tests for the symmetric window: at cursor 0 (no previous), at the
  last position (no next), in the middle, and with shuffle on. Assert the window
  indexes the same order `currentIndex` counts in.
- A test that queue-order previous and history previous actually differ after a
  jump — otherwise the G1 change is untested by construction.
- A test that all three Android previous entry points resolve to the same track.
- Cache: a prefetched spectrogram survives a recomposition without a second FFI
  call; eviction happens outside ±2.
- The GNOME frontend still uses history for previous. Assert it, do not assume
  it — this is the divergence G2 accepted, and an accidental global change would
  be silent.

## Definition of done

`loadUpcomingTracks`' caller can ask for a window on both sides of the cursor,
gets a clear "no previous exists" at the start of the queue, reads an absolute
`currentIndex` off `LibraryPlayback`, and finds artwork, bars and spectrogram
already warm for ±2. Android's previous means `queue[cursor - 1]` everywhere;
GNOME's still means history.
