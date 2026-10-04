---
slug: after-an-app-update-b
worktree: /home/marvin/Projects/reprise-after-an-app-update-b
branch: feature/after-an-app-update-b
phase: reviewed
codex_session:
created: 2026-10-04
---
# Strand b — Kotlin retries, the cover pass on restore and on network return, and the rules

Mother plan: `docs/plans/after-an-app-update.md`. Read its "Why" and decisions D3–D6 first.
Main sources are in `android/app/src/main/java/io/github/marvinbaudach/reprise/` and tests in
`android/app/src/test/java/io/github/marvinbaudach/reprise/`.

## File ownership

Touch only these files:
- `android/app/src/main/java/io/github/marvinbaudach/reprise/**`
- `android/app/src/test/java/io/github/marvinbaudach/reprise/**`
- `docs/ux-rules.md`
- this strand file

Do not touch `crates/**`. Strand a changes Rust behaviour, but no FFI signature, so build
against today's bindings.

## Tasks (test first: watch each new test fail before the fix)

1. **Shared classification.** Add a small pure function, in a new file, for example
   `TrackAnalysisRetry.kt`:
   - It answers whether an `AndroidAnalysisOutcome?` result, or a thrown error, is
     non-final: `CANCELLED`, `PHONE_SOURCE_CHANGED`, or a thrown exception.
   - It also defines `MAX_ANALYSIS_ATTEMPTS = 3`.
   - Pure JUnit test.
2. **NAV-15c, service.** Extend `ReprisePlaybackServiceAnalysisTest`, which uses Robolectric
   and `RecordingAnalysisService`, test first:
   - A non-final result for track 41 lets the next `onPlaybackChanged` with track 41 request
     again.
   - There are at most 3 requests for 41 in total.
   - A final result (`COMPUTED`, `DECODE_FAILED`, …) stops the requests.
   - No second request while one is in flight.
   - The counter resets when the current track changes.

   Implementation:
   - `trackAnalysisRequest` reports its result back on the main thread, through an
     overridable or internal settle callback.
   - `handleTrackAnalysis` keeps the per-track attempt and in-flight state instead of the
     bare `analysedTrackId` dedup.
   - Log non-`COMPUTED` outcomes at debug level with `TAG_ANALYSIS`.
3. **NAV-15c, loader.** In `TrackAnalysisLoaderTest` (plain JUnit; `Log` is unavailable,
   follow the existing comment in `prepare`), test first:
   - `importAnalysis` returning `CANCELLED` then `COMPUTED` leads to 2 calls and a final
     `revision` bump after the second.
   - A thrown error is retried the same way.
   - A final outcome is called once.
   - At most 3 calls in total.
   - The 2 s pause between attempts is injectable, so the test does not sleep.

   Implementation: `prepare` evaluates the outcome with the shared function.
4. **NET-7d, restore.** In the `LibrarySession` tests (see `BrowseSurfaceTest.kt:434` and
   the fake ports), test first:
   - `restore()` with a readable remembered tree calls the new constructor callback exactly
     once, after `port.configureTree`.
   - It is not called when there is no tree or the tree is unreadable.

   Implementation:
   - Add the callback to `LibrarySession`, as `afterRestoreConfigured: () -> Unit = {}`.
   - Wire it in `MainActivity` to `surfaceState::startArtistPhotoBackfill`.
   - Do NOT put the hook into `configureTree`, which also runs right before every scan.
4b. Keep the onCreate start (`MainActivity.kt:231`) and `afterScan` unchanged.
5. **NET-7d, network return.** In `MobileSurfaceViewModel`, add a method the network-return
   path calls once per real return, for example `networkReturnedRestartArtwork()`. It calls
   `startArtistPhotoBackfill()` unless the user stopped the artwork download in this
   process.
   - `cancelArtistPhotoBackfill()` sets that flag. A scan clears it: `afterScan` goes through
     a ViewModel method that clears the flag and starts, wired in `MainActivity`.
   - Wire it in `MainActivity` so that only the real return triggers it. The 3/10/30 s
     follow-ups that call `artwork::networkReturned` must not trigger it.
   - Read `NetworkReturn.kt` and `MobileSurfaceViewModel.kt:294-313` (including the replayed
     pending return on foreground) and pick the seam where exactly one call per return
     happens.
   - Tests, first, in `NetworkReturnArtworkTest`/`MobileSurfaceStateTest` or a new
     rule-named test:
     - one return gives one start, and the follow-ups give none;
     - after a cancel, a return does not start;
     - after a scan, a return starts again.
6. **Rules.** In `docs/ux-rules.md`, § NET and § NAV:
   - Add **NAV-15c** [active] [android] next to NAV-15/NAV-15b, and **NET-7d** [active]
     [android] after NET-7c, with the texts from the mother plan's D6.
   - In NET-7b, replace the sentence "There is no polling or restart of the background
     pass." with a pointer to NET-7d.
   - Follow the rulebook introduction for the traceability format, and make sure the new test
     names carry the rule IDs.
7. **Gates.**
   - `scripts/check-android-suite.sh` must pass: it builds the host `.so`, regenerates the
     bindings and runs `:app:testDebugUnitTest` and `:app:assembleDebug`.
   - The repo's rule-traceability checks for `docs/ux-rules.md`, if any script under
     `scripts/` checks rule-named tests.
   - Every edited code file stays under 800 lines.

## Not here

The Rust compute fallback (D1) and the inherited-cancel fix (D2) belong to strand a. No test
here may assume them; the Kotlin tests fake `importAnalysis` and the service request.
