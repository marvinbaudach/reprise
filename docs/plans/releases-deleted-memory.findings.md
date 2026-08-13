# Review findings — releases-deleted-memory (2026-08-11, phase `reviewed`)

Worktree `~/Projects/reprise-releases-deleted-memory`, branch
`feature/releases-deleted-memory`, base `origin/dev`. Three reviewers: Rust,
security, spec/rule conformance. Security cleared the change (no critical, no
major); the other two independently found the same critical defect.

**User decisions on this review (2026-08-11):**
1. Scope of `/refactor`: **everything — critical, major and minor.**
2. Translations: **write "Release" into all six other locales**, drop the fuzzy
   marks, so `scripts/tests/gettext-catalogs.sh` goes green again.
3. Edge case "one deleted song, album kept, same-titled single in the catalog":
   **the single disappears** — current behaviour is correct, pin it with a test
   and make decision 2's prose in the spec unambiguous.

## Critical

**K1 — the two deletion paths are inverted.** `exclude_tracks_matching_paths`
(`queries/maintenance.rs:411-424`), which is what the context menu's "Remove
from library" (`ui/delete_tracks.rs:115,236`) actually calls, passes
`remember_deletion = false` and writes no memory. The wired non-trash path,
`purge_tombstones` (`queries/maintenance.rs:714`), is reachable in production
only from the Missing-files flow (`ui/issues/missing_view.rs:261,591,765` via
`window_action_wiring.rs:114`) — the flow decision 1 explicitly excludes.

Failure: unmount a drive, clean up the Missing card, and every album whose
tracks all lived there is remembered as "deliberately deleted" and hidden.
Meanwhile the action the user actually means writes nothing.

Root cause is the spec, not Codex: lines 20-23 of
`docs/superpowers/specs/2026-08-11-releases-deleted-memory-design.md` claim
"Remove from library writes a tombstone purely for the ten-second undo", which
is stale for current `origin/dev`. Fix the spec, the plan, and NR-32's text
alongside the code — NR-32 currently asserts two things the code does not do.

Also required: the spec's own test `nr_32_missing_file_writes_no_memory` was
never implemented (only the weaker `nr_32_missing_sibling_writes_no_memory`),
and nothing covers `exclude_tracks_matching_paths` at all.

## Major

**S1 — re-acquisition forgets across scopes.** `forgotten_keys` is a bare
`(artist_key, title_key)` set (`deleted_releases.rs:150-176`); the un-hide loop
ignores `release_type` (`:177-182`) and `forget_deleted_release_memory`
(`:245-251`) has no `scope` predicate. Reproduced by the Rust reviewer:
re-acquiring the album drops the surviving `track` entry and un-hides a single
that is still absent from the library. `remaining.retain` (`:183-185`) only
patches the in-memory vec — the DB row is gone for good.

**S2 — reversal is per-row, the memory is one-to-many.** An `album` memory
hides both the album and its EP twin (pinned by
`nr_32_album_memory_also_hides_the_ep_twin`), but "Show again" on one row
deletes the memory and un-hides only that row; the twin stays hidden forever
with no state left to explain it.

**S3 — cost.** `apply_deleted_release_memory` runs a full `tracks` scan (via
`local_library_index`) plus a full `new_releases` scan, unconditionally — even
with an empty memory table (`deleted_releases.rs:114-116`) — inside the write
transaction. It sits in `sync_releases` (`artist_news_pipeline.rs:503`), which
runs once per artist in the refresh loop (`:248`): N artists ⇒ N full library
scans. The existing code deliberately hoists exactly this work out of the loop
(`artist_news_pipeline.rs:173`). `remember_deleted_releases` adds a second full
scan per deletion (`deleted_releases.rs:55-71`), and the startup purge runs on
the GTK main thread (`missing_view.rs:591`).

**S4 — a gate goes red.** `scripts/tests/gettext-catalogs.sh:31` forbids any
fuzzy entry; the six new ones fail it. Per decision 2 above, resolve by
translating, not by relaxing the gate.

## Minor (all in scope per decision 1)

- **M1 test honesty** — all seven tests in `deleted_releases_tests.rs` call
  `remember_deleted_releases` directly and then delete rows with hand-written
  SQL. They would stay green if both production call sites
  (`maintenance_delete.rs:36,63`) were removed. Route them through the real
  paths.
- **M2** `db_deleted_releases.rs:32-63` — the `..._idempotently` test only
  proves the version guard; the second `migrate_v69` returns at `:17` before
  `CREATE TABLE IF NOT EXISTS` ever runs twice.
- **M3** `maintenance_delete.rs:96` — `unreachable!()` in production code
  behind a generic `RemoveGuard` signature that does not enforce the invariant.
- **M4** `deleted_releases.rs:75-79` — asymmetric empty-key filter: the track
  branch requires artist and title non-empty, the album branch only checks the
  album, so a track with no artist can write an `artist_key = ""` row.
- **M5** `deleted_releases.rs:36-51` — one uncached `query_row` per id; use
  `prepare_cached` or a single statement.
- **M6** — the file split dropped the ADR-002 comment justifying
  `unchecked_transaction()` and the auto-clean "disarmed under us" note
  (now `maintenance_delete.rs:1087-1136`). Behaviour is equivalent; restore the
  reasoning.
- **M7** `deleted_releases.rs:55-58` — after "undo" on a tombstone batch,
  nothing re-runs `apply`; a memory written moments earlier keeps the row
  hidden until the next catalog sync.
- **M8** `maintenance_delete.rs:35` — `SELECT unixepoch()` is the crate's only
  such call; everything else uses `strftime('%s','now')`.
- **M9** — NR-32 carries `[gtk]` but has no gtk-level test; the traceability
  gate only checks the first tag, so it passes silently.
- **M10** — `.superpowers/sdd/progress.md` is the repo-wide SDD log (Codex
  appended its own section correctly, nothing existing was modified), but it is
  a shared merge-conflict surface across parallel worktrees. Decide whether the
  entry stays.
- **M11** — the progress log claims "missing cleanup and auto-clean write no
  memory" was proven; no test in the diff supports that claim.

## Explicitly clean (verified, do not re-litigate)

Transaction handling (memory written inside the same transaction, before the
deletes; `BEGIN IMMEDIATE` closes the old TOCTOU window), the survivor
predicate including the deliberate absence of a `missing_since` filter, one
single normalization via `artist_news::normalize`, no SQL built from values,
the stale-path guard (stricter than before), NR-31/NR-33 append-only handling,
`.pot` regeneration with zero msgid drift, all touched files < 800 lines,
`check-architecture.sh` and `check-ux-traceability.sh` fail only on
pre-existing `origin/dev` baselines.
