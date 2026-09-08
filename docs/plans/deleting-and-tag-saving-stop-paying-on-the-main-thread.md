---
slug: deleting-and-tag-saving-stop-paying-on-the-main-thread
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-06
strands: a,b
merge_order: a,b
---
# Deleting and tag-saving stop paying on the main thread

Follow-up to `responsive-editing-and-one-table-grammar.md` (shipped 2026-09-06,
PR #848). That plan's §6 measurement missed two of its goals — the delete
latency (G3) and the tag-save delta (G2) — and its decision rules name the
follow-ups. This plan does exactly those two things plus the two housekeeping
items its reviews left behind. Grilled 2026-09-06; the grill's outcome is
recorded in §7.

Two strands: **A** (delete finish) and **B** (tag save). Strand files:
`…-a.md`, `…-b.md`. Merge order A → B.

## 0. What was measured (2026-09-06, real library, 1944 rows, artist sort)

Control = dev before the mother plan (`1aa6afe819`); fix = merged dev
(`623d852186`). Headless harness in `~/.local/share/reprise-measure-20260906/`
(`measure.sh`, `parse.py`, `aggregate.py`); it reproduces the desktop baseline
within 3 ms, so its numbers stand in for the desktop.

| Gesture | control | fix | fix breakdown (ms) |
|---|---|---|---|
| G1 delete 1 non-loaded | 55 ms | 61 ms | purge 48 · advance 0 · reload 11 |
| G2 delete 1 loaded | 79 ms | 112 ms | purge 43 · advance 8 · reload 56 |
| G3 delete 13 incl. loaded | 181 ms | 170 ms | purge 52 · advance 49 · reload 68 |
| G4 tag-edit 8 rows (Genre) | – | `reload_ms` 263, `delta=false` 4/4 | one 46 000 px viewport jump after the save |

`purge` is `player.purge_queue_ids(&removed_ids)` measured as `mutated_ms` in
`delete_tracks.rs::finish`; it costs 43–52 ms whether 1 or 13 rows are
deleted. `advance` is `advance_after_user_catalog_delete`. The deferred
browse-bar refresh costs 0 ms.

**What the code says about the numbers** (read on `origin/dev` during the grill):

- `purge_queue_ids` (`ui/playback/queue_transport.rs`) mutates the in-memory
  queues and calls `notify_queue_changed`, which (1) updates the MPRIS/agent
  queue mirror (in memory), (2) runs the registered listeners — sidebar queue
  count from an in-memory provider, `reload_queue_if_visible`, now-playing,
  the shared queue model — and (3) calls `feed_next`
  (`ui/playback/up_next_transport.rs`): a settings read, `query_live_track_ids`
  (every present track id into a `HashSet`), `query_available_episode_ids`,
  `query_track_summary`, then `player.set_next(path)`. Only `feed_next` leaves
  memory. **Nothing in `notify_queue_changed` feeds the glide**: the
  follow-to-next-track destination is computed from in-memory state
  (`scroll_glide.destination()`, via `notify_current_track_changed` →
  `track_reveal::defer`). The mother plan's carve-out is therefore decided: no
  phase of `notify_queue_changed` has to stay synchronous for the glide.
- `advance_common` has no loop over the deleted ids. Per call it runs
  `query_live_track_ids` and `query_available_episode_ids` once, the in-memory
  target search, then `present_queue_item`. The same full
  `query_live_track_ids` materialisation runs again in `feed_next` and in
  `start_current_item` — two to three full-library queries per delete, and on
  every track transition. Why 13 rows cost 49 ms and one row 8 ms the code
  does not say.
- The tag-save delta compares `before` = `OpenedReloadState.view_ids`, which is
  **`BrowseSnapshot::ids()`** — only the ids the browsed DB query returned at
  editor open (`tag_edit_flow.rs:301`) — against `after` =
  `shared.current_view_ids()` at save time. One differing id and
  `tag_save_model_change` returns `None` (`tag_save_refresh.rs:29`). The
  harness log shows no snapshot warning, so `has_pre_save_view` is most likely
  true and the refusal is `before != after`; the alternative is a view reload
  between open and save. Undiagnosed; B0 diagnoses it.

## 1. Goals

Gated (the plan is not done until the merged tree shows them in the harness):

- **G1** `mutated_ms` < 10 ms on every delete gesture.
- **G2** `advance_ms` < 20 ms for a 13-row delete that includes the loaded track.
- **G3** Total main-thread span (`purge → delete batch completed`, stage
  `finish`) < 30 ms for one non-loaded row — the mother plan's one-row target,
  redeemed here.
- **G4** A Genre edit on 8 rows under artist sort takes the delta path
  (`delta=true`) and writes no viewport adjustment larger than one row height
  (45 px) after the save.
- **G5** A save that touches the sort field keeps the first edited row inside
  the viewport after the full reload (the rule
  `tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`
  already states).

Reported, not gated:

- **R1** Total spans for "delete 1 loaded" and "delete 13 incl. loaded". The
  reload (56–68 ms, includes the re-centring of the new current track) is out
  of scope; the report states what remains once purge and advance are fixed,
  so the decision whether the reload gets its own plan is made with numbers.
- **R2** Adjustment writes after a sort-field save (see G5), with their sizes.

Non-goals: the tag editor open path (met, `build_ms` 20 ms); the loaded-track
reload; anything on Android; the track list's `BrowseBar` (own draft, §6).

## 2. Rules that bind every task

- **R-threshold.** Every task first lands an instrumentation commit that adds
  per-phase `*_ms` fields to an existing `info` line. Only a phase measured at
  **≥ 5 ms** in the harness moves to a deferred idle or gets replaced. A phase
  under 5 ms stays as it is, whatever it looks like. The fields stay in the
  code afterwards — they are the acceptance instrument for the next plan too.
- **R-feed.** `feed_next` becomes deferred for **every** caller of
  `notify_queue_changed`, not just the delete path. Synchronously remains only
  the safety step: if the pre-fed next item is among the ids just removed, call
  `player.set_next(None)` before returning, so a track that ends inside the
  idle gap does not start a file from the trash. The deferred feed is
  coalesced with a "feed pending" flag: several purges in one main-loop
  iteration produce one feed.
- **R-equivalence.** If `query_live_track_ids` is replaced by a per-id point
  query, a unit test proves the point query and the `HashSet` variant agree
  for a live id, a missing id, a `missing_since` id and a `removed_at` id.
- **R-two-pass.** The code phase runs **twice per strand**: pass 1 is the
  instrumentation commits only; then the session builds the release binary
  from the worktree, runs the harness against it and writes the table into
  the strand file; pass 2 gets the strand file with the numbers and applies
  R-threshold. `phase: coded` is set after pass 2. Codex cannot run the
  harness — it lives outside the worktree and needs Xvfb, the DB snapshot and
  a release build.
- **R-control.** The control for every measurement in this plan is the mother
  plan's measured fix arm (`623d852186`, table in §0). No new control arm.
- **R-guard.** `nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one`
  runs after every commit of strand A; the queue display tests and the gapless
  tests stay green.

## 3. Strands and file ownership

**Strand A — delete finish and housekeeping** (`…-a.md`). Owns
`crates/reprise-gnome/src/ui/playback/**`, `crates/reprise-gnome/src/ui/delete_tracks*.rs`,
`crates/reprise-gnome/src/ui/mpris_mirror.rs`, `crates/reprise-gnome/src/ui/scan/scan_watcher.rs`,
`crates/reprise-gnome/src/ui/window/window_action_wiring.rs`,
`crates/reprise-gnome/src/ui/releases/releases_presentation.rs`,
`crates/reprise-core/src/queries/maintenance.rs`, `crates/reprise-core/src/artist_news*.rs`.
Reads but never edits `crates/reprise-gnome/src/ui/track_list/delete_follow_display_tests.rs`.

**Strand B — tag save** (`…-b.md`). Owns `crates/reprise-gnome/src/ui/tag_edit/**`,
`crates/reprise-gnome/src/ui/track_list/tag_mutation_refresh*.rs`,
`crates/reprise-gnome/src/ui/track_list/track_list_model_change.rs`.

The intersection is empty. Both strands add fields to existing log lines; the
harness parser (`parse.py`, outside the repo) is the session's to adapt.

## 4. Merge order

**A → B.** No dependency; a fixed order so `land.sh` rebases the second strand
onto the dev the first one produced.

## 5. Post-merge cross-checks

Run on a detached worktree of the merged dev, in this order:

1. **Harness, all four gestures, on the merged tree.** The strand measurements
   ran on separate binaries; this is the first run with A and B together.
   Gates G1–G5 from §1; R1/R2 into the report.
2. `cargo clippy --all-targets --workspace` — exit 0, 0 warnings.
3. The delete display tests one process each (`scripts/check-display-tests.sh --list`
   for the module paths), plus `nav_10b…` and `tag_1_year…_after_resort`.
4. `scripts/check-architecture.sh`, `scripts/check-ux-traceability.sh`, then
   the full display suite `scripts/check-display-tests.sh` (`SUITE EXIT=0`).
5. The real `CI` workflow run for the merge commit is green (dev runs get
   cancelled by the next merge; the evidence is the next completed run that
   still contains the commit).

## 6. Leftovers this plan does not take

- The track list's `BrowseBar` onto the shared `FilterBar`/`FilterModel`
  grammar (strand b's dropped fifth commit, three special cases). Own,
  ungrilled draft: `docs/plans/browse-bar-joins-the-filter-grammar.draft.md`.
- B4 ("one delta reload for the four source tables") is **already done** on
  `origin/dev`: `releases_model.rs`, `radio_model.rs`, `podcasts_model.rs` and
  `concerts_model.rs` all call `list_store_delta::replace`. The strand-b report
  marked it deferred without reading the code. Nothing to do.
- The loaded-track reload (R1) — decided after this plan's numbers.
- When the harness is no longer needed:
  `rm -rf ~/.local/share/reprise-measure-20260906` (reflink mirror, 4 400 files).

## 7. Grill record (2026-09-06)

1. Strand C (BrowseBar grammar + B4) out — own draft; B4 found already done.
2. Strand D (housekeeping) folded into A: the lexical-order test is about the
   ordering A rewrites; a separate strand only creates a seam.
3. Task 1: instrument first; 5 ms threshold; `feed_next` rule applies to every
   caller of `notify_queue_changed`.
4. Task 2: same rule; `query_live_track_ids` → point query allowed, bound to the
   threshold, with the equivalence test.
5. Targets: per-phase gates plus the one-row total; loaded-track totals reported.
6. B: diagnosis commit first, `view_ids` decoupling as the named candidate,
   reproducing test.
7. Task 5 (sort-field save keeps the edited row visible) stays in B — user
   decision against the recommendation to drop it.
8. Two Codex passes per strand with a session-run measurement between them.
9. Ownership, merge order A → B, post-merge checks as in §3–§5.

## Parallelität

Two strands as cut in §3; file groups disjoint. Merge order A → B (§4).
Post-merge cross-checks in §5 — the harness run on the merged tree is the one
comparison no strand can make alone. A third strand was considered (the
housekeeping items) and folded into A because its test asserts the very
ordering A changes. Both strands run their two Codex passes concurrently; the
two release builds per pass are the cost.
