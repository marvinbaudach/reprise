---
slug: responsive-editing-and-one-table-grammar
worktree:
branch:
phase: shipped
codex_session:
created: 2026-09-05
strands: a,b,c
merge_order: a,c,b
---
# Responsive editing and one table grammar

Mother plan. Three strands, each with its own file, worktree and branch:

| Strand | File | Purpose |
|---|---|---|
| A — `edits` | `docs/plans/responsive-editing-and-one-table-grammar-a.md` | the tag editor and the delete path stop stalling |
| B — `tables` | `docs/plans/responsive-editing-and-one-table-grammar-b.md` | one filter bar, one sort grammar, one delta reload for the source views |
| C — `structure` | `docs/plans/responsive-editing-and-one-table-grammar-c.md` | `wire()` becomes an index of one file per concern |

This file holds what the strands share: the measurements, the goals, the rules
for the implementer, the cut with its file ownership, the merge order, the
post-merge cross-checks and the measurement protocol. It is frozen once the
plan phase ends. **The code phase builds each strand's prompt from §2 of this
file plus the strand file** — the strand files do not repeat the rules.

Read against `origin/dev` @ `2a4c6cc07f` (2026-09-05). Every `file:line` below
comes from that tree; the files this plan touches are identical on the
`song-visuals-ask-the-stored-category` branch the survey ran on.

Two user reports started this plan, and one structural question:

1. **The tag editor opens and closes sluggishly.**
2. **Deleting tracks is slow, and the table judders after confirming.**
3. **Five table views, five copies of the same grammar** — filter bar, sort,
   reload, empty state — where `docs/ux-rules.md` FIL-2a already says there is
   one grammar.

The audit `docs/plans/repo-audit-2026-08-31.findings.md` (T1-4, T1-5, T1-6)
and the consolidation plan `docs/plans/consolidation-plan.md` (Package 2.2
"one filter bar", Package 3.4 "view ports instead of `RuntimeWiring`") already
name most of the structure. This plan is the *how* for the parts that are worth
doing now, and it says explicitly what it leaves out.

## 0. What was measured before writing this

Reprise logs with microsecond timestamps into the user journal, so three real
deletions from 2026-09-05 could be reconstructed without a build
(`journalctl --user -o short-precise`, PIDs 3161373 and 3293202). All three
were **trash** deletions on the host backend, view sorted by artist, ~1950 rows.

| Gesture | purge → next step | reload query → toast | main-thread total (purge → toast) |
|---|---|---|---|
| 12:16:43 — delete 1 track, not the loaded one | 40.5 ms | 15.3 ms | **55.9 ms** |
| 21:17:11 — delete 1 track, the loaded one | 67.7 ms | 17.8 ms | **85.7 ms** |
| 12:26:37 — delete 13 tracks incl. the loaded one | 41.4 ms + 36.2 ms (next track starts) + 25.5 ms | 73.7 ms | **177.9 ms** |

The 12:26 case then centres the new current track 188 ms after the toast
(`track_reveal: current track centered … change=AutomaticAdvance`). The
watcher's reconcile fires 2.0 s after each deletion with all counters at zero
and triggers no reload — it is **not** a cause. The time between the confirm
click and the worker's result is **not logged today**; neither is anything on
the tag editor's open path (`tag_edit_flow.rs:328` logs "tag editor presented"
at `debug`, which the journal does not carry). The single tag-edit event on
2026-09-04 (8 tracks) shows only "batch completed".

What the code says about the two paths, verified at HEAD:

- **Tag editor open.** `tag_editor_widgets.rs:82-87` runs
  `cover::resolve_source` → `cover::thumbnail` → `gdk::Texture::from_filename`
  synchronously on the main thread while the dialog is being built.
  `thumbnail` (`reprise-core/src/cover.rs:212`) reads the full cover bytes —
  for embedded art that means parsing the audio file with lofty — hashes them,
  and on a cache miss decodes, resizes and PNG-encodes. The track list does
  exactly this work through `CoverLoader::load_into_picture`
  (`ui/cover/cover_loader.rs:176`) inside `gio::spawn_blocking`, with a 256-entry
  texture LRU; the dialog bypasses that and pays the I/O every time. The dialog
  is built from scratch per `present()` (`tag_editor.rs:79`), ~50 widgets, and
  `adw::Dialog` (`tag_editor_form.rs:365`) animates in only after the build.
- **Tag editor close.** Save keeps the dialog open with a progress label on the
  button until the worker (`one_shot_task::spawn_with_progress`,
  `tag_edit_flow.rs:443`) has written every file, then closes it
  (`tag_edit_flow.rs:481`) and runs `finish_apply` (`:504`), which calls
  `refresh_after_tag_mutation_with_anchor` / `reload_with_anchor` — a **full
  model invalidation** (`model_change = None`) preceded by a sorted full-table
  id query (`Shared::current_view_ids`). The delete path already does better:
  `delete_tracks.rs:353` passes both a `ModelChange` delta and the fresh id list
  to `reload_with_anchor_and_viewport`, and `track_list_reload.rs:263` then skips
  the id query and `track_list_model.rs:566` rebinds only the changed range.
- **Delete worker.** `trash_tracks_with` (`reprise-core/src/library/trash_tracks.rs:113`)
  calls the trash action once per file, sequentially. On the Flatpak backend
  `portal_delete` (`reprise-platform-linux/src/trash.rs:40`) opens a **new**
  `zbus::blocking::Connection::session()` per file (`:46`). The DB commit is one
  transaction (`maintenance_delete.rs:203-227`).
- **Delete finish, main thread** (`delete_tracks.rs:378-405`): the
  `on_library_mutated` callback (`window_action_wiring.rs:320`) runs
  `sidebar.refresh("track removed from library")` — database-backed counts —
  and `player.purge_queue_ids`; then `advance_after_user_catalog_delete`
  (`queue_transport.rs:767`); then `browse_bar.refresh()`; then the delta
  reload. Everything before the reload delays the frame in which the rows
  disappear. The ~40 ms between the purge log line and the browse-bar line is
  present in all three events and belongs to something in that gap
  (`notify_queue_changed`, the sidebar rebuild, or `browse_bar.refresh` before
  its own log line) — Task A0 pins it, §6 reads it.

None of these are guesses about *where* the time goes; the split *inside* the
gaps is what A0 measures first. Every fix task states which number it moves.

## 1. Goals and non-goals

**Goals**

- G1 — Opening the tag editor does no cover I/O on the main thread; the cover
  arrives through the same loader and cache as the track list (MOT-6 "nothing
  blocks").
- G2 — After a tag save, the track list applies a delta instead of a full
  invalidation whenever the edit did not change row order or membership.
- G3 — A deletion shows its rows leaving before any secondary refresh runs.
  The **expectation** is that the main-thread total from worker result to toast
  drops from 56–178 ms to under **30 ms** for one row and under **60 ms** for
  13 rows in a ~2 000-row view, measured on the same machine and library as the
  baseline. This is not a gate strand A must pass: A lands on green tests and
  gates; §6 measures the expectation after landing, and a missed target opens a
  follow-up plan, never a revert.
- G4 — The trash backend opens one portal connection per batch, not per file.
- G5 — Both paths log their phases at `info` with millisecond fields, so the
  journal stays a passive probe for the next report.
- G6 — Releases, radio, podcasts, concerts and the track list share one filter
  bar implementation (`FilterBar<M: FilterModel>`); the four source views share
  one sort grammar and one delta-reload path. Their existing tests pass
  unchanged and are the proof.
- G7 — `wire()` in `ui/window/window_runtime_wiring.rs:94` becomes a short
  ordered list of calls into one file per concern.

**Non-goals, with the reason**

- The two cover pipelines (`ui/cover/*` vs `ui/podcasts/source_image*.rs`)
  stay separate. The podcast side is URL-based with an 8-thread fetch pool and
  its own thumbnail-first decode; nothing measured says it costs anything, and a
  merge would risk the podcast rows for a code-size win. Separate plan.
- `podcasts_view_*.rs` stays as it is. The ten files were split for the size
  gate, but the cut lines (actions, copy, data, downloads, marker, requests,
  selection, shortcuts) are by responsibility; re-cutting buys nothing.
- The three threading mechanisms in `ui/` stay. `one_shot_task` (named
  workers with a result channel), `gio::spawn_blocking` (thumbnails) and the
  artwork fetch pool serve different needs; a single abstraction over them was
  proposed only by counting call sites.
- Package 3.4 (view ports replacing `RuntimeWiring`) is *not* done here; G7 is
  the mechanical step that makes it possible afterwards.
- The per-view empty-state enums stay; they are state machines with different
  variants, and the widget behind them (`ui/source_empty_state.rs`) is already
  shared by all four views.
- Fixing whatever owns the unlogged ~40 ms in the delete finish (§0, last
  bullet) beyond what Task A4 defers. Codex has no display and no real library,
  so it cannot read A0's fields; §6 reads them after landing and its decision
  rule opens a follow-up plan if one step still costs ≥ 20 ms. That is why
  `ui/playback/queue_transport.rs` and `ui/sidebar/sidebar.rs` are owned by no
  strand.
- Android, and the AGENTS.md "Active file ownership" records for
  `feat/list-geometry-service`, `feature/multi-surface-frontends` and
  `feature/library-doctor-fix-round-3` — all three branches are gone from
  `origin`; the records are stale and no longer bind anything, but rewriting
  them is not this plan's job.

## 2. Rules for the implementer — read first

- **Test first, per task.** Each task names its tests. A test that is green on
  its first run measured nothing; the red run goes into the commit message.
- **Base is `origin/dev`.** `MERGE_READINESS_BASE_REF=origin/dev` for
  `scripts/check-merge-readiness.sh`, `GITHUB_BASE_REF=dev` for
  `scripts/ci-quality.sh`; both refuse to run against `main`. Both also require a
  clean worktree *including untracked files*.
- **Gates before every commit** (AGENTS.md): `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace` (never bare `cargo test`), `cargo audit`,
  `scripts/check-architecture.sh` (every Rust file < 800 lines — and
  `cargo fmt` can push a 795-line file over it), and after any `reprise-core`
  change the purity proof
  `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` must
  print nothing.
- **Long output goes to a file.** `cargo test --workspace > $LOG/test.log 2>&1`,
  then `grep -c '^test result: FAILED' $LOG/test.log` and
  `grep -n 'FAILED\|panicked' $LOG/test.log | head`. Never read a verdict through
  a pipe: `cmd | tail` reports `tail`'s exit status.
- **`cargo test` filter traps.** `--exact` and `--lib` run zero tests in
  `reprise-gnome` (there is no `[lib]`; the tests live in `--bin reprise`).
  Filter with a substring and check the "N passed" count.
- **Display tests run one per process.** GTK initialises once per process, so a
  filter that matches twelve `#[ignore]` display tests aborts in `gtk_init`
  before the first one. One test:
  `xvfb-run -a cargo test -p reprise-gnome -- --ignored <full_test_name>`.
  More than one: `scripts/check-display-tests.sh` (the `--rule-named` subset is
  what merge-readiness runs).
- **`cargo test` measures a different track-list model.** `TrackListModel` is
  a plain `ListModel` under `cfg(test)` and a `SectionModel` in production
  (`track_list_model.rs:182-191`). A reload timing from a unit test is not
  evidence for the app; the numbers that count come from the journal fields
  Task A0 adds, read from the real binary against the real library (§6).
- **UX rules bind.** `docs/ux-rules.md` sections K (filter), L (tag editor),
  O (motion), G (feedback) and Z (track browser) apply. A changed behaviour
  under an existing rule keeps a rule-named test. **Only strand B edits
  `docs/ux-rules.md`** (section K exceptions); strands A and C add no rule text.
  `scripts/check-ux-traceability.sh` needs the rule ID only *in the test name*
  — the rules file never lists tests — so A's `mot_6_…` display test needs no
  rules edit.
- **RefCell discipline.** Copy values out of a `borrow()` in their own
  statement before calling into GTK; the tag editor and delete paths both hold
  `Rc<Shared>` with `RefCell` slots.
- **Touch only the files your strand owns** (§3). A task that seems to need a
  file outside the ownership list stops and reports; it does not edit the file.
- `$LOG` is a directory outside the repo, e.g. `/tmp/reprise-responsive`.

## 3. The cut — file ownership

Three strands, disjoint by file, all cut from `origin/dev` @ `2a4c6cc07f`.
Ownership is by glob; a strand never edits a file another strand owns, and
files owned by nobody are read-only for all three.

**Strand A — `edits`** (tag editor and delete path)
- Owns: `crates/reprise-gnome/src/ui/tag_edit/**`,
  `crates/reprise-gnome/src/ui/delete_tracks*.rs`,
  `crates/reprise-gnome/src/ui/window/window_action_wiring.rs`,
  `crates/reprise-platform-linux/src/trash.rs`,
  `crates/reprise-core/src/library/trash_tracks.rs`.
- Reads but never edits: `ui/track_list/**`, `ui/cover/**`,
  `ui/browse/browse_bar.rs` (calls `refresh()` only),
  `ui/playback/queue_transport.rs`, `ui/sidebar/sidebar.rs`.
- Tasks A0–A4.

**Strand B — `tables`** (the four source views and the track list's bar)
- Owns: `crates/reprise-gnome/src/ui/browse/**` — including `browse_bar.rs`,
  whose **public API is frozen**: `BrowseBar` keeps every `pub` signature the
  track list and strand A call —
  `crates/reprise-gnome/src/ui/releases/**`, `crates/reprise-gnome/src/ui/radio/**`,
  `crates/reprise-gnome/src/ui/concerts/**`,
  `crates/reprise-gnome/src/ui/podcasts/podcasts_{filter_bar,model,presentation,view}*.rs`,
  `crates/reprise-gnome/src/ui/library_doctor/review_filter_bar.rs`,
  `crates/reprise-gnome/src/ui/table_columns/**`,
  `crates/reprise-gnome/src/ui/list_store_delta.rs`,
  `crates/reprise-gnome/src/ui/filter_bar_layout.rs`,
  `crates/reprise-gnome/src/ui/source_empty_state.rs`,
  `docs/ux-rules.md`, and **one** of `scripts/check-frontend-thinness.sh` or
  `scripts/check-project-quality.sh` (for B5).
- Never edits: `ui/podcasts/source_image*.rs`, `ui/track_list/**`,
  `ui/window/**`.
- Tasks B1–B5.

**Strand C — `structure`** (`wire()`)
- Owns: `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`,
  `crates/reprise-gnome/src/ui/window/wiring/**` (new).
- Never edits: `ui/window/window_action_wiring.rs` (A's), any view file (B's).
  The view constructors it calls keep their signatures — a B task that wants to
  change `ConcertsView::new` or `ReleasesView::new` may not; it adds a setter
  instead.
- Task C1.

**Owned by nobody, read-only for all:** `ui/playback/queue_transport.rs`,
`ui/sidebar/sidebar.rs`, `ui/track_list/**` (including `track_list_reload.rs`,
`adjustment_hold.rs`, `scroll_glide.rs`), `ui/cover/**`.

## 4. Merge order

**A → C → B.** A is the user-facing fix and the smallest; C is a pure
relocation that rebases trivially; B is the largest and rebases last onto a
`dev` that already carries the other two. Land one at a time with `land.sh`,
rebasing the next strand onto the `dev` the previous landing produced.

## 5. Post-merge cross-checks

Nobody can run these inside one strand; each reads a file some other strand
owns. Run them on the merged tree, in this order.

1. **After A and B are both in:** run the delete display tests once more —
   `delete_tracks_display_tests.rs`, `delete_tracks_large_block_display_tests.rs`,
   `track_list/delete_follow_display_tests.rs`, one process each. `finish()`
   (A) now calls the `browse_bar.refresh()` that B has rebuilt internally in
   B2's fifth commit.
2. **After C and B are both in:** `cargo clippy --all-targets --workspace` on
   the merged tree — C's `wiring/*.rs` calls the view constructors B owns.
3. **After all three:** `scripts/check-architecture.sh` (line ceiling) and
   `scripts/check-ux-traceability.sh` (B's section K edits and A's MOT-6 test
   name) on the merged tree.
4. **After all three:** the measurement protocol in §6, and the G3 expectation
   compared against §0's table.
5. **B's report** says whether B2's fifth commit (`browse_bar`) landed. If it
   did not, a follow-up plan for the track list's bar is opened; the four
   source conversions stand on their own.
6. **After the last strand, once:** the full display suite (874 tests, ~50 min,
   one process each) on merged `dev` via `scripts/check-display-tests.sh`,
   sharded if wanted, detached under a wake lock. The `--rule-named` subset the
   merge-readiness gate runs per strand is not this; this is the whole suite.

## 6. Measurement protocol (orchestrator, not Codex)

Codex is headless, has no display and no real library, so nothing here is a
strand task. The orchestrator runs it.

**Before `/code`:** nothing to do — the baseline in §0 is already the journal.

**After each strand lands, and once more after the last:**

1. Copy the real library DB with its `-wal`/`-shm`, `wal_checkpoint(TRUNCATE)`,
   into an isolated `XDG_DATA_HOME`/`XDG_CONFIG_HOME`; never measure against a
   synthetic fixture (a generated library measured 8 ms where the real one took
   92 — memory note "measuring GTK main-thread stalls").
2. Run the merged `dev` build from the checkout with
   `REPRISE_DEBUG_SCROLL=1 REPRISE_LOG="reprise::ui::track_list::track_list_builder=debug,info"`
   (memory note "measuring viewport motion") and perform the four gestures:
   - delete 1 track that is not the loaded one;
   - delete 1 track that is the loaded one;
   - delete 13 tracks including the loaded one;
   - tag-edit 8 tracks (a field that does not change the sort order).
3. Pull the A0 fields from the journal:
   `journalctl --user -o short-precise --since -10min | sed 's/\x1b\[[0-9;]*m//g' | grep -E 'delete (confirmed|batch completed)|tag editor presented|tag-edit batch completed'`.
   Report per gesture: `build_ms`, `write_ms`, `reload_ms`, `delta`,
   `worker_ms`, `mutated_ms`, `advance_ms`, `browse_bar_ms`, `main_thread_ms`;
   plus the number of adjustment writes in the 500 ms after the toast and the
   largest single write. The judder claim is settled by those writes, not by
   watching.
4. Interleave arms if a before/after comparison is wanted (A, B, A, B), report
   median and range, never a single run.

**Decision rules, applied to the last run:**

- **G3 missed** (one row ≥ 30 ms or 13 rows ≥ 60 ms `main_thread_ms`): open a
  follow-up plan; do not revert A.
- **One step still ≥ 20 ms** after A4 (`mutated_ms`, `advance_ms` or
  `browse_bar_ms`): open a small follow-up plan for that step alone. If it is
  `purge_queue_ids` → `notify_queue_changed` in `queue_transport.rs`, whatever
  it persists or rebuilds moves to the same deferred idle as A4's sidebar
  refresh, *unless* it feeds the glide destination. If it is `sidebar.refresh`
  or `browse_bar.refresh`, A4 already deferred them and the number says the
  deferral did not take — that is a bug report against A4, not a new task.
- **Adjustment writes larger than a row height** after the toast: the cause is
  in the anchor restore (`adjustment_hold.rs`, `scroll_glide.rs`), owned by
  nobody here. Report the write sequence; open a plan there.

## 7. Risks and abort criteria

- **A4 changes the order of side effects around BROWSE-11.** The glide
  destination must still be set before the anchor restore; if
  `delete_follow_display_tests` goes red and the fix would touch
  `track_list_reload.rs` (not owned by A), stop and report — that file is the
  list-geometry service's and a change there is a different plan.
- **B2 discovers a fifth grammar.** If a source needs more than two
  special-case methods outside `FilterModel`, the abstraction is wrong for it;
  leave that source on its own bar, say so, and keep the other conversions.
  This applies to `BrowseBar` too: if the fifth commit cannot keep every `pub`
  signature, it is dropped and the four stand.
- **C1 hoists a local that carried a borrow.** A `RefCell` borrow that lived
  across two groups inside one function becomes visible when the groups are
  separate functions; that is a finding, not an obstacle — copy the value out
  and note it.
- **The judder does not move.** Handled by §6's third decision rule; no strand
  widens for it.

## Parallelität

The cut is §3, the merge order §4, the post-merge cross-checks §5. Three
strands is the cap; three cargo builds run in parallel. Each strand's worktree
is `<repo-parent>/reprise-responsive-editing-and-one-table-grammar-<n>` on
`feature/responsive-editing-and-one-table-grammar-<n>`, created by the code
phase before the first Codex run.
