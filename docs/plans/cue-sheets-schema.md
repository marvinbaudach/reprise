---
slug: cue-sheets-schema
worktree: /home/marvin/Projects/reprise-cue-sheets-schema
branch: feature/cue-sheets-schema
phase: reviewed
codex_session:
created: 2026-10-06
---
# CUE sheets — wave 3 schema (v91)

Single-strand precondition of `docs/plans/cue-sheets-surfaces.md`. It lands on `dev`
**before** `/code docs/plans/cue-sheets-surfaces.md` starts, so the three surface strands
branch from a `dev` that already has v91 and never register a migration themselves.

Why a separate plan: `migration_registry_is_contiguous_without_duplicates` (`db.rs:149`)
fails any branch that registers v92 without v91, and wave 3 needs one schema change from
the scanner side (exclusion identity, findings B9 and A11) and one from the analysis side
(persisted failure, finding C8). Owning both in one strand would make the other strand wait
for that strand's merge mid-run.

**Scope rule:** the migration only. No behaviour changes and no accessors: nothing reads the
new columns or the new table yet. Every reader and writer is a task of strand b or c of the
mother plan, and they code against the exact names fixed here.

Implementer: an Opus `worker` (`model: "opus"` on the spawn; Codex is out until
2026-10-09 23:21). Test-first, AGENTS.md gate battery before the commit
(`cargo +1.99.0 clippy --all-targets --workspace -- -D warnings` is CI's toolchain).

## Facts (origin/dev `d68a21e2a0`)

- Registry: `crates/reprise-core/src/db_migrations.rs:23` `MIGRATIONS`, last line
  `migration!(90, crate::db_cue_segments::migrate_v90)` (`:105`). Pattern to copy:
  `db_cue_segments.rs` (`const VERSION`, module doc naming what the version does, one
  `migrate_vNN(conn)` fn). Modules are declared as private `mod` in core `lib.rs` (`:49`, `:58`).
- `library_exclusions` got `segment_index INTEGER NOT NULL DEFAULT 0` in v90
  (`db_cue_segments.rs:77`). Accessors: `db_library_exclusions.rs` (63 lines).
- Render-data tables `track_spectrograms` (`db_spectrogram.rs:15`) and `track_loudness`
  carry the source fingerprint `source_mtime, source_size, source_device, source_inode`
  plus a format version; `source_fingerprint(&transaction, track_id)` (`db_spectrogram.rs:69`)
  and `track_source_fingerprint` (`:349`) read it from `tracks`. `db_spectrogram.rs` is 730
  lines; strand c adds the failure accessors in a sibling, not there.
- No analysis failure is persisted anywhere today (desktop counts `summary.failed`; Android
  keeps `analysis_failed: Arc<Mutex<HashSet<i64>>>` in `android-ffi/src/library_types.rs:47`).
- The phone runs the same core migrations on its own DB.

## Task s1 — migration v91

New file `crates/reprise-core/src/db_cue_wave3.rs` (private `mod` in `lib.rs`,
`migration!(91, crate::db_cue_wave3::migrate_v91)` in the registry), module doc explaining
both halves.

1. `library_exclusions` gains five nullable columns (NULL for whole-file exclusions and for
   rows written before v91):
   - `segment_start_ms INTEGER` — the excluded segment's start (finding B9)
   - `segment_title TEXT` — its title at exclusion time (finding B9)
   - `cue_path TEXT`, `cue_mtime INTEGER`, `cue_size INTEGER` — the version of the sheet the
     excluded segment came from, so the scanner can recognise a fully excluded CUE file as
     "already applied" (finding A11)
2. New table:
   ```sql
   CREATE TABLE IF NOT EXISTS render_data_failures (
     track_id       INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
     source_mtime   INTEGER NOT NULL CHECK (source_mtime >= 0),
     source_size    INTEGER NOT NULL CHECK (source_size >= 0),
     source_device  INTEGER,
     source_inode   INTEGER,
     format_version INTEGER NOT NULL CHECK (format_version > 0),
     reason         TEXT NOT NULL,
     failed_at      INTEGER NOT NULL
   );
   ```
   This is the DDL that shipped. Its fingerprint columns and CHECKs follow
   `track_spectrograms` exactly, including the nullable `source_device`/`source_inode`.
   `track_id` is a track row, so a CUE segment has its own marker.
   Check whether `tracks` deletes cascade elsewhere the same way (foreign keys pragma) and
   match what `track_spectrograms` does.
3. v91 also extends both invalidation triggers, `invalidate_track_render_data` and
   `invalidate_segment_render_data`, to delete the track's `render_data_failures` row.

Tests (beside the existing `db_*_migration_tests.rs`, new `db_cue_wave3_migration_tests.rs`):
fresh DB has both; a v90 DB with exclusion rows migrates and keeps them with NULLs; the
registry contiguity test stays green.

## No accessors here

Core `db_*` modules are private `mod`s with `pub(crate)` functions, so an accessor without a
production caller fails `clippy -D warnings` as dead code. The typed accessors therefore
belong to the strands that call them: strand b extends `db_library_exclusions.rs`, strand c
creates `db_render_data_failures.rs`. This plan fixes only the names they code against.

## Done

- Gates green, one commit, PR into `dev` per `docs/agents/branching.md`, landed with
  `land.sh`. Then the installed `reprise-mcp` refuses the newer schema — rebuild it as the
  SessionStart hook says.
- Then, and only then: `/code docs/plans/cue-sheets-surfaces.md`.

## Parallelität

Not cut: one migration file, its test file, one registry line, one `mod` line — a single small strand.
It is itself the cut's precondition for the mother plan.
