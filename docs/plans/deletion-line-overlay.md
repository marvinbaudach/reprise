---
slug: deletion-line-overlay
worktree: /home/marvin/Projects/reprise-deletion-line-overlay
branch: feature/deletion-line-overlay
phase: shipped
codex_session:
created: 2026-10-02
---
# The deletion line floats over the list instead of pushing it

## Decisions from the grill (2026-10-02)

1. **Scope: the deletion line only.** The four other in-flow lines are named as
   open deviations in FB-9 and handed to a follow-up issue.
2. **Placement: top-centre over the pager, `surfaceContainerHigh` at alpha
   0.94.** The bottom edge stays clear for the mini player.
3. **Taps pass through the pill.** It has no pointer input and no
   tap-to-dismiss. It leaves on its own.
4. **Rulebook: an Android paragraph under FB-9 in the ACC-8 style.** No new ID.
5. **Follow-up issue: `needs-triage`.**
6. **One strand.** T3 is a coverage test, so it is allowed to be green from the
   start. The `BrowseStatusLines.kt` extraction is mandatory.

## Goal

On Android, the library screen's deletion status line ("Deleting N tracks…",
"N tracks deleted", "Still deleting…", "The list changed before the tracks of …
were found. Nothing was done.") is inserted into the layout above the pager when
it appears and removed when it goes. Every row of the list moves down by about
24 px under the user's finger while a delete runs, and back up when the line
leaves. Device evidence: `~/.cache/reprise-scratch/device-check/run0164-repro/t3_montage.png`,
last frame (0.1.164).

After this change, the line is a pill drawn **over** the top edge of the list.
The list's geometry is identical whether the line is showing or not.

## The rule this answers to

`docs/ux-rules.md` **FB-9** [active] [gtk] — "Transient status indicators do not
displace existing layout." Its first prohibition is literally the current
behaviour: "never insert a banner above the content and remove it again". Its
first-choice implementation is (1) *chrome*: "the header, footer or edge region
… as an overlay with no layout height of its own". Background status "fades in
place with the Micro token (150 ms), never with a height animation".

FB-9's level is `[gtk]`, so the traceability gate does not read Android tests.
ACC-8 sets the precedent for this case: the rule text gains an Android paragraph
that names the Kotlin test covering the Android half, and the level stays as it
is. No new rule ID is needed — this is not a new behaviour, it is FB-9 reaching
the second frontend.

P-4 [planned] ("nothing shifts uninvited") points the same way but is weaker. Its
"process started by the user" exemption could be read as permitting the shift.
FB-9 is the active, specific rule and settles it.

## Current code (origin/dev, `e4a4dd01e7`)

- `android/app/src/main/java/io/github/marvinbaudach/reprise/DeletionMessages.kt`
  (125 lines). `DeletionMessageLine(surface)`, line 96, returns early when
  `deletionProgress` and `deletionMessage` are both null. Otherwise it draws a
  full-width `Column` with padding 16/4 dp, which holds the progress `Text` and
  `TransientMessageText(message, …)`.
- `android/app/src/main/java/io/github/marvinbaudach/reprise/BrowseScreen.kt`
  (**792 lines**). Inside `Scaffold { contentPadding -> Column { … } }` the
  in-flow children are, in order: search field, `LibrarySummaryActions`,
  `BrowseErrorLine`(browseError), `BrowseErrorLine`(playback.error),
  `DeletionMessageLine(surfaceState)` (line ~624), the fault notice,
  `ArtistPhotoProgressBar`, and then `HorizontalPager(Modifier.weight(1f).testTag("library-destination-pager"))`.
- `TransientMessage.kt`. `TransientMessageText` renders the timed message in
  `colorScheme.error` and dismisses it after `TRANSIENT_MESSAGE_MS`.
- Tests that exist already:
  - `DeletionProgressLineTest` (70 lines) covers the stale bound.
  - `TrackIdResolutionOffMainThreadTest.aRowThatLeavesWhileResolvingSaysOnTheScreenLineThatNothingWasDone`
    proves that the *sink* receives the "list changed" text. It uses a
    recording sink, not the real `MobileSurfaceViewModel` and not the line, so
    nothing yet shows the text reaching the screen.
  - `ArtistPhotoProgressBarTest.browseUsesTheSameProgressLabelsAboveItsPager`
    is a ready-made full `BrowseScreen` harness to copy.

## Design

1. **`DeletionMessageLine` becomes a pill and takes a `modifier`.** Signature:
   `DeletionMessageLine(surface, modifier: Modifier = Modifier)`.
   - The content is unchanged: progress text and/or transient message, the stale
     bound, and the dismissal timer.
   - The container changes from a full-width padded `Column` to a wrap-content
     pill. It is a plain `Box`/`Column` with `background(color, RoundedCornerShape(50))`
     and horizontal padding of about 16 dp and vertical padding of about 6 dp.
   - Background: `colorScheme.surfaceContainerHigh` at alpha `0.94f`.
     "Semi-transparent" is kept high on purpose, because the pill sits over
     arbitrary row text and must keep its contrast.
   - Text: progress in `onSurface`; the message keeps `colorScheme.error`, as
     `TransientMessageText` already draws it.
   - **It must not be a Material3 `Surface`, and it must have no pointer
     modifiers.** A non-clickable M3 `Surface` still consumes touches. A pill
     without pointer input lets a tap fall through to the row underneath, so
     the pill can never eat a tap meant for the list.
   - Semantics: `liveRegion = LiveRegionMode.Polite`, so TalkBack announces the
     line without moving focus (ACC: no focus theft).
   - Test tag: `"deletion-message-line"`.
   - Width: `widthIn(max = …)`, with `fillMaxWidth` padding of 16 dp from the
     screen edges as the bound. A long "The list changed before the tracks of
     <long artist> were found…" wraps inside the pill instead of running off
     screen. Its own height may differ between states. FB-9's "own height must
     not change" prohibition is about displacing neighbours, and an overlay
     displaces none.
2. **Fade, not slide.** Show and hide with `AnimatedVisibility(visible, enter = fadeIn(tween(150)), exit = fadeOut(tween(150)))`.
   This is FB-9's Micro token, and Android has no motion-token module.
   - Name the duration as a constant, `DELETION_LINE_FADE_MS = 150`.
   - The content shown during the exit fade is the last non-null state. Remember
     it, so the pill does not collapse to empty while fading out.
   - No `expandVertically`.
3. **`BrowseScreen` places the pill over the pager, not above it.**
   - Replace the in-flow `DeletionMessageLine(surfaceState)` call and its comment.
   - Wrap the pager: `Box(Modifier.weight(1f)) { HorizontalPager(Modifier.fillMaxSize().testTag("library-destination-pager"), …) ; DeletionMessageLine(surfaceState, Modifier.align(Alignment.TopCenter).padding(top = 8.dp)) }`.
   - Keep the "screen-level on purpose" comment beside the new call.
   - **Top, not bottom.** The bottom edge of the content area is where the
     `LibraryBottomFrame` (mini player and tabs) sits, which the user asked to
     keep clear. The pager's top edge sits directly under the summary actions.
     This is FB-9's "edge region".
4. **File size.** `BrowseScreen.kt` is at 792 of 800 lines. The wrap adds about 6
   lines, so a sibling extraction is mandatory, not optional. Move the in-flow
   status block into a new
   `android/app/src/main/java/io/github/marvinbaudach/reprise/BrowseStatusLines.kt`
   as `@Composable internal fun BrowseStatusLines(…)`:
   - `browseError` with its origin check;
   - `playback.error`;
   - the fault notice with its dock and now-playing-sheet guard;
   - `ArtistPhotoProgressBar`.

   The extraction is mechanical: identical behaviour and identical order. Do not
   trim comments to fit. `BrowseScreen.kt` must end well under 800 lines.

## Tests (Robolectric, `android/app/src/test/...`, test-first)

Each one is written first and must fail on the current code, except where noted.

- **T1. `DeletionLineOverlayTest.aDeletionLineDoesNotMoveTheListUnderTheFinger`**
  (fails today).
  - Build the full `BrowseScreen`, copied from the
    `browseUsesTheSameProgressLabelsAboveItsPager` harness, with an artists
    window that holds several rows.
  - Read the unclipped top of the first visible artist row and of
    `library-destination-pager`.
  - Call `viewModel.begin("Deleting 13 tracks…")` on the UI thread and wait for
    idle. Assert that `deletion-message-line` is displayed and that both tops
    are unchanged to the pixel.
  - Then `finish("13 tracks deleted")` and advance the clock past
    `TRANSIENT_MESSAGE_MS` and the fade. Assert that the line no longer exists
    and the tops are still unchanged.
- **T2. `DeletionLineOverlayTest.aTapOnTheDeletionLineReachesTheRowUnderneath`**
  (fails today, because the line is in flow, so no row lies under it).
  - While the line shows, `performTouchInput { click(center) }` on the
    `deletion-message-line` node's centre, injected at root coordinates.
  - Assert that the row under it reacts. Use whatever the existing harness
    exposes, for example `openArtist` firing or the pending-artist state.
  - If the harness cannot observe a row tap cheaply, fall back to a minimal
    `Box { LazyColumn(rows with clickable counters); DeletionMessageLine(…, Modifier.align(TopCenter)) }`
    composition and assert that the counter of the row under the pill went up.
- **T3. `DeletionLineOverlayTest.aListThatChangedWhileResolvingIsSaidOnTheScreen`.**
  - Compose a `MobileSurfaceViewModel` (the real sink) with `DeletionMessageLine`
    under `LocalDeletionMessages provides viewModel`, plus a `TrackContextMenu`
    whose row leaves while `resolveTrackIds` is gated. Mirror the gate pattern
    of `aRowThatLeavesWhileResolvingSaysOnTheScreenLineThatNothingWasDone`.
  - Assert that `onNodeWithText("The list changed before the tracks of Whole Artist were found. Nothing was done.")`
    is displayed inside `deletion-message-line`.
  - This closes finding 3 of `HANDOFF-2026-10-01-device-run-0164-findings.md`:
    the path from sink to screen is proven without a debug-only delay hook.
  - Expected: it passes against today's line as well. It is a coverage test,
    not a red-first test. Say so in its KDoc.
- The existing `DeletionProgressLineTest` stays green unchanged. If it located
  the line by layout, adjust only its finders.
- Existing `BrowseScreen` tests (`ComposeBehaviorTest`, `ArtistPortraitLiveRefreshTest`,
  `ArtistPhotoProgressBarTest`) stay green.

## Rulebook

Append an Android paragraph to **FB-9** in `docs/ux-rules.md`, in the ACC-8
style:

> On Android the library screen's transient status is drawn over the top edge
> of the list as a pill with no layout height and no pointer input, fading with
> 150 ms; the list never moves when it appears or leaves. The level stays
> `[gtk]`; the Android half is covered by `DeletionLineOverlayTest`, which the
> traceability gate does not read. The other Android status lines still in flow
> above the pager — the browse error, the playback error, the fault notice and
> the artist-photo progress bar — are open deviations from the first
> prohibition, not a second sanctioned pattern.

No status change and no new ID.

## Non-goals

- Moving the other in-flow lines (the browse error, playback error, fault notice
  and `ArtistPhotoProgressBar`) to the overlay. They have the same defect, but
  they are separate surfaces with separate lifetimes and are named as open
  deviations above. At landing, open one GitHub issue for them with the label
  **`needs-triage`**, not `ready-for-agent`: the artist-photo progress bar still
  has an open design question (a 2–3 px edge bar with Cancel, per FB-9). Link
  the FB-9 paragraph, and name `BrowseStatusLines.kt` as the place to work.
- The GTK frontend, MCP and core. This is pure presentation, so per the "every
  feature reaches every frontend" rule there is nothing to expose. Say so in
  the PR.
- The debug-only id-resolution delay hook. T3 replaces it.

## Gates

- `cd android && ./gradlew :app:testDebugUnitTest --tests '*DeletionLineOverlayTest*' --tests '*DeletionProgressLineTest*' --tests '*ComposeBehaviorTest*' --tests '*ArtistPhotoProgressBarTest*' --tests '*TrackIdResolutionOffMainThreadTest*'`.
  Then run the full Android suite through `scripts/check-android-suite.sh`.
- File-size rule: `BrowseScreen.kt`, `BrowseStatusLines.kt` and
  `DeletionMessages.kt` all end under 800 lines.
- `scripts/check-merge-readiness.sh` on a clean worktree before landing.
- Device check after landing (manual, device-lock): reseed the ZZ artists,
  scroll to the bottom, delete, and record. The rows must not move when
  "Deleting 13 tracks…" appears.

## Parallelität

The plan cannot be cut, so it runs as **one strand**.

- `BrowseScreen.kt` is touched by both the extraction (task 4) and the overlay
  wrap (task 3).
- T1 and T2 need the finished `BrowseScreen` and the finished pill together.
- T3 and the pill both depend on `DeletionMessages.kt`.
- The FB-9 paragraph names the test class, so it has to follow the tests.

There is no disjoint file group that would let a second worktree go green on its
own, and the whole change is roughly 150 lines of production code plus tests.

File ownership: `android/app/src/main/java/io/github/marvinbaudach/reprise/{BrowseScreen,BrowseStatusLines,DeletionMessages}.kt`,
`android/app/src/test/java/io/github/marvinbaudach/reprise/DeletionLineOverlayTest.kt`
(new), finder-only edits in `DeletionProgressLineTest.kt`, and the FB-9
paragraph in `docs/ux-rules.md`.

AGENTS.md lists `docs/ux-rules.md` as owned by strand A (Flathub readiness). On
2026-10-02 no branch or open PR for that strand existed, only Dependabot PRs, so
the entry is stale and the FB-9 edit is free to land.

Post-merge cross-checks: none. Every verification above reads only files this
strand owns.
