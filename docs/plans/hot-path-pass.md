---
slug: hot-path-pass
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-04
strands: a,b
merge_order: a,b
---
# Hot-path pass — mother plan

A behaviour-preserving performance and refactoring pass over paths that run per
track, per row bind, per scan entry or per queue change. Three read-only surveys
(core/view, GNOME/platform, Android FFI/CLI/MCP) produced the candidates. The
user accepted these on 2026-10-04: the core/FFI/MCP group, the GNOME group, the
`playlist_tracks(track_id)` index, the Library Doctor algorithmics, and larger
windows for the Up Next footer total.

## Shared rules for both strands

- **Behaviour is preserved exactly.** Ordering, duplicates, missing ids, error
  propagation and first-match semantics stay what they are today. When an item
  turns out not to be exactly preserving on closer reading, skip it and say why
  in the final summary. Do not invent a near-equivalent.
- **Proof per item, as named in the strand file.** Use one of three kinds:
  - a statement-count test using the `trace_v2` pattern of
    `present_tracks_by_ids_match_per_id_results_and_run_one_statement`
    (`crates/reprise-core/src/queries/surface_browse.rs`);
  - an `EXPLAIN QUERY PLAN` test;
  - an oracle test that keeps the old algorithm as a test-only reference.

  Where the strand file says "no new test", the existing suite is the proof.
- **No speed claims without a measurement.** Summaries say what was changed
  structurally (statements, clones, complexity), not invented milliseconds.
- **File-size cap:** every code file created or substantially edited ends below
  800 lines. Extract a cohesive sibling module instead of trimming docs.
- **Out of scope** (deliberately rejected after the survey):
  - the track-reveal and current-selection id re-resolution (`track_reveal.rs`,
    `current_track_selection.rs`), because its fresh resolve per attempt is deliberate;
  - the FFI analysis backfill loop, which is deliberate as well;
  - the waveform accessible-value deduplication, which changes `ValueNow` granularity;
  - the sidebar rebuild;
  - the PCM buffer reuse in the GStreamer appsink;
  - anything under `crates/reprise-view/**`;
  - the list-geometry paths named in `AGENTS.md`;
  - `crates/reprise-core/src/cover_download*.rs`;
  - `scripts/` and `AGENTS.md`.

## The cut

| Strand | Purpose | Owns |
|---|---|---|
| A — `hot-path-pass-a.md` | scanner and playlist statements, the FFI queue path | `crates/reprise-core/src/library/{scanner_entry,exclusions,import_errors,playlist_membership,playlists}.rs` and their test siblings; `crates/reprise-core/src/spectrogram.rs`; `crates/reprise-android-ffi/src/**` |
| B — `hot-path-pass-b.md` | in-memory algorithms in core, GNOME bind and queue paths | `crates/reprise-core/src/library/stats_snapshot.rs` and its tests; `crates/reprise-core/src/library/library_doctor/{scan,review,grouping,types}.rs` and their tests; `crates/reprise-core/src/visuals/modes/bars.rs`; `crates/reprise-gnome/src/ui/track_list/{track_list_columns,track_list_title_column}.rs`; `crates/reprise-gnome/src/ui/tag_edit/tag_save_refresh.rs`; `crates/reprise-gnome/src/ui/cover/cover_cache.rs`; `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs`; `crates/reprise-gnome/src/ui/library_doctor/{summary_model,write_jobs}.rs` |

The two groups are disjoint. Neither strand reads a file the other owns for
verification.

## Merge order

The strands are independent and land in completion order; the later one rebases onto the
`dev` the earlier landing produced.

## Deferred: the refactor wave owns these paths

A parallel session runs `refactor-wave-2026-10` (three strands, same base commit). It owns
`reprise-core/src/queries/**`, `reprise-core/src/{db,db_schema_baseline,db_migrations}.rs`,
`reprise-core/src/library/settings*.rs` and the `ui/{playback,preferences,window,style}` files of
its strand C, `queue_transport.rs` among them, plus call-site files of its strand A, among them
`reprise-mcp/src/data.rs` and `ui/tag_edit/tag_edit_flow.rs`. These accepted items therefore wait until that
wave has landed:
- `prepare_cached` in the browse queries;
- the batched `track_source_paths` for Android play/queue;
- renumbering each playlist once per delete;
- the `playlist_tracks(track_id)` index (next free migration in the new migration list);
- the `HashSet` in `queue_purge_plan` and in `tag_edit_flow.rs`;
- the MCP page-collector refactor.

## Post-merge cross-checks

- After both land: `cargo test --workspace` on `dev`, plus the core purity check
  (`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must be empty).
