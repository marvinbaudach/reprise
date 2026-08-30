---
slug: the-write-slot-says-who-holds-it
worktree: /home/marvin/Projects/reprise-the-write-slot-says-who-holds-it
branch: feature/the-write-slot-says-who-holds-it
phase: shipped
codex_session:
created: 2026-08-29
---
# The write slot says who holds it

## The bug, as measured

`claim_tag_write_slot()` (`crates/reprise-core/src/library/tag_write_lock.rs:13`)
refuses every tag write while any row in `tag_write_jobs` sits in `prepared` or
`running`. Nothing in production ever moves a row out of those states.

Measured in the live database on 2026-08-29:

| | |
|---|---|
| Job 19 | `kind=doctor_apply`, `total_tracks=275`, `state=running` |
| created | 22:53:24 |
| app died | ~14 s later — 18 files `complete`, 1 `running`, 256 `pending` |
| restart | 22:53:38, cleaned up nothing |
| effect | Doctor apply, Doctor revert **and** the tag editor dead — permanently, not until restart |

The recovery exists and is tested — `LibraryDoctor::finalize_incomplete_writes()`
(`crates/reprise-core/src/library/library_doctor/write_recovery.rs:155`) — and is
called from three tests and no production code. `reprise-mcp/src/startup.rs:53`
calls `recover_incomplete_tag_write_jobs`, which only *classifies* and discards
its `Vec`. The GTK app calls neither.

What the user saw was the raw `thiserror` Display of `TagWriteBusy`, piped
untranslated into `toasts::show` by `watch_write_job`'s `Ok(Err(error))` branch
(`crates/reprise-gnome/src/ui/library_doctor/write_jobs.rs`).

## Two locks are the root cause

There are two different locks guarding one resource, with two different
messages, and the difference in their capitalisation is what identified the
failing path:

| | `TagWriteGate` (`ui/tag_write_gate.rs`) | `claim_tag_write_slot()` |
|---|---|---|
| mechanism | process-local `AtomicBool` | `SELECT EXISTS` over `tag_write_jobs` |
| scope | one process | the database |
| message | "**A**nother tag-writing job…" | "**a**nother tag-writing job…" |
| in the screenshot | **free** — the click passed it | **occupied** — the click died here |

`crates/reprise-mcp/src/doctor_actions.rs:44` opens the same database and runs
the same `LibraryDoctor` writes; five `reprise-mcp` processes were live against
`~/.local/share/reprise/reprise.db` during the measurement. So the slot is
genuinely cross-process and the `AtomicBool` can never describe it. Any fix that
makes only the gate visible fixes nothing.

**This plan collapses the two into one.** `TagWriteGate` is deleted.

## The liveness problem, and why it is not optional

`finalize_incomplete_writes()` cannot currently tell a crashed job from one
running right now in another process. This was checked in the source rather than
assumed:

- `recovery.rs:164` selects journal rows with `outcome IN ('pending','prepared')`
  — precisely a job's **not-yet-done** work.
- `classify_file` (`recovery.rs:79`) stamps `file_state='pending'` with all-pending
  fields as `NotApplied`, with no liveness test of any kind.

A live job's untouched tail is exactly that set. So wiring recovery at startup
*without* a liveness signal would finalize a running MCP job's rows out from
under it — a corruption path strictly worse than the wedge it fixes.

The schema (V19, `db_tag_write_jobs.rs`) carries no timestamp other than
`created_at`/`finished_at`, so no liveness signal exists to read.

### Decision: an flock'd lock file, not a heartbeat column

A `heartbeat_at` column was considered and rejected: it needs a schema
migration (irreversible in a way a follow-up PR is not), it adds writes inside
the per-file execution loop that the test suite covers heavily, and it forces an
arbitrary staleness threshold.

The lock file is exact instead of heuristic — **the kernel releases an flock when
the holding process dies**, which is precisely the crash semantics needed — and
needs no migration and no execution-loop change.

Precedent and dependencies are already in place:

- `crates/reprise-android-ffi/src/play_journal.rs:235` already uses
  `std::fs::File::try_lock()`; `runtime_service/lease.rs:97` uses the same
  concept via `rustix`. Follow the `try_lock` form — **no new dependency**.
- The lock must live in `reprise-core`: `scripts/check-architecture.sh` holds
  `reprise-mcp` to `reprise-core` alone, so `reprise-platform-linux` is not
  available to it.

## Decisions taken in the grill

Both were open in the draft and are settled here. They change signatures, so
they are stated before the tasks that carry them.

### The lock is three-valued, and the third value is asymmetric

`try_lock()` in this toolchain returns `Result<(), TryLockError>` with **three**
outcomes, not two — `play_journal.rs:249` already discriminates all three:

| outcome | meaning |
|---|---|
| `Ok(())` | lock acquired, and it is enforced |
| `Err(WouldBlock)` | another live holder |
| `Err(Error(_))` | the filesystem does not enforce advisory locks |

An `Option<TagWriteLock>` cannot express the third, and the third is the
dangerous one: read as "no lock held" it means "no live writer", which is the
statement under which recovery finalizes a running MCP job's rows.

**The policy is asymmetric — writing carries on, recovery does not:**

- a write job whose lock is `Unenforceable` **proceeds**. The user is never
  locked out of tag writing by a filesystem property, and mutual exclusion does
  not depend on the lock anyway: `claim_tag_write_slot()`'s row check stays and
  keeps doing that job. The lock adds *liveness detection*, it does not replace
  the exclusion.
- recovery and the slot probe treat `Unenforceable` as **"a writer may be
  alive"** and finalize nothing.

So the worst case on such a filesystem is today's wedge — the bug being fixed —
and never a job finalized under a live writer. That asymmetry is the whole point
of naming the third state instead of folding it into `None`.

### `Orphaned` is recovered silently — no banner

With recovery wired at startup and the probe exact, the only reachable orphan is
"process alive, worker thread dead", for which there is no evidence. `Orphaned`
stays in `TagWriteSlotStatus` — it costs nothing, the probe is the same one, and
the tests need to name the state — but it gets **no user-facing surface**: no
banner in `review_page.rs`, no new strings, no insertion point. Detecting it
triggers the same finalization recovery already performs, silently.

## Tasks

### 1. `TagWriteLock` in core

New `crates/reprise-core/src/library/tag_write_lock.rs` (it already owns the slot
concept) — a guard over `<db_dir>/tag-write.lock`:

```rust
pub enum TagWriteLockAttempt {
    /// Acquired, and the filesystem enforces it. Only this proves no other writer.
    Held(TagWriteLock),
    /// Another live holder.
    Busy,
    /// The filesystem does not enforce advisory locks. Writing carries on;
    /// nothing may conclude from this that no writer is alive.
    Unenforceable,
}

pub enum TagWriteLiveness {
    Live,
    Absent,
    /// Unenforceable — indistinguishable from `Live` for every decision.
    Unknown,
}
```

- `TagWriteLock::acquire(db_dir) -> io::Result<TagWriteLockAttempt>` — discriminate
  all three `try_lock()` outcomes exactly as `play_journal.rs:249` does, and log
  the `Unenforceable` case at `warn!` with the same wording pattern. Write `pid=`
  into the file after winning, as `lease.rs:107` does, so the holder is
  identifiable in diagnostics.
- `Drop` releases; the kernel also releases on process death. That is the point.
- `probe(db_dir) -> TagWriteLiveness` — **must use its own `open()`**: flock is
  per-open-file-description, so probing through an fd this process already holds
  the lock on succeeds spuriously, and the GTK app both writes and polls. Release
  immediately on success.
- Every consumer must be forced to handle `Unenforceable` — no `Option`
  conversion helper that collapses it back into two states, because that helper
  is exactly how the corruption path returns.

### 2. The lock brackets the job, with no window

Ordering is the whole risk and is specified here rather than left to
implementation: **acquire before the transaction that inserts the job row,
release after the transaction that finalizes it.** Any other order leaves a row
with no lock held, which recovery would read as orphaned and finalize under a
live writer.

- `crates/reprise-core/src/library/tag_write_job/` — `prepare_job` takes the
  attempt and hands back the guard (or the `Unenforceable` marker); `run_job`
  holds it to completion. It threads through both API surfaces.
- `Busy` is the only outcome that refuses the job.
- Callers to update: `library_doctor/write_jobs.rs`, `library_doctor/auto_apply.rs`,
  `tag_edit/tag_edit_flow.rs`, and `reprise-mcp/src/doctor_actions.rs`.

### 3. Delete `TagWriteGate`

`crates/reprise-gnome/src/ui/tag_write_gate.rs` and its five call sites
(`write_jobs.rs:29`, `:82`, `auto_apply.rs:28`, `tag_edit_flow.rs:403`,
`track_list.rs:350`). The core lock subsumes it — one lock, one message, one
truth. Its test moves to the new lock.

### 4. Recovery only finalizes what is provably dead

- `write_recovery.rs:155` — `finalize_incomplete_writes()` runs **only while
  holding a `TagWriteLockAttempt::Held`**. Holding an enforced lock *is* the proof
  no writer is alive. `Busy` and `Unenforceable` both mean: do nothing, and say so
  in the log. Stays idempotent.
- `crates/reprise-gnome/src/main.rs:197` — call it right after
  `startup_report::mark("database migrated")`, before the UI is built. It must
  **not** panic: log and continue. A failed recovery may never stop the app from
  starting.
- `crates/reprise-mcp/src/startup.rs:53` — replace classify-and-discard with the
  real finalization, same non-fatal handling.

### 5. `tag_write_slot_status()` — one query plus one probe

```rust
pub enum TagWriteSlotStatus {
    Free,
    Busy(TagWriteSlotOwner),
    Orphaned(TagWriteSlotOwner),
}

pub struct TagWriteSlotOwner {
    pub job_id: i64,
    pub kind: TagWriteJobKind,   // tag_editor | doctor_apply | doctor_revert
    pub completed_tracks: usize,
    pub total_tracks: usize,
    pub created_at: i64,
}
```

- row in `prepared`/`running` + probe `Live` **or `Unknown`** → `Busy`. Treating
  `Unknown` as busy is the same asymmetry as task 4: it costs a stale card, and
  the alternative costs data.
- row + probe `Absent` → `Orphaned`, recovered silently on detection (see the
  grill decision above).
- no row → `Free`.
- `completed_tracks` counts `tag_write_job_files WHERE state='complete'`.

### 6. The sidebar card shows the slot, whoever holds it

`DoctorProgressCard` already hangs in the sidebar (`library_doctor/mod.rs:203`,
`sidebar.append_doctor_card`), so it is visible from every screen. Today it only
ever shows this process's Doctor jobs.

- Drive it from `tag_write_slot_status()`, so a `tag_editor` save — today
  invisible in the Doctor while blocking it — and an MCP-owned job both appear.
- `DoctorJobKind` (`progress_card.rs:11`) is `Scan | Apply | Revert`; add the
  tag-editor case with its own title.
- Cross-process state has nothing to subscribe to, so poll: a ~1 s tick that runs
  **only** while the slot is known busy or a Doctor screen is open. Never a
  permanent timer.

This is the user's stated requirement — *show the running progress rather than
refusing a new start with an error message.* **Queueing a blocked apply is
explicitly out of scope**; a blocked apply stays un-started, with the reason
visible. It is a defensible follow-up, not this change.

### 7. Apply/revert stop flattening their errors

- `library_doctor/jobs.rs:155` and `:174` — `run_apply`/`run_revert` return
  `Result<_, String>`. `run_auto_apply` (`jobs.rs:118`) already returns the
  structured `JobFailure`, matched at `jobs.rs:42`; make all three agree.
- `DoctorError::TagWriteBusy(_)` (`library_doctor/types.rs:328`) is matchable, so
  once the flattening is gone the UI can separate "slot busy" from every other
  failure.
- `watch_write_job` — on busy, point at the card; on other failures a translated
  message, with the raw error logged and never shown.

### 8. Strings — and all seven catalogs

`crates/reprise-gnome/src/ui/strings_library_doctor.rs`, existing pattern: `N_!`
constants (`:19`) and plural helpers (`:545`). New: the tag-editor job title and
the replacement for the raw busy toast. `TAG_WRITE_BUSY` (`:78`) loses its last
caller and goes.

**The gate makes this bigger than it looks.** `scripts/tests/gettext-catalogs.sh`
runs `msgcmp` for every locale in `po/LINGUAS` — `ar bn de es fr hi zh_CN` — and
additionally requires `de` and `es` to have **no untranslated entries**. So each
new `N_!` msgid must be added to all seven `.po` files, translated in `de` and
`es`, and the removed `TAG_WRITE_BUSY` msgid must be dropped from all seven.
The gate stops at the first locale it can fault, so a green `ar` says nothing
about the other six — fix them as a set, not one gate run at a time.

## Verification

- **The measured case, on a real DB:** seed job 19's exact shape (275 files, 18
  `complete`, 1 `running`, 256 `pending`) and assert the slot is free afterwards
  and an apply can start.
- **The race, all three arms:** with the lock held by another handle, recovery
  must finalize **nothing**; with it free and enforced, it finalizes; with the
  attempt `Unenforceable`, it must finalize **nothing**. The first and third arms
  are what protect a live MCP write — without them this change is more dangerous
  than the bug.
- **Probe correctness:** a process holding the lock must not probe its own lock
  as free (the separate-`open()` requirement in task 1).
- **Idempotence:** recovery twice leaves the same rows.
- **Slot status:** free / busy / orphaned against seeded rows, including
  `completed_tracks` on a partially-written job, and `Unknown` liveness mapping to
  `Busy` rather than `Orphaned`.
- **UI:** a busy slot renders the card with the owner's kind; a free slot leaves
  Apply sensitive.
- `scripts/tests/gettext-catalogs.sh` must be green — see task 8; it is the gate
  most likely to be left red by this change.
- `scripts/check-architecture.sh` must stay green — it is the rule that put the
  lock in core.
- Use the repo's existing gate; do not invent a new test command.

## Parallelität

**The cut was attempted and does not hold. One strand.**

- Tasks 6, 7 and 8 all write `strings_library_doctor.rs`; 6 and 7 both write
  under `library_doctor/`. Every candidate split puts two branches in the same
  files — an add/add conflict on every landing.
- Task 1's `TagWriteLock` and task 5's `tag_write_slot_status()` are
  **compile-time** preconditions for tasks 2, 4 and 6. Rust will not build a
  strand that consumes an API another unmerged strand introduces, so this is not
  a mere merge-order preference.
- Task 3 (deleting `TagWriteGate`) touches the same five call sites task 2
  rewrites. Splitting them guarantees a conflict.
- The one independent piece — task 4's two call sites in `main.rs` and
  `startup.rs` — is about twenty lines, and even that depends on task 1 for the
  lock it must hold. A worktree to parallelise it buys no wall-clock and costs a
  seam.

Order inside the single strand: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Each of the first
four is the next one's precondition.

**No post-merge cross-checks** — there is no seam to check.
