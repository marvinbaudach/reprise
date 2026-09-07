---
slug: up-next-and-resort-reload-pay-for-the-change-only-a
worktree: /home/marvin/Projects/reprise-up-next-and-resort-reload-pay-for-the-change-only-a
branch: feature/up-next-and-resort-reload-pay-for-the-change-only-a
phase: planned
codex_session:
created: 2026-09-07
---
# Strand A — the Up Next panel pays for the removed rows only

Mother plan: `docs/plans/up-next-and-resort-reload-pay-for-the-change-only.md`
(read §0–§2 first; §2's rules bind every task here). This strand starts from
a dev that already contains P0 (`queue_snapshot_change.rs` exists).

## Why

After a delete, the Up Next panel's deferred refresh costs 39–48 ms, all of
it one full-range `items_changed(0, old, new)` on the Up Next
`TrackListModel`, because the queue's sequence identity changes on every
removal and `leading_removal_change_from` then knows nothing narrower. GTK
walks the whole range twice (`MultiSelection` + view). The DB share is ~1 ms.

## File ownership

- `crates/reprise-view/src/queue.rs` and its test files
- `crates/reprise-gnome/src/ui/playback/queue_transport_projection.rs` (+ its tests)
- `crates/reprise-gnome/src/ui/track_list/queue_snapshot_change.rs` (+ its tests)
- `crates/reprise-gnome/src/ui/playback/instrumentation.rs` (+ tests), only if a log field is needed
- `crates/reprise-gnome/src/ui/playback/player_controller.rs` — only the one field that holds the last composed tail (A1) and its initialisation; nothing else in that file

Not this strand's: `reprise-core` (no queue bookkeeping — grill Q2),
`track_list_model.rs`, `up_next_panel.rs`, `track_list/mod.rs`, anything under
`ui/tag_edit`. The list is a statement of ownership, not a fence: if a task
cannot be met without touching another file, stop and say so in
`.pipeline-codex.md` rather than guessing.

## Tasks

### A1 — the projection diffs the tail it hands out

`queue_transport_projection.rs::queue_view_model()`:

- Keep the last composed tail in the controller (a `RefCell<Option<LastTail>>`
  field on `PlayerController`, declared and initialised in `player_controller.rs`)
  with `sequence: (u64, u64)`, `start: usize`, `ids: Vec<i64>`), taken from
  `queue.remaining_window(0, queue.remaining_len())` at the moment the view
  model is composed.
- On every call compute the new tail ids, and when a previous tail exists,
  trim the common prefix and suffix (ids, not positions — the same shape as
  `track_list_model_change::changed_range`, which is B's file: do not import
  it, a ten-line local trim is fine). If anything differs, attach
  `TailChange { base: (old_sequence, old_start), position, removed, added }`
  in tail coordinates to the `VirtualContext` via
  `VirtualContext::identified_with_change(..)` (A2). One covering span,
  never several: scattered removals give `(first, last-first+1,
  last-first+1-n)`; contiguous ones `(p, n, 0)`; the current track being
  removed appears as the tail losing its head; a reorder gives a wide span.
- Store the new tail as the last one *after* composing, every time — also
  when no previous tail existed (first call).
- Log one info line `queue tail change` with fields `position, removed,
  added, tail_len` whenever a hint is attached (this is what the harness
  parser reads for G-A2; do not log when nothing changed).

Tests (projection tests file): a removal in the middle attaches `(p, n, 0)`;
two separated removals attach the covering span; removing the current track
attaches a head removal; the very first call attaches nothing; a call with
an unchanged queue attaches nothing.

### A2 — the view model takes the hint against its base

`reprise-view/src/queue.rs`:

- `pub struct TailChange { pub base: (u64, u64) /*sequence*/, pub base_start: usize, pub position: usize, pub removed: usize, pub added: usize }`
  (shape as you see fit; the base must identify the old `VirtualContextIdentity`
  fully — sequence and start).
- `VirtualContext` gains `change_from_previous: Option<TailChange>`;
  constructor `identified_with_change(count, sequence, start, change)`.
  `identified(..)` keeps `None`. Equality/`Clone` derive as before.
- `leading_removal_change_from(old)` keeps its two existing shapes untouched
  and adds a third, tried last:
  `old.items == self.items` (materialized prefix unchanged)
  ∧ `old.context.identity == Some(identity_of(hint.base))`
  ∧ `old.count - hint.removed + hint.added == self.count`
  → `Some((items.len() + position, removed, added))` (u32-checked like the
  existing code). Anything else stays `None`.
- Update the doc comment: the function now returns three exact shapes; its
  name is renamed in a follow-up after both strands land (mother §5.3), not
  here.

Tests: contiguous removal; scattered removal (covering span); head removal;
base identity mismatch → `None`; materialized prefix changed → `None` (a
Play Next row deleted); count inconsistent → `None`; the two old shapes'
tests unchanged.

### A3 — the sections signal follows the span

`queue_snapshot_change.rs::queue_snapshot_emissions`: when the items change
is narrow (not `covers_every_row`) and `sections_changed`, emit
`sections_changed(position, new_total - position)` instead of
`(0, new_total)`. Rows before `position` keep their section; the header tile
that GTK may drop is at or after `position` (the
`examples/queue_section_shift_repro.rs` case is a removal at a section start).
The full-range shape keeps today's behaviour (no sections signal, the items
signal already re-matches every header).

Tests: the existing `queue_snapshot_emissions_*` adjusted where the expected
range narrows; "removal at a section start re-sections from there";
"removal in the middle of the last section re-sections its tail only";
"full range still emits no sections signal".

### A4 — proof on the triple

In `queue_snapshot_change.rs` tests: a `QueueViewModel` pair built with A2's
hint yields `(prefix + p, n, 0)` from `queue_snapshot_change`; the same pair
without the hint yields the full range. This is G-A2's unit-test gate.

## Deviations

A3 was reverted to the full `(0, new_total)` sections range. The emitter
cannot see section starts, and an A2 tail hint may begin after the header row
of the section whose length changed. A safe post-merge narrowing requires
`set_queue_snapshot` to pass the section start containing `position`; that
follow-up belongs to the other strand's `track_list_model.rs`.

## Verification (scope per mother §2)

`cargo fmt`; `cargo clippy --all-targets --workspace -- -D warnings`;
`cargo test -p reprise-view`; `cargo test -p reprise-gnome queue_` (the
projection, snapshot-change and instrumentation tests); the display tests
`ui::delete_tracks::` (six, one process each) and
`nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one`, run
the way `scripts/check-display-tests.sh` runs a single test. Do NOT run the
full display suite, the Android suite, `cargo test --workspace`, or any
repo-wide gate script; AGENTS.md's gate instruction does not apply to this
strand run.

## §M — measurements (written by the session)

_(filled in after the code phase: worktree release binary, runs A1–A3 of the
harness, G-A1 against F4–F6 and C1–C3, the `queue tail change` triples per
gesture.)_

## Report

_(Codex's summary from `.pipeline-codex.md`, condensed by the session.)_
