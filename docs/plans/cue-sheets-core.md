---
slug: cue-sheets-core
worktree: ../reprise-cue-sheets-core
branch: feature/cue-sheets-core
phase: refactored
codex_session:
created: 2026-10-04
---
# CUE sheets — core (wave 2)

Starts from dev only after both wave-1 strands of `docs/plans/loudness-and-cue-sheets.md` landed (it needs `cue/**`, `PlaybackItem`, `tag_scan_version` and per-track loudness). Spec: `docs/superpowers/specs/2026-10-04-cue-sheets-design.md`; decisions 4–7 of the mother plan bind this plan. Migration number: v90 — r128's loudness migration took v89.

## Parallelität
Cannot be cut: C1–C4 all change `tracks` identity, the scanner write path and the contract, and every later task compiles against C1.

## Tasks

### C1 — Track identity: `segment_index`, table rebuild (migration v90)
- Columns `segment_index INTEGER NOT NULL DEFAULT 0` (0 = whole file),
  `segment_start_ms INTEGER`, `segment_end_ms INTEGER`, `cue_path TEXT` (NULL for an
  embedded sheet), `cue_mtime INTEGER` (the sheet's mtime, NULL for an embedded sheet: the
  rescan needs it to notice a changed sheet, and nothing else stores it).
  `library_exclusions` gains `segment_index INTEGER NOT NULL DEFAULT 0` in the same
  migration and its two unique indexes include it, so removing one CUE track from the
  library hides that track only. Uniqueness becomes `UNIQUE(path, segment_index)` — not
  `(path, segment_start_ms)`: SQLite treats NULLs as distinct in UNIQUE, so a nullable
  start column would allow duplicate whole-file rows.
- Requires rebuilding `tracks` (inline UNIQUE cannot be dropped): 12-step procedure with
  `PRAGMA foreign_keys=OFF` **outside** the transaction, copy, drop, rename, recreate
  every index and trigger on `tracks`, `PRAGMA foreign_key_check`, ON again. Test on a
  DB with rows in all ten referencing tables.
- `Track` and `TrackSummary` gain one `segment: Option<TrackSegment>` field (`index`,
  `start_ms`, `end_ms`, `cue_path`; `None` = whole file); `TRACK_COLUMNS` and `row_to_track`
  read the four columns.

### C2 — Scanner: sheets beside files and embedded `CUESHEET`
- The walk collects `.cue` entries per directory (precedent: `mobile_sync.observe`);
  embedded `CUESHEET` read via `ItemKey::Unknown("CUESHEET")` for FLAC.
- A file covered by a valid sheet is written as its segments (upsert on
  `(path, segment_index)`); its whole-file row, if any, is removed in the same
  transaction (its id is not reused). An invalid sheet leaves the file as one track and
  records an import issue the Library Doctor shows.
- Rescan: sheet mtime change re-segments; sheet removed ⇒ back to one whole-file row.
- `tag_scan_version` (from R2) applies per file.
- Move detection (`scanner_move.rs`): a `(device, inode)` match moves all segments.

### C3 — Every path lookup
Review and fix each by-path site: `scanner_entry.rs` (known_row, restore, upsert),
`queries/maintenance.rs::track_id_for_path` (+ `file_open.rs`, `playlist_io.rs`),
`tag_edit_seed.rs`, `tag_edit.rs`, `relink.rs`, `rhythmbox_import.rs`,
`scanner_mobile_sync.rs`, `db_mobile_sync.rs`, `ai_promotion.rs`, `artist_context.rs`,
`maintenance_delete.rs`, `maintenance_missing.rs`, `exclusions.rs`, `import_errors.rs`.
`file_open.rs`: opening a CUE-covered file queues all its segments in order. Rule: a by-path lookup either addresses the whole file (returns all segments) or
takes `segment_index`; never "the first row by accident". Test per site.

### C4 — Contract: `PlaybackItem.segment`; analysis per segment
- `PlaybackItem { path, gain_db, segment: Option<(i64, i64)> }` (desktop/Android use it in wave 3).
- `RenderDataSession` accepts segment boundaries and yields one `TrackRenderData` (peaks,
  spectrogram, loudness) per segment from one decode; the backfill groups pending
  segment rows by file.
- Analysis sidecars are not written for segment tracks (one sidecar name per file would
  collide); the phone analyses segments itself with the same session code.

---

## As built (deviations from the tasks above)

- **C1** — v90 also adds `cue_mtime` and `cue_size` (the sheet's mtime and size; without them a
  changed sheet cannot be noticed) and `library_exclusions.segment_index` with both unique indexes widened. The rebuild
  renames with `legacy_alter_table` because `listen_events_fill_snapshot` reads `tracks`; ten
  referencing tables was nine foreign keys on eight tables. `Track`/`TrackSummary` carry one
  `segment: Option<TrackSegment>`. A second trigger drops a track's analysis when its cut changes.
- **C2** — the tag-scan version is 2 (a FLAC's `CUESHEET` is read in the same open as its tags),
  so every file is read once more. lofty 0.25 has no `ItemKey::Unknown`; the comment comes from
  `FlacFile` directly. A sheet is found by listing the directory of each audio file before the
  batch is classified, not by observing the walk, which delivers a sheet after its audio.
  Sheets an earlier scan applied are recognised from `cue_path` + `cue_mtime` and not re-read.
  A sheet that parses but does not fit leaves a whole-file row that remembers it. A broken sheet
  is `ImportErrorKind::InvalidCueSheet`, keyed by the sheet. `report.added/updated` count tracks.
- **C3** — `track_ids_for_path` is new; `track_id_for_path` is the first track in play order. An
  M3U import uses `playlist_tracks_for_path`: a line naming a CUE file adds its tracks still in
  the library, and a run of lines naming the same file adds them once. Tag editing,
  Rhythmbox ratings, sidecars, mobile metadata and instrumental promotion address whole-file
  rows. `(id, path)` sites are unchanged: they already name one row.
- **C4** — `SegmentedRenderDataSession` wraps one `RenderDataSession` per track instead of
  changing `RenderDataSession` itself. `pending_render_data_tracks` lists whole-file rows;
  `pending_segment_render_data_files` lists CUE files. The Android backfill keeps analysing
  whole-file rows only until wave 3, and its foreground decode refuses a CUE track. The
  on-play analysis (`waveform_cache`) asks the backend for the track's own stretch and stores
  nothing where the backend cannot cut a file. `AnalysisSidecar::for_track` is `None` for a CUE
  track. `PlaybackItem.segment` is filled where a summary is at hand and ignored by every backend
  until wave 3.

## Review fixes (wave 2 review)

- A sheet the scan cannot see (unlisted directory, probe `Unknown`, unreadable sheet) is
  `Cover::Unknown`: rows a sheet cut stay untouched; other files are read as if no sheet were
  there. Only a sheet that was read and does not parse or fit is broken.
- An edited sheet keeps each song on its row: start and title, then a unique title, then the
  start, then the position (`scanner_segment_match.rs`).
- The parser caps a sheet at 1 MiB and 999 tracks, sidecar or embedded. The v90 migration fails
  only on dangling references it made itself.
- A dismissed issue on an audio file no longer blocks a sheet that arrived beside it; a broken
  embedded sheet honours its dismissal; a dismissed rejection survives a re-read. A directory
  with a file no sheet claims has its applied sheets read again.
- Move detection matches a CUE file by size, its tracks' album and its length.
- Relink counts every track of a CUE file and leaves a removed sibling removed. A phone listen or
  rating of a CUE file is unresolved. A CUE track reads and writes no lyrics beside its file.
- Analysis of a CUE track is stored only for the cut it was measured from; a stretch past the
  decoded end stays pending; the on-play decode measures the file's other pending tracks too
  and is cancelled when the listener moves on.
- Not changed: the phone's foreground analysis of a CUE track (C7). Its refusal costs one query
  and no decode, Kotlin treats `DecodeFailed` as final, and the backfill never lists CUE tracks,
  so `mark_failed` would change nothing observable.

## Left for wave 3

- Deferred review findings: a sidecar that parses but does not fit does not fall back to a valid
  embedded sheet (A10); a fully excluded CUE file is re-read every scan (A11); phone ratings and
  play counts for CUE files are dropped without a count (B8); an exclusion is keyed by position,
  so a sheet edit hides another track (B9); the last track ends at the metadata duration, not
  at EOF (C5); segment cutting counts frames instead of using PTS (C6); a file whose rate or
  channels change mid-stream, and one truncated before a track, is decoded again on every
  backfill run (C8).

- Desktop and Android playback ignore `PlaybackItem.segment`: a CUE track plays its file from 0:00.
- Delete, trash and device sync still treat a path as one track; sync sees several track ids
  sharing one file and device path. A CUE track shows the interim "Unclassified" copy for its
  sheet's issue, and the issue is keyed by the sheet, so "Retry" on it finds no audio.
- The Android backfill and the phone's own scan analyse whole-file tracks only.
- An issue for a deleted sheet stays until dismissed.
- Tag scan version 2 re-reads every file once; the scan lists each audio directory a second time
  (free on Unix, one more cursor query per directory over SAF — not measured).
