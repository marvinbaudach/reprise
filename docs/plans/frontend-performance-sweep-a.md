---
slug: frontend-performance-sweep-a
worktree: /home/marvin/Projects/reprise-frontend-performance-sweep-a
branch: feature/frontend-performance-sweep-a
phase: shipped
codex_session:
created: 2026-08-24
---
# Strand A — Android: stop the position tick from driving the library

Mother plan: `docs/plans/frontend-performance-sweep.md`. Read it first — it
carries the rule this strand runs under (a task that cannot show its number is
reverted, not shipped).

**Owns `android/**`.** Nothing outside that path may be touched, with one
exception named in A0.

Line numbers are against `origin/dev` at `7eaf16e4d3` (re-checked after dev moved; no Android or showroom file changed in between).

---

## A0 — The ownership note (do this first, it is two lines)

Two planned-but-unwritten plans claim the files this strand changes:
`docs/plans/android-now-playing-desync-throttles-the-scene-b.md` and `-c.md`.

Add to each, under its file list, one note in the plan's own voice: that
`PlaybackUiState.kt` now carries a second, position-free record
(`LibraryPlayback`) which the whole library tree reads instead of the full
state, that this is what keeps a 500 ms position tick out of the list, and that
a later scene rebuild must not merge the two records back together.

This is the one edit outside `android/**`. It exists because an ownership
agreement that lives only in a session plan has already cost this repository an
add/add conflict with a parallel agent who never read it.

---

## A1 — Give the library tree a playback state without the clock in it

### The defect

`Media3PlaybackPort.kt:24` posts a position update every 500 ms. Each one
replaces the whole `PlaybackUiState` record (`MainActivity.kt:174`), and that
record travels from `MainActivity.kt:254` through `LibraryScreen` →
`BrowseScreen` → `BrowseTabs` → `TrackRows` into **every** `TrackListItem`
(`LibraryTrackRows.kt:185`). Twice a second, every visible row is invalidated.

The rows want none of it. What the library tree actually reads:

| File | Reads |
|------|-------|
| `LibraryScreen.kt` | nothing — pass-through |
| `BrowseTabs.kt` | nothing — pass-through |
| `LibraryTrackRows.kt:201` | `playbackPresentation` → `currentTrackId`, `isPlaying` |
| `LibraryFramePolicy.kt:47,50` | `currentTrackId`, `isPlaying` |
| `NowPlayingQueue.kt:64` | `currentTrackId` |
| `BrowseScreen.kt:423,428,513` | `currentTrackId`, `currentTrackUri`, `error` |
| `DockMode.kt:152,153` | `playPauseSymbol`, `playPauseLabel` (derive from `state`) |
| `LibraryFrame.kt:303,304` | `playPauseSymbol`, `playPauseLabel` |
| `LibraryFrame.kt:322` | `progressFraction` — the only real position reader in the tree |

`MainActivity.kt:613` already documents the mechanism from the other side:
"that whole record is replaced by the next 500 ms position tick". The
consequence for recomposition was never drawn.

### The change

Add to `PlaybackUiState.kt`:

```kotlin
@Immutable
internal data class LibraryPlayback(
    val currentTrackId: Long? = null,
    val currentTrackUri: String? = null,
    val state: AndroidPlaybackState = AndroidPlaybackState.STOPPED,
    val error: String? = null,
)

internal fun PlaybackUiState.libraryPlayback() = LibraryPlayback(
    currentTrackId = currentTrackId,
    currentTrackUri = currentTrackUri,
    state = state,
    error = error,
)
```

Give `isPlaying`, `playPauseShowsPause`, `playPauseSymbol` and `playPauseLabel`
a `LibraryPlayback` overload — they only ever read `state`, so no call site has
to learn a new name.

In `MainActivity`'s `setContent`, derive it once so an unchanged track does not
propagate at all:

```kotlin
val libraryPlayback by remember { derivedStateOf { playbackState.value.libraryPlayback() } }
```

Then change the parameter type from `PlaybackUiState` to `LibraryPlayback` in
`LibraryScreen`, `BrowseScreen`, `BrowseTabs`, `TrackRows`, `TrackListItem`,
`LibraryTrackRow`, `NowPlayingQueue`, `DockMode` and `LibraryFramePolicy`
(`playbackPresentation` included).

`NowPlayingSheet`, `NowPlayingScene` and `SceneDriver` keep the full
`PlaybackUiState`. They animate against the clock; that is legitimate.

### A1b — the mini player is the one place that needs both

`LibraryFrame` needs the transport symbol (position-free) *and* the 3 dp
progress bar at `LibraryFrame.kt:318–326`. Reading `playback.progressFraction`
inside its composition is what drags the whole mini player — cover, title,
buttons — into the 2 Hz cycle.

Move that read into the draw phase instead of passing the record down. Colours
resolve in composition (they are constant); the fraction is read when the frame
is painted:

```kotlin
// progress: () -> Float, passed in as { playbackState.value.progressFraction }
val rail = MaterialTheme.colorScheme.outlineVariant
val fill = MaterialTheme.colorScheme.primary
Box(
    modifier = Modifier
        .fillMaxWidth()
        .height(3.dp)
        .align(Alignment.BottomStart)
        .drawBehind {
            drawRect(color = rail)
            drawRect(color = fill, size = Size(size.width * progress(), size.height))
        },
)
```

A state read inside `drawBehind` invalidates the draw phase only: a position
tick repaints three device pixels and recomposes nothing.

The `progress` lambda should be created once, not
written as a fresh `{ ... }` at the call site on every recomposition.

---

## A2 — WITHDRAWN: the diagnosis was wrong

This task claimed that `MobileSurfaceViewModel`'s eight plain `var` fields make
the class unstable, and that an unstable parameter makes the 25 composables
taking it non-skippable. The first half is true and the second half has not been
true since Compose compiler 2.0.20.

This project builds with the Kotlin Compose plugin **2.4.10**, where **strong
skipping is on by default** and is not disabled anywhere in `android/`. Under
strong skipping, a composable with unstable parameters is still skippable — the
unstable ones are compared by instance — and lambdas capturing unstable values
are memoised automatically. The measurement confirms it: all 24 composables that
take the ViewModel already report `restartable skippable`, 223 of 343 overall.

So the increase in skippable composables this task required cannot happen, and
the `remember`-wrapped-lambda rule it carried was guarding against a hazard that
strong skipping already removes.

Withdrawn under this plan's own rule: a task whose measurement comes back flat
is reverted, not shipped. The temporary Compose report configuration used to
establish this was removed again.

What survives from it: A1's finding was the real one. A position tick genuinely
produced a *new* record every 500 ms, and strong skipping cannot help there —
the value really had changed. That is why A1 moved the count from 21 to 1 and
A2 would have moved nothing.

## The measurement for A1: a test, not a profiler session

`app/src/test/` already runs Compose under Robolectric —
`createAndroidComposeRule` in `MainActivityDockTest`, `MobileBottomTabsTest`,
`ArtistPortraitSurfaceTest` and three more. The proof goes there, so it is
repeatable and so it holds the regression shut afterwards.

Write one test that:

1. Renders the library with a handful of tracks and a playing track.
2. Counts recompositions of one visible `LibraryTrackRow` — a
   `SideEffect { count++ }` inside the row's content, or an equivalent probe
   that cannot be optimised away.
3. Advances only `positionMs` on the playback state 20 times.
4. Asserts the counter is still 1, and that the mini player's progress bar has
   in fact moved (a row that stops updating for the right reason and a bar that
   stops moving for the wrong one look identical to a counter).

The assertion must fail before the change. Run it against the current code
first and record that it does; a green test on unfixed code measures nothing.

This test is the deliverable of A1. Without it the task cannot
claim its number.

---

## A3 — Size the artwork cache to the screen it serves

### The defect

`ArtworkCache.kt:33` holds 12 entries, and that one LRU is shared by all three
sizes: `LIST`, `NOW_PLAYING` and `ARTIST_DETAIL` all key into `visuals`
(`ArtworkCache.kt:10–20`). At 72 dp rows a phone shows about eleven at once, so
one open now-playing cover plus one artist portrait evict list entries that are
still on screen, and the prefetch (`MobileSurfaceViewModel.kt:187`) pushes from
the same end.

### The change

Give each size its own budget instead of raising one number. Derive the `LIST`
budget from what a screen actually holds — visible rows at the smallest row
height, plus the prefetch window, plus headroom — and write that derivation
into a comment, so the next person changes it for a reason. `NOW_PLAYING` and
`ARTIST_DETAIL` stay small: large bitmaps, one on screen at a time.

Leave the fog LRU alone. It is keyed by image identity and is a different
question.

Add hit/miss counters to `ArtworkCache.artwork()` and keep them — they are what
makes the measurement possible, and the next change to this file will want them
too.

### Measurement

Scroll a library of at least 300 tracks to the bottom and back to the top once,
at a steady speed; compare the hit rate before and after. A second run with
now-playing opened mid-scroll shows whether cross-size eviction is gone.

---

## Order of work

A0 (two lines) → A1 + A1b → the test → A2 → A3. The test sits between A1 and A2
deliberately: it must be red on today's code, green after A1, and stay green
through A2.

## Verification

The Android JVM suite. JDK 21; the suite script sets `LD_LIBRARY_PATH` itself —
setting it by hand invalidates the evidence. Frame-time claims from a debug
build are worthless and none are made here; the recomposition count is a
counter, not a timing, which is exactly why it survives Robolectric.
