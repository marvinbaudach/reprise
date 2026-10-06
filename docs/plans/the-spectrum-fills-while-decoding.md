---
slug: the-spectrum-fills-while-decoding
worktree: /home/marvin/Projects/reprise-the-spectrum-fills-while-decoding
branch: feature/the-spectrum-fills-while-decoding
phase: planned
codex_session:
created: 2026-10-05
---
# The spectrum fills while decoding (#1089)

## Problem

On Android, a track without a desktop sidecar shows nothing in the seek bar until the phone
has decoded the whole file. `decode_one` (`crates/reprise-android-ffi/src/track_analysis/compute.rs`)
feeds the full PCM stream into `RenderDataSession`, and only `finish()` → `set_track_render_data`
stores anything.

The 2026-10-05 emulator run is in the #1089 comment; evidence is in
`~/.local/share/reprise-device-run-20261005-1089/`. Release build, 4-minute MP3s:

| Case | Track start → `Computed` |
|---|---|
| Idle emulator | 45–49 s |
| Fourth track of a skip chain | 120 s |
| Host under load | 147–325 s |

- Once the analysis is stored, it shows up while the track is still playing. The `revision` refresh works.
- A track change does **not** cancel the outgoing track's foreground decode. The two decodes run in parallel and compete for the CPU.
- The Pixel decodes at about 10× real time, the idle emulator at about 5×. Decoding is ahead of the playhead after the first second, so a partial picture is useful almost at once.

## Goal

1. **Progressive rendering:** while the playing track is being analysed, the seek bar shows the part already decoded, filling from the left. The undecoded rest stays the plain line.
2. **Cancel on track change:** leaving a track stops its foreground decode. The abandoned track is not retried until it plays again; the backfill picks it up later.

Out of scope:
- A fast low-rate first pass. Revisit only if (1) is measured insufficient.
- #1129, the backfill restarting on playback-state flicker.
- #1128, the release-build crash.
- The desktop: `RenderDataSession::finish` and every desktop caller keep their exact behaviour.

## Facts the design rests on (origin/dev `c2c7d34f01`)

**`RenderDataSession`** (`crates/reprise-core/src/render_data_session.rs`):
- Per 1600-sample frame at 32 kHz (20 frames/s), it appends a `(sum_squares, count)` pair to the private `frames: Vec<(f64,u64)>`. `SpectrogramAccumulator` appends 24-byte frames to private `cells`.
  - Both vectors are append-only, and their entries are final once pushed.
  - Spectrogram cells are absolute dBFS bytes (`absolute_dbfs_to_byte`) with no global scale. A prefix of `cells` is therefore final.
- `finish(self)` is the only whole-buffer pass:
  - `rebucket_peaks(&frames, 1000)` divides by the final frame count N.
  - `finish_waveform_peaks` normalises to the track's loudest bucket: `round(sqrt(rms/max)*255)`.
- There is no snapshot accessor today.

**Stored form:**
- Tables: `tracks.waveform_peaks`, `track_spectrograms`, `track_loudness`.
- There is no partial or complete marker. A partial row would look finished:
  - to `render_data_already_valid`, which checks peaks and spectrogram only;
  - to `track_render_bars`;
  - and, with a loudness row, to sync, the sidecar, `mobile_import` and the backfill.
- **Therefore partial data never touches the database.**

**Foreground compute:**
- `import_track_analysis` → `compute(track_id, false, Some(&backfill), None)`. `current_slot` is `None`, so a foreground sink is registered nowhere and cannot be cancelled from outside.
- `AnalysisInFlight` dedups per track. A second caller (the service and the UI loader both import the playing track) waits on a condvar and gets the first caller's outcome.
- A waiter that receives `Cancelled` **retries**, up to `MAX_FOREGROUND_COMPUTE_ROUNDS = 3` (`compute.rs:270-274`). Cancelling A's sink with today's outcome would therefore make the other path re-decode A.

**Reads:**
- `track_render_bars(track_id, bar_count)` returns `None` unless both peaks and spectrogram are stored. Otherwise it returns exactly `bar_count` bars: `shape_display_peaks` plus a colour derived from `centroid_curve`, smoothed over 8 s using `duration_ms`.
- `track_spectrogram(track_id)` returns the whole stored blob.

**Kotlin:**
- `TrackAnalysisLoader` (417 lines):
  - The import and read lanes are each `limitedParallelism(1)`.
  - `prepare(B)` queues behind a running `prepare(A)` decode.
  - `invalidate()` clears only cached **null** entries, so a cached non-null partial would never be replaced.
  - `revision` is a single global counter, bumped after every import attempt.
- `SpectralSeekTrack.kt` (230 lines):
  - It loads on `(trackId, count, revision)` and draws `PlainSeekTrack` while there are no bars.
  - `SpectralBars` assumes the list spans the full width.
  - The build animation is keyed on `cueRevision`, not on the bars, so swapping bar lists does not re-fade.
- `NowPlayingScene.kt` (665 lines):
  - `rememberSpectrogram` → `remember(frames) { SceneState(frames) }`, so **every new `SpectrogramFrames` instance resets the scene**: shimmer, envelopes, fog.
  - The scene reads the spectrogram only as a fallback while no live PCM sink exists (`SceneDriver.fallbackBands`).
  - In the emulator run, the cover-tile scene went from flat to full at the same frame as the seek bar, so this fallback is user-visible.
- `ReprisePlaybackService.handleTrackAnalysis` sees a track change at `currentTrackId != analysisTrackId` (lines 405-410). The old id is still readable there. This is the one place that always observes playback, with or without UI.
- Files near the 800-line cap:
  - `NowPlayingSheet.kt` (794) and `MainActivity.kt` (776): do not grow them.
  - `ReprisePlaybackService.kt` (695): small additions only.
  - `MainActivityConfigurationTest.kt` (965): do not touch.

## Decisions

**D1 — Partial results live in memory, never in the database.**
- A decode publishes its sink in a new per-track registry (`track_analysis/decodes.rs`) for its whole lifetime. This covers foreground and backfill decodes alike: a foreground request for the playing track can be waiting on the backfill's decode of that track.
- The entry records:
  - the `Arc<AnalysisPcmSink>`;
  - the expected frame count, `ceil(duration_ms * 20 / 1000)` from the track row `decode_one` already reads. With `duration_ms <= 0` it is `None`, which means no progress for that track.
  - whether the decode is foreground or background.
- The entry is removed when the decode returns, whatever the outcome.
- The final store path is unchanged.

**D2 — A snapshot API on the session, additive only.**
- Add `RenderDataSession::partial(&self, expected_frames: usize) -> Option<PartialRenderData>`.
- `PartialRenderData { waveform_peaks: Vec<u8>, spectrogram: TrackSpectrogram, covered_fraction: f32 }`.
- Peaks use a **fixed** bucket mapping from the expected length, not the current N. Bucket `b` covers frames `[b*E/1000, (b+1)*E/1000)`, and only buckets whose range is fully decoded are emitted (`k` buckets, a prefix).
- Normalisation is `finish_waveform_peaks`'s rule applied to those `k` buckets. Share the helper; do not copy it.
- `spectrogram` is the decoded prefix of `cells`, whole 24-byte frames only.
- `covered_fraction = k / 1000`, clamped to `[0, 1]`. If the stream outruns `E` (the duration was short), the fraction stays at 1.0 and everything decoded is in the 1000 buckets.
- It returns `None` when `k == 0`.
- `finish()` is untouched. If `render_data_session.rs` would cross 800 lines, put the partial logic in a sibling `render_data_partial.rs`.

**D3 — A separate progress read.** The existing reads keep their meaning: stored, final.
- New export `MusicLibrary::track_analysis_progress(track_id: i64, bar_count: u32) -> Result<Option<AndroidTrackAnalysisProgress>, LibraryError>`.
- `AndroidTrackAnalysisProgress { covered_fraction: f32, bars: Vec<AndroidTrackRenderBar>, spectrogram: AndroidTrackSpectrogram }`.
- **Bars:**
  - `n = max(1, round(bar_count * covered_fraction))` bars from the partial peaks.
  - Use the same `shape_display_peaks` and colour path as `track_render_bars`, with the centroid smoothing duration set to `covered_fraction * duration_ms`.
  - Factor the shared part out of `track_render_bars`; do not duplicate it.
- It returns `None` when:
  - no decode is registered for the track;
  - the registered decode has no expected length;
  - stored render data exists (the caller should read the final data).
- Code lives in `track_analysis/progress.rs` (new). No `reprise-view` change is planned. Growing `reprise-view` trips the thinness floor.

**D4 — A superseded decode is final for its waiters.**
- New outcome `AndroidAnalysisOutcome::Superseded`.
- The sink records the reason it was cancelled: the existing `Cancelled`, or the new `Superseded`.
- `decode_one` returns `Superseded` for the second reason. `compute` treats `Claim::Done(Superseded)` as final: no retry round.
- Nothing is stored. The track stays pending for the backfill.

**D5 — The service cancels the outgoing foreground decode.**
- New export `MusicLibrary::supersede_foreground_track_analysis(keep_track_id: Option<i64>)`. It cancels every **foreground** registry entry whose track differs from `keep`, with reason `Superseded`.
- It never touches background entries. The backfill already yields through `preempt_current_unless`.
- It is non-blocking: it flips flags under the registry mutex and joins nothing.
- `ReprisePlaybackService.handleTrackAnalysis` calls it on the `currentTrackId != analysisTrackId` branch, with `keep = currentTrackId`, and **only when the new `currentTrackId` is non-null**. A stop or the end of the queue cancels nothing, so the running analysis finishes and is stored for the next play (G6). A pause never changes the track id, so it cancels nothing either. The call is posted to `analysisScope` so the main thread never enters the FFI. It is logged at `Log.d(TAG_ANALYSIS, …)`.
- `SUPERSEDED` is final in `trackAnalysisIsNonFinal`.

**D6 — The loader stops importing a track nobody plays any more.**
- `TrackAnalysisLoader` keeps the latest `prepare` id (`@Volatile`).
- An import attempt dequeued for a different id is skipped: no import, no revision bump.
- A retry pause that ends for a superseded id ends the loop.
- This removes the queued stale `prepare(A)` that would otherwise start a fresh decode of A after the service's cancel.

**D7 — Progress reaches the UI by polling, never by caching.**
- New port method `TrackAnalysisPort.loadProgress(trackId: Long, count: Int, deliver: (TrackAnalysisProgress?) -> Unit)`, with a default `deliver(null)` so the existing fakes keep compiling.
- The loader runs it on the read lane and **never caches** the result.
- `TrackAnalysisProgress(coveredFraction: Float, bars: List<SpectralBar>, frames: SpectrogramFrames)`.
- Consumers poll every `ANALYSIS_PROGRESS_POLL_MS = 1_000` while they have no final data. The final data arrives through the existing `revision` → `loadBars`/`loadSpectrogram` path and replaces the partial.

**D8 — The seek bar draws the decoded part.**
- While `bars` (final) is null, `SpectralSeekTrack` polls `loadProgress` from a `LaunchedEffect(analysis, trackId, count, revision)` loop with `delay(ANALYSIS_PROGRESS_POLL_MS)`. The loop ends when the composable leaves or the final bars arrive.
- `SpectralBars` gets a `coveredFraction` parameter (default `1f`):
  - bars are laid out over `coveredFraction * width`;
  - the stride stays `coveredWidth / bars.size`;
  - the remainder draws as the `PlainSeekTrack` line.
- No build animation for partials. They snap, as a same-track bar swap already does today.
- Keep the code out of `NowPlayingSheet.kt`. A new sibling `SpectralSeekProgress.kt` is fine.

**D9 — The scene adopts growing frames without resetting.**
- `SceneState` becomes keyed by `trackId`, not by the frames instance. A new `SceneState.adoptFrames(frames)` swaps in a longer `SpectrogramFrames` for the same track and keeps envelopes, shimmer and fog.
- `SceneDriver` reads frames from the state instead of a constructor `val`.
- `rememberSpectrogram` polls `loadProgress` like D8 while the final spectrogram is null.
- Positions past the last decoded frame already clamp to the last frame (`SpectrogramFrames.frameIndexFor`).

**D10 — New UX rules** in `docs/ux-rules.md`, next to NAV-15c, `[android]`, each marked `<!-- REVIEW: rule proposal -->`. Rule-named tests use the `nav_15d_` and `nav_15e_` prefixes.
- **NAV-15d:** While the phone computes the playing track's analysis, the seek bar shows the part already decoded, filling from the left, and the rest stays the plain line. The partial picture is held only in memory: it is never stored and never counts as analysed for sync, sidecar or backfill. The Now Playing scene adopts the growing spectrum without restarting.
- **NAV-15e:** Switching to another track stops the outgoing track's foreground analysis (a stop or a pause does not), and the abandoned track is not retried until it plays again. The backfill picks it up later. A queued analysis for a track that is no longer playing is skipped.

## Tasks (test first: write the failing test, see it fail, implement, see it pass)

Each task's file list is a starting point, not a fence. Stop only if the *contract* turns out to be wrong.

**T1 — `RenderDataSession::partial`** (`crates/reprise-core/src/render_data_session.rs`, or the sibling `render_data_partial.rs`; tests in a sibling `render_data_session_partial_tests.rs`).

Tests:
- `partial_fills_buckets_from_the_left`: half of an expected 10 s stream gives about 500 buckets and `covered_fraction ≈ 0.5`.
- `partial_spectrogram_is_a_prefix_of_the_final`: the same input, `finish()` afterwards, and the partial cells equal the first cells of the final.
- `partial_is_none_before_one_bucket_is_complete`.
- `partial_clamps_when_the_stream_outruns_the_expected_length`.
- `partial_peaks_use_the_finish_normalisation`: the loudest emitted bucket is 255.
- The existing tests stay green unchanged, including `session_peaks_match_the_desktop_accumulator`.

**T2 — Decode registry and progress read** (`crates/reprise-android-ffi/src/track_analysis/{decodes.rs (new), progress.rs (new), compute.rs, mod.rs}`, `track_analysis.rs` for the shared bar shaping, `lib.rs`/`library_types.rs` for wiring).
- `decode_one` registers and deregisters every decode.
- `AnalysisPcmSink` gets a snapshot method under its existing session mutex.

Tests (extend the `compute_tests.rs` seams: `ClosureDecoder`, `wait_flag`/`set_flag`):
- `nav_15d_a_running_decode_reports_progress_for_its_track`: the decoder pushes half and blocks on a flag. Progress is `Some`, the fraction is about 0.5, and `bars.len()` is about half of `bar_count`.
- `nav_15d_progress_is_none_without_a_decode_and_after_the_store`.
- `nav_15d_a_cancelled_decode_leaves_no_render_data`: cancelled mid-stream, the track is still in `pending_render_data_tracks`, with no peaks and no spectrogram row.
- `nav_15d_the_backfill_decode_reports_progress_too`.

**T3 — `Superseded` and the cancel export** (`compute.rs`, `decodes.rs`, `mobile_sync.rs` or a new sibling for the export).

Tests:
- `nav_15e_superseding_keeps_the_playing_track`: decodes for A and B are running; `supersede(Some(B))` cancels A only.
- `nav_15e_a_superseded_decode_is_final_for_its_waiter`: two callers on A, the first decoding. After the supersede, both get `Superseded` and the decoder ran exactly once.
- `nav_15e_superseding_never_cancels_the_backfill`.
- `supersede_with_no_decodes_is_a_no_op`.

**T4 — Kotlin plumbing** (`TrackAnalysisRetry.kt`, `TrackAnalysisLoader.kt`, `MainActivity.kt` (the loader construction line only), `ReprisePlaybackService.kt`; tests in `TrackAnalysisLoaderTest.kt`, `TrackAnalysisRetryTest.kt` and `ReprisePlaybackServiceAnalysisTest.kt`).
- `SUPERSEDED` is final.
- Add `loadProgress` (uncached, read lane) and the latest-id skip (D6).
- The service supersede call (D5).

Tests:
- `nav_15e_superseded_is_final`.
- `nav_15e_a_queued_prepare_for_a_track_no_longer_playing_is_skipped`.
- `nav_15d_progress_reads_are_never_cached`: two calls give two reads.
- `nav_15e_a_track_change_supersedes_the_outgoing_analysis`: `RecordingAnalysisService` records the call with `keep = B`.
- `nav_15e_stopping_playback_supersedes_nothing`: a snapshot with no current track records no call.

**T5 — Seek bar** (`SpectralSeekTrack.kt`, a new `SpectralSeekProgress.kt`; test in `SpectralSeekTrackPixelsTest.kt` with the `PixelAnalysis` fake extended by `loadProgress`).
- `nav_15d_partial_bars_cover_only_the_decoded_part`: `coveredFraction = 0.5`. Bar pixels appear in the left half; the right half shows only the plain line.
- `nav_15d_final_bars_replace_the_partial`.

**T6 — Scene** (`NowPlayingScene.kt`, `scene/SceneState.kt`, `SceneDriver.kt`; tests in `NowPlayingSceneEngineTest.kt` and `SceneDriverTest.kt`).
- `nav_15d_growing_frames_do_not_reset_the_scene`: the `SceneState` identity and its envelopes survive `adoptFrames` with more frames.
- `nav_15d_the_driver_reads_adopted_frames`.

**T7 — Rules** (`docs/ux-rules.md`): NAV-15d and NAV-15e as in D10, written in the same commit as the code that makes them true (process rule), or as the last commit with all named tests present.

## Verification (Codex, in the worktree)

- Rust:
  - `cargo fmt --check`.
  - `cargo clippy -p reprise-core -p reprise-android-ffi --all-targets -- -D warnings`.
  - `cargo test -p reprise-core render_data_session`.
  - `cargo test -p reprise-android-ffi track_analysis`.
  - `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` (must be empty).
- Android: `scripts/check-android-suite.sh` with the worktree-local env prefix:
  ```
  ANDROID_HOME=/home/marvin/.local/share/android-sdk ANDROID_SDK_ROOT=/home/marvin/.local/share/android-sdk \
  JAVA_HOME=/usr/lib/jvm/java-21-openjdk ANDROID_USER_HOME="$PWD/.cache/android-user-home" \
  XDG_DATA_HOME="$PWD/.cache/xdg-data" GRADLE_USER_HOME="$PWD/.gradle-user-home" scripts/check-android-suite.sh
  ```
  If new tests make the suite's floor check complain, raise the floor to the measured count.
- `scripts/check-ux-traceability.sh`.
- **Do NOT run** the unfiltered `cargo test --workspace`, `scripts/check-merge-readiness.sh`, or the GNOME display suites. If AGENTS.md says to run the full gate before committing, that does not apply to this run; the orchestrator runs it after the code phase. Commit when the filtered checks above pass. A `settle()`-family failure in `device_sync_runtime_tests` is load noise, not this change.

## Parallelität

**No cut: one strand.** The reasons are below.

- **One shared new core.** Both goals rest on the same new decode registry (`track_analysis/decodes.rs`) and on `decode_one`/`compute` in `compute.rs`:
  - progress reads it (T2);
  - supersede cancels through it and changes `compute`'s retry rule (T3).
  
  Splitting T2 and T3 would give two strands writing `compute.rs` and the registry.
- **The UniFFI boundary couples Rust and Kotlin.** `Superseded` extends `AndroidAnalysisOutcome`, and `trackAnalysisIsNonFinal` must handle it. The new progress record and the supersede export exist for Kotlin only. A Rust-only strand would leave the generated bindings ahead of their Kotlin consumers. That is the cross-strand compile dependency a disjointness check cannot see, measured on `equalizer-profiles-lead-the-surface`.
- **`TrackAnalysisLoader.kt` carries both D6 (cancel) and D7 (progress).**

The only disjoint group is T6 (the scene files), but it consumes T4's `loadProgress` port method. Cutting it off would need a merge order and buy a few minutes on a single Codex run.

- **Merge order:** none.
- **Sibling work excluded:**
  - `#1129` (backfill restart; `ReprisePlaybackService` backfill start/cancel, `backfill.rs` `run_worker`). This plan must not change the backfill's start/cancel triggers.
  - `fix/release-workmanager-keep` (#1128, `android/app/proguard-rules.pro`).

**Post-merge cross-checks:**
1. The full gate on `dev`:
   - `cargo test --workspace`;
   - `cargo clippy --all-targets --workspace -- -D warnings`;
   - `scripts/check-android-suite.sh`;
   - the Android lint stage.
2. Emulator rerun of the 2026-10-05 arms. The release APK needs #1128 landed or the keep rule applied.
   - Arm (a): first visible partial bar ≤ 3 s after the track starts on an idle host. Final bars replace it at `Computed`.
   - Arm (b): after the skip, no `Computed analysis for track A` line, and a `Superseded` settle for A.
   - Arm (c), the skip chain: the last track's first partial bar ≤ 3 s.
   - Control: an analysed track is unchanged, warm at once.
3. Device sync status and `complete_render_data_track_ids` are unchanged for a track whose decode was superseded. It stays pending.

## Grill decisions (2026-10-05)

- **G1 — Where partial results live:** in memory only (D1). The database sees only finished analyses. A process restart loses the partial state, and the decode starts over.
- **G2 — Bar heights while decoding:** they grow along. Partial peaks use `finish`'s normalisation over the decoded prefix, and the display shaping over the prefix's own percentile window. Earlier bars may shrink a little when a louder passage decodes. At `covered_fraction = 1` the partial computation equals the final one, up to the expected-versus-actual frame count, so the swap to the final data does not jump.
- **G3 — Cadence:** consumers poll `loadProgress` every 1 s (`ANALYSIS_PROGRESS_POLL_MS = 1_000`), only while visible and without final data. There is no push callback from Rust.
- **G4 — The scene (T6) is in scope.** `SceneState` is keyed by track and adopts growing frames without a reset. That also removes today's one reset when the final spectrogram arrives.
- **G5 — Cancel design:** only the service supersedes, and `Superseded` is a final outcome for every waiter (D4, D5). The loader skips an import for an id that is no longer the latest `prepare` (D6). Cancelling from both paths was rejected because of the race window; reusing `Cancelled` was rejected because the waiter would re-decode.
- **G6 — Stop and pause:** only a switch to another track supersedes. A stop or the end of the queue lets the analysis finish and be stored, and a pause never cancels.
- **G7 — Rules:** NAV-15d and NAV-15e are new `[android]` rules beside NAV-15c (D10), marked `<!-- REVIEW: rule proposal -->`. Both IDs were verified free on origin/dev; only NAV-15, NAV-15b and NAV-15c exist. `[android]` is an established test level (NAV-15c, PLAY-20c, FB-16).
- **G8 — The cut:** one strand, as argued in "Parallelität".
- **G9 — Post-merge device run:** the emulator rerun needs #1128 (the R8 keep rule for WorkManager's database) on `dev` first. Otherwise the release APK does not start.
