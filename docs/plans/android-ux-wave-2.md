---
slug: android-ux-wave-2
worktree:
branch:
phase: reviewed
codex_session:
created: 2026-10-05
strands: a,b
merge_order: a,b
---
# Android UX wave 2 — undo, widget, Android Auto, cover and status fixes

Second wave of the 2026-10-04 UX idea round (wave 1 = GNOME sleep timer and quick open,
`docs/plans/gnome-ux-wave-1.md`). Base: `dev` after #1086/#1087 (after-an-app-update).
Implementation by Claude `worker` agents, not Codex (Codex is rate-limited until 2026-10-09).

Kotlin root `K` = `android/app/src/main/java/io/github/marvinbaudach/reprise/`, tests under
`android/app/src/test/java/io/github/marvinbaudach/reprise/` (Robolectric/JUnit). Gate:
`scripts/check-android-suite.sh`; Rust gates from AGENTS.md for any `crates/**` change.

## Decisions (grill, 2026-10-04/05)

1. **Deferred delete replaces the confirm dialog.** Deleting tracks removes them from the
   list and the queue at once and shows a snackbar "N tracks will be deleted" (future tense:
   nothing is gone yet) with a separate Undo button for 6 s. Only
   when the snackbar times out (or is dismissed by a new delete) is the file actually deleted
   through the existing `trashTracks` path. If the app dies inside the window, nothing is
   deleted — the safe direction. Undo restores the rows, and puts the queue rows back at their old
   positions when the queue still has the size the delete left; otherwise, or when the delete
   skipped the playing track, they come back as the next rows to play (as FB-16 and FB-17 say).
2. Undo snackbar also for **Remove from queue** (re-insert at the old position when the
   queue still has the size the removal left, otherwise as the next row).
3. The deletion status line ("Deleting N tracks…") must not move the list — overlay, never
   reflow (decided 2026-10-04). It already lives in an overlay `Box` layer
   (`LibraryStatusChrome.kt`), so the 24 px shift comes from somewhere else: measure first.
4. **Android Auto: full browse tree** — Recently played, Playlists, Albums, Artists — via a
   media3 `MediaLibraryService`. The missing FFI reads (playlists + their tracks, recently
   played) are added to `reprise-android-ffi` over existing `reprise-core` queries.
5. **Home-screen widget with Glance**, two sizes: 4×1 (cover, title/artist, previous /
   play-pause / next) and 2×2 (cover with play/pause). Tap on cover opens the app.
6. #998: an open now-playing view picks up a cover that lands after it was shown.

## Strand a — library surface (`docs/plans/android-ux-wave-2-a.md`)

Owns: `K/TrackContextMenu.kt`, `K/DeletionMessages.kt`, `K/LibraryStatusSlot.kt`,
`K/LibraryStatusChrome.kt`, `K/BrowseScreen.kt`, `K/MobileSurfaceViewModel.kt`,
`K/TransientMessage.kt`, `K/LibraryTrackRows.kt`, `K/LibraryRemovalRefresh.kt`,
`K/TrackCover.kt`, `K/ArtworkCache.kt`, `K/ArtistCover.kt`, new `K/UndoSnackbar*.kt`,
`K/PendingDeletion*.kt`, `android/app/src/main/res/values*/strings.xml` (existing file only),
and tests for these.

Tasks:
- a1 **Snackbar host.** Add a `SnackbarHost` to the library overlay layer (the `BoxScope`
  chrome, not the list `Column`), bottom-aligned above the bottom frame, so it never reflows
  the list. One host, reused by a2/a3.
- a2 **Deferred delete.** Replace the confirm dialog in `TrackContextMenu.kt` (~395-436) with
  a `PendingDeletion` (ids, removed rows, removed queue positions, deadline). Hide the rows
  and drop them from the upcoming queue immediately; if a pending track is the current one,
  skip to the next as the current delete does. On timeout → call the existing
  `controls.deleteTracks(ids)` path. On Undo → restore rows and re-insert queue entries at
  their old positions (`move_upcoming_track` / `queue_tracks_*` FFI). A second delete inside
  the window commits the first one immediately. Leaving the app (onStop) commits pending
  deletes? **No** — it keeps the window; process death cancels (decision 1). Test: undo
  restores the exact list and queue; timeout calls deleteTracks once; process-death
  simulation deletes nothing.
- a3 **Undo for Remove from queue** (`TrackContextMenu.kt` ~173-176): snackbar "Removed from
  queue" with a separate Undo button; Undo re-inserts at the old position if the queue
  still has the size the removal left (`restoreQueued` compares sizes, nothing else),
  otherwise puts the row back as the next one to play. The row also comes back as next when
  the remembered size is null: `PendingDeletions.begin` clears it when a delete skips the
  playing track, because the queue's positions are relative to the playing track.
- a4 **Deletion line shift.** First reproduce with a Robolectric layout test that measures
  the list's top offset with and without a running deletion. Find what reflows (likely a
  padding/inset driven by `deletionProgress`), and make the line a pure overlay. The test
  asserts the offset is identical.
- a5 **#998.** Add a track-cover revision to the `rememberTrackArtworkVisual` key
  (`TrackCover.kt:385`), bumped when a cover download lands for that track, following the
  `artistPortraitRevision` pattern (`TrackCover.kt:71`, `ArtistCover.kt:31`). Add a
  track-cover invalidation to `ArtworkCache` so a cached placeholder is dropped. Test: a view
  showing the placeholder switches to the cover after the download event.
- Copy: new strings go into the existing `strings.xml` with German (`values-de`) and Spanish
  (`values-es`) translations if those resource directories exist.

## Strand b — playback service, Auto and widget (`docs/plans/android-ux-wave-2-b.md`)

Owns: `K/ReprisePlaybackService.kt`, `K/Media3PlaybackPort.kt`, new `K/library/**` (browse
tree) and `K/widget/**`, `android/app/src/main/AndroidManifest.xml`,
`android/app/build.gradle.kts`, `android/gradle/**` (version catalog, if used),
`android/app/src/main/res/xml/**` (new), `android/app/src/main/res/layout/**` (new, if
needed), a NEW `android/app/src/main/res/values/strings_media.xml` (+ `-de`/`-es`),
`crates/reprise-android-ffi/src/media_browse.rs` (new) + its `lib.rs` mod line, and tests.

Tasks:
- b1 **Media metadata.** `Media3PlaybackPort` sets `MediaMetadata` (title, artist, album,
  artwork URI or data, duration) on every `MediaItem`. Notification, lock screen, Auto and
  the widget all read it. Test: built items carry the metadata.
- b2 **FFI reads** (`media_browse.rs`, `#[uniffi::export]`, read-only, over existing
  `reprise-core` queries — no `reprise-core` changes): `list_playlists()`,
  `playlist_track_ids(id)`, `recently_played_track_ids(limit)`. Rust unit tests; Rust gates.
  If a needed core query does not exist, stop and report instead of adding one.
- b3 **MediaLibraryService.** Convert `ReprisePlaybackService` to `MediaLibraryService`
  (same media3 1.11.1 artifact). `onGetLibraryRoot`, `onGetChildren` for: Recently played,
  Playlists → tracks, Albums → tracks, Artists → albums → tracks. Playable leaves map to the
  existing play path with the container as queue context. Paging per media3 contract.
  Manifest: add the `MediaLibraryService` and `android.media.browse.MediaBrowserService`
  intent actions; add `res/xml/automotive_app_desc.xml` (`<uses name="media"/>`) and its
  `meta-data`. Keep the existing session behaviour unchanged for the app itself. Tests: tree
  shape with a fixture library; a leaf plays with its container as queue.
- b4 **Glance widget.** Add the Glance dependency (latest stable compatible with the
  project's Compose/Kotlin versions). `GlanceAppWidget` with responsive sizes 4×1 and 2×2,
  `GlanceAppWidgetReceiver`, `res/xml/*_widget_info.xml`, previews. State comes from the
  playback service (snapshot on change → `updateAll`), artwork as a downscaled `Bitmap` via
  FFI `track_artwork`. Buttons send media commands to the service (no app launch); cover tap
  opens `MainActivity`. Empty state when nothing played yet: app icon + "Reprise".
  Tests: state mapping and action intents (Robolectric).

## Parallelität

Two strands, disjoint by the ownership lists above. Shared-risk files and how they are split:
- strings: a uses the existing `strings.xml`; b only the new `strings_media.xml`.
- `MainActivity.kt` (767 lines): neither strand edits it. If b needs a launch intent extra,
  it uses the existing launch path.
- `ActivityPlaybackControls.kt` / `PlaybackControls.kt`: owned by neither; a needs only
  existing methods (`deleteTracks`, queue moves). If a needs a new control method, it is
  added in a new extension file in strand a.

Merge order: a, then b (b is larger and touches the manifest/gradle; rebasing it is the
cheaper side).

Post-merge cross-checks (read files no single strand owns): status 2026-10-05: step 1 passed on dev,
step 3 landed as #1118, step 2 (the phone pass) is still open.
1. `scripts/check-android-suite.sh` and the Rust gates on merged `dev`.
2. On the phone (device lock!): deferred delete + undo, queue undo, deletion line does not
   move the list, #998 cover appears, widget both sizes, notification shows title/cover,
   Android Auto via the Desktop Head Unit if available (otherwise record as manual).
3. UX rules in `docs/ux-rules.md` for deferred delete / undo (section G or N), the widget and
   Auto browse tree (section H), written by Opus in a docs PR after both land.
