---
slug: up-next-and-resort-reload-pay-for-the-change-only
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-07
strands: a,b
merge_order: a,b
---
# The Up Next panel and the resort reload pay for the change only

Follow-up to `deleting-and-tag-saving-stop-paying-on-the-main-thread.md`
(landed as #849/#850/#851). Its handover (2026-09-07) left three things
behind: the deferred Now-Playing refresh at ~40 ms per queue change, the G5
sort-field reload at ~340 ms, and a harness parser that does not read
`now_playing_deferred_ms`. This plan takes all three.

One preparatory commit (P0), then two strands: **A** (the Up Next panel after
a queue change) and **B** (the track list after a sort-field save). Strand
files: `…-a.md`, `…-b.md`. Merge order A → B; the strands are independent.
The harness parser is the session's own task (§5), not a strand.

## 0. What was measured (2026-09-07, throwaway probes, real library mirror, 1 941–1 986 rows, artist sort)

Binary: `~/Projects/reprise-dev-postmerge` at dev `62aaf7b214` plus timing
probes that were never committed (the two patch scripts are kept in the
harness directory under `probes/`, with a README). Runs F8–F11 of the harness in
`~/.local/share/reprise-measure-20260906/` (`measure-debug.sh`, a copy of
`measure.sh` whose `REPRISE_LOG` is overridable). A foreign Codex held all
`heavy-run` slots during the runs, so absolute numbers sit ~10 % above the
handover's; the *split* is what this plan builds on.

### The deferred Now-Playing refresh (`now_playing_deferred_ms`, 39–48 ms per delete)

| step inside `UpNextPanel::set_queue_model` | ms |
|---|---|
| `model.upcoming()`, section headers | 0 |
| `TrackListModel::set_queue_snapshot` → `items_changed(0, 1940, 1939)` | **37–46** |
| footer duration, 10 windows × `query_queue_duration_ms` | 2–4 |

The whole cost is one full-range `items_changed`. It is full-range because
`PlayQueue::remove_ids` bumps the sequence revision, so
`QueueViewModel::leading_removal_change_from` sees a different context
identity and gives up; `queue_snapshot_change` then falls back to
`(0, old_total, new_total)`. The DB is not it (the one window refetch after
the cache clear costs ~1 ms); the time is GTK's own per-item work on the
range — the `ReloadBreakdown` of the analogous track-list case counts 4 149
`item()` calls for 1 972 rows, i.e. the `gtk::MultiSelection` and the view
each walk the whole range (inference from the call count, not a profile).

### The tag save (`reload_ms`, G4 Genre 169–201 ms, G5 Artist 319–355 ms)

| part of `reload_ms` | G4 (delta, `metadata_only`) | G5 (full resort) |
|---|---|---|
| synchronous: `current_view_ids()` + change computation + cover/browse/metadata refresh | 5–10 | 5–8 |
| **idle wait** (`idle_add_local_once` scheduled → fired) | **166–198** | 7–203 |
| idle work: `run_query` | – | **75–86** |
| of which `items_changed(0, 1972, 1972)` (`ReloadBreakdown.items_changed_us`) | – | 77 |
| of which the count query / state swap / geometry | – | 0.1 / 0.2 / 2–4 |
| of which `item()` + window queries (4 149 `item` calls, 4 windows) | – | 3 |
| idle work: `restore_reload_anchor` | – | **54–67** |
| of which `current_view_ids()` + `select_captured_ids` | – | 0 + 0 |
| of which `schedule_post_save_sort_anchor` → `apply` (viewport moves 46 305 → 86 058 px) | – | ~55 |

Two conclusions:

1. **G4 is already fine.** Its 170–200 ms is idle starvation while the tag
   editor dialog animates shut; the main thread is not blocked and the idle's
   own work is 2–3 ms. The metric `reload_ms` conflates wait and work — a
   reporting bug, not a performance one (task B0).
2. **G5's real work is ~140 ms in two halves.** 77 ms is GTK walking a
   full-range `items_changed` twice for a change that moved eight contiguous
   rows. ~55 ms is the viewport jump to the edited block, i.e. rebinding ~22
   rows at a new place; that half is the price of the jump the user asked for
   and is only instrumented here (§6).

## 1. Goals

- **G-A1** After deleting one non-loaded track (harness G1), the deferred Up
  Next refresh costs < 10 ms (`now_playing_deferred_ms`, median of 3 runs;
  was 39–48).
- **G-A2** Deleting 13 rows including the loaded one (G3) emits no full-range
  `items_changed` on the Up Next model — the emitted span covers at most the
  removed rows' extent (pinned by unit tests on the emitted triple; the
  harness reports the refresh time, not gated).
- **G-B1** A multi-select save that edits the sort field on contiguous rows
  (G5) emits one block move (`items_changed(from, n, 0)` then
  `items_changed(to, 0, n)`) instead of `items_changed(0, total, total)`;
  `reload_work_ms` (the idle's own time, new field) < 80 ms (was ~140).
- **G-B2** The tag-edit completion line separates `idle_wait_ms` from
  `reload_work_ms`; G4's `reload_work_ms` stays < 10 ms.
- **G-C** `parse.py`/`aggregate.py` report `now_playing_deferred_ms` per
  delete gesture and `idle_wait_ms`/`reload_work_ms`/`emit` per tag gesture.
- Every existing display test that pins these paths still passes, in
  particular `tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`,
  the six `ui::delete_tracks::*`, and
  `nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one`.

## 2. Rules that bind every task

- **Harness numbers are the session's to obtain**, exactly as in the mother
  plan before: Codex proves behaviour with unit and display tests; the session
  builds the strand's release binary, runs `measure.sh <arm> <label>` three
  times per arm, and writes the numbers into the strand file's `## §M`
  section. Codex never runs the harness (it lives outside the repo and needs
  Xvfb and the library mirror).
- **No new full-range emission anywhere.** A task that cannot express a change
  narrowly keeps *today's* fallback; it must not add a new one.
- **The intermediate list must be consistent.** Between two `items_changed`
  emissions GTK re-reads `n_items()` and `item()`; whatever overlay serves the
  intermediate state is installed before the first emission and removed
  before the second, inside the same synchronous call.
- **A hint is trusted only against its base.** Any change hint that rides on
  a snapshot names the identity it was computed from; the consumer applies it
  only when its own previous snapshot has that identity, otherwise today's
  fallback. A missed snapshot (panel hidden, refresh skipped) must degrade to
  full range, never to a wrong span.
- **Gated codepaths get a unit test that would fail on the old shape.** The
  narrow emission is asserted on the emitted triple, not on timing.
- **Verification scope for Codex** (this overrides AGENTS.md's "run the gate
  before committing" for strand runs — deliberate): `cargo fmt`, `cargo
  clippy --all-targets --workspace -- -D warnings`, `cargo test -p <crate>`
  for the crates the strand owns, and the named display tests one process
  each the way `scripts/check-display-tests.sh` runs a single test (read it,
  do not run it whole). Not the Android suite, not the full display suite,
  not the repo-wide gate scripts.

## 3. Strands and file ownership

### P0 — preparation, before the worktrees fork (Codex on a branch off dev, landed with `land.sh --no-plan`)

Move the two pure functions `queue_snapshot_change` and
`queue_snapshot_emissions` from `ui/track_list/track_list_model.rs` into a new
`ui/track_list/queue_snapshot_change.rs` (declared in `track_list/mod.rs`),
together with their tests from `track_list_model_tests.rs`
(`queue_snapshot_emissions_*`). `query_section_change` stays where it is (it
belongs to the query path, strand B). No behaviour change; the two call sites
in `set_queue_snapshot` are re-pointed. This is what makes the seam A's:
without it A could neither narrow the sections signal nor be sure of the
emitted triple.

### Strand A — the Up Next panel pays for the removed rows only

Owns (after P0):
- `crates/reprise-view/src/queue.rs` and its tests
- `crates/reprise-gnome/src/ui/playback/queue_transport_projection.rs` (+ its tests)
- `crates/reprise-gnome/src/ui/track_list/queue_snapshot_change.rs` (+ its tests)
- `crates/reprise-gnome/src/ui/playback/instrumentation.rs` (+ tests), only if a log field is added
- `crates/reprise-gnome/src/ui/playback/player_controller.rs`, only the field that holds A1's last composed tail and its initialisation (added 2026-09-07 after Codex stopped at the missing owner — correctly)

Does **not** touch `reprise-core`, `track_list_model.rs`, `up_next_panel.rs`
or any `tag_edit`/other `track_list` file.

**A1 — the projection diffs the tail it hands out.** `queue_view_model()`
keeps the last tail it composed: `(sequence_identity, start, Vec<i64> tail
ids)` from `remaining_window(0, remaining_len())`. On the next call it
computes the new tail, trims the common prefix and suffix against the old one
(the `changed_range` idea, ids instead of positions), and — when the old
identity is known and anything changed — attaches
`TailChange { base: (old_sequence, old_start), position, removed, added }`
in tail coordinates to the `VirtualContext`. One covering span, never several
emissions: scattered removals give `(first, last-first+1, last-first+1-n)`,
contiguous ones `(p, n, 0)`, the current track being removed shows up as the
tail losing its head `(0, 1, 0)` combined with the rest. A reorder gives a
wide span, which is what a reorder costs. The projection logs one info line
`queue tail change` with `position, removed, added, tail_len` whenever it
attaches a hint.

**A2 — the view model takes the hint against its base.** `VirtualContext`
gains `change_from_previous: Option<TailChange>` (constructor
`identified_with_change`). `leading_removal_change_from(old)` keeps its two
existing shapes and adds a third, tried last: `old.items == self.items`
(materialized prefix unchanged), `old.context.identity == Some(hint.base)`,
and `old.count - removed + added == self.count` → return
`(items.len() + position, removed, added)`. Every other combination returns
`None` as today (a Play Next entry among the deleted rows changes the prefix
and stays full-range — §6). Unit tests: contiguous removal, scattered
removal, head removal, base mismatch → `None`, prefix changed → `None`,
count inconsistent → `None`.

**A3 — the sections signal follows the span.** `queue_snapshot_emissions`
today turns every narrow change with `sections_changed` into
`sections_changed(0, new_total)`. Rows before the change keep their section;
the rows from `position` on are the ones whose header tile may be dropped
(the `examples/queue_section_shift_repro.rs` case is a removal at a section
start). So: narrow `sections_changed` to `(position, new_total - position)`
when the items change is narrow; keep `(0, new_total)` for the full-range
shape. Tests: the existing `queue_snapshot_emissions_*` plus "removal at a
section start re-sections from there" and "removal in the middle of the last
section re-sections its tail only".

### Strand B — a sort-field save moves the block instead of reloading the list

Owns:
- `crates/reprise-gnome/src/ui/track_list/track_list_model.rs`, a new
  `track_list_model_move.rs` next to it (the file is at 790 lines; the overlay
  goes into the new module), and their test files
- `crates/reprise-gnome/src/ui/track_list/track_list_model_change.rs`
- `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`
- `crates/reprise-gnome/src/ui/track_list/tag_mutation_refresh.rs` and its display tests
- `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`
- `crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs`, `tag_save_refresh.rs` (+ tests)
- `crates/reprise-gnome/src/ui/track_list/mod.rs` (the one `mod track_list_model_move;` line)

Does **not** touch `queue_snapshot_change.rs`, `reprise-view`, or anything
under `ui/playback`.

**B0 — the completion line tells wait from work.** `tag_mutation_refresh`'s
idle records `Instant::now()` when scheduled and when fired; the `tag-edit
batch completed` line gains `idle_wait_ms`, `reload_work_ms` (the idle
closure's own duration) and `emit` (`"metadata"`, `"span"`, `"move"`,
`"full"`); `reload_ms` and `delta` stay for continuity.
`reload_with_anchor_and_viewport` logs `query_ms`/`restore_ms` on an info
line `reload split`, and `restore_reload_anchor` logs
`ids_ms`/`select_ms`/`apply_ms` — the permanent form of §0's probes.

**B1 — `changed_range` recognises a block move.** `ModelChange` gains
`kind: ModelChangeKind` with `Span` (today's shape) and
`BlockMove { from: u32, to: u32, len: u32 }`. After the prefix/suffix trim:
if the trimmed `before` and `after` slices have equal length, the changed ids
form one contiguous run in each, and removing that run from both leaves
identical sequences, the result is `BlockMove` (`position/removed/added`
still describe the covering span for the existing guard). Scattered edits
keep `Span`. Unit tests: eight contiguous rows moving up; moving down; to the
very top (F7–F11's case, `first_mismatch=0`); a non-contiguous selection →
`Span`; an edit that does not move → `None`.

**B2 — the model emits the move over a consistent intermediate list.**
`set_query_browsed_ai_inner` keeps its guard (generation, totals, bounds).
For `BlockMove`, after the state swap and cache clear, it installs
`pending_insert: Option<(u32 /*to*/, u32 /*len*/)>` in `ModelState`, emits
`items_changed(from, len, 0)` — while `n_items()` answers `total - len` and
`item(p)` maps `p >= to ? p + len : p` — clears the overlay, then emits
`items_changed(to, 0, len)`. The overlay and the two mapping helpers live in
`track_list_model_move.rs` and are unit-tested as pure functions. The
generation moves once, before the first emission, as today. Selection needs
nothing new: `MultiSelection` keeps selected *objects*, the moved rows are
new objects after the cache clear, and `select_captured_ids` re-selects them
as after every reload.

**B3 — the flow asks for the move.** `tag_edit_flow::finish_apply` routes a
sort-field edit to `refresh_after_tag_mutation_with_save_anchor` today (no
ids, `delta=false`); `tag_save_model_change` answers `None` for it. Now a
sort-field edit with a `BlockMove` result goes through the view-ids path with
the `PostSaveSortAnchor` viewport (the anchor logic in `restore_reload_anchor`
is unchanged; it already accepts `resolved_ids`, which also saves the second
sorted id query). Display test:
`tag_1_artist_save_on_contiguous_rows_emits_one_block_move` — eight
contiguous rows, artist edited to a value that sorts first, assert the
diagnostic trail shows exactly the two emissions and the eight rows are
selected and inside the viewport afterwards. The existing
`tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`
must pass unchanged.

**B4 — housekeeping.** Doc comments on `ModelChange` and
`set_query_browsed_ai_inner` state the two shapes; `track_list_model.rs` ends
below 800 lines.

## 4. Merge order

P0 first, on dev, before any strand worktree exists. Then A, then B. Nothing
in B depends on A; the order only fixes which strand rebases. Both carry this
mother plan unchanged.

## 5. Post-merge cross-checks and session tasks

1. **Harness parser (session, before the strands' §M runs).** Extend
   `parse.py` to read `now_playing_deferred_ms` from the `queue listeners
   deferred` line that follows each `delete batch completed`, the `queue tail
   change` triple, and `idle_wait_ms`/`reload_work_ms`/`emit` from `tag-edit
   batch completed`; extend `aggregate.py` accordingly (G-C).
2. **Harness acceptance on merged dev (session).** Three `fix` runs against
   the control C1–C3 and the fix F4–F6 tables: G-A1 (< 10 ms), G-B1
   (`reload_work_ms` G5 < 80), G-B2 (G4 < 10); the previous plan's gates must
   still hold (G1 purge→completed < 30 ms, G3 `mutated_ms` < 10 and
   `advance_ms` < 20, G4 `emit` not full, G5 rows in viewport).
3. Rename `leading_removal_change_from` → `change_from` (one commit,
   `land.sh --no-plan`).
4. `cargo clippy --all-targets --workspace -- -D warnings`; the named display
   tests one process each; `check-architecture.sh`, `check-ux-traceability.sh`;
   the dev CI run that contains both merges.
5. `measure-debug.sh` stays in the harness directory as its documented debug
   entry; the handover names it.

## 6. Leftovers this plan does not take

- The ~55 ms of `restore_reload_anchor` on G5 is the viewport jump to the
  edited block (rebinding ~22 rows at a new place, cover thumbnails included).
  B0 makes it visible as `apply_ms`; whether it holds anything unnecessary is
  a decision for after this plan's numbers.
- The idle starvation during the tag editor's close animation (170–200 ms)
  is libadwaita's dialog transition; the reload is *supposed* to wait for it
  (see the comment in `tag_mutation_refresh`).
- R1 from the previous plan (loaded-track reload 56 ms on G2) — unchanged.
- Deleting a track that sits in Play Next changes the materialized prefix,
  which A2's third shape rejects; that case keeps today's full range. Rare;
  noted, not taken.
- Scattered deletions far apart pay the covering span (one emission by
  decision, grill Q3).

## 7. Grill record (2026-09-07)

1. Strand B scope: block move (B0–B4), not metric-only.
2. Strand A mechanism: the projection diffs the tail ids it hands out; no
   `reprise-core` bookkeeping.
3. Scattered removals: one covering span, no multi-emission overlay.
4. Seam: P0 moves `queue_snapshot_change`/`queue_snapshot_emissions` into
   their own module before the fork, so A owns the sections signal.
5. Gates: G-A1 < 10 ms, G-B1 < 80 ms, G-B2 G4 < 10 ms, G-A2 on the triple.
6. Cut: two strands, merge order A → B.
7. The ~55 ms viewport jump: instrumented only (B0), not attacked.

## Parallelität

P0 is sequential by design (it changes B's file to create A's). After it,
two strands with disjoint file groups as listed in §3:

- **A** owns `reprise-view/src/queue.rs`, `ui/playback/queue_transport_projection.rs`,
  `ui/track_list/queue_snapshot_change.rs`, `ui/playback/instrumentation.rs`
  and their tests. It changes no track-list source other than
  `queue_snapshot_change.rs` and no tag-edit file.
- **B** owns `ui/track_list/{track_list_model,track_list_model_move,track_list_model_change,track_list_reload,tag_mutation_refresh,reload_anchor_scroll,mod}.rs`,
  `ui/tag_edit/{tag_edit_flow,tag_save_refresh}.rs` and their tests. It
  changes no queue or playback file.

The seam is `set_queue_snapshot` in `track_list_model.rs` (B's file) calling
`queue_snapshot_change::*` (A's file) with an unchanged signature — A widens
what those functions return, B does not touch the calls. `track_list/mod.rs`
is B's; P0 already carries A's `mod queue_snapshot_change;` line, so A needs
no edit there.

Merge order A → B. Cross-strand comparisons moved to §5: the harness runs
(both strands' gates read the same `app.log`), the rename, clippy over the
merged tree, the CI run.
