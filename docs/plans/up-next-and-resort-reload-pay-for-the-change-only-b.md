---
slug: up-next-and-resort-reload-pay-for-the-change-only-b
worktree: /home/marvin/Projects/reprise-up-next-and-resort-reload-pay-for-the-change-only-b
branch: feature/up-next-and-resort-reload-pay-for-the-change-only-b
phase: refactored
codex_session:
created: 2026-09-07
---
# Strand B — a sort-field save moves the block instead of reloading the list

Mother plan: `docs/plans/up-next-and-resort-reload-pay-for-the-change-only.md`
(read §0–§2 first; §2's rules bind every task here). This strand starts from
a dev that already contains P0 (`queue_snapshot_change` and
`queue_snapshot_emissions` live in `queue_snapshot_change.rs`, which is
strand A's file — do not edit it).

## Why

A multi-select save that edits the sort field (harness G5: Artist on eight
contiguous rows, artist sort) reloads the list full-range:
`items_changed(0, 1972, 1972)` costs 77 ms of GTK's own walking for a change
that moved eight rows as one block. The other ~55 ms of the idle is the
viewport jump to the block, which stays (mother §6). And the completion
line's `reload_ms` counts 170–200 ms of idle wait behind the dialog's close
animation as if it were work, which hides that G4 is already at 2–3 ms.

## File ownership

- `crates/reprise-gnome/src/ui/track_list/track_list_model.rs` and its test
  files (`track_list_model_tests.rs`, `track_list_model_scalability_tests.rs`)
- new `crates/reprise-gnome/src/ui/track_list/track_list_model_move.rs` (+ tests)
- `crates/reprise-gnome/src/ui/track_list/track_list_model_change.rs`
- `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`
- `crates/reprise-gnome/src/ui/track_list/tag_mutation_refresh.rs` and its display test files
- `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`
- `crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs`, `tag_save_refresh.rs` (+ tests)
- `crates/reprise-gnome/src/ui/track_list/mod.rs` — only the
  `mod track_list_model_move;` line

Not this strand's: `queue_snapshot_change.rs`, `reprise-view`, anything under
`ui/playback`. The list is a statement of ownership, not a fence: if a task
cannot be met without touching another file, stop and say so in
`.pipeline-codex.md` rather than guessing.

## Tasks

### B0 — the completion line tells wait from work

- `tag_mutation_refresh::refresh_with_reload_change`: record `Instant::now()`
  when the idle is scheduled and again when it fires; hand the idle's own
  duration and the wait to whoever logs `tag-edit batch completed`
  (`tag_edit_flow`'s `log_completed`), as new fields `idle_wait_ms`,
  `reload_work_ms`, and `emit` with the values `"metadata"` (the
  `metadata_only` branch), `"span"`, `"move"`, `"full"`. Keep `reload_ms` and
  `delta` for continuity with the F-run tables.
- `track_list_reload::reload_with_anchor_and_viewport` logs one info line
  `reload split` with `query_ms`, `restore_ms`, `had_ids`.
- `restore_reload_anchor` logs one info line `restore split` with `ids_ms`
  (the `current_view_ids()` call, 0 when `resolved_ids` was given),
  `select_ms`, `apply_ms` (everything after the selection, i.e. the scroll
  placement).

These are the permanent form of the session's probes (mother §0) and what
the harness parser reads (G-B2, G-C). No behaviour change in B0.

### B1 — `changed_range` recognises a block move

`track_list_model_change.rs`:

- `ModelChange` gains `kind: ModelChangeKind` — `Span` (today's shape) or
  `BlockMove { from: u32, to: u32, len: u32 }`. `position/removed/added`
  keep describing the covering span, so the existing guard in
  `set_query_browsed_ai_inner` (generation, totals, bounds) needs no change.
- After the prefix/suffix trim: if the trimmed `before[prefix..before_end]`
  and `after[prefix..after_end]` have equal length, the changed ids form one
  contiguous run in each (same order), and removing that run from both leaves
  identical sequences → `BlockMove { from: prefix + run_start_before, to:
  prefix + run_start_after, len }`. Otherwise `Span`.
- Unit tests: eight contiguous rows moving up; moving down; to the very top
  (`first_mismatch = 0`, the F7–F11 case); a non-contiguous selection →
  `Span`; an edit that does not move → `None`; a run whose internal order
  changed → `Span`.

### B2 — the model emits the move over a consistent intermediate list

- `track_list_model_move.rs` (new, declared in `track_list/mod.rs`):
  `impl TrackListModel { pub(super) fn emit_block_move(&self, from, to, len, total) }`
  plus two pure helpers `intermediate_n_items(total, len)` and
  `intermediate_position(p, to, len)` (`p >= to ? p + len : p`), unit-tested.
- `ModelState` gains `pending_insert: Option<(u32 /*to*/, u32 /*len*/)>`.
  `n_items()` and `item()` consult it: while it is set, `n_items` answers
  `total - len` and `item(p)` reads the underlying model at
  `intermediate_position(p, to, len)`.
- `set_query_browsed_ai_inner`, for a `BlockMove` that passes the guard:
  state swap and cache clear as today, generation moves once as today, then
  `emit_block_move`: install the overlay → `items_changed(from, len, 0)` →
  remove the overlay → `items_changed(to, 0, len)`. Both emissions inside the
  same synchronous call; the overlay never outlives it. `query_section_change`
  (sections) runs as for a `Span` with the covering triple.
- Selection needs nothing new: `MultiSelection` keeps selected *objects*, the
  moved rows are new objects after the cache clear, and
  `select_captured_ids` re-selects them as after every reload.
- `track_list_model.rs` must end below 800 lines (it is at 790): move what
  B2 adds into the new module, and if that is not enough, move the queue
  snapshot part's remaining private helpers (not `set_queue_snapshot`'s
  public shape) — never anything into A's `queue_snapshot_change.rs`.

Tests (`track_list_model_tests.rs`, GTK-free where possible, else the
display-test style already used there): a `BlockMove` through
`set_query_browsed_ai_changed` records exactly two `ItemsChanged` events in
the diagnostic trail, `(from, len, 0)` then `(to, 0, len)`; `n_items()` and
`item()` between the two (assert via a connected `items-changed` handler)
report the intermediate list; a `Span` still records one event; the overlay
is `None` after the call.

### B3 — the flow asks for the move

`tag_edit_flow::finish_apply` and `tag_save_refresh::tag_save_model_change`:

- Today a sort-field edit ends in `refresh_after_tag_mutation_with_save_anchor`
  (no ids, `delta=false`, full reload) because `tag_save_model_change` answers
  `None` for it (its `before != after` guard). Relax that guard to: order
  unchanged → `changed_range` as today; order changed → only a `BlockMove`
  result passes, a reordered `Span` still answers `None` (today's full
  reload). Nothing else it refuses today becomes accepted.
- Route that result through the view-ids path
  (`refresh_after_tag_mutation_with_view_ids` or a sibling that also takes the
  viewport) with `ReloadViewport::PostSaveSortAnchor`, so the edited block
  stays in view exactly as today. `restore_reload_anchor` is unchanged; it
  already accepts `resolved_ids`, which saves the second sorted id query.
- `emit` (B0) reports `"move"` for this path.

Display test `tag_1_artist_save_on_contiguous_rows_emits_one_block_move`
(next to the existing `tag_1_*` tests): eight contiguous rows selected, artist
edited to a value that sorts first under artist sort; after the save the
diagnostic trail shows exactly two `ItemsChanged` events for that reload, the
eight rows are selected, and all eight are inside the viewport. The existing
`tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`,
`tag_1_tag_save_refresh_paints_no_frame_at_the_table_top`,
`tag_1_save_refresh_shows_the_written_tag_on_screen` and
`tag_1_save_refresh_requeries_the_view_once` must pass unchanged.

### B4 — housekeeping

Doc comments on `ModelChange`, `set_query_browsed_ai_inner` and the new
module state the two shapes and the overlay's lifetime; `track_list_model.rs`
below 800 lines; no `unwrap` on the u32 conversions (follow the existing
`try_from … .ok()?` style).

## Verification (scope per mother §2)

`cargo fmt`; `cargo clippy --all-targets --workspace -- -D warnings`;
`cargo test -p reprise-gnome track_list_model`, `… track_list_model_change`,
`… tag_save_refresh`; the display tests named in B3 plus the six
`ui::delete_tracks::*` and
`tag_1_restoring_dialog_focus_after_a_save_keeps_the_viewport`, one process
each, the way `scripts/check-display-tests.sh` runs a single test. Do NOT run
the full display suite, the Android suite, `cargo test --workspace`, or any
repo-wide gate script; AGENTS.md's gate instruction does not apply to this
strand run.

## §M — measurements (written by the session)

Release binary of this worktree at `57da9ab6e4` (after check + refactor),
harness `~/.local/share/reprise-measure-20260906`, runs SB1–SB3, machine
quiet. Control = C1–C3, fix reference = F9–F11 (the `*_ms` split fields did
not exist before this strand, so the fix column shows `reload_ms` only).

| gesture | metric | control | fix ref (F9–F11) | this strand (SB1–SB3) | gate |
|---|---|---|---|---|---|
| G4 tag-edit 8 genre | `emit` | – | – | metadata | – |
| G4 | `reload_work_ms` | – | – | **3 (2–5)** | G-B2 < 10 ✓ |
| G4 | `idle_wait_ms` | – | – | 185 (6–256) | (dialog close animation, not gated) |
| G4 | `reload_ms` | – | 169–201 | 189 (9–262) | – |
| G5 tag-edit 8 artist (sort field) | `emit` | – | – | move | G-B1 one block move ✓ |
| G5 | `reload_work_ms` | – | – | **32 (31–32)** | G-B1 < 80 ✓ |
| G5 | `query_ms` / `restore_ms` | – | 75–86 / 54–67 | 9 (9–9) / 22 (21–23) | – |
| G5 | `idle_wait_ms` | – | – | 183 (175–184) | – |
| G5 | `reload_ms` | – | 319–355 | 215 (208–218) | – |

Visual gate: `runs/SB1/9-after-G5.png` shows the eight edited rows
(`Zz Measured Artist`) selected and inside the viewport after the resort.
The remaining G5 time is `idle_wait_ms` (the wait behind the dialog close
animation) and the viewport jump in `restore_ms`, both outside this strand
(mother §6).

## Report

Codex, five commits + two refactor commits, `cargo clippy -D warnings`
clean, `track_list_model` 34 passed, `track_list_model_change` 8,
`tag_save_refresh` 6, twelve isolated display tests (five B3 tag-save tests,
six `ui::delete_tracks::*`, `tag_1_restoring_dialog_focus_after_a_save_keeps_the_viewport`)
passed. Review: 3 findings, 2 survived — the completion line reported
`emit="metadata" reload_work_ms=0` for saves that ran no deferred reload
(fields now omitted in that case, tested), and no emission-level test for a
downward block move (added, mutation-checked against `>=`→`>`).
