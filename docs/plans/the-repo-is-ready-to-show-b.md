---
slug: the-repo-is-ready-to-show-b
worktree: /home/marvin/Projects/reprise-the-repo-is-ready-to-show-b
branch: feature/the-repo-is-ready-to-show-b
phase: planned
codex_session:
created: 2026-08-31
---
# Strand b — The Rust code

Part of `docs/plans/the-repo-is-ready-to-show.md`. Read the mother plan first:
it carries the decisions, the full cut, the merge order and the post-merge
cross-checks. **Merge position: second, after strand a.**

Correctness, performance, the runtime's crates, and the GP detectors. The
structural refactors (FilterBar, cover pipelines, the file-length metric) are
deliberately **not** here — see the mother plan's "Second run".

## File ownership

`crates/**` **except** `crates/reprise-android-ffi/**` (strand a's),
`Cargo.toml`, `Cargo.lock`, `acceptance/**`,
`scripts/check-architecture.sh`, `scripts/check-gnome-idioms.sh`,
`scripts/check-ai-hygiene.sh`, `docs/adr/**`.

Touch nothing else. In particular: no `meson.build`, no `data/`, no manifest, no
`README.md`/`AGENTS.md`/`CONTRIBUTING.md`, no `po/`, no `docs/` outside
`docs/adr/`.

Strand a has already landed when this runs, so the runtime's build and install
targets are gone and the crates are merely unused.

---

## b1 — The app reports a broken database instead of aborting

`crates/reprise-gnome/src/main.rs:194` is
`.expect("failed to open or migrate database")`, and there is no
`panic::set_hook` anywhere in `crates/reprise-gnome/src`. The project's own
consolidation findings label both release blockers (E1/task 0.5 and E2/T1) and
both are unfixed. A reviewer whose database is locked gets an abort and a Rust
backtrace; any other panic dies silently.

Two changes:

- Replace the `expect` with an explicit match. On error, present an
  `AdwAlertDialog` naming the database path and the underlying error, then exit
  non-zero. The app must not abort with a backtrace.
- Install a `std::panic::set_hook` early in `main` that logs payload and
  location through `tracing` before delegating to the previous hook, so a panic
  anywhere else leaves a trace.

**Failing test first.** Extract the decision into a testable function (open →
`Result<Db, E>` → a presentable message) and assert the error branch produces
the message rather than panicking. If the dialog itself cannot be asserted
headlessly, keep the dialog call a thin wrapper around the tested function and
say so in the commit message — do not assert a display-gated test and call it
proof. Note that `if gtk4::init().is_err() { return; }` at the top of a test
turns "no display" into a pass; if you write such a test, invert its assertion
once under Xvfb to prove it runs at all.

Commit: `fix: report a broken database instead of aborting`

## b2 — The runtime crates go, with ADR 003

Decision taken: **shelve.** Evidence, all verified: `reprise-runtime` has
exactly one workspace dependent (`crates/reprise-platform-linux/Cargo.toml:39`),
and only to build the binary strand a has already stopped installing;
`RuntimeSession::from_client` (`crates/reprise-gnome/src/ui/runtime/session.rs:72`)
is called once, from `session_tests.rs:42` under `#[cfg(test)]`;
`ui/runtime/{session,commands}.rs` sit under `#![allow(dead_code)]`;
`reprise-mcp` reaches the live D-Bus interfaces directly and uses only
`reprise_runtime_protocol` wire types.

- Delete `crates/reprise-runtime/` and `crates/reprise-runtime-client/`.
- Delete `crates/reprise-gnome/src/ui/runtime/` and its
  `pub(crate) mod runtime;` at `crates/reprise-gnome/src/ui/mod.rs:94`; remove
  the `reprise_runtime*` references from `crates/reprise-gnome/src/ui/diagnostics.rs`.
- Delete `crates/reprise-platform-linux/src/runtime_service/` and
  `crates/reprise-platform-linux/src/bin/reprise-runtime.rs`; drop the
  `reprise-runtime` / `reprise-runtime-client` dependencies and the `[[bin]]`
  section from that crate's `Cargo.toml`.
- Drop both crates from the workspace `members` in the root `Cargo.toml`.
- **Keep `crates/reprise-runtime-protocol`.** It is the DTO layer the
  direct-path D-Bus interfaces and `reprise-mcp` already use.

Write `docs/adr/003-runtime-ownership.md` in the same commit: the verdict, the
evidence above, what was deleted, what was kept, and that
`consolidation-plan.md:711-733` recommended exactly this. Wave 4 was gated on
this ADR existing.

After this the workspace has **nine** crates.

Commit: `refactor: shelve the runtime crates (ADR 003)`

## b3 — Device sync resolves the desired set in one query

`crates/reprise-gnome/src/ui/device_sync/device_sync_effects.rs:497-502` loops
per desired file calling `query_present_track_by_id`
(`crates/reprise-core/src/queries/surface_browse.rs:216`, which builds its SQL
with `format!` and prepares a fresh statement each call). There is no `.await`
in the loop, and it is driven from `MainContext::ref_thread_default().spawn_local`
(`device_sync_runtime.rs:554`) against an `Rc<Db>` (`:241`) — structurally
main-thread-only.

Replace with one batched `IN (…)` query. Bind the ids; do not build the list by
string concatenation.

**Failing test first:** assert one prepared statement for N ids, and that the
returned set matches the per-id results for a fixture with present, missing and
unknown ids.

This is a live violation of **GP-2**. GP-2 nonetheless stays `[planned]` after
this run — see b6.

Commit: `perf(device-sync): resolve the desired set in one query`

## b4 — The default library sort is served from an index

Consolidation task 1.1, fully specified there, never executed. Its own estimate
is "by a wide margin the largest single effect… a factor of 30 to 100".

**The plan's migration number is stale.** It says 50 → 51;
`crates/reprise-core/src/db.rs:26` reads `SUPPORTED_SCHEMA_VERSION = 80`. This
is **migration 81**.

- New `crates/reprise-core/src/db_sort_indexes.rs`; `mod db_sort_indexes;` in
  `lib.rs` beside the other `db_*`; bump the version and call the migration at
  the end of `migrate_with_cache_dirs`.
- Copy the structure of `db_recently_added.rs::migrate_v35` — version check,
  `unchecked_transaction`, `execute_batch`, `pragma_update`, `commit` — rather
  than inventing one.

```sql
CREATE INDEX IF NOT EXISTS idx_tracks_present_artist_order
ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
WHERE missing_since IS NULL AND removed_at IS NULL;
```

The column order must match `SORT_WHITELIST["artist"]` in
`crates/reprise-core/src/queries/clauses.rs` **exactly**; the `WHERE` must match
`clauses::PRESENT`.

**Failing tests:**

- `v81_serves_the_default_artist_sort_from_an_index` — over
  `queries::build_track_query("artist", "ASC", false)` (re-exported at
  `queries/mod.rs:119`; the builder needs two bound parameters for
  `LIMIT ?1 OFFSET ?2`), `EXPLAIN QUERY PLAN` must name
  `idx_tracks_present_artist_order` and must **not** contain
  `USE TEMP B-TREE FOR ORDER BY`. On an empty table the planner may spurn the
  index — fill a few hundred rows and `ANALYZE`, or reuse the fixture in
  `queries/tests.rs`.
- `v81_is_idempotent_and_bumps_the_schema_version`.

**Measure, do not claim.** `scripts/performance-baseline.sh ~/perf/before` on
the untouched base, `…/after` with the index, then
`scripts/performance-query-compare.sh ~/perf/before ~/perf/after`. The tree must
be clean for both runs — the script records the commit in its manifest. Run it
**without** `--quick`; the 100k run is the interesting one. Put the comparison
JSON in the commit message. If the numbers differ markedly from the plan's
estimate, the measurement is the truth.

Commit: `perf(db): serve the default library sort from an index`

## b5 — The remaining row tooltips stop costing a roundtrip

`lazy_tooltip.rs` exists and the track list uses it. `gtk_widget_set_tooltip_*`
costs a display roundtrip, and these call it directly inside virtualised bind
paths:

`crates/reprise-gnome/src/ui/concerts/concerts_columns.rs:207,218`,
`concerts/concerts_status_cells.rs:39`,
`releases/releases_columns.rs:189,194,209`,
`library_doctor/review_row.rs:246,248,252,253,269` (five per row bind, the worst
instance).

Route them through the existing helper.

**Fix the correctness bug found alongside:** `concerts_columns.rs:207` sets a
per-column tooltip, then `:208` calls `apply_row_link_presentation`
(`concerts_status_cells.rs:33-41`), which unconditionally overwrites it for
every text column. The `city_tooltip` value is dead code today, and the pair
costs two roundtrips instead of one. Decide which tooltip is correct, keep that
one, and add a test pinning it.

Commit: `perf(ui): defer the remaining row tooltips`

## b6 — The GP detectors stop counting test code

Measured during planning by running the gates:

| Rule | Gate reports | Reality |
|---|---|---|
| GP-2 | 43 blocking calls | nearly all in `*_tests.rs`, plus a legitimate `sleep` in `ui/lyrics/lyrics_worker.rs:146` |
| GP-4 | **3064** `unwrap()` | ~24 in production; the examples are `gtk4::init().unwrap()` under `#[cfg(test)]` |
| GP-3 | 2 strong captures | real: `ui/artwork_consent_banner.rs:97,113` |
| GP-19 | 12 banner blocks | real |
| GP-20 | 19 `#[allow(dead_code)]` without a reason | real |

The numbers are artefacts, which is why no GP rule could ever be flipped — and a
reviewer who runs the gate reads "3064 unwraps" about a codebase that has 24.

- In `scripts/check-gnome-idioms.sh` and `scripts/check-ai-hygiene.sh`, scope
  every detector to production code: exclude `#[cfg(test)]` blocks, `*_tests.rs`
  files, `tests/` and `examples/`.
- Then fix what remains real: the 2 strong captures (GP-3), the 12 banner
  comment blocks (GP-19), and give each of the 19 `#[allow(dead_code)]` a stated
  reason or delete it (GP-20). Two are known stale and should simply go:
  `session_player.rs:36` (`restore_should_start_playback`, reached only from
  `debug_assert!`/tests) and `issue_collapse.rs:38` (`CollapsedList::new`, zero
  callers).
- Prove the detectors still bite: reintroduce one violation of each fixed rule,
  run the gate, then revert.
  **The signal is the warning text, not the exit code.** `scripts/lib/rulebook.sh`
  makes `rulebook_exit` return 0 for a `[planned]` rule no matter what
  `report_violation` was handed — the conformance suite's own
  `rulebook_lib_reports_planned_rules_without_failing` test pins exactly that. A
  working detector prints `warning: GP-N [planned] — …` naming the reintroduced
  site while still exiting 0. Checking `$?` here reads every detector as broken.

**GP-2 stays `[planned]`.** Its detector looks for `sleep`/`block_on` and does
not find the real violation — the synchronous query loop b3 fixes. Making that
detector correct is separate work; do not flip GP-2 here. **GP-4 stays
`[planned]`** too: the honest number is ~24, not zero, and some are in UI paths.

Strand c flips GP-3, GP-19 and GP-20 (plus GP-12/13/16) once this lands.

Commit: `build: scope the GNOME idiom gates to production code`

## b7 — Five hardcoded labels become translatable

Five `.set_label(...)` calls bypass gettext. They live in this strand's files;
the matching `po/POTFILES.in` entries are strand c's task. Mark the strings
here; c gives them a catalogue.

Commit: `i18n: mark the last hardcoded labels for translation`

## b8 — One dangling documentation citation

`acceptance/deezer-placeholder-portraits/run-accept.sh:23` cites
`docs/plans/portrait-placeholder-fingerprint.md`, which is deleted. Nothing
catches it because `scripts/check-architecture.sh` scans only `crates/` and
`scripts/`.

Fix the citation here. **Do not extend the scan** — that is second-run work, and
extending it while strand c deletes 82 plan files would put a moving gate and a
moving tree in the same run.

Commit: `docs: fix the dangling plan citation in the acceptance runner`

---

## Done when

`cargo build --workspace` succeeds with nine crates; `docs/adr/003-runtime-ownership.md`
exists; the new migration tests pass and the perf comparison JSON is in b4's
commit message; `check-gnome-idioms.sh` and `check-ai-hygiene.sh` report
production-only numbers and were each proven to still bite; the workspace suite
is green run serially (`cargo test --workspace -- --test-threads=1` — parallel
runs are known flaky in `reprise-platform-linux`,
`reprise-core::podcasts::ytdlp` and `reprise-android-ffi` for unrelated
reasons).

No file outside this strand's ownership is touched.
