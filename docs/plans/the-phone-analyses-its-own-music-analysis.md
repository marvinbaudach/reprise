---
slug: the-phone-analyses-its-own-music-analysis
worktree: /home/marvin/Projects/reprise-the-phone-analyses-its-own-music-analysis
branch: feature/the-phone-analyses-its-own-music-analysis
phase: shipped
codex_session:
created: 2026-09-18
---
# Strand `analysis` — the phone computes waveform and spectrogram

Strand of `docs/plans/the-phone-analyses-its-own-music.md` (the mother plan:
context, decisions 1–7 and 10–13, rules, gates, device run). Read it first;
this file holds only this strand's tasks and ownership. Base `origin/dev` @
`3403a6e2cb`.

## Ownership

Owns exactly:

- `crates/reprise-core/src/lib.rs` (two `pub mod` lines), `crates/reprise-core/src/render_data_session.rs` (new), `crates/reprise-core/src/pcm_resample.rs` (new)
- `crates/reprise-android-ffi/src/lib.rs`, `crates/reprise-android-ffi/src/library_types.rs`, `crates/reprise-android-ffi/src/track_analysis.rs`, `crates/reprise-android-ffi/src/track_analysis/**` (new: `compute.rs`, `backfill.rs`, `compute_tests.rs`, `backfill_tests.rs`), `crates/reprise-android-ffi/src/mobile_sync.rs`
- `android/app/src/main/java/io/github/marvinbaudach/reprise/{MediaCodecTrackDecoder.kt (new), SharedMusicLibrary.kt, ReprisePlaybackService.kt, TrackAnalysisLoader.kt, TrackAnalysisBackfillPolicy.kt (new)}`
- `android/app/src/test/java/io/github/marvinbaudach/reprise/{TrackAnalysisLoaderTest.kt, TrackAnalysisBackfillPolicyTest.kt (new), ReprisePlaybackService*Test.kt}`
- this file

Does **not** touch `MainActivity.kt`, `MobileSurfaceViewModel.kt`,
`NowPlayingScene.kt`, `NowPlayingPanelsTest.kt`, `waveform.rs`,
`spectrogram.rs`, `playback_session.rs`, `artist_portrait*.rs`,
`online_sources.rs`, any `Cargo.toml`/`Cargo.lock`. The list is the starting
point, not a fence: if state must live in a file outside it, stop and report
rather than edit it.

## Rust facts this strand builds on

- `SpectrogramAccumulator::new()` / `push(&[f32])` / `finish() -> TrackSpectrogram`
  (`crates/reprise-core/src/spectrogram.rs:228-305`), expects 32 kHz mono
  (`SPECTROGRAM_SAMPLE_RATE_HZ`, `:12`), 1600 samples per frame
  (`SPECTROGRAM_FRAME_RATE_HZ = 20`, `:14`).
- `WaveformAccumulator::new(expected_samples, buckets)` (`waveform.rs:106`),
  `STORED_PEAK_COUNT = 1000` (`:8`), `TrackRenderData { waveform_peaks, spectrogram }`
  (`:45-48`). Peaks are `0..=255` sqrt-normalised RMS per bucket
  (`finish_waveform`, `:160-`); the session reproduces that normalisation
  from per-frame RMS.
- Live PCM downmix to mono already exists in
  `crates/reprise-android-ffi/src/visualizer/live_audio.rs` (`buffer_pcm_i16`);
  reuse or mirror it — do not add a third downmix.
- `set_track_render_data(&Db, track_id, TrackSourceFingerprint, &TrackRenderData) -> SpectrogramStoreOutcome`
  (`db_spectrogram.rs:74`), `track_source_fingerprint(&Db, track_id)` (`:261`),
  `pending_render_data_tracks(&Db) -> Vec<PendingRenderDataTrack>` (`:307`),
  `complete_render_data_track_ids` (`:153`).
- `import_track_analysis` (`crates/reprise-android-ffi/src/mobile_sync.rs:5-37`)
  calls `import_analysis_for_track(source, db, id)` (`mobile_import.rs:63-75`)
  and maps `AnalysisImportOutcome::{Imported, AlreadyImported, Missing, Invalid, PhoneSourceChanged}`
  (`mobile_import.rs:13-19`) to the UniFFI enum Kotlin sees.
- `MusicLibrary { writer, reader, tree, cache_root, database_path, portrait_fetch, portrait_backfill }`
  (`library_types.rs:25-35`), `writer()`/`reader()` (`:39,45`),
  `try_lock_writer` (`writer_backoff.rs`), construction in
  `open_with_portrait_fetcher` (`lib.rs:91-115`).
- UniFFI callback interfaces already in the crate: `artist_portrait.rs:173`,
  `source.rs:65`, `library_types.rs:125`, `playback.rs:106` — follow their
  form; a UniFFI object (`#[derive(uniffi::Object)]`) can be passed to a
  callback method as `Arc<T>`.

## A1 — `RenderDataSession` in core (pure, host-tested)

`crates/reprise-core/src/render_data_session.rs`:

```rust
pub struct RenderDataSession { /* LinearResampler, SpectrogramAccumulator, per-frame sum-of-squares/count, pending mono samples */ }
impl RenderDataSession {
    pub fn new() -> Self;
    /// Interleaved 16-bit PCM at the caller's rate and channel count.
    /// Downmix → resample to SPECTROGRAM_SAMPLE_RATE_HZ → accumulators.
    /// A rate or channel change mid-stream is an error.
    pub fn push_pcm_i16(&mut self, samples: &[i16], sample_rate_hz: u32, channel_count: u32) -> Result<(), RenderDataSessionError>;
    /// Ends the stream: spectrogram from the accumulator; peaks re-bucketed
    /// from the per-frame RMS into STORED_PEAK_COUNT buckets (decision 4),
    /// normalised exactly like `finish_waveform`. Empty stream is an error.
    pub fn finish(self) -> Result<TrackRenderData, RenderDataSessionError>;
}
```

`crates/reprise-core/src/pcm_resample.rs`: `LinearResampler::new(from_hz, to_hz)`,
`push(&mut self, mono: &[f32], out: &mut Vec<f32>)`, fractional phase kept
across chunks; identity when `from_hz == to_hz`. Both files get `pub mod`
lines in `crates/reprise-core/src/lib.rs`; nothing else in core changes. The
per-frame RMS is one `f64` sum and one count per 1600-sample frame — no
sample buffer.

Tests (in the two files; `cargo test -p reprise-core render_data_session pcm_resample`):
- `resampler_keeps_phase_across_chunk_boundaries` — a 1 kHz sine at 48 kHz
  pushed in ragged chunks equals the same sine pushed in one chunk.
- `resampler_output_length_matches_the_ratio` — 48 000 in → 32 000 out ±1.
- `resampler_is_identity_at_equal_rates`.
- `session_frame_count_follows_the_duration` — 10 s of 44.1 kHz stereo →
  200 frames (`TrackSpectrogram::frame_count`) and 1000 peaks.
- `session_puts_a_sine_in_the_right_band` — a 440 Hz tone lights the band
  containing 440 Hz and nothing two bands up.
- `session_peaks_match_the_desktop_accumulator` — a stepped-envelope signal
  through `WaveformAccumulator::new(exact_count, 1000)` and through the
  session: every peak within ±2.
- `session_rejects_a_rate_change_mid_stream`.
- `session_rejects_an_empty_stream`.

## A2 — the FFI: sink, decoder callback, compute-on-missing, dedup

`crates/reprise-android-ffi/src/track_analysis/compute.rs`, declared as
`mod compute;` **inside `track_analysis.rs`** (plus `#[cfg(test)] #[path] mod compute_tests;`
the way `artist_portrait.rs:218-220` does it). Own `#[uniffi::export]` blocks:

```rust
#[uniffi::export(callback_interface)]
pub trait TrackPcmDecoder: Send + Sync {
    /// Decode `track_uri` from the start and push 16-bit interleaved PCM into
    /// `sink` until end of stream, until the sink refuses a chunk, or until
    /// the decoder fails. `background` asks the decoder to lower the calling
    /// thread's priority for the duration of the call.
    fn decode(&self, track_uri: String, sink: Arc<AnalysisPcmSink>, background: bool) -> Result<(), AnalysisDecodeError>;
}

#[derive(uniffi::Object)]
pub struct AnalysisPcmSink { session: Mutex<Option<RenderDataSession>>, cancelled: AtomicBool, refused: Mutex<Option<String>> }
#[uniffi::export]
impl AnalysisPcmSink {
    /// `false` = stop decoding: cancelled, or the session refused the chunk.
    pub fn push_pcm_i16(&self, bytes: Vec<u8>, sample_rate_hz: u32, channel_count: u32) -> bool;
}
```

`MusicLibrary` (`library_types.rs`) gains
`pcm_decoder: Mutex<Option<Box<dyn TrackPcmDecoder>>>`,
`analysis_in_flight: Mutex<HashSet<i64>>` (plus its condvar or equivalent so
a second caller for the same id waits for the first result),
`analysis_failed: Mutex<HashSet<i64>>` and `analysis_backfill: TrackAnalysisBackfill`
(A3), initialised in `open_with_portrait_fetcher` (`lib.rs:91-115`). New
exports on `MusicLibrary` in `compute.rs`: `register_track_pcm_decoder(decoder: Box<dyn TrackPcmDecoder>)`.

`import_track_analysis` (`mobile_sync.rs`) becomes import-or-compute:

1. the existing sidecar import; `Imported`, `AlreadyImported`,
   `PhoneSourceChanged` return as today;
2. on `Missing`/`Invalid`: if the id is in flight, wait for that result and
   return it; else mark in flight, cancel the backfill's current item if it
   is another id (A3), read the track URI and `track_source_fingerprint`
   under `reader` and **release the guard**, call the decoder with a fresh
   sink (`background = false`), `finish()`, then `writer()` →
   `set_track_render_data` → release; clear in flight;
3. the Kotlin-facing outcome enum grows `Computed`, `DecodeFailed`,
   `NoDecoder`, `Cancelled`; grep the enum's name across `android/` before
   touching it — every `when` over it is in files this strand owns
   (`TrackAnalysisLoader.kt`), otherwise stop and report.

Tests (`track_analysis/compute_tests.rs`; fake decoder implemented in Rust,
real temp DB with a scanned track; `cargo test -p reprise-android-ffi track_analysis`):
- `a_missing_sidecar_is_computed_and_stored` — outcome `Computed`,
  `track_render_bars` is `Some`, `pending_render_data_tracks` no longer lists
  the track.
- `a_present_sidecar_wins_and_the_decoder_is_never_called`.
- `a_computed_analysis_makes_a_later_sidecar_already_imported`.
- `the_writer_is_free_while_the_decoder_runs` — the fake decoder calls
  `writer.try_lock()` inside `decode` and asserts `Ok`.
- `a_decoder_failure_is_reported_and_stores_nothing`.
- `a_file_replaced_during_the_decode_is_not_stored` — the fake decoder
  changes the recorded fingerprint during `decode`; outcome
  `PhoneSourceChanged`, no row.
- `no_registered_decoder_reports_no_decoder`.
- `two_callers_for_one_track_decode_once` — two threads, one decode call
  counted, both get `Computed`.

## A3 — the backfill worker

`crates/reprise-android-ffi/src/track_analysis/backfill.rs`:
`TrackAnalysisBackfill` (a handle like `PortraitBackfill`: `start`, `cancel`,
`progress`), exports on `MusicLibrary`:
`start_track_analysis_backfill(listener: Box<dyn TrackAnalysisProgressListener>)`,
`cancel_track_analysis_backfill()`, `track_analysis_backfill_progress()`.
The worker thread: `pending_render_data_tracks` under `reader` (release,
then iterate ids in stable order, skipping `analysis_failed` and ids in
flight), per track the A2 compute path with `background = true`; a decode
**failure** (the decoder returned `Err`) adds the id to `analysis_failed`.
**Cancellation is not a failure:** when the sink's `cancelled` flag stopped
the decoder, the outcome is `Cancelled`, nothing is stored, the id is *not*
added to `analysis_failed`, and the track stays pending — the compute path
must distinguish "the decoder gave up" from "we told it to stop" before it
touches the failed set. Preemption: A2's foreground path flips the
`cancelled` flag of the worker's current sink when that item is another id;
the worker re-reads pending after each item, so a track completed in the
foreground is simply gone from the list and a preempted one comes back. One worker at a time; `start` while running is a
no-op; `cancel` joins. Progress = `{ done, total, failed }`.

Tests (`backfill_tests.rs`):
- `the_backfill_drains_pending_tracks_in_id_order`.
- `cancel_stops_at_the_next_chunk_and_leaves_the_track_pending`.
- `a_failed_track_is_skipped_for_the_rest_of_the_process`.
- `a_foreground_request_preempts_the_worker` — the worker is mid-decode on
  id 2; a foreground `import_track_analysis(5)` finishes first; id 2 is
  decoded again afterwards.
- `start_while_running_is_a_no_op`.

## A4 — Kotlin: the decoder and where it is registered

`MediaCodecTrackDecoder.kt` (new): implements `TrackPcmDecoder`; opens the
SAF URI with `contentResolver.openFileDescriptor(uri, "r")`, `MediaExtractor`
→ first `audio/*` track → `MediaCodec.createDecoderByType` in synchronous
mode; the output format's `KEY_SAMPLE_RATE`/`KEY_CHANNEL_COUNT` are passed
with every chunk (16-bit PCM is the default output encoding — never request
float); stops when `pushPcmI16` returns `false`; releases codec, extractor
and descriptor in `finally`. With `background = true` it calls
`Process.setThreadPriority(THREAD_PRIORITY_BACKGROUND)` on entry and restores
the previous priority on exit. Any exception → `AnalysisDecodeError`. Not
Robolectric-testable; verified on the device.

`SharedMusicLibrary.kt`: right after the library is opened, register
`MediaCodecTrackDecoder(applicationContext.contentResolver)`. Nothing else
changes there.

`TrackAnalysisLoader.kt`: `importAnalysis` moves from `analysisLane()`
(`:326`, shared with `readBars`) to its own single-thread lane so a 3-second
compute never queues a bar read behind it; `prepare` and `prefetch` keep
their contracts; a `Computed` outcome bumps `revision` exactly like
`Imported` and is logged at info level with the track id (the device run reads
that line).

Tests (`TrackAnalysisLoaderTest.kt`): `a_computed_analysis_refreshes_the_bars`,
`a_slow_import_does_not_block_a_bar_read` (fake import blocks on a latch;
`loadBars` completes first).

## A5 — Kotlin: the service requests the current track and runs the backfill

`TrackAnalysisBackfillPolicy.kt` (new, pure):
`fun analysisBackfillShouldRun(playing: Boolean, powerSaveMode: Boolean): Boolean`.

`ReprisePlaybackService.kt`: on every snapshot from `coreListener`
(`:53-65`): (a) when `current_track_id` changed, call
`library.importTrackAnalysis(id)` on a background dispatcher (never main —
`LibraryWriteThreadGuard`); (b) evaluate the policy with
`PowerManager.isPowerSaveMode` and call `startTrackAnalysisBackfill` /
`cancelTrackAnalysisBackfill` on transitions only; `onDestroy` cancels.
Progress goes to logcat only — no UI in this plan.

Tests: `TrackAnalysisBackfillPolicyTest.kt` — the four combinations; the
service's request-on-change is tested where the service's existing tests
live (a fake library counting `importTrackAnalysis` calls: one per track
change, none on a position tick).

## A6 — gates and hand-over

Run the gate list from the mother plan (scoped, never `--workspace`). Record
in `.pipeline-codex.md`: the red-then-green of every test above, the exact
`cargo test` and suite counts, and the enum name whose `when` blocks were
grepped in A2.
