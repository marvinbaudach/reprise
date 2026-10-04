---
slug: after-an-app-update
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-04
strands: a,b
merge_order: a,b
---
# After an app update, the phone shows its visualizer and fetches covers by itself

## Why

The user reported this on 2026-10-04. After an app update the Android app shows no seek-bar
spectrum or visualizer for tracks that were never synced from the desktop. An update means no
folder pick and no scan. Album covers are not downloaded automatically either.

The root cause comes from a code trace on `origin/dev` 78005329e3:

1. **Start ordering.** On a cold start, `LibrarySession.restore()` registers the SAF tree on
   `Dispatchers.IO` (`port.configureTree` → `MusicLibrary::set_tree_uri`). Three things run
   before it:
   - the artwork backfill, started synchronously in `MainActivity.onCreate:231`;
   - the restored track's `TrackAnalysisLoader.prepare`;
   - the service's `trackAnalysisRequest`.
2. **Analysis aborts without a tree.**
   - `import_track_analysis` → `import_via_sidecar` calls `configured_tree()?`
     (`mobile_sync.rs:37`). That returns `Err(TreeNotConfigured)`.
   - So the compute fallback is never reached, although compute needs no tree:
     `decode_one` opens `track.path` through the registered decoder.
   - A service-only start (media resumption without the activity) never registers a tree, so
     every track fails.
3. **No retry.**
   - The service sets `analysedTrackId` before the request and swallows the error
     (`ReprisePlaybackService.kt:257-286`).
   - The loader swallows the error, bumps `revision` and reads null
     (`TrackAnalysisLoader.kt:123-150`).
   - Neither asks again for the same track.
4. **`Cancelled` is inherited.**
   - A foreground request can join the backfill's in-flight decode of the same track.
   - If pause or power-save then cancels the backfill, that request inherits `Cancelled`
     (`compute.rs:183-193`, `256-257`).
5. **The cover pass never chains.**
   - `start_artist_portrait_backfill_with` captures `configured_tree()` at start
     (`artist_portrait.rs:311`). The onCreate start has no tree, so the album-cover pass never
     chains.
   - `restore()` never calls `afterScan`, so nothing restarts the pass until a scan.
6. **#1051.** After an offline start, the background pass is not restarted on network return.
   Only visible surfaces ask again (NET-7b).

The core already supports a second start: `PortraitBackfill::start` "only replaces the
listener" during an active run. So a second `startArtistPhotoBackfill()` after the tree lands
attaches a listener that captured the tree, and the chain fires when the running portrait pass
completes. When no run is active, it starts a fresh one.

## Decisions (settled in the grill, 2026-10-04)

- **D1 — Compute needs no tree.**
  - In `import_via_sidecar`, `LibraryError::TreeNotConfigured` maps to
    `AnalysisImportOutcome::Missing`. Every other error still propagates.
  - Accepted cost: in the start race, a synced track with a sidecar is computed rather than
    imported. The result is equivalent and takes 1.5–5 s on the Pixel.
- **D2 — A foreground caller never inherits `Cancelled`.**
  - In `AnalysisContext::compute`, a caller with `background == false` that gets
    `Claim::Done(Cancelled)` joins or claims again, for at most 3 rounds in total. After that
    it returns `Cancelled`.
  - Background callers are unchanged.
- **D3 — The service retries non-final outcomes.**
  - Non-final: a thrown exception, `CANCELLED`, `PHONE_SOURCE_CHANGED`.
  - Final: everything else.
  - The next `onPlaybackChanged` for the same current track requests again, with at most
    3 attempts per track and never two in flight.
  - The counter resets when the current track changes. The state is touched on the main
    thread only.
- **D3b — The loader follows the same rule.**
  - `TrackAnalysisLoader.prepare` evaluates the outcome. On a non-final one it retries after a
    short pause (2 s), for at most 3 attempts in total. Each attempt is followed by the
    existing invalidate and `revision` bump.
  - The final/non-final split is one shared Kotlin function, used by both the service and the
    loader.
- **D4 — The cover pass starts once the tree is registered on restore.**
  - `LibrarySession.restore()` calls a new constructor callback (for example
    `afterRestoreConfigured: () -> Unit = {}`) right after `port.configureTree(treeUri)`.
  - `MainActivity` wires it to `surfaceState::startArtistPhotoBackfill`.
  - The hook does NOT go into `configureTree` itself: that also runs right before every scan,
    where a pass would read the pre-scan album list. Scans keep `afterScan`.
  - The onCreate start stays.
- **D5 — A network return restarts the background pass (#1051).**
  - Once per real return (`NetworkReturnDetector`'s offline→online), never on the
    3/10/30 s follow-ups. The progress card shows as on an app start.
  - It does not restart after the user pressed "Stop artwork download" in this process. That
    is a `MobileSurfaceViewModel` flag, set by `cancelArtistPhotoBackfill`. A scan
    (`afterScan`) clears it, and so does a new process.
- **D6 — UX rules** in `docs/ux-rules.md`, with rule-named tests:
  - **NAV-15c** [active] [android]: the playing track gets its spectrum without a desktop
    sync.
    - This holds whether or not the library folder is registered yet, and whether or not a
      desktop sync ever ran.
    - Without a sidecar the phone computes.
    - A non-final failure (error, cancellation, file changed during the computation) is
      retried at most three times per track and never twice at once.
    - A cancelled background analysis never leaves the presented track without a result.
  - **NET-7d** [active] [android]: the cover pass runs on every app start, including after an
    update without a scan, once the library folder is registered.
    - It restarts once on every real network return after an offline period, but not on a
      switch between two online networks.
    - It does not restart if the user stopped the download in this process; the next scan or
      app start runs it again.
    - The progress card shows as on a start.
  - **NET-7b:** the sentence "There is no polling or restart of the background pass." is
    replaced by a pointer to NET-7d. The rest of NET-7b stays.

Out of scope, on purpose:
- #1041: a stop during the portrait phase skips the next cover pass.
- #998: an open now-playing view keeps its placeholder.
- #1059: cover lookup misses albums whose title differs only by a dash variant.

## Parallelität

- **Strand a — Rust analysis (D1, D2).** File: `docs/plans/after-an-app-update-a.md`.
  - Owns `crates/reprise-android-ffi/src/mobile_sync.rs`,
    `crates/reprise-android-ffi/src/track_analysis/compute.rs`,
    `crates/reprise-android-ffi/src/track_analysis/compute_tests.rs`, and new
    `crates/reprise-android-ffi/src/track_analysis/*_tests.rs` files.
- **Strand b — Kotlin and rules (D3–D6).** File: `docs/plans/after-an-app-update-b.md`.
  - Owns `android/app/src/main/java/io/github/marvinbaudach/reprise/**`,
    `android/app/src/test/java/io/github/marvinbaudach/reprise/**`, and `docs/ux-rules.md`.
- **Disjoint.** No FFI signature changes, so b builds against today's bindings.
- **Merge order:** a, then b. That way the rule text b adds is already true on dev.
- **Post-merge cross-checks** (no single strand can make them):
  1. On dev after both land: `cargo test --workspace` and `scripts/check-android-suite.sh`.
  2. **Device run, the user's scenario: an update, no scan.** Taken before landing, on a
     scratch merge of a+b under `~/.cache/reprise-scratch/`, with an arm64 release APK. The
     Pixel is used under `device-lock --wait`, and the probe folder is
     `Reprise-Probe/` inside the granted tree.
     1. With the old app, play an unsynced probe track.
     2. `adb install -r` the new APK. No scan. Relaunch.
     3. The restored probe track shows its spectrum within about 10 s.
     4. A probe album without art gets its cover in the list and in now-playing.
     5. Start offline, then let the network return: the cover arrives without a relaunch.
     6. Service-only start: logs `Computed` for the next unsynced track.
     7. Clean up: delete the probe folder, rescan, and record any queue side effect.
