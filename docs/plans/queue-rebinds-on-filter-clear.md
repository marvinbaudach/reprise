---
slug: queue-rebinds-on-filter-clear
worktree: /home/marvin/Projects/reprise-queue-rebinds-on-filter-clear
branch: feature/queue-rebinds-on-filter-clear
phase: planned
codex_session:
created: 2026-08-26
---
# Clearing the filter rebinds a running snapshot to the whole library

Spec: `docs/superpowers/specs/2026-08-26-queue-follows-the-filtered-view-design.md`

## Goal

Today a snapshot born in a filtered Music library gets the whole library back
**only once it has run dry** (PLAY-11). Remove that exhaustion precondition:
with titles still ahead, clearing the filter rebinds the queue to the
now-unfiltered visible list, the running title keeping the cursor and never
restarting.

Nothing else about the snapshot principle moves. Narrowing a filter, swapping a
chip, typing in the search field, changing the sort — all still leave a running
queue alone (PLAY-3b, PLAY-8, unchanged).

## Why this shape

The broad reading — "the queue always mirrors the visible list" — would replace
PLAY-3b, PLAY-8 and PLAY-11 at once, and PLAY-8 is `[core]`, so it would reach
`reprise-runtime` and `reprise-android-ffi` too. Narrowed to the one transition
actually asked for, the change lands inside machinery that already exists for
PLAY-11 and touches exactly two files plus the rulebook.

## What is already there — verified, do not rebuild

- **The trigger.** `window.rs:170-186` already calls
  `player.continue_library_after_filter_clear()` on exactly the reload where a
  Library view has become filter-free. No change needed there.
- **The two gates.** `cleared_filter_origin` (`library_continuation.rs:247`) —
  was the snapshot born in a *filtered* library root? It is also the loop guard:
  a successful continuation rewrites `play_origin` to the unfiltered library, so
  the reload it causes cannot re-trigger it. `cleared_library_filter_handoff`
  (`:48`) — is the view now the whole live library? Measured by **row count**,
  not by reading filter state and not by id list, since the id query stops at
  `QUEUE_LIMIT`.
- **The id source.** `view_refill_ids` on `PlayerController`
  (`player_controller.rs:311,548`) → `VisibleView { ids, total }`.
- **The queue write that does not restart playback.** The bound PLAY-11 arm uses
  three lines (`library_continuation.rs:192-194`):

  ```rust
  self.queue.borrow_mut().set_tracks(ids, 0);
  *self.play_origin.borrow_mut() = Some(PlayOrigin::library());
  self.notify_queue_changed();
  ```

  It deliberately avoids `play_from_view`, which goes through `play_track_id` →
  `present_track(…, StartPlayback::Yes)` and would restart the title and end its
  listening session.
- **`set_tracks` already produces both orders we need** (`queue.rs:73-123`).
  Unshuffled: `order = 0..len`, `pos = start_index`. Shuffled: Fisher-Yates over
  the whole order, then the track at `pos` pulled to the front with `pos = 0` —
  which is exactly "running title at the cursor, everything else freshly
  shuffled". **No core change is needed. `reprise-core` is not touched by this
  plan.**

The single line in the way: `continue_library_after_filter_clear`
(`library_continuation.rs:149-156`) returns `false` on `remaining != 0`.

## Decisions taken in the grill

1. **Shuffle on → everything behind the running title is freshly shuffled**, not
   "survivors keep their order". Clearing the filter is meant to get you out of
   the filter, not to make you sit through its last 20 hits first. Already-played
   hits may recur — the same trade PLAY-11's existing arm makes.
2. **The ids are always the visible list**, capped at `QUEUE_LIMIT` (10 000)
   like every other queue build. Past 10 000 live titles the rebind queues the
   first 10 000 in view order — the same queue a click on a row would have
   produced. One id source, one code path.
3. **PLAY-11 stands.** Its behaviour genuinely does not change, so it keeps its
   `[active]` status, its nine `play_11_…` tests and its whole
   `scripts/cua-e2e/filter_clear_playback.sh` scenario set. PLAY-15 is added
   beside it.
4. **Rust tests only.** No cua-e2e scenario: `check-ux-traceability.sh` is
   satisfied by the `fn play_15_…` names, and the display suite runs manually.

Inherited from PLAY-11 and deliberately not changed: the rebind fires while
**paused** (the guard is "a title is loaded", not "is playing") and never fires
while the FIL-7 AI exclusion is on, because an AI-filtered view is never "the
whole library" by row count (`library_continuation.rs:39-40`).

## Task 1 — the rebind arm as a pure function

`library_continuation.rs` is **584 lines**; a function plus seven tests breaches
the 800-line rule. New sibling
`crates/reprise-gnome/src/ui/playback/library_continuation_rebind.rs`, declared
from `library_continuation.rs` and re-using its two gate helpers.

First step, before writing tests: read `cleared_library_filter_handoff`
(`library_continuation.rs:48`) and confirm its exact signature and what it needs
from the caller. The plan assumes it can be called with the same arguments the
existing arms pass it.

Then test-first, in that file's own `#[cfg(test)] mod tests`, modelled on the
`immediate_library_continuation` tests (pure functions, no GTK, no display).
Write all seven, run them, watch them fail:

1. `play_15_clearing_the_filter_rebinds_a_snapshot_with_a_future` — titles
   ahead, filtered library origin, view now the whole library → returns the
   visible ids and the running title's index in them.
2. `play_15_rebind_requires_a_cleared_library_filter` — origin was a playlist or
   album → `None`. This is also what keeps a queue started from MPRIS or a
   file-manager double-click out: `PlayOrigin::library()` carries an empty
   track state, and `cleared_filter_origin` demands a non-empty one.
3. `play_15_rebind_stays_shut_on_a_capped_view_that_is_still_filtered` — the
   row-count gate's own refusal, modelled on
   `play_11_a_capped_view_that_is_still_filtered_stays_shut` (`:426`). Note that
   a merely *narrowed* view is stopped one level earlier and by a different
   mechanism: `window.rs:170-186` only calls the entry point when
   `filter.trim().is_empty() && browse.is_empty()`, so it never reaches this
   function at all. Do not write a test that attributes that guarantee to the
   handoff gate.
4. `play_15_rebind_never_overrides_repeat` — `Repeat::One` and `Repeat::All` →
   `None`. Same reasoning as the existing arm: they never reach the end this
   rule is about.
5. `play_15_rebind_keeps_the_running_title_at_the_cursor` — the returned index
   points at the running title; it is not moved, duplicated or dropped.
6. `play_15_rebind_of_a_library_larger_than_the_visible_id_cap_queues_the_cap` —
   `visible.ids` a 10 000-row prefix, `visible.total` larger: the rebind happens
   and queues the prefix. Modelled on
   `play_11_continues_a_library_larger_than_the_visible_id_cap` (`:396`), where
   that asymmetry was first made deliberate.
7. `play_15_rebind_needs_the_running_title_to_be_in_the_new_list` — the current
   id absent from `visible.ids` → `None`, nothing is rewritten.

Then implement:

```rust
fn rebind_to_unfiltered_view(
    origin: Option<&PlayOrigin>,
    repeat: Repeat,
    remaining: usize,
    current_track_id: Option<i64>,
    visible: &VisibleView,
) -> Option<(Vec<i64>, usize)>
```

Returns the visible ids together with the running title's index in them — that
index is what `set_tracks` needs, and it serves both orders: unshuffled it
becomes `pos`, shuffled it selects the track that gets pulled to the front.

`None` on any of: `repeat != Repeat::Off`, `remaining == 0` (that case belongs to
the existing arms), no current track, an origin that is not a cleared filtered
library root, a handoff gate that stays shut, or a current id missing from
`visible.ids`.

Gate for this task: `cargo test -p reprise-gnome library_continuation`.

## Task 2 — wire the arm into the entry point

`continue_library_after_filter_clear` (`library_continuation.rs:149`) stops
bailing on `remaining != 0` and dispatches instead: `remaining == 0` to today's
path, byte-for-byte unchanged; `remaining > 0` to `rebind_to_unfiltered_view`,
then the same three lines the bound arm already uses —
`set_tracks(ids, start_index)`, `play_origin` rewritten to
`PlayOrigin::library()`, `notify_queue_changed()`.

Keep `repeat != Repeat::Off` as an early return for both paths; only the
`remaining` half of that guard splits.

RefCell discipline (AGENTS.md's #1 recurring panic class): copy `repeat`,
`remaining` and the current track id out in their own statements, and hold no
`Ref`/`RefMut` on `queue` or `play_origin` across `notify_queue_changed()` —
that call re-enters GTK. The existing arm's borrow pattern is the model; follow
it exactly rather than inventing a second one.

Test-first, in `library_continuation_rebind.rs`:

8. `play_15_a_rebind_rewrites_the_origin_so_the_reload_it_causes_cannot_rebind_again`
   — the loop guard, asserted rather than assumed.

Then confirm, without editing, that `window.rs:170-186` already reaches the new
path, and say so in the commit message. A "no change needed" nobody recorded
reads later like a step someone forgot.

Log line: distinguish the two arms, so a support log says which one fired.

## Task 3 — the rules

Read `docs/ux-rules.md` section C before editing. PLAY-14 is the highest ID in
use; PLAY-15 is free.

Add, `[active]` in this same commit per the append-only contract:

> **PLAY-15** `[active]` `[gtk]` — A snapshot born in a filtered Music library is
> rebound the moment that view becomes completely unfiltered again, even while it
> still has titles ahead. The running title keeps the cursor and is never
> restarted; the now-unfiltered visible list follows it — in the view's own
> order, or freshly shuffled behind the running title when shuffle is on. The
> exhausted case stays PLAY-11's. Any other filter change — narrowing, swapping a
> facet, typing in the search field — leaves the snapshot alone, as PLAY-3b and
> PLAY-8 require. Repeat One/All are never rebound; Missing and deleted titles
> are excluded.

Then:

- PLAY-11 keeps `[active]` and its sentence "clearing a filter never rewrites a
  snapshot with a future" gains "except as PLAY-15 provides".
- PLAY-3b and PLAY-8 gain the same cross-reference. PLAY-8 already points at
  PLAY-11 for the exhausted case, so it is a one-clause amendment.
- `play_11_immediate_continuation_leaves_a_queue_with_a_future_alone`
  (`library_continuation.rs:468`) asserts exactly what this plan removes. Rewrite
  it as a PLAY-15 test in this commit — that is the "tests are re-pointed"
  clause doing its work.
- Two other tests read as if they were PLAY-15 territory —
  `play_11_clearing_the_filter_binds_the_continuation_while_the_title_still_plays`
  (`:351`) and `play_11_bound_continuation_plays_every_title_once` (`:369`) — but
  both pass `remaining = 0`: there "while the title still plays" means the queue
  is already exhausted and only the audio is still running. Checked, not assumed.
  They stay put.

Gate for this task: `scripts/check-ux-traceability.sh`.

## Task 4 — full gate and ledger

From the repo root, in this order:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit                       # only RUSTSEC-2024-0436 is accepted; a new one = STOP
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
```

No core purity proof is needed — `reprise-core` is not touched.

Test-count baseline comes from the latest entry in
`.superpowers/sdd/progress.md`, never from AGENTS.md. Expected delta: **+8**
`reprise-gnome` tests — seven from Task 1 and one from Task 2. The ninth change
is a rename, not an addition:
`play_11_immediate_continuation_leaves_a_queue_with_a_future_alone` is inverted
and renamed to a `play_15_…` name, so it stays one test. Say "8 new, 1 renamed
and inverted" in the ledger line so the baseline delta reconciles.

Do not run the cua-e2e suite: `play-11-filter-clear` covers behaviour this plan
does not change, and it needs a display session.

Append one line to `.superpowers/sdd/progress.md` in the agreed format.

## Not in this plan

- Filter changes short of clearing, and sort changes. PLAY-3b and PLAY-8 stand.
- Any view but the Music library root — `cleared_filter_origin` already refuses
  playlists, albums, smart lists and podcasts.
- Any change to `reprise-core`, `reprise-runtime` or `reprise-android-ffi`.
- Any new user-facing control, setting or translatable string.
- Making the handoff gate aware of the FIL-7 AI exclusion. That is PLAY-11's
  shared helper and its own decision.

## Known consequence to state in the commit

`set_tracks` calls `note_sequence_changed()`, so play-history back targets from
before a rebind expire. `sequence_identity()` is read only by history
(`playback_history.rs:43`, `playback_history_transport.rs:80,111,141`,
`queue_transport_projection.rs:99`,
`reprise-android-ffi/src/playback_session/history.rs:102,108`,
`reprise-runtime/src/transport_history.rs:73`, `transport_controls.rs:157,200`),
which uses it to ask whether a remembered playhead is still valid in the current
play order. A rebind shifts indices, so bumping is the honest answer — and it is
what PLAY-11's existing arm already does on the same code path.

## Parallelität

**No cut. One strand.**

The attempt, on record: after the grill dropped the core method, the whole
change is one new file, one edit to its parent, and the rulebook. There is no
disjoint file group left to give a second strand.

Even the rulebook cannot be split off: `check-ux-traceability.sh` resolves
PLAY-15 against a test name Task 1 creates, so a strand owning only
`docs/ux-rules.md` could not go green before the merge **in principle** — the
exact failure mode that cost a whole strand in the Flathub wave.

Post-merge cross-checks: none — nothing here is split across branches.
