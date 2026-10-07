---
slug: cue-sheets-surfaces-c
worktree: /home/marvin/Projects/reprise-cue-sheets-surfaces-c
branch: feature/cue-sheets-surfaces-c
phase: planned
codex_session:
created: 2026-10-06
---
# CUE sheets — surfaces, strand c: Android + analysis

Mother plan: `docs/plans/cue-sheets-surfaces.md` (decisions, working rules, rule ids,
post-merge checks). Read it first; this file holds only strand c's ownership and tasks.

**Purpose:** the phone plays a CUE track as its own stretch with its own metadata and
analyses it; analysis on both platforms cuts by timestamp, lets the last track run to EOF and
remembers a failed decode (findings C5, C6, C7, C8).

Precondition: `cue-sheets-schema` (v91) is on `dev` — the table `render_data_failures`
exists. Its accessors are this strand's (c5). #1149 (`the-spectrum-fills-while-decoding`) has
landed.

Sibling: `feature/the-backfill-keeps-its-decode` (worktree
`/home/marvin/Projects/reprise-the-backfill-keeps-its-decode`) changes
`ReprisePlaybackService.kt`, `TrackAnalysisBackfillPolicy.kt`, `BackfillStopGrace.kt`,
`PlaybackUiState.kt` — the Android backfill area c5/c6 rework. If it has landed when `/code`
starts, branch after it; if it is still open, rebase onto it before landing and read its
diff before touching those files.

## Owns

- `crates/reprise-android-ffi/**` **except** strand b's `src/{listen_export_journal.rs,
  listen_export_recorder.rs,library_listen_report.rs,source.rs,source_tests.rs}`. This
  includes FFI `src/lib.rs` (the `analysis_failed` set is created at `:118`) — the one crate
  root a strand may edit; strand b does not touch it.
- `android/**`.
- Core: `render_data_segments.rs`, `render_data_segments_tests.rs`, `waveform_cache.rs`,
  `db_spectrogram.rs`, `db_render_data_failures.rs`, `spectrogram_backfill.rs`.
- platform-linux: `waveform.rs` (render-data backend, not the player).
- `docs/ux-rules.md` section E only: MTP-66, MTP-67, MTP-68 (69 reserved).

Not owned, read-only: `core/cue/**`, `scanner_cue*.rs` (strand b), `render_data_session.rs`
and `render_data_partial.rs` (unchanged by this plan — the inner session only ever sees
already-sliced PCM). **Seam:** `playback_session.rs:637` calls b's
`listen_export_journal::prepare_report`; leave that call as it is.

Near the cap: `db_spectrogram.rs` 730, `platform-linux/waveform.rs` 692, FFI
`track_analysis/compute.rs` 662, `playback_session.rs` 756, `NowPlayingSheet.kt` 794,
`spectrogram_backfill.rs` 619.

## Facts (origin/dev `d68a21e2a0`, after #1149)

- `TrackRow` (`android-ffi/src/browse.rs:32`) drops `segment`. Port
  (`playback.rs:106 AndroidPlaybackPort`): `play_path(path, gain_db)` / `set_next(uri,
  gain_db)`; the session builds items with `segment: None`.
- Kotlin package `io/github/marvinbaudach/reprise` (AGENTS.md's `de/reprise/spike` is stale).
  `Media3PlaybackPort.kt` keys everything by uri (metadata LRU, `refreshQueued`,
  `attachArtwork`, `resolveTrackMetadata(uri)` → `trackByUri` = first segment). No
  `ClippingConfiguration` exists. Position ticker emits `player.currentPosition/duration` —
  item-relative for a clipped item.
- `TrackGainAudioSink` switches gain per announced output stream offset
  (`setOutputStreamOffsetUs`) — one per MediaItem; clipping may change that: test with two
  clips of one file and measure on the device.
- No `androidTest/` dir; `ANDROID_TEST_FLOOR=334` in `scripts/check-android-suite.sh`.
- Foreground analysis: C7 refusal at `track_analysis/compute.rs:521`
  (`if track.segment.is_some() → DecodeFailed`). `AnalysisPcmSink::push_pcm_i16(&self, bytes,
  sample_rate_hz, channel_count) -> bool` (`compute.rs:166`) drives a whole-file
  `RenderDataSession` (`:72`, `:86`); partial snapshots via `PartialRenderData` (`:126`).
- Segment cutting (`render_data_segments.rs`): frame counting — `frames_seen` (`:27`),
  `route()` (`:84-116`), cut `:103-111` via `frame_at(ms, rate)` (`:128`). The last segment's
  end is the metadata `duration_ms` (`cue/segments.rs:181`), so a short metadata duration drops
  the tail and a long one leaves the track pending forever (finding C5). PTS placement needs an
  API change in `render_data_segments.rs` only; its only session caller is
  `platform-linux/waveform.rs` (`:132` new, `:136` `push_pcm_f32`, in `extract_segments` `:118`).
  `SegmentBounds` users: `core/waveform.rs:106`, `spectrogram_backfill.rs:124/167-173`,
  `waveform_cache.rs:62-90/117`.
- PTS is at hand but discarded: desktop `waveform.rs:248 pcm_of` maps `sample.buffer()`;
  Kotlin `MediaCodecTrackDecoder.kt:149` pushes with `bufferInfo` in scope
  (`presentationTimeUs` unused); loop `runDecodeLoop` `:100`.
- Pending work: `pending_render_data_tracks` (`db_spectrogram.rs:414`, used by desktop
  `spectrogram_backfill.rs:41` and Android `track_analysis/backfill.rs:259`),
  `pending_segment_render_data_files` (`:453`, desktop only), `pending_segment_tracks_of`
  (`:459`), `set_segment_render_data` (`:91`). Android backfill has no segment handling.
- Failures: desktop retries every run (`summary.failed`); Android keeps
  `analysis_failed: Arc<Mutex<HashSet<i64>>>` in memory (`library_types.rs:47`, `lib.rs:118`,
  `backfill.rs:378`). Finding C8: a file whose rate/channels change mid-stream, or truncated
  before a track, is decoded again on every backfill run.

## Tasks

### c1 — segment through the FFI and the port
- `TrackRow` gains `segment_start_ms: Option<i64>` and `segment_end_ms: Option<i64>`; the
  last segment of its file gets `segment_end_ms = None` (decision 3; derive "last" as
  `max(segment_index)` per path). The session builds items with the segment; the port carries
  the bounds (`play_path`/`set_next` take a record or extra parameters).
- Kotlin resolves metadata by track id (or `(uri, start)`), never `trackByUri` alone; metadata
  LRU, `refreshQueued`, `attachArtwork` stop keying by uri.
- Tests: FFI — two segments of one file give distinct rows and the last has no end; Kotlin —
  two queued segments of one uri show their own titles.

### c2 — clipped playback (decisions 1–3)
- `MediaItem.ClippingConfiguration(start, end)`, `end = C.TIME_END_OF_SOURCE` when
  `segment_end_ms` is `None`; position/duration item-relative (Media3); per-item gain via
  `TrackGainAudioSink` unchanged (test two clips of one file get their own gains).
- No crossfade on the phone at a CUE transition, if the phone crossfades at all — check and
  state the finding in the commit.
- Robolectric tests on `Media3PlaybackPort`. Rules **MTP-66** (own stretch, own metadata) and
  **MTP-68** (last track plays to its end).

### c3 — segment cutting by timestamp (finding C6)
- `SegmentedRenderDataSession::push_pcm_*` takes the chunk's start time (PTS in ns or µs —
  pick one, name the unit in the parameter); `route()` places each chunk by its timestamp,
  tolerates a gap (silence is not invented; the stretch just has fewer samples) and an
  overlap (later samples win or are dropped — document which).
- Desktop `waveform.rs` passes `buffer.pts()`; the Android path (c4) passes
  `presentationTimeUs`.
- Tests in `render_data_segments_tests.rs`: a dropped chunk does not shift later tracks; a
  timestamp-less chunk falls back to the running frame count.

### c4 — the last segment runs to EOF (finding C5, decision 3)
- In the segmented session the file's last segment absorbs every sample until the decoded
  end, whatever its `end_ms`; at `finish()` it counts complete. `waveform_cache.rs`
  `segment_bounds` and the pending queries treat a finished last segment as complete even when
  the decoded length is shorter than its metadata duration.
- Tests: metadata duration 2 s short and 2 s long — the last track is complete either way and
  the earlier tracks are unchanged.

### c5 — failed decodes are remembered (finding C8)
- New `db_render_data_failures.rs` (wired from `db_spectrogram.rs` or an extracted sibling,
  not core `lib.rs`): `record_render_data_failure(conn, track_id, reason)` (reads the current
  fingerprint from `tracks`, like `source_fingerprint`), `render_data_failed(conn, track_id)`
  (true only while fingerprint and format version still match), `clear_render_data_failure`.
- Desktop backfill and Android backfill record `record_render_data_failure` when a file
  cannot be measured (mid-stream rate/channel change, truncation before a track, decode error);
  `pending_render_data_tracks` and `pending_segment_render_data_files` skip tracks where
  `render_data_failed` holds. The Android in-memory `analysis_failed` set is replaced by the
  marker. A changed file (fingerprint) is pending again.
- A re-cut segment is pending again too. v91 already extends both invalidation triggers
  (`invalidate_track_render_data`, `invalidate_segment_render_data`) to delete the track's
  `render_data_failures` row, so this strand writes no trigger and no migration; c5 only
  tests the behaviour through its accessors. The v91 table's `source_device`/`source_inode`
  are nullable, like `track_spectrograms`, so `record_render_data_failure` must accept a
  track without a stat identity.
- Extract from `db_spectrogram.rs` (730) before touching the pending queries.
- Tests: a failing file is decoded once across two backfill runs; touching it makes it pending;
  changing a segment's bounds in its CUE sheet makes that track pending.

### c6 — the phone analyses CUE tracks (finding C7)
- Remove the refusal at `compute.rs:521`. Foreground and backfill compute a CUE file once with
  `SegmentedRenderDataSession` (timestamps from c3) and store each track via
  `set_segment_render_data`; the Android backfill lists `pending_segment_render_data_files`.
  The partial/progress path of #1149 keeps working for the track being played (its stretch
  only).
- `MediaCodecTrackDecoder.kt` passes `presentationTimeUs`.
- Tests: FFI — one decode stores render data for every track of the file; Kotlin — the seek bar
  of segment 2 fills from its own start. Rule **MTP-67**.

### No device run inside this strand
- A file + `.cue` on the phone is cut only once strand b's SAF fix (b6) is in, so the gap,
  per-clip gain and EOF measurement on the device is post-merge check 2 of the mother plan.

## Done

All tasks committed, gate battery + Android suite green on the worktree. The device and
cross-strand proofs (post-merge checks 2, 3, 5) are not this strand's.

## As built

Commits on `feature/cue-sheets-surfaces-c`, in the order they landed (c3, c4 and c5 first:
they are core-only and c6 builds on all three):

- **c3** `712ba9016b`: `SegmentedRenderDataSession::push_pcm_*` take `start_us: Option<i64>`
  (microseconds, rounded to the nearest frame). A gap leaves its stretch short; audio for a
  stretch already measured is dropped, so the earlier copy wins. A timestamp within
  `CONTIGUOUS_TOLERANCE_US` (1 ms) of the running count is taken as the count, which keeps
  decoders that truncate to whole microseconds bit-exact. Frames stamped before 0 are dropped.
  Desktop `waveform.rs` passes each buffer's PTS (`start_us_of`).
- **c4** `eb376c094c`: `SegmentBounds` gained `last_in_file`, and `same_cut` compares only
  start and end. The store checks the cut alone, so the flag never reads as a re-cut. "Last"
  is the highest `segment_index` of the path over all rows, missing ones included, in the
  pending queries, `waveform_cache::segment_bounds` and the FFI alike. The pending queries
  moved to `db_spectrogram_pending.rs`.
- **c5** `a857c3958e`: `db_render_data_failures.rs` (hung off `db_spectrogram.rs`). The
  accessors are reachable from outside core as `reprise_core::spectrogram_backfill::*`,
  because `core/db.rs` is not this strand's. A successful store clears the marker. Desktop
  and phone record a decode error, a CUE file that cannot be decoded, and a stretch the
  stream never reached. Cancelled, superseded and source-changed decodes record nothing.
  The phone's in-memory `analysis_failed` set is gone. The desktop backfill's tests moved to
  `spectrogram_backfill_tests.rs`.
- **c1** `87ba207bc5`: `TrackRow.segment_start_ms/segment_end_ms` (UniFFI default `None`),
  built through `track_segment.rs`. The port takes `AndroidPlaybackItem { track_id, uri,
  gain_db, segment }`; the session resolves the segment by track id, as it does the gain.
  - Kotlin keys metadata by `PlaybackKey(trackId, uri)`, and each `MediaItem` carries its
    `PlaybackRequest` as its tag. A late answer or cover rebuilds each item from its own
    request.
  - Deviation: a cover stays keyed by uri, because it belongs to the file and every track of
    a CUE file shows it. The bug the plan meant, a rebuild by uri swapping track 2 for
    track 1, is fixed.
  - The widget resolves metadata by key too; it had the same `trackByUri` flaw.
  - The `[planned]` drafts for MTP-66/67/68 are in this commit, because its tests already
    carry those ids.
- **c2** `cdfa279999`: `PlaybackItems.build` sets the `ClippingConfiguration`, with
  `C.TIME_END_OF_SOURCE` for the last track. Crossfade finding: the phone never crossfades.
  Media3 has none, and `AndroidPlaybackBackend` maps Crossfade to Gapless. MTP-66 and
  MTP-68 are active. The MTP-68 text covers playback only; the analysis-to-EOF clause moved
  to MTP-67.
- **c6** `916d5bee5d`:
  - The C7 refusal was removed. `segment_job.rs` plans one decode of the file: the track
    asked for plus its pending siblings, the last running to EOF.
  - The PCM sink moved to `sink.rs`. It holds either a whole-file session or a segmented
    one, and the partial picture comes from the asked-for track's stretch.
  - The backfill lists pending CUE files after whole files. It counts each track of a
    decoded file as stored or remembered as failed.
  - Kotlin calls the new `push_pcm_i16_at` with `presentationTimeUs`. The old
    `push_pcm_i16` stays for a chunk without a time. MTP-67 is active.

Checked after c6: a clipped item that gains its cover is updated in place by a real ExoPlayer
(`mtp_66_a_clipped_cue_track_that_gains_its_cover_is_updated_in_place`), so a late cover does
not re-prepare the clip. A CUE track has no sidecar to import: `register_sidecar` registers
whole-file rows only (`segment_index = 0`), so the segmented compute always runs for it.

Left open, by design or for a later change:
- A foreground request for a track whose file the backfill is decoding under a sibling's id
  preempts the backfill and decodes the file again. This is documented in `segment_job.rs`
  and was not optimised.
- A SAF failure (no file descriptor, resolver gone) surfaces as a decode error and is now
  remembered until the file's fingerprint changes. Before c5 such a failure was retried on
  the next process start.
- Media3 clipping with a start above 0 needs a seekable source (FLAC with a seek table or
  binary search, WAV, CBR or Xing MP3). An unseekable file fails to play its later tracks.
  The device check shows whether this matters.
- Post-merge device checks, as listed in the mother plan: the gap between two contiguous
  clips, each clip's own gain, the last track playing to EOF, and `presentationTimeUs`
  placement on real decoders. A decoder that repeats or does not advance
  `presentationTimeUs` would have its buffers dropped as late (the earlier copy wins), so
  the stored tracks come out short. To catch that, compare each stored track's frame count
  with its duration.
