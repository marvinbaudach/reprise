---
slug: hot-path-pass-b
worktree: /home/marvin/Projects/reprise-hot-path-pass-b
branch: feature/hot-path-pass-b
phase: planned
codex_session:
created: 2026-10-04
---
# Hot-path pass — strand B: in-memory algorithms, GNOME bind and queue paths

Read the mother plan `docs/plans/hot-path-pass.md` first. Its shared rules,
out-of-scope list and file ownership bind this strand.

## Gate cadence for this run (overrides AGENTS.md "Gates before every commit")

AGENTS.md asks for the full gate battery before every commit. On this run that
rule is **overridden**, because the host is shared with other heavy runs.

- **Per commit:**
  - `cargo fmt --check`;
  - `cargo clippy -p <each touched crate> --all-targets -- -D warnings`;
  - `cargo test -p <each touched crate>`.
- **Once, before the final commit:** the full battery from AGENTS.md:
  - `cargo fmt --check`;
  - `cargo clippy --all-targets --workspace -- -D warnings`;
  - `cargo test --workspace`;
  - `cargo audit`. RUSTSEC-2024-0436 is the only accepted advisory; a new one means STOP and report;
  - the core purity check.

Never launch the app. No headless or display run is part of this strand.

One commit per task below. Commit subjects are plain English sentences with no
`type:` prefix and no agent attribution trailers. Do not push.

## Tasks

### B1 — the stats ribbon is computed with a prefix sum
- **File:** `crates/reprise-core/src/library/stats_snapshot.rs`, the `ribbon` computation in
  `compute_with_pattern`.
- **Now:** every bucket filters all `listen_rows`, which is O(buckets × events).
- **Change:**
  - Verify that `listen_rows` is ordered by `played_at` for every caller. The survey saw
    `ORDER BY played_at` in `stats_screen.rs:210`; if any caller is not ordered, sort once.
  - Build a prefix sum over the per-row values the ribbon needs, then take each bucket by
    `partition_point` on its bounds, keeping today's exact inclusive or exclusive edges.
  - Also compute `local_parts` and `week_start` once per row instead of 3–4 times, for
    active days, active weeks, `this_week_ms` and `best_week`.
- **Test first:** an oracle test. Keep today's ribbon algorithm as a `#[cfg(test)]`
  reference and compare it with the new one on a fixture:
  - events exactly on bucket boundaries;
  - empty buckets;
  - one bucket holding all events;
  - an empty history.

### B2 — Library Doctor: three quadratic spots and one needless clone
All files are under `crates/reprise-core/src/library/library_doctor/`.

- **B2a, `scan.rs`, `take_local_fallback` (near :757–762):**
  - `group.members.clone()` runs for every group before checking membership. Clone only
    inside the branch where the track is found, before the `retain`.
  - Do not use `swap_remove`: it reorders persisted proposals.
- **B2b, `review.rs`, `set_remote_visible` (near :463–475):** `prior_rows.iter().find(..)`
  runs per rebuilt row. Build a `HashMap` keyed by
  `(track_id, field, current, proposed, source)` with `entry().or_insert`, which keeps
  first-match semantics.
  - This needs `Hash`/`Eq` on `DoctorValue` and `ProposalSource` (`types.rs` near :109, :145).
  - If either holds a float or another type that cannot derive `Hash` soundly, skip B2b
    and report.
- **B2c, `review.rs` (near :350, :368):** `sort_by_key` computes a `HashMap`+`min()` key on
  every comparison. Switch to `sort_by_cached_key`, which is equally stable.
- **B2d, `grouping.rs` (near :82 and :100):**
  - Replace `seeds.iter_mut().find(|s| s.key == key)` per track with a key→index map.
  - Replace `album_from_seed`'s rescan of all rows per seed with rows bucketed by key once,
    in `session.rows()` order.
- **B2e, group count:** `crates/reprise-gnome/src/ui/library_doctor/summary_model.rs`
  (near :156) and `write_jobs.rs` (near :193) build the full grouping only to call `.len()`.
  - Add a counting function next to `group_review_rows` in `grouping.rs` that counts
    groups without materialising them, and use it at both sites.
  - It must use the same grouping key, so share the key function rather than copying it.
- **Proof:**
  - B2b and B2d: a test with interleaved keys asserting identical group order, row order
    and first-match picks. Reuse existing tests where they already cover this.
  - B2e: a test asserting `count == group_review_rows(..).len()` on a mixed fixture.
  - B2a and B2c: no new test.

### B3 — bar colours are computed once per bar
- **File:** `crates/reprise-core/src/visuals/modes/bars.rs` (`neon()` near :29).
- **Now:** `hsla_to_rgb` runs about 1400 times per frame, but the colour depends only on
  the bar (64 per frame).
- **Change:**
  - Verify that the inputs to the colour are constant per bar within a frame. If they are
    not, skip and report.
  - Compute it once per bar and reuse it for that bar's segments or pixels.
- **Test first:** render one fixed frame and compare its buffer before and after.
  Reuse an existing visual test if one renders bars.

### B4 — cell binds stop deep-cloning the queue item
- **Files:** `crates/reprise-gnome/src/ui/track_list/track_list_columns.rs` (near :404, and the
  cover column near :552 and :575) and `track_list_title_column.rs` (near :140).
- **Now:** `rendered_metadata = metadata.clone()` deep-clones a `QueueItemMetadata`, a
  `Track` with about 8 Strings, once per cell per bind, so roughly 10 per row. The clone is
  only kept so the now-playing marker closure can re-apply later.
- **Precondition, check first:**
  - Grep `crates/reprise-gnome` for every `borrow_mut`, `replace` or `try_borrow_mut` on a
    `glib::BoxedAnyObject` that carries `QueueItemMetadata`.
  - If any exists, STOP this task and report the sites. Capturing the box would then risk a
    `BorrowMutError` panic.
- **Change, when the precondition holds:**
  - The closure captures `boxed.clone()`, a GObject refcount bump, instead of the deep clone.
  - Inside the closure it borrows with `boxed.borrow::<QueueItemMetadata>()` in its own
    scope, as `rating_column.rs:87` does.
  - The `Ref` lives only for the `apply_now_playing_item` call and is never stored.
  - The metadata-generation check and the `queue_item_at` fallback stay exactly as they are.
  - In the cover column, also drop the second `.clone()` at :575, and borrow instead of
    cloning at :552 wherever the value is not moved into a `'static` closure.
- **Proof:** no new behaviour test. The existing track-list tests are the proof. In the
  summary, state the precondition grep and its (empty) result.

### B5 — the now-playing sync captures a key, not the item
- **File:** `crates/reprise-gnome/src/ui/track_list/track_list_columns.rs`,
  `sync_now_playing_row` (near :79–101), called twice per title bind.
- **Now:** it clones the whole item into an `idle_add_local_once`.
- **Change:** capture only what `is_now_playing` needs, the item kind and its id, as a
  small `Copy` key. Compare it against the playing track and episode in the idle. Make the
  pure comparison function take that key.
- **Proof:** if a test of `is_now_playing` exists, extend it to cover the key form for a
  track and an episode. Otherwise add one.

### B6 — tag-save refresh checks membership with a set
- **File:** `crates/reprise-gnome/src/ui/tag_edit/tag_save_refresh.rs` (near :109).
- **Change:** `updated_ids.contains` runs per write. Build a `HashSet<i64>` once for the
  membership check and keep the `Vec` wherever order matters.
- **Not here:** `tag_edit_flow.rs` (near :554) and `ui/playback/queue_transport.rs`
  (`queue_purge_plan`) have the same pattern. Both belong to the parallel refactor wave
  (`refactor-wave-2026-10`, strands A and C), so do not edit either.
- **Proof:** no new test. Existing tests cover the order.

### B7 — a cover-cache hit on the newest entry skips the LRU scan
- **File:** `crates/reprise-gnome/src/ui/cover/cover_cache.rs`, `touch` (near :109).
- **Change:** return early when `lru.back() == Some(path)`. The LRU order stays identical.
- **Proof:** no new test.

### B8 — the Up Next footer total runs in larger windows
- **File:** `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs`, `set_queue_model` (near :219–233).
- **Change:**
  - Replace the literal `200` with a named constant `FOOTER_DURATION_WINDOW: usize = 2000`.
  - Add a comment: each window binds at most that many ids per `IN (…)` statement, well
    under SQLite's 32766 host-parameter limit of the bundled SQLite.
  - The loop body, the `saturating_add` and the error handling stay as they are.
- **Proof:** no new test. The footer's existing tests are the proof. Do not edit `reprise-core`
  or `reprise-view` for this task.

## Done when

Every task is committed, or skipped with a stated reason, and the full battery is green.
The final summary (`.pipeline-codex.md`) lists per task:
- what changed structurally;
- the proof that ran;
- any skip and its reason;
- the exact gate commands and their results.
