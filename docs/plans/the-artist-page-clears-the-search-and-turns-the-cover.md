---
slug: the-artist-page-clears-the-search-and-turns-the-cover
worktree: /home/marvin/Projects/reprise-the-artist-page-clears-the-search-and-turns-the-cover
branch: feature/the-artist-page-clears-the-search-and-turns-the-cover
phase: reviewed
codex_session:
created: 2026-08-29
---
# The artist page clears the search and turns the cover

On the phone, the Artists tab hands the listener a search field that outstays
its welcome and results that answer a question nobody asked, and then drops them
on an artist page that is a static portrait over two lists. Three changes, all
in the Android app under `android/app/src/main/java/de/reprise/spike/`:

1. Tapping a search result closes the field **and** empties the query, so the
   artist page carries no search at all.
2. The artist search answers with artists. The Albums section goes.
3. The artist portrait gets the desktop's turning cover disc behind it.

## What is actually true today

### The field survives the navigation

`LibrarySearchField` is composed on one condition and one only —
`if (searchVisible)` (`BrowseScreen.kt:593`). The `openArtist` callback
(`BrowseScreen.kt:652-663`) sets `selectedArtist`, resets two offsets and clears
`browseError`; it never touches `searchVisible` and never touches `searchText`.

Only one path closes the field on its own — `MobileSurfaceViewModel.selectTab`
(`MobileSurfaceViewModel.kt:227-230`), and only for `BrowseTab.QUEUE`, with the
comment saying why: a standing search is *meant* to follow the listener from tab
to tab. That reasoning is about tabs. Opening an artist is not a tab change, and
the comment does not cover it.

So the artist page is drawn under an open, still-filled search field, and going
back lands on the filtered list rather than the catalog.

`ArtistDetailSurfaceTest.kt:218` asserts the label is displayed, but inside a
`setContent` that composes `LibrarySearchField` on its own — it pins the string,
not the behaviour. Nothing pins "the field survives opening an artist". Nothing
has to be un-asserted, only asserted the other way.

### The Albums section is a leftover of a dropped tab

`ArtistsTab` has three branches (`BrowseTabs.kt:176-315`): selected album,
selected artist, and then — for a non-blank query — `ArtistSearchSections`
(`BrowseTabs.kt:397-450`), a `LazyColumn` with an "Albums" heading over
`searchAlbums()` results and an "Artists" heading over `searchArtists()`
results. Only a blank query falls through to `ArtistRows` (`BrowseTabs.kt:553`).

Commit `f3504da892` ("Android: reach albums through the artist, drop the Albums
and Favourites tabs") added that section as the compensation for removing the
Albums tab.

It is not free of consequences elsewhere:

- **It is the only reason `OpenAlbumOrigin` has two values.** `openAlbumDetail`
  is called from exactly one place (`BrowseScreen.kt:670`), which picks
  `ARTIST_SEARCH` when no artist is open and `ARTIST_DETAIL` otherwise. Its one
  consumer is the restore gate at `MobileSurfaceViewModel.kt:352-360`, which
  drops a restored `openAlbum` whose `openArtist` is gone.
- **It owns a second scroll anchor.** `LibraryListKey.ARTIST_SEARCH_ALBUMS`
  (`MobileSurfaceViewModel.kt:34`) and its tag
  `"library-artist-search-albums-list"` (`LibraryTrackRows.kt:238`) exist only
  for that list. The unfiltered artist list uses `LibraryListKey.ARTISTS`.
- **It costs the filtered list its grid.** `ArtistRows` renders a
  `LazyVerticalGrid` on `SurfaceLayout.WIDE_SHORT` (`BrowseTabs.kt:563-585`) and
  a `LazyColumn` otherwise. `ArtistSearchSections` has no grid branch, so in
  landscape the filtered artists are one column while the unfiltered ones are
  two.
- **It forces two strings off the generic path.** The label is special-cased for
  the Artists tab — `"Search albums and artists"` instead of
  `"Search ${tab.label.lowercase()}"` (`BrowseTabs.kt:99-104`) — and the empty
  state is a hand-written `"No matching albums or artists."`
  (`BrowseTabs.kt:284`) instead of `BrowseTab.emptyMessage(searchText)`
  (`BrowseTabs.kt:37-46`), which already yields `"No matching artists."`.

`TitlesTab` is the shape this collapses to (`BrowseTabs.kt:49-78`): one list key
for filtered and unfiltered alike, with `emptyMessage(searchText)` carrying the
difference.

**The price, stated once.** After this, an album whose *title* matches but whose
*artist name* does not is not reachable from search — only by walking to its
artist. That is the deliberate reversal of `f3504da892`, and it is what was
asked for.

### The portrait is static, and the desktop's disc already has an Android port

`ArtistPortraitHeader` (`ArtistCover.kt:66-89`) is a `Column` with an
`ArtworkCover` at 210 dp and a details line, composed as one `item` of the
artist page's `LazyColumn` (`BrowseTabs.kt:353`) — so anything drawn inside it
scrolls away with it.

What "wie beim Desktop" refers to exists:
`crates/reprise-gnome/src/ui/now_playing/cover_shimmer.rs` — a blurred,
circularly masked copy of the cover turning one revolution per 60 s behind the
cover, its opacity lifted by bass. And it is **already ported to Android**:

```kotlin
// NowPlayingShimmer.kt:58-66
internal fun DrawScope.drawNowPlayingShimmer(
    fog: CoverFogBitmap?,
    center: Offset,
    coverDiameterDp: Float,
    elapsedSeconds: Double,
    swell: Float,
    opacity: Float,
    rotationsEnabled: Boolean,
)
```

with `TURN_SECONDS = 60.0` and `DESKTOP_DIAMETER_TO_COVER_RATIO = 520f / 168f`
carried over, and the note that the ratio, not a dp size, is the geometry
contract (`NowPlayingShimmer.kt:33-41`). Its only caller today is
`drawPlayedNowPlayingShimmer` (`NowPlayingScene.kt:358-374`).

Its inputs are reachable off the now-playing scene, with two exceptions:

| input | on the artist page |
|---|---|
| `fog: CoverFogBitmap?` | `rememberCoverFogBitmap(visual.image, …)` (`CoverFogBitmap.kt:105`) — prepares off the main thread via `Dispatchers.Default`, caches in `SharedArtworkCache` |
| `center`, `coverDiameterDp` | the portrait's own geometry |
| `opacity`, `rotationsEnabled` | `1f`; `LocalAmbientMotionController.current.sceneRenderPower().fogRotates` (`AmbientRuntime.kt:24-27`) |
| `elapsedSeconds` | **not reachable** — `SceneState.shimmerElapsedSeconds` is advanced by `SceneDriver` |
| `swell` | **not reachable** — `SceneState.fogLevel` is fed by `FogDrive.step` from spectrogram bands |

And the fact that decides how far to chase those two: **`NowPlayingScene` is not
composed while the artist page is on screen.** The sheet sits inside
`AnimatedVisibility(visible = nowPlayingExpanded && …)` (`BrowseScreen.kt:736-764`),
which removes its content from the composition when collapsed. So `SceneState`
is neither alive nor ticking behind the library — hoisting it into a
`CompositionLocal` would hand the artist page a frozen value, and getting a live
one means a *second* `SceneDriver` plus a second spectrogram load running beside
the library list. That is a new signal path and new per-frame cost, guarded by
`NowPlayingFrameRateTest`, for a background bloom.

Therefore the artist page brings its **own** wall clock, and its swell is the
transport's playing flag eased into a level. `LibraryPlayback.state`
(`PlaybackUiState.kt:23-28`) is already a parameter of `ArtistDetailSections`.
`NowPlayingShimmerSpec.alpha` reads swell through
`NowPlayingFogSpec.normalizedSwell` (`NowPlayingFog.kt:24`), which clamps to
0..1, so `0f` gives the rest disc and `1f` the lit one with the tuned
proportions between them untouched.

This is honestly less than the now-playing disc: it brightens *because a track
is playing*, not *with the music*. That is the trade the composition boundary
forces, and it is the whole of the difference. **Decided and closed** — do not
reopen it by wiring up a second `SceneDriver`.

## What changes

### Task 1 — opening an artist clears the search

`BrowseScreen.kt`, in the `openArtist` callback (`:652`), on success only:

```kotlin
surfaceState.closeSearch()
if (searchText.isNotEmpty()) {
    surfaceState.updateSearch("")
    loadedTabs = emptySet()
}
```

**Do not call `search("")` here.** `search` (`BrowseScreen.kt:276-309`) runs
`artistsFor(...)` synchronously — a blocking JNI + SQLite call at the exact
moment of a navigation. Clearing `loadedTabs` instead re-keys the prefetch effect
(`BrowseScreen.kt:373-411`), which runs the same query inside
`withContext(Dispatchers.IO)` and assigns `visibleArtists` when it lands. The
artist page is on screen while that runs, so the stale filtered window is never
seen; by the time "back" is pressed the catalog is in.

**Not** an `onFocusChanged` / `onFocusEvent` listener on the field, despite the
word "Fokusverlust" in the request: the request defines it as "ich klicke auf ein
Resultat", and a real focus listener also fires on recomposition and on dispose,
which would eat the query while the listener is still scrolling results.

**Only `openArtist`.** The `openAlbum` callback (`:664`) gets nothing: after
Task 2 an album is reachable only from the artist detail page, where this has
already run. A second copy there would be a branch no test can reach.

Consequence to carry: the restore gate (`MobileSurfaceViewModel.kt:350`) keeps
paged-in windows only while `it.searchText == searchText`. Clearing the query
while an artist is open therefore drops the *filtered* windows from the restore —
correct, because they are no longer what the tab shows, and the open artist
survives on its own field (`LoadedLibraryWindows.openArtist`).

### Task 2 — the artist search answers with artists

Delete the album half and let the search branch collapse into the list below it.

`BrowseScreen.kt`:
- `visibleArtistSearchAlbums` (`:151`), `artistSearchAlbumsRequestedOffset`
  (`:163`), `loadMoreArtistSearchAlbums` (`:425-428`);
- the `searchAlbums` call in `search()` (`:292-296`) and in the prefetch effect
  (`:392-397`), and the `artistSearchAlbums` field of `LoadedTab`;
- the two `if (selectedArtist == null)` ternaries choosing between search and
  detail offsets/loaders (`:686-694`) — both collapse to the detail side.

`BrowseTabs.kt`:
- `ArtistSearchSections` (`:397-450`) goes entirely, with the `albumResults`
  parameter of `ArtistsTab` (`:180`);
- the `searchText.isNotBlank()` branch (`:280-301`) goes, so the function falls
  through to the existing `artists.rows.isEmpty()` / `ArtistRows` tail — the
  shape `TitlesTab` already has. The filtered list thereby gains the landscape
  grid it lacks today, and shares the `ARTISTS` scroll anchor with the catalog.
  That anchor sharing is intended: it is what `TITLES` does, and
  `within(itemCount)` clamps a stale index;
- `:99-104`: drop the `tab == BrowseTab.ARTISTS` special case; the label becomes
  `"Search artists"` from the generic branch.

`MobileSurfaceViewModel.kt` / `LibraryTrackRows.kt`:
- remove `LibraryListKey.ARTIST_SEARCH_ALBUMS` (`:34`) and its tag
  (`LibraryTrackRows.kt:238`);
- remove `OpenAlbumOrigin` (`:39-42`), the `openAlbumOrigin` field of
  `LoadedLibraryWindows` (`:73`), the `origin` parameter of `openAlbumDetail`
  (`BrowseScreen.kt:259`) and its uses (`:160`, `:263`, `:337`, `:664-670`).
  The restore gate (`:352-360`) loses its `openAlbumOrigin ==` clause and keeps
  the rest: an open album on the Artists tab with no open artist is dropped.

Keep `SectionHeading` — `ArtistDetailSections` still uses it for "Albums" and
"Other titles".

**The `searchAlbums` UI parameter chain dies with the section, the port method
does not.** `BrowseScreen`'s three uses (`:295`, `:393`, `:429`) are all in the
Artists branch, so after this it is an unused parameter threaded through four
files: `MainActivitySurface.kt:23` → `LibraryScreen.kt:25,80` →
`BrowseScreen.kt:104`. Remove those, and the `searchAlbums = …` argument at the
five `ComposeBehaviorTest.kt` call sites (`:181`, `:285`, `:379`, `:583`, `:640`)
and in the `MainActivityConfigurationTest` fixture (`:672-677`).

**Keep** `LibrarySession.searchAlbums` (`LibrarySession.kt:34`, `:175-178`) and
`AndroidLibrarySessionPort.searchAlbums` (`:88-91`): `LibrarySession.kt:303`
counts the library's albums for the summary row with
`port.searchAlbums("", countOnlyLibraryWindow()).total`. That is not a search and
it is not going away.

`LibraryScreen.kt` is **not** a second `ArtistsTab` call site — it is a
pass-through to `BrowseScreen` (`:69-99`) and needs no other change.

### Task 3 — the turning disc behind the portrait

New file `ArtistPortraitShimmer.kt`. The package's files are small and
single-purpose; `ArtistCover.kt` stays about the portrait itself, which is
unchanged.

One composable that draws the disc and nothing else:

```kotlin
@Composable
internal fun ArtistPortraitShimmer(
    visual: ArtworkVisual?,
    playing: Boolean,
    coverDiameterDp: Float,
    centerFraction: Float,
    modifier: Modifier = Modifier,
)
```

- **No portrait, no disc.** Return immediately when `visual?.image == null`, so a
  portraitless artist schedules no frames. A disc built from a flat fallback
  colour would show no rotation anyway.
- `fog` from `rememberCoverFogBitmap(image, …)`.
- `rotationsEnabled` from
  `LocalAmbientMotionController.current.sceneRenderPower().fogRotates`. This is
  not decoration: a private `rememberInfiniteTransition` would keep turning with
  the screen off and would ignore `ANIMATOR_DURATION_SCALE`, the two things
  `AmbientRuntime.kt` exists to observe and `AmbientMotionTest.kt` guards.
- `elapsedSeconds` from a `LaunchedEffect(rotationsEnabled)` that accumulates
  `withFrameNanos` deltas into a `mutableDoubleStateOf` and **returns when
  `rotationsEnabled` is false**, so a stopped disc schedules no frames. Deltas
  rather than a wall clock, so the disc resumes where it stopped instead of
  jumping.
- `swell` from an `Animatable(0f)` eased to `1f` while `playing`, back to `0f`
  otherwise. `NowPlayingShimmerSpec` normalises it, so no new constant is needed;
  reuse the package's existing animation-spec conventions rather than inventing a
  duration.
- Draws in a `Canvas` via `drawNowPlayingShimmer`, `center` computed from the box
  size and `centerFraction` so it sits behind the portrait rather than the middle
  of the page.

Call site: `ArtistDetailSections` (`BrowseTabs.kt:330-393`). Wrap the
`LazyColumn` in a `Box` and put `ArtistPortraitShimmer(head, …)` **before** it —
behind the list, outside it, at a fixed position that does **not** follow the
scroll. A disc drawn inside the `"artist-portrait-head"` item would scroll off
with the header; one that tracks `listState` would read scroll state every frame.
`head` is already the artist's `ArtworkVisual` (`BrowseTabs.kt:332-337`), and
`playing` is `playback.state == AndroidPlaybackState.PLAYING` from a parameter
that is already there.

## How this is verified

Robolectric + Compose testing under `android/app/src/test/java/de/reprise/spike/`.
Every claim below fails on today's code.

**Task 1.** In `ArtistSearchActivityTest`, which already drives the real
`MainActivity`: open the Artists tab, open the search, type a query matching one
artist, tap the artist row, assert the field is gone
(`onNodeWithText("Search artists").assertDoesNotExist()`) — then press back and
assert an artist the query excluded is on screen again. That second half is what
proves the query was *cleared* rather than merely hidden; a test asserting only
the field's absence passes on a `closeSearch()` that left the text standing.

Two things this test will get wrong if they are not said:

- **Pick a query that actually excludes something.** The fixture's
  `searchArtists` is a plain substring match over `"Artist 1".."Artist 450"`
  (`MainActivityConfigurationTest.kt:682-685`), so most queries exclude almost
  nothing and the "back" assertion passes vacuously. `"Artist 45"` matches only
  `Artist 45` and `Artist 450`, which makes `"Artist 1"` a genuine excluded row.
- **`waitForIdle()` is not enough for the second half.** The catalog is reloaded
  on `Dispatchers.IO` by the prefetch effect, and Robolectric's idle check does
  not reliably span that. Use `waitUntil { … }` on the excluded artist appearing.

**Task 2.** `emptyArtistSearchShowsArtistsWithoutAnAlbumSection`
(`ArtistDetailSurfaceTest.kt:198`) already asserts no "Albums" heading for a
blank query; the new test is the same assertion for a non-blank one.

**Task 3.** Pixel capture, which this suite already does — `VisualizerScenePixelsTest`,
`SpectralSeekTrackPixelsTest` and `NowPlayingSceneVerificationTest` all use
`captureToImage`. `AmbientMotionTest`'s bounds trick does not apply here:
`drawNowPlayingShimmer` rotates inside the `DrawScope` via `rotate()`, so there
is no child node whose position carries the transform.

- *It turns*: `mainClock.autoAdvance = false`, capture, advance 15 s — a quarter
  of the 60 s period — capture again, assert the frames differ. Compare a region
  **off** the disc's centre; rotation moves nothing at the axis.
- *It stops*: the same two captures with `animationsEnabled = false` on
  `ConfigurationTestApplication` (`MainActivityConfigurationTest.kt:454`) must be
  identical.

A test that reads the scheduling flag instead would be asking the flag about
itself — the trap `AMBIENT_FIELD_TAGS` was written against
(`AmbientSurface.kt:146-158`). `ambientScheduleEvents` on the fixture may
supplement the pixel tests; it may not replace them.

### Tests this breaks, and what each becomes

Found by grepping the removed strings and tags. All are real; none may be
deleted silently.

| test | today | becomes |
|---|---|---|
| `ArtistSearchActivityTest.artistSearchQueriesBothKindsAndOpensAnAlbumDirectly` (`:39`) | searches "artist 2", asserts both "Full Album 2" and "Artist 2", opens the album | searches an artist, asserts artists only and no "Albums" heading, opens the **artist**. Natural home for the Task 1 assertion too. |
| `MainActivityConfigurationTest.artistSearchFiltersAlbumsAndClosingItRestoresTheArtistCatalog` (`:107`) | query `"full"` matches only albums; asserts "Full Album 2" shown, "Artist 2" absent | query must match artists. The fixture holds `"Artist 1".."Artist 450"` (`:477-485`), searched by substring on the name (`:682-685`). Keep the shape: filter, clear, close, catalog back. |
| `MainActivityConfigurationTest.filteredAlbumPaginationAndItsSearchSurviveRecreation` (`:141`) | pages to index 210 of the filtered albums via `library-artist-search-albums-list`, rotates, asserts "Full Album 212" | the same against `library-artists-list` with a query matching all 450 artists and `"Artist 212"`. Watch the rotation target `w916dp-h412dp-land`: the filtered list is a `LazyVerticalGrid` there afterwards, so the anchor crosses list→grid. The machinery exists (`ObserveLibraryGridAnchor`; `:89` asserts it for the unfiltered list), but this is the assertion most likely to need adjusting. |
| `MobileHeaderRowTest` (`:79`) | label `"Search albums and artists"` | `"Search artists"` |
| `ArtistDetailSurfaceTest.artistSearchFieldNamesAlbumsAndArtists` (`:211`) | same label | rename the test with the string |
| `ArtistDetailSurfaceTest` (`:185-195`) | opens an album from artist search, asserts "Back to artists" absent | that path is gone; fold the surviving assertion — an album opened from artist *detail* keeps its way back — into the detail case, or drop it with a note in the commit |
| `MobileSurfaceStateTest.pagedInWindowsAreRestoredOnlyWithTheSearchThatProducedThem` (`:173`) | builds `LoadedLibraryWindows(artistSearchAlbums = LibraryWindow(total = 450, …))` | the field is gone; the test's point — windows are restored only under the query that produced them — is carried by `artists` instead |
| `MobileSurfaceStateTest` (`:226`) | constructs `openAlbumOrigin = OpenAlbumOrigin.ARTIST_SEARCH` | the field is gone; the case it stood for is the `openArtist == null` branch |
| `ComposeBehaviorTest` (`:181`, `:285`, `:379`, `:583`, `:640`) and the `MainActivityConfigurationTest` fixture (`:672-677`) | pass a `searchAlbums` lambda into the screen | the parameter is gone; drop the argument |

Run the Android unit suite from `android/` with the project's `testDebugUnitTest`
task via `./gradlew`, plus the repo's existing Android lint/format gate. Do not
invent a new gate script.

## Risks

- **A short window where the filtered rows outlive the field.** `visibleArtists`
  is `remember(state)` (`BrowseScreen.kt:154`) — keyed on `state`, not on
  `searchText` as the offsets at `:163`/`:166` are. Today `search()` assigns it
  synchronously (`:291`), so no window exists. After Task 1 the reload runs on
  `Dispatchers.IO`, and between the tap and its landing `visibleArtists` still
  holds the filtered rows: a listener who taps "Back to artists" inside that
  window sees a short list with no search field to explain it. Accepted, not
  redesigned — the window is one IO round-trip while a full artist page is being
  drawn over it. Do not "fix" it by moving the query back onto the main thread.
- **The pixel diff.** One revolution takes 60 s, so two frames milliseconds apart
  are identical. Advance a quarter turn, and sample away from the axis.
- **The disc's alpha on a light background.** `PHONE_FOG_ALPHA_SCALE = 1f/3f` was
  tuned against `AmbientTrueBlack` in the now-playing scene
  (`NowPlayingShimmer.kt:26-31`). The artist page sits on the ordinary Material
  surface in both palettes. If the disc is invisible in light or muddy in dark,
  adjust the **fallback colour handed to `rememberCoverFogBitmap`**, never the
  shared spec constants — the now-playing tuning must not move.
- **Recomposition cost.** Read the elapsed-seconds state **inside the `Canvas`
  draw lambda only**. A read in the composable's own scope would recompose the
  whole artist page 60 times a second.
- **Disc size.** At a 210 dp portrait the disc is ~650 dp across, wider than any
  phone. That is expected — it is a full-bleed bloom, as in the now-playing scene
  where a 272 dp cover yields ~840 dp.

## Parallelität

**No cut. One strand.**

The reason is file ownership, not size. Tasks 1 and 2 both restructure
`BrowseScreen.kt` and `BrowseTabs.kt`: Task 1 edits the `openArtist` callback at
`BrowseScreen.kt:652`, Task 2 deletes the ternaries at `BrowseScreen.kt:686-694`
— inside the same `ArtistsTab(...)` call — and removes the
`searchText.isNotBlank()` branch of `ArtistsTab` in `BrowseTabs.kt`, which is the
branch Task 1's tap lands on. Task 3 adds a new file, but its call site is
`ArtistDetailSections` in `BrowseTabs.kt`, the same file Task 2 rewrites.

A cut of "Task 3 owns only `ArtistPortraitShimmer.kt`, Tasks 1+2 own the rest and
make the one call" would be a strand whose entire deliverable cannot be composed
or tested until the other lands — a strand that cannot go green on its own, which
is the failure mode this section exists to prevent.

The three tasks also share their test files: `ArtistDetailSurfaceTest.kt`,
`ArtistSearchActivityTest.kt` and `MainActivityConfigurationTest.kt` are touched
by Task 1's new assertions and by Task 2's rewrites alike.

**Post-merge cross-checks:** none — a single strand has no seam.
