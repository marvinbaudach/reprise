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

## As built

- **b1** — `trash_tracks_with` keeps its signature and acts per file through the new
  `plan_file_trash`; `plan_trash`/`commit_trash` are unchanged, so the Android trash boundary
  (strand c) still trashes per `(id, path)`. A sheet goes along when the file's tracks remember
  it (`cue_path` on a segment row) and no other file in the library or among the exclusions
  names it; that includes a sidecar that did not fit and gave way to the embedded sheet (b5),
  and CUE-11 says so. The sheet goes after every file of the selection it describes, and
  stays when any of them failed to go. A sidecar whose trash fails after its audio went is only
  logged. The
  dialog asks the catalog only when the selection holds a CUE track; the result toast adds how
  many CUE tracks were hidden. `run_delete` moved to `delete_tracks_run.rs`; the CUE copy lives
  in `strings_delete_cue.rs` (new in `po/POTFILES.in`), since `strings.rs` is not this strand's.
- **b2** — exclusions write every v91 column through `ON CONFLICT … DO UPDATE` on both partial
  indexes. On every sheet the scan applies, a file's exclusions are matched like rows
  (`scanner_segment_match`) and each matched one takes its song's current position, start,
  title, sheet version and file mtime/size; an unmatched one is parked at `-id` and matches by
  start and title only. So a track that moves into a hidden song's old position never collides
  with it. A file with no rows whose v91 exclusions agree on the file mtime and the governing
  sheet takes the unchanged fast path; a pre-v91 exclusion opts out until a scan re-places it. When the
  directory or a sheet cannot be seen this scan, such a file is skipped as unchanged too, as
  rows a sheet cut are, so it never comes back as one whole-file track.
- **b3** — classification in `tag_edit/tag_edit_selection.rs`; the notice and the left-out
  count are toasts. Single-track browsing in the editor skips CUE tracks too.
- **b4** — one `WHOLE_FILE` clause on the three selectors (`current_view` goes through
  `present_track_ref`).
- **b5** — Retry uses `ImportErrorEntry::retry_root` (a method, because `queries/mod.rs` is not
  this strand's). Vanished sheets are cleared when their directory is listed, filtered by
  `parent_of` and the `.cue` extension, so an embedded sheet's issue (keyed by the audio file)
  is never touched. A10: a sidecar whose cut fails gives way to a valid embedded sheet and the
  rows remember the sidecar's version, so the next scan is `(0, 0, 1)`; a sheet that stopped
  covering the file (`Unfit`) is not remembered.
- **b6** — `scanner_cue::resolve` addresses each audio file by its own path where the source's
  paths are file names joined to their directory, otherwise by `display_name` joined to
  `parent_of(sheet)`; desktop resolution is byte-for-byte the old one.
- **b7** — per-track ledger rows as planned. `SyncTrack`/`DesiredManagedFile` gained no fields
  (foreign test literals); the CUE data travels as `MirrorPlaylistSnapshot.cue_files`, filled by
  the snapshot loaders. One member carries the transfer (a file-level `SyncTrack`: album title,
  file length); the others become `SharedRecord`s, written by the new `RecordSharedFile` effect
  after the copy or against the resident file. Every track of a CUE file takes its device path
  from the file — its album and album artist, or with none the performer of its first track — so
  a compilation sheet with only per-track performers still yields one path. A row no longer wanted whose file another row
  still needs is `ManagedRemoval::Unshared` (forget only); when no row of a file is wanted any
  more, one row removes the file and the rest are forgotten, and the file's bytes are counted
  once, whether it leaves, is kept for stability or is kept because it is missing. The derived sheet is
  `DerivedCueWrite`, written by `WriteDerivedCue` under the Copying step (no new `SyncStep`:
  foreign matches); it is kept by `known_paths` and leaves as an orphan with its file. Its
  name carries an FNV-1a hash of its contents (`album.1a2b3c4d.cue`): the inventory knows
  resident files only by size, and a moved `INDEX` keeps a sheet's size, so a changed sheet
  gets a new name and the old one leaves as an orphan. A frozen smart playlist that keeps a
  CUE file also keeps its sheet. Known gap: the sheet leaves only through the orphan pass, so
  a run without a successful device inspection (`managed_files_scanned == false`) leaves it
  behind when its file goes; the next inspected run removes it. Analysis
  sidecars are not planned for CUE files. Per-playlist and picker size estimates count a
  CUE file once, at its whole length however few of its tracks are selected (the sync page and
  `project_playlist_sizes`, which takes the playlist's CUE files). A playlist naming one CUE track still names the whole device file in
  its M3U, so the phone's import adds the whole album (CUE-6 behaviour), and a hidden track's
  stretch plays as part of the track before it on the phone.
- **b8** — `RPT-BACK` and `RPT-LIST` are format 2; v1 is refused. The phone journal keeps its
  sequence state but drops entries still in the v1 report format (one-time loss on the
  developer's phone; the alternative bricked every later write). `record_listen`/`record_rating`
  take `impl Into<ReportedTrack>`, so strand c's `&str` call sites compile unchanged. A start
  matches within one CD frame (14 ms); the derived sheet's ms→frames rounding is pinned by a
  round-trip test over every frame of 80 minutes (step 7).
- **Not changed, for the reviewer:** CUE-6 still says the phone's listens and ratings address
  whole-file tracks only. Its `cue_6_` tests live in files this strand does not own
  (`cue_lookup_tests.rs`, `media_browse_tests.rs`, `file_open.rs`, `playlist_io_tests.rs`), so
  replacing it with CUE-19 is left for a decision; CUE-17 now covers CUE tracks.
