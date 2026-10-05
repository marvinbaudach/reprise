---
slug: cue-sheets-core
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-04
---
# CUE sheets — core (wave 2)

Starts from dev only after both wave-1 strands of `docs/plans/loudness-and-cue-sheets.md` landed (it needs `cue/**`, `PlaybackItem`, `tag_scan_version` and per-track loudness). Spec: `docs/superpowers/specs/2026-10-04-cue-sheets-design.md`; decisions 4–7 of the mother plan bind this plan. Migration number: next free after r128's v88.

## Parallelität
Cannot be cut: C1–C4 all change `tracks` identity, the scanner write path and the contract, and every later task compiles against C1.

## Tasks

### C1 — Track identity: `segment_index`, table rebuild (migration v89)
- Columns `segment_index INTEGER NOT NULL DEFAULT 0` (0 = whole file),
  `segment_start_ms INTEGER`, `segment_end_ms INTEGER`, `cue_path TEXT` (NULL for an
  embedded sheet). Uniqueness becomes `UNIQUE(path, segment_index)` — not
  `(path, segment_start_ms)`: SQLite treats NULLs as distinct in UNIQUE, so a nullable
  start column would allow duplicate whole-file rows.
- Requires rebuilding `tracks` (inline UNIQUE cannot be dropped): 12-step procedure with
  `PRAGMA foreign_keys=OFF` **outside** the transaction, copy, drop, rename, recreate
  every index and trigger on `tracks`, `PRAGMA foreign_key_check`, ON again. Test on a
  DB with rows in all ten referencing tables.
- `Track`, `TRACK_COLUMNS`, `row_to_track`, `TrackSummary` gain the segment fields.

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
