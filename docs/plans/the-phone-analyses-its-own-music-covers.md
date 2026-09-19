---
slug: the-phone-analyses-its-own-music-covers
worktree: /home/marvin/Projects/reprise-the-phone-analyses-its-own-music-covers
branch: feature/the-phone-analyses-its-own-music-covers
phase: reviewed
codex_session:
created: 2026-09-18
---
# Strand `covers` — album covers from the internet

Strand of `docs/plans/the-phone-analyses-its-own-music.md` (the mother plan:
context, decisions 8–13, rules, gates, device run). Read it first; this file
holds only this strand's tasks and ownership. Base `origin/dev` @ `3403a6e2cb`.

## Ownership

Owns exactly:

- `crates/reprise-core/src/cover_download.rs`, `crates/reprise-core/src/cover_download_retry_tests.rs`
- `crates/reprise-core/src/artist_portrait/cover_backfill.rs` and `cover_backfill_tests.rs` (new), plus the `mod` lines for them in `crates/reprise-core/src/artist_portrait/mod.rs`. The portrait engine `backfill.rs` is a **read-only reference** — it is not edited (B3).
- `crates/reprise-android-ffi/src/artist_portrait.rs`, `crates/reprise-android-ffi/src/artist_portrait_tests.rs`, `crates/reprise-android-ffi/src/artist_portrait/**` (new: `album_cover.rs`, `album_cover_tests.rs`), `crates/reprise-android-ffi/src/online_sources.rs`
- `android/app/src/main/java/io/github/marvinbaudach/reprise/{TrackCover.kt, LibrarySession.kt, AndroidLibrarySessionPort.kt, MainActivity.kt, ArtistPhotoBackfillConnection.kt, ArtistPhotoOffer.kt, ArtistPhotoOfferBanner.kt, BrowseTabs.kt, NowPlayingScene.kt, ArtistCover.kt}` — `NowPlayingScene.kt` for exactly one argument at `:467` (the now-playing request site, B4); `panelHasVisualData` (`:238-260`) and `NowPlayingPanelsTest` are not touched
- `android/app/src/main/java/io/github/marvinbaudach/reprise/settings/OnlineSourcesSettingsPage.kt`
- `android/app/src/test/java/io/github/marvinbaudach/reprise/{TrackArtworkTest.kt, ArtistArtworkTest.kt, ArtistPhotoOfferTest.kt, OnlineSourcesSettingsPageTest.kt, AlbumCoverFetchTest.kt (new)}`
- this file

Does **not** touch `crates/reprise-android-ffi/src/lib.rs`,
`library_types.rs`, `track_analysis*`, `mobile_sync.rs`,
`crates/reprise-core/src/lib.rs`, `cover.rs`, `MobileSurfaceViewModel.kt`,
`ReprisePlaybackService.kt`, `SharedMusicLibrary.kt`, `TrackAnalysisLoader.kt`,
any `Cargo.toml`/`Cargo.lock`. No struct field is added to `MusicLibrary`
(B2 injects at call level). The list is the starting point, not a fence: if
state must live in a file outside it, stop and report rather than edit it.

## Facts this strand builds on

- `fetch_and_cache(album_artist, album, mbid: Option<&str>, album_dirs) -> CoverFetchOutcome`
  (`cover_download.rs:329-343`) delegates to `fetch_and_cache_with(…, mb_fetch, caa_fetch)`
  (`:345`); every path it writes derives from the XDG `downloaded_dir()`
  (`:93`): `store_downloaded` (`:562-579`), `store_album_downloaded` (`:583-603`,
  with the album-directory write-back), `negative_marker_path` (`:119`),
  `publish_marker`/`note_publication` (`:549-560`). `downloaded_dir_in(cache_root)`
  (`:97`) and `downloaded_cover_path_in(cache_root, key)` (`:107`) already
  exist for readers. `album_key(album_artist, album)` (`:82`).
- The Android resolver (`cover.rs:83-93`, `resolve_source_with_source`)
  checks `downloaded_cover_path_in(cache_root, key)` as stage 1; the FFI
  passes `self.cache_root` (`crates/reprise-android-ffi/src/lib.rs:294-312`,
  `track_artwork(track_uri, size)`). `cache_dir_with_root(cache_root)` is
  `cache_root.join("reprise/covers")` (`cover.rs:203-206`).
- The desktop caller (`crates/reprise-gnome/src/ui/cover/cover_download_worker.rs:220-262`)
  picks the artist, memoises attempts per `album_key` except
  `TransientFailure`, and passes `tag.release_mbid`.
- `artist_portrait_fetch` (`crates/reprise-android-ffi/src/artist_portrait.rs:106-130`)
  is the template for the on-demand call; `start_artist_portrait_backfill`
  (`:184-215`) for the backfill; `PortraitBackfill::start(database_path, cache_dir, fetch, listener)`
  (`crates/reprise-core/src/artist_portrait/backfill.rs:135-160`) opens its own
  `Db` on the worker, rechecks consent through `network_allowed_or_off`, and
  publishes `PortraitBackfillProgress` (`:37`).
- `ArtworkRequest(trackUri, size, title, artist, kind, artistName, allowFetch)`
  (`ArtworkRequestGate.kt:16-24`); `TrackArtwork(resolve, resolveArtistPortraitCached, resolveArtistPortraitFetched)`
  (`TrackCover.kt:50-62`), `resolveVisual` (`:201-209`), fallback (`:240`),
  `rememberTrackArtworkVisual` (`:314-326`); artist detail requests with
  `allowFetch = true` (`BrowseTabs.kt:351`); `AlbumDetailHeader` (`BrowseTabs.kt:169`).
- `LibrarySession.artworkPaths` (`LibrarySession.kt:87-99`) memoises resolved
  paths per `ArtworkCacheKey` with a generation counter; `artworkFor` (`:237-`).
- `MainActivity.kt:89-93` constructs `TrackArtwork` with `session::…` references;
  `:208` calls `surfaceState.connectArtistPhotoBackfill(library) { … }`.
- The settings row: `SettingsSwitchRow` (`settings/SettingsControls.kt:18-50`),
  used in `OnlineSourcesSettingsPage.kt`; the offer: `shouldOfferArtistPhotos(gateEnabled, settled, artistCount)`
  (`ArtistPhotoOffer.kt:15`).

## B1 — `fetch_and_cache_in(cache_root, …)` in core

`cover_download.rs`: thread a `downloaded_dir: &Path` through
`fetch_and_cache_with`, `store_album_downloaded`, `store_downloaded`,
`negative_marker_path` and `note_publication`/`publish_marker` (a
`_in(dir, …)` sibling for each, the way `artist_portrait::load_or_fetch_in`
was carved out). The desktop keeps calling `fetch_and_cache`, which now
delegates with `downloaded_dir()`; its behaviour is unchanged by
construction. New: `pub fn fetch_and_cache_in(cache_root: &Path, album_artist: &str, album: &str, mbid: Option<&str>, album_dirs: &[PathBuf]) -> CoverFetchOutcome`
using `downloaded_dir_in(cache_root)`. The negative marker and the publish
marker live under the same root as the cover. Keep the file under the 800
line cap: if the `_in` siblings push it over, extract `cover_download/store.rs`
(a child module of `cover_download.rs`, no `lib.rs` line).

Tests (`cover_download_retry_tests.rs` or a child-module test file):
`a_cover_lands_under_the_given_root`, `a_negative_marker_lands_under_the_given_root`,
`the_default_root_path_is_unchanged` (the existing global-path tests stay
green untouched — that is the proof).

## B2 — the FFI: on-demand fetch, mirroring `artist_portrait_fetch`

`crates/reprise-android-ffi/src/artist_portrait/album_cover.rs`, declared as
`mod album_cover;` **inside `artist_portrait.rs`** (plus `#[cfg(test)] #[path] mod album_cover_tests;`),
own `#[uniffi::export] impl MusicLibrary` block:

- `album_cover_fetch(track_uri: &str, size: AndroidArtworkSize) -> Result<Option<String>, LibraryError>`:
  1. gate: `network_allowed_or_off(&reader, &ARTWORK_MODULE)`; off → `Ok(None)`
     without touching the network;
  2. under `reader`: the track by URI (album, album artist / artist — copy the
     desktop's choice from `cover_download_worker.rs:220-236`), release the
     guard; release MBID from the tag when the resolver's source carries it,
     else `None`;
  3. offline resolution first (`track_artwork`'s path): real art found →
     return its thumbnail path, no request (decision 9);
  4. per-process `attempted: Mutex<HashMap<String, CoverFetchOutcome>>` keyed by
     `album_key`, as the desktop worker keeps it; a memorised miss returns
     `Ok(None)`; `TransientFailure` is never memorised;
  5. `fetch_and_cache_in(&self.cache_root, artist, album, mbid, &[])`; on a
     fetched cover re-run the normal `track_artwork` resolution and return its
     thumbnail path.
  Test injection at call level: an internal
  `album_cover_fetch_with(&self, track_uri, size, fetch: &dyn Fn(&str, &str, Option<&str>) -> CoverFetchOutcome)`
  that the export calls with the real `fetch_and_cache_in` closure. No field
  on `MusicLibrary`; the `attempted` map is a `static` `OnceLock<Mutex<…>>`
  in this module (the library is a process singleton). **`cargo test` runs
  every test of the crate in one process**, so the module exposes
  `pub(crate) fn reset_album_cover_state_for_tests()` and every test in
  `album_cover_tests.rs` and `artist_portrait_tests.rs` that touches this
  path calls it first — otherwise a memorised miss from one test leaks into
  the next and the file becomes order-dependent.

Tests (`album_cover_tests.rs`): `the_gate_off_fetches_nothing`,
`local_art_is_never_replaced_by_a_download`,
`a_fetched_cover_is_found_by_the_resolver` (the fake fetch writes a PNG under
`downloaded_dir_in(cache_root)`; `track_artwork` then resolves it),
`a_miss_is_remembered_for_the_process`, `a_transient_failure_is_retried`,
`the_reader_is_released_before_the_fetch` (the fake fetch takes
`reader.try_lock()` and asserts `Ok`).

## B3 — the cover pass rides the artwork backfill

`start_artist_portrait_backfill` (`artist_portrait.rs:184-215`) keeps its name
and stays the single entry point. The cover pass is a **separate run with
its own handle**, `CoverBackfill` in
`crates/reprise-core/src/artist_portrait/cover_backfill.rs`, written after
the pattern of `PortraitBackfill` (`backfill.rs:115-`: own worker thread, own
`Db::open_ready(database_path)` on the worker, a `cancel` flag, a listener)
— **`backfill.rs` is read, not edited**, and its `launch` engine is not
threaded with a second worklist. Chaining: the FFI's forwarding closure in
`start_artist_portrait_backfill` watches the portrait progress it already
receives and, on the portrait run's completion state, starts the cover run
with the same consent closure; `cancel_artist_portrait_backfill` cancels
both. The cover handle lives as a `static OnceLock<CoverBackfill>` in
`album_cover.rs` (same rule as `attempted`, same reset fn) — no field on
`MusicLibrary`.

The cover worklist, read on the worker from its own `Db`: one representative
track per album; per album the offline resolution first, a fetch only on a
placeholder (decision 9), one album at a time (the MusicBrainz pacing lives
in `cover_download`), consent rechecked through `network_allowed_or_off`
before every album. Progress: the listener's `ArtistPortraitProgressUpdate`
gains `covers_done`/`covers_total`; the Kotlin binding
(`ArtistPhotoBackfillConnection.kt`, B5) forwards them. If the chain cannot
be built from the progress the FFI closure already sees, stop and report —
do not edit `backfill.rs` or `library_types.rs`.

Tests (`cover_backfill_tests.rs`): `the_cover_pass_starts_after_the_portraits`,
`albums_with_local_art_are_skipped`, `cancel_stops_the_cover_pass`,
`consent_withdrawn_mid_run_stops_the_pass`.

## B4 — Kotlin: fetch at the two rungs that may fetch

- `LibrarySession.kt` / `AndroidLibrarySessionPort.kt`: `artworkFetched(trackUri, size): String?`
  → `library.albumCoverFetch(trackUri, size)`; on a non-null result the memo
  entries for that track URI in `artworkPaths` are dropped (or the
  generation bumped — read `:87-99` and pick the one the existing
  invalidation uses), so the next `artworkFor` sees the cover.
- `TrackCover.kt`: `TrackArtwork` gains `resolveAlbumCoverFetched: (String, AndroidArtworkSize) -> String? = { _, _ -> null }`
  so every existing construction compiles; `resolveVisual` (`:201-209`) calls
  it for `kind == TRACK && allowFetch` when `resolve` came back unresolved,
  on the full-size lane (`artworkFullSizeLane`, `:363`), never on the list
  lane. `rememberTrackArtworkVisual` (`:314`) gains `allowFetch: Boolean = false`
  and passes it into the request.
- `MainActivity.kt:89-93`: `resolveAlbumCoverFetched = session::artworkFetched`
  — this strand's only line in that file.
- The now-playing full-size cover requests with `allowFetch = true`: the
  site is `NowPlayingScene.kt:467` (`rememberTrackArtworkVisual(...)`) — one
  added argument, nothing else in that file. `AlbumDetailHeader`
  (`BrowseTabs.kt:169`) requests with `allowFetch = true`. `DockMode.kt:49`,
  `NowPlayingSheet.kt:454`, `LibraryFrame.kt:287`, `LibraryTrackRows.kt:386`
  and every list row stay `false`; `MobileSurfaceViewModel.kt`'s prefetch
  stays list-sized and fetch-free.

Tests: `TrackArtworkTest.kt` — `a_list_row_never_fetches_a_cover`,
`the_now_playing_rung_fetches_when_local_art_is_missing`,
`the_now_playing_rung_does_not_fetch_when_local_art_exists`,
`a_fetched_cover_replaces_the_placeholder_without_a_restart`;
`AlbumCoverFetchTest.kt` — a `LibrarySession` double proves the gate-off
path never reaches `artworkFetched` from a list request (the gate itself is
proven in Rust, B2).

## B5 — Kotlin: wording and the backfill connection

`settings/OnlineSourcesSettingsPage.kt`: the row title becomes "Download
artwork"; the supporting text names both — artist photos and album covers,
from MusicBrainz and the Cover Art Archive — and says covers are only fetched
for albums without their own. `ArtistPhotoOffer.kt`/`ArtistPhotoOfferBanner.kt`:
the offer text says artwork; `shouldOfferArtistPhotos` keeps its name and
signature. `ArtistPhotoBackfillConnection.kt`: forwards the extended progress
(covers included) into the existing binding. Tests:
`OnlineSourcesSettingsPageTest.kt`, `ArtistPhotoOfferTest.kt` updated for the
wording; the connection test (where the existing one lives) sees
`covers_done` arrive on the main thread.

## B6 — gates and hand-over

Run the gate list from the mother plan (scoped, never `--workspace`). Record
in `.pipeline-codex.md`: the red-then-green of every test above, the exact
`cargo test` and suite counts, and the file claimed for the now-playing
request site in B4.
