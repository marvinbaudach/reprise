---
slug: deleting-and-tag-saving-stop-paying-on-the-main-thread-a
worktree: /home/marvin/Projects/reprise-deleting-and-tag-saving-stop-paying-on-the-main-thread-a
branch: feature/deleting-and-tag-saving-stop-paying-on-the-main-thread-a
phase: planned
codex_session:
created: 2026-09-06
---
# Strand A — delete finish: the queue purge and the advance leave the delete frame

Strand A of `docs/plans/deleting-and-tag-saving-stop-paying-on-the-main-thread.md`.
Read the mother plan's §0 (what the code does), §1 (goals G1–G3, R1) and §2
(rules) first; every rule in §2 binds every task here.

Runs in two Codex passes (mother §2 R-two-pass): pass 1 = A0 and A1 only, then
the session measures and writes §M below; pass 2 = A2–A4 against the numbers.

## File ownership

- Owns: `crates/reprise-gnome/src/ui/playback/**`,
  `crates/reprise-gnome/src/ui/delete_tracks*.rs`,
  `crates/reprise-gnome/src/ui/mpris_mirror.rs`,
  `crates/reprise-gnome/src/ui/scan/scan_watcher.rs`,
  `crates/reprise-gnome/src/ui/window/window_action_wiring.rs`,
  `crates/reprise-gnome/src/ui/releases/releases_presentation.rs`,
  `crates/reprise-core/src/queries/maintenance.rs`,
  `crates/reprise-core/src/artist_news.rs`, `crates/reprise-core/src/artist_news_view.rs`.
- Reads but never edits: `crates/reprise-gnome/src/ui/track_list/**` (the
  delete display tests are the guard, not the subject), `ui/sidebar/**`.
- Does not touch `ui/tag_edit/**` (strand B).

## Pass 1 — instrumentation

### A0 — `notify_queue_changed` shows its phases

In `queue_transport.rs::notify_queue_changed`, time the three phases and put
them on the existing `up next changed` line as `mirror_ms`, `listeners_ms`,
`feed_ms` (u128 milliseconds, like `mutated_ms`). Do not change behaviour.
Acceptance: the line carries the three fields; `cargo test -p reprise-gnome
queue` green.

### A1 — `advance_common` and `present_queue_item` show their phases

In `up_next_transport.rs::advance_common`: `live_ids_ms` (both liveness
queries together), `target_ms` (the in-memory search), `present_ms`
(`present_queue_item`). Inside `present_queue_item`, one more level if it has
distinguishable steps (player load, current-track notification). Put them on
an existing `info` line of the advance, or add one `info` line
`playback advanced` with `reason` if none exists. Acceptance as A0.

**Pass 1 ends here.** The session builds the worktree's release binary, runs
`measure.sh fix` on it and `aggregate.py`, and writes the table into §M.

## Pass 2 — move or replace by threshold (mother §2 R-threshold)

### A2 — the ≥ 5 ms phases of `notify_queue_changed` leave the frame

Expected mover: `feed_next`. Apply mother §2 R-feed exactly: synchronous
`set_next(None)` when the pre-fed item is among the removed ids; the feed
itself in `glib::idle_add_local_once`, coalesced through a "feed pending"
`Cell<bool>` on the transport; every caller of `notify_queue_changed` gets the
deferred feed, the delete path gets no special case. If the listeners or the
mirror measured ≥ 5 ms, they move too, with the same idle; the sidebar
queue-count listener may share the deferred idle `finish()` already uses for
the sidebar refresh.
Acceptance: `mutated_ms` < 10 ms on every gesture in the harness (G1);
`nav_10b…` green after the commit; the queue and gapless tests green; a unit
test that a purge of the pre-fed next id clears `set_next` synchronously.

### A3 — the ≥ 5 ms phases of the advance

Expected: `live_ids_ms`. If it is ≥ 5 ms, replace `query_live_track_ids` in
`advance_common`, `feed_next` and `start_current_item` by a point query
`query_track_is_live(conn, id)` (`SELECT 1 FROM tracks WHERE id = ? AND
<PRESENT>`) in `crates/reprise-core/src/queries/maintenance.rs`, evaluated
per item the target search actually tests. Keep `query_live_track_ids` for
its other callers. Mother §2 R-equivalence test in `maintenance.rs`'s tests.
If `present_ms` carries the cost instead, defer what is not the player load
and does not feed the glide, per R-threshold.
Acceptance: `advance_ms` < 20 ms for the 13-row gesture (G2); total span for
one non-loaded row < 30 ms (G3).

### A4 — housekeeping

- `ui/scan/scan_watcher.rs:233`
  `catalog_deletion_has_its_own_sidebar_refresh_before_queue_purge` reads
  `window_action_wiring.rs` via `include_str!` and asserts a lexical order that
  no longer models the confirmed-delete path (the sidebar refresh is deferred
  since #845). Replace it with a runtime-order test on the closure if the
  ordering still matters after A2 (record calls in order, assert), otherwise
  retire it with a one-line comment naming #366 and this plan.
- `reprise_core::artist_news_view::sort_rows_by_display_text` is re-exported
  from `artist_news.rs:90` as `sort_release_rows_by_display_text` and has no
  caller; `ReleaseRowSortKey` in `releases_presentation.rs` does the work.
  Delete the function and the re-export, keep its tests only if they cover
  logic that survives elsewhere.
Acceptance: `cargo clippy --all-targets --workspace` exit 0; no `dead_code`
allowance added.

## §M — measurements (written by the session between the passes)

_pass 1 table goes here: per gesture, `mutated_ms` with `mirror_ms` /
`listeners_ms` / `feed_ms`; `advance_ms` with `live_ids_ms` / `target_ms` /
`present_ms`; medians over three runs, control = mother §0 fix arm._

_pass 2 / acceptance table goes here._

## Report

State per task what moved, what stayed under the threshold and why, the §M
numbers, and R1 (the loaded-track totals) for the mother plan's §5.
