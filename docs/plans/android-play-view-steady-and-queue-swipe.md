---
slug: android-play-view-steady-and-queue-swipe
worktree:
branch:
phase: shipped
codex_session:
created: 2026-08-11
---
# Android: the play view stops jumping, and the queue stops stealing the swipe

Status: **planned — ready to start**
Base: `origin/dev` at `5995f70e77` (the play-view rebuild and the queue tab have
landed). Every file and line reference below was read against that commit.
As of: 2026-08-11

Runs in parallel with `android-desktop-visualizer`. That branch owns
`NowPlayingScene.kt` and the Rust side; this one owns `BrowseScreen.kt`,
`LibraryFrame.kt` and `LibraryTrackRows.kt`. The two touch
`FavouriteHeartButton.kt` from different directions — see *Ownership* below.

## Product goal

Two complaints, one branch, because both are about a gesture or a frame that
does something the user did not ask for.

1. **Changing song must not move the screen.** Today the whole play view slides
   out of the bottom and back in on every track change.
2. **A horizontal swipe belongs to the tabs.** The queue rows claim it for
   themselves, so swiping out of the queue tab removes a track instead.

## Decisions taken with the user (2026-08-11)

1. **The last answered row stays on screen** until the answer for the new track
   arrives — in the play view *and* behind the mini player. No blank, no
   collapse.
2. **While it is stale, the heart and the context menu are inactive.** That is
   what makes staying honest: the row is shown, but it cannot be acted on,
   so it can never answer for a track that is no longer playing.
3. **A stop still blanks immediately.** When playback stops there is no new
   answer coming, and the surface should go, not linger.
4. **The queue swipe goes, with no replacement.** Removing from the queue lives
   in the context menu (long press) and stays there.

# Package A — the play view stands still

## A1 — one term instead of two

`BrowseScreen.kt:433` is the whole bug:

```kotlin
val currentTrack = answeredTrack?.takeIf { it.id == playingTrackId }?.track
```

It conflates two questions that have different answers at different times:

- *is anything playing?* — known synchronously, `playback.currentTrackId`
  (`BrowseScreen.kt:416`)
- *which row describes it?* — answered later by `loadTrack`
  (`BrowseScreen.kt:420-426`), because reading it takes the library lock a
  folder scan holds for its whole walk

Visibility is currently wired to the second (`BrowseScreen.kt:631`), so the
`AnimatedVisibility` collapses on every change and its `slideOutVertically` /
`slideInVertically` pair (`BrowseScreen.kt:633-638`) plays the jump the user
sees.

Replace `currentTrack` with two derived values, both from the existing
`answeredTrack` state:

- **`shownTrack: LibraryTrack?`** — `answeredTrack?.track`, cleared when
  `playingTrackId` becomes `null`. It survives a change to a different id.
- **`shownTrackIsStale: Boolean`** — `answeredTrack != null &&
  answeredTrack.id != playingTrackId`.

Then:

- `BrowseScreen.kt:631` — `visible = nowPlayingExpanded && playingTrackId != null`
- `BrowseScreen.kt:626`, `:640`, `:654` and the `LibraryFrame` argument at
  `:466` take `shownTrack`
- `LibraryFrame.kt:132/150/153` keeps its `currentTrack: LibraryTrack?`
  parameter but is now fed `shownTrack`, so the row stops collapsing on a
  change

Keep the comment at `BrowseScreen.kt:427-432` alive rather than deleting it:
rewrite it to say what now holds — the row stays, the actions do not, and a
stop still blanks.

**Do not** add a second loader, a placeholder track, or a timeout. The window
is exactly "between two answers"; nothing else needs to know about it.

## A2 — a stale row cannot be acted on

`FavouriteHeartButton` (`FavouriteHeart.kt:22-33`) writes through
`LocalPlaybackControls.setFavourite` for the `track` it was given. Give it an
`enabled: Boolean = true` parameter, defaulting to today's behaviour, and pass
`!shownTrackIsStale` from the two places that show the playing track:
`NowPlayingScene.kt:261` and `NowPlayingSheet.kt:245`. Disabled means it does
not fire and reads as disabled — not invisible, or the header would twitch,
which is the very thing this branch removes.

The track context menu is reached by long press (`TrackContextMenu.kt:96-102`).
The play view's own menu button follows the same rule as the heart. Rows in the
library lists are untouched — they are never stale.

## A3 — tests

Compose tests in the existing `MainActivity*Test` shape, driving a `loadTrack`
that can be held open:

- a track change with a delayed answer keeps `now-playing-gestures` on screen
  for the whole gap, and the previously answered title stays visible
- the same gap leaves `now-playing-heart` present but not enabled, and a click
  on it does not reach `setFavourite`
- once the answer lands, title and heart follow the new track and the heart is
  enabled again
- `playingTrackId` going `null` blanks the play view and the mini player
  immediately, without waiting for anything
- the mini player row does not disappear across a change

# Package B — the swipe belongs to the tabs

## B1 — remove the queue's horizontal drag

`LibraryTrackRows.kt` is the only place in the app with a horizontal drag
gesture. Remove it entirely:

- the `swipeOffset` state (`:233`)
- the conditional `pointerInput` block with `detectHorizontalDragGestures`
  (`:235-252`), including the `queueActions.remove` call in its `onDragEnd`
- the `translationX = swipeOffset` in the row's `graphicsLayer` (`:263`)
- `QUEUE_REMOVE_FRACTION` (`:427`)

`queueActions.remove` itself **stays** — the context menu calls it
(`TrackContextMenu.kt:164-170`). The vertical reorder handle
(`LibraryTrackRows.kt:393-425`, `detectVerticalDragGestures`) is untouched.

With no child consuming horizontal drags, the `HorizontalPager`
(`BrowseScreen.kt:500-607`) receives them again in the queue tab as it already
does in the other four.

## B2 — tests

`MainActivityQueueTest.kt` currently asserts the opposite of what should now
hold. Turn it around rather than deleting it:

- `fullWidthSwipeRemovesButShortFlickDoesNot` (`:99-111`) becomes a test that a
  full-width horizontal swipe on a queue row removes **nothing**
- `staleFalseReloadsTruthInsteadOfLeavingTheCapturedRows` (`:114-125`) uses the
  swipe only as a way to trigger a queue mutation; re-point it at the context
  menu path so it still covers what it was written for
- the `swipeRow` helper (`:202-208`) stays, now used to prove the row survives
- add: a horizontal swipe started on a queue row changes the selected tab

Keep at least one test that removing from the queue *via the context menu*
still works — that is now the only way, and nothing else guards it.

# Ownership

`android-desktop-visualizer` owns `NowPlayingScene.kt` and everything under
`crates/`. This branch needs exactly one line in that file
(`NowPlayingScene.kt:261`, the heart's `enabled`). Whichever branch merges
second rebases and re-applies that one line; do not restructure the header.

The file lists above are a **starting point**, not a boundary — if the change
needs a neighbouring file, take it, and say so in the handover.

## Verification

`JAVA_HOME=/usr/lib/jvm/java-21-openjdk` — the system default is JDK 26 and
Robolectric dies on it with "major version 70", which reads like a broken
change. Delete `android/app/build/test-results/testDebugUnitTest` before the
run and check the XMLs are fresh afterwards; judge by *suite* count and test
count, not by the colour of the final line.

Visual acceptance on the emulator: play a track, let it advance to the next on
its own, and watch the play view — nothing moves but the text and the cover.
Then swipe left and right across the queue tab and confirm the tab changes and
no track disappears.

## Out of scope

- The asynchronous artwork, fog and spectrogram loads. They change colour and
  texture during the gap, never geometry — the layout places header, title,
  progress and transport against the screen height
  (`NowPlayingScene.kt:185-208`), so a two-line title pushes nothing.
- Any replacement gesture for removing from the queue.
- The dock-mode surface beyond feeding it `shownTrack`.

## Risks

- **A long gap now shows an old row for longer than it used to.** That is the
  accepted trade; the disabled actions are what keep it honest. If the gap is
  ever measured in seconds rather than frames, the fix belongs in the loader,
  not here.
- **`answeredTrack` is also read by the dock offer** (`BrowseScreen.kt:654`).
  Feeding it `shownTrack` means the offer no longer flickers on a change —
  intended, but worth a look on a wide-short layout.
