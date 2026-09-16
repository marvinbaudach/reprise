---
slug: the-scan-leases-the-writer-per-batch
worktree: /home/marvin/Projects/reprise-the-scan-leases-the-writer-per-batch
branch: feature/the-scan-leases-the-writer-per-batch
phase: refactored
codex_session:
created: 2026-09-16
---
# The scan leases the writer per batch

**Hypothesis.** The scan held the writer for the whole walk; leasing it once
per batch with source reads outside the lease bounds each hold to database
work, while a separate tail lease keeps final reconciliation atomic.

## Problem

`MusicLibrary::scan` (`crates/reprise-android-ffi/src/lib.rs:170-189`) takes
`self.writer()` on line 174 and keeps that `MutexGuard<Db>` alive across
`scan_folder_with_source_and_progress`. Inside, `scan_folder_inner`
(`crates/reprise-core/src/library/scanner.rs:554-587`) opens **one**
`unchecked_transaction` and runs the whole SAF walk in it: per file a
`known_row` lookup (`scanner.rs:119-125`), `import_entry` with the tag read
through `LibrarySource::open_read` — a Binder round trip to Kotlin's
`AndroidSafSource.listChildren`/`openRead` — then `apply_mobile_sync`,
`gather_vanish_evidence`, `decide_outcome`, `tx.commit()`.

So on Android the shared writer mutex is held for the entire walk: minutes for
a real library. Every other writer parks behind it. #969 (queue persister) and
#973 (play journal) made the two background writers give up with bounded
backoff instead of hanging, but the other 26 `writer()` call sites in the crate
still `lock()` blindly, and the ANRs on the Pixel (2026-09-03, 2026-09-14:
main thread in `lock_contended`, scan thread inside `listChildren`) were this
hold. The Android backlog memory lists "Scan-Lock lösen" as the next step
after the CI gates.

The desktop does not share the mutex problem — `scan_worker.rs:238` opens a
private `Db::open_migrated` — but it shares the transaction shape: one write
transaction across the walk, so desktop writers wait on SQLite's 5 s
`busy_timeout` during a scan and fail after it.

## Goal

The writer is held for **one batch of walk items**, never across the walk:

- On Android `try_lock_writer` (from #973) succeeds while the scan is parked
  inside a source call. Concretely: with the scan blocked in `list_children`
  (the `read_during_scan_tests.rs` rendezvous), a waiter gets the writer.
- The longest single hold during the walk is bounded by one batch's work, not
  by library size. The scan logs its longest lease at the end so the bound can
  be read off logcat on the Pixel.
- Scan results are unchanged: same `ScanReport` counters, same vanish
  decisions, same `ScanOutcome` variants, no uniffi/Kotlin change. The core
  scanner test suite (`scanner_*_tests.rs`, 3.5k lines) stays green
  unmodified except where a test asserts the single-transaction shape itself.

## Decisions (grilled 2026-09-16)

1. **Three-phase batch** (D2): the tag read runs with the writer free; a
   lease holds DB work only. Alternative (a) — one lease around today's
   `handle_walk_item` per batch — rejected: hold would be 16 × SAF I/O.
2. **`SCAN_LEASE_ITEMS = 16`, `synchronous` untouched.** The pragma is a
   separate, desktop-affecting decision (follow-up); D5's log line supplies
   the number the constant is later judged by.
3. **Partial commits accepted** (D6). No staging table: the scan is
   idempotent, a committed import is true, and staging would touch the
   migration path for a guarantee nobody uses.
4. **`impl ScanWriter for Mutex<Db>` lives in core** (D1), so `Db::conn()`
   stays crate-private and the Android crate passes `&*self.writer`.
5. **Proof**: core lease tests on a file-backed temp DB; the Android test
   parks on the second directory so it fails when batching is broken.

## Design

### D1. A `ScanWriter` lease in core, implemented for `Db`, `Connection` and `Mutex<Db>`

```rust
// crates/reprise-core/src/library/scan_writer.rs
pub trait ScanWriter {
    /// Runs `work` with the writer connection, then gives it back. The scanner
    /// opens and commits one transaction inside every lease; a lease is never
    /// held across two batches.
    fn lease(
        &self,
        work: &mut dyn FnMut(&Connection) -> Result<(), ScanError>,
    ) -> Result<(), ScanError>;
}
impl ScanWriter for Db          { /* work(self.conn()) */ }
impl ScanWriter for Connection  { /* work(self) */ }
impl ScanWriter for Mutex<Db>   { /* lock().map_err(poisoned)?; work(guard.conn()) */ }
```

The `Mutex<Db>` impl lives in core because `Db::conn()` is `pub(crate)`;
Android then passes `&*self.writer` and nothing new becomes `pub` on `Db`. The
scan is the party that *waits* (plain `lock()` is right here); the parties
that must not wait already use `try_lock_writer`.

Entry points: `scan_folder_with_source_and_progress(source, db: &Db, …)` and
`scan_folder_in(conn, …)` keep their signatures and forward to a new
`scan_folder_with_writer_and_progress(source, writer: &dyn ScanWriter, root,
on_progress)`. Desktop callers are untouched.

### D2. Lease granularity: `SCAN_LEASE_ITEMS = 16` walk items per batch, the tag read outside the lease

The walk is visitor-driven (`LibrarySource::walk(root, order, &mut dyn
LibraryWalkVisitor)`, `source.rs:312`; `walk_root` at `scanner.rs:314` feeds
`handle_walk_item` from the closure). The visitor buffers up to 16
`LibraryWalkItem`s; the buffer is processed in **three phases** and the
visitor returns `Continue`:

1. **lease A** (DB only): for each item, today's `known_row` lookup and
   classification (`entry::scan_entry`'s head: `file_facts` uses the
   listing's metadata, `scanner_entry.rs:81` probes only when the listing
   gave none) → an `EntryPlan` per item: `Skip(outcome)` or `Import { facts,
   is_update }`. Commit, release.
2. **unlocked**: `track_meta::read_meta_with_fallback(source, path)` for each
   `Import` — the SAF `open_read` + lofty parse, the expensive part
   (`scanner_entry.rs:451`). Result kept per item (`Ok(meta)` or the
   `ScanError::Import` that `import_entry` classifies today).
3. **lease B**: one transaction; `import_readable_entry` / the error branch
   per `Import`, `record_walk_error` for `Error` items, progress and
   `WalkTrace`/`ScanReport` bookkeeping as today. Commit, release.

So a lease holds DB work only — milliseconds for 16 rows — and the SAF I/O
that dominates the walk runs with the writer free. `import_entry` splits into
"read meta" (caller) and `import_readable_entry` (already a separate function,
`scanner_entry.rs:452`), `EntryScan` is built per lease. Move detection's
probe (`scanner_move.rs:62`) and `restore_present_row` stay inside lease B:
one SAF `probe` per *moved* file is rare and cheap next to a tag read.

The batch is 16, not 1, because every lease B is a commit and
`Db::open_migrated` leaves `synchronous` at SQLite's default (FULL —
`db.rs:45-61` sets only `journal_mode`, `foreign_keys`, `busy_timeout`), so a
commit per file is an fsync per file on phone flash. Named constant; D5's log
line replaces the guess with a number.

**Alternative (a), rejected unless the grill revives it:** keep
`handle_walk_item` as is and only wrap each 16-item batch in one lease. Half
the diff, but the hold is then 16 × (SAF open + parse) — plausibly seconds on
a phone, longer than the play recorder's own 1.75 s schedule — and the goal
would ship without a number that could fail.

### D3. The tail (mobile sync, vanish, outcome) runs in its own single lease

After the walk: one lease around `apply_mobile_sync` → `gather_vanish_evidence`
→ `decide_outcome` → commit, the same order as today's lines 570-587. This is
the transaction whose atomicity matters (vanish marks are decided against the
complete trace), and it stays atomic. Its probes (`scanner_vanish.rs:257,333`)
touch only vanished candidates, normally a handful; an unreachable root is
caught earlier by `guard_root_before_walk` and never reaches the probes.

### D4. Not in this plan: the remaining source calls inside a lease

Lease B keeps the move-detection probe (`scanner_move.rs:62`) and the tail
lease keeps the vanish probes (`scanner_vanish.rs:257,333` — one per
unobserved candidate). Both are per-exception, not per-file; pulling them out
means restructuring `scanner_move.rs` (395 lines) and `scanner_vanish.rs`
(791) for a hold D5 will show to be small. Follow-up, with D5's number.

### D5. One debug log line per scan with the longest lease

`scan_folder_with_writer_and_progress` wraps each lease in an `Instant` and
logs `tracing::debug!(longest_lease_ms, leases, "scan writer leases")` once at
the end. No `ScanReport` field, no uniffi change.

### D6. Semantics that change, stated

- **Partial commits.** A scan that fails after batch k leaves batches 1..k
  committed (files that are on disk and were read correctly) and marks nothing
  missing, because the tail lease never runs. Today the whole walk rolls back.
  This is acceptable: a committed import is true regardless of whether the
  walk finished, and the next scan continues from there. Any test that asserts
  the rollback ("a failing scan leaves the library untouched") is the one
  place the suite may need an adjusted expectation — the task list names
  finding it.
- **Interleaved writers.** Between two batches another Android writer can
  commit (trash, mobile sync, appearance). A trashed file that the walk already
  listed is then read by `import_entry` and fails; that path already exists
  (`record_walk_error`, `WalkTrace.failed`, `scanner_vanish.rs:59`), it was
  just unreachable on Android because nothing could write during a scan. The
  desktop already lives with interleaving through `busy_timeout`.
- **Mid-scan inserts are safe.** A row inserted by another writer between the
  walk and the tail lease (mobile sync is the only Android writer that
  inserts tracks) is a vanish candidate that the walk never observed.
  `mark_vanished_with` (`scanner_vanish.rs:241-262`) probes every unobserved
  candidate before marking and skips it on `Present`; only `Absent` or a
  walk-confirmed absence marks. Cost: one SAF probe per such row inside the
  tail lease — rare by construction. No `seen_in_scan` column is needed.
- **A stale `EntryPlan`.** Between lease A and lease B another writer can
  change a row the batch classified (trash removes it; a repair updates it).
  Lease B re-reads `known_row` for each `Import` and drops the plan when the
  row no longer matches what lease A saw (`is_update` flipped, or the row is
  gone) — the same re-check-in-commit rule the trash-boundary memory names.
  The dropped item counts as skipped, not failed.

## Tasks

1. `crates/reprise-core/src/library/scan_writer.rs`: `ScanWriter` trait and the
   three impls; `mod scan_writer;` + re-export next to the scanner's public
   entry points. Unit test: the `Mutex<Db>` impl releases between two leases
   (a second thread `try_lock`s successfully between them) and maps a poisoned
   mutex to `ScanError`.
2. `scanner_entry.rs`: split `scan_entry` into `classify_entry(tx, …) ->
   EntryPlan` (known_row + facts, no tag read) and `apply_entry(tx, plan,
   meta_result)` (today's `import_readable_entry` / error branch, plus the
   stale-plan re-check from D6); `import_entry`'s `read_meta_with_fallback`
   call moves to the caller. `EntryScan` no longer owns a `tx` across
   batches — it is built per lease.
3. `scanner.rs` (+ new `scanner_batches.rs`, since 692 lines will not hold
   the loop): `scan_folder_with_writer_and_progress`; the visitor buffers
   `SCAN_LEASE_ITEMS` items and runs D2's three phases; `scan_folder_inner`
   becomes batches + tail lease (D3). Existing `&Db` / `&Connection` entry
   points forward. The D5 log line. Both files under 800 lines.
4. Core tests, new file `scanner_lease_tests.rs`, all on a **file-backed**
   temp DB (a second `Connection` to `:memory:` is a different database):
   - **no lease during the walk's I/O**: a `ScanWriter` fake sets an
     `AtomicBool held` inside `lease`; a `ScriptedSource` wrapper asserts
     `held == false` in `read_directory`, `walk` item delivery and
     `open_read`. (`probe` may run inside lease B — move detection — and is
     not asserted.)
   - **batches commit before the walk ends**: a scripted source with 40
     items whose `open_read` for item 33 blocks on a channel; a second
     connection to the same file sees 32 rows while blocked. Control arm on
     `origin/dev`: sees 0.
   - **lease count** = 1 estimate + 2 per batch + 1 tail for a 40-item source
     (8 leases; the estimate is excluded from longest-batch timing).
   - **partial failure**: the fake writer returns `ScanError` on lease B of
     batch 2: batch 1's rows present, `missing_since` NULL everywhere,
     outcome is the error.
   - **stale plan**: between lease A and lease B of a batch (the `open_read`
     channel again), a second connection deletes one classified row; the
     scan completes, the row stays absent, the item counts as skipped.
   - The existing suites run unmodified; if one asserts full rollback, adjust
     that one assertion to D6 and say so in the commit.
5. `crates/reprise-android-ffi/src/lib.rs::scan`: drop the `self.writer()`
   guard; pass `&*self.writer` as the `ScanWriter`. Nothing else in the crate
   changes.
6. Android test, new file `write_during_scan_tests.rs` next to
   `read_during_scan_tests.rs`, reusing its `FixtureSource` rendezvous but
   parking on the **second** directory's `list_children` (after batch 1 has
   committed — the first-directory rendezvous would pass with batching
   broken): while parked, `try_lock_writer(&library.writer)` returns `Some`,
   and a `PlayRecorder` write lands before the scan is released (a fresh
   reader sees the play count and batch 1's tracks). Control arm on
   `origin/dev`: `try_lock` returns `None`.
7. Gates: the core and android-ffi test suites, workspace clippy with
   warnings denied, rustfmt check; no Kotlin diff;
   `scripts/check-merge-readiness.sh` is **not** part of the code phase (the
   Codex run for #973 drifted into it and was OOM-killed there).
8. Commit messages name the hypothesis: the scan held the writer for the
   whole walk; a lease per batch with the tag read outside bounds the hold to
   DB work, the tail stays atomic.

## Control arm

The branch-only entry point cannot compile on `63408d1487`, so the old behavior
was established by review of its single whole-walk transaction: while item 33
is blocked, no batch can have committed (`completed_batches_are_visible_before_
the_walk_ends` would read 0 rather than 32 rows); the writer is leased once
rather than seven batch/tail leases; and Android's `try_lock` necessarily
returns `None` while that whole-walk guard is held. The refactored 40-item scan
now adds a separate progress-estimate lease, so its current count is eight.

## Risks

- **Batch memory.** Phase 2 holds up to 16 `TrackMeta` results at once —
  tags only; cover bytes are not part of `TrackMeta` on Android (no analysis
  backend). If a meta result turns out to carry embedded art, 16 × that is
  still bounded; the constant is the knob.
- **The scanner_entry split** touches the best-tested part of core
  (`scanner_*_tests.rs`, 3.5k lines). That suite is the regression net; the
  plan forbids editing it except the one rollback assertion D6 names.
- **Partial commits** (D6) are a behaviour change on the desktop too. Named,
  accepted, tested.
- **Minimum scan cost.** A non-empty single-batch scan takes four writer
  leases: progress estimate, classify, apply, and tail. Three of those leases
  open and commit write transactions; the estimate is read-only. The desktop
  tag-edit reconciliation path pays this synchronously after every edit.
- **`Mutex<Db>` in core** couples core to the Android locking model by one
  impl. It is 10 lines and keeps `Db::conn()` crate-private; the alternative
  (a `pub fn conn()`) leaks more.

## Follow-ups (not this plan)

- D4: the move-detection and vanish probes out of their leases, once D5 says
  how long those holds really are.
- The 26 blind `writer().lock()` sites: which run on the main thread, and
  should they use `try_lock_writer` — Android backlog item (3)/(4).
- `synchronous=NORMAL` for WAL on Android (fewer fsyncs; the standard WAL
  setting). Separate decision, desktop-affecting.

## Parallelität

Cannot be cut. Task 4 compiles only against task 2's new entry point, and
task 5 verifies through it; a strand that cannot go green before the merge is
not a strand. Tasks 1–3 all change `scanner.rs`' transaction shape and are
verified by one test file. Single strand.
