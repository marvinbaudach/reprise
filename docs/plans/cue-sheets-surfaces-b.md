---
slug: cue-sheets-surfaces-b
worktree: /home/marvin/Projects/reprise-cue-sheets-surfaces-b
branch: feature/cue-sheets-surfaces-b
phase: planned
codex_session:
created: 2026-10-06
---
# CUE sheets — surfaces, strand b: desktop sites, scanner, sync, return channel

Mother plan: `docs/plans/cue-sheets-surfaces.md` (decisions, working rules, rule ids,
post-merge checks). Read it first; this file holds only strand b's ownership and tasks.

**Purpose:** every desktop place that still treats a path as one track acts per file or per
segment; sheets cut synced files on the phone (SAF); a CUE album syncs once with a derived
sheet; phone listens and ratings reach the right track; wave-2 findings A10, A11, B4/B8, B9
are fixed.

Precondition: `cue-sheets-schema` (v91) is on `dev` — the nullable columns
`library_exclusions.{segment_start_ms,segment_title,cue_path,cue_mtime,cue_size}` exist. Their
typed accessors are this strand's (b2, in `db_library_exclusions.rs`). This strand registers
no migration.

## Owns

- GNOME: `ui/delete_tracks*.rs`, `ui/track_list/{track_menu.rs,track_list_context_menu.rs,
  track_list_context_action_states.rs}`, `ui/tag_edit/**`, `ui/import_errors_view.rs`,
  `ui/device_sync/**`, the `ui/strings_*.rs` modules those use (`strings_tag_edit.rs`,
  `strings_issues.rs`, the delete and device-sync string modules).
- Core: `cue/**`, `library/{scanner_cue*.rs,scanner_segments.rs,scanner_entry.rs,
  scanner_segment_match.rs,scanner_mobile_sync.rs,trash_tracks.rs,exclusions.rs,
  library_doctor/scope.rs}`, `db_library_exclusions.rs`, `queries/{maintenance.rs,
  maintenance_delete.rs,import_errors.rs}`, `device_sync.rs`, `device_sync/**`,
  `db_device_sync.rs`, `db_mobile_sync.rs`.
- FFI (five files only): `crates/reprise-android-ffi/src/{listen_export_journal.rs,
  listen_export_recorder.rs,library_listen_report.rs,source.rs,source_tests.rs}`.
- platform-linux: `trash.rs`, `device_sync*.rs`, `device_transfer.rs`.
- Matching `po/` entries. `docs/ux-rules.md` section AK only: CUE-11 … CUE-18 (19/20 reserved).

New modules hang off a parent this strand owns (`#[path]` or a `mod` line in that parent) —
never a crate root or `ui/mod.rs`.

**Seam constraint:** `playback_session.rs` (strand c) calls
`listen_export_journal::prepare_report` (`playback_session.rs:637`). Keep its signature;
resolve segment identity inside the files above.

Near the cap (extract before growing): `delete_tracks.rs` 782, `track_menu.rs` 793,
`tag_edit_flow.rs` 790, `maintenance.rs` 760, `mirror.rs` 765, `device_sync/settings.rs` 773,
`platform-linux/device_sync.rs` 768, `scanner_entry.rs` 728.

## Facts (origin/dev `c0188ae2e1`/`d68a21e2a0`)

- Trash: `trash_tracks_with` trashes per `(id, path)`; N segments of one file → the first call
  trashes the album file, the rest fail, unselected siblings stay as rows on a trashed file.
  No `.cue` handling anywhere.
- Remove from library: `exclusions::record_track` copies `segment_index` (CUE-4 `[active]`).
  Exclusions carry no title/start, so a sheet edit that shifts positions hides another song
  (finding B9).
- Applied sheets: `scanner_cue.rs:158 load_applied` rebuilds "applied" from `tracks.cue_path/
  cue_mtime/cue_size`; a sheet whose files are all excluded has no rows, so it is re-read
  every scan (finding A11, `scanner_cue.rs:399-402` needs `!applied.files.is_empty()`).
- Tag editor opens for segments and every save fails (`tag_mutation.rs:145` refuses
  `segment_index > 0`); the "Edit failed tracks…" retry (`tag_edit_flow.rs:~183`,
  `tracks_and_bitrates_for_ids`) silently drops them.
- Doctor scope (`library_doctor/scope.rs`) does not filter segments; MCP doctor tools share it.
- Import errors: `InvalidCueSheet` shows the interim "Unclassified" copy
  (`import_errors_view.rs:49`); Retry calls `scan_folder(<sheet path>)` and finds no audio; an
  issue for a deleted sheet stays until dismissed.
- Device sync: everything keyed by track id (`device_files` PK `(device_serial, track_id)`).
  Device path is derived per track title (`sanitize.rs:44 device_track_path`) → N segments plan
  N copies; byte estimates count the file N times; removing one id would delete a file others
  need. A `.cue` on the device would be removed as an orphan (`mirror.rs:489
  plan_orphan_removals`; `is_removable_managed_path` knows only `.lrc` and report files).
  Default profile transcodes to `opus_160`.
- Return channel: `RPT-BACK` (`core/device_sync/listen_report.rs`, binary LE,
  `FORMAT_VERSION = 1`, entries keyed by `device_path` only). Phone writer path, all FFI:
  `listen_export_recorder.rs:74 write_queued_listens`, `library_listen_report.rs:49` (and
  `record_rating` `:30`) → `listen_export_journal.rs:81 prepare_report` (journal magic
  `RPT-JRNL`) → core types. Desktop `resolve_track` (`listen_report.rs:220-262`) filters
  `segment_index = 0`. `RPT-LIST` (`track_metadata_list.rs`) is written on the desktop by
  `ui/device_sync/device_sync_effects.rs:423/526` (effect from `machine.rs:569-580`) and applied
  on the phone by `scanner_mobile_sync.rs:~105` (`segment_index = 0`). Phone path registry
  `track_mobile_sync_paths(track_id PK, device_path)` maps every segment row to the shared path.
- SAF: `cue::resolve_file(sheet_dir, name, existing)` and sheet discovery use
  `Path::parent()`/path equality; core already abstracts directories with
  `LibrarySource::parent_of` (`library/source.rs:258`). FFI `BridgedSource` lives in
  `android-ffi/src/source.rs`, tests in `source_tests.rs` (module line in FFI `lib.rs:45`,
  existing — add tests there, add no module). Inferred, untested: a `.cue` beside a synced file
  never cuts it on the phone.

## Tasks

### b1 — trash per file (decision 9)
- Group the selection by file: a file whose every present segment is selected is trashed
  once, with its sidecar `.cue` if no other present file still references it; a multi-FILE
  sheet goes only with the last of its files; its rows are removed. A partial file's selected
  segments are hidden via `record_track`.
- Dialog copy counts trashed files and hidden tracks separately.
- Rule **CUE-11**; tests in `trash_tracks` (all / partial / multi-FILE / embedded sheet).

### b2 — exclusion identity (decision 7, findings B9 and A11)
- `db_library_exclusions.rs` gains the typed read/write of the v91 columns; `record_track`
  stores `segment_start_ms`, `segment_title` and the sheet version through them. `matches_segment` uses the `scanner_segment_match` order (start+title, unique
  title, start, position).
- `exclusions.rs` writes with `INSERT OR REPLACE`; once the v91 columns are filled, a
  re-exclusion hitting the `(device, inode, segment_index)` unique index would REPLACE the row
  and null them. Switch that statement to `ON CONFLICT … DO UPDATE` (or write every v91 column
  explicitly) and test that re-excluding a hidden segment keeps its start, title and sheet
  version.
- `load_applied` also reads the sheet version from exclusions, so a fully excluded CUE file
  takes the unchanged fast path (finding A11).
- Tests: a sheet edit inserting a track keeps the hidden song hidden and the others visible;
  a fully excluded file's sheet is not re-read on an unchanged rescan. Rule **CUE-18**.

### b3 — tag editor leaves CUE tracks out (decision 5)
- Classify the selection in a sibling extracted from `tag_edit_flow.rs` (790 lines); notice
  for segment-only selections; a mixed selection edits the whole-file rows and names the count
  left out; the failed-tracks retry path applies the same classification.
- Rule **CUE-12** (CUE-5 stays as the core refusal).

### b4 — Doctor scope skips CUE tracks
- All four `scope.rs` selectors filter `segment_index = 0`; MCP shares the scope. Rule **CUE-13**.

### b5 — sheet import errors (decision 6) and finding A10
- `kind_copy` gives `InvalidCueSheet` its own title/row copy; Retry rescans the sheet's
  directory; a deleted sheet's issue clears when its directory is scanned and the sheet is gone.
- Finding A10: a sidecar that parses but does not fit falls back to a valid embedded sheet.
- Rule **CUE-14**.

### b6 — sheets cut synced files on the phone (SAF)
- First a failing test in FFI `source_tests.rs` with a SAF-shaped `BridgedSource` (document-URI
  paths) holding an audio file and its `.cue`; then make sheet discovery and `resolve_file`
  go through `LibrarySource::parent_of` (or the equivalent) so it cuts. Desktop behaviour stays
  identical (existing `scanner_cue*`/`cue/*` tests green).
- Rule **CUE-16** (`[planned]` here; its live device proof is post-merge check 2 — flip to
  `[active]` on the rule-named FFI test).

### b7 — device sync per file with a derived sheet (decision 4)
- `query_sync_tracks`/`SyncTrack` carry the segment and the sheet. `build_plan` groups cut
  tracks by source file: one transfer (transcoded per the profile, once), device name from
  the source stem, size counted once; ledger rows per track id share the device path; removal
  only with the last track that needs the file.
- New `device_sync/derived_cue.rs`: serialises a sheet for the device — `FILE` names the
  device file (its extension after transcoding), tracks and INDEX 01 from the segment rows
  (so an embedded sheet becomes a sidecar). It has no source on the desktop: the plan emits
  an effect that writes these generated bytes beside the device file (not a copy effect).
  Round-trip test through `cue::parse` + `cue::resolve_file` against the device file name.
- The derived `.cue` is a managed path (`is_removable_managed_path`), never an orphan, and is
  removed with its file. Lyrics sidecars are not planned per segment. Nothing is ever written
  into the music collection.
- Rule **CUE-15**.

### b8 — phone listens and ratings per track (decision 8, findings B4/B8)
- `RPT-BACK` format v2: each listen and rating entry carries the device path plus the
  segment start (`None` for a whole file). The phone side resolves the segment start from the
  track id it already gets (`RecordedListen { track_id }`, `listen_export_recorder.rs:11`; the
  path comes from core `mobile_import::device_path_for_track`); `prepare_report`'s
  signature stays. The desktop `resolve_track` resolves `(device_path, segment_start)` to the
  segment row via the ledger; v1 reports are rejected outright (unreleased, no compat).
- `RPT-LIST` v2 carries the segment start per entry; `scanner_mobile_sync` applies ratings and
  counts to the matching segment row.
- An entry that still does not resolve is counted in the sync summary and logged — never
  dropped silently.
- Tests: core round trip phone-writer → desktop-reader for two tracks of one file; FFI test
  that a listen of segment 3 is journalled with its start. Rule **CUE-17**.

## Done

All tasks committed, gate battery green on the worktree. The device proofs (post-merge checks
2–4) are not this strand's — do not attempt them here.
