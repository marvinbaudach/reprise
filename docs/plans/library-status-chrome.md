---
slug: library-status-chrome
worktree: /home/marvin/Projects/reprise-library-status-chrome
branch: feature/library-status-chrome
phase: planned
codex_session:
created: 2026-10-02
---
# The library's status lives in its chrome, so the list never moves

Closes #1037. Follows #1036, which turned the deletion line into an overlay pill.

## Decisions (user, 2026-10-02)

1. **Artist-photo progress moves into the summary row.** The card disappears.
   - A progress bar 3 dp high is drawn over the top edge of the library pager. It has no layout height.
   - The phase and the count go into the summary text, for example "68 artists · Artwork 0/2" or "68 artists · Waiting for a connection".
   - While a run is active, the ⋮ menu gains a cancel entry. The ✕ disappears.
2. **On a detail page the status pill sits under the page header,** not over it. The pill stays screen-level.
3. **Errors share one status slot at the top with the deletion pill,** in a fixed order. Errors carry a close button. Only that button takes pointer input; the pill itself still lets taps through.

One strand: every part touches `BrowseScreen.kt`. There is no disjoint file group.

## Current code (origin/dev)

- `BrowseScreen.kt` has 789 lines. The order of its content is:
  - `LibrarySummaryActions`, at line ~609
  - `BrowseStatusLines(...)`, at line ~617
  - `Box(Modifier.weight(1f)) { HorizontalPager(...) ; DeletionMessageLine(surfaceState, Modifier.align(TopCenter).padding(top = 8.dp)) }`, at lines ~625–745
- `BrowseStatusLines.kt` has 32 lines. It holds, in flow:
  - `browseError`, behind its origin check
  - `playback.error`
  - `playback.faultNotice`, guarded by `!dockMode && !sheet.currentState && !sheet.targetState`
  - `ArtistPhotoProgressBar(progress = surfaceState.visibleArtistPhotoProgress, dismiss = surfaceState::dismissArtistPhotoProgress)`
- `DeletionMessages.kt` has 180 lines. `DeletionMessageLine` is the pill. It uses `AnimatedVisibility`, fades with `DELETION_LINE_FADE_MS = 150`, and has `semantics(mergeDescendants) { liveRegion = Polite }` and the tag `deletion-message-line`.
- `ArtistPhotoProgressBar.kt` has 237 lines.
  - It draws the card through `ArtistPhotoProgressCard` (a Surface) and uses the tags `artist-photo-progress`, `-label`, `-counter`, `-track` and `-failure`.
  - The phases are PREPARING, RUNNING ("Downloading artwork"), WAITING ("Waiting for a connection") and COMPLETE ("Artwork complete").
  - `failed > 0` adds the line "N without a photo".
  - It dismisses itself 4 s after a COMPLETE run, or 10 s after one with failures.
  - The ✕ calls `dismissArtistPhotoProgress()`. That only hides the progress for the current `runId`; the download keeps running.
- `LibraryFrame.kt` has 448 lines. `LibrarySummaryActions(tab, summary: () -> String, searching, toggleSearch, rescan, openSettings)` draws a 48 dp row. The row holds the summary text (tag `library-summary-text`), search, and a ⋮ menu with Rescan and Settings.
- `BrowseErrorLine.kt` draws error-coloured text with no way to dismiss it.
- `PlaybackUiState.kt` holds two values:
  - `LibraryPlayback.error: String?` is persistent.
  - `faultNotice: TransientMessage?` clears itself after `TRANSIENT_MESSAGE_MS`.
- `browseError` is set and cleared in `BrowseScreen` (`clearBrowseError()`).
- `BrowseTabs.kt` has 721 lines. It holds `ArtistDetailHeader`, the back arrow and the title, which is sized by its content. Check whether an album detail header exists as well.

## Design

### A. Summary row and edge progress (decision 1)

- **Summary text.**
  - While `visibleArtistPhotoProgress` is non-null, add a suffix to `LibrarySummaryActions`' summary: `" · Artwork {done}/{total}"`. During PREPARING the suffix is `" · Preparing artwork"` and during WAITING it is `" · Waiting for a connection"`.
  - When a run is COMPLETE with failures, the suffix is `" · {failed} without a photo"` until the existing auto-dismiss.
  - When a run is COMPLETE without failures, there is no suffix: the bar fills and fades.
  - Build the string through a small pure function in a new `ArtistPhotoProgressSummary.kt`, so it can be unit-tested.
  - Reuse the existing label strings and resources. The row's height stays 48 dp, and the text ellipsizes as it does today.
- **Edge bar.**
  - Add a new composable, `ArtistPhotoEdgeProgress(progress, modifier)`, in place of the card in `ArtistPhotoProgressBar.kt`.
  - Draw it as a 3 dp determinate `LinearProgressIndicator`, or as an indeterminate one during PREPARING and WAITING. Waiting may also be static: keep whatever the current track does and respect reduced motion as it does today.
  - Place it in the pager `Box` with `Modifier.align(TopCenter).fillMaxWidth()`.
  - It has no pointer input. It fades in and out over 150 ms (reuse the deletion constant or add a shared `STATUS_FADE_MS`). Keep the tag `artist-photo-progress-track`.
- **Cancel.**
  - While a run is in PREPARING, RUNNING or WAITING, the ⋮ menu shows a third entry, "Stop artwork download".
  - Look for a cancel API on the view model or FFI. If one exists, the entry cancels the run.
  - If none exists, the entry calls `dismissArtistPhotoProgress()`, labelled "Hide artwork progress", and the plan records that a real cancel needs a core API. Do NOT add FFI or core API in this change; report it.
- Remove the card (`ArtistPhotoProgressCard`, the ✕ and the failure line) and `ArtistPhotoProgressBar` from `BrowseStatusLines`.
- Delete unused code outright. There is no backward compatibility to keep.

### B. One status slot (decision 3)

- Add `LibraryStatusSlot(...)` in a new file, `LibraryStatusSlot.kt`. It replaces both `BrowseStatusLines` and the separate `DeletionMessageLine` call; delete `BrowseStatusLines.kt`.
- The slot shows at most ONE pill. The candidates, in priority order:
  1. `browseError`, behind its origin check
  2. `playback.error`
  3. `playback.faultNotice`, under the existing dock and sheet guard
  4. the deletion line (progress or message)
- **Error pills.**
  - Use the same pill look as the deletion pill and the error colour for the text.
  - Each has a trailing close `IconButton` with the content description "Dismiss". It is the ONLY pointer-input element.
  - Close behaviour per kind:
    - `browseError`: call the existing `clearBrowseError()`.
    - `playback.error`: if it has no clear API, keep a remembered "dismissed value" and hide it until the value changes.
    - `faultNotice`: dismiss its current occurrence the same way.
- **Deletion pill.** Unchanged: no pointer input. Keep `DeletionMessageLine` and its tests green; the slot calls it when no error wins.
- **Accessibility.** Every pill keeps `liveRegion = Polite`, merged descendants and tags. Error pills get the tag `library-status-error`, and the deletion pill keeps `deletion-message-line`.
- **Fade.** Switching between candidates crossfades over 150 ms. There is never a height animation.

### C. Under the detail header (decision 2)

- `BrowseScreen` provides a `LibraryStatusTopInset` holder through a `CompositionLocal`, as a `MutableState<Dp>`.
- `ArtistDetailHeader`, and the album detail header if one exists, report their measured height through `onSizeChanged` while composed, and reset it to 0 in `DisposableEffect.onDispose`.
- The slot's top padding is `8.dp + inset`. Apply the inset only while the pager's settled current page is the page that hosts the detail, so a detail kept alive off-screen does not push the pill on another tab.
- The edge progress bar is NOT offset. It stays on the pager's top edge.

### File size

`BrowseScreen.kt` must end at or below its current 789 lines. Moving the deletion call into the slot should net it smaller. Every touched file must stay under 800 lines; extract a sibling module if one would not.

## Tests (Robolectric, test-first)

| ID | Test | What it shows |
|---|---|---|
| S1 | `ArtistPhotoProgressSummaryTest` | Every phase, with and without failures, maps to the expected suffix. Pure, red first. |
| S2 | `LibraryStatusChromeTest.artworkProgressDoesNotMoveTheList` | Full `BrowseScreen` harness (copy from `DeletionLineOverlayTest`). Shows a progress run (PREPARING, then RUNNING, then COMPLETE, then dismissed), then asserts the pager top and the first-row top are unchanged to the pixel, that the summary text carries the suffix, and that `artist-photo-progress-track` exists while running and is gone after. |
| S3 | `LibraryStatusChromeTest.anErrorDoesNotMoveTheListAndItsCloseButtonHidesIt` | Set a browse error and a playback error. The tops are unchanged, the highest priority shows, closing it reveals the next, and closing that one empties the slot. |
| S4 | `LibraryStatusChromeTest.aTapBesideTheCloseButtonReachesTheRowUnderneath` | Inject a tap in root coordinates on the error pill's text area, away from the button. The row under it opens. |
| S5 | `LibraryStatusChromeTest.onADetailPageTheStatusSitsBelowTheHeader` | Open an artist detail, start a deletion line, and assert pill top ≥ header bottom. On the Titles tab with the detail still alive, the pill sits at the pager top + 8 dp. |
| S6 | Menu | While running, ⋮ shows the cancel/hide entry, and choosing it hides the bar and the suffix. Absent when idle. |

Update the existing tests that find the card (`ArtistPhotoProgressBarTest`, including `browseUsesTheSameProgressLabelsAboveItsPager`, `MobileHeaderRowTest` and any finders on `BrowseErrorLine`) to the new shape. Rewrite assertions about the card's look so they cover the summary and bar instead, and keep their behavioural intent: dismissal sticks for one run, auto-dismiss timing, and the failure count is shown. `DeletionLineOverlayTest` and `DeletionProgressLineTest` stay green.

## Rulebook

Rewrite the FB-9 Android paragraph in `docs/ux-rules.md`, which #1036 added. The new text:

- On Android, the library's transient status lives in chrome.
- Errors and the deletion line share one pill slot over the top edge of the list. It sits below the header on a detail page, shows one pill at a time in priority order, has no layout height, and only an error's close button takes input.
- Artwork progress is a 3 dp bar on the pager's top edge, with its phase and count in the summary row and its cancel in the overflow menu.
- The list never moves when any of these appear or leave.
- Coverage: `DeletionLineOverlayTest` and `LibraryStatusChromeTest`. The level stays `[gtk]`.
- Drop the "open deviations" sentence. Add no new ID and change no status.

## Gates

- `ANDROID_HOME=/home/marvin/.local/share/android-sdk scripts/check-android-suite.sh`. Never use raw gradlew for the verdict, because it needs the host library.
- The file-size rule.
- No device, no adb.

## Non-goals

- GTK, core and FFI. If a real cancel needs core API, report it and do not build it.
- Other screens' status lines, for example the now-playing sheet.

## File ownership

`android/app/src/main/java/io/github/marvinbaudach/reprise/{BrowseScreen,BrowseStatusLines,LibraryStatusSlot,DeletionMessages,ArtistPhotoProgressBar,ArtistPhotoProgressSummary,LibraryFrame,BrowseTabs,BrowseErrorLine}.kt`, any new sibling a size extraction needs, Android string resources for the new labels, the matching tests under `android/app/src/test/...`, the FB-9 paragraph in `docs/ux-rules.md`, and this plan.

## Parallelität

This plan cannot be cut. Parts A, B and C all edit `BrowseScreen.kt` and the pager `Box`, and S2, S3 and S5 need the whole thing. It runs as one strand, and there are no post-merge cross-checks.
