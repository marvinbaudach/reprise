---
slug: artist-portrait-fixes-a
worktree: /home/marvin/Projects/reprise-artist-portrait-fixes-a
branch: feature/artist-portrait-fixes-a
phase: shipped
codex_session:
created: 2026-10-12
---
# Strand A: the Android artist head scrolls whole (ART-3)

Mother plan: `docs/plans/artist-portrait-fixes.md`, decisions 7 and 8.

**Owns:**
- `android/app/src/main/java/io/github/marvinbaudach/reprise/{BrowseTabs,ArtistCover,ArtistPortraitShimmer}.kt`
- new sibling files `ArtistDetailHead*.kt` in that package
- the artist-page tests named below, plus new test files under
  `android/app/src/test/java/io/github/marvinbaudach/reprise/`
- the ART-3 entry in `docs/ux-rules.md`

Touch nothing else.

## Current state

`ArtistsTab` (`BrowseTabs.kt`) puts a `Column` around two things:

- `ArtistDetailHeader`: the title row, which carries `reportLibraryStatusTopInset("artist-detail-header")`,
  plus `ArtistPlayButton` and its `TransientMessageText`.
- `ArtistDetailSections`: a `BoxWithConstraints` holding `ArtistPortraitShimmer` (fills the box,
  fixed) and a `LazyColumn` whose first item is `ArtistPortraitHeader`.

The portrait is clipped at the list top edge under the Play button.

## Tasks

1. **Rule.** Append to `docs/ux-rules.md` after ART-2:

   > - **ART-3** [active] [android] — On the phone's artist page the Play button and the portrait
   >   scroll away together as one head. The title row stays pinned: transparent while the list
   >   rests at the top, it takes the surface-container colour as soon as content passes beneath
   >   it, so nothing is ever cut at an edge that is not drawn. The glow behind the portrait moves
   >   with the head and fades as it leaves.

2. **Failing tests first** (Robolectric). Test names start with `ART_3_` so that
   `scripts/check-ux-traceability.sh` counts them.
   - `ART_3_thePlayButtonScrollsAwayWithThePortrait` starts from the artist list.
     1. Open an artist with enough albums to scroll.
     2. Scroll the list.
     3. Assert that `artist-detail-play` and `artist-portrait-head-image` have left the viewport
        while the title row ("Back to artists") is still displayed.
   - `ART_3_theTitleRowTakesItsContainerOnlyOnceTheListScrolls`. Expose the state as a semantics
     property (e.g. `ArtistHeaderScrolledKey`); do not assert pixel colours. The property is
     false at rest, true after scrolling, and false again after scrolling back to the top.
   - A plain unit test for a pure `artistHeadShimmerTransform(firstVisibleIndex, offsetPx,
     headHeightPx)` returning translationY and alpha:
     - at rest: translation 0, alpha 1;
     - while the head leaves: translation −offset and a linear alpha;
     - from index 1 on: alpha 0.

3. **Restructure the page.**
   - **Layout.** The page becomes a `Box`:
     1. at the bottom, the shimmer, translated and faded through `graphicsLayer` from
        `artistHeadShimmerTransform`;
     2. the `LazyColumn`, filling the page, with top content padding equal to the measured
        title-row height;
     3. on top, the title row, aligned to the top.
   - **Scroll state.** Hoist the `LazyListState` so the title row can read
     `derivedStateOf { firstVisibleItemIndex > 0 || firstVisibleItemScrollOffset > 0 }`. Animate
     the container colour, transparent to `MaterialTheme.colorScheme.surfaceContainer`, with
     `animateColorAsState`.
   - **No per-frame recomposition.** Read scroll offsets only inside `graphicsLayer`, the draw
     lambdas and `derivedStateOf`.
   - **The head item.** Key it `artist-portrait-head`. It holds the Play button with its
     `TransientMessageText`, then the portrait. Strand C will add the credit line below it.
     `itemCount` and the anchor arithmetic (`rememberLibraryListState`, `ObserveLibraryListAnchor`)
     count it exactly once.
   - **The status inset.** `reportLibraryStatusTopInset` stays on the pinned title row, so the
     FB-9 status pill still sits below it.
   - **Edge cases.**
     - With no albums and no tracks, the page still shows "No tracks by this artist." and has no
       head.
     - `ArtistLoadingPage` keeps the title row only.
     - The overflow menu stays on the title row.
   - **File size.** Extract the head and the pinned title row into `ArtistDetailHead.kt` so
     `BrowseTabs.kt` gets smaller, not bigger. Every file stays under 800 lines.

4. **Keep the existing suites green.** Adapt expectations only where the head moved:
   - `ArtistPortraitSurfaceTest`: `theDetailHeadShowsOnlyThePortraitAndNotTheCountsOrName`,
     `thePortraitKeepsSpaceBeforeTheFirstSectionHeading` and
     `aClosedSwitchLeavesTheDetailHeadOnTheGeneratedAvatar`.
   - `ArtistDetailSurfaceTest`: the shimmer tests and `artistPageListsTheArtistsAlbums`.
   - `LibraryStatusChromeTest.onAnArtistPlayFailureTheStatusSitsBelowTheMessage`.
   - `DeleteRefreshBehaviorTest.theArtistPagePlayLeavesOutATrackWaitingToBeDeleted`.
   - `MobileSurfaceStateTest`: `twoArtistPagesKeepTheirOwnScrollAnchor` and
     `twoAlbumsKeepTheirOwnScrollAnchor`.

5. **Gates:**
   - `scripts/check-android-suite.sh`, which needs `ANDROID_HOME`
   - `npm --prefix android run lint`
   - `npm --prefix android run test:lint`
   - `scripts/check-ux-traceability.sh`
   - `cargo fmt --check`, as a sanity check: no Rust is touched

   No device access is needed. The visual check is post-merge item 2 in the mother plan.
