---
slug: hot-path-pass-deferred
worktree: /home/marvin/Projects/reprise-hot-path-pass-deferred
branch: feature/hot-path-pass-deferred
phase: planned
codex_session:
created: 2026-10-05
---
# Hot-path pass — the deferred items that are free now

Follows the hot-path pass, whose two strands landed as #1073 and #1076 (`dev` @ e0598026a1). Its
six deferred items waited for refactor wave 1 (#1068, #1070, #1071), which has landed. They were
re-surveyed against `origin/dev` @ e0598026a1 on 2026-10-05. Claude workers implement this plan:
the Codex quota is exhausted until 2026-10-09.

## Re-survey result and the cut

Two unlanded branches own files that some of these items need:

- `feature/loudness-and-cue-sheets-r128` (plan `loudness-and-cue-sheets-r128.md` on that branch)
  owns `crates/reprise-android-ffi/**`, `crates/reprise-core/src/db.rs` and
  `queries/{track_gain*,mod}.rs`. It registers migration **v88** and bumps the version asserts in
  `db_concerts_migration_tests.rs` and `db_device_sync.rs`. Its worktree holds uncommitted
  android-ffi edits, so the claim has a live owner.
- `feature/refactor-wave-2026-10-lints` owns every file under `crates/` that carries an `allow`
  attribute. It has already changed `queries/mod.rs` and `ui/tag_edit/tag_edit_flow.rs`.

| Item | Verdict | Why |
|---|---|---|
| A2 `prepare_cached` in the browse queries | narrowed into T1 | Only the delete path runs static statements once per item. The browse statements run once per view, and most of their SQL text depends on the active filters. Each connection's statement cache holds 16 entries and is shared with the statements #1073 cached; its capacity would be set in the r128-owned `db.rs`. The browse statements are left as they are. |
| A3 batched `track_source_paths` (Android play/queue, and the `trash_boundary` twin) | deferred | r128 owns `reprise-android-ffi/**` and `queries/mod.rs`, where `track_source_path` lives. |
| A4 renumber each playlist once per delete | T1 | — |
| A8 MCP page collector | T2 | — |
| A9 `playlist_tracks(track_id)` index | deferred | r128 takes v88 and bumps the same asserts. Either landing order forces a renumber. Take the next free version once r128 has landed. |
| A10(a) `HashSet` in `queue_purge_plan` | T3 | — |
| A10(b) `HashSet` in `tag_edit_flow.rs` | deferred | Owned by the lint wave (it carries an `allow` and was changed there), and the file is at 794 of 800 lines. |

Out of scope: `tag_save_refresh.rs::plan` and `tag_reload_anchor.rs::first_sort_key_write` are in the
same O(n·m) class, but they were never accepted. They are candidates for the A10(b) round.

## Shared rules

- Behaviour-preserving. Write the test first wherever a test can pin something; otherwise the named
  existing suite is the proof.
- Touch only the files a task lists. Never touch `crates/reprise-core/src/queries/mod.rs`,
  `crates/reprise-core/src/db*.rs`, `crates/reprise-core/src/lib.rs`, anything under
  `crates/reprise-android-ffi/`, or anything under `crates/reprise-gnome/src/ui/tag_edit/`.
- Stay compatible with the lint wave that lands next:
  - a new suppression carries `reason = "…"`;
  - no new `clippy::too_many_arguments` suppression;
  - no `RefCell` borrow or lock guard held in a `match`/`if let` scrutinee.
- Every code file ends below 800 lines. If a change would cross that, stop and report rather than
  trimming comments.
- English everywhere. One commit per task, with a prose subject in the style of `git log`.

## Tasks

### T1 — bulk track deletes renumber each playlist once (A4, narrowed A2)

Files: `crates/reprise-core/src/queries/maintenance_delete.rs`,
`crates/reprise-core/src/library/playlists.rs` (`renumber_positions` only),
`crates/reprise-core/src/queries/tests_maintenance.rs`.

- **Today:** `delete_requests` reads the playlists that hold a track and deletes the track (the FK
  cascade removes its `playlist_tracks` rows). It then renumbers every affected playlist, inside the
  per-track loop. Removing N tracks of one playlist renumbers it N times, each pass
  O(playlist length).
- **Change:**
  - Keep reading the affected playlist ids before each delete, because the cascade removes them.
  - Add them to one ordered set (`BTreeSet`), and only when `delete_guarded_track` actually removed
    a row.
  - After the loop, still inside the same transaction, renumber each playlist in the set once.
- **Narrowed A2:** the per-request `SELECT DISTINCT playlist_id …` moves onto `prepare_cached`. So
  does the position `SELECT` in `renumber_positions`, whose `UPDATE` is already cached.
- **Check before changing:** nothing inside the loop may read playlist positions after a delete. If
  something does, stop and report.
- `renumber_positions` walks positions in ascending order, which stays correct with several gaps.
  Keep that.
- **Test first:** add the following test to `tests_maintenance.rs` unless one already covers it.
  - It removes several tracks in one call:
    - tracks at adjacent and at non-adjacent positions in the same playlist;
    - one track that sits in two playlists;
    - one track that appears twice in the same playlist (the key is `(playlist_id, position)`, so
      this is allowed);
    - one request whose guarded delete removes nothing.
  - It asserts that every playlist ends gapless, with the surviving tracks in their original order.
  - It pins behaviour, so it passes both before and after the change.
- **Proof:** `cargo test -p reprise-core`, covering the maintenance, auto-clean, issues and
  playlists suites.

### T2 — agent searches read each summary list in one pass (A8)

Files: `crates/reprise-core/src/queries/library_views.rs`,
`crates/reprise-core/src/queries/library_views_tests.rs`, `crates/reprise-mcp/src/data.rs`,
`crates/reprise-mcp/tests/library_browse.rs`.

- **Today:** `all_artist_summaries` and `all_album_summaries` in `data.rs` loop `query_artists` and
  `query_albums` in 500-row windows (`SUMMARY_WINDOW_SIZE`).
  - Every window re-runs the grouped query with a growing `OFFSET`, and runs a count query first.
    That is O(n²/500) work plus ceil(n/500) count queries per MCP call.
  - `search_artists` and `search_albums` then filter in Rust with `to_lowercase().contains`, and
    page with `skip`/`take`.
- **Change:**
  - Add core functions that return all artist summaries and all album summaries in one query each.
    Use the same projection, grouping and order as the windowed queries, and no `LIMIT`, `OFFSET`
    or count.
  - Reuse the existing SQL builders. `queries/mod.rs` re-exports `library_views::*`, so a new
    `pub fn` there needs no edit to `mod.rs`.
  - `data.rs` calls them. The window loop and `SUMMARY_WINDOW_SIZE` go away.
  - The Rust filter stays exactly as it is. SQLite `LIKE` folds case for ASCII only, so moving the
    filter into SQL would change the results for non-ASCII names ("BJÖRK" against "björk"). It
    would also change the album match, which goes through `TRIM(album)` and the effective album
    artist.
- **Test first:**
  - In `library_browse.rs`, pin the current behaviour (these tests pass before and after):
    - a non-ASCII mixed-case needle that matches both an artist and an album;
    - an album that matches only through its effective album artist;
    - `total` and pagination, with `offset` and `limit` crossing a page boundary.
  - In `library_views_tests.rs`, assert that each one-pass function equals the concatenation of
    every window of the windowed query, with a small `limit` so that several windows exist. This
    test fails to compile first.
- **Proof:** `cargo test -p reprise-core -p reprise-mcp`.

### T3 — queue purges deduplicate with a set (A10(a))

File: `crates/reprise-gnome/src/ui/playback/queue_transport.rs`.

- `queue_purge_plan` dedupes `immediate` with `Vec::contains` inside the loop over the ids, which
  is O(n²). Keep the `Vec` for order and add a `HashSet` for membership.
- **Proof:** the purge tests in `queue_transport_tests.rs`, among them
  `play_5a_background_purge_defers_loaded_track_while_purging_future_entries`, which feeds
  duplicates. Run them filtered: `cargo test -p reprise-gnome queue_transport`.

### T4 — plan bookkeeping

- Delete `docs/plans/hot-path-pass.md`, `hot-path-pass-a.md` and `hot-path-pass-b.md`. They have
  shipped, and nothing in `crates/`, `scripts/` or `android/` cites them (checked), so
  `docs/plans/README.md` drops them. This plan carries their deferred list forward.

## Verification

The orchestrator runs these over the whole branch on its own worktree:

- `cargo fmt --check`;
- `cargo clippy --locked --all-targets --workspace -- -D warnings`;
- the workspace tests the way the merge gate runs them, including the serial `reprise-platform-linux` part;
- `cargo audit`, whose only accepted advisory is RUSTSEC-2024-0436;
- the core purity check;
- `scripts/check-architecture.sh`;
- the 800-line check on every touched code file;
- the merge-readiness wrapper.

## Landing

- A3, A9 and A10(b) remain open and are recorded in #1080, together with
  their unblock conditions: A3 and A9 wait for r128 to land, and A10(b) waits for the lint wave.
  The issue also lists the two out-of-scope candidates.
- That issue, not a plan file, is their durable record.

## Parallelität

One strand.
- T1 and T2 both change `reprise-core`, which rebuilds everything downstream.
- T3 is a few lines.
- A second worktree would double the cargo builds and gain no wall-clock time.

The tasks run in order: T1, T2, T3, T4. There are no post-merge cross-checks beyond the gate.
