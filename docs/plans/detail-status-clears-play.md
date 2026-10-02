---
slug: detail-status-clears-play
worktree: /home/marvin/Projects/reprise-detail-status-clears-play
branch: feature/detail-status-clears-play
phase: refactored
codex_session:
created: 2026-10-02
---
# The status pill on a detail page clears the Play button (#1049)

## Problem

Device run of #1042 (Pixel, 0.1.169): on the **album** detail page the library status error pill
(x 60–1018 px, y ≈ 495–603 px) covers the page's Play button (y 508–579 px). Only a teal sliver
of the button shows.

Cause (verified in code on `origin/dev`, `android/app/src/main/java/io/github/marvinbaudach/reprise/BrowseTabs.kt`):

- `AlbumDetailHeader` and `ArtistDetailHeader` each wrap their own `Column` in
  `Modifier.reportLibraryStatusTopInset(tag)` (`LibraryStatusTopInset.kt`). That reports only
  the back-arrow/title rows (plus, on the artist page, `TrackContextMenuMessage`).
- The Play button is a **sibling below** the header: `AlbumDetailPage` calls
  `ListPlayButton(...)` after `AlbumDetailHeader(...)`, and the artist page calls
  `ArtistPlayButton(selectedArtist.artist)` after `ArtistDetailHeader(...)`. `ArtistPlayButton`
  also renders a `TransientMessageText` below the button.
- `LibraryStatusChrome` pads the slot by `8.dp + inset`, so the pill lands 8 dp under the title
  block — on top of the Play row.
- The artist page has the same geometry. The device run only showed the narrow, centred deletion
  pill there, which happened to miss the left-aligned button. The wide error pill covers it too.
- The S5 tests (`LibraryStatusChromeTest.onADetailPageTheStatusSitsBelowTheHeader`,
  `onAnAlbumDetailPageTheStatusSitsBelowTheHeader`) assert `pill top == header bottom + 8 dp`, so
  they pin the wrong edge. The album test even runs against an album with no tracks
  ("No tracks in this album."), where no Play button exists.

## Decision (grilled 2026-10-02: pill below the Play row; both detail pages)

On a detail page, the measured "header" is everything between the page top and the start of the
list: the title block **plus** the Play row (or the empty-state notice that replaces it), plus
the transient message the artist Play button can show. The pill sits 8 dp below that.

## Tasks (test-first)

### Task 1 — failing tests

In `android/app/src/test/java/io/github/marvinbaudach/reprise/LibraryStatusChromeTest.kt`:

- Give the harness's `openAlbum` an album with **at least one track row**, so the album detail page
  shows its Play button. Keep a way to open an empty album only if another test needs it
  (none does today).
- Add test tags to the two Play buttons: `album-detail-play` and `artist-detail-play`.
  (`ListPlayButton` is private in `BrowseTabs.kt` — give it a `modifier: Modifier = Modifier`
  parameter, chained after its padding, and pass `Modifier.testTag(...)` from the two call sites;
  `ArtistPlayButton` passes `artist-detail-play`.)
- S5a (artist) and S5b (album): with an **error pill** (`library-status-error`, the wide one) and
  with the deletion line, assert:
  - `pill top >= bottomInPixels("<page>-detail-play")` (the coordinate assertion the device run
    asked for), and
  - `pill top == bottomInPixels("<page>-detail-header") + 8 dp` still holds, now that the
    header tag measures the whole block.
- Run the tests and see S5a/S5b fail against the current code.

### Task 2 — measure the whole block

In `BrowseTabs.kt`:

- Give `AlbumDetailHeader` and `ArtistDetailHeader` a trailing slot parameter
  `below: @Composable ColumnScope.() -> Unit = {}`, rendered **inside** the measured `Column`
  after the existing content. The `reportLibraryStatusTopInset(tag)` modifier stays on that one
  `Column`, so each page keeps exactly one measurement site.
- `AlbumDetailPage`: move the `ListPlayButton(...)` / "No tracks in this album." branch into the
  header's `below` slot. `TrackRows` stays outside, below the header.
- Artist page (`selectedArtist != null` branch): move the "No tracks by this artist." /
  `ArtistPlayButton(...)` branch into `ArtistDetailHeader`'s `below` slot.
  `ArtistDetailSections` and the rest stay outside.
- Loading pages (`AlbumLoadingPage`, `ArtistLoadingPage`) keep the header without a slot.
- Visual layout must not change: same order, same paddings, same vertical positions of every
  row. Only the measured bounds grow.
- `BrowseTabs.kt` is 723 lines on `origin/dev`; it must stay under 800.

Run the tests: S5a/S5b green, the rest of `LibraryStatusChromeTest` green.

### Task 3 — rulebook

`docs/ux-rules.md`, the FB-9 Android paragraph (~line 1508, "below the header on a detail
page"): say that on a detail page the pill sits below the header **and the page's Play button**.
One-sentence edit, no status change.

## Verification

- `android/gradlew --project-dir android :app:testDebugUnitTest --tests '*LibraryStatusChromeTest*'` red before
  Task 2, green after.
- The full Android unit suite (`scripts/check-android-suite.sh`) green. Log to a file; report
  the counts.
- No Rust files change, so the cargo gates are not affected (do not run them).
- Device check (later, human or a locked session): album detail page with an error pill — the
  Play button fully visible above the pill.

## Files

`android/app/src/main/java/io/github/marvinbaudach/reprise/BrowseTabs.kt`,
`android/app/src/test/java/io/github/marvinbaudach/reprise/LibraryStatusChromeTest.kt`,
`docs/ux-rules.md` (FB-9 paragraph only), this plan. The list is a starting point, not a fence:
stop only if the contract itself turns out wrong.

## Parallelität

Cannot be cut: every task touches the same detail-page layout in `BrowseTabs.kt` and the S5 tests
that read it. One strand, no post-merge cross-checks.
